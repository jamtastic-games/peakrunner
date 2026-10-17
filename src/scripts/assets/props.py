"""Original procedural scenery props, scattered deterministically over a
map's terrain after its structures are built and baked.

Props are PeakRunner's own low-poly shapes built with the kit's `Mesh`
primitives. They never come from another game. Each map picks a theme (rocks,
tufts, ferns, logs, standing stones, ice, scrub, bones...) made from the
map's existing painted materials.

Rules (docs/map-pipeline.md):

* Big props (boulders, outcrops, standing stones, logs, large ice shards) are
  solid, simple and sunk into the ground so no edge hovers. They are placed in
  mirrored pairs through the midpoint of the two flags, keep off the ski lanes
  (flag to flag, flag to each control point), stay `BASE_CLEAR` from each flag
  so base approaches stay open, and never exceed
  `PROP_COLLISION_TRIS` per map.
* Cover (walls, log piles, rock clusters, ruins, crates, ice ridges, debris)
  is solid and deliberate: mirrored pieces beside the ski lanes and around
  control points that block movement and shots (see `place_cover`).
* Small props (ferns, heather, scrub, saplings, bones) and the grass layer
  (`ground_layer`) are render-only, and are stored as GPU instances of a few
  shared shapes per kind (`Instancer`, `props.bin`) rather than baked vertices.
* Nothing lands in protected space: under or near any existing collision
  geometry (bases, towers, bridges, pads, trees), terrain holes (bunker cuts,
  sewer and cavern mouths, trenches) and their neighbours, around flags, spawn
  points and control-point rings, below water, or off the map edge.
* Density thins with distance from the flags and control points.

Props are appended after the structure lightmap bake, keeping the kit's live
lighting path (vertex light layer -1). They do not add lightmap pages, and the
terrain shade map (terrain_shade.py) shades them where they stand.
"""
import math
import random
import struct
import zlib

import numpy as np

SOURCE_VERSION = 6
# Seeds the scatter. Kept at 5 when instancing arrived so every placement,
# cover piece and collision triangle stayed exactly where it was.
PLACEMENT_VERSION = 5
# Render-only props (small scenery and the grass layer) are not baked into
# vertices.bin. Each kind gets INSTANCE_VARIANTS shapes built once at size 1;
# every placement is an instance of one of them (props.bin).
INSTANCE_VARIANTS = 24
# Solid triangles all of a map's props (scenery plus cover) may add, outside
# the per-base budget.
PROP_COLLISION_TRIS = 4000
# Of which scenery (boulders, outcrops, logs, stones) may take at most this;
# the rest is kept for cover.
SCENERY_COLLISION_TRIS = 2500
# Render-only triangles for the ground layer (grass clumps and meadow patches).
GROUND_TRIS = 80000
# Cover: deliberate solid pieces beside the ski lanes and around control points.
COVER_OFFSETS = (36.0, 48.0, 62.0) # metres to the side of a lane's centre line
COVER_SPACING = 30.0               # between any two cover pieces
COVER_GAP = 4.0                    # walkable gap to any other solid prop
CROUCH = (1.25, 1.5)               # crouch-cover height range
FULL = (2.6, 3.6)                  # full-cover height range
LANE_HALF_WIDTH = 28.0
MARGIN_BIG = 8.0
MARGIN_SMALL = 4.0
FLAG_CLEAR = 25.0
# Big props also keep off each base's approaches, where players and bots path in.
BASE_CLEAR = 80.0
SPAWN_CLEAR = 12.0
RING_CLEAR = 8.0
EDGE = 48.0
OCC_CELL = 2.0

# kind: (material(s), count, size range, solid). Counts are targets; big
# counts are pairs (each placement is mirrored). `cover` entries are
# (kind, material(s), pairs, length range, height class) and are placed as
# deliberate mirrored cover spots; `ground` is the grass layer (material,
# clump size range).
THEMES = {
    'old-holler': dict(
        big=[('boulder', 'rock', 24, (1.3, 3.0)), ('log', 'bark', 8, (3.0, 6.0))],
        cover=[('log_pile', 'bark', 4, (4.0, 6.0), 'crouch'), ('rock_cluster', 'rock', 4, (1.6, 2.2), 'full'),
               ('stone_wall', ('rock', 'moss'), 3, (6.0, 9.0), 'crouch')],
        small=[('fern', 'leaf', 1300, (.8, 1.5))],
        ground=('moss', (.55, 1.15))),
    'tower-complex': dict(
        big=[('outcrop', 'rock', 22, (2.2, 4.6))],
        cover=[('rock_cluster', 'rock', 5, (1.8, 2.4), 'full'), ('debris', ('panel', 'trim'), 5, (3.5, 5.0), 'crouch')],
        small=[('scrub', 'leaf', 1000, (.8, 1.6))],
        ground=('moss', (.5, 1.05))),
    'cairnhold': dict(
        big=[('standing_stone', 'concrete', 12, (3.0, 5.5)), ('boulder', 'rock', 20, (1.2, 2.6))],
        cover=[('stone_wall', ('concrete', 'moss'), 5, (7.0, 11.0), 'crouch'),
               ('stone_wall', ('concrete', 'moss'), 3, (5.0, 7.0), 'full'), ('rock_cluster', 'rock', 6, (1.6, 2.2), 'full')],
        small=[('heather', 'moss', 1900, (.7, 1.4))],
        ground=('leaf', (.5, 1.05))),
    'frostline': dict(
        big=[('snow_rock', ('rock', 'meadow'), 22, (1.5, 3.4)), ('ice_shard', 'soil', 12, (2.6, 5.0))],
        cover=[('ice_ridge', ('soil', 'meadow'), 5, (6.0, 9.0), 'full'),
               ('rock_cluster', ('rock', 'meadow'), 4, (1.4, 2.0), 'crouch')],
        small=[('sapling', ('leaf', 'bark'), 260, (2.0, 4.0)), ('ice_shard', 'soil', 320, (.6, 1.6))],
        ground=('moss', (.45, .95))),
    'dustreach': dict(
        big=[('boulder', 'rock', 24, (1.4, 3.4))],
        cover=[('ruin_wall', 'rock', 5, (6.0, 10.0), 'full'), ('crates', 'bark', 4, (2.6, 4.0), 'crouch'),
               ('rock_cluster', 'rock', 2, (1.6, 2.2), 'full')],
        small=[('scrub', 'moss', 1000, (.7, 1.4)), ('bones', 'bark', 40, (1.2, 2.2))],
        ground=('soil', (.6, 1.25))),
}


class Terrain:
    """The pack's 256x256 heightfield (height.bin), sampled like the engine."""

    def __init__(self, height_bytes, weights_bytes, holes, step=8.0, water=None):
        self.h = np.frombuffer(height_bytes, '<u2').reshape(256, 256).astype(np.float64)/32
        self.w = np.frombuffer(weights_bytes, np.uint8).reshape(256, 256, 4).astype(np.float64)/255
        self.step = step; self.water = water
        cut = np.zeros((256, 256), bool)
        for i in holes: cut[i//256, i % 256] = True
        # Holes plus one cell around them: bunker cuts, sewer and cavern
        # mouths, trenches.
        near = cut.copy()
        for dz in (-1, 0, 1):
            for dx in (-1, 0, 1): near |= np.roll(np.roll(cut, dz, 0), dx, 1)
        self.cut = near

    def height(self, x, z):
        fx = min(max(x/self.step, 0), 255); fz = min(max(z/self.step, 0), 255)
        x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, 255), min(z0+1, 255); tx, tz = fx-x0, fz-z0
        a = self.h[z0, x0]+(self.h[z0, x1]-self.h[z0, x0])*tx
        b = self.h[z1, x0]+(self.h[z1, x1]-self.h[z1, x0])*tx
        return a+(b-a)*tz

    def slope(self, x, z):
        d = 2.0
        gx = (self.height(x+d, z)-self.height(x-d, z))/(2*d); gz = (self.height(x, z+d)-self.height(x, z-d))/(2*d)
        return math.degrees(math.atan(math.hypot(gx, gz)))

    def weight(self, x, z):
        ix = min(max(int(x/self.step), 0), 255); iz = min(max(int(z/self.step), 0), 255)
        return self.w[iz, ix]

    def under_water(self, x, z, y):
        """True when ground height `y` at (x, z) is under, or within a metre of,
        water. `water` is None, one legacy height for the whole map, or a list
        of manifest water volumes (see assets/water_bodies.py)."""
        if self.water is None: return False
        if isinstance(self.water, (int, float)): return y < self.water+1.0
        from assets import water_bodies
        return any(y < v['surface']+1.0 and water_bodies.contains(v, x, z) for v in self.water)

    def near_hole(self, x, z):
        ix = min(max(int(x/self.step), 0), 255); iz = min(max(int(z/self.step), 0), 255)
        return bool(self.cut[iz, ix])


class Protection:
    """Everywhere a prop may not stand."""

    def __init__(self, collision, flags, spawn_points, control_points, extra=()):
        tris = np.frombuffer(bytes(collision), '<f4').reshape(-1, 3, 3) if len(collision) else np.zeros((0, 3, 3), np.float32)
        n = int(2048/OCC_CELL)
        lo = tris.min(1); hi = tris.max(1)
        occ = np.zeros((n, n), bool)
        for (x0, _, z0), (x1, _, z1) in zip(lo, hi):
            i0 = max(int(x0//OCC_CELL), 0); i1 = min(int(x1//OCC_CELL), n-1)
            j0 = max(int(z0//OCC_CELL), 0); j1 = min(int(z1//OCC_CELL), n-1)
            if i0 <= i1 and j0 <= j1: occ[j0:j1+1, i0:i1+1] = True
        self.cells = occ
        self.circles = [(f[0], f[2], FLAG_CLEAR) for f in flags]
        self.circles += [(p[0], p[2], SPAWN_CLEAR) for team in spawn_points for p in team]
        self.circles += [(c['pos'][0], c['pos'][2], c.get('radius', 12)+RING_CLEAR) for c in control_points]
        self.circles += list(extra)
        self.interest = [(f[0], f[2]) for f in flags]+[(c['pos'][0], c['pos'][2]) for c in control_points]
        self.lanes = []
        for i, a in enumerate(flags):
            for b in flags[i+1:]: self.lanes.append(((a[0], a[2]), (b[0], b[2])))
            for c in control_points: self.lanes.append(((a[0], a[2]), (c['pos'][0], c['pos'][2])))

    def occupied(self, x, z, margin):
        r = int(math.ceil(margin/OCC_CELL)); n = self.cells.shape[0]
        i = int(x//OCC_CELL); j = int(z//OCC_CELL)
        return bool(self.cells[max(j-r, 0):min(j+r+1, n), max(i-r, 0):min(i+r+1, n)].any())

    def in_circle(self, x, z, pad=0.0):
        return any(math.hypot(x-cx, z-cz) < r+pad for cx, cz, r in self.circles)

    def lane_distance(self, x, z):
        best = math.inf
        for (ax, az), (bx, bz) in self.lanes:
            dx, dz = bx-ax, bz-az; t = ((x-ax)*dx+(z-az)*dz)/max(dx*dx+dz*dz, 1e-9)
            t = min(max(t, 0), 1); best = min(best, math.hypot(x-(ax+t*dx), z-(az+t*dz)))
        return best

    def flag_distance(self, x, z):
        return min(math.hypot(x-fx, z-fz) for fx, fz, _ in self.circles[:2])

    def interest_distance(self, x, z):
        return min((math.hypot(x-a, z-b) for a, b in self.interest), default=0.0)


def _rot(p, yaw, tilt=0.0, axis_yaw=0.0):
    """Tilt a local point about a horizontal axis, then turn it by yaw."""
    x, y, z = p
    if tilt:
        ca, sa = math.cos(axis_yaw), math.sin(axis_yaw)
        u = x*ca+z*sa; v = -x*sa+z*ca           # into the tilt axis frame
        c, s = math.cos(tilt), math.sin(tilt)
        u, y = u*c-y*s, u*s+y*c
        x = u*ca-v*sa; z = u*sa+v*ca
    c, s = math.cos(yaw), math.sin(yaw)
    return (x*c+z*s, y, -x*s+z*c)


def _hull(mesh, rng, r, height, mat, solid, lon=6, sink=0.0, squash=1.0):
    """Irregular low-poly rock: a lat/long hull with jittered radii."""
    rings = [(-.35, .95), (.25, 1.0), (.75, .72)]
    pts = []
    for ry, rr in rings:
        row = []
        for k in range(lon):
            a = (k+rng.uniform(-.18, .18))*math.tau/lon
            j = rng.uniform(.78, 1.18)
            row.append((r*rr*j*math.cos(a)*squash, height*ry-sink, r*rr*j*math.sin(a)))
        pts.append(row)
    top = (rng.uniform(-.15, .15)*r, height*rng.uniform(.95, 1.1)-sink, rng.uniform(-.15, .15)*r)
    bottom = (0, -height*.6-sink, 0)
    tris = []
    for k in range(lon):
        n = (k+1) % lon
        tris.append((bottom, pts[0][n], pts[0][k]))
        for i in range(len(rings)-1):
            a, b, c, d = pts[i][k], pts[i][n], pts[i+1][n], pts[i+1][k]
            tris += [(a, b, c), (a, c, d)]
        tris.append((pts[-1][k], pts[-1][n], top))
    for t in tris: mesh.triangle(list(t), mat, solid)
    return len(tris)


def boulder(mesh, rng, size, mat, solid=True):
    return _hull(mesh, rng, size, size*rng.uniform(.55, .8), mat, solid, sink=size*.18)


def outcrop(mesh, rng, size, mat, solid=True):
    n = 0
    for k in range(rng.choice([2, 3])):
        s = size*rng.uniform(.55, 1.0)
        ox, oz = rng.uniform(-.8, .8)*size, rng.uniform(-.8, .8)*size
        saved = mesh.origin
        mesh.origin = (saved[0]+ox, saved[1], saved[2]+oz)
        n += _hull(mesh, rng, s, s*rng.uniform(.8, 1.3), mat, solid, sink=s*.25)
        mesh.origin = saved
    return n


def snow_rock(mesh, rng, size, mats, solid=True):
    rock, snow = mats
    h = size*rng.uniform(.55, .8)
    n = _hull(mesh, rng, size, h, rock, solid, sink=size*.18)
    n += _hull(mesh, rng, size*.62, h*.28, snow, False, lon=6, sink=-(h*.78-size*.18))
    return n


def standing_stone(mesh, rng, size, mat, solid=True):
    r = size*rng.uniform(.16, .22); t = r*rng.uniform(.55, .8)
    lean = rng.uniform(0, math.radians(9)); lean_dir = rng.uniform(0, math.tau)
    sides = 5; base = []; head = []
    for k in range(sides):
        a = (k+rng.uniform(-.12, .12))*math.tau/sides
        base.append(_rot((r*math.cos(a), -1.2, r*math.sin(a)*.7), 0, lean, lean_dir))
        head.append(_rot((t*math.cos(a), size, t*math.sin(a)*.7), 0, lean, lean_dir))
    cap = _rot((0, size+t*.6, 0), 0, lean, lean_dir)
    for k in range(sides):
        n = (k+1) % sides
        mesh.quad(base[k], head[k], head[n], base[n], mat, solid)
        mesh.triangle([head[k], cap, head[n]], mat, solid)
    return sides*3


def log(mesh, rng, size, mat, solid=True):
    r = rng.uniform(.3, .45)
    mesh.box((0, r*.55, 0), (size, r*1.7, r*1.7), mat, solid)
    return 12


def ice_shard(mesh, rng, size, mat, solid=True):
    r = size*rng.uniform(.14, .22); lean = rng.uniform(math.radians(4), math.radians(22))
    d = rng.uniform(0, math.tau); sides = 4
    base = [_rot((r*math.cos(k*math.tau/sides), -.6, r*math.sin(k*math.tau/sides)), 0, lean, d) for k in range(sides)]
    tip = _rot((0, size, 0), 0, lean, d)
    for k in range(sides):
        mesh.triangle([base[k], tip, base[(k+1) % sides]], mat, solid)
    return sides


def tuft(mesh, rng, size, mat, solid=False):
    blades = rng.randint(3, 5)
    for _ in range(blades):
        a = rng.uniform(0, math.tau); off = rng.uniform(0, size*.25)
        bx, bz = off*math.cos(a), off*math.sin(a)
        lean = rng.uniform(.1, .5); h = size*rng.uniform(.7, 1.1); w = size*.07
        tx, tz = bx+lean*h*math.cos(a), bz+lean*h*math.sin(a)
        px, pz = -math.sin(a)*w, math.cos(a)*w
        mesh.triangle([(bx-px, -.05, bz-pz), (bx+px, -.05, bz+pz), (tx, h, tz)], mat, False)
    return blades


def fern(mesh, rng, size, mat, solid=False):
    fronds = 6
    for k in range(fronds):
        a = (k+rng.uniform(-.2, .2))*math.tau/fronds
        ca, sa = math.cos(a), math.sin(a); w = size*.12
        mid = (ca*size*.5, size*.45, sa*size*.5); tip = (ca*size, size*.15, sa*size)
        px, pz = -sa*w, ca*w
        base = (0, -.05, 0)
        left = (mid[0]-px, mid[1], mid[2]-pz); right = (mid[0]+px, mid[1], mid[2]+pz)
        mesh.triangle([base, right, left], mat, False)
        mesh.triangle([left, right, tip], mat, False)
    return fronds*2


def heather(mesh, rng, size, mat, solid=False):
    return _hull(mesh, rng, size*.5, size*.35, mat, False, lon=4, sink=size*.1, squash=1.2)


def scrub(mesh, rng, size, mat, solid=False):
    n = _hull(mesh, rng, size*.45, size*.6, mat, False, lon=4, sink=-size*.25)
    for _ in range(3):
        a = rng.uniform(0, math.tau); w = size*.05
        tip = (math.cos(a)*size*.35, size*.5, math.sin(a)*size*.35)
        mesh.triangle([(-w, -.05, 0), (w, -.05, 0), tip], mat, False)
    return n+3


def sapling(mesh, rng, size, mats, solid=False):
    leaf, bark = mats
    mesh.column((0, -.2, 0), size*.05, size*.35, bark, 4, top=size*.03, solid=False)
    mesh.column((0, size*.2, 0), size*.3, size*.5, leaf, 6, top=.02, solid=False)
    mesh.column((0, size*.52, 0), size*.2, size*.48, leaf, 6, top=.02, solid=False)
    return 4*3+6*3*2


def bones(mesh, rng, size, mat, solid=False):
    n = 0; ribs = 5; spine = size
    mesh.box((0, .08, 0), (spine, .12, .14), mat, False); n += 12
    for k in range(ribs):
        x = (k/(ribs-1)-.5)*spine*.8; r = size*.28*(1-.35*abs(k/(ribs-1)-.5)*2)
        for side in (-1, 1):
            prev = (x, .1, 0)
            for s in range(1, 4):
                a = s/3*math.pi*.5
                p = (x, .1+math.sin(a)*r*.9, side*(1-math.cos(a)+.3)*r)
                mesh.triangle([prev, (x+.05, prev[1], prev[2]), p], mat, False); n += 1
                prev = p
    return n


def clump(mesh, rng, size, mat, solid=False):
    """A dense grass clump: 8-12 blades from one root, read as a patch at eye level."""
    blades = rng.randint(8, 12)
    for _ in range(blades):
        a = rng.uniform(0, math.tau); off = rng.uniform(0, size*.45)
        bx, bz = off*math.cos(a), off*math.sin(a)
        lean = rng.uniform(.15, .55); h = size*rng.uniform(.6, 1.1); w = size*.09
        tx, tz = bx+lean*h*math.cos(a), bz+lean*h*math.sin(a)
        px, pz = -math.sin(a)*w, math.cos(a)*w
        mesh.triangle([(bx-px, -.05, bz-pz), (bx+px, -.05, bz+pz), (tx, h, tz)], mat, False)
    return blades


BUILDERS = dict(boulder=boulder, outcrop=outcrop, snow_rock=snow_rock, standing_stone=standing_stone,
                log=log, ice_shard=ice_shard, tuft=tuft, fern=fern, heather=heather, scrub=scrub,
                sapling=sapling, bones=bones, clump=clump)


# ---- Cover ------------------------------------------------------------------
# Cover pieces are solid, low-poly and sit on the terrain: every box reaches
# COVER_SINK below the lowest ground under it, so no edge hovers and nothing
# leaves a gap a player could slip under. Pieces are built in a local frame
# whose long axis is x; `ground(lx, lz)` gives the terrain height there relative
# to the piece's origin. Each builder returns its half extents (hx, hz).
COVER_SINK = .5
SEGMENT = 2.5


def _ground_span(ground, x0, x1, hz):
    ys = [ground(x, z) for x in (x0, (x0+x1)/2, x1) for z in (-hz, 0.0, hz)]
    return min(ys), max(ys)


def _wall_run(mesh, ground, length, height, thick, mat, heights=None):
    """A wall along local x in ~2.5 m segments, each from below its lowest
    ground to `height` above its highest ground."""
    n = max(1, round(length/SEGMENT)); seg = length/n
    tops = []
    for i in range(n):
        x0 = -length/2+i*seg; x1 = x0+seg
        lo, hi = _ground_span(ground, x0, x1, thick/2)
        h = height if heights is None else heights[i]
        top = hi+h; bottom = lo-COVER_SINK
        # Segments overlap by 5 cm so the joints never open.
        mesh.box(((x0+x1)/2, (top+bottom)/2, 0), (seg+.05, top-bottom, thick), mat, True)
        tops.append((x0, x1, top))
    return tops


def stone_wall(mesh, rng, length, height, ground, mats):
    stone, cap = mats if isinstance(mats, tuple) else (mats, mats)
    thick = .9
    tops = _wall_run(mesh, ground, length, height, thick, stone)
    # Mossy capstones, render-only, sunk into the wall top so no face is coplanar.
    for x0, x1, top in tops:
        x = x0+.3
        while x < x1-.3:
            w = rng.uniform(.45, .8)
            # Raised 4 cm off the joint and staggered, so no capstone face is
            # coplanar with its neighbour's or the wall's.
            mesh.box((min(x+w/2, x1-.25), top+.04+rng.uniform(0, .03), rng.uniform(-.05, .05)), (w, .22, thick+.1+rng.uniform(0, .04)), cap, False)
            x += w+rng.uniform(.05, .2)
    return length/2, thick/2


def ruin_wall(mesh, rng, length, height, ground, mats):
    thick = 1.1
    n = max(1, round(length/SEGMENT))
    # Broken sandstone: every segment still gives full cover at chest height.
    heights = [height*rng.uniform(.62, 1.0) for _ in range(n)]
    heights[rng.randrange(n)] = height
    _wall_run(mesh, ground, length, height, thick, mats, heights)
    for _ in range(4):
        lx = rng.uniform(-length/2, length/2); lz = rng.choice((-1, 1))*rng.uniform(.9, 1.6)
        s = rng.uniform(.3, .6)
        mesh.box((lx, ground(lx, lz)+s*.3, lz), (s, s*.8, s), mats, False)
    return length/2, thick/2


def log_pile(mesh, rng, length, height, ground, mats):
    r = height/3.3
    lo, hi = _ground_span(ground, -length/2, length/2, 2*r)
    base = hi-.1
    for z in (-r*.95, r*.95):
        top = base+2*r-.15
        mesh.box((0, (top+lo-COVER_SINK)/2, z), (length, top-(lo-COVER_SINK), 2*r), mats, True)
    top_log = base+2*r-.35
    mesh.box((rng.uniform(-.3, .3), top_log+r, 0), (length*.85, 2*r, 2*r), mats, True)
    return length/2, 2*r


def crates(mesh, rng, length, height, ground, mats):
    s = min(1.35, length/2)
    count = max(2, round(length/s))
    # The crates stand flush along the run: no gap a crouching shot can pass.
    w = length/count
    lo, hi = _ground_span(ground, -length/2, length/2, s/2)
    for i in range(count):
        x = -length/2+w/2+i*w
        top = hi+s
        mesh.box((x, (top+lo-COVER_SINK)/2, rng.uniform(-.1, .1)), (w, top-(lo-COVER_SINK), s), mats, True)
    if height > s*1.5:
        mesh.box((-length/2+s/2+.1, hi+s*1.5-.02, 0), (s*.95, s, s*.95), mats, True)
    return length/2, s/2+.1


def debris(mesh, rng, length, height, ground, mats):
    plate, trim = mats
    thick = 1.0
    _wall_run(mesh, ground, length, height, thick, plate)
    # A second, lower plate leaning against the first, and render-only ribs.
    lo, hi = _ground_span(ground, -length/4, length/4, 1.8)
    mesh.box((length*.15, (hi+height*.55+lo-COVER_SINK)/2, .95), (length*.5, hi+height*.55-(lo-COVER_SINK), .6), plate, True)
    for i in range(3):
        x = -length/2+(i+.5)*length/3
        mesh.box((x, ground(x, 0)+height*.5, 0), (.18, height+.1, thick+.12), trim, False)
    return length/2, 1.3


def ice_ridge(mesh, rng, length, height, ground, mats):
    ice, snow = mats
    thick = 1.4
    n = max(1, round(length/SEGMENT))
    heights = [height*rng.uniform(.82, 1.0) for _ in range(n)]
    tops = _wall_run(mesh, ground, length, height, thick, ice, heights)
    for x0, x1, top in tops:
        mesh.box(((x0+x1)/2, top-.03, 0), (x1-x0+.02, .16, thick+.08), snow, False)
        ice_shard(_Offset(mesh, ((x0+x1)/2, top-.3, rng.uniform(-.3, .3))), rng, rng.uniform(.8, 1.5), ice, False)
    return length/2, thick/2


def rock_cluster(mesh, rng, size, height, ground, mats):
    rock, snow = mats if isinstance(mats, tuple) else (mats, None)
    lo, hi = _ground_span(ground, -size*1.6, size*1.6, size)
    lift = hi-lo
    parts = [(0.0, 0.0, size, height+lift), (size*.75, rng.uniform(-.25, .25)*size, size*.8, (height+lift)*.8),
             (-size*.7, rng.uniform(-.25, .25)*size, size*.75, (height+lift)*.72)]
    for ox, oz, r, h in parts:
        # Offsets stay under the radii, so the rocks overlap and no wedge gap
        # opens. Each rock drops to the lowest ground under its own footprint
        # (its hull flares to about 1.2 r), so no edge floats on a slope.
        low = min(ground(ox+1.25*r*math.cos(a), oz+1.25*r*math.sin(a)) for a in (k*math.tau/8 for k in range(8)))
        low = min(low, ground(ox, oz))
        # The hull's widest ring sits at a quarter of its height; put it just
        # under the lowest ground so no bulge overhangs a downhill side, and
        # stretch the rock so its top still reaches h.
        H = (h-low+.1)/.75
        base = low-.25*H-.1
        _hull(_Offset(mesh, (ox, base, oz)), rng, r, H, rock, True)
        if snow: _hull(_Offset(mesh, (ox, base+H*.62, oz)), rng, r*.6, H*.2, snow, False, lon=6, sink=0)
    return size*1.7, size*1.1


class Instancer:
    """Render-only props as GPU instances: a few shared local meshes per kind
    and one small record per placement, written to `props.bin`.

    Layout (little endian), read by crates/core/src/map_pack.rs `PropSet`:
      header   b'PRP1', u32 mesh count M, u32 instance count N, u32 0
      M x      u32 first vertex, vertex count, first instance, instance count
      V x 8 f32  local position, local normal, material layer, 0
      N x 8 f32  world x, y, z, yaw (radians), uniform scale, mesh index, 0, 0
    Instances are sorted by mesh so each mesh is one instanced draw. A variant
    is built at size 1 at the origin; the renderer applies the kit's yaw
    (Mesh.point) and scale, and recomputes the kit's planar world UVs."""
    MAGIC = b'PRP1'

    def __init__(self, kit):
        self.kit = kit; self.keys = {}; self.meshes = []; self.records = []; self.triangles = 0

    def add(self, kind, mats, x, y, z, size, yaw, shape):
        variant = shape % INSTANCE_VARIANTS
        key = (kind, mats if isinstance(mats, str) else tuple(mats), variant)
        index = self.keys.get(key)
        if index is None:
            mesh = self.kit.Mesh()
            BUILDERS[kind](mesh, random.Random(zlib.crc32(repr(key).encode())), 1.0, mats, False)
            index = self.keys[key] = len(self.meshes); self.meshes.append(mesh)
        self.records.append((index, x, y, z, yaw, size))
        tris = len(self.meshes[index].vertices)//36
        self.triangles += tris
        return tris

    def to_bytes(self):
        if not self.records: return b''
        order = sorted(range(len(self.records)), key=lambda i: self.records[i][0])
        table, verts, first_vertex = [], [], 0
        counts = [0]*len(self.meshes)
        for index, *_ in self.records: counts[index] += 1
        first_instance = 0
        for index, mesh in enumerate(self.meshes):
            v = np.frombuffer(mesh.vertices.tobytes(), '<f4').reshape(-1, 12)
            out = np.zeros((len(v), 8), '<f4')
            out[:, 0:6] = v[:, 0:6]; out[:, 6] = v[:, 10]
            verts.append(out)
            table.append((first_vertex, len(v), first_instance, counts[index]))
            first_vertex += len(v); first_instance += counts[index]
        inst = np.zeros((len(self.records), 8), '<f4')
        for row, i in enumerate(order):
            index, x, y, z, yaw, size = self.records[i]
            inst[row] = (x, y, z, yaw, size, index, 0, 0)
        head = struct.pack('<4sIII', self.MAGIC, len(self.meshes), len(self.records), 0)
        head += b''.join(struct.pack('<4I', *t) for t in table)
        return head+np.concatenate(verts).tobytes()+inst.tobytes()

    def summary(self):
        return dict(meshes=len(self.meshes), instances=len(self.records), triangles=self.triangles)


class _Offset:
    """Mesh proxy that shifts local points by a fixed offset."""
    def __init__(self, mesh, offset): self.mesh, self.o = mesh, offset
    def _p(self, p): return (p[0]+self.o[0], p[1]+self.o[1], p[2]+self.o[2])
    def triangle(self, pts, mat, solid=True): self.mesh.triangle([self._p(p) for p in pts], mat, solid)
    def quad(self, a, b, c, d, mat, solid=True): self.mesh.quad(*(self._p(p) for p in (a, b, c, d)), mat, solid)


COVER_BUILDERS = dict(stone_wall=stone_wall, ruin_wall=ruin_wall, log_pile=log_pile, crates=crates,
                      debris=debris, ice_ridge=ice_ridge, rock_cluster=rock_cluster)
# Steepest ground each cover kind may stand on.
COVER_SLOPE = dict(stone_wall=22, ruin_wall=20, log_pile=16, crates=14, debris=18, ice_ridge=22, rock_cluster=32)
# Rough footprint radius factor (x size) used for sinking and spacing.
FOOT = dict(boulder=1.1, outcrop=1.9, snow_rock=1.1, standing_stone=.35, log=.55, ice_shard=.35,
            tuft=.4, fern=1.0, heather=.7, scrub=.6, sapling=.4, bones=.8)


def _ground(terrain, x, z, radius):
    """Lowest terrain under a footprint, so no edge hovers."""
    samples = [terrain.height(x, z)]
    for k in range(8):
        a = k*math.tau/8
        samples.append(terrain.height(x+radius*math.cos(a), z+radius*math.sin(a)))
    return min(samples)


def _slope_ok(kind, slope):
    if kind in ('standing_stone', 'log'): return slope < 18
    if kind in ('tuft', 'fern', 'heather', 'sapling', 'bones'): return slope < 30
    if kind == 'scrub': return slope < 34
    return slope < 42


def _terrain_ok(kind, terrain, x, z):
    w = terrain.weight(x, z)   # [layer0 (grass/snow/sand), rock, soil, moss]
    if kind in ('tuft', 'fern', 'heather', 'bones'): return w[1] < .55
    if kind in ('boulder', 'outcrop', 'snow_rock'): return True
    return w[1] < .8


def scatter(kit, theme, seed, terrain, protect):
    """Build the theme's props into a fresh kit Mesh. Returns
    (render vertex bytes, collision bytes, caster vertex bytes, summary,
    props.bin bytes for the render-only instances)."""
    spec = THEMES[theme]
    mesh = kit.Mesh(); casters = kit.Mesh(); instancer = Instancer(kit)
    # Big props mirror through the midpoint of the two flags (the first two
    # interest points), which is each map's symmetry centre.
    (ax, az), (bx, bz) = protect.interest[:2]
    cx, cz = (ax+bx)/2, (az+bz)/2
    placed, big = {}, []
    rng = random.Random(seed*7919+PLACEMENT_VERSION)

    def valid(kind, x, z, size, is_big):
        if not (EDGE <= x <= 2048-EDGE and EDGE <= z <= 2048-EDGE): return None
        foot = FOOT[kind]*size
        if terrain.near_hole(x, z): return None
        if protect.occupied(x, z, (MARGIN_BIG if is_big else MARGIN_SMALL)+foot): return None
        if protect.in_circle(x, z, foot): return None
        if is_big and protect.lane_distance(x, z) < LANE_HALF_WIDTH+foot: return None
        if is_big and protect.flag_distance(x, z) < BASE_CLEAR+foot: return None
        if not _slope_ok(kind, terrain.slope(x, z)) or not _terrain_ok(kind, terrain, x, z): return None
        y = _ground(terrain, x, z, foot)
        if terrain.under_water(x, z, y): return None
        return y

    def build(target, kind, mats, x, y, z, size, yaw, solid, shape):
        # Each prop's shape comes from its own seed, drawn from the scatter's
        # generator, so builds are deterministic and mirrored pairs match.
        # Render-only props become instances; returns their triangle count.
        if not solid and target is mesh:
            return instancer.add(kind, mats, x, y, z, size, yaw, shape)
        target.origin = (x, y, z); target.yaw = yaw
        BUILDERS[kind](target, random.Random(shape), size, mats, solid)
        return 0

    for kind, mats, pairs, (smin, smax) in spec['big']:
        count = 0
        for _ in range(pairs*60):
            if count >= pairs: break
            x, z = rng.uniform(EDGE, 2048-EDGE), rng.uniform(EDGE, 2048-EDGE)
            size = rng.uniform(smin, smax); yaw = rng.uniform(0, math.tau); shape = rng.getrandbits(32)
            if rng.random() > .3+.7*math.exp(-protect.interest_distance(x, z)/260): continue
            mx, mz = 2*cx-x, 2*cz-z
            if math.hypot(x-mx, z-mz) < 40: continue
            ya = valid(kind, x, z, size, True); yb = valid(kind, mx, mz, size, True)
            if ya is None or yb is None: continue
            if any(math.hypot(x-px, z-pz) < 10 or math.hypot(mx-px, mz-pz) < 10 for px, pz, *_ in big): continue
            pair = kit.Mesh()
            build(pair, kind, mats, x, ya, z, size, yaw, True, shape)
            build(pair, kind, mats, mx, yb, mz, size, yaw+math.pi, True, shape)
            # The budget is a hard limit: a pair that would exceed it is dropped.
            if (len(mesh.collision)+len(pair.collision))//9 > SCENERY_COLLISION_TRIS: break
            mesh.vertices.extend(pair.vertices); mesh.collision.extend(pair.collision)
            casters.vertices.extend(pair.vertices)
            big += [(x, z, kind, size, ya), (mx, mz, kind, size, yb)]
            count += 1
        placed[kind] = placed.get(kind, 0)+count*2

    cover = place_cover(kit, spec.get('cover', ()), rng, terrain, protect, (cx, cz), big, mesh, casters, placed)
    # Small props and grass keep off the footprint of every solid prop.
    blocked = [(x, z, FOOT[kind]*size+.5) for x, z, kind, size, _ in big]
    blocked += [(c['x'], c['z'], max(c['hx'], c['hz'])+.8) for c in cover]

    def clear_of_solids(x, z):
        return all(math.hypot(x-bx, z-bz) >= r for bx, bz, r in blocked)

    for kind, mats, count, (smin, smax) in spec['small']:
        # Small props grow in patches, so a player at ground level sees
        # vegetation around them rather than one tuft per hectare.
        n = 0
        for _ in range(count*6):
            if n >= count: break
            px, pz = rng.uniform(EDGE, 2048-EDGE), rng.uniform(EDGE, 2048-EDGE)
            if rng.random() > .2+.8*math.exp(-protect.interest_distance(px, pz)/400): continue
            if valid(kind, px, pz, smax, False) is None: continue
            radius = rng.uniform(3.5, 8.0)
            for _ in range(rng.randint(5, 12)):
                if n >= count: break
                a = rng.uniform(0, math.tau); r = radius*math.sqrt(rng.random())
                x, z = px+r*math.cos(a), pz+r*math.sin(a)
                size = rng.uniform(smin, smax); yaw = rng.uniform(0, math.tau); shape = rng.getrandbits(32)
                y = valid(kind, x, z, size, False)
                if y is None or not clear_of_solids(x, z): continue
                build(mesh, kind, mats, x, y, z, size, yaw, False, shape)
                n += 1
        placed[kind] = placed.get(kind, 0)+n

    ground_tris = 0
    if 'ground' in spec:
        mat, (smin, smax) = spec['ground']
        clumps, patches, ground_tris = ground_layer(mesh, rng, mat, smin, smax, valid, clear_of_solids, protect, build)
        placed['clump'] = clumps; placed['meadow_patch'] = patches
    summary = dict(theme=theme, version=SOURCE_VERSION, counts=placed,
                   render_triangles=len(mesh.vertices)//36+instancer.triangles,
                   baked_triangles=len(mesh.vertices)//36, instanced=instancer.summary(),
                   solid_triangles=len(mesh.collision)//9,
                   ground_triangles=ground_tris,
                   big=[[round(x, 2), math.floor(y*100)/100, round(z, 2), kind, round(size, 2)] for x, z, kind, size, y in big],
                   cover=[[round(c['x'], 2), round(c['y'], 2), round(c['z'], 2), c['kind'], round(c['length'], 2),
                           round(c['height'], 2), round(c['yaw'], 4), round(c['hx'], 2), round(c['hz'], 2)] for c in cover])
    return mesh.vertices.tobytes(), mesh.collision.tobytes(), casters.vertices.tobytes(), summary, instancer.to_bytes()


def _cover_candidates(protect, rng):
    """Spots beside each ski lane (both sides, several offsets) and in a ring
    around each control point, where a player wants something to duck behind."""
    out = []
    for (ax, az), (bx, bz) in protect.lanes:
        dx, dz = bx-ax, bz-az; length = math.hypot(dx, dz)
        if length < 1: continue
        ux, uz = dx/length, dz/length; px, pz = -uz, ux
        k = max(4, int(length/45))
        for i in range(k):
            t = .12+.76*(i+.5)/k
            for side in (-1, 1):
                for off in COVER_OFFSETS:
                    j = rng.uniform(-8, 8)
                    x = ax+dx*t+px*side*off+ux*j; z = az+dz*t+pz*side*off+uz*j
                    # Walls face down the lane, towards the fire coming along it.
                    out.append((x, z, math.atan2(px, pz)))
    for cx, cz in protect.interest[2:]:
        for k in range(8):
            a = k*math.tau/8+rng.uniform(-.2, .2); r = rng.uniform(34, 46)
            out.append((cx+r*math.cos(a), cz+r*math.sin(a), a+math.pi/2))
    rng.shuffle(out)
    return out


def place_cover(kit, entries, rng, terrain, protect, centre, big, mesh, casters, placed):
    """Mirrored cover pieces at candidate spots. Returns the placed pieces."""
    cx, cz = centre
    cover = []
    if not entries: return cover
    candidates = _cover_candidates(protect, rng)
    solids = [(x, z, FOOT[kind]*size) for x, z, kind, size, _ in big]

    def ground_fn(x0, y0, z0, yaw):
        c, s = math.cos(yaw), math.sin(yaw)
        return lambda lx, lz: terrain.height(x0+lx*c+lz*s, z0-lx*s+lz*c)-y0

    def fits(x, z, yaw, reach, kind):
        if not (EDGE+reach <= x <= 2048-EDGE-reach and EDGE+reach <= z <= 2048-EDGE-reach): return False
        c, s = math.cos(yaw), math.sin(yaw)
        for lx in (-reach, 0.0, reach):
            px, pz = x+lx*c, z-lx*s
            if terrain.near_hole(px, pz) or protect.occupied(px, pz, MARGIN_BIG): return False
            if protect.in_circle(px, pz, 2.0): return False
            if protect.lane_distance(px, pz) < LANE_HALF_WIDTH+2: return False
            if protect.flag_distance(px, pz) < BASE_CLEAR: return False
            if terrain.slope(px, pz) > COVER_SLOPE[kind]: return False
            if terrain.under_water(px, pz, terrain.height(px, pz)): return False
        if any(math.hypot(x-sx, z-sz) < sr+reach+COVER_GAP for sx, sz, sr in solids): return False
        if any(math.hypot(x-p['x'], z-p['z']) < COVER_SPACING for p in cover): return False
        return True

    queue = []
    for kind, mats, pairs, (lmin, lmax), cls in entries:
        queue += [(kind, mats, (lmin, lmax), cls)]*pairs
    order = list(range(len(queue)))
    rng.shuffle(order)
    counts = {}
    used = set()
    for qi in order:
        kind, mats, (lmin, lmax), cls = queue[qi]
        length = rng.uniform(lmin, lmax)
        height = rng.uniform(*(CROUCH if cls == 'crouch' else FULL))
        shape = rng.getrandbits(32)
        reach = length/2+1.5 if kind != 'rock_cluster' else length*1.8
        for ci, (x, z, yaw) in enumerate(candidates):
            if ci in used: continue
            mx, mz, myaw = 2*cx-x, 2*cz-z, yaw+math.pi
            if math.hypot(x-mx, z-mz) < 60: continue
            if not fits(x, z, yaw, reach, kind): continue
            if not fits(mx, mz, myaw, reach, kind): continue
            pair = kit.Mesh(); pieces = []
            for px, pz, pyaw in ((x, z, yaw), (mx, mz, myaw)):
                y = min(terrain.height(px, pz), _ground(terrain, px, pz, reach*.6))
                pair.origin = (px, y, pz); pair.yaw = pyaw
                hx, hz = COVER_BUILDERS[kind](pair, random.Random(shape), length, height, ground_fn(px, y, pz, pyaw), mats)
                pieces.append(dict(x=px, y=y, z=pz, kind=kind, length=length, height=height, yaw=pyaw, hx=hx, hz=hz))
            if (len(mesh.collision)+len(pair.collision))//9 > PROP_COLLISION_TRIS: return cover
            mesh.vertices.extend(pair.vertices); mesh.collision.extend(pair.collision)
            casters.vertices.extend(pair.vertices)
            cover += pieces
            used.add(ci)
            counts[kind] = counts.get(kind, 0)+2
            break
    for k, v in counts.items(): placed['cover_'+k] = v
    return cover


def ground_layer(mesh, rng, mat, smin, smax, valid, clear_of_solids, protect, build):
    """Grass where players fight: dense meadow patches along the ski lanes and
    around flags and control points, and lighter clumps everywhere else, up
    to GROUND_TRIS triangles. Returns (clumps, patches, triangles)."""
    budget = GROUND_TRIS
    clumps = patches = tris = 0

    def weight(x, z):
        near = max(math.exp(-protect.lane_distance(x, z)/80), math.exp(-protect.interest_distance(x, z)/300))
        return .12+.88*near

    def plant(x, z):
        nonlocal clumps, tris
        size = rng.uniform(smin, smax); yaw = rng.uniform(0, math.tau); shape = rng.getrandbits(32)
        y = valid('tuft', x, z, size, False)
        if y is None or not clear_of_solids(x, z): return
        tris += build(mesh, 'clump', mat, x, y, z, size, yaw, False, shape)
        clumps += 1

    # Meadow patches take roughly two thirds of the budget.
    for _ in range(40000):
        if tris >= budget*.66: break
        x, z = rng.uniform(EDGE, 2048-EDGE), rng.uniform(EDGE, 2048-EDGE)
        if rng.random() > weight(x, z)**1.5: continue
        if valid('tuft', x, z, smax, False) is None: continue
        radius = rng.uniform(4.0, 9.0); patches += 1
        # About one clump per 2.5 square metres inside a patch.
        for _ in range(int(math.pi*radius*radius/2.5)):
            a = rng.uniform(0, math.tau); r = radius*math.sqrt(rng.random())
            plant(x+r*math.cos(a), z+r*math.sin(a))
    for _ in range(200000):
        if tris >= budget: break
        x, z = rng.uniform(EDGE, 2048-EDGE), rng.uniform(EDGE, 2048-EDGE)
        if rng.random() > weight(x, z): continue
        plant(x, z)
    return clumps, patches, tris
    return clumps, patches
