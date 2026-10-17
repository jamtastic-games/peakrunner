"""Original Frostline landmark: a lit navigation beacon on the central ridge,
the one thing players can find in a whiteout. A lattice mast on four raked
legs carries a perch platform at 12 m and a lantern cage at the top; four
low windbreak walls round the foot give cover. Four-fold symmetric, so it is
the same from either base.

It is also Frostline's Capture & Hold centre point (the manifest's
`beacon` control point, active in CTF with a drain field). The shared C&H
tower (cnh_tower.py) stands on the same origin: its pylon rises inside the
lattice legs, so the perch is a ring round the pylon, and the tower's cover
walls and ring posts sit inside the windbreaks. A render-only drain emitter
round the perch (four lit projector heads angled down at the field, and a
halo under the lantern) reads as the thing that drains an enemy's energy
while the point is held.

Local origin is the knob's ground level; the terrain is flattened around it
to SITE_R.
"""
import math

from assets import cnh_tower
from assets.structure_kit import Builder

ASSET_ID = 'frostline-beacon-v2'
SITE_R = 20.0
PAD_R, PAD_TOP = 9.0, .02
LEG_FOOT, LEG_TOP, MAST_H = 4.0, 1.2, 26.0
PERCH_Y, PERCH_HALF, PERCH_HOLE = 12.4, 3.6, 1.8
BREAK_R, BREAK_H, BREAK_SPAN = 13.0, 1.3, math.radians(12)
METAL, GRATE, GLOW, HULL = 'trim', 'grate', 'light', 'concrete'


def leg_offset(y):
    """Half-spread of the raked legs at height y."""
    return LEG_FOOT+(LEG_TOP-LEG_FOOT)*y/MAST_H


def _strut(mesh, p0, p1, r, mat, solid=True):
    """Square prism of half-width r between two points (rising legs)."""
    (x0, y0, z0), (x1, y1, z1) = p0, p1
    c0 = [(x0-r, y0, z0-r), (x0+r, y0, z0-r), (x0+r, y0, z0+r), (x0-r, y0, z0+r)]
    c1 = [(x1-r, y1, z1-r), (x1+r, y1, z1-r), (x1+r, y1, z1+r), (x1-r, y1, z1+r)]
    for i in range(4):
        j = (i+1) % 4
        mesh.quad(c0[i], c1[i], c1[j], c0[j], mat, solid)
    mesh.quad(*c1[::-1], mat, solid)


def build(mesh):
    b = Builder(mesh, 'light', metal=METAL, glow=GLOW)
    b.prism(0, 0, PAD_R, PAD_R, PAD_TOP-.6, PAD_TOP, 8, HULL, phase=math.pi/8)
    b.prism(0, 0, PAD_R+.05, PAD_R+.05, PAD_TOP-.35, PAD_TOP-.05, 8, METAL, phase=math.pi/8, solid=False, cap=False)
    for sx in (-1, 1):
        for sz in (-1, 1):
            _strut(mesh, (sx*LEG_FOOT, PAD_TOP, sz*LEG_FOOT), (sx*LEG_TOP, MAST_H, sz*LEG_TOP), .28, METAL)
    # Cross bracing, render only.
    for y in range(3, int(MAST_H), 4):
        o = leg_offset(y)
        for axis in ('x', 'z'):
            for s in (-1, 1):
                if axis == 'x': mesh.box((0, y, s*o), (2*o, .1, .1), METAL, False)
                else: mesh.box((s*o, y, 0), (.1, .1, 2*o), METAL, False)
    # Perch: a ring round the C&H pylon (four slabs about a square hole).
    h, o = PERCH_HOLE, PERCH_HALF
    b.slab(-o, o, -o, -h, PERCH_Y, GRATE, .4)
    b.slab(-o, o, h, o, PERCH_Y, GRATE, .4)
    b.slab(-o, -h, -h, h, PERCH_Y, GRATE, .4)
    b.slab(h, o, -h, h, PERCH_Y, GRATE, .4)
    for s in (-1, 1):
        mesh.box((0, PERCH_Y+1.0, s*PERCH_HALF), (2*PERCH_HALF, .08, .08), METAL, False)
        mesh.box((s*PERCH_HALF, PERCH_Y+1.0, 0), (.08, .08, 2*PERCH_HALF), METAL, False)
    # Lantern cage and warning lamps.
    mesh.box((0, MAST_H+.2, 0), (3.2, .3, 3.2), METAL, False)
    mesh.box((0, MAST_H+1.3, 0), (1.6, 1.8, 1.6), GLOW, False)
    mesh.box((0, MAST_H+2.4, 0), (2.4, .25, 2.4), METAL, False)
    b.lamp((0, MAST_H+1.3, 0), 1.2)
    for sx in (-1, 1):
        for sz in (-1, 1):
            o = leg_offset(18.0)
            mesh.box((sx*o, 18.0, sz*o), (.5, .5, .5), GLOW, False)
    b.lamp((0, PERCH_Y+2.5, 0), .6)
    for s in (-1, 1):
        mesh.box((s*(PAD_R-1.2), PAD_TOP+.04, 0), (.3, .04, 3.0), GLOW, False)
        mesh.box((0, PAD_TOP+.04, s*(PAD_R-1.2)), (3.0, .04, .3), GLOW, False)
    # Windbreaks on the diagonals.
    walls = []
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        p0 = (BREAK_R*math.cos(a-BREAK_SPAN), BREAK_R*math.sin(a-BREAK_SPAN))
        p1 = (BREAK_R*math.cos(a+BREAK_SPAN), BREAK_R*math.sin(a+BREAK_SPAN))
        b.beam(p0, p1, .7, -.4, BREAK_H, HULL)
        b.beam(p0, p1, .8, BREAK_H-.12, BREAK_H+.04, METAL, solid=False)
        walls.append((p0, p1))
    # The Capture & Hold tower on the same origin, and the drain emitter.
    point = cnh_tower.build(mesh)
    _drain_emitter(mesh, b)
    return {'lantern': (0, MAST_H+1.3, 0), 'perch': (0, PERCH_Y, 0), 'point': point['point'],
            'point_beacon': point['beacon'],
            'windbreaks': [((a[0]+c[0])/2, 0, (a[1]+c[1])/2) for a, c in walls]}


EMITTER_Y = PERCH_Y+1.6


def _drain_emitter(mesh, b):
    """Render-only: four projector heads on arms off the perch rail, each
    angled down and out at the field, and a lit halo ring under the lantern."""
    for k in range(4):
        a = k*math.pi/2+math.pi/4
        dx, dz = math.cos(a), math.sin(a)
        r0 = PERCH_HALF*1.3
        base = (dx*r0, EMITTER_Y, dz*r0)
        mesh.box((dx*(r0-.9), EMITTER_Y, dz*(r0-.9)), (.18, .18, .18), METAL, False)
        # Projector: a short tapered barrel pointing down-and-out, lit at the lens.
        tip = (dx*(r0+1.4), EMITTER_Y-.9, dz*(r0+1.4))
        side = (-dz*.35, 0, dx*.35)
        for sgn in (1, -1):
            up = (0, .35*sgn, 0)
            mesh.quad(tuple(base[i]+side[i]*sgn+up[i] for i in range(3)), tuple(base[i]-side[i]*sgn+up[i] for i in range(3)),
                      tuple(tip[i]-side[i]*sgn*.6+up[i]*.6 for i in range(3)), tuple(tip[i]+side[i]*sgn*.6+up[i]*.6 for i in range(3)),
                      METAL, False)
        mesh.box(tip, (.5, .5, .5), GLOW, False)
        b.lamp(tip, .5)
    for k in range(12):
        a0, a1 = k*math.pi/6, (k+1)*math.pi/6
        r, y = 2.2, MAST_H-1.2
        p0, p1 = (r*math.cos(a0), y, r*math.sin(a0)), (r*math.cos(a1), y, r*math.sin(a1))
        mesh.quad(p0, p1, (p1[0], y+.18, p1[2]), (p0[0], y+.18, p0[2]), GLOW, False)
        mesh.quad(p1, p0, (p0[0], y+.18, p0[2]), (p1[0], y+.18, p1[2]), GLOW, False)
