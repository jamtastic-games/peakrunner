#!/usr/bin/env python3
"""Original Dustreach CTF map: ruined sandstone citadels on raised terraces,
each with a sunken flag court behind its keep, forward watch ruins, caravan
landing daises and the Sun Gate on the central saddle, over original rolling
dune terrain under a clear desert sky. No extracted assets, no external height
data, no recorded audio.

Usage (from src/, with scripts/requirements.txt installed): build-dustreach.py [OUTPUT] [--no-bake]
Default output is the embedded assets/maps/dustreach; refuses to overwrite.
"""
import importlib.util
import json
import math
from pathlib import Path
import random
import struct
import sys
from array import array

import numpy as np

from assets import cnh_tower
from assets import dustreach_citadel
from assets import dustreach_gate
from assets import dustreach_materials
from assets import dustreach_sewer
from assets import dustreach_terrain
from assets import map_models
from assets import pack_writer
from assets import turret_arcs
from assets import water_bodies
from assets import structure_kit

ROOT = Path(__file__).resolve().parent.parent
# The citadels' hard-surface detail (docs/map-pipeline.md, "Authored models"):
# built by scripts/models/dustreach_models.py from the citadel builder's own
# boxes, placed on each base in its frame. Nothing stands in for it: the
# builder's walls stay drawn and the detail sits on them.
MODELS = ROOT/'assets/models/structures/dustreach/model.glb'
CITADEL_MODEL, GATE_MODEL, RUIN_WALL_MODEL = 'dr-citadel', 'dr-gate', 'dr-ruin-wall'


def model_placements(definition):
    """Each citadel, the Sun Gate and each broken midfield wall: its model
    where the builder put it."""
    out = [dict(model=CITADEL_MODEL, position=list(map(float, b['position'])), yaw=float(b.get('yaw', 0)), team=b['team'])
           for b in sorted(definition['bases'], key=lambda b: b['team'])]
    out.append(dict(model=GATE_MODEL, position=list(map(float, definition['gate']['position'])), yaw=0.0, team=None))
    for wx, y, wz, yaw, kind in pieces(definition):
        if kind == 'wall':
            out.append(dict(model=RUIN_WALL_MODEL, position=[float(wx), float(y), float(wz)],
                            yaw=round(math.degrees(yaw), 6), team=None))
    return out
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
CENTRE = dustreach_terrain.CENTRE


def spec():
    return json.loads((ROOT/'maps/dustreach.json').read_text())


def to_local(base, x, z):
    """World (x, z) -> base-local (x, z); inverse of kit Mesh.point."""
    ox, _, oz = base['position']; a = math.radians(base['yaw'])
    c, s = math.cos(a), math.sin(a)
    dx, dz = x-ox, z-oz
    return dx*c-dz*s, dx*s+dz*c


def to_world(base, x, z):
    ox, _, oz = base['position']; a = math.radians(base['yaw'])
    c, s = math.cos(a), math.sin(a)
    return ox+x*c+z*s, oz-x*s+z*c


def _shape_distance(shape, x, z):
    if shape[0] == 'rect':
        _, x0, x1, z0, z1 = shape
        return np.hypot(np.maximum(np.maximum(x0-x, x-x1), 0), np.maximum(np.maximum(z0-z, z-z1), 0))
    _, cx, cz, r = shape
    return np.maximum(np.hypot(x-cx, z-cz)-r, 0)


def pieces(definition):
    """Midfield ruins in world space: (x, ground y, z, yaw, kind). Each sits
    on the natural ground at its centre."""
    out = []
    for x, z, yaw, kind in dustreach_gate.scatter():
        wx, wz = CENTRE+x, CENTRE+z
        y = float(dustreach_terrain.natural(np.array([wx]), np.array([wz]), definition['seed'])[0])
        out.append((wx, round(y, 3), wz, yaw, kind))
    return out


def terrain_sites(definition):
    """(surface, weight) pairs over world grids for every structure site."""
    smooth = dustreach_terrain._smooth
    out = []
    for base in definition['bases']:
        oy = base['position'][1]
        for _, shape, surface, falloff in dustreach_citadel.sites():
            def site(shape=shape, surface=surface, falloff=falloff, base=base, oy=oy):
                def surf(x, z):
                    lx, lz = to_local(base, x, z)
                    return np.vectorize(surface, otypes=[float])(lx, lz)+oy
                def weight(x, z):
                    lx, lz = to_local(base, x, z)
                    return 1-smooth(0, falloff, _shape_distance(shape, lx, lz))
                return surf, weight
            out.append(site())
    gx, gy, gz = definition['gate']['position']
    out.append((lambda x, z: np.full_like(x, gy-.06),
                lambda x, z: 1-smooth(0, 22, _shape_distance(('disc', gx, gz, dustreach_gate.SITE_R), x, z))))
    for wx, y, wz, _, _ in pieces(definition):
        out.append((lambda x, z, y=y: np.full_like(x, y-.1),
                    lambda x, z, wx=wx, wz=wz: 1-smooth(0, 10, _shape_distance(('disc', wx, wz, dustreach_gate.PIECE_R), x, z))))
    # Capture & Hold towers stand on a level plateau (cnh_tower.MAX_TILT).
    for cx, cz in tower_points(definition):
        y = float(dustreach_terrain.natural(np.array([cx]), np.array([cz]), definition['seed'])[0])
        out.append((lambda x, z, y=y: np.full_like(x, y),
                    lambda x, z, cx=cx, cz=cz: 1-smooth(0, 16, _shape_distance(('disc', cx, cz, cnh_tower.RING+4), x, z))))
    return out


def tower_points(definition):
    """(x, z) of every control point that gets a cnh_tower. The centre point
    is the Sun Gate itself, whose arch spans the flag lane, so it gets ring
    markers from dustreach_gate instead of a pylon in the lane."""
    return [(c['x'], c['z']) for c in definition.get('control_points', []) if c.get('structure') != 'gate']


def build_points(definition, mesh, grid):
    """Build the Capture & Hold towers and return (control_points, instances)."""
    points, instances = [], []
    for c in definition.get('control_points', []):
        if c.get('structure') == 'gate':
            ground = definition['gate']['position'][1]
        else:
            ground = round(terrain_height(c['x'], c['z'], grid), 3)
            mesh.origin = (c['x'], ground, c['z']); mesh.yaw = 0.0
            anchors = cnh_tower.build(mesh)
            instances.append(dict(asset=cnh_tower.ASSET_ID, id=c['id'], position=[c['x'], ground, c['z']],
                                  anchors={k: mesh.point(v) for k, v in anchors.items()}))
        points.append({'id': c['id'], 'name': c['name'], 'pos': [c['x'], ground, c['z']],
                       'radius': cnh_tower.RING, 'ctf_active': False})
    return points, instances


def holes(definition):
    """Terrain cells cut under the underground level (cistern, tunnel, tower
    room) and for the cross-map sewer: sorted cell indices. Every base rect
    must lie on the 8 m grid and under the terrace or the tower, so the ground
    at the cut's edge keeps the flat site height. Covered sewer cells are
    roofed by an exact copy of the terrain (dustreach_sewer.lid); its trench
    and shafts are open, walled up to the ground on the grid lines."""
    step = dustreach_terrain.STEP
    cells = set()
    for base in definition['bases']:
        for name, (x0, x1, z0, z1) in dustreach_citadel.HOLES.items():
            (ax, az), (bx, bz) = to_world(base, x0, z0), to_world(base, x1, z1)
            wx0, wx1, wz0, wz1 = min(ax, bx), max(ax, bx), min(az, bz), max(az, bz)
            for v in (wx0, wx1, wz0, wz1):
                if abs(v/step-round(v/step)) > 1e-9: raise ValueError(f'{name} is not on the 8 m grid')
            for iz in range(round(wz0/step), round(wz1/step)):
                for ix in range(round(wx0/step), round(wx1/step)):
                    cells.add((ix, iz))
    sewer = set(dustreach_sewer.all_cells())
    if cells & sewer: raise ValueError('the sewer must not cut into a base')
    return sorted(iz*256+ix for ix, iz in cells | sewer)


def terrain_grid(definition):
    return dustreach_terrain.heights(definition['seed'], terrain_sites(definition))


def terrain_height(x, z, grid):
    return pack_writer.sample_height(grid, x, z)


def build_structures(definition, mesh):
    """Every structure into one kit mesh. Returns flags, spawns, spawn points,
    instances and the triangle count of the two citadels."""
    flags, spawns, spawn_points, instances = [], [], [], []
    for base in definition['bases']:
        team = base['team']
        mesh.origin = tuple(base['position']); mesh.yaw = math.radians(base['yaw'])
        anchors = dustreach_citadel.build(mesh, team, f'base-{team}')
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=dustreach_citadel.ASSET_ID, **base,
            anchors={name: [mesh.point(p) for p in value] if isinstance(value, list) else mesh.point(value)
                     for name, value in anchors.items() if name != 'spawn_points'}))
    base_triangles = len(mesh.collision)//9
    mesh.origin = tuple(definition['gate']['position']); mesh.yaw = 0
    anchors = dustreach_gate.build(mesh)
    instances.append(dict(asset=dustreach_gate.ASSET_ID, **definition['gate'],
                          anchors={k: [mesh.point(p) for p in v] if isinstance(v, list) else mesh.point(v)
                                   for k, v in anchors.items()}))
    for wx, y, wz, yaw, kind in pieces(definition):
        mesh.origin = (wx, y, wz); mesh.yaw = yaw
        dustreach_gate.build_piece(mesh, kind)
        instances.append(dict(asset=f'{dustreach_gate.ASSET_ID}-{kind}', position=[wx, y, wz],
                              yaw=round(math.degrees(yaw), 6)))
    return flags, spawns, spawn_points, instances, base_triangles


def paint_sky(textures, count, layers):
    """Replace the six sky faces' whole mip chains (level-major layout)."""
    for face, layer in enumerate(layers):
        img = dustreach_materials.sky(face, kit.unit, kit.smooth, kit.cloud_noise)
        side, offset = 256, 0
        while True:
            size = side*side*4
            textures[offset+layer*size:offset+(layer+1)*size] = img
            offset += count*size
            if side == 1: break
            img = kit.downsample(img, side); side //= 2
    return textures


def wind(seed):
    """Original filtered-noise desert wind with slow gusts: a continuous
    four-second loop, not a recording. The filters are warmed over one
    identical period so the loop point is seamless."""
    rng = random.Random(seed+911); count = 44100*4
    white = [rng.uniform(-1, 1) for _ in range(count)]
    out = array('f'); slow = mid = 0.0
    for i in range(count*2):
        n = white[i % count]; slow = .996*slow+.004*n; mid = .85*mid+.15*n
        if i >= count:
            t = math.tau*(i-count)/count
            gust = .55+.3*math.sin(t)+.15*math.sin(3*t+1.3)
            out.append((slow*.55+mid*.02)*gust)
    if sys.byteorder != 'little': out.byteswap()
    return out.tobytes()


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    definition = spec()
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points, instances, base_triangles = build_structures(definition, mesh)
    grid = terrain_grid(definition)
    water, water_volumes = water_bodies.apply(definition, grid)
    before = len(mesh.collision)//9
    sewer, sewer_lamps = dustreach_sewer.build(mesh, grid)
    mesh.lamps.extend(sewer_lamps)
    lid_render, lid_collision = dustreach_sewer.lid(grid)
    sewer_triangles = len(mesh.collision)//9-before+len(lid_collision)//9
    before = len(mesh.collision)//9
    control_points, tower_instances = build_points(definition, mesh, grid)
    instances.extend(tower_instances)
    tower_triangles = len(mesh.collision)//9-before
    instances.append(dict(asset=dustreach_sewer.ASSET_ID, position=list(sewer['centre']), yaw=0,
                          anchors={k: list(v) for k, v in sewer.items()}))
    # The sewer's roof collides like the ground it replaces. It is appended
    # after baking (below) so it keeps the terrain shading path.
    mesh.collision.extend(lid_collision)
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = water_bodies.wet_banks(dustreach_terrain.weights(grid), grid, water, water_volumes,
                                     definition['water']['channel']).tobytes()
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, list(dustreach_materials.MATERIALS),
                                dustreach_materials.texture, definition['seed'], kit)
    paint_sky(textures, count, old['sky_layers'])
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
    vertices += np.asarray(lid_render, '<f4').tobytes()
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(), 'weights.rgba': bytes(weights),
             'textures.rgba': bytes(textures), 'ambient.f32': wind(definition['seed']),
             'models.glb': map_models.encode([(name, map_models.read(MODELS, name))
                                              for name in (CITADEL_MODEL, GATE_MODEL, RUIN_WALL_MODEL)])}
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    gx, gy, gz = definition['gate']['position']
    manifest.update(version=1, id=definition['id'], name=definition['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=holes(definition), entities=turret_arcs.assign(mesh.entities, flags),
        instances=instances, ambient_emitters=[[gx, gy+20, gz, .25, 400, 2400]],
        sky={'visibleDistance': '2600', 'fogDistance': '1500', 'fogColor': '0.80 0.69 0.52'},
        asset_sha256=pack_writer.source_hash(dustreach_citadel.__file__),
        gate_asset_sha256=pack_writer.source_hash(dustreach_gate.__file__),
        sewer_asset_sha256=pack_writer.source_hash(dustreach_sewer.__file__),
        control_points=control_points, look=definition['look'],
        models=model_placements(definition),
        model_sha256=pack_writer.source_hash(MODELS), model_kit_sha256=pack_writer.source_hash(map_models.__file__),
        water_enabled=False, water_volumes=water_volumes,
        water_source_sha256=pack_writer.source_hash(water_bodies.__file__),
        cnh_asset_sha256=pack_writer.source_hash(cnh_tower.__file__),
        structure_kit_sha256=pack_writer.source_hash(structure_kit.__file__),
        terrain_source_sha256=pack_writer.source_hash(dustreach_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(dustreach_materials.__file__),
        provenance='PeakRunner original Dustreach geometry, original procedural dune terrain, '
                   'original material kit, sky and synthesized wind; no extracted assets, '
                   'no external height data, no recordings',
        definition_sha256=pack_writer.source_hash(ROOT/'maps/dustreach.json'))
    pack_writer.add_props_and_shade(kit, files, manifest, 'dustreach', definition['seed'], manifest['holes'], flags, spawn_points,
                                    control_points, water=water_volumes)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {definition["name"]}: {len(mesh.collision)//9} solid triangles '
          f'({base_triangles//2} per citadel, '
          f'{len(mesh.collision)//9-base_triangles-sewer_triangles-tower_triangles} gate and ruins, '
          f'{sewer_triangles} sewer, {tower_triangles} C&H towers), '
          f'{len(manifest["holes"])} terrain holes, '
          f'{len(mesh.vertices)//36+len(lid_render)//36} render triangles'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/dustreach', bake='--no-bake' not in sys.argv)
