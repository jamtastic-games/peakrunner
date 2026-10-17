"""Dustreach's citadels as hard-surface detail (docs/model-pipeline.md,
"Hard-surface structures"), built in Blender headless:

    blender -b -P scripts/models/dustreach_models.py -- OUT.glb [--texture 4096]

Three roots, each in its builder's frame and placed where the map puts it:
`dr-citadel` (scripts/assets/dustreach_citadel.py, on both bases), `dr-gate`
(the Sun Gate, scripts/assets/dustreach_gate.py) and `dr-ruin-wall` (each
broken midfield wall, its `build_piece`). The walls, floors and roofs stay
the map builder's (drawn, lightmapped, solid); this detail is made from the
builder's own boxes (scripts/assets/cladding.py: the outdoor faces and open
tops nothing covers) and stands within a few centimeters of them:

- copings on every wall and merlon top, flush with the top, lipped over the
  faces that drop away; trims round the exposed edges of slabs and roofs;
- an amber light line under each coping, fins up the long faces (clear of the
  team banners), a skirt where a face meets the ground or a floor, and guards
  on the convex corners.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, '..'))
import hardsurface as hs
from hardsurface import finish, root, setup
from cladding_models import Style, clad, recorded
from assets import dustreach_citadel as cit
from assets import dustreach_gate as gate

args = sys.argv[sys.argv.index('--')+1:]
OUT = args[0]
TEX = int(args[args.index('--texture')+1]) if '--texture' in args else 4096
hs.PLATE_BACKS = False
hs.PACK = dict(margin=.001, margin_method='FRACTION', shape_method='AABB', rotate=True)
hs.GRATE = (.3, .3)
hs.FLAT_TEXEL = .25
hs.SPREAD = 200.0             # the roots overlap round their origins: bake them apart
STYLE = Style(ground=-.1, light='amber')   # the builder blends the ground to -0.1 under structures


setup()
root('dr-citadel')
clad(recorded(lambda m: cit.build(m, 0, 'red')), STYLE)
root('dr-gate')
clad(recorded(gate.build), STYLE)
root('dr-ruin-wall')
clad(recorded(lambda m: gate.build_piece(m, 'wall')), STYLE)
finish(OUT, TEX, generator='PeakRunner scripts/models/dustreach_models.py', material='dustreach')
