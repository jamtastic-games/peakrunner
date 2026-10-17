"""Baked terrain shade map (`shade.rg`): sun visibility and ambient occlusion
over the whole 2 km terrain tile.

The payload holds 1024x1024 texels, two bytes each, rows along +z and columns
along +x. Each texel covers 2 m (the 256-cell, 8 m terrain tile), and texel
(i, j) is centred at x = (i+0.5)*2, z = (j+0.5)*2.

* R: sun visibility, 0 (fully shadowed) to 255 (lit), from three sources:
  - terrain self-shadow: a soft ray-march along the sun over the heightfield;
  - mesh shadow: every opaque render triangle (bases, floating hulls,
    bridges, towers, big props) rasterised into an orthographic depth map
    along the sun, then tested with a small filtered lookup;
  - a light blur for soft edges.
* G: ambient occlusion: terrain concavity (height below its broad
  neighbourhood) times overhead cover (how close the nearest structure hangs
  above), so the ground under a floating base or a bridge darkens softly.

The terrain shader (src/map.wgsl) multiplies the sun term by R and the sky and
ground ambient by G, for the terrain and for props. The sun direction must be
the one the renderer and the structure lightmaps use.

Terrain is not in the caster set, so there is no shadow acne. The caster bias
is small (BIAS metres) and there is no peter-panning at contact.
"""
import hashlib
from pathlib import Path

import numpy as np

SIZE = 1024
TEXEL = 2.0
STEP = 8.0
DEPTH_RES = 1.0
BIAS = 0.35
MARCH_STEP = 3.0


def heights(height_bytes):
    return np.frombuffer(height_bytes, '<u2').reshape(256, 256).astype(np.float64)/32


def sample(grid, x, z, step=STEP):
    """Bilinear heightfield lookup, vectorised; matches the engine's."""
    fx = np.clip(x/step, 0, 255); fz = np.clip(z/step, 0, 255)
    x0 = np.floor(fx).astype(np.int64); z0 = np.floor(fz).astype(np.int64)
    x1 = np.minimum(x0+1, 255); z1 = np.minimum(z0+1, 255)
    tx = fx-x0; tz = fz-z0
    a = grid[z0, x0]+(grid[z0, x1]-grid[z0, x0])*tx
    b = grid[z1, x0]+(grid[z1, x1]-grid[z1, x0])*tx
    return a+(b-a)*tz


def _blur(img, radius):
    """Separable box-of-binomials blur with edge padding (deterministic)."""
    k = np.array([1.0])
    for _ in range(2*radius): k = np.convolve(k, [.5, .5])
    r = len(k)//2
    for axis in (0, 1):
        pad = [(0, 0), (0, 0)]; pad[axis] = (r, r)
        p = np.pad(img, pad, mode='edge')
        out = np.zeros_like(img)
        for i, w in enumerate(k):
            out += w*(p[i:i+img.shape[0], :] if axis == 0 else p[:, i:i+img.shape[1]])
        img = out
    return img


def _texels():
    c = (np.arange(SIZE)+.5)*TEXEL
    x, z = np.meshgrid(c, c)            # [row=z, col=x]
    return x.ravel(), z.ravel()


def terrain_visibility(grid, x, z, h, sun):
    """Soft ray-march toward the sun over the heightfield."""
    vis = np.ones_like(h)
    ymax = grid.max()
    idx = np.arange(len(h))
    t = 2.0
    base = h+.3
    while len(idx):
        py = base[idx]+sun[1]*t
        keep = py < ymax+.5
        idx = idx[keep]; py = py[keep]
        if not len(idx): break
        th = sample(grid, x[idx]+sun[0]*t, z[idx]+sun[2]*t)
        v = np.clip((py-th)/(.03*t+.6), 0, 1)
        vis[idx] = np.minimum(vis[idx], v)
        alive = vis[idx] > 0
        idx = idx[alive]
        t += MARCH_STEP
    return vis


def _basis(sun):
    up = np.array([0., 1., 0.]) if abs(sun[1]) < .9 else np.array([1., 0., 0.])
    u = np.cross(sun, up); u /= np.linalg.norm(u)
    w = np.cross(sun, u)
    return u, w


def _raster_max(a, b, s, res):
    """Rasterise triangles (a, b coordinates in metres, s value per vertex)
    into a grid holding the maximum interpolated s per pixel."""
    amin, bmin = a.min()-2*res, b.min()-2*res
    W = int(np.ceil((a.max()-amin)/res))+3; H = int(np.ceil((b.max()-bmin)/res))+3
    depth = np.full((H, W), -np.inf, np.float64)
    pa = (a-amin)/res; pb = (b-bmin)/res
    for i in range(len(pa)):
        xa, ya, sa = pa[i], pb[i], s[i]
        x0 = int(np.floor(xa.min())); x1 = int(np.ceil(xa.max()))
        y0 = int(np.floor(ya.min())); y1 = int(np.ceil(ya.max()))
        area = (xa[1]-xa[0])*(ya[2]-ya[0])-(xa[2]-xa[0])*(ya[1]-ya[0])
        if abs(area) < 1e-6:
            continue
        gx, gy = np.meshgrid(np.arange(x0, x1+1)+.5, np.arange(y0, y1+1)+.5)
        w0 = ((xa[1]-gx)*(ya[2]-gy)-(xa[2]-gx)*(ya[1]-gy))/area
        w1 = ((xa[2]-gx)*(ya[0]-gy)-(xa[0]-gx)*(ya[2]-gy))/area
        w2 = 1-w0-w1
        inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (w2 >= -1e-6)
        if not inside.any():
            # Sub-pixel sliver: mark the pixel under its centroid.
            cx, cy = int(xa.mean()), int(ya.mean())
            depth[cy, cx] = max(depth[cy, cx], float(sa.max()))
            continue
        val = w0*sa[0]+w1*sa[1]+w2*sa[2]
        sub = depth[y0:y1+1, x0:x1+1]
        np.maximum(sub, np.where(inside, val, -np.inf), out=sub)
    return depth, amin, bmin


def mesh_visibility(tris, sun, x, z, h):
    """Sun visibility from mesh casters via an orthographic depth map."""
    if not len(tris): return np.ones_like(h)
    u, w = _basis(sun)
    a = tris@u; b = tris@w; s = tris@sun
    depth, amin, bmin = _raster_max(a, b, s, DEPTH_RES)
    p = np.stack([x, h, z], -1)
    pa = (p@u-amin)/DEPTH_RES; pb = (p@w-bmin)/DEPTH_RES; ps = p@sun
    lit = np.zeros_like(h)
    offsets = [(-1.2, -1.2), (0, -1.2), (1.2, -1.2), (-1.2, 0), (0, 0), (1.2, 0), (-1.2, 1.2), (0, 1.2), (1.2, 1.2)]
    H, W = depth.shape
    for da, db in offsets:
        ia = np.clip((pa+da).astype(np.int64), 0, W-1); ib = np.clip((pb+db).astype(np.int64), 0, H-1)
        lit += depth[ib, ia] <= ps+BIAS
    return lit/len(offsets)


def overhead_ao(tris, x, z, h):
    """Soft darkening under anything hanging over the ground."""
    if not len(tris): return np.ones_like(h)
    a = tris[..., 0]; b = tris[..., 2]; s = tris[..., 1]
    # Only faces above the ground they hang over matter; a coarse 2 m map.
    top, amin, bmin = _raster_max(a, b, s, TEXEL)
    ia = np.clip(((x-amin)/TEXEL).astype(np.int64), 0, top.shape[1]-1)
    ib = np.clip(((z-bmin)/TEXEL).astype(np.int64), 0, top.shape[0]-1)
    gap = top[ib, ia]-h
    ao = np.where(np.isfinite(gap) & (gap > -.5), 1-.5*np.exp(-np.maximum(gap, 0)/18), 1.0)
    return ao


def concavity_ao(grid, x, z, h):
    broad = _blur(grid, 3)
    cav = np.maximum(sample(broad, x, z)-h, 0)
    return 1-np.clip(cav/40, 0, .3)


def casters(vertex_bytes, skip_materials=()):
    """Opaque render triangles (N,3,3) from kit vertex bytes."""
    v = np.frombuffer(vertex_bytes, '<f4').reshape(-1, 3, 12)
    keep = ~np.isin(v[:, 0, 10], list(skip_materials))   # column 10: material layer
    # Terrain-encoded lids over cut cells (column 11 == -2) are the ground
    # itself: the heightfield already self-shadows, and as casters they sit
    # exactly on the texels being baked and shadow them.
    keep &= v[:, 0, 11] != -2
    return v[keep][:, :, :3].astype(np.float64)


def bake(height_bytes, vertex_bytes, sun, light_material):
    """Return (shade.rg bytes, manifest metadata)."""
    grid = heights(height_bytes)
    sun = np.asarray(sun, np.float64); sun = sun/np.linalg.norm(sun)
    x, z = _texels()
    h = sample(grid, x, z)
    tris = casters(vertex_bytes, (light_material,))
    vis = terrain_visibility(grid, x, z, h, sun)*mesh_visibility(tris, sun, x, z, h)
    vis = _blur(vis.reshape(SIZE, SIZE), 2)
    ao = (concavity_ao(grid, x, z, h)*overhead_ao(tris, x, z, h)).reshape(SIZE, SIZE)
    ao = np.clip(_blur(ao, 2), .4, 1)
    out = np.empty((SIZE, SIZE, 2), np.uint8)
    out[..., 0] = np.clip(np.round(vis*255), 0, 255)
    out[..., 1] = np.clip(np.round(ao*255), 0, 255)
    meta = dict(size=SIZE, texel_m=TEXEL, sun=[round(float(v), 6) for v in sun],
                shaded_fraction=round(float((out[..., 0] < 128).mean()), 4),
                baker_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    return out.tobytes(), meta
