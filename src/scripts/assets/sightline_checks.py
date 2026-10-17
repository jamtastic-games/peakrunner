"""Shared turret-sightline rule for the map suites (mirrors the server).

A turret engages a target when it is inside its field of fire (`facing` and
`arc` from turret_arcs.py), within range, and its chest is visible from the
barrel start. Doorways and windows are open, so the rule for rooms is: no
turret may see a standable point more than `DOOR_DEPTH` metres inside a room.
Points within that depth of an opening's inner face are the doorway itself
and are allowed.
"""
import math

import numpy as np

DOOR_DEPTH = 3.0
RANGE = 150.0          # sensor-boosted turret range, as in equipment::profile
MUZZLE_GAP = 0.6
ALL_ROUND_RANGE = 15.0 # equipment::ALL_ROUND_RANGE: covered in every direction


def in_arc(e, targets):
    """Boolean mask: which `targets` (N x 3, chest points) the turret may
    engage: within ALL_ROUND_RANGE horizontally, or inside its field of fire."""
    if e.get('facing') is None or e.get('arc', 360) >= 360:
        return np.ones(len(targets), bool)
    f = np.array(e['facing'], float); f /= np.linalg.norm(f)
    d = targets[:, [0, 2]]-np.array(e['position'])[[0, 2]]
    n = np.linalg.norm(d, axis=1)
    cos = (d@f)/np.maximum(n, 1e-6)
    return (n <= ALL_ROUND_RANGE) | (cos >= math.cos(math.radians(e['arc'])/2))


def near_openings(points, openings, depth=DOOR_DEPTH):
    """Mask of `points` (N x 3) whose horizontal distance to any opening
    segment is below `depth`. `openings` are ((x0, z0), (x1, z1)) world
    segments along each opening's inner face."""
    pts = np.asarray(points)[:, [0, 2]]
    near = np.zeros(len(pts), bool)
    for a, b in openings:
        a = np.array(a, float); b = np.array(b, float); ab = b-a
        t = np.clip(((pts-a)@ab)/max(ab@ab, 1e-9), 0, 1)
        near |= np.linalg.norm(pts-(a+t[:, None]*ab), axis=1) < depth
    return near


def visible(world, e, targets):
    """Mask of chest `targets` this turret would engage (arc, range, LOS)."""
    targets = np.asarray(targets, float)
    p = np.array(e['position'], float)
    d = targets-p; dist = np.linalg.norm(d, axis=1)
    mask = (dist < RANGE) & in_arc(e, targets)
    out = np.zeros(len(targets), bool)
    if mask.any():
        starts = p+d[mask]/dist[mask, None]*(e['radius']+MUZZLE_GAP)
        out[mask] = ~world.blocked_many(starts, targets[mask])
    return out
