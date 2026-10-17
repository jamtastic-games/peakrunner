"""Cairnhold's bases as hard-surface detail (docs/model-pipeline.md,
"Hard-surface structures"), built in Blender headless:

    blender -b -P scripts/models/cairnhold_models.py -- OUT.glb [--texture 4096]

One root, `ch-base`, in a base's own frame (scripts/assets/cairnhold_base.py:
the bunker and its gatehouse, the trench, the guard hut and flag tower, the
battery and the ledge), placed on both bases. The builder's walls, floors
and roofs stay drawn, lightmapped and solid; the detail is generated from
its own boxes (scripts/models/cladding_models.py): steel copings and trims
over the graphite plate, amber light lines, light fins clear of the team
banners, skirts and corner guards, all within a few centimeters of the
solid. The Ring (prisms) has none.
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, '..'))
import hardsurface as hs
from hardsurface import finish, root, setup
from cladding_models import Style, clad, recorded
from assets import cairnhold_base as cb

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 4096
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.001, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.GRATE = (.3, .3)
hs.FLAT_TEXEL = .25


def ground(x, z):
    """The knoll round the guard hut and tower, the battery's bench, and the
    bunker's floor level elsewhere (the dug-in walls' buried parts get detail
    nobody sees)."""
    if cb.HX0-4 <= x <= cb.HX1+4 and cb.HZ0-4 <= z <= cb.HZ1+12: return cb.HUT_RING-.1
    if math.hypot(x-cb.BATTERY[0], z-cb.BATTERY[1]) < 16: return cb.BATTERY_GROUND-.1
    return -.1


STYLE = Style(ground=ground, light='amber', fin='light', cap='steel', guard='light', banner_height=1.2)

setup()
root('ch-base')
clad(recorded(lambda m: cb.build(m, 0, 'red')), STYLE)
finish(OUT, TEX, generator='PeakRunner scripts/models/cairnhold_models.py', material='cairnhold')
