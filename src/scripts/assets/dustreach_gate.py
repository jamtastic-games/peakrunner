"""The Sun Gate: Dustreach's neutral landmark on the central saddle, plus the
broken walls and columns scattered over the dunes.

A monumental arch of two sandstone piers under a deep lintel and an attic
spans the flag lane, which passes straight through its 12 m opening. Ruined
wing walls step down on either side and four standing columns frame it. The
lintel and attic tops are high ground. It is cover and a landmark only; no
neutral mechanic is attached. Original geometry.

Built at the caller's origin (ground level at the gate, Y=0) with no rotation.
Every piece has a partner under the map's 180-degree rotation (x, z) ->
(-x, -z), so neither team gets a better side.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'dustreach-gate-v2'

# The gate is the centre Capture & Hold point (its arch spans the flag lane,
# so no pylon goes there): eight render-only marker posts stand on the ring.
RING = 12.0
RING_POSTS = tuple(22.5+45*k for k in range(8))

FOOT = -2.0
PAVING = (20.0, 9.0)                                 # render-only paving, half x and z
PIER = (6.0, 12.0, 3.5)                              # |x| from, to; half depth
PIER_TOP, LINTEL_TOP, ATTIC_TOP = 17.0, 20.5, 22.0
LINTEL_X, LINTEL_Z, ATTIC_X, ATTIC_Z = 13.0, 4.0, 9.0, 3.0
WINGS = ((12.0, 17.0, 9.0), (17.0, 22.0, 6.0), (22.0, 26.0, 3.4))   # |x| from, to, top
WING_Z = 1.0
COLUMNS = ((30.0, 10.0), (30.0, -10.0), (-30.0, 10.0), (-30.0, -10.0))
COLUMN_R, COLUMN_TOP = 1.1, 10.0
STUMPS = ((22.0, -15.0, 1.2, 2.2), (-22.0, 15.0, 1.2, 2.2))       # x, z, r, top
SITE_R = 36.0

# Midfield ruins: (x, z, yaw degrees, kind) offsets from the map centre for
# the red half; each gets a partner at (-x, -z, yaw+180). Kinds: 'wall' is a
# broken wall 12 m long, 'columns' two standing columns and a stump.
SCATTER = ((-120.0, -330.0, 20.0, 'wall'), (135.0, -240.0, -35.0, 'wall'),
           (-170.0, -150.0, 70.0, 'columns'), (70.0, -130.0, 10.0, 'columns'),
           (170.0, -440.0, 55.0, 'wall'), (-70.0, -250.0, -15.0, 'columns'))
PIECE_R = 12.0

STONE, METAL, GLOW, DECK, TIMBER = 'concrete', 'trim', 'light', 'panel', 'grate'


def scatter():
    """All midfield pieces for both halves: (x, z, yaw radians, kind)."""
    out = []
    for x, z, yaw, kind in SCATTER:
        out.append((x, z, math.radians(yaw), kind))
        out.append((-x, -z, math.radians(yaw+180), kind))
    return out


def build(mesh):
    b = Builder(mesh, METAL, metal=METAL, glow=GLOW)
    wall = b.wall
    # Paving under the arch: render only, so the lane stays level ground.
    ax, az = PAVING
    mesh.quad((-ax, .05, -az), (-ax, .05, az), (ax, .05, az), (ax, .05, -az), DECK, False)
    mesh.quad((-ax, .06, -.4), (-ax, .06, .4), (ax, .06, .4), (ax, .06, -.4), METAL, False)
    bt = 0.0
    # Piers, lintel and attic.
    p0, p1, pd = PIER
    for s in (-1, 1):
        a, c = sorted((s*p0, s*p1))
        wall(a, c, -pd, pd, FOOT, PIER_TOP, STONE)
        for y in (bt+.5, 9.0, PIER_TOP-.6):          # iron bands round the piers
            for into in (-1, 1): b.face_box('z', into*pd, into, a, c, y, y+.35, .15, METAL)
        for into in (-1, 1):                          # a lit niche on each face
            b.face_box('z', into*pd, into, (a+c)/2-.6, (a+c)/2+.6, 4.0, 7.5, .05, GLOW)
            b.lamp(((a+c)/2, 5.5, into*(pd+2)), .6)
    wall(-LINTEL_X, LINTEL_X, -LINTEL_Z, LINTEL_Z, PIER_TOP, LINTEL_TOP, STONE)
    wall(-ATTIC_X, ATTIC_X, -ATTIC_Z, ATTIC_Z, LINTEL_TOP, ATTIC_TOP, STONE)
    for into in (-1, 1):
        b.face_box('z', into*LINTEL_Z, into, -LINTEL_X, LINTEL_X, PIER_TOP, PIER_TOP+.4, .2, METAL)
        b.face_box('z', into*LINTEL_Z, into, -LINTEL_X, LINTEL_X, LINTEL_TOP-.4, LINTEL_TOP, .2, METAL)
        # The sun disc on both faces of the lintel: rings of iron and glow.
        z = into*(LINTEL_Z+.06)
        cy = (PIER_TOP+LINTEL_TOP)/2
        for r, mat in [(1.6, METAL), (1.25, GLOW), (.7, DECK)]:
            pts = [(r*math.cos(i*math.tau/20), cy+r*math.sin(i*math.tau/20), z+into*(1.6-r)*.02) for i in range(20)]
            for i in range(20):
                tri = [(0, cy, pts[i][2]), pts[(i+1) % 20], pts[i]]
                mesh.triangle(tri if into < 0 else tri[::-1], mat, False)
        for dx in range(-7, 8, 2):                    # sun rays across the lintel face
            if dx == 0: continue
            b.face_box('z', into*LINTEL_Z, into, dx*1.05-.12, dx*1.05+.12, cy-.9, cy+.9, .05, METAL)
        b.lamp((0, cy, into*(LINTEL_Z+3)), .9)
    # Underside of the arch lit for the bake.
    b.lamp((0, PIER_TOP-1.5, 0), .8)
    # Ruined wing walls stepping down from each pier.
    for s in (-1, 1):
        for u0, u1, top in WINGS:
            a, c = sorted((s*u0, s*u1))
            wall(a, c, -WING_Z, WING_Z, FOOT, top, STONE)
    # Standing columns with capitals, and broken stumps.
    for x, z in COLUMNS:
        b.prism(x, z, COLUMN_R+.3, COLUMN_R+.3, FOOT, .6, 8, STONE, phase=math.pi/8)
        b.prism(x, z, COLUMN_R, COLUMN_R*.9, .6, COLUMN_TOP, 10, STONE)
        b.prism(x, z, COLUMN_R+.35, COLUMN_R+.35, COLUMN_TOP, COLUMN_TOP+.6, 8, STONE, phase=math.pi/8)
        b.prism(x, z, COLUMN_R+.04, COLUMN_R+.04, 5.0, 5.3, 10, METAL, solid=False, cap=False)
    for x, z, r, top in STUMPS:
        b.prism(x, z, r, r, FOOT, top, 10, STONE)
    # Capture ring markers: low bronze posts with glowing caps, render only.
    for deg in RING_POSTS:
        x, z = RING*math.cos(math.radians(deg)), RING*math.sin(math.radians(deg))
        b.prism(x, z, .22, .18, -.6, 1.3, 8, METAL, solid=False)
        b.prism(x, z, .26, .26, 1.3, 1.55, 8, GLOW, solid=False)
        b.lamp((x, 1.9, z), .35)
    return {
        'arch': (0.0, bt, 0.0),
        'lintel_top': (0.0, ATTIC_TOP, 0.0),
        'columns': [(x, COLUMN_TOP+.6, z) for x, z in COLUMNS],
        'view': (38.0, 9.0, -44.0),
    }


def build_piece(mesh, kind):
    """One midfield ruin at the caller's origin and yaw (ground at Y=0)."""
    b = Builder(mesh, METAL, metal=METAL, glow=GLOW)
    if kind == 'wall':
        for x0, x1, top in ((-6.0, -2.0, 4.8), (-2.0, 2.5, 3.1), (2.5, 6.0, 1.7)):
            b.wall(x0, x1, -.7, .7, FOOT, top, STONE)
        b.face_box('z', -.7, -1, -6.0, 6.0, 1.0, 1.3, .1, METAL)
    elif kind == 'columns':
        for x, z, top in ((-3.0, 0.0, 7.5), (3.0, 0.0, 5.2)):
            b.prism(x, z, 1.0, .9, FOOT, top, 10, STONE)
            b.prism(x, z, 1.04, 1.04, 2.6, 2.9, 10, METAL, solid=False, cap=False)
        b.prism(0.0, 3.5, 1.1, 1.1, FOOT, 1.2, 10, STONE)
    else:
        raise ValueError(kind)
