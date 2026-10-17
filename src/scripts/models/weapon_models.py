"""PeakRunner's weapons as hard-surface models (docs/model-pipeline.md,
"Weapons" and "Hard-surface structures"; look: docs/art-direction.md),
built in Blender headless:

    blender -b -P scripts/models/weapon_models.py -- OUT.glb [--weapon ID] [--texture 2048]

(`sh scripts/models/build.sh SOURCES weapons/disc` runs it: the weapon is the
output's folder.)

Each weapon is one part, `body`, in the weapon frame: meters, -Z forward, +Y
up, the origin at the firing hand's grip (`grip_r`). Its points are where
the game holds and fires it, unchanged from the models these replace:
`grip_r` (the origin), `grip_l` (the support hand; the palm goes a little
further left) and `muzzle` (where rounds leave, which the weapon's
definition repeats: drawlist::muzzle_tests).

The family: gunmetal receivers with chamfered edges in bright steel, painted
panels, warm-black rubber grips, cyan-white energy parts, one team stripe,
and each weapon's signature shape (the art direction's table). Physically
based: metal, roughness, occlusion and normal maps (hardsurface `PBR`).
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import Solid, finish, loft, root, setup, solid_box

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
WEAPON = args[args.index('--weapon')+1] if '--weapon' in args else os.path.basename(os.path.dirname(os.path.abspath(OUT)))
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 2048
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.002, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.FLAT_TEXEL = .35
hs.GRATE = (.008, .5)
hs.PBR = True


# --- Shapes ---------------------------------------------------------------------

def ring_z(r, z, n=16, cx=0.0, cy=0.0, ry=None, phase=None):
    """A ring of n points round an axis along Z through (cx, cy); `ry` makes
    it an ellipse."""
    a0 = math.pi/n if phase is None else phase
    ry = r if ry is None else ry
    return [(cx+r*math.cos(a0+2*math.pi*k/n), cy+ry*math.sin(a0+2*math.pi*k/n), z) for k in range(n)]


def tube(name, sections, mats, caps=('hull', 'hull'), n=16, cx=0.0, cy=0.0, bevel=.003, smooth=40.0):
    """A solid of revolution round an axis along Z: (z, r) sections, or (z,
    rx, ry) for an elliptical one; `mats` one material, or one per span."""
    s = Solid(name, bevel, smooth)
    rings = [ring_z(sec[1], sec[0], n, cx, cy, sec[2] if len(sec) > 2 else None) for sec in sections]
    loft(s, rings, mats if isinstance(mats, str) else (lambda k, i: mats[i]), caps=caps)
    return s


def band(name, r0, r1, z0, z1, mat, n=16, cx=0.0, cy=0.0):
    """A flat ring round an axis along Z: radii r0..r1 from z0 to z1."""
    s = Solid(name, 0, smooth=40)
    loft(s, [ring_z(r, z, n, cx, cy) for z, r in ((z0, r1), (z1, r1), (z1, r0), (z0, r0))], mat, caps=(None, None))
    a, b = len(s.v)-n, len(s.v)-4*n
    for k in range(n):
        s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)
    return hs.flat(s, mat)


def section(w, y0, y1, top=.25, bot=.15):
    """A chamfered section (x, y): half width w from y0 to y1, its top
    corners cut by `top` of the width and its bottom ones by `bot`."""
    ct, cb = w*top, w*bot
    return [(-w+cb, y0), (w-cb, y0), (w, y0+cb), (w, y1-ct), (w-ct, y1), (-w+ct, y1), (-w, y1-ct), (-w, y0+cb)]


def prism(name, stations, mats='hull', caps=('hull', 'hull'), bevel=.004):
    """A body along Z through stations (z, section): every section the same
    count of points, in order round it. `mats` is one material, or a
    function (side, span) -> material."""
    s = Solid(name, bevel)
    loft(s, [[(x, y, z) for x, y in sec] for z, sec in stations], mats, caps=caps)
    return s


def box(name, lo, hi, mats='hull', bevel=.003):
    solid_box(name, lo, hi, mats, bevel=bevel)


def plate_x(name, x, y0, y1, z0, z1, mat, h=.003, out=-1):
    """A thin plate on a side face at x (out: -1 the left side, +1 the right)."""
    hs.quad_plate((x, y0, z0), (x, y0, z1), (x, y1, z1), (x, y1, z0), 0, 1, 0, 1, mat, h=h, away=(x-out, (y0+y1)/2, (z0+z1)/2),
                  name=name)


def plate_y(name, y, x0, x1, z0, z1, mat, h=.003, out=1):
    """A thin plate on a top (out 1) or bottom (out -1) face at y."""
    hs.quad_plate((x0, y, z0), (x1, y, z0), (x1, y, z1), (x0, y, z1), 0, 1, 0, 1, mat, h=h, away=((x0+x1)/2, y-out, (z0+z1)/2),
                  name=name)


def vents_x(name, x, y0, y1, z0, z1, count, out=-1, depth=.004):
    """A row of vent slats down a side: dark slots between raised bars."""
    plate_x(name+'-back', x, y0, y1, z0, z1, 'recess', h=.001, out=out)
    pitch = (z1-z0)/count
    for k in range(count):
        a = z0+pitch*(k+.25)
        b = a+pitch*.5
        x0, x1 = sorted((x, x+out*depth))
        box(name, (x0, y0+.002, a), (x1, y1-.002, b), 'hull', bevel=.0008)


def bolt(name, p, axis='x', r=.0045, h=.003, mat='steel'):
    """A hex bolt head standing out of a face along +-axis."""
    x, y, z = p
    s = Solid(name, .0006, smooth=1)
    n = 6
    sign = 1 if axis[0] != '-' else -1
    ax = axis[-1]
    def pt(a, d):
        u, v = r*math.cos(a), r*math.sin(a)
        if ax == 'x': return (x+sign*d, y+u, z+v)
        if ax == 'y': return (x+u, y+sign*d, z+v)
        return (x+u, y+v, z+sign*d)
    loft(s, [[pt(2*math.pi*k/n, d) for k in range(n)] for d in (-.001, h)], mat, caps=(mat, mat))


def handle(name, top, bottom, w=.031, d=.046, mat='rubber', grooves=3):
    """A pistol grip from `top` (where it meets the receiver) down to
    `bottom`, raked: a rounded section, finger grooves down its front, a
    flared base."""
    (tx, ty, tz), (bx, by, bz) = top, bottom
    s = Solid(name, .005, smooth=40)
    rings = []
    n = 8 + grooves*2
    for i in range(n+1):
        t = i/n
        y = ty+(by-ty)*t
        z = tz+(bz-tz)*t
        groove = .006*max(0.0, math.sin(t*math.pi*grooves*1.0+.4)) if .1 < t < .9 else 0.0
        flare = 1.0+.18*max(0.0, (t-.85)/.15)
        ww, dd = w*flare, d*flare
        sec = [(-ww*.5, -dd*.42), (-ww*.38, -dd*.5+groove), (ww*.38, -dd*.5+groove), (ww*.5, -dd*.42),
               (ww*.5, dd*.36), (ww*.32, dd*.5), (-ww*.32, dd*.5), (-ww*.5, dd*.36)]
        rings.append([(tx+x, y, z+zz) for x, zz in sec])
    loft(s, rings, mat, caps=('hidden', mat))
    return s


def trigger_group(front=-.075, back=-.01, y=-.008, w=.022):
    """A trigger guard loop under the receiver and the trigger in it."""
    hs.beam('guard', (0.0, y-.02, front), (0.0, y-.02, back), w, .008, 'hull', bevel=.002)
    hs.beam('guard', (0.0, y-.02, front), (0.0, y+.03, front-.006), w, .008, 'hull', bevel=.002)
    hs.beam('trigger', (0.0, y+.022, back-.034), (0.0, y-.008, back-.04), .009, .007, 'steel', bevel=.0015)


def glow_strip(name, p0, p1, w, normal):
    """A lit strip from p0 to p1 on a face whose outward normal is `normal`."""
    import numpy as np
    p0, p1, nrm = np.array(p0, float), np.array(p1, float), np.array(normal, float)
    d = p1-p0
    side = np.cross(d/np.linalg.norm(d), nrm)
    side = side/np.linalg.norm(side)*w/2
    a, b, c, e = p0-side, p1-side, p1+side, p0+side
    hs.quad_plate(tuple(a), tuple(b), tuple(c), tuple(e), 0, 1, 0, 1, 'glow', h=.002, away=tuple(p0-nrm*.05), name=name)


def points(grip_l, muzzle):
    hs.POINTS.extend([('grip_r', (0.0, 0.0, 0.0)), ('grip_l', grip_l), ('muzzle', muzzle)])


# --- Weapons ---------------------------------------------------------------------

def disc():
    """The disc launcher: a flat drum magazine on top, the next disc's rim
    glowing through a slot in its cover; a long receiver over the pistol
    grip; a wide, flat launch tube ending in a clamshell muzzle whose jaws
    open round the bore."""
    # Receiver: a tall rear block stepping down to the fore end, gunmetal
    # with painted side panels.
    rec = lambda w, y0, y1: section(w, y0, y1, .3, .18)
    prism('receiver', [(.19, rec(.034, .03, .118)), (.12, rec(.038, .02, .128)), (-.05, rec(.038, .02, .13)),
                       (-.12, rec(.036, .035, .12)), (-.32, rec(.034, .05, .115))],
          lambda k, i: 'plate' if k in (2, 6) else 'hull', caps=('hull', 'hidden'), bevel=.004)
    box('butt', (-.032, .022, .185), (.032, .122, .205), 'rubber', bevel=.006)
    for sx in (-1, 1):
        x = sx*.0385
        plate_x('side-panel', x, .05, .1, -.04, .1, 'plate', h=.003, out=sx)
        vents_x('vent', x+sx*.003, .058, .092, .11, .17, 5, out=sx)
        hs.quad_plate((x, .1, -.03), (x, .1, .16), (x, .106, .16), (x, .106, -.03), 0, 1, 0, 1, 'team', h=.0035,
                      away=(x-sx, .1, .06), name='team-stripe')
        for z in (-.035, .095):
            bolt('bolt', (x+sx*.003, .045, z), axis='x' if sx > 0 else '-x', r=.004)
    # Energy cell on the left side, cyan through a steel cage.
    box('cell-cage', (-.052, .055, .0), (-.036, .095, .085), {'*': 'hull', '+x': 'hidden'}, bevel=.003)
    plate_x('cell', -.052, .062, .088, .008, .077, 'glow', h=.002, out=-1)
    for z in (.025, .045, .065):
        box('cell-bar', (-.056, .058, z-.003), (-.051, .092, z+.003), 'steel', bevel=.001)
    # Drum magazine: a flat cylinder on top, its cover plated, a slot round
    # its left side showing the next disc's lit rim, a hub.
    cz, cy, r = -.17, .168, .172
    s = Solid('drum', .004, smooth=30)
    rings = [[(r*k_*math.cos(a), y, cz+r*k_*math.sin(a)) for a in [2*math.pi*(j+.5)/40 for j in range(40)]]
             for y, k_ in ((.128, .9), (.136, 1.0), (.198, 1.0), (.206, .94))]
    loft(s, rings, lambda k, i: ['steel', 'hull', 'steel'][i], caps=('hidden', 'plate'))
    # The window: an arc of the drum's left side cut dark, the disc rim
    # inside it lit.
    for j in range(14):
        a0 = math.radians(150+j*4.4)
        a1 = a0+math.radians(3.2)
        p = lambda a, y, k_: (r*k_*math.cos(a), y, cz+r*k_*math.sin(a))
        q = Solid('drum-slot', 0, smooth=1)
        v = q.pts([p(a0, .157, 1.004), p(a1, .157, 1.004), p(a1, .177, 1.004), p(a0, .177, 1.004),
                   p(a0, .157, .99), p(a1, .157, .99), p(a1, .177, .99), p(a0, .177, .99)])
        for f, m in (((0, 1, 2, 3), 'glow'), ((4, 7, 6, 5), 'hidden'), ((0, 4, 5, 1), 'recess'), ((2, 6, 7, 3), 'recess'),
                     ((1, 5, 6, 2), 'recess'), ((3, 7, 4, 0), 'recess')):
            q.face([v[i] for i in f], m)
    hub = Solid('drum-hub', .003, smooth=30)
    loft(hub, [[(.05*k_*math.cos(2*math.pi*(j+.5)/16), y, cz+.05*k_*math.sin(2*math.pi*(j+.5)/16)) for j in range(16)]
               for y, k_ in ((.2, 1.0), (.214, 1.0), (.218, .8))], 'steel', caps=('hidden', 'hull'))
    for j in range(6):
        a = 2*math.pi*j/6
        hs.beam('drum-rib', (math.cos(a)*.06, .209, cz+math.sin(a)*.06), (math.cos(a)*.155, .209, cz+math.sin(a)*.155),
                .014, .006, 'hull', bevel=.0015)
    hs.quad_plate((-.03, .214, cz-.006), (.03, .214, cz-.006), (.03, .214, cz+.006), (-.03, .214, cz+.006), 0, 1, 0, 1,
                  'team', h=.002, away=(0.0, 0.0, cz), name='drum-team')
    # The fore end round the bore: a broad, flat housing, the support
    # hand's place on its left (grip_l), a top rail.
    prism('fore', [(-.31, section(.05, .062, .142, .3, .3)), (-.5, section(.058, .062, .142, .3, .3)),
                   (-.56, section(.062, .065, .14, .25, .25))], 'hull', caps=('hidden', 'hidden'), bevel=.004)
    for sx in (-1, 1):
        plate_x('fore-panel', sx*.0565, .078, .126, -.34, -.49, 'plate', h=.003, out=sx)
    glow_strip('fore-light', (-.06, .102, -.36), (-.06, .102, -.47), .006, (-1, 0, 0))
    box('rail', (-.014, .142, -.5), (.014, .152, -.32), 'hull', bevel=.002)
    for z in [-.33-.03*k for k in range(6)]:
        box('rail-tooth', (-.016, .152, z-.008), (.016, .158, z), 'steel', bevel=.001)
    # The launch tube: a wide, flat bore, and the clamshell's two jaws
    # flaring open round it at the muzzle, cheeks either side.
    box('bore', (-.074, .086, -.648), (.074, .12, -.55), 'recess', bevel=0)
    prism('jaw-top', [(-.55, [(-.084, .12), (.084, .12), (.078, .146), (-.078, .146)]),
                      (-.62, [(-.09, .124), (.09, .124), (.084, .15), (-.084, .15)]),
                      (-.652, [(-.094, .128), (.094, .128), (.09, .144), (-.09, .144)])], 'hull', caps=('hidden', 'steel'))
    prism('jaw-bottom', [(-.55, [(-.078, .062), (.078, .062), (.084, .086), (-.084, .086)]),
                         (-.62, [(-.084, .056), (.084, .056), (.09, .082), (-.09, .082)]),
                         (-.652, [(-.09, .062), (.09, .062), (.094, .078), (-.094, .078)])], 'hull', caps=('hidden', 'steel'))
    for sx in (-1, 1):
        x0, x1 = sorted((sx*.074, sx*.094))
        box('cheek', (x0, .078, -.645), (x1, .128, -.556), 'plate', bevel=.003)
        bolt('cheek-bolt', (sx*.094, .103, -.6), axis='x' if sx > 0 else '-x', r=.005)
    glow_strip('bore-light', (-.07, .087, -.6), (.07, .087, -.6), .004, (0, 1, 0))
    # Grip, guard and trigger.
    handle('grip', (0.0, .03, .0), (0.0, -.085, .038))
    trigger_group(front=-.085, back=-.01, y=.012, w=.022)
    points(grip_l=(0.0, .115, -.356), muzzle=(0.0, .103, -.648))


def receiver(z_back, z_front, y0=.022, y1=.118, w=.036, panel=True, stripe=True, butt=True, vents=True):
    """The family's receiver over the pistol grip: a chamfered gunmetal
    block with painted side panels, vents, the team stripe, bolts and a
    rubber butt pad."""
    rec = lambda ww, a, b: section(ww, a, b, .3, .18)
    prism('receiver', [(z_back, rec(w*.92, y0+.008, y1-.004)), (z_back-.03, rec(w, y0, y1)), (z_front+.02, rec(w, y0, y1)),
                       (z_front, rec(w*.92, y0+.006, y1-.006))],
          lambda k, i: 'plate' if k in (2, 6) else 'hull', caps=('hull', 'hull'), bevel=.004)
    if butt:
        box('butt', (-w*.9, y0, z_back), (w*.9, y1-.004, z_back+.018), 'rubber', bevel=.006)
    for sx in (-1, 1):
        x = sx*(w+.0005)
        mid = (y0+y1)/2
        if panel:
            plate_x('side-panel', x, mid-.024, mid+.02, z_front+.05, z_back-.07, 'plate', h=.003, out=sx)
        if vents:
            vents_x('vent', x+sx*.003, y0+.012, y0+.036, z_back-.06, z_back-.035+min(.0, .0), 4, out=sx)
        if stripe:
            hs.quad_plate((x, y1-.018, z_front+.03), (x, y1-.018, z_back-.04), (x, y1-.012, z_back-.04), (x, y1-.012, z_front+.03),
                          0, 1, 0, 1, 'team', h=.0035, away=(x-sx, y1, (z_front+z_back)/2), name='team-stripe')
        for z in (z_front+.03, z_back-.05):
            bolt('bolt', (x+sx*.003, y0+.012, z), axis='x' if sx > 0 else '-x', r=.004)


def grip_and_trigger(guard_front=-.085):
    handle('grip', (0.0, .03, .0), (0.0, -.085, .038))
    trigger_group(front=guard_front, back=-.01, y=.012, w=.022)


def foregrip(top, length=.075, rake=.012):
    """A vertical fore-grip hanging under the fore end, for a support hand
    held below (grip_l under the weapon)."""
    x, y, z = top
    handle('foregrip', (x, y, z), (x, y-length, z+rake), w=.03, d=.036, grooves=2)


def chaingun():
    """The repeater: a thick heat-sink shroud of fins round the barrel,
    ending in a slotted muzzle brake; a curved magazine angled forward."""
    receiver(.2, -.17, w=.04)
    # The barrel shroud: a drum of fins, a collar at each end.
    cy = .098
    tube('shroud-core', [(-.15, .036), (-.62, .036)], 'hull', caps=('hidden', 'hull'), n=16, cy=cy)
    for k in range(13):
        z = -.18-.034*k
        band('fin', .036, .058 if k % 3 else .062, z-.009, z+.009, 'hull' if k % 3 else 'steel', n=20, cy=cy)
    band('collar', .03, .062, -.17, -.145, 'steel', n=20, cy=cy)
    band('collar', .03, .056, -.625, -.6, 'steel', n=20, cy=cy)
    # The muzzle brake: a block with slots through its sides.
    prism('brake', [(-.62, section(.032, cy-.03, cy+.03, .3, .3)), (-.685, section(.032, cy-.03, cy+.03, .3, .3))], 'hull',
          caps=('hidden', 'recess'))
    for z in (-.635, -.65, -.665):
        for sx in (-1, 1):
            x0, x1 = sorted((sx*.0322, sx*.0345))
            box('brake-slot', (x0, cy-.018, z-.004), (x1, cy+.018, z+.004), 'recess', bevel=0)
    # Glow line along the shroud's top and the barrel's bore.
    glow_strip('heat-line', (0.0, cy+.064, -.2), (0.0, cy+.064, -.58), .005, (0, 1, 0))
    tube('bore', [(-.684, .012), (-.687, .012)], 'recess', caps=('recess', 'recess'), n=12, cy=cy, bevel=0)
    # The magazine: a curved box under the receiver ahead of the grip.
    s = Solid('magazine', .004)
    secs = []
    for t in [k/6 for k in range(7)]:
        y = .02-.16*t
        z = -.07-.05*t*t-.02*t
        secs.append([(-.026, y, z-.026), (.026, y, z-.026), (.026, y, z+.028), (-.026, y, z+.028)])
    loft(s, secs, lambda k, i: 'plate' if k in (1, 3) else 'hull', caps=('hidden', 'rubber'))
    for sx in (-1, 1):
        glow_strip('mag-window', (sx*.0265, -.02, -.086), (sx*.0265, -.1, -.11), .01, (sx, 0, 0))
    grip_and_trigger(guard_front=-.06)
    # The support hand on the shroud's lower left.
    points(grip_l=(0.0, .054, -.325), muzzle=(0.0, .098, -.685))


def grenade_launcher():
    """A stubby, wide barrel over a six-chamber revolving cylinder."""
    receiver(.2, -.08, w=.04)
    cy = .074
    # The cylinder: a fat drum along the barrel's axis, six chambers round
    # it (dark holes in its faces), flutes between them.
    tube('cylinder', [(-.075, .066), (-.08, .072), (-.235, .072), (-.24, .066)], ['steel', 'hull', 'steel'],
         caps=('hull', 'hull'), n=24, cy=cy, bevel=.003)
    for k in range(6):
        a = 2*math.pi*k/6+math.pi/6
        x, y = math.cos(a)*.044, cy+math.sin(a)*.044
        tube('chamber', [(-.2405, .016), (-.244, .016)], 'recess', caps=('recess', 'recess'), n=12, cx=x, cy=y, bevel=0)
        tube('chamber', [(-.0745, .016), (-.071, .016)], 'recess', caps=('recess', 'recess'), n=12, cx=x, cy=y, bevel=0)
        a2 = a+math.pi/6
        hs.beam('flute', (math.cos(a2)*.071, cy+math.sin(a2)*.071, -.09), (math.cos(a2)*.071, cy+math.sin(a2)*.071, -.225),
                .012, .004, 'recess', bevel=0)
    # The barrel: short, wide, with a ringed muzzle and a dark bore.
    tube('barrel', [(-.24, .046), (-.5, .046), (-.51, .052), (-.556, .052), (-.566, .048)], ['hull', 'steel', 'steel', 'steel'],
         caps=('hidden', 'recess'), n=24, cy=.098, bevel=.003)
    tube('bore', [(-.5, .032), (-.567, .032)], 'recess', caps=('recess', 'recess'), n=20, cy=.098, bevel=0)
    for z in (-.3, -.36, -.42):
        band('barrel-ring', .045, .05, z-.008, z+.008, 'plate', n=24, cy=.098)
    glow_strip('barrel-light', (-.047, .098, -.28), (-.047, .098, -.45), .006, (-1, 0, 0))
    # The pump under the barrel, where the support hand holds.
    prism('pump', [(-.17, section(.03, -.045, .0, .3, .3)), (-.3, section(.03, -.045, .0, .3, .3))], 'rubber',
          caps=('rubber', 'rubber'), bevel=.004)
    for z in (-.19, -.21, -.23, -.25, -.27):
        box('pump-rib', (-.032, -.048, z-.004), (.032, -.042, z+.004), 'hull', bevel=.001)
    box('pump-rail', (-.012, .0, -.31), (.012, .03, -.12), 'hull', bevel=.002)
    grip_and_trigger(guard_front=-.07)
    points(grip_l=(0.0, -.024, -.225), muzzle=(0.0, .098, -.566))


def plasma():
    """A glass chamber of glowing plasma mid-body in a steel cage, and a
    short, wide emitter; a fore-grip under the front."""
    receiver(.17, -.1, w=.04)
    cy = .083
    tube('chamber-end', [(-.09, .05), (-.115, .054)], 'hull', caps=('hidden', 'hidden'), n=20, cy=cy)
    tube('plasma', [(-.11, .04), (-.335, .04)], 'glow', caps=('glow', 'glow'), n=20, cy=cy, bevel=0)
    tube('chamber-end', [(-.33, .054), (-.355, .05)], 'hull', caps=('hidden', 'hidden'), n=20, cy=cy)
    for k in range(6):
        a = 2*math.pi*k/6
        hs.beam('cage', (math.cos(a)*.047, cy+math.sin(a)*.047, -.11), (math.cos(a)*.047, cy+math.sin(a)*.047, -.335),
                .008, .006, 'steel', bevel=.0015)
    band('cage-ring', .042, .052, -.228, -.217, 'steel', n=20, cy=cy)
    # The barrel to the emitter: a housing, then the wide flared bell.
    prism('fore', [(-.35, section(.042, .03, .13, .3, .2)), (-.55, section(.046, .03, .135, .3, .2))], 'hull',
          caps=('hidden', 'hidden'), bevel=.004)
    for sx in (-1, 1):
        plate_x('fore-panel', sx*.0465, .05, .11, -.37, -.53, 'plate', h=.003, out=sx)
        vents_x('fore-vent', sx*.0495, .055, .1, -.4, -.5, 6, out=sx)
    tube('emitter', [(-.545, .05), (-.6, .056), (-.635, .072), (-.646, .07)], ['hull', 'steel', 'steel'], caps=('hidden', 'recess'),
         n=24, cy=cy, bevel=.003)
    tube('emitter-core', [(-.6, .03), (-.647, .03)], 'glow', caps=('glow', 'glow'), n=16, cy=cy, bevel=0)
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        hs.beam('emitter-fin', (math.cos(a)*.056, cy+math.sin(a)*.056, -.57), (math.cos(a)*.074, cy+math.sin(a)*.074, -.642),
                .01, .006, 'plate', bevel=.002)
    foregrip((0.0, .03, -.445), length=.08)
    grip_and_trigger()
    points(grip_l=(0.0, -.049, -.445), muzzle=(0.0, .083, -.646))


def blaster():
    """The sidearm: compact, finned heat sinks along its slide, a ring
    muzzle."""
    prism('frame', [(.085, section(.028, .018, .066, .25, .2)), (-.1, section(.028, .018, .066, .25, .2)),
                    (-.12, section(.024, .03, .066, .25, .2))], 'hull', caps=('hull', 'hidden'), bevel=.003)
    prism('slide', [(.09, section(.03, .066, .118, .3, .1)), (-.29, section(.03, .066, .118, .3, .1))],
          lambda k, i: 'plate' if k in (2, 6) else 'hull', caps=('hull', 'hidden'), bevel=.004)
    for k in range(8):
        z = -.06-.026*k
        box('sink-fin', (-.034, .1, z-.006), (.034, .124, z+.006), 'steel' if k % 4 == 0 else 'hull', bevel=.0015)
    for sx in (-1, 1):
        x = sx*.0305
        hs.quad_plate((x, .07, -.25), (x, .07, .05), (x, .075, .05), (x, .075, -.25), 0, 1, 0, 1, 'team', h=.003,
                      away=(x-sx, .07, -.1), name='team-stripe')
        glow_strip('slide-light', (x+sx*.002, .09, .03), (x+sx*.002, .09, -.04), .008, (sx, 0, 0))
    # The ring muzzle.
    tube('barrel', [(-.28, .02), (-.31, .02)], 'steel', caps=('hidden', 'hidden'), n=16, cy=.087)
    band('ring', .022, .036, -.334, -.31, 'steel', n=24, cy=.087)
    band('ring-glow', .019, .023, -.333, -.326, 'glow', n=24, cy=.087)
    tube('bore', [(-.31, .016), (-.334, .016)], 'recess', caps=('recess', 'recess'), n=16, cy=.087, bevel=0)
    handle('grip', (0.0, .03, .0), (0.0, -.09, .04), w=.029, d=.042)
    trigger_group(front=-.07, back=-.008, y=.008, w=.02)
    points(grip_l=(0.0, .088, -.183), muzzle=(0.0, .087, -.332))


def mortar():
    """A fat, short tube on a carrying frame: a funnel breech at the back,
    stabilizer fins round it, a ringed muzzle; the support hand on a
    waisted collar along the tube."""
    cy = .209
    tube('breech', [(.39, .06), (.33, .1), (.25, .1), (.2, .078)], ['steel', 'hull', 'hull'], caps=('recess', 'hidden'),
         n=24, cy=cy)
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        prism('fin', [(.38, [(math.cos(a)*.095-math.sin(a)*.004, cy+math.sin(a)*.095+math.cos(a)*.004),
                             (math.cos(a)*.095+math.sin(a)*.004, cy+math.sin(a)*.095-math.cos(a)*.004),
                             (math.cos(a)*.17+math.sin(a)*.004, cy+math.sin(a)*.17-math.cos(a)*.004),
                             (math.cos(a)*.17-math.sin(a)*.004, cy+math.sin(a)*.17+math.cos(a)*.004)]),
                       (.24, [(math.cos(a)*.095-math.sin(a)*.004, cy+math.sin(a)*.095+math.cos(a)*.004),
                              (math.cos(a)*.095+math.sin(a)*.004, cy+math.sin(a)*.095-math.cos(a)*.004),
                              (math.cos(a)*.13+math.sin(a)*.004, cy+math.sin(a)*.13-math.cos(a)*.004),
                              (math.cos(a)*.13-math.sin(a)*.004, cy+math.sin(a)*.13+math.cos(a)*.004)])],
               'plate', caps=('hull', 'hull'), bevel=.002)
    tube('tube', [(.2, .085), (-.42, .085), (-.46, .06), (-.62, .06), (-.66, .09), (-.86, .09), (-.892, .095)],
         ['hull', 'steel', 'rubber', 'steel', 'hull', 'steel'], caps=('hidden', 'recess'), n=32, cy=cy, bevel=.004)
    tube('bore', [(-.8, .07), (-.893, .07)], 'recess', caps=('recess', 'recess'), n=24, cy=cy, bevel=0)
    for z in (-.1, -.2, -.3, -.72, -.79):
        band('tube-ring', .084, .094, z-.01, z+.01, 'plate', n=32, cy=cy)
    for sx in (-1, 1):
        glow_strip('tube-light', (sx*.086, cy, -.05), (sx*.086, cy, -.35), .008, (sx, 0, 0))
    hs.quad_plate((-.04, cy+.086, -.38), (.04, cy+.086, -.38), (.04, cy+.086, .1), (-.04, cy+.086, .1), 0, 1, 0, 1, 'team',
                  h=.003, away=(0.0, cy, -.1), name='team-stripe')
    # The frame from the tube down to the grip, and the grip.
    prism('frame', [(.16, section(.03, .02, .13, .2, .2)), (-.16, section(.03, .02, .13, .2, .2))], 'hull',
          caps=('hull', 'hull'), bevel=.004)
    grip_and_trigger()
    points(grip_l=(0.0, .193, -.546), muzzle=(0.0, .209, -.892))


def laser():
    """A long open frame round a glowing crystal rod, coils along the
    barrel, a scope over the receiver."""
    receiver(.19, -.14, w=.034)
    cy = .078
    # The frame: two rails above and below the rod, joined by yokes.
    for y0, y1 in ((cy+.032, cy+.05), (cy-.05, cy-.032)):
        prism('frame-rail', [(-.12, section(.028, y0, y1, .3, .3)), (-.7, section(.028, y0, y1, .3, .3))], 'hull',
              caps=('hidden', 'hull'), bevel=.003)
    for z in (-.14, -.42, -.68):
        prism('yoke', [(z, section(.032, cy-.052, cy+.052, .25, .25)), (z-.03, section(.032, cy-.052, cy+.052, .25, .25))],
              'plate', caps=('plate', 'plate'), bevel=.003)
    tube('rod', [(-.13, .011), (-.735, .011)], 'glow', caps=('glow', 'glow'), n=12, cy=cy, bevel=0)
    for k in range(9):
        z = -.18-.025*k
        band('coil', .014, .022, z-.006, z+.006, 'amber' if k % 2 else 'steel', n=16, cy=cy)
    tube('emitter', [(-.7, .026), (-.74, .03), (-.757, .024)], ['steel', 'steel'], caps=('hidden', 'recess'), n=16, cy=cy)
    # The scope.
    box('scope-mount', (-.012, .118, -.07), (.012, .13, .05), 'hull', bevel=.002)
    tube('scope', [(.08, .02), (.06, .026), (-.1, .026), (-.13, .032)], ['steel', 'hull', 'steel'], caps=('glass', 'glass'),
         n=16, cy=.155)
    grip_and_trigger()
    points(grip_l=(0.0, .067, -.399), muzzle=(0.0, .078, -.757))


def railgun():
    """Two parallel rails with an open gap at the front, a capacitor block
    of lit cells behind them; a fore-grip under the rails."""
    receiver(.21, -.06, w=.04)
    # The capacitor block, cells glowing down both sides.
    prism('capacitor', [(-.05, section(.05, .02, .15, .25, .2)), (-.27, section(.05, .02, .15, .25, .2))],
          lambda k, i: 'plate' if k in (2, 6) else 'hull', caps=('hidden', 'hull'), bevel=.004)
    for sx in (-1, 1):
        for k in range(4):
            z = -.075-.05*k
            box('cell', (sx*.05-.004, .045, z-.018), (sx*.05+.004, .125, z+.018), {'*': 'steel', ('+x' if sx > 0 else '-x'): 'glow'},
                bevel=.002)
    # The rails: two long bars above and below the open gap, the gap lit.
    cy = .115
    for y0, y1, mat in ((cy+.016, cy+.042, 'steel'), (cy-.042, cy-.016, 'steel')):
        prism('rail', [(-.27, section(.022, y0, y1, .2, .2)), (-.835, section(.022, y0, y1, .2, .2))], mat,
              caps=('hidden', 'hull'), bevel=.003)
    for z in (-.3, -.5):
        prism('rail-clamp', [(z, section(.03, cy-.05, cy+.05, .25, .25)), (z-.03, section(.03, cy-.05, cy+.05, .25, .25))],
              'hull', caps=('hull', 'hull'), bevel=.003)
    glow_strip('gap', (0.0, cy-.016, -.53), (0.0, cy-.016, -.83), .03, (0, 1, 0))
    glow_strip('gap', (0.0, cy+.016, -.53), (0.0, cy+.016, -.83), .03, (0, -1, 0))
    foregrip((0.0, .075, -.398), length=.085)
    box('fore-mount', (-.018, .075, -.42), (.018, .095, -.37), 'hull', bevel=.002)
    grip_and_trigger()
    points(grip_l=(0.0, .002, -.398), muzzle=(0.0, .115, -.835))


def drain_gun():
    """A wide intake nozzle at the front and a canister along the side."""
    receiver(.08, -.2, w=.038, butt=False)
    cy = .1
    tube('barrel', [(-.2, .035), (-.62, .035)], 'hull', caps=('hidden', 'hidden'), n=20, cy=cy)
    for z in (-.3, -.4, -.5):
        band('barrel-ring', .034, .042, z-.008, z+.008, 'steel', n=20, cy=cy)
    tube('intake', [(-.6, .04), (-.7, .06), (-.85, .1), (-.906, .104)], ['steel', 'hull', 'steel'], caps=('hidden', 'recess'),
         n=28, cy=cy, bevel=.003)
    tube('intake-throat', [(-.75, .045), (-.907, .09)], 'recess', caps=('recess', 'recess'), n=28, cy=cy, bevel=0)
    band('intake-glow', .05, .056, -.76, -.75, 'glow', n=28, cy=cy)
    tube('canister', [(-.15, .028), (-.17, .034), (-.42, .034), (-.44, .028)], ['steel', 'glass', 'steel'], caps=('hull', 'hull'),
         n=16, cx=-.06, cy=.06)
    tube('canister-glow', [(-.18, .022), (-.41, .022)], 'glow', caps=('glow', 'glow'), n=12, cx=-.06, cy=.06, bevel=0)
    foregrip((0.0, .03, -.31), length=.075)
    grip_and_trigger()
    points(grip_l=(0.0, -.049, -.31), muzzle=(0.0, .1, -.906))


def target_marker():
    """A small hand designator: a lens at the front, an antenna."""
    prism('body', [(.09, section(.034, .02, .13, .3, .2)), (-.33, section(.03, .03, .13, .3, .2))],
          lambda k, i: 'plate' if k in (2, 6) else 'hull', caps=('hull', 'hidden'), bevel=.004)
    tube('lens-housing', [(-.32, .04), (-.39, .045), (-.408, .043)], ['hull', 'steel'], caps=('hidden', 'glass'), n=24, cy=.097)
    tube('lens', [(-.405, .03), (-.409, .03)], 'glow', caps=('glow', 'glow'), n=24, cy=.097, bevel=0)
    hs.beam('antenna', (.025, .13, .06), (.03, .29, .08), .01, .01, 'steel', bevel=.002)
    hs.beam('antenna-tip', (.03, .28, .079), (.031, .3, .081), .016, .016, 'glow', bevel=0)
    for sx in (-1, 1):
        glow_strip('marker-light', (sx*.033, .09, -.1), (sx*.033, .09, -.25), .008, (sx, 0, 0))
    foregrip((0.0, .03, -.189), length=.06)
    grip_and_trigger()
    points(grip_l=(0.0, .017, -.189), muzzle=(0.0, .097, -.408))


def breacher():
    """A wide, flat, rectangular spread muzzle and a rack of shells on the
    side."""
    receiver(.2, -.14, w=.04)
    prism('barrel-block', [(-.13, section(.05, .035, .11, .25, .2)), (-.42, section(.06, .04, .105, .25, .2))], 'hull',
          caps=('hidden', 'hidden'), bevel=.004)
    prism('spreader', [(-.42, section(.062, .042, .102, .2, .2)), (-.49, section(.085, .047, .097, .2, .2)),
                       (-.508, section(.088, .05, .094, .2, .2))], (lambda k, i: 'steel' if i else 'hull'),
          caps=('hidden', 'recess'), bevel=.003)
    box('spread-slot', (-.078, .062, -.509), (.078, .082, -.49), 'recess', bevel=0)
    for k in range(5):
        z = -.18-.04*k
        tube('shell', [(z+.012, .011), (z-.012, .011)], 'amber', caps=('steel', 'steel'), n=10, cx=.058, cy=.07, bevel=.0015)
    box('shell-rack', (.044, .05, -.37), (.05, .09, -.15), 'hull', bevel=.002)
    for sx in (-1, 1):
        plate_x('block-panel', sx*.0505, .05, .095, -.16, -.4, 'plate', h=.003, out=sx)
    glow_strip('block-light', (-.052, .08, -.2), (-.055, .08, -.38), .006, (-1, 0, 0))
    grip_and_trigger()
    points(grip_l=(0.0, .068, -.26), muzzle=(0.0, .072, -.508))


def gust_cannon():
    """A wide bell mouth with turbine blades turning in its throat."""
    receiver(.06, -.2, w=.045, butt=False)
    cy = .186
    tube('body', [(-.15, .07), (-.45, .08), (-.55, .1)], ['hull', 'plate'], caps=('hull', 'hidden'), n=28, cy=cy)
    tube('bell', [(-.55, .1), (-.7, .13), (-.82, .18), (-.886, .205)], ['steel', 'hull', 'steel'], caps=('hidden', 'recess'),
         n=36, cy=cy, bevel=.004)
    tube('throat', [(-.6, .08), (-.887, .19)], 'recess', caps=('recess', 'recess'), n=36, cy=cy, bevel=0)
    tube('hub', [(-.6, .03), (-.66, .035), (-.68, .02)], 'steel', caps=('steel', 'steel'), n=16, cy=cy)
    for k in range(9):
        a = 2*math.pi*k/9
        hs.beam('blade', (math.cos(a)*.03, cy+math.sin(a)*.03, -.64), (math.cos(a+.3)*.085, cy+math.sin(a+.3)*.085, -.62),
                .012, .03, 'hull', bevel=.002)
    band('bell-glow', .19, .205, -.85, -.84, 'glow', n=36, cy=cy)
    for z in (-.25, -.35):
        band('body-ring', .07, .082, z-.012, z+.012, 'steel', n=28, cy=cy)
    prism('mount', [(-.08, section(.035, .1, .13, .2, .2)), (-.48, section(.035, .1, .13, .2, .2))], 'hull', caps=('hull', 'hull'))
    foregrip((0.0, .075, -.451), length=.08)
    box('fore-mount', (-.02, .075, -.47), (.02, .11, -.43), 'hull', bevel=.002)
    grip_and_trigger()
    points(grip_l=(0.0, -.005, -.451), muzzle=(0.0, .186, -.886))


def hailstorm():
    """Three big barrels in a triangular housing, fed by a belt from a
    drum at the back."""
    receiver(.2, -.2, w=.045)
    cy = .156
    for k in range(3):
        a = math.pi/2+2*math.pi*k/3
        bx, by = math.cos(a)*.04, cy-.03+math.sin(a)*.04
        tube('barrel', [(-.22, .022), (-.77, .022), (-.793, .02)], ['hull', 'steel'], caps=('hidden', 'recess'), n=16, cx=bx, cy=by)
        tube('bore', [(-.77, .012), (-.794, .012)], 'recess', caps=('recess', 'recess'), n=12, cx=bx, cy=by, bevel=0)
    for z in (-.25, -.45, -.68):
        prism('housing', [(z, [(math.cos(math.pi/2+2*math.pi*k/3+math.pi/3)*.085, cy-.03+math.sin(math.pi/2+2*math.pi*k/3+math.pi/3)*.085)
                               for k in (0, 1, 2)]),
                          (z-.05, [(math.cos(math.pi/2+2*math.pi*k/3+math.pi/3)*.085, cy-.03+math.sin(math.pi/2+2*math.pi*k/3+math.pi/3)*.085)
                                   for k in (0, 1, 2)])], 'plate', caps=('hull', 'hull'), bevel=.006)
    tube('drum', [(.05, .07), (.18, .07)], ['hull'], caps=('steel', 'steel'), n=24, cx=-.09, cy=.06)
    band('drum-ring', .068, .074, .1, .11, 'glow', n=24, cx=-.09, cy=.06)
    for k in range(6):
        hs.beam('belt', (-.09+.012*k, .13-.005*k, .02-.03*k), (-.075+.012*k, .13-.005*k, -.02-.03*k), .03, .012, 'amber', bevel=.002)
    foregrip((0.0, .075, -.479), length=.08)
    grip_and_trigger()
    points(grip_l=(0.0, .002, -.479), muzzle=(0.0, .156, -.793))


def hunter():
    """A shoulder launch tube with a targeting dish on top."""
    cy = .151
    tube('launch-tube', [(.41, .085), (.36, .095), (-.58, .095), (-.64, .1)], ['steel', 'hull', 'steel'], caps=('recess', 'recess'),
         n=28, cy=cy, bevel=.004)
    for z in (.25, .0, -.3, -.5):
        band('tube-band', .094, .104, z-.015, z+.015, 'plate', n=28, cy=cy)
    hs.quad_plate((-.04, cy+.096, -.25), (.04, cy+.096, -.25), (.04, cy+.096, .2), (-.04, cy+.096, .2), 0, 1, 0, 1, 'team', h=.003,
                  away=(0.0, cy, 0.0), name='team-stripe')
    # The targeting dish on a short mast.
    hs.beam('dish-mast', (-.06, cy+.06, -.1), (-.09, cy+.17, -.1), .016, .016, 'steel', bevel=.002)
    dish = Solid('dish', .002, smooth=35)
    loft(dish, [[(-.09+math.cos(2*math.pi*j/16)*r_, cy+.18+d, -.1+math.sin(2*math.pi*j/16)*r_) for j in range(16)]
                for r_, d in ((.01, .0), (.05, .02), (.065, .034))], 'plate', caps=('hull', 'recess'))
    hs.beam('dish-feed', (-.09, cy+.19, -.1), (-.09, cy+.235, -.1), .006, .006, 'glow', bevel=0)
    prism('frame', [(.12, section(.032, .02, cy-.07, .2, .2)), (-.35, section(.032, .02, cy-.07, .2, .2))], 'hull', caps=('hull', 'hull'))
    foregrip((0.0, .085, -.316), length=.07)
    grip_and_trigger()
    points(grip_l=(0.0, .023, -.316), muzzle=(0.0, .151, -.64))


def lance():
    """A long pointed emitter, stacked rings along it glowing as it
    charges."""
    receiver(.2, -.12, w=.038)
    cy = .11
    tube('emitter', [(-.1, .045), (-.2, .045), (-.62, .03), (-.745, .008)], ['hull', 'plate', 'steel'], caps=('hidden', 'glow'),
         n=24, cy=cy, bevel=.003)
    for k in range(8):
        z = -.24-.045*k
        r = .045-.015*(k/7)
        band('charge-ring', r-.004, r+.012, z-.008, z+.008, 'glow' if k % 2 == 0 else 'steel', n=24, cy=cy)
    tube('core', [(-.6, .006), (-.75, .006)], 'glow', caps=('glow', 'glow'), n=8, cy=cy, bevel=0)
    foregrip((0.0, .065, -.213), length=.07)
    grip_and_trigger()
    points(grip_l=(0.0, .004, -.213), muzzle=(0.0, .11, -.745))


def longbore():
    """A very long, thin barrel with a muzzle brake, and a large scope."""
    receiver(.28, -.18, w=.034)
    cy = .127
    tube('barrel', [(-.18, .02), (-.87, .014)], 'hull', caps=('hidden', 'hidden'), n=16, cy=cy)
    prism('brake', [(-.87, section(.024, cy-.022, cy+.022, .3, .3)), (-.946, section(.024, cy-.022, cy+.022, .3, .3))], 'hull',
          caps=('hidden', 'recess'))
    for z in (-.885, -.905, -.925):
        for sx in (-1, 1):
            x0, x1 = sorted((sx*.0242, sx*.0262))
            box('brake-slot', (x0, cy-.012, z-.004), (x1, cy+.012, z+.004), 'recess', bevel=0)
    prism('handguard', [(-.17, section(.034, .05, .15, .3, .25)), (-.45, section(.03, .06, .148, .3, .25))], 'plate',
          caps=('hidden', 'hull'), bevel=.004)
    box('scope-mount', (-.014, .118, -.12), (.014, .135, .06), 'hull', bevel=.002)
    tube('scope', [(.12, .022), (.09, .03), (-.16, .03), (-.21, .04), (-.23, .04)], ['steel', 'hull', 'hull', 'steel'],
         caps=('glass', 'glass'), n=20, cy=.17)
    band('scope-ring', .03, .036, -.05, -.03, 'steel', n=20, cy=.17)
    hs.beam('bipod', (-.01, .05, -.43), (-.03, -.03, -.45), .012, .012, 'hull', bevel=.002)
    hs.beam('bipod', (.01, .05, -.43), (.03, -.03, -.45), .012, .012, 'hull', bevel=.002)
    grip_and_trigger()
    points(grip_l=(0.0, .052, -.31), muzzle=(0.0, .127, -.946))


def multitool():
    """A three-pronged emitter claw on a tool head, and a gauge."""
    receiver(.12, -.1, w=.034, butt=False)
    cy = .056
    tube('head', [(-.09, .036), (-.32, .04), (-.36, .045)], ['hull', 'steel'], caps=('hidden', 'hull'), n=20, cy=cy)
    for k in range(3):
        a = math.pi/2+2*math.pi*k/3
        hs.beam('prong', (math.cos(a)*.03, cy+math.sin(a)*.03, -.35), (math.cos(a)*.044, cy+math.sin(a)*.044, -.458),
                .012, .01, 'steel', bevel=.002)
    tube('core', [(-.36, .014), (-.43, .014)], 'glow', caps=('glow', 'glow'), n=12, cy=cy, bevel=0)
    tube('gauge', [(-.15, .022), (-.16, .022)], 'hull', caps=('hull', 'glass'), n=16, cx=-.04, cy=.09)
    hs.quad_plate((-.04, .09-.016, -.161), (-.04, .09+.016, -.161), (-.04, .09+.016, -.161), (-.04, .09-.016, -.161), 0, 1, 0, 1,
                  'amber', h=.001, away=(-.04, .09, 0.0), name='gauge-needle')
    plate_y('tool-panel', cy+.041, -.02, .02, -.3, -.12, 'team', h=.002)
    grip_and_trigger()
    points(grip_l=(0.0, .082, -.242), muzzle=(0.0, .056, -.458))


def slugger():
    """A heavy hand cannon with a huge bore."""
    prism('frame', [(.08, section(.03, .018, .068, .25, .2)), (-.12, section(.03, .018, .068, .25, .2))], 'hull',
          caps=('hull', 'hidden'), bevel=.003)
    tube('barrel', [(-.02, .036), (-.28, .036), (-.309, .04)], ['hull', 'steel'], caps=('hidden', 'recess'), n=24, cy=.051)
    tube('bore', [(-.24, .024), (-.31, .024)], 'recess', caps=('recess', 'recess'), n=20, cy=.051, bevel=0)
    prism('top-strap', [(.07, section(.026, .085, .1, .3, .1)), (-.27, section(.022, .085, .095, .3, .1))], 'plate',
          caps=('hull', 'hull'), bevel=.002)
    for sx in (-1, 1):
        glow_strip('barrel-light', (sx*.037, .051, -.06), (sx*.037, .051, -.22), .006, (sx, 0, 0))
    handle('grip', (0.0, .03, .0), (0.0, -.09, .04), w=.03, d=.044)
    trigger_group(front=-.07, back=-.008, y=.008, w=.02)
    points(grip_l=(0.0, .048, -.168), muzzle=(0.0, .051, -.309))


def spark_rifle():
    """Forked electrode prongs at the muzzle, insulator rings along the
    barrel."""
    receiver(.2, -.12, w=.036)
    cy = .076
    tube('barrel', [(-.11, .022), (-.62, .018)], 'hull', caps=('hidden', 'hidden'), n=16, cy=cy)
    for k in range(7):
        z = -.16-.06*k
        tube('insulator', [(z+.014, .024), (z, .034), (z-.014, .024)], ['light', 'light'], caps=('hidden', 'hidden'), n=16, cy=cy)
    for sx in (-1, 1):
        hs.beam('electrode', (sx*.012, cy, -.6), (sx*.03, cy+.006, -.704), .008, .012, 'steel', bevel=.002)
        hs.beam('electrode-tip', (sx*.03, cy+.006, -.69), (sx*.03, cy+.006, -.706), .01, .014, 'glow', bevel=0)
    foregrip((0.0, .045, -.154), length=.06)
    grip_and_trigger()
    points(grip_l=(0.0, -.017, -.154), muzzle=(0.0, .076, -.704))


def tether():
    """A barbed hook on the barrel's tip, a cable spool on the side."""
    receiver(.16, -.15, w=.036)
    cy = .124
    tube('barrel', [(-.14, .026), (-.72, .022)], 'hull', caps=('hidden', 'hidden'), n=16, cy=cy)
    tube('hook-shank', [(-.72, .012), (-.77, .012)], 'steel', caps=('hidden', 'hidden'), n=12, cy=cy)
    for k in range(3):
        a = 2*math.pi*k/3
        hs.beam('barb', (math.cos(a)*.01, cy+math.sin(a)*.01, -.788), (math.cos(a)*.04, cy+math.sin(a)*.04, -.75), .008, .006,
                'steel', bevel=.0015)
    spool = Solid('spool', .003, smooth=30)
    loft(spool, [[(.04+d, .07+math.cos(2*math.pi*j/20)*r_, -.25+math.sin(2*math.pi*j/20)*r_) for j in range(20)]
                 for d, r_ in ((0.0, .055), (.01, .055), (.01, .04), (.04, .04), (.04, .055), (.05, .055))],
         lambda k, i: ['hull', 'steel', 'amber', 'steel', 'hull'][i], caps=('hidden', 'hull'))
    foregrip((0.0, .055, -.41), length=.07)
    grip_and_trigger()
    points(grip_l=(0.0, -.015, -.41), muzzle=(0.0, .124, -.788))


def torch():
    """Twin fuel cylinders under the body, a nozzle with a pilot light and a
    heat-shield ring."""
    receiver(.12, -.2, w=.034, butt=False)
    cy = .121
    tube('barrel', [(-.19, .024), (-.74, .02)], 'hull', caps=('hidden', 'hidden'), n=16, cy=cy)
    tube('nozzle', [(-.74, .022), (-.8, .03), (-.823, .028)], ['steel', 'steel'], caps=('hidden', 'recess'), n=16, cy=cy)
    band('heat-shield', .03, .06, -.71, -.69, 'steel', n=24, cy=cy)
    hs.beam('pilot', (.0, cy-.03, -.76), (.0, cy-.03, -.82), .008, .008, 'steel', bevel=.001)
    hs.beam('pilot-flame', (.0, cy-.03, -.818), (.0, cy-.03, -.83), .01, .01, 'glow', bevel=0)
    for sx in (-1, 1):
        tube('fuel', [(-.2, .026), (-.24, .03), (-.56, .03), (-.6, .026)], ['steel', 'plate', 'steel'], caps=('hull', 'hull'),
             n=16, cx=sx*.032, cy=.045)
        band('fuel-mark', .029, .032, -.27, -.26, 'amber', n=16, cx=sx*.032, cy=.045)
    foregrip((0.0, .02, -.344), length=.07)
    grip_and_trigger()
    points(grip_l=(0.0, -.047, -.344), muzzle=(0.0, .121, -.823))


WEAPONS = {'disc': disc, 'chaingun': chaingun, 'grenade_launcher': grenade_launcher, 'plasma': plasma, 'blaster': blaster,
           'mortar': mortar, 'laser': laser, 'railgun': railgun, 'drain-gun': drain_gun, 'target-marker': target_marker,
           'breacher': breacher, 'gust-cannon': gust_cannon, 'hailstorm': hailstorm, 'hunter': hunter, 'lance': lance,
           'longbore': longbore, 'multitool': multitool, 'slugger': slugger, 'spark-rifle': spark_rifle, 'tether': tether,
           'torch': torch}

setup()
root('body'); WEAPONS[WEAPON]()
finish(OUT, TEX, generator='PeakRunner scripts/models/weapon_models.py', material=f'weapon-{WEAPON}')
