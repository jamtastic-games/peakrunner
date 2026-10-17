"""Where a structure's authored detail goes, from the builder's own boxes
(docs/model-pipeline.md, "Hard-surface structures"): the outside faces and
tops of its solid boxes that nothing covers and that stand under open sky.

A model script records a builder's `mesh.box` and `mesh.ramp` calls (and
its banners, kept clear) in the structure's local frame (`recorder`), then
asks for:

- `faces`: each solid box's vertical faces, less what other solids (boxes
  and ramps) cover from outside, split into runs where the air just outside
  is open to the sky (outside, not a room). Rects are (u0, u1, y0, y1): u
  runs along z on an 'x' face (a plane of constant x) and along x on a 'z'
  face; `out` is the side the face looks to (+1 or -1).
- `tops`: each solid box's top, less what stands on it, where it is open to
  the sky, with which of its four edges drop to an exposed face.

Detail built on these stays within a few centimeters of the solid surface
(the model script's job), so collision and look agree. Pure Python, so a
Blender script and the map tests can both use it.
"""
import math

EPS = .02          # a box this near a face covers it
STEP = .5          # sky samples along a face, meters
KEEP_CLEAR = ('ember', 'glacier')   # render-only quads in these (banners) stay uncovered


class Box:
    __slots__ = ('x0', 'x1', 'y0', 'y1', 'z0', 'z1', 'mat', 'solid', 'tag')

    def __init__(self, x0, x1, y0, y1, z0, z1, mat='concrete', solid=True, tag=None):
        self.x0, self.x1 = sorted((x0, x1)); self.y0, self.y1 = sorted((y0, y1)); self.z0, self.z1 = sorted((z0, z1))
        self.mat, self.solid, self.tag = mat, solid, tag

    def size(self):
        return self.x1-self.x0, self.y1-self.y0, self.z1-self.z0

    def __repr__(self):
        return f'Box(x {self.x0:g}..{self.x1:g}, y {self.y0:g}..{self.y1:g}, z {self.z0:g}..{self.z1:g}, {self.mat})'


def recorder(mesh_class):
    """A subclass of a kit mesh class that also records its boxes and ramps
    (local frame: build with origin (0, 0, 0) and yaw 0). Boxes made by its
    `equipment` (stations, mounts) are tagged 'equipment'."""
    class Recorder(mesh_class):
        def __init__(self, *a, **k):
            super().__init__(*a, **k)
            self.boxes, self.ramps, self.keep_clear = [], [], []
            self.tag = None

        def box(self, p, size, mat='concrete', solid=True):
            (x, y, z), (sx, sy, sz) = p, size
            self.boxes.append(Box(x-sx/2, x+sx/2, y-sy/2, y+sy/2, z-sz/2, z+sz/2, mat, solid, self.tag))
            return super().box(p, size, mat, solid)

        def equipment(self, *a, **k):
            self.tag = 'equipment'
            try: return super().equipment(*a, **k)
            finally: self.tag = None

        def quad(self, a, b, c, d, mat, solid=True):
            # Team banners and other flat render-only quads on faces: kept clear.
            if not solid and mat in KEEP_CLEAR:
                xs, ys, zs = zip(a, b, c, d)
                self.keep_clear.append(Box(min(xs), max(xs), min(ys), max(ys), min(zs), max(zs), mat, False))
            return super().quad(a, b, c, d, mat, solid)

        def ramp(self, x, width, z0, z1, y0, y1, mat='grate'):
            # Its slab's bounds: the surface and the 0.6 m beneath it.
            self.ramps.append(Box(x-width/2, x+width/2, min(y0, y1)-.6, max(y0, y1), z0, z1, mat))
            return super().ramp(x, width, z0, z1, y0, y1, mat)
    return Recorder


def _subtract(rects, cut):
    """Rects (u0, u1, y0, y1) less the rect `cut`."""
    a0, a1, b0, b1 = cut
    out = []
    for u0, u1, y0, y1 in rects:
        if a1 <= u0 or a0 >= u1 or b1 <= y0 or b0 >= y1:
            out.append((u0, u1, y0, y1)); continue
        if y0 < b0: out.append((u0, u1, y0, b0))
        if b1 < y1: out.append((u0, u1, b1, y1))
        m0, m1 = max(y0, b0), min(y1, b1)
        if u0 < a0: out.append((u0, a0, m0, m1))
        if a1 < u1: out.append((a1, u1, m0, m1))
    return out


def open_sky(solids, x, y, z):
    """Nothing solid stands over (x, z) above height y."""
    return not any(b.x0 < x < b.x1 and b.z0 < z < b.z1 and b.y1 > y+EPS for b in solids)


def _runs(flags, u0, step):
    """Consecutive True samples as (u0, u1) runs."""
    out, start = [], None
    for i, f in enumerate(flags+[False]):
        if f and start is None: start = i
        if not f and start is not None:
            out.append((u0+start*step, u0+i*step)); start = None
    return out


class Face:
    __slots__ = ('box', 'axis', 'plane', 'out', 'rects')

    def __init__(self, box, axis, plane, out, rects):
        self.box, self.axis, self.plane, self.out, self.rects = box, axis, plane, out, rects

    def point(self, u, y, d=0.0):
        """(x, y, z) at u, height y, d out from the face."""
        return (self.plane+self.out*d, y, u) if self.axis == 'x' else (u, y, self.plane+self.out*d)


def faces(boxes, ramps=(), ground=-.1, min_area=1.0):
    """The exposed, outdoor parts of every solid box's four vertical faces,
    above `ground`: a height, or a function of (x, z) for structures whose
    parts stand on different ground (taken at each box's middle)."""
    solids = [b for b in boxes if b.solid]
    covers = solids+list(ramps)
    out = []
    for b in solids:
        for axis, side in (('x', -1), ('x', 1), ('z', -1), ('z', 1)):
            plane = (b.x0 if side < 0 else b.x1) if axis == 'x' else (b.z0 if side < 0 else b.z1)
            u0, u1 = (b.z0, b.z1) if axis == 'x' else (b.x0, b.x1)
            g = ground((b.x0+b.x1)/2, (b.z0+b.z1)/2) if callable(ground) else ground
            rects = [(u0, u1, max(b.y0, g), b.y1)] if b.y1 > g else []
            for c in covers:
                if c is b or not rects: continue
                lo, hi = (c.x0, c.x1) if axis == 'x' else (c.z0, c.z1)
                # It occupies the air just outside this face.
                if side > 0 and not (lo <= plane+EPS and hi >= plane+.05): continue
                if side < 0 and not (hi >= plane-EPS and lo <= plane-.05): continue
                cu = (c.z0, c.z1) if axis == 'x' else (c.x0, c.x1)
                rects = _subtract(rects, (cu[0], cu[1], c.y0, c.y1))
            # Outdoors: split each rect where the air just outside it is under a roof.
            outdoor = []
            for r0, r1, y0, y1 in rects:
                if (r1-r0)*(y1-y0) < min_area: continue
                n = max(1, int(round((r1-r0)/STEP)))
                step = (r1-r0)/n
                flags = []
                for i in range(n):
                    u = r0+(i+.5)*step
                    p = (plane+side*.3, y1-.05, u) if axis == 'x' else (u, y1-.05, plane+side*.3)
                    flags.append(open_sky(solids, *p))
                outdoor += [(a, c, y0, y1) for a, c in _runs(flags, r0, step) if (c-a)*(y1-y0) >= min_area]
            if outdoor:
                out.append(Face(b, axis, plane, side, outdoor))
    return out


class Top:
    __slots__ = ('box', 'rects')

    def __init__(self, box, rects):
        self.box, self.rects = box, rects


def tops(boxes, min_area=.2):
    """The exposed, open-sky parts of every solid box's top: rects
    (x0, x1, z0, z1)."""
    solids = [b for b in boxes if b.solid]
    out = []
    for b in solids:
        rects = [(b.x0, b.x1, b.z0, b.z1)]
        for c in solids:
            if c is b: continue
            if c.y0 <= b.y1+EPS and c.y1 > b.y1+EPS:          # stands on it, or passes through its top
                rects = _subtract(rects, (c.x0, c.x1, c.z0, c.z1))
        rects = [r for r in rects if (r[1]-r[0])*(r[3]-r[2]) >= min_area
                 and open_sky(solids, (r[0]+r[1])/2, b.y1, (r[2]+r[3])/2)]
        if rects:
            out.append(Top(b, rects))
    return out


def clear_spans(face, keep_clear, y0, y1, pad=.4, min_height=0.0):
    """u spans on `face` taken by keep-clear quads (banners) overlapping
    heights y0..y1, padded; quads lower than `min_height` (a stripe along a
    wall) don't count."""
    out = []
    for q in keep_clear:
        if q.y1-q.y0 < min_height: continue
        lo, hi = (q.x0, q.x1) if face.axis == 'x' else (q.z0, q.z1)
        if hi < face.plane-.2 or lo > face.plane+.2 or q.y1 < y0 or q.y0 > y1: continue
        u = (q.z0, q.z1) if face.axis == 'x' else (q.x0, q.x1)
        out.append((u[0]-pad, u[1]+pad))
    return out


def edge_exposed(face_list, box, axis, side, u0, u1, y):
    """Whether `box`'s face (axis, side) is exposed just under height y
    over most of u0..u1."""
    for f in face_list:
        if f.box is box and f.axis == axis and f.out == side:
            covered = sum(max(0.0, min(u1, r1)-max(u0, r0)) for r0, r1, y0, y1 in f.rects if y0 < y-.05 <= y1+EPS)
            return covered >= .6*(u1-u0)
    return False
