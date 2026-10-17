"""Walk-graph route checks on a built pack (collision.bin + height.bin + holes).

Counts how many independent walkable ways lead from open ground into a
region (a station's floors, a flag deck, a generator room). A base with one
way in is easy to camp; docs/map-pipeline.md asks for at least three into a
main building, two onto a flag deck and exactly two into a generator room.

Model (walking only; no jumps, jets or drops): standing nodes sit on a 1 m
grid on every floor with 2.6 m of clear headroom (the body's four 0.52 m
spheres reach floor+2.56 m). Neighbours (8-connected) join when the floor
changes by at most 0.75 m and a 0.9 m wide band is clear between them at
knee, chest and head height. An entry is a cluster of graph edges crossing
from open ground's reachable component into the region; crossings within
ENTRY_MERGE metres of each other are one entry.

Airborne model (opt-in with airborne=True, for floating bases that no one
can walk to). It adds, on top of walking:

- Open sky. Every standable node with nothing solid above it for SKY metres
  is reachable from the field: in this game players ski and jet onto exposed
  decks, and each map's own tests keep its decks within jet reach of the
  terrain or a pad. Open-sky nodes seed the outside, alongside the ground.
- Drops. From a standing node a player can step off into a neighbouring
  column and fall to its highest floor that is more than MAX_RISE lower, if
  the body band is clear across and then all the way down.
- Hops. A short jet move from an outside node to a node in the region, at
  most HOP_REACH metres apart horizontally and JET_RISE vertically. It counts
  when one of five simple paths is clear for the body band: straight;
  straight up then across; across then straight down; or along x then z
  (or z then x) at the higher of the two heights. Each inside node near a
  wall or edge tries only its HOP_TRIES nearest outside candidates. JET_RISE is a conservative
  cut of the engine's standing climb: JET_ACCEL 37.28 against 20 gravity
  reaches the 16 m/s thrust knee in 0.93 s (7.4 m), holds about 17.8 m/s for
  the rest of the 4 s tank (about 53 m), then coasts about 8 m: roughly 69 m.
  Hops only cross the region's boundary; long flights between exterior decks
  are already covered by the open-sky rule.
- Over-the-top hops (opt-in, `overhead=(dh, ...)`). A sixth path climbs to
  dh metres above the inside node, crosses at that height and drops straight
  down onto it: the only way in through a slit or hatch high in a room's
  wall or roof. Off by default, so existing counts do not change.

Flag routes (flag_routes): distinct (entry, approach) pairs from open
ground to a flag, see that function.

A crossing's entry point is where its path first enters the region's volume
(the node box raised by HEADROOM), so every route through one door or one
shaft opening lands in one cluster.
"""
import json
import math
from collections import deque
from pathlib import Path

import numpy as np

GRID = 1.0
HEADROOM = 2.6
MAX_RISE = 0.75
EDGE_HEIGHTS = (0.45, 1.2, 2.2)
LATERAL = 0.45
BUCKET = 4.0
ENTRY_MERGE = 3.0
SKY = 40.0
HOP_REACH = 12.0
HOP_TRIES = 6
JET_RISE = 60.0


class Pack:
    def __init__(self, pack):
        pack = Path(pack)
        self.manifest = json.loads((pack/'map.json').read_text())
        t = np.frombuffer((pack/'collision.bin').read_bytes(), '<f4').astype(np.float64).reshape(-1, 3, 3)
        self.t = t
        self.a = t[:, 0]; self.e1 = t[:, 1]-t[:, 0]; self.e2 = t[:, 2]-t[:, 0]
        self.lo = t.min(1); self.hi = t.max(1)
        n = np.cross(self.e1, self.e2); self.ny = n[:, 1]/(np.linalg.norm(n, axis=1)+1e-12)
        self.h = np.frombuffer((pack/'height.bin').read_bytes(), '<u2').reshape(256, 256)/32.0
        self.step = self.manifest.get('terrain_step', 8.0)
        self.holes = set(self.manifest.get('holes', []))
        self.buckets = {}
        b0 = np.floor(self.lo[:, [0, 2]]/BUCKET).astype(int); b1 = np.floor(self.hi[:, [0, 2]]/BUCKET).astype(int)
        for i in range(len(t)):
            for bx in range(b0[i, 0], b1[i, 0]+1):
                for bz in range(b0[i, 1], b1[i, 1]+1):
                    self.buckets.setdefault((bx, bz), []).append(i)
        self.buckets = {k: np.array(v) for k, v in self.buckets.items()}

    def hole(self, x, z):
        ix, iz = int(math.floor(x/self.step)), int(math.floor(z/self.step))
        return iz*256+ix in self.holes

    def terrain(self, x, z):
        fx = min(max(x/self.step, 0), 255); fz = min(max(z/self.step, 0), 255)
        x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, 255), min(z0+1, 255)
        tx, tz = fx-x0, fz-z0
        a = self.h[z0, x0]+(self.h[z0, x1]-self.h[z0, x0])*tx
        b = self.h[z1, x0]+(self.h[z1, x1]-self.h[z1, x0])*tx
        return a+(b-a)*tz

    def near(self, x, z, r=1):
        bx, bz = int(math.floor(x/BUCKET)), int(math.floor(z/BUCKET))
        parts = [self.buckets[k] for k in ((bx+i, bz+j) for i in range(-r, r+1) for j in range(-r, r+1)) if k in self.buckets]
        return np.unique(np.concatenate(parts)) if parts else np.zeros(0, int)

    def column(self, x, z):
        """Heights of every non-vertical collision surface over (x, z)."""
        idx = self.near(x, z, 0)
        idx = idx[np.abs(self.ny[idx]) > 1e-3] if len(idx) else idx
        if not len(idx): return np.zeros(0)
        a, e1, e2 = self.a[idx], self.e1[idx], self.e2[idx]
        px, pz = x-a[:, 0], z-a[:, 2]
        det = e1[:, 0]*e2[:, 2]-e1[:, 2]*e2[:, 0]
        ok = np.abs(det) > 1e-12; det = np.where(ok, det, 1)
        u = (px*e2[:, 2]-pz*e2[:, 0])/det; v = (e1[:, 0]*pz-e1[:, 2]*px)/det
        k = ok & (u >= -1e-7) & (v >= -1e-7) & (u+v <= 1+1e-7)
        return np.sort(a[k, 1]+u[k]*e1[k, 1]+v[k]*e2[k, 1])

    def blocked(self, starts, ends, r=1):
        """Segment-vs-soup test for rays that all lie near one another: every
        ray must stay within about 4*r metres (horizontally) of the batch's
        middle."""
        mid = (starts.mean(0)+ends.mean(0))/2
        idx = self.near(mid[0], mid[2], r)
        if not len(idx): return np.zeros(len(starts), bool)
        A, E1, E2 = self.a[idx][None], self.e1[idx][None], self.e2[idx][None]
        s = starts[:, None]; d = ends[:, None]-s
        h = np.cross(d, E2); det = (E1*h).sum(-1)
        ok = np.abs(det) > 1e-9; inv = np.where(ok, 1/np.where(ok, det, 1), 0)
        tv = s-A; u = (tv*h).sum(-1)*inv; q = np.cross(tv, E1)
        v = (d*q).sum(-1)*inv; t = (E2*q).sum(-1)*inv
        return (ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (t > 1e-4) & (t < 1-1e-4)).any(1)


def floors(pack, x, z):
    """Standing heights over (x, z): structure floors and open terrain, each
    with at least HEADROOM clear above."""
    ys = pack.column(x, z)
    cand = list(ys)
    if not pack.hole(x, z): cand.append(pack.terrain(x, z))
    cand = sorted(set(round(float(y), 3) for y in cand))
    out = []
    ground = None if pack.hole(x, z) else pack.terrain(x, z)
    for y in cand:
        if ground is not None and y < ground-.05: continue      # under the ground
        above = ys[ys > y+.05]
        if len(above) and above[0] < y+HEADROOM: continue
        if out and y-out[-1] < .3: out[-1] = y; continue
        out.append(y)
    return out


def path_clear(pack, points):
    """Is a body-band polyline clear? Each leg is tested at EDGE_HEIGHTS above
    its points, on its centre line and LATERAL either side (for vertical legs,
    either side along x and along z)."""
    pts = np.asarray(points, float)
    for a, b in zip(pts, pts[1:]):
        d = b-a
        if np.hypot(d[0], d[2]) > 1e-6:
            side = np.array([-d[2], 0, d[0]])/np.hypot(d[0], d[2])
            offsets = [side*o for o in (-LATERAL, 0.0, LATERAL)]
        else:
            offsets = [np.zeros(3)]+[np.array(v)*LATERAL for v in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1))]
        starts = np.array([a+o+(0, h, 0) for h in EDGE_HEIGHTS for o in offsets])
        ends = np.array([b+o+(0, h, 0) for h in EDGE_HEIGHTS for o in offsets])
        if pack.blocked(starts, ends, r=2).any(): return False
    return True


def first_inside(points, volume, step=.25):
    """First point along a polyline inside an axis box (x0,x1,y0,y1,z0,z1)."""
    x0, x1, y0, y1, z0, z1 = volume
    pts = np.asarray(points, float)
    for a, b in zip(pts, pts[1:]):
        n = max(1, int(np.linalg.norm(b-a)/step))
        for t in np.linspace(0, 1, n+1):
            p = a+(b-a)*t
            if x0 <= p[0] <= x1 and y0 <= p[1] <= y1 and z0 <= p[2] <= z1: return p
    return pts[-1]


class Graph:
    def __init__(self, pack, x0, x1, z0, z1, airborne=False):
        self.pack = pack
        self.airborne = airborne
        self.nodes = []                      # (x, y, z)
        self.at = {}                         # (ix, iz) -> [node ids]
        for ix in range(int(math.floor(x0/GRID)), int(math.ceil(x1/GRID))+1):
            for iz in range(int(math.floor(z0/GRID)), int(math.ceil(z1/GRID))+1):
                x, z = ix*GRID+.5*GRID, iz*GRID+.5*GRID
                for y in floors(pack, x, z):
                    self.at.setdefault((ix, iz), []).append(len(self.nodes))
                    self.nodes.append((x, y, z))
        self.nodes = np.array(self.nodes)
        self.cell = {i: key for key, ids in self.at.items() for i in ids}
        self.adj = [[] for _ in range(len(self.nodes))]
        self.down = [[] for _ in range(len(self.nodes))]   # directed drops (airborne only)
        self._edges()
        if airborne: self._drops()

    def structure(self, i):
        x, y, z = self.nodes[i]
        return self.pack.hole(x, z) or abs(y-self.pack.terrain(x, z)) > .3

    def open_sky(self, i):
        x, y, z = self.nodes[i]
        above = self.pack.column(x, z)
        return not ((above > y+.05) & (above < y+SKY)).any()

    def edge_node(self, i):
        return len(self.adj[i]) < 8

    def _drops(self):
        for i in range(len(self.nodes)):
            if not self.structure(i): continue
            ix, iz = self.cell[i]; ax, ay, az = self.nodes[i]
            for dx in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    if not dx and not dz: continue
                    lower = [j for j in self.at.get((ix+dx, iz+dz), ()) if self.nodes[j, 1] < ay-MAX_RISE]
                    if not lower or any(abs(self.nodes[j, 1]-ay) <= MAX_RISE for j in self.at.get((ix+dx, iz+dz), ())): continue
                    j = max(lower, key=lambda k: self.nodes[k, 1])
                    bx, by, bz = self.nodes[j]
                    if path_clear(self.pack, [(ax, ay, az), (bx, ay, bz), (bx, by, bz)]):
                        self.down[i].append(j)

    def _edges(self):
        pairs = []
        for (ix, iz), ids in self.at.items():
            for dx, dz in ((1, 0), (0, 1), (1, 1), (1, -1)):
                for j in self.at.get((ix+dx, iz+dz), ()):
                    for i in ids:
                        if abs(self.nodes[i, 1]-self.nodes[j, 1]) <= MAX_RISE*(1.42 if dx and dz else 1): pairs.append((i, j))
        if not pairs: return
        pairs = np.array(pairs)
        a, b = self.nodes[pairs[:, 0]], self.nodes[pairs[:, 1]]
        d = b-a; flat = d.copy(); flat[:, 1] = 0
        side = np.stack([-flat[:, 2], np.zeros(len(d)), flat[:, 0]], 1)
        side /= np.linalg.norm(side, axis=1, keepdims=True)
        ok = np.ones(len(pairs), bool)
        order = np.lexsort((np.floor(a[:, 2]/BUCKET), np.floor(a[:, 0]/BUCKET)))
        keys = np.floor(a[order][:, [0, 2]]/BUCKET).astype(int)
        cuts = np.flatnonzero(np.any(np.diff(keys, axis=0) != 0, 1))+1
        for grp in np.split(order, cuts):
            starts, ends = [], []
            for h in EDGE_HEIGHTS:
                for off in (-LATERAL, 0.0, LATERAL):
                    o = side[grp]*off+np.array([0, h, 0])
                    starts.append(a[grp]+o); ends.append(b[grp]+o)
            hit = self.pack.blocked(np.concatenate(starts), np.concatenate(ends)).reshape(len(EDGE_HEIGHTS)*3, len(grp))
            ok[grp] = ~hit.any(0)
        for (i, j), good in zip(pairs, ok):
            if good: self.adj[i].append(j); self.adj[j].append(i)

    def reach(self, seeds, blocked):
        seen = np.zeros(len(self.nodes), bool); q = deque()
        for s in seeds:
            if not blocked[s]: seen[s] = True; q.append(s)
        while q:
            i = q.popleft()
            for j in (*self.adj[i], *self.down[i]):
                if not seen[j] and not blocked[j]: seen[j] = True; q.append(j)
        return seen


def hop_paths(a, b):
    """The five simple jet paths from node a to node b (see module doc)."""
    (ax, ay, az), (bx, by, bz) = a, b
    top = max(ay, by)
    return [[a, b],
            [a, (ax, top, az), (bx, top, bz), b],
            [a, (bx, top, bz), b],
            [a, (bx, top, az), (bx, top, bz), b],
            [a, (ax, top, bz), (bx, top, bz), b]]


def entries(graph, inside, outside_seeds, volume=None, overhead=()):
    """Clusters of edges from the region's outside (reached from the seeds
    without entering the region) into the region. Returns cluster centres.
    `volume` (x0,x1,y0,y1,z0,z1) places airborne crossings where their path
    enters the region. `overhead` (airborne only) adds hops that climb to
    each given height above the inside node, cross there and drop straight
    down: a slit or hatch in a room's ceiling or upper wall."""
    outside = graph.reach(outside_seeds, inside)
    return [np.mean([p for p, _ in c], 0) for c in _clusters(_crossings(graph, inside, outside, volume, overhead))]


def _crossings(graph, inside, outside, volume, overhead=()):
    """(point, inside node) for every walk edge, drop and hop from a node in
    the `outside` mask into the `inside` mask."""
    cross = [((graph.nodes[i]+graph.nodes[j])/2, j) for i in np.flatnonzero(outside) for j in graph.adj[i] if inside[j]]
    if graph.airborne:
        for i in np.flatnonzero(outside):
            for j in graph.down[i]:
                if inside[j]:
                    a, b = graph.nodes[i], graph.nodes[j]
                    cross.append((first_inside([a, (b[0], a[1], b[2]), b], volume), j))
        # Every real opening has an outside standing spot close to it (a
        # bridge by a door, a shaft floor under a hole, a ledge by a hatch),
        # so each inside edge node tries only its HOP_TRIES nearest outside
        # edge nodes within reach.
        near = [j for j in np.flatnonzero(inside) if graph.edge_node(j)]
        cand = np.array([i for i in np.flatnonzero(outside) if graph.edge_node(i)], int)
        if near and len(cand):
            nodes = graph.nodes; out_pts = nodes[cand]
            for j in near:
                b = nodes[j]
                d = np.hypot(out_pts[:, 0]-b[0], out_pts[:, 2]-b[2])
                ok = np.flatnonzero((d <= HOP_REACH) & (np.abs(out_pts[:, 1]-b[1]) <= JET_RISE))
                if not len(ok): continue
                ok = ok[np.argsort(np.linalg.norm(out_pts[ok]-b, axis=1))[:HOP_TRIES]]
                for k in ok:
                    i = cand[k]
                    if j in graph.adj[i]: continue
                    a = tuple(nodes[i]); paths = hop_paths(a, tuple(b))
                    paths += [[a, (a[0], b[1]+dh, a[2]), (b[0], b[1]+dh, b[2]), tuple(b)]
                              for dh in overhead if b[1]+dh > a[1]]
                    path = next((p for p in paths if path_clear(graph.pack, p)), None)
                    if path is not None:
                        cross.append((first_inside(path, volume), j)); break
    return cross


def _clusters(cross):
    """Group crossings whose points lie within ENTRY_MERGE of each other."""
    clusters = []
    for item in cross:
        for c in clusters:
            if min(np.linalg.norm(item[0]-q[0]) for q in c) < ENTRY_MERGE: c.append(item); break
        else: clusters.append([item])
    # Merge clusters that grew into each other.
    merged = True
    while merged:
        merged = False
        for i in range(len(clusters)):
            for j in range(i+1, len(clusters)):
                if min(np.linalg.norm(p[0]-q[0]) for p in clusters[i] for q in clusters[j]) < ENTRY_MERGE:
                    clusters[i] += clusters.pop(j); merged = True; break
            if merged: break
    return clusters


def box_region(graph, x0, x1, y0, y1, z0, z1):
    n = graph.nodes
    return (n[:, 0] >= x0) & (n[:, 0] <= x1) & (n[:, 1] >= y0) & (n[:, 1] <= y1) & (n[:, 2] >= z0) & (n[:, 2] <= z1)


def base_entries(pack, centre, regions, extent=64.0, seed_radius=48.0, airborne=False, overhead=()):
    """Entries into each named world-space region box (x0, x1, y0, y1, z0, z1)
    around one base: {name: [entry centres]}. Open ground is every terrain
    node at least seed_radius from the base centre, inside a square of
    half-size `extent`. With airborne=True, open-sky decks also seed the
    outside and drops and hops count (see the module doc); `overhead` adds
    over-the-top hops (see entries). Pass a Pack or a pack directory."""
    pack = pack if isinstance(pack, Pack) else Pack(pack)
    cx, cz = centre
    g = Graph(pack, cx-extent, cx+extent, cz-extent, cz+extent, airborne=airborne)
    seeds = list(open_ground(g, cx, cz, seed_radius))
    if airborne: seeds += [i for i in range(len(g.nodes)) if g.structure(i) and g.open_sky(i)]
    out = {}
    for name, box in regions.items():
        x0, x1, y0, y1, z0, z1 = box
        out[name] = entries(g, box_region(g, *box), seeds, (x0, x1, y0, y1+HEADROOM, z0, z1), overhead)
    return out


def open_ground(graph, cx, cz, radius):
    """Seed nodes: terrain-level nodes on a ring at least `radius` from the centre."""
    n = graph.nodes
    far = np.hypot(n[:, 0]-cx, n[:, 2]-cz) >= radius
    ground = np.array([abs(y-graph.pack.terrain(x, z)) < .3 for x, y, z in n])
    return np.flatnonzero(far & ground)


def flag_routes(pack, centre, building, flag_zone, extent=64.0, seed_radius=48.0, airborne=False, overhead=()):
    """Distinct routes to a flag (docs/map-pipeline.md: "two main entrances
    but ~10 ways of getting to the flag").

    A route is a pair (entry, approach): an entry is a distinct way from open
    ground into the `building` box (as base_entries counts it), and an
    approach is a distinct way into the `flag_zone` box (a few metres round
    the flag) that a player coming through that entry can reach while
    staying inside the building. Walks, drops and (airborne) jet hops all
    count; crossings within ENTRY_MERGE metres are one entry or approach.
    An entry that lands straight in the flag zone is one route by itself.
    Returns {'entries': [...], 'approaches': [[...] per entry], 'routes': n}.
    Both boxes are world-space (x0, x1, y0, y1, z0, z1)."""
    pack = pack if isinstance(pack, Pack) else Pack(pack)
    cx, cz = centre
    g = Graph(pack, cx-extent, cx+extent, cz-extent, cz+extent, airborne=airborne)
    seeds = list(open_ground(g, cx, cz, seed_radius))
    if airborne: seeds += [i for i in range(len(g.nodes)) if g.structure(i) and g.open_sky(i)]
    inside_b = box_region(g, *building)
    zone = box_region(g, *flag_zone)
    bx0, bx1, by0, by1, bz0, bz1 = building
    fx0, fx1, fy0, fy1, fz0, fz1 = flag_zone
    outside = g.reach(seeds, inside_b)
    entry_clusters = _clusters(_crossings(g, inside_b, outside, (bx0, bx1, by0, by1+HEADROOM, bz0, bz1), overhead))
    fence = zone | ~inside_b
    out = {'entries': [], 'approaches': [], 'routes': 0}
    for cluster in entry_clusters:
        landing = sorted({j for _, j in cluster})
        out['entries'].append(np.mean([p for p, _ in cluster], 0))
        if any(zone[j] for j in landing):
            out['approaches'].append([np.mean([p for p, _ in cluster], 0)]); out['routes'] += 1; continue
        mine = g.reach(landing, fence)
        found = [np.mean([p for p, _ in c], 0)
                 for c in _clusters(_crossings(g, zone, mine, (fx0, fx1, fy0, fy1+HEADROOM, fz0, fz1), overhead))]
        out['approaches'].append(found); out['routes'] += len(found)
    return out
