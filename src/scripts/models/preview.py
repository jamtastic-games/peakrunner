"""Render preview views of a model file (glTF) with Blender, headless:

    blender -b -P scripts/models/preview.py -- MODEL.glb OUT_PREFIX [--views fp,side,top,three]

Each view is a 1024x640 PNG on a neutral gray background with soft studio
light, framed on the model's bounds. Prints the triangle count, the bounds
and the textures' sizes.
"""
import math
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
src, prefix = args[0], args[1]
views = ["side", "three", "top", "front"]
if "--views" in args:
    views = args[args.index("--views") + 1].split(",")

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)
meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
tris = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in meshes)
lo = Vector((1e9, 1e9, 1e9))
hi = -lo
for o in meshes:
    for c in o.bound_box:
        w = o.matrix_world @ Vector(c)
        lo = Vector(map(min, lo, w))
        hi = Vector(map(max, hi, w))
print(f"triangles {tris}; bounds {tuple(round(x, 3) for x in lo)} .. {tuple(round(x, 3) for x in hi)}")
for img in bpy.data.images:
    print(f"image {img.name}: {img.size[0]}x{img.size[1]}")
for o in bpy.context.scene.objects:
    if o.type == "EMPTY":
        print(f"point {o.name}: {tuple(round(x, 3) for x in o.matrix_world.translation)}")

scene = bpy.context.scene
scene.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items] else "BLENDER_EEVEE"
scene.render.resolution_x, scene.render.resolution_y = 1024, 640
scene.world = bpy.data.worlds.new("bg")
scene.world.use_nodes = True
scene.world.node_tree.nodes["Background"].inputs[0].default_value = (0.42, 0.42, 0.44, 1)
scene.world.node_tree.nodes["Background"].inputs[1].default_value = 0.6
center = (lo + hi) / 2
size = max((hi - lo).length, 0.01)
for name, rot, energy in [("key", (50, 0, 35), 4.0), ("fill", (60, 0, -120), 1.5), ("rim", (20, 0, 180), 2.5)]:
    light = bpy.data.lights.new(name, "SUN")
    light.energy = energy
    light.angle = math.radians(8)
    obj = bpy.data.objects.new(name, light)
    obj.rotation_euler = [math.radians(a) for a in rot]
    scene.collection.objects.link(obj)
cam_data = bpy.data.cameras.new("cam")
cam_data.lens_unit = "FOV"
cam_data.angle = math.radians(40)
cam = bpy.data.objects.new("cam", cam_data)
scene.collection.objects.link(cam)
scene.camera = cam
# Blender is Z-up; the glTF importer turns the file's +Y up into +Z and its
# -Z forward into +Y.
directions = {
    "side": Vector((-1, 0, 0)),
    "three": Vector((-0.8, -0.55, 0.45)),
    "top": Vector((0, 0.001, 1)),
    "front": Vector((0, 1, 0.15)),
    "back": Vector((0, -1, 0.15)),
    "fp": Vector((0.45, -1.0, 0.45)),
}
for view in views:
    d = directions[view].normalized()
    cam.location = center + d * size * 1.5
    cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = f"{prefix}-{view}.png"
    bpy.ops.render.render(write_still=True)
print("rendered", ", ".join(views))
