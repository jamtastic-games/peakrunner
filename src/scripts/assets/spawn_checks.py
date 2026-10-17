"""Whole-world spawn checks on a built pack (collision.bin + height.bin).

A spawn must face open floor: a player who spawns and walks forward should
stay on the spawn's own floor and not look straight into a wall. Checked on
the pack itself so neighbouring structures and terrain count too.
"""
import json
import math
from pathlib import Path

import numpy as np

EYE = 0.5          # spawn centres sit 1.2 m above the floor; eye ~1.7 m, waist 0.6 m
CLEAR = 6.0        # metres of open view straight ahead
WALK = 4.5         # half-second walk plus braking coast
FLOOR_TOL = 0.3
BODY = 0.52


class World:
    def __init__(self, pack):
        pack = Path(pack)
        self.manifest = json.loads((pack/'map.json').read_text())
        t = np.frombuffer((pack/'collision.bin').read_bytes(), '<f4').astype(np.float64).reshape(-1, 3, 3)
        self.a = t[:, 0]; self.e1 = t[:, 1]-t[:, 0]; self.e2 = t[:, 2]-t[:, 0]
        self.lo = t.min(1); self.hi = t.max(1)
        self.h = np.frombuffer((pack/'height.bin').read_bytes(), '<u2').reshape(256, 256)/32.0
        self.step = self.manifest.get('terrain_step', 8.0)

    def terrain(self, x, z):
        fx = min(max(x/self.step, 0), 255); fz = min(max(z/self.step, 0), 255)
        x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, 255), min(z0+1, 255)
        tx, tz = fx-x0, fz-z0
        a = self.h[z0, x0]+(self.h[z0, x1]-self.h[z0, x0])*tx
        b = self.h[z1, x0]+(self.h[z1, x1]-self.h[z1, x0])*tx
        return a+(b-a)*tz

    def ray(self, o, d, tmax):
        o = np.asarray(o, float); d = np.asarray(d, float); end = o+d*tmax
        m = np.all(self.hi >= np.minimum(o, end)-1e-6, 1) & np.all(self.lo <= np.maximum(o, end)+1e-6, 1)
        a, e1, e2 = self.a[m], self.e1[m], self.e2[m]
        if not len(a):
            return math.inf
        h = np.cross(d, e2); det = np.einsum('ij,ij->i', e1, h)
        ok = np.abs(det) > 1e-9; det = np.where(ok, det, 1); s = o-a
        u = np.einsum('ij,ij->i', s, h)/det; q = np.cross(s, e1)
        v = (q@d)/det; t = np.einsum('ij,ij->i', e2, q)/det
        k = ok & (u >= -1e-7) & (v >= -1e-7) & (u+v <= 1+1e-7) & (t >= 0) & (t <= tmax)
        return float(t[k].min()) if k.any() else math.inf

    def support(self, x, y, z):
        """Highest floor under (x, y, z): structure or terrain."""
        down = self.ray((x, y, z), (0, -1, 0), 50.0)
        struct = y-down if down < math.inf else -math.inf
        return max(struct, self.terrain(x, z))


def problems(pack):
    """Human-readable failures for every spawn point in the pack."""
    w = World(pack)
    out = []
    for team, points in enumerate(w.manifest.get('spawn_points', [])):
        for x, y, z, yaw in points:
            fwd = np.array([-math.sin(yaw), 0.0, -math.cos(yaw)])
            floor = w.support(x, y, z)
            side = np.array([fwd[2], 0.0, -fwd[0]])
            # Sweep the body's width (radius 0.52 m) at knee, waist and eye height.
            hit = min(w.ray(np.array([x, floor+h, z])+side*o, fwd, CLEAR)
                      for h in (0.2, 0.6, 1.2+EYE) for o in (-BODY, 0.0, BODY))
            if hit < CLEAR:
                out.append(f'team {team} spawn ({x:.1f},{y:.1f},{z:.1f}) yaw {yaw:.2f}: wall {hit:.1f} m ahead')
                continue
            for i in range(1, int(WALK/0.5)+1):
                p = np.array([x, y, z])+fwd*(0.5*i)
                below = w.support(p[0], y, p[2])
                if abs(below-floor) > FLOOR_TOL:
                    out.append(f'team {team} spawn ({x:.1f},{y:.1f},{z:.1f}) yaw {yaw:.2f}: '
                               f'floor changes {below-floor:+.1f} m after {0.5*i:.1f} m')
                    break
    return out


# Doorways and windows are open, so an indoor spawn must not sit in a
# straight line through an opening from far out in the field, where someone
# could camp it. Only spawns in rooms count: a roof within ROOF_ABOVE and
# walls within WALLED in at least 6 of 8 directions. Outdoor spawns and ones
# under open colonnades are visible by design.
FAR = (40.0, 70.0, 100.0)
AZIMUTHS = 48
ROOF_ABOVE = 12.0
OUTSIDE = 35.0     # viewers nearer than this to the team's own flag are inside its base
WALLED = 15.0


def _in_room(w, x, y, z):
    if w.ray((x, y, z), (0, 1, 0), ROOF_ABOVE) == math.inf:
        return False
    hits = sum(w.ray((x, y+.8, z), (math.cos(a), 0, math.sin(a)), WALLED) < WALLED
               for a in np.arange(8)*math.tau/8)
    return hits >= 6


def _terrain_clear(w, a, b, step=2.0):
    n = max(2, int(np.linalg.norm(b-a)/step))
    for t in np.linspace(0, 1, n)[1:-1]:
        p = a+(b-a)*t
        if p[1] < w.terrain(p[0], p[2]):
            return False
    return True


def exposed(pack):
    """Indoor spawns visible (spawn chest in clear line) from a standing or
    jetting viewer 40-100 m away in any direction, outside the team's own
    base (more than OUTSIDE from its flag)."""
    w = World(pack)
    out = []
    for team, points in enumerate(w.manifest.get('spawn_points', [])):
        fx, _, fz = w.manifest['flags'][team]
        for x, y, z, _ in points:
            if not _in_room(w, x, y, z):
                continue
            chest = np.array([x, y+0.8, z])
            for dist in FAR:
                for k in range(AZIMUTHS):
                    a = k*math.tau/AZIMUTHS
                    vx, vz = x+dist*math.cos(a), z+dist*math.sin(a)
                    if math.hypot(vx-fx, vz-fz) < OUTSIDE:
                        continue
                    for lift in (1.7, 10.0):
                        v = np.array([vx, w.terrain(vx, vz)+lift, vz])
                        d = chest-v; n = float(np.linalg.norm(d))
                        if w.ray(v, d/n, n-0.05) == math.inf and _terrain_clear(w, v, chest):
                            out.append(f'team {team} spawn ({x:.1f},{y:.1f},{z:.1f}) seen from '
                                       f'({vx:.0f},{v[1]:.0f},{vz:.0f}), {dist:.0f} m away')
                            break
                    else:
                        continue
                    break
                else:
                    continue
                break
    return out
