"""Original rolling terrain for Tower Complex.

Generated only from PeakRunner's own hash noise and base placement; no
heightfield from any other game is read, resampled or fitted.

Shape: one continuous landscape of broad, domain-warped hills rather than a
floor with mounds on it. Sinuous valleys wind through it where a low-frequency
field crosses its midline, so the ground reads as drained rather than dotted
with spikes. A gentle basin lies between the two floating bases, and each base
sits over a shoulder that rises behind it, so leaving either base is a long
downhill ski toward the middle. A thermal-erosion pass then relaxes any slope
steeper than the talus angle, which rounds off isolated summits and softens
creases. Ground under the hulls, the landing pads and the Capture & Hold
towers is lowered or levelled with smooth blends, never with hard caps, so no
flat plates or ledges appear.
"""
import numpy as np

N, STEP = 256, 8.0
PAD_CLEARANCE = 34   # deck to ground under a landing pad; keel tip 16 m down
PAD_REACH = 70       # ...and at most this far, so a jet from the ground reaches it
HULL_CLEARANCE = 62  # deck to ground under a base (deepest keel tip is 34 m down)
TALUS_DEG = 36.0     # thermal erosion relaxes slopes steeper than this
EROSION_STEPS = 60
POINT_FLAT = 24.0    # C&H plateau radius (the ring is 12 m); blends out to 2x


def _hash(ix, iz, seed):
    n = (ix.astype(np.uint64)*np.uint64(374761393) + iz.astype(np.uint64)*np.uint64(668265263)
         + np.uint64(seed)*np.uint64(1274126177)) & np.uint64(0xffffffff)
    n = ((n ^ (n >> np.uint64(13)))*np.uint64(1274126177)) & np.uint64(0xffffffff)
    return ((n ^ (n >> np.uint64(16))) & np.uint64(65535)).astype(np.float64)/65535.0


def _value(x, z, seed):
    ix, iz = np.floor(x), np.floor(z)
    u, v = x-ix, z-iz
    u, v = u*u*(3-2*u), v*v*(3-2*v)
    ix, iz = ix.astype(np.int64)+4096, iz.astype(np.int64)+4096
    a, b = _hash(ix, iz, seed), _hash(ix+1, iz, seed)
    c, d = _hash(ix, iz+1, seed), _hash(ix+1, iz+1, seed)
    return (a*(1-u)+b*u)*(1-v)+(c*(1-u)+d*u)*v


def _fbm(x, z, wavelength, octaves, seed, ridged=False):
    # Shared with the Cairnhold, Frostline and Dustreach terrain modules:
    # changing this changes their packs.
    total, amp, norm = 0.0, 1.0, 0.0
    for o in range(octaves):
        n = _value(x/wavelength, z/wavelength, seed+o*101)
        if ridged: n = 1-np.abs(2*n-1); n = n*n
        total = total+amp*n; norm += amp
        amp *= .5; wavelength /= 2.03
    return total/norm


def _smooth(a, b, t):
    t = np.clip((t-a)/(b-a), 0, 1)
    return t*t*(3-2*t)


def _smin(a, b, k):
    """Smooth minimum: equals min(a, b) far from the crossover, blended over
    about k metres around it, so a clearance cap never leaves a plate."""
    h = np.clip(.5+.5*(b-a)/k, 0, 1)
    return b*(1-h)+a*h-k*h*(1-h)


def _erode(h, steps=EROSION_STEPS, talus=TALUS_DEG, rate=.35):
    """Thermal erosion: wherever the drop to a neighbour exceeds the talus
    height, move a share of the excess downhill. Deterministic and local."""
    limit = STEP*np.tan(np.radians(talus))
    h = h.copy()
    offsets = ((1, 0), (-1, 0), (0, 1), (0, -1))
    for _ in range(steps):
        p = np.pad(h, 1, mode='edge')
        moved = np.zeros_like(h)
        for dz, dx in offsets:
            nb = p[1+dz:N+1+dz, 1+dx:N+1+dx]
            excess = np.maximum(h-nb-limit, 0)*rate*.25
            moved -= excess
            # Deposit on the neighbour: shift the excess field onto it.
            dep = np.zeros_like(h)
            zs = slice(max(dz, 0), N+min(dz, 0)); zd = slice(max(-dz, 0), N+min(-dz, 0))
            xs = slice(max(dx, 0), N+min(dx, 0)); xd = slice(max(-dx, 0), N+min(-dx, 0))
            dep[zs, xs] = excess[zd, xd]
            moved += dep
        h += moved
    return h


def _blur(h, passes=2):
    for _ in range(passes):
        p = np.pad(h, 1, mode='edge')
        h = (p[:-2, 1:-1]+p[2:, 1:-1]+p[1:-1, :-2]+p[1:-1, 2:]+4*p[1:-1, 1:-1])/8
    return h


def heights(bases, seed, deck_y=240.0, pads=(), points=(), summits=()):
    """256x256 heights in metres (row = z cell, column = x cell).

    `pads` are (x, z, deck_y) landing pads: the ground under each stays
    PAD_CLEARANCE below its deck. `points` are (x, z) Capture & Hold tower
    sites: the ground is levelled into a gentle plateau there so the ring is
    walkable. `summits` are (x, z, height, radius) tall hills raised before
    erosion, so they grow shoulders and spurs rather than standing as spikes."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    (rx, rz), (bx, bz) = [(b[0], b[2]) for b in bases]
    cx, cz = (rx+bx)/2, (rz+bz)/2
    axis = np.array([bx-rx, bz-rz], dtype=np.float64); half = np.linalg.norm(axis)/2; axis /= 2*half
    u = (x-cx)*axis[0]+(z-cz)*axis[1]
    w = -(x-cx)*axis[1]+(z-cz)*axis[0]
    # Domain warp shared by every layer, so hills and valleys curve together.
    wx = x+140*(_fbm(x+900, z, 700, 3, seed+7)-.5)*2
    wz = z+140*(_fbm(x, z+900, 700, 3, seed+11)-.5)*2

    # Broad rolling hills (long wavelengths only: no spikes to begin with).
    h = 150+100*(_fbm(wx, wz, 520, 3, seed)-.5)*2
    h = h+46*(_fbm(wx+300, wz+500, 230, 3, seed+3)-.5)*2
    h = h+14*(_fbm(x, z, 100, 2, seed+5)-.5)*2
    # Drainage: meandering valleys where a slow field crosses its midline.
    river = _fbm(wx*.9+2000, wz*.9, 820, 2, seed+13)
    h = h-34*np.exp(-((river-.5)/.055)**2)
    # A shallow basin between the bases and a broad rim around the play space.
    r = np.hypot(u/(half+260), w/320)
    h = h-62*np.exp(-3.0*r*r)+46*_smooth(.9, 1.35, r)
    # Each base sits over a shoulder that rises behind it (downhill start).
    for sx, sz, sign in ((rx, rz, -1), (bx, bz, 1)):
        du = (x-sx)*axis[0]+(z-sz)*axis[1]
        dw = -(x-sx)*axis[1]+(z-sz)*axis[0]
        h = h+48*np.exp(-(dw/190)**2)*_smooth(-120, 200, du*sign)
    # Outer edge: climb into bounding hills beyond the play space.
    edge = np.maximum(np.abs(x-1024), np.abs(z-1024))
    h = h+70*_smooth(780, 1010, edge)
    # Tall summits: broad domes with a partial domain warp, so their outlines
    # curve with the hills while their centres stay where they were placed,
    # and ridged spurs run down their shoulders.
    for k, (sx, sz, height, radius) in enumerate(summits):
        d = np.hypot((x-sx)+.35*(wx-x), (z-sz)+.35*(wz-z))/radius
        spur = _fbm(x+k*613, z-k*389, radius*.9, 3, seed+31, ridged=True)
        h = h+height*np.exp(-d*d)*(.78+.44*spur)+.18*height*np.exp(-d*d/4)

    h = _blur(_erode(h), 1)

    # Clearance under the floating hulls and pads, with smooth blends.
    for sx, sz in ((rx, rz), (bx, bz)):
        d = np.hypot(x-sx, z-sz)
        cap = deck_y-HULL_CLEARANCE+np.maximum(d-40, 0)*.55
        h = _smin(h, cap, 14)
    for px, pz, py in pads:
        d = np.hypot(x-px, z-pz)
        h = _smin(h, py-PAD_CLEARANCE+np.maximum(d-20, 0)*.6, 10)
        # ...and a rise below it, so the deck stays in jet reach of the ground.
        h = -_smin(-h, -(py-PAD_REACH-np.maximum(d-24, 0)*.9), 10)
    # Capture & Hold plateaus: blend toward the local mean height.
    for px, pz in points:
        d = np.hypot(x-px, z-pz)
        near = d < POINT_FLAT
        level = float(h[near].mean())
        t = .2+.8*_smooth(POINT_FLAT, 2*POINT_FLAT, d)   # keep a gentle tilt, not a plate
        h = level+(h-level)*t
    return np.clip(h, 2, 2040)


def slope_degrees(h):
    gz, gx = np.gradient(h, STEP)
    return np.degrees(np.arctan(np.hypot(gx, gz)))


def weights(h):
    """Meadow/rock/soil splat per cell: rock on steep ground, soil on the
    middle slopes and high ground, meadow on gentle low ground."""
    s = slope_degrees(h)
    rock = np.clip((s-24)/14, 0, 1)*.9+.08
    soil = np.clip(1-rock, 0, 1)*np.clip(.25+(s-8)/30+(h-170)/160, 0, .85)
    meadow = np.clip(1-rock-soil, 0, 1)
    out = np.stack([meadow, rock, soil, np.zeros_like(h)], axis=-1)
    return np.round(out*255).astype(np.uint8)
