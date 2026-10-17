"""Snow pines for Frostline: positions from PeakRunner's own hash noise,
point-symmetric about the map centre, only on slopes a tree could hold and
clear of every structure and the main lane. Trunks are solid cover; the
snow-laden boughs are render-only so nothing snags a skier.
"""
import math

import numpy as np

ATTEMPTS, MAX_SLOPE, LANE_HALF = 130, 34.0, 36.0


def positions(seed, height, slope, exclusions, noise):
    """[(x, y, z, h)] world positions. `height(x, z)` and `slope(x, z)` sample
    the finished terrain; `exclusions` are (x, z, r) discs kept clear."""
    out = []
    for i in range(ATTEMPTS):
        x = 180+noise(i, 1, seed)*1688
        z = 180+noise(i, 2, seed)*(1024-180)
        # One try in each half: the halves' ground differs, so each stand
        # grows where its own slopes hold it.
        for px, pz in ((x, z), (2048-x, 2048-z)):
            if abs(px-1024) < LANE_HALF: continue
            if any(math.hypot(px-ex, pz-ez) < r for ex, ez, r in exclusions): continue
            if slope(px, pz) > MAX_SLOPE: continue
            out.append((px, height(px, pz)-.6, pz, 9+noise(i, 3, seed)*8))
    return out


def build(mesh, trees):
    for x, y, z, h in trees:
        mesh.column((x, y, z), .42, h*.8, 'bark', 6, top=.14)
        for level, (r, y0, y1) in enumerate(((h*.28, .28, .62), (h*.21, .5, .8), (h*.13, .7, 1.0))):
            mesh.column((x, y+h*y0, z), r, h*(y1-y0), 'leaf', 6, top=.12, solid=False)
    return len(trees)


def slope_sampler(grid, step=8.0):
    gz, gx = np.gradient(grid, step)
    s = np.degrees(np.arctan(np.hypot(gx, gz)))
    def slope(x, z):
        return float(s[min(max(int(round(z/step)), 0), 255), min(max(int(round(x/step)), 0), 255)])
    return slope
