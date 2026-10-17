"""Ozarktic Blast's outpost tunnels (docs/ozarktic-blast.md): from each
team's cliff-side outpost, under the upland and down the slope behind its
base, into the short tunnel between the generator room and the outbuilding.
The generator room keeps its two ways in; its tunnel door is now reached
from the outpost too.

Built in world coordinates, one 8 m terrain cell wide, in straight legs:
- the branch, out through the short tunnel's back wall (the base's local +Z);
- the row, along world X under the slope behind the base;
- the column, along world Z to the stair house on the outpost's flank.

Every cell is a terrain hole (`cells`). Over the tunnel the lid is the
ground itself: the terrain's own heights at the cell corners (`profile`
pins those vertices, raising one only where the tunnel needs more cover),
laid in rock, so the land runs on over it. The floor climbs from the base's
underground level to the stair house and never dips back down, at most
`SLOPE_DEG`: a ski run down to the base, a walk up. The stair house covers
its own cells and sits into the hillside behind the outpost, its roof over
the ground round it; only an apron at its open end is levelled.
Original geometry; no source geometry is used.
"""
import math

from assets import ozarktic_base as base
from assets.structure_kit import Builder

ASSET_ID = 'ozarktic-tunnel-v1'
STEP = 8.0
H = 5.5                        # headroom
ROOF = .6                      # ceiling slab under the lid
FILL = .3                      # least ground over the roof
COVER = H+ROOF+FILL
SLOPE_DEG = 25.0
INSET = .8                     # wall thickness: the cell edge to the tunnel's inside
BRANCH_X = (24.0, 32.0)        # the branch, base-local x (between the hall and the outbuilding)
BRANCH_Z = (8.0, 32.0)         # base-local z: from the short tunnel's back wall; the last cell turns
DOOR = (BRANCH_X[0]+INSET, BRANCH_X[1]-INSET)   # the opening in the short tunnel's back wall
COLUMN_IX = 89                 # grid column (x 712..720, just behind the outpost) of the leg to the stair house
STAIR_CELLS = 2
STAIR_NEAR = 15.0              # the stair house's open end, this far short of the outpost centre along Z
STAIR_DROP = 7.5               # tunnel floor at the stair foot, below the sill
STAIR_ROOF = 4.5               # stair house roof, over the sill (at least; it clears the ground round it)
APRON = (3.0, 6.0)             # levelled ground at the open end: beyond its sides, and out from it
LID_SUB = 4                    # the lid is drawn as LID_SUB x LID_SUB quads a cell (collision: two triangles)
HULL, DECKING, GRATE, METAL, GLOW, LID = 'concrete', 'panel', 'grate', 'trim', 'light', 'rock'


def _q(y):
    """Up to the heightfield's 1/32 m, so a pinned vertex stores exactly."""
    return math.ceil(y*32-1e-9)/32


def stair_sill(outpost_y):
    """The stair house's sill: the outpost's levelled ground."""
    return outpost_y-.1


def path(team, base_def, outpost, to_world):
    """The tunnel's cells, junction to stair house, as grid (ix, iz); the
    stair house's two cells; and the direction into the stair house along Z."""
    cells = []
    for lz in (BRANCH_Z[0]+4, BRANCH_Z[0]+12, BRANCH_Z[0]+20):
        wx, wz = to_world(base_def, sum(BRANCH_X)/2, lz)
        cells.append((int(wx//STEP), int(wz//STEP)))
    ix, iz = cells[-1]
    cells += [(x, iz) for x in range(ix-1, COLUMN_IX, -1)]+[(COLUMN_IX, iz)]
    _, ox, oy, oz, _ = outpost
    dz = 1 if oz > iz*STEP else -1
    near = oz-dz*STAIR_NEAR                      # the open end's z
    last = int((near-dz*.001)//STEP)             # the stair cell at the open end
    stair = [(COLUMN_IX, last-dz), (COLUMN_IX, last)]
    cells += [(COLUMN_IX, z) for z in range(iz+dz, stair[0][1], dz)]
    return cells, stair, dz


def cells(definition, to_world, outposts):
    """Every tunnel and stair house cell, both teams."""
    out = set()
    for b in definition['bases']:
        c, s, _ = path(b['team'], b, outposts[b['team']], to_world)
        out.update(c); out.update(s)
    return out


def _edge(a, c):
    """The grid vertices of the side shared by cells a and c."""
    if c[0] != a[0]:
        x = max(a[0], c[0]); return [(x, a[1]), (x, a[1]+1)]
    z = max(a[1], c[1]); return [(a[0], z), (a[0]+1, z)]


def _corners(c):
    ix, iz = c
    return [(ix, iz), (ix+1, iz), (ix, iz+1), (ix+1, iz+1)]


def profile(team, base_def, outpost, to_world, height):
    """The floor at every cell edge, junction to stair foot, and the pinned
    height of every lid vertex. `height(ix, iz)` is the terrain before pins.
    Returns dict(cells, stair, dz, edges, floor, corner, pins, raised)."""
    tun, stair, dz = path(team, base_def, outpost, to_world)
    junction_out = (tun[0][0]-tun[1][0], tun[0][1]-tun[1][1])     # from cell 0 back toward the short tunnel
    edges = [_edge(tun[0], (tun[0][0]+junction_out[0], tun[0][1]+junction_out[1]))]
    edges += [_edge(tun[i], tun[i+1]) for i in range(len(tun)-1)]+[_edge(tun[-1], stair[0])]
    corner = []
    s = [0.0]
    for i, c in enumerate(tun):
        a = tun[i-1] if i else (c[0]+junction_out[0], c[1]+junction_out[1])
        b = tun[i+1] if i+1 < len(tun) else stair[0]
        turn = (c[0]-a[0], c[1]-a[1]) != (b[0]-c[0], b[1]-c[1])
        corner.append(turn)
        s.append(s[-1]+(0.0 if turn else STEP))
    k = math.tan(math.radians(SLOPE_DEG))
    ub = [min(height(*v) for v in e)-COVER for e in edges]
    for i, c in enumerate(tun):
        if corner[i]:
            m = min([ub[i], ub[i+1]]+[height(*v)-COVER for v in _corners(c)])
            ub[i] = ub[i+1] = m
    f0 = base_def['position'][1]+base.GEN_FLOOR
    fe = stair_sill(outpost[2])-STAIR_DROP
    length = s[-1]
    lo = [max(f0-k*x, fe-k*(length-x)) for x in s]
    f = [max(min(u, f0+k*x, fe+k*(length-x)), l) for u, x, l in zip(ub, s, lo)]
    for i in range(1, len(f)): f[i] = min(f[i], f[i-1]+k*(s[i]-s[i-1]))
    for i in range(len(f)-2, -1, -1): f[i] = min(f[i], f[i+1]+k*(s[i+1]-s[i]))
    # Never back down toward the outpost: a ski run all the way to the base.
    for i in range(len(f)-2, -1, -1): f[i] = min(f[i], f[i+1])
    f[0], f[-1] = f0, fe
    # Pins: every lid corner at its own ground, raised where the roof needs it.
    need = {}
    for i, c in enumerate(tun):
        for e, y in ((edges[i], f[i]), (edges[i+1], f[i+1])):
            for v in e: need[v] = max(need.get(v, -1e9), y+COVER)
        if corner[i]:
            for v in _corners(c): need[v] = max(need.get(v, -1e9), f[i]+COVER)
    pins, raised = {}, 0.0
    for v, y in need.items():
        g = height(*v)
        pins[v] = _q(max(g, y))
        raised = max(raised, y-g)
    # The stair house's roof clears the ground at every corner of its cells.
    ground = max(height(*v) for c in stair for v in _corners(c))
    roof = max(stair_sill(outpost[2])+STAIR_ROOF, ground+.3)
    # The lid shades like the ground round it: each corner takes the
    # terrain's own normal there (terrain::normal_on, from its neighbours).
    normals = {}
    for (ix, iz) in pins:
        n = ((height(ix-1, iz)-height(ix+1, iz))/2, STEP, (height(ix, iz-1)-height(ix, iz+1))/2)
        length = math.sqrt(sum(c*c for c in n))
        normals[(ix, iz)] = tuple(c/length for c in n)
    return dict(cells=tun, stair=stair, dz=dz, edges=edges, floor=f, corner=corner, s=s, pins=pins, raised=raised,
                stair_roof=roof, normals=normals)


def stair_site(outpost, stair, dz):
    """(x0, x1, z0, z1, y) of the levelled apron at the stair house's open
    end, toward the outpost."""
    x0, x1 = stair[0][0]*STEP-APRON[0], (stair[0][0]+1)*STEP+APRON[0]
    zs = [c[1]*STEP for c in stair]+[(c[1]+1)*STEP for c in stair]
    open_z = max(zs) if dz > 0 else min(zs)
    z0, z1 = sorted((open_z, open_z+dz*APRON[1]))
    return (x0, x1, z0, z1, stair_sill(outpost[2]))


class _World:
    """A Builder over a mesh whose origin and yaw are cleared, so its
    coordinates are world coordinates."""
    def __init__(self, mesh, team):
        self.mesh = mesh
        self.saved = (mesh.origin, mesh.yaw)
        mesh.origin, mesh.yaw = (0.0, 0.0, 0.0), 0.0
        self.b = Builder(mesh, ['ember', 'glacier'][team], interior=HULL, metal=METAL, glow=GLOW)

    def done(self):
        self.mesh.origin, self.mesh.yaw = self.saved


def _quad_facing(mesh, pts, normal, mat, solid=True):
    """A quad wound so its normal points along `normal`."""
    a, b, c = pts[0], pts[1], pts[2]
    u = (b[0]-a[0], b[1]-a[1], b[2]-a[2]); v = (c[0]-a[0], c[1]-a[1], c[2]-a[2])
    n = (u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0])
    if sum(p*q for p, q in zip(n, normal)) < 0: pts = pts[::-1]
    mesh.quad(*pts, mat, solid)


def build(mesh, team, prof, outpost):
    """The tunnel and its stair house into `mesh`, in world coordinates.
    Returns anchors (world points)."""
    w = _World(mesh, team)
    b, m = w.b, mesh
    tun, edges, f, corner, pins = prof['cells'], prof['edges'], prof['floor'], prof['corner'], prof['pins']
    def side_runs(c, ein, eout):
        """The cell's interior box and its walled sides."""
        ix, iz = c
        x0, x1, z0, z1 = ix*STEP, (ix+1)*STEP, iz*STEP, (iz+1)*STEP
        open_sides = set()
        for e in (ein, eout):
            (ax, az), (bx, bz) = e
            if ax == bx: open_sides.add(('x', ax*STEP))
            else: open_sides.add(('z', az*STEP))
        return (x0, x1, z0, z1), open_sides

    lamps_every = 2
    for i, c in enumerate(tun):
        (x0, x1, z0, z1), open_sides = side_runs(c, edges[i], edges[i+1])
        fa, fb = f[i], f[i+1]
        # The floor runs from the entry edge to the exit edge; across, the
        # interior between the walls (or the cell side where it opens).
        ix0 = x0 if ('x', x0) in open_sides else x0+INSET
        ix1 = x1 if ('x', x1) in open_sides else x1-INSET
        iz0 = z0 if ('z', z0) in open_sides else z0+INSET
        iz1 = z1 if ('z', z1) in open_sides else z1-INSET
        def floor_at(x, z):
            if corner[i]: return fa
            (ax, az), _ = edges[i]
            if edges[i][0][0] == edges[i][1][0]:      # entry edge on an x line: runs along x
                t = abs(x-ax*STEP)/STEP
            else:
                t = abs(z-az*STEP)/STEP
            return fa+(fb-fa)*t
        corners = [(ix0, iz0), (ix1, iz0), (ix1, iz1), (ix0, iz1)]
        _quad_facing(m, [(x, floor_at(x, z), z) for x, z in corners], (0, 1, 0), DECKING)
        _quad_facing(m, [(x, floor_at(x, z)+H, z) for x, z in corners], (0, -1, 0), HULL)
        # Walls: the inner face along each closed side.
        for axis, plane, into in (('x', x0, 1), ('x', x1, -1), ('z', z0, 1), ('z', z1, -1)):
            if (axis, plane) in open_sides: continue
            face = plane+into*INSET
            if axis == 'x':
                pts = [(face, floor_at(face, iz0), iz0), (face, floor_at(face, iz1), iz1),
                       (face, floor_at(face, iz1)+H, iz1), (face, floor_at(face, iz0)+H, iz0)]
                _quad_facing(m, pts, (into, 0, 0), HULL)
            else:
                pts = [(ix0, floor_at(ix0, face), face), (ix1, floor_at(ix1, face), face),
                       (ix1, floor_at(ix1, face)+H, face), (ix0, floor_at(ix0, face)+H, face)]
                _quad_facing(m, pts, (0, 0, into), HULL)
        if corner[i]:
            # The inside corner of the turn: a pillar the width of the walls.
            (ax, az), (bx, bz) = edges[i]; (cx, cz), (dx_, dz_) = edges[i+1]
            shared = {(ax, az), (bx, bz)} & {(cx, cz), (dx_, dz_)}
            (px, pz), = shared
            px, pz = px*STEP, pz*STEP
            sx = -1 if px == x1 else 1; sz = -1 if pz == z1 else 1
            qx, qz = px+sx*INSET, pz+sz*INSET
            _quad_facing(m, [(qx, fa, pz), (qx, fa, qz), (qx, fa+H, qz), (qx, fa+H, pz)], (sx, 0, 0), HULL)
            _quad_facing(m, [(px, fa, qz), (qx, fa, qz), (qx, fa+H, qz), (px, fa+H, qz)], (0, 0, sz), HULL)
        # A lit strip down the middle of the ceiling, and a lamp every other cell.
        cxm, czm = (x0+x1)/2, (z0+z1)/2
        along_x = not corner[i] and edges[i][0][0] == edges[i][1][0]
        if along_x:
            pts = [(ix0, floor_at(ix0, czm)+H-.02, czm-.25), (ix1, floor_at(ix1, czm)+H-.02, czm-.25),
                   (ix1, floor_at(ix1, czm)+H-.02, czm+.25), (ix0, floor_at(ix0, czm)+H-.02, czm+.25)]
        else:
            pts = [(cxm-.25, floor_at(cxm, iz0)+H-.02, iz0), (cxm+.25, floor_at(cxm, iz0)+H-.02, iz0),
                   (cxm+.25, floor_at(cxm, iz1)+H-.02, iz1), (cxm-.25, floor_at(cxm, iz1)+H-.02, iz1)]
        _quad_facing(m, pts, (0, -1, 0), GLOW, False)
        if i % lamps_every == 0: b.lamp((cxm, floor_at(cxm, czm)+H-.4, czm), .9)
        _lid(m, c, pins, prof['normals'])

    # --- Stair house ----------------------------------------------------
    stair, dz = prof['stair'], prof['dz']
    sill = stair_sill(outpost[2])
    sx0, sx1 = stair[0][0]*STEP, (stair[0][0]+1)*STEP
    zs = [c[1]*STEP for c in stair]+[(c[1]+1)*STEP for c in stair]
    foot_z = min(zs) if dz > 0 else max(zs)       # meets the tunnel
    open_z = max(zs) if dz > 0 else min(zs)       # the open end, toward the outpost
    fe = f[-1]
    roof = prof['stair_roof']
    ix0, ix1 = sx0+INSET, sx1-INSET
    # The ramp up from the tunnel floor, and a landing at the sill.
    land = open_z-dz*3.0
    # (2 cm short of the walls: the ramp's edge skirts stay off their faces.)
    m.ramp((ix0+ix1)/2, ix1-ix0-.04, foot_z, land, fe, sill, GRATE)
    _quad_facing(m, [(ix0, sill, land), (ix1, sill, land), (ix1, sill, open_z), (ix0, sill, open_z)], (0, 1, 0), DECKING)
    lo_z, hi_z = min(foot_z, open_z), max(foot_z, open_z)
    # Side walls from below the tunnel floor to the roof; the back wall over
    # the tunnel's ceiling; the roof slab.
    b.wall(sx0, sx0+INSET, lo_z, hi_z, fe-1.0, roof, HULL)
    b.wall(sx1-INSET, sx1, lo_z, hi_z, fe-1.0, roof, HULL)
    hz0, hz1 = (foot_z, foot_z+dz*INSET) if dz > 0 else (foot_z-INSET, foot_z)
    b.wall(ix0, ix1, min(hz0, hz1), max(hz0, hz1), fe+H, roof, HULL)
    b.slab(sx0, sx1, lo_z, hi_z, roof+.6, HULL, .6)
    # Light strips, a lamp over the ramp, and the team's colour over the door.
    b.face_box('z', open_z, dz, sx0, sx1, roof-.1, roof+.5, .3, METAL)
    b.face_quad('z', open_z, dz, sx0+.2, sx1-.2, roof+.0, roof+.45, ['ember', 'glacier'][team], off=.32)
    for x in (ix0+.06, ix1-.06):
        _quad_facing(m, [(x, fe+.05, foot_z), (x, sill+.05, land), (x, sill+.35, land), (x, fe+.35, foot_z)], (1 if x < sx0+4 else -1, 0, 0), GLOW, False)
    b.lamp(((sx0+sx1)/2, roof-.5, (foot_z+open_z)/2), 1.0)
    w.done()
    return dict(junction=((edges[0][0][0]+edges[0][1][0])*STEP/2, f[0], (edges[0][0][1]+edges[0][1][1])*STEP/2),
                stair_door=((sx0+sx1)/2, sill, open_z), stair_foot=((sx0+sx1)/2, fe, foot_z))


def _lid(m, c, pins, normals):
    """The ground over a tunnel cell, from its pinned corners: solid as two
    triangles; drawn as a finer grid blending the corners' heights and the
    terrain's normals, so it shades like the ground round it."""
    ix, iz = c
    corner = lambda dx, dz: (pins[(ix+dx, iz+dz)], normals[(ix+dx, iz+dz)])
    (y00, n00), (y10, n10), (y01, n01), (y11, n11) = corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1)
    q = [(ix*STEP, y00, iz*STEP), (ix*STEP, y01, (iz+1)*STEP), ((ix+1)*STEP, y11, (iz+1)*STEP), ((ix+1)*STEP, y10, iz*STEP)]
    for tri in (_up(q[0], q[1], q[2]), _up(q[0], q[2], q[3])):
        m.collision.extend(v for p in tri for v in p)
    def at(u, v):
        y = y00*(1-u)*(1-v)+y10*u*(1-v)+y01*(1-u)*v+y11*u*v
        n = [a*(1-u)*(1-v)+b*u*(1-v)+c_*(1-u)*v+d*u*v for a, b, c_, d in zip(n00, n10, n01, n11)]
        length = math.sqrt(sum(x*x for x in n))
        return ((ix+u)*STEP, y, (iz+v)*STEP), tuple(x/length for x in n)
    k = LID_SUB
    grid = [[at(i/k, j/k) for j in range(k+1)] for i in range(k+1)]
    for i in range(k):
        for j in range(k):
            for tri in ((grid[i][j], grid[i][j+1], grid[i+1][j+1]), (grid[i][j], grid[i+1][j+1], grid[i+1][j])):
                pts = _up(*(p for p, _ in tri))
                m.triangle(pts, LID, False)
                for slot, p in enumerate(pts):
                    n = next(nn for pp, nn in tri if pp == p)
                    start = len(m.vertices)-12*(3-slot)+3
                    m.vertices[start:start+3] = type(m.vertices)('f', n)


def _up(a, b, c):
    """Three points wound so their normal points up."""
    u = (b[0]-a[0], b[2]-a[2]); v = (c[0]-a[0], c[2]-a[2])
    return [a, b, c] if u[1]*v[0]-u[0]*v[1] > 0 else [a, c, b]
