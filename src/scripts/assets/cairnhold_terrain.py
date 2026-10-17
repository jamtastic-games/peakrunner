"""Original rugged highland terrain for Cairnhold.

Generated only from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and an authored layout; no heightfield from any other game
is read, resampled or fitted. The layout is point-symmetric about the map
centre (1024, 1024). Round the middle (the Ring and its mesa) the ground is
shared, the same both ways; out toward the bunkers it is not: each half has
hills and gullies of its own, like real land, and the two halves measure
alike (assets/fairness.py), so neither team has a better half.

v2 (2026-10-07) spreads the bunkers to about 1,060 m between flags and lays
outdoor lanes between them (world Z is the flag axis, red at low Z):

- The centre lane down the flag axis: the home knoll, a dip in front of the
  bunker, a forward rise, the burn valley, then the central mesa carrying
  the Ring, passed over or round either side. The shortest way, but into
  the Ring's turret.
- Each side, a ridge line beside the centre lane (the high road: long
  sightlines and downhill runs), broken by a pass in front of each bunker
  and one by each burn valley, so players switch lanes on the way.
- Beyond it, a winding glen down the length of the field (the low road:
  sheltered, longer), deepest through the middle, where the flank cairn
  stands, and climbing at each end to the bunkers' flanks: the plasma
  battery on one side, the landing ledge on the other.

Ridged fractal detail keeps almost all of the ground steep; it is damped
along each lane so the lanes read.

Structures sit in "sites": the ground is blended toward an authored surface
around each one, and cells a structure is dug into become terrain holes whose
boundary vertices are pinned to heights the structure's walls cover.
"""
from functools import lru_cache

import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
# Authored profile along the flag axis (|Z|, metres from the centre -> m).
# The knoll carries the flag stand (|Z| 530); the bunker (|Z| 444) is dug
# into its forward slope, the dip lies in front of it.
AXIS = ((0.0, 246.0), (50.0, 246.0), (205.0, 184.0), (290.0, 212.0), (365.0, 186.0),
        (530.0, 226.0))
KNOLL_Z = AXIS[-1][0]
# Each side, outward from the centre lane: a ridge line (crest |X| about
# RIDGE_X, RIDGE metres over the flank level, broken by passes) and a glen
# (floor |X| about GLEN_X, GLEN metres under it), both meandering.
RIDGE_X, RIDGE, RIDGE_W = 178.0, 44.0, 46.0
GLEN_X, GLEN, GLEN_W = 330.0, 42.0, 60.0
FLANK = 206.0            # the flank level at midfield
# Passes through each ridge line (|Z|): in front of each bunker and by the
# burn valleys.
PASSES = (410.0, 170.0)
# Detail amplitudes (m) tuned for a 26-30 degree median slope.
RIDGED, ROLL, FINE = 58.0, 27.0, 8.0


def _profile(Z):
    """Smooth piecewise profile through the AXIS points, level at each."""
    d = np.abs(Z)
    h = np.full_like(d, AXIS[0][1])
    for (d0, h0), (d1, h1) in zip(AXIS, AXIS[1:]):
        h = np.where(d >= d0, h0+(h1-h0)*_smooth(d0, d1, d), h)
    return h


def _meander(Z, side, x, amp, wave, phase):
    """X of a meandering line |X| about x (side -1 west, +1 east).
    Point-symmetric: the result at (-Z, -side) is minus that at (Z, side)."""
    return side*(x+amp*np.sin(side*Z/wave+phase))


def glen_centre(Z, side):
    return _meander(Z, side, GLEN_X, 36, 175, .6)


def ridge_centre(Z, side):
    return _meander(Z, side, RIDGE_X, 16, 120, 2.1)


def _lanes(X, Z):
    """The ridge lines and glens, relative to the flank level."""
    h = np.zeros_like(X)
    for side in (-1, 1):
        # The glen runs the length of the field; it shallows toward each
        # end, where it climbs to the bunkers' flanks.
        d = np.abs(Z)
        glen = GLEN*(.35+.65*(1-_smooth(380, 540, d)))*(1-_smooth(560, 700, d))
        h = h-glen*np.exp(-((X-glen_centre(Z, side))/GLEN_W)**2)
        cut = 1.0
        for p in PASSES:
            cut = cut-.75*np.exp(-((d-p)/36)**2)
        ridge = RIDGE*cut*(1-_smooth(470, 600, d))
        h = h+ridge*np.exp(-((X-ridge_centre(Z, side))/RIDGE_W)**2)
        # Beyond the glen the ground climbs to the outer hills.
        h = h+52*_smooth(GLEN_X+60, GLEN_X+260, X*side)
    return h


def macro(x, z):
    """The authored layout: the axis profile in the middle, the flank level
    with its ridge lines and glens outward. Exactly point-symmetric about
    the centre."""
    X, Z = x-CENTRE, z-CENTRE
    axis = _profile(np.clip(Z, -KNOLL_Z, KNOLL_Z))
    # Beyond each knoll the ground climbs into back-country hills.
    back = 30*_smooth(KNOLL_Z, KNOLL_Z+150, np.abs(Z))
    # The flank level follows the axis near the bunkers (the knolls run out
    # across the flanks) and settles to FLANK through the middle.
    flank = FLANK+(axis-FLANK)*_smooth(300, 470, np.abs(Z))*.7
    w = np.exp(-(X/130)**2)
    return axis*w+flank*(1-w)+back+_lanes(X, Z)*(1-.9*w)


def _half(x, z, seed):
    """One unsymmetrised copy of the rugged detail."""
    h = RIDGED*_fbm(x+300, z+500, 150, 4, seed+3, ridged=True)
    h = h+ROLL*(_fbm(x, z, 110, 4, seed)-.5)*2
    h = h+FINE*(_fbm(x+900, z+100, 40, 2, seed+5)-.5)*2
    return h


# The rugged detail is damped inside each lane so the lanes read: the flag
# axis by AXIS_DAMP over AXIS_WIDTH, the glens by LANE_DAMP, the ridge
# crests a little.
AXIS_DAMP, AXIS_WIDTH = .8, 70.0
LANE_DAMP = .6
# The far half's detail comes from its own noise (seed offset), not a mirror;
# chosen for the fairest pair of halves. The middle stays shared out to the
# first radius (m from the centre), each half's own beyond the second.
OTHER_HALF = 2018
SHARED = (160.0, 260.0)


def _damp(x, z):
    X, Z = x-CENTRE, z-CENTRE
    keep = 1-AXIS_DAMP*np.exp(-(X/AXIS_WIDTH)**2)
    for side in (-1, 1):
        keep = keep*(1-LANE_DAMP*np.exp(-((X-glen_centre(Z, side))/(GLEN_W*.8))**2))
        keep = keep*(1-.4*np.exp(-((X-ridge_centre(Z, side))/(RIDGE_W*.6))**2))
    return keep


def _detail(x, z, seed):
    # The far point's share turns from its mirror to its own noise; cos/sin
    # weights keep the detail's strength the same through the turn.
    own = _smooth(*SHARED, np.hypot(x-CENTRE, z-CENTRE))*np.pi/2
    far = _half(2*CENTRE-x, 2*CENTRE-z, seed)*np.cos(own)+_half(2*CENTRE-x, 2*CENTRE-z, seed+OTHER_HALF)*np.sin(own)
    return (_half(x, z, seed)+far)/2*_damp(x, z)


@lru_cache(maxsize=8)
def _offset(seed):
    """Mean detail over the flag-axis corridor, from a fixed sample grid, so
    the authored profile heights hold there on average."""
    z, x = np.mgrid[494:1555:8, 944:1105:8].astype(np.float64)
    return float(_detail(x, z, seed).mean())


def natural(x, z, seed):
    """Natural ground before any structure site."""
    h = macro(x, z)+_detail(x, z, seed)-_offset(seed)
    edge = np.maximum(np.abs(x-CENTRE), np.abs(z-CENTRE))
    return h+70*_smooth(800, 1010, edge)


def heights(seed, sites=(), rings=None):
    """256x256 heights in metres (row = z cell, column = x cell).

    `sites` are (surface(x,z)->height or None, weight(x,z)->0..1) pairs over
    world grids; `rings` maps (ix, iz) grid vertices to pinned heights."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    h = natural(x, z, seed)
    for surface, weight in sites:
        w = weight(x, z)
        h = h*(1-w)+surface(x, z)*w
    for (ix, iz), y in (rings or {}).items():
        h[iz, ix] = y
    return np.clip(h, 2, 2040)


def weights(h):
    """Meadow/rock/soil/moss splat: bare rock on the steep faces, soil on the
    middle slopes, meadow in sheltered low ground, moss on high benches."""
    s = slope_degrees(h)
    rock = np.clip((s-28)/16, 0, 1)*.88+.08
    soil = np.clip(1-rock, 0, 1)*np.clip(.2+(s-10)/34, 0, .8)
    moss = np.clip(1-rock-soil, 0, 1)*np.clip((h-238)/20, 0, 1)
    meadow = np.clip(1-rock-soil-moss, 0, 1)
    out = np.stack([meadow, rock, soil, moss], axis=-1)
    return np.round(out*255).astype(np.uint8)
