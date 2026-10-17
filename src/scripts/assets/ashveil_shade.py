"""Ashveil's terrain shade map (`shade.rg`) for its 2.56 m terrain grid.

The same bake as `terrain_shade` (sun visibility over the heightfield and
from mesh casters, then ambient occlusion from concavity and overhead
cover), on a 1024 x 1024 map over the 655.36 m the shader samples
(256 * `terrain_step`): 0.64 m texels. `terrain_shade` assumes the 8 m
grid, and other packs record its hash, so this module reuses its helpers
and supplies the step.
"""
import hashlib
from pathlib import Path

import numpy as np

from assets import terrain_shade as base

SIZE = base.SIZE


def sample(grid, x, z, step):
    return base.sample(grid, x, z, step)


def terrain_visibility(grid, x, z, h, sun, step, march=1.5):
    vis = np.ones_like(h)
    ymax = grid.max()
    idx = np.arange(len(h))
    t = 1.0
    start = h+.2
    while len(idx):
        py = start[idx]+sun[1]*t
        keep = py < ymax+.5
        idx = idx[keep]; py = py[keep]
        if not len(idx): break
        th = sample(grid, x[idx]+sun[0]*t, z[idx]+sun[2]*t, step)
        v = np.clip((py-th)/(.03*t+.35), 0, 1)
        vis[idx] = np.minimum(vis[idx], v)
        idx = idx[vis[idx] > 0]
        t += march
    return vis


def mesh_visibility(tris, sun, x, z, h, res=.5):
    if not len(tris): return np.ones_like(h)
    u, w = base._basis(sun)
    depth, amin, bmin = base._raster_max(tris@u, tris@w, tris@sun, res)
    p = np.stack([x, h, z], -1)
    pa = (p@u-amin)/res; pb = (p@w-bmin)/res; ps = p@sun
    lit = np.zeros_like(h)
    H, W = depth.shape
    offsets = [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]
    for da, db in offsets:
        ia = np.clip((pa+da).astype(np.int64), 0, W-1); ib = np.clip((pb+db).astype(np.int64), 0, H-1)
        lit += depth[ib, ia] <= ps+base.BIAS
    return lit/len(offsets)


def overhead_ao(tris, x, z, h, texel):
    if not len(tris): return np.ones_like(h)
    top, amin, bmin = base._raster_max(tris[..., 0], tris[..., 2], tris[..., 1], texel)
    ia = np.clip(((x-amin)/texel).astype(np.int64), 0, top.shape[1]-1)
    ib = np.clip(((z-bmin)/texel).astype(np.int64), 0, top.shape[0]-1)
    gap = top[ib, ia]-h
    return np.where(np.isfinite(gap) & (gap > -.5), 1-.5*np.exp(-np.maximum(gap, 0)/12), 1.0)


def concavity_ao(grid, x, z, h, step):
    broad = base._blur(grid, 6)
    cav = np.maximum(sample(broad, x, z, step)-h, 0)
    return 1-np.clip(cav/14, 0, .3)


def bake(height_bytes, vertex_bytes, sun, light_material, step):
    """Return (shade.rg bytes, manifest metadata)."""
    grid = base.heights(height_bytes)
    sun = np.asarray(sun, np.float64); sun = sun/np.linalg.norm(sun)
    texel = 256*step/SIZE
    c = (np.arange(SIZE)+.5)*texel
    xg, zg = np.meshgrid(c, c)
    x, z = xg.ravel(), zg.ravel()
    h = sample(grid, x, z, step)
    tris = base.casters(vertex_bytes, (light_material,))
    vis = terrain_visibility(grid, x, z, h, sun, step)*mesh_visibility(tris, sun, x, z, h)
    vis = base._blur(vis.reshape(SIZE, SIZE), 2)
    ao = (concavity_ao(grid, x, z, h, step)*overhead_ao(tris, x, z, h, texel)).reshape(SIZE, SIZE)
    ao = np.clip(base._blur(ao, 2), .4, 1)
    out = np.empty((SIZE, SIZE, 2), np.uint8)
    out[..., 0] = np.clip(np.round(vis*255), 0, 255)
    out[..., 1] = np.clip(np.round(ao*255), 0, 255)
    meta = dict(size=SIZE, texel_m=round(texel, 4), sun=[round(float(v), 6) for v in sun],
                shaded_fraction=round(float((out[..., 0] < 128).mean()), 4),
                baker_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                base_baker_sha256=hashlib.sha256(Path(base.__file__).read_bytes()).hexdigest())
    return out.tobytes(), meta
