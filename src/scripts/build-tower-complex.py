#!/usr/bin/env python3
"""Original tower/generator/ship-platform/turret-pod CTF base, using
PeakRunner's original material kit.
"""
import importlib.util
import json
from pathlib import Path
import math
import struct
import sys
from assets import tower_complex
from assets import tower_complex_materials
from assets import tower_complex_terrain
from assets import landing_pad
from assets import pack_writer
from assets import turret_arcs
from assets import cnh_tower
from assets import centre_turret
from assets import water_bodies

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)

def summits(spec):
    """Each listed summit plus its point-symmetric partner through the
    midpoint of the two bases, so both teams get the same high ground."""
    (rx, _, rz), (bx, _, bz) = [b['position'] for b in spec['bases']]
    cx, cz = (rx+bx)/2, (rz+bz)/2
    out = []
    for s in spec.get('summits', []):
        out.append((s['x'], s['z'], s['height'], s['radius']))
        out.append((2*cx-s['x'], 2*cz-s['z'], s['height'], s['radius']))
    return out

def terrain_grid(spec):
    """Original rolling terrain (see assets/tower_complex_terrain.py)."""
    return tower_complex_terrain.heights([b['position'] for b in spec['bases']], spec['seed'],
                                         pads=[(p['position'][0], p['position'][2], p['position'][1])
                                               for p in spec.get('pads', [])],
                                         points=[(c['x'], c['z']) for c in spec.get('control_points', [])],
                                         summits=summits(spec))

def terrain_height(x, z, grid):
    """Bilinear sample of the grid, matching the engine's heightfield lookup."""
    return pack_writer.sample_height(grid, x, z)


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    spec = json.loads((ROOT/'maps/tower-complex.json').read_text())
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points = [], [], []
    instances = []
    for base in spec['bases']:
        team = base['team']
        mesh.origin = tuple(base['position']); mesh.yaw = math.radians(base['yaw']); base_yaw = mesh.yaw
        anchors = tower_complex.build(mesh, team, f'base-{team}')
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+base_yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=tower_complex.ASSET_ID, **base,
            anchors={name: [mesh.point(p) for p in value] if name == 'entrances' else mesh.point(value)
                     for name,value in anchors.items() if name != 'spawn_points'}))
    for pad in spec.get('pads', []):
        mesh.origin = tuple(pad['position']); mesh.yaw = math.radians(pad['yaw'])
        anchors = landing_pad.build(mesh, pad['team'])
        instances.append(dict(asset=landing_pad.ASSET_ID, **pad,
            anchors={name: [mesh.point(p) for p in value] if isinstance(value, list) else mesh.point(value)
                     for name, value in anchors.items()}))
    grid = terrain_grid(spec)
    water, water_volumes = water_bodies.apply(spec, grid)
    # Capture & Hold towers, standing on the levelled plateau at each point.
    control_points = []
    for c in spec.get('control_points', []):
        ground = round(terrain_height(c['x'], c['z'], grid), 3)
        mesh.origin = (c['x'], ground, c['z']); mesh.yaw = 0.0
        anchors = cnh_tower.build(mesh)
        instances.append(dict(asset=cnh_tower.ASSET_ID, id=c['id'], position=[c['x'], ground, c['z']],
                              anchors={k: mesh.point(v) for k, v in anchors.items()}))
        entry = {'id': c['id'], 'name': c['name'], 'pos': [c['x'], ground, c['z']],
                 'radius': cnh_tower.RING, 'ctf_active': bool(c.get('ctf_active', False))}
        if c.get('turret'):
            # The centre's turret, on top of the tower's crown (control::PointTurret).
            entry['turret'] = {'deployable': c['turret'], 'at': [c['x'], round(ground+centre_turret.TURRET_Y, 3), c['z']]}
            if c.get('turret_range') == centre_turret.HALF_TO_BASE:
                entry['turret']['range'] = centre_turret.HALF_TO_BASE
        control_points.append(entry)
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = bytearray(water_bodies.wet_banks(tower_complex_terrain.weights(grid), grid, water, water_volumes,
                                               spec['water']['channel']).tobytes())
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark'],
                                tower_complex_materials.texture, spec['seed'], kit)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        import time
        started = time.time()
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
        bake_seconds = round(time.time()-started, 1)
    files = {'height.bin':bytes(heights), 'vertices.bin':vertices,
             'collision.bin':mesh.collision.tobytes(),
             'weights.rgba':bytes(weights),
             'textures.rgba':bytes(textures),
             'ambient.f32':shared['ambient']}
    manifest={k:old[k] for k in ['terrain_layers','sky_layers','water_layer','water']}
    manifest['texture_count']=count
    if lightmap: manifest['lightmap']=lightmap
    # The centre turret's reach, now that the bases' buildings stand.
    centre_turret.resolve_ranges(control_points, mesh.entities)
    manifest.update(version=1, id=spec['id'], name=spec['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=[], entities=turret_arcs.assign(mesh.entities, flags), instances=instances, ambient_emitters=[],
        sky={'visibleDistance':'2500','fogDistance':'1500','fogColor':'0.74 0.80 0.88'},
        control_points=control_points, look=spec['look'],
        water_enabled=False, water_volumes=water_volumes,
        water_source_sha256=pack_writer.source_hash(water_bodies.__file__),
        cnh_asset_sha256=pack_writer.source_hash(cnh_tower.__file__),
        asset_sha256=pack_writer.source_hash(tower_complex.__file__),
        pad_asset_sha256=pack_writer.source_hash(landing_pad.__file__),
        terrain_source_sha256=pack_writer.source_hash(tower_complex_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(tower_complex_materials.__file__),
        provenance='PeakRunner original tower-complex geometry, original procedural terrain and original material kit; no extracted assets or external height data',
        definition_sha256=pack_writer.source_hash(ROOT/'maps/tower-complex.json'))
    pack_writer.add_props_and_shade(kit, files, manifest, 'tower-complex', spec['seed'], [], flags, spawn_points, control_points,
                                    water=water_volumes)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {spec["name"]}: {len(mesh.collision)//9} solid triangles'
          + (f', {lightmap["pages"]} lightmap pages in {bake_seconds} s' if lightmap else ', unbaked'))

if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/tower-complex', bake='--no-bake' not in sys.argv)
