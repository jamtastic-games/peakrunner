"""Original rolling dune terrain for Dustreach.

Generated only from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and an authored profile; no heightfield from any other game is
read, resampled or fitted. The layout is point-symmetric about the map centre
(1024, 1024): the profile along the flag axis and the dune banks. Round the
middle (the Sun Gate and the wadi points) and along the sewer (one culvert,
built for one half and turned for the other) the ground is shared, the same
both ways; elsewhere it is not: each half's dunes and hollows are its own,
like real land, and the two halves measure alike (assets/fairness.py), so
neither team has a better half.

Along the flag axis (world Z, red at low Z): a dune ridge behind each citadel
for a downhill start, the citadel's plateau, a basin in front of it, two
transverse dune crests with a basin between, a dip, and the raised central
saddle carrying the Sun Gate. Long crescent dunes run diagonally across the
whole field: a gentle windward face and a steeper lee face, warped so no two
crests line up.

v2 (2026-10-07) spreads the citadels to about 1,300 m between flags and lays
the field out as lanes (each side the same, turned for the other team):

- the centre lane through the Sun Gate, over the crests;
- each side of it, the dune sea: crescent dunes for rolling jumps;
- beyond that, a wadi: a dry riverbed sunk into the dunes, winding down the
  length of the field. Its smooth floor runs gently downhill from each
  citadel's flank to a dry pan at midfield (the wadi capture points), so it
  is the sheltered fast road, and climbs again to the far citadel;
- outside the wadis, high dune banks;
- and under it all the sewer (dustreach_sewer).

Structures sit in "sites": the ground is blended toward an authored surface
around each one. The only cuts are the whole-cell holes under each citadel's
underground level (dustreach_citadel.HOLES); every one lies under the terrace
or the tower, so the heights here are never pinned.
"""
from functools import lru_cache

import numpy as np

from assets import dustreach_sewer
from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
BASE_D = 633.0             # citadel origin distance from the centre along Z
BASE_Y = 130.0             # citadel plateau height
# Authored profile along the axis: (distance from centre, height).
PROFILE = ((0, 152), (85, 131), (170, 143), (275, 116), (385, 139), (505, 114), (BASE_D, BASE_Y),
           (705, 142), (830, 166))
DUNE, DUNE_WAVE, DUNE_ANGLE = 11.5, 124.0, .62
ROLL, FINE = 10.0, 2.4
# Off the axis the ground follows a gentler flank level: TREND (low through
# the middle, rising to the citadels) plus FLANK_SHARE of the profile's
# crests and basins.
TREND, TREND_RISE, FLANK_SHARE, AXIS_HALF_WIDTH = 112.0, 18.0, .35, 190.0
# The wadis: centreline |X| about WADI_X, meandering; a flat floor
# WADI_FLOOR wide, banks easing out to WADI_BANK; the floor WADI_DEPTH under
# the trend, so it runs gently downhill from each citadel's flank to a dry
# pan at midfield.
WADI_X, WADI_FLOOR, WADI_BANK, WADI_DEPTH = 285.0, 14.0, 38.0, 14.0
BANK_X = 440.0             # the outer dune banks


def _profile(d):
    """Smoothstep interpolation through PROFILE; level at each control point."""
    h = np.full_like(d, float(PROFILE[0][1]))
    for (d0, h0), (d1, h1) in zip(PROFILE, PROFILE[1:]):
        h = np.where(d >= d0, h0+(h1-h0)*_smooth(d0, d1, d), h)
    return h


def _dunes(x, z, seed):
    """Crescent dunes: an asymmetric saw profile along a warped diagonal."""
    X, Z = x-CENTRE, z-CENTRE
    c, s = np.cos(DUNE_ANGLE), np.sin(DUNE_ANGLE)
    warp = (_fbm(x+700, z+200, 320, 3, seed+31)-.5)*1.4
    phase = (X*c+Z*s)/DUNE_WAVE+warp
    f = phase-np.floor(phase)
    # Windward face rises over 70 % of the wavelength, the lee drops over 30 %.
    rise = _smooth(0, .7, f)
    fall = 1-_smooth(.7, 1.0, f)
    shape = np.where(f < .7, rise, fall)
    strength = .45+.55*_fbm(x+100, z+900, 260, 2, seed+33)
    return DUNE*strength*(shape-.5)*2


def wadi_centre(Z, side):
    """X of a wadi's centreline (side -1 west, +1 east), meandering.
    Point-symmetric: the result at (-Z, -side) is minus that at (Z, side)."""
    return side*(WADI_X+38*np.sin(side*Z/190+.9)+14*np.sin(side*Z/71+2.0))


def trend(Z):
    return TREND+TREND_RISE*_smooth(150, 640, np.abs(Z))


def wadi_floor(Z):
    return trend(Z)-WADI_DEPTH


def wadi_weight(x, z):
    """0..1: how much of the ground at (x, z) is wadi floor (1 on the floor,
    easing to 0 at the top of the banks); 0 beyond the wadi's ends."""
    X, Z = x-CENTRE, z-CENTRE
    w = np.zeros_like(X)
    for side in (-1, 1):
        d = np.abs(X-wadi_centre(Z, side))
        w = np.maximum(w, 1-_smooth(WADI_FLOOR, WADI_BANK, d))
    return w*(1-_smooth(640, 720, np.abs(Z)))


def _half(x, z, seed):
    X = x-CENTRE
    h = _dunes(x, z, seed)
    # High dune banks along both sides of the field, outside the wadis.
    for side in (-1, 1):
        bank = 34*np.exp(-((X-side*BANK_X)/110)**2)
        h = h+bank*(.6+.4*_fbm(x, z+side*600, 240, 3, seed+23))
    h = h+ROLL*(_fbm(x, z, 150, 4, seed)-.5)*2
    h = h+FINE*(_fbm(x+900, z+100, 36, 2, seed+5)-.5)*2
    return h


# Dune detail is damped inside the flag-axis corridor so the authored basin ->
# crest -> saddle sequence still reads along the main lane.
AXIS_DAMP, AXIS_WIDTH = .45, 60.0
# The far half's detail comes from its own noise (seed offset), not a mirror;
# chosen for the fairest pair of halves. The middle stays shared out to the
# first radius (m from the centre), each half's own beyond the second.
OTHER_HALF = 5045
SHARED = (180.0, 290.0)
CORRIDOR = (16.0, 40.0)        # shared within the first distance (m) of a sewer cell, own beyond the second


def _sewer_distance(x, z):
    """Metres to the nearest sewer cell of either half."""
    d = np.full(np.shape(x), np.inf)
    for ix, iz in dustreach_sewer.all_cells():
        x0, z0 = ix*STEP, iz*STEP
        dx = np.maximum(np.maximum(x0-x, x-(x0+STEP)), 0); dz = np.maximum(np.maximum(z0-z, z-(z0+STEP)), 0)
        d = np.minimum(d, np.hypot(dx, dz))
    return d


def _detail(x, z, seed):
    # The far point's share turns from its mirror to its own noise; cos/sin
    # weights keep the detail's strength the same through the turn.
    own = _smooth(*SHARED, np.hypot(x-CENTRE, z-CENTRE))*_smooth(*CORRIDOR, _sewer_distance(x, z))*np.pi/2
    far = _half(2*CENTRE-x, 2*CENTRE-z, seed)*np.cos(own)+_half(2*CENTRE-x, 2*CENTRE-z, seed+OTHER_HALF)*np.sin(own)
    d = (_half(x, z, seed)+far)/2
    return d*(1-AXIS_DAMP*np.exp(-((x-CENTRE)/AXIS_WIDTH)**2))


@lru_cache(maxsize=8)
def _offset(seed):
    """Mean detail over the flag-axis corridor, from a fixed sample grid."""
    z, x = np.mgrid[392:1657:8, 960:1089:8].astype(np.float64)
    return float(_detail(x, z, seed).mean())


def natural(x, z, seed):
    """Natural ground before any structure site: the profile along the axis
    and the flank level outward, the dunes, and each wadi cut to its floor."""
    Z = z-CENTRE
    prof = _profile(np.abs(Z))
    flank = trend(Z)+FLANK_SHARE*(prof-trend(Z))
    w = np.exp(-((x-CENTRE)/AXIS_HALF_WIDTH)**2)
    h = prof*w+flank*(1-w)
    h = h+_detail(x, z, seed)-_offset(seed)
    w = wadi_weight(x, z)
    h = h*(1-w)+np.minimum(h, wadi_floor(Z))*w
    edge = np.maximum(np.abs(x-CENTRE), np.abs(z-CENTRE))
    return h+60*_smooth(840, 1010, edge)


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
    """Sand/sandstone/ochre/hardpan splat: bare sandstone on steep faces,
    darker ochre sand on the middle slopes, gravel hardpan on the flats and
    pale wind-rippled sand everywhere else."""
    s = slope_degrees(h)
    rock = np.clip((s-27)/12, 0, 1)*.9+.04
    ochre = np.clip(1-rock, 0, 1)*np.clip((s-11)/22, 0, .7)
    hardpan = np.clip(1-rock-ochre, 0, 1)*np.clip((7-s)/6, 0, .8)
    sand = np.clip(1-rock-ochre-hardpan, 0, 1)
    out = np.stack([sand, rock, ochre, hardpan], axis=-1)
    return np.round(out*255).astype(np.uint8)
