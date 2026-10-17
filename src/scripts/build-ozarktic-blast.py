#!/usr/bin/env python3
"""Original Ozarktic Blast CTF map: frosted pine highlands where a tall
central mountain splits the field. Ember holds a bluff base on the Giant
Hill, Glacier a hollow base in the valley; each has a flag deck, a command
hall over its generator room, a tunnel to an outbuilding, a tunnel up to
its outpost and a spire in front. Original procedural terrain, materials
and synthesized wind; no
extracted assets, no external height data, no recordings.
See docs/ozarktic-blast.md.

Usage (from src/, with scripts/requirements.txt installed): build-ozarktic-blast.py [OUTPUT] [--no-bake]
Default output is the embedded assets/maps/ozarktic-blast; refuses to overwrite.
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
from assets import ozarktic_base
from assets import ozarktic_landmarks
from assets import ozarktic_materials
from assets import ozarktic_terrain
from assets import ozarktic_tunnel
from assets import pack_writer
from assets import turret_arcs
from assets import structure_kit

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
DEFINITION = ROOT/'maps/ozarktic-blast.json'
SPIRE_SITE = (9.0, 10.0)                  # levelled disc radius and falloff under a spire


def spec():
    return json.loads(DEFINITION.read_text())


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


def spires(definition):
    """(team, world x, ground y, world z) of each base's spire, on the
    natural ground at its foot."""
    out = []
    for base in definition['bases']:
        wx, wz = to_world(base, 0.0, ozarktic_base.SPIRE_OFFSET)
        y = float(ozarktic_terrain.natural(np.array([wx]), np.array([wz]), definition['seed'])[0])
        out.append((base['team'], round(wx, 3), round(y, 3), round(wz, 3)))
    return out


def perch_site(definition):
    """(world x, ground y, world z, yaw radians) of the sniper's perch."""
    x, z = definition['perch']['position']
    y = float(ozarktic_terrain.natural(np.array([x*1.0]), np.array([z*1.0]), definition['seed'])[0])
    return float(x), round(y, 3), float(z), math.radians(definition['perch']['yaw'])


def _ground(definition, x, z):
    return round(float(ozarktic_terrain.natural(np.array([x*1.0]), np.array([z*1.0]), definition['seed'])[0]), 3)


def dock_sites(definition):
    """(world x, rim y, world z, yaw radians, ship yaw radians) of each dry dock."""
    return [(float(d['position'][0]), _ground(definition, *d['position']), float(d['position'][1]),
             math.radians(d['yaw']), math.radians(d['ship_yaw'])) for d in definition['docks']]


def ship_sites(definition):
    """(world x, floor y, world z, yaw radians) of each dock's hovering ship."""
    return [(x, y+ozarktic_landmarks.HOVER, z, sy) for x, y, z, _, sy in dock_sites(definition)]


def outpost_sites(definition):
    """(team, world x, floor y, world z, yaw radians) of each team's outpost."""
    return [(o['team'], float(o['position'][0]), _ground(definition, *o['position']), float(o['position'][1]),
             math.radians(o['yaw'])) for o in definition['outposts']]


def _local_rect_weight(cx, cz, yaw, rect, falloff, smooth):
    c, s = math.cos(yaw), math.sin(yaw)
    def weight(x, z):
        dx, dz = x-cx, z-cz
        lx, lz = dx*c-dz*s, dx*s+dz*c
        return 1-smooth(0, falloff, _shape_distance(('rect', *rect), lx, lz))
    return weight


def terrain_sites(definition):
    """(surface, weight) pairs over world grids for every structure site."""
    smooth = ozarktic_terrain._smooth
    out = []
    for base in definition['bases']:
        oy = base['position'][1]
        for _, shape, surface, falloff in ozarktic_base.sites(base['style']):
            def site(shape=shape, surface=surface, falloff=falloff, base=base, oy=oy):
                def surf(x, z):
                    lx, lz = to_local(base, x, z)
                    return np.vectorize(surface, otypes=[float])(lx, lz)+oy
                def weight(x, z):
                    lx, lz = to_local(base, x, z)
                    return 1-smooth(0, falloff, _shape_distance(shape, lx, lz))
                return surf, weight
            out.append(site())
    r, falloff = SPIRE_SITE
    for _, wx, y, wz in spires(definition):
        out.append((lambda x, z, y=y: np.full_like(x, y-.1),
                    lambda x, z, wx=wx, wz=wz: 1-smooth(0, falloff, _shape_distance(('disc', wx, wz, r), x, z))))
    # Dry docks: the rim levelled round a sunken basin.
    hx, hz, depth = ozarktic_landmarks.BASIN
    for x, y, z, yaw, _ in dock_sites(definition):
        c, s = math.cos(yaw), math.sin(yaw)
        def dock_surface(wx, wz, x=x, y=y, z=z, c=c, s=s):
            dx, dz = wx-x, wz-z
            lx, lz = dx*c-dz*s, dx*s+dz*c
            inside = np.hypot(np.maximum(np.abs(lx)-hx, 0), np.maximum(np.abs(lz)-hz, 0))
            return y-.1-depth*(1-smooth(0, 1.5, inside))
        out.append((dock_surface, _local_rect_weight(x, z, yaw, (-hx-26, hx+26, -hz-26, hz+26), 16, smooth)))
    # Outposts stand on levelled pads cut into the upland.
    for _, x, y, z, yaw in outpost_sites(definition):
        out.append((lambda wx, wz, y=y: np.full_like(wx, y-.1),
                    _local_rect_weight(x, z, yaw, (-ozarktic_landmarks.OUT_W-4, ozarktic_landmarks.OUT_W+4,
                                                   -ozarktic_landmarks.OUT_D-8, ozarktic_landmarks.OUT_D+3), 12, smooth)))
    # The perch stands on a levelled pad reaching back under its ramp.
    px, py, pz, pyaw = perch_site(definition)
    pc, ps = math.cos(pyaw), math.sin(pyaw)
    def perch_weight(x, z):
        dx, dz = x-px, z-pz
        lx, lz = dx*pc-dz*ps, dx*ps+dz*pc
        return 1-smooth(0, 10, _shape_distance(('rect', -9.0, 9.0, -9.0, ozarktic_landmarks.PERCH_RAMP[2]+3), lx, lz))
    out.append((lambda x, z, y=py: np.full_like(x, y-.1), perch_weight))
    # Each outpost tunnel's stair house opens onto a levelled apron.
    for x0, x1, z0, z1, y in stair_pads(definition):
        out.append((lambda x, z, y=y: np.full_like(x, y),
                    lambda x, z, r=(x0, x1, z0, z1): 1-smooth(0, 10, _shape_distance(('rect', *r), x, z))))
    for c in definition.get('control_points', []):
        cx, cz = c['x'], c['z']
        y = float(ozarktic_terrain.natural(np.array([cx]), np.array([cz]), definition['seed'])[0])
        out.append((lambda x, z, y=y: np.full_like(x, y),
                    lambda x, z, cx=cx, cz=cz: 1-smooth(0, 16, _shape_distance(('disc', cx, cz, cnh_tower.RING+4), x, z))))
    return out


def tunnel_paths(definition):
    """Each team's outpost tunnel: (team, base, outpost site)."""
    outposts = {o[0]: o for o in outpost_sites(definition)}
    return [(b['team'], b, outposts[b['team']]) for b in definition['bases']]


def stair_pads(definition):
    """(x0, x1, z0, z1, y) of the apron at each outpost tunnel's stair house."""
    out = []
    for t, b, o in tunnel_paths(definition):
        _, stair, dz = ozarktic_tunnel.path(t, b, o, to_world)
        out.append(ozarktic_tunnel.stair_site(o, stair, dz))
    return out


def holes(definition):
    """Terrain cells cut under the underground level: sorted cell indices.
    Every base rect must lie on the 8 m grid and under a structure; the
    outpost tunnels' cells lie under their lids (the ground, pinned) or
    their stair houses."""
    step = ozarktic_terrain.STEP
    cells = set()
    for base in definition['bases']:
        for name, (x0, x1, z0, z1) in ozarktic_base.HOLES.items():
            (ax, az), (bx, bz) = to_world(base, x0, z0), to_world(base, x1, z1)
            wx0, wx1, wz0, wz1 = min(ax, bx), max(ax, bx), min(az, bz), max(az, bz)
            for v in (wx0, wx1, wz0, wz1):
                if abs(v/step-round(v/step)) > 1e-9: raise ValueError(f'{name} is not on the 8 m grid')
            for iz in range(round(wz0/step), round(wz1/step)):
                for ix in range(round(wx0/step), round(wx1/step)):
                    cells.add((ix, iz))
    cells |= ozarktic_tunnel.cells(definition, to_world, {o[0]: o for o in outpost_sites(definition)})
    return sorted(iz*256+ix for ix, iz in cells)


def tunnel_profiles(definition, unpinned):
    """Each team's outpost tunnel: its cells, floor and pinned lid corners,
    from the terrain before the pins."""
    height = lambda ix, iz: float(unpinned[iz, ix])
    return {t: ozarktic_tunnel.profile(t, b, o, to_world, height) for t, b, o in tunnel_paths(definition)}


def terrain_grid(definition):
    sites = terrain_sites(definition)
    unpinned = ozarktic_terrain.heights(definition['seed'], sites)
    pins = {}
    for prof in tunnel_profiles(definition, unpinned).values(): pins.update(prof['pins'])
    return ozarktic_terrain.heights(definition['seed'], sites, pins)


def terrain_height(x, z, grid):
    return pack_writer.sample_height(grid, x, z)


def build_points(definition, mesh, grid):
    """Build the Capture & Hold towers and return (control_points, instances)."""
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
    """Every base and spire into one kit mesh. Returns flags, spawns, spawn
    points, instances and the triangle count per base (hall, tunnel,
    outbuilding, deck, spire, outpost and outpost tunnel)."""
    flags, spawns, spawn_points, instances, per_base = [], [], [], [], []
    spire_at = {team: (wx, y, wz) for team, wx, y, wz in spires(definition)}
    for base in definition['bases']:
        team = base['team']; before = len(mesh.collision)//9
        mesh.origin = tuple(base['position']); mesh.yaw = math.radians(base['yaw'])
        anchors = ozarktic_base.build(mesh, team, f'base-{team}', base['style'])
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=ozarktic_base.ASSET_ID, **base,
            anchors={name: [mesh.point(p) for p in value] if isinstance(value, list) else mesh.point(value)
                     for name, value in anchors.items() if name not in ('spawn_points', 'spire')}))
        mesh.origin = spire_at[team]
        spire = ozarktic_base.build_spire(mesh, team, f'base-{team}')
        instances.append(dict(asset=ozarktic_base.SPIRE_ID, team=team, position=list(spire_at[team]),
                              yaw=base['yaw'], anchors={k: mesh.point(v) for k, v in spire.items()}))
        per_base.append(len(mesh.collision)//9-before)
    # Each team's outpost counts toward its base.
    for team, x, y, z, yaw in outpost_sites(definition):
        before = len(mesh.collision)//9
        mesh.origin = (x, y, z); mesh.yaw = yaw
        post = ozarktic_landmarks.build_outpost(mesh, team, f'base-{team}')
        instances.append(dict(asset=ozarktic_landmarks.OUTPOST_ID, team=team, position=[x, y, z], yaw=round(math.degrees(yaw), 6),
                              anchors={k: mesh.point(v) for k, v in post.items()}))
        per_base[team] += len(mesh.collision)//9-before
    # Each team's tunnel from its outpost down to its base counts toward it.
    sites = terrain_sites(definition)
    profiles = tunnel_profiles(definition, ozarktic_terrain.heights(definition['seed'], sites))
    for team, b, outpost in tunnel_paths(definition):
        before = len(mesh.collision)//9
        anchors = ozarktic_tunnel.build(mesh, team, profiles[team], outpost)
        instances.append(dict(asset=ozarktic_tunnel.ASSET_ID, team=team, anchors=anchors))
        per_base[team] += len(mesh.collision)//9-before
    before = len(mesh.collision)//9
    for (x, y, z, yaw, _), (sx, sy, sz, syaw) in zip(dock_sites(definition), ship_sites(definition)):
        mesh.origin = (x, y, z); mesh.yaw = yaw
        dock = ozarktic_landmarks.build_dock(mesh)
        instances.append(dict(asset=ozarktic_landmarks.DOCK_ID, position=[x, y, z], yaw=round(math.degrees(yaw), 6),
                              anchors={k: [mesh.point(p) for p in v] if isinstance(v, list) else mesh.point(v) for k, v in dock.items()}))
        mesh.origin = (sx, sy, sz); mesh.yaw = syaw
        ship = ozarktic_landmarks.build_ship(mesh)
        instances.append(dict(asset=ozarktic_landmarks.SHIP_ID, position=[sx, sy, sz], yaw=round(math.degrees(syaw), 6),
                              anchors={k: [mesh.point(p) for p in v] if isinstance(v, list) else mesh.point(v) for k, v in ship.items()}))
    x, y, z, yaw = perch_site(definition)
    mesh.origin = (x, y, z); mesh.yaw = yaw
    perch = ozarktic_landmarks.build_perch(mesh)
    instances.append(dict(asset=ozarktic_landmarks.PERCH_ID, position=[x, y, z], yaw=definition['perch']['yaw'],
                          anchors={k: mesh.point(v) for k, v in perch.items()}))
    per_base.append(len(mesh.collision)//9-before)   # landmarks, reported after the bases
    return flags, spawns, spawn_points, instances, per_base


def wind(seed):
    """Original filtered-noise cold wind through pines with slow gusts and a
    faint high whistle: a continuous four-second loop, not a recording. The
    filters are warmed over one identical period so the loop is seamless."""
    rng = random.Random(seed+913); count = 44100*4
    white = [rng.uniform(-1, 1) for _ in range(count)]
    out = array('f'); slow = mid = high = 0.0
    for i in range(count*2):
        n = white[i % count]; slow = .994*slow+.006*n; mid = .8*mid+.2*n; high = .35*high+.65*n
        if i >= count:
            t = math.tau*(i-count)/count
            gust = .5+.32*math.sin(t)+.18*math.sin(2*t+.7)
            out.append((slow*.5+mid*.03+high*.006*gust)*gust)
    if sys.byteorder != 'little': out.byteswap()
    return out.tobytes()


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
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = ozarktic_terrain.weights(grid).tobytes()
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, list(ozarktic_materials.MATERIALS),
                                ozarktic_materials.texture, definition['seed'], kit)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(), 'weights.rgba': bytes(weights),
             'textures.rgba': bytes(textures), 'ambient.f32': wind(definition['seed'])}
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    peak = (ozarktic_terrain.CENTRE, ozarktic_terrain.RIDGE['top']+30, ozarktic_terrain.CENTRE)
    manifest.update(version=1, id=definition['id'], name=definition['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=holes(definition),
        entities=turret_arcs.assign(mesh.entities, flags), instances=instances,
        ambient_emitters=[[peak[0], peak[1]-40, peak[2], .22, 500, 2600]],
        sky={'visibleDistance': '3200', 'fogDistance': '2400', 'fogColor': '0.76 0.82 0.88'},
        asset_sha256=pack_writer.source_hash(ozarktic_base.__file__),
        cnh_asset_sha256=pack_writer.source_hash(cnh_tower.__file__),
        landmark_asset_sha256=pack_writer.source_hash(ozarktic_landmarks.__file__),
        tunnel_asset_sha256=pack_writer.source_hash(ozarktic_tunnel.__file__),
        structure_kit_sha256=pack_writer.source_hash(structure_kit.__file__),
        terrain_source_sha256=pack_writer.source_hash(ozarktic_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(ozarktic_materials.__file__),
        control_points=control_points, look=definition['look'], water_enabled=False,
        provenance='PeakRunner original Ozarktic Blast geometry, original procedural highland terrain, '
                   'original material kit and synthesized wind; no extracted assets, '
                   'no external height data, no recordings',
        definition_sha256=pack_writer.source_hash(DEFINITION))
    pack_writer.add_props_and_shade(kit, files, manifest, 'frostline', definition['seed'], manifest['holes'], flags,
                                    spawn_points, control_points)
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {definition["name"]}: {len(mesh.collision)//9} solid triangles '
          f'({"/".join(str(t) for t in per_base[:2])} per base incl. spire and outpost, {per_base[2]} docks, ships and perch, '
          f'{tower_triangles} C&H towers), '
          f'{len(manifest["holes"])} terrain holes, {len(mesh.vertices)//36} render triangles'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/ozarktic-blast', bake='--no-bake' not in sys.argv)
