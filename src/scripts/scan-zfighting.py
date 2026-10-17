#!/usr/bin/env python3
"""Scan built map packs for surfaces that can flicker (z-fight).

Stricter than `assets/surface_checks.z_fighting`:
- faces up to NEAR apart count (default 0.5 cm, env SCAN_NEAR in metres),
  not only exact coplanars (depth precision falls off with distance, so
  sub-centimetre gaps flicker far away);
- same-material pairs are reported too (baked lighting can still differ),
  but only mixed-material spots fail the scan;
- overlap is tested at a grid of interior sample points of each triangle, so
  partial overlaps are found, not only ones containing a centroid.
Pairs sealed by an opposite-facing face in the same plane, or whose every
overlapping sample point is buried inside the collision solid, are skipped.

Usage (from src/, with scripts/requirements.txt installed): scan-zfighting.py [MAP_DIR ...]
Default: every embedded pack under assets/maps. Prints clusters per map
(1 m cells) with their materials, largest first (SCAN_LIMIT per map).
Exit status 1 if any mixed-material spot is found.
"""
from pathlib import Path
import sys

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from assets.surface_checks import _embedded

LIMIT = int(__import__('os').environ.get('SCAN_LIMIT', '12'))
NEAR = float(__import__('os').environ.get('SCAN_NEAR', '.005'))   # metres: same-facing faces this close flicker at any range
MIN_AREA = .01      # m^2: ignore slivers
MARGIN = .03        # barycentric margin: touching edges are not overlaps
# Barycentric sample points over each triangle's interior (a 6-step grid).
SAMPLES = np.array([[a/6, b/6, (6-a-b)/6] for a in range(1, 6) for b in range(1, 6-a)] +
                   [[1/3, 1/3, 1/3], [.45, .45, .1], [.1, .45, .45], [.45, .1, .45]])


def _bary_inside(pts, tri, margin=MARGIN):
    """Which 2D points lie inside the 2D triangle, `margin` in from its edges."""
    a, b, c = tri
    v0, v1 = c-a, b-a
    d00, d01, d11 = v0 @ v0, v0 @ v1, v1 @ v1
    den = d00*d11-d01*d01
    if abs(den) < 1e-12: return np.zeros(len(pts), bool)
    v2 = pts-a
    d20, d21 = v2 @ v0, v2 @ v1
    u = (d11*d20-d01*d21)/den; v = (d00*d21-d01*d20)/den
    return (u > margin) & (v > margin) & (u+v < 1-margin)


def scan(vertices, collision=None, near=NEAR, cell=4.0):
    """Pairs of overlapping same-facing triangles whose planes are closer than
    `near`: (area, centre, material_a, material_b, gap, index_a, index_b).
    Candidates share a cell of a 3D grid over their bounding boxes; the gap
    is measured between the actual planes (a face's centre against the other
    face's plane), never from the world origin, where a slight tilt between
    nearly parallel normals would swamp it."""
    v = np.asarray(vertices, np.float64).reshape(-1, 3, 12)
    solids = None if collision is None else np.asarray(collision, np.float64).reshape(-1, 3, 3)
    p = v[:, :, :3]; mat = v[:, 0, 10].astype(int)
    n = np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]); ln = np.linalg.norm(n, axis=1)
    ok = np.flatnonzero(ln > 2*MIN_AREA)
    unit = np.zeros_like(n); unit[ok] = n[ok]/ln[ok, None]
    lo = p.min(1); hi = p.max(1)
    cells = {}
    for i in ok:
        a = np.floor(lo[i]/cell).astype(int); b = np.floor(hi[i]/cell).astype(int)
        for x in range(a[0], b[0]+1):
            for y in range(a[1], b[1]+1):
                for z in range(a[2], b[2]+1):
                    cells.setdefault((x, y, z), []).append(i)
    seen = set(); found = []
    for members in cells.values():
        if len(members) < 2: continue
        mem = np.array(members)
        for x in range(len(mem)):
            i = mem[x]; rest = mem[x+1:]
            if not len(rest): continue
            m = (unit[rest] @ unit[i]) > .999
            m &= np.all((lo[rest] < hi[i]+near) & (hi[rest] > lo[i]-near), 1)
            for j in rest[m]:
                key = (min(i, j), max(i, j))
                if key in seen: continue
                seen.add(key)
                gap = max(abs(unit[i] @ (p[j].mean(0)-p[i][0])), abs(unit[j] @ (p[i].mean(0)-p[j][0])))
                if gap > near: continue
                axis = int(np.abs(unit[i]).argmax()); keep = [c for c in range(3) if c != axis]
                pi = p[i][:, keep]; pj = p[j][:, keep]
                in_i = _bary_inside(SAMPLES @ pj, pi); in_j = _bary_inside(SAMPLES @ pi, pj)
                if not (in_i.any() or in_j.any()): continue
                # 3D points where the two faces actually overlap.
                overlap = np.vstack([(SAMPLES @ p[j])[in_i], (SAMPLES @ p[i])[in_j]])
                small = j if ln[j] < ln[i] else i
                centre = p[small].mean(0)
                # Sealed: an opposite-facing face in the same plane covers it.
                opp = mem[((unit[mem] @ unit[small]) < -.999)]
                sealed = False
                for q in opp[:200]:
                    if abs(unit[q] @ (centre-p[q][0])) < near and _bary_inside((SAMPLES @ p[small][:, keep])[:1], p[q][:, keep], -1e-4).all():
                        sealed = True; break
                # Hidden only if every overlapping point is buried (a wall
                # standing on the seam), not just the smaller face's centre.
                if sealed or all(_embedded(q+unit[small]*.02, solids) for q in overlap): continue
                found.append((float(min(ln[i], ln[j])/2), centre, int(mat[i]), int(mat[j]), float(gap), int(i), int(j)))
    return found


def clusters(found, cell=1.0):
    out = {}
    for area, c, ma, mb, gap, *_ in found:
        key = tuple(np.floor(c/cell).astype(int))
        e = out.setdefault(key, dict(area=0.0, centre=c, mats=set(), gap=gap, pairs=0))
        e['area'] += area; e['mats'] |= {ma, mb}; e['pairs'] += 1; e['gap'] = min(e['gap'], gap)
    return sorted(out.values(), key=lambda e: -e['area'])


def main():
    here = Path(__file__).resolve().parent.parent
    dirs = [Path(a) for a in sys.argv[1:]] or sorted(p for p in (here/'assets/maps').iterdir() if (p/'map.json').exists())
    import importlib.util
    spec = importlib.util.spec_from_file_location('kit', here/'scripts/build-original-map.py')
    kit = importlib.util.module_from_spec(spec); spec.loader.exec_module(kit)
    names = kit.MATERIALS
    total = 0
    for d in dirs:
        verts = np.frombuffer((d/'vertices.bin').read_bytes(), '<f4')
        coll = np.frombuffer((d/'collision.bin').read_bytes(), '<f4')
        found = scan(verts, coll)
        mixed = [f for f in found if f[2] != f[3]]
        same = [f for f in found if f[2] == f[3]]
        cl = clusters(mixed)
        total += len(cl)
        print(f'{d.name}: {len(verts)//36} triangles; {len(cl)} spots with different materials '
              f'({len(mixed)} pairs), {len(clusters(same))} same-material spots')
        for e in cl[:LIMIT]:
            mats = '/'.join(names[m] if 0 <= m < len(names) else str(m) for m in sorted(e['mats']))
            c = e['centre']
            print(f'   {e["area"]:7.2f} m²  at ({c[0]:7.1f}, {c[1]:6.1f}, {c[2]:7.1f})  gap {e["gap"]*100:4.1f} cm  {mats}  ({e["pairs"]} pairs)')
    return 1 if total else 0


if __name__ == '__main__':
    sys.exit(main())
