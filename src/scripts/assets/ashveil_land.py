"""Ashveil's ground: an original volcanic caldera (docs/ashveil.md).

The whole 256 x 256 tile (2.56 m a cell) is made here from `maps/ashveil.json`
"land": a caldera floor that rises toward its walls past the fence, a low
lava dome in the middle that carries the temple, a breached cinder cone in
the north with a lava flow out of its breach, a small spatter cone in the
northeast, an old flow lobe from the east wall whose flat top makes a
skiing lane, a fissure across the south with spatter ramparts along both
sides of its lava, two collapse pits where lava tubes run under the dome to
the cathedral, and a raised shelf in the east for the basalt mesa. Rolling
and ridged noise roughen it, and a few rounds of slumping settle slopes over
the angle of repose, so it reads as real land: nothing mirrored, nothing
traced.

World frame: metres, X east, Y up, Z south; the tile's corner at the origin;
row k of the grid is z = k * STEP, column i is x = i * STEP.
"""
import math

import numpy as np

N = 256
STEP = 2.56


def smoothstep(e0, e1, x):
    t = np.clip((x-e0)/(e1-e0), 0.0, 1.0)
    return t*t*(3-2*t)


def coords():
    xs = np.arange(N)*STEP
    return np.meshgrid(xs, xs)


# --- Noise --------------------------------------------------------------------

def gradient_noise(X, Z, seed, scale):
    """Perlin-style gradient noise in about -1..1 at world (X, Z), with
    features about `scale` m apart."""
    rng = np.random.default_rng(seed & 0xffffffff)
    perm = rng.permutation(256)
    angles = rng.uniform(0, math.tau, 256)
    gx, gz = np.cos(angles), np.sin(angles)
    x, z = X/scale, Z/scale
    x0, z0 = np.floor(x).astype(np.int64), np.floor(z).astype(np.int64)
    fx, fz = x-x0, z-z0

    def grad(ix, iz, dx, dz):
        h = perm[(perm[ix & 255]+iz) & 255]
        return gx[h]*dx+gz[h]*dz

    u, v = fx*fx*fx*(fx*(fx*6-15)+10), fz*fz*fz*(fz*(fz*6-15)+10)
    n00, n10 = grad(x0, z0, fx, fz), grad(x0+1, z0, fx-1, fz)
    n01, n11 = grad(x0, z0+1, fx, fz-1), grad(x0+1, z0+1, fx-1, fz-1)
    return ((n00*(1-u)+n10*u)*(1-v)+(n01*(1-u)+n11*u)*v)*1.6


def fbm(X, Z, seed, scale, octaves=4, gain=.5):
    out, amp, total = np.zeros_like(X, np.float64), 1.0, 0.0
    for k in range(octaves):
        out += amp*gradient_noise(X, Z, seed+k*101, scale/2**k)
        total += amp; amp *= gain
    return out/total


def ridged(X, Z, seed, scale, octaves=3):
    """Sharp-crested noise in 0..1: the pressure ridges of a lava field."""
    out, amp, total = np.zeros_like(X, np.float64), 1.0, 0.0
    for k in range(octaves):
        n = 1-np.abs(gradient_noise(X, Z, seed+k*211, scale/2**k))
        out += amp*n*n; total += amp; amp *= .5
    return out/total


# --- Shapes -------------------------------------------------------------------

def polyline(points, step=1.0):
    """Catmull-Rom through `points` [(x, z)], about every `step` m:
    (positions (n, 2), unit tangents (n, 2), distance along (n,))."""
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
    s = np.r_[0, np.cumsum(np.linalg.norm(np.diff(q, axis=0), axis=1))]
    tan = np.gradient(q, axis=0)
    tan /= np.maximum(np.linalg.norm(tan, axis=1), 1e-9)[:, None]
    return q, tan, s


def along(X, Z, q, tan, s):
    """For every corner: the distance to the polyline, the signed lateral
    offset (+ left of travel), the distance along it and the nearest
    sample's index; past either end the lateral offset is the distance from
    that end, and `beyond` says how far past it."""
    P = np.c_[X.ravel(), Z.ravel()]
    best = np.full(len(P), np.inf); idx = np.zeros(len(P), np.int64)
    for k in range(0, len(q), 2):
        d = np.hypot(P[:, 0]-q[k, 0], P[:, 1]-q[k, 1])
        closer = d < best
        best[closer] = d[closer]; idx[closer] = k
    d = P-q[idx]
    t = tan[idx]
    lat = d[:, 0]*-t[:, 1]+d[:, 1]*t[:, 0]
    fwd = d[:, 0]*t[:, 0]+d[:, 1]*t[:, 1]
    beyond = np.where(idx >= len(q)-2, np.maximum(fwd, 0), 0)+np.where(idx == 0, np.maximum(-fwd, 0), 0)
    shape = X.shape
    return (best.reshape(shape), lat.reshape(shape), (s[idx]+np.clip(fwd, -2, 2)).reshape(shape),
            idx.reshape(shape), beyond.reshape(shape))


def ring_noise(theta, seed, lobes=3.0):
    """Smooth noise round a circle (about -1..1), `lobes` bumps a turn."""
    return gradient_noise(np.cos(theta)*lobes*8, np.sin(theta)*lobes*8, seed, 8.0)


def cone(X, Z, c, seed):
    """A cinder cone: flanks from `base` m out up to its rim at `rim_r`,
    `height` m over its surroundings, stretched `stretch` along `facing`
    degrees, its outline wandering, gullies down its flanks, a crater bowl
    `crater` m deep, and an optional breach (a notch down to the crater
    floor) toward `breach` degrees (from +X toward +Z), `breach_width`
    degrees wide. Returns (rise over the ground, crater floor mask)."""
    dx, dz = X-c['at'][0], Z-c['at'][1]
    a = math.radians(c.get('facing', 0.0)); k = c.get('stretch', 1.0)
    u, v = dx*math.cos(a)+dz*math.sin(a), -dx*math.sin(a)+dz*math.cos(a)
    theta = np.arctan2(dz, dx)
    wobble = 1+c.get('wobble', .1)*ring_noise(theta, seed)
    r = np.hypot(u/k, v)/wobble
    flank = smoothstep(c['base'], c['rim_r'], r)**1.15
    gullies = c.get('gullies', 0)
    if gullies:
        g = (.5+.5*np.cos(gullies*theta+2.5*ring_noise(theta, seed+5, 2.0)))**8
        flank = flank-c.get('gully_depth', .07)*g*np.sin(np.clip((r-c['rim_r'])/(c['base']-c['rim_r']), 0, 1)*math.pi)
    rise = c['height']*flank
    inner = c['rim_r']*.82
    bowl = c['crater']*(1-smoothstep(inner, c['rim_r'], r))*(1-(r/inner).clip(0, 1)**2*.35)
    rise = rise-bowl
    floor = r < inner*.72
    if 'breach' in c:
        off = np.abs((np.degrees(theta)-c['breach']+180) % 360-180)
        notch = (1-smoothstep(c['breach_width']*.35, c['breach_width']*.5, off))*smoothstep(inner*.6, c['rim_r'], r)
        notch *= 1-smoothstep(c['rim_r'], c['base']*.85, r)
        low = c['height']-c['crater']+.6
        rise = np.where(notch > 0, rise*(1-notch)+np.minimum(rise, low)*notch, rise)
    return rise, floor, r


def lobe(X, Z, f, seed):
    """A lava flow lobe along `points`: a top `thick` m over the ground it
    covered, its width swelling and pinching along it, raised levees at its
    edges, its sides falling off over `edge` m, pressure ridges across its
    top and a bulging front at the end. Returns (rise, top mask, edge
    mask)."""
    q, tan, s = polyline(f['points'])
    d, lat, a, idx, beyond = along(X, Z, q, tan, s)
    length = s[-1]
    t = np.clip(a/length, 0, 1)
    rng = np.random.default_rng(seed)
    ph = rng.uniform(0, math.tau, 3)
    swell = 1+.22*np.sin(a/31+ph[0])+.12*np.sin(a/13+ph[1])
    half = f['width']/2*(f.get('taper', .65)+(1-f.get('taper', .65))*t)*swell
    half = half*(1+.18*gradient_noise(X, Z, seed, 30.0))
    edge = f.get('edge', 6.0)
    front_len = f.get('front', 8.0)
    # The front bulges: past the end the outline rounds off.
    reach = np.hypot(np.maximum(beyond, 0), lat)
    end = np.where(beyond > 0, reach, np.abs(lat))
    side = 1-smoothstep(half-edge, half, end)
    body = side
    levee = f.get('levee', .8)*np.exp(-((end-(half-edge*.9))/(edge*.45))**2)
    ridges = f.get('ridges', 1.0)*(ridged(X, Z, seed+3, 26.0, 2)-.55)
    thick = f['thick']*(.75+.25*t)
    rise = (thick+levee+ridges*(side > .9))*body
    return rise, body > .85, (body > .05) & (body < .85)


def trench(X, Z, f, seed):
    """A fissure: a trench along `points`, `width` m wide and `depth` deep,
    between spatter ramparts `rampart` m high standing `spread` m out on
    each side, broken where `gaps` (fractions along) let people through.
    Returns (rise (negative in the trench), trench mask, path)."""
    q, tan, s = polyline(f['points'])
    d, lat, a, idx, beyond = along(X, Z, q, tan, s)
    length = s[-1]
    ends = smoothstep(0, 18, a)*smoothstep(0, 18, length-a)*(beyond < 1)
    half = f['width']/2*(1+.25*gradient_noise(X, Z, seed, 22.0))
    cut = f['depth']*(1-smoothstep(half*.55, half, np.abs(lat)))*ends
    bumps = .55+.45*np.clip(gradient_noise(X, Z, seed+7, 14.0)+.4, 0, 1)
    wall = f['rampart']*np.exp(-((np.abs(lat)-f['spread'])/(f['spread']*.42))**2)*bumps*ends
    for g in f.get('gaps', []):
        wall *= smoothstep(f['gap']*.35, f['gap']*.5, np.abs(a-g*length))
    return wall-cut, (np.abs(lat) < half*.55) & (ends > .5), (q, tan, s)


def hill(X, Z, at, radius, height, flat=.35, seed=0):
    """An older rounded hill the flows went round: a gentle dome with a
    broad top and a wandering outline."""
    dx, dz = X-at[0], Z-at[1]
    r = np.hypot(dx, dz)/(1+.18*ring_noise(np.arctan2(dz, dx), seed, 2.0))
    t = np.clip(r/radius, 0, 1)
    return height*(1-smoothstep(flat, 1.0, t))


def heights(land, fence):
    """The ground (N, N) and its features: masks ('flow_top', 'flow_edge',
    'cinder', 'trench', 'craters', 'old'), the fissure's path and each
    cone's crater floor height."""
    seed = land['seed']
    X, Z = coords()
    x0, z0, x1, z1 = fence
    # Warped coordinates, so nothing lines up with the grid.
    wx = X+28*fbm(X, Z, seed+1, 160.0, 2); wz = Z+28*fbm(X, Z, seed+2, 160.0, 2)
    # The caldera floor and its walls rising past the fence, ragged.
    edge = np.minimum.reduce([wx-x0, x1-wx, wz-z0, z1-wz])
    rim = land['rim']
    wall = rim['height']*(1-smoothstep(-rim['crest'], rim['reach'], edge))**1.6
    wall *= 1+.35*(ridged(X, Z, seed+9, 90.0)-.5)
    # Past its crest, just outside the fence, the rim drops into the lava sea.
    outside = np.clip((-edge-rim['crest'])/rim['drop'], 0, 1)
    lean = np.zeros_like(X)
    for at, height, reach in land['swells']:
        lean += height*np.exp(-((X-at[0])**2+(Z-at[1])**2)/(2*reach**2))
    h = land['floor']+wall+lean
    h = h*(1-outside**1.5)+(rim['sea']-6.0)*outside**1.5
    roll = land['roll']
    h += roll[0]*fbm(wx, wz, seed, 170.0, 3)+roll[1]*fbm(wx, wz, seed+31, 55.0, 3)
    h += roll[2]*(ridged(wx, wz, seed+57, 70.0)-.5)
    # The ring fault's benches: arcs round the basin where the ground steps
    # up outward, smoothed into ramps at `ramps` (degrees).
    for b in land['benches']:
        dx, dz = X-b['centre'][0], Z-b['centre'][1]
        theta = np.degrees(np.arctan2(dz, dx)) % 360
        r = np.hypot(dx, dz)*(1+.07*ring_noise(np.radians(theta), seed+b['seed'], 2.5))
        lo, hi = b['arc']
        span = ((theta-lo) % 360 <= (hi-lo) % 360)
        fade = np.minimum((theta-lo) % 360, (hi-theta) % 360)
        on = span*smoothstep(0, 18, fade)
        for ramp in b.get('ramps', []):
            on *= smoothstep(4, 11, np.abs((theta-ramp+180) % 360-180))*.85+.15
        h += b['rise']*smoothstep(b['radius']-b['scarp'], b['radius']+b['scarp'], r)*on
    quiet = np.zeros_like(X)
    old = np.zeros_like(X, bool)
    # Older hills (kipukas): the ash on them is lighter.
    for k, (at, radius, height) in enumerate(land['hills']):
        rise = hill(X, Z, at, radius, height, seed=seed+300+k)
        h += rise
        old |= rise > height*.25
    # Hummocky lava field: tumuli and ridges where `hummocks` say.
    for at, radius, height in land['hummocks']:
        w = 1-smoothstep(radius*.6, radius, np.hypot(X-at[0], Z-at[1]))
        h += height*np.clip(ridged(wx, wz, seed+71, 38.0, 2)-.45, 0, 1)*2*w

    # The dome under the temple: a flat top, smooth flanks.
    dm = land['dome']
    r = np.hypot(X-dm['at'][0], Z-dm['at'][1])
    w = 1-smoothstep(dm['flat'], dm['reach'], r)
    t = np.clip((r-dm['flat'])/(dm['reach']-dm['flat']), 0, 1)
    h = h*(1-w)+(dm['top']-(dm['top']-h)*t**1.4)*w
    h = np.where(r < dm['flat'], dm['top'], h)
    quiet = np.maximum(quiet, 1-smoothstep(dm['flat'], dm['flat']+22, r))

    # The mesa's shelf.
    ms = land['mesa']
    r = np.hypot(X-ms['at'][0], Z-ms['at'][1])
    w = 1-smoothstep(ms['flat'], ms['reach'], r)
    h = h*(1-w)+ms['top']*w
    quiet = np.maximum(quiet, 1-smoothstep(ms['flat'], ms['flat']+10, r))

    masks = {k: np.zeros_like(X, bool) for k in ('flow_top', 'flow_edge', 'cinder', 'trench', 'craters')}
    masks['old'] = old
    floors = []
    for k, f in enumerate(land['flows']):
        rise, top, side = lobe(X, Z, f, seed+400+k*13)
        h += rise
        masks['flow_top'] |= top; masks['flow_edge'] |= side
        masks['old'] &= ~(top | side)
    for k, c in enumerate(land['cones']):
        ring = np.abs(np.hypot(X-c['at'][0], Z-c['at'][1])-c['base']) < 4
        base = float(np.mean(h[ring]))
        rise, floor, r = cone(X, Z, c, seed+500+k*17)
        inside = r < c['base']
        h = np.where(inside, np.maximum(h, base+rise), h)
        h = np.where(floor, base+c['height']-c['crater'], h)
        floors.append(base+c['height']-c['crater'])
        masks['cinder'] |= r < c['base']*.95
        masks['craters'] |= floor
        masks['old'] &= ~(r < c['base'])
        quiet = np.maximum(quiet, (r < c['rim_r']*.8)*1.0)
    rise, cut, path = trench(X, Z, land['fissure'], seed+600)
    h += rise
    masks['trench'] |= cut
    # Ponds: hollows whose floors lie `depth` under their rims; lava fills
    # them (`pools`).
    ponds = []
    for k, p in enumerate(land['ponds']):
        dx, dz = X-p['at'][0], Z-p['at'][1]
        r = np.hypot(dx, dz)/(1+.15*ring_noise(np.arctan2(dz, dx), seed+700+k, 2.0))
        rim = float(np.median(h[r < p['r']*.5] if p.get('low') else h[np.abs(r-p['r']) < 3]))
        bowl = np.clip(1-smoothstep(p['r']*.55, p['r'], r), 0, 1)
        h = np.minimum(h, h*(1-bowl)+(rim-p['depth'])*bowl)
        ponds.append((p, rim-p['depth']))
        quiet = np.maximum(quiet, 1-smoothstep(p['r'], p['r']+4, r))

    h += land['grit']*fbm(X, Z, seed+77, 9.0, 2)*(1-quiet)
    h = slump(h, land['repose'], 24, keep=quiet > .99)
    return h, masks, {'fissure': path, 'crater_floors': floors, 'ponds': ponds}


def cut_tubes(h, land):
    """The lava tubes' trenches and their collapse pits, cut into the
    finished ground `h`: each tube's floor is flat at `floor`, `width` m
    wide, its walls one cell steep, from its pit (a bowl `r` m across round
    the tube's first point) to its last point. Returns the cut ground and,
    per tube, its path with the ground that stood over it (for the roof)."""
    X, Z = coords()
    out = h.copy()
    tubes = []
    for t in land['tubes']:
        q, tan, s = polyline(t['points'], .5)
        d, lat, a, idx, beyond = along(X, Z, q, tan, s)
        half = t['width']/2
        floor = t['floor']
        slot = np.where(beyond > 0, 1e9, np.abs(lat))
        wall = np.clip((slot-half-1.6)/STEP, 0, 1)
        out = np.minimum(out, floor+(h-floor)*wall)
        # The pit: a bowl down to the floor, its sides steep but climbable.
        px, pz = t['points'][0]
        r = np.hypot(X-px, Z-pz)
        bowl = smoothstep(t['pit']*.4, t['pit'], r)
        out = np.minimum(out, floor+(h-floor)*bowl**.7)
        tubes.append({'q': q, 'tan': tan, 's': s, 'ground': h, 'floor': floor, 'width': t['width'],
                      'height': t['height'], 'pit': (px, pz, t['pit'])})
    return out, tubes


def level(h, spots, radius=5.0, blend=4.0):
    """Ground levelled flat round each spot (x, z) to its own mean height
    within `radius`, blending back over `blend` m: footing for spawns."""
    X, Z = coords()
    out = h.copy()
    for x, z in spots:
        r = np.hypot(X-x, Z-z)
        flat = float(h[r < radius].mean())
        w = 1-smoothstep(radius, radius+blend, r)
        out = out*(1-w)+flat*w
    return out


def slump(h, degrees, rounds, keep=None):
    """Thermal erosion: wherever a corner stands more than the angle of
    repose over a neighbour, half the excess slides down to it. `keep`
    corners don't move."""
    talus = math.tan(math.radians(degrees))*STEP
    h = h.copy()
    for _ in range(rounds):
        move = np.zeros_like(h)
        for axis, sh in ((0, 1), (0, -1), (1, 1), (1, -1)):
            nb = np.roll(h, sh, axis)
            d = h-nb-talus
            d[0, :] = d[-1, :] = 0; d[:, 0] = d[:, -1] = 0
            m = np.where(d > 0, d*.12, 0)
            move -= m
            move += np.roll(m, -sh, axis)
        if keep is not None: move[keep] = 0
        h += move
    return h


def slope_degrees(h):
    gz, gx = np.gradient(h, STEP)
    return np.degrees(np.arctan(np.hypot(gx, gz)))
