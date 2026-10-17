"""Shared assembly for original map packs: terrain sampling, material
overrides, the lightmap bake and the six-file pack write.

Used by build-tower-complex.py and build-cairnhold.py. Output layout matches
what crates/core/src/map_pack.rs validates: six payloads plus map.json with
their SHA-256 digests.
"""
import hashlib
import json
from pathlib import Path

# The renderer's fixed map sun (src/map_scene.rs uniform), so baked shadows on
# the structures agree with the terrain's live lighting.
MAP_SUN = (-0.57735, 0.57735, -0.57735)


def sample_height(grid, x, z, step=8):
    """Bilinear sample of a 256x256 height grid, quantised like height.bin,
    matching the engine's heightfield lookup."""
    fx, fz = min(max(x/step, 0), 255), min(max(z/step, 0), 255)
    x0, z0 = int(fx), int(fz); x1, z1 = min(x0+1, 255), min(z0+1, 255)
    tx, tz = fx-x0, fz-z0
    q = lambda ix, iz: round(float(grid[iz, ix])*32)/32
    a = q(x0, z0)+(q(x1, z0)-q(x0, z0))*tx; b = q(x0, z1)+(q(x1, z1)-q(x0, z1))*tx
    return a+(b-a)*tz


def paint_materials(textures, count, materials, texture, seed, kit):
    """Replace each named kit material's whole mip chain in a level-major
    textures.rgba (every layer at 256, then every layer at 128, ...).
    Replacing only level 0 would leave distant surfaces on the shared map's
    texture."""
    for material in materials:
        layer = kit.MATERIALS.index(material)
        img = texture(material, seed+layer, kit.noise, kit.value_noise)
        side, offset = 256, 0
        while True:
            size = side*side*4
            textures[offset+layer*size:offset+(layer+1)*size] = img
            offset += count*size
            if side == 1: break
            img = kit.downsample(img, side); side //= 2
    return textures


def bake_lightmaps(vertices, textures, count, lamps, kit, zones=()):
    """Bake lightmap pages for the render vertices and append them as new
    texture layers, keeping the level-major mip layout (`zones`: ambient
    zones, `lightmap_bake`).
    Returns (vertex bytes, textures, new texture count, manifest metadata)."""
    import numpy as np
    from assets import lightmap_bake
    baked, pages = lightmap_bake.bake(np.frombuffer(vertices, '<f4'), lamps, MAP_SUN,
                                      kit.MATERIALS.index('light'), count, zones=zones)
    mips = [[page] for page in pages]
    for chain in mips:
        side = 256
        while side > 1: chain.append(kit.downsample(chain[-1], side)); side //= 2
    merged, side, offset = bytearray(), 256, 0
    for level in range(9):
        size = side*side*4
        merged += textures[offset:offset+count*size]
        for chain in mips: merged += chain[level]
        offset += count*size; side //= 2
    meta = dict(pages=len(pages), first_layer=count, texel_m=.5, sun=list(MAP_SUN),
                lamps=len(lamps),
                baker_sha256=hashlib.sha256(Path(lightmap_bake.__file__).read_bytes()).hexdigest())
    return baked.astype('<f4').tobytes(), merged, count+len(pages), meta


def add_props_and_shade(kit, files, manifest, theme, seed, holes, flags, spawn_points, control_points,
                        extra_clear=(), water=None, sun=None):
    """Scatter the theme's props (assets/props.py) over the terrain, append
    them to vertices.bin and collision.bin, then bake the terrain shade map
    (assets/terrain_shade.py) with structures and big props as casters.
    Call after the structure lightmap bake, with files['collision.bin']
    holding only the structures."""
    from assets import props, terrain_shade
    terrain = props.Terrain(files['height.bin'], files['weights.rgba'], holes, water=water)
    protect = props.Protection(files['collision.bin'], flags, spawn_points, control_points, extra_clear)
    verts, solid, casters, summary, instances = props.scatter(kit, theme, seed, terrain, protect)
    structures = files['vertices.bin']
    files['vertices.bin'] = structures+verts
    files['collision.bin'] = files['collision.bin']+solid
    # Render-only props travel as GPU instances (props.Instancer, props.bin).
    if instances: files['props.bin'] = instances
    sun = sun or (manifest.get('look') or {}).get('sun_direction') or MAP_SUN
    files['shade.rg'], shade = terrain_shade.bake(files['height.bin'], structures+casters, sun,
                                                  kit.MATERIALS.index('light'))
    summary['source_sha256'] = source_hash(props.__file__)
    manifest['props'] = summary; manifest['terrain_shade'] = shade
    return summary


def source_hash(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


# How far the smaller face of an exactly overlapping coplanar pair is moved
# forward, so the depth buffer always draws it in front (see resolve_overlaps).
OVERLAP_NUDGE = 0.01


def resolve_overlaps(vertices, collision):
    """Render vertices (bytes) with every visible overlapping, coplanar,
    same-facing pair of triangles separated: the smaller moves OVERLAP_NUDGE
    forward along its normal. Two such faces carry different baked lighting
    and the depth buffer can't order them, so they flickered. Nothing is
    removed (a smaller face may be an intended marking), lighting and
    collision are unchanged, and a pack without overlaps is left identical."""
    from assets import surface_checks
    import numpy as np
    v = np.frombuffer(vertices, '<f4').astype(np.float64).reshape(-1, 3, 12).copy()
    moved = False
    # A face nudged in front of one partner can land on another nudged face,
    # so repeat until no pair is left (each pass moves the smaller again).
    for _ in range(4):
        pairs = surface_checks.overlap_pairs(v.reshape(-1).astype('<f4'), np.frombuffer(collision, '<f4'))
        if not pairs: break
        p = v[:, :, :3]
        area = np.linalg.norm(np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]), axis=1)
        for k in sorted({j if area[j] <= area[i] else i for i, j, *_ in pairs}):
            n = np.cross(p[k, 1]-p[k, 0], p[k, 2]-p[k, 0])
            v[k, :, :3] += n/np.linalg.norm(n)*OVERLAP_NUDGE
        moved = True
    return v.reshape(-1).astype('<f4').tobytes() if moved else vertices


def write_pack(output, files, manifest):
    """Separate overlapping render faces, record every payload digest in the
    manifest, then write the pack."""
    if 'vertices.bin' in files and 'collision.bin' in files:
        files['vertices.bin'] = resolve_overlaps(files['vertices.bin'], files['collision.bin'])
    manifest['files'] = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}
    output.mkdir(parents=True)
    for name, data in files.items(): (output/name).write_bytes(data)
    (output/'map.json').write_text(json.dumps(manifest, indent=2)+'\n')
