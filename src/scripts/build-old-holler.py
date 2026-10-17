#!/usr/bin/env python3
"""Build the Old Holler pack: the original kit layout with the cleaned base
and field structures, Old Holler materials and baked lighting.

Usage: build-old-holler.py [OUTPUT] [--no-bake]
Default OUTPUT is assets/maps/old-holler; refuses to overwrite. Terrain,
scenery placement and equipment come from maps/old-holler.json and the kit
(scripts/build-original-map.py), exactly as the original kit build placed them.
"""
from array import array
import importlib.util
import json
import math
from pathlib import Path
import random
import struct
import sys

from assets import cnh_tower
from assets import map_models
from assets import pack_writer
from assets import turret_arcs
from assets import water_bodies
from assets import old_holler_base
from assets import old_holler_materials
from assets import old_holler_structures

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)

PAINTED = ['meadow', 'moss', 'concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light']
# Old Holler's own look (manifest `look`, peakrunner_core::look): a low
# overcast over rainy hollows. The sun keeps the baked direction (the
# lightmaps assume it), dimmed and cooled, with only a faint disc through the
# cloud; mist settles in the ravine below the bases (height fog).
FOG_COLOR = '0.62 0.66 0.68'
LOOK = {
    'sun_color': [0.86, 0.88, 0.92],
    'sun_disc': 0.35,
    'exposure': 1.05,
    'ambient_sky': [0.56, 0.6, 0.64],
    'ambient_ground': [0.47, 0.5, 0.44],
    'height_fog': {'density': 0.006, 'base': 70.0, 'falloff': 30.0},
    'sky': {'zenith': [0.36, 0.41, 0.46], 'horizon': [0.62, 0.66, 0.68], 'cloud_cover': 0.85,
            'cloud_color': [0.7, 0.72, 0.74], 'cloud_scale': 1.4, 'sun_size': 0.035},
}


def spec():
    definition = json.loads((ROOT/'maps/old-holler.json').read_text())
    kit.validate(definition)
    return definition


def render_triangles(vertices):
    """Drop zero-area render triangles (the kit's broadleaf canopies collapse
    to a point at each pole). They draw nothing, and a lightmap chart for a
    triangle without a normal would bake NaN texels."""
    import numpy as np
    tris = np.frombuffer(vertices, '<f4').reshape(-1, 3, 12)
    p = tris[:, :, :3].astype(np.float64)
    area = np.linalg.norm(np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]), axis=1)
    return tris[area > 1e-9].tobytes()


# The authored models (docs/map-pipeline.md, "Authored models"): built by
# scripts/models/old_holler_models.py, one scene root per structure, all on
# one texture. Each base places its own (the flag tower, skybridge and the
# base's outside), in the base's frame, and each field structure its own.
MODELS = ROOT/'assets/models/structures/old-holler/model.glb'
BASE_MODEL = 'oh-base'
FIELD_MODELS = {'tower': 'oh-lookout', 'bunker': 'oh-bunker', 'landing_pad': 'oh-pad', 'bridge': 'oh-bridge'}
TEAM_MODELS = ('tower', 'landing_pad')
CROSSING_MODEL = 'oh-crossing'       # the Crossing point's platform and pier


def standin_last(mesh):
    """Move the geometry standing in for the authored models (mesh.standin,
    vertex float ranges) to the end of the structures' vertices, where the
    game can skip it as one run; returns (the rest, the stand-in) as bytes."""
    v, keep, moved, at = mesh.vertices, array('f'), array('f'), 0
    for a, c in sorted(getattr(mesh, 'standin', [])):
        keep.extend(v[at:a]); moved.extend(v[a:c]); at = c
    keep.extend(v[at:])
    return keep, moved


def model_placements(definition):
    placed = [dict(model=BASE_MODEL, position=list(map(float, b['position'])), yaw=float(b.get('yaw', 0)), team=b['team'])
              for b in sorted(definition['bases'], key=lambda b: b['team'])]
    for o in definition['objects']:
        if o['asset'] not in FIELD_MODELS: continue
        x, y, z = o['position']
        if o.get('grounded'): y = kit.height(x, z, definition)
        placed.append(dict(model=FIELD_MODELS[o['asset']], position=[float(x), float(y), float(z)],
                           yaw=float(o.get('yaw', 0)), team=o.get('team', 0) if o['asset'] in TEAM_MODELS else None))
    for cp in definition.get('control_points', []):
        if cp.get('platform'):
            x, z = cp['position']
            placed.append(dict(model=CROSSING_MODEL, position=[float(x), 0.0, float(z)], yaw=0.0, team=None))
    return placed


def place(mesh, obj, definition):
    """One authored field object, as the kit places it."""
    team = obj.get('team', 0)
    circuit = next(b['id'] for b in definition['bases'] if b['team'] == team)
    asset = obj['asset']
    if asset == 'tower': old_holler_structures.lookout(mesh, team, circuit)
    elif asset == 'bunker': old_holler_structures.bunker(mesh)
    elif asset == 'landing_pad': return old_holler_structures.landing_pad(mesh, team)
    elif asset == 'bridge': old_holler_structures.bridge(mesh)
    elif asset in ['inventory', 'generator', 'sensor', 'turret', 'repair']:
        mesh.equipment(asset, (0, 0, 0), team, obj.get('circuit', circuit), obj.get('weapon', 'bullet'))
    else: raise ValueError(f'Unknown field asset {asset}')
    return {}


# Capture & Hold points (docs/capture-and-hold.md). The knolls stand on flat,
# tree-free ground (terrain tilt under the plinth well inside
# cnh_tower.MAX_TILT) and mirror each other through the map centre (980, 940).
# The centre lies over the flooded ravine, so the Crossing tower stands on a
# round platform on a pier from the ravine floor, joined to the west side of
# the bridge at mid-span; its deck is level with the bridge deck.
PLATFORM_R, DECK_TOP, PIER_R = 13.0, 105.7, 4.0


def control_point(mesh, cp, d):
    """Build one point's structure; returns its manifest entry."""
    x, z = cp['position']
    if cp.get('platform'):
        ground = kit.height(x, z, d)
        mesh.origin = (x, 0, z); mesh.yaw = 0.0
        if not hasattr(mesh, 'standin'): mesh.standin = []
        start = len(mesh.vertices)
        mesh.column((0, ground-2, 0), PIER_R, DECK_TOP-1.4-(ground-2), 'concrete', 12)
        mesh.column((0, DECK_TOP-1.4, 0), PLATFORM_R, 1.0, 'trim', 24)
        mesh.column((0, DECK_TOP-.4, 0), PLATFORM_R, .4, 'grate', 24)
        mesh.standin.append((start, len(mesh.vertices)))     # the model `oh-crossing`
        y = DECK_TOP
    else:
        y = kit.height(x, z, d)
    mesh.origin = (x, y, z); mesh.yaw = 0.0
    cnh_tower.build(mesh, footing_top=-.02 if cp.get('platform') else 0.0)
    mesh.instances.append(dict(asset='cnh_tower', id=cp['id'], position=[x, y, z]))
    return {'id': cp['id'], 'name': cp['name'], 'pos': [x, y, z], 'radius': cnh_tower.RING, 'ctf_active': False}


# The flooded ravine is real water: the whole ravine (every cell below the
# surface lies in z 880-1024, running the full width of the map) slows and
# floats players, with a gentle current along it. Its surface is the kit's
# water height, so it matches the ravine carved into the terrain.
RAVINE = [0.0, 872.0, 2048.0, 1040.0]
RAVINE_FLOW = [0.8, 0.0]
RAVINE_COLOR = [0.16, 0.26, 0.20]


def ravine_water(d):
    surface = float(d['environment']['water_height'])
    return {'surface': surface, 'rect': list(RAVINE), 'depth': 22.0,
            'flow': list(RAVINE_FLOW), 'color': list(RAVINE_COLOR)}


def build(output, bake=True):
    if output.exists(): raise ValueError(f'Refusing to overwrite {output}; use a new output directory')
    d = spec()
    rng = random.Random(d['seed'])
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points = [], [], []
    for b in sorted(d['bases'], key=lambda b: b['team']):
        mesh.origin = tuple(b['position']); mesh.yaw = math.radians(b.get('yaw', 0))
        anchors = old_holler_base.build(mesh, b['team'], b['id'], d['base_equipment'])
        flags.append(mesh.point(anchors['flag'])); spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % math.tau] for *p, yaw in anchors['spawn_points']])
        mesh.instances.append(dict(asset='base', **b, anchors={
            k: [mesh.point(p) for p in v] if isinstance(v, list) else mesh.point(v)
            for k, v in anchors.items() if k != 'spawn_points'}))
    base_triangles = len(mesh.collision)//9
    points = [control_point(mesh, cp, d) for cp in d.get('control_points', [])]
    for obj in d['objects']:
        p = list(obj['position'])
        if obj.get('grounded'): p[1] = kit.height(p[0], p[2], d)
        mesh.origin = tuple(p); mesh.yaw = math.radians(obj.get('yaw', 0))
        anchors = place(mesh, obj, d)
        mesh.instances.append(dict(obj, position=p, **({'anchors': {
            k: [mesh.point(q) for q in v] if isinstance(v, list) else mesh.point(v)
            for k, v in anchors.items()}} if anchors else {})))
    # Scenery: the kit's seeded trees and rocks, placed exactly as before.
    for asset, count in [('tree', d['scenery']['trees']), ('rock', d['scenery']['rocks'])]:
        placed = 0
        for _ in range(count*40):
            x, z = rng.uniform(180, 1860), rng.uniform(200, 1840)
            y = kit.height(x, z, d)
            if y < 65 or any(math.hypot(x-b['position'][0], z-b['position'][2]) < 140 for b in d['bases']): continue
            if any(math.hypot(x-o['position'][0], z-o['position'][2]) < 35 for o in d['objects']): continue
            if abs(x-1000) < 35 and abs(z-940) < 150: continue
            mesh.origin = (x, y, z); mesh.yaw = rng.random()*math.tau
            getattr(mesh, asset)(rng); mesh.instances.append(dict(asset=asset, position=mesh.origin, yaw=math.degrees(mesh.yaw)))
            placed += 1
            if placed == count: break
    heights, weights, holes = bytearray(), bytearray(), []
    for iz in range(256):
        for ix in range(256):
            x, z = ix*8, iz*8; h = kit.height(x, z, d)
            heights.extend(struct.pack('<H', round(h*32)))
            slope = math.hypot(kit.height(x+2, z, d)-kit.height(x-2, z, d), kit.height(x, z+2, d)-kit.height(x, z-2, d))/4
            rock = kit.smooth((slope-.25)/.8); soil = (1-rock)*(.2+.16*math.sin(x/70)*math.cos(z/100))
            moss = (1-rock-soil)*(.2+.15*math.sin((x+z)/90)); grass = 1-rock-soil-moss
            weights.extend(round(w*255) for w in [grass, rock, soil, moss])
            cx, cz = x+4, z+4
            for b in d['bases']:
                bx, by, bz = b['position']; a = -math.radians(b.get('yaw', 0))
                lx = (cx-bx)*math.cos(a)+(cz-bz)*math.sin(a); lz = -(cx-bx)*math.sin(a)+(cz-bz)*math.cos(a)
                lx, lz = round(lx, 6), round(lz, 6)
                if abs(lx) < 36 and -56 < lz < 32:
                    holes.append(iz*256+ix); break
    # Aprons covering the grid-snapped edge of each basement cut.
    for b in d['bases']:
        mesh.origin = tuple(b['position']); mesh.yaw = math.radians(b.get('yaw', 0))
        for x in [-34, 34]: mesh.box((x, -.27, -12), (4, .6, 88), 'concrete')
        # The rear strip's east end is the service shed (old_holler_base).
        rear_end = old_holler_base.CEIL_END
        mesh.box(((rear_end-32)/2, -.27, 30), (rear_end+32, .6, 4), 'concrete')
        for x in [-21, 21]: mesh.box((x, -.27, -40), (22, .6, 24), 'concrete')
        mesh.box((0, -.27, -54), (64, .6, 4), 'grate')
    water_volumes = [ravine_water(d)]
    keep, moved = standin_last(mesh)
    if sys.byteorder != 'little': keep.byteswap(); moved.byteswap(); mesh.collision.byteswap()
    textures = bytearray(kit.base_textures(d['seed']))
    manifest = kit.layer_manifest(d['environment']['water_height'])
    count = manifest['texture_count']
    pack_writer.paint_materials(textures, count, PAINTED, old_holler_materials.texture, d['seed'], kit)
    # Zero-area triangles are dropped from both runs, then the stand-in run
    # goes last: [first vertex, vertex count].
    rest, standing = render_triangles(keep.tobytes()), render_triangles(moved.tobytes())
    standin = [len(rest)//48, len(standing)//48]
    vertices, lightmap = rest+standing, None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
        manifest['texture_count'] = count; manifest['lightmap'] = lightmap
    files = {'height.bin': bytes(heights), 'weights.rgba': bytes(weights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(), 'textures.rgba': bytes(textures),
             'ambient.f32': kit.base_ambient(d['seed']),
             'models.glb': map_models.encode([(name, map_models.read(MODELS, name))
                                              for name in (BASE_MODEL, *FIELD_MODELS.values(), CROSSING_MODEL)])}
    e = d['environment']
    manifest.update(version=1, name=d['name'], flags=flags, spawns=spawns, spawn_points=spawn_points,
        holes=sorted(set(holes)), sky={'visibleDistance': str(e['visibility']), 'fogDistance': str(e['fog_start']), 'fogColor': FOG_COLOR},
        look=LOOK,
        ambient_emitters=[[1000, 100, 940, .35, 200, 2000]], entities=turret_arcs.assign(mesh.entities, flags), instances=mesh.instances,
        models=model_placements(d), standin=standin,
        model_sha256=pack_writer.source_hash(MODELS), model_kit_sha256=pack_writer.source_hash(map_models.__file__),
        control_points=points, cnh_asset=cnh_tower.ASSET_ID,
        water_enabled=False, water_volumes=water_volumes,
        water_source_sha256=pack_writer.source_hash(water_bodies.__file__),
        materials=list(kit.MATERIALS), asset_catalog=list(kit.ASSETS),
        provenance='PeakRunner original procedural kit v2 (Old Holler); no extracted assets',
        base_asset=old_holler_base.ASSET_ID, structures_asset=old_holler_structures.ASSET_ID,
        base_asset_sha256=pack_writer.source_hash(old_holler_base.__file__),
        structures_asset_sha256=pack_writer.source_hash(old_holler_structures.__file__),
        material_source_sha256=pack_writer.source_hash(old_holler_materials.__file__),
        definition_sha256=pack_writer.source_hash(ROOT/'maps/old-holler.json'))
    pack_writer.add_props_and_shade(kit, files, manifest, 'old-holler', d['seed'], holes, flags, spawn_points, points,
                                    water=water_volumes)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {d["name"]}: {len(mesh.collision)//9} solid triangles ({base_triangles//2} per base), '
          f'{len(files["vertices.bin"])//144} render triangles, {len(mesh.entities)} equipment objects'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/old-holler', bake='--no-bake' not in sys.argv)
