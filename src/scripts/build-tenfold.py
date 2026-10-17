#!/usr/bin/env python3
"""Tenfold Tower: a duel server's lobby. A round tower of ten floors on a
plaza, every floor a blade zone (`sim::blade`: a blade in every hand, no
jets, a side game that counts for nothing), joined by a spiral stair round a
glowing core; rolling hills round the plaza to ski while you wait.
Original geometry (`assets/tenfold_tower.py`), procedural terrain and an
original material kit; nothing from any other game is read or fitted.

    python3 scripts/build-tenfold.py [OUTPUT] [--no-bake]
"""
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys

from assets import pack_writer
from assets import tenfold_materials
from assets import tenfold_terrain
from assets import tenfold_tower as tower
from assets import terrain_shade

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)

CENTRE = tenfold_terrain.CENTRE
BASE = (CENTRE, 100.0, CENTRE)        # the ground floor's top on the tower's axis
PLAZA_R = 44.0
KIT_LAYERS = len(kit.MATERIALS)+6     # the kit's materials and the six sky faces


def layer_of(material):
    if material in kit.MATERIALS: return kit.MATERIALS.index(material)
    return KIT_LAYERS+tower.EXTRA.index(material)


class Mesh(kit.Mesh):
    """The kit's mesh, with the tower's own materials after the sky's layers."""
    def triangle(self, pts, material, solid=True):
        pts = [self.point(p) for p in pts]
        n = kit.unit(kit.cross(kit.sub(pts[1], pts[0]), kit.sub(pts[2], pts[0])))
        axis = max(range(3), key=lambda i: abs(n[i]))
        uv_axes = [(2, 1), (0, 2), (0, 1)][axis]
        layer = layer_of(material)
        for p in pts:
            uv = (p[uv_axes[0]]/4, p[uv_axes[1]]/4)
            self.vertices.extend((*p, *n, *uv, 0, 0, layer, -1))
        if solid: self.collision.extend(v for p in pts for v in p)


def with_layers(textures, count, images):
    """`textures` (level-major, `count` layers) with `images` appended as new
    layers, each with its mip chain."""
    chains = []
    for img in images:
        chain, side = [img], 256
        while side > 1: chain.append(kit.downsample(chain[-1], side)); side //= 2
        chains.append(chain)
    out, side, offset = bytearray(), 256, 0
    for level in range(9):
        size = side*side*4
        out += textures[offset:offset+count*size]
        for chain in chains: out += chain[level]
        offset += count*size; side //= 2
    return out


def plaza(mesh, b):
    """The plaza round the tower: paving, a ring of lamp posts, benches."""
    mesh.origin, mesh.yaw = BASE, 0.0
    tower.ring_band(mesh, tower.R_OUT, PLAZA_R, 0, math.tau, -0.6, 0.0, 'concrete', segs=48, ends=False, faces='uob', collide='u')
    tower.ring_band(mesh, PLAZA_R, PLAZA_R+.4, 0, math.tau, -1.2, 0.1, 'trim', segs=48, ends=False, faces='uo', collide='')
    for i in range(12):
        a = math.radians(15+30*i)
        x, _, z = tower.pol(36.0, a)
        tower.post(mesh, x, z, .14, 0.0, 4.2, 'trim', sides=6)
        mesh.box((x, 4.3, z), (.5, .2, .5), 'light', False)
        b.lamp((x, 4.0, z), 1.0)
    for i in range(4):
        # Benches between the doors, clear of where people join.
        a = math.radians(45+90*i)
        tower.block(mesh, BASE, 27.0, a, (3.0, .5, .8), 0.0, 'marble')
        mesh.origin, mesh.yaw = BASE, 0.0


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    spec = json.loads((ROOT/'maps/tenfold.json').read_text())
    mesh = Mesh(); mesh.lamps = []
    from assets.structure_kit import Builder
    plaza(mesh, Builder(mesh, 'ember'))
    built = tower.build(mesh, BASE)
    mesh.origin, mesh.yaw = BASE, 0.0
    # Where people join: on every floor of the tower (one spot each team, two
    # of the floor's own spawns), and round the plaza facing the tower; and
    # two flag marks the lobby never shows.
    spawn_points = [[], []]
    for zone in built['zones']:
        for team, k in ((0, 0), (1, 2)):
            x, y, z, yaw = zone['spawns'][k]
            spawn_points[team].append([x, y, z, round(math.radians(yaw) % math.tau, 6)])
    for i in range(8):
        a = math.radians(11.25+45*i)
        x, _, z = tower.pol(30.0, a)
        spawn_points[i % 2].append([round(BASE[0]+x, 3), round(BASE[1]+1.2, 3), round(BASE[2]+z, 3), round(a % math.tau, 6)])
    flags = [[BASE[0]-34.0, BASE[1], BASE[2]], [BASE[0]+34.0, BASE[1], BASE[2]]]
    spawns = [p[0][:3] for p in spawn_points]
    grid = tenfold_terrain.heights(spec['seed'])
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = bytearray(tenfold_terrain.weights(grid).tobytes())
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    count = old['texture_count']
    assert count == KIT_LAYERS
    extra = [tenfold_materials.texture(m, spec['seed']+100+i, kit.noise, kit.value_noise) for i, m in enumerate(tower.EXTRA)]
    textures = with_layers(bytearray(shared['textures']), count, extra)
    count += len(extra)
    pack_writer.paint_materials(textures, count, ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark', 'leaf', 'moss'],
                                tenfold_materials.texture, spec['seed'], kit)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit,
                                                                         zones=tower.ambient_zones(BASE, built['floors']))
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices, 'collision.bin': mesh.collision.tobytes(),
             'weights.rgba': bytes(weights), 'textures.rgba': bytes(textures), 'ambient.f32': shared['ambient']}
    files['shade.rg'], shade = terrain_shade.bake(files['height.bin'], vertices, pack_writer.MAP_SUN, kit.MATERIALS.index('light'))
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    manifest.update(version=1, id=spec['id'], name=spec['name'], lobby=True, flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=[], entities=[],
        instances=[dict(asset=tower.ASSET_ID, position=list(BASE), floors=tower.NAMES)], ambient_emitters=[],
        sky={'visibleDistance': '2600', 'fogDistance': '1500', 'fogColor': spec['fog_color']},
        control_points=[], look=spec['look'], water_enabled=False, water_volumes=built['water_volumes'],
        zones=built['zones'], moveables=tower.moveables(BASE),
        smoke=[[round(BASE[0]+x, 3), round(BASE[1]+tower.floor_y(6)+h, 3), round(BASE[2]+z, 3), 1.2]
               for x, h, z in (tower.pol(r, math.radians(a), h) for r, a, h in tower.STEAM)],
        material_layers={m: layer_of(m) for m in tower.EXTRA},
        terrain_shade=shade,
        asset_sha256=pack_writer.source_hash(tower.__file__),
        terrain_source_sha256=pack_writer.source_hash(tenfold_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(tenfold_materials.__file__),
        provenance='PeakRunner original tower geometry, original procedural terrain and original material kit; no extracted assets or external height data',
        definition_sha256=pack_writer.source_hash(ROOT/'maps/tenfold.json'))
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {spec["name"]}: {len(mesh.collision)//9} solid triangles, {len(mesh.vertices)//12//3} render triangles, '
          f'{len(mesh.lamps)} lamps' + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/tenfold', bake='--no-bake' not in sys.argv)
