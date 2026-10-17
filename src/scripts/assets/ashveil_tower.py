"""Ashveil's skull tower: team 1's flag base (docs/ashveil.md), our own design.

Three great skulls stacked on one axis on a stone plinth ringed by a lava
moat, about 105 m to the top skull's broken crown, merged into one hollow
shell of bone about 3 m thick. They taper upward (46, 40 and 28 m wide) and
nest: each sits deep in the crown of the one below, looking a little down,
so the stack reads as one bone fortress with faces in it. The faces are
chunky and faceted (flat-shaded planes): a brow shelf over big round
sockets, box cheeks and muzzle, a triangular nose, a bar of teeth.

Inside is one great cavity: the gate hall in the lowest skull opens up
into the cavern of the middle one, and a neck leads up into the top one's
room, open to the sky through its broken crown. Every opening is a
skull's own: the lowest's open jaw is the gate (fangs over it), the eye
sockets and noses are holes through the shell. The middle skull's sockets
are dark with fire deep in them.

The flag stands on a bone ledge in the middle skull's north eye, open to
the face and to the cavern behind it. Nothing has stairs, ramps or
scaffolding: bone shelves on the cavity's walls, at most 18.5 m apart in
height, lead from the hall floor up past the eyes to the crown, with fires
on them. Outside there's only bone and fire: patches of half-buried skulls
on the sides and back (the faces stay bare), long bones and seams of
vertebrae set into the shell, buttresses of long bones, vertebra pillars
with fires on top either side of a forecourt, sandbags round the plinth,
stakes along the moat and a grate bridge over it, and a rusted chimney
smoking out of the broken crown.

Everything is built in the tower's frame: metres, +x forward (the gate and
the bridge), +y up, z across, the origin at the centre of the plinth's top.
`build` returns the parts with their light modes and the points the
builder needs (flag, stations, spawns, lamps, smoke).
"""
import math

import numpy as np

from assets import ashveil_shapes as A

# The skulls, lowest to highest: width, teeth-line origin (x, y, z), pitch
# (degrees about z; negative looks down), yaw (degrees from +x toward +z),
# open jaw.
SKULLS = ((46.0, (2.0, 8.0, 0.0), -8.0, 0.0, True),
          (40.0, (-2.0, 38.0, 0.0), -6.0, -4.0, False),
          (28.0, (-8.0, 75.0, 0.0), -10.0, 8.0, False))
SHELL = 3.0
NECK = (-6.0, 70.0, 90.0, 7.5)          # x, from y, to y, radius: joins the top two
CELL = 1.25
PLINTH = 36.0          # plinth radius
MOAT = 50.0            # outer edge of the moat
MOAT_LAVA = -5.2       # moat lava surface, below the plinth top
MOAT_FLOOR = -6.6
EYE = (.42, .42, .20)                   # socket centre (f, u, |s|) in widths
EYE_R = (.24, .17, .17)                 # its radii
EYE_LEDGE = 47.2                        # the middle skull's eye ledges, on their rims
FLAG = (16.0, -8.0)                     # in the north eye (x, z)
CUT = ((.04, .90, .02), (-.28, 1.0, .18))  # the top skull's broken crown (point, normal in widths)
BRIDGE = (PLINTH-.3, 64.0, 4.5, -2.2)   # x0, x1, half width, far end's top
BANK = -2.4                             # the ground round the moat, under the plinth's top
COURT = (27.0, 0.0, 0.0)                # the forecourt's middle (its control point)
# The side gates (degrees from +x toward +z): stone causeways cross the moat
# there from the valleys' mouths, over the falls where their rivers pour in.
SIDE_GATES = (96.0, -96.0)
CAUSEWAY = 3.6                          # half width
# Gaps (degrees, half widths) in the plinth's parapet and the moat's rim:
# exactly the bridge's and the causeways' widths, so the parapet meets
# their curbs and nothing can step off beside them into the moat.
_gap = lambda half, r: math.degrees(math.asin(half/r))
PARAPET_GAPS = ((0.0, _gap(4.5, PLINTH-.55)), (96.0, _gap(CAUSEWAY, PLINTH-.55)), (-96.0, _gap(CAUSEWAY, PLINTH-.55)))
RIM_GAPS = ((0.0, _gap(4.5, MOAT+.25)), (96.0, _gap(CAUSEWAY, MOAT+.25)), (-96.0, _gap(CAUSEWAY, MOAT+.25)))


# --- Small pieces -----------------------------------------------------------

def _frame(a, b, up=(0.0, 1.0, 0.0)):
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    t = b-a; length = np.linalg.norm(t); t = t/length
    up = np.asarray(up, np.float64)
    if abs(t @ up) > .98: up = np.array([1.0, 0, 0]) if abs(t[0]) < .9 else np.array([0, 0, 1.0])
    s = np.cross(t, up); s /= np.linalg.norm(s)
    u = np.cross(s, t)
    return a, t, s, u, length


def beam(a, b, w, h, material='timber', solid=True, up=(0.0, 1.0, 0.0), drop=0.0):
    """A box from a to b, w across and h deep (its top `drop` below the
    line a-b; with drop = h/2 the line runs along its middle)."""
    o, t, s, u, length = _frame(a, b, up)
    form = A.box((0, -w/2, -h), (length, w/2, 0), material, solid)
    m = np.c_[np.stack([t, s, u], 1), o-u*drop]
    v = form.v@m[:, :3].T+m[:, 3]
    out = A.Form(v, form.f, material, False, solid)
    A._orient(out)
    return out


def ridge(a, b, w, h, material='concrete', solid=True):
    """A curb from a to b with a peaked top (a triangle w across, h high,
    on the line a-b): too steep to stand on, so nobody plans a step off it."""
    o, t, s, u, length = _frame(a, b)
    form = A.extrude([(-w/2, 0), (w/2, 0), (0, h)], 0, length, material, solid)
    v = o+np.outer(form.v[:, 0], s)+np.outer(form.v[:, 1], u)+np.outer(form.v[:, 2], t)
    out = A.Form(v, form.f, material, False, solid)
    A._orient(out)
    return out


def plank(a, b, w, thick=.3, material='timber', solid=True):
    """A walkable plank whose top runs through a and b."""
    return beam(a, b, w, thick, material, solid)


def post(x, z, y0, y1, w=.36, material='timber', solid=True):
    return A.box((x-w/2, y0, z-w/2), (x+w/2, y1, z+w/2), material, solid)


def ycyl(profile, sides, material, solid=True, at=(0.0, 0.0, 0.0), squash=1.0):
    """A lathe about +y: profile [(r, y), ...]."""
    form = A.lathe(profile, sides=sides, material=material, smooth=False, solid=solid)
    v = form.v[:, [0, 2, 1]]*np.array([1, 1, squash])+np.asarray(at, np.float64)
    out = A.Form(v, form.f[:, ::-1], material, False, solid)
    A._orient(out)
    return out


def lantern(p, lamps, power=1.3):
    """A small iron lantern with a glowing core, and its baked lamp."""
    x, y, z = p
    out = [A.box((x-.22, y-.35, z-.22), (x+.22, y+.35, z+.22), 'light', False),
           A.box((x-.3, y+.35, z-.3), (x+.3, y+.5, z+.3), 'panel', False),
           A.box((x-.26, y-.45, z-.26), (x+.26, y-.35, z+.26), 'panel', False)]
    lamps.append(((x, y, z), power))
    return out


def brazier(p, smoke, lamps, scale=1.0, smoky=True):
    """An iron fire bowl on three legs: glowing coals, a lamp and (unless
    indoors) smoke."""
    x, y, z = p; s = scale
    out = [ycyl([(0, .55*s), (.55*s, .62*s), (.75*s, 1.05*s), (.68*s, 1.1*s), (0, 1.0*s)], 10, 'panel', True, (x, y, z))]
    for k in range(3):
        a = k*math.tau/3
        out.append(beam((x+math.cos(a)*.7*s, y, z+math.sin(a)*.7*s), (x+math.cos(a)*.3*s, y+.7*s, z+math.sin(a)*.3*s),
                        .14*s, .14*s, 'panel', False, drop=.07*s))
    out.append(ycyl([(0, 1.0*s), (.6*s, 1.02*s), (.45*s, 1.25*s), (0, 1.3*s)], 8, 'light', False, (x, y, z)))
    lamps.append(((x, y+1.6*s, z), 1.4*s))
    if smoky: smoke.append([x, y+1.4*s, z, .45*s])
    return out


def bone(a, b, r, material='bone', solid=False):
    """A long bone: a shaft with knobbed ends."""
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    t = (b-a)/np.linalg.norm(b-a)
    out = [A.tube([a, (a*2+b)/3, (a+b*2)/3, b], [r*1.1, r*.85, r*.85, r*1.1], sides=7, material=material, solid=solid)]
    o, _, s, u, _ = _frame(a, b)
    for end, sign in ((a, -1), (b, 1)):
        for k in (-1, 1):
            c = end+t*sign*r*.4+s*k*r*.75
            out.append(A.blob(c-r*1.05, c+r*1.05, 7+k, rough=.05, rings=4, sides=6, material=material, solid=solid))
    return out


def vertebrae(x, z, y0, count, w, material='bone'):
    """A pillar of vertebrae: thick discs with wings and a spine."""
    out = []
    h = w*.55
    for k in range(count):
        y = y0+k*h
        out.append(ycyl([(0, y), (w*.48, y+.04*w), (w*.5, y+h*.5), (w*.46, y+h*.82), (0, y+h*.86)], 9, material, True, (x, 0, z)))
        out.append(A.box((x-w*.18, y+h*.25, z-w*.75), (x+w*.18, y+h*.55, z+w*.75), material, False))
        out.append(A.box((x-w*.7, y+h*.3, z-w*.12), (x-w*.3, y+h*.7, z+w*.12), material, False))
    return out


def tooth(top, length, width, depth, down=True, material='bone', solid=True):
    """A tooth hanging from `top` (or standing on it with down=False)."""
    prof = [(0, 0), (.42, .06), (.5, .35), (.44, .7), (.26, .93), (0, 1.0)]
    form = A.lathe([(r*width, z*length) for r, z in prof], sides=8, material=material, smooth=False, solid=solid)
    v = form.v.copy()
    v[:, 1] *= depth/width
    sign = -1 if down else 1
    w = np.c_[v[:, 1], sign*v[:, 2], v[:, 0]]+np.asarray(top, np.float64)
    out = A.Form(w, form.f, material, False, solid)
    A._orient(out)
    return out


# --- The skulls ---------------------------------------------------------------

def skull_field(width, jaw=False, deep_eyes=False, seed=0, base=0.0, rough=.006, holes=True):
    """Signed distance (m) to a solid, chunky skull `width` across, in its
    own frame: +x forward, +y up, z across; the upper teeth line at y = 0.
    `jaw`: the lower jaw open and broken at the chin (the gate), the head
    on a drum down to the floor. `base` (in widths below the teeth line)
    cuts it flat there; None leaves it open below. The eyes, nose and mouth
    are carved as hollows unless `holes` is false (`tower_field` cuts them
    through its shell instead)."""
    W = float(width)
    k = .04

    def field(p):
        q = np.asarray(p, np.float64)/W
        m = q.copy(); m[:, 2] = np.abs(m[:, 2])
        d = A.sd_ellipsoid(q, (0, .58, 0), (.5, .54, .46))
        d = A.smin(d, A.sd_ellipsoid(q, (.12, .69, 0), (.33, .29, .43)), .06)
        d = A.smin(d, A.sd_box(q, (.38, .56, 0), (.14, .07, .36), .03), k)
        d = A.smin(d, A.sd_box(m, (.22, .32, .30), (.18, .13, .16), .05), k)
        d = A.smin(d, A.sd_capsule(m, (.20, .34, .38), (-.08, .46, .46), .06), k)
        d = A.smin(d, A.sd_box(q, (.28, .18, 0), (.22, .15, .26), .07), k)
        d = A.smin(d, A.sd_box(q, (.34, .02, 0), (.14, .07, .24), .03), k)
        if jaw:
            ring = np.hypot((q[:, 0]+.04)/.46, q[:, 2]/.40)
            d = A.smin(d, np.maximum((ring-1)*.40, q[:, 1]-.36), k)
            d = A.smin(d, A.sd_box(m, (.28, -.18, .28), (.20, .12, .14), .05), k)
        if holes: d = A.smax(d, -_holes(q, m, jaw, deep_eyes), .02)
        d = A.smax(d, -A.sd_ellipsoid(m, (.02, .48, .48), (.2, .16, .08)), .04)
        if rough: d = d+rough*(A.fbm(q*4.5, seed, 1.0, octaves=2)-.5)*2
        if base is not None: d = np.maximum(d, -base-q[:, 1])
        return d*W
    return field


def _holes(q, m, jaw, deep_eyes):
    """The eye sockets, the triangular nose and (with the jaw) the mouth, in
    widths."""
    r = EYE_R if deep_eyes else (.16, .15, .15)
    d = A.sd_ellipsoid(m, EYE, r)
    # The nose: a triangle (apex up) pushed through the face.
    u, s = q[:, 1], m[:, 2]
    a, b = np.array([.08, .09]), np.array([.38, 0.0])      # (u, |s|): base corner, apex
    n = np.array([a[1]-b[1], b[0]-a[0]]); n /= np.linalg.norm(n)
    tri = np.maximum(.08-u, (u-a[0])*n[0]+(s-a[1])*n[1])
    nose = np.maximum(tri, np.abs(q[:, 0]-.46)-.18)
    d = np.minimum(d, nose)
    if jaw: d = np.minimum(d, A.sd_box(q, (.50, -.14, 0), (.42, .19, .25), .04))
    return d


def rotation(pitch, yaw):
    """A skull's frame to the tower's: pitched about z, then turned about y."""
    p, y = math.radians(pitch), math.radians(yaw)
    rz = np.array([[math.cos(p), -math.sin(p), 0], [math.sin(p), math.cos(p), 0], [0, 0, 1.0]])
    ry = np.array([[math.cos(y), 0, -math.sin(y)], [0, 1.0, 0], [math.sin(y), 0, math.cos(y)]])
    return ry@rz


FRAMES = [(np.asarray(at, np.float64), rotation(pitch, yaw)) for _, at, pitch, yaw, _ in SKULLS]


def local(k, p):
    """Tower points in skull k's frame (metres)."""
    o, r = FRAMES[k]
    return (np.asarray(p, np.float64)-o)@r


def world(k, p):
    """Skull k's points in the tower's frame."""
    o, r = FRAMES[k]
    return np.asarray(p, np.float64)@r.T+o


def tower_solid(seed=0):
    """Signed distance (m) to the three skulls and the neck merged, solid
    (before `tower_field` hollows them)."""
    fields = [skull_field(w, jaw=jaw, deep_eyes=True, seed=seed+k, base=None if jaw else 0.0, holes=False)
              for k, (w, _, _, _, jaw) in enumerate(SKULLS)]
    nx, ny0, ny1, nr = NECK

    def solid(p):
        p = np.asarray(p, np.float64)
        d = None
        for k, f in enumerate(fields):
            e = f(local(k, p))
            d = e if d is None else A.smin(d, e, 1.8)
        return A.smin(d, A.sd_capsule(p, (nx, ny0, 0), (nx, ny1, 0), nr), 2.0)
    return solid


def tower_field(seed=0):
    """Signed distance (m) to the tower's bone in its frame: the three
    skulls and the neck merged, hollowed to a SHELL-thick wall, the eyes,
    noses and gate cut through it, the top skull's crown broken off, flat
    on the plinth."""
    solid = tower_solid(seed)

    def field(p):
        p = np.asarray(p, np.float64)
        d = solid(p)
        d = np.maximum(d, -(d+SHELL))
        for k, (w, _, _, _, jaw) in enumerate(SKULLS):
            q = local(k, p)/w
            m = q.copy(); m[:, 2] = np.abs(m[:, 2])
            d = A.smax(d, -_holes(q, m, jaw, True)*w, .4)
        c, n = np.asarray(CUT[0], np.float64), np.asarray(CUT[1], np.float64)
        w3 = SKULLS[2][0]
        q3 = local(2, p)/w3
        cut = ((q3-c)@(n/np.linalg.norm(n))+.08*(A.fbm(q3*9, seed+3, 1.0)-.5)*2)*w3
        d = np.maximum(d, cut)
        return np.maximum(d, -p[:, 1])
    return field


def socket(p):
    """Signed distance (m) to the middle skull's eye sockets, and how far
    up each point is in them (-1 at their bottom, 1 at their top)."""
    w = SKULLS[1][0]
    q = local(1, p)/w
    q[:, 2] = np.abs(q[:, 2])
    return A.sd_ellipsoid(q, EYE, EYE_R)*w, (q[:, 1]-EYE[1])/EYE_R[1], (q[:, 0]-EYE[0])/EYE_R[0]


def skull(width, cell, seed=0, **kw):
    """A solid skull's mesh in its own frame (see `skull_field`)."""
    lo = (-.6*width, -.05*width, -.56*width)
    hi = (.6*width, 1.16*width, .56*width)
    return A.isosurface(skull_field(width, seed=seed, **kw), lo, hi, cell, 'bone', smooth=False)


def split(form, keep):
    """Two forms from one: the faces whose centroid passes `keep` and the
    rest."""
    c = form.v[form.f].mean(1)
    k = keep(c)
    return (A.Form(form.v, form.f[k], form.material, form.smooth, form.solid),
            A.Form(form.v, form.f[~k], form.material, form.smooth, form.solid))


def moved(form, offset, scale=1.0, yaw=0.0, tilt=0.0):
    """A copy tilted `tilt` (radians) about z, turned `yaw` (radians, +x
    toward +z) about +y, scaled and moved."""
    c, s = math.cos(yaw), math.sin(yaw)
    v = form.v*scale
    if tilt:
        ct, st = math.cos(tilt), math.sin(tilt)
        v = np.c_[v[:, 0]*ct-v[:, 1]*st, v[:, 0]*st+v[:, 1]*ct, v[:, 2]]
    v = np.c_[v[:, 0]*c-v[:, 2]*s, v[:, 1], v[:, 0]*s+v[:, 2]*c]+np.asarray(offset, np.float64)
    return A.Form(v, form.f, form.material, form.smooth, form.solid)


# --- The tower ------------------------------------------------------------------

def surface_radius(field, y, angle, reach=40.0, x0=0.0):
    """How far out from the axis (at x0) the bone's outer surface is at
    height y in direction `angle` (radians from +x toward +z)."""
    r = np.arange(reach, 0, -.5)
    pts = np.c_[x0+np.cos(angle)*r, np.full_like(r, y), np.sin(angle)*r]
    inside = field(pts) < 0
    return float(r[np.argmax(inside)]) if inside.any() else 0.0


def shelf(x0, x1, z0, z1, top, thick=1.2, seed=0, material='bone_dark'):
    """A bone shelf on the cavity's wall: a rounded slab, lumpy underneath."""
    rx, rz = (x1-x0)/2, (z1-z0)/2
    cx, cz = (x0+x1)/2, (z0+z1)/2
    k = min(rx, rz)*.45
    poly = []
    for qx, qz, a0 in ((1, 1, 0), (-1, 1, 90), (-1, -1, 180), (1, -1, 270)):
        for t in np.linspace(0, 90, 4):
            a = math.radians(a0+t)
            poly.append((cx+qx*(rx-k)+math.cos(a)*k, cz+qz*(rz-k)+math.sin(a)*k))
    form = A.extrude(poly, top-thick, top, material, True, axis='y')
    under = A.blob((cx-rx*.8, top-thick*2.2, cz-rz*.8), (cx+rx*.8, top-thick*.5, cz+rz*.8), seed, rough=.15, rings=4,
                   sides=8, material=material, solid=False)
    return [form, under]


# Bone shelves up the cavity (x0, x1, z0, z1, top), at most 18.5 m apart in
# height: the hall's back, under the lowest skull's eyes, the junction's
# sides, the middle skull's back, its eye ledges on their rims (the flag on
# the north one, a station on the south one), its walls, the neck's back,
# under the top skull's eyes, its crown's inner lip.
SHELVES = [(-18.0, -2.0, -9.0, 9.0, 15.5), (6.0, 17.0, 5.5, 14.0, 18.5), (6.0, 17.0, -14.0, -5.5, 18.5),
           (-12.0, 5.0, 10.0, 17.5, 36.5), (-12.0, 5.0, -17.5, -10.0, 36.5), (-20.0, -9.0, -8.0, 8.0, 47.5),
           (8.0, 22.0, -12.5, -3.2, EYE_LEDGE), (8.0, 22.0, 3.2, 12.5, EYE_LEDGE),
           (-10.0, 6.0, 12.0, 18.5, 66.0), (-10.0, 6.0, -18.5, -12.0, 66.0), (-18.0, -7.0, -7.0, 7.0, 77.5),
           (-5.0, 5.0, 1.5, 8.5, 81.0), (-5.0, 5.0, -8.5, -1.5, 81.0), (-16.0, -5.0, -6.0, 6.0, 99.5)]


def axis_x(y):
    """The stack's axis (x) at height y: under each skull's cranium."""
    return float(np.interp(y, [0, 30, 45, 68, 80, 120], [2.0, 2.0, -2.0, -2.0, -8.0, -8.0]))


def vertebra(c, out, w):
    """One vertebra set into the bone at c: a disc, its spine out along
    `out` (unit, across), its wings along the surface."""
    c, out = np.asarray(c, np.float64), np.asarray(out, np.float64)
    t = np.cross([0, 1.0, 0], out)
    h = w*.55
    disc = ycyl([(0, -h*.45), (w*.48, -h*.4), (w*.5, 0), (w*.46, h*.35), (0, h*.4)], 9, 'bone', False, c)
    return [disc, beam(c, c+out*w*.8, w*.3, w*.3, 'bone', False, drop=w*.15),
            beam(c-t*w*.75, c+t*w*.75, w*.22, w*.22, 'bone', False, drop=w*.11)]


def build(seed=0, cell=CELL):
    """Every part of the tower: [(form, lit)], lit 'baked', 'live' or 'glow'
    (firelight drawn like cooled lava, but no lava ground), and the points
    {'flag', 'stations', 'spawns', 'lamps', 'smoke'} in the tower's frame (a
    station is (x, floor, z, facing in degrees from +x toward +z))."""
    parts, lamps, smoke = [], [], []
    rng = np.random.default_rng(seed+77)

    def add(forms, lit='baked'):
        parts.extend((f, lit) for f in (forms if isinstance(forms, list) else [forms]))

    # The plinth: a stone drum from the moat's floor to the top, a bronze lip.
    add(ycyl([(0, MOAT_FLOOR-1.5), (PLINTH+.8, MOAT_FLOOR-1.5), (PLINTH+.5, -1.4), (PLINTH, -.5), (PLINTH, 0), (0, 0)],
             64, 'concrete'))
    add(ycyl([(PLINTH-.2, -.05), (PLINTH+.35, -.05), (PLINTH+.35, .16), (PLINTH-.2, .16)], 64, 'trim', False))
    def ring(r0, r1, y0, y1, gaps):
        # A stone wall round the axis from r0 to r1, y0 up to a peaked top
        # at y1 (too steep to stand on), a bronze band under the peak,
        # broken at `gaps`.
        cuts = sorted(((a-h) % 360, (a+h) % 360) for a, h in gaps)
        k = (r1-r0)*.7
        for (_, end0), (start1, _) in zip(cuts, cuts[1:]+cuts[:1]):
            arc = (end0, start1 if start1 > end0 else start1+360)
            for profile, material, solid in (([(r0, y0), (r1, y0), (r1, y1-k), ((r0+r1)/2, y1), (r0, y1-k), (r0, y0)],
                                              'concrete', True),
                                             ([(r1, y1-k-.22), (r1+.06, y1-k-.22), (r1+.06, y1-k-.06), (r1, y1-k-.06),
                                               (r1, y1-k-.22)], 'trim', False)):
                wall = A.lathe(profile, sides=96, arc=arc, material=material, smooth=False, solid=solid)
                wall = A.Form(wall.v[:, [0, 2, 1]], wall.f[:, ::-1], material, False, solid)
                A._orient(wall)
                add(wall)
    # A parapet round its edge (gates for the bridge and the causeways), so
    # nobody skis off into the moat by accident; a jump clears it.
    ring(PLINTH-1.0, PLINTH-.1, 0, 1.3, PARAPET_GAPS)

    # A rim of stone round the moat's outer edge, 1.3 m over the bank, open
    # where the bridge and the causeways land.
    ring(MOAT-.3, MOAT+.8, MOAT_FLOOR-1, BANK+1.3, RIM_GAPS)
    # The causeways: stone ramps (under 8°) from the valleys' mouths over
    # the moat to the side gates, curbed, on a pier; under each the valley's
    # river pours into the moat.
    falls = []
    for g in SIDE_GATES:
        a = math.radians(g)
        d, t = np.array([math.cos(a), 0, math.sin(a)]), np.array([-math.sin(a), 0, math.cos(a)])
        inner, outer = d*(PLINTH-.6), d*(MOAT+4.0)+np.array([0, BANK, 0])
        add(plank(inner, outer, CAUSEWAY*2, 1.0, 'concrete'))
        for side in (-1, 1):
            o = t*side*(CAUSEWAY-.2)
            add(ridge(inner+o, outer+o, .7, 1.2))
        mid = d*(PLINTH+MOAT)/2
        y = BANK*((PLINTH+MOAT)/2-(PLINTH-.6))/((MOAT+4.0)-(PLINTH-.6))
        ends = np.array([mid-t*(CAUSEWAY-.4), mid+t*(CAUSEWAY-.4)])
        lo, hi = ends.min(0)-1.2, ends.max(0)+1.2
        add(A.box((lo[0], MOAT_FLOOR-1, lo[2]), (hi[0], y-1.2, hi[2]), 'concrete'))
        top, low = d*(MOAT+3.6)+np.array([0, BANK-.3, 0]), d*(MOAT-.8)+np.array([0, MOAT_LAVA+.05, 0])
        v = [top-t*3.2, top+t*3.2, low+t*3.6, low-t*3.6]
        falls.append(A.Form(v, [(0, 1, 2), (0, 2, 3)], 'water', False, False))

    # The bone: one faceted hollow shell. Fire deep in the middle skull's
    # sockets, on their lower and rear walls; the rest of each socket dark.
    field = tower_field(seed)
    bone_ = A.isosurface(field, (-38, -1, -33), (36, 112, 33), cell, 'bone', smooth=False)
    # The cavity's walls (the shell's inner face) soot-dark, the outside pale.
    solid = tower_solid(seed)
    inner, bone_ = split(bone_, lambda c: solid(c) < -SHELL*.5)
    inner.material = 'bone_dark'
    add(inner)
    # Outside, broad patches of filthier bone (about 18 m across) and grime
    # gathered under brows, cheeks and jaws, so no one texture covers it.
    def grimy(c):
        t = bone_.v[bone_.f]
        n = np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0])
        down = -n[:, 1]/np.maximum(np.linalg.norm(n, axis=1), 1e-9)
        return A.fbm(c/18.0, seed+60, 1.0)+np.clip(down, 0, 1)*.3 > .56
    grime, bone_ = split(bone_, grimy)
    grime.material = 'bone_grime'
    add(grime)

    def fire(c):
        d, up, fwd = socket(c)
        return (d > -1.4) & (d < .5) & (up < .15) & (fwd < .6)
    glow, bone_ = split(bone_, fire)
    add(bone_); add(glow, 'glow')

    # Teeth, built in each skull's frame: fangs over the gate, lower teeth
    # on the jaw's sides, the middle skull's row over the lowest one's brow.
    def skull_tooth(k, top, length, width, depth, down=True):
        t = tooth(top, length, width, depth, down)
        t.v = world(k, t.v)
        return t
    w1 = SKULLS[0][0]
    for k in range(-3, 4):
        x = (.44-.5*(k/3.5)**2*.12)*w1
        long_ = 5.2 if abs(k) == 2 else 3.0 if abs(k) == 3 else 3.4
        add(skull_tooth(0, (x, .06*w1, k*2.9), long_, 2.8 if abs(k) == 2 else 2.4, 1.8))
    for side in (-1, 1):
        for j, (f, s, h) in enumerate(((.42, .27, 3.0), (.36, .3, 2.6), (.30, .33, 2.2))):
            add(skull_tooth(0, (f*w1, -.06*w1, side*s*w1), h, 2.2, 1.6, down=False))
    w2 = SKULLS[1][0]
    for k in range(-3, 4):
        x = (.44-.5*(k/3.5)**2*.12)*w2
        add(skull_tooth(1, (x, .05*w2, k*2.6), 3.4 if abs(k) == 2 else 2.6, 2.2, 1.6))

    # Bone shelves up the cavity, with fires (no smoke indoors).
    for k, (x0, x1, z0, z1, top) in enumerate(SHELVES):
        add(shelf(x0, x1, z0, z1, top, thick=2.6, seed=seed+40+k))
        if x1-x0 > 10 or z1-z0 > 10:
            for fx_, fz_ in ((x0+2.2, z0+2.2), (x1-2.2, z1-2.2)):
                add(brazier((fx_, top, fz_), smoke, lamps, 1.0, smoky=False))
        else:
            add(brazier(((x0+x1)/2, top, z0+1.8 if abs(z0) > abs(z1) else z1-1.8), smoke, lamps, .9, smoky=False))
    for x, z in ((-4.0, -10.0), (-4.0, 10.0), (8.0, -12.0), (8.0, 12.0)):
        add(brazier((x, 0, z), smoke, lamps, 1.5, smoky=False))

    # The flag's dais on the north eye's ledge: a bone drum, an iron ring.
    fx, fz = FLAG
    fy = EYE_LEDGE
    add(ycyl([(0, fy), (1.6, fy), (1.5, fy+.3), (0, fy+.3)], 14, 'bone', True, (fx, 0, fz)))
    add(ycyl([(1.52, fy+.3), (1.7, fy+.3), (1.7, fy+.36), (1.52, fy+.36)], 14, 'trim', False, (fx, 0, fz)))
    add(ycyl([(0, fy+.3), (.6, fy+.3), (.6, fy+.34), (0, fy+.34)], 12, 'light', False, (fx, 0, fz)))

    # A rusted chimney rising out of the broken crown, smoking.
    chimney = [(-15.0, 88.0, 0), (-15.0, 104.0, 0), (-15.5, 116.0, .4), (-15.8, 127.0, .6)]
    add(A.tube(chimney, [2.3, 2.3, 2.2, 2.1], sides=10, material='ember', smooth=True))
    for y in (106.0, 116.0, 126.0):
        x = np.interp(y, [p[1] for p in chimney], [p[0] for p in chimney])
        z = np.interp(y, [p[1] for p in chimney], [p[2] for p in chimney])
        add(ycyl([(2.25, -.4), (2.5, -.4), (2.5, .4), (2.25, .4)], 10, 'panel', False, (x, y, z)))
    smoke.append([-15.8, 128.0, .6, 3.4])

    # Bone masonry on the sides and back (the faces stay bare): patches of
    # half-buried skulls, rows round the foot, clusters at the temples, a
    # cairn round the broken crown; long bones and seams of vertebrae set
    # into the shell.
    small = skull(1.0, .2, seed+5, rough=.02)
    small.solid = False
    filthy = A.Form(small.v, small.f, 'bone_grime', small.smooth, False)

    def embed(y, a, size, sink, jitter=.35, tilt=.44):
        x0 = axis_x(y)
        r = surface_radius(field, y, a, reach=45.0, x0=x0)
        if r <= 0: return
        rc = r+size*(.5-sink)
        add(moved(filthy if rng.random() < .4 else small, (x0+math.cos(a)*rc, y-.55*size, math.sin(a)*rc), size,
                  a+rng.uniform(-jitter, jitter), rng.uniform(-tilt, tilt)))
    patches = [(12, 65, 7, 3, 5), (12, -65, 7, 3, 5), (25, 115, 7, 3, 5), (25, -115, 7, 3, 5),
               (39, -65, 9, 2.5, 4.5), (48, 80, 9, 2.5, 4.5), (58, 155, 9, 2.5, 4.5),
               (72, -100, 8, 2, 3.5), (84, 65, 8, 2, 3.5), (98, 140, 6, 1.5, 2.5)]
    for y0, a0, n, s0, s1 in patches:
        for _ in range(n):
            embed(y0+rng.uniform(-5, 5), math.radians(a0+rng.uniform(-18, 18)), rng.uniform(s0, s1), rng.uniform(.35, .5))
    for y, n, size, gap in ((1.0, 20, 4.2, 1.15), (6.0, 16, 3.4, 1.3)):
        for k in range(n):
            a = math.tau*k/n
            if abs(math.atan2(math.sin(a), math.cos(a))) < gap: continue
            embed(y+.6*math.sin(k*2.3)+size*.55, a, size*(1+.15*math.sin(k*1.7)), .5)
    for y in (31.0, 58.0, 89.0):
        for side in (-1, 1):
            for k in range(3):
                embed(y+rng.uniform(-4, 4), side*math.radians(90+rng.uniform(-12, 12)), rng.uniform(3.2, 4.8), .55)
    for k in range(12):
        a = rng.uniform(.9, math.tau-.9)
        embed(rng.uniform(86, 93), a, rng.uniform(2.8, 4.2), .4)
    for _ in range(24):
        y = rng.uniform(6, 98); a = math.radians(rng.uniform(50, 310))
        length, rad = rng.uniform(6, 11), rng.uniform(.6, 1.0)
        phi = math.radians(rng.uniform(30, 60))*rng.choice([-1, 1])
        x0 = axis_x(y)
        r = surface_radius(field, y, a, reach=45.0, x0=x0)
        if r <= 0: continue
        c = np.array([x0+math.cos(a)*r, y, math.sin(a)*r])
        t = np.array([-math.sin(a), 0, math.cos(a)])
        d = math.cos(phi)*np.array([0, 1.0, 0])+math.sin(phi)*t
        ends = []
        for e in (c-d*length/2, c+d*length/2):
            ea = math.atan2(e[2], e[0]-axis_x(e[1]))
            er = surface_radius(field, e[1], ea, reach=45.0, x0=axis_x(e[1]))
            if er <= 0: break
            ends.append((axis_x(e[1])+math.cos(ea)*(er-.35*rad), e[1], math.sin(ea)*(er-.35*rad)))
        if len(ends) == 2: add(bone(ends[0], ends[1], rad, 'bone_grime' if rng.random() < .5 else 'bone'))
    for a0, y0, y1 in ((-110, 8, 38), (95, 35, 72), (175, 70, 103)):
        a = math.radians(a0)
        for y in np.arange(y0, y1, 2.4):
            x0 = axis_x(y)
            r = surface_radius(field, y, a, reach=45.0, x0=x0)
            if r <= 0: continue
            out = np.array([math.cos(a), 0, math.sin(a)])
            add(vertebra((x0+math.cos(a)*(r-.6), y, math.sin(a)*(r-.6)), out, 3.0))
    # Long-bone buttresses against the back, femurs piled on the forecourt.
    for side in (-1, 1):
        add(bone((-32.0, 0, side*14.0), (-18.0, 34.0, side*11.0), 1.4))
        add(bone((-34.0, 0, side*6.0), (-20.0, 28.0, side*4.0), 1.2))
        add(bone((-30.0, 0, side*18.0), (-12.0, 48.0, side*14.0), 1.3))
        for j, (a, b) in enumerate((((21.0, .9, side*22.0), (28.0, .9, side*27.0)), ((22.0, .9, side*27.5), (29.5, 1.0, side*22.5)),
                                    ((23.5, 2.2, side*23.0), (27.0, 2.4, side*27.0)))):
            add(bone(a, b, .8+.1*j))
    for x, z, s, a in [(28.5, -19.5, 4.0, .4), (30.2, -22.5, 3.2, -.2), (26.5, -23.2, 3.0, .9),
                       (28.5, 19.5, 4.0, -.4), (30.4, 22.2, 3.1, .3), (26.2, 23.0, 2.8, -.8)]:
        add(moved(small, (x, -.05*s, z), s, a))

    # The forecourt: vertebra pillars with fires on top, great braziers.
    for side in (-1, 1):
        add(vertebrae(31.0, side*13.5, 0, 9, 4.25))
        add(brazier((31.0, 9*.55*4.25-.2, side*13.5), smoke, lamps, 2.0))
        add(brazier((34.0, 0, side*6.6), smoke, lamps, 1.6))

    # Sandbags inside the parapet round the front (not across the bridge), stakes
    # along the moat's far bank.
    bag = A.blob((-.7, -.28, 0), (.7, .28, .55), seed+9, rough=.08, rings=4, sides=7, material='sacking', flat_bottom=.1)
    bag.v = bag.v[:, [0, 2, 1]]; bag.f = bag.f[:, ::-1]
    for row in range(2):
        r = PLINTH-2.0
        n = int(math.tau*r/1.45)
        for k in range(n):
            a = math.atan2(math.sin(math.tau*(k+.5*row)/n), math.cos(math.tau*(k+.5*row)/n))
            if abs(a) < .16 or abs(a) > 2.0: continue
            if any(abs(abs(a)-math.radians(abs(g))) < .14 for g in SIDE_GATES): continue
            if row and k % 3 == 0: continue
            add(moved(bag, (math.cos(a)*r, row*.52, math.sin(a)*r), 1+.06*math.sin(k*2.1), a+math.pi/2))
    for k in range(52):
        a = math.tau*(k+.5)/52
        if abs(math.atan2(math.sin(a), math.cos(a))) < .2: continue
        r = MOAT+2.6+.8*math.sin(k*1.3)
        bx, bz = math.cos(a)*r, math.sin(a)*r
        for j, lean in enumerate((-.4, .4)):
            tip = (math.cos(a)*(r+2.6), MOAT_LAVA+5.6+.9*j, math.sin(a)*(r+2.6))
            o = (bx+lean*math.sin(a), MOAT_LAVA+.2, bz-lean*math.cos(a))
            parts.append((beam(o, tip, .34, .34, 'timber', False, drop=.17), 'live'))

    # The bridge: a grate deck on iron girders and two piers, rails, lanterns.
    bx0, bx1, half, end = BRIDGE
    add(plank((bx0, 0, 0), (bx1, end, 0), half*2, .4, 'grate'))
    for z in (-half+.4, half-.4):
        add(beam((bx0, -.4, z), (bx1, end-.4, z), .6, 1.2, 'panel'))
        add(ridge((bx0, 0, z*1.02), (bx1, end, z*1.02), .5, 1.2, 'panel'))
        add(beam((bx0, 1.15, z*1.03), (bx1, end+1.15, z*1.03), .12, .12, 'ember', False, drop=.06))
        for k in range(8):
            t = k/7; x = bx0+(bx1-bx0)*t; y = end*t
            add(post(x, z*1.03, y, y+1.15, .14, 'ember', False))
    for x in (43.0, 54.0):
        y = end*(x-bx0)/(bx1-bx0)
        add(A.box((x-1.0, MOAT_FLOOR-1, -half), (x+1.0, y-1.6, half), 'concrete'))
    for x, y in ((bx0+1.8, 0.0), (bx1-.8, end)):
        for z in (-half-.3, half+.3):
            add(post(x, z, y, y+2.8, .34, 'panel'))
            add(lantern((x, y+3.1, z), lamps, 1.2))

    flag = (fx, fy+.36, fz)
    stations = [(-14.0, 0.0, 0.0, 0.0), (13.5, EYE_LEDGE, .2*SKULLS[1][0], 180.0)]
    spawns = [(-8.0, 0.0, -7.0), (-8.0, 0.0, 7.0), (-2.0, 0.0, -5.0), (-2.0, 0.0, 5.0)]
    return dict(parts=parts, lamps=lamps, smoke=smoke, flag=flag, stations=stations, spawns=spawns, falls=falls, court=COURT)


def moat_lava(rings=3, segments=96):
    """The moat's lava: an annulus at its surface in the tower's frame, as
    (vertices, faces, rim) with rim 0 at the moat's middle, 1 at its banks."""
    r = np.linspace(PLINTH-.6, MOAT+.6, rings*2+1)
    a = np.linspace(0, math.tau, segments, endpoint=False)
    R, Ang = np.meshgrid(r, a, indexing='ij')
    v = np.stack([np.cos(Ang)*R, np.full_like(R, MOAT_LAVA), np.sin(Ang)*R], -1).reshape(-1, 3)
    f = []
    for i in range(len(r)-1):
        for j in range(segments):
            k = (j+1) % segments
            a0, a1, b0, b1 = i*segments+j, i*segments+k, (i+1)*segments+j, (i+1)*segments+k
            f += [(a0, a1, b1), (a0, b1, b0)]
    mid = (PLINTH+MOAT)/2
    rim = np.clip(np.abs(R.ravel()-mid)/((MOAT-PLINTH)/2), 0, 1)
    return A.Form(v, f, 'water', False, False, rim)
