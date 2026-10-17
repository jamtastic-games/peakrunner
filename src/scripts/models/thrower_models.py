"""Football's thrower as a hard-surface model (docs/model-pipeline.md,
"Hard-surface structures"; docs/football.md), built in Blender headless:

    blender -b -P scripts/models/thrower_models.py -- OUT.glb [--texture 1024]

`assets/models/equipment/thrower/model.glb`, drawn in first and third person
in place of the built-in parts. Its frame is the old built-in thrower's: -Z
forward, +Y up, origin at the middle of the body, the pistol grip under the
receiver. One part, `body`: a pale receiver with a short stock and a team
stripe, a coil housing ringed by three amber rings (lit brighter while the
holder carries the ball), a flared muzzle with four clamp petals round a
wide bore, a spine rail on top, and a pistol grip with a trigger guard.
Points: `grip_r` (the pistol grip), `grip_l` (the support hand's place on
the coil housing's left side; the game puts the palm a little further
out), `muzzle` (the bore's mouth).
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import Solid, finish, loft, root, setup, solid_box

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 1024
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.002, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.FLAT_TEXEL = .35
hs.PBR = True                # metal, roughness, occlusion and normal maps for the model shader


def ring_z(r, z, n=12, cx=0.0, cy=0.0, phase=None):
    """A ring of n points round the Z axis at depth z."""
    a0 = math.pi/n if phase is None else phase
    return [(cx+r*math.cos(a0+2*math.pi*k/n), cy+r*math.sin(a0+2*math.pi*k/n), z) for k in range(n)]


def tube_z(name, sections, mats, caps, bevel=.004, n=12, cy=0.0):
    """A solid round the Z axis through (z, r) sections."""
    s = Solid(name, bevel)
    loft(s, [ring_z(r, z, n, cy=cy) for z, r in sections], mats, caps=caps)
    return s


def band_z(name, r0, r1, z0, z1, mat, n=12, cy=0.0):
    """A flat ring round the Z axis: radii r0..r1 from z0 to z1."""
    s = Solid(name, 0)
    loft(s, [ring_z(r, z, n, cy=cy) for z, r in ((z0, r1), (z1, r1), (z1, r0), (z0, r0))], mat, caps=(None, None))
    a, b = len(s.v)-n, len(s.v)-4*n
    for k in range(n):
        s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)
    return hs.flat(s, mat)


def body():
    # Receiver: a chamfered block from the stock forward to the coil, pale
    # over a dark belly.
    s = Solid('receiver', .012)
    sec = lambda z, w, top, bot: [(-w, bot), (w, bot), (w*1.05, (top+bot)/2), (w*.82, top), (-w*.82, top), (-w*1.05, (top+bot)/2)]
    loft(s, [[(x, y, z) for x, y in sec(z, w, t, b)] for z, w, t, b in
             ((.33, .05, .07, -.06), (.2, .062, .085, -.075), (-.02, .062, .085, -.075), (-.07, .056, .07, -.065))],
         lambda k, i: 'hull' if k in (0, 1, 5) else 'light', caps=('hull', 'hidden'))
    solid_box('stock-pad', (-.046, -.055, .33), (.046, .062, .37), 'rubber', bevel=.008)
    for side in (-1, 1):
        x0, x1 = sorted((side*.063, side*.068))
        solid_box('team-stripe', (x0, .012, -.01), (x1, .032, .26), 'team', bevel=0)
    # Status lens and chevrons on the left side (the side first person sees).
    hs.quad_plate((-.0655, .03, .05), (-.0655, .03, .1), (-.0655, .055, .1), (-.0655, .055, .05), 0, 1, 0, 1,
                  'glow', h=.004, away=(0.0, 0.0, 0.0), name='lens')
    for k in range(3):
        z = -.0+.035*k
        hs.quad_plate((-.0655, -.045, z), (-.0655, -.045, z+.02), (-.0655, -.03, z+.02), (-.0655, -.03, z), 0, 1, 0, 1,
                      'amber', h=.003, away=(0.0, 0.0, 0.0), name='chevron')
    # Spine rail along the top, from the receiver over the coil.
    solid_box('spine', (-.018, .085, -.33), (.018, .105, .24), 'plate', bevel=.005)
    for z in (-.3, -.18, -.06, .06, .18):
        solid_box('spine-clamp', (-.03, .072, z-.012), (.03, .1, z+.012), 'hull', bevel=.004)
    # Coil housing round the bore, three amber rings, and the flared muzzle
    # with four clamp petals round a wide dark bore.
    tube_z('coil', ((-.06, .068), (-.08, .076), (-.34, .076), (-.36, .07)), 'hull', ('hidden', 'hull'), n=12)
    for z in (-.12, -.2, -.28):
        band_z('coil-ring', .074, .086, z-.018, z+.018, 'amber', n=12)
    tube_z('muzzle', ((-.355, .072), (-.4, .092), (-.45, .1), (-.462, .096)), 'steel', ('hidden', 'recess'), n=12)
    tube_z('bore', ((-.38, .056), (-.464, .062)), 'recess', ('recess', 'recess'), n=12, bevel=0)
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        c, sn = math.cos(a), math.sin(a)
        hs.beam('petal', (c*.095, sn*.095, -.39), (c*.118, sn*.118, -.47), .03, .012, 'plate', bevel=.003)
    # Pistol grip and trigger guard under the receiver.
    hs.beam('grip', (0.0, -.06, .16), (0.0, -.2, .205), .042, .058, 'rubber', bevel=.008)
    hs.beam('guard', (0.0, -.075, .07), (0.0, -.075, .14), .03, .012, 'hull', bevel=.003)
    hs.beam('guard', (0.0, -.075, .07), (0.0, -.11, .1), .03, .012, 'hull', bevel=.003)
    hs.beam('trigger', (0.0, -.07, .115), (0.0, -.095, .12), .012, .008, 'steel', bevel=.002)
    hs.POINTS.extend([('grip_r', (0.0, -.135, .185)), ('grip_l', (-.03, 0.0, -.2)), ('muzzle', (0.0, 0.0, -.465))])


setup()
root('body'); body()
finish(OUT, TEX, generator='PeakRunner scripts/models/thrower_models.py', material='thrower')
