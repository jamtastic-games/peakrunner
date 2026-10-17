#!/usr/bin/env python3
"""Tests for the original Ozarktic Blast map (docs/ozarktic-blast.md,
docs/map-pipeline.md step 5). Run from src/ with scripts/requirements.txt installed.

Pack-level checks read OZARKTIC_PACK (a built pack directory) when set, else
the embedded assets/maps/ozarktic-blast."""
import hashlib
import importlib.util
import math
import os
from pathlib import Path
import sys
import tempfile
import unittest

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from assets import budgets
from assets import ozarktic_base as base
from assets import ozarktic_materials
from assets import ozarktic_terrain as terrain
from assets import ozarktic_tunnel as tunnel
from assets import sightline_checks


def _load(name, file):
    loader = importlib.util.spec_from_file_location(name, HERE/file)
    module = importlib.util.module_from_spec(loader); loader.loader.exec_module(module)
    return module
kit = _load('kit', 'build-original-map.py')
build = _load('ozarktic_build', 'build-ozarktic-blast.py')
PACK = Path(os.environ.get('OZARKTIC_PACK') or HERE.parent/'assets/maps/ozarktic-blast')
LIFT = base.SPAWN_LIFT


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


def one_base(style, team=0, circuit='one', origin=(0, 0, 0), yaw=0.0):
    mesh = kit.Mesh(); mesh.lamps = []; mesh.origin = origin; mesh.yaw = yaw
    anchors = base.build(mesh, team, circuit, style)
    return mesh, anchors


HX0, HX1, HZ0, HZ1 = base.HALL
IX0, IX1, IZ0, IZ1 = HX0+base.W, HX1-base.W, HZ0+base.W, HZ1-base.W
OX0, OX1, OZ0, OZ1 = base.OUT
OI = (OX0+base.W, OX1-base.W, OZ0+base.W, OZ1-base.W)


class BaseGeometry(unittest.TestCase):
    """Both base styles, built alone at the origin."""
    @classmethod
    def setUpClass(cls):
        cls.built = {s: one_base(s) for s in base.STYLES}
        cls.soups = {s: Soup(m.collision) for s, (m, _) in cls.built.items()}

    def floor_ok(self, soup, x, y, z, headroom=2.6, label=''):
        t, ny = soup.hits((x, y+3, z), (0, -1, 0))
        self.assertTrue(len(t) and abs(3-t[0]) < 1e-3 and ny[0] > .999, (label, x, y, z, 'floor', t[:1]))
        if headroom: self.assertGreater(soup.first((x, y+.2, z), (0, 1, 0)), headroom, (label, x, y, z, 'headroom'))

    def test_floors_have_floor_and_headroom(self):
        for style, soup in self.soups.items():
            hall = [(x, 0.0, z) for x in (-8, -2, 4, 12) for z in (-5, 2, 10)]+[(-13, 0.0, -4), (-13, 0.0, 12)]
            gen = [(x, base.GEN_FLOOR, z) for x in (-8, -2, 10, 13) for z in (-5, 2, 13)]
            tunnel = [(x, base.GEN_FLOOR, 4.0) for x in (18, 26, 34, 39)]
            out = [(x, base.GEN_FLOOR, z) for x in (42, 48) for z in (0, 8)]
            ledge = [(x, 0.0, -4.0) for x in (42, 46, 50, 53)]
            deck = [(x, base.DECK_Y, z) for x in (-12, 0, 12) for z in (-40, -30, -20)]
            roof = [(x, base.ROOF, z) for x in (-12, 0) for z in (-4, 12)]
            for label, pts, room in (('hall', hall, 2.8), ('gen', gen, 6.5), ('tunnel', tunnel, 4.0), ('out', out, 2.8),
                                     ('ledge', ledge, 2.8 if style == 'bluff' else None), ('deck', deck, None), ('roof', roof, None)):
                for x, y, z in pts: self.floor_ok(soup, x, y, z, room or 0, (style, label))
            # Open to the sky: the deck, the roof and the crater.
            for x, y, z in deck+roof+(out if style == 'hollow' else []):
                self.assertEqual(soup.first((x, y+.2, z), (0, 1, 0)), np.inf, (style, 'sky', x, z))

    def test_ramps_climb_with_headroom_and_closed_undersides(self):
        ramps = [(base.STAIR, 0.0, base.GEN_FLOOR, base.GEN_FLOOR), (base.OUT_RAMP, 0.0, base.GEN_FLOOR, base.GEN_FLOOR),
                 (base.DECK_RAMP, 0.0, base.DECK_Y, 0.0), (base.ROOF_RAMP, 0.0, base.ROOF, 0.0)]
        for style, soup in self.soups.items():
            for ramp, ya, yb, bottom in ramps:
                x0, x1, za, zb = ramp
                self.assertLessEqual(math.degrees(math.atan(abs(yb-ya)/abs(zb-za))), 30.0, ramp)
                lo, hi = sorted((za, zb))
                for z in np.arange(lo+.4, hi-.3, 1.0):
                    y = base.ramp_y(ramp, z, ya, yb)
                    for x in (x0+.8, (x0+x1)/2, x1-.8):
                        t, ny = soup.hits((x, y+1, z), (0, -1, 0))
                        self.assertLess(abs(1-t[0]), .03, (style, ramp, x, z))
                        self.assertGreater(abs(ny[0]), .85, (style, ramp, z))
                        self.assertGreater(soup.first((x, y+.2, z), (0, 1, 0)), 2.8, (style, ramp, z, 'headroom'))
                    # Nothing can be walked into underneath either edge.
                    if y-bottom > 2.0:
                        for x in (x0-.4, x1+.4):
                            if IX0 < x < IX1 or OI[0] < x < OI[1] or x < HX0 or x > HX1:
                                d = soup.first((x, bottom+1.0, z), (1 if x < x0 else -1, 0, 0), 1.0)
                                self.assertLess(d, 1.0, (style, ramp, x, z, 'open underside'))
            # The bridge from the roof falls gently onto the deck.
            for z in np.arange(base.DECK[3]+.4, base.HALL[2]-.3, 1.0):
                t, _ = soup.hits((0.0, base.bridge_y(z)+1, z), (0, -1, 0))
                self.assertLess(abs(1-t[0]), .03, (style, 'bridge', z))

    def test_stair_opening_has_headroom_and_rails(self):
        for style, soup in self.soups.items():
            x = sum(base.STAIR[:2])/2
            # The hall floor is open over the upper stair and closed over the lower.
            self.assertEqual(soup.first((x, .5, 2.0), (0, -1, 0), .4), np.inf)
            self.assertLess(soup.first((x, .5, base.STAIR_HOLE_Z+2), (0, -1, 0), .6), .6)
            # A rail stops a walk off the hall floor into the stair well.
            self.assertLess(soup.first((base.STAIR[1]+1.0, .6, 3.0), (-1, 0, 0), 1.5), 1.5)

    def test_generator_room_is_seven_metres_with_the_stair_against_the_wall(self):
        for style, soup in self.soups.items():
            gx, gy, gz = base.GEN
            self.assertGreaterEqual(soup.first((gx+4, gy+.1, gz), (0, 1, 0))+.1, 7.0)
            self.assertEqual(base.STAIR[0], IX0)

    def test_spawns_stand_on_the_hall_floor_away_from_the_stair(self):
        for style, (mesh, anchors) in self.built.items():
            soup = self.soups[style]
            top = np.array(anchors['stair_top'])
            for x, y, z, yaw in anchors['spawn_points']:
                self.assertAlmostEqual(y, LIFT)
                self.floor_ok(soup, x, 0.0, z, 2.8, (style, 'spawn'))
                self.assertGreater(math.hypot(x-top[0], z-top[2]), 8.0, (style, x, z, 'camps the stair'))
                self.assertEqual(soup.first((x, y, z), (0, 0, -1), 6.0), np.inf, (style, x, z, 'faces a wall'))
            self.assertEqual(len(anchors['spawn_points']), 8)

    def test_flag_deck_is_open_and_the_flag_sits_on_it(self):
        for style, soup in self.soups.items():
            fx, fy, fz = base.FLAG
            self.floor_ok(soup, fx, fy, fz, None, (style, 'flag'))
            self.assertEqual(soup.first((fx, fy+.2, fz), (0, 1, 0)), np.inf)

    def test_holes_are_on_the_grid_and_every_cell_has_a_floor(self):
        for style, soup in self.soups.items():
            for name, (x0, x1, z0, z1) in base.HOLES.items():
                for v in (x0, x1, z0, z1): self.assertEqual(v % 8, 0, (name, v))
                for cx in np.arange(x0+4, x1, 8):
                    for cz in np.arange(z0+4, z1, 8):
                        t, ny = soup.hits((cx, 10.0, cz), (0, -1, 0))
                        self.assertTrue(len(t) and 10-t[-1] <= base.GEN_FLOOR+.01, (style, name, cx, cz, 'no floor below'))
                        # The cut is roofed (hall, tunnel lid, bunker) or an open walled shaft.
                        covered = 10-t[0] >= -.2
                        shaft = style == 'hollow' and name == 'outbuilding'
                        self.assertTrue(covered or shaft, (style, name, cx, cz))

    def test_transform_invariance_and_budget(self):
        for style in base.STYLES:
            a, _ = one_base(style)
            b, _ = one_base(style, origin=(1024, 200, 648), yaw=math.pi)
            self.assertEqual(len(a.collision), len(b.collision))
            self.assertEqual(len(a.entities), len(b.entities))
            m = kit.Mesh(); m.lamps = []
            base.build(m, 0, 'x', style); base.build_spire(m, 0, 'x')
            self.assertLessEqual(len(m.collision)//9, budgets.COLLISION_TRIS_PER_BASE, style)
            kinds = sorted(e['kind'] for e in m.entities)
            self.assertEqual(kinds, ['generator', 'inventory', 'inventory', 'inventory', 'sensor', 'turret', 'turret'])

    def test_two_instances_have_unique_ids_and_independent_circuits(self):
        m = kit.Mesh(); m.lamps = []
        base.build(m, 0, 'base-0', 'bluff'); base.build_spire(m, 0, 'base-0')
        base.build(m, 1, 'base-1', 'hollow'); base.build_spire(m, 1, 'base-1')
        ids = [e['id'] for e in m.entities]
        self.assertEqual(len(ids), len(set(ids)))
        for e in m.entities: self.assertEqual(e['circuit'], f'base-{e["team"]}')

    def test_styles_share_rooms_and_entrances(self):
        a, b = (self.built[s][0] for s in base.STYLES)
        self.assertEqual(sorted(e['kind'] for e in a.entities), sorted(e['kind'] for e in b.entities))
        self.assertNotEqual(len(a.collision), len(b.collision))   # they do differ: bunker vs crater

    def test_materials_are_deterministic_and_distinct(self):
        images = {}
        for name in ('meadow', 'rock', 'soil', 'concrete', 'ember', 'glacier'):
            a = ozarktic_materials.texture(name, 7, kit.noise, kit.value_noise)
            self.assertEqual(a, ozarktic_materials.texture(name, 7, kit.noise, kit.value_noise))
            self.assertEqual(len(a), 256*256*4)
            self.assertTrue(all(a[i] == 255 for i in range(3, len(a), 4*97)))
            images[name] = a
        self.assertEqual(len({hashlib.sha256(v).hexdigest() for v in images.values()}), len(images))

    def test_lightmap_bake_is_deterministic(self):
        from assets import lightmap_bake
        m = kit.Mesh(); m.lamps = []; base.build_spire(m, 0, 'x')
        verts = np.array(m.vertices, np.float32).reshape(-1, 12)
        sun = (-.57735, .57735, -.57735); light = kit.MATERIALS.index('light')
        a, pa = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=1)
        b, pb = lightmap_bake.bake(verts, m.lamps, sun, light, 20, processes=3)
        self.assertTrue(np.array_equal(a, b)); self.assertEqual(pa, pb)


class WholeMap(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.spec = build.spec()
        cls.grid = build.terrain_grid(cls.spec)
        mesh = kit.Mesh(); mesh.lamps = []
        cls.flags, _, cls.spawn_points, cls.instances, cls.per_base = build.build_structures(cls.spec, mesh)
        cls.mesh = mesh
        cls.soup = Soup(mesh.collision)

    def h(self, x, z):
        return build.terrain_height(x, z, self.grid)

    def test_terrain_shape_and_layout(self):
        """The layout idea (a ridge astride the flag line, high west, low
        east with the docks, flags on low ground with a rise behind) at our
        own scale; heights and shapes are original (docs/ozarktic-blast.md)."""
        (ex, ey, ez), (gx, gy, gz) = self.flags
        self.assertLess(abs(ex-gx), 1.0)
        self.assertLess(abs(ey-gy), 3.0, 'fair: the flags stand level')
        fg = (self.h(ex, ez)+self.h(gx, gz))/2
        c = terrain.CENTRE
        ridge = self.h(terrain.FLAG_X-10, c)
        self.assertTrue(70 < ridge-fg < 110, ('ridge over the flags', ridge-fg))
        # A steep east face and a low east valley holding the docks.
        self.assertGreater(ridge-self.h(terrain.FLAG_X+60, c), 50)
        for x, y, z, _, _ in build.dock_sites(self.spec):
            self.assertLess(y, fg, 'the docks sit low')
        # High west upland on both halves.
        for z in (c-250, c, c+250):
            self.assertGreater(self.h(terrain.FLAG_X-230, z), fg+40, z)
        # The ground keeps rising behind each base (whose level site already
        # sits partway up that rise).
        self.assertGreater(self.h(terrain.FLAG_X, c-terrain.FLAG_DZ-100), fg+5)
        self.assertGreater(self.h(terrain.FLAG_X, c+terrain.FLAG_DZ+100), fg+5)
        # Detail isn't mirrored: the halves differ in the small.
        g = self.grid[40:216, 40:216]
        self.assertGreater(np.abs(g-g[::-1, :]).mean(), 2.0)
        # Capture points on the centre line, fair to both flags.
        for p in self.spec['control_points']:
            d = [math.hypot(p['x']-f[0], p['z']-f[2]) for f in self.flags]
            self.assertLess(abs(d[0]-d[1])/max(d), .02, (p['id'], d))
        flag_distance = math.hypot(gx-ex, gz-ez)
        self.assertTrue(580 < flag_distance < 700, flag_distance)
        s = terrain.slope_degrees(self.grid)[40:216, 40:216]
        self.assertTrue(11 < np.median(s) < 18 and 24 < np.percentile(s, 90) < 36 and .05 < (s > 30).mean() < .16,
                        (np.median(s), np.percentile(s, 90), (s > 30).mean()))

    def test_ships_hover_over_their_docks_with_hatch_only_hideouts(self):
        from assets import ozarktic_landmarks as lm
        for (dx, dy, dz, dyaw, _), (x, y, z, yaw) in zip(build.dock_sites(self.spec), build.ship_sites(self.spec)):
            self.assertAlmostEqual(y-dy, lm.HOVER)
            # The basin is cut and its floor reachable under the ship.
            self.assertLess(self.h(dx, dz), dy-lm.BASIN[2]+.6)
            m = kit.Mesh(); m.lamps = []; m.origin = (x, y, z); m.yaw = yaw
            anchors = lm.build_ship(m)
            hatches = [m.point(h) for h in anchors['hatches']]
            soup = Soup(m.collision)
            for r in (1.5, 6.0, 10.0):
                for a in np.linspace(0, math.tau, 7, endpoint=False):
                    p = m.point((r*math.cos(a+.1), 0, r*math.sin(a+.1)))
                    if any(math.hypot(p[0]-hx, p[2]-hz) < 3.5 for hx, _, hz in hatches): continue
                    t, ny = soup.hits((p[0], p[1]+3, p[2]), (0, -1, 0))
                    self.assertTrue(len(t) and abs(3-t[0]) < 1e-3 and abs(ny[0]) > .999, ('floor', r, a))
                    self.assertGreater(soup.first((p[0], p[1]+.2, p[2]), (0, 1, 0)), 3.8, ('headroom', r, a))
            for hx, hy, hz in hatches:
                self.assertEqual(soup.first((hx, hy+1.5, hz), (0, -1, 0)), np.inf, ('hatch', hx, hz))
            slits = 0
            for i in range(8):
                a = math.pi/8+(i+.5)*math.tau/8
                o = m.point((0, 1.5, 0))
                d = np.array(m.point((math.cos(a), 0, math.sin(a))))-np.array(m.point((0, 0, 0)))
                slits += soup.first(o, d/np.linalg.norm(d), 20) == np.inf
            self.assertEqual(slits, len(lm.SLIT_OCTANTS))

    def test_dock_gantries_stand_on_the_rim(self):
        from assets import ozarktic_landmarks as lm
        for x, y, z, yaw, _ in build.dock_sites(self.spec):
            m = kit.Mesh(); m.origin = (x, y, z); m.yaw = yaw
            lm.build_dock(m)
            for gz in lm.GANTRY_Z:
                for side in (-1, 1):
                    foot = m.point((side*lm.GANTRY_X, 0, gz))
                    self.assertLess(abs(self.h(foot[0], foot[2])-(y-.1)), .6, (x, z, side, gz))

    def test_outposts_have_an_open_door_and_two_inventories(self):
        from assets import ozarktic_landmarks as lm
        for team, x, y, z, yaw in build.outpost_sites(self.spec):
            m = kit.Mesh(); m.lamps = []; m.origin = (x, y, z); m.yaw = yaw
            anchors = lm.build_outpost(m, team, 'c')
            soup = Soup(m.collision)
            self.assertEqual(sum(e['kind'] == 'inventory' for e in m.entities), 2)
            door = m.point((0, 1.2, -lm.OUT_D-1.0)); inside = m.point((0, 1.2, 0))
            d = np.array(inside)-np.array(door)
            self.assertEqual(soup.first(door, d/np.linalg.norm(d), np.linalg.norm(d)), np.inf, 'the door is open')
            self.assertGreater(soup.first(m.point((0, .2, 0)), (0, 1, 0)), 4.5)
            self.assertLess(abs(self.h(x, z)-(y-.1)), .5)
        per_team = {0: 0, 1: 0}
        for e in self.mesh.entities:
            if e['kind'] == 'inventory': per_team[e['team']] += 1
        self.assertEqual(per_team, {0: 5, 1: 5})

    def test_perch_platform_is_walled_and_its_ramp_climbs(self):
        from assets import ozarktic_landmarks as lm
        x, y, z, yaw = build.perch_site(self.spec)
        m = kit.Mesh(); m.lamps = []; m.origin = (x, y, z); m.yaw = yaw
        lm.build_perch(m)
        soup = Soup(m.collision)
        for lx, lz in ((0, 0), (-3, -3), (3, 3)):
            p = m.point((lx, lm.PERCH_H, lz))
            t, ny = soup.hits((p[0], p[1]+3, p[2]), (0, -1, 0))
            self.assertTrue(len(t) and abs(3-t[0]) < 1e-3, (lx, lz))
        x0, x1, zg, zt = lm.PERCH_RAMP
        self.assertLessEqual(math.degrees(math.atan(lm.PERCH_H/(zg-zt))), 30)
        for lz in np.arange(zt+.5, zg-.5, 2.0):
            ly = lm.PERCH_H*(zg-lz)/(zg-zt)
            p = m.point(((x0+x1)/2, ly, lz))
            self.assertLess(abs(soup.first((p[0], p[1]+1, p[2]), (0, -1, 0))-1), .05, lz)
        # Crouch cover on the field side stops a level shot.
        o = m.point((0, lm.PERCH_H+.9, 0)); f = np.array(m.point((0, lm.PERCH_H+.9, -10)))-np.array(o)
        self.assertLess(soup.first(o, f/np.linalg.norm(f), 10), 6)
        # The ground stays under its ramp foot.
        foot = m.point(((x0+x1)/2, 0, zg-1))
        self.assertLess(abs(self.h(foot[0], foot[2])-(y-.1)), .4)

    def test_ground_stays_under_the_bases(self):
        for b in self.spec['bases']:
            oy = b['position'][1]
            for lx in np.arange(HX0, OX1+.1, 4.0):
                for lz in np.arange(HZ0, HZ1+.1, 4.0):
                    x, z = build.to_world(b, lx, lz)
                    self.assertLess(abs(self.h(x, z)-(oy-.1)), .35, (b['team'], lx, lz, self.h(x, z)-oy))

    def test_spires_stand_on_their_ground(self):
        for team, x, y, z in build.spires(self.spec):
            self.assertLess(abs(self.h(x, z)-(y-.1)), .3, (team, self.h(x, z), y))

    def test_holes_are_under_structures(self):
        """Every hole is under a base, an outpost tunnel's lid or its stair
        house; each lid's corners keep the terrain's own heights, pinned."""
        cells = set(build.holes(self.spec))
        under_bases = 2*sum((x1-x0)*(z1-z0)//64 for x0, x1, z0, z1 in base.HOLES.values())
        profiles = build.tunnel_profiles(self.spec, terrain.heights(self.spec['seed'], build.terrain_sites(self.spec)))
        tunnels = {c[1]*256+c[0] for p in profiles.values() for c in p['cells']+p['stair']}
        self.assertEqual(len(cells), under_bases+len(tunnels))
        self.assertTrue(tunnels <= cells)
        for p in profiles.values():
            self.assertEqual(p['raised'], 0.0, 'the lids follow the land')
            for (ix, iz), y in p['pins'].items():
                self.assertEqual(float(self.grid[iz, ix]), y, (ix, iz))

    def test_outpost_tunnels_run_floored_lit_and_covered(self):
        """Down each tunnel's middle, from the base's short tunnel to the stair
        foot: a floor under foot, the full headroom over it, never steeper
        than the limit, never climbing toward the base, and ground over it."""
        profiles = build.tunnel_profiles(self.spec, terrain.heights(self.spec['seed'], build.terrain_sites(self.spec)))
        k = math.tan(math.radians(tunnel.SLOPE_DEG))
        for team, p in profiles.items():
            f, s, edges = p['floor'], p['s'], p['edges']
            for i in range(len(f)-1):
                if s[i+1] > s[i]: self.assertLessEqual(abs(f[i+1]-f[i]), k*(s[i+1]-s[i])+1e-6, (team, i))
                if i: self.assertGreaterEqual(f[i+1], f[i]-1e-9, (team, i, 'climbs toward the outpost all the way'))
            for i, c in enumerate(p['cells']):
                (ax, az), (bx, bz) = edges[i]; (cx, cz), (dx, dz) = edges[i+1]
                a = ((ax+bx)*4.0, (az+bz)*4.0); b2 = ((cx+dx)*4.0, (cz+dz)*4.0)
                for t in np.linspace(.1, .9, 5):
                    x, z = a[0]+(b2[0]-a[0])*t, a[1]+(b2[1]-a[1])*t
                    fy = f[i]+(f[i+1]-f[i])*t
                    down = self.soup.first((x, fy+1.5, z), (0, -1, 0), 3.0)
                    self.assertLess(abs(down-1.5), .05, (team, c, t, down))
                    self.assertGreater(self.soup.first((x, fy+.1, z), (0, 1, 0)), tunnel.H-.3, (team, c, t))
                    self.assertGreater(self.h(x, z)-(fy+tunnel.H), tunnel.ROOF, (team, c, t))
            # Lamps all the way.
            lamps = [l for l in self.mesh.lamps if any(abs(l[0][0]-(cx+.5)*8) < 4.5 and abs(l[0][2]-(cz+.5)*8) < 4.5 for cx, cz in p['cells'])]
            self.assertGreaterEqual(len(lamps), len(p['cells'])//2)

    def test_outpost_tunnels_join_the_short_tunnel_and_reach_their_outposts(self):
        for b in self.spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            # Through the short tunnel's back-wall door, along the floor.
            x = sum(base.BRANCH_DOOR)/2
            start = m.point((x, base.GEN_FLOOR+1.2, base.TUN_Z[0]+3.0)); end = m.point((x, base.GEN_FLOOR+1.2, base.TUN_Z[1]+6.0))
            d = np.array(end)-np.array(start)
            self.assertEqual(self.soup.first(start, d/np.linalg.norm(d), np.linalg.norm(d)), np.inf, (b['team'], 'the door is open'))
        sites = {o[0]: o for o in build.outpost_sites(self.spec)}
        for team, inst in ((i['team'], i) for i in self.instances if i['asset'] == tunnel.ASSET_ID):
            door, foot = np.array(inst['anchors']['stair_door']), np.array(inst['anchors']['stair_foot'])
            # The stair house: open at its end, a ramp no steeper than 30 deg
            # up from the tunnel, and the outpost a short walk away.
            out = door+np.array([0, 1.5, 0])+np.sign(door[2]-foot[2])*np.array([0, 0, 2.0])
            self.assertEqual(self.soup.first(door+np.array([0, 1.5, 0]), (0, 0, np.sign(door[2]-foot[2])), 2.0), np.inf, team)
            self.assertLessEqual(math.degrees(math.atan2(door[1]-foot[1], abs(door[2]-foot[2])-3.0)), 30.0, team)
            self.assertLess(abs(self.h(*out[[0, 2]])-door[1]), .3, (team, 'level ground at the door'))
            _, ox, oy, oz, _ = sites[team]
            self.assertLess(math.hypot(door[0]-ox, door[2]-oz), 25.0, team)

    def test_turrets_cannot_see_into_rooms(self):
        """No turret engages a standable point in the hall, generator room,
        tunnel or bunker beyond the doorway depth of an open door. The crater
        (hollow style) is open to the sky and so exempt, like any open ground."""
        rooms = [('hall', IX0, IX1, IZ0, IZ1, 0.0), ('generator', IX0, IX1, IZ0, IZ1, base.GEN_FLOOR),
                 ('tunnel', HX1+.5, OX0-.5, base.TUN_Z[0]+base.W, base.TUN_Z[1]-base.W, base.GEN_FLOOR)]
        bunker = [('bunker', OI[0], OI[1], OI[2], OI[3], base.GEN_FLOOR), ('bunker ledge', OI[0], OI[1], OI[2], base.LEDGE_Z, 0.0)]
        targets, openings, counts = [], [], {}
        for b in self.spec['bases']:
            m = kit.Mesh(); m.origin = tuple(b['position']); m.yaw = math.radians(b['yaw'])
            pt = lambda x, z: tuple(m.point((x, 0, z))[i] for i in (0, 2))
            openings += [(pt(base.DOOR_FRONT[0], IZ0), pt(base.DOOR_FRONT[1], IZ0)),
                         (pt(base.DOOR_REAR[0], IZ1), pt(base.DOOR_REAR[1], IZ1)),
                         (pt(IX0, base.DOOR_WEST[0]), pt(IX0, base.DOOR_WEST[1])),
                         (pt(base.OUT_DOOR[0], OI[2]), pt(base.OUT_DOOR[1], OI[2]))]
            for name, x0, x1, z0, z1, floor in rooms+(bunker if b['style'] == 'bluff' else []):
                for x in np.arange(x0+.5, x1, 1.0):
                    for z in np.arange(z0+.5, z1, 1.0):
                        if self.soup.first(m.point((x, floor+.1, z)), (0, 1, 0), 2.3) < 2.3: continue
                        t, _ = self.soup.hits(m.point((x, floor+1, z)), (0, -1, 0))
                        if not len(t) or abs(t[0]-1) > .02: continue
                        targets.append(m.point((x, floor+LIFT+.8, z))); counts[name] = counts.get(name, 0)+1
        from assets import ozarktic_landmarks as lm
        for team, x, y, z, yaw in build.outpost_sites(self.spec):
            m = kit.Mesh(); m.origin = (x, y, z); m.yaw = yaw
            pt = lambda lx, lz: tuple(m.point((lx, 0, lz))[i] for i in (0, 2))
            openings.append((pt(lm.OUT_DOOR[0], -lm.OUT_D+.7), pt(lm.OUT_DOOR[1], -lm.OUT_D+.7)))
            for lx in np.arange(-lm.OUT_W+1.2, lm.OUT_W-1.0, 1.0):
                for lz in np.arange(-lm.OUT_D+1.2, lm.OUT_D-1.0, 1.0):
                    if self.soup.first(m.point((lx, .1, lz)), (0, 1, 0), 2.3) < 2.3: continue
                    targets.append(m.point((lx, LIFT+.8, lz))); counts['outpost'] = counts.get('outpost', 0)+1
        # Down the outpost tunnels too.
        profiles = build.tunnel_profiles(self.spec, terrain.heights(self.spec['seed'], build.terrain_sites(self.spec)))
        for p in profiles.values():
            for i, (ix, iz) in enumerate(p['cells']):
                y = (p['floor'][i]+p['floor'][i+1])/2
                targets.append(((ix+.5)*8, y+LIFT+.8, (iz+.5)*8)); counts['outpost tunnel'] = counts.get('outpost tunnel', 0)+1
        targets = np.array(targets)
        from assets import turret_arcs
        entities = turret_arcs.assign([dict(e) for e in self.mesh.entities], self.flags)
        turrets = [e for e in entities if e['kind'] == 'turret']
        self.assertEqual(len(turrets), 4)
        allowed = sightline_checks.near_openings(targets, openings)
        for e in turrets:
            seen = sightline_checks.visible(self.soup, e, targets) & ~allowed
            self.assertEqual(int(seen.sum()), 0, (e['id'], targets[seen][:5]))
        for name in ('hall', 'generator', 'tunnel', 'bunker'): self.assertGreater(counts.get(name, 0), 40, counts)
        self.assertGreater(counts.get('outpost', 0), 20, counts)

    def test_budget_per_base(self):
        for n in self.per_base: self.assertLessEqual(n, budgets.COLLISION_TRIS_PER_BASE)

    def test_unbaked_rebuilds_are_byte_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            digests = []
            for name in ('a', 'b'):
                out = Path(tmp)/name
                build.build(out, bake=False)
                digests.append({f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in out.iterdir()})
            self.assertEqual(digests[0], digests[1])
            self.assertEqual(len(digests[0]), 9)   # map.json, six payloads, shade.rg, props.bin


@unittest.skipUnless((PACK/'map.json').exists(), f'no built pack at {PACK}')
class BuiltPack(unittest.TestCase):
    """Checks on a built pack: routes, spawns, props and surfaces."""
    def test_rooms_have_the_right_number_of_ways_in(self):
        import json
        from assets import route_checks
        manifest = json.loads((PACK/'map.json').read_text())
        pack = route_checks.Pack(PACK)
        spec = build.spec()
        for b in spec['bases']:
            oy = b['position'][1]
            def box(lx0, lx1, ly0, ly1, lz0, lz1):
                (ax, az), (bx, bz) = build.to_world(b, lx0, lz0), build.to_world(b, lx1, lz1)
                return (min(ax, bx), max(ax, bx), oy+ly0, oy+ly1, min(az, bz), max(az, bz))
            cx, cz = build.to_world(b, 8.0, 0.0)
            found = route_checks.base_entries(pack, (cx, cz), {
                'hall': box(IX0, IX1, -.5, 1.5, IZ0, IZ1),
                'deck': box(*base.DECK[:2], base.DECK_Y-.5, base.DECK_Y+1.5, *base.DECK[2:]),
                'generator': box(IX0, IX1, base.GEN_FLOOR-.5, base.GEN_FLOOR+1.5, IZ0, IZ1)}, extent=90.0)
            self.assertGreaterEqual(len(found['hall']), 3, (b['team'], found['hall']))
            self.assertGreaterEqual(len(found['deck']), 2, (b['team'], found['deck']))
            self.assertEqual(len(found['generator']), 2, (b['team'], found['generator']))
        self.assertEqual([p['id'] for p in manifest['control_points']], ['ridge-top', 'dock-yard'])
        self.assertIn('sky', manifest['look'])

    def test_spawns_face_open_floor_and_hide_from_the_field(self):
        from assets import spawn_checks
        self.assertEqual(spawn_checks.problems(PACK), [])
        self.assertEqual(spawn_checks.exposed(PACK), [])

    def test_props_and_terrain_shade(self):
        from assets import prop_checks
        prop_checks.check_pack(self, PACK)

    def test_no_z_fighting(self):
        from assets import surface_checks
        verts = np.frombuffer((PACK/'vertices.bin').read_bytes(), '<f4').reshape(-1, 12)
        self.assertEqual(surface_checks.z_fighting(verts), [])


if __name__ == '__main__':
    unittest.main()
