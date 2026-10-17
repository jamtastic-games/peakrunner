"""Original neutral landmark for Reefbreak (docs/reefbreak.md).

The lighthouse (build_lighthouse) on the central island: a tapered white
tower with a railed gallery near the top (the map's sniping spot, reached
by jetting) and a glowing lantern room above it. A low keeper's wall
round its foot gives cover. Local frame: ground Y=0 at its foot.

No source geometry is used. Dressing is render-only. If the caller's mesh
has a `lamps` list, light fixtures add bake samples.
"""
import math

from assets.structure_kit import Builder

LIGHTHOUSE_ID = 'reefbreak-lighthouse-v1'
R0, R1, TOP = 5.2, 3.6, 26.0
GALLERY_Y, GALLERY_R = 24.0, 6.4
LANTERN = (TOP, TOP+4.2, 2.8)
YARD_R, YARD_H = 12.0, 1.3
YARD_GAPS = (0.0, 180.0)                 # openings in the yard wall (degrees about +Z)

HULL, METAL, GLOW, ROCK, GRATE, PANEL = 'concrete', 'trim', 'light', 'rock', 'grate', 'panel'


def build_lighthouse(mesh):
    b = Builder(mesh, 'light', interior=HULL, metal=METAL, glow=GLOW)
    b.prism(0, 0, R0, R1, -3.0, TOP, 12, HULL, phase=math.pi/12)
    taper = lambda y: R0+(R1-R0)*(y+3.0)/(TOP+3.0)
    for y0, y1 in ((6.0, 8.0), (15.0, 17.0)):
        r = taper(y0)+.04
        b.prism(0, 0, r, taper(y1)+.04, y0, y1, 12, 'ember', phase=math.pi/12, solid=False, cap=False)
    b.prism(0, 0, GALLERY_R, GALLERY_R, GALLERY_Y-.6, GALLERY_Y, 16, METAL)
    # Railing: posts and a rail, solid to waist height so it's cover.
    b.prism(0, 0, GALLERY_R, GALLERY_R, GALLERY_Y, GALLERY_Y+1.1, 16, GRATE, cap=False)
    y0, y1, r = LANTERN
    b.prism(0, 0, r, r, y0, y1, 10, GLOW, solid=False)
    b.prism(0, 0, r+.3, 1.0, y1, y1+1.6, 10, METAL)
    b.lamp((0.0, (y0+y1)/2, 0.0), 2.0)
    b.lamp((0.0, GALLERY_Y+1.5, GALLERY_R-.8), .9)
    # Keeper's yard wall with two openings.
    n = 24
    for i in range(n):
        a0, a1 = i*360.0/n, (i+1)*360.0/n
        mid = (a0+a1)/2
        if any(abs((mid-g+180) % 360-180) < 12 for g in YARD_GAPS): continue
        p0 = (YARD_R*math.sin(math.radians(a0)), YARD_R*math.cos(math.radians(a0)))
        p1 = (YARD_R*math.sin(math.radians(a1)), YARD_R*math.cos(math.radians(a1)))
        b.beam(p0, p1, .8, -1.0, YARD_H, ROCK)
    return dict(gallery=(0.0, GALLERY_Y, GALLERY_R-1.0), yard=(0.0, 0.0, YARD_R-3.0))
