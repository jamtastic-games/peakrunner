#!/usr/bin/env python3
"""Original Frostline CTF map: two polar research stations on mountain
shelves in a whiteout, each with a relay outpost, a plasma emplacement and a
vehicle apron, a lit navigation beacon on the central ridge with an ice
cavern through the ridge beneath it, and snow pines, over original steep snow
terrain. No extracted assets, no external height data.

Usage (from src/, with scripts/requirements.txt installed): build-frostline.py [OUTPUT] [--no-bake]
Default output is the embedded assets/maps/frostline; refuses to overwrite.
"""
import importlib.util
import json
import math
import random
from array import array
from pathlib import Path
import struct
import sys

import numpy as np

from assets import cnh_tower
from assets import centre_turret
from assets import frostline_beacon
from assets import frostline_cavern
from assets import frostline_flora
from assets import frostline_materials
from assets import frostline_station
from assets import frostline_terrain
from assets import map_models
from assets import pack_writer
from assets import turret_arcs
from assets import water_bodies
from assets import structure_kit

ROOT = Path(__file__).resolve().parent.parent
# The stations' hard-surface detail (docs/map-pipeline.md, "Authored models"):
# built by scripts/models/frostline_models.py from the station builder's own
# boxes, placed on each base in its frame. Nothing stands in for it: the
# builder's walls stay drawn and the detail sits on them.
MODELS = ROOT/'assets/models/structures/frostline/model.glb'
STATION_MODEL = 'fl-station'
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
STEP = frostline_terrain.STEP
FOG = {'visibleDistance': '900', 'fogDistance': '180', 'fogColor': '0.84 0.87 0.90'}
# Frostline's look (manifest `look`, peakrunner_core::look): a cold, bright
# overcast. The sun keeps the baked direction (the lightmaps assume it), pale
# and cool, with a small dim disc through high cloud; whiteout mist pools in
# the valleys (height fog below the station shelves) while the fog distances
# keep the beacon ridge readable from either base.
LOOK = {
    'sun_color': [0.92, 0.95, 1.0],
    'sun_disc': 0.45,
    'exposure': 1.0,
    'ambient_sky': [0.66, 0.72, 0.8],
    'ambient_ground': [0.7, 0.72, 0.75],
    'height_fog': {'density': 0.006, 'base': 205.0, 'falloff': 35.0},
    'sky': {'zenith': [0.5, 0.6, 0.72], 'horizon': [0.84, 0.87, 0.9], 'cloud_cover': 0.72,
            'cloud_color': [0.9, 0.92, 0.95], 'cloud_scale': 1.2, 'sun_size': 0.03},
}
# Capture & Hold points (docs/capture-and-hold.md): the beacon is the centre
# (see frostline_beacon.py); the Cols are flank shelves mirrored through the
# map centre on the centre line, 420 m from both flags.
POINT_SITE_R, POINT_FALLOFF = 16.0, 18.0


def spec():
    return json.loads((ROOT/'maps/frostline.json').read_text())


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


def terrain_sites(spec):
    smooth = frostline_terrain._smooth
    out = []
    for base in spec['bases']:
        oy = base['position'][1]
        for _, shape, level, falloff in frostline_station.sites():
            def site(shape=shape, level=level, falloff=falloff, base=base, oy=oy):
                def surf(x, z): return np.full_like(x, level+oy)
                def weight(x, z):
                    lx, lz = to_local(base, x, z)
                    return 1-smooth(0, falloff, _shape_distance(shape, lx, lz))
                return surf, weight
            out.append(site())
    bx, by, bz = spec['beacon']['position']
    out.append((lambda x, z: np.full_like(x, by-.06),
                lambda x, z: 1-smooth(0, 30, _shape_distance(('disc', bx, bz, frostline_beacon.SITE_R), x, z))))
    for cp in flank_points(spec):
        level = cp['level']
        out.append((lambda x, z, level=level: np.full_like(x, level),
                    lambda x, z, cp=cp: 1-smooth(0, POINT_FALLOFF, _shape_distance(('disc', cp['x'], cp['z'], POINT_SITE_R), x, z))))
    return out


def flank_points(spec):
    """Flank control points with their shelf level: the mean natural ground
    at the ring centre and its mirror point (each half's ground is its own
    out there, so the pair shares one level), quantized like height.bin so
    the ring floor is exact."""
    out = []
    for cp in spec.get('control_points', []):
        if 'x' not in cp: continue
        h = float(frostline_terrain.natural(np.array([cp['x'], 2048.0-cp['x']], float),
                                            np.array([cp['z'], 2048.0-cp['z']], float), spec['seed']).mean())
        out.append(dict(cp, level=round(h*32)/32))
    return out


def holes(spec):
    """Terrain cells cut for the basement generator rooms and their tunnels,
    and for the central ice cavern and its two open trenches: sorted cell
    indices. Every rect must lie on the 8 m grid. Base cells lie under the
    station, the cable duct or the service shed; cavern cells are roofed by an
    exact copy of the terrain (frostline_cavern.lid); trench cells are open
    to the sky with a floor and granite walls up to the ground."""
    cells = set()
    for base in spec['bases']:
        for name, (x0, x1, z0, z1) in frostline_station.HOLES.items():
            (ax, az), (bx, bz) = to_world(base, x0, z0), to_world(base, x1, z1)
            wx0, wx1, wz0, wz1 = min(ax, bx), max(ax, bx), min(az, bz), max(az, bz)
            for v in (wx0, wx1, wz0, wz1):
                if abs(v/STEP-round(v/STEP)) > 1e-9: raise ValueError(f'{name} is not on the 8 m grid')
            for iz in range(round(wz0/STEP), round(wz1/STEP)):
                for ix in range(round(wx0/STEP), round(wx1/STEP)):
                    cells.add((ix, iz))
    cells.update(frostline_cavern.cells())
    return sorted(iz*256+ix for ix, iz in cells)


def terrain_grid(spec):
    return frostline_terrain.heights(spec['seed'], terrain_sites(spec))


def terrain_height(x, z, grid):
    return pack_writer.sample_height(grid, x, z)


def tree_exclusions(spec):
    out = []
    for base in spec['bases']:
        for (lx, lz), r in (((-8, 0), 62), (frostline_station.OUTPOST, 26), (frostline_station.EMPLACEMENT, 22)):
            out.append((*to_world(base, lx, lz), r))
    bx, _, bz = spec['beacon']['position']
    out.append((bx, bz, 34))
    for cp in flank_points(spec): out.append((cp['x'], cp['z'], 26))
    return out


def trees(spec, grid):
    return frostline_flora.positions(spec['seed'], lambda x, z: terrain_height(x, z, grid),
                                     frostline_flora.slope_sampler(grid), tree_exclusions(spec), kit.noise)


def paint_sky(textures, count, layers, seed):
    """Replace the six sky cube layers' whole mip chains with the whiteout."""
    for face, layer in enumerate(layers[:6]):
        img = frostline_materials.sky(face, seed+face, kit.noise, kit.value_noise)
        side, offset = 256, 0
        while True:
            size = side*side*4
            textures[offset+layer*size:offset+(layer+1)*size] = img
            offset += count*size
            if side == 1: break
            img = kit.downsample(img, side); side //= 2
    return textures


def polar_wind(seed):
    """Original synthesized polar wind: low roar with sharp gusts and a thin
    whistle band. A four-second loop; filters are warmed over one identical
    period so the loop point is seamless. Not a recording."""
    rng = random.Random(seed+733); count = 44100*4
    white = [rng.uniform(-1, 1) for _ in range(count)]
    out = array('f'); low = band = band_lp = 0.0
    for i in range(count*2):
        n = white[i % count]
        low = .997*low+.003*n
        band_lp = .6*band_lp+.4*n; band = .92*band+.08*(n-band_lp)
        if i >= count:
            t = math.tau*(i-count)/count
            gust = .45+.25*math.sin(t)+.2*max(0.0, math.sin(2*t+.7))**3+.1*math.sin(5*t+2.1)
            out.append((low*.6+band*.18*gust)*gust)
    if sys.byteorder != 'little': out.byteswap()
    return out.tobytes()


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    definition = spec()
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points, instances = [], [], [], []
    for base in definition['bases']:
        team = base['team']
        mesh.origin = tuple(base['position']); mesh.yaw = math.radians(base['yaw'])
        anchors = frostline_station.build(mesh, team, f'base-{team}')
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=frostline_station.ASSET_ID, **base,
            anchors={name: [mesh.point(p) for p in value] if isinstance(value, list) else mesh.point(value)
                     for name, value in anchors.items() if name != 'spawn_points'}))
    base_triangles = len(mesh.collision)//9
    mesh.origin = tuple(definition['beacon']['position']); mesh.yaw = 0
    anchors = frostline_beacon.build(mesh)
    instances.append(dict(asset=frostline_beacon.ASSET_ID, **definition['beacon'],
                          anchors={k: [mesh.point(p) for p in v] if isinstance(v, list) else mesh.point(v)
                                   for k, v in anchors.items()}))
    beacon_triangles = len(mesh.collision)//9-base_triangles
    # Control points: the beacon (centre, CTF-active with a drain field) and
    # the two flank Cols, each a shared C&H tower on a levelled shelf.
    control_points = []
    for cp in definition.get('control_points', []):
        if cp.get('centre'):
            x, y, z = definition['beacon']['position']
        else:
            level = next(f for f in flank_points(definition) if f['id'] == cp['id'])['level']
            x, y, z = cp['x'], level, cp['z']
            before = len(mesh.collision)
            mesh.origin = (x, y, z); mesh.yaw = 0.0
            tower = cnh_tower.build(mesh)
            instances.append(dict(asset=cnh_tower.ASSET_ID, id=cp['id'], position=[x, y, z],
                                  anchors={k: mesh.point(v) for k, v in tower.items()}))
        entry = {'id': cp['id'], 'name': cp['name'], 'pos': [x, y, z], 'radius': cnh_tower.RING,
                 'ctf_active': bool(cp.get('ctf_active', False))}
        if 'drain' in cp: entry['drain'] = dict(cp['drain'])
        if cp.get('turret'):
            # The centre's turret, on top of the tower's crown (control::PointTurret).
            entry['turret'] = {'deployable': cp['turret'], 'at': [x, round(y+centre_turret.TURRET_Y, 3), z]}
            if cp.get('turret_range') == centre_turret.HALF_TO_BASE:
                entry['turret']['range'] = centre_turret.HALF_TO_BASE
        control_points.append(entry)
    point_triangles = len(mesh.collision)//9-base_triangles-beacon_triangles
    grid = terrain_grid(definition)
    water, water_volumes = water_bodies.apply(definition, grid)
    before = len(mesh.collision)//9
    anchors = frostline_cavern.build(mesh, grid)
    lid_render, lid_collision = frostline_cavern.lid(grid)
    cavern_triangles = len(mesh.collision)//9-before+len(lid_collision)//9
    instances.append(dict(asset=frostline_cavern.ASSET_ID, position=list(anchors['centre']), yaw=0,
                          anchors={k: list(v) for k, v in anchors.items()}))
    mesh.origin = (0, 0, 0); mesh.yaw = 0
    # No pine stands in the meltwater pools or on their banks; pairs stay mirrored.
    pines = [p for p in trees(definition, grid)
             if not any(water_bodies.radius_t(b, p[0], p[2]) < 1.0+water_bodies.BANK for b in water)]
    frostline_flora.build(mesh, pines)
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = water_bodies.wet_banks(frostline_terrain.weights(grid), grid, water, water_volumes,
                                     definition['water']['channel']).tobytes()
    # The cavern roof collides like the ground it replaces. It is appended
    # after baking (below) so it keeps the terrain shading path.
    mesh.collision.extend(lid_collision)
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, ['meadow', 'rock', 'soil', 'moss', 'concrete', 'panel', 'grate', 'trim',
                                                  'ember', 'glacier', 'light', 'bark', 'leaf'],
                                frostline_materials.texture, definition['seed'], kit)
    paint_sky(textures, count, old['sky_layers'], definition['seed']+500)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
    vertices += np.asarray(lid_render, '<f4').tobytes()
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(), 'weights.rgba': bytes(weights),
             'textures.rgba': bytes(textures), 'ambient.f32': polar_wind(definition['seed']),
             'models.glb': map_models.encode([(STATION_MODEL, map_models.read(MODELS, STATION_MODEL))])}
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    # The centre turret's reach, now that the bases' buildings stand.
    centre_turret.resolve_ranges(control_points, mesh.entities)
    manifest.update(version=1, id=definition['id'], name=definition['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=holes(definition), entities=turret_arcs.assign(mesh.entities, flags),
        instances=instances, ambient_emitters=[], sky=dict(FOG), trees=len(pines),
        control_points=control_points, look=LOOK,
        models=[dict(model=STATION_MODEL, position=list(map(float, b['position'])), yaw=float(b.get('yaw', 0)), team=b['team'])
                for b in sorted(definition['bases'], key=lambda b: b['team'])],
        model_sha256=pack_writer.source_hash(MODELS), model_kit_sha256=pack_writer.source_hash(map_models.__file__), water_enabled=False, water_volumes=water_volumes,
        water_source_sha256=pack_writer.source_hash(water_bodies.__file__),
        cnh_asset_sha256=pack_writer.source_hash(cnh_tower.__file__),
        asset_sha256=pack_writer.source_hash(frostline_station.__file__),
        beacon_asset_sha256=pack_writer.source_hash(frostline_beacon.__file__),
        cavern_asset_sha256=pack_writer.source_hash(frostline_cavern.__file__),
        flora_source_sha256=pack_writer.source_hash(frostline_flora.__file__),
        structure_kit_sha256=pack_writer.source_hash(structure_kit.__file__),
        terrain_source_sha256=pack_writer.source_hash(frostline_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(frostline_materials.__file__),
        provenance='PeakRunner original Frostline geometry, original procedural terrain, flora, material kit and synthesized wind; '
                   'no extracted assets or external height data',
        definition_sha256=pack_writer.source_hash(ROOT/'maps/frostline.json'))
    pack_writer.add_props_and_shade(kit, files, manifest, 'frostline', definition['seed'], manifest['holes'], flags, spawn_points,
                                    control_points, water=water_volumes)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {definition["name"]}: {len(mesh.collision)//9} solid triangles '
          f'({base_triangles//2} per base, {beacon_triangles} beacon and centre tower, {point_triangles} flank towers, '
          f'{cavern_triangles} cavern, {len(pines)} pines), '
          f'{len(mesh.vertices)//36} render triangles'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/frostline', bake='--no-bake' not in sys.argv)
