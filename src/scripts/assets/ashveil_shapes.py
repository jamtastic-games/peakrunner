"""Ashveil's structures and formations: original procedural geometry.

Generic forms (a lathe, a swept profile, an extrusion, a crag, a tube, a
spindle, a sheet, an isosurface, ...) that the map's own modules build its
structures and rock from, and a recipe builder for lists of them.

A form is built in a piece's local frame (metres, local Z up), then placed by
a 3x4 matrix. Faces wind counter-clockwise seen from outside in a
right-handed frame; a placement that mirrors (negative determinant) reverses
them so the world winding stays outward.

Mesh: vertices (V, 3) and faces (F, 3) with one material and normal mode
per form. `place` returns world triangles (F, 3, 3) and vertex normals.
"""
import math

import numpy as np


class Form:
    """An indexed mesh in a local frame: one material, flat or smooth.
    `rim`, when given, is each vertex's share of the way out from a lava
    flow's middle (0) to its edge (1), for the shader's cooling crust."""

    def __init__(self, v, f, material, smooth=False, solid=True, rim=None):
        self.v = np.asarray(v, np.float64).reshape(-1, 3)
        self.f = np.asarray(f, np.int64).reshape(-1, 3)
        self.material, self.smooth, self.solid = material, smooth, solid
        self.rim = None if rim is None else np.asarray(rim, np.float64).reshape(-1)


def _rng(seed):
    return np.random.default_rng(seed & 0xffffffff)


# --- Polygon triangulation -------------------------------------------------

def _area2(p):
    x, y = p[:, 0], p[:, 1]
    return float(np.dot(x, np.roll(y, -1))-np.dot(np.roll(x, -1), y))


def triangulate(poly):
    """Ear clipping of a simple polygon [(x, y), ...] in either order.
    Returns index triples, counter-clockwise."""
    p = np.asarray(poly, np.float64)
    idx = list(range(len(p)))
    if _area2(p) < 0: idx.reverse()
    out = []
    guard = 0
    while len(idx) > 3 and guard < 10000:
        guard += 1
        n = len(idx)
        for k in range(n):
            a, b, c = idx[k-1], idx[k], idx[(k+1) % n]
            pa, pb, pc = p[a], p[b], p[c]
            cross = (pb[0]-pa[0])*(pc[1]-pa[1])-(pb[1]-pa[1])*(pc[0]-pa[0])
            if cross <= 1e-12: continue
            inside = False
            for j in idx:
                if j in (a, b, c): continue
                q = p[j]
                d1 = (pb[0]-pa[0])*(q[1]-pa[1])-(pb[1]-pa[1])*(q[0]-pa[0])
                d2 = (pc[0]-pb[0])*(q[1]-pb[1])-(pc[1]-pb[1])*(q[0]-pb[0])
                d3 = (pa[0]-pc[0])*(q[1]-pc[1])-(pa[1]-pc[1])*(q[0]-pc[0])
                if d1 >= -1e-12 and d2 >= -1e-12 and d3 >= -1e-12: inside = True; break
            if inside: continue
            out.append((a, b, c)); idx.pop(k); break
        else:
            # Degenerate remainder (collinear points): fan it.
            out += [(idx[0], idx[i], idx[i+1]) for i in range(1, len(idx)-1)]
            idx = idx[:3]
            break
    if len(idx) == 3: out.append(tuple(idx))
    return out


# --- Generic forms ---------------------------------------------------------

def lathe(profile, sides=24, arc=(0.0, 360.0), squareness=0.0, material='concrete', smooth=True,
          solid=True, center=(0.0, 0.0), scale=(1.0, 1.0), caps=True, rough=0.0, seed=0):
    """Revolve a profile [(r, z), ...] about local Z. An open profile runs
    from the axis (or a rim) up; a closed one (first == last) is a ring
    section. A partial `arc` (degrees from +X toward +Y) caps both ends
    unless `caps` is false. `squareness` 0..1 morphs the circle toward a
    rounded square; `rough` pushes the surface in and out with noise (a
    fraction of the radius)."""
    prof = [tuple(map(float, p)) for p in profile]
    closed = len(prof) > 2 and prof[0] == prof[-1]
    if closed: prof = prof[:-1]
    full = abs((arc[1]-arc[0])-360.0) < 1e-6
    steps = sides if full else max(1, int(math.ceil(sides*(arc[1]-arc[0])/360.0)))
    angles = np.radians(np.linspace(arc[0], arc[1], steps+1))[:steps if full else steps+1]
    rows = len(angles)
    v = []
    for a in angles:
        c, s = math.cos(a), math.sin(a)
        if squareness:
            k = 2+squareness*8
            m = (abs(c)**k+abs(s)**k)**(-1/k)
            c, s = c*m, s*m
        for r, z in prof: v.append((center[0]+r*c*scale[0], center[1]+r*s*scale[1], z))
    n = len(prof)
    f = []
    seg = n if closed else n-1
    for i in range(rows if full else rows-1):
        i2 = (i+1) % rows
        for j in range(seg):
            j2 = (j+1) % n
            a, b, c2, d = i*n+j, i2*n+j, i2*n+j2, i*n+j2
            f += [(a, b, c2), (a, c2, d)]
    v = np.array(v)
    if rough:
        r = np.hypot(v[:, 0]-center[0], v[:, 1]-center[1])
        d = rough*(fbm(v*.9, seed, 1.0)-.5)*2*r
        a = np.arctan2(v[:, 1]-center[1], v[:, 0]-center[0])
        v[:, 0] += d*np.cos(a); v[:, 1] += d*np.sin(a)
        v[:, 2] += rough*.5*(fbm(v*.9+13, seed+1, 1.0)-.5)*2*np.maximum(r, .5)
    form = Form(v, f, material, smooth, solid)
    if not full and closed and caps:
        # End caps: the profile polygon at each end.
        poly = [(r, z) for r, z in prof]
        tri = triangulate(poly)
        caps = []
        for i, flip in ((0, True), (rows-1, False)):
            for a, b, c in tri:
                t = (i*n+a, i*n+b, i*n+c)
                caps.append(t[::-1] if flip else t)
        form = merge([form, Form(v, caps, material, False, solid)])
    _orient(form)
    return form


def extrude(polygon, z0, z1, material='concrete', solid=True, top=None, axis='z'):
    """A prism: `polygon` [(a, b), ...] in the plane across `axis`, from z0
    to z1 along it. `top` (optional) is a plane (k0, ka, kb): the top face
    sits at k0 + ka*a + kb*b instead of z1 (a ramp)."""
    p = np.asarray(polygon, np.float64)
    if _area2(p) < 0: p = p[::-1]
    n = len(p)
    hi = np.full(n, float(z1)) if top is None else top[0]+top[1]*p[:, 0]+top[2]*p[:, 1]
    lo = np.full(n, float(z0))
    v = np.concatenate([np.c_[p, lo], np.c_[p, hi]])
    tri = triangulate(p)
    f = [(c, b, a) for a, b, c in tri]+[(a+n, b+n, c+n) for a, b, c in tri]
    for i in range(n):
        j = (i+1) % n
        f += [(i, j, j+n), (i, j+n, i+n)]
    form = Form(_axis(v, axis), f, material, False, solid)
    _orient(form)
    return form


def _axis(v, axis):
    """Map (a, b, h) with h along `axis` into (x, y, z): the polygon is in
    (x, y) for 'z', (y, z) for 'x' and (x, z) for 'y'."""
    if axis == 'z': return v
    if axis == 'x': return v[:, [2, 0, 1]]
    return v[:, [0, 2, 1]]


def box(lo, hi, material='concrete', solid=True):
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    return extrude([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], z0, z1, material, solid)


def slab(lo, hi, seed, rough=.12, cells=(6, 6), material='bark', solid=True):
    """A rough rock panel filling the box, thin along X: both broad faces
    are noisy grids, so rows of them make a cave wall."""
    lo, hi = np.asarray(lo, float), np.asarray(hi, float)
    ny, nz = cells
    ys = np.linspace(lo[1], hi[1], ny+1); zs = np.linspace(lo[2], hi[2], nz+1)
    Y, Z = np.meshgrid(ys, zs)                   # (nz+1, ny+1)
    depth = hi[0]-lo[0]
    v, f = [], []
    for side, x in ((0, hi[0]), (1, lo[0])):
        p = np.c_[np.full(Y.size, x), Y.ravel(), Z.ravel()]
        d = rough*depth*(fbm(p*.8+side*31, seed+side, 1.0)-.5)*2
        edge = (Y.ravel() == lo[1]) | (Y.ravel() == hi[1]) | (Z.ravel() == lo[2]) | (Z.ravel() == hi[2])
        p[:, 0] += np.where(edge, 0, d)
        base = len(v); v += p.tolist()
        for i in range(nz):
            for j in range(ny):
                a = base+i*(ny+1)+j; b = a+1; c = a+ny+2; dd = a+ny+1
                f += [(a, b, c), (a, c, dd)] if side == 0 else [(a, c, b), (a, dd, c)]
    n = (ny+1)*(nz+1)
    ring = [(0, j) for j in range(ny)]+[(i, ny) for i in range(nz)]+[(nz, j) for j in range(ny, 0, -1)]+[(i, 0) for i in range(nz, 0, -1)]
    for k in range(len(ring)):
        (i0, j0), (i1, j1) = ring[k], ring[(k+1) % len(ring)]
        a, b = i0*(ny+1)+j0, i1*(ny+1)+j1
        f += [(a, a+n, b+n), (a, b+n, b)]
    form = Form(v, f, material, False, solid)
    _orient(form)
    return form


def ellipsoid_grid(rings, sides):
    """Unit sphere as a latitude-longitude grid (poles shared)."""
    v = [(0.0, 0.0, -1.0)]
    for i in range(1, rings):
        th = math.pi*i/rings-math.pi/2
        for j in range(sides):
            ph = 2*math.pi*j/sides
            v.append((math.cos(th)*math.cos(ph), math.cos(th)*math.sin(ph), math.sin(th)))
    v.append((0.0, 0.0, 1.0))
    f = []
    top = len(v)-1
    for j in range(sides):
        f.append((0, 1+(j+1) % sides, 1+j))
    for i in range(rings-2):
        for j in range(sides):
            a = 1+i*sides+j; b = 1+i*sides+(j+1) % sides
            c = b+sides; d = a+sides
            f += [(a, b, c), (a, c, d)]
    base = 1+(rings-2)*sides
    for j in range(sides):
        f.append((base+j, base+(j+1) % sides, top))
    return np.array(v), f


def _noise3(p, seed, scale):
    """Smooth value noise in 3D (deterministic, vectorised)."""
    q = p/scale
    i = np.floor(q).astype(np.int64); t = q-i
    t = t*t*(3-2*t)
    def h(ix, iy, iz):
        n = (ix*73856093) ^ (iy*19349663) ^ (iz*83492791) ^ (seed*2654435761 & 0x7fffffff)
        n = (n ^ (n >> 13))*1274126177
        return ((n ^ (n >> 16)) & 0xffff)/65535.0
    out = 0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (t[:, 0] if dx else 1-t[:, 0])*(t[:, 1] if dy else 1-t[:, 1])*(t[:, 2] if dz else 1-t[:, 2])
                out = out+w*h(i[:, 0]+dx, i[:, 1]+dy, i[:, 2]+dz)
    return out


def fbm(p, seed, scale, octaves=3):
    total, amp, norm = 0, 1.0, 0
    for o in range(octaves):
        total = total+amp*_noise3(p, seed+o*101, scale/2**o)
        norm += amp; amp *= .5
    return total/norm


def blob(lo, hi, seed, rough=.18, rings=8, sides=12, material='bark', flat_bottom=None, lumps=.0, solid=True):
    """A boulder filling the box lo..hi: a noisy ellipsoid, faceted.
    `flat_bottom` (0..1 of the height) cuts the base flat."""
    lo, hi = np.asarray(lo, float), np.asarray(hi, float)
    c, r = (lo+hi)/2, (hi-lo)/2
    v, f = ellipsoid_grid(rings, sides)
    rng = _rng(seed)
    d = 1+rough*(fbm(v*2.0, seed, 1.0)-.5)*2+rough*.5*(rng.random(len(v))-.5)
    if lumps: d = d+lumps*(fbm(v*1.1+7, seed+5, 1.0)-.5)*2
    v = v*d[:, None]
    v = (v-v.min(0))/(v.max(0)-v.min(0))*2-1
    w = c+v*r
    if flat_bottom is not None:
        cut = lo[2]+(hi[2]-lo[2])*flat_bottom
        w[:, 2] = np.maximum(w[:, 2], cut)
    form = Form(w, f, material, False, solid)
    _orient(form)
    return form


def crag(lo, hi, seed, peaks=5, sharp=1.6, rough=.12, rings=10, sides=20, material='bark', solid=True,
         base=0.0, spread=.6, spike=(.35, .7)):
    """A jagged rock: a polar heightfield over the box's ellipse, the max of
    a few spikes (`spike` radii, out to `spread` from the centre), closed
    underneath at `base` (fraction of the height)."""
    lo, hi = np.asarray(lo, float), np.asarray(hi, float)
    c, r = (lo+hi)/2, (hi-lo)/2
    rng = _rng(seed)
    spikes = []
    for k in range(peaks):
        a = rng.uniform(0, 2*math.pi); d = rng.uniform(0, spread)*(0 if k == 0 else 1)
        spikes.append((math.cos(a)*d, math.sin(a)*d, 1.0 if k == 0 else rng.uniform(.45, .9), rng.uniform(*spike)))
    zb = lo[2]+(hi[2]-lo[2])*base
    v, f = [], []
    grid = []
    for i in range(rings+1):
        rho = i/rings
        row = []
        for j in range(sides):
            a = 2*math.pi*j/sides
            x, y = rho*math.cos(a), rho*math.sin(a)
            h = 0.0
            for sx, sy, sh, sr in spikes:
                dd = math.hypot(x-sx, y-sy)/sr
                h = max(h, sh*max(0.0, 1-dd)**sharp)
            h = max(h, .12*(1-rho**2))
            if i == rings: h = 0.0
            row.append(len(v)); v.append([x, y, h])
            if i == 0: break
        grid.append(row)
    v = np.array(v)
    v[:, :2] += rough*(np.c_[fbm(v*3+1, seed, 1.0), fbm(v*3+9, seed+3, 1.0)]-.5)*(v[:, 2:3]+.3)
    v[:, 2] *= 1+rough*(fbm(v*4+5, seed+7, 1.0)-.5)*2
    centre = grid[0][0]
    for j in range(sides):
        f.append((centre, grid[1][j], grid[1][(j+1) % sides]))
    for i in range(1, rings):
        for j in range(sides):
            a, b = grid[i][j], grid[i][(j+1) % sides]
            c2, d = grid[i+1][(j+1) % sides], grid[i+1][j]
            f += [(a, d, c2), (a, c2, b)]
    # Base cap at the rim.
    rim = grid[rings]
    bottom = len(v)
    v = np.vstack([v, [0, 0, 0]])
    for j in range(sides):
        f.append((bottom, rim[(j+1) % sides], rim[j]))
    w = np.empty_like(v)
    w[:, 0] = c[0]+v[:, 0]*r[0]; w[:, 1] = c[1]+v[:, 1]*r[1]
    w[:, 2] = zb+v[:, 2]*(hi[2]-zb)
    form = Form(w, f, material, False, solid)
    _orient(form)
    return form


def spindle(lo, hi, seed, sides=6, waist=.45, foot=.55, jitter=.12, material='leaf', solid=True, rings=5):
    """A shard or crystal: a faceted spindle along Z, widest at `waist`
    (fraction of the height), pointed at the top, `foot` wide at the bottom
    (0: pointed there too)."""
    lo, hi = np.asarray(lo, float), np.asarray(hi, float)
    c, r = (lo+hi)/2, (hi-lo)/2
    rng = _rng(seed)
    prof = []
    for i in range(rings+1):
        t = i/rings
        w = foot+(1-foot)*(t/waist)**.8 if t <= waist else 1-(t-waist)/(1-waist)
        prof.append((max(w, 0.0), t))
    v, f = [], []
    phase = rng.uniform(0, 2*math.pi)
    for i, (w, t) in enumerate(prof):
        for j in range(sides):
            a = phase+2*math.pi*j/sides+rng.uniform(-.25, .25)
            ww = w*(1+rng.uniform(-jitter, jitter))
            v.append((math.cos(a)*ww, math.sin(a)*ww, t))
    v = np.array(v)
    for i in range(rings):
        for j in range(sides):
            a = i*sides+j; b = i*sides+(j+1) % sides
            f += [(a, b, b+sides), (a, b+sides, a+sides)]
    bot = len(v); v = np.vstack([v, [0, 0, 0]])
    for j in range(sides): f.append((bot, (j+1) % sides, j))
    w = np.empty_like(v)
    w[:, 0] = c[0]+v[:, 0]*r[0]; w[:, 1] = c[1]+v[:, 1]*r[1]; w[:, 2] = lo[2]+v[:, 2]*(hi[2]-lo[2])
    form = Form(w, f, material, False, solid)
    _orient(form)
    return form


def tube(points, radii, sides=8, seed=0, rough=0.0, material='bark', smooth=False, solid=True, caps=True):
    """A tube along a 3D polyline with a radius per point, closed at both
    ends. `rough` displaces the rings with noise (rock limbs)."""
    pts = np.asarray(points, np.float64); radii = np.asarray(radii, np.float64)
    n = len(pts)
    tang = np.gradient(pts, axis=0)
    tang /= np.maximum(np.linalg.norm(tang, axis=1), 1e-9)[:, None]
    ref = np.array([0, 0, 1.0]) if abs(tang[0, 2]) < .9 else np.array([1.0, 0, 0])
    v = []
    nrm = np.cross(tang[0], ref); nrm /= np.linalg.norm(nrm)
    for i in range(n):
        t = tang[i]
        nrm = nrm-t*np.dot(nrm, t); nrm /= max(np.linalg.norm(nrm), 1e-9)
        b = np.cross(t, nrm)
        for j in range(sides):
            a = 2*math.pi*j/sides
            v.append(pts[i]+radii[i]*(math.cos(a)*nrm+math.sin(a)*b))
    v = np.array(v)
    if rough:
        v += rough*(np.c_[fbm(v*.7, seed, 1.0), fbm(v*.7+11, seed+1, 1.0), fbm(v*.7+23, seed+2, 1.0)]-.5)*2 \
            * np.repeat(radii, sides)[:, None]
    f = []
    for i in range(n-1):
        for j in range(sides):
            a = i*sides+j; b = i*sides+(j+1) % sides
            f += [(a, b+sides, b), (a, a+sides, b+sides)]
    if caps:
        s0 = len(v); e0 = s0+1
        v = np.vstack([v, pts[0], pts[-1]])
        for j in range(sides):
            f.append((s0, j, (j+1) % sides))
            f.append((e0, (n-1)*sides+(j+1) % sides, (n-1)*sides+j))
    form = Form(v, f, material, smooth, solid)
    _orient(form)
    return form


def sheet(lo, hi, seed, lobes=5, segments=28, rings=4, thick=None, material='water', solid=False):
    """A flat irregular pad (a lava pool or flow) inside the box footprint:
    a wobbling outline at the box's top, as a polar grid fine enough to
    drape over the ground (`assemble`'s `ground`), with a short rim."""
    lo, hi = np.asarray(lo, float), np.asarray(hi, float)
    c, r = (lo+hi)/2, (hi-lo)/2
    rng = _rng(seed)
    ph = rng.uniform(0, 2*math.pi, 3)
    a = np.linspace(0, 2*math.pi, segments, endpoint=False)
    w = .78+.12*np.sin(lobes*a+ph[0])+.07*np.sin((lobes+2)*a+ph[1])+.05*np.sin(3*a+ph[2])
    w = w/w.max()
    z1 = hi[2]; z0 = z1-(thick if thick else max(hi[2]-lo[2], .05))
    v = [(c[0], c[1], z1)]
    for i in range(1, rings+1):
        k = i/rings
        v += [(c[0]+np.cos(t)*ww*r[0]*k, c[1]+np.sin(t)*ww*r[1]*k, z1) for t, ww in zip(a, w)]
    rim = len(v)
    v += [(c[0]+np.cos(t)*ww*r[0], c[1]+np.sin(t)*ww*r[1], z0) for t, ww in zip(a, w)]
    f = []
    for j in range(segments):
        f.append((0, 1+j, 1+(j+1) % segments))
    for i in range(1, rings):
        o, n = 1+(i-1)*segments, 1+i*segments
        for j in range(segments):
            k = (j+1) % segments
            f += [(o+j, n+j, n+k), (o+j, n+k, o+k)]
    edge = 1+(rings-1)*segments
    for j in range(segments):
        k = (j+1) % segments
        f += [(edge+j, rim+j, rim+k), (edge+j, rim+k, edge+k)]
    # Open underneath: the grid faces up and the rim out.
    rim = [0.0]+[i/rings for i in range(1, rings+1) for _ in range(segments)]+[1.0]*segments
    return Form(v, f, material, False, solid, rim)


def merge(forms):
    """One form from several of the same material (normals per face)."""
    v, f, off = [], [], 0
    for fm in forms:
        v.append(fm.v); f.append(fm.f+off); off += len(fm.v)
    out = Form(np.concatenate(v), np.concatenate(f), forms[0].material, forms[0].smooth, forms[0].solid)
    return out


def _orient(form):
    """Make the faces of a closed form wind outward: flip all if the signed
    volume is negative."""
    t = form.v[form.f]
    vol = np.einsum('ij,ij->i', t[:, 0], np.cross(t[:, 1], t[:, 2])).sum()
    if vol < 0: form.f = form.f[:, ::-1].copy()


# --- Signed distance forms ---------------------------------------------------

def isosurface(field, lo, hi, cell, material='glacier', smooth=True, solid=True):
    """The surface where `field` (points (N, 3) -> signed distances,
    negative inside) crosses zero inside the box lo..hi, as quads on a
    `cell` grid (surface nets: one vertex per cell the surface passes
    through, at the mean of its edge crossings). The box's boundary counts
    as outside, so the surface is always closed. Each quad's two triangles
    are consecutive, (a, b, c) then (a, c, d), and wind outward."""
    lo, hi = np.asarray(lo, np.float64), np.asarray(hi, np.float64)
    n = np.ceil((hi-lo)/cell).astype(int)+1
    axes = [lo[i]+np.arange(n[i])*cell for i in range(3)]
    grid = np.stack(np.meshgrid(*axes, indexing='ij'), -1)
    d = np.asarray(field(grid.reshape(-1, 3)), np.float64).reshape(tuple(n))
    d[0], d[-1], d[:, 0], d[:, -1], d[:, :, 0], d[:, :, -1] = (cell,)*6
    d = np.where(d == 0, 1e-9, d)
    cells = tuple(n-1)
    total = np.zeros(cells+(3,)); count = np.zeros(cells)
    crossings = []
    for axis in range(3):
        a = [slice(None)]*3; b = [slice(None)]*3
        a[axis] = slice(0, -1); b[axis] = slice(1, None)
        d0, d1 = d[tuple(a)], d[tuple(b)]
        cut = (d0 < 0) != (d1 < 0)
        idx = np.argwhere(cut)
        t = d0[cut]/(d0[cut]-d1[cut])
        p = grid[tuple(a)][cut].copy(); p[:, axis] += t*cell
        # The four cells round each edge: offsets 0/-1 on the other axes.
        u, v = [k for k in range(3) if k != axis]
        for du in (0, -1):
            for dv in (0, -1):
                c = idx.copy(); c[:, u] += du; c[:, v] += dv
                ok = (c >= 0).all(1) & (c < np.array(cells)).all(1)
                np.add.at(total, tuple(c[ok].T), p[ok]); np.add.at(count, tuple(c[ok].T), 1)
        crossings.append((axis, idx, d0[cut] < 0))
    active = count > 0
    lookup = np.full(cells, -1, np.int64); lookup[active] = np.arange(active.sum())
    v = total[active]/count[active][:, None]
    faces = []
    for axis, idx, inside_low in crossings:
        u, w = [(1, 2), (2, 0), (0, 1)][axis]
        ring = []
        for du, dw in ((-1, -1), (0, -1), (0, 0), (-1, 0)):
            c = idx.copy(); c[:, u] += du; c[:, w] += dw
            ring.append(c)
        ok = np.all([(c >= 0).all(1) & (c < np.array(cells)).all(1) for c in ring], 0)
        q = np.stack([lookup[tuple(c[ok].T)] for c in ring], 1)
        q = np.where(inside_low[ok][:, None], q, q[:, ::-1])
        q = q[(q >= 0).all(1)]
        # Split each quad along its shorter diagonal (fewer folds).
        long_ = np.linalg.norm(v[q[:, 0]]-v[q[:, 2]], axis=1) > np.linalg.norm(v[q[:, 1]]-v[q[:, 3]], axis=1)
        q = np.where(long_[:, None], q[:, [1, 2, 3, 0]], q)
        faces.append(np.stack([q[:, [0, 1, 2]], q[:, [0, 2, 3]]], 1).reshape(-1, 3))
    f = np.concatenate(faces) if faces else np.zeros((0, 3), np.int64)
    return Form(v, f, material, smooth, solid)


def sd_ellipsoid(p, c, r):
    """Signed distance (a close bound) to an ellipsoid."""
    r = np.asarray(r, np.float64)
    q = (p-np.asarray(c, np.float64))/r
    k0 = np.linalg.norm(q, axis=-1); k1 = np.linalg.norm(q/r, axis=-1)
    return k0*(k0-1)/np.maximum(k1, 1e-9)


def sd_box(p, c, half, round_=0.0):
    q = np.abs(p-np.asarray(c, np.float64))-(np.asarray(half, np.float64)-round_)
    return np.linalg.norm(np.maximum(q, 0), axis=-1)+np.minimum(q.max(-1), 0)-round_


def sd_capsule(p, a, b, r):
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    pa, ba = p-a, b-a
    h = np.clip((pa@ba)/max(ba@ba, 1e-12), 0, 1)
    return np.linalg.norm(pa-h[..., None]*ba, axis=-1)-r


def smin(a, b, k):
    """Smooth union (polynomial, blend width k)."""
    if k <= 0: return np.minimum(a, b)
    h = np.clip(.5+.5*(b-a)/k, 0, 1)
    return b*(1-h)+a*h-k*h*(1-h)


def smax(a, b, k):
    """Smooth intersection; smax(a, -b, k) carves b out of a."""
    return -smin(-a, -b, k)


# --- Recipes ---------------------------------------------------------------

FORMS = {'lathe': lathe, 'extrude': extrude, 'box': box, 'blob': blob, 'crag': crag, 'spindle': spindle,
         'tube': tube, 'sheet': sheet, 'slab': slab}
SEEDED = ('blob', 'crag', 'spindle', 'sheet', 'tube', 'slab', 'lathe')
LIT = ('baked', 'live', 'lava', 'flow')


def build(recipe, seed):
    """The forms of one kind from its recipe {"lit": mode, "parts": [...]};
    modes: "baked" (lightmapped), "live", "lava" and "flow" (cooler lava,
    heat):
    each part {"form": name, ...arguments}, optionally "lit" (its own
    mode), "repeat", "mirror" and "stretch" ([sx, sy, sz] about the local
    origin). Returns (form, lit) pairs; seeds are offset per part."""
    out = []
    default = recipe.get('lit', 'live')
    for k, part in enumerate(recipe['parts']):
        args = {key: value for key, value in part.items() if key not in ('form', 'mirror', 'repeat', 'stretch', 'lit')}
        form = part['form']
        if form in SEEDED and form != 'lathe': args.setdefault('seed', seed+k*7919)
        if form == 'lathe' and args.get('rough'): args.setdefault('seed', seed+k*7919)
        lit = part.get('lit', default)
        if lit not in LIT: raise ValueError(f'unknown light mode {lit}')
        made = FORMS[form](**args)
        if 'stretch' in part: made.v = made.v*np.asarray(part['stretch'], np.float64)
        copies = [made]
        # `repeat`: [count, degrees] copies turned about local Z.
        if 'repeat' in part:
            count, step = part['repeat']
            for i in range(1, count):
                a = math.radians(step*i); c, s = math.cos(a), math.sin(a)
                v = made.v.copy(); v[:, 0], v[:, 1] = made.v[:, 0]*c-made.v[:, 1]*s, made.v[:, 0]*s+made.v[:, 1]*c
                copies.append(Form(v, made.f, made.material, made.smooth, made.solid, made.rim))
        # `mirror`: "x" or "y" adds a reflected copy.
        if 'mirror' in part:
            ax = 'xyz'.index(part['mirror'])
            for fm in list(copies):
                v = fm.v.copy(); v[:, ax] *= -1
                copies.append(Form(v, fm.f[:, ::-1], fm.material, fm.smooth, fm.solid, fm.rim))
        out += [(c, lit) for c in copies]
    return out


def place(form, m, corners=False):
    """World triangles (F, 3, 3) and vertex normals (F, 3, 3) of a form
    under a 3x4 layout matrix; with `corners`, also each triangle's vertex
    indices into the form (F, 3)."""
    a = np.asarray(m, np.float64)[:, :3]; t = np.asarray(m, np.float64)[:, 3]
    w = form.v@a.T+t
    f = form.f
    if np.linalg.det(a) < 0:
        # Mirrored: reverse each face but keep its first corner, and swap
        # each pair, so a quad's two halves stay (a, b, c), (a, c, d) for
        # the lightmap baker's quad charts.
        f = f[:, [0, 2, 1]]
        order = np.arange(len(f))
        pairs = len(f)//2*2
        order[0:pairs:2] += 1; order[1:pairs:2] -= 1
        f = f[order]
    tris = w[f]
    fn = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    fn /= np.maximum(np.linalg.norm(fn, axis=1), 1e-12)[:, None]
    if form.smooth:
        vn = np.zeros_like(w)
        for k in range(3): np.add.at(vn, f[:, k], fn)
        vn /= np.maximum(np.linalg.norm(vn, axis=1), 1e-12)[:, None]
        n = vn[f]
        # Keep hard edges hard: a vertex normal far from its face's is replaced.
        bad = np.einsum('fkj,fj->fk', n, fn) < .5
        n = np.where(bad[..., None], fn[:, None, :], n)
    else:
        n = np.repeat(fn[:, None, :], 3, 1)
    keep = np.linalg.norm(np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0]), axis=1) > 1e-8
    return (tris[keep], n[keep], f[keep]) if corners else (tris[keep], n[keep])


def records(tris, normals, material_index, light=-1.0, rim=None):
    """Kit render vertices (12 floats each): position, normal, the kit's
    planar uv (metres / 4 on the face's two widest axes), lightmap uv,
    material, light layer. A lava flow's `rim` (F, 3) goes in the lightmap
    uv's x as 1 + rim (lava has no lightmap; 0 there means no rim)."""
    fn = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    axis = np.argmax(np.abs(fn), axis=1)
    pick = np.array([(2, 1), (0, 2), (0, 1)])[axis]
    out = np.zeros((len(tris), 3, 12), np.float32)
    out[..., :3] = tris; out[..., 3:6] = normals
    for k in range(3):
        out[:, k, 6] = tris[np.arange(len(tris)), k, pick[:, 0]]/4
        out[:, k, 7] = tris[np.arange(len(tris)), k, pick[:, 1]]/4
    out[..., 10] = material_index; out[..., 11] = light
    if rim is not None: out[..., 8] = 1+rim
    return out


# --- Placing on the ground ---------------------------------------------------

def drape(tris, ground, lift, reach):
    """Triangles with each corner within `reach` of the ground set `lift`
    over it (a lava flow lying on the terrain), and their flat normals."""
    t = tris.copy()
    g = ground(t[..., 0], t[..., 2])
    near = np.abs(t[..., 1]-g) < reach
    t[..., 1] = np.where(near, g+lift, t[..., 1])
    fn = np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0])
    fn /= np.maximum(np.linalg.norm(fn, axis=1), 1e-12)[:, None]
    return t, np.repeat(fn[:, None, :], 3, 1)
