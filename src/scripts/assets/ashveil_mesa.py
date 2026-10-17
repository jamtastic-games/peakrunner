"""Ashveil's east base: a mesa of basalt columns (docs/ashveil.md), our
own design.

Hexagonal columns, packed as lava cracks them when it cools, stand on the
shelf in the east: a flat top in the middle where team 0's flag stands on
an open altar, terraces stepping down toward the temple on the west, and a
taller rank behind it on the east. Along the south side the columns hang
over a grotto: the lower ones broke away, leaving a hall 8 m high whose
roof is the columns' ends, open to the south and the west, where the base's
two stations stand. Braziers burn on the top's edge.

Everything is in world metres; bearings are degrees from +X toward +Z.
"""
import math

import numpy as np

from assets import ashveil_shapes as A
from assets import ashveil_tower as TW

R = 1.72               # a column's circumradius (2.98 m flat to flat)
PITCH = 3.06           # centre to centre across a row
TOP = 16.0             # the flat top over the shelf
FLAT = 7.6             # its radius
REACH = 20.5           # the columns' footprint
TERRACE = 1.6          # the giant's steps
STAIR = (135.0, 225.0)  # bearings of the stepped side, toward the temple
GROTTO = (55.0, 142.0, 8.2, 15.5)   # bearings and radii of the hall under the south side
ROOF = 8.5             # its height
MOUTH = (82.0, 100.0)  # the opening in its outer wall (bearings); its west end opens onto the steps


def hexagon(x, z, y0, y1, material='leaf', solid=True, tilt=(0.0, 0.0)):
    """A column: a hexagonal prism (pointy along z) from y0 to y1, its top
    tipped by `tilt` (rise per metre along x and z)."""
    a = np.radians(30+60*np.arange(6))
    ring = np.c_[x+R*np.cos(a), z+R*np.sin(a)]
    form = A.extrude(ring, y0, y1, material, solid, axis='y')
    if tilt != (0.0, 0.0):
        top = form.v[:, 1] > y1-1e-6
        form.v[top, 1] += (form.v[top, 0]-x)*tilt[0]+(form.v[top, 2]-z)*tilt[1]
    return form


def bearing_of(dx, dz):
    return math.degrees(math.atan2(dz, dx)) % 360.0


def in_arc(b, a0, a1):
    return (b-a0) % 360.0 <= (a1-a0) % 360.0


def build(center, shelf, ground, seed=0):
    """The mesa round `center` (x, z) on a shelf at `shelf`. `ground(x, z)`
    gives the terrain's height (columns reach down into it). Returns
    {'parts', 'lamps', 'smoke', 'flag': the altar's top (x, y, z),
    'stations': [((x, y, z), heading)], 'spawns': [(x, z, heading)],
    'altar': (x, z, r) the area kept clear round the flag}."""
    cx, cz = center
    rng = np.random.default_rng(seed+71)
    parts, lamps, smoke = [], [], []

    def add(forms, lit='baked'):
        parts.extend((f, lit) for f in (forms if isinstance(forms, list) else [forms]))

    g0, g1, r0, r1 = GROTTO
    row = PITCH*math.sqrt(3)/2
    wobble = rng.uniform(0, math.tau, 3)
    for j in range(-9, 10):
        for i in range(-9, 10):
            x = cx+PITCH*(i+.5*(j % 2)); z = cz+row*j
            dx, dz = x-cx, z-cz
            r = math.hypot(dx, dz); b = bearing_of(dx, dz)
            edge = REACH*(1+.08*math.sin(math.radians(b)*3+wobble[0])+.05*math.sin(math.radians(b)*5+wobble[1]))
            if r > edge: continue
            stair = in_arc(b, *STAIR)
            if r < FLAT:
                h = TOP
            elif stair:
                # The giant's staircase: terraces down toward the temple.
                h = round((TOP-(r-FLAT)*1.2)/TERRACE)*TERRACE+rng.uniform(-.35, .35)
            elif r < edge-4.5:
                # Sheer faces, and a rank of organ pipes behind the top.
                east = max(0.0, math.cos(math.radians(b)))
                h = TOP+rng.uniform(-2.2, 1.4)+(east*rng.uniform(3.0, 10.0) if r < FLAT+3.4 else 0.0)
            else:
                # Broken stumps at the foot.
                h = rng.uniform(1.5, 7.5)
            h = max(h, 1.2)
            foot = min(ground(x+ox, z+oz) for ox, oz in ((R, 0), (-R, 0), (0, R), (0, -R)))-1.5
            grotto = in_arc(b, g0, g1) and r0 <= r <= r1+.8
            mouth = in_arc(b, *MOUTH) and r > r1-.5
            if grotto or mouth:
                if h < ROOF+1.8: continue
                bottom = shelf+ROOF+rng.uniform(0, .6)
                add(hexagon(x, z, bottom, shelf+h))
            else:
                tilt = (0.0, 0.0) if r < FLAT+R else (rng.uniform(-.03, .03), rng.uniform(-.03, .03))
                add(hexagon(x, z, foot, shelf+h, tilt=tilt))
            # The columns' tops are paler where they broke.
            add(hexagon(x, z, shelf+h-.1, shelf+h+.02, 'bark', False))

    # The altar: a low slab of dark stone with a bronze rim on the top.
    y = shelf+TOP
    add(TW.ycyl([(0, y), (2.6, y), (2.6, y+.28), (2.3, y+.35), (0, y+.35)], 6, 'concrete', True, (cx, 0, cz)))
    add(TW.ycyl([(2.3, y+.35), (2.45, y+.35), (2.45, y+.4), (2.3, y+.4)], 6, 'trim', False, (cx, 0, cz)))
    flag = (cx, round(y+.35+.05, 3), cz)
    for k in range(4):
        a = math.radians(45+90*k)
        add(TW.brazier((cx+7.0*math.cos(a), y, cz+7.0*math.sin(a)), smoke, lamps, 1.0))

    # The grotto's stations, against its inner wall facing out across it,
    # and lanterns along its roof.
    stations = []
    for b in (75.0, 118.0):
        a = math.radians(b)
        x, z = cx+(r0+1.9)*math.cos(a), cz+(r0+1.9)*math.sin(a)
        stations.append(((x, shelf, z), b))
    for b in (70.0, 100.0, 130.0):
        a = math.radians(b)
        add(TW.lantern((cx+(r0+r1)/2*math.cos(a), shelf+ROOF-.9, cz+(r0+r1)/2*math.sin(a)), lamps, 1.2), 'baked')
    spawns = []
    for b in (-40.0, -12.0, 16.0, 44.0):
        a = math.radians(b)
        spawns.append((cx+(REACH+7.0)*math.cos(a), cz+(REACH+7.0)*math.sin(a), b))
    return dict(parts=parts, lamps=lamps, smoke=smoke, flag=flag, stations=stations, spawns=spawns,
                altar=(cx, cz, 4.0))
