#!/usr/bin/env python3
"""Scan every embedded map pack for visible clipping:

- flicker: overlapping, coplanar render faces facing the same way (the depth
  buffer can't order them; same-material pairs flicker too once their lighting
  differs), not sealed against an opposite face or buried in a solid;
- terrain: floors (upward faces) with terrain rising through them where the
  terrain isn't cut away, so the ground shows inside a room or through a deck.

Usage, from src/ with scripts/requirements.txt installed:
  python3 scripts/scan-clipping.py [MAP...]
"""
import json, sys
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'scripts'))
from assets import surface_checks
from assets.props import Terrain

FLOOR_NY = 0.7          # upward faces
TERRAIN_ABOVE = 0.08    # metres of ground above a floor that shows


def flicker(vertices, collision):
    """Every visible overlapping coplanar pair, same-material pairs included
    (the build separates them: pack_writer.resolve_overlaps)."""
    v = np.asarray(vertices, np.float64).reshape(-1, 3, 12)
    return [(area, centre, int(v[i, 0, 10]), int(v[j, 0, 10])) for i, j, area, centre in surface_checks.overlap_pairs(vertices, collision)]


def terrain_through_floors(vertices, terrain):
    v = np.asarray(vertices, np.float64).reshape(-1, 3, 12)
    p = v[:, :, :3]
    n = np.cross(p[:, 1] - p[:, 0], p[:, 2] - p[:, 0])
    area = np.linalg.norm(n, axis=1) / 2
    ny = n[:, 1] / np.maximum(1e-9, 2 * area)
    out = []
    for i in np.flatnonzero((ny > FLOOR_NY) & (area > 0.05)):
        tri = p[i]
        worst = 0.0
        for w in ((1/3, 1/3, 1/3), (0.6, 0.2, 0.2), (0.2, 0.6, 0.2), (0.2, 0.2, 0.6)):
            q = w[0] * tri[0] + w[1] * tri[1] + w[2] * tri[2]
            cx, cz = int(min(max(q[0] / terrain.step, 0), 255)), int(min(max(q[2] / terrain.step, 0), 255))
            if terrain.cut[cz, cx]:
                continue
            above = terrain.height(q[0], q[2]) - q[1]
            worst = max(worst, above)
        if worst > TERRAIN_ABOVE:
            out.append((round(float(worst), 2), round(float(area[i]), 2), [round(float(c), 1) for c in tri.mean(0)], int(v[i, 0, 10])))
    return sorted(out, key=lambda e: -e[0] * e[1])


def scan(name):
    root = ROOT / 'assets/maps' / name
    manifest = json.loads((root / 'map.json').read_text())
    read = lambda f: (root / f).read_bytes()
    vertices = np.frombuffer(read('vertices.bin'), '<f4')
    collision = np.frombuffer(read('collision.bin'), '<f4')
    terrain = Terrain(read('height.bin'), read('weights.rgba'), manifest.get('holes', []), step=manifest.get('terrain_step', 8.0))
    return flicker(vertices, collision), terrain_through_floors(vertices, terrain)


if __name__ == '__main__':
    names = sys.argv[1:] or sorted(p.name for p in (ROOT / 'assets/maps').iterdir() if (p / 'map.json').exists())
    for name in names:
        fl, tf = scan(name)
        print(f'{name}: {len(fl)} flicker pairs ({sum(a for a, *_ in fl):.1f} m2), {len(tf)} floors with terrain through them')
        for area, centre, ma, mb in fl[:6]:
            print(f'   flicker {area:6.2f} m2 at {centre} materials {ma}/{mb}')
        for above, area, centre, mat in tf[:6]:
            print(f'   terrain {above:5.2f} m over a {area:.1f} m2 floor at {centre} (material {mat})')
