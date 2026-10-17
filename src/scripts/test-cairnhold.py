#!/usr/bin/env python3
"""Test the Cairnhold builders.
Run from src/ with scripts/requirements.txt installed: scripts/test-cairnhold.py"""
import hashlib
import importlib.util
import math
from pathlib import Path
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from assets import budgets
from assets import fairness
from assets import cairnhold_base as base
from assets import cairnhold_materials
from assets import cairnhold_ring
from assets import cairnhold_terrain
from assets import sightline_checks
from assets import turret_arcs

HERE = Path(__file__).parent
def _load(name, file):
    loader = importlib.util.spec_from_file_location(name, HERE/file)
    module = importlib.util.module_from_spec(loader); loader.loader.exec_module(module)
    return module
kit = _load('kit', 'build-original-map.py')
build = _load('cairnhold_build', 'build-cairnhold.py')

LIFT = base.SPAWN_LIFT


class Soup:
    """Vectorised ray queries against a collision triangle soup."""
    def __init__(self, collision):
        t = np.array(collision, np.float64).reshape(-1, 3, 3)
        self.a = t[:, 0]; self.e1 = t[:, 1]-t[:, 0]; self.e2 = t[:, 2]-t[:, 0]
        n = np.cross(self.e1, self.e2); self.n = n/(np.linalg.norm(n, axis=1, keepdims=True)+1e-12)
        self.lo = t.min(1); self.hi = t.max(1); self.tris = t

    def hits(self, o, d, tmax=np.inf):
        """Distances and normal.y of every hit along a ray, nearest first."""
        o, d = np.asarray(o, float), np.asarray(d, float)
        end = o+d*min(tmax, 1e4)
        m = np.all(self.hi >= np.minimum(o, end)-1e-6, 1) & np.all(self.lo <= np.maximum(o, end)+1e-6, 1)
        a, e1, e2 = self.a[m], self.e1[m], self.e2[m]
        h = np.cross(d, e2); det = np.einsum('ij,ij->i', e1, h)
        ok = np.abs(det) > 1e-9; det = np.where(ok, det, 1); s = o-a
        u = np.einsum('ij,ij->i', s, h)/det; q = np.cross(s, e1)
        v = (q@d)/det; t = np.einsum('ij,ij->i', e2, q)/det
        k = ok & (u >= -1e-7) & (v >= -1e-7) & (u+v <= 1+1e-7) & (t >= 0) & (t <= tmax)
        order = np.argsort(t[k])
        return t[k][order], self.n[m][k][order, 1]

    def first(self, o, d, tmax=np.inf):
        t, _ = self.hits(o, d, tmax)
        return t[0] if len(t) else np.inf

    def blocked_many(self, starts, ends):
        """For segments start->end (N,3), whether any triangle is crossed."""
        out = np.zeros(len(starts), bool)
        for i in range(0, len(starts), 256):
            s = starts[i:i+256][:, None]; d = ends[i:i+256][:, None]-s
            e1, e2, a = self.e1[None], self.e2[None], self.a[None]
            h = np.cross(d, e2); det = (e1*h).sum(-1)
            ok = np.abs(det) > 1e-9; inv = np.where(ok, 1/np.where(ok, det, 1), 0)
            tv = s-a; u = (tv*h).sum(-1)*inv; q = np.cross(tv, e1)
            v = (d*q).sum(-1)*inv; t = (e2*q).sum(-1)*inv
            out[i:i+256] = (ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > 1e-4) & (t < 1-1e-4)).any(1)
        return out


def one_base(team=0, circuit='one', origin=(0, 0, 0), yaw=0.0):
    mesh = kit.Mesh(); mesh.origin = origin; mesh.yaw = yaw
    anchors = base.build(mesh, team, circuit)
    return mesh, anchors


class CairnholdTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mesh, cls.anchors = one_base()
        cls.soup = Soup(cls.mesh.collision)

    # --- Floors, headroom and standing support ----------------------------
    def test_room_floors_have_floor_and_headroom(self):
        hx0, hx1, hz0, hz1 = base.HALL_HOLE
        samples = [(x, z, 0) for x in (-12, -8, 8, 12) for z in (-13, -6, 2)
                   if not (hx0 <= x <= hx1 and hz0 <= z <= hz1)]                        # hall, off the stair opening
        samples += [(x, z, base.VAULT_FLOOR) for x in (4.2, 9.8) for z in (-8, -2)]      # vault
        samples += [(x, z, base.VAULT_FLOOR) for x in (12.0, 14.4) for z in (5, 11)]      # walkway by the well
        samples += [(x, z, base.PIT_FLOOR) for x, z in ((4.0, 11.2), (9.6, 11.2), (6.8, 4.4))]  # generator well
        samples += [(x, 8, 3.0) for x in (48, 64, 84)] + [(84, z, 3.0) for z in (-4, -16)]  # sally port, level runs
        samples += [(x, z, base.EXIT_GROUND) for x in (82, 86) for z in (-42.2, -37)]    # exit house
        samples += [(x, z, 0) for x in (-12, 12) for z in (6, 10)]                  # inventory room
        samples += [(x, 16, 0) for x in (0, 10, 13)]                                # generator room
        samples += [(x, z, base.HUT_FLOOR) for x in (-5, 0, 6) for z in (base.HZ0+4, base.HZ0+12, base.HZ0+20)]  # hut
        samples += [(x, 78, base.HUT_ROOF) for x in (9, 12, 14)]                                    # tower floor
        samples += [(x, z, base.TOWER_MID) for x in (3.5, 9, 14) for z in (93, 94.5)]              # tower landing
        for x, z, y in samples:
            t, ny = self.soup.hits((x, y+3, z), (0, -1, 0))
            self.assertTrue(len(t) and abs(3-t[0]) < 1e-6 and ny[0] > .999, (x, z, 'floor'))
            self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 2.4, (x, z, 'headroom'))

    def test_trench_climbs_with_headroom(self):
        for z in np.arange(base.TZ0+.5, base.TZ1, 2.0):
            for x in (2.2, 4, 5.8):
                y = base.trench_floor(z)
                t, ny = self.soup.hits((x, y+2, z), (0, -1, 0))
                self.assertLess(abs(2-t[0]), .02, (x, z))
                self.assertGreater(ny[0], .9, (x, z, 'trench slope'))
                self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 4.0, (x, z, 'trench headroom'))
        self.assertLess(math.degrees(math.atan(base.TRENCH_RISE/(base.TZ1-base.TZ0))), 23)

    def test_stand_ramp_reaches_the_roof_through_the_hatch(self):
        r0, r1, zl, zh = base.RAMP
        rise = base.HUT_ROOF-base.HUT_FLOOR
        for z in np.arange(zh+.5, zl, 1.5):
            y = base.HUT_FLOOR+rise*(zl-z)/(zl-zh)
            t, _ = self.soup.hits(((r0+r1)/2, y+1, z), (0, -1, 0))
            self.assertLess(abs(1-t[0]), .02, (z, 'ramp surface'))
            self.assertGreater(self.soup.first(((r0+r1)/2, y+.2, z), (0, 1, 0)), 2.4, (z, 'ramp headroom'))
        # Nothing can be walked into underneath the ramp: its sides are closed.
        for z in np.arange(zh+1, zl-1, 2.0):
            self.assertLess(self.soup.first((r0-1, base.HUT_FLOOR+.5, z), (1, 0, 0)), 1.01, z)

    def test_standing_support_is_the_floor_top_everywhere(self):
        """The engine's support ray starts 0.15 m above the feet. Wherever a
        player can stand, its first hit must be the up-facing floor top."""
        tx0, tx1, tz0, tz1 = base.TW
        regions = [(-15, 15, -19, 19, 0), (base.HX0+1, base.HX1-1, base.HZ0+1, base.HZ1-1, base.HUT_FLOOR),
                   (base.HX0+.5, base.HX1-.5, base.HZ0+.5, base.HZ1-.5, base.HUT_ROOF),
                   (-15.5, 15.5, -19.5, 19.5, base.ROOF),
                   (tx0+.6, tx1-.6, tz0+.6, tz1-.6, base.TOWER_TOP), (tx0+1, tx1-1, 92.2, 95, base.TOWER_MID),
                   (base.BRIDGE[0]+.6, base.BRIDGE[1], base.BRIDGE[2]+.6, base.BRIDGE[3]-.6, base.TOWER_TOP),
                   (-5.1, 5.1, -21.1, -13.9, base.TIERS[0][2])]
        for s in (-1, 1):
            for x0, x1, z0, z1, level in [(6.2, 10.8, -19.1, -13.2, base.TIERS[1][2]),
                                          (12.6, 15.8, -19.1, -13.2, base.TIERS[2][2]),
                                          (16.2, 23.8, -19.8, -12.2, base.ROOF)]:
                a, c = sorted((s*x0, s*x1)); regions.append((a, c, z0, z1, level))
        px, pz = base.PAD; H = base.PAD_HALF-.6
        regions.append((px-H, px+H, pz-H, pz+H, base.PAD_TOP))
        bx, bz = base.BATTERY; r = base.BATTERY_R*.9
        regions.append((bx-r, bx+r, bz-r, bz+r, base.BATTERY_GROUND+base.BATTERY_H))
        vx0, _, vz0, vz1 = base.VAULT
        regions += [(vx0+.6, base.STAIR[0]-.6, vz0+.6, vz1-.6, base.VAULT_FLOOR),
                    (base.PIT[0]+.6, base.PIT[1]-.6, base.PIT[2]+.6, base.PIT[3]-.6, base.PIT_FLOOR),
                    (41.5, 87.0, 5.4, 10.6, 3.0), (81.4, 86.6, -19.5, 4.0, 3.0),
                    (81.4, 86.6, -42.6, -36.2, base.EXIT_GROUND)]
        checked = 0
        for x0, x1, z0, z1, level in regions:
            for x in np.arange(x0, x1+.01, .8):
                for z in np.arange(z0, z1+.01, .8):
                    t, ny = self.soup.hits((x, level+2.6, z), (0, -1, 0))
                    if not len(t) or abs(2.6-t[0]) > .02 or np.any((np.abs(t-t[0]) < .01) & (ny < .99)): continue
                    first, fy = self.soup.hits((x, level+.15, z), (0, -1, 0))
                    checked += 1
                    self.assertTrue(len(first) and first[0] < .17 and fy[0] > .99, (x, z, level, first[:2]))
        self.assertGreater(checked, 2500)

    def test_no_accidental_slopes_on_walkable_surfaces(self):
        n, cen = self.soup.n, self.soup.tris.mean(1)
        ents = [e['position'] for e in self.mesh.entities]
        r0, r1, zl, zh = base.RAMP
        bx, bz = base.BATTERY
        for i in np.where((n[:, 1] > .3) & (n[:, 1] < .9999))[0]:
            x, y, z = cen[i]
            if base.TX0 <= x <= base.TX1 and base.TZ0 <= z <= base.TZ1: continue        # trench floor/roof
            if base.STAIR[0] <= x <= base.STAIR[1] and base.STAIR[2] <= z <= base.STAIR[3]: continue  # vault stair
            if base.PIT_RAMP[0] <= x <= base.PIT_RAMP[1] and base.PIT_RAMP[2] <= z <= base.PIT_RAMP[3]: continue  # well ramp
            if 16 <= x <= base.T_EAST_FLOOR[2][0]+.1 and base.T_EAST[2] <= z <= base.T_EAST[3]: continue  # sally port, east climb
            if base.T_SOUTH[0] <= x <= base.T_SOUTH[1] and -36 <= z <= -20: continue    # sally port, south climb
            if y > 5 and any(x0 <= x <= x1 and z0 <= z <= z1 for name, (x0, x1, z0, z1) in base.HOLES.items()
                             if name.startswith('tunnel')): continue                     # lids: ground over the tunnel
            if r0 <= x <= r1 and zh <= z <= zl: continue                                 # hut ramp
            if base.R1[0] <= x <= base.R1[1] and base.R1[2] <= z <= base.R1[3]: continue  # tower R1
            if base.R2[0] <= x <= base.R2[1] and base.R2[3] <= z <= base.R2[2]: continue  # tower R2
            if base.XR[0] <= x <= base.XR[1] and base.XR[3] <= z <= base.XR[2]: continue  # exterior ramp
            if math.hypot(x-bx, z-bz) < base.BATTERY_R+12: continue                      # battery plinth + ramp
            if math.hypot(x-base.FLAG[0], z-base.FLAG[1]) < 2.1: continue                # flag plinth chamfer
            if any(math.hypot(x-e[0], z-e[2]) < 3.3 for e in ents): continue            # kit equipment
            if abs(x-base.MAST[0]) < 1 and abs(z-base.MAST[1]) < 1: continue             # sentry mast
            self.fail(('accidental slope', round(float(n[i, 1]), 4), cen[i]))

    def test_no_overlapping_coplanar_floor_plates(self):
        class Recording(kit.Mesh):
            def __init__(self):
                super().__init__(); self.plates = []
            def box(self, p, size, mat='concrete', solid=True):
                if solid and size[1] <= 1.25:
                    self.plates.append((p[1]+size[1]/2, p, size))
                super().box(p, size, mat, solid)
        mesh = Recording(); base.build(mesh, 0, 'one')
        for i, (ta, a, sa) in enumerate(mesh.plates):
            for tb, b, sb in mesh.plates[i+1:]:
                if abs(ta-tb) > 1e-6: continue
                overlap = all(min(a[k]+sa[k]/2, b[k]+sb[k]/2)-max(a[k]-sa[k]/2, b[k]-sb[k]/2) > 1e-6 for k in (0, 2))
                self.assertFalse(overlap, f'coplanar plate overlap at y={ta}: {a} {sa} / {b} {sb}')

    # --- Spawns, flag, equipment ------------------------------------------
    def test_spawn_points_stand_on_floor_with_room_and_face_open_space(self):
        points = self.anchors['spawn_points']
        self.assertEqual(len(points), 8)
        self.assertEqual(tuple(self.anchors['spawn']), tuple(points[0][:3]))
        for x, y, z, yaw in points:
            floor = y-LIFT
            t, ny = self.soup.hits((x, y-.37, z), (0, -1, 0))
            self.assertAlmostEqual(y-.37-t[0], floor, places=3); self.assertGreater(ny[0], .999)
            self.assertGreater(self.soup.first((x, floor+.2, z), (0, 1, 0)), 2.4, (x, z, 'headroom'))
            for ang in np.linspace(0, 2*np.pi, 8, endpoint=False):
                for h in (.3, 1., 1.8):
                    self.assertGreater(self.soup.first((x, floor+h, z), (np.cos(ang), 0, np.sin(ang))), .7, (x, z, h))
            facing = (-math.sin(yaw), 0, -math.cos(yaw))
            self.assertGreater(self.soup.first((x, floor+1.6, z), facing), 3., (x, z, 'faces a wall'))

    def test_flag_stands_on_its_plinth_open_to_the_sky(self):
        fx, fy, fz = self.anchors['flag']
        top = base.TOWER_TOP+base.PLINTH
        self.assertTrue(12 <= base.TOWER_TOP-base.HUT_RING <= 18, 'the flag platform stands 12-18 m over the knoll')
        t, ny = self.soup.hits((fx, fy+1, fz), (0, -1, 0))
        self.assertAlmostEqual(fy+1-t[0], top, places=4)
        self.assertAlmostEqual(fy, top+.05, places=6)
        self.assertEqual(self.soup.first((fx, fy+.1, fz), (0, 1, 0)), np.inf, 'flag must be open to the sky')

    def test_transform_invariance_and_budget(self):
        a, anchors = one_base(0, 'one')
        b, moved = one_base(1, 'two', (321, 75, 987), .73)
        self.assertEqual(anchors, moved)
        self.assertEqual(len(a.collision), len(b.collision))
        for i in range(0, len(a.collision), 3):
            expected = b.point(tuple(a.collision[i:i+3]))
            for x, y in zip(expected, b.collision[i:i+3]): self.assertAlmostEqual(x, y, places=3)
        self.assertTrue(all(e['team'] == 1 and e['circuit'] == 'two' for e in b.entities))
        kinds = sorted(e['kind'] for e in b.entities)
        self.assertEqual(kinds, ['generator', 'inventory', 'inventory', 'sensor', 'turret', 'turret', 'turret'])
        self.assertEqual(sorted(e['weapon'] for e in b.entities if e['kind'] == 'turret'), ['bullet', 'bullet', 'plasma'])
        self.assertLess(len(b.collision)//9, budgets.COLLISION_TRIS_PER_BASE)

    def test_two_instances_have_unique_ids_and_independent_circuits(self):
        mesh = kit.Mesh(); base.build(mesh, 0, 'red')
        mesh.origin = (2048, 0, 2048); mesh.yaw = math.pi; base.build(mesh, 1, 'blue')
        ids = [e['id'] for e in mesh.entities]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual({e['circuit'] for e in mesh.entities}, {'red', 'blue'})
        for team in (0, 1):
            self.assertEqual(sum(e['kind'] == 'generator' and e['team'] == team for e in mesh.entities), 1)

    def test_materials_are_deterministic_opaque_and_distinct(self):
        names = ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark']
        tex = {}
        for name in names:
            a = cairnhold_materials.texture(name, 7, kit.noise, kit.value_noise)
            self.assertEqual(a, cairnhold_materials.texture(name, 7, kit.noise, kit.value_noise), name)
            self.assertEqual(len(a), 256*256*4)
            self.assertTrue(all(a[i] == 255 for i in range(3, len(a), 4)), name)
            tex[name] = a
        for i, a in enumerate(names):
            for b in names[i+1:]: self.assertNotEqual(tex[a], tex[b], (a, b))
        from assets import tower_complex_materials
        for name in names:
            self.assertNotEqual(tex[name], tower_complex_materials.texture(name, 7, kit.noise, kit.value_noise), name)

    # --- Facade, flag tower, sunken trench ----------------------------------
    def test_facade_steps_up_and_recesses_the_portal(self):
        tops = [t for _, _, t in base.TIERS]+[base.ROOF]
        self.assertEqual(tops, sorted(tops, reverse=True))
        self.assertGreaterEqual(base.TIERS[0][2], 2*base.ROOF, 'gatehouse rises well above the old roof line')
        # The portal is open from the apron through the door and across the
        # hall: nothing stands behind the door before the first partition.
        for y in (.3, 1.2, 2.2, 3.8):
            for x in (-1.8, 0, 1.8):
                t = self.soup.first((x, y, -30), (0, 0, 1))
                self.assertGreater(t, base.BZ0+base.WALL+30+8, msg=(x, y))
        # Recess: the jambs stand 2 m proud of the front wall either side of the door.
        self.assertAlmostEqual(self.soup.first((4.8, 2, -30), (0, 0, 1)), base.GATE_Z+30, delta=.01)
        self.assertAlmostEqual(self.soup.first((3.2, 2, -30), (0, 0, 1)), base.BZ0+30, delta=.01)
        # Over the 6 m door the roof slab's front edge closes the opening.
        self.assertAlmostEqual(self.soup.first((0, 6.5, -30), (0, 0, 1)), base.BZ0+30, delta=.01)
        # The roof turret stands on the gatehouse.
        turret = [e for e in self.mesh.entities if e['kind'] == 'turret']
        self.assertTrue(any(abs(e['position'][1]-base.TIERS[0][2]-2.8) < 1e-6 for e in turret))

    def test_flag_tower_route_climbs_from_the_hut_to_the_platform(self):
        def climb(ramp, y_a, y_b, lo, hi):
            x = (ramp[0]+ramp[1])/2
            for z in np.arange(lo, hi, 1.0):
                y = base.ramp_y(ramp, z, y_a, y_b)
                t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
                self.assertLess(abs(1-t[0]), .02, (ramp, z, 'surface'))
                self.assertGreater(abs(ny[0]), .85, (ramp, z))    # the engine's sweep is double-sided
                self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 2.4, (ramp, z, 'headroom'))
        climb(base.R1, base.HUT_ROOF, base.TOWER_MID, base.R1[2]+.3, base.R1[3])
        climb(base.R2, base.TOWER_MID, base.TOWER_TOP, base.R2[3]+.3, base.R2[2])
        # The top hatch is open above R2; the hut hatch is open above the hut ramp.
        g0, g1, ga, gb = base.TOP_HATCH
        for x in np.arange(g0+.3, g1, 1.0):
            for z in np.arange(ga+.3, gb, 1.0):
                y = base.ramp_y(base.R2, z, base.TOWER_MID, base.TOWER_TOP)
                self.assertEqual(self.soup.first((x, y+.2, z), (0, 1, 0)), np.inf, (x, z, 'open hatch'))
        # Nothing hides under R1: its open side is closed down to the floor.
        for z in np.arange(base.R1[2]+1, base.R1[3], 2.0):
            self.assertLess(self.soup.first((base.R1[1]+1, base.HUT_ROOF+.4, z), (-1, 0, 0)), 1.01, z)
        # The platform stands 12-18 m over the knoll, with open sky over the deck.
        self.assertTrue(12 <= base.TOWER_TOP-base.HUT_RING <= 18)
        self.assertEqual(self.soup.first((base.FLAG[0], base.TOWER_TOP+.5, base.FLAG[1]+3), (0, 1, 0)), np.inf)

    def test_exterior_ramp_and_bridge_reach_the_platform(self):
        xa, xb, zg, zt = base.XR
        slope = math.degrees(math.atan((base.TOWER_TOP-base.HUT_RING)/(zg-zt)))
        self.assertLess(slope, 27)
        x = (xa+xb)/2
        for z in np.arange(zt+.3, zg-.3, 1.0):
            y = base.exterior_ramp_y(z)
            t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
            self.assertLess(abs(1-t[0]), .02, (z, 'surface'))
            self.assertEqual(self.soup.first((x, y+.2, z), (0, 1, 0)), np.inf, (z, 'open sky'))
        # Its underside is closed on both sides along its whole height.
        for z in np.arange(zt+1, zg-2, 2.0):
            for sx in (-1, 1):
                edge = xa if sx < 0 else xb
                y = max(base.HUT_ROOF if z < base.HZ1 else base.HUT_RING, base.exterior_ramp_y(z)-2)+.3
                if y > base.exterior_ramp_y(z)-.7: continue
                self.assertLess(self.soup.first((edge+sx, y, z), (-sx, 0, 0)), 1.01, (z, sx))
        # From the bridge a body walks straight onto the platform.
        for h in (.3, 1.0, 1.8):
            self.assertGreater(self.soup.first((base.BRIDGE[0]+1, base.TOWER_TOP+h, 77.8), (1, 0, 0)), 9, h)

    # --- Vault and sally port (v3) ------------------------------------------
    def test_vault_has_exactly_two_ways_in(self):
        """The vault's walls have one opening (the tunnel door); the only other
        way in is the stair from the hall. Its ceiling is closed except over
        the stair, and the generator stands on its floor."""
        F = base.VAULT_FLOOR
        vx0, vx1, vz0, vz1 = base.VAULT
        gx, gy, gz = self.anchors['generator']
        px0, px1, pz0, pz1 = base.PIT
        self.assertTrue(px0 < gx < px1 and pz0 < gz < pz1 and gy == base.PIT_FLOOR)
        # The 5.8 m kit generator clears the ceiling with room for its hit bar
        # (3.9 m over its entity point, 2.5 m up the model).
        self.assertGreater(self.soup.first((gx+2.6, base.PIT_FLOOR+5.9, gz), (0, 1, 0)), .9, 'no room over the generator')
        self.assertLess(base.PIT_FLOOR+2.5+3.9, base.VAULT_CEIL-.3)
        # Its ramp is under 30 degrees and closed beneath.
        r0, r1, rza, rzb = base.PIT_RAMP
        self.assertLess(math.degrees(math.atan((F-base.PIT_FLOOR)/(rzb-rza))), 30)
        for z in np.arange(rza+.3, rzb-.2, .6):
            y = F+(base.PIT_FLOOR-F)*(z-rza)/(rzb-rza)
            t, ny = self.soup.hits(((r0+r1)/2, y+1, z), (0, -1, 0))
            self.assertLess(abs(1-t[0]), .02, z); self.assertGreater(ny[0], .85, z)
        step = .25
        sides = {'south': [((x, vz0+.3), (0, 0, -1)) for x in np.arange(vx0+.3, base.STAIR[0]-.29, step)],
                 'north': [((x, vz1-.3), (0, 0, 1)) for x in np.arange(vx0+.3, vx1-.29, step)],
                 'west': [((vx0+.3, z), (-1, 0, 0)) for z in np.arange(vz0+.3, vz1-.29, step)],
                 'east': [((vx1-.3, z), (1, 0, 0)) for z in np.arange(base.STAIR[3]+.4, vz1-.29, step)]}
        runs = {}
        for name, probes in sides.items():
            widths, width = [], 0
            for (x, z), d in probes + [((0, 0), None)]:
                if d is not None and all(self.soup.first((x, F+h, z), d, 2.0) == np.inf for h in (.6, 1.6)): width += step
                elif width: widths.append(width); width = 0
            runs[name] = widths
        self.assertEqual(runs['south'], []); self.assertEqual(runs['west'], []); self.assertEqual(runs['north'], [])
        self.assertEqual(len(runs['east']), 1, runs['east'])
        self.assertAlmostEqual(runs['east'][0], base.TUN_DOOR[1]-base.TUN_DOOR[0]-.3, delta=.6)
        for x in np.arange(vx0+.5, base.STAIR[0], 1.0):
            for z in np.arange(vz0+.5, vz1, 1.0):
                self.assertLess(self.soup.first((x, F+.3, z), (0, 1, 0)), base.VAULT_CEIL-F, (x, z, 'open ceiling'))
        # The stair: under 30 degrees, headroom all the way, closed beneath its open edge.
        s0, s1, zh, zf = base.STAIR
        self.assertLess(math.degrees(math.atan(-F/(zf-zh))), 30)
        for z in np.arange(zh+.3, zf-.2, .7):
            for x in (s0+.6, (s0+s1)/2, s1-.6):
                y = base.stair_y(z)
                t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
                self.assertLess(abs(1-t[0]), .02, (x, z)); self.assertGreater(ny[0], .85, (x, z))
                # The engine's body sweep reaches 2.64 m above a surface.
                self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 2.6, (x, z, 'headroom'))
            if base.stair_y(z)-.9 > F+.3:
                self.assertLess(self.soup.first((s0-1, F+.3, z), (1, 0, 0)), 1.01, (z, 'open under the stair'))

    def test_sally_port_is_walkable_roofed_and_lidded(self):
        """Along the whole tunnel: a floor at `tunnel_floor`, 6.5 m of headroom
        (or open sky under the two skylights; checked to 4.2 m here),
        walls either side, no slope over 22 degrees, and its lid top flush with
        the ring height it pins the terrain to."""
        path = [(x, 8.0) for x in np.arange(16.3, 84.0, .7)] + [(84.0, z) for z in np.arange(8.0, -42.5, -.7)]
        for (x0, z0), (x1, z1) in zip(path, path[1:]):
            dy = base.tunnel_floor(x1, z1)-base.tunnel_floor(x0, z0)
            self.assertLess(math.degrees(math.atan(abs(dy)/math.hypot(x1-x0, z1-z0))), 22.0, (x0, z0))
        for x, z in path:
            y = base.tunnel_floor(x, z)
            t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
            self.assertLess(abs(1-t[0]), .03, (x, z)); self.assertGreater(ny[0], .9, (x, z))
            self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 4.2, (x, z, 'headroom'))
            if x > base.T_SOUTH[0] and z > base.T_SOUTH[3]-.1: continue                  # the open corner
            if z < base.T_EXIT[3]: continue                                              # exit house (door, vestibule)
            across = (0, 0, 1) if z > base.T_SOUTH[3]-.1 else (1, 0, 0)
            for s in (1, -1):
                self.assertLess(self.soup.first((x, y+1.2, z), np.multiply(across, s), 8.0), 3.4, (x, z, 'wall'))
        for name in ('tunnel east', 'tunnel south'):
            x0, x1, z0, z1 = base.HOLES[name]
            for x in np.arange(x0+.2, x1, 2.0):
                for z in np.arange(z0+.2, z1, 2.0):
                    well = name == 'tunnel east' and any(s0 <= x <= s1 for s0, s1 in base.SKYLIGHTS) \
                        and z0+base.WALL < z < z1-base.WALL
                    t = self.soup.first((x, 60, z), (0, -1, 0))
                    want = base.tunnel_floor(x, z) if well else base.ring_height(name, x, z)
                    self.assertAlmostEqual(60-t, want, delta=.02, msg=(name, x, z, 'skylight' if well else 'lid'))

    def test_exit_house_door_is_open_and_the_battery_faces_away(self):
        """The exit house's door is open straight in: a body band walks from
        the bench through the door and into the tunnel. The battery, whose
        barrel used to look down it, faces the field and never engages a
        player inside the house or the tunnel."""
        ox0, ox1, oz0, oz1 = base.T_EXIT
        g = base.EXIT_GROUND
        door_z = sum(base.EXIT_DOOR)/2
        route = [(ox1+3, door_z), (ox0+2.0, door_z), (ox0+2.0, oz1+1.0)]
        for (x0, z0), (x1, z1) in zip(route, route[1:]):
            for off in (-.5, 0, .5):
                dx, dz = x1-x0, z1-z0; n = math.hypot(dx, dz); px, pz = -dz/n*off, dx/n*off
                a, c = np.array((x0+px, g+1.2, z0+pz)), np.array((x1+px, g+1.2, z1+pz))
                self.assertEqual(self.soup.first(a, (c-a)/n, n), np.inf, ((x0, z0), (x1, z1), off))
        spec, mesh, soup, _, _ = self.whole_map()
        battery = [e for e in mesh.entities if e['kind'] == 'turret' and e['weapon'] == 'plasma']
        self.assertEqual(len(battery), 2)
        for e, b in zip(sorted(battery, key=lambda e: e['team']), sorted(spec['bases'], key=lambda b: b['team'])):
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            inside = [m.point((x, g+LIFT+.8, z)) for x in np.arange(ox0+1.4, ox1-1, 1.0) for z in np.arange(oz0+1.4, oz1, 1.0)]
            inside += [m.point((x, base.south_floor(z)+LIFT+.8, z)) for x in (81.6, 84.0, 86.4) for z in np.arange(-35.5, -10, 2.0)]
            self.assertFalse(sightline_checks.visible(soup, e, np.array(inside)).any(), e['id'])

    def test_no_spawn_camps_the_generator(self):
        """No spawn in the hall, the vault or the tunnel; all at least 15 m in
        a straight line (further on foot) from the stair head."""
        hx, _, hz = self.anchors['stair_head']
        for x, y, z, _ in self.anchors['spawn_points']:
            self.assertFalse(-base.BX < x < base.BX and base.BZ0 < z < base.PART_A[0] and y < 2, (x, z, 'in the hall'))
            self.assertGreater(y, 0, (x, z, 'below the hall floor'))
            self.assertGreaterEqual(math.hypot(x-hx, z-hz), 15.0, (x, z))

    def test_hole_cells_never_fall_through(self):
        """Every point over a dug-in footprint has collision under it at or
        above that structure's lowest floor, so there is no seam to drop
        through between the cut terrain and the structure."""
        floors = {'bunker': lambda x, z: -.05, 'trench': lambda x, z: base.trench_floor(z)-.05,
                  'hut': lambda x, z: base.HUT_FLOOR-.05,
                  'tunnel east': lambda x, z: base.east_floor(x)-1.05, 'tunnel south': lambda x, z: base.south_floor(z)-1.05,
                  'sally exit': lambda x, z: base.EXIT_GROUND-1.05}
        for name, (x0, x1, z0, z1) in base.HOLES.items():
            xs = np.concatenate([np.arange(x0+.1, x1, 1.0), [x0+.02, x1-.02]])
            zs = np.concatenate([np.arange(z0+.1, z1, 1.0), [z0+.02, z1-.02]])
            for x in xs:
                for z in zs:
                    t = self.soup.first((x, 90, z), (0, -1, 0))
                    self.assertLess(t, np.inf, (name, x, z))
                    self.assertGreaterEqual(90-t, floors[name](x, z), (name, x, z, 90-t))

    def test_trench_is_sunk_flush_with_the_hillside(self):
        spec, _, _, grid, _ = self.whole_map()
        for b in spec['bases']:
            oy = b['position'][1]
            for z in np.arange(base.TZ0+2, base.TZ1-2, 3.0):
                roof = base.trench_roof(z)
                for x in (base.TX0-7.5, base.TX0-.3, base.TX1+.3, base.TX1+7.5):   # between flush grid lines
                    ground = build.terrain_height(*build.to_world(b, x, z), grid)-oy
                    self.assertAlmostEqual(ground, roof, delta=.05, msg=(b['team'], x, z))
        # The hut terrace sits just above the knoll, the platform well above it.
        self.assertLess(base.HUT_ROOF-base.HUT_RING, 2)
        self.assertAlmostEqual(base.trench_roof(base.TZ1), base.HUT_RING)
        self.assertAlmostEqual(base.trench_roof(base.TZ0), base.ROOF)

    def test_wing_blocks_and_ramp_foot_meet_the_ground(self):
        spec, _, _, grid, _ = self.whole_map()
        for b in spec['bases']:
            oy = b['position'][1]
            h = lambda x, z: build.terrain_height(*build.to_world(b, x, z), grid)-oy
            for s in (-1, 1):
                for x in np.arange(base.BX+.5, base.WING_X, 1.5):
                    for z in np.arange(base.BZ0+.5, base.WING_Z1, 1.5):
                        self.assertTrue(-.5 < h(s*x, z) <= base.ROOF+1e-6, (s*x, z))        # buried in the block
                    self.assertAlmostEqual(h(s*x, base.WING_Z1+.3), base.ROOF, delta=.06)  # flush behind it
                    self.assertAlmostEqual(h(s*x, base.BZ0-.5), -.1, delta=.25)          # apron in front
            xa, xb, zg, _ = base.XR
            for x in np.arange(xa, xb+.1, 1.0):
                for z in np.arange(base.HZ1+.5, zg+2, 1.0):
                    self.assertAlmostEqual(h(x, z), base.HUT_RING, delta=.05, msg=(x, z))

    # --- Whole map: sightlines, terrain, holes -----------------------------
    @classmethod
    def whole_map(cls):
        if not hasattr(cls, '_map'):
            spec = build.spec(); mesh = kit.Mesh()
            flags = [None, None]
            for b in spec['bases']:
                mesh.origin = tuple(b['position']); mesh.yaw = math.radians(b['yaw'])
                anchors = base.build(mesh, b['team'], f"base-{b['team']}")
                flags[b['team']] = mesh.point(anchors['flag'])
            turret_arcs.assign(mesh.entities, flags)
            mesh.origin = tuple(spec['ring']['position']); mesh.yaw = 0
            cairnhold_ring.build(mesh)
            grid, holes = build.terrain_grid(spec)
            cls._map = spec, mesh, Soup(mesh.collision), grid, holes
        return cls._map

    def test_turrets_cannot_see_into_rooms(self):
        """No turret or the plasma battery engages a player (field of fire,
        range, clear line from its barrel to the chest) anywhere inside a
        bunker room, the trench, the hut, the vault or the sally port, beyond
        the doorway depth (sightline_checks.DOOR_DEPTH) of the open front and
        exit-house doors. Allowed: the ramp directly under the stand's open
        roof hatch."""
        spec, mesh, soup, _, _ = self.whole_map()
        tx0, tx1, tz0, tz1 = base.TW
        vx0, _, vz0, vz1 = base.VAULT
        rooms = [(-15, 15, base.BZ0+base.WALL+.6, 19, lambda x, z: 0.0),
                 (base.TX0+1.6, base.TX1-1.6, base.TZ0+.6, base.TZ1-.6, lambda x, z: base.trench_floor(z)),
                 (base.HX0+1.4, base.HX1-1.4, base.HZ0+1.4, base.HZ1-1.4, lambda x, z: base.HUT_FLOOR),
                 (tx0+1.4, tx1-1.4, tz0+1.4, tz1-1.4, lambda x, z: base.HUT_ROOF),
                 (tx0+1.4, tx1-1.4, base.LANDING_Z[0]+.4, base.LANDING_Z[1]-.3, lambda x, z: base.TOWER_MID),
                 (vx0+.6, base.STAIR[0]-.6, vz0+.6, base.PIT_RAMP[2]-.2, lambda x, z: base.VAULT_FLOOR),  # vault
                 (base.PIT[0]+.6, base.PIT[1]-.6, base.PIT[2]+.6, base.PIT[3]-.6, lambda x, z: base.PIT_FLOOR),  # well
                 (base.STAIR[0]+.6, base.VAULT[1]-.6, base.STAIR[3]+.4, vz1-.6, lambda x, z: base.VAULT_FLOOR),  # walkway
                 (16.6, 86.6, 5.4, 10.6, lambda x, z: base.east_floor(x)),                     # sally port, east
                 (81.4, 86.6, -38.4, 4.0, lambda x, z: base.south_floor(z)),                   # sally port, south
                 (81.4, 86.6, -42.6, -36.2, lambda x, z: base.EXIT_GROUND)]                    # exit house
        r0, r1, zl, zh = base.RAMP; h0, h1, hz0, hz1 = base.HATCH
        targets, openings = [], []
        ox1 = base.T_EXIT[1]-.8
        for b in spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            pt = lambda x, z: tuple(m.point((x, 0, z))[i] for i in (0, 2))
            openings += [(pt(base.DOOR[0], base.BZ0+base.WALL), pt(base.DOOR[1], base.BZ0+base.WALL)),
                         (pt(ox1, base.EXIT_DOOR[0]), pt(ox1, base.EXIT_DOOR[1]))]
            for x0, x1, z0, z1, floor in rooms:
                for x in np.arange(x0, x1+.01, 1.0):
                    for z in np.arange(z0, z1+.01, 1.0):
                        y = floor(x, z)
                        if y != base.TOWER_MID and h0-.5 <= x <= h1 and hz0 <= z <= hz1+2: continue   # hatch columns
                        if soup.first(m.point((x, y+.1, z)), (0, 1, 0), 2.3) < 2.3: continue   # inside a prop
                        targets.append(m.point((x, y+LIFT+.8, z)))
        targets = np.array(targets)
        turrets = [e for e in mesh.entities if e['kind'] == 'turret']
        self.assertEqual(len(turrets), 6)
        allowed = sightline_checks.near_openings(targets, openings)
        for e in turrets:
            seen = sightline_checks.visible(soup, e, targets) & ~allowed
            self.assertEqual(int(seen.sum()), 0, (e['id'], targets[seen][:5]))
        self.assertGreater(len(targets), 900)

    def test_terrain_is_fair_rugged_and_matches_targets(self):
        """Each half its own ground (not a mirror), the two measuring alike
        (assets/fairness.py), rugged, with the authored profile."""
        spec, _, _, grid, holes = self.whole_map()
        flags = [None, None]
        for b in spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            flags[b['team']] = m.point(base.build(m, b['team'], 'c')['flag'])
        again, _ = build.terrain_grid(spec)
        self.assertTrue(np.array_equal(grid, again), 'terrain must be deterministic')
        self.assertFalse(fairness.symmetric(grid), 'each half its own ground')
        for row in fairness.assert_fair(self, grid, flags): print('half', {k: round(v, 2) for k, v in row.items()})
        # Slope over the play area's terrain cells (hole cells render no ground).
        slope = cairnhold_terrain.slope_degrees(grid)
        cut = np.zeros(65536, bool); cut[holes] = True; cut = cut.reshape(256, 256)
        # The flank Capture & Hold plateaus are level by design, like the cut
        # cells; leave them out of the ruggedness statistics.
        zc, xc = np.mgrid[0:256, 0:256]*8.0
        for c in spec.get('control_points', []):
            if not c.get('on_ring'): cut |= np.hypot(xc-c['x'], zc-c['z']) <= build.PLATEAU_R+8
        rows, cols = slice(int((1024-600)/8), int((1024+600)/8)+1), slice(int((1024-420)/8), int((1024+420)/8)+1)
        play = slope[rows, cols][~cut[rows, cols]]
        self.assertTrue(25 <= np.median(play) <= 30, np.median(play))
        self.assertLess((play < 3).mean(), .03)
        # Two valleys and a raised middle along the flag axis.
        prof = lambda z: build.terrain_height(1024, z, grid)
        self.assertGreater(prof(1024)-prof(1024-205), 20); self.assertGreater(prof(1024)-prof(1024+205), 20)
        # Each bunker sits on its knoll's forward slope: the ground falls away
        # in front of the apron into the valley, and rises behind to the stand.
        for b in spec['bases']:
            front = build.terrain_height(*build.to_world(b, 0, base.BZ0-3), grid)
            ahead = min(build.terrain_height(*build.to_world(b, 0, base.BZ0-d), grid) for d in (40, 50, 60))
            behind = build.terrain_height(*build.to_world(b, -6, base.HZ0+4), grid)
            self.assertGreater(front-ahead, 3, (b['team'], front, ahead))
            self.assertGreater(behind-front, 15, (b['team'], front, behind))
        flags = []
        for b in spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            flags.append(m.point((base.FLAG[0], 0, base.FLAG[1])))
        self.assertTrue(1040 <= math.dist(flags[0][::2], flags[1][::2]) <= 1080)

    def test_ridge_lines_glens_and_passes_make_the_lanes(self):
        """v2 lanes: each side a ridge line over the centre lane, broken by
        passes, and a glen beyond it, deepest through the middle; the flank
        cairns stand in the glens. Heights are the built ground."""
        spec, _, _, grid, _ = self.whole_map()
        h = lambda x, z: build.terrain_height(x, z, grid)
        T = cairnhold_terrain
        for side in (-1, 1):
            for Z in (-300.0, 0.0, 300.0):
                ridge = h(1024+float(T.ridge_centre(np.array(Z), side)), 1024+Z)
                glen = h(1024+float(T.glen_centre(np.array(Z), side)), 1024+Z)
                self.assertGreater(ridge-glen, 60, (side, Z, ridge, glen))
                self.assertGreater(ridge-h(1024, 1024+Z), 12, (side, Z, 'the ridge stands over the centre lane'))
            for p in T.PASSES:
                for Z in (-p, p):
                    crest = h(1024+float(T.ridge_centre(np.array(Z), side)), 1024+Z)
                    beside = max(h(1024+float(T.ridge_centre(np.array(Z+dz), side)), 1024+Z+dz) for dz in (-90, 90))
                    self.assertGreater(beside-crest, 15, (side, Z, 'pass'))
        cairns = [c for c in spec['control_points'] if not c.get('on_ring')]
        for c, side in zip(cairns, (-1, 1)):
            self.assertLess(abs(c['x']-1024-float(T.glen_centre(np.array(c['z']-1024.0), side))), 8, c['id'])

    def test_holes_are_exactly_the_dug_in_footprints_and_walls_cover_the_pins(self):
        spec, _, _, grid, holes = self.whole_map()
        hole_set = set(holes)
        expected = set()
        for b in spec['bases']:
            for x0, x1, z0, z1 in base.HOLES.values():
                for x in np.arange(x0+4, x1, 8):
                    for z in np.arange(z0+4, z1, 8):
                        wx, wz = build.to_world(b, x, z)
                        expected.add(int(wz//8)*256+int(wx//8))
        self.assertEqual(hole_set, expected)
        _, rings = build.holes_and_rings(spec)
        for (vx, vz), y in rings.items():
            self.assertEqual(grid[vz, vx], y)
            for b in spec['bases']:
                lx, lz = build.to_local(b, vx*8, vz*8)
                for name, (x0, x1, z0, z1) in base.HOLES.items():
                    if x0-1e-6 <= lx <= x1+1e-6 and z0-1e-6 <= lz <= z1+1e-6:
                        lo, hi = base.wall_span(name, lx, lz)
                        self.assertTrue(lo < y-b['position'][1] <= hi+1e-6, (name, lx, lz, y))

    def test_ground_stays_under_decks_ramps_and_the_ring(self):
        spec, _, _, grid, _ = self.whole_map()
        h = lambda x, z: build.terrain_height(x, z, grid)
        for b in spec['bases']:
            oy = b['position'][1]
            px, pz = base.PAD; H = base.PAD_HALF
            for x in np.arange(px-H, px+H+.1, 2):
                for z in np.arange(pz-H, pz+H+.1, 2):
                    wx, wz = build.to_world(b, x, z)
                    self.assertTrue(base.PAD_TOP-1.2 < h(wx, wz)-oy < base.PAD_TOP-.03, ('pad', x, z))
            bx, bz = base.BATTERY
            for r in np.arange(0, base.BATTERY_R*math.cos(math.pi/8)+10, 1.5):
                for dx in (-1.9, 0, 1.9):
                    wx, wz = build.to_world(b, bx+dx, bz+r)
                    self.assertLessEqual(h(wx, wz)-oy, base.BATTERY_GROUND+1e-6, ('battery', r))
            # The front apron is level with the bunker floor.
            for x in (-4, 0, 4):
                wx, wz = build.to_world(b, x, base.BZ0-3)
                self.assertAlmostEqual(h(wx, wz)-oy, -.1, delta=1/32)   # height.bin quantisation
        rx, ry, rz = spec['ring']['position']
        for r in np.arange(0, cairnhold_ring.PYLON_R+2.5, 1.0):
            for a in np.linspace(0, math.tau, 24, endpoint=False):
                self.assertAlmostEqual(h(rx+r*math.cos(a), rz+r*math.sin(a)), ry-.06, delta=1/32)

    def test_ring_is_symmetric_and_open_on_the_axes(self):
        mesh = kit.Mesh(); anchors = cairnhold_ring.build(mesh)
        soup = Soup(mesh.collision)
        pts = np.array(anchors['pylons'])
        self.assertTrue(np.allclose(np.sort(pts[:, [0, 2]], 0), np.sort(-pts[:, [0, 2]], 0)))
        for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            # Each ramp lane leads from outside the pylons up onto the dais.
            ts, _ = soup.hits((dx*30, cairnhold_ring.DAIS_H+1.2, dz*30), (-dx, 0, -dz), 60)
            self.assertTrue(len(ts) == 0 or ts[0] > 28, (dx, dz, ts[:2]))
        self.assertLess(len(mesh.collision)//9, 400)

    # --- Pack --------------------------------------------------------------
    def test_lightmap_bake_is_deterministic(self):
        from assets import lightmap_bake
        m = kit.Mesh(); m.lamps = []; cairnhold_ring.build(m)
        verts = np.array(m.vertices, np.float32).reshape(-1, 12)
        sun = (-.57735, .57735, -.57735); light = kit.MATERIALS.index('light')
        a, pa = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=1)
        b, pb = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=3)
        self.assertTrue(np.array_equal(a, b)); self.assertEqual(pa, pb)
        self.assertTrue(np.all((a[:, 8] >= 0) & (a[:, 8] <= 1) & (a[:, 9] >= 0) & (a[:, 9] <= 1)))

    def test_unbaked_rebuilds_are_byte_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            digests = []
            for name in ('a', 'b'):
                out = Path(tmp)/name
                build.build(out, bake=False)
                digests.append({f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in out.iterdir()})
            self.assertEqual(digests[0], digests[1])
            self.assertEqual(len(digests[0]), 10)   # map.json, six payloads, models.glb, shade.rg, props.bin

class AuthoredModels(unittest.TestCase):
    """The bases' hard-surface detail (scripts/models/cairnhold_models.py)
    in the committed pack: placed on both bases, copings flush with the
    solid tops, and every vertex near the boxes it was made from."""
    @classmethod
    def setUpClass(cls):
        import json
        from assets import cladding, map_models
        cls.pack = HERE.parent/'assets/maps/cairnhold'
        cls.man = json.loads((cls.pack/'map.json').read_text())
        cls.models, cls.cladding = map_models, cladding
        cls.mesh = cladding.recorder(kit.Mesh)(); cls.mesh.lamps = []
        base.build(cls.mesh, 0, 'red')
        cls.model = map_models.read(cls.pack/'models.glb', 'ch-base')

    def test_each_base_places_its_model(self):
        placed = [p for p in self.man['models'] if p['model'] == 'ch-base']
        self.assertEqual(sorted((p['team'], tuple(p['position']), p['yaw']) for p in placed),
                         sorted((b['team'], tuple(map(float, b['position'])), float(b['yaw'])) for b in build.spec()['bases']))
        self.assertGreater(len(self.model.tri), 1000)

    def test_copings_are_flush_with_the_wall_tops(self):
        checked = 0
        for top in self.cladding.tops(self.mesh.boxes):
            b = top.box
            if b.tag or b.mat != 'concrete' or min(b.x1-b.x0, b.z1-b.z0) > 1.7: continue
            pts = [((x0+x1)/2, (z0+z1)/2) for x0, x1, z0, z1 in top.rects]
            for (x, z), y in zip(pts, self.models.surface_heights(self.model, pts, b.y1+.3)):
                self.assertTrue(b.y1-1e-3 <= y <= b.y1+.02, (b, x, z, y))
                checked += 1
        self.assertGreaterEqual(checked, 40)

    def test_detail_hugs_the_solid(self):
        boxes = np.array([(b.x0, b.y0, b.z0, b.x1, b.y1, b.z1) for b in self.mesh.boxes if b.solid])
        pos = np.unique(self.model.pos.round(4), axis=0)
        d = np.maximum(np.maximum(boxes[None, :, :3]-pos[:, None], pos[:, None]-boxes[None, :, 3:]), 0)
        self.assertLess(float(np.sqrt((d**2).sum(-1)).min(1).max()), .35)


class PlayabilityV4(unittest.TestCase):
    """The playability-survey fixes: a
    two-level hall, 6 x 6 m doorways, a 6.5 m vault and tunnel with two
    skylight wells, and a fully open flag platform."""
    @classmethod
    def setUpClass(cls):
        cls.mesh, cls.anchors = one_base()
        cls.soup = Soup(cls.mesh.collision)

    def test_hall_is_two_levels_with_gallery_doorways(self):
        ix = base.BX-base.WALL
        gx = ix-base.GALLERY_W
        hz0, hz1 = base.HALL_Z
        # The tall part of the hall: a clear 12 m+ from the floor to its roof.
        for x in (-8.0, 0.0, 8.0):
            for z in (hz0+1.0, -6.0, base.PART_A[0]-base.GALLERY_W-1.0):
                if base.HALL_HOLE[0] <= x <= base.HALL_HOLE[1] and base.HALL_HOLE[2] <= z <= base.HALL_HOLE[3]: continue
                self.assertGreater(self.soup.first((x, .2, z), (0, 1, 0)), 12.0, (x, z, 'tall hall'))
        # The gallery: a floor at hillside level round the sides and back,
        # with 5.5 m+ over it.
        gy = base.GALLERY_Y
        for x, z in [(-ix+1.5, hz0+1.5), (-ix+1.5, -4.0), (ix-1.5, -10.0), (ix-1.5, 1.0), (0.0, base.PART_A[0]-1.5)]:
            t, ny = self.soup.hits((x, gy+3, z), (0, -1, 0))
            self.assertTrue(len(t) and abs(3-t[0]) < 1e-6 and ny[0] > .999, (x, z, 'gallery floor'))
            self.assertGreater(self.soup.first((x, gy+.2, z), (0, 1, 0)), 5.5, (x, z, 'gallery headroom'))
        # Each side wall's doorway opens from the hillside straight onto the
        # gallery and across the void: 6 m wide, 6 m tall.
        ga, gb = base.GALLERY_DOOR
        for s in (-1, 1):
            for z in np.arange(ga+.3, gb-.29, .5):
                for dy in (.4, 3.0, 5.6):
                    t = self.soup.first((s*40, gy+dy, z), (-s, 0, 0))
                    self.assertGreater(t, 40-gx, (s, z, dy, 'gallery doorway'))
            self.assertGreaterEqual(gb-ga, 6.0); self.assertGreaterEqual(base.HALL_CEIL-gy, 6.0)
            # The ground outside the doorway is at gallery level.
            self.assertAlmostEqual(base.bunker_ground((ga+gb)/2), gy, delta=.01)

    def test_front_and_interior_doorways_are_six_by_six(self):
        self.assertGreaterEqual(base.DOOR[1]-base.DOOR[0], 6.0)
        self.assertGreaterEqual(base.DOOR_TOP, 6.0)
        for d0, d1 in base.PART_A_DOORS+[base.PART_B_DOOR, base.BACK_DOOR]:
            self.assertGreaterEqual(d1-d0, 6.0)
        # A skier at head height passes the front door and the vestibule.
        for x in (-2.7, 0, 2.7):
            for y in (.3, 3.0, 5.6):
                self.assertGreater(self.soup.first((x, y, -30), (0, 0, 1)), 30+base.BZ0+base.WALL+8, (x, y))

    def test_vault_and_tunnel_have_six_and_a_half_metres(self):
        F = base.VAULT_FLOOR
        self.assertGreaterEqual(base.VAULT_CEIL-F, 6.5)
        self.assertGreaterEqual(base.TUN_H, 6.5)
        for x in (4.2, 9.8):
            for z in (-8, -2):
                self.assertGreater(self.soup.first((x, F+.2, z), (0, 1, 0)), 6.2, (x, z))
        for x in (20.0, 30.0, 44.0, 58.0, 76.0):
            if any(s0 <= x <= s1 for s0, s1 in base.SKYLIGHTS): continue
            y = base.tunnel_floor(x, 8.0)
            self.assertGreater(self.soup.first((x, y+.2, 8.0), (0, 1, 0)), 6.2, x)

    def test_skylights_drop_into_the_tunnel_and_can_be_jetted_out(self):
        ez0, ez1 = base.T_EAST[2:]
        for s0, s1 in base.SKYLIGHTS:
            x = (s0+s1)/2
            for z in (ez0+base.WALL+.6, (ez0+ez1)/2, ez1-base.WALL-.6):
                t = self.soup.first((x, 60, z), (0, -1, 0))
                self.assertAlmostEqual(60-t, base.tunnel_floor(x, z), delta=.02, msg=(x, z))
            # Shallow enough to jet straight out of (well under a full tank).
            self.assertLess(base.east_lid(s1)-base.tunnel_floor(x, 8.0), 16.0)
            # The wells open into the tunnel, not the vault.
            self.assertGreater(s0, base.VAULT[1]+10)

    def test_flag_platform_is_fully_open(self):
        fx, fz = base.FLAG
        top = base.TOWER_TOP
        tx0, tx1, tz0, tz1 = base.TW
        # No parapet: nothing stands on the platform's edges.
        for x in np.arange(tx0+.2, tx1, .8):
            for z in (tz0+.2, tz1-.2):
                self.assertGreater(self.soup.first((x, top+.05, z), (0, 1, 0)), 1.5, (x, z))
        for z in np.arange(tz0+.2, tz1, .8):
            for x in (tx0+.2, tx1-.2):
                self.assertGreater(self.soup.first((x, top+.05, z), (0, 1, 0)), 1.5, (x, z))
        # Straight fly-in lines to the flag from every direction but the two
        # that cross the sentry mast and the sensor at the platform's back.
        blockers = [base.MAST, base.SENSOR]
        clear = 0
        for k in range(16):
            a = k*math.tau/16
            d = np.array([-math.cos(a), 0, -math.sin(a)])
            o = np.array([fx+30*math.cos(a), top+1.2, fz+30*math.sin(a)])
            crosses = any(np.linalg.norm(np.cross(np.array([bx-o[0], 0, bz-o[2]]), d)) < 2.6
                          and 0 < np.dot(np.array([bx-o[0], 0, bz-o[2]]), d) < 30 for bx, bz in blockers)
            if crosses: continue
            self.assertGreater(self.soup.first(o, d), 28.5, (k, 'flag line blocked'))
            clear += 1
        self.assertGreaterEqual(clear, 12)

    def test_no_z_fighting_on_the_base_the_ring_or_the_towers(self):
        from assets import surface_checks, cnh_tower
        self.assertEqual(surface_checks.z_fighting(self.mesh.vertices, self.mesh.collision), [])
        m = kit.Mesh(); m.origin = (0, 0, 0); m.yaw = 0.0
        cairnhold_ring.build(m)
        m.origin = (0, cairnhold_ring.DAIS_H, 0)
        cnh_tower.build(m)
        self.assertEqual(surface_checks.z_fighting(m.vertices, m.collision), [])


class CommittedPack(unittest.TestCase):
    """Checks on the embedded pack: capture points, flag routes, hovering
    edges and overlapping surfaces over the whole map."""
    PACK = Path(__file__).resolve().parent.parent/'assets/maps/cairnhold'

    @classmethod
    def setUpClass(cls):
        import json
        cls.manifest = json.loads((cls.PACK/'map.json').read_text())
        cls.spec = build.spec()
        cls.heights = np.frombuffer((cls.PACK/'height.bin').read_bytes(), np.uint16).reshape(256, 256)/32.0

    def test_three_capture_points_ring_centre_and_mirrored_flanks(self):
        from assets import cnh_tower, centre_turret
        points = self.manifest['control_points']
        self.assertEqual([p['name'] for p in points], ['The Ring', 'West Cairn', 'East Cairn'])
        for p in points:
            self.assertEqual(p['radius'], cnh_tower.RING)
        # The Ring is the CTF centre, its rocket turret on the tower's crown.
        self.assertEqual([p['ctf_active'] for p in points], [True, False, False])
        ring = points[0]['pos']
        self.assertEqual(points[0]['turret'], {'deployable': 'rocket-turret', 'at': [ring[0], round(ring[1]+centre_turret.TURRET_Y, 3), ring[2]],
                                               'range': centre_turret.half_to_base(ring, self.manifest['entities'])}, 'reaching half way to the bases')
        self.assertTrue(all('turret' not in p for p in points[1:]))
        rx, ry, rz = self.spec['ring']['position']
        self.assertEqual(points[0]['pos'], [rx, ry+cairnhold_ring.DAIS_H, rz], 'the Ring point stands on the dais')
        self.assertGreaterEqual(cairnhold_ring.DAIS_R, cnh_tower.RING, 'the capture ring is the dais')
        (ax, _, az), (bx, _, bz) = points[1]['pos'], points[2]['pos']
        self.assertEqual((ax+bx, az+bz), (2*rx, 2*rz), 'flank points mirror through the centre')
        self.assertAlmostEqual(points[1]['pos'][1], points[2]['pos'][1], delta=.05)
        from assets import pack_writer
        for p in points[1:]:
            x, y, z = p['pos']
            # A level plateau across the capture ring (walkable, under 3 m of
            # relief), and the tower's buried solids covering the ground
            # under its plinth and cover walls.
            ring = [pack_writer.sample_height(self.heights, x+dx, z+dz)
                    for dx in np.arange(-cnh_tower.RING, cnh_tower.RING+.1, 2.0)
                    for dz in np.arange(-cnh_tower.RING, cnh_tower.RING+.1, 2.0) if math.hypot(dx, dz) <= cnh_tower.RING]
            self.assertLess(max(ring)-min(ring), 3.0, p['id'])
            outline = [(math.cos(a)*(cnh_tower.PLINTH_R+.4), math.sin(a)*(cnh_tower.PLINTH_R+.4))
                       for a in np.linspace(0, math.tau, 16, endpoint=False)]
            for k in range(4):
                a = math.pi/4+k*math.pi/2
                for off in (-cnh_tower.COVER_LEN/2, 0, cnh_tower.COVER_LEN/2):
                    outline.append((math.cos(a)*cnh_tower.COVER_R-math.sin(a)*off, math.sin(a)*cnh_tower.COVER_R+math.cos(a)*off))
            for dx, dz in outline:
                h = pack_writer.sample_height(self.heights, x+dx, z+dz)
                self.assertGreater(h, y-cnh_tower.SINK+.2, (p['id'], dx, dz, 'base shows above ground'))
                self.assertLess(h, y+cnh_tower.PLINTH_H-.05, (p['id'], dx, dz, 'buried'))
        # And none of the flank towers sits near a base structure.
        for p in points[1:]:
            for bat in (a for i in self.manifest['instances'] if 'battery' in i.get('anchors', {}) for a in [i['anchors']['battery']]):
                self.assertGreater(math.hypot(p['pos'][0]-bat[0], p['pos'][2]-bat[2]), 80)

    def test_many_distinct_routes_to_each_flag(self):
        """User rule: "two main entrances but ~10 ways of getting to the flag".
        The flag platform is fully open, so every way into the hut and tower
        (walk, drop or jet) times every way from it onto the platform round
        the flag counts (route_checks.flag_routes, airborne)."""
        from assets import route_checks
        pack = route_checks.Pack(self.PACK)
        fx, fz = base.FLAG
        for b in self.spec['bases']:
            ox, oy, oz = b['position']; s = -1 if b['yaw'] == 180 else 1
            def world(bx):
                x0, x1, y0, y1, z0, z1 = bx
                xs = sorted((ox+s*x0, ox+s*x1)); zs = sorted((oz+s*z0, oz+s*z1))
                return (xs[0], xs[1], oy+y0, oy+y1, zs[0], zs[1])
            r = route_checks.flag_routes(pack, (ox+s*fx, oz+s*fz),
                                         world((base.HX0, base.HX1, base.HUT_FLOOR-.5, base.TOWER_TOP+.5, base.HZ0, base.HZ1)),
                                         world((fx-5, fx+5, base.TOWER_TOP-.5, base.TOWER_TOP+.8, fz-5, fz+5)), airborne=True)
            self.assertGreaterEqual(len(r['entries']), 8, (b['team'], r['entries']))
            self.assertGreaterEqual(r['routes'], 10, (b['team'], [len(a) for a in r['approaches']]))

    def test_no_hovering_wall_bottoms(self):
        """No wall's lowest edge floats 0.3-2.5 m over the terrain without a
        floor under it (the visual audit's check, with the engine's bilinear
        terrain height)."""
        from assets import pack_writer
        v = np.frombuffer((self.PACK/'vertices.bin').read_bytes(), np.float32).reshape(-1, 3, 12).astype(float)
        holes = set(self.manifest['holes'])
        p = v[:, :, :3]; nz = np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]); ln = np.maximum(np.linalg.norm(nz, axis=1), 1e-9)
        ny = np.abs(nz[:, 1])/ln
        walls = np.nonzero((ny < .05) & (ln/2 > .05))[0]; floors = np.nonzero(ny > .9)[0]
        grid = {}
        for f in floors:
            lo = p[f].min(0); hi = p[f].max(0)
            for gx in range(int(lo[0]//4), int(hi[0]//4)+1):
                for gz in range(int(lo[2]//4), int(hi[2]//4)+1): grid.setdefault((gx, gz), []).append(f)
        found = []
        for w in walls:
            b = p[w][p[w][:, 1].argmin()]
            x, z = int(b[0]//8), int(b[2]//8)
            if not (0 <= x < 256 and 0 <= z < 256) or z*256+x in holes: continue
            ground = pack_writer.sample_height(self.heights, b[0], b[2]); gap = b[1]-ground
            if not (.3 < gap < 2.5): continue
            supported = any(p[f].min(0)[0]-.3 <= b[0] <= p[f].max(0)[0]+.3 and p[f].min(0)[2]-.3 <= b[2] <= p[f].max(0)[2]+.3
                            and ground-.1 <= p[f][:, 1].max() <= b[1]+.05
                            for f in grid.get((int(b[0]//4), int(b[2]//4)), []))
            if not supported: found.append([round(float(c), 1) for c in b])
        self.assertEqual(found, [])

    def test_no_z_fighting_anywhere_on_the_map(self):
        from assets import surface_checks
        v = np.frombuffer((self.PACK/'vertices.bin').read_bytes(), np.float32)
        c = np.frombuffer((self.PACK/'collision.bin').read_bytes(), np.float32)
        self.assertEqual(surface_checks.z_fighting(v, c), [])

    def test_own_look_and_fog(self):
        import json
        look = self.manifest['look']
        self.assertEqual(look, self.spec['look'])
        self.assertNotIn('sun_direction', look, 'keeps the baked sun')
        self.assertEqual(self.manifest['sky']['fogColor'], self.spec['fog_color'])
        for other in ('old-holler', 'tower-complex', 'frostline', 'dustreach'):
            f = self.PACK.parent/other/'map.json'
            self.assertNotEqual(json.loads(f.read_text()).get('look', {}).get('sky'), look['sky'], other)


class SpawnForwardClearance(unittest.TestCase):
    """Every committed spawn faces open floor: a clear body-width view for
    6 m and the same floor for a half-second walk (docs/map-pipeline.md)."""
    def test_committed_spawns_face_open_floor(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/cairnhold'
        self.assertEqual(spawn_checks.problems(pack), [])

    def test_no_indoor_spawn_shows_through_an_opening_from_the_field(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/cairnhold'
        self.assertEqual(spawn_checks.exposed(pack), [])


class PropsAndShade(unittest.TestCase):
    """Props and the terrain shade map on the committed pack (assets/prop_checks.py)."""
    def test_props_and_terrain_shade(self):
        from assets import prop_checks
        prop_checks.check_pack(self, Path(__file__).resolve().parent.parent/'assets/maps/cairnhold')

class Water(unittest.TestCase):
    """Mirrored ponds carved into the terrain (assets/water_bodies.py, water_checks.py)."""
    PACK = Path(__file__).resolve().parent.parent/'assets/maps/cairnhold'

    def test_ponds_are_carved_clear_and_mirrored(self):
        from assets import water_checks
        rows = water_checks.assert_ponds(self, self.PACK, 2, 2.2, 3.4)
        for r in rows: print('water', r)

    def test_legacy_plane_is_off(self):
        import json
        m = json.loads((self.PACK/'map.json').read_text())
        self.assertFalse(m['water_enabled'])
        for v in m['water_volumes']: self.assertLessEqual(len(v['polygon']), 64)

    def test_no_solid_prop_stands_in_water(self):
        import json
        from assets import water_bodies
        m = json.loads((self.PACK/'map.json').read_text())
        solid = [(b[0], b[1], b[2]) for b in m['props']['big']]+[(c[0], c[1], c[2]) for c in m['props']['cover']]
        for x, y, z in solid:
            for v in m['water_volumes']:
                self.assertFalse(water_bodies.contains(v, x, z) and y < v['surface']+1.0, (x, y, z, v['surface']))

if __name__ == '__main__':
    unittest.main()
