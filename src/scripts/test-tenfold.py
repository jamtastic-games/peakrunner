#!/usr/bin/env python3
"""Test Tenfold Tower (the duel lobby's tower of blade floors).

    python3 scripts/test-tenfold.py
"""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

import numpy as np

from assets import spawn_checks
from assets import surface_checks
from assets import tenfold_materials
from assets import tenfold_tower as tower

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT/'assets/maps/tenfold'
build_loader = importlib.util.spec_from_file_location('build', ROOT/'scripts/build-tenfold.py')
build = importlib.util.module_from_spec(build_loader); build_loader.loader.exec_module(build)
kit = build.kit
BASE = build.BASE
# The whole tower and plaza: one structure, its own budget.
COLLISION_TRIS = 15000


def polar(r, a, y):
    return (BASE[0]+r*math.sin(a), BASE[1]+y, BASE[2]+r*math.cos(a))


class TenfoldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((PACK/'map.json').read_text())
        cls.world = spawn_checks.World(PACK)

    def test_ten_blade_floors_each_with_its_own_spawns(self):
        zones = self.manifest['zones']
        self.assertEqual(len(zones), tower.FLOORS)
        for k, z in enumerate(zones, 1):
            self.assertEqual(z['kind'], 'blade')
            self.assertTrue(z['name'].startswith(f'Floor {k} · '))
            self.assertEqual(len(z['spawns']), 4)
            y = BASE[1]+tower.floor_y(k)
            for x, sy, sz, _ in z['spawns']:
                top = self.world.support(x, sy, sz)
                self.assertAlmostEqual(sy-top, 1.2, places=2, msg=(k, x, sz))
                self.assertGreaterEqual(top, y-.01, msg=(k, x, sz))
                # Room to stand: nothing within a step all round, at the knee
                # and at the eye.
                for d in range(8):
                    a = d*math.pi/4
                    for h in (-.6, .5):
                        self.assertGreater(self.world.ray((x, sy+h, sz), (math.sin(a), 0, math.cos(a)), 3), 1.0, msg=(k, x, sz, d))

    def test_people_join_on_every_floor_and_the_plaza(self):
        floors = [{tuple(s[:3]) for s in z['spawns']} for z in self.manifest['zones']]
        for team in self.manifest['spawn_points']:
            inside = [tuple(p[:3]) for p in team if math.hypot(p[0]-BASE[0], p[2]-BASE[2]) < tower.R_IN]
            # One on every floor, each a floor spawn of its own.
            self.assertEqual(sorted(k for k, f in enumerate(floors) for q in inside if q in f), list(range(tower.FLOORS)))
            for x, y, z, yaw in team:
                r = math.hypot(x-BASE[0], z-BASE[2])
                if r < tower.R_IN: continue
                self.assertTrue(tower.R_OUT < r < build.PLAZA_R)
                look = (-math.sin(yaw), -math.cos(yaw))
                self.assertGreater(look[0]*(BASE[0]-x)+look[1]*(BASE[2]-z), .99*r)
        # The plaza's face open ground (the floors' are checked above, with
        # room all round instead).
        plaza = json.loads((PACK/'map.json').read_text())
        plaza['spawn_points'] = [[p for p in team if math.hypot(p[0]-BASE[0], p[2]-BASE[2]) > tower.R_OUT] for team in plaza['spawn_points']]
        with tempfile.TemporaryDirectory() as tmp:
            for f in PACK.iterdir(): (Path(tmp)/f.name).symlink_to(f)
            (Path(tmp)/'map.json').unlink(); (Path(tmp)/'map.json').write_text(json.dumps(plaza))
            self.assertEqual(spawn_checks.problems(tmp), [])

    def test_every_floor_meets_the_stair_at_its_landing(self):
        for k in range(1, tower.FLOORS+1):
            y = BASE[1]+tower.floor_y(k)
            # The landing is flush with the floor, and so is the floor off it.
            for r in (3.8, 6.0, 9.0):
                x, _, z = polar(r, 0.0, 0)
                self.assertAlmostEqual(self.world.support(x, y+1, z), y, places=2, msg=(k, r))
            if k == tower.FLOORS: continue
            # Halfway round, the flight is halfway up; it climbs no steeper
            # than a body walks.
            mid = math.pi
            x, _, z = polar((tower.R_CORE+tower.R_STAIR)/2, mid, 0)
            frac = (mid-tower.sector_angle(tower.LANDING))/(tower.sector_angle(tower.SECTORS-tower.LANDING)-tower.sector_angle(tower.LANDING))
            self.assertAlmostEqual(self.world.support(x, y+tower.STOREY*frac+1, z), y+tower.STOREY*frac, delta=.15, msg=k)
            rise = tower.STOREY/(tower.sector_angle(tower.SECTORS-tower.LANDING)-tower.sector_angle(tower.LANDING))
            self.assertLess(math.degrees(math.atan(rise/tower.R_CORE)), 34.0)

    def test_liquids_sit_in_their_pits_inside_the_tower(self):
        volumes = self.manifest['water_volumes']
        lava = [v for v in volumes if v.get('lava')]
        water = [v for v in volumes if not v.get('lava')]
        self.assertTrue(lava and water)
        for v in lava: self.assertGreaterEqual(v['burn'], 300, 'molten metal kills')
        for v in volumes:
            pts = np.array(v['polygon'])
            r = np.hypot(pts[:, 0]-BASE[0], pts[:, 1]-BASE[2])
            self.assertTrue((r > tower.R_STAIR-.01).all() and (r < tower.R_IN+.01).all())
            # Each surface lies just under a floor's level, in a pit.
            y = v['surface']-BASE[1]
            k = round(y/tower.STOREY)+1
            self.assertTrue(-1.0 < y-tower.floor_y(k) < 0, v)
            cx, cz = pts.mean(0)
            self.assertLess(self.world.support(cx, v['surface']+.5, cz), v['surface'], 'the pit floor is under the surface')

    def test_tower_budget_and_no_z_fighting(self):
        mesh = build.Mesh(); mesh.lamps = []
        tower.build(mesh, BASE)
        self.assertLess(len(mesh.collision)//9, COLLISION_TRIS)
        self.assertEqual(surface_checks.z_fighting(mesh.vertices, mesh.collision), [])
        self.assertLess(len(json.loads((PACK/'map.json').read_text())['files']), 20)

    def test_materials_are_deterministic_and_distinct(self):
        names = list(tenfold_materials.PALETTES)
        textures = {n: tenfold_materials.texture(n, 7, kit.noise, kit.value_noise) for n in names}
        self.assertEqual(textures['stone'], tenfold_materials.texture('stone', 7, kit.noise, kit.value_noise))
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
