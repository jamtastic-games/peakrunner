#!/usr/bin/env python3
"""Test Highgoal (the raised-goal football arena)."""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

import numpy as np

from assets import budgets
from assets import highgoal_arena as arena
from assets import highgoal_terrain as terrain
from assets import highgoal_materials
from assets import spawn_checks
from assets import surface_checks

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT/'assets/maps/highgoal'
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader); loader.loader.exec_module(kit)
build_loader = importlib.util.spec_from_file_location('build', ROOT/'scripts/build-highgoal.py')
build = importlib.util.module_from_spec(build_loader); build_loader.loader.exec_module(build)


def half(team=0):
    mesh = kit.Mesh(); mesh.lamps = []
    mesh.origin = (terrain.CENTRE, 0.0, terrain.CENTRE); mesh.yaw = 0.0 if team == 0 else math.pi
    return mesh, arena.build(mesh, team, centre_marks=team == 0)


class HighgoalTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((PACK/'map.json').read_text())
        cls.world = spawn_checks.World(PACK)

    def test_goal_height_matches_the_engine_constant(self):
        src = (ROOT/'crates/core/src/football.rs').read_text()
        self.assertIn(f'pub const RAISED_GOAL_HEIGHT: f32 = {arena.GOAL_HEIGHT};', src)

    def test_the_zone_sits_on_the_slab_top_and_the_slab_is_solid(self):
        f = self.manifest['football']
        self.assertEqual(f['radius'], arena.ZONE_R)
        for zone in f['end_zones']:
            top = self.world.support(zone[0], zone[1], zone[2])
            self.assertAlmostEqual(top, terrain.FLOOR_Y+arena.GOAL_HEIGHT, places=3)
            self.assertAlmostEqual(zone[1]-top, 1.2, places=3)
            # The whole zone footprint is standable slab.
            for ang in np.linspace(0, 2*np.pi, 12, endpoint=False):
                x, z = zone[0]+math.cos(ang)*(arena.ZONE_R-.5), zone[2]+math.sin(ang)*(arena.ZONE_R-.5)
                self.assertAlmostEqual(self.world.support(x, zone[1], z), top, places=3)
            # Under the slab is open floor between the pillars.
            self.assertAlmostEqual(self.world.support(zone[0], terrain.FLOOR_Y+5, zone[2]), terrain.FLOOR_Y, places=2)

    def test_arena_is_walled_and_bounds_sit_inside(self):
        x0, z0, x1, z1 = self.manifest['football']['bounds']
        c = terrain.CENTRE
        for d in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)):
            hit = self.world.ray((c, terrain.FLOOR_Y+20, c), d, 400)
            reach = terrain.HALF_W if d[0] else terrain.HALF_L
            self.assertAlmostEqual(hit, reach, delta=.05, msg=d)
        self.assertTrue(c-terrain.HALF_W < x0 < x1 < c+terrain.HALF_W and c-terrain.HALF_L < z0 < z1 < c+terrain.HALF_L)

    def test_spawns_are_grounded_and_face_up_the_arena(self):
        self.assertEqual(spawn_checks.problems(PACK), [])
        for team, points in enumerate(self.manifest['spawn_points']):
            self.assertEqual(len(points), 8)
            for x, y, z, yaw in points:
                self.assertAlmostEqual(y, terrain.FLOOR_Y+1.2, places=3)
                self.assertGreater(-math.cos(yaw)*(terrain.CENTRE-z), 0)

    def test_no_z_fighting_symmetry_and_budget(self):
        a, _ = half(0); b, _ = half(1)
        for mesh in (a, b):
            self.assertEqual(surface_checks.z_fighting(mesh.vertices, mesh.collision), [])
            self.assertLess(len(mesh.collision)//9, budgets.COLLISION_TRIS_PER_BASE)
        pa = np.array(a.collision).reshape(-1, 3); pb = np.array(b.collision).reshape(-1, 3)
        pb[:, 0] = 2*terrain.CENTRE-pb[:, 0]; pb[:, 2] = 2*terrain.CENTRE-pb[:, 2]
        self.assertTrue(np.allclose(np.sort(pa, axis=0), np.sort(pb, axis=0), atol=1e-3))

    def test_materials_are_deterministic_and_distinct(self):
        names = ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light']
        textures = {n: highgoal_materials.texture(n, 7, kit.noise, kit.value_noise) for n in names}
        self.assertEqual(textures['concrete'], highgoal_materials.texture('concrete', 7, kit.noise, kit.value_noise))
        means = {n: tuple(np.frombuffer(t, np.uint8).reshape(-1, 4)[:, :3].mean(0).round()) for n, t in textures.items()}
        self.assertEqual(len(set(means.values())), len(names), means)

    def test_unbaked_rebuilds_are_byte_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            digests = []
            for name in ('a', 'b'):
                out = Path(tmp)/name
                build.build(out, bake=False)
                digests.append({f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in out.iterdir()})
            self.assertEqual(digests[0], digests[1])


if __name__ == '__main__':
    unittest.main()
