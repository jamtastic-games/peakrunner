#!/usr/bin/env python3
"""Test Ashveil's generators and a built pack (docs/ashveil.md; by default
the shipped one). Everything the map is made of is our own and in the tree.
Run from src/ with scripts/requirements.txt installed: scripts/test-ashveil.py [PACK]"""
import hashlib
import json
import math
from pathlib import Path
import sys
import unittest

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from assets import ashveil_beasts as BE
from assets import ashveil_land as LAND
from assets import ashveil_mesa as ME
from assets import ashveil_ring as RG
from assets import ashveil_shapes as A
from assets import ashveil_temple as TP
from assets import ashveil_tower as TW
from assets import ashveil_undercroft as UC
from assets import spawn_checks

HERE = Path(__file__).parent
PACK = Path(sys.argv.pop(1)) if len(sys.argv) > 1 else HERE.parent/'assets/maps/ashveil'


def spec_value(key):
    return json.loads((HERE.parent/'maps/ashveil.json').read_text())[key]


def tower_point(f, y, w):
    """World point of a point in the skull tower's frame."""
    t = spec_value('tower')
    a = math.radians(t['facing']); c, s = math.cos(a), math.sin(a)
    return (t['at'][0]+f*c-w*s, t['plinth']+y, t['at'][1]+f*s+w*c)


def volume(form):
    t = form.v[form.f]
    return np.einsum('ij,ij->i', t[:, 0], np.cross(t[:, 1], t[:, 2])).sum()/6


class Generators(unittest.TestCase):
    def test_ear_clipping_keeps_a_concave_outline(self):
        poly = [(0, 0), (4, 0), (4, 3), (2, 1), (0, 3)]
        tri = A.triangulate(poly)
        p = np.array(poly, float)
        area = sum(abs((p[b, 0]-p[a, 0])*(p[c, 1]-p[a, 1])-(p[b, 1]-p[a, 1])*(p[c, 0]-p[a, 0]))/2 for a, b, c in tri)
        self.assertEqual(len(tri), 3)
        self.assertAlmostEqual(area, 8.0)

    def test_closed_forms_wind_outward(self):
        forms = [A.lathe([(0, 0), (1, 0), (1, 2), (0, 2)], sides=12),
                 A.lathe([(1, 0), (2, 0), (2, 1), (1, 1), (1, 0)], arc=(0, 90), sides=16),
                 A.extrude([(0, 0), (3, 0), (3, 1), (0, 1)], 0, 2, axis='x'),
                 A.extrude([(0, 0), (3, 0), (3, 1), (0, 1)], 0, 2, axis='y'),
                 A.blob((-1, -1, 0), (1, 1, 1), 7), A.crag((-2, -2, 0), (2, 2, 3), 7, peaks=9),
                 A.spindle((-.5, -.5, 0), (.5, .5, 2), 7), A.slab((-.5, -2, 0), (0, 2, 3), 7),
                 A.tube([(0, 0, 0), (0, 0, 2), (1, 0, 3)], [.4, .3, .2])]
        for form in forms:
            self.assertGreater(volume(form), 0, form.material)

    def test_mirrored_placement_keeps_faces_outward_and_quads_paired(self):
        form = A.box((0, 0, 0), (1, 2, 3))
        mirror = [[-1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]]   # x flipped
        tris, normals = A.place(form, mirror)
        centre = tris.reshape(-1, 3).mean(0)
        out = np.einsum('ij,ij->i', normals[:, 0], tris.mean(1)-centre)
        self.assertTrue((out > 0).all())
        for k in range(0, len(tris), 2):
            a, b = tris[k], tris[k+1]
            if np.array_equal(a[0], b[0]): self.assertTrue(np.array_equal(b[1], a[2]))

    def test_isosurface_closes_and_winds_outward(self):
        ball = A.isosurface(lambda p: np.linalg.norm(p, axis=1)-2.0, (-3, -3, -3), (3, 3, 3), .25)
        self.assertAlmostEqual(volume(ball), 4/3*math.pi*8, delta=1.0)
        edges = np.sort(np.concatenate([ball.f[:, [0, 1]], ball.f[:, [1, 2]], ball.f[:, [2, 0]]]), 1)
        _, uses = np.unique(edges, axis=0, return_counts=True)
        self.assertTrue((uses == 2).all(), 'every edge between two faces')

    def test_tower_is_one_hollow_shell_open_from_the_gate_to_the_crown(self):
        field = TW.tower_field()
        empty = lambda p: field(np.array([p], np.float64))[0] > .3
        fx, fz = TW.FLAG
        nx, n0, n1, _ = TW.NECK
        w2, w3 = TW.SKULLS[1][0], TW.SKULLS[2][0]
        eye = TW.world(1, np.array([[TW.EYE[0]*w2, TW.EYE[1]*w2, -TW.EYE[2]*w2]]))[0]
        crown = TW.world(2, np.array([[0, .85*w3, 0]]))[0]
        path = [(30, 3, 0), (16, 3, 0), (-4, 3, 0)]+[(TW.axis_x(y), y, 0) for y in range(5, 81, 5)] \
            + [(fx, TW.EYE_LEDGE+2, fz), (eye[0]+12, eye[1], eye[2])]+[(nx, y, 0) for y in np.linspace(n0, n1, 6)] \
            + [tuple(crown)]
        for p in path:
            self.assertTrue(empty(p), p)
        # Coming in from behind, the bone is a shell a few metres thick.
        x = np.arange(-60, 0, .25)
        solid = field(np.c_[x, np.full_like(x, 30.0), np.zeros_like(x)]) < 0
        first = np.argmax(solid)
        run = np.argmax(~solid[first:])
        self.assertTrue(2.0 <= run*.25 <= 5.0, run*.25)

    def test_tower_shelves_climb_a_jet_at_a_time_and_meet_the_wall(self):
        field = TW.tower_field()
        tops = sorted(s[4] for s in TW.SHELVES)
        self.assertLess(tops[0], 20.0)
        self.assertLessEqual(max(np.diff(tops)), 19.0)
        for x0, x1, z0, z1, top in TW.SHELVES:
            X, Z = np.meshgrid(np.linspace(x0, x1, 9), np.linspace(z0, z1, 7))
            at = lambda y: field(np.c_[X.ravel(), np.full(X.size, y), Z.ravel()])
            self.assertGreater((np.min([at(top+h) for h in (.5, 2.0, 3.5)], 0) > .3).mean(), .3, top)
            self.assertGreater((at(top-1.3) < 0).mean(), .02, f'the shelf at {top} touches bone')

    def test_carving_cuts_away_the_inside_and_keeps_the_rest(self):
        quad = np.array([[(0, 0, 0), (10, 0, 10), (10, 0, 0)], [(0, 0, 0), (0, 0, 10), (10, 0, 10)]], float)
        cut = UC.carve(quad, UC.square((5, 5), 2), -1, 1)
        area = lambda t: np.linalg.norm(np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0]), axis=1).sum()/2
        self.assertAlmostEqual(area(cut), 100-16, 6)
        c = cut.mean(1)
        self.assertFalse(((abs(c[:, 0]-5) < 2) & (abs(c[:, 2]-5) < 2)).any())
        self.assertIs(UC.carve(quad, UC.square((5, 5), 2), 1, 2), quad, 'out of the height range')
        ring = UC.carve(quad, UC.polygon((5, 5), 3, 32), -1, 1)
        self.assertAlmostEqual(area(ring), 100-32*9*math.sin(math.tau/32)/2, 6)
        self.assertEqual(UC.box_minus((0, 0, 0), (10, 1, 2), (3, -1), (6, 5)), [((0, 0, 0), (3, 1, 2)), ((6, 0, 0), (10, 1, 2))])

    def test_the_crypt_fits_under_the_temple_and_walks_from_the_gallery_down(self):
        c, top = (300.0, 300.0), 39.0
        built = UC.build(c, top, TP.TOP)
        floor = top-UC.DEPTH
        self.assertEqual(built['point'], (300.0, floor+UC.DAIS[2], 300.0))
        lo, hi = built['hall']
        self.assertEqual((lo[1], hi[1]), (floor, UC.VAULT))
        self.assertLess(UC.VAULT, TP.TOP-TP.THICK, 'the dome stays under the temple floor')
        ramps = 0
        for form, lit in built['parts']:
            t = form.v[form.f]
            d = np.zeros(len(t))
            for k in range(8):
                a = math.radians(45*k)
                d = np.maximum(d, ((t[..., 0]-c[0])*math.cos(a)+(t[..., 2]-c[1])*math.sin(a)).max(1))
            self.assertTrue((d <= TP.APOTHEM+1.5).all(), 'under the temple floor')
            if lit == 'hidden':
                n = np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0]); n /= np.linalg.norm(n, axis=1)[:, None]
                slope = n[(n[:, 1] > .1) & (n[:, 1] < .99)]
                self.assertTrue((slope[:, 1] >= .75).all(), 'walkable')
                ramps += 1
        self.assertEqual(ramps, 4, 'two flights for each stair')

    def test_the_cut_lies_under_the_temple_floor(self):
        c = (300.0, 300.0)
        vents = TP.build(c, UC.carve)['vents']
        cells = UC.cut(c, LAND.STEP, vents)
        self.assertGreater(len(cells), 150)
        for cell in cells:
            i, k = cell % 256, cell//256
            xs, zs = np.array([i, i+1, i+1, i])*LAND.STEP, np.array([k, k, k+1, k+1])*LAND.STEP
            self.assertTrue(UC.inside_octagon(c, TP.APOTHEM, xs, zs).all())
        for vx, vz, r in vents:
            need = {int(vz/LAND.STEP)*256+int(vx/LAND.STEP)}
            self.assertTrue(need <= cells, 'the vents are cut')

    def test_the_temple_rises_to_eight_spire_tips(self):
        built = TP.build((300.0, 300.0), UC.carve)
        tips = built['spires']
        self.assertEqual(len(tips), 8)
        for x, y, z in tips:
            self.assertAlmostEqual(y, TP.TOP+TP.SPIRE_TALL)
            self.assertAlmostEqual(math.hypot(x-300, z-300), TP.SPIRE_R, 6)
        for form, lit in built['parts']:
            if form.solid and lit != 'hidden':
                self.assertLessEqual(form.v[:, 1].max(), TP.TOP+TP.SPIRE_TALL+1e-6)

    def test_the_land_is_ours_and_the_same_every_time(self):
        spec = json.loads((HERE.parent/'maps/ashveil.json').read_text())
        a, masks, feat = LAND.heights(spec['land'], spec['ring']['fence'])
        b, _, _ = LAND.heights(spec['land'], spec['ring']['fence'])
        self.assertTrue(np.array_equal(a, b))
        self.assertTrue(masks['craters'].any() and masks['trench'].any() and masks['flow_top'].any())
        X, Z = LAND.coords()
        for c, floor in zip(spec['land']['cones'], feat['crater_floors']):
            rim = a[np.abs(np.hypot(X-c['at'][0], Z-c['at'][1])-c['rim_r']) < 3]
            self.assertGreater(np.percentile(rim, 90), floor+c['crater']*.6, c['name'])
        cut, tubes = LAND.cut_tubes(a, spec['land'])
        for t in tubes:
            x, z = t['q'][len(t['q'])//2]
            self.assertAlmostEqual(cut[int(z/LAND.STEP), int(x/LAND.STEP)], t['floor'])

    def test_the_beasts_stand_on_the_ground_as_arches(self):
        flat = lambda x, z: 50.0
        spec = json.loads((HERE.parent/'maps/ashveil.json').read_text())['beasts']
        forms = BE.build(spec, flat, 3)
        self.assertGreater(len(forms), 100)
        bones = np.concatenate([f.v for f in forms if f.solid])
        self.assertLess(bones[:, 1].min(), 50.0, 'their feet are in the ash')
        self.assertGreater(bones[:, 1].max(), 50.0+25.0, 'the stalker stands tall')
        for f in forms: self.assertTrue(f.material in ('bone', 'bone_grime'), f.material)
        # The ribcage is a tunnel: open along its middle up to well over head height.
        r = spec['ribcages'][0]
        a = math.radians(r['facing'])
        for t in np.linspace(-.4, .4, 9):
            x, z = r['at'][0]+math.cos(a)*t*r['length'], r['at'][1]+math.sin(a)*t*r['length']
            near = bones[(np.hypot(bones[:, 0]-x, bones[:, 2]-z) < 3.0) & (bones[:, 1] < 50.0+8.0)]
            self.assertEqual(len(near), 0, (x, z))

    def test_the_mesa_holds_the_flag_high_and_its_stations_under_it(self):
        built = ME.build((300.0, 300.0), 60.0, lambda x, z: 60.0)
        x, y, z = built['flag']
        self.assertAlmostEqual(y, 60.0+ME.TOP+.4, 6)
        self.assertEqual(len(built['stations']), 2)
        for (sx, sy, sz), heading in built['stations']:
            over = [f for f, _ in built['parts'] if f.solid and f.v[:, 1].max() > 62.0
                    and np.hypot(f.v[:, 0].mean()-sx, f.v[:, 2].mean()-sz) < ME.R*.8]
            self.assertTrue(over, 'under the columns')
            self.assertTrue(all(f.v[:, 1].min() >= 60.0+ME.ROOF-1e-6 for f in over), 'with headroom')

    def test_forms_are_deterministic(self):
        a = A.crag((-2, -2, 0), (2, 2, 3), 11, peaks=8)
        b = A.crag((-2, -2, 0), (2, 2, 3), 11, peaks=8)
        self.assertTrue(np.array_equal(a.v, b.v))


@unittest.skipUnless((PACK/'map.json').exists(), 'no built Ashveil pack')
class Pack(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.m = json.loads((PACK/'map.json').read_text())
        cls.tris = np.frombuffer((PACK/'collision.bin').read_bytes(), '<f4').reshape(-1, 3, 3)

    def test_payloads_match_their_digests(self):
        for name, digest in self.m['files'].items():
            self.assertEqual(hashlib.sha256((PACK/name).read_bytes()).hexdigest(), digest, name)

    def test_grid_and_limits(self):
        self.assertEqual(self.m['terrain_step'], LAND.STEP)
        self.assertLessEqual(len(self.tris), 500_000)
        extent = (self.tris.max(1)-self.tris.min(1)).max()
        self.assertLessEqual(extent, 512)
        self.assertLessEqual(len(self.m['water_volumes']), 32)
        self.assertLessEqual(len(self.m['smoke']), 64)

    def test_it_is_all_our_own(self):
        self.assertNotIn('source_sha256', self.m)
        self.assertIn('original', self.m['provenance'])
        for key in ('land_source_sha256', 'temple_source_sha256', 'mesa_source_sha256', 'undercroft_source_sha256'):
            self.assertIn(key, self.m)

    def test_spawns_split_evenly_and_face_open_floor(self):
        teams = self.m['spawn_points']
        self.assertEqual([len(t) for t in teams], [16, 16])
        for team, flag in zip(teams, self.m['flags']):
            other = self.m['flags'][1-self.m['flags'].index(flag)]
            for x, y, z, yaw in team:
                self.assertTrue(0 <= yaw < 2*math.pi)
                self.assertLess(math.dist((x, z), flag[::2]), math.dist((x, z), other[::2]))
        self.assertEqual(spawn_checks.problems(PACK), [])
        for x, z, r in BE.footprints(spec_value('beasts')):
            for team in teams:
                for sx, _, sz, _ in team:
                    self.assertGreater(math.dist((sx, sz), (x, z)), r, 'no spawn among the bones')
        self.assertEqual(spawn_checks.exposed(PACK), [])

    def test_stations_serve_both_teams(self):
        stations = [e for e in self.m['entities'] if e['kind'] == 'inventory']
        self.assertEqual(len(stations), 2*(len(spec_value('stations'))+2+len(TW.build()['stations'])))
        for a, b in zip(stations[::2], stations[1::2]):
            self.assertEqual(a['position'], b['position'])
            self.assertEqual({a['team'], b['team']}, {0, 1})
            self.assertEqual(a['circuit'], 'always-on')

    def test_updrafts_rise_over_the_pools(self):
        pools = self.m['water_volumes']
        zones = [z for z in self.m['zones'] if z['kind'] == 'updraft']
        self.assertEqual(len(zones), len(pools))
        self.assertGreaterEqual(len(pools), 8)
        h = np.frombuffer((PACK/'height.bin').read_bytes(), '<u2').reshape(256, 256)/32
        for z, p in zip(zones, pools):
            x0, z0, x1, z1 = p['rect']
            self.assertTrue(x0 < z['at'][0] < x1 and z0 < z['at'][2] < z1)
            self.assertAlmostEqual(z['at'][1]-z['size'][1], p['surface'], places=1)
            self.assertTrue(0 < z['lift'] <= 80)
            # The lava shows: the ground dips under it in the pool's middle.
            i, k = int(z['at'][0]/LAND.STEP), int(z['at'][2]/LAND.STEP)
            self.assertLess(h[k, i], p['surface'])

    def test_lava_ground_burns_the_flows_and_spares_starts_and_cuts(self):
        lava = np.frombuffer((PACK/'lava.bin').read_bytes(), np.uint8).reshape(256, 256, 4)
        surface = (lava[..., 0].astype(int) | (lava[..., 1].astype(int) << 8))/32
        on = surface > 0
        skim = on & (lava[..., 3] == 1)
        self.assertGreater((on & ~skim).sum(), 100, 'the flows burn')
        self.assertGreater(skim.sum(), 100, 'the pools and rivers burn')
        step = self.m['terrain_step']
        cx = (np.arange(256)+.5)*step
        X, Z = np.meshgrid(cx, cx)
        self.assertTrue((lava[skim, 2] == 150).all())
        self.assertTrue((lava[on & ~skim, 2] == spec_value('lava_ground')['flow_burn']).all())
        self.assertFalse(on.ravel()[self.m['holes']].any(), 'none under the cut')
        points = [(p[0], p[2]) for team in self.m['spawn_points'] for p in team]+[(f[0], f[2]) for f in self.m['flags']] \
            + [(e['position'][0], e['position'][2]) for e in self.m['entities']]
        for x, z in points:
            self.assertFalse((on & (np.hypot(X-x, Z-z) <= spec_value('lava_ground')['keep_clear'])).any(), (x, z))

    def test_the_mesa_flag_stands_in_the_open_on_top(self):
        lo, hi = self.tris.min(1), self.tris.max(1)
        wall = (hi[:, 1]-lo[:, 1] > 100) | ((hi[:, 0]-lo[:, 0] > 60) & (hi[:, 2]-lo[:, 2] > 60))
        x, y, z = self.m['flags'][0]
        near = (lo[:, 0] <= x+4) & (hi[:, 0] >= x-4) & (lo[:, 2] <= z+4) & (hi[:, 2] >= z-4) \
            & (hi[:, 1] >= y+0.3) & (lo[:, 1] <= y+20) & ~wall
        self.assertFalse(near.any(), f'something stands over or round the flag at {(x, z)}')
        h = np.frombuffer((PACK/'height.bin').read_bytes(), '<u2').reshape(256, 256)/32
        self.assertGreater(y-h[int(z/LAND.STEP), int(x/LAND.STEP)], ME.TOP-1, 'up on the mesa')

    def test_tower_flag_stands_in_its_eye_open_to_the_face_and_the_cavern(self):
        from importlib import util
        loader = util.spec_from_file_location('builder', HERE/'build-ashveil.py')
        builder = util.module_from_spec(loader); loader.loader.exec_module(builder)
        x, y, z = self.m['flags'][1]
        fx, fz = TW.FLAG
        self.assertLess(math.dist((x, y, z), tower_point(fx, TW.EYE_LEDGE+.36, fz)), .5)
        chest = (x, y+1.2, z)
        self.assertGreater(builder.clearance(self.tris, chest, (0, 1, 0)), 5.0, 'headroom')
        out = np.subtract(tower_point(40, TW.EYE_LEDGE+1.6, fz), chest)
        self.assertGreaterEqual(builder.clearance(self.tris, chest, out), 40.0, 'out of the eye')
        back = np.subtract(tower_point(-10, TW.EYE_LEDGE+1.6, fz*.5), chest)
        self.assertGreaterEqual(builder.clearance(self.tris, chest, back), 20.0, 'back into the cavern')

    def test_valleys_run_lava_round_the_sides(self):
        lava = np.frombuffer((PACK/'lava.bin').read_bytes(), np.uint8).reshape(256, 256, 4)
        on = (lava[..., 0].astype(int) | lava[..., 1].astype(int)) > 0
        step = self.m['terrain_step']
        for v in spec_value('ring')['valleys']:
            q, _, s = RG.spline(v['points'])
            ix, iz = np.clip((q/step).astype(int), 2, 253).T
            wet = [on[z-2:z+3, x-2:x+3].any() for x, z in zip(ix, iz)]
            self.assertGreater(np.mean(wet), .6, v['name'])
            self.assertGreater(s[-1], 600, v['name'])

    def test_the_cathedral_opens_from_the_tubes_vents_and_well_down_to_the_post(self):
        from importlib import util
        loader = util.spec_from_file_location('builder', HERE/'build-ashveil.py')
        builder = util.module_from_spec(loader); loader.loader.exec_module(builder)
        points = {p['id']: p for p in self.m['control_points']}
        post = points['sanctum']
        self.assertTrue(post['ctf_active'] and post['capture'] <= 1.5 and post['drain']['airborne'] and post['drain']['radius'] == 130.0)
        self.assertEqual(len(post['spires']), 8)
        x, y, z = post['pos']
        cx, cz = spec_value('land')['dome']['at']
        self.assertEqual((x, z), (cx, cz))
        # The hall burns the post's holders: its box holds the post and
        # reaches the gallery, wall to wall under the temple.
        lo, hi = post['cook']['min'], post['cook']['max']
        self.assertTrue(lo[0] < x < hi[0] and lo[1] < y < hi[1] and lo[2] < z < hi[2])
        self.assertTrue(all(abs(hi[k]-lo[k]-2*UC.H) < 1e-3 for k in (0, 2)) and hi[1]-lo[1] > 25, (lo, hi))
        self.assertEqual(post['cook']['damage'], 20.0)
        self.assertTrue(all(abs(math.hypot(s[0]-x, s[2]-z)-TP.SPIRE_R) < .01 and s[1] > y+50 for s in post['spires']))
        # Neither flag is in the lightning's reach.
        for f in self.m['flags']:
            self.assertGreater(math.dist((x, z), (f[0], f[2])), post['drain']['radius']+15)
        gallery = y-UC.DAIS[2]+UC.DEPTH
        # From each tube's passage straight into the hall over the gallery.
        for s in (-1, 1):
            mouth = (x+s*(UC.H+4.0), gallery+1.5, z+1.0)
            self.assertGreater(builder.clearance(self.tris, mouth, (-s, 0, 0), 60), 4.0+UC.GALLERY)
        # From the post straight up the well into the temple's cage.
        self.assertGreater(builder.clearance(self.tris, (x, y+UC.POST+1.0, z), (0, 1, 0), 60), 25.0)
        # Down each vent from the temple's floor to the gallery's level.
        for b in UC.VENTS:
            vx, vz = x+22.0*math.cos(math.radians(b)), z+22.0*math.sin(math.radians(b))
            self.assertGreater(builder.clearance(self.tris, (vx, TP.TOP+1.0, vz), (0, -1, 0), 40), TP.TOP+1.0-gallery-.1)
        # Every cut cell lies under the temple's floor.
        for cell in self.m['holes']:
            i, k = cell % 256, cell//256
            xs, zs = np.array([i, i+1, i+1, i])*LAND.STEP, np.array([k, k, k+1, k+1])*LAND.STEP
            self.assertTrue(UC.inside_octagon((cx, cz), TP.APOTHEM, xs, zs).all(), cell)
        self.assertGreaterEqual(len(points), 2, 'Capture & Hold needs two')

    def test_fence_and_ceiling_close_the_play_square(self):
        top = self.tris[:, :, 1].max()
        self.assertGreater(top, 190)
        spans = self.tris.max(1)-self.tris.min(1)
        tall = self.tris[spans[:, 1] > 150]
        self.assertGreater(len(tall), 0)

if __name__ == '__main__':
    unittest.main()
