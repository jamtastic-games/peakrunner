"""Original Dustreach sewer: a stone culvert under the dunes from one team's
rear to the other's, with open drop shafts to the surface.

Layout, in world coordinates. The map is point-symmetric about (1024, 1024)
and so is everything here: the red half is built in world coordinates and the
blue half is its 180-degree rotation. Every wall stands on an 8 m grid line,
so each opening meets the terrain edge exactly.

- Red leg: one cell wide (x 1072-1080), from the red mouth (z 360) straight
  to the cross hall (z 1016). The leg runs 16 m east of the watch tower and
  never passes under the citadel, its cistern, its tunnel or its storehouse.
- Cross hall: two cells deep (z 1016-1032), x 968-1080, under the Sun Gate.
  The red leg enters its east end from the south; the blue leg leaves its
  west end to the north. The two right-angle turns and the hall make the
  culvert a slower crossing than skiing over the saddle.
- Red mouth: an open trench, x 1072-1080, z 296-360, behind the citadel's
  back-left corner beside the watch tower (54 m from the red flag at the
  portal). It climbs about 16 m from the portal to the ground behind the
  base (at most 22 degrees, steepest at the mouth), in the open where the tower's sentry and the
  defenders can cover it. The culvert's last 32 m into the portal may climb
  at 16 degrees so the trench stays walkable.
- Midfield shaft: an open 8 m shaft in the red leg at the dune dip
  (x 1072-1080, z 936-944), 52 m off the flag axis and 88 m short of the
  gate; about 9 m deep.
- Flank branch and shaft: a one-cell culvert east along z 392-400 from the
  red leg to an open shaft at x 1152-1160, beyond the red landing pad on the
  citadel's left flank (the pad side). There is one flank shaft per base: the
  other flank would need a branch across the citadel's front or under it.

Sizing: the culvert is 8 m wide wall to wall and 5.5 m tall inside. A body is
0.52 m in radius and a skier turns on only a 128 m radius at 60 m/s, so the
legs are straight and the turns are at the hall only. The floor follows the
dunes 8.5 m down (5.5 m of headroom plus at least 3 m of cover) under a
10-degree limit on its grade, so it rolls gently with the surface and a
skier keeps momentum along a leg. v2's legs run 656 m under two dune crests,
so the crests are rounded (CREST): a skier holding ski stays on the floor
instead of lifting into the ceiling.

Terrain: nothing is re-shaped. Every culvert cell is cut from the height
field (`holes()`), and each covered cell gets a roof that copies the terrain
surface exactly (`lid()`), so the dunes look and collide the same. The trench
and the two shafts are open to the sky; their walls rise to the terrain edge
on the grid lines.
"""
import math

import numpy as np

ASSET_ID = 'dustreach-sewer-v2'
STEP = 8.0
CX = 1024.0
N = 256
HEAD = 5.5                     # clear height inside the culvert
COVER = 3.0                    # minimum ground over the ceiling
GRADE = math.tan(math.radians(10))
# Crests round off: no floor corner stands more than CREST over the straight
# line between its neighbours, a 2-degree bend per cell at most, so a skier
# holding ski stays on the floor (under the 5.5 m ceiling) to about 65 m/s.
CREST = STEP/2*math.radians(2)
PORTAL_GRADE = math.tan(math.radians(16))  # the last 32 m into each mouth
PORTAL_CELLS = 4
SAMPLE = .5

LEG_IX = 134                   # x 1072-1080
LEG_Z = (360.0, 1016.0)        # portal to hall
TRENCH_Z = (296.0, 360.0)
HALL_IX, HALL_IZ = (121, 135), (127, 129)     # x 968-1080, z 1016-1032 (both halves)
MID_IZ = 117                   # z 936-944, the midfield shaft cell in the leg
BRANCH_IZ = 49                 # z 392-400
BRANCH_IX = (135, 145)         # x 1080-1160; the last cell is the flank shaft
FLANK_IX = 144

WALL, FLOOR, ROOF, IRON, GLOW, GRIT = 'concrete', 'rock', 'concrete', 'trim', 'light', 'soil'
BACK = .4                      # render-only backing plate offset (see _quad)
LAMP_EVERY = 16.0


# --- Cells -----------------------------------------------------------------
def red_cells():
    """{(ix, iz): kind} for the red half. Kinds: 'leg', 'hall', 'branch'
    (covered culvert), 'trench', 'mid', 'flank' (open to the sky)."""
    cells = {}
    iz0, iz1 = int(LEG_Z[0]//STEP), int(LEG_Z[1]//STEP)
    for iz in range(iz0, iz1):
        cells[(LEG_IX, iz)] = 'mid' if iz == MID_IZ else 'leg'
    for iz in range(int(TRENCH_Z[0]//STEP), int(TRENCH_Z[1]//STEP)):
        cells[(LEG_IX, iz)] = 'trench'
    for ix in range(128, HALL_IX[1]):
        for iz in range(*HALL_IZ):
            cells[(ix, iz)] = 'hall'
    for ix in range(*BRANCH_IX):
        cells[(ix, BRANCH_IZ)] = 'flank' if ix == FLANK_IX else 'branch'
    return cells


def mirror(cell):
    return (N-1-cell[0], N-1-cell[1])


def all_cells():
    red = red_cells()
    out = dict(red)
    for c, kind in red.items(): out[mirror(c)] = kind
    return out


OPEN = ('trench', 'mid', 'flank')


def holes():
    return sorted(iz*N+ix for ix, iz in all_cells())


def roofed_cells():
    return sorted(c for c, kind in all_cells().items() if kind not in OPEN)


# --- Terrain queries (the client's rendered surface) -----------------------
def quantize(grid):
    """Heights exactly as the client reads height.bin (u16, 1/32 m)."""
    return np.round(np.asarray(grid, np.float64)*32)/32


def surface(q, x, z):
    """Rendered terrain height, with the client's alternating diagonals."""
    fx, fz = x/STEP, z/STEP
    ix, iz = min(int(fx), N-2), min(int(fz), N-2)
    tx, tz = fx-ix, fz-iz
    a, b, c, d = q[iz, ix], q[iz, ix+1], q[iz+1, ix], q[iz+1, ix+1]
    if (ix ^ iz) & 1 == 0:
        return float(a+(d-c)*tx+(c-a)*tz) if tx < tz else float(a+(b-a)*tx+(d-b)*tz)
    return float(a+(b-a)*tx+(c-a)*tz) if tx+tz <= 1 else float(d+(c-d)*(1-tx)+(b-d)*(1-tz))


def edge(q, x, z):
    """Terrain height on a grid line: exact linear interpolation along it."""
    fx, fz = x/STEP, z/STEP
    ix, iz = int(math.floor(fx+1e-9)), int(math.floor(fz+1e-9))
    tx, tz = fx-ix, fz-iz
    if tx < 1e-9: return float(q[iz, ix]+(q[iz+1, ix]-q[iz, ix])*tz) if tz > 1e-9 else float(q[iz, ix])
    if tz < 1e-9: return float(q[iz, ix]+(q[iz, ix+1]-q[iz, ix])*tx)
    raise ValueError('edge() needs a point on a grid line')


def _lowest(q, x0, x1, z0, z1):
    return min(surface(q, x, z) for x in np.arange(x0, x1+1e-6, SAMPLE) for z in np.arange(z0, z1+1e-6, SAMPLE))


# --- Floor profile ------------------------------------------------------------
def _solve(values, edges, groups, crests=()):
    """Largest floor under `values` (corner requirements) with every
    neighbouring pair of corners within its edge's grade over one cell, every
    group of corners level, and every crest corner (a, b, c) at most CREST
    over the line from a to c. Values only ever decrease, so this
    converges."""
    v = dict(values)
    for _ in range(20000):
        changed = False
        for a, b, grade in edges:
            for s, t in ((a, b), (b, a)):
                lim = v[t]+grade*STEP
                if v[s] > lim+1e-12: v[s] = lim; changed = True
        for a, b, c in crests:
            lim = (v[a]+v[c])/2+CREST
            if v[b] > lim+1e-9: v[b] = lim; changed = True
        for g in groups:
            m = min(v[k] for k in g)
            for k in g:
                if v[k] > m: v[k] = m; changed = True
        if not changed: return v
    raise RuntimeError('floor profile did not converge')


def plan(grid):
    """Red-half floor heights at cell corners. Every covered cell keeps
    COVER metres of ground over its ceiling: a corner takes the lowest
    requirement of every cell it touches, so the straight floor between two
    corners never rises above what either cell allows. The junction cell,
    both shaft cells and the hall are level."""
    q = quantize(grid)
    x0, x1 = LEG_IX*STEP, (LEG_IX+1)*STEP
    zs = np.arange(LEG_Z[0], LEG_Z[1]+.1, STEP)
    cell_req = [_lowest(q, x0, x1, z, z+STEP)-COVER-HEAD for z in zs[:-1]]
    hall_req = _lowest(q, HALL_IX[0]*STEP, HALL_IX[1]*STEP, HALL_IZ[0]*STEP, HALL_IZ[1]*STEP)-COVER-HEAD
    values = {('leg', i): min(cell_req[max(i-1, 0)], cell_req[min(i, len(cell_req)-1)]) for i in range(len(zs))}
    values['leg', len(zs)-1] = min(values['leg', len(zs)-1], hall_req)
    xs = np.arange(BRANCH_IX[0]*STEP, BRANCH_IX[1]*STEP+.1, STEP)
    bz0, bz1 = BRANCH_IZ*STEP, (BRANCH_IZ+1)*STEP
    b_req = [_lowest(q, x, x+STEP, bz0, bz1)-COVER-HEAD for x in xs[:-1]]
    values.update({('branch', i): min(b_req[max(i-1, 0)], b_req[min(i, len(b_req)-1)]) for i in range(len(xs))})
    # The culvert climbs more steeply for its last 32 m into the red mouth,
    # so the open trench behind the base stays walkable.
    edges = [(('leg', i), ('leg', i+1), PORTAL_GRADE if i < PORTAL_CELLS else GRADE) for i in range(len(zs)-1)]
    edges += [(('branch', i), ('branch', i+1), GRADE) for i in range(len(xs)-1)]
    j = int((BRANCH_IZ*STEP-LEG_Z[0])//STEP)
    m = int((MID_IZ*STEP-LEG_Z[0])//STEP)
    groups = [[('leg', j), ('leg', j+1), ('branch', 0)],          # the branch leaves a level junction
              [('leg', m), ('leg', m+1)],                          # the midfield shaft cell
              [('branch', len(xs)-2), ('branch', len(xs)-1)]]      # the flank shaft cell
    crests = [(('leg', i-1), ('leg', i), ('leg', i+1)) for i in range(PORTAL_CELLS+1, len(zs)-1)]
    crests += [(('branch', i-1), ('branch', i), ('branch', i+1)) for i in range(1, len(xs)-1)]
    v = _solve(values, edges, groups, crests)
    leg = np.array([v['leg', i] for i in range(len(zs))])
    branch = np.array([v['branch', i] for i in range(len(xs))])
    return {'leg_z': zs, 'leg': leg, 'branch_x': xs, 'branch': branch, 'hall': float(leg[-1]), 'q': q}


def _shape(t):
    """Trench climb from the portal (t 0) to the mouth (t 1): gentle at the
    portal, steepest at the mouth, so the floor cuts into ground that falls
    toward the base there (the mouth's floor never rises above the dunes)."""
    t = min(max(t, 0.0), 1.0)
    return .4*t+.6*t*t


def trench_floor(p, x, z):
    """Red trench floor: level with the leg at the portal, meeting the
    terrain on the trench's outer grid row exactly (no step, no gap)."""
    z_out, z_in = TRENCH_Z
    lp = float(p['leg'][0])
    s = _shape((z_in-z)/(z_in-z_out))
    return lp+(edge(p['q'], x, z_out)-lp)*s


# --- Geometry -------------------------------------------------------------------
def _quad(mesh, a, b, c, d, mat, solid=True, back=None):
    """Kit quad without zero-area halves (they would bake to NaN texels).
    `back` offsets a render-only backing plate towards the surface's hidden
    side: the cut cells have no terrain behind them and the bake probes both
    sides of every surface, so a lone plate there bakes wrong from behind."""
    tris = [tri for tri in ((a, b, c), (a, c, d))
            if np.linalg.norm(np.cross(np.subtract(tri[1], tri[0]), np.subtract(tri[2], tri[0]))) > 1e-6]
    for tri in tris: mesh.triangle(list(tri), mat, solid)
    if back is not None:
        for tri in tris: mesh.triangle([tuple(np.add(pt, back)) for pt in tri], GRIT, False)


def _corner_floor(p, x, z, kind):
    if kind == 'hall': return p['hall']
    if kind in ('branch', 'flank'): return float(np.interp(x, p['branch_x'], p['branch']))
    if kind == 'trench': return trench_floor(p, x, z)
    return float(np.interp(z, p['leg_z'], p['leg']))


def _edge_kind(cells, cell, neighbour):
    """How a red-half cell's side to `neighbour` is closed: None (open to the
    next culvert cell), 'wall' (culvert wall floor to ceiling), 'full' (an
    open cell's wall from its floor up to the terrain edge) or 'upper' (from
    the covered cell's ceiling up to the terrain edge, over an opening)."""
    kind, other = cells[cell], cells.get(neighbour)
    if other is None: return 'full' if kind in OPEN else 'wall'
    if (kind in OPEN) == (other in OPEN): return None
    return 'upper' if kind in OPEN else None


def _half(mesh, p, lamps):
    q = p['q']
    cells = all_cells()
    red = red_cells()
    for (ix, iz), kind in sorted(red.items()):
        x0, x1, z0, z1 = ix*STEP, (ix+1)*STEP, iz*STEP, (iz+1)*STEP
        f = {(x, z): _corner_floor(p, x, z, kind) for x in (x0, x1) for z in (z0, z1)}
        # Floor, and the ceiling over covered cells.
        _quad(mesh, (x0, f[x0, z0], z0), (x0, f[x0, z1], z1), (x1, f[x1, z1], z1), (x1, f[x1, z0], z0),
              FLOOR, back=(0.0, -BACK, 0.0))
        if kind not in OPEN:
            _quad(mesh, (x0, f[x0, z0]+HEAD, z0), (x1, f[x1, z0]+HEAD, z0), (x1, f[x1, z1]+HEAD, z1),
                  (x0, f[x0, z1]+HEAD, z1), ROOF, back=(0.0, BACK, 0.0))
        # Sides: west, east (constant x), south, north (constant z).
        for (ax, az, bx, bz), neighbour, out in (((x0, z0, x0, z1), (ix-1, iz), (-1, 0)),
                                                 ((x1, z0, x1, z1), (ix+1, iz), (1, 0)),
                                                 ((x0, z0, x1, z0), (ix, iz-1), (0, -1)),
                                                 ((x0, z1, x1, z1), (ix, iz+1), (0, 1))):
            how = _edge_kind(cells, (ix, iz), neighbour)
            if how is None: continue
            back = (out[0]*BACK, 0.0, out[1]*BACK)
            fa, fb = f[ax, az], f[bx, bz]
            if how == 'wall':
                _quad(mesh, (ax, fa, az), (bx, fb, bz), (bx, fb+HEAD, bz), (ax, fa+HEAD, az), WALL, back=back)
            else:
                ta, tb = edge(q, ax, az), edge(q, bx, bz)
                if how == 'full':
                    # Trench and shaft walls stand a metre lower behind, so
                    # their backs stay under the ground where it falls away.
                    back = (out[0]*BACK, -1.0, out[1]*BACK)
                    _quad(mesh, (ax, fa, az), (bx, fb, bz), (bx, tb, bz), (ax, ta, az), WALL, back=back)
                else:
                    # Over the opening into the covered culvert: from its
                    # ceiling (floors meet level at the shared edge) up to
                    # the ground. The hidden side is the void over that ceiling.
                    _quad(mesh, (ax, fa+HEAD, az), (bx, fb+HEAD, bz), (bx, tb, bz), (ax, ta, az), WALL,
                          back=(out[0]*BACK, -1.0, out[1]*BACK))
        _dress(mesh, p, (ix, iz), kind, f, lamps)


def _dress(mesh, p, cell, kind, f, lamps):
    """Render-only dressing within 0.52 m of a solid surface: a dark drainage
    channel down the floor, a grit course along the wall feet, and in covered
    cells a lamp strip and stone ribs every 16 m."""
    ix, iz = cell
    if kind == 'trench': return      # its floor curves across the width; a flat strip would show through
    x0, x1, z0, z1 = ix*STEP, (ix+1)*STEP, iz*STEP, (iz+1)*STEP
    along_x = kind in ('hall', 'branch', 'flank')
    cxm, czm = (x0+x1)/2, (z0+z1)/2
    fm = sum(f.values())/4
    if along_x:
        a, b = (f[x0, z0]+f[x0, z1])/2, (f[x1, z0]+f[x1, z1])/2
        if kind != 'hall' or iz == HALL_IZ[0]:
            zc = z1 if kind == 'hall' else czm
            mesh.quad((x0, a+.02, zc-.6), (x0, a+.02, zc+.6), (x1, b+.02, zc+.6), (x1, b+.02, zc-.6), IRON, False)
    else:
        a, b = (f[x0, z0]+f[x1, z0])/2, (f[x0, z1]+f[x1, z1])/2
        mesh.quad((cxm-.6, a+.02, z0), (cxm-.6, b+.02, z1), (cxm+.6, b+.02, z1), (cxm+.6, a+.02, z0), IRON, False)
    if kind in OPEN:
        return
    top = fm+HEAD
    on_grid = (cxm if along_x else czm)-STEP/2
    if abs((on_grid % LAMP_EVERY)) < 1e-6:
        if along_x:
            mesh.box((cxm, top-.06, czm), (5.0, .1, .7), GLOW, False)
        else:
            mesh.box((cxm, top-.06, czm), (.7, .1, 5.0), GLOW, False)
        lamps.append(((cxm, top-1.2, czm), .8))
        # Stone ribs on the cell boundary behind the lamp.
        if along_x:
            x = x0
            for zz, s in ((z0, 1), (z1, -1)):
                if kind == 'hall' and (zz == HALL_IZ[0]*STEP+STEP): continue
                mesh.box((x, fm+HEAD/2, zz+s*.14), (.5, HEAD, .28), WALL, False)
        else:
            z = z0
            for xx, s in ((x0, 1), (x1, -1)):
                mesh.box((xx+s*.14, fm+HEAD/2, z), (.28, HEAD, .5), WALL, False)


def _shaft_dressing(mesh, p, cell, lamps, rng_seed):
    """Around an open shaft or the trench top: a flush stone kerb, a grate
    leaf thrown open beside the hole, iron rungs down one wall, and four
    short solid posts with lamp caps marking the corners."""
    q = p['q']
    ix, iz = cell
    x0, x1, z0, z1 = ix*STEP, (ix+1)*STEP, iz*STEP, (iz+1)*STEP
    for (ax, az), (bx, bz), (ox, oz) in ((((x0, z0), (x1, z0), (0, -1))), (((x1, z0), (x1, z1), (1, 0))),
                                         (((x1, z1), (x0, z1), (0, 1))), (((x0, z1), (x0, z0), (-1, 0)))):
        ta, tb = edge(q, ax, az), edge(q, bx, bz)
        oa = (ax+ox*.5, surface(q, ax+ox*.5, az+oz*.5)+.08, az+oz*.5)
        ob = (bx+ox*.5, surface(q, bx+ox*.5, bz+oz*.5)+.08, bz+oz*.5)
        mesh.quad((ax, ta+.08, az), oa, ob, (bx, tb+.08, bz), WALL, False)
    for cx, cz in ((x0-.5, z0-.5), (x1+.5, z0-.5), (x1+.5, z1+.5), (x0-.5, z1+.5)):
        g = surface(q, cx, cz)
        mesh.box((cx, g+.45, cz), (.34, 1.2, .34), IRON, True)
        mesh.box((cx, g+1.12, cz), (.26, .16, .26), GLOW, False)
    lamps.append((((x0+x1)/2, surface(q, x0-1, z0-1)+2.0, (z0+z1)/2), .5))
    # The grate leaf lies open on the ground beside the hole's -x side.
    gx0, gx1, gz0, gz1 = x0-4.2, x0-.8, z0+2.0, z0+5.4
    corners = [(gx0, gz0), (gx0, gz1), (gx1, gz1), (gx1, gz0)]
    pts = [(x, surface(q, x, z)+.06, z) for x, z in corners]
    mesh.quad(*pts, IRON, False)
    for k in range(1, 5):
        z = gz0+(gz1-gz0)*k/5
        mesh.quad((gx0, surface(q, gx0, z)+.09, z-.08), (gx0, surface(q, gx0, z)+.09, z+.08),
                  (gx1, surface(q, gx1, z)+.09, z+.08), (gx1, surface(q, gx1, z)+.09, z-.08), 'grate', False)
    # Rungs down the +x wall, from the lip to the culvert ceiling.
    f = _corner_floor(p, x1, (z0+z1)/2, red_cells()[cell])
    t = edge(q, x1, z0)
    y = t-.6
    while y > f+.8:
        mesh.box((x1-.12, y, (z0+z1)/2), (.2, .06, 1.2), IRON, False)
        y -= .9


def build(mesh, grid):
    """Add both halves of the sewer. `grid` is the terrain height grid
    (row = z cell). Returns (anchors in world coordinates, bake lamps)."""
    p = plan(grid)
    check_cover(p)
    red_lamps = []
    base_origin, base_yaw = mesh.origin, mesh.yaw
    mesh.origin, mesh.yaw = (0.0, 0.0, 0.0), 0.0
    _half(mesh, p, red_lamps)
    for cell in [c for c, k in red_cells().items() if k in ('mid', 'flank')]:
        _shaft_dressing(mesh, p, cell, red_lamps, 0)
    _portal_dressing(mesh, p, red_lamps)
    mesh.origin, mesh.yaw = (2*CX, 0.0, 2*CX), math.pi
    _half(mesh, p, [])
    for cell in [c for c, k in red_cells().items() if k in ('mid', 'flank')]:
        _shaft_dressing(mesh, p, cell, [], 0)
    _portal_dressing(mesh, p, [])
    mesh.origin, mesh.yaw = base_origin, base_yaw
    lamps = []
    for pos, power in red_lamps:
        lamps.append((pos, power))
        lamps.append(((2*CX-pos[0], pos[1], 2*CX-pos[2]), power))
    return anchors(p), lamps


def _portal_dressing(mesh, p, lamps):
    """A stone frame round the red portal and a lamp over it; the grille
    gate stands folded open against the trench's west wall."""
    q = p['q']
    x0, x1, z = LEG_IX*STEP, (LEG_IX+1)*STEP, LEG_Z[0]
    fl = float(p['leg'][0])
    for x in (x0+.25, x1-.25):
        mesh.box((x, fl+HEAD/2, z-.2), (.5, HEAD, .4), WALL, False)
    mesh.box(((x0+x1)/2, fl+HEAD+.3, z-.2), (STEP, .6, .4), WALL, False)
    mesh.box(((x0+x1)/2, fl+HEAD-.06, z-.45), (3.0, .1, .5), GLOW, False)   # hung under the lintel, not flush with it
    lamps.append((((x0+x1)/2, fl+HEAD-1.0, z-1.5), .8))
    for k in range(6):
        zz = z-1.4-k*.5
        mesh.box((x0+.1, fl+2.2, zz), (.12, 4.0, .08), IRON, False)
    mesh.box((x0+.1, fl+4.2, z-2.65), (.12, .1, 2.6), IRON, False)
    mesh.box((x0+.1, fl+.3, z-2.65), (.12, .1, 2.6), IRON, False)


def anchors(p):
    """World-space anchors for both halves (blue = 180-degree rotation)."""
    red = {
        'mouth_red': ((LEG_IX+.5)*STEP, float(p['leg'][0]), LEG_Z[0]),
        'mouth_red_top': ((LEG_IX+.5)*STEP, float(edge(p['q'], (LEG_IX+.5)*STEP, TRENCH_Z[0])), TRENCH_Z[0]),
        'shaft_mid_red': ((LEG_IX+.5)*STEP, float(np.interp((MID_IZ+.5)*STEP, p['leg_z'], p['leg'])), (MID_IZ+.5)*STEP),
        'shaft_flank_red': ((FLANK_IX+.5)*STEP, float(p['branch'][-1]), (BRANCH_IZ+.5)*STEP),
    }
    out = {'centre': (CX, p['hall'], CX)}
    for name, (x, y, z) in red.items():
        out[name] = (x, y, z)
        out[name.replace('red', 'blue')] = (2*CX-x, y, 2*CX-z)
    return out


def check_cover(p, minimum=COVER):
    """The ground must stay at least `minimum` metres over every covered
    cell's ceiling (sampled every half metre)."""
    q = p['q']
    for (ix, iz), kind in red_cells().items():
        if kind in OPEN: continue
        x0, z0 = ix*STEP, iz*STEP
        for x in np.arange(x0, x0+STEP+1e-6, SAMPLE):
            for z in np.arange(z0, z0+STEP+1e-6, SAMPLE):
                # Bilinear over the cell's four floor corners (the ceiling is planar per triangle;
                # the corners bound both triangles from above).
                top = max(_corner_floor(p, xx, zz, kind) for xx in (x0, x0+STEP) for zz in (z0, z0+STEP))+HEAD
                if surface(q, x, z)-top < minimum-1e-6:
                    raise ValueError(f'culvert roof too thin at ({x}, {z}): {surface(q, x, z)-top:.2f} m')


def lid(grid):
    """Terrain copy over every covered cut cell: (render vertex floats in the
    client's terrain encoding, collision floats). The renderer skips cut
    cells, so these triangles stand in for them exactly."""
    q = quantize(grid)
    def vertex(ix, iz):
        x, z = ix*STEP, iz*STEP
        n = np.array([(q[iz, ix-1]-q[iz, ix+1])/2, 2*4.0, (q[iz-1, ix]-q[iz+1, ix])/2])
        n = n/np.linalg.norm(n)
        return [x, float(q[iz, ix]), z, *map(float, n), x/8, z/8, (x/STEP+.5)/256, (z/STEP+.5)/256, 0.0, -2.0]
    render, collision = [], []
    for ix, iz in roofed_cells():
        i, r, i1, r1 = (ix, iz), (ix, iz+1), (ix+1, iz), (ix+1, iz+1)
        tris = [(i, r, r1), (i, r1, i1)] if (ix ^ iz) & 1 == 0 else [(i, r, i1), (i1, r, r1)]
        for tri in tris:
            for cx, cz in tri:
                v = vertex(cx, cz)
                render.extend(v); collision.extend(v[:3])
    return render, collision
