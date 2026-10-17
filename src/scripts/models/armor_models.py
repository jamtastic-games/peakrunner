"""PeakRunner's armors as hard-surface models (docs/model-pipeline.md, "Armor";
look: docs/art-direction.md), built in Blender headless:

    blender -b -P scripts/models/armor_models.py -- OUT.glb [--size small|medium|large] [--texture 2048]

(`sh scripts/models/build.sh SOURCES armor/medium` runs it for one size: the
size is the output's folder.)

One file per body size, a part per bone, each made in its bone's frame (the
pipeline's table): `helmet` (the head), `torso` and `pack` (the chest),
`pelvis`, `upper_arm_l/_r`, `forearm_l/_r`, `thigh_l/_r`, `shin_l/_r` (limb
frames: origin at the upper joint, -Y down the limb, X the body's right) and
`boot_l/_r` (the foot). A left piece is its right piece mirrored. Every piece
stays inside the box the previous generated armor was fitted to, and the
gloves (`glove_l/_r`, on the hand frames: origin at the wrist, -Y along the
hand, the palm toward -Z) inside the built-in glove's. `grip_l` and
`grip_r` are the hands on a weapon (the support hand on its left side, the
firing hand round its pistol grip), in the weapon's frame at the hand, in
first and third person; the points `wrist_grip_l/_r` are where each one's
cuff meets the forearm.

The design follows the first concept round (the
closed-visor lineup): faceted, chamfered plates over a quilted charcoal
undersuit; team paint on the chest plate, pauldrons, thighs and helmet crown;
a closed helmet with a cyan visor band and a vented faceplate; a jetpack
with one, two or four nozzles. Named points: `suit` (the undersuit is the
model's: the game draws none), `visor` (the helmet is closed: no face) and
`nozzle_0`... (where each jet flame leaves, in the chest frame).
"""
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import Solid, circle_y, finish, loft, root, setup, solid_box

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
# The size: --size, else the folder the file goes in (assets/models/armor/<size>/).
SIZE = args[args.index('--size')+1] if '--size' in args else os.path.basename(os.path.dirname(os.path.abspath(OUT)))
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 2048
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.002, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.FLAT_TEXEL = .35
hs.GRATE = (.02, .45)
hs.SPREAD = 2.0              # the parts overlap round their bone origins: bake them apart
hs.PBR = True                # metal, roughness, occlusion and normal maps for the model shader

# Per size: the body's bulk (torso width and depth, limb thickness), plate
# thickness, and the pieces' own measures. Medium is the base; the large is
# a wall, the small slim.
SIZES = {
    'small': dict(body=.9, depth=.92, limb=.88, plate=.85, helmet=(.140, .150, .158, .06, .335), neck=.10, collar=0,
                  pauldron=(.095, .31, .47), pack=(.135, .2, .54, .11, .27), nozzles=[0.0], canisters=[], abs=2,
                  antenna=True, heavy=0, glove=.92, boot=(.096, -.215, .10, .092)),
    'medium': dict(body=1.0, depth=1.0, limb=1.0, plate=1.0, helmet=(.152, .160, .168, .05, .345), neck=.11, collar=1,
                   pauldron=(.125, .33, .49), pack=(.18, .15, .58, .12, .31), nozzles=[-.09, .09], canisters=[-.16, .16],
                   abs=3, antenna=False, heavy=0, glove=1.0, boot=(.108, -.235, .115, .102)),
    'large': dict(body=1.2, depth=1.12, limb=1.18, plate=1.3, helmet=(.160, .166, .176, .045, .335), neck=.135, collar=2,
                  pauldron=(.165, .37, .53), pack=(.28, .08, .68, .16, .42), nozzles=[-.2, -.068, .068, .2],
                  canisters=[-.25, -.11, .11, .25], abs=3, antenna=False, heavy=1, glove=1.18, boot=(.128, -.26, .13, .122)),
}
P = SIZES[SIZE]
SUIT = 'recess'
BEVEL = .004


class Body:
    """A surface round a vertical axis through (cx, cz): rings (y, rx,
    rz_front, rz_back), linear between them. Angle 0 is the front (-Z), 90
    the +X side."""
    def __init__(self, rings, cx=0.0, cz=0.0):
        self.rings, self.cx, self.cz = sorted(rings), cx, cz

    def radii(self, y):
        r = self.rings
        if y <= r[0][0]: return r[0][1:]
        if y >= r[-1][0]: return r[-1][1:]
        for (y0, *a), (y1, *b) in zip(r, r[1:]):
            if y0 <= y <= y1:
                t = (y-y0)/(y1-y0)
                return tuple(p+(q-p)*t for p, q in zip(a, b))

    def point(self, a, y, off=0.0):
        rx, rf, rb = self.radii(y)
        s, c = math.sin(a), math.cos(a)
        return (self.cx+s*(rx+off), y, self.cz-c*((rf if c > 0 else rb)+off))


class XBody:
    """A surface round an axis along X through (cy, cz): rings (x, r).
    Angle 0 is the front (-Z), 90 the top."""
    def __init__(self, rings, cy, cz=0.0):
        self.rings, self.cy, self.cz = sorted(rings), cy, cz

    def point(self, a, x, off=0.0):
        r = self.rings
        if x <= r[0][0]: rr = r[0][1]
        elif x >= r[-1][0]: rr = r[-1][1]
        else:
            for (x0, a0), (x1, a1) in zip(r, r[1:]):
                if x0 <= x <= x1: rr = a0+(a1-a0)*(x-x0)/(x1-x0); break
        return (x, self.cy+math.sin(a)*(rr+off), self.cz-math.cos(a)*(rr+off))


def wrap(name, body, a0, a1, n, y0, y1, off, t, mat, edge='steel', inner='hidden', bevel=BEVEL, rows=4):
    """A plate round `body` from angle a0 to a1 (degrees) in n facets, from y0
    to y1 (numbers, or functions of the angle) in `rows` bands that follow the
    body's curve, its face `off` out, `t` thick. A plate of many facets is a
    curved one: shaded smooth across them, its edges still crisp."""
    angs = [a0+(a1-a0)*i/n for i in range(n+1)]
    f0 = y0 if callable(y0) else (lambda a: y0)
    f1 = y1 if callable(y1) else (lambda a: y1)

    def section(f):
        return ([body.point(math.radians(a), f(a), off) for a in angs]
                + [body.point(math.radians(a), f(a), off-t) for a in reversed(angs)])
    s = Solid(name, bevel, smooth=32 if n >= 10 else 1.0)

    def mats(k, i):
        if k < n: return mat
        if k == n or k == 2*n+1: return edge
        return inner
    bands = [section(lambda a, i=i: f0(a)+(f1(a)-f0(a))*i/rows) for i in range(rows+1)]
    loft(s, bands, mats, caps=(edge, edge))
    return hs.flat(s, mat)


def tube(name, body, ys, n=12, quilt=0.0, caps=('recess', 'recess'), mat=SUIT):
    """The undersuit round `body` through heights `ys`, every other ring
    pulled in by `quilt` (the quilting's seams)."""
    rings = [[body.point(2*math.pi*(k+.5)/n, y, -quilt if i % 2 else 0.0) for k in range(n)] for i, y in enumerate(ys)]
    s = Solid(name, 0, smooth=60)
    loft(s, rings, mat, caps=caps)
    return s


def steps(y0, y1, d):
    n = max(1, round(abs(y1-y0)/d))
    return [y0+(y1-y0)*i/n for i in range(n+1)]


def mirror(src, dst):
    """Copies of root `src`'s solids under root `dst`, mirrored across x = 0."""
    i = hs.ROOTS.index(src)
    root(dst)
    for s in [s for s in hs.SOLIDS if s.root == i]:
        m = Solid(s.name, s.bevel, s.smooth)
        m.v = [(-x, y, z) for x, y, z in s.v]
        m.f = [list(reversed(f)) for f in s.f]
        m.m = list(s.m)
        m.hidden = set(s.hidden)
        m.texel = s.texel


class Shape(Body):
    """A Body whose radii ease between its rings (Catmull-Rom) instead of
    running straight, for the undersuit's rounded forms."""
    def radii(self, y):
        r = self.rings
        if y <= r[0][0]: return r[0][1:]
        if y >= r[-1][0]: return r[-1][1:]
        k = max(i for i in range(len(r)-1) if r[i][0] <= y)
        p = lambda i: r[min(max(i, 0), len(r)-1)]
        (y0, *a), (y1, *b) = p(k), p(k+1)
        am, bp = p(k-1)[1:], p(k+2)[1:]
        t = (y-y0)/(y1-y0)
        t2, t3 = t*t, t*t*t
        return tuple(.5*((2*q1)+(-q0+q2)*t+(2*q0-5*q1+4*q2-q3)*t2+(-q0+3*q1-3*q2+q3)*t3)
                     for q0, q1, q2, q3 in zip(am, a, b, bp))


# The undersuit's forms in each bone's frame (y along the bone: the torso's
# from the pelvis point, a limb's down from its upper joint), rings of
# (y, side radius, front radius, back radius): hips and seat, a waist, a
# broad ribcage and chest, trapezius into the neck; a thigh thick at the top
# tapering to the knee, a calf, a slim ankle; a deltoid, an elbow and a
# forearm that narrows to the wrist. Scaled by the size's bulk.
TORSO = [(-.21, .115, .080, .090), (-.15, .160, .102, .116), (-.07, .176, .108, .122), (.01, .168, .100, .112),
         (.09, .152, .097, .104), (.17, .160, .107, .107), (.26, .182, .121, .114), (.35, .200, .131, .119),
         (.43, .204, .126, .118), (.49, .188, .110, .110), (.54, .140, .086, .090), (.58, .076, .068, .070),
         (.65, .062, .058, .062), (.69, .058, .055, .060)]
LEG = [(.10, .084, .084, .094), (.02, .097, .097, .107), (-.06, .100, .100, .104), (-.16, .094, .098, .095),
       (-.26, .085, .090, .085), (-.36, .072, .080, .072), (-.44, .064, .072, .064), (-.50, .066, .064, .074),
       (-.58, .068, .060, .082), (-.68, .060, .056, .070), (-.78, .048, .048, .054), (-.86, .042, .044, .046),
       (-.90, .042, .044, .046)]
ARM = [(.07, .055, .055, .055), (.02, .066, .064, .064), (-.05, .068, .062, .062), (-.13, .060, .058, .060),
       (-.22, .056, .056, .058), (-.30, .050, .050, .052), (-.35, .052, .052, .054), (-.42, .055, .050, .052),
       (-.52, .046, .042, .044), (-.60, .038, .034, .036), (-.63, .036, .032, .034)]


def shape(rings, kx, kz, dy=0.0):
    return Shape([(y+dy, rx*kx, rf*kz, rb*kz) for y, rx, rf, rb in rings])


def torso_shape():
    return shape(TORSO, P['body'], P['depth'])


def leg_shape():
    return shape(LEG, P['limb'], P['limb'])


def arm_shape(dy=0.0):
    """The arm's form down from the shoulder; `dy` .32 puts it in the
    forearm's frame (from the elbow)."""
    return shape(ARM, P['limb'], P['limb'], dy)


# The engine's skeleton (player_model::pose) at rest: the pelvis point, the
# neck over it, shoulders, hips, and limb lengths; the arms hang out a
# little (an A pose) so the suit's armpits stay open.
PELVIS_Y, NECK, SHOULDER, HIP = .95, .58, (.30, .47), (.16, -.05)
THIGH = SHIN = .44
UPPER, FORE = .32, .30
A_POSE = math.radians(14)


def translate(x, y, z):
    m = np.eye(4)
    m[:3, 3] = (x, y, z)
    return m


def limb_frame(a, b, side, rest):
    """player_model::limb_frame: the origin at `a`, +Y back up the limb from
    `b`, X as near `side` as it can be, Z behind (its stretch is 1 at rest)."""
    a, b, side = (np.array(v, np.float64) for v in (a, b, side))
    y = (a-b)/np.linalg.norm(a-b)
    x = side-y*np.dot(side, y)
    x /= np.linalg.norm(x)
    m = np.eye(4)
    m[:3, 0], m[:3, 1], m[:3, 2], m[:3, 3] = x, y, np.cross(x, y), a
    return m


def skeleton():
    """Each bone's rest frame (the file's coordinates), by side (-1 left)."""
    bones = {'pelvis': translate(0, PELVIS_Y, 0), 'chest': translate(0, PELVIS_Y, 0), 'head': translate(0, PELVIS_Y+NECK, 0)}
    for sx, tag in ((-1, 'l'), (1, 'r')):
        d = np.array([sx*math.sin(A_POSE), -math.cos(A_POSE), 0])
        shoulder = np.array([sx*SHOULDER[0], PELVIS_Y+SHOULDER[1], 0])
        elbow = shoulder+d*UPPER
        bones[f'upper_arm_{tag}'] = limb_frame(shoulder, elbow, (1, 0, 0), UPPER)
        bones[f'forearm_{tag}'] = limb_frame(elbow, elbow+d*FORE, (1, 0, 0), FORE)
        hip = np.array([sx*HIP[0], PELVIS_Y+HIP[1], 0])
        knee, ankle = hip-[0, THIGH, 0], hip-[0, THIGH+SHIN, 0]
        bones[f'thigh_{tag}'] = limb_frame(hip, knee, (1, 0, 0), THIGH)
        bones[f'shin_{tag}'] = limb_frame(knee, ankle, (1, 0, 0), SHIN)
    return bones


def ease(a, b, x):
    t = min(max((x-a)/(b-a), 0.0), 1.0)
    return t*t*(3-2*t)


def suit_tube(name, body, ys, n, frame, weigh, mirror=1):
    """A length of the undersuit round `body` (in a bone's frame, mirrored
    across x for the left side), placed at rest by `frame`, each ring's
    points weighted to the bones by `weigh(point)`."""
    s = Solid(name, 0, smooth=60)
    rings = []
    for y in ys:
        local = [body.point(2*math.pi*(k+.5)/n, y) for k in range(n)]
        if mirror < 0:
            local = [(-x, yy, z) for x, yy, z in reversed(local)]
        rings.append([(p, tuple((frame @ np.array([*p, 1.0]))[:3])) for p in local])
    loft(s, [[q for _, q in r] for r in rings], SUIT, caps=(SUIT, SUIT))
    s.weights = [weigh(p) for r in rings for p, _ in r]
    return s


def suit():
    """The undersuit, one skinned piece: the torso blended from the hips to
    the chest across the waist and into the head at the neck, its shoulders
    following the arms a little; each arm from the shoulder (inside the
    pauldron) to the wrist, bending at the elbow; each leg from inside the
    hips to the ankle (inside the boot), bending at the knee."""
    bones = skeleton()
    hs.JOINTS[:] = [(f'bone_{name}', frame) for name, frame in bones.items()]

    def torso_weights(p):
        x, y, _ = p
        chest = ease(.03, .19, y)
        head = ease(.55, .63, y)
        arm = ease(.13, .2*P['body'], abs(x))*ease(.30, .42, y)*(1-ease(.50, .55, y))*.4
        w = {'bone_pelvis': 1-chest, 'bone_chest': chest*(1-head)*(1-arm), 'bone_head': head}
        w['bone_upper_arm_l' if x < 0 else 'bone_upper_arm_r'] = chest*(1-head)*arm
        return w
    suit_tube('suit-torso', torso_shape(), steps(-.21, .69, .03), 28, bones['chest'], torso_weights)
    for sx, tag in ((-1, 'l'), (1, 'r')):
        def leg_weights(p, tag=tag):
            t = -p[1]
            thigh, shin = ease(-.07, .07, t), ease(.40, .49, t)
            return {'bone_pelvis': 1-thigh, f'bone_thigh_{tag}': thigh*(1-shin), f'bone_shin_{tag}': shin}
        suit_tube('suit-leg', leg_shape(), steps(.10, -.90, .025), 20, bones[f'thigh_{tag}'], leg_weights, sx)

        def arm_weights(p, tag=tag):
            t = -p[1]
            upper, fore = ease(-.06, .05, t), ease(.28, .37, t)
            return {'bone_chest': 1-upper, f'bone_upper_arm_{tag}': upper*(1-fore), f'bone_forearm_{tag}': fore}
        suit_tube('suit-arm', arm_shape(), steps(.07, -.63, .025), 18, bones[f'upper_arm_{tag}'], arm_weights, sx)


# --- Helmet (the head: origin at the neck, the face toward -Z) ------------------

def helmet():
    W, F, B, y0, y1 = P['helmet']
    H = y1-y0
    Y = lambda t: y0+t*H
    T = (0, .1, .36, .6, .8, .93, 1.0)
    shell = Body([(Y(t), w*W, f*F, b*B) for t, (w, f, b) in zip(T, ((.74, .78, .84), (.86, .9, .93), (1, 1, 1), (1, .99, 1),
                                                                     (.9, .86, .93), (.66, .58, .74), (.36, .28, .44)))], cz=.005)
    # The shell is round (smooth, finely divided); the plates on it are flat
    # facets with crisp edges.
    s = Solid('shell', 0, smooth=55)
    T = (0, .1, .2, .3, .36, .45, .55, .6, .7, .8, .87, .93, .97, 1.0)
    shell = Body([(Y(t), w*W, f*F, b*B) for t, (w, f, b) in zip(T, (
        (.74, .78, .84), (.86, .9, .93), (.94, .96, .98), (.99, .995, .998), (1, 1, 1), (1.002, 1.0, 1.0), (1, .995, 1),
        (1, .99, 1), (.96, .94, .97), (.9, .86, .93), (.81, .75, .86), (.66, .58, .74), (.5, .42, .58), (.36, .28, .44)))], cz=.005)
    rings = [[shell.point(2*math.pi*(k+.5)/32, Y(t)) for k in range(32)] for t in T]
    loft(s, rings, 'hull', caps=('recess', ('tip', (0.0, y1, .0), 'hull')))
    # The visor: a dark band round the front with a lit line in it.
    wrap('visor', shell, -66, 66, 16, Y(.40), Y(.56), .007, .014, 'glass', edge='plate')
    wrap('visor-light', shell, -58, 58, 16, Y(.455), Y(.50), .02, .009, 'glow', edge='glow')
    # The faceplate under it: vented, its corners swept up to the cheeks.
    wrap('faceplate', shell, -46, 46, 6, lambda a: Y(.04+.1*abs(a)/46), Y(.37), .012, .02, 'plate')
    for t in (.12, .19, .26):
        wrap('vent', shell, -18, 18, 3, Y(t), Y(t+.03), .034, .008, 'grille', edge='recess', bevel=0)
    # The chin guard juts forward under the vents.
    wrap('chin', shell, -26, 26, 4, Y(-.02), Y(.1), .032, .03, 'plate')
    # Cheek plates and the crown in team paint, a brow over the visor.
    for a0, a1 in ((52, 94), (-94, -52)):
        wrap('cheek', shell, a0, a1, 3, Y(.12), Y(.40), .01, .016, 'team')
    wrap('crown', shell, -54, 54, 10, Y(.68), Y(.92), .006, .014, 'team')
    if SIZE != 'small':
        wrap('brow', shell, -68, 68, 12, Y(.565), Y(.64), .014, .022, 'plate')
    # Ear pods with a light, an antenna on the small one.
    for sx in (-1, 1):
        x = sx*(W+.004)
        lo, hi = sorted((x-sx*.02, x+sx*.022))
        solid_box('ear', (lo, Y(.28), -.035), (hi, Y(.56), .045), {'*': 'plate', '-x' if sx < 0 else '+x': 'hull'}, bevel=.006)
        xf = hi if sx > 0 else lo
        hs.quad_plate((xf, Y(.36), -.012), (xf, Y(.36), .022), (xf, Y(.48), .022), (xf, Y(.48), -.012), 0, 1, 0, 1,
                      'glow', h=.004, away=(0.0, Y(.42), 0.0), name='ear-light')
    if P['antenna']:
        x = W*.82
        hs.beam('antenna', (x, Y(.55), .05), (x+.01, y1+.13, .085), .016, .016, 'steel', bevel=.003)
        hs.beam('antenna-tip', (x+.0095, y1+.12, .084), (x+.01, y1+.15, .086), .022, .022, 'glow', bevel=0)
    # The back: a rear plate with a lit sensor eye in it.
    wrap('rear-plate', shell, 150, 210, 6, Y(.3), Y(.62), .008, .016, 'plate')
    wrap('rear-eye', shell, 174, 186, 2, Y(.44), Y(.5), .03, .01, 'glow', edge='steel')
    hs.POINTS.append(('visor', (0.0, 0.0, 0.0)))


# --- Torso and pack (the chest frame: origin at the pelvis point) -------------

def torso():
    core = torso_shape()
    t = P['plate']
    span = 70 if SIZE == 'small' else 76
    corner = lambda a: max(0.0, abs(a)/span-.5)/.5
    # The chest: a gunmetal frame and over it the team plate, a broad
    # shield whose lower edge comes to a V under the sternum and whose upper
    # corners cut away toward the collar; a ridge down its middle, lights
    # either side and a light bar under its point.
    wrap('chest-frame', core, -span-10, span+10, 14, lambda a: .19+.07*abs(a)/(span+10), lambda a: .52-.09*corner(a),
         .03*t, .02*t, 'hull', rows=5)
    wrap('chest', core, -span, span, 14, lambda a: .245+.08*abs(a)/span, lambda a: .505-.11*corner(a),
         .052*t, .024*t, 'team', rows=5)
    wrap('sternum', core, -5, 5, 2, .30, .49, .07*t, .022, 'plate')
    for a0, a1 in ((30, 44), (-44, -30)):
        wrap('chest-light', core, a0, a1, 2, .345, .358, .07*t, .012, 'glow', edge='glow')
    wrap('chest-bar', core, -12, 12, 2, .228, .242, .056*t, .01, 'glow', edge='glow')
    # The abdomen: rows of plates narrowing toward the belt.
    for i in range(P['abs']):
        y = .155-i*.052
        half = 50-6*i
        for a0, a1 in ((-half, -3), (3, half)):
            wrap('abs', core, a0, a1, 4, y, y+.044, .03*t, .02*t, 'plate')
    # Side plates over the ribs, the back plate with a spine ridge and ribs.
    for a0, a1 in ((84, 116), (-116, -84)):
        wrap('side-plate', core, a0, a1, 4, .14, .44, .026*t, .018, 'hull')
    wrap('back', core, 118, 242, 12, .16, .52, .03*t, .022*t, 'hull', rows=5)
    wrap('spine', core, 172, 188, 2, .2, .5, .052*t, .02, 'plate')
    for a0, a1 in ((146, 154), (206, 214)):
        wrap('back-rib', core, a0, a1, 1, .24, .48, .046*t, .014, 'plate')
    # Collar round the neck: low, raised, or the heavy's high ring.
    neck = P['neck']
    hi = (.60, .625, .74)[P['collar']]
    ring = Body([(.52, neck*1.25, neck*1.15, neck*1.2), (hi, neck*(1.05+.15*P['collar']), neck*(1.0+.12*P['collar']),
                                                           neck*(1.1+.12*P['collar']))])
    wrap('collar', ring, -180, 180, 16, .52, hi, 0.0, .03, 'plate', edge='hull', inner='recess')
    if P['collar'] == 2:
        wrap('collar-team', ring, -60, 60, 6, hi-.05, hi-.02, .006, .01, 'team')
    # Pauldrons: angular hoods over the shoulders (in the chest frame: they
    # stay put), team paint over a gunmetal lame that runs under their edge,
    # a rim round their front and back, a light at the front.
    r, cx, cy = P['pauldron']
    for sx in (-1, 1):
        xs = [sx*(cx-.9*r), sx*(cx-.1*r), sx*(cx+.6*r), sx*(cx+1.0*r)]
        rr = [r*.86, r*1.06, r*1.0, r*.55]
        hood = XBody(sorted(zip(xs, rr)), cy-.2*r)
        wrap('pauldron', hood, -28, 208, 16, xs[0], xs[-1], 0.0, .028*t, 'team', inner='recess', rows=6)
        lame = XBody(sorted(zip(xs, [v*.9 for v in rr])), cy-.62*r)
        wrap('pauldron-lame', lame, -32, 212, 16, xs[0], xs[-1], 0.0, .022*t, 'hull', inner='recess')
        for a0, a1 in ((-44, -22), (202, 224)):
            wrap('pauldron-rim', hood, a0, a1, 2, xs[0], xs[2], .004, .02, 'plate', inner='recess')
        mid = sorted((xs[1], xs[2]))
        wrap('pauldron-light', hood, -16, -6, 1, mid[0]+.01, mid[1]-.01, .014, .006, 'glow', edge='glow')


def pack():
    """The jetpack, seen from behind all game in third person: a housing
    wider at the shoulders than the hips, a raised spine with a team fin
    rising off its top, two lit bars across it, hoses down recessed
    channels either side to the nozzles, thruster domes at the top corners
    and lit rings in the nozzles' throats."""
    hx, y0, y1, z0, z1 = P['pack']
    zc = (z0+z1)/2
    housing = Solid('pack', .008)
    ring = lambda y, w, zb: [(x, y, z) for x, z in hs.plan(z0, zb, w, .03, .055)]
    loft(housing, [ring(y0, hx*.74, z1-.035), ring(y0+.07, hx*.92, z1-.004), ring(y1-.12, hx, z1), ring(y1-.03, hx*.96, z1-.012),
                   ring(y1, hx*.84, z1-.04)],
         lambda k, i: ['plate', 'hull', 'hull', 'plate'][i], caps=('recess', 'plate'))
    # The spine and the team fin off its top.
    sw = .036 if len(P['nozzles']) < 4 else .046
    prism_y = lambda name, ys, mats, caps: loft(Solid(name, .004), [[(x, y, z) for x, z in sec] for y, sec in ys], mats, caps=caps)
    prism_y('pack-spine', [(y0+.04, [(-sw, z1-.01), (sw, z1-.01), (sw*.8, z1+.026), (-sw*.8, z1+.026)]),
                           (y1-.01, [(-sw, z1-.01), (sw, z1-.01), (sw*.8, z1+.03), (-sw*.8, z1+.03)])],
            'plate', ('hull', 'plate'))
    fin = Solid('pack-fin', .004)
    v = fin.pts([(-sw*1.25, y1-.07, z1+.014), (sw*1.25, y1-.07, z1+.014), (0.0, y1+.11, z1-.006),
                 (-sw*1.25, y1-.07, z1-.03), (sw*1.25, y1-.07, z1-.03), (0.0, y1+.11, z1-.04)])
    for f, m in (((0, 1, 2), 'team'), ((3, 5, 4), 'team'), ((0, 3, 4, 1), 'hidden'), ((1, 4, 5, 2), 'team'), ((0, 2, 5, 3), 'team')):
        fin.face([v[i] for i in f], m)
    # Lit bars across the top, either side of the spine.
    for sx in (-1, 1):
        x0, x1 = sorted((sx*(sw+.012), sx*hx*.78))
        hs.quad_plate((x0, y1-.095, z1), (x1, y1-.095, z1), (x1, y1-.075, z1), (x0, y1-.075, z1), 0, 1, 0, 1, 'glow', h=.006,
                      away=(0, (y0+y1)/2, z0), name='pack-bar')
        # A recessed channel down the side, and a hose in it to the nozzles.
        cx = sx*(sw+.04)
        hs.quad_plate((cx-.016, y0+.08, z1), (cx+.016, y0+.08, z1), (cx+.016, y1-.13, z1), (cx-.016, y1-.13, z1), 0, 1, 0, 1,
                      'recess', h=.001, away=(0, (y0+y1)/2, z0), name='pack-channel')
        seg('pack-hose', (cx, y1-.13, z1+.004), (cx, y0+.09, z1+.004), .011, .011, 'rubber', n=8)
        seg('pack-hose', (cx, y0+.09, z1+.004), (P['nozzles'][0 if sx < 0 else -1], y0+.01, zc+.03), .011, .011, 'rubber', n=8)
        for y in (y1-.16, (y0+y1)/2, y0+.12):
            hs.beam('hose-clamp', (cx-.02, y, z1+.006), (cx+.02, y, z1+.006), .012, .008, 'steel', bevel=.0015)
        # Side panels of the housing, a light at each lower corner.
        x = sx*hx*.995
        hs.quad_plate((x, y0+.1, z0+.03), (x, y0+.1, z1-.03), (x, y1-.14, z1-.03), (x, y1-.14, z0+.03), 0, 1, 0, 1, 'plate', h=.005,
                      away=(0, (y0+y1)/2, zc), name='pack-side')
        xl0, xl1 = sorted((sx*hx*.62, sx*hx*.74))
        hs.quad_plate((xl0, y0+.03, z1-.03), (xl1, y0+.03, z1-.03), (xl1, y0+.06, z1-.012), (xl0, y0+.06, z1-.012), 0, 1, 0, 1,
                      'glow', h=.003, away=(0, (y0+y1)/2, z0), name='pack-corner-light')
    # Nozzles under it, each a bell with a lit throat ring, and where its flame leaves.
    R = .048 if len(P['nozzles']) < 4 else .052
    for k, x in enumerate(P['nozzles']):
        bell = Solid('nozzle', .004, smooth=35)
        loft(bell, [circle_y(x, zc, R*a, y, 16) for y, a in ((y0+.02, .8), (y0-.02, 1.0), (y0-.06, 1.04), (y0-.1, 1.14))],
             lambda kk, i: ['steel', 'hull', 'plate'][i], caps=('hidden', 'recess'))
        hs.ring_solid('nozzle-light', x, zc, R*.62, R*.98, y0-.106, y0-.098, 16, 'glow')
        hs.ring_solid('nozzle-band', x, zc, R*1.0, R*1.1, y0-.05, y0-.035, 16, 'hull')
        hs.POINTS.append((f'nozzle_{k}', (x, y0-.1, zc)))
    # Thruster domes at the top corners.
    for x in (P['canisters'] or [-hx*.6, hx*.6]):
        c = Solid('thruster', .004, smooth=35)
        dz = zc+.01
        loft(c, [circle_y(x, dz, .046*a, y, 16) for y, a in ((y1-.07, 1.0), (y1+.01, 1.0), (y1+.04, .82), (y1+.058, .45))],
             lambda kk, i: ['hull', 'steel', 'steel'][i], caps=('hidden', 'steel'))
        hs.ring_solid('thruster-light', x, dz, .03, .047, y1+.0, y1+.008, 16, 'glow')


# --- Pelvis (upright at the pelvis point) --------------------------------------

def pelvis():
    hips = torso_shape()
    t = P['plate']
    wrap('belt', hips, -180, 180, 20, -.035, .025, .016, .024, 'rubber', edge='rubber')
    wrap('buckle', hips, -12, 12, 2, -.03, .02, .036, .016, 'steel')
    # Pouches on the belt: blocks set square to the hips at their angles.
    for a in (48, -48, 132, -132):
        x, _, z = hips.point(math.radians(a), -.03, .036)
        c, sn = math.cos(math.radians(a)), math.sin(math.radians(a))
        pouch = Solid('pouch', .004)
        corners = [(x+sn*dr+c*dt, y, z-c*dr+sn*dt) for y in (-.085, .02) for dr, dt in ((0, -.034), (.04, -.034), (.04, .034), (0, .034))]
        v = pouch.pts(corners)
        for f, m in (((0, 1, 2, 3), 'rubber'), ((4, 7, 6, 5), 'steel'), ((0, 4, 5, 1), 'rubber'), ((1, 5, 6, 2), 'rubber'),
                     ((2, 6, 7, 3), 'rubber'), ((3, 7, 4, 0), 'hidden')):
            pouch.face([v[i] for i in f], m)
    # The groin plate, hip plates over the legs' tops and the seat (the
    # tassets over the thighs' fronts ride the thighs: `thigh`).
    wrap('groin', hips, -26, 26, 4, -.205, -.05, .026*t, .02, 'plate')
    for a0, a1 in ((62, 118), (-118, -62)):
        wrap('hip', hips, a0, a1, 5, -.18, -.01, .03*t, .02*t, 'hull')
    wrap('seat', hips, 146, 214, 6, -.17, -.04, .026*t, .018, 'hull')
    hs.POINTS.append(('suit', (0.0, 0.0, 0.0)))


# --- Limbs (origin at the upper joint, -Y down the limb, X the body's right) ---

def upper_arm():
    b = arm_shape()
    t = P['plate']
    a0, a1 = (0, 180) if P['heavy'] else (18, 162)
    wrap('arm-plate', b, a0, a1, 8, -.07, -.25, .03*t, .02*t, 'plate')
    wrap('arm-light', b, 86, 94, 1, -.11, -.21, .044*t, .006, 'glow', edge='glow')
    wrap('arm-band', b, 0, 360, 14, -.245, -.228, .012, .012, 'hull', edge='hull')


def forearm():
    """A gauntlet over most of the forearm, its lower plate stopping short
    of the wrist so the glove's cuff shows below it, an elbow cop."""
    b = arm_shape(UPPER)
    t = P['plate']
    off, th = (.034, .028) if P['heavy'] else (.028, .022)
    wrap('elbow', b, 132, 228, 4, .018, -.07, off+.006, .02, 'hull')
    wrap('gauntlet', b, -40, 220, 12, -.04, -.16, off*t, th*t, 'plate', rows=5)
    wrap('gauntlet-lower', b, -30, 210, 12, -.15, -.245, off*t*.85, th*t*.9, 'plate')
    wrap('gauntlet-rail', b, 80, 100, 2, -.05, -.22, off*t+.01, .01, 'steel')
    wrap('gauntlet-light', b, 84, 96, 1, -.07, -.14, off*t+.016, .005, 'glow', edge='glow')
    wrap('wrist-band', b, 0, 360, 14, -.25, -.268, .006, .009, 'hull', edge='hull')


def thigh():
    b = leg_shape()
    t = P['plate']
    # A team plate down the front and outside, coming to a point at the
    # knee; straps above and below it.
    wrap('thigh-plate', b, -36, 136, 10, lambda a: -.33+.06*abs(a-50)/86, lambda a: -.06-.015*abs(a-50)/86,
         .032*t, .024*t, 'team', rows=5)
    wrap('thigh-back', b, 150, 230, 6, -.34, -.08, .024*t, .018*t, 'hull')
    # The tasset over the top of the thigh's front, under the belt.
    wrap('tasset', b, -40, 40, 6, -.075, .0, .05*t, .02*t, 'plate')
    wrap('thigh-strap', b, 0, 360, 16, -.04, -.022, .012, .01, 'rubber', edge='rubber')
    # (Kept clear of the knee, where the undersuit bends toward the shin.)
    wrap('thigh-strap', b, 0, 360, 16, -.30, -.284, .01, .009, 'rubber', edge='rubber')


def shin():
    # The leg's form in the shin's frame (from the knee down).
    b = knee = shape(LEG, P['limb'], P['limb'], THIGH)
    t = P['plate']
    # The knee pad over the joint (hexagonal: pointed top and bottom), the
    # greave down the front with a ridge and a light, a calf plate.
    # (It stops at the joint: rigid on the shin, any of it above the knee
    # would stand out past a deeply bent knee.)
    wrap('knee', knee, -56, 56, 6, lambda a: -.12+.04*abs(a)/56, lambda a: .028-.03*abs(a)/56, .032*t, .03*t, 'hull')
    wrap('knee-cap', knee, -30, 30, 4, lambda a: -.09+.02*abs(a)/30, lambda a: .012-.02*abs(a)/30, .05*t, .02, 'plate')
    wrap('greave', b, -112, 112, 12, -.41, lambda a: -.115-.03*abs(a)/112, .03*t, .022*t, 'plate', rows=5)
    wrap('greave-ridge', b, -10, 10, 2, -.39, -.13, .05*t, .014, 'steel')
    wrap('shin-light', b, -4, 4, 1, -.32, -.16, .056*t, .006, 'glow', edge='glow')
    wrap('calf', b, 132, 228, 6, -.36, -.12, .024*t, .018*t, 'hull')


def boot():
    w, zt, zh, top = P['boot']
    plan = lambda ww, z0, z1, cf, cb: hs.plan(z0, z1, ww, cf, cb)
    sole = Solid('sole', .005)
    loft(sole, [[(x, y, z) for x, z in plan(w, zt, zh, w*.62, w*.3)] for y in (-.17, -.125)], 'rubber', caps=('rubber', 'hidden'))
    upper = Solid('boot', .006)
    loft(upper, [[(x, y, z) for x, z in plan(ww, z0, zh-.004, ww*.62, ww*.32)] for y, ww, z0 in
                 ((-.125, w*.97, zt+.004), (-.06, w*.95, zt+.04), (.0, w*.8, -w*.7))],
         lambda k, i: ['hull', 'plate'][i], caps=('hidden', 'hidden'))
    toe = Solid('toe', .005)
    loft(toe, [[(x, y, z) for x, z in plan(ww, z0, z0+.10, ww*.62, .0)] for y, ww, z0 in
               ((-.127, w*1.0, zt-.002), (-.065, w*.97, zt+.026))], 'plate', caps=('hidden', 'plate'))
    # The boot's shaft hugs the undersuit's ankle (its inside just clear of
    # it), so no hollow ring shows round a thin ankle.
    r = .074*P['limb']
    cuff = Body([(-.03, r, r, r*1.05), (top, r*.94, r*.94, r*.98)], cz=.006)
    wrap('cuff', cuff, -180, 180, 12, -.03, top, 0.0, .022, 'plate', edge='hull', inner='recess')
    wrap('strap', cuff, -180, 180, 12, .015, .04, .012, .01, 'rubber', edge='rubber')
    wrap('cuff-light', cuff, 80, 100, 2, top-.045, top-.02, .016, .005, 'glow', edge='glow')
    # Plates seated on the shaft (their inner faces inside it), so none
    # stands off it as a thin hook.
    wrap('ankle', cuff, 55, 125, 4, -.02, top-.055, .016, .03, 'hull')
    wrap('ankle', cuff, -125, -55, 4, -.02, top-.055, .016, .03, 'hull')
    wrap('heel', cuff, 145, 215, 4, -.11, -.01, .014, .036, 'plate')


def glove():
    """The hand frame: origin at the wrist, -Y along the hand, the palm toward
    -Z. A cuff, a dark armored palm with a gunmetal back plate and knuckle
    guard, four jointed fingers curled loosely toward the palm, a thumb on
    the inner side, a light on the back."""
    g = P['glove']
    G = lambda *v: tuple(c*g for c in v)
    cuff = Body([(.025, .046*g, .046*g, .046*g), (-.02, .049*g, .049*g, .049*g)])
    wrap('glove-cuff', cuff, 0, 360, 10, -.02, .025, 0.0, .012, 'hull', edge='hull', inner='recess')
    hand = Solid('glove', .005)
    ring = lambda y, w, z0, z1: [(x, y, z) for x, z in hs.plan(z0, z1, w, .012*g, .012*g)]
    loft(hand, [ring(-.012*g, .042*g, -.03*g, .032*g), ring(-.06*g, .046*g, -.03*g, .03*g),
                ring(-.088*g, .044*g, -.026*g, .024*g)], 'rubber', caps=('hidden', 'rubber'))
    solid_box('glove-back', G(-.038, -.07, .024), G(.038, -.02, .042), {'*': 'plate', '-z': 'hidden'}, bevel=.004)
    solid_box('knuckles', G(-.042, -.094, .006), G(.042, -.078, .03), {'*': 'steel', '-z': 'hidden'}, bevel=.003)
    hs.quad_plate(G(-.012, -.06, .042), G(.012, -.06, .042), G(.012, -.035, .042), G(-.012, -.035, .042),
                  0, 1, 0, 1, 'glow', h=.003, away=(0.0, -.05, 0.0), name='glove-light')
    # Fingers: three segments each, the knuckle straight on, then curling
    # toward the palm.
    for k, x in enumerate((-.03, -.01, .01, .03)):
        long = (.0, .006, .004, -.004)[k]
        a = G(x, -.084, .004)
        b = G(x, -.112-long, .0)
        c = G(x, -.126-long, -.026)
        d = G(x, -.12-long*.5, -.046)
        for name, p0, p1 in (('finger-1', a, b), ('finger-2', b, c), ('finger-3', c, d)):
            hs.beam(name, p0, p1, .017*g, .019*g, 'rubber', bevel=.003)
    hs.beam('thumb', G(-.036, -.03, -.016), G(-.054, -.068, -.036), .021*g, .023*g, 'rubber', bevel=.004)
    hs.beam('thumb-tip', G(-.054, -.068, -.036), G(-.042, -.094, -.05), .019*g, .021*g, 'rubber', bevel=.003)


def seg(name, p0, p1, r0, r1, mat, n=10, domes=(True, True)):
    """A rounded length from p0 to p1, radius r0 tapering to r1, its ends
    domed (or flat, for an end that sinks into something): a finger's bone,
    a thumb's, a cuff."""
    p0, p1 = np.array(p0, np.float64), np.array(p1, np.float64)
    d = (p1-p0)/np.linalg.norm(p1-p0)
    u = np.cross(d, [0.0, 1.0, 0.0] if abs(d[1]) < .9 else [1.0, 0.0, 0.0])
    u /= np.linalg.norm(u)
    v = np.cross(d, u)
    ring = lambda c, r: [tuple(c+(u*math.cos(2*math.pi*k/n)+v*math.sin(2*math.pi*k/n))*r) for k in range(n)]
    s = Solid(name, 0, smooth=60)
    rings = ([ring(p0-d*r0*.55, r0*.62)] if domes[0] else []) + [ring(p0, r0), ring(p1, r1)] \
        + ([ring(p1+d*r1*.55, r1*.62)] if domes[1] else [])
    caps = (('tip', tuple(p0-d*r0*.85), mat) if domes[0] else mat, ('tip', tuple(p1+d*r1*.85), mat) if domes[1] else mat)
    loft(s, rings, mat, caps=caps)
    return s


def guard(name, p0, p1, back, w, h, mat='hull'):
    """An armored plate along p0..p1 on its `back` side (a direction), w
    wide and h thick: a finger's knuckle guard, a hand's back plate."""
    p0, p1, back = (np.array(v, np.float64) for v in (p0, p1, back))
    d = (p1-p0)/np.linalg.norm(p1-p0)
    back = back-d*np.dot(back, d)
    back /= np.linalg.norm(back)
    side = np.cross(d, back)
    sec = [(-w/2, 0.0), (w/2, 0.0), (w/2*.82, h), (-w/2*.82, h)]
    s = Solid(name, .002)
    loft(s, [[tuple(p+side*a+back*b) for a, b in sec] for p in (p0, p1)], mat, caps=(mat, mat))
    return s


def finger(name, joints, r, back, plates=True):
    """A finger through its joints (knuckle, two middle joints, tip), each
    bone rounded and a little thinner than the last, guards on the back of
    the first two."""
    rs = (r, r*.92, r*.84, r*.72)
    for k, (p0, p1) in enumerate(zip(joints, joints[1:])):
        seg(f'{name}-{k}', p0, p1, rs[k], rs[k+1], 'rubber')
        if plates and k < 2:
            a, b = np.array(p0), np.array(p1)
            guard(f'{name}-guard', a+(b-a)*.12+np.array(back)*rs[k]*.55, a+(b-a)*.88+np.array(back)*rs[k+1]*.55, back,
                  rs[k]*1.7, .0045, 'hull')


def grip():
    """The support hand on a weapon, made in the weapon's frame at the hand
    (+X toward the weapon, +Y up, -Z forward): a palm on the weapon's left
    side, four jointed fingers curling over its top with guards on their
    backs, a thumb under it, an armored back with a light, and the cuff
    running back and down to the wrist (drawlist::vm_forearm: 2 cm left, 5
    cm down and 10 cm back), where the forearm piece meets it."""
    g = 1+(P['glove']-1)*.5
    G = lambda *v: tuple(c*g for c in v)
    # The palm: a rounded slab against the weapon's side.
    palm = Solid('grip-palm', .006)
    loft(palm, [[G(x, y, z) for x, z in ((-.024, -.046), (.02, -.046), (.024, -.03), (.024, .04), (.02, .046), (-.024, .046),
                                         (-.028, .03), (-.028, -.03))] for y in (-.05, .044)], 'rubber', caps=('rubber', 'rubber'))
    # The back of the hand: a gunmetal plate, a light.
    guard('grip-back', G(-.026, -.036, .0), G(-.026, .04, .0), (-1, 0, 0), .084*g, .012*g, 'hull')
    hs.quad_plate(G(-.039, -.004, -.014), G(-.039, -.004, .014), G(-.039, .024, .014), G(-.039, .024, -.014), 0, 1, 0, 1,
                  'glow', h=.003, away=(0.0, 0.0, 0.0), name='grip-light')
    # Four fingers over the top, the middle two a little longer.
    for k, z in enumerate((-.034, -.0115, .0115, .034)):
        reach = (0.0, .006, .004, -.006)[k]
        joints = [G(-.008, .044, z), G(.022, .074+reach*.5, z), G(.055, .084+reach, z), G(.078, .062+reach, z)]
        finger('grip-finger', joints, .0098*g*(.92 if k == 3 else 1.0), (-.4, 1, 0))
    finger('grip-thumb', [G(.004, -.03, .03), G(.03, -.05, .0), G(.058, -.058, -.034)], .0115*g, (0, -1, .2), plates=False)
    # The cuff back to the wrist, banded.
    seg('grip-cuff', G(-.006, -.03, .03), G(-.021, -.052, .108), .042*g, .046*g, 'rubber', n=14, domes=(True, False))
    seg('grip-cuff-band', G(-.018, -.047, .088), G(-.021, -.052, .106), .049*g, .049*g, 'hull', n=14, domes=(False, False))
    hs.POINTS.append(('wrist_grip_l', G(-.021, -.052, .108)))


def grip_r():
    """The firing hand round a pistol grip, made in the weapon's frame at
    the grip (+X the weapon's right, +Y up, -Z forward; the grip a handle
    about 3 cm wide and 4.5 cm deep, raked back): the palm on its right and
    behind it, four jointed fingers wrapped round its front with guards on
    their backs, an armored back of the hand, the thumb up its left side,
    and the cuff running back and down to the wrist (point `wrist_grip_r`),
    where the forearm meets it."""
    g = 1+(P['glove']-1)*.5
    G = lambda *v: tuple(c*g for c in v)
    palm = Solid('trigger-palm', .006)
    loft(palm, [[G(x, y, z) for x, z in ((.02, -.03), (.046, -.03), (.05, -.01), (.05, .03), (.04, .05), (.0, .05),
                                         (-.012, .036), (-.006, .02))] for y in (-.052, .03)], 'rubber', caps=('rubber', 'rubber'))
    guard('trigger-back', G(.05, -.046, .01), G(.05, .026, .01), (1, 0, 0), .07*g, .012*g, 'hull')
    hs.quad_plate(G(.063, -.02, .0), G(.063, -.02, .026), G(.063, .01, .026), G(.063, .01, .0), 0, 1, 0, 1,
                  'glow', h=.003, away=(0.0, 0.0, 0.0), name='trigger-light')
    # The fingers round the front of the handle, index at the top.
    for k, y in enumerate((.019, .0, -.019, -.037)):
        tip = (-.002, .002, .0, -.004)[k]
        joints = [G(.036, y, -.026), G(.012, y, -.04+tip), G(-.016, y, -.032+tip), G(-.024, y, -.012)]
        finger('trigger-finger', joints, .0092*g*(.9 if k == 3 else 1.0), (.3, 0, -1))
    finger('trigger-thumb', [G(-.004, -.024, .04), G(-.022, .006, .022), G(-.024, .03, .004)], .0105*g, (-1, 0, 0), plates=False)
    seg('trigger-cuff', G(.018, -.04, .036), G(.025, -.075, .1), .042*g, .046*g, 'rubber', n=14, domes=(True, False))
    seg('trigger-cuff-band', G(.023, -.07, .084), G(.025, -.075, .1), .049*g, .049*g, 'hull', n=14, domes=(False, False))
    hs.POINTS.append(('wrist_grip_r', G(.025, -.075, .1)))


def sleeves():
    """The undersuit's arm in two rigid pieces, for first person only (the
    third-person body wears the skinned undersuit): `sleeve_upper_r` in the
    upper arm's frame, `sleeve_fore_r` in the forearm's (left mirrored)."""
    for tag in ('r',):
        root(f'sleeve_upper_{tag}')
        tube('sleeve', arm_shape(), steps(.04, -.34, .025), 18, caps=(SUIT, SUIT))
        root(f'sleeve_fore_{tag}')
        tube('sleeve', arm_shape(UPPER), steps(.04, -.31, .025), 18, caps=(SUIT, SUIT))
    mirror('sleeve_upper_r', 'sleeve_upper_l')
    mirror('sleeve_fore_r', 'sleeve_fore_l')


setup()
root('undersuit'); suit()
hs.SKINNED.add('undersuit')
root('helmet'); helmet()
root('torso'); torso()
root('pack'); pack()
root('pelvis'); pelvis()
for name, fn in (('upper_arm', upper_arm), ('forearm', forearm), ('thigh', thigh), ('shin', shin), ('boot', boot),
                 ('glove', glove)):
    root(f'{name}_r'); fn()
    mirror(f'{name}_r', f'{name}_l')
root('grip_l'); grip()
root('grip_r'); grip_r()
sleeves()
finish(OUT, TEX, generator='PeakRunner scripts/models/armor_models.py', material=f'armor-{SIZE}')
