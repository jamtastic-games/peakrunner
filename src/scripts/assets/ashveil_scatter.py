"""Ashveil's field: the rock in the caldera and the land past it (our own
design, docs/ashveil.md).

- Basalt outcrops: clusters of short hexagonal columns where `outcrops`
  in `maps/ashveil.json` "field" says, broken off at different heights.
- Loose rock: crags, obsidian shards and boulders strewn on a jittered
  grid, thinner along the lanes (`lanes`) so the ways across stay open but
  never bare, and kept off everything `keep_out` marks (spawns, stations,
  the temple, the mesa, the tower, the tubes, lava).
- The fissure: spatter cones along its ramparts and rock arches over it.
- Steam: vents (smoke) by the craters and the fissure, plumes on the rim.
- The far land: a ring of peaks and two big volcanoes beyond the tile,
  drawn and lit live, with nothing to hit.

Returns (form, matrix, lit) triples; matrices map the forms' local frames
(Z up) to the world.
"""
import math

import numpy as np

from assets import ashveil_land as L
from assets import ashveil_ring as RG
from assets import ashveil_shapes as A

IDENT = np.c_[np.eye(3), np.zeros(3)]


def hexagon(x, z, y0, y1, r, material='leaf', solid=True):
    a = np.radians(30+60*np.arange(6)+0.0)
    ring = np.c_[x+r*np.cos(a), z+r*np.sin(a)]
    return A.extrude(ring, y0, y1, material, solid, axis='y')


def outcrop(x, z, radius, height, ground, rng, steep=1.6):
    """A cluster of basalt columns round (x, z): taller in the middle
    (`steep` sharpens the shoulder: a neck stands nearly sheer), broken off
    at uneven heights, a few fallen pieces at the foot."""
    out = []
    r = rng.uniform(.8, 1.15)
    pitch = r*math.sqrt(3)+.06
    row = pitch*math.sqrt(3)/2
    lean = rng.uniform(0, math.tau)
    n = int(radius/row)+1
    for j in range(-n, n+1):
        for i in range(-n, n+1):
            px = x+pitch*(i+.5*(j % 2)); pz = z+row*j
            d = math.hypot(px-x, pz-z)
            edge = radius*(1+.25*math.sin(math.atan2(pz-z, px-x)*2+lean))
            if d > edge or rng.random() < .12: continue
            h = height*(1-(d/edge)**steep)*rng.uniform(.6 if steep > 2 else .55, 1.0)
            if h < .7: continue
            foot = min(ground(px+ox, pz+oz) for ox, oz in ((r, 0), (-r, 0), (0, r), (0, -r)))-1.2
            out.append((hexagon(px, pz, foot, ground(px, pz)+h, r), IDENT, 'live'))
    for _ in range(int(rng.integers(1, 4))):
        a = rng.uniform(0, math.tau); d = radius+rng.uniform(1, 4)
        px, pz = x+math.cos(a)*d, z+math.sin(a)*d
        length = rng.uniform(2.5, 5.0)
        piece = hexagon(0, 0, -length/2, length/2, r*.9)
        v = piece.v.copy()
        piece.v = np.c_[v[:, 1], v[:, 2], v[:, 0]]  # lying along x
        out.append((piece, RG._matrix(rng.uniform(0, math.tau), 0.0, (px, ground(px, pz)+r*.6, pz)), 'live'))
    return out


def near_lines(x, z, lines, reach):
    for q in lines:
        d = np.hypot(q[:, 0]-x, q[:, 1]-z)
        if d.min() < reach: return True
    return False


def field(spec, ground, keep_out, slope, fissure, seed):
    """Every piece of loose rock, the outcrops, and the fissure's cones and
    arches. `ground(x, z)` is the terrain's height, `keep_out(x, z, r)` says
    whether a piece of radius r there would sit on something kept clear,
    `slope(x, z)` the ground's slope (degrees), `fissure` its path (q, tan,
    s) from `ashveil_land`."""
    f = spec['field']
    rng = np.random.default_rng(seed+911)
    out = []
    lanes = [L.polyline(points, 2.0)[0] for points in f['lanes']]
    x0, z0, x1, z1 = spec['ring']['fence']
    for x, z, radius, height in f['outcrops']:
        out += outcrop(x, z, radius, height, ground, rng)
    # Volcanic necks: tall plugs of columns the softer rock round them
    # wore away.
    for x, z, radius, height in f['necks']:
        out += outcrop(x, z, radius, height, ground, rng, steep=4.0)
    # Loose rock in clusters: a seed every so often, a few pieces round it.
    step = f['every']
    for gx in np.arange(x0+20, x1-20, step):
        for gz in np.arange(z0+20, z1-20, step):
            chance = f['density']*(.4 if near_lines(gx, gz, lanes, f['lane_clear']) else 1.0)
            if rng.random() > chance: continue
            cx, cz = gx+rng.uniform(-.45, .45)*step, gz+rng.uniform(-.45, .45)*step
            big = rng.random() < .3
            for _ in range(int(rng.integers(1, 5))):
                x, z = cx+rng.normal(0, 4.5), cz+rng.normal(0, 4.5)
                kind = rng.random(); size = rng.uniform(0, 1)**1.5; k = int(rng.integers(1 << 30))
                if slope(x, z) > 34: continue
                if kind < .45:
                    s_ = (5.0+size*9.0) if big else (2.0+size*4.0)
                    if keep_out(x, z, s_): continue
                    form = A.crag((-s_, -s_*.75, -2), (s_, s_*.75, s_*rng.uniform(.8, 1.6)), k, peaks=6, material='bark')
                elif kind < .62:
                    s_ = .8+size*1.8; hgt = s_*rng.uniform(2.4, 4.2)
                    if keep_out(x, z, s_): continue
                    form = A.spindle((-s_*.5, -s_*.42, -1.2), (s_*.5, s_*.42, hgt), k, material='leaf')
                else:
                    s_ = (1.5+size*3.5) if big else (.9+size*1.8)
                    if keep_out(x, z, s_): continue
                    form = A.blob((-s_*.8, -s_*.65, -1.0), (s_*.8, s_*.65, s_*.75), k, material='bark', flat_bottom=.15)
                y = ground(x, z)
                out.append((form, RG._matrix(rng.uniform(0, math.tau), rng.uniform(-.12, .12), (x, y, z)), 'live'))

    # The fissure: spatter cones on its ramparts, rock arches over it.
    q, tan, s = fissure
    fs = spec['land']['fissure']
    length = s[-1]
    for d in np.arange(14.0, length-14.0, f['spatter_every']):
        i = int(np.searchsorted(s, d))
        if any(abs(d-g*length) < fs['gap']*.8 for g in fs['gaps']): continue
        if any(abs(d-a*length) < 10 for a in f['arches']): continue
        side = 1 if rng.random() < .5 else -1
        left = np.array([-tan[i, 1], tan[i, 0]])
        x, z = q[i]+left*side*fs['spread']
        if keep_out(x, z, 3.0): continue
        r = rng.uniform(2.2, 3.6); h = rng.uniform(2.0, 4.0)
        cone = A.lathe([(r, 0), (r*.62, h*.8), (r*.45, h), (r*.3, h*.7), (r*.25, h*.55)], sides=11,
                       material='leaf', rough=.12, seed=int(rng.integers(1 << 30)))
        out.append((cone, RG._matrix(rng.uniform(0, math.tau), 0.0, (x, ground(x, z)-.4, z)), 'live'))
    for t in f['arches']:
        i = int(np.searchsorted(s, t*length))
        left = np.array([-tan[i, 1], tan[i, 0]])
        span = fs['spread']+3.0
        a, b = q[i]-left*span, q[i]+left*span
        ya, yb = ground(*a), ground(*b)
        top = max(ya, yb)+f['arch_rise']
        pts, rad = [], []
        for k in range(9):
            u = k/8
            p = a+(b-a)*u
            y = (ya*(1-u)+yb*u)*(1-math.sin(math.pi*u))+top*math.sin(math.pi*u)
            pts.append((p[0], y-1.0, p[1])); rad.append(2.6-.8*math.sin(math.pi*u))
        out.append((A.tube(pts, rad, sides=9, seed=int(rng.integers(1 << 30)), rough=.22, material='bark'), IDENT, 'live'))
    return out


def steam(spec, ground):
    """Smoke sources [x, y, z, size]: vents by the craters and the fissure,
    and plumes on the caldera's rim."""
    out = []
    for x, z, size in spec['field']['vents']:
        out.append([round(x, 2), round(ground(x, z)+.3, 2), round(z, 2), size])
    for x, z, size in spec['field']['plumes']:
        out.append([round(x, 2), round(ground(x, z)+2.0, 2), round(z, 2), size])
    return out


def far_land(spec, seed):
    """Peaks and two volcanoes past the tile: (form, matrix, 'live')."""
    rng = np.random.default_rng(seed+1201)
    out = []
    cx, cz = 326.0, 326.0
    for k in range(26):
        a = k/26*math.tau+rng.uniform(-.08, .08)
        d = rng.uniform(760, 1250)
        x, z = cx+math.cos(a)*d, cz+math.sin(a)*d
        w = rng.uniform(110, 240); h = rng.uniform(120, 300)
        form = A.crag((-w, -w*.8, -40), (w, w*.8, h), int(rng.integers(1 << 30)), peaks=7, sharp=1.3, material='bark')
        out.append((form, RG._matrix(rng.uniform(0, math.tau), 0.0, (x, 20.0, z)), 'live'))
    for (x, z), base, height, crater in (((1040.0, -380.0), 420.0, 520.0, 70.0), ((-520.0, 980.0), 330.0, 380.0, 50.0)):
        prof = [(base, 0), (base*.62, height*.42), (base*.32, height*.82), (crater*1.4, height), (crater, height*.97),
                (crater*.7, height*.9)]
        form = A.lathe(prof, sides=28, material='bark', rough=.06, seed=int(rng.integers(1 << 30)))
        out.append((form, RG._matrix(0.0, 0.0, (x, 10.0, z)), 'live'))
    return out


def breakouts(spec, ground, keep_out, seed):
    """Small lava breakouts glowing in hollows of the caldera's floor (flow
    lava: it burns like the flows), kept off the lanes and everything
    `keep_out` marks: (form, matrix) pairs; the builder drapes them on the
    ground."""
    f = spec['field']
    rng = np.random.default_rng(seed+1307)
    lanes = [L.polyline(points, 2.0)[0] for points in f['lanes']]
    x0, z0, x1, z1 = spec['ring']['fence']
    spots = []
    for x in np.arange(x0+30, x1-30, 7.0):
        for z in np.arange(z0+30, z1-30, 7.0):
            h = ground(x, z)
            ring = np.mean([ground(x+10*math.cos(a), z+10*math.sin(a)) for a in np.linspace(0, math.tau, 8, endpoint=False)])
            if ring-h < .5: continue
            if near_lines(x, z, lanes, f['lane_clear']*1.3): continue
            if keep_out(x, z, 7.0): continue
            spots.append((ring-h, x, z))
    spots.sort(reverse=True)
    out, taken = [], []
    for _, x, z in spots:
        if any(math.dist((x, z), p) < f['breakout_spacing'] for p in taken): continue
        taken.append((x, z))
        r = rng.uniform(2.5, 6.0)
        form = A.sheet((-r, -r*rng.uniform(.5, .9), 0), (r, r*rng.uniform(.5, .9), .05), int(rng.integers(1 << 30)),
                       lobes=int(rng.integers(3, 7)), segments=22, rings=3)
        out.append((form, RG._matrix(rng.uniform(0, math.tau), 0.0, (x, ground(x, z), z))))
        if len(out) >= f['breakouts']: break
    return out


def near_land(spec, seed):
    """Towering basalt spires and natural rock arches just past the tile,
    round the caldera's rim: (form, matrix, 'live'), nothing to hit."""
    rng = np.random.default_rng(seed+1409)
    out = []
    cx, cz = 326.0, 326.0
    for k in range(16):
        a = k/16*math.tau+rng.uniform(-.1, .1)
        d = rng.uniform(380, 470)
        x, z = cx+math.cos(a)*d, cz+math.sin(a)*d
        w = rng.uniform(24, 42); h = rng.uniform(110, 210)
        form = A.spindle((-w, -w*.8, -60), (w, w*.8, h), int(rng.integers(1 << 30)), sides=7, waist=.2, foot=.75,
                         jitter=.16, material='leaf', rings=7)
        out.append((form, RG._matrix(rng.uniform(0, math.tau), rng.uniform(-.06, .06), (x, 40.0, z)), 'live'))
    for k in range(5):
        a = (k+.5)/5*math.tau+rng.uniform(-.2, .2)
        d = rng.uniform(420, 480)
        mx, mz = cx+math.cos(a)*d, cz+math.sin(a)*d
        span = rng.uniform(120, 170); rise = rng.uniform(85, 130)
        t = a+math.pi/2+rng.uniform(-.5, .5)
        ux, uz = math.cos(t), math.sin(t)
        pts, rad = [], []
        for j in range(11):
            u = j/10
            pts.append((mx+ux*span*(u-.5), 20.0+rise*math.sin(math.pi*u)**.8, mz+uz*span*(u-.5)))
            rad.append(22.0-9.0*math.sin(math.pi*u))
        out.append((A.tube(pts, rad, sides=9, seed=int(rng.integers(1 << 30)), rough=.25, material='bark'), IDENT, 'live'))
    return out
