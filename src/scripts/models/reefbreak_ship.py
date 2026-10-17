"""Reefbreak's frigate as a hard-surface model (docs/model-pipeline.md,
"Hard-surface structures"), built in Blender headless:

    blender -b -P scripts/models/reefbreak_ship.py -- OUT.glb [--texture 4096]

A remeshed source is soft at every edge, and the pipeline's remesh softens
it further. The ship's concepts draw a vessel of flat plates and sharp
chines, so it is built here from their side and top views instead, in parts:

- the hull: one chined section lofted along it, scaled down to a single
  point at the raked prow and toward the transom (so every facet is flat),
  with the bay cut through both flanks;
- layered armor plates on the hull's bands, a few in team paint;
- the spine, the deck's vent grilles, the stepped bridge with its window
  band, the engine housing aft;
- two engine pods on pylons with ring nozzles and swept fins, two lower
  nozzles in the transom, six hover pods under the keel;
- the bay's frames and ledges, the side door's frame, running lights.

Every edge sharper than 30 degrees gets a chamfer in machined steel
(Blender's bevel, its new faces in the steel material), so edges catch the
light as the art direction asks; faces are shaded flat, round parts smooth.
One texture holds it all: each material's color, with ambient occlusion
baked in, and a glow texture whose alpha is the team mask.

Coordinates here are hull-local (scripts/assets/reefbreak_base.py): meters,
the hold floor at Y=0, the bow at -Z, port at -X. The file is written in the
structure's frame (base-local: the bow at +X, port at -Z). The rooms, doors
and openings that reefbreak_base.py fits inside are noted where they bind.
"""
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import (SOLIDS, Solid, beam, box, circle, circle_y, finish, loft, plan, quad_plate, revolve,
                         ring_solid, root, setup, solid_box, tier)

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 4096
# Hull-local (x, y, z) to the file's frame, base-local (-z, y, x).
hs.FRAME = lambda p: (-p[2], p[1], p[0])


# --- The hull ----------------------------------------------------------------
# Its half section (starboard), from the keel's middle round to the deck's:
# keel edge, lower hull, chine, the side (straight up from the chine to the
# top of the bay's openings), shoulder, deck edge. Over the midbody it is
# this; toward the bow it scales down onto the prow's tip, and toward the
# transom about the deck's middle, so each band stays one flat facet.
MID = ((0.0, -2.2), (5.4, -2.2), (7.5, -0.9), (8.2, 0.3), (8.2, 5.0), (7.3, 6.2), (6.6, 7.0), (0.0, 7.0))
TIP = (-42.0, 4.6)                       # z, y of the prow's point
Z_PROW, Z_AFT, Z_TRANSOM = -23.8, 28.0, 34.6
STERN_SCALE = 0.8                        # the transom's section, about (0, 7)
KEEL, LOWER, CHINE, SIDE, SHOULDER, EDGE, DECK = range(7)   # bands: MID[k] to MID[k+1]
BAND_MATS = ('hull', 'hull', 'hull', 'hull', 'hull', 'light', 'hull')
DECK_Y = MID[-1][1]


def half(z):
    if z <= Z_PROW:
        u = max((z-TIP[0])/(Z_PROW-TIP[0]), 0.0)
        return [(x*u, TIP[1]+(y-TIP[1])*u) for x, y in MID]
    if z <= Z_AFT:
        return list(MID)
    u = (z-Z_AFT)/(Z_TRANSOM-Z_AFT)
    s = 1+(STERN_SCALE-1)*u
    return [(x*s, DECK_Y+(y-DECK_Y)*s) for x, y in MID]


def ring(z):
    h = half(z)
    return [(x, y, z) for x, y in h+[(-x, y) for x, y in reversed(h[1:-1])]]


def ring_band(k):
    return k if k < 7 else 13-k


def deck_y(z):
    return half(z)[-1][1]


def band_point(z, band, t, side=1):
    h = half(z)
    (xa, ya), (xb, yb) = h[band], h[band+1]
    return np.array([side*(xa+(xb-xa)*t), ya+(yb-ya)*t, z])


def band_normal(z, band, t, side=1):
    e = 1e-3
    dz = band_point(z+e, band, t, side)-band_point(z-e, band, t, side)
    dt = band_point(z, band, min(t+e, 1), side)-band_point(z, band, max(t-e, 0), side)
    n = np.cross(dz, dt)
    n /= np.linalg.norm(n)
    p = band_point(z, band, t, side)
    return n if np.dot(n, p-np.array([0.0, 2.4, p[2]])) > 0 else -n


def plate(band, z0, z1, t0, t1, mat, h=0.12, inset=0.06, side=1, skew=(0.0, 0.0), bevel=0.035, name='plate'):
    """An armor plate on a band of the hull, from z0 to z1 along it and t0
    to t1 across it (fractions of the band, from its lower end); `skew`
    shifts its ends along Z at t1 (angled ends). Its foot sinks into the
    hull; its top is h proud, inset by `inset`, so its sides slope."""
    s = Solid(name, bevel)
    n = max(1, int(math.ceil((z1-z0)/4.0)))
    zm = (z0+z1)/2
    width = np.linalg.norm(band_point(zm, band, 1.0)-band_point(zm, band, 0.0))

    def outline(off, ins):
        dt = ins/width
        rows = []
        for t, za, zb in ((t0+dt, z0+ins, z1-ins), (t1-dt, z0+skew[0]+ins, z1+skew[1]-ins)):
            zs = [za+(zb-za)*i/n for i in range(n+1)]
            rows.append([band_point(z, band, t, side)+band_normal(z, band, t, side)*off for z in zs])
        return rows

    (b0, b1), (t0_, t1_) = outline(-0.05, 0.0), outline(h, inset)
    B0, B1, T0, T1 = s.pts(b0), s.pts(b1), s.pts(t0_), s.pts(t1_)
    for i in range(n):
        s.face([T0[i], T0[i+1], T1[i+1], T1[i]], mat)
        s.face([B0[i], B1[i], B1[i+1], B0[i+1]], 'recess')
        s.face([B0[i], B0[i+1], T0[i+1], T0[i]], mat)
        s.face([B1[i], T1[i], T1[i+1], B1[i+1]], mat)
    s.face([B0[0], T0[0], T1[0], B1[0]], mat)
    s.face([B0[n], B1[n], T1[n], T0[n]], mat)
    return s


def build_hull():
    s = Solid('hull', 0.09)
    mats = [BAND_MATS[ring_band(k)] for k in range(14)]
    loft(s, [ring(Z_PROW), ring(Z_AFT), ring(Z_TRANSOM)], mats,
         caps=(('tip', (0.0, TIP[1], TIP[0]), mats), 'plate'))
    return s


# The bay (reefbreak_base.py BAY_*): through both flanks from the floor
# (Y=0) to the roof's underside (5), over the openings' span; its end walls
# stand 1 m forward and 0.5 m aft of that, inside the hull.
BAY_OPEN, BAY_ROOF, BAY_ENDS, BAY_WALL_X = (-5.9, 6.7), 5.0, (-6.9, 7.2), 7.45


# The porthole from the balcony up through the bay's floor (reefbreak_base.py
# PORTHOLE: x, z, its octagon's half width; round here, to the octagon's
# flats).
PORTHOLE = (0.0, 0.4, 1.6)


def bay_cutter():
    """What's cut out of the hull for the bay: a box whose faces become the
    sill, the lintel's underside and the openings' reveals, and the
    porthole's round shaft from the keel up into it."""
    s = Solid('bay-cut', 0.0)
    box(s, (-20.0, 0.0, BAY_OPEN[0]), (20.0, BAY_ROOF, BAY_OPEN[1]),
        {'-y': 'plate', '+y': 'recess', '-z': 'plate', '+z': 'plate', '*': 'recess'})
    ox, oz, r = PORTHOLE
    loft(s, [[(ox+r*math.cos(2*math.pi*k/24), y, oz+r*math.sin(2*math.pi*k/24)) for k in range(24)] for y in (-3.0, .3)],
         'steel', caps=('steel', 'steel'))
    SOLIDS.remove(s)
    return s


def build_bay():
    for side in (-1, 1):
        # Between the bay's end walls and its openings the flank is solid: its
        # inner face, where the game's bay lining stops (|x| = BAY_WALL_X).
        for z0, z1 in ((BAY_ENDS[0], BAY_OPEN[0]-.05), (BAY_OPEN[1]+.05, BAY_ENDS[1])):
            x0, x1 = sorted((side*(BAY_WALL_X+.01), side*7.95))
            solid_box('bay-jamb', (x0, -.05, z0), (x1, BAY_ROOF+.05, z1), 'plate', bevel=0.0)
        # The frame: jambs on the side, the header on the shoulder.
        plate(SIDE, BAY_OPEN[0]-1.8, BAY_OPEN[0]-.05, 0.0, 1.0, 'team', h=.26, inset=.05, side=side, name='bay-frame')
        plate(SIDE, BAY_OPEN[1]+.05, BAY_OPEN[1]+1.8, 0.0, 1.0, 'team', h=.26, inset=.05, side=side, name='bay-frame')
        plate(SHOULDER, BAY_OPEN[0]-1.8, BAY_OPEN[1]+1.8, 0.02, .95, 'team', h=.24, inset=.05, side=side, name='bay-frame')
        for z0 in (BAY_OPEN[0]-1.05, BAY_OPEN[1]+.75):
            plate(SIDE, z0, z0+.3, .06, .94, 'glow', h=.29, inset=0, side=side, bevel=0, name='bay-light')
        # Lit edges round the opening, on the reveals and the lintel.
        xs = sorted((side*7.92, side*8.1))
        solid_box('bay-light', (xs[0], .25, BAY_OPEN[0]), (xs[1], BAY_ROOF-.25, BAY_OPEN[0]+.04), 'glow', 0)
        solid_box('bay-light', (xs[0], .25, BAY_OPEN[1]-.04), (xs[1], BAY_ROOF-.25, BAY_OPEN[1]), 'glow', 0)
        solid_box('bay-light', (xs[0], BAY_ROOF-.04, BAY_OPEN[0]+.25), (xs[1], BAY_ROOF, BAY_OPEN[1]-.25), 'glow', 0)
        # Amber marks at the sill's ends.
        for z0, z1 in ((BAY_OPEN[0]+.1, BAY_OPEN[0]+.9), (BAY_OPEN[1]-.9, BAY_OPEN[1]-.1)):
            xa = sorted((side*7.95, side*8.05))
            solid_box('bay-mark', (xa[0], -.02, z0), (xa[1], .02, z1), 'amber', 0)


# The ledges under both openings (reefbreak_base.py LEDGE, PLATFORM), the
# port one the entry platform where the flag stands: grated, level with the
# bay's floor (its top just under the sill's, which overlaps it), lit along
# the outer edge's face so the floor stays flat, on gussets.
LEDGE = (7.85, 11.0, -9.6, 10.4, -.005)
PLATFORM = 13.0


def build_ledges():
    x0, _, z0, z1, top = LEDGE
    for side, x1 in ((-1, PLATFORM), (1, LEDGE[1])):
        xa, xb = sorted((side*x0, side*x1))
        solid_box('ledge', (xa, top-.4, z0), (xb, top, z1),
                  {'+y': 'grate', '-y': 'plate', '*': 'steel'}, bevel=.03)
        xe = sorted((side*(x1-.01), side*(x1+.03)))
        solid_box('ledge-light', (xe[0], top-.24, z0+.1), (xe[1], top-.12, z1-.1), 'glow', 0)
        for z in (-8.8, -4.2, 0.4, 5.0, 9.6):
            s = Solid('gusset', .02)
            pts = [(side*7.4, top-.4, z), (side*(x1-.4), top-.4, z), (side*7.4, -2.0, z)]
            loft(s, [[(x, y, z-.15) for x, y, _ in pts], [(x, y, z+.15) for x, y, _ in pts]], 'plate',
                 caps=('plate', 'plate'))


# The gangway (reefbreak_base.py GANGWAY, base-local x0, x1, z at the
# platform, z at the foot): in this frame it runs out to port from the
# platform's edge (x = -13, the floor's height) down to its pad on the reef
# crest, across z = -5.5 to -1.5. The game's collision is a plain ramp, the
# columns' boxes and the pad's prism; this is its look: girders with team
# curbs and lit faces, lit chevrons pointing up to the flag, a keel beam and
# ribs beneath, two braced pairs of columns, and the pad with lit rings.
GANGWAY = (1.5, 5.5, -13.0, -58.0)
GANGWAY_Y, GANGWAY_PAD_R, GANGWAY_BENTS, SAND = -13.0, 6.0, (-25.0, -41.0), -21.25


def build_gangway():
    gx0, gx1, top, foot = GANGWAY
    w0, w1 = -gx1, -gx0                  # across it, this frame's z
    wc = (w0+w1)/2
    x_at = lambda f: top+(foot-top)*f     # f: 0 at the platform, 1 at the foot
    y_at = lambda f: GANGWAY_Y*f
    f_of = lambda x: (x-top)/(foot-top)

    def ring(f, pts):
        return [(x_at(f), y_at(f)+dy, w) for w, dy in pts]
    s = Solid('gangway-deck', .02)
    sec = [(w0+.1, -.3), (w1-.1, -.3), (w1-.1, -.005), (w0+.1, -.005)]
    loft(s, [ring(0, sec), ring(1, sec)], ['plate', 'steel', 'grate', 'steel'], caps=('steel', 'steel'))
    # The girders, rising to curbs either side, team paint along their tops.
    for edge, out in ((w0, -1), (w1, 1)):
        wi, wo = edge-out*.05, edge+out*.35
        sec = [(wi, -1.0), (wo, -1.0), (wo, .08), (wo-out*.16, .26), (wi, .26)]
        if out < 0:
            sec = sec[::-1]
        s = Solid('gangway-girder', .03)
        mats = ['plate', 'plate', 'steel', 'team', 'hull'] if out > 0 else ['team', 'steel', 'plate', 'plate', 'hull']
        loft(s, [ring(-.004, sec), ring(1.0, sec)], mats, caps=('plate', 'plate'))
        lw = sorted((wo+out*.005, wo+out*.03))
        s = Solid('gangway-light', 0)
        lsec = [(lw[0], -.42), (lw[1], -.42), (lw[1], -.3), (lw[0], -.3)]
        loft(s, [ring(.01, lsec), ring(.99, lsec)], 'glow', caps=('glow', 'glow'))
    # Lit chevrons on the deck, pointing up toward the platform.
    for f in np.arange(.06, .95, .075):
        for out in (-1, 1):
            we = wc+out*1.55
            df = .3/(top-foot)              # a chevron's arm, 0.3 m along
            A, B = (x_at(f), y_at(f), wc), (x_at(f+df), y_at(f+df), wc)
            C, D = (x_at(f+df*4.5), y_at(f+df*4.5), we), (x_at(f+df*3.5), y_at(f+df*3.5), we)
            quad_plate(A, B, C, D, 0, 1, 0, 1, 'glow', h=.012, away=(x_at(f), y_at(f)-5, wc), name='chevron')
    # Beneath: a keel beam and ribs.
    beam('gangway-keel', (x_at(0), y_at(0)-.7, wc), (x_at(1), y_at(1)-.7, wc), .7, .8, 'recess')
    for f in np.arange(.04, .99, .065):
        beam('gangway-rib', (x_at(f), y_at(f)-.62, w0+.15), (x_at(f), y_at(f)-.62, w1-.15), .22, .62, 'plate')
    # Two bents: a pair of tapered columns under the edges (the collision's
    # boxes), a cap beam, X bracing, lit collars at the waterline.
    for x in GANGWAY_BENTS:                 # base-local z is this frame's x
        f = f_of(x)
        ytop = y_at(f)-.95
        cols = (w0+.35, w1-.35)
        for w in cols:
            s = Solid('gangway-column', .03)
            loft(s, [circle_y(x, w, r, y, 8, 0) for y, r in ((SAND-2.0, .45), (ytop, .38))], 'plate',
                 caps=('recess', 'plate'))
            s = Solid('gangway-collar', .02, smooth=25)
            loft(s, [circle_y(x, w, r, y, 16) for y, r in ((SAND+.05, .62), (SAND+.32, .62), (SAND+.32, .5),
                                                            (SAND+.44, .5), (SAND+.44, .62), (SAND+.6, .62))],
                 lambda k, i: ['plate', 'recess', 'glow', 'recess', 'plate'][i], caps=('recess', 'steel'))
        beam('gangway-cap', (x, ytop-.15, w0-.1), (x, ytop-.15, w1+.1), .7, .45, 'plate')
        ylo = SAND+1.2
        beam('gangway-brace', (x+.05, ylo, cols[0]), (x+.05, ytop-.6, cols[1]), .16, .16, 'steel')
        beam('gangway-brace', (x-.05, ylo, cols[1]), (x-.05, ytop-.6, cols[0]), .16, .16, 'steel')
    # The pad at its foot, matching the collision's 12-sided prism.
    px, pw = foot-GANGWAY_PAD_R+2.0, wc
    s = Solid('gangway-pad', .04)
    loft(s, [circle_y(px, pw, r, y, 12, 0) for y, r in ((GANGWAY_Y-1.6, GANGWAY_PAD_R-.5), (GANGWAY_Y-.45, GANGWAY_PAD_R),
                                                         (GANGWAY_Y-.005, GANGWAY_PAD_R))],
         lambda k, i: ['plate', 'steel'][i], caps=('recess', 'plate'))
    for r0, r1, mat in ((4.55, 4.75, 'glow'), (5.1, 5.6, 'team')):
        s = Solid('pad-ring', 0)
        y0, y1 = GANGWAY_Y-.005, GANGWAY_Y+.008
        outer = [circle_y(px, pw, r1, y, 24) for y in (y0, y1)]
        inner = [circle_y(px, pw, r0, y, 24) for y in (y1, y0)]
        loft(s, outer+inner, mat, caps=(None, None))
        # A ring is a loop of quads: close it from the last ring to the first.
        n = 24
        a, b = len(s.v)-n, len(s.v)-4*n
        for k in range(n):
            s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)


def build_plates():
    # Prow: team plates either side of a pale ridge on its deck, team on
    # its shoulders, a pale plate along each flank with a lit strip.
    for side in (-1, 1):
        plate(DECK, -37.2, -25.0, .12, .66, 'team', side=side, skew=(-2.2, -1.2))
        plate(DECK, -39.6, -24.6, .93, 1.0, 'light', h=.1, inset=.02, side=side)
        plate(SHOULDER, -35.5, -24.4, .08, .9, 'plate', side=side, skew=(-1.0, -.4))
        plate(SIDE, -36.8, -24.4, .2, .78, 'light', side=side, skew=(-1.6, -.6))
        plate(SIDE, -32.0, -26.0, .56, .62, 'glow', h=.15, inset=0, side=side, bevel=0, name='light')
        plate(EDGE, -33.0, -24.4, .15, .85, 'plate', h=.08, inset=.03, side=side)

    # Flanks forward of the bay: an upper row, a lower row, and between the
    # lower ones a recess with pipes.
    upper = [(-23.5, -17.2, (0.0, -.9)), (-16.9, -12.4, (0, 0)), (-12.1, -8.0, (.6, 0))]
    lower = [(-23.5, -19.0, (0, .8)), (-10.6, -8.0, (-.8, 0))]
    # Aft of the bay; the starboard flank has the engine room's side door.
    aft_upper = [(8.6, 11.7, (0, 0)), (12.0, 16.4, (0, 0)), (16.7, 20.8, (.7, 0))]
    aft_lower = [(8.6, 11.7, (0, 0)), (12.0, 20.8, (0, 0))]
    for side in (-1, 1):
        door = [(24.6, 27.7, (0, -.9))] if side > 0 else [(21.1, 24.3, (0, 0)), (24.6, 27.7, (0, -.9))]
        tones = iter(['plate', 'light', 'plate', 'light', 'plate', 'plate', 'light'])
        for z0, z1, sk in upper+aft_upper+door:
            plate(SIDE, z0, z1, .56, .97, next(tones, 'plate'), side=side, skew=sk)
            for z in np.arange(z0+.8, z1-.8, 2.4):
                plate(SIDE, z, z+.6, .86, .9, 'glow', h=.15, inset=0, side=side, bevel=0, name='light')
        for z0, z1, sk in lower+aft_lower+door:
            plate(SIDE, z0, z1, .05, .36, 'plate', side=side, skew=sk)
        plate(SIDE, -18.8, -10.8, .05, .48, 'recess', h=.03, inset=0, side=side, bevel=0)
        # The belt: a steel rub strip along the flank between the rows.
        belt = [(-23.6, -7.75), (8.55, 27.9)] if side < 0 else [(-23.6, -7.75), (8.55, 20.8), (24.6, 27.9)]
        for z0, z1 in belt:
            plate(SIDE, z0, z1, .49, .545, 'steel', h=.2, inset=.04, side=side, bevel=.02, name='belt')
        for y in (1.0, 1.42, 1.84):
            revolve('pipe', side*8.4, y, [(-18.6, .12), (-11.0, .12)], 8, ['steel'], ('steel', 'steel'), smooth=50)
        for z in (-17.2, -14.8, -12.4):
            xa = sorted((side*8.15, side*8.62))
            solid_box('clamp', (xa[0], .8, z-.18), (xa[1], 2.05, z+.18), 'recess', .02)
        # The stern's taper, the lower hull, the shoulders.
        plate(SIDE, 28.3, 34.3, .05, .42, 'plate', side=side)
        for z0, z1 in ((-7.9, 8.5), (25.3, 27.6)):
            plate(LOWER, z0, z1, .12, .88, 'plate', h=.1, side=side)
        plate(SHOULDER, -23.4, -15.0, .08, .92, 'plate', side=side, skew=(0, -.6))
        plate(SHOULDER, -14.4, -8.3, .08, .92, 'light', side=side, skew=(.6, .6))
        plate(SHOULDER, 9.1, 14.6, .08, .92, 'light', side=side, skew=(-.6, -.6))
        plate(SHOULDER, 15.2, 20.8, .08, .92, 'plate', side=side)
        plate(SHOULDER, 21.3, 27.7, .08, .92, 'team', side=side, skew=(0, .8))
        for z in np.arange(-22.0, 27.0, 6.0):
            if BAY_OPEN[0]-2.5 < z < BAY_OPEN[1]+2.5:
                continue
            plate(EDGE, z, z+.5, .3, .7, 'glow', h=.03, inset=0, side=side, bevel=0, name='light')
        # Vent grilles on the deck either side of the spine.
        for z0, z1 in ((-22.4, -15.4), (-14.6, -8.4), (-5.0, 5.6)):
            plate(DECK, z0, z1, .2, .52, 'grille', h=.1, inset=.05, side=side)


# The engine room's side door (reefbreak_base.py SIDE_DOOR), starboard.
SIDE_DOOR = (21.6, 23.8, 3.2)


def build_side_door():
    z0, z1, top = SIDE_DOOR
    tt = (top+.6-MID[3][1])/(MID[4][1]-MID[3][1])
    th = (top+.1-MID[3][1])/(MID[4][1]-MID[3][1])
    plate(SIDE, z0-.7, z0-.1, 0.0, tt, 'light', h=.2, inset=.04, name='door-frame')
    plate(SIDE, z1+.1, z1+.7, 0.0, tt, 'light', h=.2, inset=.04, name='door-frame')
    plate(SIDE, z0-.1, z1+.1, th, tt, 'light', h=.2, inset=.04, name='door-frame')
    plate(SIDE, z0+.3, z1-.3, th+.02, th+.05, 'amber', h=.22, inset=0, bevel=0, name='door-mark')


SPINE_W, SPINE_TOP = 2.6, 7.45
# The deck hatch into the hold (reefbreak_base.py HATCH) opens in the spine.
HATCH = (-1.4, 1.4, -17.0, -14.0)


def build_spine():
    s = Solid('spine', .05)

    def sec(z, w, top):
        yb = deck_y(z)-.05
        c = min(.22, (top-yb)/2)
        return [(-w, yb, z), (w, yb, z), (w, top-c, z), (w-c, top, z), (-w+c, top, z), (-w, top-c, z)]
    rings = [sec(-26.3, 1.8, deck_y(-26.3)+.1), sec(-24.6, SPINE_W, SPINE_TOP), sec(Z_PROW, SPINE_W, SPINE_TOP),
             sec(9.9, SPINE_W, SPINE_TOP)]
    loft(s, rings, ['recess', 'plate', 'steel', 'light', 'steel', 'plate'], caps=('plate', 'plate'))
    top = SPINE_TOP
    for z0, z1 in ((-22.5, -18.6), (-12.6, -8.2), (-4.6, 5.0)):
        quad_plate((-1.2, top, z0), (1.2, top, z0), (1.2, top, z1), (-1.2, top, z1), 0, 1, 0, 1, 'grille', h=.04)
    for x in (-2.25, 2.25):
        for z0, z1 in ((-23.5, -17.8), (-13.2, -7.0), (-5.0, 5.4), (6.4, 9.2)):
            quad_plate((x-.06, top, z0), (x+.06, top, z0), (x+.06, top, z1), (x-.06, top, z1), 0, 1, 0, 1, 'glow', h=.02)
    # Caution marks at the hatch's corners.
    h0, h1, hz0, hz1 = HATCH
    for x0, x1 in ((h0-.7, h0-.15), (h1+.15, h1+.7)):
        for z0, z1 in ((hz0-.6, hz0-.2), (hz1+.2, hz1+.6)):
            quad_plate((x0, top, z0), (x1, top, z0), (x1, top, z1), (x0, top, z1), 0, 1, 0, 1, 'amber', h=.02)


# The bridge: two stepped tiers aft of the bay, over the engine room. Its
# roof carries the game's turret (reefbreak_base.py TURRET, Y=10.75) and the
# sensor on the mast step (SENSOR, Y=10.95).
ROOF_Y, MAST_STEP = 10.75, 10.95


def build_bridge():
    lo1, hi1 = plan(9.6, 28.6, 6.0, 2.2, .8), plan(10.5, 28.4, 5.65, 2.0, .7)
    tier('bridge-1', DECK_Y-.05, 9.0, lo1, hi1, ['hull', 'hull', 'plate', 'hull', 'plate', 'hull', 'plate', 'hull'], 'hull')
    lo2, hi2 = plan(12.4, 26.8, 4.7, 1.6, .6), plan(13.0, 26.6, 4.45, 1.5, .5)
    tier('bridge-2', 8.95, ROOF_Y, lo2, hi2, 'plate', 'plate')
    # The window band round its front and front corners, lit beneath.
    for k in (7, 0, 1):
        j = (k+1) % 8
        A, B = (lo2[k][0], 8.95, lo2[k][1]), (lo2[j][0], 8.95, lo2[j][1])
        C, D = (hi2[j][0], ROOF_Y, hi2[j][1]), (hi2[k][0], ROOF_Y, hi2[k][1])
        quad_plate(A, B, C, D, .03, .97, .28, .74, 'glass', h=.03, away=(0.0, 9.8, 19.0), name='window')
        quad_plate(A, B, C, D, .05, .95, .18, .22, 'glow', h=.03, away=(0.0, 9.8, 19.0), name='light')
    # Side windows near the front, a team plate aft, a pale plate below.
    for k in (2, 6):
        j = (k+1) % 8
        u = (lambda a, b: (a, b)) if k == 2 else (lambda a, b: (1-b, 1-a))   # u runs aft on port
        A, B = (lo2[k][0], 8.95, lo2[k][1]), (lo2[j][0], 8.95, lo2[j][1])
        C, D = (hi2[j][0], ROOF_Y, hi2[j][1]), (hi2[k][0], ROOF_Y, hi2[k][1])
        quad_plate(A, B, C, D, *u(.03, .3), .32, .7, 'glass', h=.03, away=(0.0, 9.8, 19.0), name='window')
        quad_plate(A, B, C, D, *u(.62, .97), .15, .85, 'team', h=.08, away=(0.0, 9.8, 19.0), bevel=.03)
        A, B = (lo1[k][0], DECK_Y, lo1[k][1]), (lo1[j][0], DECK_Y, lo1[j][1])
        C, D = (hi1[j][0], 9.0, hi1[j][1]), (hi1[k][0], 9.0, hi1[k][1])
        quad_plate(A, B, C, D, *u(.05, .4), .2, .8, 'light', h=.08, away=(0.0, 8.0, 19.0), bevel=.03)
    # Running lights along the lower tier's top edge.
    for x in (-5.5, 5.5):
        for z0 in (13.0, 18.0, 23.0):
            solid_box('light', (x-.08, 8.98, z0), (x+.08, 9.02, z0+1.2), 'glow', 0)
    # The mast step for the sensor, and two vent boxes on the roof.
    tier('mast-step', ROOF_Y-.05, MAST_STEP, plan(21.4, 25.2, 1.7, .5, .3), plan(21.5, 25.1, 1.6, .45, .25),
         'steel', 'steel', bevel=.02)
    for x in (-3.0, 3.0):
        tier('vent', ROOF_Y-.05, ROOF_Y+.35, plan(19.0, 25.0, .8, .2, .2), plan(19.1, 24.9, .7, .15, .15),
             'plate', 'grille', bevel=.03)


def build_stern():
    # The engine housing on deck aft of the bridge, with its grille.
    tier('housing', DECK_Y-.05, 8.3, plan(28.2, 34.3, 5.0, .6, .4), plan(28.4, 34.1, 4.7, .5, .3),
         'plate', 'light', bevel=.06)
    quad_plate((-3.4, 8.3, 29.6), (3.4, 8.3, 29.6), (3.4, 8.3, 33.4), (-3.4, 8.3, 33.4), 0, 1, 0, 1, 'grille', h=.05)
    for side in (-1, 1):
        cx, cy, pr = side*9.9, 5.6, 2.3
        # The engine pod: octagonal, a raised ring, then the nozzle, its
        # inside lit dimly and a bright ring just inside the lip.
        s = Solid('pod', .05)
        prof = [(27.0, 1.35), (28.7, pr), (34.9, pr), (34.9, 2.65), (36.3, 2.65), (36.3, 2.25), (37.0, 2.25)]
        loft(s, [circle(cx, cy, r, z, 8) for z, r in prof],
             lambda k, i: ['hull', 'hull', 'recess', 'plate', 'recess', 'hull'][i], caps=('recess', 'recess'))
        revolve('nozzle', cx, cy, [(36.8, 2.13), (37.3, 2.36), (39.7, 2.59), (39.75, 2.36), (39.45, 2.3), (37.8, 1.95)],
                24, ['steel', 'hull', 'steel', 'glow', 'dim'], ('recess', 'dim'))
        # Team paint on its top and outer upper faces.
        for k in (1, 0 if side > 0 else 2):
            a = circle(cx, cy, pr, 0, 8)
            p, q = a[k], a[k+1]
            quad_plate((p[0], p[1], 29.4), (q[0], q[1], 29.4), (q[0], q[1], 34.4), (p[0], p[1], 34.4), 0, 1, .06, .94,
                       'team', h=.08, away=(cx, cy, None), bevel=.025)
        # The pylon from the hull's side.
        s = Solid('pylon', .05)
        xi, xo = side*6.0, side*(9.9-1.5)
        loft(s, [[(xi, 4.6, 29.0), (xi, 4.6, 34.2), (xi, 6.1, 34.2), (xi, 6.1, 29.0)],
                 [(xo, 4.9, 29.6), (xo, 4.9, 33.6), (xo, 6.0, 33.6), (xo, 6.0, 29.6)]], 'plate', caps=('plate', 'plate'))
        # The swept fin, canted outward from the pod's top.
        cant = math.radians(18)
        d = np.array([side*math.sin(cant), math.cos(cant), 0.0])
        nf = np.array([math.cos(cant), -side*math.sin(cant), 0.0])
        root = np.array([cx, cy+pr*math.cos(math.pi/8)-.05, 0.0])

        def fin_ring(f, le, te, t):
            c = root+d*(6.4*f)
            L = te-le
            pts = [(le, 0), (le+.2*L, t), (te-.12*L, t), (te, 0), (te-.12*L, -t), (le+.2*L, -t)]
            return [c+nf*o+np.array([0, 0, z]) for z, o in pts]
        fin = lambda f: fin_ring(f, 31.2+(36.6-31.2)*f, 37.6+(39.7-37.6)*f, .22+(.14-.22)*f)
        s = Solid('fin', .03)
        loft(s, [fin(-.04), fin(.7), fin(1.0)], lambda k, i: ['hull', 'team'][i], caps=('recess', 'team'))
        a, b = fin(.42), fin(.66)
        i, j = (1, 2) if side > 0 else (5, 4)          # the outer face
        quad_plate(a[j], a[i], b[i], b[j], .05, .95, .3, .7, 'glow', h=.02, away=(0.0, root[1], None), name='light')
        # A lower nozzle in the transom.
        lx, ly = side*3.2, 1.6
        s = Solid('lower-housing', .04)
        loft(s, [circle(lx, ly, r, z, 8) for z, r in [(33.9, 1.55), (35.4, 1.55)]], 'plate', caps=('recess', 'steel'))
        revolve('nozzle', lx, ly, [(35.2, 1.2), (35.6, 1.35), (37.3, 1.48), (37.35, 1.32), (37.1, 1.28), (35.9, 1.1)],
                24, ['steel', 'hull', 'steel', 'glow', 'dim'], ('recess', 'dim'))
    # The transom's middle: a plate between the nozzles.
    quad_plate((-1.4, -.2, Z_TRANSOM), (1.4, -.2, Z_TRANSOM), (1.4, 5.8, Z_TRANSOM), (-1.4, 5.8, Z_TRANSOM),
               0, 1, 0, 1, 'light', h=.18, away=(0.0, 2.4, 0.0), bevel=.03)


HOVER_PODS = ((3.4, -27.5, -0.8), (4.2, 31.6, -1.2))   # reefbreak_base.py: x, z, the keel's height there


def build_hover_pods():
    for px, z, t in HOVER_PODS:
        for side in (-1, 1):
            cx = side*px
            solid_box('strut', (cx-.7, t-.75, z-.9), (cx+.7, t+.3, z+.9), 'recess', .03)
            s = Solid('hover-pod', .03, smooth=25)
            loft(s, [circle_y(cx, z, r, t+dy, 24) for dy, r in [(-.7, 1.7), (-1.0, 2.05), (-1.52, 2.05), (-1.52, 1.95),
                                                                (-1.66, 1.95), (-1.66, 2.05), (-1.75, 2.05), (-1.92, 1.75)]],
                 lambda k, i: ['hull', 'plate', 'recess', 'glow', 'recess', 'hull', 'steel'][i], caps=('recess', 'glow'))
            for dx, dz in ((0, 1), (0, -1), (1, 0), (-1, 0)):
                c = np.array([cx+dx*2.05, z+dz*2.05])
                w = np.array([.5 if dx == 0 else .25, .25 if dx == 0 else .5])
                solid_box('pod-cheek', (c[0]-w[0], t-1.5, c[1]-w[1]), (c[0]+w[0], t-.75, c[1]+w[1]), 'plate', .03)


# The middle and bottom levels (reefbreak_base.py): the station room's
# module hung under the hold (half width, z0, z1, bottom), the keel under
# the bridge holding the generator room, the balcony between them under the
# entry platform, and the doors that open on them.
MID_FLOOR = -6.0
STATION_MODULE = (6.4, -21.6, -8.2, -7.0)
KEEL_MODULE = (6.0, 8.8, 25.0, -12.6)
PORT_DOOR = (-14.6, -11.2)
MID_DOOR = (-2.0, 2.0, MID_FLOOR+3.4)
BALCONY = (-13.5, 5.0, -8.3, 8.9, MID_FLOOR)


def module(name, hx, z0, z1, bottom, top=-1.3, chamfer=.6, end_chamfer=0.0):
    """A chamfered box hung under the hull (sunk into it at the top), its
    bottom edges chamfered all round."""
    def ring(z, c):
        return [(-hx+chamfer, bottom+c, z), (hx-chamfer, bottom+c, z), (hx, bottom+chamfer+c, z), (hx, top, z),
                (-hx, top, z), (-hx, bottom+chamfer+c, z)]
    s = Solid(name, .06)
    rings = ([ring(z0, end_chamfer), ring(z0+end_chamfer, 0)] if end_chamfer else [ring(z0, 0)]) + \
            ([ring(z1-end_chamfer, 0), ring(z1, end_chamfer)] if end_chamfer else [ring(z1, 0)])
    loft(s, rings, ['plate', 'steel', 'hull', 'recess', 'hull', 'steel'], caps=('hull', 'hull'))
    return s


def side_plates(x, z0, z1, y0, y1, side, segments):
    """Plates and a lit strip down a module's flank (the plane |x| = x)."""
    for a, c in segments:
        quad_plate((side*x, y0, a), (side*x, y0, c), (side*x, y1, c), (side*x, y1, a), .03, .97, .06, .94, 'light' if (a*7) % 3 < 1 else 'plate',
                   h=.12, away=(0.0, (y0+y1)/2, None), bevel=.03)
    quad_plate((side*x, y1+.25, z0+.6), (side*x, y1+.25, z1-.6), (side*x, y1+.4, z1-.6), (side*x, y1+.4, z0+.6), 0, 1, 0, 1,
               'glow', h=.03, away=(0.0, y1, None), name='light')


def door_frame(face, plane, u0, u1, y0, top, mat, out):
    """Plates round a doorway in a flat face: face 'x' (the plane x = plane,
    u along z) or 'z' (z = plane, u along x); out: the way it faces."""
    def q(a, c, ya, yb, m, h):
        if face == 'x':
            pts = [(plane, ya, a), (plane, ya, c), (plane, yb, c), (plane, yb, a)]
            away = (plane-out, (ya+yb)/2, (a+c)/2)
        else:
            pts = [(a, ya, plane), (c, ya, plane), (c, yb, plane), (a, yb, plane)]
            away = ((a+c)/2, (ya+yb)/2, plane-out)
        quad_plate(*pts, 0, 1, 0, 1, m, h=h, away=away, bevel=.025 if h > .1 else 0, name='door-frame')
    q(u0-.7, u0-.06, y0, top+.6, mat, .22)
    q(u1+.06, u1+.7, y0, top+.6, mat, .22)
    q(u0-.06, u1+.06, top+.06, top+.6, mat, .22)
    for a in (u0-.42, u1+.27):
        q(a, a+.15, y0+.3, top+.2, 'glow', .25)


def build_modules():
    sx, sz0, sz1, sb = STATION_MODULE
    module('station-module', sx, sz0, sz1, sb)
    kx, kz0, kz1, kb = KEEL_MODULE
    module('keel', kx, kz0, kz1, kb, chamfer=.8, end_chamfer=.8)
    m0, m1, mtop = MID_DOOR
    pz0, pz1 = PORT_DOOR
    for side in (-1, 1):
        segs = [(sz0+.4, pz0-.9), (pz1+.9, sz1-.4)] if side < 0 else [(sz0+.4, -15.0), (-14.6, sz1-.4)]
        side_plates(sx, sz0, sz1, sb+1.4, -2.4, side, segs)
        side_plates(kx, kz0, kz1, kb+2.0, -2.6, side, [(kz0+1.2, 13.8), (14.2, 19.6), (20.0, kz1-1.2)])
    door_frame('x', -sx, pz0, pz1, MID_FLOOR, mtop, 'light', -1)
    door_frame('z', sz1, m0, m1, MID_FLOOR, mtop, 'light', 1)
    # The keel's face at the balcony's end: no door, a plain armored face
    # with a vent and a lit band across it.
    away = (0.0, MID_FLOOR, kz0+5)
    quad_plate((-4.6, MID_FLOOR+.6, kz0), (4.6, MID_FLOOR+.6, kz0), (4.6, -2.6, kz0), (-4.6, -2.6, kz0), 0, 1, 0, 1,
               'light', h=.14, away=away, bevel=.03)
    quad_plate((-2.2, MID_FLOOR+1.2, kz0), (2.2, MID_FLOOR+1.2, kz0), (2.2, MID_FLOOR+2.2, kz0), (-2.2, MID_FLOOR+2.2, kz0),
               0, 1, 0, 1, 'grille', h=.17, away=away, bevel=.02)
    quad_plate((-4.4, -2.45, kz0), (4.4, -2.45, kz0), (4.4, -2.3, kz0), (-4.4, -2.3, kz0), 0, 1, 0, 1, 'glow', h=.06,
               away=away, name='light')
    # Thrusters glowing under both.
    for (x, z), y in [((0.0, -18.2), sb), ((0.0, -11.6), sb), ((0.0, 13.0), kb), ((0.0, 21.0), kb)]:
        ring_solid('thruster', x, z, 1.6, 2.2, y-.06, y+.02, 24, 'glow')


def build_balcony():
    # The porthole's rim under the hull, lit, over the balcony.
    ox, oz, r = PORTHOLE
    ring_solid('porthole-rim', ox, oz, r+.02, r+.45, -2.27, -2.17, 24, 'glow')
    x0, x1, z0, z1, top = BALCONY
    solid_box('balcony', (x0, top-.4, z0), (x1, top-.005, z1), {'+y': 'grate', '-y': 'plate', '*': 'steel'}, bevel=.03)
    solid_box('balcony-light', (x0-.03, top-.24, z0+.3), (x0+.01, top-.12, z1-.3), 'glow', 0)
    solid_box('balcony-light', (x1-.01, top-.24, z0+.3), (x1+.03, top-.12, z1-.3), 'glow', 0)
    for z in (-6.4, 0.3, 7.0):
        beam('hanger', (x0+.6, -.4, z), (x0+.6, top, z), .26, .26, 'steel')
        beam('hanger', (x1-.6, -2.2, z), (x1-.6, top, z), .26, .26, 'steel')


# The stern lift (reefbreak_base.py LIFT, PLATFORM_BASE): glowing rings up a
# low-gravity column from the platform base's deck to a hatch under the aft
# room; the platform base, an octagonal deck hovering low under the stern
# with the vehicle pad.
LIFT = (1.5, 27.2, 30.2)
PLATFORM_BASE = (0.0, 33.0, 8.0, -17.0)


def build_lift():
    lw, lz0, lz1 = LIFT
    lz = (lz0+lz1)/2
    pcx, pcz, pr, ptop = PLATFORM_BASE
    keel = 7.0-9.2*half_scale(lz1)
    for y in np.arange(ptop+1.6, keel-.8, 2.5):
        ring_solid('lift-ring', 0.0, lz, 1.85, 2.15, y, y+.14, 24, 'glow')
    # The hatch's frame under the hull.
    for a, c, d, e in ((-lw-.4, -lw, lz0-.4, lz1+.4), (lw, lw+.4, lz0-.4, lz1+.4), (-lw, lw, lz0-.4, lz0), (-lw, lw, lz1, lz1+.4)):
        quad_plate((a, keel, d), (c, keel, d), (c, keel, e), (a, keel, e), 0, 1, 0, 1, 'glow', h=.03, away=(0.0, keel+5, lz))
    # The platform base: an octagon whose faces stand at its half width.
    R = pr/math.cos(math.pi/8)
    s = Solid('platform-base', .05)
    loft(s, [circle_y(pcx, pcz, r, y, 8) for y, r in ((ptop-.005, R), (ptop-.6, R), (ptop-1.3, R-.8))],
         lambda k, i: ['steel', 'plate'][i], caps=('grate', 'plate'))
    ring_solid('base-light', pcx, pcz, R-.01, R+.03, ptop-.3, ptop-.18, 8, 'glow')
    ring_solid('lift-foot', 0.0, lz, 2.05, 2.35, ptop, ptop+.015, 24, 'glow')
    # The vehicle pad, marked in team paint, aft on the deck.
    ring_solid('vehicle-pad', pcx, pcz+4.2, 3.0, 3.5, ptop, ptop+.015, 24, 'team')
    ring_solid('vehicle-pad', pcx, pcz+4.2, .0001, 3.0, ptop, ptop+.008, 24, 'plate')
    for x, z in ((-4.6, pcz-4.0), (4.6, pcz-4.0), (-4.6, pcz+4.6), (4.6, pcz+4.6)):
        s = Solid('base-thruster', .02, smooth=25)
        loft(s, [circle_y(x, z, r, y, 16) for y, r in ((ptop-1.2, 1.2), (ptop-1.6, 1.3), (ptop-2.0, 1.0))],
             lambda k, i: ['plate', 'steel'][i], caps=('recess', 'glow'))


def half_scale(z):
    """How far the hull's section is scaled down at z (1 before the stern)."""
    return 1.0 if z <= Z_AFT else 1+(STERN_SCALE-1)*(z-Z_AFT)/(Z_TRANSOM-Z_AFT)


setup()
root('reefbreak-ship')
hull = build_hull()
cutter = bay_cutter()
build_bay()
build_ledges()
build_gangway()
build_plates()
build_side_door()
build_spine()
build_bridge()
build_stern()
build_hover_pods()
build_modules()
build_balcony()
build_lift()
finish(OUT, TEX, {hull: cutter}, generator='PeakRunner scripts/models/reefbreak_ship.py', material='reefbreak-ship')
