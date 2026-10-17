"""Original Frostline landmark: an ice cavern through the central beacon ridge.

A second lane over the middle of the map. Skiers either climb the ridge and
cross at the beacon, or cut through the ridge underneath it. The cavern links
neither base; both mouths open onto the snowfields between the ridge and the
valleys, so capping still means skiing the open slopes.

Layout, in world coordinates. The map is point-symmetric about (1024, 1024),
and so is everything here:

- The cavern runs straight along the flag axis, between portal faces at
  z = 976 and z = 1072 (d = 48 m either side of the centre). A skier turns at
  only v^2 / 28 m/s^2 (a 128 m radius at 60 m/s), so any bend would put them
  into the wall; straight along the axis is the only straight line that keeps
  the point symmetry and grid-aligned mouths.
- Inside, it is 24 m wide, with vertical ice walls 8 m tall and a vault rising
  5 m more to a 13 m crown. That leaves room for three skiers abreast (a
  0.52 m body), for a jet hop over another player, and for disc arcs.
- The floor dips from 219 m at the portals to 214 m in the middle on a cosine,
  at most about 6 degrees, so a skier rolls in and carries through without
  a flat dead zone.
- An open trench, 32 m wide and 32 m long, runs from each portal out to where
  the slope meets the floor height (about 220 m). Its granite walls rise to
  the ground at both edges.

Terrain: nothing is re-shaped. The cavern and trench cells are cut from the
height field (`holes()`), and a roof copies the terrain surface of every cut
cell over the cavern exactly: the same quantized heights, triangle diagonals,
normals, UVs and splat weights as the client's terrain mesh (`lid()`). So the
ridge still looks and collides like ridge, and the slope statistics do not
change at all.
"""
import math

import numpy as np

from assets.structure_kit import Builder

ASSET_ID = 'frostline-cavern-v2'
SNOW_CAP, WALL_CAP, CORNICE = 5.0, 3.0, 1.0   # snow band heights (m) and cornice overhang
STEP = 8.0
CX = 1024.0                 # axis and centre
CUT_X = (1008.0, 1040.0)    # cut cells across the axis (4 cells)
HALF_W = 12.0               # cavern half width (walls at 1012 / 1036)
PORTAL_D, TRENCH_L = 48.0, 32.0
FLOOR_MOUTH, FLOOR_MID = 219.0, 214.0
WALL_H, RISE = 8.0, 5.0     # wall height, vault rise above it
VAULT_SEGMENTS = 6
BACK = .4                   # render-only backing plate offset (see _quad)
ICE, FLOOR, GRANITE, GLOW = 'soil', 'meadow', 'rock', 'light'
LAMP_Z = (-44.0, -32.0, -20.0, -8.0)   # d-signed positions of crown lamps, red half


def floor_at(d):
    """Cavern floor height at distance d = |z-1024| from the centre."""
    d = min(abs(d), PORTAL_D)
    return FLOOR_MOUTH-(FLOOR_MOUTH-FLOOR_MID)*(1+math.cos(math.pi*d/PORTAL_D))/2


def vault(dx):
    """Height above the floor of the cavern's inner surface at |x-1024| = dx."""
    dx = min(abs(dx), HALF_W)
    return WALL_H+RISE*math.sqrt(max(0.0, 1-(dx/HALF_W)**2))


def _vault_xs():
    """Vault sample offsets from the axis: elliptical arc, spring to crown."""
    out = []
    for i in range(VAULT_SEGMENTS+1):
        a = math.pi*i/VAULT_SEGMENTS
        out.append(-HALF_W*math.cos(a))
    return out


def cells():
    """(ix, iz) of every cut cell: the cavern (roofed by `lid`) and both
    trenches (open to the sky)."""
    ix0, ix1 = int(CUT_X[0]//STEP), int(CUT_X[1]//STEP)
    iz0 = int((CX-PORTAL_D-TRENCH_L)//STEP); iz1 = int((CX+PORTAL_D+TRENCH_L)//STEP)
    return [(ix, iz) for iz in range(iz0, iz1) for ix in range(ix0, ix1)]


def roofed(ix, iz):
    return CX-PORTAL_D <= iz*STEP < CX+PORTAL_D


def holes():
    return sorted(iz*256+ix for ix, iz in cells())


def quantize(grid):
    """Heights exactly as the client reads height.bin (u16, 1/32 m)."""
    return np.round(np.asarray(grid, np.float64)*32)/32


def edge(q, x, z):
    """Terrain height on a grid line (x or z on a multiple of 8): exact
    linear interpolation along the cell edge."""
    fx, fz = x/STEP, z/STEP
    ix, iz = int(math.floor(fx)), int(math.floor(fz))
    tx, tz = fx-ix, fz-iz
    if tx < 1e-9: return float(q[iz, ix]+(q[iz+1, ix]-q[iz, ix])*tz) if tz > 1e-9 else float(q[iz, ix])
    if tz < 1e-9: return float(q[iz, ix]+(q[iz, ix+1]-q[iz, ix])*tx)
    raise ValueError('edge() needs a point on a grid line')


def _trench_floor(q, x, z):
    """Red trench floor: flat at the portal, meeting the terrain row at the
    trench's outer end exactly (so there is no step or gap)."""
    z_out, z_in = CX-PORTAL_D-TRENCH_L, CX-PORTAL_D
    t = (z_in-z)/TRENCH_L                       # 0 at the portal, 1 at the outer end
    s = t*t*(3-2*t)
    return FLOOR_MOUTH+(edge(q, x, z_out)-FLOOR_MOUTH)*s


def _quad(mesh, a, b, c, d, mat, solid=True, back=None):
    """Kit quad without zero-area halves (they would bake to NaN texels).

    `back` is an offset towards the surface's hidden side. The cut cells have
    no terrain behind them and the bake probes both sides of every surface,
    so a lone plate there can look more open from behind and bake dark. A
    render-only backing plate at that offset closes the hidden side."""
    tris = [tri for tri in ((a, b, c), (a, c, d))
            if np.linalg.norm(np.cross(np.subtract(tri[1], tri[0]), np.subtract(tri[2], tri[0]))) > 1e-6]
    # Fronts first, then backs: consecutive halves of a planar quad share one
    # lightmap chart, which interleaving would break.
    for tri in tris: mesh.triangle(list(tri), mat, solid)
    if back is not None:
        for tri in tris: mesh.triangle([tuple(np.add(p, back)) for p in tri], GRANITE, False)


def _half(mesh, q):
    """Red half, in red world coordinates: the trench, the portal and the
    cavern from the portal to the centre line."""
    b = Builder(mesh, 'glacier', interior=ICE, metal='trim', glow=GLOW)
    x0, x1 = CUT_X
    z_out, z_portal = CX-PORTAL_D-TRENCH_L, CX-PORTAL_D
    grid_xs = [x0+i*STEP for i in range(int((x1-x0)/STEP)+1)]
    tz = [z_out+i*STEP for i in range(int(TRENCH_L/STEP)+1)]
    # Trench floor (grid-aligned quads) and granite side walls up to the ground.
    # Trench-wall backs sit a metre lower so they stay under the ground beside
    # the trench where it falls away from the edge.
    down, out = (0.0, -BACK, 0.0), {x0: (-BACK, -1.0, 0.0), x1: (BACK, -1.0, 0.0)}
    for za, zb in zip(tz, tz[1:]):
        for xa, xb in zip(grid_xs, grid_xs[1:]):
            _quad(mesh, (xa, _trench_floor(q, xa, za), za), (xb, _trench_floor(q, xb, za), za),
                  (xb, _trench_floor(q, xb, zb), zb), (xa, _trench_floor(q, xa, zb), zb), FLOOR, back=down)
        for x in (x0, x1):
            fa, fb = _trench_floor(q, x, za), _trench_floor(q, x, zb)
            ta, tb = edge(q, x, za), edge(q, x, zb)
            # Granite below a snow band at the top, so the cut reads as a
            # snow-capped bank rather than a bare grey slab from afar.
            sa, sb = max(fa, ta-WALL_CAP), max(fb, tb-WALL_CAP)
            _quad(mesh, (x, fa, za), (x, fb, zb), (x, sb, zb), (x, sa, za), GRANITE, back=out[x])
            if ta-sa > .05 or tb-sb > .05:
                _quad(mesh, (x, sa, za), (x, sb, zb), (x, tb, zb), (x, ta, za), FLOOR, back=out[x])
    # Portal face: granite from the floor (beside the opening) or the vault
    # (over it) up to the terrain edge on the z = 976 grid row.
    xs = sorted(set(grid_xs) | {CX+dx for dx in _vault_xs()})
    for xa, xb in zip(xs, xs[1:]):
        over_opening = abs((xa+xb)/2-CX) < HALF_W
        def bottom(x):
            return FLOOR_MOUTH+vault(x-CX) if over_opening else FLOOR_MOUTH
        ta, tb = edge(q, xa, z_portal), edge(q, xb, z_portal)
        sa, sb = max(bottom(xa), ta-SNOW_CAP), max(bottom(xb), tb-SNOW_CAP)
        _quad(mesh, (xa, bottom(xa), z_portal), (xb, bottom(xb), z_portal),
              (xb, sb, z_portal), (xa, sa, z_portal), GRANITE, back=(0.0, -1.0, BACK))
        if ta-sa > .05 or tb-sb > .05:
            _quad(mesh, (xa, sa, z_portal), (xb, sb, z_portal), (xb, tb, z_portal), (xa, ta, z_portal), FLOOR,
                  back=(0.0, -1.0, BACK))
        # Snow cornice: a render-only lip drooping out over the face's top edge.
        lip = [(xa, ta, z_portal), (xb, tb, z_portal), (xb, tb-.4, z_portal-CORNICE), (xa, ta-.4, z_portal-CORNICE)]
        mesh.quad(*lip, FLOOR, False)
        mesh.quad(*lip[::-1], FLOOR, False)
    # Cavern: floor, ice walls and vault, sampled every 8 m along z.
    cz = [z_portal+i*STEP for i in range(int(PORTAL_D/STEP)+1)]
    vx = [CX+dx for dx in _vault_xs()]
    for za, zb in zip(cz, cz[1:]):
        fa, fb = floor_at(za-CX), floor_at(zb-CX)
        _quad(mesh, (CX-HALF_W, fa, za), (CX+HALF_W, fa, za), (CX+HALF_W, fb, zb), (CX-HALF_W, fb, zb), FLOOR, back=down)
        for x, s in ((CX-HALF_W, -1), (CX+HALF_W, 1)):
            _quad(mesh, (x, fa, za), (x, fb, zb), (x, fb+WALL_H, zb), (x, fa+WALL_H, za), ICE, back=(s*BACK, 0.0, 0.0))
        for xa, xb in zip(vx, vx[1:]):
            ya, yb = vault(xa-CX), vault(xb-CX)
            lean = ((xa+xb)/2-CX)/HALF_W
            _quad(mesh, (xa, fa+ya, za), (xb, fa+yb, za), (xb, fb+yb, zb), (xa, fb+ya, zb), ICE,
                  back=(BACK*lean, BACK, 0.0))
    # Dressing: a hazard-free granite sill under each wall, ice-glow strips
    # along the crown with bake lamps, and icicles hanging from the vault.
    for x, s in ((CX-HALF_W, 1), (CX+HALF_W, -1)):
        for za, zb in zip(cz, cz[1:]):
            fa, fb = floor_at(za-CX), floor_at(zb-CX)
            mesh.quad((x+s*.03, fa, za), (x+s*.03, fb, zb), (x+s*.03, fb+.6, zb), (x+s*.03, fa+.6, za), GRANITE, False)
    for dz in LAMP_Z:
        z = CX+dz; f = floor_at(dz)
        mesh.box((CX, f+WALL_H+RISE-.08, z), (1.2, .12, 5.0), GLOW, False)
        b.lamp((CX, f+WALL_H+RISE-1.2, z), .9)
        for s in (-1, 1):
            mesh.box((CX+s*(HALF_W-.1), f+WALL_H-.4, z), (.12, .5, 3.0), GLOW, False)
            b.lamp((CX+s*(HALF_W-1.5), f+WALL_H-1.0, z), .45)
    b.lamp((CX, FLOOR_MOUTH+WALL_H, z_portal+2.0), .6)
    for dx, dz, length in ((-7.5, -38.0, 2.2), (5.0, -27.0, 1.6), (-3.0, -15.0, 2.6), (8.5, -6.0, 1.8)):
        x, z = CX+dx, CX+dz
        top = floor_at(dz)+vault(dx)
        b.prism(x, z, .02, .35, top-length, top+.05, 5, ICE, solid=False, cap=False)
    # Cover: one ice boulder per half, beside the ski line.
    bx, bz = CX-7.5, CX-20.0
    f = floor_at(bz-CX)
    b.prism(bx, bz, 2.4, 1.6, f-.6, f+2.4, 8, ICE, phase=math.pi/8)


def build(mesh, grid):
    """Add both halves of the cavern and its trenches. `grid` is the terrain
    height grid (row = z cell). Returns anchors in world coordinates."""
    q = quantize(grid)
    _check_cover(q)
    for origin, yaw in (((0.0, 0.0, 0.0), 0.0), ((2*CX, 0.0, 2*CX), math.pi)):
        mesh.origin, mesh.yaw = origin, yaw
        _half(mesh, q)
    mesh.origin, mesh.yaw = (0.0, 0.0, 0.0), 0.0
    return {'mouth_red': (CX, FLOOR_MOUTH, CX-PORTAL_D), 'mouth_blue': (CX, FLOOR_MOUTH, CX+PORTAL_D),
            'centre': (CX, FLOOR_MID, CX)}


def _check_cover(q, minimum=3.0):
    """The ridge must stay at least `minimum` metres above the vault
    everywhere over the cavern, and above the wall tops beside it."""
    for z in np.arange(CX-PORTAL_D, CX+PORTAL_D+.01, 1.0):
        for x in np.arange(CUT_X[0], CUT_X[1]+.01, 1.0):
            dx = x-CX
            top = floor_at(z-CX)+(vault(dx) if abs(dx) <= HALF_W else WALL_H+RISE)
            ground = surface(q, x, z)
            if ground-top < minimum:
                raise ValueError(f'cavern roof too thin at ({x}, {z}): {ground-top:.2f} m')


def surface(q, x, z):
    """Rendered terrain height, with the client's alternating diagonals."""
    fx, fz = x/STEP, z/STEP
    ix, iz = min(int(fx), 254), min(int(fz), 254)
    tx, tz = fx-ix, fz-iz
    a, b, c, d = q[iz, ix], q[iz, ix+1], q[iz+1, ix], q[iz+1, ix+1]
    if (ix ^ iz) & 1 == 0:
        return float(a+(d-c)*tx+(c-a)*tz) if tx < tz else float(a+(b-a)*tx+(d-b)*tz)
    return float(a+(b-a)*tx+(c-a)*tz) if tx+tz <= 1 else float(d+(c-d)*(1-tx)+(b-d)*(1-tz))


def lid(grid):
    """Terrain copy over every roofed cut cell: (render vertex floats in the
    client's terrain encoding, collision floats). The renderer skips cut
    cells, so these triangles stand in for them exactly."""
    q = quantize(grid)
    def vertex(ix, iz):
        x, z = ix*STEP, iz*STEP
        n = np.array([(q[iz, ix-1]-q[iz, ix+1])/2, 2*4.0, (q[iz-1, ix]-q[iz+1, ix])/2])
        n = n/np.linalg.norm(n)
        return [x, float(q[iz, ix]), z, *map(float, n), x/8, z/8, (x/STEP+.5)/256, (z/STEP+.5)/256, 0.0, -2.0]
    render, collision = [], []
    for ix, iz in cells():
        if not roofed(ix, iz): continue
        i, r, i1, r1 = (ix, iz), (ix, iz+1), (ix+1, iz), (ix+1, iz+1)
        tris = [(i, r, r1), (i, r1, i1)] if (ix ^ iz) & 1 == 0 else [(i, r, i1), (i1, r, r1)]
        for tri in tris:
            for cx, cz in tri:
                v = vertex(cx, cz)
                render.extend(v); collision.extend(v[:3])
    return render, collision
