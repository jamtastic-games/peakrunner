"""Frostline's stations as hard-surface detail (docs/model-pipeline.md,
"Hard-surface structures"), built in Blender headless:

    blender -b -P scripts/models/frostline_models.py -- OUT.glb [--texture 4096]

One root, `fl-station`, in a base's own frame (scripts/assets/frostline_station.py:
the station, the relay outpost, the plasma emplacement and the vehicle
apron), placed on both bases. The builder's walls, floors and roofs stay
drawn, lightmapped and solid; the detail is generated from its own boxes
(scripts/models/cladding_models.py): graphite copings and trims, cyan light
lines, light-gray fins clear of the team panels, skirts and corner guards,
all within a few centimeters of the solid; a coping ring and light line on
the emplacement's octagon by hand. The beacon (a lattice mast) has none.
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
from assets import frostline_station as st

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 4096
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.001, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.GRATE = (.3, .3)
hs.FLAT_TEXEL = .25


def ground(x, z):
    """The shelf under the station and apron; the outpost's and the
    emplacement's own ground on their shoulders."""
    for (cx, cz), g in ((st.OUTPOST, st.OG), (st.EMPLACEMENT, st.EG)):
        if math.hypot(x-cx, z-cz) < 24.0: return g-.1
    return st.G-.1


STYLE = Style(ground=ground, light='glow', fin='light', banner_height=1.2)


def emplacement():
    """The plasma emplacement is an octagonal prism, which the generator
    leaves alone: a coping ring round its top edge and a light line under
    its team band, by hand."""
    ex, ez = st.EMPLACEMENT
    top = st.EG+st.E_H
    r = lambda y: st.E_R0+(st.E_R1-st.E_R0)*(y-(st.EG-.4))/(top-(st.EG-.4))   # its corner radius at y
    hs.ring_solid('emplacement-coping', ex, ez, st.E_R1-.45, st.E_R1+.09, top-.2, top+.012, 8, 'hull')
    y = top-.74
    hs.ring_solid('emplacement-light', ex, ez, r(y)-.02, r(y)+.03, y-.04, y+.04, 8, 'glow')


setup()
root('fl-station')
clad(recorded(lambda m: st.build(m, 0, 'red')), STYLE)
emplacement()
finish(OUT, TEX, generator='PeakRunner scripts/models/frostline_models.py', material='frostline')
