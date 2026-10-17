"""Ashveil's ring: two lava valleys sweeping round the caldera's sides from
the skull tower, and the tower's site (our own design, docs/ashveil.md;
`maps/ashveil.json` "ring").

`shape` carves them into the caldera's ground and levels the tower's site;
corners marked `protect` are left as they are.

A valley is a spline of points: a flat floor `floor` m either side of it
`depth` m under the ground along it, walls rising at `inner` degrees on
the side toward the temple and `outer` toward the fence, and a hot lava
river meandering down the floor in a shallow channel, with cooled crust
along its banks. The floor meets the tower's moat at its start and climbs
to the ground over its last `ramp` m. Rock arches span the river, fords
cross it dry, and crags and shards stand on the walls.
"""
import math

import numpy as np

from assets import ashveil_shapes as A

N = 256


def spline(points, step=1.0):
    """Centripetal-ish Catmull-Rom through `points` [(x, z)], sampled about
    every `step` m: (positions (n, 2), unit tangents (n, 2), distance along)."""
    p = np.asarray(points, np.float64)
    p = np.vstack([2*p[0]-p[1], p, 2*p[-1]-p[-2]])
    out = []
    for i in range(1, len(p)-2):
        a, b, c, d = p[i-1], p[i], p[i+1], p[i+2]
        n = max(2, int(np.linalg.norm(c-b)/step))
        t = np.linspace(0, 1, n, endpoint=False)[:, None]
        out.append(.5*((2*b)+(-a+c)*t+(2*a-5*b+4*c-d)*t**2+(-a+3*b-3*c+d)*t**3))
    out.append(p[-2][None])
    q = np.concatenate(out)
    seg = np.linalg.norm(np.diff(q, axis=0), axis=1)
    s = np.r_[0, np.cumsum(seg)]
    tan = np.gradient(q, axis=0)
    tan /= np.maximum(np.linalg.norm(tan, axis=1), 1e-9)[:, None]
    return q, tan, s


def smooth(v, window):
    """Moving average over `window` samples, edges held."""
    w = max(1, int(window)) | 1
    pad = np.pad(v, w//2, mode='edge')
    return np.convolve(pad, np.ones(w)/w, mode='valid')


def nearest(points, q, tan, reach):
    """For each point (M, 2): the nearest sample of a spline, the signed
    lateral offset (+ to the left of travel) and the along offset. Points
    farther than `reach` from every sample get index -1."""
    best = np.full(len(points), np.inf); idx = np.full(len(points), -1)
    lo, hi = q.min(0)-reach, q.max(0)+reach
    near = np.nonzero(((points >= lo) & (points <= hi)).all(1))[0]
    sub = points[near]
    for k in range(0, len(q), 64):
        d2 = ((sub[:, None, :]-q[None, k:k+64])**2).sum(-1)
        j = d2.argmin(1); v = d2[np.arange(len(sub)), j]
        better = v < best[near]
        best[near[better]] = v[better]; idx[near[better]] = j[better]+k
    idx[best > reach*reach] = -1
    ok = idx >= 0
    rel = np.zeros_like(points); rel[ok] = points[ok]-q[idx[ok]]
    t = tan[np.maximum(idx, 0)]
    lateral = rel[:, 0]*-t[:, 1]+rel[:, 1]*t[:, 0]
    along = (rel*t).sum(1)
    return idx, lateral, along


def dilate(mask, steps):
    """A boolean mask grown by `steps` cells (8-connected)."""
    m = mask.copy()
    for _ in range(steps):
        g = m.copy()
        g[1:] |= m[:-1]; g[:-1] |= m[1:]; g[:, 1:] |= m[:, :-1]; g[:, :-1] |= m[:, 1:]
        g[1:, 1:] |= m[:-1, :-1]; g[:-1, :-1] |= m[1:, 1:]; g[1:, :-1] |= m[:-1, 1:]; g[:-1, 1:] |= m[1:, :-1]
        m = g
    return m


def falloff(mask, steps):
    """1 on `mask`, falling to 0 over `steps` cells round it."""
    w = mask.astype(np.float64); m = mask.copy()
    for k in range(1, steps+1):
        g = dilate(m, 1)
        w[g & ~m] = 1-k/(steps+1)
        m = g
    return w


class Valley:
    """One valley's centreline, floor heights and river, from its spec."""

    def __init__(self, spec, ring, grid, step, start_floor, centre):
        self.name = spec['name']
        self.q, self.tan, self.s = spline(spec['points'])
        self.length = self.s[-1]
        self.ring = ring
        r = ring
        # Ground along the centreline (the lowest within a few metres), less
        # the depth, smoothed; it meets the moat's bank at the start and the
        # ground over the last `ramp` m.
        ground = np.array([min(_sample(grid, step, x+dx, z+dz) for dx, dz in ((0, 0), (6, 0), (-6, 0), (0, 6), (0, -6)))
                           for x, z in self.q])
        # The floor keeps to the low ground: never steeper along the valley
        # than `grade`, so it cuts gorges through the hills.
        raw = smooth(ground, 21)-r['depth']
        floor = raw.copy()
        ds = np.r_[0, np.diff(self.s)]
        g = math.tan(math.radians(r['grade']))
        for k in range(1, len(floor)): floor[k] = min(floor[k], floor[k-1]+g*ds[k])
        for k in range(len(floor)-2, -1, -1): floor[k] = min(floor[k], floor[k+1]+g*ds[k+1])
        floor = np.maximum(floor, r['min_floor'])
        start = np.clip(1-self.s/r['lead'], 0, 1)
        floor = floor*(1-start)+start_floor*start
        end = np.clip((self.s-(self.length-r['ramp']))/r['ramp'], 0, 1)
        end = end*end*(3-2*end)
        floor = floor*(1-end)+smooth(ground, 15)*end
        self.floor = smooth(floor, 41)
        self.end = end
        # Which side faces the temple: + lateral is left of travel.
        left = np.c_[-self.tan[:, 1], self.tan[:, 0]]
        self.inner_left = ((np.asarray(centre)-self.q)*left).sum(1) > 0
        # The river wanders across the floor, narrows and widens, and stops
        # at the fords.
        rng = np.random.default_rng(spec.get('seed', 1))
        ph = rng.uniform(0, math.tau, 4)
        s = self.s
        self.meander = (r['meander']*(np.sin(s/37+ph[0])*.65+np.sin(s/17+ph[1])*.35))*(1-start)*(1-end)
        self.half = r['river']*(1+.25*np.sin(s/29+ph[2])+.12*np.sin(s/11+ph[3]))
        wet = s < self.length-r['ramp']*.8
        for f in spec.get('fords', []):
            wet &= np.abs(s-f*self.length) > r['ford']/2
        self.wet = wet
        self.arches = spec.get('arches', [])

    def river_point(self, i, lateral):
        """World (x, z) at sample i, `lateral` m left of the river's line."""
        left = np.array([-self.tan[i, 1], self.tan[i, 0]])
        return self.q[i]+left*(self.meander[i]+lateral)


def _sample(grid, step, x, z):
    fx, fz = np.clip(x/step, 0, N-1.001), np.clip(z/step, 0, N-1.001)
    x0, z0 = int(fx), int(fz); tx, tz = fx-x0, fz-z0
    return (grid[z0, x0]*(1-tx)+grid[z0, x0+1]*tx)*(1-tz)+(grid[z0+1, x0]*(1-tx)+grid[z0+1, x0+1]*tx)*tz


def footprint(tris, grid, step, above=3.0, grow=1):
    """Corners where triangles meet the ground (a placed rock's foot: the
    triangle's heights overlap the ground's under it, up to `above` m over
    it), grown by `grow` cells."""
    mask = np.zeros((N, N), bool)
    if not len(tris): return mask
    t = np.asarray(tris, np.float64)
    inside = (t[:, :, [0, 2]].max(1) >= 0).all(1) & (t[:, :, [0, 2]].min(1) <= (N-1)*step).all(1)
    t = t[inside]
    lo = np.clip(np.floor(t[:, :, [0, 2]].min(1)/step), 0, N-1).astype(int)
    hi = np.clip(np.ceil(t[:, :, [0, 2]].max(1)/step), 0, N-1).astype(int)
    ylo, yhi = t[:, :, 1].min(1), t[:, :, 1].max(1)
    for k in range(len(t)):
        (x0, z0), (x1, z1) = lo[k], hi[k]
        g = grid[z0:z1+1, x0:x1+1]
        if ylo[k] < g.max()+above and yhi[k] > g.min()-1: mask[z0:z1+1, x0:x1+1] = True
    return dilate(mask, grow)


def tower_frame(tower):
    """World (x, z) -> the tower's frame (forward, across), and back."""
    a = math.radians(tower['facing'])
    c, s = math.cos(a), math.sin(a)
    tx, tz = tower['at']
    to = lambda x, z: ((x-tx)*c+(z-tz)*s, -(x-tx)*s+(z-tz)*c)
    back = lambda f, w: (tx+f*c-w*s, tz+f*s+w*c)
    return to, back


def shape(grid, step, ring, tower, moat, protect, centre):
    """The ring's ground: valleys carved, the tower's site levelled, the
    moat cut. `protect` (N, N) marks corners never to change (the measured
    square, things placed outside it). Returns the new grid, the valleys
    and per-corner masks: 'crust' (0..1, near the rivers and the moat),
    'floor' (on a valley's floor)."""
    plinth_r, moat_r, moat_lava, moat_floor = moat
    top = tower['plinth']
    h = grid.copy()
    xs = np.arange(N)*step
    X, Z = np.meshgrid(xs, xs)
    pts = np.c_[X.ravel(), Z.ravel()]

    # The tower's site: under the plinth, the moat, the far bank, the
    # bridge's landing, then back to the ground.
    to, back = tower_frame(tower)
    F, W = to(X, Z)
    R = np.hypot(F, W)
    bank = top-2.4
    site = np.where(R < plinth_r+.4, top-1.0, top+moat_floor)
    # The far bank: level at `bank` for 12 m past the moat (the moat's rim
    # stands on it), wherever the ground was higher or lower.
    rise = np.clip((R-moat_r)/2.0, 0, 1)
    site = np.where(R >= moat_r, (top+moat_floor)*(1-rise)+bank*rise, site)
    blend = np.clip((R-moat_r-12)/14, 0, 1)
    # Under the bridge's far end and past it, the bank runs on to meet it.
    blend = np.where((F > 0) & (np.abs(W) < 14), np.clip((R-70)/14, 0, 1), blend)
    h = site*(1-blend)+grid*blend
    site_zone = R < moat_r+28

    # The valleys.
    start_floor = bank
    valleys = []
    crust = np.zeros((N, N)); floor_mask = np.zeros((N, N), bool)
    carve = np.full((N, N), np.inf)
    fill = np.full((N, N), np.nan)
    tin, tout = math.tan(math.radians(ring['inner'])), math.tan(math.radians(ring['outer']))
    for spec in ring['valleys']:
        v = Valley(spec, ring, grid, step, start_floor, centre)
        valleys.append(v)
        reach = ring['floor']+80
        idx, lat, along = nearest(pts, v.q, v.tan, reach)
        ok = idx >= 0
        i = np.maximum(idx, 0)
        # Past either end the valley stops: no carve beyond its last sample.
        beyond = ((i == 0) & (along < -1)) | ((i == len(v.q)-1) & (along > 1))
        ok &= ~beyond
        f = v.floor[i]
        inner = np.where(lat > 0, v.inner_left[i], ~v.inner_left[i])
        a = np.abs(lat)
        target = np.where(a < ring['floor'], f, f+(a-ring['floor'])*np.where(inner, tin, tout))
        # The river's channel and its crust.
        rl = lat-v.meander[i]
        hw = v.half[i]
        wet = v.wet[i]
        dip = np.where(wet & (np.abs(rl) < hw*1.2), ring['channel']*(1-(rl/(hw*1.2))**2), 0)
        target = target-dip
        # Nothing of the valley inside the moat's ring (the river runs into it).
        ok &= R.ravel() >= moat_r+1
        on_floor = ok & (a < ring['floor'])
        carve.ravel()[ok] = np.minimum(carve.ravel()[ok], target[ok])
        fl = fill.ravel(); fl[on_floor] = np.where(np.isnan(fl[on_floor]), target[on_floor], np.minimum(fl[on_floor], target[on_floor])); fill = fl.reshape(N, N)
        floor_mask.ravel()[on_floor] = True
        c = np.where(ok & wet, np.clip(1-(np.abs(rl)-hw)/ring['crust'], 0, 1), 0)
        crust = np.maximum(crust, c.reshape(N, N))
    new = np.minimum(h, carve)
    new = np.where(np.isnan(fill), new, fill)

    # Leave protected ground alone, blending the valleys' walls into it.
    keep = protect & ~site_zone
    w = falloff(keep, 3)
    h = new*(1-w)+grid*w
    # The moat's crust.
    crust = np.maximum(crust, np.clip(1-np.abs(R-(plinth_r+moat_r)/2)/((moat_r-plinth_r)/2+3), 0, 1)*(R < moat_r+4))
    return h, valleys, {'crust': crust, 'floor': floor_mask}


def rivers(valleys, ground, step=1.0):
    """The rivers' lava, in the world: hot cores (flat across, a little
    under the floor) and cooled crust along their banks (lying on the
    ground). Returns (hot forms, crust forms) for the lava and flow light
    modes."""
    hot, cool = [], []
    for v in valleys:
        r = v.ring
        idx = np.arange(0, len(v.q), max(1, int(step)))
        runs, cur = [], []
        for i in idx:
            if v.wet[i]: cur.append(i)
            elif cur: runs.append(cur); cur = []
        if cur: runs.append(cur)
        for run in runs:
            if len(run) < 3: continue
            across = np.linspace(-1, 1, 7)
            verts, faces = [], []
            for k, i in enumerate(run):
                y = v.floor[i]-r['surface']
                for a in across:
                    x, z = v.river_point(i, a*v.half[i]*1.15)
                    verts.append((x, y, z))
                if k:
                    o, n = (k-1)*len(across), k*len(across)
                    for j in range(len(across)-1):
                        faces += [(o+j, n+j, n+j+1), (o+j, n+j+1, o+j+1)]
            f = _up(np.array(verts), np.array(faces))
            hot.append(A.Form(verts, f, 'water', False, False))
            for side in (-1, 1):
                verts, rim, faces = [], [], []
                bands = np.linspace(0, 1, 6)
                for k, i in enumerate(run):
                    for b in bands:
                        lat = side*(v.half[i]*(.85+b*0)+b*r['crust']*.75)
                        x, z = v.river_point(i, lat)
                        verts.append((x, ground(x, z)+.1, z)); rim.append(b)
                    if k:
                        o, n = (k-1)*len(bands), k*len(bands)
                        for j in range(len(bands)-1):
                            faces += [(o+j, n+j, n+j+1), (o+j, n+j+1, o+j+1)]
                f = _up(np.array(verts), np.array(faces))
                cool.append(A.Form(verts, f, 'water', False, False, rim))
    return hot, cool


def _up(v, f):
    """Faces turned to face up."""
    f = np.asarray(f, np.int64)
    n = np.cross(v[f[:, 1]]-v[f[:, 0]], v[f[:, 2]]-v[f[:, 0]])
    return np.where((n[:, 1] < 0)[:, None], f[:, ::-1], f)


def dressing(valleys, ground, seed):
    """Rock on the valleys' walls and rims (crags, shards, boulders) and
    the rock arches over their rivers, as (form, lit) pairs in the world."""
    out = []
    rng = np.random.default_rng(seed)
    for v in valleys:
        r = v.ring
        for s in np.arange(r['lead'], v.length-r['ramp']*.6, r['rock_every']):
            i = int(np.searchsorted(v.s, s))
            for side in (-1, 1):
                if rng.random() < .35: continue
                lat = side*(r['floor']+rng.uniform(1, 13))
                left = np.array([-v.tan[i, 1], v.tan[i, 0]])
                x, z = v.q[i]+left*lat
                y = ground(x, z)
                size = rng.uniform(3, 8)
                kind = rng.random()
                k = int(rng.integers(1 << 30))
                if kind < .45:
                    form = A.crag((-size, -size*.8, -2), (size, size*.8, size*rng.uniform(.8, 1.6)), k, peaks=5,
                                  material='bark')
                elif kind < .75:
                    hgt = size*rng.uniform(1.6, 2.8)
                    form = A.spindle((-size*.35, -size*.3, -1.5), (size*.35, size*.3, hgt), k, material='leaf')
                else:
                    form = A.blob((-size*.7, -size*.6, -1.2), (size*.7, size*.6, size*.8), k, material='bark', flat_bottom=.15)
                a = rng.uniform(0, math.tau)
                lean = rng.uniform(-.18, .18)
                m = _matrix(a, lean, (x, y, z))
                out.append((form, m))
        for t in v.arches:
            i = int(np.searchsorted(v.s, t*v.length))
            left = np.array([-v.tan[i, 1], v.tan[i, 0]])
            span = r['floor']+6
            a, b = v.q[i]-left*span, v.q[i]+left*span
            ya, yb = ground(*a), ground(*b)
            top = max(ya, yb, v.floor[i])+r['arch_rise']
            pts, rad = [], []
            for k in range(9):
                u = k/8
                p = a+(b-a)*u
                y = (ya*(1-u)+yb*u)*(1-math.sin(math.pi*u))+top*math.sin(math.pi*u)
                pts.append((p[0], y-1.0, p[1])); rad.append(3.6-1.1*math.sin(math.pi*u))
            out.append((A.tube(pts, rad, sides=9, seed=int(rng.integers(1 << 30)), rough=.22, material='bark'),
                        np.c_[np.eye(3), np.zeros(3)]))
    return out


def _matrix(yaw, lean, at):
    """Local (Z up) to world (Y up): turned `yaw` about up, leaning `lean`."""
    c, s = math.cos(yaw), math.sin(yaw)
    rz = np.array([[c, -s, 0], [s, c, 0], [0, 0, 1.0]])
    cl, sl = math.cos(lean), math.sin(lean)
    rx = np.array([[1, 0, 0], [0, cl, -sl], [0, sl, cl]])
    swap = np.array([[1, 0, 0], [0, 0, 1], [0, 1, 0.0]])   # local z up -> world y up (a mirror)
    m = swap@rz@rx
    return np.c_[m, np.asarray(at, np.float64)]
