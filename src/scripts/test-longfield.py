#!/usr/bin/env python3
"""Test Longfield (the football stadium)."""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

import numpy as np

from assets import budgets
from assets import longfield_stadium as stadium
from assets import longfield_terrain as terrain
from assets import longfield_materials
from assets import spawn_checks
from assets import surface_checks
from assets.tower_complex_terrain import slope_degrees

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT/'assets/maps/longfield'
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader); loader.loader.exec_module(kit)
build_loader = importlib.util.spec_from_file_location('build', ROOT/'scripts/build-longfield.py')
build = importlib.util.module_from_spec(build_loader); build_loader.loader.exec_module(build)


def half(team=0):
    mesh = kit.Mesh(); mesh.lamps = []
    mesh.origin = (terrain.CENTRE, 0.0, terrain.CENTRE); mesh.yaw = 0.0 if team == 0 else math.pi
    return mesh, stadium.build(mesh, team, centre_marks=team == 0)


class LongfieldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((PACK/'map.json').read_text())
        cls.grid = terrain.heights(json.loads((ROOT/'maps/longfield.json').read_text())['seed'])

    def test_field_is_flat_and_the_bowl_is_skiable_and_symmetric(self):
        z, x = np.mgrid[0:256, 0:256].astype(np.float64)*8
        on_field = terrain._outside(x, z) <= 0
        self.assertEqual(float(np.ptp(self.grid[on_field])), 0.0, 'the turf is dead flat')
        # Vertex i mirrors 256-i about the centre vertex (index 128 = 1024 m).
        inner = self.grid[1:, 1:]
        self.assertTrue(np.allclose(inner, inner[::-1, ::-1], atol=1e-6), 'point-symmetric bowl')
        s = slope_degrees(self.grid)
        bank = (terrain._outside(x, z) > 10) & (terrain._outside(x, z) < terrain.BANK-10)
        self.assertGreater(float(np.median(s[bank])), 25, 'banks steep enough to ski down')
        self.assertLess(float(s[bank].max()), 60, 'no cliff a player cannot ski')

    def test_field_manifest_matches_the_stadium(self):
        f = self.manifest['football']
        (ex, ey, ez), (gx, gy, gz) = f['end_zones']
        self.assertAlmostEqual(abs(gz-ez), 250, delta=1)
        self.assertEqual((ex, gx), (terrain.CENTRE, terrain.CENTRE))
        self.assertEqual(f['radius'], stadium.ZONE_R)
        x0, z0, x1, z1 = f['bounds']
        for p in f['end_zones']+f['kickoff']:
            self.assertTrue(x0 < p[0] < x1 and z0 < p[2] < z1, p)
            self.assertAlmostEqual(p[1]-terrain.FIELD_Y, 1.2 if p in f['end_zones'] else 1.0, places=3)
        # Each kickoff is on its own team's side of the centre line.
        self.assertLess(f['kickoff'][0][2], terrain.CENTRE); self.assertGreater(f['kickoff'][1][2], terrain.CENTRE)
        self.assertEqual(self.manifest['entities'], [], 'no turrets or stations in football')

    def test_spawns_are_grounded_face_up_field_and_have_room(self):
        self.assertEqual(spawn_checks.problems(PACK), [])
        for team, points in enumerate(self.manifest['spawn_points']):
            self.assertEqual(len(points), 8)
            zone = self.manifest['football']['end_zones'][team]
            for x, y, z, yaw in points:
                self.assertAlmostEqual(y, terrain.FIELD_Y+1.2, places=3)
                self.assertGreater(math.dist((x, z), (zone[0], zone[2])), stadium.ZONE_R+4, 'spawn off its own zone')
                ahead = (-math.sin(yaw), -math.cos(yaw))
                self.assertGreater(ahead[1]*(terrain.CENTRE-z), 0, 'faces up the field')

    def test_the_run_into_each_end_zone_is_open(self):
        w = spawn_checks.World(PACK)
        # Every run in from the field side and both flanks; the goal gate
        # stands behind the zone.
        for zone in self.manifest['football']['end_zones']:
            for ang in np.linspace(0, 2*np.pi, 32, endpoint=False):
                d = np.array([math.cos(ang), 0, math.sin(ang)])
                start = np.array(zone)-d*25
                if (start[2]-zone[2])*(terrain.CENTRE-zone[2]) < 0: continue
                self.assertEqual(w.ray(start, d, 25-stadium.ZONE_R), math.inf, ('blocked approach', zone, ang))

    def test_no_z_fighting_and_collision_within_budget(self):
        for team in (0, 1):
            mesh, _ = half(team)
            self.assertEqual(surface_checks.z_fighting(mesh.vertices, mesh.collision), [])
            self.assertLess(len(mesh.collision)//9, budgets.COLLISION_TRIS_PER_BASE)

    def test_halves_are_point_symmetric(self):
        a, anchors_a = half(0); b, anchors_b = half(1)
        pa = np.array(a.collision).reshape(-1, 3); pb = np.array(b.collision).reshape(-1, 3)
        mirrored = pb.copy(); mirrored[:, 0] = 2*terrain.CENTRE-pb[:, 0]; mirrored[:, 2] = 2*terrain.CENTRE-pb[:, 2]
        self.assertTrue(np.allclose(np.sort(pa, axis=0), np.sort(mirrored, axis=0), atol=1e-3))

    def test_materials_are_deterministic_and_distinct(self):
        names = ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark']
        textures = {n: longfield_materials.texture(n, 7, kit.noise, kit.value_noise) for n in names}
        self.assertEqual(textures['panel'], longfield_materials.texture('panel', 7, kit.noise, kit.value_noise))
        means = {n: tuple(np.frombuffer(t, np.uint8).reshape(-1, 4)[:, :3].mean(0).round()) for n, t in textures.items()}
        self.assertEqual(len(set(means.values())), len(names), means)
        self.assertTrue(all(len(t) == 256*256*4 for t in textures.values()))

    def test_unbaked_rebuilds_are_byte_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            digests = []
            for name in ('a', 'b'):
                out = Path(tmp)/name
                build.build(out, bake=False)
                digests.append({f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in out.iterdir()})
            self.assertEqual(digests[0], digests[1])
            self.assertEqual(len(digests[0]), 8)   # map.json, six payloads, shade.rg


if __name__ == '__main__':
    unittest.main()
