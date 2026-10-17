"""Original steep snow-mountain terrain for Frostline.

Generated only from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and an authored profile; no heightfield from any other game
is read, resampled or fitted. The layout is point-symmetric about the map
centre (1024, 1024): the profile along the flag axis and the side ridges.
Round the middle (the beacon, the ice cavern and the cols) the ground is
shared, the same both ways; out toward the stations it is not: each half has
hills and gullies of its own, like real land, and the two halves measure
alike (assets/fairness.py), so neither team has a better half.

Along the flag axis (world Z, red at low Z): each station sits on a shelf high
on its home massif, which climbs to peaks behind it. In front of the shelf the
ground drops steeply into an upper valley, crosses a spur, drops again into
the deep inner valley, then climbs a long snowfield to the central ridge and
its beacon. Ridged fractal detail keeps most of the ground steep.

v2 (2026-10-07) spreads the stations to about 1,330 m between flags and lays
the mountain out as lanes (each side the same, turned for the other team):

- the centre lane over the valleys and the spur, then over the beacon ridge
  or through the ice cavern under it (the profile inside the inner valleys
  is unchanged, so the cavern is too);
- each side, an arete: a high, continuous ridge line from the station's
  shoulder to the beacon ridge, notched where the valleys cross it and dipping
  to a col where it meets the ridge (the Cols' capture points stand there).
  The high road: little climbing once up, long runs off either side;
- beyond each arete, a glacier valley down the length of the field: the
  low, sheltered road, falling from the shelves' flanks to midfield.
"""
from functools import lru_cache

import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
# Authored profile along the axis (m from centre -> height). Inside VALLEY_D
# it is v1's (the cavern depends on it).
RIDGE, VALLEY, SHELF, PEAKS = 262.0, 168.0, 230.0, 330.0
VALLEY_D, SHELF_D0, SHELF_D1, PEAK_D = 215.0, 590.0, 710.0, 840.0
SPUR, SPUR_D, UPPER, UPPER_D = 214.0, 335.0, 186.0, 455.0
# The aretes: crest |X| about ARETE_X, at ARETE_TOP, notched to NOTCH at the
# valleys' |Z| and dipping to COL where they meet the beacon ridge.
ARETE_X, ARETE_TOP, ARETE_W = 250.0, 292.0, 52.0
NOTCH, NOTCHES, COL = 232.0, (VALLEY_D, UPPER_D), 266.0
# The glacier valleys: floor |X| about GLACIER_X, from GLACIER_END by the
# shelves down to GLACIER_MID at midfield.
GLACIER_X, GLACIER_W, GLACIER_END, GLACIER_MID = 420.0, 64.0, 205.0, 176.0
# Detail amplitudes (m) tuned for a ~36 degree median slope.
RIDGED, ROLL, FINE = 110, 40, 13
AXIS_DAMP, AXIS_WIDTH = .72, 60.0
# The far half's detail comes from its own noise (seed offset), not a mirror;
# chosen for the fairest pair of halves. The middle stays shared out to the
# first radius (m from the centre), each half's own beyond the second.
OTHER_HALF = 1009
SHARED = (200.0, 320.0)


def _profile(d):
    ridge_to_valley = RIDGE+(VALLEY-RIDGE)*_smooth(0, VALLEY_D, d)
    valley_to_spur = VALLEY+(SPUR-VALLEY)*_smooth(VALLEY_D, SPUR_D, d)
    spur_to_upper = SPUR+(UPPER-SPUR)*_smooth(SPUR_D, UPPER_D, d)
    upper_to_shelf = UPPER+(SHELF-UPPER)*_smooth(UPPER_D, SHELF_D0, d)
    shelf_to_peaks = SHELF+(PEAKS-SHELF)*_smooth(SHELF_D1, PEAK_D, d)
    return np.where(d < VALLEY_D, ridge_to_valley, np.where(d < SPUR_D, valley_to_spur, np.where(
        d < UPPER_D, spur_to_upper, np.where(d < SHELF_D1, upper_to_shelf, shelf_to_peaks))))


def _meander(Z, side, x, amp, wave, phase):
    """X of a meandering line |X| about x (side -1 west, +1 east).
    Point-symmetric: the result at (-Z, -side) is minus that at (Z, side)."""
    return side*(x+amp*np.sin(side*Z/wave+phase))


def arete_centre(Z, side):
    return _meander(Z, side, ARETE_X, 18, 140, 1.3)


def glacier_centre(Z, side):
    return _meander(Z, side, GLACIER_X, 30, 200, .4)


def arete_crest(Z):
    """The aretes' crest height along Z: the col at the centre, notches at
    the valleys, easing into the shoulders above the shelves."""
    d = np.abs(Z)
    top = COL+(ARETE_TOP-COL)*_smooth(30, 150, d)
    for n in NOTCHES:
        top = top-(ARETE_TOP-NOTCH)*np.exp(-((d-n)/38)**2)
    return top


def glacier_floor(Z):
    return GLACIER_MID+(GLACIER_END-GLACIER_MID)*_smooth(80, 600, np.abs(Z))


def _lanes(x, z, h, seed):
    """Blend the aretes and glacier valleys into ground `h`. The glacier
    floors keep gentle moguls."""
    X, Z = x-CENTRE, z-CENTRE
    d = np.abs(Z)
    reach = 1-_smooth(600, 700, d)
    for side in (-1, 1):
        a = np.exp(-((X-arete_centre(Z, side))/ARETE_W)**2)*reach
        h = h*(1-a)+np.maximum(h, arete_crest(Z)-((X-arete_centre(Z, side))/ARETE_W)**2*14)*a
        g = np.exp(-((X-glacier_centre(Z, side))/GLACIER_W)**4)*reach
        floor = glacier_floor(Z)+9*(_fbm(x+300, z+1100, 70, 3, seed+41)-.5)*2
        h = h*(1-g)+np.minimum(h, floor)*g
    return h


def _half(x, z, seed):
    """One unsymmetrised copy of the mountain detail."""
    h = RIDGED*_fbm(x+200, z+700, 170, 5, seed+7, ridged=True)
    h = h+ROLL*(_fbm(x, z, 120, 4, seed+2)-.5)*2
    h = h+FINE*(_fbm(x+500, z+300, 36, 2, seed+9)-.5)*2
    return h


def _detail(x, z, seed):
    # The far point's share turns from its mirror to its own noise; cos/sin
    # weights keep the detail's strength the same through the turn.
    own = _smooth(*SHARED, np.hypot(x-CENTRE, z-CENTRE))*np.pi/2
    far = _half(2*CENTRE-x, 2*CENTRE-z, seed)*np.cos(own)+_half(2*CENTRE-x, 2*CENTRE-z, seed+OTHER_HALF)*np.sin(own)
    d = (_half(x, z, seed)+far)/2
    return d*(1-AXIS_DAMP*np.exp(-((x-CENTRE)/AXIS_WIDTH)**2))


@lru_cache(maxsize=8)
def _offset(seed):
    """Mean detail over the flag-axis corridor, so the authored profile
    heights hold there on average."""
    z, x = np.mgrid[304:1745:8, 960:1089:8].astype(np.float64)
    return float(_detail(x, z, seed).mean())


def natural(x, z, seed):
    """Natural ground before any structure site."""
    h = _profile(np.abs(z-CENTRE))+_detail(x, z, seed)-_offset(seed)
    h = _lanes(x, z, h, seed)
    edge = np.maximum(np.abs(x-CENTRE), np.abs(z-CENTRE))
    return h+90*_smooth(880, 1010, edge)


def heights(seed, sites=()):
    """256x256 heights in metres (row = z cell, column = x cell). `sites`
    are (surface(x,z)->height, weight(x,z)->0..1) pairs over world grids."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    h = natural(x, z, seed)
    for surface, weight in sites:
        w = weight(x, z)
        h = h*(1-w)+surface(x, z)*w
    return np.clip(h, 2, 2040)


def weights(h):
    """Four splat channels painted over the kit's terrain slots:
    fresh snow, bare granite, wind-scoured ice crust, snowy scree."""
    s = slope_degrees(h)
    rock = np.clip((s-44)/16, 0, 1)*.85+.02
    scree = np.clip(1-rock, 0, 1)*np.clip((s-30)/20, 0, .55)
    ice = np.clip(1-rock-scree, 0, 1)*np.clip((190-h)/30, 0, .8)
    snow = np.clip(1-rock-scree-ice, 0, 1)
    out = np.stack([snow, rock, ice, scree], axis=-1)
    return np.round(out*255).astype(np.uint8)


__all__ = ['heights', 'natural', 'weights', 'slope_degrees', 'N', 'STEP', 'CENTRE']
