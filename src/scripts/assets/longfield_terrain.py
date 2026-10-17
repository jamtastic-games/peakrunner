"""Original stadium terrain for Longfield, the first football map.

Generated from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and an authored bowl; no heightfield from any other game is
read, resampled or fitted. Point-symmetric about the map centre (1024, 1024).

A flat turf field (140 x 340 m, the end zones 250 m apart along world Z) sits
in a bowl: steep, skiable banks rise 42 m on every side to a level rim for the
stands, and rolling hills continue beyond it to the map edge.
"""
import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
FIELD_Y = 100.0
# Half-extents of the flat field and the bowl around it (m).
HALF_W, HALF_L = 70.0, 170.0
BANK, RISE = 55.0, 42.0
RIM = 20.0
RIM_Y = FIELD_Y+RISE


def _outside(x, z):
    """Distance outside the field rectangle (0 on the field)."""
    dx = np.maximum(np.abs(x-CENTRE)-HALF_W, 0)
    dz = np.maximum(np.abs(z-CENTRE)-HALF_L, 0)
    return np.hypot(dx, dz)


def heights(seed):
    """256x256 heights in metres (row = z cell, column = x cell)."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    d = _outside(x, z)
    h = FIELD_Y+RISE*_smooth(0, BANK, d)
    # Rolling country beyond the rim, symmetrised so neither end is favoured.
    hills = (_fbm(x, z, 260, 4, seed)+_fbm(2*CENTRE-x, 2*CENTRE-z, 260, 4, seed))/2
    h = h+(70*hills+25)*_smooth(BANK+RIM, BANK+RIM+220, d)
    return np.clip(h, 2, 2040)


def weights(h):
    """Meadow/rock/soil/moss splat: mown stripes of meadow and moss on the
    field every 16 m, grass on the banks with rock on the steepest faces, and
    rougher soil and moss outside the stadium."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    d = _outside(x, z)
    s = slope_degrees(h)
    field = d <= 0
    stripe = ((np.floor((z-CENTRE)/16) % 2) == 0)
    meadow = np.where(field, np.where(stripe, .95, .35), .75)
    moss = np.where(field, np.where(stripe, .05, .65), .1)
    rock = np.where(field, 0, np.clip((s-38)/14, 0, 1)*.8)
    soil = np.where(field, 0, np.clip((d-BANK-RIM)/120, 0, .45))
    total = meadow+moss+rock+soil
    out = np.stack([meadow, rock, soil, moss], axis=-1)/total[..., None]
    return np.round(out*255).astype(np.uint8)

