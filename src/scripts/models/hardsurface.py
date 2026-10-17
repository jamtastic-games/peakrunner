"""Shared tools for hard-surface structure models (docs/model-pipeline.md,
"Hard-surface structures"), for Blender's Python: closed solids with a
material per face, chamfered in steel, shaded flat, laid out on one texture
and baked (color with ambient occlusion, glow, team mask), and written as
binary glTF. A builder script imports it, sets `FRAME` (its coordinates to
the file's), makes its solids (each belongs to the current `ROOT`, one scene
root per structure in the file), and calls `finish`.

    blender -b -P scripts/models/<builder>.py -- OUT.glb [--texture 4096]
"""
import math
import os
import struct
import sys

import bmesh
import bpy
import numpy as np

def srgb(h):
    return tuple(int(h[i:i+2], 16)/255 for i in (1, 3, 5))


# Materials: albedo, glow (None: none). The art direction's palette, a
# little lighter than its swatches since the game lights models dimly.
MATS = {
    'hull': ('#59606A', None), 'plate': ('#6C747E', None), 'light': ('#9AA3AD', None),
    'steel': ('#B2BBC5', None), 'recess': ('#1D1F23', None), 'rubber': ('#24201D', None),
    'team': ('#808080', None), 'glow': ('#9AF2FF', '#5CE6FF'), 'dim': ('#24525E', '#1C7890'),
    'amber': ('#E8B448', '#E0A030'), 'glass': ('#111820', '#0B2E38'), 'grille': ('#272A2F', None),
    'grate': ('#3A3F45', None),
}
NAMES = list(MATS)
MAT = {n: i for i, n in enumerate(NAMES)}
# Physically based materials (with PBR set): roughness and metalness. Bare
# gunmetal and steel are metal; plates are painted (satin), team paint a
# glossier coat over them; the undersuit, rubber and recesses are matte.
SURFACE = {
    'hull': (.42, .85), 'plate': (.5, .0), 'light': (.52, .0), 'steel': (.3, 1.0), 'recess': (.82, .0),
    'rubber': (.9, .0), 'team': (.38, .0), 'glow': (.3, .0), 'dim': (.4, .0), 'amber': (.35, .0),
    'glass': (.06, .0), 'grille': (.55, .55), 'grate': (.55, .65),
}
# Painted materials: they wear to bare metal at scratches (PBR only).
PAINTED = ('plate', 'light', 'team')

TEX = 4096
SOLIDS = []
POINTS = []                              # named points: (name, (x, y, z)), written as empty nodes
ROOTS = []                               # scene roots in the file, in order
ROOT = None                              # the root new solids belong to
FRAME = lambda p: p                      # a builder's coordinates -> the file's
# Settings a builder may change (the defaults are what the first models were
# built with): whether a plate keeps its hidden back face, how islands are
# packed on the texture, and the grate's slot period and duty.
PLATE_BACKS = True
PACK = dict(margin=0.0015, rotate=True)
GRATE = (.12, .35)
# Bake each scene root this far apart along X (meters; 0: where they are), so
# roots that overlap in the file (an armor's parts round their bone origins,
# a map's structures round theirs) don't shade each other.
SPREAD = 0.0
# Physically based output: besides the albedo and glow, a metallic-roughness
# texture with the occlusion in its red channel (`SURFACE`, worn at scratches)
# and a tangent-space normal map of each material's surface detail
# (`detail_tree`), with the tangents written; the occlusion then leaves the
# albedo. Off by default, which keeps the first models' two textures.
PBR = False
# Skinning (a builder sets them): the joints, each a name and its rest frame
# (4x4, the file's coordinates, columns as the engine's), in the file's
# order; and the roots whose mesh bends with them (their solids' `weights`).
JOINTS = []
SKINNED = set()
# How fine the surface detail is: 1 for models seen at arm's length (armor,
# weapons; the patterns' sizes are in centimeters), larger for big ones.
DETAIL = 1.0
# The texture detail new solids get, relative to the rest (a builder lowers
# it for parts seldom seen up close, leaving more for the rest).
TEXEL = 1.0
# The detail plates and rings of one flat color (glow, team paint, glass)
# get, relative to TEXEL: they need little, and a ring's island is mostly hole.
FLAT = ('glow', 'dim', 'amber', 'team', 'glass')
FLAT_TEXEL = 1.0


def flat(s, mat):
    """`s`, given FLAT_TEXEL's share of the detail if `mat` is a flat color."""
    if mat in FLAT:
        s.texel = s.texel*FLAT_TEXEL
    return s


def root(name):
    """Make `name` the scene root new solids belong to."""
    global ROOT
    if name not in ROOTS: ROOTS.append(name)
    ROOT = name


class Solid:
    """A closed mesh (the builder's coordinates) with a material per face,
    chamfered by `bevel` meters and shaded flat across edges sharper than
    `smooth` degrees; it belongs to the current root."""

    def __init__(self, name, bevel=0.05, smooth=1.0):
        self.name, self.bevel, self.smooth = name, bevel, smooth
        self.v, self.f, self.m = [], [], []
        self.hidden = set()                  # faces dropped once normals are known
        self.texel = TEXEL
        self.root = ROOTS.index(ROOT) if ROOT in ROOTS else 0
        # Per point, {joint name: weight} for a skinned solid (else None).
        self.weights = None
        SOLIDS.append(self)

    def pts(self, pts):
        i = len(self.v)
        self.v.extend(tuple(float(c) for c in p) for p in pts)
        return list(range(i, i+len(pts)))

    def face(self, idx, mat):
        """A face of material `mat`; 'hidden' marks one never seen (buried,
        or against another solid), dropped from the model."""
        if mat == 'hidden':
            self.hidden.add(len(self.f))
            mat = 'recess'
        self.f.append(list(idx))
        self.m.append(MAT[mat])


def loft(s, rings, mats, caps=('hull', 'hull')):
    """Quads between consecutive rings (each the same count of points, in
    order round it); `mats` is one material, one per side of the ring, or a
    function (side, span) -> material. A cap is a material (a flat face), a
    ('tip', point, material) fan, or None."""
    n = len(rings[0])
    pick = mats if callable(mats) else (lambda k, i: mats) if isinstance(mats, str) else (lambda k, i: mats[k])
    idx = [s.pts(r) for r in rings]
    for i, (a, b) in enumerate(zip(idx, idx[1:])):
        for k in range(n):
            j = (k+1) % n
            s.face([a[k], a[j], b[j], b[k]], pick(k, i))
    for end, cap in zip((idx[0], idx[-1]), caps):
        if cap is None:
            continue
        if isinstance(cap, tuple):
            t = s.pts([cap[1]])[0]
            for k in range(n):
                s.face([t, end[k], end[(k+1) % n]], cap[2] if isinstance(cap[2], str) else cap[2][k])
        else:
            s.face(end, cap)


def box(s, lo, hi, mats='hull'):
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    v = s.pts([(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
               (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)])
    faces = {'-z': (0, 3, 2, 1), '+z': (4, 5, 6, 7), '-y': (0, 1, 5, 4), '+y': (3, 7, 6, 2),
             '-x': (0, 4, 7, 3), '+x': (1, 2, 6, 5)}
    for k, f in faces.items():
        s.face([v[i] for i in f], mats if isinstance(mats, str) else mats.get(k, mats['*']))


def solid_box(name, lo, hi, mats='hull', bevel=0.03):
    box(Solid(name, bevel), lo, hi, mats)


def circle(cx, cy, r, z, n, phase=None):
    """A ring round the axis along Z through (cx, cy)."""
    phase = math.pi/n if phase is None else phase
    return [(cx+r*math.cos(phase+2*math.pi*k/n), cy+r*math.sin(phase+2*math.pi*k/n), z) for k in range(n)]


def circle_y(cx, cz, r, y, n, phase=None):
    """A ring round the vertical axis through (cx, cz)."""
    phase = math.pi/n if phase is None else phase
    return [(cx+r*math.cos(phase+2*math.pi*k/n), y, cz+r*math.sin(phase+2*math.pi*k/n)) for k in range(n)]


def revolve(name, cx, cy, profile, n, mats, caps, bevel=0.0, smooth=25.0):
    """A solid of revolution about the axis along Z through (cx, cy):
    `profile` is (z, r) points, `mats` one per span between them."""
    s = Solid(name, bevel, smooth)
    loft(s, [circle(cx, cy, r, z, n) for z, r in profile], lambda k, i: mats[i], caps)
    return s


def quad_plate(A, B, C, D, u0, u1, v0, v1, mat, h=0.04, away=(0.0, 2.4, None), bevel=0.0, name='decal'):
    """A thin plate on the flat quad ABCD (A at u, v = 0, 0, B at 1, 0, C at
    1, 1, D at 0, 1) over [u0, u1] x [v0, v1], h proud of it on the side away
    from `away` (a point; None takes the quad's own coordinate)."""
    A, B, C, D = (np.array(p, np.float64) for p in (A, B, C, D))
    q = lambda u, v: (1-v)*((1-u)*A+u*B)+v*((1-u)*D+u*C)
    n = np.cross(B-A, D-A)
    n /= np.linalg.norm(n)
    m = q(.5, .5)
    ref = np.array([m[i] if c is None else c for i, c in enumerate(away)])
    if np.dot(n, m-ref) < 0:
        n = -n
    corners = [q(u0, v0), q(u1, v0), q(u1, v1), q(u0, v1)]
    s = Solid(name, bevel)
    lo = s.pts([c-n*0.02 for c in corners])
    hi = s.pts([c+n*h for c in corners])
    s.face(hi, mat)
    s.face(lo[::-1], 'recess')
    if not PLATE_BACKS:
        s.hidden.add(len(s.f)-1)
    for i in range(4):
        j = (i+1) % 4
        s.face([lo[i], lo[j], hi[j], hi[i]], mat)
    return flat(s, mat)


def beam(name, p0, p1, w, h, mat, bevel=.02):
    """A square beam from p0 to p1, w wide (level) and h deep."""
    p0, p1 = np.array(p0, np.float64), np.array(p1, np.float64)
    d = (p1-p0)/np.linalg.norm(p1-p0)
    side = np.cross(d, [0.0, 1.0, 0.0])
    side = side/np.linalg.norm(side) if np.linalg.norm(side) > 1e-6 else np.array([1.0, 0.0, 0.0])
    up = np.cross(side, d)
    sec = [(-w/2, -h/2), (w/2, -h/2), (w/2, h/2), (-w/2, h/2)]
    s = Solid(name, bevel)
    loft(s, [[p+side*a+up*b for a, b in sec] for p in (p0, p1)], mat, caps=(mat, mat))
    return s


def ring_solid(name, cx, cz, r0, r1, y0, y1, n, mat):
    s = Solid(name, 0, smooth=25)
    loft(s, [circle_y(cx, cz, r, y, n) for y, r in ((y0, r1), (y1, r1), (y1, r0), (y0, r0))], mat, caps=(None, None))
    a, b = len(s.v)-n, len(s.v)-4*n
    for k in range(n):
        s.face([a+k, a+(k+1) % n, b+(k+1) % n, b+k], mat)
    return flat(s, mat)


def plan(z0, z1, w, cf, cb):
    """An octagonal plan (x, z): front at z0, back at z1, half width w, its
    front corners cut by cf and its back corners by cb."""
    return [(-w+cf, z0), (w-cf, z0), (w, z0+cf), (w, z1-cb), (w-cb, z1), (-w+cb, z1), (-w, z1-cb), (-w, z0+cf)]


def tier(name, y0, y1, low, high, side_mats, top_mat, bevel=.07):
    """A block on a plan: `low` at y0 rising to `high` (same count of points)
    at y1."""
    s = Solid(name, bevel)
    loft(s, [[(x, y0, z) for x, z in low], [(x, y1, z) for x, z in high]], side_mats, caps=('recess', top_mat))
    return s


# --- Into Blender, chamfer, shade, unwrap, bake, write ------------------------

def to_blender(p):
    """A builder's point in Blender's frame of the file's axes: FRAME takes
    it to the file's (x, y, z), which Blender reads as (x, -z, y)."""
    x, y, z = FRAME(p)
    return (x, -z, y)


def make_object(s, cutter=None):
    mesh = bpy.data.meshes.new(s.name)
    mesh.from_pydata([to_blender(p) for p in s.v], [], s.f)
    mesh.polygons.foreach_set('material_index', s.m)
    mesh.attributes.new('root', 'INT', 'FACE').data.foreach_set('value', [s.root]*len(s.f))
    mesh.attributes.new('texel', 'FLOAT', 'FACE').data.foreach_set('value', [s.texel]*len(s.f))
    for n in NAMES:
        mesh.materials.append(MATERIALS[n])
    bm = bmesh.new()
    bm.from_mesh(mesh)
    if s.hidden:
        layer = bm.faces.layers.int.new('hidden')
        bm.faces.ensure_lookup_table()
        for i in s.hidden: bm.faces[i][layer] = 1
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    bmesh.ops.dissolve_degenerate(bm, dist=1e-5, edges=bm.edges)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    if s.hidden:
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[layer]], context='FACES_ONLY')
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(s.name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    if s.weights is not None:
        # Bone weights by position: the points merged above share theirs.
        at = {tuple(round(c, 5) for c in to_blender(p)): w for p, w in zip(s.v, s.weights)}
        groups = {}
        for v in mesh.vertices:
            for name, w in at.get(tuple(round(c, 5) for c in v.co), {}).items():
                if w <= 0: continue
                g = groups.get(name) or obj.vertex_groups.get(name) or obj.vertex_groups.new(name=name)
                groups[name] = g
                g.add([v.index], w, 'REPLACE')
    if cutter is not None:
        mod = obj.modifiers.new('cut', 'BOOLEAN')
        mod.operation, mod.object, mod.solver = 'DIFFERENCE', cutter, 'EXACT'
        mod.material_mode = 'TRANSFER'
        mod.use_self = True                   # the cutter's parts overlap
    if s.bevel > 0:
        mod = obj.modifiers.new('bevel', 'BEVEL')
        mod.width, mod.segments, mod.limit_method = s.bevel, 1, 'ANGLE'
        mod.angle_limit = math.radians(30)
        mod.material = MAT['steel']
        mod.use_clamp_overlap = True
        mod.harden_normals = False
    bpy.context.view_layer.objects.active = obj
    for mod in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=mod.name)
    obj.data.shade_smooth()
    obj.data.set_sharp_from_angle(angle=math.radians(s.smooth))
    return obj


def color_tree(nt, name):
    """The albedo of material `name` as a node output: its color with a
    little large-scale variation; grilles and grates get their slots."""
    base = srgb(MATS[name][0])
    coord = nt.nodes.new('ShaderNodeTexCoord').outputs['Object']
    noise = nt.nodes.new('ShaderNodeTexNoise')
    noise.inputs['Scale'].default_value = .35
    noise.inputs['Detail'].default_value = 3.0
    nt.links.new(coord, noise.inputs['Vector'])
    vary = nt.nodes.new('ShaderNodeMapRange')
    vary.inputs['To Min'].default_value, vary.inputs['To Max'].default_value = .9, 1.1
    nt.links.new(noise.outputs['Fac'], vary.inputs['Value'])
    rgb = nt.nodes.new('ShaderNodeRGB')
    rgb.outputs[0].default_value = (*base, 1)
    out = nt.nodes.new('ShaderNodeVectorMath')
    out.operation = 'SCALE'
    nt.links.new(rgb.outputs[0], out.inputs[0])
    nt.links.new(vary.outputs['Result'], out.inputs['Scale'])
    color = out.outputs[0]
    sep = nt.nodes.new('ShaderNodeSeparateXYZ')
    nt.links.new(coord, sep.inputs[0])

    def slots(axis, period, duty):
        div = nt.nodes.new('ShaderNodeMath')
        div.operation = 'DIVIDE'
        nt.links.new(sep.outputs[axis], div.inputs[0])
        div.inputs[1].default_value = period
        fr = nt.nodes.new('ShaderNodeMath')
        fr.operation = 'FRACT'
        nt.links.new(div.outputs[0], fr.inputs[0])
        lt = nt.nodes.new('ShaderNodeMath')
        lt.operation = 'LESS_THAN'
        nt.links.new(fr.outputs[0], lt.inputs[0])
        lt.inputs[1].default_value = duty
        return lt.outputs[0]

    def darken(mask, by):
        dark = nt.nodes.new('ShaderNodeVectorMath')
        dark.operation = 'SCALE'
        nt.links.new(color, dark.inputs[0])
        dark.inputs['Scale'].default_value = by
        mix = nt.nodes.new('ShaderNodeMix')
        mix.data_type = 'RGBA'
        sock = lambda socks, ident: next(x for x in socks if x.identifier == ident)
        nt.links.new(mask, sock(mix.inputs, 'Factor_Float'))
        nt.links.new(color, sock(mix.inputs, 'A_Color'))
        nt.links.new(dark.outputs[0], sock(mix.inputs, 'B_Color'))
        return sock(mix.outputs, 'Result_Color')
    if name in ('hull', 'plate') and not PBR:
        # Panel seams across the ship every 2.4 m.
        color = darken(slots('X', 2.4, .014), .55)
    if PBR and name in PAINTED:
        # Paint worn to bright metal at scratches.
        mix = nt.nodes.new('ShaderNodeMix')
        mix.data_type = 'RGBA'
        sock = lambda socks, ident: next(x for x in socks if x.identifier == ident)
        bare = nt.nodes.new('ShaderNodeRGB')
        bare.outputs[0].default_value = (*srgb(MATS['steel'][0]), 1)
        nt.links.new(scratches(nt), sock(mix.inputs, 'Factor_Float'))
        nt.links.new(color, sock(mix.inputs, 'A_Color'))
        nt.links.new(bare.outputs[0], sock(mix.inputs, 'B_Color'))
        color = sock(mix.outputs, 'Result_Color')
    if name == 'grille':
        color = darken(slots('X', .3, .45), .35)
    if name == 'grate':
        mx = nt.nodes.new('ShaderNodeMath')
        mx.operation = 'MAXIMUM'
        nt.links.new(slots('X', *GRATE), mx.inputs[0])
        nt.links.new(slots('Y', *GRATE), mx.inputs[1])
        color = darken(mx.outputs[0], .35)
    return color


def _math(nt, op, a, b=None):
    n = nt.nodes.new('ShaderNodeMath')
    n.operation = op
    for i, v in enumerate((a, b)):
        if v is None: continue
        if isinstance(v, (int, float)): n.inputs[i].default_value = v
        else: nt.links.new(v, n.inputs[i])
    return n.outputs[0]


def _noise(nt, scale, detail=2.0, stretch=None):
    """Noise over the object's coordinates (meters), `scale` cells a meter
    (times DETAIL's inverse); `stretch` scales the axes first."""
    coord = nt.nodes.new('ShaderNodeTexCoord').outputs['Object']
    if stretch:
        m = nt.nodes.new('ShaderNodeVectorMath')
        m.operation = 'MULTIPLY'
        m.inputs[1].default_value = stretch
        nt.links.new(coord, m.inputs[0])
        coord = m.outputs[0]
    n = nt.nodes.new('ShaderNodeTexNoise')
    n.inputs['Scale'].default_value = scale/DETAIL
    n.inputs['Detail'].default_value = detail
    nt.links.new(coord, n.inputs['Vector'])
    return n.outputs['Fac']


def scratches(nt):
    """A mask of light chips in the paint, sparse: small flecks of a fine
    noise, kept where a broader noise says the surface took wear (clean,
    field-maintained gear: no grime, no long cracks)."""
    worn = _math(nt, 'GREATER_THAN', _noise(nt, 2.5, 2.0), .62)
    chips = _math(nt, 'GREATER_THAN', _noise(nt, 70.0, 5.0), .76)
    return _math(nt, 'MULTIPLY', chips, worn)


def detail_tree(nt, name):
    """The height of material `name`'s surface detail (meters, tiny): orange
    peel and scratches on paint, brushing on steel, a diamond quilt on the
    undersuit, grain on rubber, slots in grilles and grates."""
    if name in PAINTED or name == 'hull':
        peel = _math(nt, 'MULTIPLY', _noise(nt, 260.0, 2.0), .00012)
        cut = _math(nt, 'MULTIPLY', scratches(nt), -.0005)
        return _math(nt, 'ADD', peel, cut)
    if name == 'steel':
        return _math(nt, 'MULTIPLY', _noise(nt, 30.0, 6.0, stretch=(1.0, 1.0, 40.0)), .00015)
    if name == 'recess':
        # Diamond quilting about 3 cm across, stitched seams between the pads.
        coord = nt.nodes.new('ShaderNodeTexCoord').outputs['Object']
        sep = nt.nodes.new('ShaderNodeSeparateXYZ')
        nt.links.new(coord, sep.inputs[0])
        k = 2*math.pi/(.03*DETAIL)
        def band(a, b):
            v = _math(nt, 'ADD', _math(nt, 'MULTIPLY', sep.outputs[a], k), _math(nt, 'MULTIPLY', sep.outputs[b], k))
            return _math(nt, 'ABSOLUTE', _math(nt, 'SINE', v))
        pad = _math(nt, 'MULTIPLY', _math(nt, 'MINIMUM', band('X', 'Z'), band('Z', 'Y')), .0012)
        weave = _math(nt, 'MULTIPLY', _noise(nt, 900.0, 1.0), .00006)
        return _math(nt, 'ADD', pad, weave)
    if name == 'rubber':
        return _math(nt, 'MULTIPLY', _noise(nt, 420.0, 2.0), .00025)
    if name in ('grille', 'grate'):
        return _math(nt, 'MULTIPLY', _noise(nt, 200.0, 2.0), .0001)
    return _math(nt, 'MULTIPLY', _noise(nt, 300.0, 1.0), .00003)


def set_pass(target, kind):
    for name in NAMES:
        mat = MATERIALS[name]
        nt = mat.node_tree
        nt.nodes.clear()
        out = nt.nodes.new('ShaderNodeOutputMaterial')
        em = nt.nodes.new('ShaderNodeEmission')
        nt.links.new(em.outputs[0], out.inputs['Surface'])
        img = nt.nodes.new('ShaderNodeTexImage')
        img.image = target
        nt.nodes.active = img
        if kind == 'color':
            nt.links.new(color_tree(nt, name), em.inputs['Color'])
        elif kind == 'glow':
            g = MATS[name][1]
            em.inputs['Color'].default_value = (*srgb(g), 1) if g else (0, 0, 0, 1)
        elif kind == 'team':
            em.inputs['Color'].default_value = (1, 1, 1, 1) if name == 'team' else (0, 0, 0, 1)
        elif kind == 'surface':
            # Red unused here (the occlusion goes there), green roughness,
            # blue metalness; paint wears to smooth bare metal at scratches.
            rough, metal = SURFACE.get(name, (.6, .0))
            vary = _math(nt, 'MULTIPLY', _math(nt, 'SUBTRACT', _noise(nt, 6.0, 3.0), .5), .12)
            r = _math(nt, 'ADD', rough, vary)
            m = metal
            if name in PAINTED:
                wear = scratches(nt)
                r = _math(nt, 'ADD', r, _math(nt, 'MULTIPLY', wear, .3-rough))
                m = _math(nt, 'MULTIPLY', wear, 1.0)
            comb = nt.nodes.new('ShaderNodeCombineXYZ')
            comb.inputs[0].default_value = 1.0
            nt.links.new(r, comb.inputs[1])
            if isinstance(m, float): comb.inputs[2].default_value = m
            else: nt.links.new(m, comb.inputs[2])
            nt.links.new(comb.outputs[0], em.inputs['Color'])
        elif kind == 'normal':
            # The detail's height as a bump on a plain surface: the bake
            # writes the bent normal in the mesh's tangent frame.
            nt.nodes.remove(em)
            bsdf = nt.nodes.new('ShaderNodeBsdfDiffuse')
            bump = nt.nodes.new('ShaderNodeBump')
            bump.inputs['Strength'].default_value = 1.0
            bump.inputs['Distance'].default_value = 1.0
            nt.links.new(detail_tree(nt, name), bump.inputs['Height'])
            nt.links.new(bump.outputs['Normal'], bsdf.inputs['Normal'])
            nt.links.new(bsdf.outputs[0], out.inputs['Surface'])


def bake(obj, kind):
    img = bpy.data.images.new(f'bake-{kind}', TEX, TEX, alpha=False, float_buffer=True)
    img.colorspace_settings.name = 'Non-Color'
    set_pass(img, kind)
    scene = bpy.context.scene
    scene.cycles.samples = 48 if kind == 'ao' else 8 if kind == 'normal' else 4
    scene.render.bake.margin = 10
    bpy.ops.object.select_all(action='DESELECT')
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    if kind == 'normal':
        bpy.ops.object.bake(type='NORMAL', normal_space='TANGENT', margin=10)
    else:
        bpy.ops.object.bake(type='AO' if kind == 'ao' else 'EMIT', margin=10)
    px = np.array(img.pixels[:], dtype=np.float32).reshape(TEX, TEX, 4)[..., :3]
    print(f'baked {kind}')
    return px


def save(name, rgb, alpha, fmt):
    img = bpy.data.images.new(name, TEX, TEX, alpha=True)
    img.pixels[:] = np.concatenate([np.clip(rgb, 0, 1), alpha[..., None]], -1).ravel()
    path = os.path.join(bpy.app.tempdir, f'{name}.{fmt.lower()}')
    st = bpy.context.scene.render.image_settings
    st.file_format = fmt
    st.color_mode = 'RGB' if fmt == 'JPEG' else 'RGBA'
    st.quality, st.compression = 90, 90
    img.save_render(path, scene=bpy.context.scene)
    return open(path, 'rb').read()


def save_data(name, rgb, fmt):
    """A data texture (normals, roughness and metalness) as its raw values,
    with none of the display transform `save` applies to colors."""
    img = bpy.data.images.new(name, TEX, TEX, alpha=False)
    img.colorspace_settings.name = 'Non-Color'
    img.pixels[:] = np.concatenate([np.clip(rgb, 0, 1), np.ones(rgb.shape[:2]+(1,), np.float32)], -1).ravel()
    path = os.path.join(bpy.app.tempdir, f'{name}.{fmt.lower()}')
    img.filepath_raw = path
    img.file_format = fmt
    if fmt == 'JPEG':
        bpy.context.scene.render.image_settings.quality = 92
    img.save()
    return open(path, 'rb').read()


def arrays(obj):
    """Per scene root: positions and normals (the file's axes), texture
    coordinates and triangles."""
    mesh = obj.data
    mesh.calc_loop_triangles()
    nt = len(mesh.loop_triangles)
    tri = np.empty(nt*3, dtype=np.int64)
    mesh.loop_triangles.foreach_get('loops', tri)
    poly = np.empty(nt, dtype=np.int64)
    mesh.loop_triangles.foreach_get('polygon_index', poly)
    roots = np.empty(len(mesh.polygons), dtype=np.int64)
    mesh.attributes['root'].data.foreach_get('value', roots)
    lv = np.empty(len(mesh.loops), dtype=np.int64)
    mesh.loops.foreach_get('vertex_index', lv)
    co = np.empty(len(mesh.vertices)*3, dtype=np.float32)
    mesh.vertices.foreach_get('co', co)
    cn = np.empty(len(mesh.loops)*3, dtype=np.float32)
    mesh.corner_normals.foreach_get('vector', cn)
    uv = np.empty(len(mesh.loops)*2, dtype=np.float32)
    mesh.uv_layers.active.data.foreach_get('uv', uv)
    g = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]], dtype=np.float32)   # Blender -> file
    pos = co.reshape(-1, 3)[lv] @ g.T
    nrm = cn.reshape(-1, 3) @ g.T
    uvs = uv.reshape(-1, 2)*np.array([1, -1], dtype=np.float32)+np.array([0, 1], dtype=np.float32)
    skin = None
    if JOINTS:
        # Each vertex's four strongest bones (indices into JOINTS), weights
        # summing to one; none for a vertex in no group.
        index = {name: i for i, (name, _) in enumerate(JOINTS)}
        names = [index.get(g.name, -1) for g in obj.vertex_groups]
        joints = np.zeros((len(mesh.vertices), 4), np.uint16)
        weights = np.zeros((len(mesh.vertices), 4), np.float32)
        for v in mesh.vertices:
            gw = sorted(((g.weight, names[g.group]) for g in v.groups if names[g.group] >= 0 and g.weight > 0), reverse=True)[:4]
            total = sum(w for w, _ in gw)
            for k, (w, j) in enumerate(gw):
                joints[v.index, k], weights[v.index, k] = j, w/total
        skin = (joints[lv], weights[lv])
    if PBR:
        # The tangent frame the normal map was baked in (MikkTSpace): the
        # tangent and the bitangent's sign per corner.
        mesh.calc_tangents()
        tg = np.empty(len(mesh.loops)*3, dtype=np.float32)
        mesh.loops.foreach_get('tangent', tg)
        sign = np.empty(len(mesh.loops), dtype=np.float32)
        mesh.loops.foreach_get('bitangent_sign', sign)
        tan = np.concatenate([tg.reshape(-1, 3) @ g.T, sign[:, None]], 1)
    tri = tri.reshape(-1, 3)
    out = []
    for r in range(len(ROOTS) or 1):
        loops = tri[roots[poly] == r].ravel()
        parts = [pos[loops]*1e5, nrm[loops]*1e3, uvs[loops]*1e5] + ([tan[loops]*1e3] if PBR else [])
        key = np.round(np.concatenate(parts, 1)).astype(np.int64)
        _, first, inverse = np.unique(key, axis=0, return_index=True, return_inverse=True)
        bones = (skin[0][loops][first], skin[1][loops][first]) if skin is not None and ROOTS and ROOTS[r] in SKINNED else None
        out.append((pos[loops][first], nrm[loops][first], uvs[loops][first], inverse.ravel().astype(np.uint32),
                    tan[loops][first] if PBR else None, bones))
    return out


def write_glb(path, obj, albedo, glow, generator, material, surface=None, normal=None):
    binary = bytearray()
    views, accessors = [], []

    def view(data, target=None):
        while len(binary) % 4:
            binary.append(0)
        views.append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data), **({'target': target} if target else {})})
        binary.extend(data)
        return len(views)-1

    def accessor(arr, kind, ctype, target):
        a = {'bufferView': view(arr.tobytes(), target), 'componentType': ctype, 'count': len(arr), 'type': kind}
        if kind == 'VEC3':
            a['min'], a['max'] = arr.min(0).tolist(), arr.max(0).tolist()
        accessors.append(a)
        return len(accessors)-1
    meshes, nodes, total, lo, hi = [], [], 0, None, None
    names = ROOTS or ['hull']
    for name, (pos, nrm, uvs, idx, tan, bones) in zip(names, arrays(obj)):
        if not len(idx): continue
        attrs = {'POSITION': accessor(pos.astype(np.float32), 'VEC3', 5126, 34962),
                 'NORMAL': accessor(nrm.astype(np.float32), 'VEC3', 5126, 34962),
                 'TEXCOORD_0': accessor(uvs.astype(np.float32), 'VEC2', 5126, 34962)}
        if tan is not None:
            attrs['TANGENT'] = accessor(tan.astype(np.float32), 'VEC4', 5126, 34962)
        if bones is not None:
            attrs['JOINTS_0'] = accessor(bones[0].astype(np.uint16), 'VEC4', 5123, 34962)
            attrs['WEIGHTS_0'] = accessor(bones[1].astype(np.float32), 'VEC4', 5126, 34962)
        ind = accessor(idx, 'SCALAR', 5125, 34963)
        meshes.append({'name': name, 'primitives': [{'attributes': attrs, 'indices': ind, 'material': 0}]})
        nodes.append({'name': name, 'mesh': len(meshes)-1, **({'skin': 0} if bones is not None else {})})
        total += len(idx)//3
        lo = pos.min(0) if lo is None else np.minimum(lo, pos.min(0))
        hi = pos.max(0) if hi is None else np.maximum(hi, pos.max(0))
    for name, p in POINTS:
        nodes.append({'name': name, 'translation': [float(c) for c in FRAME(p)]})
    skins = []
    if JOINTS and any('skin' in n for n in nodes):
        # The joints as nodes at their rest frames, and the skin: their
        # inverse rest frames (column-major, as glTF and the engine read).
        first = len(nodes)
        for name, rest in JOINTS:
            nodes.append({'name': name, 'matrix': [float(c) for c in np.asarray(rest, np.float64).T.ravel()]})
        inverse = np.stack([np.linalg.inv(np.asarray(rest, np.float64)).T for _, rest in JOINTS]).astype(np.float32)
        skins = [{'joints': list(range(first, first+len(JOINTS))), 'inverseBindMatrices': accessor(inverse.reshape(-1, 16), 'MAT4', 5126, None)}]
    images = [{'bufferView': view(albedo), 'mimeType': 'image/jpeg'}, {'bufferView': view(glow), 'mimeType': 'image/png'}]
    mat = {'name': material, 'pbrMetallicRoughness': {'baseColorTexture': {'index': 0},
           'metallicFactor': 0.0, 'roughnessFactor': 0.8}, 'emissiveTexture': {'index': 1}, 'emissiveFactor': [1.0, 1.0, 1.0]}
    if surface is not None:
        # The occlusion shares the metallic-roughness texture's red channel.
        images += [{'bufferView': view(surface), 'mimeType': 'image/jpeg'}, {'bufferView': view(normal), 'mimeType': 'image/jpeg'}]
        mat['pbrMetallicRoughness'].update({'metallicFactor': 1.0, 'roughnessFactor': 1.0, 'metallicRoughnessTexture': {'index': 2}})
        mat['occlusionTexture'] = {'index': 2}
        mat['normalTexture'] = {'index': 3}
    doc = {'asset': {'version': '2.0', 'generator': generator}, 'scene': 0,
           'scenes': [{'nodes': list(range(len(nodes)))}], 'nodes': nodes, 'meshes': meshes,
           'materials': [mat], **({'skins': skins} if skins else {}),
           'textures': [{'source': i} for i in range(len(images))], 'images': images, 'accessors': accessors,
           'bufferViews': views, 'buffers': [{'byteLength': len(binary)}]}
    js = __import__('json').dumps(doc, separators=(',', ':')).encode()
    js += b' '*(-len(js) % 4)
    while len(binary) % 4:
        binary.append(0)
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(struct.pack('<4sII', b'glTF', 2, 12+8+len(js)+8+len(binary)))
        f.write(struct.pack('<I4s', len(js), b'JSON')+js)
        f.write(struct.pack('<I4s', len(binary), b'BIN\0')+bytes(binary))
    print(f'wrote {path}: {total} triangles, {len(nodes)} roots ({", ".join(n["name"] for n in nodes)}), '
          f'bounds {lo.round(2).tolist()} .. {hi.round(2).tolist()}, {os.path.getsize(path)//1024} KiB')


def setup():
    """A fresh Blender scene and the materials (call before making solids)."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.view_settings.view_transform = 'Standard'
    bpy.context.scene.view_settings.look = 'None'
    for n in NAMES:
        m = bpy.data.materials.new(n)
        m.use_nodes = True
        MATERIALS[n] = m


MATERIALS = {}


def texel_scale(mesh):
    """Scale each face's texture coordinates by its solid's texel factor
    (in edit mode, before packing, which keeps islands' relative sizes)."""
    bpy.ops.object.mode_set(mode='OBJECT')
    texel = np.empty(len(mesh.polygons), np.float32)
    mesh.attributes['texel'].data.foreach_get('value', texel)
    if np.all(texel == 1.0):
        bpy.ops.object.mode_set(mode='EDIT')
        return
    start = np.empty(len(mesh.polygons), np.int64)
    total = np.empty(len(mesh.polygons), np.int64)
    mesh.polygons.foreach_get('loop_start', start)
    mesh.polygons.foreach_get('loop_total', total)
    per_loop = np.repeat(texel, total)
    order = np.repeat(start, total)+(np.arange(total.sum())-np.repeat(np.cumsum(total)-total, total))
    scale = np.empty(len(mesh.loops), np.float32)
    scale[order] = per_loop
    uv = np.empty(len(mesh.loops)*2, np.float32)
    mesh.uv_layers.active.data.foreach_get('uv', uv)
    mesh.uv_layers.active.data.foreach_set('uv', (uv.reshape(-1, 2)*scale[:, None]).ravel())
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.select_all(action='SELECT')


def spread(mesh, step):
    """Move each root's vertices `step` times its index along X (Blender's
    frame); a negative step moves them back."""
    if not step:
        return
    roots = np.empty(len(mesh.polygons), np.int64)
    mesh.attributes['root'].data.foreach_get('value', roots)
    vroot = np.zeros(len(mesh.vertices), np.int64)
    for poly, r in zip(mesh.polygons, roots):
        for v in poly.vertices: vroot[v] = r
    co = np.empty(len(mesh.vertices)*3, np.float32)
    mesh.vertices.foreach_get('co', co)
    co = co.reshape(-1, 3)
    co[:, 0] += vroot*step
    mesh.vertices.foreach_set('co', co.ravel())
    mesh.update()


def finish(out, tex, cutters=None, generator='PeakRunner scripts/models/hardsurface.py', material='structure'):
    """Every solid made so far into Blender (cut by `cutters`: {solid:
    cutter solid}), chamfered, shaded, joined, laid out, baked and written to
    `out` with a `tex` texture."""
    global TEX
    TEX = tex
    cutters = cutters or {}
    cut_objects = {id(s): make_object(c) for s, c in cutters.items()}
    objects = [make_object(s, cut_objects.get(id(s))) for s in SOLIDS]
    for o in cut_objects.values():
        bpy.data.objects.remove(o)
    bpy.ops.object.select_all(action='DESELECT')
    for o in objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    ship = bpy.context.view_layer.objects.active
    ship.name = 'structure'
    print(f'{len(SOLIDS)} parts, {sum(len(p.vertices)-2 for p in ship.data.polygons)} triangles')
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.0, area_weight=0.0, scale_to_bounds=False)
    texel_scale(ship.data)
    bpy.ops.uv.pack_islands(**PACK)
    bpy.ops.object.mode_set(mode='OBJECT')
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    try:
        prefs = bpy.context.preferences.addons['cycles'].preferences
        prefs.compute_device_type = 'METAL'
        prefs.get_devices()
        for d in prefs.devices:
            d.use = True
        scene.cycles.device = 'GPU'
    except Exception:
        scene.cycles.device = 'CPU'
    scene.world = bpy.data.worlds.new('world')
    scene.world.light_settings.distance = 3.0
    spread(ship.data, SPREAD)
    if PBR:
        # Tangents need triangles (and the same ones the file is written in).
        tri = ship.modifiers.new('triangulate', 'TRIANGULATE')
        tri.keep_custom_normals = True
        bpy.context.view_layer.objects.active = ship
        bpy.ops.object.modifier_apply(modifier=tri.name)
    kinds = ('color', 'glow', 'team', 'ao') + (('surface', 'normal') if PBR else ())
    baked = dict(zip(kinds, (bake(ship, k) for k in kinds)))
    spread(ship.data, -SPREAD)
    color, glow, team, ao = (baked[k] for k in ('color', 'glow', 'team', 'ao'))
    team = team[..., 0] > .5
    lit = glow.max(-1) > .02
    if PBR:
        # The occlusion goes to its own channel; the albedo keeps only a
        # trace of it in its crevices.
        albedo = np.where(lit[..., None], color, color*(.82+.18*ao[..., :1]))
        surface = baked['surface'].copy()
        surface[..., 0] = ao[..., 0]
        maps = dict(surface=save_data('structure-surface', surface, 'JPEG'),
                    normal=save_data('structure-normal', baked['normal'], 'JPEG'))
    else:
        shade = .38+.62*ao[..., :1]
        albedo = np.where(lit[..., None], color, color*shade)
        maps = {}
    write_glb(out, ship, save('structure-albedo', albedo, np.ones(team.shape), 'JPEG'),
              save('structure-glow', np.where(lit[..., None], glow, 0), team.astype(np.float32), 'PNG'), generator, material, **maps)
