"""Original terrain for Reefbreak (docs/reefbreak.md).

Generated only from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and authored shapes; no heightfield from any other game is
read, resampled, traced or fitted. The layout: an atoll, the flags on
opposite sides of its reef ring and each base a ship beside its flag.

Flag axis is world Z (Ember at low Z) through the map centre.
- A central island with a lighthouse, a gentle dome rising ISLAND['h'] over
  the sea.
- The reef ring at RING['r'] from the centre: a low, broad crest a few
  metres over the sea, broken by four shallow channels, well off the flag
  axis. The flags stand on its crest.
- A shallow lagoon between the island and the ring (wading depth, so it can
  be skied through with drag) and sand spits joining the island to the ring.
- Outside the ring: deep sea, a sandbar under each base's stranded ship,
  and a ring of outer islands (one C-shaped pair on the flanks).
- Far out the seabed climbs to a ring of sea cliffs bounding the map.

The atoll's frame is point-symmetric through the centre ((x, z) -> (2048-x,
2048-z)): the island, the reef ring and its channels, the lagoon, the spits
and the sandbars under the ships. The rest is not, like real islands: each
outer island and blue hole has a twin of its own shape and place on the far
side (OUTER_TWIN, BLUE_TWIN), and the reef's lumps and the land's detail
come from each half's own noise. The halves measure alike (land area, depth,
slope: assets/fairness.py and the suite), so neither team has the better
ground.
"""
import math

import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
SEA = 60.0                                   # water surface
DEEP = SEA-9.0                               # open seabed
LAGOON = SEA-.3                              # lagoon floor: ankle deep, skiable with a little drag
SHELF = SEA-.4                               # the shallow shelf between the ring and the outer islands
SHELF_R = 640.0                              # the shelf drops to the deep sea beyond this radius
# Blue holes: deep pits in the shelf (angle degrees, radius, pit radius); each also mirrored.
BLUE_HOLES = ((40.0, 420.0, 34.0), (118.0, 410.0, 28.0), (-60.0, 430.0, 30.0))
RING = dict(r=300.0, half=50.0, h=8.0)       # reef crest radius, half-width, height over the sea
CHANNELS = (55.0, 125.0)                     # degrees from the Glacier (+Z) axis toward +X; mirrored
CHANNEL_HALF = 16.0                          # channel half-width (metres, along the ring)
ISLAND = dict(r=135.0, h=26.0)
SPITS = (90.0,)                              # sand spits island -> ring, degrees (and the mirror)
SPIT_HALF, SPIT_H = 26.0, 1.4
# Outer islands: (angle degrees, radius, rx, rz, height over sea); each also mirrored.
OUTER = ((28.0, 520.0, 96.0, 70.0, 26.0), (62.0, 545.0, 80.0, 108.0, 34.0),
         (150.0, 515.0, 100.0, 72.0, 22.0), (-34.0, 505.0, 82.0, 66.0, 20.0),
         (-68.0, 575.0, 70.0, 62.0, 16.0))
# Their twins across the centre: about the same size each, in their own place.
# The third is each team's outpost island (an outpost stands on each), so it
# keeps its twin's shape exactly.
OUTER_TWIN = ((22.0, 535.0, 110.0, 62.0, 24.0), (70.0, 530.0, 72.0, 96.0, 30.0),
              (150.0, 515.0, 100.0, 72.0, 22.0), (-40.0, 515.0, 90.0, 58.0, 18.0),
              (-62.0, 560.0, 78.0, 70.0, 20.0))
BLUE_TWIN = ((52.0, 440.0, 30.0), (104.0, 400.0, 32.0), (-48.0, 415.0, 28.0))
OTHER_HALF = 1009                            # seed offset for the far half's lumps and detail
C_ISLAND = dict(angle=90.0, r=505.0, outer=105.0, inner=52.0, h=30.0, open_deg=90.0)
SANDBAR = dict(r=360.0, rx=62.0, rz=40.0)   # under each ship (lying across the flag axis)
CLIFF = dict(r0=700.0, r1=800.0, h=46.0)
DETAIL, FINE = 2.4, .9


def polar(x, z):
    """Angle (degrees, 0 = +Z toward +X) and radius about the centre."""
    dx, dz = x-CENTRE, z-CENTRE
    return np.degrees(np.arctan2(dx, dz)), np.hypot(dx, dz)


def world(angle, r):
    a = math.radians(angle)
    return CENTRE+r*math.sin(a), CENTRE+r*math.cos(a)


def _ang_diff(a, b):
    return np.abs((a-b+180.0) % 360.0-180.0)


def _bump(d, half):
    """1 at d=0 smoothly to 0 at d=half."""
    return 1-_smooth(0, half, d)


def _half(x, z, seed, outer_islands=OUTER):
    """The large forms of one half; `natural` takes the max with the far
    half's (turned through the centre, with its own islands and noise)."""
    ang, r = polar(x, z)
    # Reef ring: crest profile across the ring with a lumpy height.
    across = np.abs(r-RING['r'])
    lump = .75+.5*_fbm(x+120, z-80, 90, 3, seed+3)
    ring = RING['h']*_bump(across, RING['half'])*lump
    for c in CHANNELS:
        for a in (c, c+180.0):
            along = _ang_diff(ang, a)*math.pi/180*RING['r']
            ring = ring*_smooth(CHANNEL_HALF*.6, CHANNEL_HALF*1.4, along)
    # Central island dome with a small flat crown.
    island = ISLAND['h']*np.minimum(_bump(r, ISLAND['r'])**.8*1.15, 1.0)
    # Sand spits from the island to the ring.
    spit = 0.0
    for s in SPITS:
        for a in (s, s+180.0):
            off = _ang_diff(ang, a)*math.pi/180*np.maximum(r, 1.0)
            inside = (r > ISLAND['r']*.6) & (r < RING['r'])
            spit = np.maximum(spit, (SPIT_H+2.6)*_bump(off, SPIT_HALF)*inside)
    # Outer islands.
    outer = 0.0
    for a, rr, rx, rz, h in outer_islands:
        cx, cz = world(a, rr)
        t = np.hypot((x-cx)/rx, (z-cz)/rz)
        outer = np.maximum(outer, h*_bump(t, 1.0)**.7)
    ca, cr = C_ISLAND['angle'], C_ISLAND['r']
    cx, cz = world(ca, cr)
    d = np.hypot(x-cx, z-cz)
    band = _bump(np.abs(d-(C_ISLAND['outer']+C_ISLAND['inner'])/2), (C_ISLAND['outer']-C_ISLAND['inner'])/2)
    # Open side faces the centre.
    face = np.degrees(np.arctan2(x-cx, z-cz))
    open_dir = (ca+180.0)
    mouth = _smooth(C_ISLAND['open_deg']*.35, C_ISLAND['open_deg']*.6, _ang_diff(face, open_dir))
    outer = np.maximum(outer, C_ISLAND['h']*band*mouth)
    # Sandbar under the Glacier ship (+Z side); the mirror gives Ember's.
    sx, sz = world(0.0, SANDBAR['r'])
    bar = np.hypot((x-sx)/SANDBAR['rx'], (z-sz)/SANDBAR['rz'])
    sandbar = 1.0*_bump(bar, 1.0)
    return ring, island, spit, outer, sandbar


def natural(x, z, seed):
    """Natural ground before any structure site."""
    x = np.asarray(x, dtype=np.float64); z = np.asarray(z, dtype=np.float64)
    a = _half(x, z, seed); b = _half(2*CENTRE-x, 2*CENTRE-z, seed+OTHER_HALF, OUTER_TWIN)
    ring, island, spit, outer, sandbar = (np.maximum(p, q) for p, q in zip(a, b))
    _, r = polar(x, z)
    # Seabed: lagoon inside the ring, deep outside, climbing to the cliffs.
    inside = 1-_smooth(RING['r']-RING['half']*.4, RING['r']+RING['half']*.8, r)
    shelf = 1-_smooth(SHELF_R-40.0, SHELF_R+40.0, r)
    bed = DEEP+(SHELF-DEEP)*shelf
    bed = bed+(LAGOON-bed)*inside
    for sign, holes in ((1, BLUE_HOLES), (-1, BLUE_TWIN)):
        for a, rr, pr in holes:
            cx, cz = world(a, rr)
            if sign < 0: cx, cz = 2*CENTRE-cx, 2*CENTRE-cz
            pit = _bump(np.hypot(x-cx, z-cz), pr)
            bed = bed+(DEEP-bed)*pit
    bed = np.maximum(bed, (SEA-.25)*sandbar+DEEP*(1-sandbar))
    cliff = CLIFF['h']*_smooth(CLIFF['r0'], CLIFF['r1'], r)
    raw = np.maximum.reduce([ring, island, spit, outer])
    # Land rises out of the seabed: blend from the bed to SEA-2+raw as the
    # raw height grows from 0 to 2 m, so shores shelve instead of stepping.
    rise = np.clip(raw/2.0, 0, 1)
    h = np.maximum(bed, bed+(SEA-2.0+raw-bed)*rise)
    h = np.maximum(h, bed+cliff)
    # Detail from each half's own noise (the far point's sampled with its own seed).
    both = lambda f: (f(x, z, seed)+f(2*CENTRE-x, 2*CENTRE-z, seed+OTHER_HALF))/2
    above = np.clip((h-SEA+1.0)/4.0, 0, 1)
    detail = both(lambda u, v, s: DETAIL*(_fbm(u, v, 60, 3, s+7)-.5)*2)*above
    fine = both(lambda u, v, s: FINE*(_fbm(u+400, v, 18, 2, s+11)-.5)*2)
    return h+detail+fine


def heights(seed, sites=()):
    """256x256 heights in metres (row = z cell, column = x cell).
    `sites` are (surface(x,z)->height, weight(x,z)->0..1) pairs over world grids."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    h = natural(x, z, seed)
    for surface, weight in sites:
        w = weight(x, z)
        h = h*(1-w)+surface(x, z)*w
    return np.clip(h, 2, 2040)


def weights(h):
    """Beach grass / coral rock / dry sand / wet sand splat: wet sand and
    seabed below and just above the waterline, dry sand on the low flats,
    grass on higher gentle ground, bare rock on steep faces."""
    s = slope_degrees(h)
    rock = np.clip((s-26)/10, 0, 1)*.94+.02
    wet = np.clip(1-rock, 0, 1)*np.clip((SEA+1.2-h)/1.6, 0, 1)
    grass = np.clip(1-rock-wet, 0, 1)*np.clip((h-SEA-4.0)/4.0, 0, .85)
    sand = np.clip(1-rock-wet-grass, 0, 1)
    out = np.stack([grass, rock, sand, wet], axis=-1)
    return np.round(out*255).astype(np.uint8)


def sea_volume():
    """The manifest water volume covering the whole map."""
    return {'surface': SEA, 'rect': [0.0, 0.0, 2048.0, 2048.0], 'depth': 40.0, 'color': [0.06, 0.34, 0.42], 'skim': True}
