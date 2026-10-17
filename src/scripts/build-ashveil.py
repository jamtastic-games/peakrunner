#!/usr/bin/env python3
"""Ashveil, a volcanic Team Deathmatch, CTF and Capture & Hold map: our own
caldera (docs/ashveil.md). A spired temple on a low dome in the middle,
an octagonal cathedral dug under it that two lava tubes, two vents and a
well lead into, a breached cinder cone in the north and a spatter cone in
the northeast, an old flow from the east wall, a fissure across the south,
lava ponds with hot air rising over them, basalt outcrops, rock arches,
the bones of three giant beasts, and loose rock;
team 1's skull tower in its lava moat in the west, team 0's basalt mesa in
the east, and two lava valleys sweeping round the sides.

Everything is generated here from `maps/ashveil.json` by our own modules:
the ground (`assets/ashveil_land`), the ring of valleys and the tower's site
(`assets/ashveil_ring`), the temple (`assets/ashveil_temple`), the
cathedral (`assets/ashveil_undercroft`), the mesa (`assets/ashveil_mesa`),
the skull tower (`assets/ashveil_tower`), the beasts and rock arches
(`assets/ashveil_beasts`), the field's rock and the far land
(`assets/ashveil_scatter`) and every texture (`assets/ashveil_materials`).
Every bit of lava that shows burns (`lava.bin`, `lava_ground`).

Usage (from src/, with scripts/requirements.txt installed): build-ashveil.py [OUTPUT] [--no-bake]
Default output is assets/maps/ashveil; refuses to overwrite.
"""
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys

import numpy as np

from assets import ashveil_beasts as BE
from assets import ashveil_land as LAND
from assets import ashveil_materials as M
from assets import ashveil_mesa as ME
from assets import ashveil_ring as RG
from assets import ashveil_scatter as SC
from assets import ashveil_shade
from assets import ashveil_shapes as A
from assets import ashveil_temple as TP
from assets import ashveil_tower as TW
from assets import ashveil_undercroft as UC
from assets import lightmap_bake
from assets import pack_writer

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.util.spec_from_file_location('kit', ROOT/'scripts/build-original-map.py')
kit = importlib.util.module_from_spec(loader)
loader.loader.exec_module(kit)
STEP = LAND.STEP
# Collision is kept this far round the fence; past it the scenery is
# backdrop (live lit, nothing to hit).
SOLID_MARGIN = 30.0
# Fence and ceiling sheets are cut into pieces no longer than this (map
# packs refuse a collision triangle over 512 m).
PIECE = 120.0
SPAWN_LIFT = 1.2
POOL_MARGIN = 2.0
IDENT = np.c_[np.eye(3), np.zeros(3)]


def spec():
    return json.loads((ROOT/'maps/ashveil.json').read_text())


def materials(count=None):
    """Texture layer of every kit material, and of Ashveil's own (`M.EXTRA`,
    appended after the shared pack's `count` layers)."""
    out = {name: kit.MATERIALS.index(name) for name in kit.MATERIALS}
    if count is not None: out.update({name: count+k for k, name in enumerate(M.EXTRA)})
    return out


def append_layers(textures, count, images):
    """`textures` (level-major mip chains of `count` layers) with `images`
    (256 px RGBA) added as new layers after them."""
    chains = []
    for img in images:
        chain, side = [img], 256
        while side > 1: chain.append(kit.downsample(chain[-1], side)); side //= 2
        chains.append(chain)
    out, side, offset = bytearray(), 256, 0
    for level in range(9):
        size = side*side*4
        out += textures[offset:offset+count*size]
        for chain in chains: out += chain[level]
        offset += count*size; side //= 2
    return out, count+len(images)


def tower_matrix(tower):
    """The skull tower's frame (+x forward, +y up) to the world: turned to
    its facing (degrees from +X toward +Z), on its plinth."""
    a = math.radians(tower['facing']); c, s = math.cos(a), math.sin(a)
    x, z = tower['at']
    return np.array([[c, 0, -s, x], [0, 1, 0, tower['plinth']], [s, 0, c, z]], np.float64)


def at(m, p):
    return tuple(float(v) for v in np.asarray(m)[:, :3]@np.asarray(p, np.float64)+np.asarray(m)[:, 3])


def yaw_toward(degrees):
    """Engine yaw (forward = (-sin, -cos)) facing the source's heading
    (degrees from +X toward +Z)."""
    a = math.radians(degrees)
    return math.atan2(-math.cos(a), -math.sin(a)) % (2*math.pi)


def within(area, margin):
    x0, z0, x1, z1 = area
    return lambda x, z: x0-margin <= x <= x1+margin and z0-margin <= z <= z1+margin


def ground(grid, x, z):
    return pack_writer.sample_height(grid, x, z, STEP)


def floor_below(x, z, y_from, solid, grid, holes):
    """The highest floor under (x, z) from y_from down: a solid face or
    the terrain (unless the cell is cut)."""
    best = -1e9
    cell = int(min(max(z/STEP, 0), 255))*256+int(min(max(x/STEP, 0), 255))
    if cell not in holes: best = ground(grid, x, z)
    lo, hi = solid.min(1), solid.max(1)
    near = (lo[:, 0] <= x) & (hi[:, 0] >= x) & (lo[:, 2] <= z) & (hi[:, 2] >= z) & (lo[:, 1] <= y_from)
    for t in solid[near]:
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
        det = (bz-cz)*(ax-cx)+(cx-bx)*(az-cz)
        if abs(det) < 1e-9: continue
        u = ((bz-cz)*(x-cx)+(cx-bx)*(z-cz))/det
        v = ((cz-az)*(x-cx)+(ax-cx)*(z-cz))/det
        w = 1-u-v
        if min(u, v, w) < -1e-6: continue
        y = u*ay+v*by+w*cy
        if best < y <= y_from: best = y
    return best


def pieces(lo, hi):
    """Boxes covering lo..hi, none longer than PIECE along x or z."""
    nx = max(1, math.ceil((hi[0]-lo[0])/PIECE)); nz = max(1, math.ceil((hi[2]-lo[2])/PIECE))
    xs = np.linspace(lo[0], hi[0], nx+1); zs = np.linspace(lo[2], hi[2], nz+1)
    return [((xs[i], lo[1], zs[j]), (xs[i+1], hi[1], zs[j+1])) for i in range(nx) for j in range(nz)]


def solid_box(mesh, lo, hi):
    c = [(a+b)/2 for a, b in zip(lo, hi)]; s = [b-a for a, b in zip(lo, hi)]
    mesh.collision_box(c, s)


def sheet(mesh, corners, normal):
    """One collision quad facing `normal` (collision is one-sided: it stops
    what comes at its front). The fence and ceiling are inward sheets, so
    they have no walkable tops for bots to plan over."""
    a, b, c, d = [np.asarray(p, np.float64) for p in corners]
    if np.dot(np.cross(b-a, c-a), normal) < 0: b, d = d, b
    for tri in ((a, b, c), (a, c, d)):
        mesh.collision.extend(float(v) for p in tri for v in p)


def fence_sheets(mesh, area, floor, top):
    """The play square's four inward walls from `floor` to `top`, and the
    ceiling at `top` facing down, in pieces no longer than PIECE."""
    x0, z0, x1, z1 = area
    def cuts(a, b): n = max(1, math.ceil((b-a)/PIECE)); return np.linspace(a, b, n+1)
    for za, zb in zip(cuts(z0, z1)[:-1], cuts(z0, z1)[1:]):
        sheet(mesh, [(x0, floor, za), (x0, floor, zb), (x0, top, zb), (x0, top, za)], (1, 0, 0))
        sheet(mesh, [(x1, floor, za), (x1, floor, zb), (x1, top, zb), (x1, top, za)], (-1, 0, 0))
    for xa, xb in zip(cuts(x0, x1)[:-1], cuts(x0, x1)[1:]):
        sheet(mesh, [(xa, floor, z0), (xb, floor, z0), (xb, top, z0), (xa, top, z0)], (0, 0, 1))
        sheet(mesh, [(xa, floor, z1), (xb, floor, z1), (xb, top, z1), (xa, top, z1)], (0, 0, -1))
        for za, zb in zip(cuts(z0, z1)[:-1], cuts(z0, z1)[1:]):
            sheet(mesh, [(xa, top, za), (xb, top, za), (xb, top, zb), (xa, top, zb)], (0, -1, 0))


def clearance(tris, origin, direction, reach=40.0):
    """How far a ray from `origin` goes along `direction` before it meets
    a collision triangle (`reach` when nothing is that close)."""
    o, d = np.asarray(origin, np.float64), np.asarray(direction, np.float64)
    d = d/np.linalg.norm(d)
    a = tris[:, 0]; e1, e2 = tris[:, 1]-a, tris[:, 2]-a
    p = np.cross(d, e2); det = (e1*p).sum(1)
    ok = np.abs(det) > 1e-9
    inv = np.where(ok, 1/np.where(ok, det, 1), 0)
    s = o-a; u = (s*p).sum(1)*inv
    q = np.cross(s, e1); v = (d*q).sum(1)*inv; t = (e2*q).sum(1)*inv
    hit = ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > .05) & (t < reach)
    return float(t[hit].min()) if hit.any() else reach


def open_heading(tris, x, y, z, heading, need=6.0):
    """The station's heading (degrees from +X toward +Z) if its front has
    `need` m clear (the fan of the front, from beside the booth), else the
    clearest heading round it, nearest the source's."""
    def room(deg):
        r = [math.radians(deg+t) for t in (-25, 0, 25)]
        return min(clearance(tris, (x+math.cos(a)*2.6, y+1.0, z+math.sin(a)*2.6), (math.cos(a), 0, math.sin(a))) for a in r)
    if room(heading) >= need: return heading
    best = max(range(0, 360, 15), key=lambda d: (round(room(heading+d), 1), -min(d, 360-d)))
    return (heading+best) % 360


def booth(mesh, x, y, z, yaw):
    """A shared inventory booth (our own, neutral: dark stone posts, bronze
    trim, a glowing panel) opening toward local -z, with its use point."""
    mesh.origin = (x, y, z); mesh.yaw = yaw
    mesh.box((0, .15, 0), (4.2, .3, 4.2), 'trim')
    for dx in (-1.75, 1.75):
        mesh.box((dx, 1.9, .9), (.55, 3.8, .75), 'concrete')
        mesh.box((dx, 2.1, .5), (.16, 2.6, .05), 'light', False)
    mesh.box((0, 3.85, .9), (4.1, .55, .8), 'concrete')
    mesh.box((0, 4.18, .9), (4.3, .12, .9), 'trim', False)
    mesh.box((0, 1.5, 1.5), (2.9, 3.0, .5), 'grate')
    mesh.box((0, 2.3, 1.22), (2.2, .7, .08), 'light', False)
    use = mesh.point((0, 1, -1))
    c, s = math.cos(yaw), math.sin(yaw)
    facing = [round(-s, 4)+0.0, round(-c, 4)+0.0]
    mesh.origin = (0, 0, 0); mesh.yaw = 0
    return use, facing


def flag_stand(mesh, x, y, z):
    """A low stone dais for a flag; returns the flag's foot."""
    mesh.origin = (x, y, z); mesh.yaw = 0
    mesh.column((0, -.6, 0), 2.4, .95, 'concrete', 16)
    mesh.column((0, .35, 0), 2.6, .1, 'trim', 16, solid=False)
    mesh.column((0, .35, 0), 1.0, .06, 'light', 16, solid=False)
    mesh.origin = (0, 0, 0)
    return (x, round(y+.35+.05, 3), z)


def bake(vertices, textures, count, sun, tint, lamps=()):
    """The structures' lightmaps (with `lamps`, [((x, y, z), power)]), warmed
    by `tint` (the shared baker lights with a neutral sky), appended as
    texture layers in level-major order."""
    baked, pages = lightmap_bake.bake(np.frombuffer(vertices, '<f4'), list(lamps), sun, kit.MATERIALS.index('light'), count)
    tint = np.asarray(tint, np.float64)
    warm = []
    for page in pages:
        a = np.frombuffer(page, np.uint8).reshape(256, 256, 4).astype(np.float64)
        a[..., :3] = np.clip(np.round(a[..., :3]*tint), 0, 255)
        warm.append(a.astype(np.uint8).tobytes())
    mips = [[page] for page in warm]
    for chain in mips:
        side = 256
        while side > 1: chain.append(kit.downsample(chain[-1], side)); side //= 2
    merged, side, offset = bytearray(), 256, 0
    for level in range(9):
        size = side*side*4
        merged += textures[offset:offset+count*size]
        for chain in mips: merged += chain[level]
        offset += count*size; side //= 2
    meta = dict(pages=len(pages), first_layer=count, texel_m=.5, sun=[round(float(v), 6) for v in sun], lamps=len(lamps),
                tint=list(tint), baker_sha256=pack_writer.source_hash(lightmap_bake.__file__))
    return baked.astype('<f4').tobytes(), merged, count+len(pages), meta


def lava_tops(tris, side):
    """The highest surface of the flat-ish lava triangles (|normal.y| over
    0.5: falls down rock faces are left out) at each of side x side sample
    points over the tile, -inf where there's none."""
    s = 256*STEP/side
    top = np.full((side, side), -np.inf)
    p = tris[:, :, :3].astype(np.float64)
    n = np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0])
    flat = np.abs(n[:, 1]) > 0.5*np.maximum(np.linalg.norm(n, axis=1), 1e-12)
    for t in p[flat]:
        lo = np.clip(np.ceil(t[:, [0, 2]].min(0)/s-.5), 0, side-1).astype(int)
        hi = np.clip(np.floor(t[:, [0, 2]].max(0)/s-.5), 0, side-1).astype(int)
        if (hi < lo).any(): continue
        X, Z = np.meshgrid((np.arange(lo[0], hi[0]+1)+.5)*s, (np.arange(lo[1], hi[1]+1)+.5)*s)
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
        det = (bz-cz)*(ax-cx)+(cx-bx)*(az-cz)
        if abs(det) < 1e-9: continue
        u = ((bz-cz)*(X-cx)+(cx-bx)*(Z-cz))/det; v = ((cz-az)*(X-cx)+(ax-cx)*(Z-cz))/det
        y = np.where((u >= -1e-6) & (v >= -1e-6) & (u+v <= 1+1e-6), u*ay+v*by+(1-u-v)*cy, -np.inf)
        view = top[lo[1]:hi[1]+1, lo[0]:hi[0]+1]
        np.maximum(view, y, out=view)
    return top


def lava_ground(grid, holes, collision, hot, cool, burn, flow_burn, clear):
    """`lava.bin`: the lava each terrain cell shows, so all of it burns.
    The lava plane (`hot`) is skim ground burning like the pools; the
    cooled flows (`cool`) lie on the ground they cover and burn
    `flow_burn`. A cell counts when lava shows over at least half of its
    samples. Cells under a tunnel's cut, under a floor at or just over the
    lava (the tunnel, the plazas at its ends), or within `clear` m of
    `points` (spawns, flags, stations) get none."""
    sub = 4; side = 256*sub; s = STEP/sub
    xs = (np.arange(side)+.5)*s
    X, Z = np.meshgrid(xs, xs)
    q = np.round(grid*32)/32
    fx, fz = np.clip(X/STEP, 0, 255), np.clip(Z/STEP, 0, 255)
    x0, z0 = np.minimum(fx.astype(int), 254), np.minimum(fz.astype(int), 254)
    tx, tz = fx-x0, fz-z0
    ground = (q[z0, x0]*(1-tx)+q[z0, x0+1]*tx)*(1-tz)+(q[z0+1, x0]*(1-tx)+q[z0+1, x0+1]*tx)*tz
    top_hot, top_cool = lava_tops(hot, side), lava_tops(cool, side)
    shows_hot = top_hot > ground+0.02
    shows_cool = ~shows_hot & (top_cool > ground-0.1)
    block = lambda a: a.reshape(256, sub, 256, sub)
    cover = block(shows_hot | shows_cool).mean((1, 3))
    hot_share = block(shows_hot).mean((1, 3))
    surface = np.maximum(block(np.where(shows_hot, top_hot, -np.inf)).max((1, 3)),
                         block(np.where(shows_cool, top_cool, -np.inf)).max((1, 3)))
    lava = (cover >= 0.5) & np.isfinite(surface)
    lava.ravel()[list(holes)] = False
    # Floors at or just over the lava hide it: no lava there.
    tris = collision.reshape(-1, 3, 3).astype(np.float64)
    n = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    floors = tris[n[:, 1] > 0.5*np.maximum(np.linalg.norm(n, axis=1), 1e-12)]
    for t in floors:
        lo = np.clip(np.ceil(t[:, [0, 2]].min(0)/STEP-.5), 0, 255).astype(int)
        hi = np.clip(np.floor(t[:, [0, 2]].max(0)/STEP-.5), 0, 255).astype(int)
        if (hi < lo).any() or not lava[lo[1]:hi[1]+1, lo[0]:hi[0]+1].any(): continue
        X, Z = np.meshgrid((np.arange(lo[0], hi[0]+1)+.5)*STEP, (np.arange(lo[1], hi[1]+1)+.5)*STEP)
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
        det = (bz-cz)*(ax-cx)+(cx-bx)*(az-cz)
        if abs(det) < 1e-9: continue
        u = ((bz-cz)*(X-cx)+(cx-bx)*(Z-cz))/det; v = ((cz-az)*(X-cx)+(ax-cx)*(Z-cz))/det
        y = u*ay+v*by+(1-u-v)*cy
        s0 = surface[lo[1]:hi[1]+1, lo[0]:hi[0]+1]
        over = (u >= -1e-6) & (v >= -1e-6) & (u+v <= 1+1e-6) & (y >= s0-0.05) & (y <= s0+2.5)
        lava[lo[1]:hi[1]+1, lo[0]:hi[0]+1] &= ~over
    cx = (np.arange(256)+.5)*STEP
    CX, CZ = np.meshgrid(cx, cx)
    for x, z in clear['points']:
        lava &= np.hypot(CX-x, CZ-z) > clear['radius']
    hot_cell = lava & (hot_share*2 >= cover)
    out = np.zeros((256, 256, 4), np.uint8)
    h = np.clip(np.round(np.where(lava, surface, 0)*32), 0, 65535).astype('<u2')
    out[..., 0], out[..., 1] = (h & 255).astype(np.uint8), (h >> 8).astype(np.uint8)
    out[..., 2] = np.where(hot_cell, round(burn), round(flow_burn))*lava
    out[..., 3] = hot_cell
    return out.tobytes(), int(hot_cell.sum()), int((lava & ~hot_cell).sum())

def sample(grid, x, z):
    return float(ashveil_shade.sample(grid, np.array([x]), np.array([z]), STEP)[0])


def heading_to(x, z, target):
    """Degrees from +X toward +Z from (x, z) to `target`."""
    return math.degrees(math.atan2(target[1]-z, target[0]-x))


def ribbon(q, half, ground, lift, step=1.5, lanes=5):
    """A lava ribbon along the path q (n, 2), `half` m either side (a number
    or one per sample), lying `lift` over the ground: a flow form with its
    rim (0 in the middle, 1 at the edges) for the cooling crust."""
    q = np.asarray(q, np.float64)
    seg = np.r_[0, np.cumsum(np.linalg.norm(np.diff(q, axis=0), axis=1))]
    keep = np.unique(np.searchsorted(seg, np.arange(0, seg[-1], step)))
    q = q[keep]
    half = np.broadcast_to(np.asarray(half, np.float64), (len(seg),))[keep]
    tan = np.gradient(q, axis=0); tan /= np.maximum(np.linalg.norm(tan, axis=1), 1e-9)[:, None]
    left = np.c_[-tan[:, 1], tan[:, 0]]
    w = np.linspace(-1, 1, lanes)
    v, rim = [], []
    for i in range(len(q)):
        for t in w:
            x, z = q[i]+left[i]*t*half[i]
            v.append((x, ground(x, z)+lift, z)); rim.append(abs(t))
    v = np.array(v); f = []
    for i in range(len(q)-1):
        for j in range(lanes-1):
            a, b = i*lanes+j, i*lanes+j+1
            c, d = (i+1)*lanes+j+1, (i+1)*lanes+j
            f += [(a, b, c), (a, c, d)]
    return A.Form(v, RG._up(v, f), 'water', False, False, rim)


def disc(x, z, r, y, sides=24):
    """A flat lava disc (for the lava ground under a pool only)."""
    a = np.linspace(0, math.tau, sides, endpoint=False)
    v = np.r_[[[x, y, z]], np.c_[x+r*np.cos(a), np.full(sides, y), z+r*np.sin(a)]]
    f = [(0, 1+(k+1) % sides, 1+k) for k in range(sides)]
    return A.Form(v, RG._up(v, f), 'water', False, False)


def tube_roof(t, center, end_r):
    """A lava tube's roof: from where the ground first stands a metre over
    the tube's top to `end_r` from the temple's centre (the cathedral's
    passage roofs the rest), a slab whose top follows the ground the tube
    was cut from and whose underside is the tube's ceiling. Returns the
    form, and the samples (index range) it covers."""
    q, tan, floor, height = t['q'], t['tan'], t['floor'], t['height']
    half = t['width']/2
    roof = floor+height
    pre = t['ground']
    ok = [i for i in range(len(q)) if math.dist(q[i], center) >= end_r]
    tall = [i for i in ok if sample(pre, *q[i]) >= roof+1.0]
    i0, i1 = tall[0], ok[-1]
    idx = list(range(i0, i1+1, 3))
    if idx[-1] != i1: idx.append(i1)
    lat = [-half-5.5, -half-3.5, -half-1.5, -half, -1.5, 0.0, 1.5, half, half+1.5, half+3.5, half+5.5]
    top, bot = [], []
    for i in idx:
        left = np.array([-tan[i, 1], tan[i, 0]])
        for l in lat:
            x, z = q[i]+left*l
            top.append((x, max(sample(pre, x, z)+.06, roof+.7), z)); bot.append((x, roof, z))
    n, m = len(idx), len(lat)
    v = np.array(top+bot, np.float64)
    f = []
    B = n*m
    for i in range(n-1):
        for j in range(m-1):
            a, b, c, d = i*m+j, i*m+j+1, (i+1)*m+j+1, (i+1)*m+j
            f += [(a, b, c), (a, c, d), (B+a, B+c, B+b), (B+a, B+d, B+c)]
    for i in range(n-1):
        for j in (0, m-1):
            a, d = i*m+j, (i+1)*m+j
            f += [(a, d, B+d), (a, B+d, B+a)]
    for i in (0, n-1):
        for j in range(m-1):
            a, b = i*m+j, i*m+j+1
            f += [(a, B+b, b), (a, B+a, B+b)]
    form = A.Form(v, f, 'bark', False, True)
    # Each face looking out of the slab: tops up, the underside down, sides
    # and ends away from the slab's middle.
    tris = form.v[form.f]
    nrm = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    mid = tris.mean(1)
    centre = v.mean(0)
    is_top = (form.f < B).all(1); is_bot = (form.f >= B).all(1)
    want = np.where(is_top[:, None], [0, 1, 0], np.where(is_bot[:, None], [0, -1, 0], mid-centre))
    flip = (nrm*want).sum(1) < 0
    form.f[flip] = form.f[flip][:, ::-1]
    return form, (i0, i1)


def tube_lining(t, span, seed):
    """The tube's vault: a rough half-ellipse from the floor's edges to the
    roof, its faces looking in (it's the tube's walls to hit)."""
    q, tan, floor, height = t['q'], t['tan'], t['floor'], t['height']
    half = t['width']/2
    i0, i1 = span
    i0 = max(0, i0-8)
    idx = list(range(i0, i1+1, 4))
    if idx[-1] != i1: idx.append(i1)
    k = 10
    phis = np.linspace(0, math.pi, k)
    v = []
    for i in idx:
        left = np.array([-tan[i, 1], tan[i, 0]])
        for p in phis:
            l = half*math.cos(p)
            y = floor+(height-.05)*math.sin(p)**.8
            x, z = q[i]+left*l
            v.append((x, y, z))
    v = np.array(v)
    v += .35*(np.c_[A.fbm(v*.6, seed, 1.0), np.zeros(len(v)), A.fbm(v*.6+9, seed+1, 1.0)]-.5)*2 \
        * np.tile(np.sin(phis), len(idx))[:, None]
    f = []
    for a in range(len(idx)-1):
        for b in range(k-1):
            p0, p1 = a*k+b, a*k+b+1
            p2, p3 = (a+1)*k+b+1, (a+1)*k+b
            f += [(p0, p1, p2), (p0, p2, p3)]
    f = np.array(f)
    tris = v[f]
    nrm = np.cross(tris[:, 1]-tris[:, 0], tris[:, 2]-tris[:, 0])
    mid = tris.mean(1)
    # Toward the tube's axis line at the floor's middle.
    axis = []
    for a in range(len(idx)-1):
        axis += [np.r_[(q[idx[a]]+q[idx[a+1]])/2][[0, 1]]]*(2*(k-1))
    axis = np.array(axis)
    toward = np.c_[axis[:, 0]-mid[:, 0], (floor+1.0)-mid[:, 1], axis[:, 1]-mid[:, 2]]
    flip = (nrm*toward).sum(1) < 0
    f[flip] = f[flip][:, ::-1]
    return A.Form(v, f, 'bark', False, True)


def pick_spawns(grid, slope, own, other, avoid, count, rules, taken, target):
    """Spawn spots for one team: on gentle ground, between `rules`
    flag_reach of its own flag (`own`, x z) and nearer it than the other's
    by `lead`, kept apart by `spacing` (from `taken` too), the nearest to a
    comfortable 110 m from the flag first. Each faces `target`."""
    lo, hi = rules['flag_reach']
    cands = []
    for x in np.arange(40.0, 615.0, 5.0):
        for z in np.arange(40.0, 615.0, 5.0):
            d = math.dist((x, z), own)
            if not lo <= d <= hi or d+rules['lead'] > math.dist((x, z), other): continue
            if slope(x, z) > rules['slope'] or avoid(x, z): continue
            cands.append((abs(d-110.0), x, z))
    cands.sort()
    out = []
    for _, x, z in cands:
        if any(math.dist((x, z), p) < rules['spacing'] for p in out+list(taken)): continue
        out.append((float(x), float(z)))
        if len(out) == count: break
    return [(x, z, heading_to(x, z, target)) for x, z in out]


def terrain_weights(h, masks, marks, ashen, lava, seed):
    """Four layers: rust dust (0), basalt on steep faces (1), cooled crust by
    the lava (2) and dark ash (3) on the flows, the cones, the valleys'
    floors and the tubes' ground, in drifts elsewhere; the old hills keep
    their dust."""
    X, Z = LAND.coords()
    s = LAND.slope_degrees(h)
    rock = np.clip((s-28)/14, 0, 1)*.92+.03
    near = np.maximum(RG.falloff(lava, 2), np.clip(marks['crust'], 0, 1))
    crust = np.clip(1-rock, 0, 1)*np.clip(near, 0, .95)
    ash_on = masks['flow_top'] | masks['flow_edge'] | masks['cinder'] | marks['floor'] | ashen
    drift = np.clip(LAND.fbm(X, Z, seed+15, 60.0, 3)*1.9+.1, 0, .85)
    second = np.where(ash_on, .85, drift)*np.where(masks['old'], .35, 1.0)
    ash = np.clip(1-rock-crust, 0, 1)*second
    dust = np.clip(1-rock-crust-ash, 0, 1)
    out = np.stack([dust, rock, crust, ash], axis=-1)
    out /= out.sum(-1, keepdims=True)
    return np.round(out*255).astype(np.uint8)


def build(output, bake_light=True):
    if output.exists(): raise ValueError('Refusing to overwrite existing pack')
    definition = spec()
    land, ring, tower, field = definition['land'], definition['ring'], definition['tower'], definition['field']
    seed = definition['seed']
    area = tuple(ring['fence'])
    temple = tuple(land['dome']['at'])
    mesa_at = tuple(land['mesa']['at'])
    sun = np.asarray(definition['sun'], np.float64); sun /= np.linalg.norm(sun)
    shared = kit.base_pack(); old = shared['manifest']
    mats = materials(old['texture_count'])

    # The ground: the caldera, the ring's valleys and the tower's site, the
    # lava tubes' trenches and pits.
    base, masks, feat = LAND.heights(land, area)
    moat = (TW.PLINTH, TW.MOAT, TW.MOAT_LAVA, TW.MOAT_FLOOR)
    grid, valleys, marks = RG.shape(base, STEP, ring, tower, moat, np.zeros_like(base, bool), temple)
    grid, tubes = LAND.cut_tubes(grid, land)
    X, Z = LAND.coords()
    frame = tower_matrix(tower)
    built = TW.build(seed)
    tower_flag = at(frame, built['flag'])

    # Where the lava lies: the craters, the ponds and the fissure.
    pools = []
    for c, floor in zip(land['cones'], feat['crater_floors']):
        half = c['rim_r']*.62*max(c.get('stretch', 1.0), 1.0)
        pools.append((c['at'][0], c['at'][1], half, round(floor+.35, 2)))
    for p, floor in feat['ponds']:
        pools.append((p['at'][0], p['at'][1], p['r']*.82, round(floor+.4, 2)))
    lava = masks['craters'] | masks['trench']
    for x, z, half, s in pools:
        lava |= (np.abs(X-x) < half) & (np.abs(Z-z) < half) & (grid < s)

    # Spawns: the tower's four and the mesa's four, and twelve more for each
    # team on gentle ground in its own half, facing the temple; the ground
    # under each and under the field's stations is levelled.
    slope_map = LAND.slope_degrees(grid)
    slope = lambda x, z: float(ashveil_shade.sample(slope_map, np.array([x]), np.array([z]), STEP)[0])
    tube_lines = [t['q'] for t in tubes]

    beast_rings = BE.footprints(definition['beasts'])+[(x, z, span/2+6.0) for x, z, _, span, _ in definition['rock_arches']]

    def avoid(x, z):
        if math.dist((x, z), temple) < 40 or math.dist((x, z), mesa_at) < 28: return True
        if math.dist((x, z), tower['at']) < TW.MOAT+16: return True
        i, k = int(min(max(x/STEP, 0), 255)), int(min(max(z/STEP, 0), 255))
        near = (slice(max(k-4, 0), k+5), slice(max(i-4, 0), i+5))
        if lava[near].any() or masks['cinder'][k, i] or marks['floor'][near].any(): return True
        if any(np.hypot(q[:, 0]-x, q[:, 1]-z).min() < 14 for q in tube_lines): return True
        if any(math.dist((x, z), s) < 14 for s in definition['stations']): return True
        if any(math.dist((x, z), (ox, oz)) < r*1.3+9 for ox, oz, r, _ in field['outcrops']+field['necks']): return True
        if any(math.dist((x, z), (bx, bz)) < r+6 for bx, bz, r in beast_rings): return True
        return False

    mesa_probe = ME.build(mesa_at, land['mesa']['top'], lambda x, z: sample(grid, x, z), seed)
    rules = definition['spawn_rules']
    fixed0 = [(x, z, h) for x, z, h in mesa_probe['spawns']]
    fixed1 = [(at(frame, p)[0], at(frame, p)[2]) for p in built['spawns']]
    team0 = pick_spawns(grid, slope, mesa_at, tower['at'], avoid, rules['count'], rules, [(x, z) for x, z, _ in fixed0], temple)
    team1 = pick_spawns(grid, slope, tower['at'], mesa_at, avoid, rules['count'], rules, fixed1, temple)
    flat = [(x, z) for x, z, _ in fixed0+team0+team1]+[tuple(s) for s in definition['stations']]
    grid = LAND.level(grid, flat)
    grid = np.round(grid*32)/32
    ground_at = lambda x, z: sample(grid, x, z)
    drape = lambda x, z: ashveil_shade.sample(grid, x, z, STEP)

    holes = sorted(UC.cut(temple, STEP, TP.build(temple, UC.carve)['vents']))
    hole_set = set(holes)

    baked, live, glow, tower_baked, solids = [], [], [], [], []
    hot, cool = [], []
    lamps, lit_lamps, smoke = [], [], []

    def take(parts, group, offset=None):
        for form, lit in parts:
            tris, normals = A.place(form, IDENT if offset is None else offset)
            if not len(tris): continue
            if lit == 'glow': glow.append(A.records(tris, normals, mats['water']))
            elif lit != 'hidden':
                rec = A.records(tris, normals, mats[form.material])
                if form.material.startswith('bone'): rec[..., 6:8] *= .5
                (live if lit == 'live' else group).append(rec)
            if form.solid: solids.append(tris)

    # The temple and the mesa bake with the sun and their own fires.
    tp = TP.build(temple, UC.carve, seed)
    take(tp['parts'], baked)
    me = ME.build(mesa_at, land['mesa']['top'], ground_at, seed)
    take(me['parts'], baked)
    lit_lamps += tp['lamps']+me['lamps']
    smoke += tp['smoke']+me['smoke']

    # The skull tower, its moat and falls; the cathedral and the tubes bake
    # with the tower, lit by their own fires.
    for form, lit in built['parts']:
        tris, normals = A.place(form, frame)
        if not len(tris): continue
        if lit == 'glow': glow.append(A.records(tris, normals, mats['water']))
        else:
            rec = A.records(tris, normals, mats[form.material])
            if form.material.startswith('bone'): rec[..., 6:8] *= .5
            (tower_baked if lit == 'baked' else live).append(rec)
        if form.solid: solids.append(tris)
    lamps += [(at(frame, p), w) for p, w in built['lamps']]
    smoke += [[round(v, 2) for v in at(frame, p[:3])]+[p[3]] for p in built['smoke']]
    under = UC.build(temple, land['tubes'][0]['floor'], TP.TOP, seed)
    take(under['parts'], tower_baked)
    lamps += under['lamps']
    for k, t in enumerate(tubes):
        roof, span = tube_roof(t, temple, UC.PASSAGE-.5)
        tris, normals = A.place(roof, IDENT)
        live.append(A.records(tris, normals, mats['moss'])); solids.append(tris)
        tris, normals = A.place(tube_lining(t, span, seed+31*k), IDENT)
        tower_baked.append(A.records(tris, normals, mats['bark'])); solids.append(tris)
        # Lanterns along it and lava seeping at the foot of its walls.
        q, tan = t['q'], t['tan']
        for i in range(span[0], span[1], 18):
            left = np.array([-tan[i, 1], tan[i, 0]])
            for side in (-1, 1):
                x, z = q[i]+left*side*(t['width']/2-.6)
                parts = TW.lantern((x, t['floor']+3.4, z), lamps, .9)
                take([(p, 'baked') for p in parts], tower_baked)
        for side in (-1, 1):
            left = np.c_[-tan[span[0]:span[1], 1], tan[span[0]:span[1], 0]]
            seep = ribbon(q[span[0]:span[1]]+left*side*(t['width']/2-.5), .3, lambda x, z: t['floor'], .015, lanes=3)
            tris, normals = A.place(seep, IDENT)
            glow.append(A.records(tris, normals, mats['water']))

    # Lava: the moat and the tower's falls, the valleys' rivers, the fissure
    # and the breach's stream; the pools' lava is drawn by the engine.
    hot.append(A.records(*A.place(TW.moat_lava(), frame), mats['water']))
    for form in built['falls']:
        hot.append(A.records(*A.place(form, frame), mats['water']))
    river_hot, river_crust = RG.rivers(valleys, drape)
    for form in river_hot:
        hot.append(A.records(*A.place(form, IDENT), mats['water']))
    for form in river_crust:
        tris, normals, f = A.place(form, IDENT, corners=True)
        cool.append(A.records(tris, normals, mats['water'], rim=form.rim[f]))
    q, tan, s = feat['fissure']
    fs = land['fissure']
    for form in (ribbon(q[8:-8], fs['width']*.26, ground_at, .12, lanes=5),
                 ribbon(LAND.polyline(land["flows"][1]["points"], 1.0)[0][4:-2], 2.4, ground_at, .1, lanes=3)):
        tris, normals, f = A.place(form, IDENT, corners=True)
        cool.append(A.records(tris, normals, mats['water'], rim=form.rim[f]))
    # The lava sea round the island: drawn only (nobody reaches it).
    sea = []
    y = land['rim']['sea']
    tile = 256*STEP
    for x0 in np.arange(-1800.0, 2400.0, 300.0):
        for z0 in np.arange(-1800.0, 2400.0, 300.0):
            if x0 >= 0 and x0+300 <= tile and z0 >= 0 and z0+300 <= tile: continue
            q = np.array([(x0, y, z0), (x0+300, y, z0), (x0+300, y, z0+300), (x0, y, z0+300)])
            sea.append(A.records(*A.place(A.Form(q, RG._up(q, [(0, 1, 2), (0, 2, 3)]), 'water', False, False), IDENT),
                                 mats['water']))
    pool_hot = []
    for x, z, half, s_ in pools:
        tris, normals = A.place(disc(x, z, half*1.2, s_), IDENT)
        pool_hot.append(A.records(tris, normals, mats['water']))

    # The field's rock, the valleys' rock and arches, the far land.
    keep = [(temple, 36.0), (mesa_at, 24.0), (tower['at'], TW.MOAT+12.0)]
    keep += [((x, z), 10.0) for x, z, _ in fixed0+team0+team1]+[(tuple(s), 9.0) for s in definition['stations']]
    keep += [((x, z), r) for x, z, r in beast_rings]

    def keep_out(x, z, r):
        if any(math.dist((x, z), p) < d+r for p, d in keep): return True
        i, k = int(min(max(x/STEP, 0), 255)), int(min(max(z/STEP, 0), 255))
        reach = int(r/STEP)+2
        near = (slice(max(k-reach, 0), k+reach+1), slice(max(i-reach, 0), i+reach+1))
        if lava[near].any() or marks['floor'][near].any(): return True
        if any(np.hypot(q_[:, 0]-x, q_[:, 1]-z).min() < r+t_['width']/2+4 for q_, t_ in ((t['q'], t) for t in tubes)): return True
        if any(math.dist((x, z), t['pit'][:2]) < t['pit'][2]+r+3 for t in tubes): return True
        return False

    inside = within(area, SOLID_MARGIN)
    for form, m, lit in SC.field(definition, ground_at, keep_out, slope, feat['fissure'], seed):
        tris, normals = A.place(form, m)
        if not len(tris): continue
        live.append(A.records(tris, normals, mats[form.material]))
        if form.solid and inside(*tris[:, :, [0, 2]].mean((0, 1))): solids.append(tris)
    # The beasts' bones and the field's rock arches.
    for form in BE.build(definition['beasts'], ground_at, seed)+BE.rock_arches(definition['rock_arches'], ground_at, seed):
        tris, normals = A.place(form, IDENT)
        if not len(tris): continue
        rec = A.records(tris, normals, mats[form.material])
        if form.material.startswith('bone'): rec[..., 6:8] *= .5
        live.append(rec)
        if form.solid: solids.append(tris)
    for form, m in RG.dressing(valleys, ground_at, seed):
        tris, normals = A.place(form, m)
        live.append(A.records(tris, normals, mats[form.material]))
        solids.append(tris)
    for form, m, lit in SC.far_land(definition, seed)+SC.near_land(definition, seed):
        tris, normals = A.place(form, m)
        live.append(A.records(tris, normals, mats[form.material]))
    # Lava breaking out in the floor's hollows: flows, draped on the ground.
    for form, m in SC.breakouts(definition, ground_at, keep_out, seed):
        tris, normals, f = A.place(form, m, corners=True)
        tris, normals = A.drape(tris, drape, .08, 6.0)
        cool.append(A.records(tris, normals, mats['water'], rim=form.rim[f]))
    smoke += SC.steam(definition, ground_at)

    # Kit geometry: stations, the fence and the ceiling.
    solid = np.concatenate([t.reshape(-1, 3, 3) for t in solids])
    mesh = kit.Mesh(); mesh.lamps = []
    entities = []
    stations = [((x, ground_at(x, z), z), heading_to(x, z, temple)) for x, z in definition['stations']]
    stations += me['stations']
    for tx, ty, tz, tf in built['stations']:
        stations.append((at(frame, (tx, ty+1.0, tz)), tower['facing']+tf))
    for k, (point, heading) in enumerate(stations):
        x, y, z = point
        floor = floor_below(x, z, y+1.0, solid, grid, hole_set)
        yaw = yaw_toward(open_heading(solid, x, floor, z, heading))
        use, facing = booth(mesh, x, round(floor, 3), z, yaw)
        for team in (0, 1):
            entities.append(dict(id=f'station-{k}-{team}', kind='inventory', position=[round(v, 3) for v in use],
                                 team=team, circuit='always-on', radius=3, weapon='bullet', facing=facing))
    flags = [[round(v, 3) for v in me['flag']], [round(v, 3) for v in tower_flag]]
    # Control points (the definition's `points`): the cathedral's post,
    # whose drain strikes from the temple's spires and whose hall burns its
    # holders (`cook`, health a second), the mesa's altar and the skull
    # tower's forecourt.
    where = {'post': under['point'], 'flag': tuple(flags[0]), 'tower': at(frame, built['court'])}
    control = []
    for d in definition['points']:
        d = dict(d)
        x, y, z = where[d.pop('at')]
        if d.pop('spires', False):
            d.update(spires=[[round(v, 2) for v in (tx, ty+.35, tz)] for tx, ty, tz in tp['spires']], beacon=under['beacon'])
        if 'cook' in d:
            lo, hi = under['hall']
            d['cook'] = {'min': [round(v, 3) for v in lo], 'max': [round(v, 3) for v in hi], 'damage': d['cook']}
        control.append({'pos': [round(x, 3), round(y, 3), round(z, 3)], **d})
    fence_sheets(mesh, area, float(grid.min())-6.0, ring['ceiling'])

    # Spawns.
    spawn_points = [[], []]
    for team, spots in ((0, fixed0+team0), (1, team1)):
        for x, z, heading in spots:
            floor = floor_below(x, z, ground_at(x, z)+3.0, solid, grid, hole_set)
            spawn_points[team].append([round(x, 3), round(floor+SPAWN_LIFT, 3), round(z, 3), round(yaw_toward(heading), 5)])
    for p in built['spawns']:
        x, y, z = at(frame, p)
        floor = floor_below(x, z, y+.5, solid, grid, hole_set)
        spawn_points[1].append([round(x, 3), round(floor+SPAWN_LIFT, 3), round(z, 3), round(yaw_toward(tower['facing']), 5)])

    # Render vertices: baked structures and kit geometry, then the live and
    # lava geometry after the bake (light layers -1 and -5).
    kit_vertices = np.frombuffer(mesh.vertices.tobytes(), '<f4').reshape(-1, 3, 12)
    structures = np.concatenate(baked+[kit_vertices]).astype('<f4')
    tower_all = np.concatenate(tower_baked).astype('<f4')
    live = np.concatenate(live)
    hot, cool, pool_hot = np.concatenate(hot), np.concatenate(cool), np.concatenate(pool_hot)
    glow = np.concatenate(glow)
    sea = np.concatenate(sea)
    lava_draw = np.concatenate([hot, sea, cool, glow]).copy()
    full = len(hot)+len(sea)
    lava_draw[:full, :, 11] = -5.0; lava_draw[full:full+len(cool), :, 11] = -5.5
    lava_draw[full+len(cool):, :, 11] = -5.25
    textures = bytearray(shared['textures']); count = old['texture_count']
    pack_writer.paint_materials(textures, count, list(M.MATERIALS), M.texture, seed, kit)
    textures, count = append_layers(textures, count, [M.texture(name, seed+count+k, kit.noise, kit.value_noise)
                                                      for k, name in enumerate(M.EXTRA)])
    structure_bytes, tower_bytes = structures.tobytes(), tower_all.tobytes()
    lightmap = None
    if bake_light:
        structure_bytes, textures, count, lightmap = bake(structure_bytes, textures, count, sun, definition['light_tint'],
                                                          lit_lamps)
        tower_bytes, textures, count, tower_light = bake(tower_bytes, textures, count, sun, tower['light_tint'], lamps)
        lightmap = {**lightmap, 'pages': lightmap['pages']+tower_light['pages'], 'lamps': lightmap['lamps']+tower_light['lamps'],
                    'tower': {k: tower_light[k] for k in ('pages', 'first_layer', 'tint')}}
    structure_bytes += tower_bytes
    live_bytes = live.astype('<f4').tobytes()
    vertices = structure_bytes+live_bytes+lava_draw.astype('<f4').tobytes()
    collision = np.concatenate([solid.reshape(-1, 9).astype('<f4'),
                                np.frombuffer(mesh.collision.tobytes(), '<f4').reshape(-1, 9)])

    heights = bytearray(struct.pack('<65536H', *[round(float(v)*32) for v in grid.ravel()]))
    ashen = np.zeros_like(grid, bool)
    for t in tubes:
        px, pz, pr = t['pit']
        ashen |= np.hypot(X-px, Z-pz) < pr+4
        for qx, qz in t['q'][::4]:
            ashen |= np.hypot(X-qx, Z-qz) < t['width']/2+5
    weights = terrain_weights(grid, masks, marks, ashen, lava, seed)
    files = {'height.bin': bytes(heights), 'vertices.bin': vertices, 'collision.bin': collision.tobytes(),
             'weights.rgba': weights.tobytes(), 'textures.rgba': bytes(textures), 'ambient.f32': shared['ambient']}
    files['shade.rg'], shade = ashveil_shade.bake(files['height.bin'], structure_bytes+live_bytes, sun,
                                                  kit.MATERIALS.index('light'), STEP)

    burn = 150.0
    water_volumes = [{'surface': s_, 'rect': [round(x-h, 2), round(z-h, 2), round(x+h, 2), round(z+h, 2)], 'lava': True,
                      'burn': burn, 'color': [1.0, 0.42, 0.1]} for x, z, h, s_ in pools]
    lg = definition['lava_ground']
    clear = [(p[0], p[2]) for team in spawn_points for p in team]+[(f[0], f[2]) for f in flags] \
        + [(e['position'][0], e['position'][2]) for e in entities]
    files['lava.bin'], hot_cells, cool_cells = lava_ground(grid, holes, collision, np.concatenate([hot, pool_hot]), cool,
                                                           burn, lg['flow_burn'], {'points': clear, 'radius': lg['keep_clear']})
    up = definition['updrafts']
    zones = [{'kind': 'updraft', 'at': [round(x, 2), round(s_+up['height']/2, 2), round(z, 2)],
              'size': [round(h-POOL_MARGIN, 2), up['height']/2, round(h-POOL_MARGIN, 2)], 'lift': up['lift']}
             for x, z, h, s_ in pools]

    manifest = {k: old[k] for k in ['terrain_layers', 'sky_layers', 'water_layer', 'water']}
    manifest['texture_count'] = count
    if lightmap: manifest['lightmap'] = lightmap
    manifest.update(version=1, id=definition['id'], name=definition['name'], terrain_step=STEP,
        flags=flags, spawns=[spawn_points[0][0][:3], spawn_points[1][0][:3]], exact_spawns=True,
        spawn_points=spawn_points, holes=holes, entities=entities, ambient_emitters=[definition['ambient']],
        sky={**definition['sky'], 'fogColor': definition['fog_color']},
        look={**definition['look'], 'sun_direction': [round(float(v), 5) for v in sun]},
        water_enabled=False, water_volumes=water_volumes, zones=zones, terrain_shade=shade, smoke=smoke[:64],
        control_points=control,
        land_source_sha256=pack_writer.source_hash(LAND.__file__),
        shapes_sha256=pack_writer.source_hash(A.__file__),
        temple_source_sha256=pack_writer.source_hash(TP.__file__),
        mesa_source_sha256=pack_writer.source_hash(ME.__file__),
        field_source_sha256=pack_writer.source_hash(SC.__file__),
        beasts_source_sha256=pack_writer.source_hash(BE.__file__),
        ring_source_sha256=pack_writer.source_hash(RG.__file__),
        tower_source_sha256=pack_writer.source_hash(TW.__file__),
        undercroft_source_sha256=pack_writer.source_hash(UC.__file__),
        material_source_sha256=pack_writer.source_hash(M.__file__),
        definition_sha256=pack_writer.source_hash(ROOT/'maps/ashveil.json'),
        provenance='Ashveil: PeakRunner original procedural work: the caldera, the temple, the cathedral, the mesa, '
                   'the skull tower, the valleys, every model and texture (docs/ashveil.md)')
    pack_writer.write_pack(output, files, manifest)
    (output/'info.json').write_text(json.dumps({'name': definition['name'], 'note': definition['note'],
                                                'bases': [flags[0], flags[1]], 'dust': [0.5, 0.3, 0.2]}, indent=2)+'\n')
    print(f'Built {definition["name"]}: {len(collision)} solid triangles, {len(vertices)//144} render triangles '
          f'({len(structures)+len(tower_all)} baked, {len(live)} live, {len(lava_draw)} lava), {len(holes)} terrain holes, '
          f'{len(water_volumes)} lava pools, {hot_cells} lava and {cool_cells} flow cells, {len(zones)} updrafts, '
          f'{len(entities)} station entries, {len(valleys)} valleys ({sum(v.length for v in valleys):.0f} m), '
          f'{len(lamps)+len(lit_lamps)} lamps, {len(control)} control points ({len(tp["spires"])} spires), '
          f'{len(smoke)} smoke sources'
          + (f', {lightmap["pages"]} lightmap pages' if lightmap else ', unbaked'))


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '--no-bake']
    build(Path(args[0]) if args else ROOT/'assets/maps/ashveil', bake_light='--no-bake' not in sys.argv)
