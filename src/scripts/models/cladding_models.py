"""Generated hard-surface detail for structures built from boxes
(docs/model-pipeline.md, "Hard-surface structures"), for Blender model
scripts: record a builder (`recorded`), then `clad` it with a `Style`.

On the outdoor faces and open tops that nothing covers
(scripts/assets/cladding.py) it puts:
- copings, flush with every wall and merlon top and lipped over the faces
  that drop away;
- trims round the exposed edges of slabs and roofs;
- a light line under each coping;
- fins up the long faces, clear of the team banners;
- a skirt along each face's foot;
- guards on the convex corners.

All of it stands within a few centimeters of the solid.
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, '..'))
from hardsurface import Solid, loft, quad_plate, solid_box
from assets import cladding

CAP_RISE, CAP_LIP, CAP_DROP = .012, .09, .3
THIN = 1.7                   # boxes no thicker than this are walls: capped whole
TRIM_W = .5                  # slabs: a trim this wide round their exposed edges


class Style:
    """How a map dresses its structures: the ground (a height or a function
    of local x, z), the light line's material, the fins' spacing, shortest
    face and material, the copings', skirts' and guards' materials, and the
    builder materials that are clad and capped."""
    def __init__(self, ground=-.1, light='amber', rib_spacing=4.5, rib_min_face=8.0, fin='plate', cap='hull',
                 skirt='hull', guard='plate', clad=('concrete',), capped=('concrete', 'panel'), banner_height=0.0):
        self.ground, self.light, self.rib_spacing, self.rib_min_face = ground, light, rib_spacing, rib_min_face
        self.fin, self.cap, self.skirt, self.guard, self.clad, self.capped = fin, cap, skirt, guard, clad, capped
        self.banner_height = banner_height      # team quads at least this tall are banners fins keep clear of


def recorded(build):
    """A recording of `build(mesh)` in its local frame."""
    spec = importlib.util.spec_from_file_location('kit', os.path.join(HERE, '..', 'build-original-map.py'))
    kit = importlib.util.module_from_spec(spec); spec.loader.exec_module(kit)
    mesh = cladding.recorder(kit.Mesh)()
    mesh.lamps = []
    build(mesh)
    return mesh


def coping(top, face_list, style):
    """A cap over each exposed part of a box's top: flush on top, lipped over
    the edges whose face drops away, flush where another box meets it."""
    b = top.box
    thin = min(b.x1-b.x0, b.z1-b.z0) <= THIN
    for x0, x1, z0, z1 in top.rects:
        lip = {}
        for name, axis, side, at, u0, u1 in (('-x', 'x', -1, x0 == b.x0, z0, z1), ('+x', 'x', 1, x1 == b.x1, z0, z1),
                                             ('-z', 'z', -1, z0 == b.z0, x0, x1), ('+z', 'z', 1, z1 == b.z1, x0, x1)):
            lip[name] = CAP_LIP if at and cladding.edge_exposed(face_list, b, axis, side, u0, u1, b.y1) else 0.0
        y0, y1 = b.y1-CAP_DROP, b.y1+CAP_RISE
        mats = {'*': style.cap, '+y': style.cap, '-y': 'hidden'}
        if thin:
            solid_box('coping', (x0-lip['-x'], y0, z0-lip['-z']), (x1+lip['+x'], y1, z1+lip['+z']), mats, bevel=.025)
            continue
        # A slab: trims along its lipped edges only.
        for name, lo, hi in (('-x', (x0-lip['-x'], z0), (x0+TRIM_W, z1)), ('+x', (x1-TRIM_W, z0), (x1+lip['+x'], z1)),
                             ('-z', (x0, z0-lip['-z']), (x1, z0+TRIM_W)), ('+z', (x0, z1-TRIM_W), (x1, z1+lip['+z']))):
            if lip[name] == 0: continue
            solid_box('trim', (lo[0], y0, lo[1]), (hi[0], y1, hi[1]), mats, bevel=.025)


def fin(face, u, y0, y1, style):
    """A chamfered fin up a face at u: 0.5 m at the wall, 0.3 m at its 0.18 m
    face."""
    sec = [(-.25, 0.0), (.25, 0.0), (.15, .18), (-.15, .18)]
    ring = lambda y: [face.point(u+a, y, d-.02) for a, d in sec]
    s = Solid('fin', .025)
    loft(s, [ring(y0), ring(y1)], ['hidden', 'steel', style.fin, 'steel'], caps=('hidden', 'steel'))


def dress_face(face, keep_clear, capped, style):
    b = face.box
    for u0, u1, y0, y1 in face.rects:
        h, w = y1-y0, u1-u0
        if h < 2.0 or w < 1.0: continue
        top = y1 >= b.y1-1e-6 and capped
        # A skirt along the face's foot (the ground, a floor or a roof).
        quad_plate(face.point(u0, y0, 0), face.point(u1, y0, 0), face.point(u1, y0+.5, 0), face.point(u0, y0+.5, 0),
                   0, 1, 0, 1, style.skirt, h=.12, away=face.point((u0+u1)/2, y0, -3), bevel=.03, name='skirt')
        # A light line under the coping.
        if top and w >= 2.0:
            ya, yb = b.y1-.58, b.y1-.48
            quad_plate(face.point(u0+.3, ya, 0), face.point(u1-.3, ya, 0), face.point(u1-.3, yb, 0), face.point(u0+.3, yb, 0),
                       0, 1, 0, 1, style.light, h=.03, away=face.point((u0+u1)/2, ya, -3), name='light')
        # Fins up long, tall faces, clear of banners.
        if h >= 3.0 and w >= style.rib_min_face:
            n = max(1, int(w/style.rib_spacing))
            spans = cladding.clear_spans(face, keep_clear, y0, y1, min_height=style.banner_height)
            for i in range(n):
                u = u0+(i+.5)*w/n
                if u-u0 < .8 or u1-u < .8 or any(a < u < c for a, c in spans): continue
                fin(face, u, y0+.5, (b.y1-.7) if top else y1, style)


def corners(box, face_list, style):
    """Guards up the convex corners where both faces are exposed together."""
    def runs(axis, side, u):
        for f in face_list:
            if f.box is box and f.axis == axis and f.out == side:
                return [(y0, y1) for u0, u1, y0, y1 in f.rects if u0-1e-6 <= u <= u1+1e-6]
        return []
    for sx in (-1, 1):
        for sz in (-1, 1):
            x = box.x0 if sx < 0 else box.x1
            z = box.z0 if sz < 0 else box.z1
            for a0, a1 in runs('x', sx, z):
                for c0, c1 in runs('z', sz, x):
                    y0, y1 = max(a0, c0), min(a1, c1)-(CAP_DROP+.05)
                    if y1-y0 < 2.0: continue
                    lo = (min(x, x+sx*.1)-(.3 if sx > 0 else 0), y0, min(z, z+sz*.1)-(.3 if sz > 0 else 0))
                    hi = (lo[0]+.4, y1, lo[2]+.4)
                    solid_box('corner', lo, hi, {'*': style.guard, '-y': 'hidden', '+y': 'steel'}, bevel=.04)


def clad(mesh, style=None):
    """Copings, trims, lights, fins, skirts and corner guards on a recorded
    structure's outdoor faces and open tops."""
    style = style or Style()
    face_list = cladding.faces(mesh.boxes, mesh.ramps, style.ground)
    top_list = cladding.tops(mesh.boxes)
    capped = set()
    for t in top_list:
        if t.box.tag or t.box.mat not in style.capped: continue
        coping(t, face_list, style)
        capped.add(id(t.box))
    clad_faces = [f for f in face_list if not f.box.tag and f.box.mat in style.clad]
    for f in clad_faces:
        dress_face(f, mesh.keep_clear, id(f.box) in capped, style)
    for b in {id(f.box): f.box for f in clad_faces}.values():
        corners(b, face_list, style)
