"""Turn generated models into game models (docs/model-pipeline.md), headless:

    blender -b -P scripts/models/process.py -- SPEC.json SOURCES_DIR OUT.glb

SPEC (a model's `source.json`) names one raw model per part, in SOURCES_DIR,
and says how to fit it:

    {
      "team": "magenta" | "red" | "none",   which texels are team paint
      "parts": {
        "<name>": {
          "raw": "<file in SOURCES_DIR>" | "mirror": "<other part>",
          "orient": [[...], [...], [...]],   3x3, applied in the game's axes (the
                                             generator faces things toward +Z), or
          "view": "left" | "front" | "back", the concept's view: the model is
                                             turned to match its silhouette
          "concept": "<image in SOURCES_DIR>",  (with "view")
          "fit": {"min": [x, y, z], "max": [x, y, z]},   box it's scaled into
          "uniform": true,                   keep proportions (fit the tightest side,
          "scale_by": "x",                    or this one)
          "align": {"y": "min"},             against a side of the box, not centered
          "solidify": 4,                     thicken single sheets first (voxels)
          "fill": false,                     keep openings (a helmet's face) open
          "at": [x, y, z],                   the part node's place (preview only)
          "triangles": 8000,
          "texture": 1024
        }
      },
      "points": {"<name>": [u, v, w]},       fractions of the first part's bounds
      "origin": "<point>"                    moved to (0, 0, 0)
    }

Game axes: meters, +Y up, -Z forward, +X right. The output is a binary glTF
with, per part, a node holding a mesh in its own frame, a JPEG base color and
a PNG glow texture whose alpha is the team mask; points are empty nodes.
"""
import json
import math
import os
import struct
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

args = sys.argv[sys.argv.index("--") + 1:]
spec_path, sources, out_path = args[0], args[1], args[2]
spec = json.load(open(spec_path))

# Blender's axes from the game's: x, -z, y (the glTF importer's mapping).
TO_BLENDER = Matrix(((1, 0, 0), (0, 0, -1), (0, 1, 0)))
TO_GAME = TO_BLENDER.inverted()


def hsv(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    mx, mn = rgb.max(-1), rgb.min(-1)
    d = mx - mn + 1e-6
    h = np.where(mx == r, ((g - b) / d) % 6, np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4)) * 60
    s = np.where(mx > 0, (mx - mn) / (mx + 1e-6), 0)
    return h, s, mx


def textures(image, size, team, name):
    """A JPEG base color and a PNG glow (alpha: team mask) from `image`."""
    image.scale(size, size)
    px = np.array(image.pixels[:], dtype=np.float32).reshape(size, size, 4)[..., :3]
    h, s, v = hsv(px)
    if team == "magenta":
        mask = (h > 270) & (h < 358) & (s > 0.35) & (v > 0.25)
    elif team == "red":
        mask = ((h > 345) | (h < 18)) & (s > 0.45) & (v > 0.18)
    else:
        mask = np.zeros(h.shape, bool)
    cyan = (h > 165) & (h < 205) & (s > 0.3) & (v > 0.62)
    amber = (h > 22) & (h < 52) & (s > 0.55) & (v > 0.7)
    glow = (cyan | amber) & ~mask
    luma = px @ np.array([0.299, 0.587, 0.114], dtype=np.float32)
    albedo = px.copy()
    if mask.any():
        # Team paint as gray, its median at 0.5: the engine multiplies the
        # team's color by twice this.
        gray = np.clip(luma / max(float(np.median(luma[mask])), 0.05) * 0.5, 0, 1)
        albedo[mask] = gray[mask, None]
    glow_rgb = np.where(glow[..., None], np.clip(px * 1.15, 0, 1), 0)
    out = []
    for kind, rgb, alpha, fmt in [("albedo", albedo, np.ones(mask.shape), "JPEG"), ("glow", glow_rgb, mask.astype(np.float32), "PNG")]:
        img = bpy.data.images.new(f"{name}-{kind}", size, size, alpha=True)
        img.pixels[:] = np.concatenate([rgb, alpha[..., None]], -1).ravel()
        path = os.path.join(bpy.app.tempdir, f"{name}-{kind}.{fmt.lower()}")
        scene = bpy.context.scene
        scene.render.image_settings.file_format = fmt
        scene.render.image_settings.color_mode = "RGB" if fmt == "JPEG" else "RGBA"
        scene.render.image_settings.quality = 88
        scene.render.image_settings.compression = 90
        img.save_render(path, scene=scene)
        out.append((open(path, "rb").read(), "image/jpeg" if fmt == "JPEG" else "image/png"))
    return out


GRID = 96


def splat(xy, depth, rgb, n=GRID):
    """Points (screen right and up) splatted into an n x n picture, scaled
    by their larger extent and centered, the nearest point winning each
    pixel: (mask, colors)."""
    lo, hi = xy.min(0), xy.max(0)
    span = max((hi - lo).max(), 1e-6)
    uv = (xy - (lo + hi) / 2) / span * (n - 4) + n / 2
    ij = np.clip(uv.astype(int), 0, n - 1)
    mask = np.zeros((n, n), bool)
    color = np.zeros((n, n, 3), np.float32)
    order = np.argsort(-depth)  # far first; nearer points overwrite
    rows, cols = n - 1 - ij[order, 1], ij[order, 0]
    mask[rows, cols] = True
    color[rows, cols] = rgb[order]
    # Close the pinholes between splatted points.
    for dy, dx in ((0, 1), (1, 0), (0, -1), (-1, 0)):
        gap = ~mask & np.roll(np.roll(mask, dy, 0), dx, 1)
        color[gap] = np.roll(np.roll(color, dy, 0), dx, 1)[gap]
        mask |= gap
    return mask, color


def concept_picture(path, n=GRID):
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1, :, :3]
    bpy.data.images.remove(img)
    bg = np.median(np.concatenate([px[:6, :6].reshape(-1, 3), px[-6:, -6:].reshape(-1, 3)]), 0)
    ys, xs = np.nonzero(np.abs(px - bg).max(-1) > 0.09)
    step = max(1, len(xs) // 200000)
    ys, xs = ys[::step], xs[::step]
    return splat(np.stack([xs, -ys], 1).astype(np.float32), np.zeros(len(xs)), px[ys, xs], n)


# Screen right and up, and the way to the camera, for each concept view in
# game axes: from the left (camera at -X) the muzzle (-Z) is on the left;
# from the front (camera at -Z) the piece's right (+X) is on the left; from
# behind, on the right.
VIEWS = {"left": ((0, 0, 1), (0, 1, 0), (-1, 0, 0)), "front": ((-1, 0, 0), (0, 1, 0), (0, 0, -1)),
         "back": ((1, 0, 0), (0, 1, 0), (0, 0, 1))}


def rotations():
    """The 24 turns that map axes onto axes."""
    import itertools
    for perm in itertools.permutations(range(3)):
        for signs in itertools.product((1, -1), repeat=3):
            m = np.zeros((3, 3))
            for r, (c, sgn) in enumerate(zip(perm, signs)):
                m[r, c] = sgn
            if np.linalg.det(m) > 0:
                yield m


def surface_samples(obj, count=150000):
    """Points on the surface (game axes) with their base colors."""
    mesh = obj.data
    lv = np.empty(len(mesh.loops), dtype=np.int64)
    mesh.loops.foreach_get("vertex_index", lv)
    co = np.empty(len(mesh.vertices) * 3, dtype=np.float32)
    mesh.vertices.foreach_get("co", co)
    pick = np.arange(0, len(lv), max(1, len(lv) // count))
    pos = co.reshape(-1, 3)[lv[pick]] @ np.array(TO_GAME, dtype=np.float32).T
    rgb = np.full((len(pick), 3), 0.5, np.float32)
    image = base_image(obj)
    if image is not None and mesh.uv_layers.active:
        uv = np.empty(len(mesh.loops) * 2, dtype=np.float32)
        mesh.uv_layers.active.data.foreach_get("uv", uv)
        uv = uv.reshape(-1, 2)[pick]
        small = image.copy()
        small.scale(256, 256)
        px = np.array(small.pixels[:], dtype=np.float32).reshape(256, 256, 4)[..., :3]
        bpy.data.images.remove(small)
        ij = np.clip((uv % 1.0) * 255, 0, 255).astype(int)
        rgb = px[ij[:, 1], ij[:, 0]]
    return pos, rgb


def auto_orient(obj, concept, view):
    """The turn (game axes) whose picture from `view` best matches the
    concept's: outline overlap, and colors where both have something."""
    target_mask, target_color = concept_picture(concept)
    right, up, toward = (np.array(v, dtype=np.float32) for v in VIEWS[view])
    pts, rgb = surface_samples(obj)
    best = None
    for m in rotations():
        q = pts @ m.T.astype(np.float32)
        mask, color = splat(np.stack([q @ right, q @ up], 1), -(q @ toward), rgb)
        both = mask & target_mask
        iou = both.sum() / max((mask | target_mask).sum(), 1)
        likeness = 1 - np.abs(color[both] - target_color[both]).mean() if both.any() else 0
        score = iou * 0.5 + likeness * 0.5
        if best is None or score > best[0]:
            best = (score, iou, likeness, m)
    print(f"orientation: score {best[0]:.2f} (outline {best[1]:.2f}, colors {best[2]:.2f}) with {best[3].astype(int).tolist()}")
    return best[3]


def load_part(name, p):
    """Import, orient, fit and decimate one raw model; returns its object."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.join(sources, p["raw"]))
    meshes = [o for o in bpy.data.objects if o not in before and o.type == "MESH"]
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    for o in [o for o in bpy.data.objects if o not in before and o != obj]:
        bpy.data.objects.remove(o)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    # Orientation in the game's axes, expressed in Blender's.
    # A set orientation wins over matching (a helmet's front and back match
    # its concept about as well as each other).
    if "orient" in p or "view" not in p:
        orient = Matrix(p.get("orient", [[1, 0, 0], [0, 1, 0], [0, 0, 1]]))
    else:
        orient = Matrix(auto_orient(obj, os.path.join(sources, p["concept"]), p["view"]).tolist())
    m = (TO_BLENDER @ orient @ TO_GAME).to_4x4()
    obj.data.transform(m)
    if m.determinant() < 0:
        obj.data.flip_normals()
    obj.data.update()
    # Fit into the box (game axes).
    co = np.empty(len(obj.data.vertices) * 3)
    obj.data.vertices.foreach_get("co", co)
    game = co.reshape(-1, 3) @ np.array(TO_GAME).T
    lo, hi = game.min(0), game.max(0)
    want_lo, want_hi = np.array(p["fit"]["min"]), np.array(p["fit"]["max"])
    scale = (want_hi - want_lo) / np.maximum(hi - lo, 1e-6)
    if p.get("uniform", True):
        scale = np.full(3, scale["xyz".index(p["scale_by"])] if "scale_by" in p else scale.min())
    # Centered in the box, or against a side of it per axis.
    align = p.get("align", {})
    size = (hi - lo) * scale
    place = [{"min": want_lo[k] + size[k] / 2, "max": want_hi[k] - size[k] / 2}.get(align.get("xyz"[k]), (want_lo[k] + want_hi[k]) / 2)
             for k in range(3)]
    game = (game - (lo + hi) / 2) * scale + np.array(place)
    obj.data.vertices.foreach_set("co", (game @ np.array(TO_BLENDER).T).ravel())
    obj.data.update()
    # Weld; the generated surface keeps its look but is rebuilt below.
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    # Some generated surfaces are open or partly inside out, which the
    # remesh below reads as a thin shell: close them and turn them outward.
    open_edges = [e for e in bm.edges if e.is_boundary]
    if open_edges and p.get("fill", True):
        bmesh.ops.holes_fill(bm, edges=open_edges, sides=0)
        print(f"{name}: closed {len(open_edges)} open edges")
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()
    return rebuild(obj, name, p)


def select_only(*objs):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[-1]


def rebuild(hi, name, p):
    """A clean low-poly shell of `hi` within the triangle budget, with its
    own texture layout, and `hi`'s colors baked onto it. Generated meshes
    have doubled inner surfaces and seams everywhere, which stop a plain
    decimation; a voxel remesh makes one closed surface first."""
    lo = hi.copy()
    lo.data = hi.data.copy()
    lo.name = name
    hi.name = name + "-source"
    bpy.context.scene.collection.objects.link(lo)
    while lo.data.uv_layers:
        lo.data.uv_layers.remove(lo.data.uv_layers[0])
    lo.data.materials.clear()
    # Voxels sized for a remesh of about 25 times the budget (from the
    # surface's area; `dimensions` can still hold the size before fitting),
    # coarser again if the decimation then stalls on fine detail.
    area = sum(f.area for f in hi.data.polygons)
    co = np.empty(len(hi.data.vertices) * 3)
    hi.data.vertices.foreach_get("co", co)
    span = float(np.ptp(co.reshape(-1, 3), axis=0).max())
    voxel = min(max(math.sqrt(2 * area / (p["triangles"] * 25)), span / 500), span / 150)
    if "voxels" in p:
        voxel = span / p["voxels"]
    shell = lo.data.copy()
    for attempt in range(4):
        lo.data = shell.copy()
        select_only(lo)
        if p.get("solidify"):
            # A surface with no inside (single sheets): give it thickness
            # first, or the remesh makes paper-thin slabs that won't decimate.
            mod = lo.modifiers.new("solidify", "SOLIDIFY")
            mod.thickness = voxel * p["solidify"]
            mod.offset = -1
            bpy.ops.object.modifier_apply(modifier=mod.name)
        mod = lo.modifiers.new("remesh", "REMESH")
        mod.mode = "VOXEL"
        mod.voxel_size = voxel
        bpy.ops.object.modifier_apply(modifier=mod.name)
        tris = now = sum(len(f.vertices) - 2 for f in lo.data.polygons)
        for _ in range(5):
            if now <= p["triangles"] * 1.03:
                break
            mod = lo.modifiers.new("decimate", "DECIMATE")
            mod.ratio = max(p["triangles"] / now, 0.05)
            mod.use_collapse_triangulate = True
            bpy.ops.object.modifier_apply(modifier=mod.name)
            now = sum(len(f.vertices) - 2 for f in lo.data.polygons)
        if now <= p["triangles"] * 1.3:
            break
        print(f"{name}: decimation stalled at {now} from {tris}; coarser voxels")
        voxel *= 1.5
    # A remesh of a surface that's partly inside out can come out inside
    # out: point every face outward.
    bm = bmesh.new()
    bm.from_mesh(lo.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(lo.data)
    bm.free()
    lo.data.shade_smooth()
    lo.data.set_sharp_from_angle(angle=math.radians(p.get("sharp", 40)))
    print(f"{name}: remeshed to {tris} triangles, decimated to {sum(len(f.vertices) - 2 for f in lo.data.polygons)}")
    # A fresh texture layout.
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.003, scale_to_bounds=True)
    bpy.ops.object.mode_set(mode="OBJECT")
    # The source shows its base color as light, so the bake copies it as is.
    for slot in hi.material_slots:
        tree = slot.material.node_tree if slot.material else None
        tex = next((n for n in tree.nodes if n.type == "TEX_IMAGE" and n.image), None) if tree else None
        out = next((n for n in tree.nodes if n.type == "OUTPUT_MATERIAL"), None) if tree else None
        if tex and out:
            emit = tree.nodes.new("ShaderNodeEmission")
            tree.links.new(tex.outputs["Color"], emit.inputs["Color"])
            tree.links.new(emit.outputs["Emission"], out.inputs["Surface"])
    size = p.get("texture", 1024)
    img = bpy.data.images.new(name + "-baked", size * 2, size * 2, alpha=False)
    img.colorspace_settings.name = "sRGB"
    mat = bpy.data.materials.new(name)
    tree = mat.node_tree
    node = tree.nodes.new("ShaderNodeTexImage")
    node.image = img
    tree.links.new(node.outputs["Color"], tree.nodes["Principled BSDF"].inputs["Base Color"])
    tree.nodes.active = node
    lo.data.materials.append(mat)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 4
    try:
        prefs = bpy.context.preferences.addons["cycles"].preferences
        prefs.compute_device_type = "METAL"
        prefs.get_devices()
        for d in prefs.devices:
            d.use = True
        scene.cycles.device = "GPU"
    except Exception:
        scene.cycles.device = "CPU"
    select_only(hi, lo)
    bpy.ops.object.bake(type="EMIT", use_selected_to_active=True, cage_extrusion=voxel * 4,
                        max_ray_distance=voxel * 12, margin=8)
    bpy.data.objects.remove(hi)
    return lo


def arrays(obj):
    """Positions, normals, texture coordinates (glTF's) and triangles."""
    mesh = obj.data
    mesh.calc_loop_triangles()
    nt = len(mesh.loop_triangles)
    tri = np.empty(nt * 3, dtype=np.int64)
    mesh.loop_triangles.foreach_get("loops", tri)
    lv = np.empty(len(mesh.loops), dtype=np.int64)
    mesh.loops.foreach_get("vertex_index", lv)
    co = np.empty(len(mesh.vertices) * 3, dtype=np.float32)
    mesh.vertices.foreach_get("co", co)
    cn = np.empty(len(mesh.loops) * 3, dtype=np.float32)
    mesh.corner_normals.foreach_get("vector", cn)
    uv = np.zeros(len(mesh.loops) * 2, dtype=np.float32)
    if mesh.uv_layers.active:
        mesh.uv_layers.active.data.foreach_get("uv", uv)
    g = np.array(TO_GAME, dtype=np.float32).T
    pos = co.reshape(-1, 3)[lv] @ g
    nrm = cn.reshape(-1, 3) @ g
    uvs = uv.reshape(-1, 2) * np.array([1, -1], dtype=np.float32) + np.array([0, 1], dtype=np.float32)
    key = np.round(np.concatenate([pos * 1e5, nrm * 1e3, uvs * 1e5], 1)).astype(np.int64)
    _, first, inverse = np.unique(key, axis=0, return_index=True, return_inverse=True)
    return pos[first], nrm[first], uvs[first], inverse.ravel()[tri].astype(np.uint32)


def base_image(obj):
    for slot in obj.material_slots:
        if slot.material and slot.material.node_tree:
            for n in slot.material.node_tree.nodes:
                if n.type == "TEX_IMAGE" and n.image and any(l.to_socket.name == "Base Color" for l in n.outputs[0].links):
                    return n.image
    return None


bpy.ops.wm.read_factory_settings(use_empty=True)
# Saved textures hold the colors as they are, with no view transform.
bpy.context.scene.view_settings.view_transform = "Standard"
bpy.context.scene.view_settings.look = "None"
team = spec.get("team", "none")
objects = {}
for name, p in spec["parts"].items():
    if "raw" in p:
        objects[name] = load_part(name, p)
for name, p in spec["parts"].items():
    if "mirror" in p:
        src = objects[p["mirror"]]
        obj = src.copy()
        obj.data = src.data.copy()
        obj.name = name
        bpy.context.scene.collection.objects.link(obj)
        obj.data.transform(Matrix.Scale(-1, 4, (1, 0, 0)))
        obj.data.flip_normals()
        obj.data.update()
        objects[name] = obj

# Points, as fractions of the first part's bounds, and the origin.
first = next(iter(spec["parts"]))
co = np.empty(len(objects[first].data.vertices) * 3)
objects[first].data.vertices.foreach_get("co", co)
game = co.reshape(-1, 3) @ np.array(TO_GAME).T
lo, hi = game.min(0), game.max(0)
points = {k: lo + np.array(v) * (hi - lo) for k, v in spec.get("points", {}).items()}
shift = -points[spec["origin"]] if spec.get("origin") in points else np.zeros(3)

# Assemble the file.
bin_ = bytearray()
views, accessors, meshes, nodes, materials, textures_, images = [], [], [], [], [], [], []


def view(data, target=None):
    while len(bin_) % 4:
        bin_.append(0)
    views.append({"buffer": 0, "byteOffset": len(bin_), "byteLength": len(data), **({"target": target} if target else {})})
    bin_.extend(data)
    return len(views) - 1


def accessor(arr, kind, ctype, target):
    v = view(arr.tobytes(), target)
    a = {"bufferView": v, "componentType": ctype, "count": len(arr), "type": kind}
    if kind == "VEC3":
        a["min"], a["max"] = arr.min(0).tolist(), arr.max(0).tolist()
    accessors.append(a)
    return len(accessors) - 1


cache = {}
for name, obj in objects.items():
    p = spec["parts"][name]
    pos, nrm, uvs, idx = arrays(obj)
    pos = pos + shift.astype(np.float32) if name == first or "at" not in p else pos
    src_name = p.get("mirror", name)
    if src_name not in cache:
        image = base_image(objects[src_name])
        if image is None:
            raise SystemExit(f"{name}: no base color texture")
        (alb, alb_mime), (glow, glow_mime) = textures(image, p.get("texture", spec["parts"][src_name].get("texture", 1024)), team, src_name)
        for data, mime in [(alb, alb_mime), (glow, glow_mime)]:
            images.append({"bufferView": view(data), "mimeType": mime})
            textures_.append({"source": len(images) - 1})
        materials.append({"name": src_name, "pbrMetallicRoughness": {"baseColorTexture": {"index": len(textures_) - 2},
            "metallicFactor": 0.0, "roughnessFactor": 0.8}, "emissiveTexture": {"index": len(textures_) - 1},
            "emissiveFactor": [1.0, 1.0, 1.0]})
        cache[src_name] = len(materials) - 1
    attrs = {"POSITION": accessor(pos.astype(np.float32), "VEC3", 5126, 34962),
             "NORMAL": accessor(nrm.astype(np.float32), "VEC3", 5126, 34962),
             "TEXCOORD_0": accessor(uvs.astype(np.float32), "VEC2", 5126, 34962)}
    meshes.append({"name": name, "primitives": [{"attributes": attrs, "indices": accessor(idx, "SCALAR", 5125, 34963),
        "material": cache[src_name]}]})
    node = {"name": name, "mesh": len(meshes) - 1}
    if "at" in p:
        node["translation"] = list(map(float, p["at"]))
    nodes.append(node)
    print(f"{name}: {len(idx) // 3} triangles, {len(pos)} vertices")
for k, at in points.items():
    nodes.append({"name": k, "translation": [float(x) for x in at + shift]})
    print(f"point {k}: {[round(float(x), 3) for x in at + shift]}")

doc = {"asset": {"version": "2.0", "generator": "PeakRunner scripts/models/process.py"}, "scene": 0,
       "scenes": [{"nodes": list(range(len(nodes)))}], "nodes": nodes, "meshes": meshes, "materials": materials,
       "textures": textures_, "images": images, "accessors": accessors, "bufferViews": views,
       "buffers": [{"byteLength": len(bin_)}]}
js = json.dumps(doc, separators=(",", ":")).encode()
js += b" " * (-len(js) % 4)
while len(bin_) % 4:
    bin_.append(0)
os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
with open(out_path, "wb") as f:
    f.write(struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(js) + 8 + len(bin_)))
    f.write(struct.pack("<I4s", len(js), b"JSON") + js)
    f.write(struct.pack("<I4s", len(bin_), b"BIN\0") + bytes(bin_))
print(f"wrote {out_path}: {os.path.getsize(out_path) // 1024} KiB")
