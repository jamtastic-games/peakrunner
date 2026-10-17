#!/usr/bin/env python3
"""Test the Dustreach builders.
Run from src/ with scripts/requirements.txt installed: scripts/test-dustreach.py"""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from assets import fairness
from assets import sightline_checks
from assets import turret_arcs
from assets import budgets
from assets import dustreach_citadel as base
from assets import dustreach_gate as gate
from assets import dustreach_materials
from assets import dustreach_sewer as sewer
from assets import dustreach_terrain

HERE = Path(__file__).parent
ROOT = HERE.parent
def _load(name, file):
    loader = importlib.util.spec_from_file_location(name, HERE/file)
    module = importlib.util.module_from_spec(loader); loader.loader.exec_module(module)
    return module
kit = _load('kit', 'build-original-map.py')
build = _load('dustreach_build', 'build-dustreach.py')

LIFT = base.SPAWN_LIFT
TER = base.TER


class Soup:
    """Vectorised ray queries against a collision triangle soup."""
    def __init__(self, collision):
        t = np.array(collision, np.float64).reshape(-1, 3, 3)
        self.a = t[:, 0]; self.e1 = t[:, 1]-t[:, 0]; self.e2 = t[:, 2]-t[:, 0]
        n = np.cross(self.e1, self.e2); self.n = n/(np.linalg.norm(n, axis=1, keepdims=True)+1e-12)
        self.lo = t.min(1); self.hi = t.max(1); self.tris = t

    def hits(self, o, d, tmax=np.inf):
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


class DustreachTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mesh, cls.anchors = one_base()
        cls.soup = Soup(cls.mesh.collision)

    # --- Floors, headroom and standing support ----------------------------
    def test_floors_have_floor_and_headroom(self):
        samples = [(x, z, TER) for x, z in ((-6, -11), (5, -11), (-5, -2), (5, -2), (12, -11), (12, 2.6))]   # hall (the stair opening is at x 9.3-13.2, z -5 to 2)
        samples += [(x, z, TER) for x in (-11, 11) for z in (6, 12, 24)]+[(x, z, TER) for x in (-3, 3) for z in (6, 27.5)]  # courtyard
        samples += [(s*18.2, z, TER) for s in (-1, 1) for z in (6, 15.5, 25)]           # arcades
        samples += [(x, z, base.PIT_FLOOR) for x in (-5, 0, 5) for z in (14.8, 19.2)]   # flag court
        samples += [(x, -17.4, TER) for x in (-10, 0, 10)]                              # front ledge
        for x, z, y in samples:
            t, ny = self.soup.hits((x, y+3, z), (0, -1, 0))
            self.assertTrue(len(t) and abs(3-t[0]) < 1e-6 and ny[0] > .999, (x, z, 'floor'))
            self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 2.4, (x, z, 'headroom'))

    def test_ramps_climb_with_open_sky_and_closed_undersides(self):
        for ramp, fn in [(base.FRONT_RAMP, base.front_ramp_y), (base.BACK_RAMP, base.back_ramp_y)]:
            x0, x1, zg, zt = ramp
            lo, hi = sorted((zg, zt))
            for z in np.arange(lo+.4, hi, 1.0):
                for x in (x0+.8, (x0+x1)/2, x1-.8):
                    y = fn(z)
                    t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
                    self.assertLess(abs(1-t[0]), .02, (ramp, x, z))
                    self.assertGreater(abs(ny[0]), .85, (ramp, z))
                    self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 3.0, (ramp, z, 'headroom'))
            self.assertLess(math.degrees(math.atan(TER/(hi-lo))), 20)
            # Nothing can be walked into underneath either edge.
            for z in np.arange(lo+1, hi-1, 1.5):
                y = fn(z)
                if y-.9 < .3: continue
                for edge, sx in ((x0, -1), (x1, 1)):
                    self.assertLess(self.soup.first((edge+sx, .3, z), (-sx, 0, 0)), 1.01, (ramp, z, sx))
        # The flag court ramps.
        for z in np.arange(base.PIT_Z[0]+.3, base.PIT_Z[1], 1.0):
            for x in (-5, 0, 5):
                if abs(x-base.FLAG[0]) < 2.2 and abs(z-base.FLAG[1]) < 2.2: continue   # flag plinth
                y = base.pit_y(z)
                t, _ = self.soup.hits((x, y+1, z), (0, -1, 0))
                self.assertLess(abs(1-t[0]), .02, (x, z, 'pit'))
                self.assertEqual(self.soup.first((x, y+.2, z), (0, 1, 0)), np.inf, (x, z, 'open sky'))

    def test_watch_ruin_ramp_reaches_the_deck(self):
        r0, r1, zg, zt = base.ruin_ramp()
        slope = math.degrees(math.atan(base.RUIN_TOP/(zg-zt)))
        self.assertLess(slope, 30)
        for z in np.arange(zt+.3, zg-.3, 1.0):
            y = base.ramp_y((r0, r1, zg, zt), z, 0, base.RUIN_TOP)
            t, _ = self.soup.hits(((r0+r1)/2, y+1, z), (0, -1, 0))
            self.assertLess(abs(1-t[0]), .02, z)
        cx, cz = base.RUIN
        t, ny = self.soup.hits((cx, base.RUIN_TOP+5.5, cz+3.5), (0, -1, 0))
        self.assertAlmostEqual(base.RUIN_TOP+5.5-t[0], base.RUIN_TOP, places=4)

    def test_standing_support_is_the_floor_top_everywhere(self):
        """The engine's support ray starts 0.15 m above the feet. Wherever a
        player can stand, its first hit must be the up-facing floor top."""
        regions = [(-13, 13, -15, 3, TER), (-20.5, 20.5, 4.5, 28.4, TER), (-20.5, 20.5, -17.8, -16.2, TER),
                   (-6.8, 6.8, 14.2, 19.8, base.PIT_FLOOR), (-13.8, 13.8, -15.8, 3.8, base.ROOF)]
        for s in (-1, 1):
            a, c = sorted((s*15.9, s*19.9)); regions.append((a, c, 4.1, 28.7, base.ARCADE_ROOF))
            a, c = sorted((s*14.9, s*19.1)); regions.append((a, c, -18.3, -12.7, base.PYLON_TOP))
        wx0, wx1, wz0, wz1 = base.TOWER
        regions.append((wx0+.6, wx1-.6, wz0+.6, wz1-.6, base.TOWER_TOP))
        px, pz = base.PAD; H = base.PAD_HALF-.6
        regions.append((px-H, px+H, pz-H, pz+H, base.PAD_TOP))
        cx, cz = base.RUIN; r = base.RUIN_R*.8
        regions.append((cx-r, cx+r, cz-r, cz+r, base.RUIN_TOP))
        checked = 0
        for x0, x1, z0, z1, level in regions:
            for x in np.arange(x0, x1+.01, .8):
                for z in np.arange(z0, z1+.01, .8):
                    t, ny = self.soup.hits((x, level+2.6, z), (0, -1, 0))
                    if not len(t) or abs(2.6-t[0]) > .02 or np.any((np.abs(t-t[0]) < .01) & (ny < .99)): continue
                    first, fy = self.soup.hits((x, level+.15, z), (0, -1, 0))
                    checked += 1
                    self.assertTrue(len(first) and first[0] < .17 and fy[0] > .99, (x, z, level, first[:2]))
        self.assertGreater(checked, 3000)

    def test_no_accidental_slopes_on_walkable_surfaces(self):
        n, cen = self.soup.n, self.soup.tris.mean(1)
        ents = [e['position'] for e in self.mesh.entities]
        cx, cz = base.RUIN
        ramps = [base.FRONT_RAMP, base.BACK_RAMP, base.ruin_ramp(), (-base.PIT_X, base.PIT_X, *base.PIT_Z),
                 base.STAIR, base.TOWER_RAMP, base.STORE_RAMP]
        for i in np.where((n[:, 1] > .3) & (n[:, 1] < .9999))[0]:
            x, y, z = cen[i]
            if any(r[0]-.01 <= x <= r[1]+.01 and min(r[2:])-.01 <= z <= max(r[2:])+.01 for r in ramps): continue
            if math.hypot(x-cx, z-cz) < base.RUIN_R+1: continue                        # ruin chamfer
            if math.hypot(x-base.FLAG[0], z-base.FLAG[1]) < 2.1: continue              # flag plinth
            if any(math.hypot(x-e[0], z-e[2]) < 3.3 for e in ents): continue          # kit equipment
            if any(math.hypot(x-dx, z-dz) < r+.5 for dx, dz, r in base.DRUMS): continue
            if abs(x) > base.PIT_X+.8 and abs(x) < base.PIT_X+2.2 and y > TER: continue   # braziers
            if any(math.hypot(x-s*base.ARCADE_X, z-az) < 1 for s in (-1, 1) for az in base.ARCADE_Z): continue
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

    def test_flag_sits_in_the_sunken_court_open_to_the_sky(self):
        fx, fy, fz = self.anchors['flag']
        top = base.PIT_FLOOR+base.PLINTH
        self.assertEqual(TER-base.PIT_FLOOR, 3.0, 'the court is sunk 3 m below the terrace')
        t, _ = self.soup.hits((fx, fy+1, fz), (0, -1, 0))
        self.assertAlmostEqual(fy+1-t[0], top, places=4)
        self.assertAlmostEqual(fy, top+.05, places=6)
        self.assertEqual(self.soup.first((fx, fy+.1, fz), (0, 1, 0)), np.inf, 'flag must be open to the sky')
        # The court's side walls stand between the flag and the arcades.
        for s in (-1, 1):
            self.assertLess(self.soup.first((0, base.PIT_FLOOR+1, fz), (s, 0, 0)), base.PIT_X+.01)

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
        self.assertEqual(kinds, ['generator', 'inventory', 'inventory', 'inventory', 'sensor', 'turret', 'turret', 'turret'])
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

    def test_materials_and_sky_are_deterministic_opaque_and_distinct(self):
        tex = {}
        for name in dustreach_materials.MATERIALS:
            a = dustreach_materials.texture(name, 7, kit.noise, kit.value_noise)
            self.assertEqual(a, dustreach_materials.texture(name, 7, kit.noise, kit.value_noise), name)
            self.assertEqual(len(a), 256*256*4)
            self.assertTrue(all(a[i] == 255 for i in range(3, len(a), 4)), name)
            tex[name] = a
        names = list(tex)
        for i, a in enumerate(names):
            for b in names[i+1:]: self.assertNotEqual(tex[a], tex[b], (a, b))
        from assets import tower_complex_materials, cairnhold_materials
        for name in ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark']:
            self.assertNotEqual(tex[name], tower_complex_materials.texture(name, 7, kit.noise, kit.value_noise), name)
            self.assertNotEqual(tex[name], cairnhold_materials.texture(name, 7, kit.noise, kit.value_noise), name)
        face = dustreach_materials.sky(4, kit.unit, kit.smooth, kit.cloud_noise)
        self.assertEqual(face, dustreach_materials.sky(4, kit.unit, kit.smooth, kit.cloud_noise))
        self.assertEqual(len(face), 256*256*4)
        self.assertNotEqual(face, kit.sky(4))

    # --- Underground level and storehouse (v2) -------------------------------
    def test_underground_and_storehouse_floors_have_floor_and_headroom(self):
        C, S = base.CIS_FLOOR, base.STORE_UP
        tw = (base.TUNNEL_W[2]+base.TUNNEL_W[3])/2
        # (x, z, floor, minimum headroom)
        samples = [(x, z, C, 5.5) for x, z in ((-10, -14), (-10, 4), (4, -14), (-4, 3), (3, 4), (-11, -3))]
        samples += [(-12.0, z, C, 4.2) for z in np.arange(7.5, 21.6, 2.0)]                 # tunnel north
        samples += [(x, tw, C, 4.2) for x in np.arange(-23.5, -15.9, 1.5)]                # tunnel west
        samples += [(-29.0, 18.0, C, 6.0), (-25.5, 20.0, C, 6.0), (-29.5, 28.5, base.LANDING, 4.2)]   # tower room
        samples += [(x, z, 0.0, 3.5) for x, z in ((33, 4), (46, 15), (38, 20), (33, 20), (40, 8))]   # store, ground
        samples += [(x, z, S, 4.5) for x, z in ((30, 4), (32.5, 10), (32, 20), (36, 18))]           # mezzanine, landing
        for x, z, y, room in samples:
            t, ny = self.soup.hits((x, y+3, z), (0, -1, 0))
            self.assertTrue(len(t) and abs(3-t[0]) < 1e-6 and ny[0] > .999, (x, z, y, 'floor', t[:1]))
            self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), room, (x, z, y, 'headroom'))
        # The tunnel keeps its 4.5 m headroom under a lid, not open to the terrace.
        self.assertAlmostEqual(self.soup.first((-12.0, C+.2, 12.0), (0, 1, 0)), base.TUN_CEIL-C-.2, places=4)

    def test_no_spawn_camps_the_generator_stair(self):
        """No spawn in the keep hall or cistern, and every spawn at least 15 m
        (straight line, so walking is further) from the stair head."""
        hx0, hx1, _, hz1 = base.HALL_HOLE
        head = ((hx0+hx1)/2, hz1)
        ix, iz0, iz1 = base.KX, base.KZ0, base.KZ1
        for x, y, z, _ in self.anchors['spawn_points']:
            self.assertFalse(-ix < x < ix and iz0 < z < iz1 and y > base.TER, (x, z, 'in the keep hall'))
            self.assertGreaterEqual(math.hypot(x-head[0], z-head[1]), 15.0, (x, z))

    def test_tower_back_door_is_open_straight_in(self):
        """The watch tower's back door has nothing outside or behind it: a
        body band walks and shoots straight in from the ground behind the
        tower, over the landing and down the exit ramp's line."""
        wx0, wx1, wz0, wz1 = base.TOWER
        xc = (base.TOWER_DOOR[0]+base.TOWER_DOOR[1])/2
        for off in (-1.0, 0.0, 1.0):
            for h in (.6, 1.6, 2.6):
                a = (xc+off, base.LANDING+h, wz1+8.0)
                c = (xc+off, base.LANDING+h, wz1-1.5)
                d = np.subtract(c, a); n = np.linalg.norm(d)
                self.assertEqual(self.soup.first(a, d/n, n), np.inf, (a, c))

    def test_new_ramps_climb_with_headroom_and_closed_undersides(self):
        cases = [(base.STAIR, base.stair_y, base.CIS_FLOOR, 1),        # open (railed) side at x0, probe from -x
                 (base.TOWER_RAMP, base.tower_ramp_y, base.CIS_FLOOR, -1),
                 (base.STORE_RAMP, base.store_ramp_y, 0.0, 1)]
        for ramp, fn, bottom, side in cases:
            x0, x1, za, zb = ramp
            lo, hi = sorted((za, zb))
            rise = abs(fn(zb)-fn(za))
            self.assertLess(math.degrees(math.atan(rise/(hi-lo))), 30.0, ramp)
            for z in np.arange(lo+.3, hi-.2, .8):
                for x in (x0+.6, (x0+x1)/2, x1-.6):
                    y = fn(z)
                    t, ny = self.soup.hits((x, y+1, z), (0, -1, 0))
                    self.assertLess(abs(1-t[0]), .02, (ramp, x, z))
                    self.assertGreater(abs(ny[0]), .85, (ramp, z))
                    self.assertGreater(self.soup.first((x, y+.2, z), (0, 1, 0)), 2.4, (ramp, x, z, 'headroom'))
            # Nothing can be walked into underneath the open edge.
            edge = x0 if side > 0 else x1
            for z in np.arange(lo+.5, hi-.5, .8):
                if fn(z)-.9 < bottom+.3: continue
                self.assertLess(self.soup.first((edge-side, bottom+.3, z), (side, 0, 0)), 1.01, (ramp, z))
        # The stair's head meets the hall floor flush, and the hall floor
        # resumes over the stair's low end (the opening is railed round).
        hx0, hx1, hz0, hz1 = base.HALL_HOLE
        t, ny = self.soup.hits(((hx0+hx1)/2, TER+1, hz1+.3), (0, -1, 0))
        self.assertAlmostEqual(t[0], 1.0, places=4); self.assertGreater(ny[0], .999)
        t, _ = self.soup.hits(((hx0+hx1)/2, TER+1, hz1-.3), (0, -1, 0))
        self.assertAlmostEqual(TER+1-t[0], base.stair_y(hz1-.3), delta=.02)
        for x in (hx0-.2, (hx0+hx1)/2):
            self.assertLess(self.soup.first((x, TER+.5, hz0-1.0), (0, 0, 1)), 1.01, 'south rail')

    def test_generator_room_has_exactly_two_ways_in(self):
        """The cistern's walls have one opening (the tunnel door); the only
        other way in is the stair from the hall. No view out through its
        ceiling, and the generator sits on its floor."""
        C = base.CIS_FLOOR
        cx0, cx1, cz0, cz1 = base.CIS
        gx, gy, gz = self.anchors['generator']
        self.assertTrue(cx0 < gx < cx1 and cz0 < gz < cz1 and gy == C)
        runs = {}
        step = .25
        sides = {'south': [((x, cz0+.3), (0, 0, -1)) for x in np.arange(cx0+.3, cx1-.29, step)],
                 'north': [((x, cz1-.3), (0, 0, 1)) for x in np.arange(cx0+.3, cx1-.29, step)],
                 'west': [((cx0+.3, z), (-1, 0, 0)) for z in np.arange(cz0+.3, cz1-.29, step)],
                 'east': [((cx1-.3, z), (1, 0, 0)) for z in np.arange(cz0+.3, cz1-.29, step)
                          if not base.STAIR[2]-.4 <= z <= base.STAIR[3]+.4]}
        for name, probes in sides.items():
            open_ = [all(self.soup.first((x, C+h, z), d, 2.0) == np.inf for h in (.6, 1.6)) for (x, z), d in probes]
            n, width, widths = 0, 0, []
            for o in open_ + [False]:
                if o: width += step
                elif width: widths.append(width); width = 0
            runs[name] = widths
        self.assertEqual(runs['south'], []); self.assertEqual(runs['west'], []); self.assertEqual(runs['east'], [])
        self.assertEqual(len(runs['north']), 1, runs['north'])
        self.assertAlmostEqual(runs['north'][0], base.TUN_DOOR[1]-base.TUN_DOOR[0]-.3, delta=.6)
        for x in np.arange(cx0+.5, cx1, 1.0):
            for z in np.arange(cz0+.5, cz1, 1.0):
                self.assertLess(self.soup.first((x, C+.3, z), (0, 1, 0)), base.CIS_CEIL-C, (x, z, 'open ceiling'))

    def test_holes_are_covered_on_the_grid_and_cut_edges_stay_flat(self):
        spec, _, soup, grid = self.whole_map()
        cells = build.holes(spec)
        per_base = sum((x1-x0)*(z1-z0)/64 for x0, x1, z0, z1 in base.HOLES.values())
        self.assertEqual(len(cells), 2*per_base+len(sewer.all_cells()), 'hole rects overlap or leave the grid')
        for b in spec['bases']:
            oy = b['position'][1]
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            for name, (x0, x1, z0, z1) in base.HOLES.items():
                for x in np.arange(x0+1.5, x1, 3.0):
                    for z in np.arange(z0+1.5, z1, 3.0):
                        p = m.point((x, 2.3, z))
                        t, _ = soup.hits(p, (0, -1, 0))
                        floor = 2.3-t[0] if len(t) else -1e9
                        self.assertTrue(base.CIS_FLOOR-.01 <= floor < 2.3, (name, x, z, floor))   # a floor, stair or landing
                        self.assertLess(soup.first(p, (0, 1, 0)), 30, (name, x, z, 'hole open to the sky'))
                # Grid vertices on the cut's edge keep the flat site height: no seam.
                for x in np.arange(x0, x1+.1, 8.0):
                    for z in (z0, z1):
                        wx, wz = build.to_world(b, x, z)
                        self.assertAlmostEqual(build.terrain_height(wx, wz, grid)-oy, -.1, delta=1/32, msg=(name, x, z))
                for z in np.arange(z0, z1+.1, 8.0):
                    for x in (x0, x1):
                        wx, wz = build.to_world(b, x, z)
                        self.assertAlmostEqual(build.terrain_height(wx, wz, grid)-oy, -.1, delta=1/32, msg=(name, x, z))

    def test_storehouse_bridge_route_is_open_through_the_gate(self):
        z = (base.GATE_Z[0]+base.GATE_Z[1])/2
        for x in (17.5, 20.6, base.TX+1, (base.TX+base.STORE[0])/2, base.STORE[0]+.4, base.STORE[0]+1.6, 33.0):
            t, ny = self.soup.hits((x, TER+1, z), (0, -1, 0))
            self.assertAlmostEqual(t[0], 1.0, places=4, msg=x); self.assertGreater(ny[0], .999, x)
            self.assertGreater(self.soup.first((x, TER+.2, z), (0, 1, 0)), 3.5, (x, 'headroom'))
        # Straight through the gate and the door: the first thing in the way
        # is the storehouse baffle, not the curtain or the wall.
        baffle = base.STORE[0]+base.WALL+2.4
        self.assertAlmostEqual(self.soup.first((17.0, TER+1.2, z), (1, 0, 0)), baffle-17.0, places=3)

    # --- Whole map ----------------------------------------------------------
    @classmethod
    def whole_map(cls):
        if not hasattr(cls, '_map'):
            spec = build.spec(); mesh = kit.Mesh()
            flags = build.build_structures(spec, mesh)[0]
            turret_arcs.assign(mesh.entities, flags)
            grid = build.terrain_grid(spec)
            cls._map = spec, mesh, Soup(mesh.collision), grid
        return cls._map

    # Rooms no turret may see into beyond the doorway depth of an open
    # opening: (name, x0, x1, z0, z1, floor), local. The storehouse's
    # baffled entry vestibules lie outside its boxes.
    ROOMS = [('keep hall', -(base.KX-base.WALL)+.6, base.KX-base.WALL-.6,
              base.KZ0+base.WALL+.6, base.KZ1-base.WALL-.6, TER),
             ('cistern', base.CIS[0]+.6, base.CIS[1]-.6, base.CIS[2]+.6, base.CIS[3]-.6, base.CIS_FLOOR),
             ('tunnel north', base.TUNNEL_N[0]+1.4, base.TUNNEL_N[1]-1.4, base.TUNNEL_N[2]+.6, base.TUNNEL_W[3]-1.4, base.CIS_FLOOR),
             ('tunnel west', base.TUNNEL_W[0]+.6, base.TUNNEL_W[1]+.6, base.TUNNEL_W[2]+1.4, base.TUNNEL_W[3]-1.4, base.CIS_FLOOR),
             ('tower room', base.TOWER[0]+1.4, base.TOWER_HOLE[1]-1.4, base.TOWER[2]+1.4, base.TOWER_RAMP[2], base.CIS_FLOOR),
             ('store ground', base.STORE[0]+base.WALL+3.4, base.STORE[1]-base.WALL-.6,
              base.STORE[2]+base.WALL+.6, base.STORE[3]-base.WALL-3.4, 0.0),
             ('store mezzanine', base.STORE[0]+base.WALL+3.4, base.MEZZ_X-.6,
              base.STORE[2]+base.WALL+.6, base.STORE[3]-base.WALL-.6, base.STORE_UP)]

    def test_turrets_cannot_see_into_rooms(self):
        """No turret engages a player (field of fire, sensor-extended range,
        clear line from its barrel to the chest) anywhere in the keep hall,
        the cistern, the tunnel, the tower room or the storehouse, beyond the
        doorway depth (sightline_checks.DOOR_DEPTH) of an open door."""
        spec, mesh, soup, _ = self.whole_map()
        targets, counts, openings = [], {}, []
        ix, iz0, iz1 = base.KX-base.WALL, base.KZ0+base.WALL, base.KZ1-base.WALL
        local = [(*base.DOOR, iz0), (*base.DOOR, iz1), (*base.TOWER_DOOR, base.TOWER[3]-.5)]
        for b in spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            pt = lambda x, z: tuple(m.point((x, 0, z))[i] for i in (0, 2))
            openings += [(pt(x0, z), pt(x1, z)) for x0, x1, z in local]
            for name, x0, x1, z0, z1, floor in self.ROOMS:
                for x in np.arange(x0, x1+.01, 1.0):
                    for z in np.arange(z0, z1+.01, 1.0):
                        if soup.first(m.point((x, floor+.1, z)), (0, 1, 0), 2.3) < 2.3: continue   # inside a prop
                        t, _ = soup.hits(m.point((x, floor+1, z)), (0, -1, 0))
                        if not len(t) or abs(t[0]-1) > .02: continue                              # not this floor
                        targets.append(m.point((x, floor+LIFT+.8, z))); counts[name] = counts.get(name, 0)+1
        targets = np.array(targets)
        turrets = [e for e in mesh.entities if e['kind'] == 'turret']
        self.assertEqual(len(turrets), 6)
        allowed = sightline_checks.near_openings(targets, openings)
        for e in turrets:
            seen = sightline_checks.visible(soup, e, targets) & ~allowed
            self.assertEqual(int(seen.sum()), 0, (e['id'], targets[seen][:5]))
        for name, *_ in self.ROOMS: self.assertGreater(counts.get(name, 0), 20, (name, counts))
        self.assertGreater(len(targets), 1500)

    def test_terrain_is_fair_dune_like_and_matches_targets(self):
        """Each half its own dunes (not a mirror), the two measuring alike
        (assets/fairness.py); the sewer's corridor stays shared, so the
        culvert fits both halves."""
        spec, _, _, grid = self.whole_map()
        self.assertTrue(np.array_equal(grid, build.terrain_grid(spec)), 'terrain must be deterministic')
        self.assertFalse(fairness.symmetric(grid), 'each half its own ground')
        q = sewer.quantize(grid)
        for ix, iz in {(ix+dx, iz+dz) for ix, iz in sewer.all_cells() for dx in (0, 1) for dz in (0, 1)}:
            self.assertEqual(q[iz, ix], q[256-iz, 256-ix], (ix, iz, 'the sewer corridor is shared'))
        flags = build.build_structures(spec, kit.Mesh())[0]
        for row in fairness.assert_fair(self, grid, flags): print('half', {k: round(v, 2) for k, v in row.items()})
        slope = dustreach_terrain.slope_degrees(grid)
        rows, cols = slice(int((1024-700)/8), int((1024+700)/8)), slice(int((1024-420)/8), int((1024+420)/8))
        play = slope[rows, cols]
        self.assertTrue(13 <= np.median(play) <= 19, np.median(play))
        self.assertTrue(26 <= np.percentile(play, 90) <= 34, np.percentile(play, 90))
        # The wadi floors are smooth by design (a gentle grade to the pans).
        self.assertLess((play < 3).mean(), .09)
        # The saddle at the gate is high ground; basins lie in the field and
        # in front of each citadel.
        prof = lambda z: build.terrain_height(1024, z, grid)
        for sgn in (-1, 1):
            self.assertGreater(prof(1024)-prof(1024+sgn*275), 25)
            self.assertGreater(prof(1024+sgn*dustreach_terrain.BASE_D)-prof(1024+sgn*505), 10)
        flags = []
        for b in spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            flags.append(m.point((base.FLAG[0], 0, base.FLAG[1])))
        self.assertTrue(1280 <= math.dist(flags[0][::2], flags[1][::2]) <= 1320)

    def test_wadis_are_sunken_smooth_and_run_down_to_midfield(self):
        """v2 lanes: each side a wadi sunk into the dunes, its floor smooth
        and falling gently from each citadel's flank to a pan at midfield,
        where the wadi capture point stands; the dune sea between it and the
        centre lane."""
        spec, _, _, grid = self.whole_map()
        h = lambda x, z: build.terrain_height(x, z, grid)
        T = dustreach_terrain
        for side in (-1, 1):
            floor = []
            for Z in np.arange(-600.0, 601.0, 40.0):
                x = 1024+float(T.wadi_centre(np.array(Z), side))
                f = h(x, 1024+Z); floor.append(f)
                # Sunk through the field; toward each end it rises to meet
                # the citadel's flank.
                if abs(Z) <= 440:
                    banks = [h(x+dx, 1024+Z) for dx in (-T.WADI_BANK-12, T.WADI_BANK+12)]
                    self.assertGreater(min(banks)-f, 5, (side, Z, 'sunk'))
            grade = np.degrees(np.arctan(np.abs(np.diff(floor))/40))
            self.assertLess(grade.max(), 6, (side, 'smooth floor'))
            self.assertGreater(floor[0]-floor[len(floor)//2], 12, (side, 'downhill to the pan'))
            self.assertGreater(floor[-1]-floor[len(floor)//2], 12, (side, 'and up to the far citadel'))
        wadis = [c for c in spec['control_points'] if c['id'].endswith('wadi')]
        for c, side in zip(wadis, (-1, 1)):
            self.assertLess(abs(c['x']-1024-float(T.wadi_centre(np.array(c['z']-1024.0), side))), 4, c['id'])
        for wx, _, wz, _, _ in build.pieces(spec):
            for side in (-1, 1):
                self.assertGreater(abs(wx-1024-float(T.wadi_centre(np.array(wz-1024.0), side))), 80, ('ruin in a wadi', wx, wz))

    def test_ground_stays_under_structures(self):
        spec, _, _, grid = self.whole_map()
        h = lambda x, z: build.terrain_height(x, z, grid)
        for b in spec['bases']:
            oy = b['position'][1]
            local = lambda x, z: h(*build.to_world(b, x, z))-oy
            # Under the terrace, the tower, the pad and the ruin: at or under the ground line.
            for x in np.arange(-base.TX, base.TX+.1, 2):
                for z in np.arange(base.TZ0, base.TZ1+.1, 2):
                    self.assertLess(local(x, z), .02, ('terrace', x, z))
            sx0, sx1, sz0, sz1 = base.STORE
            for x in np.arange(sx0, sx1+.1, 2):
                for z in np.arange(sz0, sz1+.1, 2):
                    self.assertLess(local(x, z), .02, ('storehouse', x, z))
            px, pz = base.PAD; H = base.PAD_HALF
            for x in np.arange(px-H, px+H+.1, 2):
                for z in np.arange(pz-H, pz+H+.1, 2):
                    self.assertLess(local(x, z), .02, ('pad', x, z))
            cx, cz = base.RUIN
            for r in np.arange(0, base.RUIN_R+.8, 1):
                for a in np.linspace(0, math.tau, 12, endpoint=False):
                    self.assertLess(local(cx+r*math.cos(a), cz+r*math.sin(a)), .02, ('ruin', r))
            # Ramp feet meet the ground: level at the foot, below the surface along it.
            for ramp, fn in [(base.FRONT_RAMP, base.front_ramp_y), (base.BACK_RAMP, base.back_ramp_y),
                             (base.ruin_ramp(), lambda z, r=base.ruin_ramp(): base.ramp_y(r, z, 0, base.RUIN_TOP))]:
                x0, x1, zg, zt = ramp
                self.assertAlmostEqual(local((x0+x1)/2, zg), -.1, delta=.05)
                for z in np.linspace(zg, zt, 9):
                    for x in (x0, x1):
                        self.assertLess(local(x, z), fn(z)+.02, (ramp, x, z))
        gx, gy, gz = spec['gate']['position']
        # The gate's paving (to 22 m out) sits on level ground: exact where the
        # 8 m grid vertices are all inside the flattened disc.
        for r in np.arange(0, gate.SITE_R-12, 2.0):
            for a in np.linspace(0, math.tau, 16, endpoint=False):
                self.assertAlmostEqual(h(gx+r*math.cos(a), gz+r*math.sin(a)), gy-.06, delta=1/32)
        # Midfield ruins stand in their ground: within a metre of their base
        # under the footprint (their walls run 2 m below it).
        for wx, y, wz, _, _ in build.pieces(spec):
            for dx in np.arange(-6, 6.1, 2.0):
                for dz in np.arange(-4, 4.1, 2.0):
                    self.assertLess(abs(h(wx+dx, wz+dz)-(y-.1)), 1.0, (wx, wz, dx, dz))

    def test_gate_is_symmetric_and_the_lane_runs_through_it(self):
        mesh = kit.Mesh(); anchors = gate.build(mesh)
        soup = Soup(mesh.collision)
        tris = np.array(mesh.collision, np.float64).reshape(-1, 3)
        mirrored = tris*np.array([-1, 1, -1])
        key = lambda a: set(map(tuple, np.round(a, 4)))
        self.assertEqual(key(tris), key(mirrored), 'the gate must match its 180 degree rotation')
        for x in (-5, 0, 5):
            for y in (.3, 1.5, 8, 16):
                self.assertEqual(soup.first((x, y, -60), (0, 0, 1)), np.inf, (x, y, 'lane blocked'))
        for x, z in ((0, 0), (8, 0), (-8, 0)):
            t, ny = soup.hits((x, gate.ATTIC_TOP+2 if abs(x) < gate.ATTIC_X else gate.LINTEL_TOP+2, z), (0, -1, 0))
            self.assertLess(t[0], 2.01); self.assertGreater(ny[0], .999)
        self.assertLess(len(mesh.collision)//9, 800)
        pieces = build.pieces(build.spec())
        self.assertEqual(len(pieces), 2*len(gate.SCATTER))
        # Placed in turned pairs; each rests on its own half's ground.
        for (x, y, z, yaw, kind), (x2, y2, z2, yaw2, kind2) in zip(pieces[::2], pieces[1::2]):
            self.assertAlmostEqual(x+x2, 2048); self.assertAlmostEqual(z+z2, 2048)
            self.assertEqual(kind, kind2)

    # --- Pack --------------------------------------------------------------
    def test_lightmap_bake_is_deterministic(self):
        from assets import lightmap_bake
        m = kit.Mesh(); m.lamps = []; gate.build(m)
        verts = np.array(m.vertices, np.float32).reshape(-1, 12)
        sun = (-.57735, .57735, -.57735); light = kit.MATERIALS.index('light')
        a, pa = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=1)
        b, pb = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=3)
        self.assertTrue(np.array_equal(a, b)); self.assertEqual(pa, pb)
        self.assertTrue(np.all((a[:, 8] >= 0) & (a[:, 8] <= 1) & (a[:, 9] >= 0) & (a[:, 9] <= 1)))

    def test_wind_loop_is_deterministic_and_bounded(self):
        a = build.wind(5); self.assertEqual(a, build.wind(5))
        samples = np.frombuffer(a, '<f4')
        self.assertEqual(len(samples), 44100*4)
        self.assertLess(np.abs(samples).max(), 1.0); self.assertGreater(np.abs(samples).max(), .01)

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
    """The hard-surface detail (scripts/models/dustreach_models.py) in the
    committed pack: where each root is placed, and that it hugs the builder's
    solid boxes it was made from (copings flush with the tops)."""
    @classmethod
    def setUpClass(cls):
        from assets import cladding, map_models
        cls.pack = ROOT/'assets/maps/dustreach'
        cls.man = json.loads((cls.pack/'map.json').read_text())
        cls.models = map_models
        cls.recorded = {}
        for name, fn in (('dr-citadel', lambda m: base.build(m, 0, 'red')), ('dr-gate', gate.build),
                         ('dr-ruin-wall', lambda m: gate.build_piece(m, 'wall'))):
            m = cladding.recorder(kit.Mesh)(); m.lamps = []; fn(m)
            cls.recorded[name] = m
        cls.cladding = cladding

    def read(self, name):
        return self.models.read(self.pack/'models.glb', name)

    def test_each_structure_places_its_model(self):
        spec = build.spec()
        placed = self.man['models']
        citadels = [p for p in placed if p['model'] == 'dr-citadel']
        self.assertEqual(sorted((p['team'], tuple(p['position']), p['yaw']) for p in citadels),
                         sorted((b['team'], tuple(map(float, b['position'])), float(b['yaw'])) for b in spec['bases']))
        gates = [p for p in placed if p['model'] == 'dr-gate']
        self.assertEqual([(tuple(g['position']), g['team']) for g in gates], [(tuple(map(float, spec['gate']['position'])), None)])
        walls = [p for p in placed if p['model'] == 'dr-ruin-wall']
        want = [(wx, y, wz, math.degrees(yaw)) for wx, y, wz, yaw, kind in build.pieces(spec) if kind == 'wall']
        self.assertEqual(len(walls), len(want))
        for w, (x, y, z, yaw) in zip(walls, want):
            self.assertTrue(np.allclose(w['position'], (x, y, z)) and abs(w['yaw']-yaw) < 1e-4, (w, x, z, yaw))
        for name in ('dr-citadel', 'dr-gate', 'dr-ruin-wall'):
            self.assertGreater(len(self.read(name).tri), 300, name)

    def test_copings_are_flush_with_the_wall_tops(self):
        """Over the middle of every exposed wall and merlon top the model's
        coping stands at most 2 cm proud: a player stands where the solid is."""
        checked = 0
        for name, m in self.recorded.items():
            model = self.read(name)
            for top in self.cladding.tops(m.boxes):
                b = top.box
                if b.tag or b.mat != 'concrete' or min(b.x1-b.x0, b.z1-b.z0) > 1.7: continue
                pts = [((x0+x1)/2, (z0+z1)/2) for x0, x1, z0, z1 in top.rects]
                for (x, z), y in zip(pts, self.models.surface_heights(model, pts, b.y1+.3)):
                    self.assertTrue(b.y1-1e-3 <= y <= b.y1+.02, (name, b, x, z, y))
                    checked += 1
        self.assertGreater(checked, 100)

    def test_detail_hugs_the_solid_boxes(self):
        """Every model vertex lies within 0.35 m of the solid boxes it was made
        from (fins, lips, skirts and guards stand at most that far out)."""
        for name, m in self.recorded.items():
            boxes = np.array([(b.x0, b.y0, b.z0, b.x1, b.y1, b.z1) for b in m.boxes if b.solid])
            pos = np.unique(self.read(name).pos.round(4), axis=0)
            far = 0.0
            for chunk in np.array_split(pos, max(1, len(pos)//2000)):
                d = np.maximum(np.maximum(boxes[None, :, :3]-chunk[:, None], chunk[:, None]-boxes[None, :, 3:]), 0)
                far = max(far, float(np.sqrt((d**2).sum(-1)).min(1).max()))
            self.assertLess(far, .35, name)


class SewerTests(unittest.TestCase):
    """The cross-map sewer (assets/dustreach_sewer.py): a covered culvert
    from one team's rear to the other's, open at two trench mouths, two
    midfield shafts and two flank shafts."""
    @classmethod
    def setUpClass(cls):
        spec, mesh, _, grid = DustreachTests.whole_map()
        cls.spec, cls.grid = spec, grid
        cls.p = sewer.plan(grid)
        m = kit.Mesh(); m.lamps = []
        cls.anchors, cls.lamps = sewer.build(m, grid)
        cls.sewer_mesh = m
        cls.lid_render, cls.lid_collision = sewer.lid(grid)
        cls.soup = Soup(list(mesh.collision)+list(m.collision)+cls.lid_collision)
        cls.structures = mesh

    def floor(self, x, z):
        """Expected floor at a world point on the culvert's centre lanes."""
        p = self.p
        rx, rz = (2048-x, 2048-z) if z > 1024 else (x, z)
        if sewer.BRANCH_IZ*8 <= rz <= sewer.BRANCH_IZ*8+8 and rx > 1080:
            return float(np.interp(rx, p['branch_x'], p['branch']))
        if rz < sewer.LEG_Z[0]: return sewer.trench_floor(p, rx, rz)
        if rz >= sewer.LEG_Z[1]: return p['hall']
        return float(np.interp(rz, p['leg_z'], p['leg']))

    def test_cells_are_symmetric_and_clear_of_every_base_room(self):
        cells = sewer.all_cells()
        for c, kind in cells.items():
            self.assertEqual(cells.get(sewer.mirror(c)), kind, c)
        base_cells = set(build.holes(self.spec))-set(sewer.holes())
        self.assertEqual(len(build.holes(self.spec)), len(base_cells)+len(cells))
        # At least 12 m from the terrace, tower, storehouse, cistern and tunnel.
        rects = [(-base.TX, base.TX, base.TZ0, base.TZ1), base.TOWER, base.STORE, *base.HOLES.values()]
        for b in self.spec['bases']:
            for x0, x1, z0, z1 in rects:
                (ax, az), (bx, bz) = build.to_world(b, x0, z0), build.to_world(b, x1, z1)
                wx0, wx1, wz0, wz1 = min(ax, bx), max(ax, bx), min(az, bz), max(az, bz)
                for ix, iz in cells:
                    cx0, cx1, cz0, cz1 = ix*8, ix*8+8, iz*8, iz*8+8
                    gap = math.hypot(max(wx0-cx1, cx0-wx1, 0), max(wz0-cz1, cz0-wz1, 0))
                    self.assertGreaterEqual(gap, 12.0, ((ix, iz), (x0, x1, z0, z1)))

    def test_floor_grades_and_cover(self):
        p = self.p
        sewer.check_cover(p)
        leg = np.degrees(np.arctan(np.abs(np.diff(p['leg']))/8))
        self.assertLessEqual(leg[sewer.PORTAL_CELLS:].max(), 10+1e-9)
        self.assertLessEqual(leg.max(), 16+1e-9)
        self.assertLessEqual(np.degrees(np.arctan(np.abs(np.diff(p['branch']))/8)).max(), 10+1e-9)
        for x in (1072, 1076, 1080):
            t = [sewer.trench_floor(p, x, z) for z in np.arange(sewer.TRENCH_Z[0], sewer.TRENCH_Z[1]+.1, 2.0)]
            self.assertLess(np.degrees(np.arctan(np.abs(np.diff(t))/2)).max(), 22, x)
        # Level cells: the junction, both shafts and the hall.
        j = int((sewer.BRANCH_IZ*8-sewer.LEG_Z[0])//8); m = int((sewer.MID_IZ*8-sewer.LEG_Z[0])//8)
        self.assertEqual(p['leg'][j], p['leg'][j+1]); self.assertEqual(p['leg'][j], p['branch'][0])
        self.assertEqual(p['leg'][m], p['leg'][m+1]); self.assertEqual(p['branch'][-1], p['branch'][-2])
        self.assertEqual(p['leg'][-1], p['hall'])

    def test_roof_copies_the_terrain_exactly(self):
        q = sewer.quantize(self.grid)
        col = np.array(self.lid_collision).reshape(-1, 3)
        self.assertEqual(len(col), 6*len(sewer.roofed_cells()))
        for x, y, z in col:
            self.assertEqual(y, q[int(round(z/8)), int(round(x/8))])
        render = np.array(self.lid_render).reshape(-1, 12)
        self.assertTrue(np.all(render[:, 11] == -2.0), 'the roof renders as terrain')
        roof = Soup(self.lid_collision)
        for ix, iz in sewer.roofed_cells():
            for fx, fz in ((.25, .3), (.6, .7)):
                x, z = (ix+fx)*8, (iz+fz)*8
                t, _ = roof.hits((x, 400, z), (0, -1, 0))
                self.assertAlmostEqual(400-t[0], sewer.surface(q, x, z), places=3)

    def route(self):
        """Centre lane from the red trench top to the blue trench top, 2 m steps."""
        p, pts = self.p, []
        for z in np.arange(586, 1018, 2.0): pts.append((1076.0, z))
        for x in np.arange(1076, 970, -2.0): pts.append((x, 1024.0))
        for z in np.arange(1030, 1462.1, 2.0): pts.append((972.0, z))
        return pts

    def assert_lane_clear(self, xz, name):
        """A body-wide band at knee, chest and head height is clear between
        every pair of lane points, each over a floor where expected."""
        pts = []
        for x, z in xz:
            e = self.floor(x, z)
            t, ny = self.soup.hits((x, e+2.0, z), (0, -1, 0))
            self.assertTrue(len(t), (name, x, z, 'no floor'))
            # The trench floor is straight between 8 m corners under its curve.
            self.assertAlmostEqual(e+2.0-t[0], e, delta=.25 if min(z, 2048-z) < sewer.LEG_Z[0] else .02, msg=(name, x, z))
            e = e+2.0-t[0]
            pts.append(np.array((x, e, z)))
        for a, b in zip(pts, pts[1:]):
            d = b-a; side = np.cross(d/np.linalg.norm(d), (0, 1, 0))
            for h in (.45, 1.2, 2.2):
                for off in (-.45, 0, .45):
                    s = a+(0, h, 0)+side*off; e = b+(0, h, 0)+side*off
                    self.assertEqual(self.soup.first(s, e-s, 1.0), np.inf, (name, a, b, h, off))

    def test_culvert_is_walkable_mouth_to_mouth_and_to_both_flank_shafts(self):
        self.assert_lane_clear(self.route(), 'main')
        for side in (1, -1):
            xs = np.arange(1076, 1157, 2.0)
            bz = sewer.BRANCH_IZ*8+4.0
            lane = [(x, bz) for x in xs] if side > 0 else [(2048-x, 2048-bz) for x in xs]
            self.assert_lane_clear(lane, f'branch {side}')

    def test_open_cells_have_walls_up_to_the_ground_and_covered_cells_a_roof(self):
        q = sewer.quantize(self.grid); cells = sewer.all_cells()
        for (ix, iz), kind in cells.items():
            x0, z0 = ix*8, iz*8
            fc = self.floor(x0+4, z0+4)
            if kind not in sewer.OPEN:
                self.assertAlmostEqual(self.soup.first((x0+4, fc+.5, z0+4), (0, 1, 0)), sewer.HEAD-.5, delta=.6)
                continue
            self.assertEqual(self.soup.first((x0+4, fc+.5, z0+4), (0, 1, 0)), np.inf, ((ix, iz), 'open to the sky'))
            for (dx, dz), (ex, ez) in (((-1, 0), (x0, None)), ((1, 0), (x0+8, None)), ((0, -1), (None, z0)), ((0, 1), (None, z0+8))):
                n = cells.get((ix+dx, iz+dz))
                if n is not None and n in sewer.OPEN: continue
                for u in np.arange(.4, 7.61, 1.2):
                    x = ex if ex is not None else x0+u
                    z = ez if ez is not None else z0+u
                    top = sewer.edge(q, x, z)
                    fl = self.floor(x-dx*.5, z-dz*.5)
                    low = fl+sewer.HEAD+.3 if n is not None else fl+.3
                    for y in np.arange(low, top-.3, 1.0):
                        o = (x-dx*.4, y, z-dz*.4)
                        self.assertLess(self.soup.first(o, (dx, 0, dz), 1.0), .6, ((ix, iz), kind, x, z, y, 'seam'))

    def test_no_turret_can_see_into_the_covered_culvert_and_no_one_spawns_there(self):
        targets = []
        for ix, iz in sewer.roofed_cells():
            for fx, fz in ((.25, .25), (.75, .25), (.25, .75), (.75, .75)):
                x, z = (ix+fx)*8, (iz+fz)*8
                targets.append((x, self.floor(x, z)+LIFT+.8, z))
        targets = np.array(targets)
        turrets = [e for e in self.structures.entities if e['kind'] == 'turret']
        for e in turrets:
            p = np.array(e['position']); d = targets-p; dist = np.linalg.norm(d, axis=1)
            near = dist < 150
            if not near.any(): continue
            starts = p+d[near]/dist[near, None]*(e['radius']+.6)
            clear = ~self.soup.blocked_many(starts, targets[near])
            self.assertEqual(int(clear.sum()), 0, (e['id'], targets[near][clear][:5]))
        import json
        pack = json.loads((HERE.parent/'assets/maps/dustreach/map.json').read_text())
        for team in pack['spawn_points']:
            for x, y, z, _ in team:
                self.assertNotIn((int(x//8), int(z//8)), sewer.all_cells(), (x, z))

    def test_openings_sit_where_the_design_says(self):
        a = self.anchors
        red_flag = np.array(build.to_world(self.spec['bases'][0], *base.FLAG))
        self.assertLess(np.hypot(*(np.array(a['mouth_red'])[[0, 2]]-red_flag)), 60)
        for name in ('shaft_mid_red', 'shaft_flank_red'):
            depth = sewer.surface(self.p['q'], a[name][0], a[name][2])-a[name][1]
            self.assertTrue(6 < depth < 20, (name, depth))
        # The flank shaft is well out on the flank: past the landing pad and
        # 90 m or more from the citadel's walls.
        fx, _, fz = a['shaft_flank_red']
        self.assertGreater(fx-4-build.to_world(self.spec['bases'][0], base.TOWER[0], 0)[0], 90)
        for name in a:
            if name.endswith('_red'):
                r, b = np.array(a[name]), np.array(a[name.replace('_red', '_blue')])
                self.assertTrue(np.allclose(r[[0, 2]]+b[[0, 2]], 2048) and r[1] == b[1], name)

    def test_geometry_fits_its_budget_without_degenerate_triangles(self):
        tris = np.array(self.sewer_mesh.collision).reshape(-1, 3, 3)
        area = np.linalg.norm(np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0]), axis=1)
        self.assertGreater(area.min(), 1e-6)
        # v2's legs are 656 m each (the citadels moved 288 m out).
        self.assertLess(len(tris)+len(self.lid_collision)//9, 2600)


def _clear(tris, o, d, tmax):
    """No collision triangle between o and o+d*tmax."""
    a, e1, e2 = tris[:, 0], tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0]
    h = np.cross(d, e2); det = (e1*h).sum(1); ok = np.abs(det) > 1e-9
    inv = np.where(ok, 1/np.where(ok, det, 1), 0); s = o-a
    u = (s*h).sum(1)*inv; q = np.cross(s, e1); v = (q@d)*inv; t = (e2*q).sum(1)*inv
    return not (ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > 1e-4) & (t < tmax)).any()


def hovering_edges(mesh, grid, lo=.05, hi=2.0):
    """Visible bottom edges of near-vertical faces floating lo..hi metres over
    the terrain with nothing solid under or beside them (the visual audit's
    "hovering edges"). A face counts only if the gap under it sees the sky, so
    edges inside the hollow terrace or tower are ignored, and a face that
    touches a solid (a band on a wall, a cap on a post) is attached."""
    col = np.asarray(mesh.collision, np.float64).reshape(-1, 3, 3)
    ren = np.frombuffer(mesh.vertices.tobytes(), np.float32).reshape(-1, 3, 12)[:, :, :3].astype(float)
    every = np.concatenate([col, ren])            # render-only parts hold each other up too (caps on posts)
    up = np.array([0., 1., 0.]); out = []
    sides = [np.array(d, float) for d in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1))]
    for tris in (col, ren):
        n = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0]); ln = np.linalg.norm(n, axis=1)
        vert = (ln > 1e-6) & (np.abs(n[:, 1]) < .1*ln)
        for tri, nn in zip(tris[vert], n[vert]/ln[vert, None]):
            e = tri[np.abs(tri[:, 1]-tri[:, 1].min()) < .01]
            if len(e) < 2: continue
            q = (e[0]+e[1])/2; gap = q[1]-build.terrain_height(q[0], q[2], grid)
            if not lo < gap < hi: continue
            attached = any(not _clear(every, q+sg*nn*d+np.array([0, .01, 0]), -up, gap+4.0)
                           for d in (.12, .3, .5) for sg in (-1, 1))
            attached = attached or any(not _clear(every, q+np.array([0, .05, 0]), d_, .3) for d_ in sides)
            if attached: continue
            for side in (nn, -nn):
                o = q+side*.08-np.array([0, .03, 0])
                if _clear(col, o, -up, gap+.02) and _clear(col, o, up, 60):
                    out.append((round(gap, 2), tuple(np.round(q, 1)))); break
    return out


class DustreachPassTests(unittest.TestCase):
    """The 2026-09 pass: survey fixes (8 m cistern, one-hall storehouse with a
    mezzanine, 6 m gates and arcades), the open flag court's routes, Capture &
    Hold points and the visual-audit fixes (no z-fighting, no hovering edges,
    procedural desert sky)."""
    @classmethod
    def setUpClass(cls):
        from assets import cnh_tower
        cls.spec = build.spec(); cls.mesh = kit.Mesh(); cls.mesh.lamps = []
        build.build_structures(cls.spec, cls.mesh)
        cls.grid = build.terrain_grid(cls.spec)
        sewer.build(cls.mesh, cls.grid)
        cls.points, _ = build.build_points(cls.spec, cls.mesh, cls.grid)
        cls.base_mesh, cls.anchors = one_base()
        cls.soup = Soup(cls.base_mesh.collision)

    def test_no_z_fighting_anywhere(self):
        from assets import surface_checks
        # Covers everything, the kit turret mounts included.
        found = surface_checks.z_fighting(self.mesh.vertices, self.mesh.collision)
        self.assertEqual(found, [], sorted(found, key=lambda f: -f[0])[:5])

    def test_no_visible_hovering_edges(self):
        found = hovering_edges(self.mesh, self.grid)
        self.assertEqual(found, [], sorted(found)[-5:])

    def test_cistern_is_eight_metres_with_the_stair_against_the_wall(self):
        self.assertGreaterEqual(base.CIS_CEIL-base.CIS_FLOOR, 8.0)
        self.assertAlmostEqual(base.STAIR[1], base.CIS[1])              # the stair runs down the east wall
        C = base.CIS_FLOOR
        for x, z in ((-8, -12), (0, -12), (-10, 3), (4, 3)):             # open floor clear of the columns
            self.assertGreater(self.soup.first((x, C+.2, z), (0, 1, 0)), 7.7, (x, z))
        # The stair leaves the whole room west of it unbisected.
        for z in (-15.0, -8.8, 4.0):                                    # rows clear of the columns and generator
            self.assertEqual(self.soup.first((base.CIS[0]+1, C+1.0, z), (1, 0, 0), base.STAIR[0]-base.CIS[0]-1.2), np.inf, z)

    def test_storehouse_is_one_tall_hall_entered_from_both_levels(self):
        i1 = base.STORE[1]-base.WALL
        for x, z in ((40, 5), (44, 9), (46, 19), (39, 19)):              # the open hall: floor to ceiling
            self.assertGreater(self.soup.first((x, .2, z), (0, 1, 0)), 9.5, (x, z))
        # Mezzanine at bridge level, reached from the gate (upper) and by the ramp (ground).
        zc = sum(base.GATE_Z)/2
        t, ny = self.soup.hits((base.STORE[0]+base.WALL+1, base.STORE_UP+1, zc), (0, -1, 0))
        self.assertAlmostEqual(t[0], 1.0, places=4)
        r0, r1, rzg, rzt = base.STORE_RAMP
        self.assertAlmostEqual(base.store_ramp_y(rzg), 0.0); self.assertAlmostEqual(base.store_ramp_y(rzt), base.STORE_UP)
        t, _ = self.soup.hits(((r0+r1)/2, base.STORE_UP+1, rzt+.4), (0, -1, 0))
        self.assertAlmostEqual(t[0], 1.0, places=3)                     # the landing continues the ramp's top
        self.assertLess(i1-base.STORE_LANDING[1], 10.5); self.assertGreater(i1-base.STORE_LANDING[1], 9.0)

    def test_gates_and_arcades_are_wide(self):
        self.assertGreaterEqual(base.DOOR[1]-base.DOOR[0], 6.0)
        self.assertGreaterEqual(base.DOOR_TOP-base.TER, 5.5)
        self.assertGreaterEqual(base.GATE_Z[1]-base.GATE_Z[0], 6.0)
        self.assertGreaterEqual(base.GATE_TOP-base.TER, 6.0)
        gaps = np.diff(base.ARCADE_Z)-2*.62
        self.assertGreaterEqual(gaps.min(), 5.0, gaps)
        self.assertGreaterEqual(base.ARCADE_ROOF-1.0-base.TER, 5.4)

    def test_flag_court_is_open_to_the_sky(self):
        fx, fz = base.FLAG
        for dx in (-5, 0, 5):
            for dz in (-3, 0, 3):
                self.assertEqual(self.soup.first((fx+dx, base.PIT_FLOOR+.5, fz+dz), (0, 1, 0)), np.inf, (dx, dz))

    def test_capture_points_are_placed_symmetric_and_level(self):
        from assets import cnh_tower
        ids = [p['id'] for p in self.points]
        self.assertEqual(ids, ['sun-gate', 'west-wadi', 'east-wadi'])
        gate = self.spec['gate']['position']
        self.assertEqual(self.points[0]['pos'], [gate[0], gate[1], gate[2]])
        w, e = np.array(self.points[1]['pos']), np.array(self.points[2]['pos'])
        self.assertTrue(np.allclose(w[[0, 2]]+e[[0, 2]], 2048), 'the flank points mirror through the centre')
        for p in self.points:
            self.assertEqual(p['radius'], cnh_tower.RING); self.assertFalse(p['ctf_active'])
        for p in self.points[1:]:
            x, y, z = p['pos']
            ring = [build.terrain_height(x+r*math.cos(a), z+r*math.sin(a), self.grid)
                    for r in (0, 4, 8, 12) for a in np.linspace(0, math.tau, 12, endpoint=False)]
            self.assertLess(max(ring)-min(ring), cnh_tower.MAX_TILT, p['id'])
        # No tower sits over a sewer or base hole.
        cells = set(build.holes(self.spec))
        for p in self.points[1:]:
            x, _, z = p['pos']
            for dx in range(-16, 17, 4):
                for dz in range(-16, 17, 4):
                    ix, iz = int((x+dx)//8), int((z+dz)//8)
                    self.assertNotIn(iz*256+ix, cells, (p['id'], dx, dz))
        # The Sun Gate carries the capture ring markers; the flag lane under it stays clear.
        self.assertEqual(gate_module_ring(), cnh_tower.RING)

    def test_committed_pack_declares_points_look_and_many_flag_routes(self):
        import json
        from assets import route_checks
        root = Path(__file__).resolve().parent.parent
        pack = root/'assets/maps/dustreach'
        manifest = json.loads((pack/'map.json').read_text())
        self.assertEqual([p['id'] for p in manifest['control_points']], ['sun-gate', 'west-wadi', 'east-wadi'])
        self.assertIn('sky', manifest['look'])
        self.assertNotIn('sun_direction', manifest['look'])          # keeps the baked sun
        rc = route_checks.Pack(pack)
        fx, fz = base.FLAG
        for b in manifest_bases(self.spec):
            ox, oy, oz, s = b
            def world(bx):
                x0, x1, y0, y1, z0, z1 = bx
                xs = sorted((ox+s*x0, ox+s*x1)); zs = sorted((oz+s*z0, oz+s*z1))
                return (xs[0], xs[1], oy+y0, oy+y1, zs[0], zs[1])
            r = route_checks.flag_routes(rc, (ox, oz+s*6), world((-base.TX, base.TX, base.PIT_FLOOR-.5, base.ROOF+1, base.TZ0, base.TZ1)),
                                         world((fx-5, fx+5, base.PIT_FLOOR-.5, base.PIT_FLOOR+.8, fz-4, fz+4)),
                                         extent=80.0, airborne=True)
            self.assertGreaterEqual(r['routes'], 12, (s, r['routes']))


def gate_module_ring():
    return gate.RING


def manifest_bases(spec):
    return [(*b['position'], -1 if b['yaw'] == 180 else 1) for b in spec['bases']]


class SpawnForwardClearance(unittest.TestCase):
    """Every committed spawn faces open floor: a clear body-width view for
    6 m and the same floor for a half-second walk (docs/map-pipeline.md)."""
    def test_committed_spawns_face_open_floor(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/dustreach'
        self.assertEqual(spawn_checks.problems(pack), [])

    def test_no_indoor_spawn_shows_through_an_opening_from_the_field(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/dustreach'
        self.assertEqual(spawn_checks.exposed(pack), [])


class PropsAndShade(unittest.TestCase):
    """Props and the terrain shade map on the committed pack (assets/prop_checks.py)."""
    def test_props_and_terrain_shade(self):
        from assets import prop_checks
        prop_checks.check_pack(self, Path(__file__).resolve().parent.parent/'assets/maps/dustreach')

class Water(unittest.TestCase):
    """Mirrored ponds carved into the terrain (assets/water_bodies.py, water_checks.py)."""
    PACK = Path(__file__).resolve().parent.parent/'assets/maps/dustreach'

    def test_ponds_are_carved_clear_and_mirrored(self):
        from assets import water_checks
        rows = water_checks.assert_ponds(self, self.PACK, 2, 2.6, 3.8)
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
