"""Ashveil's cathedral: an octagonal crypt dug under the temple
(docs/ashveil.md), our own design.

Under the temple's floor lies an octagon 37 m across, a nave sunk 11 m
under a gallery that runs right round it at the lava tubes' level, and over
it all a ribbed dome rising to the well's ring. The two lava tubes come in
through tall doors in the west and east walls, onto the gallery; two vents
drop straight from the temple's floor near its northwest and southeast
faces and open onto the gallery through low doors; the well drops from the
middle of the temple's floor onto the post. From the gallery two stairs
turn down round the walls, one from the west door along the northwest and
north walls, one from the east door along the southeast and south walls.

Below the gallery four chapels open off the nave, on the west, southwest,
east and northeast: tombs in the west and east ones, walls of skulls and
stacked bones in the others. Lava runs in channels across the floor from a
font in each chapel to the ring of fire round the post: an iron obelisk on
a low dais that any player takes for their team with a touch (the map's
`sanctum` control point). Eight basalt columns stand at the gallery's
inner corners and carry the dome's springing; lancets of lava glass burn in
the four walls with no door.

Everything is in world metres round the temple's centre; the octagon's
faces look along the axes and the diagonals (bearing 0 = +X, 90 = +Z).
`build` returns the parts with their light modes ('baked'; 'glow',
firelight; 'hidden', collision only) and what the builder needs (lamps, the
post's point, the hall's box).
"""
import math

import numpy as np

from assets import ashveil_shapes as A
from assets import ashveil_tower as TW

C22 = math.cos(math.pi/8)
T22 = math.tan(math.pi/8)
H = 18.5               # centre to the walls' inner faces
WALL = 1.2
GALLERY = 5.0          # the gallery's depth (its inner edge H - GALLERY out)
DEPTH = 10.0           # the nave below the gallery
SPRING = 7.5           # the dome springs this far over the gallery
VAULT = 55.4           # the dome's top, just under the temple's floor
OCULUS = 4.2           # the well's radius
SIDES = 32             # the well's polygon
STAIR = 3.4            # stair width
DOOR_TUBE = (7.0, 6.5)     # the tubes' doors: width, height
DOOR_VENT = (3.2, 4.4)     # the vents' doors
CUT = 25.0             # cut cells lie wholly inside this octagon (the temple's floor reaches 26)
PASSAGE = 27.0         # the tubes' passages run out to here
NICHE = (5.0, 2.8, 6.0)    # chapels: width, depth, height
CHAPELS = {180.0: 'tomb', 135.0: 'bones', 0.0: 'tomb', 315.0: 'bones'}
TUBES = (180.0, 0.0)
VENTS = (225.0, 45.0)
COLUMN = .95           # the basalt columns' radius
DAIS = (3.6, 2.2, .5)  # foot radius, top radius, height
POST = 4.4             # the obelisk's height over the dais
FIRE = (3.75, 4.25)    # the fire ring round the dais
POINT_RADIUS = 3.2     # where a touch takes the post

# The hall's warning (its holders burn inside it, `cook`), cut for whoever
# stops to read it: a dark stone tablet with tarnished bronze letters in the
# west aisle's dim north end, and one in each half of the tunnel outside.
INSIDE = ('KEEPERS OF THE FLAME, BEGONE.', 'THIS HALL CONSUMES ITS OWN', 'AND SPARES THE STRANGER.')
OUTSIDE = ('BEYOND, THE FLAME BURNS', 'ALL WHO HOLD IT.', 'THE STRANGER ENTERS UNHARMED.')
TABLET = (3.8, 1.1, .12)   # width, height, depth
RELIEF = .015              # how far the letters stand off the stone

# Our own carved capitals: strokes on a 4 x 6 grid (x right, y up), each a
# polyline; the comma and full stop dip under the line.
GLYPHS = {
    'A': [[(0, 0), (2, 6), (4, 0)], [(.7, 2.1), (3.3, 2.1)]],
    'B': [[(0, 0), (0, 6), (2.8, 6), (3.6, 5.3), (3.6, 3.7), (2.8, 3), (0, 3)], [(2.8, 3), (3.8, 2.3), (3.8, .7), (3, 0), (0, 0)]],
    'C': [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1)]],
    'D': [[(0, 0), (0, 6), (2.4, 6), (4, 4.4), (4, 1.6), (2.4, 0), (0, 0)]],
    'E': [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3), (3, 3)]],
    'F': [[(4, 6), (0, 6), (0, 0)], [(0, 3), (3, 3)]],
    'G': [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2.6), (2.3, 2.6)]],
    'H': [[(0, 0), (0, 6)], [(4, 0), (4, 6)], [(0, 3), (4, 3)]],
    'I': [[(2, 0), (2, 6)], [(1, 6), (3, 6)], [(1, 0), (3, 0)]],
    'K': [[(0, 0), (0, 6)], [(4, 6), (0, 2.2)], [(1.3, 3.4), (4, 0)]],
    'L': [[(0, 6), (0, 0), (4, 0)]],
    'M': [[(0, 0), (0, 6), (2, 2.4), (4, 6), (4, 0)]],
    'N': [[(0, 0), (0, 6), (4, 0), (4, 6)]],
    'O': [[(1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5), (4, 1), (3, 0), (1, 0)]],
    'P': [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)]],
    'R': [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(1.8, 3), (4, 0)]],
    'S': [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    'T': [[(0, 6), (4, 6)], [(2, 6), (2, 0)]],
    'U': [[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
    'W': [[(0, 6), (1, 0), (2, 4), (3, 0), (4, 6)]],
    'Y': [[(0, 6), (2, 3), (4, 6)], [(2, 3), (2, 0)]],
    ',': [[(.5, .5), (.5, 0), (0, -1)]],
    '.': [[(.5, 0), (.5, .55)]],
}
ADVANCE = {' ': 3.0, ',': 2.0, '.': 2.0}


def frame(center, bearing):
    """World (x, z) of a point u along and v out from the centre across the
    face at `bearing` degrees, and the face's normal and tangent."""
    t = math.radians(bearing)
    n = np.array([math.cos(t), math.sin(t)]); u_ = np.array([-math.sin(t), math.cos(t)])
    c = np.asarray(center, np.float64)
    return (lambda u, v: c+n*v+u_*u), n, u_


def inside_octagon(center, apothem, x, z):
    dx, dz = x-center[0], z-center[1]
    d = np.zeros_like(np.asarray(dx, np.float64))
    for k in range(8):
        t = math.radians(45*k)
        d = np.maximum(d, dx*math.cos(t)+dz*math.sin(t))
    return d <= apothem


def cut(center, step, vents):
    """The terrain cells cut for the crypt: those lying wholly inside the
    octagon `CUT` from the centre that the crypt, a vent or a tube's
    passage needs."""
    out = set()
    k0, k1 = int((center[1]-CUT-4)/step), int((center[1]+CUT+4)/step)+1
    i0, i1 = int((center[0]-CUT-4)/step), int((center[0]+CUT+4)/step)+1
    for k in range(k0, k1):
        for i in range(i0, i1):
            xs, zs = np.array([i, i+1, i+1, i])*step, np.array([k, k, k+1, k+1])*step
            if not inside_octagon(center, CUT, xs, zs).all(): continue
            x, z = (i+.5)*step, (k+.5)*step
            need = inside_octagon(center, H+WALL+1.5, x, z)
            for vx, vz, r in vents:
                need |= math.hypot(x-vx, z-vz) < r+1.9
            for bearing in TUBES:
                t = math.radians(bearing)
                v = (x-center[0])*math.cos(t)+(z-center[1])*math.sin(t)
                u = -(x-center[0])*math.sin(t)+(z-center[1])*math.cos(t)
                need |= (v > 0) & (abs(u) < DOOR_TUBE[0]/2+2.2)
            if need: out.add(k*256+i)
    return out


# --- Carving -------------------------------------------------------------------

def polygon(center, r, sides):
    """A regular polygon (x, z) round `center`, its corners at r, the first
    on +x (as `ashveil_tower.ycyl` lays its rings)."""
    a = np.arange(sides)*math.tau/sides
    return np.c_[center[0]+r*np.cos(a), center[1]+r*np.sin(a)]


def square(center, h):
    x, z = center
    return np.array([(x-h, z-h), (x+h, z-h), (x+h, z+h), (x-h, z+h)])


def _clip(poly, a, b, c):
    """The part of `poly` (rows of attributes, x at 0 and z at 2) where
    a*x + b*z <= c."""
    out = []
    n = len(poly)
    for i in range(n):
        p, q = poly[i], poly[(i+1) % n]
        dp, dq = a*p[0]+b*p[2]-c, a*q[0]+b*q[2]-c
        if dp <= 0: out.append(p)
        if (dp <= 0) != (dq <= 0): out.append(p+(q-p)*(dp/(dp-dq)))
    return out


def carve(tris, poly, y0, y1, normals=None):
    """Triangles (T, 3, K: x, y, z first) with whatever lies inside the
    convex polygon `poly` (x, z, counter-clockwise or not) cut away, for
    the triangles lying wholly between heights y0 and y1; the rest pass
    unchanged and in order. Cut pieces interpolate every attribute (and
    renormalize `normals`, a slice of them)."""
    if not len(tris): return tris
    p = np.asarray(poly, np.float64)
    if A._area2(p) < 0: p = p[::-1]
    planes = []
    for i in range(len(p)):
        (x0, z0), (x1, z1) = p[i], p[(i+1) % len(p)]
        a, b = z1-z0, -(x1-x0)                      # outward normal of a CCW edge
        planes.append((a, b, a*x0+b*z0))
    t = np.asarray(tris)
    ys = t[..., 1]
    near = (ys.min(1) >= y0) & (ys.max(1) <= y1)
    lo, hi = p.min(0), p.max(0)
    near &= (t[..., 0].max(1) > lo[0]) & (t[..., 0].min(1) < hi[0]) & (t[..., 2].max(1) > lo[1]) & (t[..., 2].min(1) < hi[1])
    for a, b, c in planes:
        near &= ~((a*t[..., 0]+b*t[..., 2]-c) >= -1e-9).all(1)
    if not near.any(): return tris
    out = []
    for k in range(len(t)):
        if not near[k]: out.append(t[k][None]); continue
        rest = [row.astype(np.float64) for row in t[k]]
        for a, b, c in planes:
            outside = _clip(rest, -a, -b, -c)
            if len(outside) >= 3:
                fan = np.array([[outside[0], outside[i], outside[i+1]] for i in range(1, len(outside)-1)])
                area = np.linalg.norm(np.cross(fan[:, 1, :3]-fan[:, 0, :3], fan[:, 2, :3]-fan[:, 0, :3]), axis=1)
                if (area > 1e-7).any(): out.append(fan[area > 1e-7].astype(t.dtype))
            rest = _clip(rest, a, b, c)
            if len(rest) < 3: break
    res = np.concatenate(out) if out else t[:0]
    if normals is not None:
        n = res[..., normals]
        res[..., normals] = n/np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-9)
    return res


def carve_form(form, poly, y0, y1):
    """A form with the inside of `poly` cut away (`carve`)."""
    tris = carve(form.v[form.f], poly, y0, y1)
    return A.Form(tris.reshape(-1, 3), np.arange(len(tris)*3).reshape(-1, 3), form.material, False, form.solid)


def box_minus(lo, hi, poly_lo, poly_hi):
    """A box (lo, hi) less the column over the rectangle poly_lo..poly_hi
    (x, z): up to four boxes."""
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    (a0, b0), (a1, b1) = poly_lo, poly_hi
    if x1 <= a0 or x0 >= a1 or z1 <= b0 or z0 >= b1: return [(lo, hi)]
    out = []
    if x0 < a0: out.append(((x0, y0, z0), (a0, y1, z1)))
    if x1 > a1: out.append(((a1, y0, z0), (x1, y1, z1)))
    xa, xb = max(x0, a0), min(x1, a1)
    if z0 < b0: out.append(((xa, y0, z0), (xb, y1, b0)))
    if z1 > b1: out.append(((xa, y0, b1), (xb, y1, z1)))
    return out


# --- Pieces ----------------------------------------------------------------------

def arch(a, b, spring, crown, r, pointed=True, material='glacier', sides=6, n=11):
    """A rib from a to b (x, z), springing at `spring` at both ends and
    rising to `crown` in the middle: pointed (two arcs meeting) or, with
    pointed False, rising the whole way from a to its crown at b."""
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    pts = []
    for t in np.linspace(0, 1, n):
        g = (1-abs(2*t-1))**.55 if pointed else math.sin(t*math.pi/2)**.8
        xz = a+(b-a)*t
        pts.append((xz[0], spring+(crown-spring)*g, xz[1]))
    return A.tube(pts, [r]*len(pts), sides=sides, material=material, solid=False)


def to_world(form, center, bearing):
    """A form built in a face's frame (x along the face, y up, z out from the
    centre across the face at `bearing`) placed in the world."""
    t = math.radians(bearing)
    n = np.array([math.cos(t), math.sin(t)]); u = np.array([math.sin(t), -math.cos(t)])
    v = form.v
    w = np.c_[center[0]+u[0]*v[:, 0]+n[0]*v[:, 2], v[:, 1], center[1]+u[1]*v[:, 0]+n[1]*v[:, 2]]
    out = A.Form(w, form.f, form.material, form.smooth, form.solid, form.rim)
    A._orient(out)
    return out


def face_point(center, bearing, x, z):
    """World (x, z) of a point x along and z out in a face's frame."""
    t = math.radians(bearing)
    return (center[0]+math.sin(t)*x+math.cos(t)*z, center[1]-math.cos(t)*x+math.sin(t)*z)


def flight(u0, u1, v0, v1, y_top, y_bot, floor, steps, material='concrete'):
    """A straight flight in a face's frame from (u0, y_top) down to
    (u1, y_bot), across v0..v1: its steps drawn down to `floor` ('baked',
    not solid) and a smooth ramp through their noses to walk on, closed
    down to the floor ('hidden')."""
    rise = (y_top-y_bot)/steps
    us = [u0+(u1-u0)*k/steps for k in range(steps+1)]
    poly = [(u0, floor)]
    for k in range(steps):
        h = y_top-(k+1)*rise
        poly += [(us[k], h), (us[k+1], h)]
    poly.append((u1, floor))
    drawn = A.extrude(poly, v0, v1, material, solid=False)
    ramp = A.extrude([(u0, floor), (u0, y_top), (u1, y_bot), (u1, floor)], v0, v1, material, solid=True)
    return [(drawn, 'baked'), (ramp, 'hidden')]


def skull_row(small, a, b, y, face, spacing, size, rng, out):
    """Skulls in a row from a to b (x, z) at height y, facing `face`
    (radians from +x toward +z), each turned and tipped a little."""
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    n = max(1, int(np.linalg.norm(b-a)/spacing))
    for k in range(n):
        x, z = a+(b-a)*(k+.5)/n
        out.append(TW.moved(small, (x, y, z), size*rng.uniform(.9, 1.1), face+rng.uniform(-.25, .25), rng.uniform(-.15, .15)))


def bone_row(a, b, y, out_dir, spacing, rng, out):
    """Stacked long bones seen end on: knobbed ends in a row from a to b
    (x, z) at height y, poking out along `out_dir` (x, z)."""
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    d = np.asarray(out_dir, np.float64)
    n = max(1, int(np.linalg.norm(b-a)/spacing))
    for k in range(n):
        x, z = a+(b-a)*(k+.5)/n
        r = rng.uniform(.075, .1)
        yy = y+rng.uniform(-.04, .04)
        tip = rng.uniform(.18, .3)
        out.append(A.tube([(x-d[0]*.1, yy, z-d[1]*.1), (x+d[0]*tip, yy, z+d[1]*tip)], [r, r*1.15], sides=6,
                          material='bone' if rng.random() < .7 else 'bone_grime', solid=False))


def letters(lines, center, face, right, unit, material='trim', stroke=.6):
    """Raised letters for `lines` on a tablet facing along z: centred on
    `center` (x, y) of the face at z = `face[0]`, standing `face[1]` off it
    (negative toward -z), reading along x by `right` (+1 or -1); one unit of
    the 4 x 6 grid is `unit` metres."""
    out, rows = [], len(lines)
    z0, z1 = sorted((face[0], face[0]+face[1]))
    for row, line in enumerate(lines):
        width = sum(ADVANCE.get(ch, 5.0) for ch in line)-1.0
        u0 = -width/2
        v0 = (rows-1)*5.0-row*10.0-3.0
        for ch in line:
            for path in GLYPHS.get(ch, []):
                for (a, b), (c, d) in zip(path[:-1], path[1:]):
                    ax, ay, bx_, by = u0+a, v0+b, u0+c, v0+d
                    dx, dy = bx_-ax, by-ay
                    n = math.hypot(dx, dy)
                    ex, ey = dx/n*stroke*.35, dy/n*stroke*.35
                    nx, ny = -dy/n*stroke/2, dx/n*stroke/2
                    quad = [(ax-ex+nx, ay-ey+ny), (bx_+ex+nx, by+ey+ny), (bx_+ex-nx, by+ey-ny), (ax-ex-nx, ay-ey-ny)]
                    out.append(A.extrude([(center[0]+right*qu*unit, center[1]+qv*unit) for qu, qv in quad], z0, z1,
                                         material, solid=False))
            u0 += ADVANCE.get(ch, 5.0)
    return out


def tablet(x, y, z, facing, lines):
    """A dark stone tablet, its back on the wall at z, facing +z or -z
    (`facing`), centred on (x, y), framed in bronze, with `lines` raised on
    it in bronze (`letters`). Read facing it, so +z reads along +x."""
    w, ht, d = TABLET
    front = z+facing*d
    lo, hi = (x-w/2, y-ht/2, min(z, front)), (x+w/2, y+ht/2, max(z, front))
    forms = [A.box(lo, hi, 'bone_dark', False)]
    rim, proud = .05, .02
    z0, z1 = sorted((front, front+facing*proud))
    for a, b in (((lo[0], hi[1]-rim), (hi[0], hi[1])), ((lo[0], lo[1]), (hi[0], lo[1]+rim)),
                 ((lo[0], lo[1]+rim), (lo[0]+rim, hi[1]-rim)), ((hi[0]-rim, lo[1]+rim), (hi[0], hi[1]-rim))):
        forms.append(A.box((a[0], a[1], z0), (b[0], b[1], z1), 'trim', False))
    longest = max(sum(ADVANCE.get(ch, 5.0) for ch in line)-1.0 for line in lines)
    unit = min((w-.3)/longest, (ht-.3)/((len(lines)-1)*10.0+7.0))
    return forms+letters(lines, (x, y), (front, facing*RELIEF), facing, unit)


def octagon(center, apothem):
    r = apothem/C22
    a = np.radians(22.5+45.0*np.arange(8))
    return np.c_[center[0]+r*np.cos(a), center[1]+r*np.sin(a)]


def yrev(profile, sides, material, solid=True, at=(0.0, 0.0, 0.0), start=0.0):
    """A lathe about +y whose first corner is at `start` degrees."""
    form = A.lathe(profile, sides=sides, material=material, smooth=False, solid=solid, arc=(start, start+360.0))
    v = form.v[:, [0, 2, 1]]+np.asarray(at, np.float64)
    out = A.Form(v, form.f[:, ::-1], material, False, solid)
    A._orient(out)
    return out


def build(center, top, well_top, seed=0):
    """Every part of the crypt round `center` (x, z): its gallery at `top`
    (the tubes' floor), the well and vents rising to `well_top` (the
    temple's floor). Returns {'parts': [(form, lit)], 'lamps': [((x, y, z),
    power)], 'point': the post's control point position, 'beacon': its
    beacon's height over it, 'hall': the hall's inside as a box (lo, hi),
    floor to dome, 'floor': the nave's height}."""
    cx, cz = center
    yg = top
    yf = yg-DEPTH
    ys = yg+SPRING
    yv = VAULT
    vi = H-GALLERY
    parts, lamps, smoke = [], [], []
    rng = np.random.default_rng(seed+313)

    def add(forms, lit='baked'):
        parts.extend((f, lit) for f in (forms if isinstance(forms, list) else [forms]))

    def prism(bearing, pts, y0, y1, material='concrete', solid=True):
        poly = [face_point(center, bearing, u, v) for u, v in pts]
        return A.extrude(poly, y0, y1, material, solid, axis='y')

    def local(form, bearing):
        return to_world(form, center, bearing)

    # The floor under everything.
    add(A.extrude(octagon(center, H+WALL), yf-1.5, yf, 'concrete', axis='y'))

    # The walls: tall doors for the tubes (west, east), low ones for the
    # vents (northwest, southeast).
    for k in range(8):
        b = 45.0*k
        li, lo = H*T22, (H+WALL)*T22
        door = DOOR_TUBE if b in TUBES else DOOR_VENT if b in VENTS else None
        top_ = ys+.8
        if door is None:
            add(prism(b, [(-li, H), (li, H), (lo, H+WALL), (-lo, H+WALL)], yf-1.5, top_))
            continue
        dw, dh = door
        add([prism(b, [(-li, H), (li, H), (lo, H+WALL), (-lo, H+WALL)], yf-1.5, yg),
             prism(b, [(-li, H), (li, H), (lo, H+WALL), (-lo, H+WALL)], yg+dh, top_),
             prism(b, [(-li, H), (-dw/2, H), (-dw/2, H+WALL), (-lo, H+WALL)], yg, yg+dh),
             prism(b, [(dw/2, H), (li, H), (lo, H+WALL), (dw/2, H+WALL)], yg, yg+dh)])
        # Bronze jambs and a lintel.
        add([prism(b, [(-dw/2-.45, H-.12), (-dw/2, H-.12), (-dw/2, H), (-dw/2-.45, H)], yg, yg+dh, 'trim', False),
             prism(b, [(dw/2, H-.12), (dw/2+.45, H-.12), (dw/2+.45, H), (dw/2, H)], yg, yg+dh, 'trim', False),
             prism(b, [(-dw/2-.45, H-.12), (dw/2+.45, H-.12), (dw/2+.45, H), (-dw/2-.45, H)], yg+dh, yg+dh+.5, 'trim', False)])

    # The gallery: a band round the walls, its top the gallery's floor,
    # with a chapel cut into its foot on four faces.
    nw, nd, nh = NICHE
    for k in range(8):
        b = 45.0*k
        trap = [(-vi*T22, vi), (vi*T22, vi), (H*T22, H), (-H*T22, H)]
        if b not in CHAPELS:
            add(prism(b, trap, yf, yg))
        else:
            vb = vi+nd
            add([prism(b, trap, yf+nh, yg),
                 prism(b, [(-vb*T22, vb), (vb*T22, vb), (H*T22, H), (-H*T22, H)], yf, yf+nh),
                 prism(b, [(-vi*T22, vi), (-nw/2, vi), (-nw/2, vb), (-vb*T22, vb)], yf, yf+nh),
                 prism(b, [(nw/2, vi), (vi*T22, vi), (vb*T22, vb), (nw/2, vb)], yf, yf+nh)])
            # A pointed bronze arch round the chapel's mouth.
            p0, p1 = face_point(center, b, -nw/2, vi-.05), face_point(center, b, nw/2, vi-.05)
            add(arch(p0, p1, yf+nh-1.4, yf+nh-.15, .14, material='trim'))
        # The gallery's edge: bronze trim and a peaked rail, open where a
        # stair leaves it.
        add(prism(b, [(-vi*T22, vi-.08), (vi*T22, vi-.08), (vi*T22, vi+.08), (-vi*T22, vi+.08)], yg-.25, yg+.03, 'trim', False))
        if b == 225.0 or b == 45.0:
            ua, ub = -1.4, vi*T22-.2
        else:
            ua, ub = -vi*T22+.2, vi*T22-.2
        if ub > ua:
            a_, b_ = face_point(center, b, ua, vi+.25), face_point(center, b, ub, vi+.25)
            add(TW.ridge((a_[0], yg, a_[1]), (b_[0], yg, b_[1]), .5, 1.0, 'concrete'))

    # The stairs: from the west door's end of the gallery along the
    # northwest wall to a landing in the corner, then along the north wall to
    # the nave; and the same turned half round from the east door.
    mid = yg-DEPTH/2
    for first, second in ((225.0, 270.0), (45.0, 90.0)):
        v0, v1 = vi-STAIR, vi
        add(prism(first, [(-v1*T22, v1), (-3.4, v1), (-3.4, v0), (-v0*T22, v0)], yf, yg))     # top landing
        for form, lit in flight(-3.4, 4.1, v0, v1, yg, mid, yf, 22):
            add(local(form, first), lit)
        corner = [face_point(center, first, 4.1, v0), face_point(center, first, 4.1, v1),
                  face_point(center, first, v1*T22, v1), face_point(center, second, -4.1, v1),
                  face_point(center, second, -4.1, v0), face_point(center, first, v0*T22, v0)]
        add(A.extrude(corner, yf, mid, 'concrete', True, axis='y'))
        for form, lit in flight(-4.1, 3.4, v0, v1, mid, yf, yf, 22):
            add(local(form, second), lit)
        # Peaked rails along the stairs' open sides.
        for b_, ua, ub, ya, yb in ((first, -3.4, 4.1, yg, mid), (second, -4.1, 3.4, mid, yf)):
            a_, c_ = face_point(center, b_, ua, v0+.25), face_point(center, b_, ub, v0+.25)
            add(TW.ridge((a_[0], ya, a_[1]), (c_[0], yb, c_[1]), .45, 1.05, 'concrete'))

    # The dome: an octagonal vault springing from the walls to the well's
    # ring, with basalt ribs on its corners and a bronze band at its foot.
    ri = H/C22
    prof_in = [(OCULUS+(ri-OCULUS)*math.cos(t*math.pi/2), ys+(yv-.55-ys)*math.sin(t*math.pi/2)) for t in np.linspace(0, 1, 9)]
    prof_out = [(r+.75*math.cos(t*math.pi/2)+.05, y+.5*math.sin(t*math.pi/2)+.35)
                for (r, y), t in zip(prof_in, np.linspace(0, 1, 9))]
    profile = prof_in+[(OCULUS, yv)]+prof_out[::-1][1:]+[(ri+1.6, ys)]+[prof_in[0]]
    add(yrev(profile, 8, 'concrete', True, (cx, 0, cz), 22.5))
    for k in range(8):
        a = math.radians(22.5+45*k)
        pts = [(cx+(r-.25)*math.cos(a), y-.25, cz+(r-.25)*math.sin(a)) for r, y in prof_in]
        add(A.tube(pts, [.38]*len(pts), sides=6, material='leaf', solid=False))
        # Engaged columns in the wall's corners carry the ribs.
        px, pz = cx+(ri-.4)*math.cos(a), cz+(ri-.4)*math.sin(a)
        add(yrev([(0, yg), (.9, yg), (.9, yg+.6), (.7, yg+.8), (.7, ys-.4), (.95, ys-.1), (.95, ys+.2), (0, ys+.2)], 6,
                 'leaf', True, (px, 0, pz), math.degrees(a)))
        for y in (yg+.85, ys-.6):
            add(yrev([(.68, y), (.76, y), (.76, y+.2), (.68, y+.2), (.68, y)], 6, 'trim', False, (px, 0, pz), math.degrees(a)))
    add(yrev([(ri-.12, ys-.2), (ri+.05, ys-.2), (ri+.05, ys+.1), (ri-.12, ys+.1), (ri-.12, ys-.2)], 8, 'trim', False,
             (cx, 0, cz), 22.5))

    # Lancets of lava glass in the four walls with no door.
    for b in (90.0, 135.0, 270.0, 315.0):
        for u in (-3.2, 3.2):
            add(prism(b, [(u-.7, H-.03), (u+.7, H-.03), (u+.7, H), (u-.7, H)], yg+1.6, ys-1.2, 'light', False))
            a_, c_ = face_point(center, b, u-.95, H-.09), face_point(center, b, u+.95, H-.09)
            add(arch(a_, c_, ys-1.5, ys-.3, .13, material='trim'))
            for du in (-.95, .95):
                q = face_point(center, b, u+du, H-.09)
                add(A.tube([(q[0], yg+1.4, q[1]), (q[0], ys-1.5, q[1])], [.13, .13], sides=6, material='trim', solid=False))
            q = face_point(center, b, u, H-1.6)
            lamps.append(((q[0], yg+4.2, q[1]), 1.1))

    # The well: a lined shaft through the dome to the temple's floor, a
    # bronze ring under the dome and a peaked bronze curb round its top.
    add(yrev([(OCULUS, yv-.6), (OCULUS+.5, yv-.6), (OCULUS+.5, well_top-.02), (OCULUS, well_top-.02), (OCULUS, yv-.6)],
             SIDES, 'concrete', True, (cx, 0, cz)))
    add(yrev([(OCULUS-.01, yv-.75), (OCULUS+.8, yv-.75), (OCULUS+.8, yv-.45), (OCULUS-.01, yv-.45), (OCULUS-.01, yv-.75)],
             SIDES, 'trim', False, (cx, 0, cz)))
    add(yrev([(OCULUS, well_top), (OCULUS+.6, well_top), (OCULUS+.3, well_top+.42), (OCULUS, well_top)], SIDES, 'trim', True,
             (cx, 0, cz)))
    # Lantern rings hung from the dome.
    for k in range(4):
        a = math.radians(45+90*k)
        lx, lz = cx+9.5*math.cos(a), cz+9.5*math.sin(a)
        hang = yv-1.0-(9.5/ri)*3.0
        add(A.tube([(lx, hang+5.0, lz), (lx, hang, lz)], [.05, .05], sides=4, material='panel', solid=False))
        add(yrev([(1.0, hang-.1), (1.15, hang-.1), (1.15, hang+.12), (1.0, hang+.12), (1.0, hang-.1)], 12, 'trim', False,
                 (lx, 0, lz)))
        for j in range(4):
            b_ = j*math.pi/2
            add(TW.lantern((lx+1.08*math.cos(b_), hang-.5, lz+1.08*math.sin(b_)), lamps, .7), 'baked')

    # The vents: lined shafts from the temple's floor down to the gallery,
    # each opening onto it through its door.
    for b in VENTS:
        t = math.radians(b)
        vx, vz = cx+22.0*math.cos(t), cz+22.0*math.sin(t)
        r = 2.0
        inward = (math.degrees(t)+180.0) % 360.0
        half = math.degrees(math.asin((DOOR_VENT[0]/2)/(r+.2)))+4.0
        ring = [(r, yg-.02), (r+.4, yg-.02), (r+.4, well_top-.02), (r, well_top-.02), (r, yg-.02)]
        add(_arc(ring, (vx, vz), inward+half, inward+360.0-half, 20))
        add(_arc([(r, yg+DOOR_VENT[1]), (r+.4, yg+DOOR_VENT[1]), (r+.4, well_top-.02), (r, well_top-.02), (r, yg+DOOR_VENT[1])],
                 (vx, vz), inward-half, inward+half, 4))
        add(yrev([(0, yg-1.0), (r+.45, yg-1.0), (r+.45, yg), (0, yg)], 20, 'concrete', True, (vx, 0, vz)))
        add(yrev([(r, well_top), (r+.5, well_top), (r+.25, well_top+.38), (r, well_top)], 20, 'trim', True, (vx, 0, vz)))
        # The short way through to the gallery: floor, cheeks and a lintel.
        dw = DOOR_VENT[0]
        add([prism(b, [(-dw/2, H+WALL-.05), (dw/2, H+WALL-.05), (dw/2, 22.0-r+.5), (-dw/2, 22.0-r+.5)], yg-1.0, yg),
             prism(b, [(-dw/2-.6, H+WALL), (-dw/2, H+WALL), (-dw/2, 22.0-r+.3), (-dw/2-.6, 22.0-r+.3)], yg, yg+DOOR_VENT[1]+.6),
             prism(b, [(dw/2, H+WALL), (dw/2+.6, H+WALL), (dw/2+.6, 22.0-r+.3), (dw/2, 22.0-r+.3)], yg, yg+DOOR_VENT[1]+.6),
             prism(b, [(-dw/2, H+WALL), (dw/2, H+WALL), (dw/2, 22.0-r+.3), (-dw/2, 22.0-r+.3)], yg+DOOR_VENT[1], yg+DOOR_VENT[1]+.6)])
        add(TW.lantern((vx, yg+3.2, vz), lamps, 1.0), 'baked')

    # The tubes' passages out under the temple's floor: floor, walls, roof.
    for b in TUBES:
        dw, dh = DOOR_TUBE
        v0, v1 = H+WALL-.05, PASSAGE
        add([prism(b, [(-dw/2-.2, v0), (dw/2+.2, v0), (dw/2+.2, v1), (-dw/2-.2, v1)], yg-1.0, yg),
             prism(b, [(-dw/2-.9, v0), (-dw/2, v0), (-dw/2, v1), (-dw/2-.9, v1)], yg, yg+dh),
             prism(b, [(dw/2, v0), (dw/2+.9, v0), (dw/2+.9, v1), (dw/2, v1)], yg, yg+dh),
             prism(b, [(-dw/2-.9, v0), (dw/2+.9, v0), (dw/2+.9, v1), (-dw/2-.9, v1)], yg+dh, yg+dh+1.2)])
        for v in (21.5, 25.5):
            for side in (-1, 1):
                q = face_point(center, b, side*(dw/2-.35), v)
                add(TW.lantern((q[0], yg+3.6, q[1]), lamps, .9), 'baked')

    # The post: an iron obelisk with burning slits on a stone dais, in a
    # ring of fire, with braziers round it.
    r0, r1, dh = DAIS
    add(yrev([(0, yf), (r0, yf), (r1, yf+dh), (0, yf+dh)], 16, 'concrete', True, (cx, 0, cz)))
    add(yrev([(r1-.05, yf+dh), (r1+.12, yf+dh-.04), (r1+.12, yf+dh+.04), (r1-.05, yf+dh+.04)], 16, 'trim', False, (cx, 0, cz)))
    yp = yf+dh
    obelisk = A.lathe([(0, 0), (.95, 0), (.95, .35), (.72, .45), (.52, POST-.75), (.66, POST-.7), (.66, POST-.55), (0, POST)],
                      sides=4, material='panel', smooth=False, arc=(45.0, 405.0))
    v = obelisk.v[:, [0, 2, 1]]+np.array([cx, yp, cz])
    obelisk = A.Form(v, obelisk.f[:, ::-1], 'panel', False, True)
    A._orient(obelisk)
    add(obelisk)
    face = lambda y: math.cos(math.pi/4)*(.72-(y-.45)/(POST-1.2)*.2)
    for k in range(4):
        a = math.radians(90*k)
        c, s = math.cos(a), math.sin(a)
        for y0, y1 in ((.9, 2.2), (2.5, 3.3)):
            w = face(y0)+.005
            add(A.box((cx+c*w-abs(s)*.09-.01, yp+y0, cz+s*w-abs(c)*.09-.01), (cx+c*w+abs(s)*.09+.01, yp+y1, cz+s*w+abs(c)*.09+.01),
                      'light', False))
    lamps.append(((cx, yp+POST+.8, cz), 2.2))
    add(yrev([(FIRE[0], yf-.12), (FIRE[1], yf-.12), (FIRE[1], yf+.012), (FIRE[0], yf+.012), (FIRE[0], yf-.12)], 32, 'water',
             False, (cx, 0, cz)), 'glow')
    for r in (FIRE[0]-.12, FIRE[1]):
        add(yrev([(r, yf), (r+.12, yf), (r+.12, yf+.06), (r, yf+.06), (r, yf)], 32, 'trim', False, (cx, 0, cz)))
    for k in range(4):
        a = math.radians(90*k+22.5)
        add(TW.brazier((cx+6.4*math.cos(a), yf, cz+6.4*math.sin(a)), smoke, lamps, 1.1, smoky=False))

    # Lava runs in channels from a font in each chapel to the ring of fire.
    small = TW.skull(1.0, .28, seed+9, rough=.02)
    small.solid = False
    for b, kind in CHAPELS.items():
        a = math.radians(b)
        c, s = math.cos(a), math.sin(a)
        add(prism(b, [(-.35, FIRE[1]+.1), (.35, FIRE[1]+.1), (.35, vi+.6), (-.35, vi+.6)], yf-.1, yf+.012, 'water', False), 'glow')
        for side in (-1, 1):
            add(prism(b, [(side*.35, FIRE[1]+.1), (side*.47, FIRE[1]+.1), (side*.47, vi+.6), (side*.35, vi+.6)],
                      yf, yf+.05, 'trim', False))
        fx, fz = face_point(center, b, 0.0, vi+1.2)
        add(yrev([(0, yf), (1.1, yf), (1.25, yf+.85), (1.0, yf+.95), (0, yf+.95)], 12, 'concrete', True, (fx, 0, fz)))
        add(yrev([(0, yf+.8), (1.0, yf+.8), (1.0, yf+.86), (0, yf+.86)], 12, 'water', False, (fx, 0, fz)), 'glow')
        lamps.append(((fx, yf+1.6, fz), 1.2))
        back = vi+nd
        if kind == 'tomb':
            # A tomb across the chapel's back, a skull at its head.
            ta, tb = face_point(center, b, -1.4, back-.85), face_point(center, b, 1.4, back-.85)
            add(prism(b, [(-1.3, back-1.5), (1.3, back-1.5), (1.3, back-.2), (-1.3, back-.2)], yf, yf+1.0, 'concrete', True))
            add(prism(b, [(-1.42, back-1.62), (1.42, back-1.62), (1.42, back-.08), (-1.42, back-.08)], yf+1.0, yf+1.25, 'glacier', False))
            hx, hz = face_point(center, b, -.95, back-.85)
            add(TW.moved(small, (hx, yf+1.25, hz), .55, a+math.pi/2, 0.0))
            for k in (-1, 1):
                p, q = face_point(center, b, -.5, back-.85+.4*k), face_point(center, b, .3, back-.85-.4*k)
                add(TW.bone((p[0], yf+1.32, p[1]), (q[0], yf+1.32, q[1]), .06))
        else:
            # Shelves of skulls between rows of stacked bones on the back wall.
            walls = []
            facing = math.atan2(-s, -c)
            p, q = face_point(center, b, -nw/2+.4, back-.25), face_point(center, b, nw/2-.4, back-.25)
            for y in (yf+1.3, yf+2.5, yf+3.7):
                add(prism(b, [(-nw/2, back-.42), (nw/2, back-.42), (nw/2, back), (-nw/2, back)], y-.12, y, 'concrete', False))
                skull_row(small, p, q, y, facing, .6, .5, rng, walls)
            for y in (yf+2.02, yf+3.22):
                bone_row(p, q, y, (-c, -s), .24, rng, walls)
            add(walls)
        add(TW.lantern((*face_point(center, b, 0.0, back-.45)[:1], yf+4.8, face_point(center, b, 0.0, back-.45)[1]), lamps, 1.0),
            'baked')

    # The warning: inside, on the southwest chapel's wall; outside, in each
    # tube's passage under the temple's floor.
    a = math.radians(135.0)
    side_u = -nw/2+.02
    tx, tz = face_point(center, 135.0, side_u, vi+nd/2+.05)
    add(_tablet_turned(center, 135.0, side_u, yf+1.8, vi+nd/2+.05, INSIDE))
    for b, z_wall, facing in ((180.0, cz-DOOR_TUBE[0]/2, 1), (0.0, cz+DOOR_TUBE[0]/2, -1)):
        x = cx+math.cos(math.radians(b))*23.6
        add(tablet(x, yg+1.6, z_wall, facing, OUTSIDE))

    point = (cx, round(yp, 3), cz)
    hall = ((cx-H, yf, cz-H), (cx+H, yv, cz+H))
    return dict(parts=parts, lamps=lamps, point=point, beacon=round(POST+.5, 2), floor=yf, hall=hall)


def _arc(profile, at, a0, a1, sides):
    """A closed ring section `profile` [(r, y)] swept about +y at `at` (x, z)
    from a0 to a1 degrees, capped at both ends."""
    form = A.lathe(profile, sides=sides, material='concrete', smooth=False, solid=True, arc=(a0, a1))
    v = form.v[:, [0, 2, 1]]+np.array([at[0], 0.0, at[1]])
    out = A.Form(v, form.f[:, ::-1], 'concrete', False, True)
    A._orient(out)
    return out


def _tablet_turned(center, bearing, u, y, v, lines):
    """The warning tablet on a chapel's side wall at u (in the face's frame,
    the wall running out along v), facing into the chapel."""
    forms = tablet(v, y, u, 1, lines)
    out = []
    for f in forms:
        local = A.Form(np.c_[f.v[:, 2], f.v[:, 1], f.v[:, 0]], f.f[:, ::-1], f.material, f.smooth, f.solid)
        out.append(to_world(local, center, bearing))
    return out
