"""Checks for a built pack's water volumes (docs/map-pipeline.md).

`report(pack_dir)` measures every volume in map.json against the terrain:
its depth at the deepest point, whether the ground rises above the surface
all round the footprint's edge (no water hanging in the air), and its
clearance from flags, spawns, control-point rings, terrain holes and the
ski lanes. Map suites assert on the numbers they care about.
"""
import json
import math
from pathlib import Path

import numpy as np

from assets import props, water_bodies

STEP = 8.0
FLAG_CLEAR = 60.0      # no water within this of a flag
SPAWN_CLEAR = 30.0     # ... or a spawn point
RING_CLEAR = 10.0      # ... or a control point's ring edge
HOLE_CLEAR = 12.0      # ... or a terrain hole (bunkers, sewer, cavern, trench)
LANE_CLEAR = 30.0      # ponds keep off the ski lanes' centre lines


def _load(pack):
    pack = Path(pack)
    man = json.loads((pack/'map.json').read_text())
    h = np.frombuffer((pack/'height.bin').read_bytes(), '<u2').reshape(256, 256)/32
    return man, h


def _height(h, x, z):
    fx = min(max(x/STEP, 0), 255); fz = min(max(z/STEP, 0), 255)
    x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, 255), min(z0+1, 255); tx, tz = fx-x0, fz-z0
    a = h[z0, x0]+(h[z0, x1]-h[z0, x0])*tx; b = h[z1, x0]+(h[z1, x1]-h[z1, x0])*tx
    return float(a+(b-a)*tz)


def _outline(volume):
    if 'rect' in volume:
        x0, z0, x1, z1 = volume['rect']
        return [(x0, z0), (x1, z0), (x1, z1), (x0, z1)]
    return [tuple(p) for p in volume['polygon']]


def _inside_points(volume, spacing=4.0):
    xs = [p[0] for p in _outline(volume)]; zs = [p[1] for p in _outline(volume)]
    out = []
    for x in np.arange(min(xs), max(xs)+spacing, spacing):
        for z in np.arange(min(zs), max(zs)+spacing, spacing):
            if water_bodies.contains(volume, x, z): out.append((float(x), float(z)))
    return out


def report(pack):
    man, h = _load(pack)
    cps = man.get('control_points', [])
    prot = props.Protection(b'', man['flags'], man.get('spawn_points', []), cps)
    cut = set(man.get('holes', []))
    out = []
    for i, v in enumerate(man.get('water_volumes', [])):
        wet = [(x, z) for x, z in _inside_points(v) if _height(h, x, z) < v['surface']]
        deepest = max((v['surface']-_height(h, x, z) for x, z in wet), default=0.0)
        edge = _outline(v)
        # Ground on the footprint's edge: every outline point of a pond must
        # sit at or above the surface, or the water would hang in the air.
        # (A rect ravine's edge runs along the map, so sample its long sides.)
        if 'rect' in v:
            x0, z0, x1, z1 = v['rect']
            edge = [(x, z) for x in np.arange(x0+STEP, x1-STEP, STEP) for z in (z0, z1)]
        dry_edge = sum(_height(h, x, z) >= v['surface']-0.05 for x, z in edge)/max(len(edge), 1)
        flag = min(math.hypot(x-f[0], z-f[2]) for x, z in wet for f in man['flags']) if wet else math.inf
        spawn = min((math.hypot(x-p[0], z-p[2]) for x, z in wet for t in man.get('spawn_points', []) for p in t),
                    default=math.inf)
        ring = min((math.hypot(x-c['pos'][0], z-c['pos'][2])-c.get('radius', 12) for x, z in wet for c in cps),
                   default=math.inf)
        # A ring only floods if the water is above its floor.
        ring_floors_dry = all(c['pos'][1] > v['surface']+0.5 or
                              min(math.hypot(x-c['pos'][0], z-c['pos'][2]) for x, z in wet or [(1e9, 1e9)]) > c.get('radius', 12)
                              for c in cps)
        hole = min((math.hypot(x-((k % 256)*STEP+4), z-((k//256)*STEP+4)) for x, z in wet for k in cut),
                   default=math.inf)
        lane = min((prot.lane_distance(x, z) for x, z in wet), default=math.inf)
        out.append(dict(index=i, surface=v['surface'], wet_area=len(wet)*16.0, deepest=round(deepest, 2),
                        dry_edge=round(dry_edge, 3), flag=round(flag, 1), spawn=round(spawn, 1), ring=round(ring, 1),
                        ring_floors_dry=ring_floors_dry, hole=round(hole, 1), lane=round(lane, 1)))
    return out


def assert_ponds(test, pack, count, min_depth, max_depth):
    """The standard assertions for carved ponds (not Old Holler's ravine)."""
    rows = report(pack)
    test.assertEqual(len(rows), count, rows)
    for r in rows:
        test.assertGreaterEqual(r['deepest'], min_depth, r)
        test.assertLessEqual(r['deepest'], max_depth, r)
        test.assertGreaterEqual(r['wet_area'], 400.0, r)
        test.assertGreaterEqual(r['dry_edge'], 0.99, r)
        test.assertGreaterEqual(r['flag'], FLAG_CLEAR, r)
        test.assertGreaterEqual(r['spawn'], SPAWN_CLEAR, r)
        test.assertGreaterEqual(r['ring'], RING_CLEAR, r)
        test.assertGreaterEqual(r['hole'], HOLE_CLEAR, r)
        test.assertGreaterEqual(r['lane'], LANE_CLEAR, r)
    # Mirrored bodies match in size and depth.
    if count == 2:
        a, b = rows
        test.assertAlmostEqual(a['wet_area'], b['wet_area'], delta=max(a['wet_area'], b['wet_area'])*.25)
        test.assertAlmostEqual(a['deepest'], b['deepest'], delta=.6)
    return rows


if __name__ == '__main__':
    import sys
    for p in sys.argv[1:]:
        for row in report(p): print(Path(p).name, row)
