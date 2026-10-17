"""Original terrain for Highgoal, the walled football arena.

Generated from PeakRunner's own hash noise (shared with Tower Complex's
terrain module); nothing from any other game is read, resampled or fitted.
A dead-flat sand floor fills the arena and runs out under its walls; low
dunes rise far beyond them for the skyline. Point-symmetric about the map
centre (1024, 1024).
"""
import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP  # noqa: F401

CENTRE = 1024.0
FLOOR_Y = 100.0
# Half-extents of the arena floor inside the walls (m).
HALF_W, HALF_L = 55.0, 110.0


def _outside(x, z, margin=40.0):
    dx = np.maximum(np.abs(x-CENTRE)-HALF_W-margin, 0)
    dz = np.maximum(np.abs(z-CENTRE)-HALF_L-margin, 0)
    return np.hypot(dx, dz)


def heights(seed):
    """256x256 heights in metres (row = z cell, column = x cell)."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    d = _outside(x, z)
    dunes = (_fbm(x, z, 180, 3, seed)+_fbm(2*CENTRE-x, 2*CENTRE-z, 180, 3, seed))/2
    h = FLOOR_Y+(45*dunes+10)*_smooth(20, 260, d)
    return np.clip(h, 2, 2040)


def weights(h):
    """Sand everywhere (the soil layer), with a little rock on dune faces."""
    s = slope_degrees(h)
    rock = np.clip((s-30)/20, 0, 1)*.6
    soil = 1-rock
    zero = np.zeros_like(h)
    out = np.stack([zero, rock, soil, zero], axis=-1)
    return np.round(out*255).astype(np.uint8)
