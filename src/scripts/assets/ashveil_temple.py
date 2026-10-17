"""Ashveil's temple: our own design (docs/ashveil.md).

An octagonal platform of dark cut stone on the dome in the middle of the
caldera, three steps up from the ground all round, open on every side.
Eight obsidian spires stand on plinths near its corners, joined high up by
a ring of pointed arches, and their tips are the lightning's (the post's
holder's colour glows on them). Over the middle a cage of eight bronze ribs
rises from short piers to a crown ring with a hanging ember; under it the
well opens through the floor to the cathedral (`ashveil_undercroft`), and
two smaller vents open near the northwest and southeast faces. Braziers
stand on the diagonals; bronze inlay rings and a sixteen-point star mark the
floor.

Everything is in world metres round the temple's centre. The octagon's
faces look along the axes and the diagonals (faces at 0, 45, ... degrees
from +X toward +Z); its corners point between them.
"""
import math

import numpy as np

from assets import ashveil_shapes as A
from assets import ashveil_tower as TW

TOP = 57.0             # the platform's floor
THICK = 1.4            # its slab (the vault sits just under it)
APOTHEM = 26.0         # the platform's octagon, centre to face
STEPS = 3              # steps down to the ground all round
STEP_RUN = 1.0
SPIRE_R = 21.0         # the spires' ring (on the corners' bearings)
SPIRE_TALL = 32.0      # tip over the floor
PIER_R = 34.0          # the flying buttresses' outer piers
ARCH_SPRING, ARCH_CROWN = 14.0, 20.0
RIB_FOOT, RIB_TOP = 10.0, 22.0   # the cage: foot radius, crown height
OCULUS = 4.2           # the well's radius
VENT_R, VENT_AT = 2.0, 22.0      # the vents: radius, distance from the centre
VENTS = (225.0, 45.0)  # their bearings (degrees from +X toward +Z)
C22 = math.cos(math.pi/8)


def octagon(center, apothem, sides=8, start=None):
    """Corners (x, z) of a regular octagon whose faces look along 0, 45, ...
    degrees, `apothem` from `center` to each face."""
    r = apothem/C22
    a = np.radians(22.5+45.0*np.arange(8)) if start is None else np.radians(start+45.0*np.arange(8))
    return np.c_[center[0]+r*np.cos(a), center[1]+r*np.sin(a)]


def edge_radius(theta, apothem):
    """Distance from the centre to the octagon's edge at bearing `theta`
    (radians)."""
    off = (theta+math.pi/8) % (math.pi/4)-math.pi/8
    return apothem/np.cos(off)


def yrev(profile, sides, material, solid=True, at=(0.0, 0.0, 0.0), start=0.0):
    """A lathe about +y (profile [(r, y), ...]) whose first corner is at
    `start` degrees from +X toward +Z."""
    form = A.lathe(profile, sides=sides, material=material, smooth=False, solid=solid, arc=(start, start+360.0))
    v = form.v[:, [0, 2, 1]]+np.asarray(at, np.float64)
    out = A.Form(v, form.f[:, ::-1], material, False, solid)
    A._orient(out)
    return out


def octo_ring(center, a0, a1, y0, y1, material, solid=True, top=None):
    """An octagonal annulus prism between apothems a0 and a1, from y0 to y1
    (or, with `top` (ya, yb), its top sloping from ya at a0 to yb at a1)."""
    inner, outer = octagon(center, a0), octagon(center, a1)
    v, f = [], []
    yt0, yt1 = (y1, y1) if top is None else top
    for k in range(8):
        v += [(inner[k][0], y0, inner[k][1]), (outer[k][0], y0, outer[k][1]),
              (outer[k][0], yt1, outer[k][1]), (inner[k][0], yt0, inner[k][1])]
    for k in range(8):
        j = (k+1) % 8
        a, b, c, d = 4*k, 4*k+1, 4*k+2, 4*k+3
        e, g, h, i = 4*j, 4*j+1, 4*j+2, 4*j+3
        f += [(d, c, h), (d, h, i)]        # top
        f += [(a, e, g), (a, g, b)]        # bottom
        f += [(b, g, h), (b, h, c)]        # outside
        f += [(a, d, i), (a, i, e)]        # inside
    form = A.Form(np.array(v, np.float64), f, material, False, solid)
    return outward(form, center)


def outward(form, center):
    """Each face of a ring-shaped solid turned to look out of it: tops up,
    bottoms down, outer walls away from `center` (x, z), inner walls toward
    it."""
    t = form.v[form.f]
    n = np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0])
    mid = t.mean(1)
    radial = np.c_[mid[:, 0]-center[0], np.zeros(len(mid)), mid[:, 2]-center[1]]
    flat = np.abs(n[:, 1]) > .5*np.linalg.norm(n, axis=1)
    ys = t[:, :, 1]
    top = flat & (np.isclose(ys, ys.max(1, keepdims=True)).all(1)) & (mid[:, 1] > form.v[:, 1].min()+1e-6)
    # Walls: outer if farther out than the ring's middle radius.
    r = np.hypot(radial[:, 0], radial[:, 2])
    rmid = np.median(np.hypot(form.v[:, 0]-center[0], form.v[:, 2]-center[1]))
    want = np.where(flat, np.where(mid[:, 1] > (form.v[:, 1].min()+form.v[:, 1].max())/2, 1.0, -1.0), 0.0)
    flip = np.where(flat, n[:, 1]*want < 0, ((n*radial).sum(1) < 0) == (r >= rmid))
    form.f[flip] = form.f[flip][:, ::-1]
    return form


def platform(center, carve):
    """The floor slab: an octagon of apothem APOTHEM with the well's round
    hole, closed underneath, as a radial grid (rings x 64 bearings), less
    the vents (`carve(tris, polygon, y0, y1)`)."""
    cx, cz = center
    n, rings = 64, 6
    th = np.linspace(0, math.tau, n, endpoint=False)
    outer = edge_radius(th, APOTHEM)
    pts_top, pts_bot = [], []
    for i in range(rings+1):
        t = i/rings
        r = OCULUS+(outer-OCULUS)*t
        pts_top.append(np.c_[cx+r*np.cos(th), np.full(n, TOP), cz+r*np.sin(th)])
        pts_bot.append(np.c_[cx+r*np.cos(th), np.full(n, TOP-THICK), cz+r*np.sin(th)])
    tris = []
    for i in range(rings):
        for j in range(n):
            k = (j+1) % n
            a, b, c, d = pts_top[i][j], pts_top[i][k], pts_top[i+1][k], pts_top[i+1][j]
            tris += [(a, c, b), (a, d, c)]                       # up
            a, b, c, d = pts_bot[i][j], pts_bot[i][k], pts_bot[i+1][k], pts_bot[i+1][j]
            tris += [(a, b, c), (a, c, d)]                       # down
    for j in range(n):
        k = (j+1) % n
        a, b, c, d = pts_bot[rings][j], pts_bot[rings][k], pts_top[rings][k], pts_top[rings][j]
        tris += [(a, b, c), (a, c, d)]                           # the rim, out
        a, b, c, d = pts_bot[0][j], pts_bot[0][k], pts_top[0][k], pts_top[0][j]
        tris += [(a, c, b), (a, d, c)]                           # the well, in
    tris = np.array(tris, np.float64)
    # Faces must look out of the slab: up on top, down underneath.
    n_ = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    mid = tris.mean(1)
    radial = np.c_[mid[:, 0]-cx, np.zeros(len(mid)), mid[:, 2]-cz]
    want = np.where(np.abs(mid[:, 1]-TOP) < 1e-6, 1.0, np.where(np.abs(mid[:, 1]-(TOP-THICK)) < 1e-6, -1.0, 0.0))
    side = (n_*radial).sum(1)
    flip = np.where(want != 0, n_[:, 1]*want < 0,
                    np.where(np.hypot(radial[:, 0], radial[:, 2]) < OCULUS+.5, side > 0, side < 0))
    tris[flip] = tris[flip][:, ::-1]
    for bearing in VENTS:
        b = math.radians(bearing)
        poly = np.c_[cx+VENT_AT*math.cos(b)+VENT_R*np.cos(np.linspace(0, math.tau, 24, endpoint=False)),
                     cz+VENT_AT*math.sin(b)+VENT_R*np.sin(np.linspace(0, math.tau, 24, endpoint=False))]
        tris = carve(tris, poly, TOP-THICK-.1, TOP+.1)
    return A.Form(tris.reshape(-1, 3), np.arange(len(tris)*3).reshape(-1, 3), 'concrete', False, True)


def spire(x, z, face):
    """An obsidian spire on a stepped plinth at (x, z) on the floor, its
    broad faces square to `face` (radians), with bronze bands, glowing
    seams and a bronze tip. Returns parts and the tip."""
    y = TOP
    parts = [(yrev([(0, y), (2.0, y), (2.0, y+.7), (1.75, y+.9), (1.75, y+2.2), (1.45, y+2.45), (0, y+2.45)], 8,
                   'concrete', True, (x, 0, z), math.degrees(face)+22.5), 'baked')]
    y0 = y+2.45
    shaft = [(0, y0), (1.45, y0), (1.25, y0+9.0), (.8, y0+20.0), (.42, y0+SPIRE_TALL-4.6), (0, y0+SPIRE_TALL-4.4)]
    parts.append((yrev(shaft, 4, 'leaf', True, (x, 0, z), math.degrees(face)+45.0), 'baked'))
    for yb in (y0+3.0, y0+12.0):
        r = 1.15-(yb-y0)/8.0*.15 if yb < y0+8 else 1.0-(yb-y0-8.0)/9.0*.38
        parts.append((yrev([(r-.04, yb), (r+.1, yb), (r+.1, yb+.35), (r-.04, yb+.35)], 4, 'trim', False, (x, 0, z),
                           math.degrees(face)+45.0), 'baked'))
    tip_base = y0+SPIRE_TALL-4.6
    parts.append((yrev([(0, tip_base-.3), (.55, tip_base-.3), (.5, tip_base+.4), (.18, tip_base+2.6), (0, TOP+SPIRE_TALL)],
                       6, 'trim', False, (x, 0, z), 0.0), 'baked'))
    # Glowing seams up the outer face.
    c, s = math.cos(face), math.sin(face)
    for k, (ya, yb) in enumerate(((y0+4.0, y0+11.0), (y0+13.0, y0+16.5))):
        w = 1.08-(ya-y0)/8.0*.15 if ya < y0+8 else .95-(ya-y0-8.0)/9.0*.36
        px, pz = x+c*w, z+s*w
        parts.append((A.box((px-abs(s)*.07-.02, ya, pz-abs(c)*.07-.02), (px+abs(s)*.07+.02, yb, pz+abs(c)*.07+.02),
                            'light', False), 'baked'))
    return parts, (x, TOP+SPIRE_TALL, z)


def pointed_arch(a, b, spring, crown, r, material, solid=True, n=13, sides=6):
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    pts = []
    for t in np.linspace(0, 1, n):
        g = (1-abs(2*t-1))**.55
        xz = a+(b-a)*t
        pts.append((xz[0], spring+(crown-spring)*g, xz[1]))
    return A.tube(pts, [r]*len(pts), sides=sides, material=material, solid=solid)


def build(center, carve, seed=0):
    """Every part of the temple round `center` (x, z). `carve` is
    `ashveil_undercroft.carve`. Returns {'parts': [(form, lit)], 'lamps',
    'smoke', 'spires': the eight tips (x, y, z), 'vents': [(x, z, r)]}."""
    cx, cz = center
    parts, lamps, smoke = [], [], []

    def add(forms, lit='baked'):
        parts.extend((f, lit) for f in (forms if isinstance(forms, list) else [forms]))

    add(platform(center, carve))
    # A bronze lip round the floor's edge, and the steps down all round
    # (drawn), with a smooth ramp under them to walk on (hidden).
    add(octo_ring(center, APOTHEM-.25, APOTHEM+.02, TOP-.02, TOP+.04, 'trim', False))
    for k in range(STEPS):
        a0, a1 = APOTHEM+k*STEP_RUN, APOTHEM+(k+1)*STEP_RUN
        y1 = TOP-(k+1)*(1.0/STEPS)
        add(octo_ring(center, a0-.01, a1, TOP-THICK-1.0, y1, 'concrete', False))
    add(octo_ring(center, APOTHEM, APOTHEM+STEPS*STEP_RUN, TOP-THICK-1.0, TOP, 'concrete', True,
                  top=(TOP, TOP-1.0)), 'hidden')

    # Floor inlay: rings and a star of bronze and glow.
    for r0, r1, mat in ((OCULUS+.6, OCULUS+.75, 'trim'), (9.0, 9.18, 'trim'), (14.0, 14.12, 'light'), (17.6, 17.78, 'trim')):
        add(yrev([(r0, TOP), (r1, TOP), (r1, TOP+.012), (r0, TOP+.012), (r0, TOP)], 48, mat, False, (cx, 0, cz)))
    for k in range(16):
        a = math.radians(k*22.5)
        r_end = 17.6 if k % 2 == 0 else 14.0
        c, s = math.cos(a), math.sin(a)
        w = .14 if k % 2 == 0 else .08
        quad = [(cx+c*9.18-s*w, cz+s*9.18+c*w), (cx+c*r_end-s*.02, cz+s*r_end+c*.02),
                (cx+c*r_end+s*.02, cz+s*r_end-c*.02), (cx+c*9.18+s*w, cz+s*9.18-c*w)]
        add(A.extrude(quad, TOP, TOP+.014, 'trim' if k % 2 == 0 else 'light', False, axis='y'))

    # The spires and the ring of arches between them.
    tips, feet = [], []
    for k in range(8):
        a = math.radians(22.5+45*k)
        x, z = cx+SPIRE_R*math.cos(a), cz+SPIRE_R*math.sin(a)
        p, tip = spire(x, z, a)
        add([f for f, _ in p])
        tips.append(tip); feet.append((x, z))
    for k in range(8):
        (ax, az), (bx, bz) = feet[k], feet[(k+1) % 8]
        da = np.array([bx-ax, bz-az]); da /= np.linalg.norm(da)
        a = (ax+da[0]*1.0, az+da[1]*1.0); b = (bx-da[0]*1.0, bz-da[1]*1.0)
        add(pointed_arch(a, b, TOP+ARCH_SPRING, TOP+ARCH_CROWN, .55, 'leaf'))
        add(pointed_arch(a, b, TOP+ARCH_SPRING+.5, TOP+ARCH_CROWN+.45, .16, 'trim', solid=False, sides=5))

    # Flying buttresses: from each spire, half way up, down and out to a
    # pier standing on the dome beyond the steps.
    for k in range(8):
        a = math.radians(22.5+45*k)
        c, s_ = math.cos(a), math.sin(a)
        px, pz = cx+PIER_R*c, cz+PIER_R*s_
        add(yrev([(0, TOP-2.5), (1.7, TOP-2.5), (1.7, TOP-.5), (1.4, TOP-.2), (1.1, TOP+6.0), (.8, TOP+7.2), (0, TOP+8.4)],
                 4, 'leaf', True, (px, 0, pz), math.degrees(a)+45.0))
        add(yrev([(1.12, TOP+2.0), (1.32, TOP+2.0), (1.28, TOP+2.4), (1.1, TOP+2.4)], 4, 'trim', False, (px, 0, pz),
                 math.degrees(a)+45.0))
        pts, radii = [], []
        for t in np.linspace(0, 1, 12):
            r = SPIRE_R+1.0+(PIER_R-1.2-SPIRE_R-1.0)*t
            y = TOP+18.0-(18.0-6.0)*t**1.6+2.2*math.sin(t*math.pi)
            pts.append((cx+r*c, y, cz+r*s_)); radii.append(.55-.15*t)
        add(A.tube(pts, radii, sides=6, material='leaf', solid=True))

    # The cage over the well: short piers, eight ribs to a crown ring, and
    # an ember hanging from it.
    crown_r, crown_y = 1.9, TOP+RIB_TOP
    for k in range(8):
        a = math.radians(45*k)
        c, s = math.cos(a), math.sin(a)
        px, pz = cx+RIB_FOOT*c, cz+RIB_FOOT*s
        add(yrev([(0, TOP), (.95, TOP), (.95, TOP+.5), (.7, TOP+.7), (.7, TOP+2.6), (.85, TOP+2.9), (0, TOP+2.9)], 8,
                 'concrete', True, (px, 0, pz), math.degrees(a)+22.5))
        pts, radii = [], []
        for t in np.linspace(0, 1, 14):
            r = RIB_FOOT+(crown_r-RIB_FOOT)*(1-math.cos(t*math.pi/2))
            y = TOP+2.9+(crown_y-TOP-2.9)*math.sin(t*math.pi/2)**.9
            pts.append((cx+r*c, y, cz+r*s)); radii.append(.34-.12*t)
        add(A.tube(pts, radii, sides=6, material='trim', solid=True))
    add(yrev([(crown_r-.3, crown_y-.35), (crown_r+.3, crown_y-.35), (crown_r+.3, crown_y+.35), (crown_r-.3, crown_y+.35),
              (crown_r-.3, crown_y-.35)], 16, 'trim', True, (cx, 0, cz)))
    # A lantern of bronze bars on the crown, and a spike over it.
    for k in range(8):
        a = math.radians(45*k+22.5)
        add(A.tube([(cx+1.5*math.cos(a), crown_y+.3, cz+1.5*math.sin(a)), (cx+1.1*math.cos(a), crown_y+3.4, cz+1.1*math.sin(a))],
                   [.12, .1], sides=4, material='trim', solid=False))
    add(yrev([(0, crown_y+3.3), (1.4, crown_y+3.3), (1.4, crown_y+3.7), (.5, crown_y+4.4), (.18, crown_y+8.5), (0, crown_y+9.5)],
             8, 'trim', True, (cx, 0, cz)))
    add(yrev([(0, crown_y+.4), (.75, crown_y+.9), (.75, crown_y+2.6), (0, crown_y+3.1)], 8, 'light', False, (cx, 0, cz)))
    lamps.append(((cx, crown_y+1.8, cz), 2.0))
    add(A.tube([(cx, crown_y, cz), (cx, crown_y-6.0, cz)], [.06, .06], sides=4, material='panel', solid=False))
    add(yrev([(0, crown_y-7.6), (.6, crown_y-7.3), (.78, crown_y-6.8), (.6, crown_y-6.3), (0, crown_y-6.0)], 10,
             'light', False, (cx, 0, cz)))
    lamps.append(((cx, crown_y-6.8, cz), 2.4))

    # Braziers on the diagonals between the cage and the spires.
    for k in range(4):
        a = math.radians(45+90*k)
        add(TW.brazier((cx+13.0*math.cos(a), TOP, cz+13.0*math.sin(a)), smoke, lamps, 1.3))

    vents = [(cx+VENT_AT*math.cos(math.radians(b)), cz+VENT_AT*math.sin(math.radians(b)), VENT_R) for b in VENTS]
    return dict(parts=parts, lamps=lamps, smoke=smoke, spires=sorted(tips), vents=vents)
