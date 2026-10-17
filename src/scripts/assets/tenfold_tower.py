"""Tenfold Tower: a duel server's lobby, a round tower of ten floors on a
plaza, each floor its own fighting floor for the blade (`sim::blade`), joined
by one spiral stair round a glowing core. Every floor is a blade zone: inside,
a blade in every hand and no jets; outside on the plaza it's quiet.

The floors, bottom to top, as test levels to dress later: a cargo bay
(crates to jump over, a catwalk), a neon alley (alley walls, puddles,
balconies), ruins (broken pillars, arches, rubble steps, dry pits), a
laboratory (glass chambers, consoles, conduits), a greenhouse (a flowing
water channel, planters, trees), a foundry (molten pits that kill, moving
platforms over them), a library (shelves, a mezzanine), a crystal cavern
(uneven ground, crystal pillars, a pool), a throne hall (pillars, a raised
dais) and the crown (an open deck under a canopy).

Local frame: origin at the tower's axis on the ground floor's top, y up; an
angle `a` is measured from +Z toward +X, so a polar point is
(r sin a, y, r cos a). Every floor's stair landing is at a = 0. All geometry is
original.

Floors are a grid of cells: rings (`RINGS`) by sectors (`SECTORS`), each cell
a height above the floor (raised: platforms and steps; below: pits, dry or
holding water or molten metal). One mesher turns a floor's cells into its
slab: tops, undersides and the faces where neighbors differ, merged along
each sector so a flat floor costs little.
"""
import math
import random

from assets.structure_kit import Builder

ASSET_ID = 'tenfold-tower-v1'
FLOORS = 10
STOREY = 8.0                 # floor to floor
SLAB = 0.5
R_CORE = 2.4                 # the glowing core the stair winds round
R_STAIR = 5.2                # the stair's outer edge: each floor's hole
R_IN, R_OUT = 18.0, 19.0     # the outer wall
RINGS = [R_STAIR+1.6*i for i in range(9)]          # 5.2 .. 18.0
SECTORS = 48                                       # 7.5 degrees each
SECTOR = math.tau/SECTORS
LANDING = 3                  # sectors each side of a = 0: the landing
STAIR_SEGMENTS = SECTORS-2*LANDING                 # 42, a = 22.5 .. 337.5 deg
STEP_UP = 0.3                # walkable steps (a body rolls up them)
RAIL_H = 1.0
CROWN_PARAPET = 1.2
CANOPY_Y = 9.0               # above the crown's floor
SPAWN_LIFT = 1.2
SKIRT = 0.35                 # the dark band round each floor's wall foot

# Extra material layers beyond the kit's (appended after the sky faces).
EXTRA = ('stone', 'marble', 'brass', 'crystal', 'neon', 'carpet', 'books', 'tile', 'crate', 'hazard', 'rust', 'steelblue',
         'copper', 'gold', 'screen')


def floor_y(k):
    """Top of floor k (1 to 10), local."""
    return STOREY*(k-1)


def pol(r, a, y=0.0):
    return (r*math.sin(a), y, r*math.cos(a))


def deg(d):
    return math.radians(d)


def sector_angle(s):
    return s*SECTOR


def ring_mid(j):
    return (RINGS[j]+RINGS[j+1])/2


def _normal(pts):
    (ax, ay, az), (bx, by, bz), (cx, cy, cz) = pts[0], pts[1], pts[2]
    u = (bx-ax, by-ay, bz-az); v = (cx-ax, cy-ay, cz-az)
    return (u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0])


def face(mesh, pts, mat, toward, solid=True):
    """A quad or triangle facing `toward` (its winding fixed to suit)."""
    n = _normal(pts)
    if n[0]*toward[0]+n[1]*toward[1]+n[2]*toward[2] < 0: pts = pts[::-1]
    if len(pts) == 4: mesh.quad(*pts, mat, solid)
    else: mesh.triangle(list(pts), mat, solid)


class Placed:
    """Move the mesh's frame to a spot (local to the tower) for a while."""
    def __init__(self, mesh, base, x, z, yaw, y=0.0):
        self.mesh, self.base, self.at, self.yaw = mesh, base, (x, y, z), yaw

    def __enter__(self):
        m = self.mesh
        self.saved = (m.origin, m.yaw)
        m.origin = (self.base[0]+self.at[0], self.base[1]+self.at[1], self.base[2]+self.at[2])
        m.yaw = self.yaw
        return m

    def __exit__(self, *_):
        self.mesh.origin, self.mesh.yaw = self.saved


# ---- One floor's plan -------------------------------------------------------

class Plan:
    """A floor's cells and what stands on them. `cells[j][s]` is (height above
    the floor, top material); pits list their liquid for the manifest."""
    def __init__(self, k, top='concrete'):
        self.k, self.y = k, floor_y(k)
        self.cells = [[(0.0, top) for _ in range(SECTORS)] for _ in range(len(RINGS)-1)]
        self.pools = []        # (j0, j1, s0, s1, surface offset, depth, kind, extras)
        self.pits = []         # (j0, j1, s0, s1, depth), dry or not
        self.spawns = []       # (r, a) for the blade zone
        self.dress = []        # callables(mesh, builder, base y)
        self.strips = (9.6, 15.8)   # lit rings under this floor's ceiling
        self.ceiling = 'panel'      # the floor above's underside, as seen from here
        self.above = None           # the floor above's plan (set by `plans`)
        self.ambient = None         # the baked sky light's tint indoors (None: as outside)
        self.gain = 1.0             # this floor's lamps, brighter or dimmer

    def ceiling_at(self, r, a):
        """The ceiling's height (local) over polar (r, a): the floor above's
        underside, lower under its pits; open over the stair's hole."""
        if self.above is None: return self.y+CANOPY_Y
        if r < R_STAIR: return self.y+STOREY
        j = max(0, min(len(RINGS)-2, int((r-RINGS[0])/1.6)))
        return self.above.y+min(self.above.cells[j][int((a % math.tau)/SECTOR) % SECTORS][0], 0.0)-SLAB

    def raise_(self, j0, j1, s0, s1, h, mat):
        for j in range(j0, j1):
            for s in range(s0, s1): self.cells[j][s % SECTORS] = (h, mat)

    def platform(self, j0, j1, s0, s1, h, mat, up=None):
        """A raised platform `h` high, with walkable steps up to it from the
        sector side `up` says: 'before' (rising toward s0), 'after' (down
        from s1) or 'both'."""
        self.raise_(j0, j1, s0, s1, h, mat)
        n = int(math.ceil(h/STEP_UP-1e-6))-1
        if up in ('before', 'both'): self.steps(j0, j1, s0-n, s0, h-STEP_UP, mat)
        if up in ('after', 'both'): self.steps(j0, j1, s1, s1+n, h-STEP_UP, mat, rising=-1)

    def pit(self, j0, j1, s0, s1, depth, mat, liquid=None, surface=None, **extras):
        """A pit `depth` deep; with a liquid, water 15 cm under the floor (or
        at `surface`) or molten metal."""
        self.raise_(j0, j1, s0, s1, -depth, mat)
        self.pits.append((j0, j1, s0, s1, depth))
        if surface is None: surface = -0.15
        if liquid:
            # One volume per run of at most six sectors (45 degrees): their
            # outlines stay star-shaped about their centroids.
            s = s0
            while s < s1:
                e = min(s1, s+6)
                self.pools.append((j0, j1, s, e, surface, depth+surface, liquid, extras))
                s = e

    def steps(self, j0, j1, s0, s1, top, mat, rising=1):
        """Walkable steps along the sectors, `STEP_UP` each, up to `top`."""
        n = len(range(s0, s1))
        for i, s in enumerate(range(s0, s1)):
            k = i+1 if rising > 0 else n-i
            self.raise_(j0, j1, s, s+1, round(min(top, STEP_UP*k), 3), mat)


def landing_clear(j, s):
    """The apron off the stair's landing stays open on every floor."""
    return j <= 2 and (s <= LANDING+1 or s >= SECTORS-LANDING-2)


# ---- The mesher ----------------------------------------------------------------

def mesh_floor(mesh, plan, skip_inner, under='panel'):
    """The floor's slab from its cells: each sector's tops and undersides
    (`under`: the ceiling of the floor below) merged along the rings, and
    the faces between differing neighbors."""
    cells, y0 = plan.cells, plan.y
    nr = len(cells)
    top = lambda j, s: y0+cells[j][s % SECTORS][0]
    bottom = lambda j, s: min(top(j, s), y0)-SLAB
    sub = 1                                      # arc pieces per sector
    def arc(r, a0, a1, y):
        return [pol(r, a0+(a1-a0)*i/sub, y) for i in range(sub+1)]
    for s in range(SECTORS):
        a0, a1 = sector_angle(s), sector_angle(s+1)
        # Tops, merged along rings of equal height and material.
        j = 0
        while j < nr:
            h, mat = cells[j][s]
            e = j+1
            while e < nr and cells[e][s] == (h, mat): e += 1
            r0, r1 = RINGS[j], RINGS[e]
            inner, outer = arc(r0, a0, a1, y0+h), arc(r1, a0, a1, y0+h)
            for i in range(sub):
                face(mesh, [inner[i], outer[i], outer[i+1], inner[i+1]], mat, (0, 1, 0))
            j = e
        # Undersides, merged likewise.
        j = 0
        while j < nr:
            b = bottom(j, s)
            e = j+1
            while e < nr and bottom(e, s) == b: e += 1
            inner, outer = arc(RINGS[j], a0, a1, b), arc(RINGS[e], a0, a1, b)
            for i in range(sub):
                face(mesh, [inner[i], outer[i], outer[i+1], inner[i+1]], under, (0, -1, 0))
            j = e
        # Faces between rings (arcs at RINGS[j]), and the hole's edge.
        for j in range(nr+1):
            r = RINGS[j]
            if j == 0:
                if skip_inner(s): continue
                lo, hi = bottom(0, s), top(0, s)
                pts_lo, pts_hi = arc(r, a0, a1, lo), arc(r, a0, a1, hi)
                for i in range(sub):
                    c = pol(r-1, (a0+a1)/2)
                    face(mesh, [pts_lo[i], pts_lo[i+1], pts_hi[i+1], pts_hi[i]], 'trim', (c[0], 0, c[2]))
                continue
            if j == nr: continue                         # against the outer wall
            _edge(mesh, arc(r, a0, a1, 0), top(j-1, s), top(j, s), bottom(j-1, s), bottom(j, s),
                  pol(r-1, (a0+a1)/2), pol(r+1, (a0+a1)/2), cells[j-1][s][1], cells[j][s][1], sub)
        # Faces between sectors (radial lines at a0).
        for j in range(nr):
            p, q = cells[j][(s-1) % SECTORS], cells[j][s]
            line = [pol(RINGS[j], a0, 0), pol(RINGS[j+1], a0, 0)]
            before, after = pol(ring_mid(j), a0-.05), pol(ring_mid(j), a0+.05)
            _edge(mesh, line, top(j, s-1), top(j, s), bottom(j, s-1), bottom(j, s), before, after, p[1], q[1], 1)


def _edge(mesh, line, top_a, top_b, bot_a, bot_b, side_a, side_b, mat_a, mat_b, sub):
    """Vertical faces along `line` (points at y 0) between cell A (toward
    `side_a`) and cell B: the step between their tops, facing the lower, and
    the step between their undersides, facing the shallower."""
    def wall(y0, y1, toward, mat):
        if y1-y0 < 1e-4: return
        for i in range(sub):
            p, q = line[i], line[i+1]
            face(mesh, [(p[0], y0, p[2]), (q[0], y0, q[2]), (q[0], y1, q[2]), (p[0], y1, p[2])], mat,
                 (toward[0]-p[0], 0, toward[2]-p[2]))
    if top_a > top_b: wall(top_b, top_a, side_b, mat_a if mat_a not in ('water',) else 'trim')
    elif top_b > top_a: wall(top_a, top_b, side_a, mat_b)
    if bot_a < bot_b: wall(bot_a, bot_b, side_b, 'trim')
    elif bot_b < bot_a: wall(bot_b, bot_a, side_a, 'trim')


# ---- Pieces --------------------------------------------------------------------

def ring_band(mesh, r0, r1, a0, a1, y0, y1, mat, solid=True, segs=None, ends=True, faces='ioub', collide=None):
    """An annular-sector solid (a ring of wall, a rail, a band): its inner,
    outer, upper and bottom faces as `faces` says, and its ends; `collide`
    says which of them are solid (default: all, when `solid`)."""
    segs = segs or max(1, int(math.ceil((a1-a0)/deg(7.5))))
    collide = (faces+'e' if solid else '') if collide is None else collide
    pts = lambda r, y: [pol(r, a0+(a1-a0)*i/segs, y) for i in range(segs+1)]
    i0, i1, o0, o1 = pts(r0, y0), pts(r0, y1), pts(r1, y0), pts(r1, y1)
    mid = lambda i: (a0+(a1-a0)*(i+.5)/segs)
    for i in range(segs):
        c = pol(1, mid(i))
        if 'o' in faces: face(mesh, [o0[i], o0[i+1], o1[i+1], o1[i]], mat, c, 'o' in collide)
        if 'i' in faces: face(mesh, [i0[i], i0[i+1], i1[i+1], i1[i]], mat, (-c[0], 0, -c[2]), 'i' in collide)
        if 'u' in faces: face(mesh, [i1[i], o1[i], o1[i+1], i1[i+1]], mat, (0, 1, 0), 'u' in collide)
        if 'b' in faces: face(mesh, [i0[i], o0[i], o0[i+1], i0[i+1]], mat, (0, -1, 0), 'b' in collide)
    if ends and (a1-a0) < math.tau-1e-6:
        for a, sgn in ((a0, -1), (a1, 1)):
            t = (math.cos(a)*sgn, 0, -math.sin(a)*sgn)
            face(mesh, [pol(r0, a, y0), pol(r1, a, y0), pol(r1, a, y1), pol(r0, a, y1)], mat, t, 'e' in collide)


def collide_band(mesh, r0, r1, a0, a1, y0, y1, faces='io', segs=1):
    """Collision only, in a band's shape (its faces as `ring_band`'s)."""
    for i in range(segs):
        p, q = a0+(a1-a0)*i/segs, a0+(a1-a0)*(i+1)/segs
        quads = {'i': [pol(r0, p, y0), pol(r0, q, y0), pol(r0, q, y1), pol(r0, p, y1)],
                 'o': [pol(r1, p, y0), pol(r1, q, y0), pol(r1, q, y1), pol(r1, p, y1)],
                 'u': [pol(r0, p, y1), pol(r1, p, y1), pol(r1, q, y1), pol(r0, q, y1)],
                 'b': [pol(r0, p, y0), pol(r1, p, y0), pol(r1, q, y0), pol(r0, q, y0)]}
        for f in faces:
            pts = [mesh.point(v) for v in quads[f]]
            for tri in ((0, 1, 2), (0, 2, 3)): mesh.collision.extend(c for k in tri for c in pts[k])


def cylinder(mesh, r, y0, y1, mat, sides=24, solid=True, cap=True, outward=True):
    for i in range(sides):
        a, b = math.tau*i/sides, math.tau*(i+1)/sides
        c = pol(1, (a+b)/2)
        t = c if outward else (-c[0], 0, -c[2])
        face(mesh, [pol(r, a, y0), pol(r, b, y0), pol(r, b, y1), pol(r, a, y1)], mat, t, solid)
        if cap: face(mesh, [(0, y1, 0), pol(r, a, y1), pol(r, b, y1)], mat, (0, 1, 0), False)


def post(mesh, x, z, r, y0, y1, mat, sides=8, solid=True):
    """An upright round pillar at (x, z)."""
    for i in range(sides):
        a, b = math.tau*i/sides, math.tau*(i+1)/sides
        p = lambda t, y: (x+r*math.sin(t), y, z+r*math.cos(t))
        c = (math.sin((a+b)/2), 0, math.cos((a+b)/2))
        face(mesh, [p(a, y0), p(b, y0), p(b, y1), p(a, y1)], mat, c, solid)
        face(mesh, [(x, y1, z), p(a, y1), p(b, y1)], mat, (0, 1, 0), solid)


def block(mesh, base, r, a, size, y0, mat, turn=0.0, solid=True):
    """A box of `size` (across, up, along the radius) at polar (r, a),
    standing on y0, its sides square to the radius (turned `turn` more)."""
    x, _, z = pol(r, a)
    with Placed(mesh, base, x, z, a+turn):
        mesh.box((0, y0+size[1]/2, 0), (size[0], size[1], size[2]), mat, solid)


def radial_wall(b, a, r0, r1, y0, y1, mat, thick=0.5, solid=True):
    p, q = pol(r0, a), pol(r1, a)
    b.beam((p[0], p[2]), (q[0], q[2]), thick, y0, y1, mat, solid)


def lamp_ring(b, y, radii=(9.0, 13.6, 17.0), every=30.0, power=1.1, phase=15.0):
    for r in radii:
        n = int(round(360/every))
        for i in range(n):
            a = deg(phase+i*every)
            p = pol(r, a, y)
            b.lamp(p, power)


def ceiling_strips(mesh, above, radii=(9.6, 15.8)):
    """Lit rings under the ceiling (render only), following the underside of
    the floor above: under its pits they drop with it."""
    for r in radii:
        j = int((r-RINGS[0])/1.6)
        for s in range(SECTORS):
            under = above.y+min(above.cells[j][s][0], 0.0)-SLAB
            ring_band(mesh, r-.25, r+.25, sector_angle(s), sector_angle(s+1), under-.08, under-.02, 'light',
                      solid=False, segs=1, ends=False, faces='b')


def ceiling_ring(mesh, p, r, half, depth, mat, faces='iob'):
    """A band round the axis hung under the ceiling, sector by sector at the
    ceiling's own height (lower under the floor above's pits)."""
    for s in range(SECTORS):
        a0, a1 = sector_angle(s), sector_angle(s+1)
        top = min(p.ceiling_at(r-half, (a0+a1)/2), p.ceiling_at(r+half, (a0+a1)/2))
        ring_band(mesh, r-half, r+half, a0, a1, top-depth, top-.005, mat, solid=False, segs=1, ends=False, faces=faces)


def decal(mesh, pts, mat, lift=.012):
    """A flat mark on a floor (drawn only): `pts` local, lifted a hair."""
    face(mesh, [(x, y+lift, z) for x, y, z in pts], mat, (0, 1, 0), solid=False)


def radial_line(mesh, a, r0, r1, y, width, mat):
    """A painted line along the radius at angle `a`."""
    t, h = (math.cos(a), 0, -math.sin(a)), width/2
    p, q = pol(r0, a, y), pol(r1, a, y)
    decal(mesh, [(p[0]-t[0]*h, y, p[2]-t[2]*h), (q[0]-t[0]*h, y, q[2]-t[2]*h),
                 (q[0]+t[0]*h, y, q[2]+t[2]*h), (p[0]+t[0]*h, y, p[2]+t[2]*h)], mat)


def arc_mark(mesh, r0, r1, a0, a1, y, mat):
    """A painted band round the axis."""
    ring_band(mesh, r0, r1, a0, a1, y+.008, y+.012, mat, solid=False, ends=False, faces='u')


def tube(mesh, p, q, rad, mat, sides=6, solid=False, caps=False):
    """A round bar from `p` to `q` (local points): pipes, cables, rungs."""
    d = (q[0]-p[0], q[1]-p[1], q[2]-p[2])
    n = math.sqrt(sum(c*c for c in d)) or 1.0
    d = (d[0]/n, d[1]/n, d[2]/n)
    ref = (0, 1, 0) if abs(d[1]) < .9 else (1, 0, 0)
    u = (d[1]*ref[2]-d[2]*ref[1], d[2]*ref[0]-d[0]*ref[2], d[0]*ref[1]-d[1]*ref[0])
    un = math.sqrt(sum(c*c for c in u)); u = (u[0]/un, u[1]/un, u[2]/un)
    v = (d[1]*u[2]-d[2]*u[1], d[2]*u[0]-d[0]*u[2], d[0]*u[1]-d[1]*u[0])
    ring = lambda c, i: tuple(c[k]+rad*(math.cos(math.tau*i/sides)*u[k]+math.sin(math.tau*i/sides)*v[k]) for k in range(3))
    for i in range(sides):
        out = tuple(math.cos(math.tau*(i+.5)/sides)*u[k]+math.sin(math.tau*(i+.5)/sides)*v[k] for k in range(3))
        face(mesh, [ring(p, i), ring(p, i+1), ring(q, i+1), ring(q, i)], mat, out, solid)
    if caps:
        for c, sgn in ((p, -1), (q, 1)):
            for i in range(sides):
                face(mesh, [c, ring(c, i), ring(c, i+1)], mat, (d[0]*sgn, d[1]*sgn, d[2]*sgn), solid)


def arc_pipe(mesh, r, a0, a1, y, rad, mat, sides=6, every=deg(7.5)):
    """A pipe running round the axis at radius `r`, height `y`."""
    segs = max(1, int(math.ceil((a1-a0)/every)))
    for i in range(segs):
        tube(mesh, pol(r, a0+(a1-a0)*i/segs, y), pol(r, a0+(a1-a0)*(i+1)/segs, y), rad, mat, sides)


def railing(mesh, r, a0, a1, y, h=RAIL_H, mat='trim', top='brass', every=deg(7.5), solid=True):
    """Posts, a mid rail and a top rail along an arc; solid, bodies stop at it."""
    segs = max(1, int(math.ceil((a1-a0)/every)))
    for i in range(segs+1):
        x, _, z = pol(r, a0+(a1-a0)*i/segs)
        post(mesh, x, z, .05, y, y+h, mat, sides=6, solid=False)
    ring_band(mesh, r-.05, r+.05, a0, a1, y+h-.08, y+h, top, solid=False, segs=segs, ends=False)
    ring_band(mesh, r-.03, r+.03, a0, a1, y+h*.5-.03, y+h*.5+.03, mat, solid=False, segs=segs, ends=False)
    if solid: collide_band(mesh, r-.05, r+.05, a0, a1, y, y+h, 'io', segs=segs)


def hanging_lamp(mesh, b, r, a, ceiling, drop, power, color, shade='trim', size=.7):
    """A shaded lamp hung on a cable `drop` under the ceiling."""
    x, _, z = pol(r, a)
    y = ceiling-drop
    tube(mesh, (x, y+.2, z), (x, ceiling, z), .025, 'trim', sides=4)
    mesh.box((x, y+.13, z), (size, .2, size), shade, False)
    h = size/2-.06
    face(mesh, [(x-h, y, z-h), (x+h, y, z-h), (x+h, y, z+h), (x-h, y, z+h)], 'light', (0, -1, 0), solid=False)
    b.lamp((x, y-.35, z), power, color)


def liner(mesh, k, mat, y0=SKIRT, y1=None, r=R_IN-.04, gaps=()):
    """A floor's own wall surface inside the round wall (drawn only), open at
    the slit windows (as `outer_wall` cuts them), the ground floor's doors
    and any `gaps` ((a0, a1) in radians); from over the skirting band up."""
    sides, y = 48, floor_y(k)
    y1 = STOREY-SLAB if y1 is None else y1
    da, slit = math.tau/sides, .35/R_OUT
    lo, hi = 1.3, 5.4
    for i in range(sides):
        a0, a1 = i*da, (i+1)*da
        mid = (a0+a1)/2
        if k == 1 and any(abs(_wrap(mid-d[0])) < d[1] for d in DOORS): continue
        if any(g0 <= mid <= g1 for g0, g1 in gaps): continue
        pieces = [(a0, a1, y0, y1)]
        if i % (sides//12) == 2:
            s0, s1 = mid-slit/2-.004, mid+slit/2+.004
            pieces = [(a0, s0, y0, y1), (s1, a1, y0, y1)]
            pieces += [(s0, s1, y0, min(lo, y1)), (s0, s1, max(hi, y0), y1)]
        for p0, p1, q0, q1 in pieces:
            if q1-q0 > .01 and p1-p0 > 1e-4:
                ring_band(mesh, r, r+.02, p0, p1, y+q0, y+q1, mat, solid=False, segs=1, ends=False, faces='i')


def gear(mesh, base, r, a, y, radius, teeth, thick, mat, spin=0.0, hub='trim'):
    """A toothed wheel on the wall at polar (r, a), facing the axis (drawn only)."""
    x, _, z = pol(r, a)
    root, tip, w = radius*.84, radius, math.tau/teeth
    pts = []
    for i in range(teeth):
        t = spin+i*w
        for f, rr in ((0, root), (.2, tip), (.5, tip), (.7, root)):
            pts.append((rr*math.cos(t+f*w), rr*math.sin(t+f*w)))
    with Placed(mesh, base, x, z, a):
        front = -thick
        for i in range(len(pts)):
            (px, py), (qx, qy) = pts[i], pts[(i+1) % len(pts)]
            face(mesh, [(0, y, front), (px, y+py, front), (qx, y+qy, front)], mat, (0, 0, -1), solid=False)
            mx, my = (px+qx)/2, (py+qy)/2
            face(mesh, [(px, y+py, front), (qx, y+qy, front), (qx, y+qy, 0), (px, y+py, 0)], mat, (mx, my, 0), solid=False)
        mesh.box((0, y, front-.08), (radius*.36, radius*.36, .16), hub, False)


# ---- The tower ---------------------------------------------------------------

def stair(mesh, b, k):
    """Floor k's flight: from its landing (a = 22.5 deg) once round the core
    to the next landing, rising a storey. A ramp underfoot (collision) with
    treads drawn on it, a closed underside, and a rail on its outer edge."""
    y0 = floor_y(k)
    a0, a1 = sector_angle(LANDING), sector_angle(SECTORS-LANDING)
    n = STAIR_SEGMENTS
    at = lambda i: (a0+(a1-a0)*i/n, y0+STOREY*i/n)
    under = 0.45
    for i in range(n):
        (pa, py), (qa, qy) = at(i), at(i+1)
        # The ramp (solid) and its underside.
        face(mesh, [pol(R_CORE, pa, py), pol(R_STAIR, pa, py), pol(R_STAIR, qa, qy), pol(R_CORE, qa, qy)], 'grate', (0, 1, 0), solid=True)
        face(mesh, [pol(R_CORE, pa, py-under), pol(R_STAIR, pa, py-under), pol(R_STAIR, qa, qy-under), pol(R_CORE, qa, qy-under)],
             'trim', (0, -1, 0), solid=False)
        c = pol(1, (pa+qa)/2)
        if k == 1:
            # The ground floor's flight is closed underneath down to the floor
            # (its first piece a triangle: a quad there would have no height
            # at its foot, and half of it no area).
            skirt = [pol(R_STAIR, pa, y0), pol(R_STAIR, qa, y0), pol(R_STAIR, qa, qy), pol(R_STAIR, pa, py)]
            face(mesh, skirt[:3] if py <= y0 else skirt, 'panel', c)
        else:
            face(mesh, [pol(R_STAIR, pa, py-under), pol(R_STAIR, qa, qy-under), pol(R_STAIR, qa, qy), pol(R_STAIR, pa, py)], 'trim', c, solid=False)
        # Treads, drawn only: a flat step at the segment's mean height and
        # its nosing, so the ramp reads as stairs.
        m = (py+qy)/2+.01
        face(mesh, [pol(R_CORE+.05, pa, m), pol(R_STAIR-.2, pa, m), pol(R_STAIR-.2, qa, m), pol(R_CORE+.05, qa, m)], 'panel', (0, 1, 0), solid=False)
        if i < n-1:
            # (The last step's nosing would lie in the landing's edge.)
            face(mesh, [pol(R_CORE+.05, qa, m-.03), pol(R_STAIR-.2, qa, m-.03), pol(R_STAIR-.2, qa, m+.03), pol(R_CORE+.05, qa, m+.03)],
                 'light' if i % 6 == 0 else 'trim', (-math.cos(qa), 0, math.sin(qa)), solid=False)
        # The rail, a piece every two segments.
        if i % 2 == 0:
            (pa, py), (qa, qy) = at(i), at(min(n, i+2))
            c = pol(1, (pa+qa)/2)
            ri, ro = R_STAIR-.15, R_STAIR
            face(mesh, [pol(ri, pa, py), pol(ri, qa, qy), pol(ri, qa, qy+RAIL_H), pol(ri, pa, py+RAIL_H)], 'trim', (-c[0], 0, -c[2]))
            face(mesh, [pol(ro, pa, py), pol(ro, qa, qy), pol(ro, qa, qy+RAIL_H), pol(ro, pa, py+RAIL_H)], 'trim', c)
            face(mesh, [pol(ri, pa, py+RAIL_H), pol(ro, pa, py+RAIL_H), pol(ro, qa, qy+RAIL_H), pol(ri, qa, qy+RAIL_H)], 'brass', (0, 1, 0), solid=False)
    if k == 1:
        # The ground floor's space under the flight is shut at its high
        # end too, so nobody walks into the wedge under it.
        face(mesh, [pol(R_CORE, a1, y0), pol(R_STAIR, a1, y0), pol(R_STAIR, a1, y0+STOREY-under), pol(R_CORE, a1, y0+STOREY-under)],
             'panel', (math.cos(a1), 0, -math.sin(a1)))
    for end, a, y in ((-1, a0, y0), (1, a1, y0+STOREY)):
        t = (math.cos(a)*end, 0, -math.sin(a)*end)
        face(mesh, [pol(R_STAIR-.15, a, y), pol(R_STAIR, a, y), pol(R_STAIR, a, y+RAIL_H), pol(R_STAIR-.15, a, y+RAIL_H)], 'trim', t)
    # A lamp every quarter turn over the flight.
    for i in range(0, n, n//4):
        a, y = at(i)
        b.lamp(pol((R_CORE+R_STAIR)/2, a, y+3.0), .8)


def landing(mesh, k, under=True):
    """Floor k's landing: the stair's flat at a = 0, flush with the floor."""
    y = floor_y(k)
    a0, a1 = -LANDING*SECTOR, LANDING*SECTOR
    ring_band(mesh, R_CORE, R_STAIR, a0, a1, y-SLAB, y, 'grate', segs=6)
    # A lit edge where the landing meets the floor.
    ring_band(mesh, R_STAIR-.06, R_STAIR, a0, a1, y, y+.02, 'light', solid=False, segs=6)


def hole_rail(mesh, k):
    """The rail round each floor's hole, open at the landing."""
    y = floor_y(k)
    ring_band(mesh, R_STAIR, R_STAIR+.15, sector_angle(LANDING), sector_angle(SECTORS-LANDING), y, y+RAIL_H, 'trim',
              segs=STAIR_SEGMENTS//2, faces='iou', collide='ioe')
    ring_band(mesh, R_STAIR-.01, R_STAIR+.16, sector_angle(LANDING), sector_angle(SECTORS-LANDING), y+RAIL_H, y+RAIL_H+.06,
              'brass', solid=False, segs=STAIR_SEGMENTS//2, ends=False)


def outer_wall(mesh, b, doors, slits=12):
    """The round wall, floor by floor: slit windows on every floor (drawn:
    too narrow to pass, so solid to bodies and shots), the ground floor's
    doors, and the crown's parapet. Collision runs up each piece of the wall
    in one."""
    sides = 48
    da = math.tau/sides
    slit = 0.35/R_OUT                     # angle of a slit's opening
    top_all = floor_y(FLOORS)+CROWN_PARAPET
    for i in range(sides):
        a0, a1 = i*da, (i+1)*da
        door = next((d for d in doors if abs(_wrap((a0+a1)/2-d[0])) < d[1]), None)
        collide_band(mesh, R_IN, R_OUT, a0, a1, door[2] if door else -SLAB-1.0, top_all, 'io' if not door else 'iob')
        collide_band(mesh, R_IN, R_OUT, a0, a1, top_all, top_all, 'u')
    for k in range(1, FLOORS+1):
        y0 = floor_y(k)-SLAB if k > 1 else -SLAB-1.0
        top = floor_y(k)+(STOREY-SLAB if k < FLOORS else CROWN_PARAPET)
        if k == FLOORS:
            ring_band(mesh, R_IN, R_OUT, 0, math.tau, y0, top, 'panel', solid=False, segs=sides, ends=False, faces='iou')
            ring_band(mesh, R_IN-.05, R_OUT+.05, 0, math.tau, top, top+.12, 'brass', solid=False, segs=sides, ends=False)
            continue
        for i in range(sides):
            a0, a1 = i*da, (i+1)*da
            mid = (a0+a1)/2
            door = next((d for d in doors if k == 1 and abs(_wrap(mid-d[0])) < d[1]), None)
            if door is not None:
                # Over a door: the lintel.
                ring_band(mesh, R_IN, R_OUT, a0, a1, floor_y(k)+door[2], top, 'panel', solid=False, segs=1, ends=False, faces='iob')
                continue
            if i % (sides//slits) == 2:
                # A slit window in the middle of this piece.
                s0, s1 = mid-slit/2, mid+slit/2
                lo, hi = floor_y(k)+1.3, floor_y(k)+5.4
                for (p0, p1, q0, q1) in ((a0, s0, y0, top), (s1, a1, y0, top), (s0, s1, y0, lo), (s0, s1, hi, top)):
                    ring_band(mesh, R_IN, R_OUT, p0, p1, q0, q1, 'panel', solid=False, segs=1, ends=False, faces='io')
                for a, sgn in ((s0, 1), (s1, -1)):
                    face(mesh, [pol(R_IN, a, lo), pol(R_OUT, a, lo), pol(R_OUT, a, hi), pol(R_IN, a, hi)], 'trim',
                         (math.cos(a)*sgn, 0, -math.sin(a)*sgn), solid=False)
                face(mesh, [pol(R_IN, s0, lo), pol(R_OUT, s0, lo), pol(R_OUT, s1, lo), pol(R_IN, s1, lo)], 'trim', (0, 1, 0), solid=False)
                face(mesh, [pol(R_IN, s0, hi), pol(R_OUT, s0, hi), pol(R_OUT, s1, hi), pol(R_IN, s1, hi)], 'trim', (0, -1, 0), solid=False)
                continue
            ring_band(mesh, R_IN, R_OUT, a0, a1, y0, top, 'panel', solid=False, segs=1, ends=False, faces='io')
        # Door jambs, sills and the lit lintels.
        if k == 1:
            for a, half, h in doors:
                lo, hi = _snap(a-half, da), _snap(a+half, da)
                for e, sgn in ((lo, 1), (hi, -1)):
                    face(mesh, [pol(R_IN, e, 0), pol(R_OUT, e, 0), pol(R_OUT, e, h), pol(R_IN, e, h)], 'trim',
                         (math.cos(e)*sgn, 0, -math.sin(e)*sgn))
                ring_band(mesh, R_IN, R_OUT, lo, hi, -SLAB-1.0, 0.0, 'concrete', segs=2, ends=False, faces='ub', collide='u')
                ring_band(mesh, R_OUT, R_OUT+.08, lo, hi, h, h+.25, 'light', solid=False, segs=4, ends=False)
        # Bands drawn round the outside at each floor line.
        ring_band(mesh, R_OUT, R_OUT+.12, 0, math.tau, floor_y(k)-.3, floor_y(k)+.1, 'trim', solid=False, segs=sides, ends=False, faces='ou')
        ring_band(mesh, R_IN-.04, R_IN, 0, math.tau, floor_y(k)+.05, floor_y(k)+SKIRT, 'trim', solid=False, segs=sides, ends=False, faces='iu')


def _wrap(a):
    return (a+math.pi) % math.tau-math.pi


def _snap(a, da):
    return round(a/da)*da


def core(mesh, b):
    """The core the stair winds round, glowing through every floor."""
    top = floor_y(FLOORS)+CANOPY_Y
    cylinder(mesh, R_CORE, -SLAB-1.0, top, 'panel', sides=24, cap=True)
    for k in range(1, FLOORS+1):
        y = floor_y(k)
        for h in (1.8, 4.2, 6.6):
            if k == FLOORS and h > 4: continue
            ring_band(mesh, R_CORE, R_CORE+.06, 0, math.tau, y+h, y+h+.18, 'light', solid=False, segs=24, ends=False)
        b.lamp((0, y+5.5, 0), .5)


# ---- The floors ----------------------------------------------------------------

# The ground floor's doors: (angle, half width, height).
DOORS = [(deg(90), deg(9), 4.6), (deg(180), deg(9), 4.6), (deg(270), deg(9), 4.6)]
AMBER = (1.0, .62, .32)
CATWALK = 2.1
DOOR_LANES = [math.degrees(d[0]) for d in DOORS]


def in_door_lane(r, a_deg, half=13.0):
    """Whether (r, a) stands in the way in from a ground-floor door."""
    return r > 11.0 and any(abs((a_deg-d+180) % 360-180) < half for d in DOOR_LANES)


def cargo_bay(p):
    """1: an industrial cargo bay in low amber light: shipping containers and
    crates to jump over or hide behind, a catwalk deck along the back wall
    behind the stair, gantry cranes overhead. The three doors open onto the
    floor, their lanes painted and kept clear."""
    # The catwalk: 330 to 37.5 degrees, 2.1 m up, steps down to 285 and
    # to 82.5, short of the doors.
    p.platform(6, 8, -4, 5, CATWALK, 'grate', up='both')
    p.strips = ()
    p.ceiling = 'trim'
    p.spawns = [(12.0, deg(70)), (11.0, deg(150)), (11.0, deg(210)), (12.0, deg(290))]
    rng = random.Random(1)
    crates = [(9.4, 50), (10.8, 62), (9.6, 120), (13.4, 158), (10.2, 185), (13.4, 202), (9.4, 240), (13.0, 252), (10.4, 300),
              (8.6, 330), (8.8, 30)]
    def clear(r, a):
        x, _, z = pol(r, deg(a))
        return all(math.hypot(x-pol(sr, sa)[0], z-pol(sr, sa)[2]) > 3.4 for sr, sa in p.spawns) and not in_door_lane(r, a)
    crates = [c for c in crates if clear(*c) and clear(c[0]+1.9, c[1]+3)]
    containers = [(16.5, 135, 'rust', 0.0), (16.5, 225, 'steelblue', 0.0), (16.4, 228, 'rust', 2.6)]
    # Barrels stand against the wall: a gap behind one narrower than a body
    # is a slot to fall into.
    barrels = [(17.56, a) for a in (106, 109, 112, 252, 256, 157, 203)]
    p.ambient = (.66, .56, .46)
    p.gain = 1.5
    def dress(mesh, b, base, y):
        top = CATWALK
        for r, a in crates:
            tall = rng.random() < .4
            s = 2.2 if tall else 1.6
            turn = deg(rng.uniform(-12, 12))
            block(mesh, base, r, deg(a), (s+.3, .14, s+.3), y, 'bark', turn=turn, solid=False)        # the pallet
            block(mesh, base, r, deg(a), (s, 2.4 if tall else 1.2, s), y+.14, 'crate', turn=turn)
            if not tall and rng.random() < .5:
                block(mesh, base, r+1.9, deg(a+3), (1.4, 1.2, 1.4), y, 'crate', turn=deg(rng.uniform(-20, 20)))
        # Shipping containers along the wall, one stacked; doors at their ends.
        for r, a, mat, lift in containers:
            block(mesh, base, r, deg(a), (6.0, 2.6, 2.4), y+lift, mat)
            for end in (-1, 1):
                for k in (-.6, .6):
                    x, _, z = pol(r+k, deg(a)+end*3.02/r)
                    post(mesh, x, z, .04, y+lift+.2, y+lift+2.4, 'trim', sides=4, solid=False)
            block(mesh, base, r, deg(a), (6.04, .12, 2.44), y+lift+2.5, 'trim', solid=False)
        for r, a in barrels:
            x, _, z = pol(r, deg(a))
            mat = ('rust', 'steelblue', 'hazard')[int(a) % 3]
            b.prism(x, z, .38, .38, y, y+1.05, 8, mat)
            b.prism(x, z, .4, .4, y+.62, y+.7, 8, 'trim', solid=False, cap=False)
        # The catwalk: its face held up on posts, a hazard edge, a rail.
        a0, a1 = sector_angle(-4), sector_angle(5)
        for i in range(10):
            x, _, z = pol(14.66, a0+(a1-a0)*i/9)
            post(mesh, x, z, .1, y, y+top, 'trim', sides=6, solid=False)
        arc_mark(mesh, 14.8, 15.15, a0, a1, y+top, 'hazard')
        railing(mesh, 14.95, a0+deg(1), a1-deg(1), y+top)
        # Its steps' edges.
        for sgn, start in ((-1, a0), (1, a1)):
            for i in range(int(round(top/STEP_UP))):
                a = start+sgn*i*SECTOR
                radial_line(mesh, a, 14.85, 17.9, y+top-STEP_UP*i, .14, 'hazard')
        # Door lanes painted to the stair.
        for d in DOOR_LANES:
            for side in (-1, 1):
                radial_line(mesh, deg(d)+side*2.2/14, 9.5, 17.95, y, .12, 'hazard')
        arc_mark(mesh, 9.0, 9.25, 0, math.tau, y, 'hazard')
        # Gantry beams overhead, each with a hoist and its hook.
        for a in (45, 135, 225, 315):
            radial_wall(b, deg(a), 6.0, 17.8, y+6.6, y+7.1, 'trim', thick=.4, solid=False)
            r = 9.0+(a % 90)/45*2.0+rng.uniform(0, 3)
            block(mesh, base, r, deg(a), (.9, .5, 1.2), y+6.1, 'hazard', solid=False)
            x, _, z = pol(r, deg(a))
            drop = rng.uniform(.8, 1.5)
            tube(mesh, (x, y+6.1-drop, z), (x, y+6.1, z), .03, 'trim', sides=4)
            mesh.box((x, y+5.95-drop, z), (.3, .3, .3), 'trim', False)
        # Dark corrugated walls, ribbed; girders where other floors have
        # lit rings.
        liner(mesh, 1, 'steelblue')
        for r in (9.6, 15.8):
            ceiling_ring(mesh, p, r, .2, .45, 'trim')
        # Wall ribs, clear of the doors and slits.
        for i in range(48):
            a = (i+1)*360/48
            if any(abs((a-d+180) % 360-180) < 11 for d in DOOR_LANES): continue
            block(mesh, base, 17.88, deg(a), (.3, STOREY-SLAB-.4, .24), y, 'trim', solid=False)
        # Low amber light from hanging work lamps.
        for a in range(0, 360, 45):
            for r in (9.6, 14.2):
                at = deg(a+22.5 if r > 10 else a)
                hanging_lamp(mesh, b, r, at, p.ceiling_at(r, at), 1.4, 1.25, AMBER)
    p.dress.append(dress)


MAGENTA = (1.0, .3, .78)
CYAN = (.3, .82, 1.0)


def wall_face(mesh, a, r, y, side, w, h, mat, off=.03, depth=0.0, solid=False):
    """Something flat on one face of a radial wall at angle `a` (side -1 or
    +1): centered `r` from the axis, `y` up, `w` along the wall, `h` tall,
    standing `off` proud (a box `depth` deep when given)."""
    t = (math.cos(a), 0, -math.sin(a))
    c = pol(r, a)
    n = .25+off
    if depth:
        x, z = c[0]+t[0]*side*(n+depth/2), c[2]+t[2]*side*(n+depth/2)
        with Placed(mesh, (mesh.origin[0], mesh.origin[1], mesh.origin[2]), x, z, a):
            mesh.box((0, y+h/2, 0), (depth, h, w), mat, solid)
        return
    x, z = c[0]+t[0]*side*n, c[2]+t[2]*side*n
    u = (math.sin(a)*w/2, 0, math.cos(a)*w/2)
    face(mesh, [(x-u[0], y, z-u[2]), (x+u[0], y, z+u[2]), (x+u[0], y+h, z+u[2]), (x-u[0], y+h, z-u[2])], mat,
         (t[0]*side, 0, t[2]*side), solid=False)


def neon_alley(p):
    """2: a neon alley at night: dark wet ground with puddles, alley walls
    lit by windows and signs, holographic signs over the alleys, balconies
    up two flights of steps, cables overhead, magenta and cyan light."""
    p.platform(6, 8, 15, 22, 2.4, 'grate', up='before')
    p.platform(6, 8, 26, 33, 2.4, 'grate', up='after')
    # Puddles in the alleys between the walls (none stands in one).
    for s0 in (20, 30):
        p.pit(2, 4, s0, s0+3, 0.4, 'trim', 'water', color=[0.3, 0.1, 0.5])
    p.spawns = [(11.0, deg(55)), (12.5, deg(175)), (12.5, deg(232)), (11.0, deg(305))]
    p.strips = ()
    p.ceiling = 'trim'
    walls = (40, 75, 110, 145, 215, 250, 285, 320)
    rng = random.Random(2)
    p.ambient = (.5, .42, .64)
    p.gain = 1.7
    def dress(mesh, b, base, y):
        liner(mesh, 2, 'trim')
        for i, a in enumerate(walls):
            ad = deg(a)
            radial_wall(b, ad, 8.6, 13.4, y, y+4.2, 'concrete')
            block(mesh, base, 11.0, ad, (.6, .14, 4.9), y+4.2, 'trim', solid=False)              # the coping
            glow = 'neon' if i % 2 else 'glacier'
            for side in (-1, 1):
                block(mesh, base, 11.0, ad+side*.035, (.08, .25, 4.2), y+3.3, glow, solid=False)
                # A door with a lit frame, windows above, an AC box, a pipe.
                r_door = 9.6 if side < 0 else 12.4
                wall_face(mesh, ad, r_door, y, side, 1.1, 2.1, 'trim', off=.03)
                wall_face(mesh, ad, r_door, y+2.1, side, 1.3, .08, glow, off=.04)
                for k, r_win in enumerate((10.6, 11.8) if side < 0 else (9.8, 11.0)):
                    glass = rng.choice(('ember', 'ember', 'light', 'screen', 'screen'))
                    wall_face(mesh, ad, r_win, y+2.6, side, .8, .7, glass, off=.03)
                wall_face(mesh, ad, 12.5 if side < 0 else 9.5, y+2.4, side, .6, .5, 'panel', depth=.4)
                wall_face(mesh, ad, 13.1 if side < 0 else 8.9, y, side, .12, 4.2, 'copper', depth=.12)
            # A holographic sign standing out from the wall's end over the alley.
            x, _, z = pol(13.6, ad)
            mesh.box((x, y+3.4, z), (.12, 1.6, .12), 'trim', False)
            sign = ('neon', 'glacier', 'screen')[i % 3]
            block(mesh, base, 13.6, ad, (.1, 1.1, 1.0), y+3.6, sign, turn=deg(90), solid=False)
            b.lamp(pol(13.9, ad, y+3.9), .55, MAGENTA if i % 2 else CYAN)
        for a in (92, 268):
            block(mesh, base, 11.0, deg(a), (3.0, 1.0, .5), y, 'trim')       # low barriers
            block(mesh, base, 11.0, deg(a), (3.0, .08, .55), y+1.0, 'neon', solid=False)
        # Signs along the outer wall.
        for a in range(10, 360, 40):
            block(mesh, base, 17.85, deg(a), (2.4, 1.0, .1), y+4.0, 'neon' if a % 80 == 10 else 'glacier', solid=False)
        # The balconies: rails, awnings, lanterns.
        for s0, s1 in ((15, 22), (26, 33)):
            a0, a1 = sector_angle(s0), sector_angle(s1)
            railing(mesh, 14.95, a0+deg(1), a1-deg(1), y+2.4, top='neon')
            ring_band(mesh, 15.6, 18.0, a0, a1, y+5.6, y+5.7, 'rust', solid=False, ends=False, faces='ub')
            for a in (a0+deg(8), a1-deg(8)):
                hanging_lamp(mesh, b, 15.9, a, y+5.6, .5, .5, MAGENTA, shade='rust', size=.35)
        # Cables strung over the alleys, sagging, and floating signs.
        for i, a in enumerate(walls):
            nxt = walls[(i+1) % len(walls)]
            if (nxt-a) % 360 > 60: continue
            for r in (9.5, 12.5):
                p0, p1 = pol(r, deg(a), y+4.3), pol(r, deg(nxt), y+4.3)
                mid = pol(r, deg(a+((nxt-a) % 360)/2), y+3.7)
                tube(mesh, p0, mid, .02, 'trim', sides=4); tube(mesh, mid, p1, .02, 'trim', sides=4)
        for a in (20, 130, 200, 340):
            block(mesh, base, 12.0, deg(a), (2.2, .9, .06), y+5.4, ('glacier', 'neon', 'screen', 'neon')[a % 4], solid=False)
        # Neon rings under the ceiling and the alleys' light.
        for r in (9.6, 15.8):
            ceiling_ring(mesh, p, r, .12, .1, 'neon' if r < 10 else 'glacier', faces='b')
        for i, a in enumerate(range(0, 360, 30)):
            b.lamp(pol(12, deg(a+15), y+6.0), .75, MAGENTA if i % 2 else CYAN)
    p.dress.append(dress)


SUNLIT = (1.0, .86, .62)


def vine(mesh, x, z, top, length, rng, mat='leaf'):
    """A vine hanging from (x, top, z): a crooked strand with leaves."""
    pts, y = [(x, top, z)], top
    for i in range(3):
        y -= length/3
        pts.append((x+rng.uniform(-.15, .15), y, z+rng.uniform(-.15, .15)))
    for p0, p1 in zip(pts, pts[1:]):
        tube(mesh, p0, p1, .035, 'bark', sides=4)
    for i in range(int(length*5)):
        t = rng.random()
        k = min(2, int(t*3)); f = t*3-k
        c = tuple(pts[k][j]+(pts[k+1][j]-pts[k][j])*f for j in range(3))
        w = rng.uniform(.22, .4)
        mesh.box((c[0]+rng.uniform(-.18, .18), min(c[1], top-w*.45), c[2]+rng.uniform(-.18, .18)), (w, w*.8, w), mat, False)


def patch(mesh, r, a, y, size, mat, rng, sides=9):
    """An irregular flat patch on a floor (moss, a stain) at polar (r, a)."""
    cx, _, cz = pol(r, a)
    pts = []
    for i in range(sides):
        t = math.tau*i/sides
        k = size*rng.uniform(.55, 1.0)
        pts.append((cx+k*math.cos(t), y, cz+k*math.sin(t)))
    for i in range(sides):
        decal(mesh, [(cx, y, cz), pts[i], pts[(i+1) % sides]], mat)


def torch(mesh, b, a, y, color, r=R_IN-.05, power=.9):
    """A torch on a bracket on the wall: its flame lights the floor round it."""
    x, _, z = pol(r-.35, a)
    wx, _, wz = pol(r, a)
    tube(mesh, (wx, y-.25, wz), (x, y, z), .04, 'brass', sides=4)
    b.prism(x, z, .1, .16, y-.3, y, 6, 'brass', solid=False)
    b.prism(x, z, .1, .02, y, y+.32, 5, 'light', solid=False, cap=False)
    b.lamp((x, y+.4, z), power, color)


def ruins(p):
    """3: ancient stone ruins in warm light: broken pillars, round arches,
    fallen drums to vault, broken wall stubs, rubble steps and dry pits,
    vines and moss."""
    p.pit(3, 5, 9, 14, 1.2, 'stone')
    p.pit(4, 6, 34, 39, 1.2, 'stone')
    p.platform(5, 7, 22, 26, 1.5, 'stone', up='both')
    p.spawns = [(13.0, deg(60)), (10.5, deg(150)), (14.5, deg(255)), (12.0, deg(300))]
    p.ceiling = 'stone'
    p.strips = ()
    rng = random.Random(3)
    p.ambient = (0.8, 0.7, 0.55)
    def dress(mesh, b, base, y):
        liner(mesh, 3, 'stone')
        ceiling = y+STOREY-SLAB
        tall = []
        for a in range(20, 350, 28):
            r = 9.0 if (a//28) % 2 else 15.6
            j, s = int((r-RINGS[0])/1.6), int(a/7.5)
            if landing_clear(j, s) or p.cells[j][s][0] != 0: continue
            h = rng.choice((1.1, 2.6, 4.8, 6.0))
            x, _, z = pol(r, deg(a))
            post(mesh, x, z, .6, y, y+h, 'stone', sides=10)
            b.prism(x, z, .78, .7, y, y+.3, 10, 'stone', phase=.31, solid=False)               # its base
            if h > 4:
                b.prism(x, z, .66, .82, y+h-.35, y+h, 10, 'stone', phase=.31, solid=False)      # a capital
                tall.append((x, z, y+h))
            else:
                # Broken: its top block knocked askew, a drum fallen beside it.
                b.prism(x, z, .58, .5, y+h, y+h+.25, 7, 'stone', phase=rng.uniform(0, 1), solid=False)
        # Round arches: two pillars and a ring of stones over them.
        for a in (120, 240):
            ends = []
            for side in (-1, 1):
                x, _, z = pol(12.0, deg(a)+side*.2)
                post(mesh, x, z, .55, y, y+4.2, 'stone', sides=10)
                b.prism(x, z, .72, .66, y+3.9, y+4.2, 10, 'stone', solid=False)
                ends.append((x, z))
            (x0, z0), (x1, z1) = ends
            cx, cz, half = (x0+x1)/2, (z0+z1)/2, math.hypot(x1-x0, z1-z0)/2
            ux, uz = (x1-x0)/(2*half), (z1-z0)/(2*half)
            n = 9
            for i in range(n):
                t0, t1 = math.pi*i/n, math.pi*(i+1)/n
                p0 = (cx-ux*half*math.cos(t0), y+4.2+half*.9*math.sin(t0), cz-uz*half*math.cos(t0))
                p1 = (cx-ux*half*math.cos(t1), y+4.2+half*.9*math.sin(t1), cz-uz*half*math.cos(t1))
                tube(mesh, p0, p1, .42, 'stone', sides=4, caps=True)
            for k in range(5):
                t = rng.uniform(.15, .85)
                vine(mesh, cx-ux*half*math.cos(math.pi*t), cz-uz*half*math.cos(math.pi*t), y+4.2+half*.9*math.sin(math.pi*t)-.5,
                     rng.uniform(1.0, 2.6), rng)
        # Fallen drums to vault, lying across the floor.
        for r, a, turn in ((12.4, 25, 75), (10.8, 205, 20), (15.0, 335, 100)):
            x, _, z = pol(r, deg(a))
            d = (math.sin(deg(a+turn))*1.6, math.cos(deg(a+turn))*1.6)
            tube(mesh, (x-d[0], y+.5, z-d[1]), (x+d[0], y+.5, z+d[1]), .5, 'stone', sides=10, solid=True, caps=True)
        # Broken wall stubs against the wall, their tops stepped (clear of
        # the raised platform's ring, where they'd leave a slot too narrow
        # for a body).
        for a0 in (40, 100, 215, 315):
            for i in range(4):
                h = (1.0, 2.4, 1.7, .8)[(i+a0) % 4]
                ring_band(mesh, 17.0, 17.96, deg(a0+i*4), deg(a0+(i+1)*4), y, y+h, 'stone', segs=1)
        for a in (75, 200, 285):
            block(mesh, base, 11.0, deg(a), (2.0, .6, 1.4), y, 'stone', turn=deg(rng.uniform(-30, 30)))
            block(mesh, base, 11.6, deg(a+5), (1.2, 1.2, 1.2), y, 'stone', turn=deg(rng.uniform(-30, 30)))
        # Rubble in the pits.
        for j0, s0, s1 in ((3, 9, 14), (4, 34, 39)):
            for _ in range(7):
                r = rng.uniform(RINGS[j0]+.4, RINGS[j0+2]-.4)
                a = rng.uniform(sector_angle(s0)+.05, sector_angle(s1)-.05)
                size = rng.uniform(.3, .7)
                block(mesh, base, r, a, (size, size*.6, size*1.2), y-1.2, 'stone', turn=rng.uniform(0, 3), solid=False)
        # Vines down the walls and from the tall pillars, moss underfoot.
        for i in range(26):
            a = deg(i*360/26+rng.uniform(-4, 4))
            x, _, z = pol(17.7, a)
            vine(mesh, x, z, ceiling-.05, rng.uniform(1.5, 4.5), rng)
        for x, z, top in tall:
            for _ in range(2):
                # Off the capital's rim, clear of the shaft.
                t = rng.uniform(0, math.tau)
                vine(mesh, x+.95*math.sin(t), z+.95*math.cos(t), top-.1, rng.uniform(1.0, 2.5), rng)
        for _ in range(22):
            r, a = rng.uniform(7, 17), rng.uniform(0, math.tau)
            j, sct = int((r-RINGS[0])/1.6), int(a/SECTOR) % SECTORS
            if p.cells[min(j, 7)][sct][0] != 0: continue
            patch(mesh, r, a, y, rng.uniform(.4, 1.1), 'moss', rng)
        for i in range(12):
            torch(mesh, b, deg(15+30*i), y+3.2, SUNLIT)
        for a in range(0, 360, 30): b.lamp(pol(11.0, deg(a), y+6.0), .55, SUNLIT)
    p.dress.append(dress)


COOL = (.82, .94, 1.0)


def laboratory(p):
    """4: a high-tech laboratory in cool white light: glass chambers on
    plinths, the energy core in its containment rings, consoles to vault,
    server racks along the wall, cable trays, holo displays overhead."""
    p.spawns = [(12.0, deg(75)), (11.0, deg(165)), (11.0, deg(195)), (12.0, deg(285))]
    p.raise_(3, 6, 9, 12, .3, 'tile'); p.raise_(3, 6, 36, 39, .3, 'tile')
    p.ceiling = 'tile'
    p.strips = (9.6, 13.0, 15.8)
    rng = random.Random(4)
    p.ambient = (0.95, 1.0, 1.08)
    def dress(mesh, b, base, y):
        liner(mesh, 4, 'tile')
        for a in (45, 105, 225, 315):
            ad = deg(a)
            block(mesh, base, 14.6, ad, (3.6, .3, 3.6), y, 'trim', solid=False)                  # the plinth
            block(mesh, base, 14.6, ad, (3.2, 4.2, 3.2), y, 'glacier')
            block(mesh, base, 14.6, ad, (3.4, .25, 3.4), y+4.2, 'trim')
            block(mesh, base, 14.6, ad, (2.6, .04, 2.6), y+4.45, 'light', solid=False)
            for cx in (-1.62, 1.62):
                for cz in (-1.62, 1.62):
                    with Placed(mesh, base, *[pol(14.6, ad)[i] for i in (0, 2)], ad):
                        mesh.box((cx, y+2.1, cz), (.14, 4.2, .14), 'trim', False)
            x, _, z = pol(14.6, ad)
            with Placed(mesh, base, x, z, ad):
                for (u0, u1, v0, v1) in ((-2.1, 2.1, -2.1, -1.92), (-2.1, 2.1, 1.92, 2.1), (-2.1, -1.92, -1.92, 1.92), (1.92, 2.1, -1.92, 1.92)):
                    decal(mesh, [(u0, y, v0), (u1, y, v0), (u1, y, v1), (u0, y, v1)], 'hazard')
        # Consoles: a desk, a sloped screen, a keyboard.
        for a in (135, 180, 255):
            ad = deg(a)
            block(mesh, base, 9.8, ad, (2.6, 1.05, .9), y, 'tile')
            block(mesh, base, 9.8, ad, (2.4, .05, .7), y+1.05, 'trim', solid=False)
            x, _, z = pol(9.95, ad)
            with Placed(mesh, base, x, z, ad):
                face(mesh, [(-1.0, y+1.1, .25), (1.0, y+1.1, .25), (1.0, y+1.75, .05), (-1.0, y+1.75, .05)], 'screen', (0, .5, -1), solid=False)
                face(mesh, [(-1.0, y+1.1, .27), (1.0, y+1.1, .27), (1.0, y+1.75, .07), (-1.0, y+1.75, .07)], 'trim', (0, .3, 1), solid=False)
                mesh.box((0, y+1.09, -.2), (1.2, .04, .3), 'trim', False)
        for a in (95, 265):
            block(mesh, base, 12.2, deg(a), (.8, .45, 5.0), y, 'brass')
            block(mesh, base, 12.2, deg(a), (.3, .02, 4.8), y+.45, 'glacier', solid=False)
        # Server racks along the wall, their fronts lit.
        for a0 in (18, 128, 236, 330):
            for i in range(3):
                ad = deg(a0+i*4.6)
                block(mesh, base, 17.45, ad, (1.3, 2.3, .9), y, 'trim')
                x, _, z = pol(16.99, ad)
                with Placed(mesh, base, x, z, ad):
                    face(mesh, [(-.5, y+.2, 0), (.5, y+.2, 0), (.5, y+2.1, 0), (-.5, y+2.1, 0)], 'screen', (0, 0, -1), solid=False)
                    for row in range(6):
                        if rng.random() < .7:
                            mesh.box((rng.uniform(-.35, .35), y+.4+row*.3, -.01), (.08, .04, .02), 'light', False)
        # The energy core in containment rings; a glowing ring round the hole.
        ring_band(mesh, R_CORE, R_CORE+.12, 0, math.tau, y+.5, y+6.5, 'glacier', solid=False, segs=24, ends=False)
        for h in (1.2, 3.0, 4.8, 6.6):
            ring_band(mesh, R_CORE+.12, R_CORE+.22, 0, math.tau, y+h, y+h+.14, 'brass', solid=False, segs=24, ends=False)
        arc_mark(mesh, R_STAIR+.2, R_STAIR+.45, sector_angle(LANDING), sector_angle(SECTORS-LANDING), y, 'glacier')
        # Cable trays round the room, and holo displays hung over the bays.
        for r in (11.4, 14.0):
            for sct in range(SECTORS):
                a0, a1 = sector_angle(sct), sector_angle(sct+1)
                hang = min(y+6.55, p.ceiling_at(r+.3, (a0+a1)/2)-.35, p.ceiling_at(r-.3, (a0+a1)/2)-.35)
                ring_band(mesh, r-.3, r+.3, a0, a1, hang, hang+.1, 'grate', solid=False, segs=1, ends=False, faces='ub')
                ring_band(mesh, r-.3, r-.26, a0, a1, hang, hang+.25, 'trim', solid=False, segs=1, ends=False, faces='io')
            for a in range(0, 360, 30):
                at = deg(a+7)
                x, _, z = pol(r, at)
                tube(mesh, (x, min(y+6.65, p.ceiling_at(r, at)-.25), z), (x, p.ceiling_at(r, at), z), .02, 'trim', sides=4)
        for a in (79, 281):
            x, _, z = pol(12.4, deg(a))
            b.prism(x, z, 1.1, 1.1, y+5.2, y+5.3, 16, 'trim', solid=False)
            b.prism(x, z, 1.0, .2, y+3.4, y+5.2, 16, 'glacier', solid=False, cap=False)
            tube(mesh, (x, y+5.3, z), (x, p.ceiling_at(12.4, deg(a)), z), .05, 'trim', sides=6)
            b.lamp((x, y+4.0, z), .6, CYAN)
        for a in range(0, 360, 30): b.lamp(pol(12, deg(a), y+6.4), 1.15, COOL)
    p.dress.append(dress)


DAYLIGHT = (1.0, .97, .86)


def tree(mesh, b, x, z, y, height, rng, crown=2.4, cap=None):
    """A tree: a tapering trunk (solid) and a crown of leafy clumps over head
    height (drawn only), each kept under `cap(x, z)` (the ceiling's height)."""
    b.prism(x, z, .32, .2, y, y+height*.75, 7, 'bark')
    for _ in range(7):
        cx, cz = x+rng.uniform(-.8, .8)*crown/2.4, z+rng.uniform(-.8, .8)*crown/2.4
        cy = y+height*rng.uniform(.62, .95)
        r = crown*rng.uniform(.28, .45)
        if cap: cy = min(cy, min(cap(cx+dx, cz+dz) for dx in (-r, r) for dz in (-r, r))-.1-r*.9)
        b.prism(cx, cz, r, r*.55, cy, cy+r*.9, 6, 'leaf', phase=rng.uniform(0, 1), solid=False)
        b.prism(cx, cz, r*.55, r, cy-r*.7, cy, 6, 'leaf', phase=rng.uniform(0, 1), solid=False, cap=False)


def greenhouse(p):
    """5: an overgrown greenhouse under a skylight: glass walls, a stream
    round the floor with lily pads and railed bridges, planted beds, trees,
    hanging planters, daylight."""
    for s0, s1 in ((6, 11), (13, 23), (25, 35), (37, 42)):
        p.pit(3, 5, s0, s1, 1.1, 'moss', 'water', color=[0.1, 0.3, 0.25])
    p.raise_(6, 8, 8, 16, .8, 'leaf'); p.raise_(6, 8, 32, 40, .8, 'leaf')
    p.spawns = [(14.5, deg(25)), (8.0, deg(130)), (15.0, deg(190)), (8.0, deg(230))]
    p.ceiling = 'glacier'
    p.strips = ()
    rng = random.Random(5)
    p.ambient = (0.9, 1.0, 0.85)
    def dress(mesh, b, base, y):
        top = y+STOREY-SLAB
        # Glass walls on a stone base, mullions between the panes.
        liner(mesh, 5, 'stone', y1=.9)
        liner(mesh, 5, 'glacier', y0=.9)
        for i in range(48):
            a = deg(i*7.5)
            x, _, z = pol(R_IN-.08, a)
            tube(mesh, (x, y+.9, z), (x, top, z), .06, 'trim', sides=4)
        for h in (.9, 3.2, 5.4):
            ring_band(mesh, R_IN-.14, R_IN-.04, 0, math.tau, y+h, y+h+.1, 'trim', solid=False, segs=48, ends=False, faces='iu')
        # The skylight: glass between ribs and rings, bright as day.
        for i in range(24):
            a = deg(i*15)
            for j in range(len(RINGS)-1):
                r0, r1 = max(RINGS[j], R_STAIR+.3), min(RINGS[j+1], R_IN-.1)
                h = min(p.ceiling_at(r0+.01, a+.01), p.ceiling_at(r0+.01, a-.01))-.12
                tube(mesh, pol(r0, a, h), pol(r1, a, h), .08, 'trim', sides=4)
        for r in (8.0, 11.0, 14.0, 17.0):
            ceiling_ring(mesh, p, r, .08, .16, 'trim')
        # Trees and their beds.
        for a, r in ((90, 15.2), (150, 15.6), (210, 15.0), (270, 15.4), (120, 8.0), (245, 8.2), (60, 16.4), (300, 16.6)):
            x, _, z = pol(r, deg(a))
            j, sct = int((r-RINGS[0])/1.6), int(deg(a)/SECTOR) % SECTORS
            ground = y+max(0.0, p.cells[min(j, 7)][sct][0])
            tree(mesh, b, x, z, ground, rng.uniform(5.0, 6.4), rng,
                 cap=lambda cx, cz: p.ceiling_at(math.hypot(cx, cz), math.atan2(cx, cz)))
        for s0, s1 in ((8, 16), (32, 40)):
            a0, a1 = sector_angle(s0), sector_angle(s1)
            ring_band(mesh, 14.8, 15.05, a0, a1, y+.8, y+.95, 'bark', solid=False, ends=False, faces='iu')
            for _ in range(14):
                a, r = rng.uniform(a0+.03, a1-.03), rng.uniform(15.4, 17.4)
                x, _, z = pol(r, a)
                w = rng.uniform(.35, .7)
                b.prism(x, z, w, w*.4, y+.8, y+.8+w*1.1, 6, 'leaf', phase=rng.uniform(0, 1), solid=False)
                if rng.random() < .5:
                    for _ in range(3):
                        fx, fz = x+rng.uniform(-.4, .4), z+rng.uniform(-.4, .4)
                        mesh.box((fx, y+.8+w*.9, fz), (.12, .1, .12), rng.choice(('ember', 'neon', 'gold', 'light')), False)
        # Lily pads on the stream; bridges with wooden rails where it parts.
        for (s0, s1) in ((6, 11), (13, 23), (25, 35), (37, 42)):
            for _ in range(int((s1-s0)*1.2)):
                a, r = rng.uniform(sector_angle(s0)+.03, sector_angle(s1)-.03), rng.uniform(10.4, 12.8)
                x, _, z = pol(r, a)
                b.prism(x, z, rng.uniform(.25, .45), .25, y-.14, y-.13, 7, 'leaf', solid=False)
        for s in (11, 23, 35, 42):
            a0, a1 = sector_angle(s), sector_angle(s+2 if s != 42 else s+12)
            for edge in (a0, a1):
                p0, p1 = pol(10.0, edge, y), pol(13.2, edge, y)
                t = (math.cos(edge), 0, -math.sin(edge))
                inset = .12 if edge == a0 else -.12
                q0 = (p0[0]+t[0]*inset, y, p0[2]+t[2]*inset); q1 = (p1[0]+t[0]*inset, y, p1[2]+t[2]*inset)
                for k in range(4):
                    f = k/3
                    pt = (q0[0]+(q1[0]-q0[0])*f, y, q0[2]+(q1[2]-q0[2])*f)
                    tube(mesh, pt, (pt[0], y+.85, pt[2]), .05, 'bark', sides=4)
                tube(mesh, (q0[0], y+.85, q0[2]), (q1[0], y+.85, q1[2]), .05, 'bark', sides=4, solid=True)
                tube(mesh, (q0[0], y+.4, q0[2]), (q1[0], y+.4, q1[2]), .05, 'bark', sides=4, solid=True)
            if s != 42:
                ring_band(mesh, 10.0, 13.2, a0, a1, y+.006, y+.012, 'bark', solid=False, ends=False, faces='u', segs=2)
        # Hanging planters, irrigation pipes along the wall.
        for i in range(10):
            a, r = deg(18+i*36+rng.uniform(-6, 6)), rng.uniform(8.0, 15.0)
            x, _, z = pol(r, a)
            tube(mesh, (x, y+4.9, z), (x, p.ceiling_at(r, a), z), .02, 'trim', sides=4)
            b.prism(x, z, .3, .42, y+4.4, y+4.9, 8, 'copper', solid=False)
            vine(mesh, x, z, y+4.6, rng.uniform(.8, 1.8), rng)
        for h in (.5, 6.2):
            arc_pipe(mesh, R_IN-.3, 0, math.tau, y+h, .07, 'copper', every=deg(15))
        for a in range(0, 360, 30):
            for r, at, power in ((12.0, deg(a), 1.35), (16.5, deg(a+15), .8)):
                b.lamp(pol(r, at, p.ceiling_at(r, at)-.6), power, DAYLIGHT)
    p.dress.append(dress)
    # The channel flows round the tower.
    p.pools = [(j0, j1, s0, s1, so, d, kind, dict(extras, flow=_tangent(s0, s1, 2.5))) for j0, j1, s0, s1, so, d, kind, extras in p.pools]


def _tangent(s0, s1, speed):
    a = sector_angle((s0+s1)/2)
    return [round(speed*math.cos(a), 3), round(-speed*math.sin(a), 3)]


FURNACE = (1.0, .5, .2)
# Where the foundry's machines vent steam: (r, angle in degrees, height).
STEAM = [(17.0, 30, .2), (17.0, 330, .2), (15.8, 95, 2.6), (15.8, 265, 2.6), (16.6, 180, 6.4)]


def pit_edges(mesh, p, y, mat, width=.3):
    """Marks round each pit's rim on the floor, so its edge reads from afar."""
    nr = len(RINGS)-1
    for j in range(nr):
        for s in range(SECTORS):
            if p.cells[j][s][0] >= 0: continue
            a0, a1 = sector_angle(s), sector_angle(s+1)
            if j > 0 and p.cells[j-1][s][0] == 0: arc_mark(mesh, RINGS[j]-width, RINGS[j], a0, a1, y, mat)
            if j < nr-1 and p.cells[j+1][s][0] == 0: arc_mark(mesh, RINGS[j+1], RINGS[j+1]+width, a0, a1, y, mat)
            for t, a, sgn in ((s-1, a0, -1), (s+1, a1, 1)):
                if p.cells[j][t % SECTORS][0] == 0:
                    mid = (RINGS[j]+RINGS[j+1])/2
                    radial_line(mesh, a+sgn*width/(2*mid), RINGS[j], RINGS[j+1], y, width, mat)


def foundry(p):
    """6: a foundry of gears and steam: molten pits that kill (their rims
    marked), walkways between them, platforms riding over the middle one,
    great gears turning on the walls, pipes, pistons venting steam, a
    crucible over the metal, orange light."""
    for s0, s1 in ((8, 14), (19, 29), (34, 40)):
        p.pit(2, 6, s0, s1, 0.9, 'trim', 'lava', surface=-0.6, burn=400.0)
    p.spawns = [(12.0, deg(122)), (12.0, deg(238)), (16.0, deg(170)), (16.0, deg(190))]
    p.ceiling = 'trim'
    p.strips = ()
    rng = random.Random(6)
    p.ambient = (.62, .46, .36)
    p.gain = 1.5
    def dress(mesh, b, base, y):
        top = y+STOREY-SLAB
        liner(mesh, 6, 'rust')
        for h in (.4, 3.0, 6.2):
            ring_band(mesh, R_IN-.12, R_IN-.04, 0, math.tau, y+h, y+h+.22, 'brass', solid=False, segs=48, ends=False, faces='iu')
        pit_edges(mesh, p, y, 'hazard')
        # Great gears on the wall, each with smaller ones meshing beside it.
        for a in (60, 180, 300):
            gear(mesh, base, R_IN-.06, deg(a), y+3.4, 2.3, 18, .3, 'brass', spin=a*.01)
            gear(mesh, base, R_IN-.06, deg(a)+3.35/R_IN, y+1.9, 1.2, 10, .25, 'copper', spin=.2)
            gear(mesh, base, R_IN-.06, deg(a)-3.0/R_IN, y+5.6, .9, 8, .2, 'copper', spin=.5)
        # Pipes round the wall; drops with valve wheels.
        for h, rad in ((5.7, .18), (6.5, .12)):
            arc_pipe(mesh, R_IN-.45, 0, math.tau, y+h, rad, 'copper', every=deg(15))
        for a in (20, 80, 140, 200, 260, 320):
            x, _, z = pol(R_IN-.45, deg(a))
            tube(mesh, (x, y, z), (x, y+5.7, z), .14, 'copper', sides=6)
            gear(mesh, base, R_IN-.62, deg(a), y+1.4, .32, 6, .06, 'hazard', hub='brass')
        # Piston machines venting steam.
        for a in (95, 265):
            ad = deg(a)
            block(mesh, base, 15.8, ad, (1.6, 1.4, 1.6), y, 'brass')
            block(mesh, base, 15.8, ad, (1.7, .12, 1.7), y+1.4, 'trim', solid=False)
            x, _, z = pol(15.8, ad)
            tube(mesh, (x, y+1.4, z), (x, y+2.6, z), .22, 'trim', sides=8, caps=True)
            tube(mesh, (x-.5, y+1.5, z), (x-.5, y+3.4, z), .12, 'copper', sides=6)
        # A crucible hung over the middle pit, tipped to pour.
        x, _, z = pol(11.6, deg(180))
        b.prism(x, z, .7, 1.1, y+4.2, y+5.4, 10, 'trim', solid=False)
        b.prism(x, z, .9, .9, y+5.3, y+5.45, 10, 'brass', solid=False, cap=False)
        for dx in (-1.0, 1.0):
            tube(mesh, (x+dx, y+5.4, z), (x+dx*.2, p.ceiling_at(11.6, deg(180))-.05, z), .04, 'trim', sides=4)
        b.lamp((x, y+3.9, z), .8, FURNACE)
        # Orange light (none low over the metal: lit unevenly, the pits'
        # floors would show through it).
        for a in range(0, 360, 45):
            hanging_lamp(mesh, b, 15.6, deg(a+22.5), p.ceiling_at(15.6, deg(a+22.5)), 1.2, .9, FURNACE, shade='rust', size=.6)
        for a in range(0, 360, 30): b.lamp(pol(8.4, deg(a), y+6.2), .5, FURNACE)
    p.dress.append(dress)


LAMPLIGHT = (1.0, .8, .52)


def ladder(mesh, x0, z0, x1, z1, y, height, lean=.5):
    """A ladder from the floor at (x0, z0) leaning `lean` toward (x1, z1)
    (drawn only)."""
    dx, dz = x1-x0, z1-z0
    n = math.hypot(dx, dz) or 1.0
    dx, dz = dx/n, dz/n
    sx, sz = -dz*.22, dx*.22
    for side in (-1, 1):
        tube(mesh, (x0+sx*side, y, z0+sz*side), (x0+sx*side+dx*lean, y+height, z0+sz*side+dz*lean), .03, 'bark', sides=4)
    for i in range(1, int(height/.3)):
        f = i*.3/height
        cx, cy, cz = x0+dx*lean*f, y+height*f, z0+dz*lean*f
        tube(mesh, (cx-sx, cy, cz-sz), (cx+sx, cy, cz+sz), .02, 'bark', sides=4)


def chandelier(mesh, b, x, z, ceiling, drop, color, power=.9):
    """A ring of candles hung on chains (drawn only), and its light."""
    y = ceiling-drop
    b.prism(x, z, .9, .9, y, y+.08, 12, 'brass', solid=False, cap=False)
    for i in range(3):
        t = math.tau*i/3
        tube(mesh, (x+.9*math.cos(t), y+.08, z+.9*math.sin(t)), (x, ceiling, z), .015, 'brass', sides=4)
    for i in range(8):
        t = math.tau*i/8
        cx, cz = x+.9*math.cos(t), z+.9*math.sin(t)
        b.prism(cx, cz, .04, .04, y+.08, y+.26, 4, 'marble', solid=False)
        b.prism(cx, cz, .03, .005, y+.26, y+.36, 4, 'light', solid=False, cap=False)
    b.lamp((x, y-.2, z), power, color)


def library(p):
    """7: a grand library in lamplight: walls of books, a mezzanine balcony
    on bookcases behind a balustrade, shelves to weave through with ladders
    against them, carpets down the aisles, reading tables, a globe,
    chandeliers under a wooden ceiling."""
    p.platform(7, 8, 8, 30, 3.0, 'bark', up='after')
    p.spawns = [(9.5, deg(60)), (13.0, deg(150)), (13.0, deg(210)), (9.5, deg(300))]
    p.ceiling = 'bark'
    p.strips = ()
    rng = random.Random(7)
    shelves = [a for a in range(30, 340, 24) if not 140 < a < 220]
    p.ambient = (.68, .58, .44)
    p.gain = 1.4
    def dress(mesh, b, base, y):
        liner(mesh, 7, 'books')
        a0, a1 = sector_angle(8), sector_angle(30)
        # The mezzanine: bookcases under it, a balustrade along it.
        ring_band(mesh, 16.36, 16.38, a0, a1, y+.1, y+2.8, 'books', solid=False, ends=False, faces='i')
        for i in range(12):
            x, _, z = pol(16.25, a0+(a1-a0)*i/11)
            post(mesh, x, z, .14, y, y+3.0, 'bark', sides=6, solid=False)
        ring_band(mesh, 16.2, 16.4, a0, a1, y+2.8, y+3.0, 'bark', solid=False, ends=False, faces='iu')
        railing(mesh, 16.55, a0+deg(.5), a1-deg(.5), y+3.0, mat='bark', top='bark', every=deg(3))
        # Shelves: capped, framed, ladders against some.
        for a in shelves:
            ad = deg(a)
            radial_wall(b, ad, 10.4, 15.6, y, y+3.6, 'books', thick=.7)
            radial_wall(b, ad, 10.3, 15.7, y+3.6, y+3.75, 'bark', thick=.82, solid=False)
            for r in (10.35, 15.65):
                block(mesh, base, r, ad, (.8, 3.6, .12), y, 'bark', solid=False)
            if rng.random() < .55:
                side = rng.choice((-1, 1))
                r = rng.uniform(11.5, 14.5)
                t = (math.cos(ad)*side, -math.sin(ad)*side)
                bx, _, bz = pol(r, ad)
                ladder(mesh, bx+t[0]*1.0, bz+t[1]*1.0, bx, bz, y, 3.5, lean=.62)
        # Carpets down the aisles and round the stair.
        for a in shelves:
            nxt = a+24
            if any(140 < c < 220 for c in (a, nxt)) or nxt > 330: continue
            mid = deg(a+12)
            for r0, r1 in ((8.8, 16.0),):
                t, h = (math.cos(mid), 0, -math.sin(mid)), .7
                p0, p1 = pol(r0, mid, y), pol(r1, mid, y)
                decal(mesh, [(p0[0]-t[0]*h, y, p0[2]-t[2]*h), (p1[0]-t[0]*h, y, p1[2]-t[2]*h),
                             (p1[0]+t[0]*h, y, p1[2]+t[2]*h), (p0[0]+t[0]*h, y, p0[2]+t[2]*h)], 'carpet')
        arc_mark(mesh, 6.4, 8.4, sector_angle(LANDING+1), sector_angle(SECTORS-LANDING-1), y, 'carpet')
        # Reading tables with lamps and books; a globe.
        for a in (90, 180, 270):
            ad = deg(a)
            block(mesh, base, 7.9, ad, (2.8, .95, 1.4), y, 'bark')
            block(mesh, base, 7.9, ad, (2.9, .06, 1.5), y+.95, 'bark', solid=False)
            for k in (-.9, .9):
                x, _, z = pol(7.9, ad+k/7.9)
                b.prism(x, z, .08, .08, y+1.01, y+1.35, 6, 'brass', solid=False)
                b.prism(x, z, .26, .1, y+1.35, y+1.55, 6, 'leaf', solid=False)
                b.lamp((x, y+1.3, z), .35, LAMPLIGHT)
            for k in range(3):
                block(mesh, base, 7.9+rng.uniform(-.4, .4), ad+rng.uniform(-.08, .08), (.3, .08+.06*k, .22), y+1.01,
                      rng.choice(('carpet', 'books', 'steelblue')), turn=rng.uniform(0, 3), solid=False)
        gx, _, gz = pol(11.0, deg(180))
        b.prism(gx, gz, .35, .2, y, y+.9, 8, 'bark')
        b.prism(gx, gz, .45, .62, y+.9, y+1.25, 10, 'steelblue', solid=False)
        b.prism(gx, gz, .62, .45, y+1.25, y+1.6, 10, 'leaf', solid=False)
        # A coffered ceiling: beams across it.
        for i in range(24):
            a = deg(i*15+7.5)
            for j in range(len(RINGS)-1):
                r0, r1 = RINGS[j], RINGS[j+1]
                h = min(p.ceiling_at(r0+.01, a), p.ceiling_at(r1-.01, a))
                radial_wall(b, a, r0, r1, h-.3, h-.005, 'bark', thick=.3, solid=False)
        for r in (9.6, 13.6):
            ceiling_ring(mesh, p, r, .15, .3, 'bark')
        for a in range(0, 360, 45):
            x, _, z = pol(12.2, deg(a+22.5))
            chandelier(mesh, b, x, z, p.ceiling_at(12.2, deg(a+22.5)), 1.6, LAMPLIGHT, 1.0)
        for a in range(0, 360, 30): b.lamp(pol(16.8, deg(a+15), y+5.6), .6, LAMPLIGHT)
    p.dress.append(dress)


VIOLET = (.7, .42, 1.0)


def boulder(b, x, z, y0, y1, r, rng, mat='rock', solid=False):
    """A rough rock: a lumpy frustum of five to seven sides; its radii at
    the foot and the top."""
    sides = rng.choice((5, 6, 7))
    r0, r1 = r*rng.uniform(.85, 1.1), r*rng.uniform(.45, .75)
    b.prism(x, z, r0, r1, y0, y1, sides, mat, phase=rng.uniform(0, 1), solid=solid)
    return r0, r1


def collide_prism(mesh, x, z, r, y0, y1, sides=8):
    """Collision only: an upright prism, its sides and top, standing in for a
    shape drawn leaning or lumpy. Leaning faces too steep to stand on make
    crevices, against a wall or each other, that hold a body off the ground
    where it can't jump; upright ones can't hold anyone up."""
    ring = lambda y: [(x+r*math.cos(i*math.tau/sides), y, z+r*math.sin(i*math.tau/sides)) for i in range(sides)]
    lo, hi = ring(y0), ring(y1)
    def tri(*pts):
        q = [mesh.point(v) for v in pts]
        mesh.collision.extend(c for v in q for c in v)
    for i in range(sides):
        j = (i+1) % sides
        tri(lo[i], hi[i], hi[j]); tri(lo[i], hi[j], lo[j])
        tri((x, y1, z), hi[j], hi[i])


# Upright rocks need gaps a body can't wedge in: closed, or wider than this.
ROCK_GAP = 1.2


def check_rock_gaps(stones, k):
    """Fail the build where two upright rocks, or a rock and the round wall,
    stand closer than a body is wide (and wider than nothing): a crevice a
    body falls into and can't climb out of without jets."""
    for i, (x, z, r, lo, hi) in enumerate(stones):
        wall = R_IN*math.cos(math.pi/48)-(math.hypot(x, z)+r)
        if 0 < wall < ROCK_GAP:
            raise SystemExit(f'floor {k}: a rock {wall:.2f} m off the wall at ({x:.1f}, {z:.1f})')
        for x2, z2, r2, lo2, hi2 in stones[i+1:]:
            gap = math.hypot(x2-x, z2-z)-r-r2
            if min(hi, hi2) > max(lo, lo2) and 0 < gap < ROCK_GAP:
                raise SystemExit(f'floor {k}: rocks {gap:.2f} m apart at ({x:.1f}, {z:.1f}) and ({x2:.1f}, {z2:.1f})')


# The cavern's crystal pillars: (angle, radius, height).
CRYSTALS = ((40, 14.5, 5.0), (100, 9.2, 3.2), (155, 13.4, 6.2), (215, 9.8, 2.6), (265, 13.6, 5.4), (325, 13.6, 3.8))


def crystal_cavern(p):
    """8: a crystal cavern in violet and cyan: uneven rock underfoot,
    boulders round the walls, stalactites and stalagmites, crystal pillars
    and clusters glowing, a still pool."""
    rng = random.Random(8)
    spots = [(10.0, deg(70)), (13.0, deg(130)), (12.5, deg(240)), (10.0, deg(300))]
    keep = {(int((r-RINGS[0])/1.6)+dj, (int(a/SECTOR)+ds) % SECTORS) for r, a in spots for dj in (-1, 0, 1) for ds in (-1, 0, 1)}
    # Flat round the rocks: the two rings by the wall, where its boulders
    # stand, and about each crystal pillar. A raised cell beside a rock walls
    # a pocket narrower than a body, which holds it up on the rims.
    def near_crystal(j, s):
        x, _, z = pol(ring_mid(j), (s+.5)*SECTOR)
        return any(math.hypot(x-pol(r, deg(a))[0], z-pol(r, deg(a))[2]) < 3.0 for a, r, _ in CRYSTALS)
    for j in range(1, 8):
        for s in range(SECTORS):
            if landing_clear(j, s) or (j, s) in keep or rng.random() < .55: continue
            h = rng.choice((.3, .3, .6))
            if j < 6 and not near_crystal(j, s): p.raise_(j, j+1, s, s+1, h, 'rock')
    p.pit(5, 7, 20, 26, 1.0, 'rock', 'water', color=[0.2, 0.15, 0.45])
    p.spawns = spots
    p.ceiling = 'rock'
    p.strips = ()
    p.ambient = (.5, .42, .74)
    p.gain = 1.8
    def foot(x, z):
        """The floor under (x, z): a pit's floor in the pool, else the floor."""
        r, a = math.hypot(x, z), math.atan2(x, z) % math.tau
        j = max(0, min(len(RINGS)-2, int((r-RINGS[0])/1.6)))
        return min(p.cells[j][int(a/SECTOR) % SECTORS][0], 0.0)
    def dress(mesh, b, base, y):
        liner(mesh, 8, 'rock')
        # Boulders round the wall: the low ones drawn lumpy but solid as
        # upright stones (`collide_prism`) set into the wall, a body's width
        # and more apart, down to the pool's floor where they stand in it;
        # the high ones drawn only.
        stones = []
        for i in range(30):
            jitter = rng.uniform(-3, 3)
            a = deg(i*12)
            out = rng.uniform(17.0, 17.5)
            h = rng.uniform(.8, 2.2)
            size = rng.uniform(.8, 1.3)
            x, _, z = pol(out, a)
            low = y+foot(x, z)-.2
            r0, r1 = boulder(b, x, z, low, y+h, size, rng)
            stand = min((r0+r1)/2, 1.05)
            # Into the wall, so no slot opens behind it.
            out = max(out, R_IN+.05-stand)
            cx, _, cz = pol(out, a)
            collide_prism(mesh, cx, cz, stand, low, y+h)
            stones.append((cx, cz, stand, low, y+h))
            a = deg(i*12+jitter)
            x, _, z = pol(17.6, a+deg(6))
            top = p.ceiling_at(17.6, a+deg(6))
            boulder(b, x, z, y+rng.uniform(2.5, 4.0), top-.02, rng.uniform(.7, 1.2), rng)
        # Stalactites, and crystal ones; stalagmites by the wall.
        for _ in range(40):
            r, a = rng.uniform(6.5, 17.0), rng.uniform(0, math.tau)
            x, _, z = pol(r, a)
            top = p.ceiling_at(r, a)
            length = rng.uniform(.6, 2.6)
            mat = 'crystal' if rng.random() < .25 else 'rock'
            b.prism(x, z, .02, rng.uniform(.18, .4), top-length, top-.01, rng.choice((5, 6)), mat, phase=rng.uniform(0, 1), solid=False, cap=False)
        for _ in range(12):
            r, a = rng.uniform(15.6, 16.6), rng.uniform(0, math.tau)
            x, _, z = pol(r, a)
            b.prism(x, z, rng.uniform(.25, .4), .03, y, y+rng.uniform(.8, 1.8), 5, 'rock', phase=rng.uniform(0, 1), solid=False, cap=False)
        # Crystal pillars and clusters, lit from within.
        for a, r, h in CRYSTALS:
            x, _, z = pol(r, deg(a))
            low = y+foot(x, z)-.2
            b.prism(x, z, .9, .25, low, y+h, 6, 'crystal', solid=False)
            b.prism(x+.9, z-.4, .5, .12, low, y+h*.55, 5, 'crystal', solid=False)
            # Solid as two upright stones that overlap, so no cleft between.
            collide_prism(mesh, x, z, .72, low, y+h, sides=6)
            collide_prism(mesh, x+.9, z-.4, .4, low, y+h*.55, sides=5)
            stones += [(x, z, .72, low, y+h), (x+.9, z-.4, .4, low, y+h*.55)]
            for _ in range(4):
                cx, cz = x+rng.uniform(-1.2, 1.2), z+rng.uniform(-1.2, 1.2)
                tilt = rng.uniform(.6, 1.6)
                b.prism(cx, cz, rng.uniform(.12, .25), .02, y-.1, y+tilt, 5, 'crystal', phase=rng.uniform(0, 1), solid=False)
            b.lamp((x, y+min(h, 3.0)*.6, z), .8, VIOLET if a % 2 else CYAN)
        check_rock_gaps(stones, 8)
        for i in range(10):
            a = deg(i*36+18)
            x, _, z = pol(16.4, a)
            for _ in range(5):
                cx, cz = x+rng.uniform(-.6, .6), z+rng.uniform(-.6, .6)
                b.prism(cx, cz, rng.uniform(.1, .22), .02, y, y+rng.uniform(.4, 1.3), 5, 'crystal', phase=rng.uniform(0, 1), solid=False)
            b.lamp((x, y+.9, z), .55, CYAN if i % 2 else VIOLET)
        for a in range(0, 360, 30):
            b.lamp(pol(11.5, deg(a), p.ceiling_at(11.5, deg(a))-1.6), .55, VIOLET if a % 60 else CYAN)
    p.dress.append(dress)


GOLDLIGHT = (1.0, .84, .56)


def statue(mesh, base, r, a, y, mat='marble'):
    """An armored figure on a plinth, facing the axis: the plinth and body
    solid, the sword drawn only."""
    x, _, z = pol(r, a)
    with Placed(mesh, base, x, z, a+math.pi):
        mesh.box((0, y+.6, 0), (1.3, 1.2, 1.3), 'marble')
        mesh.box((0, y+1.25, 0), (1.4, .1, 1.4), 'gold', False)
        for side in (-.22, .22):
            mesh.box((side, y+1.3+.45, 0), (.26, .9, .3), mat, side < 0)
        mesh.box((0, y+2.2+.4, 0), (.75, .8, .4), mat)
        mesh.box((0, y+3.05, 0), (.92, .18, .45), mat, False)
        mesh.box((0, y+3.3, 0), (.32, .36, .34), mat, False)
        mesh.box((0, y+3.52, 0), (.36, .1, .38), 'gold', False)
        for side in (-.5, .5):
            mesh.box((side, y+2.55, .05), (.2, .75, .22), mat, False)
        tube(mesh, (0, y+1.35, .45), (0, y+2.7, .45), .04, 'marble', sides=4)
        mesh.box((0, y+2.35, .45), (.5, .06, .08), 'gold', False)


def throne_hall(p):
    """9: an ornate throne hall in golden light: two rings of marble
    columns, the throne on its dais up two flights of steps, statues on
    plinths, banners, braziers, a carpet to the throne, chandeliers."""
    p.platform(4, 8, 20, 29, 1.5, 'marble', up='both')
    p.spawns = [(11.0, deg(60)), (13.0, deg(110)), (13.0, deg(250)), (11.0, deg(300))]
    p.ceiling = 'marble'
    p.strips = ()
    p.ambient = (.85, .76, .6)
    rng = random.Random(9)
    def dress(mesh, b, base, y):
        top = y+STOREY-SLAB
        liner(mesh, 9, 'marble')
        for h in (.0, 3.4):
            ring_band(mesh, R_IN-.12, R_IN-.04, 0, math.tau, y+h, y+h+.3, 'gold', solid=False, segs=48, ends=False, faces='iu')
        for r in (9.6, 13.0, 16.2):
            ceiling_ring(mesh, p, r, .18, .22, 'gold')
        # Columns: bases and gilded capitals.
        for a in range(15, 360, 30):
            for r in (8.6, 16.2):
                if r < 10 and (a < 30 or a > 330): continue
                cell = p.cells[int((r-RINGS[0])/1.6)][int(a/7.5) % SECTORS][0]
                x, _, z = pol(r, deg(a))
                ceiling = p.ceiling_at(r, deg(a))
                post(mesh, x, z, .55, y+cell, ceiling, 'marble', sides=12)
                b.prism(x, z, .8, .66, y+cell, y+cell+.4, 12, 'marble', solid=False)
                b.prism(x, z, .62, .9, ceiling-.6, ceiling-.01, 12, 'gold', solid=False, cap=False)
        # The throne: seat, high back, arms, a cushion.
        x, _, z = pol(16.2, deg(180))
        with Placed(mesh, base, x, z, deg(180)+math.pi):
            mesh.box((0, y+1.5+.35, .2), (1.6, .7, 1.0), 'gold')
            mesh.box((0, y+1.5+.74, .2), (1.3, .1, .9), 'carpet', False)
            mesh.box((0, y+1.5+1.5, .75), (1.7, 3.0, .3), 'gold')
            face(mesh, [(-.85, y+4.5, .6), (.85, y+4.5, .6), (0, y+5.4, .6)], 'gold', (0, 0, -1), solid=False)
            face(mesh, [(-.85, y+4.5, .9), (.85, y+4.5, .9), (0, y+5.4, .9)], 'gold', (0, 0, 1), solid=False)
            mesh.box((0, y+1.5+1.7, .58), (1.2, 2.2, .04), 'carpet', False)
            for side in (-.9, .9):
                mesh.box((side, y+1.5+.55, .15), (.25, 1.1, 1.1), 'gold', False)
        # Banners between the outer columns; statues at the wall.
        for a in range(0, 360, 30):
            if 140 < a < 225: continue
            block(mesh, base, 17.9, deg(a), (1.3, 3.8, .06), y+2.4, 'carpet', solid=False)
            block(mesh, base, 17.86, deg(a), (1.3, .14, .08), y+6.2, 'gold', solid=False)
        for a in (45, 75, 105, 255, 285, 315):
            statue(mesh, base, 17.2, deg(a), y)
        for a in (150, 210):
            block(mesh, base, 17.9, deg(a), (2.0, 4.5, .1), y+2.0, 'carpet', solid=False)
        # Braziers by the dais, a carpet to the throne.
        for a in (160, 200):
            x, _, z = pol(10.6, deg(a))
            b.prism(x, z, .2, .14, y, y+1.1, 8, 'gold')
            b.prism(x, z, .2, .55, y+1.1, y+1.45, 8, 'gold', solid=False, cap=False)
            b.prism(x, z, .45, .05, y+1.3, y+1.75, 6, 'light', solid=False, cap=False)
            b.lamp((x, y+2.0, z), 1.1, (1.0, .66, .32))
        for r0, r1, yy in ((6.4, 11.55, y), (11.65, 15.4, y+1.5)):
            t, h = (math.cos(math.pi), 0, -math.sin(math.pi)), .9
            p0, p1 = pol(r0, math.pi, yy), pol(r1, math.pi, yy)
            decal(mesh, [(p0[0]-t[0]*h, yy, p0[2]-t[2]*h), (p1[0]-t[0]*h, yy, p1[2]-t[2]*h),
                         (p1[0]+t[0]*h, yy, p1[2]+t[2]*h), (p0[0]+t[0]*h, yy, p0[2]+t[2]*h)], 'carpet')
        arc_mark(mesh, 6.0, 6.9, sector_angle(LANDING+1), sector_angle(SECTORS-LANDING-1), y, 'carpet')
        for a in range(0, 360, 60):
            x, _, z = pol(12.4, deg(a+30))
            chandelier(mesh, b, x, z, p.ceiling_at(12.4, deg(a+30)), 1.8, GOLDLIGHT, 1.1)
        for a in range(0, 360, 30): b.lamp(pol(12, deg(a), y+6.4), .8, GOLDLIGHT)
    p.dress.append(dress)


DOME_TOP = 17.0             # the dome's oculus over the crown's floor
OCULUS = 4.0


def crown(p):
    """10: the crown, an open deck under a gilded dome of ribs round an
    oculus: low walls to duck behind, an inlaid star, lanterns, a telescope
    at the parapet."""
    p.spawns = [(14.0, deg(60)), (14.0, deg(150)), (14.0, deg(210)), (14.0, deg(300))]
    def dress(mesh, b, base, y):
        for a in range(30, 360, 60):
            ring_band(mesh, 10.6, 11.2, deg(a-12), deg(a+12), y, y+1.1, 'marble', segs=3)
            ring_band(mesh, 10.55, 11.25, deg(a-12.2), deg(a+12.2), y+1.1, y+1.18, 'gold', solid=False, segs=3)
        for a in range(0, 360, 45):
            x, _, z = pol(17.4, deg(a+22.5))
            post(mesh, x, z, .45, y, y+CANOPY_Y, 'marble', sides=10)
            b.prism(x, z, .62, .5, y, y+.35, 10, 'gold', solid=False)
            b.prism(x, z, .5, .7, y+CANOPY_Y-.5, y+CANOPY_Y, 10, 'gold', solid=False, cap=False)
            lx, _, lz = pol(16.85, deg(a+22.5))
            b.prism(lx, lz, .16, .16, y+2.6, y+3.1, 6, 'light', solid=False)
            b.prism(lx, lz, .2, .05, y+3.1, y+3.3, 6, 'gold', solid=False)
            b.lamp((lx, y+2.8, lz), .7)
        # The canopy: a ring roof with an open eye over the stair.
        ring_band(mesh, 7.0, R_OUT+.6, 0, math.tau, y+CANOPY_Y, y+CANOPY_Y+.6, 'panel', segs=48, ends=False)
        ring_band(mesh, 7.0, 7.4, 0, math.tau, y+CANOPY_Y-.2, y+CANOPY_Y, 'light', solid=False, segs=48, ends=False)
        ring_band(mesh, R_OUT+.5, R_OUT+.7, 0, math.tau, y+CANOPY_Y, y+CANOPY_Y+.7, 'gold', solid=False, segs=48, ends=False, faces='ou')
        # The dome: gilded ribs from the canopy's rim to the oculus.
        def at(t, a):
            q = t*math.pi/2
            return pol(OCULUS+(R_OUT+.3-OCULUS)*math.cos(q), a, y+CANOPY_Y+.6+(DOME_TOP-CANOPY_Y-.6)*math.sin(q))
        for i in range(16):
            a = deg(i*22.5)
            for k in range(8):
                tube(mesh, at(k/8, a), at((k+1)/8, a), .16, 'gold', sides=6)
        for t in (.35, .7):
            q = t*math.pi/2
            arc_pipe(mesh, OCULUS+(R_OUT+.3-OCULUS)*math.cos(q), 0, math.tau, y+CANOPY_Y+.6+(DOME_TOP-CANOPY_Y-.6)*math.sin(q), .1, 'gold', every=deg(11.25))
        arc_pipe(mesh, OCULUS, 0, math.tau, y+DOME_TOP, .3, 'gold', every=deg(15))
        ring_band(mesh, OCULUS-.25, OCULUS+.25, 0, math.tau, y+DOME_TOP-.32, y+DOME_TOP-.28, 'light', solid=False, segs=24, ends=False, faces='b')
        # The core's beacon under the oculus.
        b.prism(0, 0, R_CORE+.1, 1.2, y+CANOPY_Y, y+CANOPY_Y+1.2, 16, 'gold', solid=False)
        b.prism(0, 0, .5, .5, y+CANOPY_Y+1.2, y+CANOPY_Y+2.4, 12, 'light', solid=False)
        # An inlaid star on the deck.
        for i in range(8):
            a = deg(i*45)
            for side in (-1, 1):
                p0, p1, p2 = pol(6.0, a+side*deg(9)), pol(9.8, a), pol(6.0, a)
                decal(mesh, [(p0[0], y, p0[2]), (p1[0], y, p1[2]), (p2[0], y, p2[2])], 'gold' if side > 0 else 'brass')
        arc_mark(mesh, 9.8, 10.05, 0, math.tau, y, 'gold')
        # A telescope at the parapet.
        tx, _, tz = pol(16.8, deg(0))
        for k in range(3):
            fa = deg(k*120)
            tube(mesh, (tx+.5*math.sin(fa), y, tz+.5*math.cos(fa)), (tx, y+1.3, tz), .03, 'brass', sides=4)
        tube(mesh, (tx, y+1.25, tz-.6), (tx, y+1.75, tz+.7), .1, 'brass', sides=8, caps=True)
        for a in range(0, 360, 30): b.lamp(pol(13, deg(a), y+CANOPY_Y-.6), .9)
    p.dress.append(dress)


DESIGNS = [cargo_bay, neon_alley, ruins, laboratory, greenhouse, foundry, library, crystal_cavern, throne_hall, crown]
NAMES = ['Cargo Bay', 'Neon Alley', 'Ruins', 'Laboratory', 'Greenhouse', 'Foundry', 'Library', 'Crystal Cavern', 'Throne Hall', 'Crown']
TOPS = ['concrete', 'concrete', 'stone', 'tile', 'moss', 'grate', 'bark', 'rock', 'marble', 'marble']


def plans():
    out = []
    for k, design in enumerate(DESIGNS, 1):
        p = Plan(k, TOPS[k-1])
        design(p)
        for j in range(len(RINGS)-1):
            for s in range(SECTORS):
                if landing_clear(j, s) and p.cells[j][s][0] != 0: raise ValueError(f'floor {k}: the landing apron must stay flat')
        out.append(p)
    for lower, upper in zip(out, out[1:]): lower.above = upper
    return out


def under_ceiling(mesh, base, p, first, lamps_from):
    """Fail the build if anything the floor's dressing added (vertices from
    `first`, lamps from `lamps_from`) reaches into the floor above: through
    its underside, or up into its pits."""
    if p.above is None: return                      # the crown: open to the sky
    v = mesh.vertices
    bad = []
    points = [(v[i]-base[0], v[i+1]-base[1], v[i+2]-base[2]) for i in range(first, len(v), 12)]
    points += [(q[0][0]-base[0], q[0][1]-base[1], q[0][2]-base[2]) for q in mesh.lamps[lamps_from:]]
    for x, y, z in points:
        r = math.hypot(x, z)
        a = math.atan2(x, z)
        # (A point on a cell's edge is fine when any cell it touches allows it.)
        if R_STAIR+.02 < r < R_IN-.01 and y > max(p.ceiling_at(r+dr, a+da) for dr in (-.02, .02) for da in (-.002, .002))+.01: bad.append((round(r, 2), round(math.degrees(math.atan2(x, z)) % 360, 1), round(y-p.y, 2)))
    if bad: raise ValueError(f'floor {p.k}: {len(bad)} points of its dressing reach into the floor above, e.g. (r, deg, up) {bad[:4]}')


def build(mesh, base):
    """The tower at `base` (world, the ground floor's top on the axis).
    Returns the manifest's zones, water volumes, blade spawns and anchors."""
    b = Builder(mesh, 'ember', interior='panel', metal='trim', glow='light')
    mesh.origin, mesh.yaw = base, 0.0
    core(mesh, b)
    outer_wall(mesh, b, DOORS)
    floors = plans()
    zones, volumes = [], []
    for p in floors:
        k, y = p.k, p.y
        mesh.origin, mesh.yaw = base, 0.0
        mesh_floor(mesh, p, (lambda s: True) if k == 1 else (lambda s: s < LANDING or s >= SECTORS-LANDING),
                   floors[k-2].ceiling if k > 1 else 'panel')
        landing(mesh, k)
        hole_rail(mesh, k)
        if k < FLOORS: stair(mesh, b, k)
        if k < FLOORS and p.strips: ceiling_strips(mesh, floors[k], p.strips)
        first, lamps_from = len(mesh.vertices), len(mesh.lamps)
        for dress in p.dress:
            mesh.origin, mesh.yaw = base, 0.0
            dress(mesh, b, base, y)
        under_ceiling(mesh, base, p, first, lamps_from)
        if p.gain != 1.0:
            mesh.lamps[lamps_from:] = [(q[0], q[1]*p.gain, *q[2:]) for q in mesh.lamps[lamps_from:]]
        mesh.origin, mesh.yaw = base, 0.0
        height = STOREY if k < FLOORS else CANOPY_Y+1.0
        zones.append(dict(kind='blade', name=f'Floor {k} · {NAMES[k-1]}', at=[base[0], base[1]+y+height/2-.3, base[2]], size=[R_OUT, height/2, R_OUT],
                          radius=R_IN if k < FLOORS else R_OUT, spawns=[
                              [round(base[0]+r*math.sin(a), 3), round(base[1]+y+_cell_top(p, r, a)+SPAWN_LIFT, 3), round(base[2]+r*math.cos(a), 3),
                               round(math.degrees(a) % 360, 3)] for r, a in p.spawns]))
        for j0, j1, s0, s1, surface, depth, kind, extras in p.pools:
            pts = []
            for s in range(s0, s1+1): pts.append(pol(RINGS[j1], sector_angle(s)))
            for s in range(s1, s0-1, -1): pts.append(pol(RINGS[j0], sector_angle(s)))
            v = dict(surface=round(base[1]+y+surface, 3), polygon=[[round(base[0]+q[0], 3), round(base[2]+q[2], 3)] for q in pts],
                     depth=round(depth+.2, 3))
            if kind == 'lava': v.update(lava=True, burn=extras.get('burn', 400.0))
            else:
                if 'color' in extras: v['color'] = extras['color']
                if 'flow' in extras: v['flow'] = extras['flow']
            volumes.append(v)
    return dict(zones=zones, water_volumes=volumes, floors=floors)


def ambient_zones(base, floors):
    """Each floor's own ambient light for the bake (`Plan.ambient`, ambient
    zones in `lightmap_bake`): the room from just over its own underside to
    just over the floor above's, and each of its pits down to the middle of
    the slab under it (a pit's floor lies lower than the ceiling under it
    elsewhere)."""
    centre = (base[0], base[2])
    rooms, pits = [], []
    for p in floors:
        if p.ambient is None: continue
        y0 = base[1]+p.y-SLAB+.01 if p.k > 1 else base[1]-SLAB-2.0
        y1 = base[1]+(p.above.y-SLAB+.01 if p.above else p.y+CANOPY_Y)
        rooms.append((centre, (0.0, R_IN+.2), None, (y0, y1), p.ambient))
        for j0, j1, s0, s1, depth in p.pits:
            pits.append((centre, (RINGS[j0]-.01, RINGS[j1]+.01), (sector_angle(s0)-.002, sector_angle(s1)+.002),
                         (base[1]+p.y-depth-SLAB/2, base[1]+p.y+.02), p.ambient))
    return rooms+pits


def _cell_top(p, r, a):
    j = max(0, min(len(RINGS)-2, int((r-RINGS[0])/1.6)))
    s = int((a % math.tau)/SECTOR) % SECTORS
    return max(0.0, p.cells[j][s][0])


def moveables(base):
    """The foundry's platforms, riding to and fro over its middle pit."""
    y = base[1]+floor_y(6)-0.15
    out = []
    for r, reverse in ((10.0, False), (13.2, True)):
        angles = [deg(140+80*i/6) for i in range(7)]
        if reverse: angles = angles[::-1]
        path = [[round(base[0]+pol(r, a)[0], 3), round(y, 3), round(base[2]+pol(r, a)[2], 3)] for a in angles]
        out.append(dict(size=[1.2, .15, 1.2], path=path, speed=2.2, wait=1.5, drive='loop', look='platform'))
    return out
