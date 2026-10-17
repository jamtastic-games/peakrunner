"""Render-surface checks shared by map test suites.

z_fighting(vertices): pairs of render triangles that lie in the same plane,
face the same way, overlap, and carry different materials. The depth buffer
cannot order such a pair, so it flickers between them (the "stippled roof"
the visual audit found). Faces that touch back to back (opposite normals)
are hidden, not z-fighting, and are ignored.

Input is the kit's render vertex array as floats: 12 per vertex (pos3,
normal3, uv2, lmuv2, material, light layer), three vertices per triangle.
"""
import numpy as np

PLANE_EPS = .004     # metres: faces closer than this share a plane
MIN_AREA = .01       # m^2: ignore slivers


def _inside(pt, tri, margin=.02):
    a, b, c = tri
    v0, v1, v2 = c-a, b-a, pt-a
    d00, d01, d11 = v0 @ v0, v0 @ v1, v1 @ v1
    d20, d21 = v2 @ v0, v2 @ v1
    den = d00*d11-d01*d01
    if abs(den) < 1e-12: return False
    u = (d11*d20-d01*d21)/den; v = (d00*d21-d01*d20)/den
    return u > margin and v > margin and u+v < 1-margin


def _embedded(point, solids):
    """Is the point inside the closed collision soup (odd crossings along a
    skew ray)? Faces buried inside a wall or slab can never be seen."""
    if solids is None: return False
    d = np.array([.5773, .5779, .5768])
    a, e1, e2 = solids[:, 0], solids[:, 1]-solids[:, 0], solids[:, 2]-solids[:, 0]
    h = np.cross(d, e2); det = (e1*h).sum(1); ok = np.abs(det) > 1e-12
    inv = np.where(ok, 1/np.where(ok, det, 1), 0)
    s = point-a; u = (s*h).sum(1)*inv; q = np.cross(s, e1); v = (q @ d)*inv; t = (e2*q).sum(1)*inv
    return int((ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > 1e-6)).sum()) % 2 == 1


def z_fighting(vertices, collision=None):
    """[(area, centre, material_a, material_b)] for every overlapping,
    coplanar, same-facing pair of triangles with different materials. A pair
    pressed against an opposite-facing coplanar face (for example two boxes'
    bottoms sitting on a slab's top) is sealed from view and not reported, and
    so is a pair buried inside a solid, when `collision` (the collision soup
    as floats) is given."""
    v = np.asarray(vertices, np.float64).reshape(-1, 3, 12)
    return [(area, centre, int(v[i, 0, 10]), int(v[j, 0, 10]))
            for i, j, area, centre in overlap_pairs(vertices, collision, different_materials=True)]


def overlap_pairs(vertices, collision=None, different_materials=False):
    """[(i, j, area, centre)] for every visible overlapping, coplanar,
    same-facing pair of triangles (optionally only pairs of different
    materials), by triangle index. Same rules as `z_fighting`."""
    v = np.asarray(vertices, np.float64).reshape(-1, 3, 12)
    solids = None if collision is None else np.asarray(collision, np.float64).reshape(-1, 3, 3)
    p = v[:, :, :3]; mat = v[:, 0, 10]
    n = np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]); ln = np.linalg.norm(n, axis=1)
    ok = np.flatnonzero(ln > 2*MIN_AREA)
    unit = n[ok]/ln[ok, None]
    canon = unit.copy()
    flip = np.take_along_axis(canon, np.abs(canon).argmax(1)[:, None], 1)[:, 0] < 0
    canon[flip] *= -1
    dist = (canon*p[ok, 0]).sum(1)
    groups = {}
    for k, i in enumerate(ok):
        key = (tuple(np.round(canon[k], 3)), int(round(dist[k]/PLANE_EPS)))
        groups.setdefault(key, []).append((i, k))
    out = []
    for g in groups.values():
        if len(g) < 2: continue
        for a in range(len(g)):
            i, ki = g[a]
            axis = int(np.abs(canon[ki]).argmax()); keep = [c for c in range(3) if c != axis]
            for j, kj in g[a+1:]:
                if (different_materials and mat[i] == mat[j]) or unit[ki] @ unit[kj] < 0: continue
                # Buckets key on the absolute plane distance, so two parallel
                # faces centimetres apart can share one when float32 world
                # coordinates tilt their normals slightly (a kit turret's band
                # 7 cm off its collar did). Confirm true coplanarity.
                if abs(unit[ki] @ (p[j].mean(0)-p[i][0])) > PLANE_EPS: continue
                pi, pj = p[i][:, keep], p[j][:, keep]
                if _inside(pj.mean(0), pi) or _inside(pi.mean(0), pj):
                    small = pj if ln[j] < ln[i] else pi
                    sealed = any(unit[qk] @ unit[ki] < 0 and _inside(small.mean(0), p[q][:, keep], -1e-4) for q, qk in g)
                    if sealed: continue
                    centre = (p[j] if ln[j] < ln[i] else p[i]).mean(0)
                    if _embedded(centre+unit[ki]*.02, solids): continue
                    area = min(ln[i], ln[j])/2
                    out.append((i, j, round(float(area), 3), [round(float(c), 2) for c in p[i].mean(0)]))
    return sorted(out, key=lambda e: -e[2])
