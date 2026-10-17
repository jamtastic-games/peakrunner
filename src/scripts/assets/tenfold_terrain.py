"""Original terrain round Tenfold Tower.

Generated from PeakRunner's own hash noise (shared with Tower Complex's
terrain module); nothing from any other game is read, resampled or fitted.
A level plaza holds the tower in the middle of the map; past it the ground
rolls away into broad hills and valleys for skiing while you wait.
"""
import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP  # noqa: F401

CENTRE = 1024.0
PLAZA_Y = 99.6       # a little under the plaza slab and the ground floor
PLAZA_R = 48.0


def heights(seed):
    """256x256 heights in metres (row = z cell, column = x cell)."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    d = np.hypot(x-CENTRE, z-CENTRE)
    hills = _fbm(x, z, 420, 4, seed)
    ridges = _fbm(x+311, z-173, 210, 3, seed+9, ridged=True)
    land = PLAZA_Y-6+70*hills+26*ridges+18*_smooth(300, 900, d)
    h = PLAZA_Y+(land-PLAZA_Y)*_smooth(PLAZA_R, PLAZA_R+120, d)
    return np.clip(h, 4, 2040)


def weights(h):
    """Meadow on the gentle ground, rock on steep faces, soil in the hollows."""
    s = slope_degrees(h)
    rock = np.clip((s-24)/18, 0, 1)
    soil = np.clip((PLAZA_Y+4-h)/30, 0, .5)*(1-rock)
    moss = np.clip((h-PLAZA_Y-50)/60, 0, .4)*(1-rock-soil)
    grass = np.clip(1-rock-soil-moss, 0, 1)
    out = np.stack([grass, rock, soil, moss], axis=-1)
    out /= np.maximum(out.sum(-1, keepdims=True), 1e-6)
    return np.round(out*255).astype(np.uint8)
