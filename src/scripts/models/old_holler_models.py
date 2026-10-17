"""Old Holler's structures as hard-surface models (docs/model-pipeline.md,
"Hard-surface structures"), built in Blender headless:

    blender -b -P scripts/models/old_holler_models.py -- OUT.glb [--texture 4096]

One file, one texture, a scene root per structure:

- `oh-base`: each base's look in its own frame (scripts/assets/old_holler_base.py):
  - the flag tower on its roof: a chamfered octagonal plinth and core,
    four pillars, the open deck 9 m over the roof with the flag's ring, four
    pylons and the canopy, the rocket turret's mount on top;
  - the entrance portal, armor panels and angled buttresses on the hall's
    walls, a fascia round the roof, lit edges on the ski ramps and the
    entrance trench.

- the field structures, each in its own frame
  (scripts/assets/old_holler_structures.py), fitted to the builder's solid
  shapes, which stand in for them where models aren't drawn:
  - `oh-lookout`: the lookout tower (team), a plated footing, a tapered
    shaft with split panels, lit channels and bands, the ring deck on
    gussets, posts and the roof the sensor stands on;
  - `oh-bunker`: the bunker, clad outside and in, a roof with an amber
    band, framed openings and lit ramps;
  - `oh-pad`: the landing pad (team), a chamfered rim with lights, the
    deck's team ring, target and chevrons, deploy marks and a console;
  - `oh-bridge`: the crossing's box girders, rails, cross beams and piers
    (its grated deck and ramps stay the builder's);
  - `oh-crossing`: the Crossing point's platform and its pier from the
    ravine (scripts/build-old-holler.py; the point's tower is shared).

The base's walls, roof and ramps stay the map builder's (drawn, lightmapped,
solid); this cladding stands just outside their faces. The flag tower is
the model's alone: the builder's solid geometry for it stands in where
models aren't drawn. Constants that bind to the builder are
noted where they do.
"""
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import Solid, beam, box, circle_y, finish, loft, quad_plate, ring_solid, root, setup, solid_box

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 4096
# One texture for the base and every field structure: plates drop their
# hidden backs, islands pack tightly, and the grate is coarse enough to read.
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.001, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.GRATE = (.3, .3)
hs.FLAT_TEXEL = .25
hs.SPREAD = 300.0             # the roots overlap round their origins (the bridge is 212 m): bake them apart

# old_holler_base.py: the hall, roof and entrance.
FLOOR, WALL_TOP, ROOF_TOP = -10.0, 7.4, 8.6
HALL_X, HALL_Z = 32.0, (-28.0, 28.0)      # the walls' outer faces
DOOR_X, DOOR_TOP = 10.0, 4.0              # the entrance's half width; the panel above it starts here
ROOF_X, ROOF_BACK = 32.6, 28.6            # the eaves
SHED_X0, SHED_Z1, SHED_ROOF = 19.6, 32.0, 3.4
SKI_X, SKI_W, SKI_Z = 22.0, 18.0, (-52.0, -28.0)
CHEEK_X = 9.0                             # the entrance trench's inner faces
# The flag tower on the roof.
TOWER = (0.0, 20.0)
PLINTH_R, PLINTH_FOOT, PLINTH_TOP = 7.5, ROOF_TOP, ROOF_TOP+1.6
CORE_R = 3.0
PILLAR_AT, PILLAR_R = 5.3, .55
DECK_R, DECK_TOP, DECK_T = 7.5, ROOF_TOP+9.0, 1.0
PYLON_AT, PYLON_R = 6.3, .4
CANOPY_R, CANOPY_Y, CANOPY_TOP = 6.8, DECK_TOP+8.0, DECK_TOP+9.4


def octagon_ring(cx, cz, r, y):
    """An octagon's corners round (cx, cz), its flats r from the middle."""
    return circle_y(cx, cz, r/math.cos(math.pi/8), y, 8, math.pi/8)


def octagon_solid(name, cx, cz, profile, mats, caps, bevel=.05):
    """A solid lofted through octagons: `profile` is (y, half width across
    the flats) going up, `mats` one per span."""
    s = Solid(name, bevel)
    loft(s, [octagon_ring(cx, cz, r, y) for y, r in profile], lambda k, i: mats[i] if isinstance(mats, list) else mats,
         caps)
    return s


def face_panel(cx, cz, r, k, y0, y1, u0, u1, mat, h=.1, bevel=.025, name='panel'):
    """A plate on face k of an octagonal prism (flats r from (cx, cz)),
    from y0 to y1, u0 to u1 across the face."""
    a = octagon_ring(cx, cz, r, 0.0)
    p, q = np.array(a[k]), np.array(a[(k+1) % 8])
    A, B = (p[0], y0, p[2]), (q[0], y0, q[2])
    C, D = (q[0], y1, q[2]), (p[0], y1, p[2])
    quad_plate(A, B, C, D, u0, u1, 0, 1, mat, h=h, away=(cx, (y0+y1)/2, cz), bevel=bevel, name=name)


# --- The flag tower ------------------------------------------------------------

def build_tower():
    tx, tz = TOWER
    # Plinth on the roof: a skirt at its foot, a chamfered top edge, team band
    # and a lit ring.
    f = PLINTH_FOOT
    octagon_solid('plinth', tx, tz, [(f, PLINTH_R+.4), (f+.25, PLINTH_R+.4), (f+.5, PLINTH_R), (PLINTH_TOP-.35, PLINTH_R),
                                     (PLINTH_TOP, PLINTH_R-.4)],
                  ['hull', 'steel', 'plate', 'steel'], ('hidden', 'plate'), bevel=.07)
    ring_solid('plinth-band', tx, tz, PLINTH_R/math.cos(math.pi/8)-.02, PLINTH_R/math.cos(math.pi/8)+.06, f+.75, f+1.05, 8, 'team')
    ring_solid('plinth-light', tx, tz, (PLINTH_R-.4)/math.cos(math.pi/8)-.05, (PLINTH_R-.4)/math.cos(math.pi/8)+.02,
               PLINTH_TOP, PLINTH_TOP+.04, 8, 'glow')
    # Core: chamfered at both ends, panels, lit strips up four faces, a team
    # collar under the deck.
    octagon_solid('core', tx, tz, [(PLINTH_TOP-.2, CORE_R+.6), (PLINTH_TOP+1.0, CORE_R), (DECK_TOP-DECK_T-1.6, CORE_R),
                                   (DECK_TOP-DECK_T-.4, CORE_R+.9), (DECK_TOP-DECK_T+.05, CORE_R+.9)],
                  ['steel', 'hull', 'steel', 'plate'], ('hidden', 'hidden'), bevel=.05)
    for k in range(8):
        face_panel(tx, tz, CORE_R, k, PLINTH_TOP+1.6, DECK_TOP-DECK_T-2.2, .12, .88, 'plate' if k % 2 else 'light', h=.12)
        if k % 2 == 0:
            face_panel(tx, tz, CORE_R+.12, k, PLINTH_TOP+2.2, DECK_TOP-DECK_T-2.8, .44, .56, 'glow', h=.06, bevel=0, name='light')
    ring_solid('core-band', tx, tz, CORE_R/math.cos(math.pi/8)-.02, CORE_R/math.cos(math.pi/8)+.08,
               DECK_TOP-DECK_T-2.0, DECK_TOP-DECK_T-1.7, 8, 'team')
    # Pillars on the diagonals, with collars.
    d = PILLAR_AT*math.sqrt(.5)
    for sx in (-1, 1):
        for sz in (-1, 1):
            px, pz = tx+sx*d, tz+sz*d
            octagon_solid('pillar', px, pz, [(PLINTH_TOP-.1, PILLAR_R+.25), (PLINTH_TOP+.5, PILLAR_R), (DECK_TOP-DECK_T-.5, PILLAR_R),
                                             (DECK_TOP-DECK_T+.02, PILLAR_R+.25)], ['steel', 'hull', 'steel'],
                          ('hidden', 'hidden'), bevel=.03)
    # The deck: chamfered underneath, grated on top, lit round its rim, the
    # flag's team ring in its middle.
    octagon_solid('deck', tx, tz, [(DECK_TOP-DECK_T, DECK_R-.9), (DECK_TOP-.45, DECK_R), (DECK_TOP-.005, DECK_R)],
                  ['plate', 'steel'], ('recess', 'grate'), bevel=.04)
    R = DECK_R/math.cos(math.pi/8)
    ring_solid('deck-light', tx, tz, R-.01, R+.03, DECK_TOP-.3, DECK_TOP-.16, 8, 'glow')
    ring_solid('flag-ring', tx, tz, 1.3, 1.7, DECK_TOP-.01, DECK_TOP+.012, 24, 'team')
    ring_solid('flag-ring', tx, tz, 1.85, 1.95, DECK_TOP-.01, DECK_TOP+.008, 24, 'glow')
    # Pylons up to the canopy, lit up their outer faces.
    e = PYLON_AT*math.sqrt(.5)
    for sx in (-1, 1):
        for sz in (-1, 1):
            px, pz = tx+sx*e, tz+sz*e
            octagon_solid('pylon', px, pz, [(DECK_TOP-.05, PYLON_R+.2), (DECK_TOP+.5, PYLON_R), (CANOPY_Y-.4, PYLON_R),
                                            (CANOPY_Y+.02, PYLON_R+.2)], ['steel', 'plate', 'steel'], ('hidden', 'hidden'),
                          bevel=.03)
            k = {(1, 1): 0, (-1, 1): 2, (-1, -1): 4, (1, -1): 6}[(sx, sz)]
            face_panel(px, pz, PYLON_R, k, DECK_TOP+1.2, CANOPY_Y-1.2, .3, .7, 'glow', h=.04, bevel=0, name='light')
    # The canopy: chamfered top and underside, team plates round its slope, a
    # lit ring underneath.
    octagon_solid('canopy', tx, tz, [(CANOPY_Y, CANOPY_R-.8), (CANOPY_Y+.5, CANOPY_R), (CANOPY_TOP-.5, CANOPY_R),
                                     (CANOPY_TOP-.005, CANOPY_R-.9)], ['steel', 'hull', 'steel'], ('glass', 'plate'), bevel=.05)
    for k in range(8):
        a = octagon_ring(tx, tz, CANOPY_R, CANOPY_TOP-.5)
        b = octagon_ring(tx, tz, CANOPY_R-.9, CANOPY_TOP-.005)
        quad_plate(a[k], a[(k+1) % 8], b[(k+1) % 8], b[k], .1, .9, .1, .85, 'team' if k % 2 == 0 else 'plate', h=.07,
                   away=(tx, CANOPY_Y-3, tz), bevel=.02)
    ring_solid('canopy-light', tx, tz, (CANOPY_R-.8)/math.cos(math.pi/8)-.3, (CANOPY_R-.8)/math.cos(math.pi/8),
               CANOPY_Y-.04, CANOPY_Y, 8, 'glow')
    # The rocket turret's mount on top is the map's (maps/old-holler.json): a
    # lit ring round it.
    ring_solid('canopy-mount-light', tx, tz, 2.35, 2.55, CANOPY_TOP-.01, CANOPY_TOP+.015, 24, 'glow')


# --- The base's outside ---------------------------------------------------------

def wall_cladding(axis, plane, out, u0, u1, step, gaps=(), y0=.0, y1=WALL_TOP, band='team'):
    """Armor panels and angled buttresses down a wall's outer face: axis 'x'
    is a wall at x = plane (u along z), 'z' one at z = plane (u along x);
    `out` the way it faces. Buttresses every `step`, panels between, a team
    band across them and lit strips on every other buttress."""
    def P(u, y, d):
        return (plane+out*d, y, u) if axis == 'x' else (u, y, plane+out*d)
    away = lambda u: P(u, (y0+y1)/2, -5)
    cuts = sorted(gaps)
    runs, at = [], u0
    for a, b in cuts:
        if a > at: runs.append((at, a))
        at = max(at, b)
    if u1 > at: runs.append((at, u1))
    for a, b in runs:
        n = max(1, round((b-a)/step))
        for i in range(n+1):
            u = a+(b-a)*i/n
            # A buttress: a wedge standing out 0.45 m at its foot, 0.15 at the top.
            s = Solid('buttress', .04)
            w = .55
            ring0 = [P(u-w, y0-.3, -.05), P(u+w, y0-.3, -.05), P(u+w*.7, y0-.3, .45), P(u-w*.7, y0-.3, .45)]
            ring1 = [P(u-w, y1+.05, -.05), P(u+w, y1+.05, -.05), P(u+w*.7, y1+.05, .15), P(u-w*.7, y1+.05, .15)]
            loft(s, [ring0, ring1], ['recess', 'steel', 'hull', 'steel'], caps=('hidden', 'steel'))
            if i % 2 == 0 and 0 < i < n:
                quad_plate(P(u-.12, y0+1.0, .32), P(u+.12, y0+1.0, .32), P(u+.12, y1-1.0, .2), P(u-.12, y1-1.0, .2),
                           0, 1, 0, 1, 'glow', h=.04, away=away(u), name='light')
            if i == n: break
            ua, ub = u+w+.12, u+(b-a)/n-w-.12
            if ub-ua < .5: continue
            tone = 'plate' if (i+int(a)) % 3 else 'light'
            quad_plate(P(ua, y0+.4, 0), P(ub, y0+.4, 0), P(ub, y1-2.2, 0), P(ua, y1-2.2, 0), 0, 1, 0, 1, tone, h=.12,
                       away=away((ua+ub)/2), bevel=.03)
            quad_plate(P(ua, y1-1.9, 0), P(ub, y1-1.9, 0), P(ub, y1-1.3, 0), P(ua, y1-1.3, 0), 0, 1, 0, 1, band, h=.14,
                       away=away((ua+ub)/2), bevel=.02)
            quad_plate(P(ua, y1-1.0, 0), P(ub, y1-1.0, 0), P(ub, y1-.15, 0), P(ua, y1-.15, 0), 0, 1, 0, 1, 'hull', h=.1,
                       away=away((ua+ub)/2), bevel=.02)


def fascia(axis, plane, out, u0, u1):
    """A chamfered band round the roof's edge (WALL_TOP to ROOF_TOP), lit
    along its foot."""
    def P(u, y, d):
        return (plane+out*d, y, u) if axis == 'x' else (u, y, plane+out*d)
    s = Solid('fascia', .03)
    sec = lambda u: [P(u, WALL_TOP-.15, -.05), P(u, WALL_TOP-.15, .35), P(u, ROOF_TOP-.2, .35), P(u, ROOF_TOP+.02, .1),
                     P(u, ROOF_TOP+.02, -.05)]
    loft(s, [sec(u0), sec(u1)], ['recess', 'steel', 'hull', 'steel', 'recess'], caps=('steel', 'steel'))
    s = hs.flat(Solid('fascia-light', 0), 'glow')
    lsec = lambda u: [P(u, WALL_TOP-.12, .35), P(u, WALL_TOP-.12, .39), P(u, WALL_TOP+.05, .39), P(u, WALL_TOP+.05, .35)]
    loft(s, [lsec(u0+.3), lsec(u1-.3)], 'glow', caps=('glow', 'glow'))


def build_base():
    # The hall's walls above ground: sides, back (the shed takes its east
    # end) and the front either side of the portal.
    for side in (-1, 1):
        wall_cladding('x', side*HALL_X, side, HALL_Z[0]+.6, HALL_Z[1]-.6, 7.0)
    wall_cladding('z', HALL_Z[1], 1, -HALL_X+.6, SHED_X0-.6, 7.0)
    for side in (-1, 1):
        a, b = sorted((side*(DOOR_X+2.6), side*(HALL_X-.6)))
        wall_cladding('z', HALL_Z[0], -1, a, b, 6.5)
    # Fascia round the roof.
    for side in (-1, 1):
        fascia('x', side*ROOF_X, side, -28.4, ROOF_BACK)
    fascia('z', ROOF_BACK, 1, -ROOF_X, ROOF_X)
    for side in (-1, 1):                       # the atrium is open between the halves
        fascia('z', HALL_Z[0], -1, *sorted((side*12.0, side*ROOF_X)))
    # The entrance portal: angled team jambs, a header over the door with a
    # chevron, lit edges, and lit strips down the trench.
    z = HALL_Z[0]
    for side in (-1, 1):
        s = Solid('portal-jamb', .05)
        x0, x1 = side*DOOR_X, side*(DOOR_X+2.4)
        ring0 = [(x0, -.3, z+.05), (x1, -.3, z+.05), (x1, -.3, z-.7), (x0, -.3, z-.35)]
        ring1 = [(x0, WALL_TOP, z+.05), (x1, WALL_TOP, z+.05), (x1, WALL_TOP, z-.45), (x0, WALL_TOP, z-.25)]
        loft(s, [[p if side > 0 else p for p in ring0], ring1], ['steel', 'recess', 'team', 'steel'], caps=('recess', 'steel'))
        quad_plate((side*(DOOR_X+.05), .4, z-.36), (side*(DOOR_X+.05), .4, z-.1), (side*(DOOR_X+.05), WALL_TOP-.4, z-.1),
                   (side*(DOOR_X+.05), WALL_TOP-.4, z-.26), 0, 1, 0, 1, 'glow', h=.04, away=(0.0, 3.0, z+3), name='light')
    s = Solid('portal-header', .05)
    hx = DOOR_X+2.4
    box(s, (-hx, DOOR_TOP, z-.55), (hx, WALL_TOP, z+.05), {'-z': 'plate', '-y': 'recess', '+y': 'steel', '*': 'steel'})
    for side in (-1, 1):
        quad_plate((side*.3, DOOR_TOP+.6, z-.55), (side*5.0, DOOR_TOP+.6, z-.55), (side*5.0, DOOR_TOP+1.4, z-.55),
                   (side*.3, DOOR_TOP+2.6, z-.55), 0, 1, 0, 1, 'team', h=.08, away=(0.0, 5.0, z+5), bevel=.02)
    quad_plate((-hx+.2, DOOR_TOP+.05, z-.55), (hx-.2, DOOR_TOP+.05, z-.55), (hx-.2, DOOR_TOP+.25, z-.55),
               (-hx+.2, DOOR_TOP+.25, z-.55), 0, 1, 0, 1, 'glow', h=.05, away=(0.0, 5.0, z+5), name='light')
    for side in (-1, 1):
        x = side*CHEEK_X
        for z0, z1 in ((-51.0, -40.5), (-39.5, -29.0)):
            quad_plate((x, -9.0, z0), (x, -9.0, z1), (x, -.6, z1), (x, -.6, z0), .02, .98, .05, .95, 'plate', h=.1,
                       away=(0.0, -5.0, (z0+z1)/2), bevel=.03)
        quad_plate((x, -1.3, -51.0), (x, -1.3, -29.0), (x, -1.05, -29.0), (x, -1.05, -51.0), 0, 1, 0, 1, 'glow', h=.13,
                   away=(0.0, -5.0, -40.0), name='light')
    # Lit edges and chevrons on the ski ramps.
    for side in (-1, 1):
        cx = side*SKI_X
        y_at = lambda zz: .03+(ROOF_TOP-.03)*(zz-SKI_Z[0])/(SKI_Z[1]-SKI_Z[0])
        for edge in (cx-SKI_W/2, cx+SKI_W/2):
            s = hs.flat(Solid('ski-edge', 0), 'glow')
            sec = lambda zz: [(edge-.12, y_at(zz)-.25, zz), (edge+.12, y_at(zz)-.25, zz), (edge+.12, y_at(zz)+.03, zz),
                              (edge-.12, y_at(zz)+.03, zz)]
            loft(s, [sec(SKI_Z[0]+.3), sec(SKI_Z[1]-.1)], 'glow', caps=('glow', 'glow'))
        for zz in np.arange(SKI_Z[0]+3.0, SKI_Z[1]-1.5, 3.2):
            for out in (-1, 1):
                A, B = (cx, y_at(zz+1.2), zz+1.2), (cx, y_at(zz+1.5), zz+1.5)
                C, D = (cx+out*3.0, y_at(zz+.3), zz+.3), (cx+out*3.0, y_at(zz), zz)
                quad_plate(A, B, C, D, 0, 1, 0, 1, 'glow', h=.015, away=(cx, y_at(zz)-5, zz), name='chevron')
    # The service shed against the back wall: panels and a lit door frame.
    wall_cladding('z', SHED_Z1, 1, SHED_X0+.4, HALL_X-.2, 6.0, y1=SHED_ROOF)


# --- Field structures ------------------------------------------------------------
# Each in its own frame, fitted to old_holler_structures.py's solid shapes
# (the kit's polygons have a corner on +x: phase 0, corner radii).

def kit_ring(r, y, n=8):
    return circle_y(0.0, 0.0, r, y, n, 0.0)


def annulus(name, r0, r1, y0, y1, n, mat, phase=0.0):
    """A flat ring of n sides, corner radii r0 to r1, y0 to y1."""
    s = Solid(name, 0, smooth=25)
    loft(s, [circle_y(0.0, 0.0, r, y, n, phase) for y, r in ((y0, r1), (y1, r1), (y1, r0), (y0, r0))], mat,
         caps=(None, None))
    a, b = len(s.v)-n, len(s.v)-4*n
    for k in range(n):
        s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)
    return hs.flat(s, mat)


def band(name, r, y0, y1, out, mat, n=8):
    """A band round a tapered kit prism whose corner radius is r(y),
    standing `out` proud of it."""
    s = Solid(name, 0, smooth=25)
    loft(s, [kit_ring(rr, y, n) for y, rr in ((y0, r(y0)+out), (y1, r(y1)+out), (y1, r(y1)-.03), (y0, r(y0)-.03))], mat,
         caps=(None, None))
    a, b = len(s.v)-n, len(s.v)-4*n
    for k in range(n):
        s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)
    return hs.flat(s, mat)


def plan_band(name, w, d, c, y0, y1, out, mat):
    """A band round a chamfered rectangle (half sizes w, d, corners cut c)."""
    s = Solid(name, 0, smooth=25)
    ring = lambda y, e: [(x, y, z) for x, z in hs.plan(-d-e, d+e, w+e, c, c)]
    loft(s, [ring(y0, out), ring(y1, out), ring(y1, -.02), ring(y0, -.02)], mat, caps=(None, None))
    a, b = len(s.v)-8, len(s.v)-32
    for k in range(8):
        s.face([a+k, a+(k+1) % 8, b+(k+1) % 8, b+k], mat)
    return hs.flat(s, mat)


def gusset(name, a, r0, r1, y0, y1, t, mat):
    """A triangular bracket in the vertical plane at angle a: from radius r0
    (y0 to y1) out to r1 at y1, t thick."""
    d = np.array([math.cos(a), 0.0, math.sin(a)])
    tg = np.array([-math.sin(a), 0.0, math.cos(a)])
    tri = [d*r0+[0, y0, 0], d*r0+[0, y1, 0], d*r1+[0, y1, 0]]
    s = Solid(name, .02)
    loft(s, [[p-tg*t/2 for p in tri], [p+tg*t/2 for p in tri]], mat, caps=(mat, mat))
    return s


def slab_ramp(name, x0, x1, z0, z1, y0, y1, t, top='grate', bevel=.03):
    """A sloped slab: its top y0 at z0 to y1 at z1, t thick."""
    s = Solid(name, bevel)
    v = s.pts([(x0, y0-t, z0), (x1, y0-t, z0), (x1, y1-t, z1), (x0, y1-t, z1),
               (x0, y0, z0), (x1, y0, z0), (x1, y1, z1), (x0, y1, z1)])
    for f, m in (((0, 1, 2, 3), 'hidden'), ((4, 7, 6, 5), top), ((0, 4, 5, 1), 'steel'), ((3, 2, 6, 7), 'steel'),
                 ((0, 3, 7, 4), 'hull'), ((1, 5, 6, 2), 'hull')):
        s.face([v[i] for i in f], m)
    return s


def flat_plate(x0, z0, x1, z1, y, mat, h=.012, name='decal'):
    """A thin plate lying on the level y."""
    return quad_plate((x0, y, z0), (x1, y, z0), (x1, y, z1), (x0, y, z1), 0, 1, 0, 1, mat, h=h,
                      away=((x0+x1)/2, y-5, (z0+z1)/2), name=name)


# The lookout tower: old_holler_structures.lookout.
LK_FOOT = lambda y: 7.0-.4*(y+3.0)           # the footing's corner radius, y -3 to 2
LK_SHAFT = lambda y: 3.5-.04*(y-2.0)         # the shaft's, y 2 to 22
LK_DECK, LK_DECK_TOP, LK_DECK_R = 22.0, 22.8, 7.0
LK_POST_X, LK_POST_W = 4.0, .6
LK_ROOF_Y, LK_ROOF_W, LK_ROOF_D = (27.2, 27.8), 5.5, 4.0


def build_lookout():
    hs.TEXEL = .7
    # Footing: the kit's frustum with a chamfered lip, armor plates and a
    # team band round its faces, a lit ring on top.
    s = Solid('lk-footing', .06)
    loft(s, [kit_ring(7.0, -3.0), kit_ring(LK_FOOT(1.8), 1.8), kit_ring(LK_FOOT(1.8)-.15, 2.0)],
         lambda k, i: ['hull', 'steel'][i], caps=('hidden', 'plate'))
    lo, hi = kit_ring(LK_FOOT(-.6), -.6), kit_ring(LK_FOOT(1.5), 1.5)
    for k in range(8):
        j = (k+1) % 8
        quad_plate(lo[k], lo[j], hi[j], hi[k], .1, .9, 0, .6, 'plate', h=.1, away=(0.0, .5, 0.0), bevel=.03)
        quad_plate(lo[k], lo[j], hi[j], hi[k], .06, .94, .7, .88, 'team', h=.12, away=(0.0, .5, 0.0), bevel=.02)
    annulus('lk-footing-light', 4.45, 4.7, 1.99, 2.03, 8, 'glow')
    # Shaft: the kit's taper, a collar at its foot and a capital flaring to
    # the deck; split panels with a lit channel on every other face, bands.
    s = Solid('lk-shaft', .05)
    prof = [(1.98, LK_SHAFT(2.0)+.35), (2.6, LK_SHAFT(2.6)+.35), (2.9, LK_SHAFT(2.9)), (19.6, LK_SHAFT(19.6)),
            (LK_DECK-.02, LK_SHAFT(19.6)+1.0)]
    loft(s, [kit_ring(r, y) for y, r in prof], lambda k, i: ['steel', 'steel', 'hull', 'plate'][i],
         caps=('hidden', 'hidden'))
    for y0, y1 in ((3.3, 8.65), (9.65, 15.45), (16.95, 19.3)):
        lo, hi = kit_ring(LK_SHAFT(y0), y0), kit_ring(LK_SHAFT(y1), y1)
        for k in range(8):
            j = (k+1) % 8
            if k % 2 == 0:
                for u0, u1 in ((.1, .42), (.58, .9)):
                    quad_plate(lo[k], lo[j], hi[j], hi[k], u0, u1, 0, 1, 'plate', h=.1, away=(0.0, y0, 0.0), bevel=.025)
                quad_plate(lo[k], lo[j], hi[j], hi[k], .47, .53, .04, .96, 'glow', h=.05, away=(0.0, y0, 0.0), name='light')
            else:
                quad_plate(lo[k], lo[j], hi[j], hi[k], .1, .9, 0, 1, 'light', h=.1, away=(0.0, y0, 0.0), bevel=.025)
    band('lk-band', LK_SHAFT, 8.8, 9.5, .16, 'steel')
    band('lk-band', LK_SHAFT, 15.6, 16.8, .18, 'team')
    # Gussets from the shaft's corners out under the deck.
    for k in range(8):
        gusset('lk-gusset', k*math.pi/4, LK_SHAFT(17.6)-.1, 6.1, 17.6, LK_DECK-.01, .32, 'steel')
    # The ring deck: chamfered underneath, grated, lit round its edge, a team
    # ring and a lit line inside the edge.
    hs.TEXEL = 1.0
    s = Solid('lk-deck', .04)
    loft(s, [kit_ring(6.1, LK_DECK-.02), kit_ring(LK_DECK_R, LK_DECK+.42), kit_ring(LK_DECK_R, LK_DECK_TOP-.005)],
         lambda k, i: ['plate', 'steel'][i], caps=('recess', 'grate'))
    band('lk-deck-light', lambda y: LK_DECK_R, 22.5, 22.62, .03, 'glow')
    annulus('lk-deck-team', 5.95, 6.45, LK_DECK_TOP-.01, LK_DECK_TOP+.012, 8, 'team')
    annulus('lk-deck-edge', 6.62, 6.76, LK_DECK_TOP-.01, LK_DECK_TOP+.008, 8, 'glow')
    # Posts with feet and lit outer faces.
    for sx in (-1, 1):
        x, w = sx*LK_POST_X, LK_POST_W/2+.03
        solid_box('lk-post', (x-w, LK_DECK_TOP-.01, -w), (x+w, LK_ROOF_Y[0]+.01, w), {'*': 'steel', '+y': 'hidden', '-y': 'hidden'},
                  bevel=.06)
        solid_box('lk-post-foot', (x-.5, LK_DECK_TOP-.01, -.5), (x+.5, LK_DECK_TOP+.28, .5), {'*': 'plate', '-y': 'hidden'},
                  bevel=.04)
        xo = x+sx*w
        quad_plate((xo, 23.4, -.12), (xo, 23.4, .12), (xo, 26.6, .12), (xo, 26.6, -.12), 0, 1, 0, 1, 'glow', h=.03,
                   away=(0.0, 25.0, 0.0), name='light')
    # The roof: a chamfered slab, lit round its edge, team plates at its
    # ends (the sensor stands in the middle), a light underneath.
    y0, y1 = LK_ROOF_Y
    w, d = LK_ROOF_W, LK_ROOF_D
    s = Solid('lk-roof', .04)
    rings = [[(x, y, z) for x, z in hs.plan(-dd, dd, ww, c, c)]
             for y, ww, dd, c in ((y0, w-.3, d-.3, .5), (y0+.3, w, d, .6), (y1-.005, w, d, .6))]
    loft(s, rings, lambda k, i: ['steel', 'hull'][i], caps=('recess', 'plate'))
    plan_band('lk-roof-light', w, d, .6, y0+.4, y0+.47, .02, 'glow')
    for sx in (-1, 1):
        a, b = sorted((sx*2.9, sx*(w-.5)))
        flat_plate(a, -(d-.6), b, d-.6, y1, 'team', name='lk-roof-team')
    quad_plate((-2.4, y0, -1.4), (2.4, y0, -1.4), (2.4, y0, 1.4), (-2.4, y0, 1.4), 0, 1, 0, 1, 'glow', h=.015,
               away=(0.0, y0+5, 0.0), name='light')


# The bunker: old_holler_structures.bunker.
BK_TOP, BK_CEIL, BK_FOOT = .4, 7.4, -.1
BK_OPEN_W, BK_OPEN_TOP = 6.0, 5.9


def build_bunker():
    T, C = BK_TOP, BK_CEIL
    # The floor slab: plated, a grated middle, amber edges at both openings.
    solid_box('bk-slab', (-11, -2.8, -10), (11, T, 10), {'*': 'hull', '+y': 'plate', '-y': 'hidden'}, bevel=.06)
    flat_plate(-7.5, -8.5, 7.5, 6.5, T, 'grate', h=.008, name='bk-grate')
    flat_plate(-8.8, -9.9, 8.8, -9.55, T, 'amber', h=.01, name='light')
    flat_plate(-2.8, 9.55, 2.8, 9.9, T, 'amber', h=.01, name='light')
    # Walls: plated inside, clad outside; the back wall's opening under a lintel.
    for sx in (-1, 1):
        x0, x1 = sorted((sx*9, sx*11))
        solid_box('bk-wall', (x0, T, -10), (x1, C, 8), {'*': 'hull', ('-x' if sx > 0 else '+x'): 'plate', '-y': 'hidden', '+y': 'hidden',
                                                        '+z': 'hidden'}, bevel=.05)
        b0, b1 = sorted((sx*BK_OPEN_W/2, sx*11))
        solid_box('bk-back', (b0, T, 8), (b1, C, 10), {'*': 'hull', '-z': 'plate', '-y': 'hidden', '+y': 'hidden'}, bevel=.05)
    solid_box('bk-lintel', (-BK_OPEN_W/2, BK_OPEN_TOP, 8), (BK_OPEN_W/2, C, 10), {'*': 'steel', '-y': 'recess', '+y': 'hidden'},
              bevel=.04)
    for sx in (-1, 1):
        wall_cladding('x', sx*11, sx, -9.4, 9.4, 4.7, y0=T, y1=C, band='grille')
        a, b = sorted((sx*(BK_OPEN_W/2+.6), sx*10.6))
        wall_cladding('z', 10, 1, a, b, 3.6, y0=T, y1=C, band='grille')
    quad_plate((-2.6, BK_OPEN_TOP+.45, 10), (2.6, BK_OPEN_TOP+.45, 10), (2.6, BK_OPEN_TOP+.65, 10), (-2.6, BK_OPEN_TOP+.65, 10),
               0, 1, 0, 1, 'amber', h=.04, away=(0.0, 3.0, 0.0), name='light')
    # Inside: panels down the side walls and either side of the back
    # opening, a lit line under the ceiling, ceiling lights.
    for sx in (-1, 1):
        x = sx*9
        for z0, z1 in ((-9.4, -3.6), (-3.2, 2.2), (2.6, 7.6)):
            quad_plate((x, T+.6, z0), (x, T+.6, z1), (x, C-1.3, z1), (x, C-1.3, z0), 0, 1, 0, 1, 'light', h=.08,
                       away=(sx*20.0, 3.0, (z0+z1)/2), bevel=.025)
        quad_plate((x, C-.95, -9.6), (x, C-.95, 7.8), (x, C-.8, 7.8), (x, C-.8, -9.6), 0, 1, 0, 1, 'glow', h=.04,
                   away=(sx*20.0, 3.0, 0.0), name='light')
        a, b = sorted((sx*(BK_OPEN_W/2+.4), sx*8.6))
        quad_plate((a, T+.6, 8), (b, T+.6, 8), (b, C-1.3, 8), (a, C-1.3, 8), 0, 1, 0, 1, 'light', h=.08,
                   away=((a+b)/2, 3.0, 20.0), bevel=.025)
    for x in (-5.0, 0.0, 5.0):
        quad_plate((x-.25, C, -7.5), (x+.25, C, -7.5), (x+.25, C, 5.5), (x-.25, C, 5.5), 0, 1, 0, 1, 'glow', h=.02,
                   away=(0.0, C+5, 0.0), name='light')
    # The front: jambs on the walls' ends, lit inside, and a header under
    # the roof's edge.
    for sx in (-1, 1):
        x0, x1 = sorted((sx*8.85, sx*11.1))
        solid_box('bk-jamb', (x0, T-.01, -10.4), (x1, C, -9.98), {'*': 'steel', '-z': 'plate', '+y': 'hidden'}, bevel=.05)
        xi = sx*8.85
        quad_plate((xi, T+.8, -10.3), (xi, T+.8, -10.05), (xi, C-.9, -10.05), (xi, C-.9, -10.3), 0, 1, 0, 1, 'glow', h=.03,
                   away=(sx*20.0, 3.0, -10.2), name='light')
    solid_box('bk-header', (-8.85, C-.45, -10.45), (8.85, C+.005, -9.98), {'*': 'steel', '-y': 'recess', '+y': 'hidden'}, bevel=.04)
    quad_plate((-8.4, C-.32, -10.45), (8.4, C-.32, -10.45), (8.4, C-.18, -10.45), (-8.4, C-.18, -10.45), 0, 1, 0, 1, 'amber',
               h=.03, away=(0.0, 3.0, 0.0), name='light')
    # The roof: a chamfered slab, an amber band round its edge, plated on top.
    s = Solid('bk-roof', .05)
    rings = [[(x, y, z) for x, z in hs.plan(-dd, dd, ww, c, c)]
             for y, ww, dd, c in ((C, 11.6, 10.6, .5), (C+.35, 12.0, 11.0, .8), (C+.995, 12.0, 11.0, .8))]
    loft(s, rings, lambda k, i: ['steel', 'hull'][i], caps=('hull', 'plate'))
    plan_band('bk-roof-light', 12.0, 11.0, .8, C+.5, C+.6, .02, 'amber')
    for x0, x1 in ((-10.8, -1.0), (1.0, 10.8)):
        flat_plate(x0, -9.8, x1, 9.8, C+1.0, 'light', h=.01, name='bk-roof-plate')
    # Ramps: grated, lit along their edges.
    slab_ramp('bk-ramp', -9, 9, -15, -10, BK_FOOT, T, .6)
    slab_ramp('bk-ramp', -BK_OPEN_W/2, BK_OPEN_W/2, 10, 15, T, BK_FOOT, .6)
    for xa, xb, z0, z1, ya, yb in ((-9, 9, -15, -10, BK_FOOT, T), (-BK_OPEN_W/2, BK_OPEN_W/2, 10, 15, T, BK_FOOT)):
        for e0, e1 in ((xa+.15, xa+.4), (xb-.4, xb-.15)):
            quad_plate((e0, ya, z0+.1), (e1, ya, z0+.1), (e1, yb, z1-.1), (e0, yb, z1-.1), 0, 1, 0, 1, 'amber', h=.012,
                       away=(0.0, -5.0, (z0+z1)/2), name='light')


# The landing pad: old_holler_structures.landing_pad.
PAD_R, PAD_RIM, PAD_DECK, PAD_TOP = 10.6, 11.0, 9.5, .47
PAD_SLOTS = [(-5.5, -5.5), (5.5, -5.5), (-5.5, 5.5), (4.5, 4.5)]
PAD_CONSOLE = ((7.2, 7.4), (8.8, 8.6), 2.2)


def build_pad():
    K = lambda r, y: kit_ring(r, y, 12)
    # The foundation and rim, chamfered, lit round its top.
    s = Solid('pad-base', .05)
    loft(s, [K(PAD_R, -4.0), K(PAD_R, -.05), K(PAD_RIM, -.05), K(PAD_RIM, .2), K(PAD_RIM-.2, .345)],
         lambda k, i: ['hull', 'recess', 'steel', 'steel'][i], caps=('hidden', 'plate'))
    annulus('pad-rim-light', 10.05, 10.3, .34, .358, 12, 'glow')
    # The deck: grated, a team ring near its edge, the target in the middle
    # with four chevrons pointing in, deploy marks.
    s = Solid('pad-deck', .03)
    loft(s, [K(PAD_DECK, .34), K(PAD_DECK, .42), K(PAD_DECK-.12, PAD_TOP-.002)], 'steel', caps=('hidden', 'grate'))
    annulus('pad-ring', 8.55, 9.05, PAD_TOP-.01, PAD_TOP+.012, 12, 'team')
    annulus('pad-target', 2.3, 2.9, PAD_TOP-.01, PAD_TOP+.012, 24, 'team', phase=None)
    for k in range(4):
        a = k*math.pi/2
        d, t = np.array([math.cos(a), 0, math.sin(a)]), np.array([-math.sin(a), 0, math.cos(a)])
        y = np.array([0, PAD_TOP, 0])
        tip, w = d*3.8+y, d*.5
        for side in (-1, 1):
            end = d*5.1+t*side*1.5+y
            quad_plate(tip, tip+w, end+w, end, 0, 1, 0, 1, 'team', h=.012, away=tuple(y-[0, 5, 0]), name='pad-chevron')
    for x, z in PAD_SLOTS:
        for a0, b0, a1, b1 in ((-.9, -.9, .9, -.75), (-.9, .75, .9, .9), (-.9, -.75, -.75, .75), (.75, -.75, .9, .75)):
            flat_plate(x+a0, z+b0, x+a1, z+b1, PAD_TOP, 'light', h=.01, name='pad-slot')
    for k in range(12):
        a = k*math.tau/12
        c = np.array([10.55*math.cos(a), .345, 10.55*math.sin(a)])
        flat_plate(c[0]-.15, c[2]-.15, c[0]+.15, c[2]+.15, .345, 'glow', h=.03, name='light')
    # The console: a kiosk sloping toward the pad, its screen on the slope,
    # a team stripe down its front.
    (x0, z0), (x1, z1), top = PAD_CONSOLE
    s = Solid('pad-console', .04)
    v = s.pts([(x0+.05, -.3, z0+.05), (x1-.05, -.3, z0+.05), (x1-.05, -.3, z1-.05), (x0+.05, -.3, z1-.05),
               (x0+.05, top-.45, z0+.05), (x1-.05, top-.45, z0+.05), (x1-.05, top-.02, z1-.05), (x0+.05, top-.02, z1-.05)])
    for f, m in (((0, 1, 2, 3), 'hidden'), ((4, 7, 6, 5), 'plate'), ((0, 4, 5, 1), 'steel'), ((3, 2, 6, 7), 'hull'),
                 ((0, 3, 7, 4), 'hull'), ((1, 5, 6, 2), 'hull')):
        s.face([v[i] for i in f], m)
    A, B, C, D = (np.array(s.v[i]) for i in (4, 5, 6, 7))
    quad_plate(A, B, C, D, .12, .88, .15, .85, 'glass', h=.02, away=((x0+x1)/2, 0.0, (z0+z1)/2), name='pad-screen')
    quad_plate(A, B, C, D, .18, .82, .62, .72, 'glow', h=.035, away=((x0+x1)/2, 0.0, (z0+z1)/2), name='light')
    quad_plate((x0+.55, .3, z0+.05), (x1-.55, .3, z0+.05), (x1-.55, top-.7, z0+.05), (x0+.55, top-.7, z0+.05), 0, 1, 0, 1,
               'team', h=.03, away=((x0+x1)/2, 1.0, z1), name='pad-stripe')


# The bridge: old_holler_structures.bridge (the deck and ramps are the builder's).
BR_W, BR_TOP, BR_BOT, BR_LEN = 6.5, .7, -.7, 106.0
BR_RAIL = (6.275, 6.725, 1.75)
BR_PIERS = [(z, sx) for z in (-64.0, 64.0) for sx in (-1, 1)]     # 2.2 x 4 m, x 4.4 to 6.6, down 48 m
BR_PIER_FOOT = -30.0      # drawn this far down: the banks under them stand 8 m or more higher
BR_BEAMS = range(-88, 100, 16)
# Long parts are built in pieces (an island longer than the texture would
# shrink every model's detail): girders break at stiffeners, rails at posts.
BR_GIRDER_BREAKS = [-BR_LEN, *range(-78, 91, 24), BR_LEN]
BR_RAIL_BREAKS = [-BR_LEN, *range(-88, 89, 16), BR_LEN]


def pieces(breaks):
    """Consecutive (z0, z1, cap at z0, cap at z1): the outer ends capped,
    the joints hidden."""
    return [(a, b, 'steel' if i == 0 else 'hidden', 'steel' if i == len(breaks)-2 else 'hidden')
            for i, (a, b) in enumerate(zip(breaks, breaks[1:]))]


def build_bridge():
    L = BR_LEN
    for sx in (-1, 1):
        # A box girder along each side, cladding the deck's edge, lit along
        # its face.
        hs.TEXEL = .6
        xi, xo = sx*(BR_W-.05), sx*(BR_W+.6)
        sec = [(xi, -2.4), (xo-sx*.25, -2.4), (xo, -2.0), (xo, .55), (xo-sx*.2, BR_TOP+.05), (xi, BR_TOP+.05)]
        mats = ['recess', 'steel', 'hull', 'steel', 'plate', 'recess']
        xf = xo+sx*.005
        for z0, z1, c0, c1 in pieces(BR_GIRDER_BREAKS):
            s = Solid('br-girder', .04)
            loft(s, [[(x, y, z0) for x, y in sec], [(x, y, z1) for x, y in sec]], mats, caps=(c0, c1))
            e0, e1 = z0+(1.0 if c0 == 'steel' else .35), z1-(1.0 if c1 == 'steel' else .35)
            quad_plate((xf, -.5, e0), (xf, -.5, e1), (xf, -.32, e1), (xf, -.32, e0), 0, 1, 0, 1, 'glow', h=.03,
                       away=(0.0, 0.0, 0.0), name='light')
        for z in np.arange(-L+4, L-3, 12.0):
            quad_plate((xf, -2.0, z-.15), (xf, -2.0, z+.15), (xf, -.75, z+.15), (xf, -.75, z-.15), 0, 1, 0, 1, 'steel',
                       h=.06, away=(0.0, 0.0, z), bevel=.015, name='br-stiffener')
        # The rail: a chamfered barrier, a lit line along its inner face,
        # posts every 8 m.
        hs.TEXEL = 1.0
        r0, r1, top = BR_RAIL
        a, b = sx*r0, sx*r1
        sec = [(a, BR_TOP-.02), (b, BR_TOP-.02), (b, top-.25), (b-sx*.1, top), (a+sx*.08, top), (a, top-.12)]
        mats = ['hidden', 'hull', 'steel', 'plate', 'steel', 'hull']
        for z0, z1, c0, c1 in pieces(BR_RAIL_BREAKS):
            s = Solid('br-rail', .04)
            loft(s, [[(x, y, z0) for x, y in sec], [(x, y, z1) for x, y in sec]], mats, caps=(c0, c1))
            e0, e1 = z0+(.5 if c0 == 'steel' else .4), z1-(.5 if c1 == 'steel' else .4)
            quad_plate((a, 1.3, e0), (a, 1.3, e1), (a, 1.42, e1), (a, 1.42, e0), 0, 1, 0, 1, 'glow', h=.025,
                       away=(sx*10.0, 1.0, 0.0), name='light')
        for z in np.arange(-L+2, L-1, 8.0):
            solid_box('br-post', (min(a, b)-.06, BR_TOP, z-.18), (max(a, b)+.06, top+.06, z+.18),
                      {'*': 'steel', '+y': 'plate', '-y': 'hidden'}, bevel=.03)
        # Lit studs along the deck beside the rail.
        for z in np.arange(-L+4, L-3, 8.0):
            xs = sx*(r0-.35)
            flat_plate(xs-.12, z-.3, xs+.12, z+.3, BR_TOP, 'glow', h=.015, name='light')
    # Cross beams under the deck: chamfered, tapering to their ends.
    hs.TEXEL = .5
    for z in BR_BEAMS:
        s = Solid('br-beam', .03)
        sec = lambda x, h: [(x, -.72, z-.75), (x, -.72, z+.75), (x, -.9-h, z+.6), (x, -.9-h, z-.6)]
        loft(s, [sec(-7.0, 1.6), sec(-4.0, 3.0), sec(4.0, 3.0), sec(7.0, 1.6)], lambda k, i: ['recess', 'hull', 'steel', 'hull'][k],
             caps=('steel', 'steel'))
    # Piers into the banks: chamfered, a cap under the girder, a lit strip
    # down the outer face.
    for z, sx in BR_PIERS:
        cx = sx*5.5
        s = Solid('br-pier', .06)
        ring = lambda y, w, d, c: [(cx+x, y, z+zz) for x, zz in hs.plan(-d, d, w, c, c)]
        loft(s, [ring(BR_PIER_FOOT, 1.1, 2.0, .5), ring(-3.2, 1.1, 2.0, .5), ring(-2.4, 1.6, 2.4, .5), ring(-.72, 1.6, 2.4, .5)],
             lambda k, i: ['hull', 'steel', 'plate'][i], caps=('hidden', 'recess'))
        xo = cx+sx*1.1
        quad_plate((xo, -22.0, z-.15), (xo, -30.0, z+.15), (xo, -4.0, z+.15), (xo, -4.0, z-.15), 0, 1, 0, 1, 'amber', h=.03,
                   away=(cx, -10.0, z), name='light')
    hs.TEXEL = 1.0


# The Crossing point's platform: build-old-holler.py's control_point (placed
# at the point at height 0; its tower is cnh_tower's, drawn by the builder).
CX_DECK, CX_R, CX_PIER = 105.7, 13.0, 4.0
CX_FOOT = 43.6            # the builder's pier foot: 2 m under the ravine floor there


def build_crossing():
    K = lambda r, y, n=12: kit_ring(r, y, n)
    # The pier: the kit's 12-sided column, chamfered ribs on every other
    # face, amber strips, a capital flaring under the platform.
    hs.TEXEL = .5
    s = Solid('cx-pier', .06)
    top = CX_DECK-1.4
    loft(s, [K(CX_PIER, CX_FOOT), K(CX_PIER, top-4.0), K(CX_PIER+1.6, top-.02)], lambda k, i: ['hull', 'steel'][i],
         caps=('hidden', 'hidden'))
    lo, hi = K(CX_PIER, CX_FOOT+1.0), K(CX_PIER, top-4.6)
    for k in range(12):
        j = (k+1) % 12
        if k % 2 == 0:
            quad_plate(lo[k], lo[j], hi[j], hi[k], .15, .85, 0, 1, 'plate', h=.14, away=(0.0, 70.0, 0.0), bevel=.03)
        else:
            quad_plate(lo[k], lo[j], hi[j], hi[k], .42, .58, .05, .95, 'amber', h=.04, away=(0.0, 70.0, 0.0), name='light')
    for y0 in (56.0, 78.0):
        band('cx-band', lambda y: CX_PIER, y0, y0+.8, .2, 'steel', n=12)
    # The platform: chamfered underneath, plated round a grated middle, lit
    # round its edge.
    hs.TEXEL = 1.0
    s = Solid('cx-platform', .05)
    loft(s, [K(CX_R-1.2, top-.02, 24), K(CX_R, top+.5, 24), K(CX_R, CX_DECK-.005, 24)], lambda k, i: ['plate', 'steel'][i],
         caps=('recess', 'plate'))
    s = Solid('cx-grate', 0, smooth=25)
    loft(s, [K(CX_R-2.2, CX_DECK-.01, 24), K(CX_R-2.2, CX_DECK+.006, 24)], 'plate', caps=('hidden', 'grate'))
    band('cx-light', lambda y: CX_R, top+.75, top+.88, .03, 'glow', n=24)
    annulus('cx-edge', CX_R-.75, CX_R-.55, CX_DECK-.01, CX_DECK+.01, 24, 'amber')


setup()
root('oh-base')
build_tower()
build_base()
root('oh-lookout')
build_lookout()
root('oh-bunker')
build_bunker()
root('oh-pad')
build_pad()
root('oh-bridge')
build_bridge()
root('oh-crossing')
build_crossing()
finish(OUT, TEX, generator='PeakRunner scripts/models/old_holler_models.py', material='old-holler')
