"""The sensor's radar head (docs/model-pipeline.md, "Hard-surface
structures"), built in Blender headless:

    blender -b -P scripts/models/sensor_models.py -- OUT.glb [--texture 1024]

`assets/models/equipment/sensor/model.glb`, drawn by the game on top of every
sensor's mast (the map's kit draws the base and mast; `drawlist`). Its frame:
origin on the mast's axis at the mast's top (7 m over the sensor's foot), Y up,
the antenna facing -Z. Two parts:
- `collar`: a sleeve round the mast top with a lit ring (stays put);
- `head`: the turntable, yoke and a tilted slotted antenna array with a lit
  emitter line, team-painted end caps and a reflector truss behind (it
  turns about Y while the sensor is powered).
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hardsurface as hs
from hardsurface import Solid, circle_y, finish, loft, quad_plate, root, setup, solid_box

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 1024
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.002, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.FLAT_TEXEL = .35
hs.GRATE = (.04, .5)
hs.SPREAD = 8.0


def octagon(name, r0, r1, y0, y1, mats, caps, bevel=.02):
    s = Solid(name, bevel)
    loft(s, [circle_y(0.0, 0.0, r0, y0, 8), circle_y(0.0, 0.0, r1, y1, 8)], mats, caps=caps)
    return s


def tilt(p, a, pivot):
    """`p` turned back by `a` radians about the X axis through `pivot`."""
    x, y, z = p
    py, pz = pivot
    dy, dz = y-py, z-pz
    c, s = math.cos(a), math.sin(a)
    return (x, py+dy*c-dz*s, pz+dy*s+dz*c)


def collar():
    octagon('collar', .52, .48, -.75, 0.0, 'plate', ('hidden', 'hidden'))
    hs.ring_solid('collar-light', 0.0, 0.0, .5, .54, -.42, -.36, 8, 'glow')


def head():
    # Turntable and yoke.
    octagon('turntable', .66, .6, 0.0, .26, 'hull', ('hidden', 'plate'))
    hs.ring_solid('turntable-light', 0.0, 0.0, .62, .67, .1, .15, 8, 'glow')
    for sx in (-1, 1):
        x0, x1 = sorted((sx*.28, sx*.42))
        solid_box('yoke', (x0, .25, -.12), (x1, .86, .14), {'*': 'plate', '-y': 'hidden'}, bevel=.02)
    # The antenna array: a long panel tipped back 15 degrees, slotted on its
    # face with a lit emitter line, team caps on its ends.
    a = math.radians(15)
    pivot = (1.18, 0.0)
    W, H, D = 2.4, .5, .09
    def box_pts(x0, x1, y0, y1, z0, z1):
        return [tilt(p, a, pivot) for p in ((x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
                                            (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1))]
    def tilted_box(name, lo, hi, mats, bevel=.02):
        s = Solid(name, bevel)
        v = s.pts(box_pts(lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]))
        for f, m in (((0, 3, 2, 1), mats.get('-z', mats['*'])), ((4, 5, 6, 7), mats.get('+z', mats['*'])),
                     ((0, 1, 5, 4), mats.get('-y', mats['*'])), ((3, 7, 6, 2), mats.get('+y', mats['*'])),
                     ((0, 4, 7, 3), mats.get('-x', mats['*'])), ((1, 2, 6, 5), mats.get('+x', mats['*']))):
            s.face([v[i] for i in f], m)
        return s
    y0, y1 = pivot[0]-H, pivot[0]+H
    tilted_box('array', (-W+.2, y0, -D), (W-.2, y1, D), {'*': 'hull', '-z': 'grille', '+z': 'plate'})
    for sx in (-1, 1):
        lo, hi = sorted((sx*(W-.2), sx*W))
        tilted_box('array-cap', (lo, y0-.04, -D-.03), (hi, y1+.04, D+.03), {'*': 'team'})
    # The emitter line across the face, and the feed under it.
    tilted_box('emitter', (-W+.35, pivot[0]-.04, -D-.02), (W-.35, pivot[0]+.04, -D+.01), {'*': 'glow'}, bevel=0)
    tilted_box('feed', (-.16, y0-.26, -D-.22), (.16, y0+.02, -D+.02), {'*': 'plate', '-z': 'glass'})
    # Reflector truss behind: two diagonals and a spine to the yoke.
    back = lambda x, y: tilt((x, y, D+.02), a, pivot)
    for sx in (-1, 1):
        hs.beam('truss', back(0.0, pivot[0]), back(sx*(W-.3), y1-.08), .06, .06, 'steel', bevel=0)
        hs.beam('truss', back(0.0, pivot[0]), back(sx*(W-.3), y0+.08), .06, .06, 'steel', bevel=0)
    hs.beam('spine', (0.0, .8, .02), back(0.0, pivot[0]), .12, .12, 'plate', bevel=.01)


setup()
root('collar'); collar()
root('head'); head()
finish(OUT, TEX, generator='PeakRunner scripts/models/sensor_models.py', material='sensor')
