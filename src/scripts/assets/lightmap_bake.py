"""Offline lightmap bake for original map packs (numpy, deterministic).

Input is the kit's render vertex array (12 floats per vertex: pos3, normal3,
uv2, lmuv2, material, light-layer). Every triangle, or every quad pair the kit
emitted as two consecutive triangles, gets its own padded cell in a 256x256
lightmap page. Each texel stores

    sky/ground ambient * ambient occlusion
  + sun * max(N.S, 0) * sun visibility
  + sum over lamp samples of power * color * N.L * falloff * visibility

and the vertex's light-layer index is pointed at the page, which selects the
map shader's baked path (`layer.y >= 0`: light = max(0.22, lightmap)). The
sun direction must be the one the renderer uses for the terrain so shadows
agree with it. A lamp is ((x, y, z), power), in the cool white `LAMP`, or
((x, y, z), power, (r, g, b)) in a color of its own. Optional ambient zones,
((x, z), (r0, r1), (a0, a1) or None, (y0, y1), (r, g, b)), scale the sky and
ground ambient inside an upright ring sector round (x, z) (angles in radians
from +z toward +x, a0 < a1, any turn), the last zone holding a point
deciding: a big room's ambient rays mostly travel farther than `AO_RANGE` and
count as open sky, so a closed room can be darkened (and tinted) for its own
lamps to light. Kit windings are not consistent (some quads face inward), so
each chart first probes both sides and bakes the more open one.

Results do not depend on process count or scheduling: every chart's random
ray rotation is seeded from its own index.
"""
import math
import os
from multiprocessing import get_context

import numpy as np

PAGE = 256
SKY = np.array([.56, .60, .66], np.float32)
GROUND = np.array([.34, .32, .30], np.float32)
SUN = np.array([.70, .65, .55], np.float32)
LAMP = np.array([.92, .98, 1.0], np.float32)
AO_RANGE = 4.0
AO_RAYS = 16
LAMP_RANGE = 11.0
EPS = .03

_G = {}


def _init(occ, lamps, sun, zones=()):
    v0 = occ[:, 0]; e1 = occ[:, 1]-v0; e2 = occ[:, 2]-v0
    _G.update(v0=v0, e1=e1, e2=e2, lo=occ.min(1), hi=occ.max(1), lamps=lamps, sun=sun, zones=zones)
    u = np.cross(sun, [0., 1., 0.] if abs(sun[1]) < .9 else [1., 0., 0.]); u /= np.linalg.norm(u)
    w = np.cross(sun, u)
    proj = np.stack([occ @ u, occ @ w, occ @ sun], -1)   # (T,3 verts,3)
    _G.update(su=u, sw=w, plo=proj.min(1), phi=proj.max(1))
    # Cosine-weighted, 4x4 stratified hemisphere (z up).
    k = int(math.sqrt(AO_RAYS)); a = (np.arange(k)+.5)/k
    r1, r2 = np.meshgrid(a, a); r1, r2 = r1.ravel(), r2.ravel()
    phi = 2*math.pi*r1; rad = np.sqrt(r2)
    _G['hemi'] = np.stack([rad*np.cos(phi), rad*np.sin(phi), np.sqrt(1-r2)], -1).astype(np.float32)


def _trace(orig, dirs, tmax, idx, nearest=False):
    """Moller-Trumbore of rays against candidate occluders `idx`.
    Returns a hit mask, or the nearest hit distance (inf when none)."""
    out = np.full(len(orig), np.inf if nearest else False, np.float32 if nearest else bool)
    if len(idx) == 0 or len(orig) == 0: return out
    v0, e1, e2 = _G['v0'][idx], _G['e1'][idx], _G['e2'][idx]
    step = max(1, 400_000//len(idx))
    for s in range(0, len(orig), step):
        o = orig[s:s+step, None, :]; d = dirs[s:s+step, None, :]
        p = np.cross(d, e2); det = (e1*p).sum(-1)
        ok = np.abs(det) > 1e-9
        inv = np.where(ok, 1/np.where(ok, det, 1), 0)
        tv = o-v0; u = (tv*p).sum(-1)*inv
        q = np.cross(tv, e1); v = (d*q).sum(-1)*inv; t = (e2*q).sum(-1)*inv
        hit = ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > 1e-4) & (t < tmax[s:s+step, None])
        if nearest: out[s:s+step] = np.where(hit, t, np.inf).min(1)
        else: out[s:s+step] = hit.any(1)
    return out


def _box_cand(lo, hi):
    return np.nonzero(np.all(_G['hi'] >= lo, 1) & np.all(_G['lo'] <= hi, 1))[0]


def _frame(n):
    t = np.cross(n, [0., 1., 0.] if abs(n[1]) < .9 else [1., 0., 0.]); t /= np.linalg.norm(t)
    return t, np.cross(n, t)


def _hemisphere(n, count, rng):
    t, b = _frame(n); ang = rng.uniform(0, 2*math.pi, count)
    c, s = np.cos(ang)[:, None, None], np.sin(ang)[:, None, None]
    h = _G['hemi'][None]
    rt = c*t+s*b; rb = -s*t+c*b
    return (h[..., :1]*rt+h[..., 1:2]*rb+h[..., 2:3]*n).astype(np.float32)   # (count,K,3)


def _chart(job):
    index, pts, normal = job
    rng = np.random.default_rng(1_000_003+index)
    lo = pts.min((0, 1)) if pts.ndim == 3 else pts.min(0)
    hi = pts.max((0, 1)) if pts.ndim == 3 else pts.max(0)
    grid = pts.reshape(-1, 3).astype(np.float32)
    cand = _box_cand(lo-AO_RANGE, hi+AO_RANGE)
    # Probe which side of the surface is the open, visible one.
    probe = grid[rng.choice(len(grid), min(len(grid), 6), replace=False)]
    score = []
    for side in (1, -1):
        n = normal*side; dirs = _hemisphere(n, len(probe), rng)
        o = np.repeat(probe+n*EPS, AO_RAYS, 0); d = dirs.reshape(-1, 3)
        score.append(1-_trace(o, d, np.full(len(o), 2.5, np.float32), cand).mean())
    if abs(score[0]-score[1]) < .05:
        # Open on both sides (thin decoration or a lone plate): light the side
        # that faces up, then the side that faces the sun.
        key = normal[1]*4+float(normal @ _G['sun'])
        n = normal if key >= 0 else -normal
    else:
        n = normal if score[0] > score[1] else -normal
    n = n.astype(np.float32)
    orig = grid+n*EPS
    # Ambient occlusion with distance falloff.
    dirs = _hemisphere(n, len(grid), rng).reshape(-1, 3)
    o = np.repeat(orig, AO_RAYS, 0)
    t = _trace(o, dirs, np.full(len(o), AO_RANGE, np.float32), cand, nearest=True).reshape(len(grid), AO_RAYS)
    ao = 1-np.where(np.isfinite(t), 1-t/AO_RANGE, 0).mean(1)
    up = .5+.5*n[1]
    light = np.outer(ao, GROUND*(1-up)+SKY*up)
    if _G['zones']:
        scale = np.ones((len(grid), 3), np.float32)
        for (zx, zz), (r0, r1), arc, (y0, y1), tint in _G['zones']:
            dx, dz = grid[:, 0]-zx, grid[:, 2]-zz
            r = np.hypot(dx, dz)
            inside = (r >= r0) & (r < r1) & (grid[:, 1] >= y0) & (grid[:, 1] < y1)
            if arc is not None:
                inside &= (np.arctan2(dx, dz)-arc[0]) % (2*math.pi) < arc[1]-arc[0]
            scale[inside] = tint
        light *= scale
    # Sun.
    sun = _G['sun']; ns = float(n @ sun)
    if ns > 0:
        plo = np.array([grid @ _G['su'], grid @ _G['sw']]).min(1)-.05
        phi = np.array([grid @ _G['su'], grid @ _G['sw']]).max(1)+.05
        smin = float((orig @ sun).min())
        sc = np.nonzero((_G['phi'][:, 0] >= plo[0]) & (_G['plo'][:, 0] <= phi[0]) &
                        (_G['phi'][:, 1] >= plo[1]) & (_G['plo'][:, 1] <= phi[1]) & (_G['phi'][:, 2] > smin))[0]
        blocked = _trace(orig, np.tile(sun, (len(orig), 1)).astype(np.float32), np.full(len(orig), 1e4, np.float32), sc)
        light += np.outer(np.where(blocked, 0, ns), SUN)
    # Lamps.
    lamps = _G['lamps']
    if len(lamps):
        lp, lw, lc = lamps[:, :3], lamps[:, 3], lamps[:, 4:7]
        near = np.nonzero(np.linalg.norm(np.clip(lp, lo, hi)-lp, axis=1) < LAMP_RANGE)[0]
        for k in near:
            d = lp[k]-orig; dist = np.linalg.norm(d, axis=1); dn = d/np.maximum(dist, 1e-6)[:, None]
            cos = dn @ n
            fall = 1/(1+(dist/3.5)**2)*np.clip(1-(dist/LAMP_RANGE)**2, 0, 1)**2
            w = lw[k]*np.clip(cos, 0, 1)*(.2+.8*np.clip(dn[:, 1], 0, 1))*fall
            live = np.nonzero(w > .004)[0]
            if not len(live): continue
            c = _box_cand(np.minimum(orig[live].min(0), lp[k])-.1, np.maximum(orig[live].max(0), lp[k])+.1)
            blocked = _trace(orig[live], dn[live].astype(np.float32), (dist[live]-.08).astype(np.float32), c)
            add = np.zeros(len(orig), np.float32); add[live] = np.where(blocked, 0, w[live])
            light += np.outer(add, lc[k])
    return index, light.reshape(pts.shape[:-1]+(3,)).astype(np.float32)


def _charts(tris, light_index, texel):
    """Group triangles into quad or single-triangle charts."""
    charts, i, n = [], 0, len(tris)
    while i < n:
        a = tris[i]
        if a[0, 10] == light_index:
            charts.append(('light', [i])); i += 1; continue
        if i+1 < n:
            b = tris[i+1]
            if (np.array_equal(b[0, :3], a[0, :3]) and np.array_equal(b[1, :3], a[2, :3])
                    and b[0, 10] == a[0, 10] and np.allclose(b[0, 3:6], a[0, 3:6], atol=1e-5)):
                charts.append(('quad', [i, i+1])); i += 2; continue
        charts.append(('tri', [i])); i += 1
    return charts


def _dims(kind, p, texel):
    if kind == 'quad':
        a, b, c, d = p
        w = max(np.linalg.norm(b-a), np.linalg.norm(c-d)); h = max(np.linalg.norm(d-a), np.linalg.norm(c-b))
    else:
        a, b, c = p
        w = np.linalg.norm(b-a); h = np.linalg.norm(c-a)
    clamp = lambda v: int(min(60, max(2, math.ceil(v/texel))))
    return clamp(w), clamp(h)


def _texel_points(kind, p, w, h):
    s = (np.arange(w)+.5)/w; t = (np.arange(h)+.5)/h
    S, T = np.meshgrid(s, t)            # (h,w): row = t, col = s
    S, T = S[..., None], T[..., None]
    if kind == 'quad':
        a, b, c, d = p
        lower = a*(1-S)+b*(S-T)+c*T      # triangle a,b,c: t <= s
        upper = a*(1-T)+c*S+d*(T-S)      # triangle a,c,d: t >= s
        return np.where(T <= S, lower, upper)
    a, b, c = p
    k = np.maximum(S+T, 1)               # clamp outside texels onto the hypotenuse
    return a*(1-S/k-T/k)+b*S/k+c*T/k


def bake(vertices, lamps, sun, light_index, layer_base, texel=.5, processes=None, zones=()):
    """Return (new vertex array, list of RGBA8 page bytes)."""
    verts = np.asarray(vertices, np.float32).reshape(-1, 12).copy()
    tris = verts.reshape(-1, 3, 12)
    sun = np.asarray(sun, np.float64); sun /= np.linalg.norm(sun)
    occ = tris[tris[:, 0, 10] != light_index][:, :, :3].astype(np.float32)
    lamp_arr = np.array([[*lamp[0], lamp[1], *(lamp[2] if len(lamp) > 2 else LAMP)] for lamp in lamps], np.float32).reshape(-1, 7)
    charts = _charts(tris, light_index, texel)
    # Layout: 4x4 white block for emissive surfaces at page 0 origin, then
    # shelf-pack every chart's padded cell, tallest first.
    jobs, cells = [], []
    for ci, (kind, ids) in enumerate(charts):
        if kind == 'light': continue
        t0 = tris[ids[0]]
        p = [t0[0, :3], t0[1, :3], t0[2, :3]] + ([tris[ids[1]][2, :3]] if kind == 'quad' else [])
        p = [x.astype(np.float64) for x in p]
        w, h = _dims(kind, p, texel)
        cells.append((ci, kind, p, w, h, t0[0, 3:6].astype(np.float64)))
    order = sorted(range(len(cells)), key=lambda k: (-cells[k][4], -cells[k][3], k))
    place, page, x, y, row = {}, 0, 4, 0, 4
    for k in order:
        _, _, _, w, h, _ = cells[k]; cw, ch = w+2, h+2
        if x+cw > PAGE: x, y, row = 0, y+row, 0
        if y+ch > PAGE: page, x, y, row = page+1, 0, 0, 0
        place[k] = (page, x, y); x += cw; row = max(row, ch)
    pages = np.zeros((page+1, PAGE, PAGE, 3), np.float32)
    pages[0, :4, :4] = 1
    for k, (ci, kind, p, w, h, n) in enumerate(cells):
        jobs.append((k, _texel_points(kind, p, w, h), n/np.linalg.norm(n)))
    ctx = get_context('spawn')
    procs = processes or max(1, (os.cpu_count() or 2)-1)
    with ctx.Pool(procs, initializer=_init, initargs=(occ, lamp_arr, sun, tuple(zones))) as pool:
        results = dict(pool.imap_unordered(_chart, jobs, chunksize=16))
    for k, (ci, kind, p, w, h, n) in enumerate(cells):
        img = results[k]
        # 3x3 blur inside the chart, then dilate one texel into the padding.
        pad = np.pad(img, ((1, 1), (1, 1), (0, 0)), mode='edge')
        img = sum(pad[dy:dy+h, dx:dx+w] for dy in range(3) for dx in range(3))/9
        pg, x0, y0 = place[k]
        pages[pg, y0:y0+h+2, x0:x0+w+2] = np.pad(img, ((1, 1), (1, 1), (0, 0)), mode='edge')
        # Vertex lightmap coordinates: parameter corners -> cell interior.
        ids = charts[ci][1]
        corner = {'quad': [[(0, 0), (1, 0), (1, 1)], [(0, 0), (1, 1), (0, 1)]], 'tri': [[(0, 0), (1, 0), (0, 1)]]}[kind]
        for tri_id, uvs in zip(ids, corner):
            for vi, (s, t) in enumerate(uvs):
                row = tri_id*3+vi
                verts[row, 8] = (x0+1+s*w)/PAGE; verts[row, 9] = (y0+1+t*h)/PAGE
                verts[row, 11] = layer_base+pg
    for ci, (kind, ids) in enumerate(charts):
        if kind != 'light': continue
        for vi in range(3):
            row = ids[0]*3+vi
            verts[row, 8] = verts[row, 9] = 2/PAGE; verts[row, 11] = layer_base
    rgba = []
    for pg in pages:
        a = np.empty((PAGE, PAGE, 4), np.uint8)
        a[..., :3] = np.clip(np.round(pg*255), 0, 255); a[..., 3] = 255
        rgba.append(a.tobytes())
    return verts, rgba
