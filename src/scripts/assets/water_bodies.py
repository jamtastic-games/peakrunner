"""Original ponds, tarns and oases: carve a basin into a map's heightfield and
describe it as a manifest water volume (see crates/core/src/water.rs).

A body is an ellipse in world space: {"name", "x", "z", "rx", "rz", "yaw"
(degrees, optional), "depth" (metres at the centre), "flow" [x, z] (optional),
"color" [r, g, b] (optional)}. The surface is set from the ground round the
ellipse: the median of the terrain on a ring just outside it, so a pond on a
slope is dug into the uphill side and dammed by a raised lip on the downhill
side. Inside the shore the bed shelves from ankle depth at the edge through
waist depth to the full depth at the centre, so wading reads. Outside the
shore a bank blends from just above the surface back to the natural ground.

Everything is deterministic: the same grid and bodies give the same bytes.
"""
import math

import numpy as np

STEP = 8.0
N = 256
FREEBOARD = 0.35     # bank crest above the surface at the shoreline (m)
BANK = 0.7           # bank blend width beyond the shore, as a fraction of radius
SHELF = 0.3          # outer fraction of the radius that shelves ankle -> waist
ANKLE, WAIST = 0.15, 1.2
OUTLINE = 48         # polygon points (the engine allows up to 64)
SHORE_PAD = 1.08     # outline radius: a little past the shore so no sliver shows


def _frame(body):
    yaw = math.radians(body.get('yaw', 0.0))
    return math.cos(yaw), math.sin(yaw)


def radius_t(body, x, z):
    """Normalized elliptical radius: 0 at the centre, 1 on the shore."""
    c, s = _frame(body)
    dx, dz = np.asarray(x)-body['x'], np.asarray(z)-body['z']
    u = dx*c+dz*s; v = -dx*s+dz*c
    return np.sqrt((u/body['rx'])**2+(v/body['rz'])**2)


def _sample(grid, x, z):
    fx = min(max(x/STEP, 0), N-1); fz = min(max(z/STEP, 0), N-1)
    x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, N-1), min(z0+1, N-1); tx, tz = fx-x0, fz-z0
    a = grid[z0, x0]+(grid[z0, x1]-grid[z0, x0])*tx
    b = grid[z1, x0]+(grid[z1, x1]-grid[z1, x0])*tx
    return float(a+(b-a)*tz)


def _point(body, t, a):
    c, s = _frame(body)
    u, v = t*body['rx']*math.cos(a), t*body['rz']*math.sin(a)
    return body['x']+u*c-v*s, body['z']+u*s+v*c


def _smooth(v):
    v = np.clip(v, 0.0, 1.0)
    return v*v*(3-2*v)


def depth_at(body, t):
    """Water depth for a normalized radius inside the shore (t < 1)."""
    t = np.asarray(t, dtype=np.float64)
    shelf = ANKLE+(WAIST-ANKLE)*np.clip((1-t)/SHELF, 0, 1)
    inner = 1-SHELF
    deep = WAIST+(body['depth']-WAIST)*_smooth((inner-t)/(inner*.55))
    return np.where(t > inner, shelf, deep)


def surface_of(grid, body):
    ring = [_sample(grid, *_point(body, 1.15, a)) for a in np.linspace(0, math.tau, 32, endpoint=False)]
    return round(float(np.median(ring)), 2)


def shore_point(grid, body, surface, angle):
    """Walk out from the centre along `angle` to where the finished ground
    first rises above the surface; the footprint's outline sits a little
    beyond, on dry ground, so no water hangs past the bank."""
    rmax = max(body['rx'], body['rz'])
    step = 0.25/rmax
    t = 0.5
    while t < 3.0:
        x, z = _point(body, t, angle)
        if _sample(grid, x, z) >= surface+0.02:
            return _point(body, t+0.5/rmax, angle)
        t += step
    raise ValueError(f"{body['name']}: no bank found at angle {angle:.2f}")


def carve(grid, bodies):
    """Carve every body into `grid` (a float 256x256 array, modified in place)
    and return the manifest water volumes."""
    iz, ix = np.mgrid[0:N, 0:N]
    X, Z = ix*STEP, iz*STEP
    volumes = []
    for body in bodies:
        surface = surface_of(grid, body)
        t = radius_t(body, X, Z)
        inside = t < 1.0
        # The bed is set exactly, so a pond on a slope is as deep on its
        # downhill side as its uphill side.
        bed = surface-depth_at(body, np.minimum(t, .999))
        grid[inside] = bed[inside]
        # A crest at least one grid step wide just outside the shore, so the
        # bilinear ground between grid points holds the water in, then a bank
        # blending back to the natural ground.
        plateau = 1.2*STEP/min(body['rx'], body['rz'])
        crest = surface+FREEBOARD
        grid[(t >= 1.0) & (t < 1.0+plateau)] = crest
        band = (t >= 1.0+plateau) & (t < 1.0+plateau+BANK)
        w = _smooth((t[band]-1.0-plateau)/BANK)
        grid[band] = crest+(grid[band]-crest)*w
        polygon = [[round(p, 2) for p in shore_point(grid, body, surface, a)]
                   for a in np.linspace(0, math.tau, OUTLINE, endpoint=False)]
        volume = {'surface': surface, 'polygon': polygon, 'depth': round(body['depth']+2.0, 2)}
        if body.get('flow'): volume['flow'] = [float(v) for v in body['flow']]
        if body.get('color'): volume['color'] = [float(v) for v in body['color']]
        volumes.append(volume)
    return volumes


def wet_banks(weights, grid, bodies, volumes, channel):
    """Shift the splat toward `channel` (mud, peat, ice crust, wet sand) on
    the bed and a band of bank round each body. `weights` is uint8 (N, N, 4)."""
    iz, ix = np.mgrid[0:N, 0:N]
    X, Z = ix*STEP, iz*STEP
    out = weights.astype(np.float64)/255
    for body in bodies:
        t = radius_t(body, X, Z)
        strength = np.clip((1.0+BANK*.8-t)/(BANK*.6), 0, 1)*.85
        target = np.zeros(4); target[channel] = 1.0
        out = out*(1-strength[..., None])+target*strength[..., None]
    out /= np.maximum(out.sum(-1, keepdims=True), 1e-6)
    return np.round(out*255).astype(np.uint8)


def contains(volume, x, z):
    """Point-in-footprint test for a manifest volume (rect or polygon)."""
    if 'rect' in volume:
        x0, z0, x1, z1 = volume['rect']
        return x0 <= x <= x1 and z0 <= z <= z1
    poly = volume['polygon']; inside = False
    j = len(poly)-1
    for i in range(len(poly)):
        (xi, zi), (xj, zj) = poly[i], poly[j]
        if (zi > z) != (zj > z) and x < (xj-xi)*(z-zi)/(zj-zi)+xi: inside = not inside
        j = i
    return inside


def mirror(body, centre):
    """The point-symmetric partner of a body about the map centre."""
    cx, cz = centre
    out = dict(body)
    out['x'], out['z'] = 2*cx-body['x'], 2*cz-body['z']
    out['yaw'] = body.get('yaw', 0.0)+180.0
    if body.get('flow'): out['flow'] = [-body['flow'][0], -body['flow'][1]]
    out['name'] = body['name']+' (mirror)'
    return out


def bodies_from(definition):
    """Expand a map definition's "water" entry into every body, adding each
    body's point-symmetric partner when "mirror" is set."""
    water = definition.get('water')
    if not water: return []
    out = []
    for body in water['bodies']:
        out.append(dict(body))
        if water.get('mirror'): out.append(mirror(body, water['centre']))
    return out


def apply(definition, grid):
    """Carve a definition's water into `grid`; returns (bodies, volumes)."""
    bodies = bodies_from(definition)
    return bodies, (carve(grid, bodies) if bodies else [])


__all__ = ['apply', 'bodies_from', 'carve', 'wet_banks', 'contains', 'mirror', 'radius_t', 'depth_at', 'surface_of']
