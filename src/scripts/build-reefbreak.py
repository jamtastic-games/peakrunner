#!/usr/bin/env python3
"""Original Reefbreak CTF map: a tropical atoll. A reef ring round a central
island with a lighthouse; each team's base is a freighter stranded on a
sandbar outside the ring, its flag on a pad on the reef crest in front of
the bow; a ring of outer islands beyond. Original procedural terrain,
materials and synthesized surf; no extracted assets, no external height
data, no recordings. See docs/reefbreak.md.

Usage (from src/, with scripts/requirements.txt installed): build-reefbreak.py [OUTPUT] [--no-bake]
Default output is the embedded assets/maps/reefbreak; refuses to overwrite.
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
from assets import map_models
from assets import reefbreak_base
from assets import reefbreak_landmarks
from assets import reefbreak_materials
from assets import reefbreak_terrain
from assets import pack_writer
from assets import turret_arcs
from assets import structure_kit

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
DEFINITION = ROOT/'maps/reefbreak.json'
BEACON_SITE = (7.0, 10.0)                 # levelled disc radius and falloff under a beacon
LIGHTHOUSE_SITE = (reefbreak_landmarks.YARD_R+3.0, 14.0)


def spec():
    return json.loads(DEFINITION.read_text())


def to_local(base, x, z):
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


def _ground(definition, x, z):
    return round(float(reefbreak_terrain.natural(np.array([x*1.0]), np.array([z*1.0]), definition['seed'])[0]), 3)


def beacons(definition):
    """(team, world x, ground y, world z) of each base's reef beacon."""
    out = []
    for base in definition['bases']:
        wx, wz = to_world(base, *reefbreak_base.BEACON_AT)
        out.append((base['team'], round(wx, 3), _ground(definition, wx, wz), round(wz, 3)))
    return out


def outpost_sites(definition):
    """(team, world x, floor y, world z, yaw radians) of each ground outpost."""
    return [(o['team'], float(o['position'][0]), _ground(definition, *o['position']), float(o['position'][1]),
             math.radians(o['yaw'])) for o in definition['outposts']]


def lighthouse_site(definition):
    x, z = definition['lighthouse']['position']
    return float(x), _ground(definition, x, z), float(z)


def terrain_sites(definition):
    smooth = reefbreak_terrain._smooth
    out = []
    for base in definition['bases']:
        oy = base['position'][1]
        for shape, surface, falloff in reefbreak_base.sites():
            def site(shape=shape, surface=surface, falloff=falloff, base=base, oy=oy):
                def surf(x, z):
                    lx, lz = to_local(base, x, z)
                    return np.vectorize(surface, otypes=[float])(lx, lz)+oy
                def weight(x, z):
                    lx, lz = to_local(base, x, z)
                    return 1-smooth(0, falloff, _shape_distance(shape, lx, lz))
                return surf, weight
            out.append(site())
    r, falloff = BEACON_SITE
    for _, wx, y, wz in beacons(definition):
        out.append((lambda x, z, y=y: np.full_like(x, y-.1),
                    lambda x, z, wx=wx, wz=wz: 1-smooth(0, falloff, _shape_distance(('disc', wx, wz, r), x, z))))
    for _, x, y, z, yaw in outpost_sites(definition):
        c, s_ = math.cos(yaw), math.sin(yaw)
        def weight(wx, wz, x=x, z=z, c=c, s_=s_):
            dx, dz = wx-x, wz-z
            return 1-smooth(0, 12, _shape_distance(reefbreak_base.outpost_site(), dx*c-dz*s_, dx*s_+dz*c))
        out.append((lambda wx, wz, y=y: np.full_like(wx, y-.1), weight))
    lx, ly, lz = lighthouse_site(definition)
    r, falloff = LIGHTHOUSE_SITE
    out.append((lambda x, z, y=ly: np.full_like(x, y-.1),
                lambda x, z: 1-smooth(0, falloff, _shape_distance(('disc', lx, lz, r), x, z))))
    for c in definition.get('control_points', []):
        cx, cz = c['x'], c['z']
        y = max(_ground(definition, cx, cz), reefbreak_terrain.SEA+.6)
        out.append((lambda x, z, y=y: np.full_like(x, y),
                    lambda x, z, cx=cx, cz=cz: 1-smooth(0, 16, _shape_distance(('disc', cx, cz, cnh_tower.RING+4), x, z))))
    return out


def terrain_grid(definition):
    return reefbreak_terrain.heights(definition['seed'], terrain_sites(definition))


def terrain_height(x, z, grid):
    return pack_writer.sample_height(grid, x, z)


def build_points(definition, mesh, grid):
    points, instances = [], []
    for c in definition.get('control_points', []):
        ground = round(terrain_height(c['x'], c['z'], grid), 3)
        mesh.origin = (c['x'], ground, c['z']); mesh.yaw = 0.0
        anchors = cnh_tower.build(mesh)
        instances.append(dict(asset=cnh_tower.ASSET_ID, id=c['id'], position=[c['x'], ground, c['z']],
                              anchors={k: mesh.point(v) for k, v in anchors.items()}))
        points.append({'id': c['id'], 'name': c['name'], 'pos': [c['x'], ground, c['z']],
                       'radius': cnh_tower.RING, 'ctf_active': False})
    return points, instances


def build_structures(definition, mesh):
    flags, spawns, spawn_points, instances, per_base = [], [], [], [], []
    beacon_at = {team: (wx, y, wz) for team, wx, y, wz in beacons(definition)}
    for base in definition['bases']:
        team = base['team']; before = len(mesh.collision)//9
        mesh.origin = tuple(base['position']); mesh.yaw = math.radians(base['yaw'])
        anchors = reefbreak_base.build(mesh, team, f'base-{team}')
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=reefbreak_base.ASSET_ID, **base,
            anchors={name: [mesh.point(p) for p in value] if isinstance(value, list) else mesh.point(value)
                     for name, value in anchors.items() if name not in ('spawn_points', 'beacon')}))
        mesh.origin = beacon_at[team]
        beacon = reefbreak_base.build_beacon(mesh, team, f'base-{team}')
        instances.append(dict(asset=reefbreak_base.BEACON_ID, team=team, position=list(beacon_at[team]),
                              yaw=base['yaw'], anchors={k: mesh.point(v) for k, v in beacon.items()}))
        per_base.append(len(mesh.collision)//9-before)
    for team, x, y, z, yaw in outpost_sites(definition):
        before = len(mesh.collision)//9
        mesh.origin = (x, y, z); mesh.yaw = yaw
        post = reefbreak_base.build_outpost(mesh, team, f'base-{team}')
        spawn_points[team].extend([[*mesh.point(p), (pyaw+yaw) % (2*math.pi)] for *p, pyaw in post['spawn_points']])
        instances.append(dict(asset=reefbreak_base.OUTPOST_ID, team=team, position=[x, y, z], yaw=round(math.degrees(yaw), 6),
                              anchors={k: mesh.point(v) for k, v in post.items() if k != 'spawn_points'}))
        per_base[team] += len(mesh.collision)//9-before
    before = len(mesh.collision)//9
    x, y, z = lighthouse_site(definition)
    mesh.origin = (x, y, z); mesh.yaw = 0.0
    light = reefbreak_landmarks.build_lighthouse(mesh)
    instances.append(dict(asset=reefbreak_landmarks.LIGHTHOUSE_ID, position=[x, y, z], yaw=0.0,
                          anchors={k: mesh.point(v) for k, v in light.items()}))
    per_base.append(len(mesh.collision)//9-before)
    return flags, spawns, spawn_points, instances, per_base


def surf(seed):
    """Original filtered-noise surf: a low wash with slow swells breaking
    on the reef and a faint hiss; a continuous four-second loop, not a
    recording. The filters are warmed over one identical period so the loop
    is seamless."""
    rng = random.Random(seed+417); count = 44100*4
    white = [rng.uniform(-1, 1) for _ in range(count)]
    out = array('f'); slow = mid = high = 0.0
    for i in range(count*2):
        n = white[i % count]; slow = .996*slow+.004*n; mid = .9*mid+.1*n; high = .4*high+.6*n
        if i >= count:
            t = math.tau*(i-count)/count
            swell = .45+.35*max(math.sin(t), 0)**2+.2*max(math.sin(2*t+1.3), 0)**3
            out.append((slow*.7+mid*.05+high*.01*swell)*swell)
    if sys.byteorder != 'little': out.byteswap()
    return out.tobytes()


def standin_last(mesh):
    """Move the geometry standing in for the authored models (mesh.standin,
    vertex float ranges) to the end of the structures' vertices, where the
    game can skip it as one run: [first vertex, vertex count]."""
    v, keep, moved, at = mesh.vertices, array('f'), array('f'), 0
    for a, c in sorted(mesh.standin):
        keep.extend(v[at:a]); moved.extend(v[a:c]); at = c
    keep.extend(v[at:])
    first = len(keep)//12
    keep.extend(moved)
    mesh.vertices = keep
    return [first, len(moved)//12]


def model_placements(definition):
    """Where the map's authored models are drawn: each base's ship."""
    return [dict(model=reefbreak_base.MODEL_NAME, position=list(map(float, b['position'])), yaw=float(b['yaw']), team=b['team'])
            for b in definition['bases']]


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    definition = spec()
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points, instances, per_base = build_structures(definition, mesh)
    grid = terrain_grid(definition)
    before = len(mesh.collision)//9
    control_points, tower_instances = build_points(definition, mesh, grid)
    instances.extend(tower_instances)
    tower_triangles = len(mesh.collision)//9-before
    standin = standin_last(mesh)
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = reefbreak_terrain.weights(grid).tobytes()
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, list(reefbreak_materials.MATERIALS),
                                reefbreak_materials.texture, definition['seed'], kit)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(), 'weights.rgba': bytes(weights),
             'textures.rgba': bytes(textures), 'ambient.f32': surf(definition['seed']),
             'models.glb': map_models.encode([(reefbreak_base.MODEL_NAME, reefbreak_base.map_model())])}
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    water_volumes = [reefbreak_terrain.sea_volume()]
    manifest.update(version=1, id=definition['id'], name=definition['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=[],
        entities=turret_arcs.assign(mesh.entities, flags), instances=instances,
        ambient_emitters=[[1024.0, reefbreak_terrain.SEA+2, 1024.0-reefbreak_terrain.RING['r'], .3, 400, 2600],
                          [1024.0, reefbreak_terrain.SEA+2, 1024.0+reefbreak_terrain.RING['r'], .3, 400, 2600]],
        sky={'visibleDistance': '3000', 'fogDistance': '2200', 'fogColor': '0.74 0.86 0.9'},
        models=model_placements(definition), standin=standin, zones=list(getattr(mesh, 'zones', [])),
        asset_sha256=pack_writer.source_hash(reefbreak_base.__file__),
        model_sha256=pack_writer.source_hash(reefbreak_base.MODEL),
        model_kit_sha256=pack_writer.source_hash(map_models.__file__),
        cnh_asset_sha256=pack_writer.source_hash(cnh_tower.__file__),
        landmark_asset_sha256=pack_writer.source_hash(reefbreak_landmarks.__file__),
        structure_kit_sha256=pack_writer.source_hash(structure_kit.__file__),
        terrain_source_sha256=pack_writer.source_hash(reefbreak_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(reefbreak_materials.__file__),
        control_points=control_points, look=definition['look'], water_enabled=False, water_volumes=water_volumes,
        provenance='PeakRunner original Reefbreak geometry, original procedural atoll terrain, '
                   'original authored ship model, original material kit and synthesized surf; '
                   'no extracted assets, no external height data, no recordings',
        definition_sha256=pack_writer.source_hash(DEFINITION))
    pack_writer.add_props_and_shade(kit, files, manifest, 'tower-complex', definition['seed'], manifest['holes'], flags,
                                    spawn_points, control_points, water=water_volumes)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {definition["name"]}: {len(mesh.collision)//9} solid triangles '
          f'({"/".join(str(t) for t in per_base[:2])} per base incl. beacon and outpost, {per_base[2]} lighthouse, '
          f'{tower_triangles} C&H towers), {len(mesh.vertices)//36} render triangles '
          f'({standin[1]//3} standing in for the ship models)'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/reefbreak', bake='--no-bake' not in sys.argv)
