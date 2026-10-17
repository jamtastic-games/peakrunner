#!/usr/bin/env python3
"""Tests for the original Reefbreak map (docs/reefbreak.md,
docs/map-pipeline.md step 5). Run from src/ with scripts/requirements.txt installed.

Pack-level checks read REEFBREAK_PACK (a built pack directory) when set,
else the embedded assets/maps/reefbreak."""
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import unittest

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from assets import fairness
from assets import reefbreak_base as base
from assets import reefbreak_terrain as terrain


def _load(name, file):
    loader = importlib.util.spec_from_file_location(name, HERE/file)
    module = importlib.util.module_from_spec(loader); loader.loader.exec_module(module)
    return module
build = _load('reefbreak_build', 'build-reefbreak.py')
test_oz = _load('test_oz', 'test-ozarktic-blast.py')
Soup = test_oz.Soup
PACK = Path(os.environ.get('REEFBREAK_PACK') or HERE.parent/'assets/maps/reefbreak')


class Terrain(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.definition = build.spec()
        cls.grid = build.terrain_grid(cls.definition)

    def test_halves_are_their_own_and_alike(self):
        """The atoll's frame is shared; the outer islands, blue holes, reef
        lumps and detail are each half's own, and the halves measure alike:
        as much dry land, shallows and deep water, and the same ground."""
        self.assertFalse(fairness.symmetric(self.grid), 'each half its own')
        z, x = np.mgrid[0:256, 0:256]*8.0
        play = np.hypot(x-1024, z-1024) < 650
        share = lambda mask, half: float(mask[play & half].mean())
        wet = self.grid < terrain.SEA
        for name, mask, tol in (('land', ~wet, .02), ('shallow', wet & (self.grid > terrain.SEA-.6), .03),
                                ('deep', self.grid < terrain.SEA-5, .01)):
            a, b = share(mask, z < 1024), share(mask, z > 1024)
            self.assertLess(abs(a-b), tol, (name, a, b))
        flags = build.build_structures(self.definition, build.kit.Mesh())[0]
        for row in fairness.assert_fair(self, self.grid, flags): print('half', {k: round(v, 2) for k, v in row.items()})

    def test_shallows_are_wadeable_and_there_is_land(self):
        z, x = np.mgrid[0:256, 0:256]*8.0
        r = np.hypot(x-1024, z-1024); play = r < 650
        wet = self.grid < terrain.SEA
        shallow = wet & (self.grid > terrain.SEA-.6)
        self.assertGreater(shallow[play].mean(), .4, 'most water in play is ankle deep')
        self.assertGreater((~wet)[play].mean(), .2, 'enough dry land')
        self.assertTrue((self.grid < terrain.SEA-5)[play].any(), 'deep blue holes exist')

    def test_lagoon_channels_are_off_the_flag_axis(self):
        for c in terrain.CHANNELS:
            self.assertGreater(min(abs(c), abs(180-c)), 30)


class Pack(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.m = json.loads((PACK/'map.json').read_text())
        cls.soup = Soup(np.frombuffer((PACK/'collision.bin').read_bytes(), '<f4'))

    def test_flags_stand_on_the_ships_entry_platforms(self):
        """Each flag stands on the flat floor of its ship's entry platform,
        outside the bay's port opening (facing the field), high over the
        sea, with the bay open from flank to flank in line with it and open
        sky above it."""
        a, b = (np.array(f) for f in self.m['flags'])
        # 720 m between the ships' bays; each flag stands 10.5 m out toward the field.
        self.assertAlmostEqual(np.linalg.norm(a[[0, 2]]-b[[0, 2]]), 699.0, delta=4.0)
        for f, bdef in zip((a, b), build.spec()['bases']):
            self.assertGreater(f[1]-terrain.SEA, base.HOVER-.5, 'up in the ship, over the sea')
            t = self.soup.first(f+np.array([0, 1.0, 0]), np.array([0.0, -1.0, 0.0]), 3.0)
            self.assertLess(abs(t-1.0), .05, 'on the platform')
            m = build.kit.Mesh(); m.origin = tuple(bdef['position']); m.yaw = math.radians(bdef['yaw'])
            for y in (2.0, 3.5):
                ends = [np.array(m.point(base.to_base((s*(base.PLATFORM+2.0), y, base.FLAG_HULL[2])))) for s in (-1, 1)]
                d = ends[1]-ends[0]
                self.assertEqual(self.soup.first(ends[0], d/np.linalg.norm(d), np.linalg.norm(d)), np.inf, f'open through the bay at {y} m')
            # The port opening faces the field (the map's middle), and the
            # flag stands outside it, nearer the field still.
            port, starboard = (np.array(m.point(base.to_base((s*base.BAY_X, 0, 0)))) for s in (-1, 1))
            self.assertLess(abs(port[2]-1024), abs(starboard[2]-1024))
            self.assertLess(abs(f[2]-1024), abs(port[2]-1024))
            # Open sky above the flag.
            self.assertEqual(self.soup.first(f+np.array([0, .1, 0]), np.array([0.0, 1.0, 0.0])), np.inf)

    def test_the_ships_float_with_room_to_ski_beneath(self):
        for b in build.spec()['bases']:
            self.assertGreater(b['position'][1]+base.KEEL-terrain.SEA, 8.0)
            # Straight down from under the hull: open air to the sea.
            o = np.array(b['position'])+np.array([4.0, base.KEEL-.5, 4.0])
            t = self.soup.first(o, np.array([0.0, -1.0, 0.0]), 40.0)
            self.assertTrue(math.isinf(t) or o[1]-t < terrain.SEA, 'nothing solid under the ship above the sea')

    def test_the_ships_are_drawn_from_the_maps_model(self):
        """Both ships place the map's authored model, painted per team; the
        geometry standing in for it is one whole run of the vertices."""
        from assets import map_models
        placed = self.m['models']
        self.assertEqual(sorted(p['team'] for p in placed), [0, 1])
        for p, bdef in zip(placed, build.spec()['bases']):
            self.assertEqual(p['model'], base.MODEL_NAME)
            self.assertEqual(p['position'], bdef['position'])
            self.assertEqual(p['yaw'], bdef['yaw'])
        model = map_models.read(PACK/'models.glb')
        self.assertGreater(len(model.tri), 10000, "the authored model, not a stand-in")
        first, count = self.m['standin']
        verts = np.frombuffer((PACK/'vertices.bin').read_bytes(), '<f4').reshape(-1, 12)
        self.assertEqual(first % 3, 0); self.assertEqual(count % 3, 0)
        self.assertLessEqual(first+count, len(verts))
        run = verts[first:first+count, :3]
        ships = np.array([b['position'] for b in build.spec()['bases']])
        near = np.min(np.linalg.norm(run[:, None, [0, 2]]-ships[None, :, [0, 2]], axis=2), 1)
        self.assertLess(near.max(), 70.0, 'the stand-in is the ships and their gangways')

    def test_every_spawn_stands_over_a_floor(self):
        for team in self.m['spawn_points']:
            self.assertEqual(len(team), len(base.SPAWNS)+len(base.OUT_SPAWNS))
            for x, y, z, _ in team:
                t = self.soup.first(np.array([x, y, z]), np.array([0.0, -1.0, 0.0]), 3.0)
                self.assertAlmostEqual(t, base.SPAWN_LIFT, delta=.05, msg=f'spawn at {x:.1f},{y:.1f},{z:.1f}')

    def test_rooms_have_their_ways_in(self):
        """Top level: the hold (spawns) opens on the bay and the deck hatch,
        and drops through its floor hatch to the station room. Middle: the
        station room, under the hold, opens on the balcony and to port (the
        field); a porthole over the balcony goes up into the bay. Bottom: the
        generator room, in the keel aft of the bay, is entered only from the
        back, at its aft gallery (the aft room, where the low-gravity column
        comes up, and the starboard door). The bay's aft wall is solid, and
        nothing opens from the balcony into the keel."""
        self.assertLess(base.HOLD_Z[1], base.BAY_Z[0])
        h0, h1, hz0, hz1 = base.HATCH
        self.assertTrue(base.HOLD_Z[0] < hz0 < hz1 < base.HOLD_Z[1], 'the deck hatch opens into the hold')
        f0, f1, fz0, fz1 = base.HOLD_HATCH
        sz0, sz1 = base.STATION_Z
        self.assertTrue(base.HOLD_Z[0] < sz0 < fz0 < fz1 < sz1 < base.HOLD_Z[1], 'the floor hatch drops into the station room')
        self.assertTrue(-base.STATION_X < f0 < f1 < base.STATION_X)
        self.assertTrue(sz0 < base.PORT_DOOR[0] < base.PORT_DOOR[1] < sz1)
        self.assertLess(base.to_base((-base.STATION_X, 0, 0))[2], 0, 'the port door faces the field')
        gz0, gz1 = base.GEN_Z
        self.assertTrue(base.BAY_Z[1] < gz0 < gz1 < base.AFT_ROOM[1])
        s0, s1, _ = base.SIDE_DOOR
        self.assertTrue(base.AFT_GALLERY[0] < s0 < s1 < base.AFT_GALLERY[1], 'the side door opens on the aft gallery')
        self.assertGreater(base.to_base((base.GEN_X, 0, 0))[2], 0, 'starboard, away from the field')
        _, lz0, lz1 = base.LIFT
        self.assertTrue(base.AFT_ROOM[1] < lz0 < lz1 < base.AFT_ROOM[2], 'the lift comes up in the aft room')
        # Nothing opens between the bay and the generator room, nor from the
        # balcony into the keel.
        ox, oz, _ = base.PORTHOLE
        self.assertTrue(base.BALCONY[0] < ox < base.BALCONY[1] and base.BALCONY[2] < oz < base.BALCONY[3])
        for lo, hi in base.openings():
            self.assertFalse(lo[2] < base.BAY_Z[1]+base.T+.1 and hi[2] > base.BAY_Z[1] and hi[1] > 0.1, (lo, hi))
            self.assertFalse(lo[2] < gz0 and hi[2] > base.KEEL_MODULE[1]-.1, (lo, hi))

    def test_the_generator_floor_is_reached_only_by_dropping(self):
        """No opening into the generator room comes down to its floor: its
        doors stand at its aft gallery, at the top level, and the galleries
        end short of the generator."""
        gx, (gz0, gz1), floor = base.GEN_X, base.GEN_Z, base.GEN_FLOOR
        for lo, hi in base.openings():
            touches = lo[0] < gx+base.T and hi[0] > -gx-base.T and lo[2] < gz1+base.T and hi[2] > gz0-base.T
            if touches and hi[1] > floor:
                self.assertGreaterEqual(lo[1], -.1, (lo, hi))
        g = base.GEN
        self.assertTrue(base.FORE_GALLERY[1] < g[2] < base.AFT_GALLERY[0], 'the generator stands clear of both galleries')
        self.assertEqual(g[1], floor)

    def test_low_gravity_columns_rise_from_the_platform_bases_into_the_aft_rooms(self):
        """Each ship's low-gravity column (a gravity zone at a tenth of
        gravity) rises from its platform base's deck to just over the aft
        room's floor, through an open hatch."""
        zones = [z for z in self.m.get('zones', []) if z['kind'] == 'gravity']
        self.assertEqual(len(zones), 2)
        for z, bdef in zip(zones, build.spec()['bases']):
            m = build.kit.Mesh(); m.origin = tuple(bdef['position']); m.yaw = math.radians(bdef['yaw'])
            at, size = np.array(z['at']), np.array(z['size'])
            foot = np.array(m.point(base.to_base((0.0, base.PLATFORM_BASE[3], (base.LIFT[1]+base.LIFT[2])/2))))
            self.assertLess(np.linalg.norm((at-size*[0, 1, 0])-foot), .05, 'from the platform base')
            self.assertAlmostEqual(at[1]+size[1]-bdef['position'][1], base.LIFT_TOP, places=2)
            self.assertEqual(z['gravity'], base.LOW_GRAVITY)
            self.assertLess(z['gravity'], .6, 'low gravity')
            t = self.soup.first(foot+np.array([0, .5, 0]), np.array([0.0, 1.0, 0.0]), 2*size[1]-.6)
            self.assertEqual(t, np.inf, 'an open column up through the hatch')
            t = self.soup.first(foot+np.array([0, .3, 0]), np.array([0.0, -1.0, 0.0]), 1.0)
            self.assertLess(t, .35, 'its foot on the platform base')

    def test_the_porthole_opens_from_the_balcony_into_the_bay(self):
        """Straight up from the balcony through the porthole into the bay is
        open; just beside it, the bay's floor and the hull's bottom stop you."""
        ox, oz, r = base.PORTHOLE
        for bdef in build.spec()['bases']:
            m = build.kit.Mesh(); m.origin = tuple(bdef['position']); m.yaw = math.radians(bdef['yaw'])
            P = lambda x, y, z: np.array(m.point(base.to_base((x, y, z))))
            t = self.soup.first(P(ox, base.MID_FLOOR+.3, oz), np.array([0.0, 1.0, 0.0]), 8.0)
            self.assertGreater(t, 8.0-.01 if math.isinf(t) else 7.9, 'open up into the bay')
            t = self.soup.first(P(ox, base.MID_FLOOR+1.0, oz), np.array([0.0, -1.0, 0.0]), 2.0)
            self.assertLess(abs(t-1.0), .05, 'over the balcony')
            for dx, dz in ((r+.6, 0), (0, r+.6), (-r-.6, 0)):
                t = self.soup.first(P(ox+dx, base.MID_FLOOR+.3, oz+dz), np.array([0.0, 1.0, 0.0]), 8.0)
                self.assertLess(t, 4.0, "the hull's bottom beside it")

    def test_rooms_sit_inside_the_models_skin(self):
        """Every room's lining is inside the authored hull: straight out from
        its outer faces, the model's skin is met within a short way (else the
        lining would show through the hull): the hold's panels, the station
        room's and the generator room's walls and floors, the aft room's
        walls."""
        from assets import map_models
        model = base._source()
        T = base.T
        def check(points, normal, reach):
            for p in points:
                pb = np.array(base.to_base(p)); d = np.array(base.to_base(normal))
                h = map_models.hits(model, pb, d/np.linalg.norm(d))
                self.assertTrue(len(h) and h[0] < reach, (p, normal, h[:2]))
        half = list(base.HOLD_SECTION)
        for (xa, ya), (xb, yb) in zip(half, half[1:]):
            n = np.array([yb-ya, -(xb-xa)]); n /= np.linalg.norm(n)
            if n[0]*(xa+xb)+n[1]*((ya+yb)-5) < 0: n = -n
            for t in (.1, .5, .9):
                x, y = xa+(xb-xa)*t+n[0]*T, ya+(yb-ya)*t+n[1]*T
                for side in (-1, 1):
                    if side < 0 and abs(n[0]) < 1e-9 and x == 0: continue
                    check([(side*x, y, z) for z in np.linspace(base.HOLD_Z[0]+.5, base.HOLD_Z[1]-.5, 7)], (side*n[0], n[1], 0.0), 2.0)
        for hx, (z0, z1), y0, y1, reach in ((base.STATION_X, base.STATION_Z, base.MID_FLOOR, base.STATION_TOP, 2.0),
                                            (base.GEN_X, base.GEN_Z, base.GEN_FLOOR, base.GEN_TOP, 3.5),
                                            (base.AFT_ROOM[0], base.AFT_ROOM[1:3], 0.0, base.AFT_ROOM[3], 4.5)):
            zs, ys = np.linspace(z0+.6, z1-.6, 5), np.linspace(y0+.5, y1-.5, 4)
            for side in (-1, 1):
                check([(side*(hx+T), y, z) for z in zs for y in ys], (side, 0.0, 0.0), reach)
            if y0 < 0:
                check([(x, y0-.45, z) for z in zs for x in (-hx+.5, 0.0, hx-.5)], (0.0, -1.0, 0.0), 2.0)

    def test_hull_collision_keeps_out_of_the_rooms(self):
        """The hull's collision (sampled from the model) never reaches into
        the hold, the engine room or the bay (their open space; the hull's
        ends at the bay sit inside the bay's end walls)."""
        mesh = build.kit.Mesh(); mesh.lamps = []
        bdef = build.spec()['bases'][0]
        mesh.origin = (0.0, 0.0, 0.0); mesh.yaw = 0.0
        base.build(mesh, bdef['team'], 'base-0')
        a, c = mesh.standin[0]
        v = np.array(mesh.vertices[a:c]).reshape(-1, 12)[:, :3]
        hull = base.to_hull(v).reshape(-1, 3, 3)
        for lo, hi in base.interiors():
            lo, hi = np.array(lo)+base.T+.05, np.array(hi)-base.T-.05
            centre = hull.mean(1)
            inside = np.all((centre > lo) & (centre < hi), 1)
            self.assertFalse(inside.any(), (lo, hi, hull[inside][:2]))

    def test_deck_collision_follows_the_models_deck(self):
        """Where players walk on top (the spine forward of the bay and over
        it), the collision is within a hand's width of the model's surface."""
        from assets import map_models
        model = base._source()
        errors = []
        for bdef in build.spec()['bases'][:1]:
            m = build.kit.Mesh(); m.origin = tuple(bdef['position']); m.yaw = math.radians(bdef['yaw'])
            for z in np.arange(-30.0, 8.0, 1.5):
                for x in (-2.0, -1.0, 0.0, 1.0, 2.0):
                    if base.HATCH[0]-.5 < x < base.HATCH[1]+.5 and base.HATCH[2]-.5 < z < base.HATCH[3]+.5: continue
                    lx, _, lz = base.to_base((x, 0, z))
                    top = map_models.surface_heights(model, [(lx, lz)])[0]
                    p = np.array(m.point(base.to_base((x, 20.0, z))))
                    t = self.soup.first(p, np.array([0.0, -1.0, 0.0]), 20.0)
                    errors.append(abs((20.0-t)-top))
        errors = np.array(errors)
        self.assertLess(np.percentile(errors, 90), .25, np.percentile(errors, [50, 90, 100]))

    def test_bases_keep_the_collision_budget(self):
        """Each team's ship, beacon and outpost together stay within the
        shared per-base budget (assets/budgets.py)."""
        from assets import budgets
        mesh = build.kit.Mesh(); mesh.lamps = []
        per_base = build.build_structures(build.spec(), mesh)[4]
        for team in (0, 1):
            self.assertLessEqual(per_base[team], budgets.COLLISION_TRIS_PER_BASE, per_base)

    def test_each_team_has_its_stations_and_turrets(self):
        kinds = {}
        for e in self.m['entities']:
            kinds.setdefault((e['team'], e['kind']), 0); kinds[(e['team'], e['kind'])] += 1
        for team in (0, 1):
            self.assertEqual(kinds[(team, 'inventory')], 5, 'three in the ship, two in the outpost')
            self.assertEqual(kinds[(team, 'turret')], 2)
            self.assertEqual(kinds[(team, 'generator')], 1)
            self.assertEqual(kinds[(team, 'sensor')], 1)

    def test_one_sea_volume_covers_the_map(self):
        v = self.m['water_volumes']
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0]['rect'], [0.0, 0.0, 2048.0, 2048.0])
        self.assertEqual(v[0]['surface'], terrain.SEA)

    def test_gangway_climbs_gently(self):
        gx0, gx1, gz0, gz1 = base.GANGWAY
        slope = math.degrees(math.atan2(base.LEDGE[4]-base.GANGWAY_Y, abs(gz1-gz0)))
        self.assertLess(slope, 20.0)
        # It meets the entry platform, in front of the bay's opening.
        lo, hi = sorted(-z for z in base.BAY_OPEN)    # the opening's base-local x span
        self.assertTrue(lo <= gx0 and gx1 <= hi, (gx0, gx1, lo, hi))
        self.assertAlmostEqual(gz0, -base.PLATFORM)

    def test_ship_spawns_face_open_floor_and_hide_from_the_field(self):
        """The ships' spawns (in each hold). The outposts' spawns are older
        and still fail these checks (docs/reefbreak.md, Pending)."""
        from assets import spawn_checks
        ships = [np.array(b['position']) for b in build.spec()['bases']]
        on_ship = lambda line: any(np.hypot(*(np.array([float(v) for v in line.split('(')[1].split(')')[0].split(',')])[[0, 2]]-s[[0, 2]])) < 50 for s in ships)
        self.assertEqual([p for p in spawn_checks.problems(PACK) if on_ship(p)], [])
        self.assertEqual([p for p in spawn_checks.exposed(PACK) if on_ship(p)], [])

    def test_no_turret_sees_into_the_bay_or_the_hold(self):
        """No turret engages a standable point in the flag bay or the hold
        beyond the doorway depth of an opening (sightline_checks)."""
        from assets import sightline_checks, turret_arcs
        spec = build.spec()
        mesh = build.kit.Mesh(); mesh.lamps = []
        flags, _, _, _, _ = build.build_structures(spec, mesh)
        soup = Soup(mesh.collision)
        targets, openings = [], []
        for bdef in spec['bases']:
            m = build.kit.Mesh(); m.origin = tuple(bdef['position']); m.yaw = math.radians(bdef['yaw'])
            P = lambda x, y, z: m.point(base.to_base((x, y, z)))
            xz = lambda x, z: tuple(P(x, 0, z)[i] for i in (0, 2))
            lo, hi = base.BAY_OPEN
            openings += [(xz(s*base.BAY_WALL_X, lo), xz(s*base.BAY_WALL_X, hi)) for s in (-1, 1)]
            rooms = ((base.HOLD_SECTION[0][0], base.HOLD_Z), (base.BAY_WALL_X, base.BAY_Z))
            for half, (z0, z1) in rooms:
                for x in np.arange(-half+.6, half-.5, 1.0):
                    for z in np.arange(z0+.6, z1-.5, 1.0):
                        if soup.first(P(x, .1, z), (0, 1, 0), 2.3) < 2.3: continue
                        targets.append(P(x, base.SPAWN_LIFT+.8, z))
        entities = turret_arcs.assign([dict(e) for e in mesh.entities], flags)
        targets = np.array(targets)
        allowed = sightline_checks.near_openings(targets, openings)
        for e in (e for e in entities if e['kind'] == 'turret'):
            seen = sightline_checks.visible(soup, e, targets) & ~allowed
            self.assertEqual(int(seen.sum()), 0, (e['id'], targets[seen][:5]))


if __name__ == '__main__':
    unittest.main()
