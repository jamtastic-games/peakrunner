#!/usr/bin/env python3
"""Highgoal: an original walled sand arena for Football whose goals stand on
raised platforms 7 m up that the jet alone only just reaches.
Original geometry, procedural terrain and material kit only; the classic
community arenas were studied privately for proportions and nothing else.
"""
import importlib.util
import json
from pathlib import Path
import math
import struct
import sys
from assets import highgoal_arena
from assets import highgoal_materials
from assets import highgoal_terrain
from assets import pack_writer
from assets import terrain_shade

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
CENTRE = highgoal_terrain.CENTRE
# Ball resets outside the walls (they stop it first).
BOUNDS_HALF = (highgoal_terrain.HALF_W-.5, highgoal_terrain.HALF_L-.5)


def build(output, bake=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    spec = json.loads((ROOT/'maps/highgoal.json').read_text())
    mesh = kit.Mesh(); mesh.lamps = []
    flags, spawns, spawn_points, zones, kickoff, instances = [], [], [], [], [], []
    for half in spec['halves']:
        team = half['team']
        mesh.origin = (CENTRE, 0.0, CENTRE); mesh.yaw = math.radians(half['yaw'])
        anchors = highgoal_arena.build(mesh, team, centre_marks=team == 0)
        flags.append(mesh.point(anchors['flag']))
        spawns.append(mesh.point(anchors['spawn']))
        zones.append(mesh.point(anchors['end_zone']))
        kickoff.append(mesh.point(anchors['kickoff']))
        spawn_points.append([[*mesh.point(p), (yaw+mesh.yaw) % (2*math.pi)] for *p, yaw in anchors['spawn_points']])
        instances.append(dict(asset=highgoal_arena.ASSET_ID, team=team, yaw=half['yaw'],
            anchors={k: mesh.point(v) for k, v in anchors.items() if k != 'spawn_points'}))
    grid = highgoal_terrain.heights(spec['seed'])
    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    weights = bytearray(highgoal_terrain.weights(grid).tobytes())
    if sys.byteorder != 'little': mesh.vertices.byteswap(); mesh.collision.byteswap()
    shared = kit.base_pack(); old = shared['manifest']
    textures = bytearray(shared['textures'])
    count = old['texture_count']
    pack_writer.paint_materials(textures, count, ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light'],
                                highgoal_materials.texture, spec['seed'], kit)
    vertices = mesh.vertices.tobytes()
    lightmap = None
    if bake:
        vertices, textures, count, lightmap = pack_writer.bake_lightmaps(vertices, textures, count, mesh.lamps, kit)
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices,
             'collision.bin': mesh.collision.tobytes(),
             'weights.rgba': bytes(weights),
             'textures.rgba': bytes(textures),
             'ambient.f32': shared['ambient']}
    # No scattered props in an arena; bake the terrain shade directly.
    files['shade.rg'], shade = terrain_shade.bake(files['height.bin'], vertices, pack_writer.MAP_SUN,
                                                  kit.MATERIALS.index('light'))
    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    bx, bz = BOUNDS_HALF
    manifest.update(version=1, id=spec['id'], name=spec['name'], flags=flags, spawns=spawns,
        exact_spawns=True, spawn_points=spawn_points, holes=[], entities=[], instances=instances, ambient_emitters=[],
        sky={'visibleDistance': '2500', 'fogDistance': '1500', 'fogColor': spec['fog_color']},
        control_points=[], look=spec['look'], water_enabled=False, water_volumes=[],
        football=dict(end_zones=zones, kickoff=kickoff, radius=highgoal_arena.ZONE_R,
                      bounds=[CENTRE-bx, CENTRE-bz, CENTRE+bx, CENTRE+bz]),
        terrain_shade=shade,
        asset_sha256=pack_writer.source_hash(highgoal_arena.__file__),
        terrain_source_sha256=pack_writer.source_hash(highgoal_terrain.__file__),
        material_source_sha256=pack_writer.source_hash(highgoal_materials.__file__),
        provenance='PeakRunner original arena geometry, original procedural terrain and original material kit; no extracted assets or external height data',
        definition_sha256=pack_writer.source_hash(ROOT/'maps/highgoal.json'))
    pack_writer.write_pack(output, files, manifest)
    print(f'Built {spec["name"]}: {len(mesh.collision)//9} solid triangles, {len(mesh.vertices)//12//3} render triangles'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/highgoal', bake='--no-bake' not in sys.argv)
