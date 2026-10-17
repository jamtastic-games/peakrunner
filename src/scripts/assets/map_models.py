"""Authored models in a map pack (docs/map-pipeline.md, "Authored models").

A map may carry `models.glb`: binary glTF whose scene roots are named for the
structures they dress, drawn by the game where `map.json`'s `models` list
places them (render only; collision stays procedural). The source models are
built by the model pipeline (`assets/models/structures/<name>/model.glb`,
docs/model-pipeline.md); the map builder reads them here, cuts away what
would hide its rooms, doors and hatches (`clip`), and encodes the map's file
(`encode`).

Only what the model pipeline writes is read: mesh nodes with float
positions, normals and texture coordinates, 32-bit indices, one material
with a base color and an emissive texture. Output is deterministic, so map
rebuilds stay byte-identical.
"""
import json
import struct
from pathlib import Path

import numpy as np


class Mesh:
    """Triangles with per-vertex position, normal and texture coordinate,
    in the model's frame, and the source file's material and images."""

    def __init__(self, pos, nrm, uv, tri, material, images):
        self.pos, self.nrm, self.uv, self.tri = pos, nrm, uv, tri
        self.material, self.images = material, images

    def triangles(self):
        return self.pos[self.tri]


def read(path, root=None):
    """The model file at `path` as one Mesh (every mesh node, or only the
    node named `root`; transforms applied by translation only, as the
    pipeline writes them)."""
    data = Path(path).read_bytes()
    if data[:4] != b'glTF':
        raise ValueError(f'{path}: not binary glTF')
    jlen = struct.unpack_from('<I', data, 12)[0]
    doc = json.loads(data[20:20+jlen])
    blen = struct.unpack_from('<I', data, 20+jlen)[0]
    binary = data[28+jlen:28+jlen+blen]

    def view(i):
        v = doc['bufferViews'][i]
        off = v.get('byteOffset', 0)
        return binary[off:off+v['byteLength']]

    def accessor(i, n):
        a = doc['accessors'][i]
        kind = {5126: np.float32, 5125: np.uint32, 5123: np.uint16}[a['componentType']]
        arr = np.frombuffer(view(a['bufferView']), kind, offset=a.get('byteOffset', 0), count=a['count']*n)
        return arr.reshape(-1, n) if n > 1 else arr

    pos, nrm, uv, tri = [], [], [], []
    base = 0
    for node in doc['nodes']:
        if 'mesh' not in node or (root is not None and node.get('name') != root):
            continue
        if any(k in node for k in ('rotation', 'scale', 'matrix')):
            raise ValueError(f'{path}: only translated nodes are read')
        at = np.array(node.get('translation', [0, 0, 0]), np.float64)
        for prim in doc['meshes'][node['mesh']]['primitives']:
            p = accessor(prim['attributes']['POSITION'], 3).astype(np.float64)+at
            pos.append(p)
            nrm.append(accessor(prim['attributes']['NORMAL'], 3).astype(np.float64))
            uv.append(accessor(prim['attributes']['TEXCOORD_0'], 2).astype(np.float64))
            tri.append(accessor(prim['indices'], 1).astype(np.int64).reshape(-1, 3)+base)
            base += len(p)
    if len(doc.get('materials', [])) != 1:
        raise ValueError(f'{path}: one material expected')
    images = [(view(img['bufferView']), img['mimeType']) for img in doc['images']]
    material = doc['materials'][0]
    textures = [doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]['source'],
                doc['textures'][material['emissiveTexture']['index']]['source']]
    return Mesh(np.concatenate(pos), np.concatenate(nrm), np.concatenate(uv), np.concatenate(tri),
                material['name'], [images[i] for i in textures])


def _split(poly, axis, value, keep_below):
    """Split a polygon (a list of vertex rows: x y z nx ny nz u v) by the
    plane `axis` = `value`: (the part on the kept side, the rest)."""
    inside, outside = [], []
    n = len(poly)
    for k in range(n):
        a, b = poly[k], poly[(k+1) % n]
        da, db = a[axis]-value, b[axis]-value
        a_in = da < 0 if keep_below else da > 0
        b_in = db < 0 if keep_below else db > 0
        (inside if a_in else outside).append(a)
        if a_in != b_in and da != db:
            t = da/(da-db)
            m = a+(b-a)*t
            m[axis] = value
            inside.append(m)
            outside.append(m)
    return inside, outside


def clip(mesh, boxes):
    """`mesh` with everything inside each box ((x0, y0, z0), (x1, y1, z1),
    the model's frame) cut away; triangles crossing a box's faces are split
    there, so cuts have clean straight edges."""
    rows = np.concatenate([mesh.pos, mesh.nrm, mesh.uv], 1)
    tris = rows[mesh.tri]                       # (n, 3, 8)
    for lo, hi in boxes:
        lo, hi = np.asarray(lo, np.float64), np.asarray(hi, np.float64)
        p = tris[:, :, :3]
        tmin, tmax = p.min(1), p.max(1)
        clear = np.any(tmax <= lo, 1) | np.any(tmin >= hi, 1)
        within = np.all(tmin >= lo, 1) & np.all(tmax <= hi, 1)
        cross = ~clear & ~within
        out = [tris[clear]]
        pieces = []
        for t in tris[cross]:
            poly = [row.copy() for row in t]
            # Peel off what lies outside each face in turn; what's left at
            # the end is inside the box and is dropped.
            for axis in range(3):
                for value, keep_below in ((lo[axis], False), (hi[axis], True)):
                    if len(poly) < 3:
                        break
                    poly, rest = _split(poly, axis, value, keep_below)
                    if len(rest) >= 3:
                        pieces.extend([rest[0], rest[i], rest[i+1]] for i in range(1, len(rest)-1))
        if pieces:
            out.append(np.array(pieces, np.float64))
        tris = np.concatenate(out) if len(out) > 1 else out[0]
    # Drop slivers the splits leave behind.
    p = tris[:, :, :3]
    area = np.linalg.norm(np.cross(p[:, 1]-p[:, 0], p[:, 2]-p[:, 0]), axis=1)
    tris = tris[area > 1e-8]
    flat = tris.reshape(-1, 8)
    nrm = flat[:, 3:6]
    nrm = nrm/np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
    return Mesh(flat[:, :3], nrm, flat[:, 6:8], np.arange(len(flat), dtype=np.int64).reshape(-1, 3),
                mesh.material, mesh.images)


def weld(mesh):
    """Merge vertices with identical position, normal and texture
    coordinate (as written to 1e-5)."""
    rows = np.concatenate([mesh.pos, mesh.nrm, mesh.uv], 1)
    key = np.round(rows*1e5).astype(np.int64)
    _, first, inverse = np.unique(key, axis=0, return_index=True, return_inverse=True)
    return Mesh(mesh.pos[first], mesh.nrm[first], mesh.uv[first], inverse.ravel()[mesh.tri], mesh.material, mesh.images)


def encode(roots):
    """`roots` ([(name, Mesh)], every Mesh with the same images) as one
    binary glTF (bytes): a scene root per name holding its mesh."""
    binary = bytearray()
    views, accessors, meshes, nodes = [], [], [], []

    def add_view(data, target=None):
        while len(binary) % 4:
            binary.append(0)
        views.append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data), **({'target': target} if target else {})})
        binary.extend(data)
        return len(views)-1

    def add_accessor(arr, kind, ctype, target):
        a = {'bufferView': add_view(arr.tobytes(), target), 'componentType': ctype, 'count': len(arr), 'type': kind}
        if kind == 'VEC3':
            a['min'], a['max'] = [float(v) for v in arr.min(0)], [float(v) for v in arr.max(0)]
        accessors.append(a)
        return len(accessors)-1

    images = roots[0][1].images
    for name, mesh in roots:
        if mesh.images != images:
            raise ValueError('every model in a map file shares its textures')
        m = weld(mesh)
        attrs = {'POSITION': add_accessor(m.pos.astype(np.float32), 'VEC3', 5126, 34962),
                 'NORMAL': add_accessor(m.nrm.astype(np.float32), 'VEC3', 5126, 34962),
                 'TEXCOORD_0': add_accessor(m.uv.astype(np.float32), 'VEC2', 5126, 34962)}
        meshes.append({'name': name, 'primitives': [{'attributes': attrs, 'material': 0,
                       'indices': add_accessor(m.tri.astype(np.uint32).ravel(), 'SCALAR', 5125, 34963)}]})
        nodes.append({'name': name, 'mesh': len(meshes)-1})
    image_list = [{'bufferView': add_view(data), 'mimeType': mime} for data, mime in images]
    doc = {'asset': {'version': '2.0', 'generator': 'PeakRunner scripts/assets/map_models.py'}, 'scene': 0,
           'scenes': [{'nodes': list(range(len(nodes)))}], 'nodes': nodes, 'meshes': meshes,
           'materials': [{'name': roots[0][1].material,
                          'pbrMetallicRoughness': {'baseColorTexture': {'index': 0}, 'metallicFactor': 0.0, 'roughnessFactor': 0.8},
                          'emissiveTexture': {'index': 1}, 'emissiveFactor': [1.0, 1.0, 1.0]}],
           'textures': [{'source': 0}, {'source': 1}], 'images': image_list,
           'accessors': accessors, 'bufferViews': views, 'buffers': [{'byteLength': len(binary)}]}
    js = json.dumps(doc, separators=(',', ':')).encode()
    js += b' '*(-len(js) % 4)
    while len(binary) % 4:
        binary.append(0)
    return (struct.pack('<4sII', b'glTF', 2, 12+8+len(js)+8+len(binary))
            + struct.pack('<I4s', len(js), b'JSON')+js
            + struct.pack('<I4s', len(binary), b'BIN\0')+bytes(binary))


def hits(mesh, origin, direction):
    """Distances along the ray from `origin` in `direction` (a unit vector)
    to every crossing of the model's surface, nearest first."""
    t = mesh.triangles()
    o, d = np.asarray(origin, np.float64), np.asarray(direction, np.float64)
    e1, e2 = t[:, 1]-t[:, 0], t[:, 2]-t[:, 0]
    p = np.cross(d, e2)
    det = (e1*p).sum(1)
    ok = np.abs(det) > 1e-12
    inv = np.where(ok, 1/np.where(ok, det, 1), 0)
    s = o-t[:, 0]
    u = (s*p).sum(1)*inv
    q = np.cross(s, e1)
    v = (q*d).sum(1)*inv
    dist = (e2*q).sum(1)*inv
    m = ok & (u >= 0) & (v >= 0) & (u+v <= 1) & (dist > 0)
    return np.sort(dist[m])


AXES = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))


def enclosed(mesh, point, reach=60.0):
    """Whether the model's surface lies on every side of `point` (within
    `reach` along each axis): what the hull's skin hides from outside."""
    return all(len(h) and h[0] <= reach for h in (hits(mesh, point, d) for d in AXES))


def surface_heights(mesh, xz, below=None):
    """For each (x, z), the highest point of the model's surface over it
    (below `below` if given), or NaN where there is none."""
    t = mesh.triangles()
    a, b, c = t[:, 0], t[:, 1], t[:, 2]
    out = []
    for x, z in np.asarray(xz, np.float64):
        def side(u, v):
            return (v[:, 0]-u[:, 0])*(z-u[:, 2])-(v[:, 2]-u[:, 2])*(x-u[:, 0])
        s1, s2, s3 = side(a, b), side(b, c), side(c, a)
        hit = ((s1 >= 0) & (s2 >= 0) & (s3 >= 0)) | ((s1 <= 0) & (s2 <= 0) & (s3 <= 0))
        n = np.cross(b[hit]-a[hit], c[hit]-a[hit])
        ok = np.abs(n[:, 1]) > 1e-12
        y = a[hit][ok, 1]-((x-a[hit][ok, 0])*n[ok, 0]+(z-a[hit][ok, 2])*n[ok, 2])/n[ok, 1]
        if below is not None:
            y = y[y < below]
        out.append(float(y.max()) if len(y) else float('nan'))
    return np.array(out)


def sections(tris, stations, centre_y, angles, keep=None):
    """The model's outer cross-sections across Z: for each station z, the
    farthest crossing of its surface along each of `angles` (radians from
    straight up, turning toward +X; in order round the circle) in the plane
    z = station, from (0, centre_y(z)). `tris` are the model's triangles (n, 3, 3) in the frame the
    stations run along; `keep(x, y, z)` (arrays) may reject crossings, for
    parts given their own collision. A direction with no crossing takes the
    distance of its nearest neighbours. Returns (stations, angles, 2) points."""
    tris = np.asarray(tris, np.float64)
    zmin, zmax = tris[:, :, 2].min(1), tris[:, :, 2].max(1)
    angles = np.asarray(angles, np.float64)
    rays = len(angles)
    dirs = np.stack([np.sin(angles), np.cos(angles)], 1)
    out = np.empty((len(stations), rays, 2))
    for i, z in enumerate(stations):
        cy = centre_y(z)
        t = tris[(zmin < z) & (zmax > z)]
        segs = []
        for a, b in ((0, 1), (1, 2), (2, 0)):
            p, q = t[:, a], t[:, b]
            dz = q[:, 2]-p[:, 2]
            with np.errstate(divide='ignore', invalid='ignore'):
                s = (z-p[:, 2])/dz
            ok = (dz != 0) & (s >= 0) & (s <= 1)
            segs.append(np.where(ok[:, None], p[:, :2]+s[:, None]*(q[:, :2]-p[:, :2]), np.nan))
        pts = np.stack(segs, 1)                 # (n, 3, 2): two finite points per crossing triangle
        good = ~np.isnan(pts[:, :, 0])
        two = good.sum(1) >= 2
        first = np.argmax(good, 1)
        second = 2-np.argmax(good[:, ::-1], 1)
        idx = np.arange(len(pts))
        a, b = pts[idx, first][two], pts[idx, second][two]
        e = b-a
        radius = np.full(rays, np.nan)
        for k, d in enumerate(dirs):
            o = np.array([0.0, cy])
            denom = d[0]*e[:, 1]-d[1]*e[:, 0]
            w = a-o
            with np.errstate(divide='ignore', invalid='ignore'):
                dist = (w[:, 0]*e[:, 1]-w[:, 1]*e[:, 0])/denom
                u = (w[:, 0]*d[1]-w[:, 1]*d[0])/denom
            hit = (denom != 0) & (dist > 0) & (u >= 0) & (u <= 1)
            if keep is not None and hit.any():
                hx, hy = o[0]+dist*d[0], o[1]+dist*d[1]
                hit &= keep(hx, hy, np.full(len(hx), z))
            if hit.any(): radius[k] = dist[hit].max()
        missing = np.isnan(radius)
        if missing.all(): raise ValueError(f'no surface at z={z}')
        if missing.any():
            known = np.flatnonzero(~missing)
            for k in np.flatnonzero(missing):
                gap = (known-k) % rays
                after, before = known[np.argmin(gap)], known[np.argmax(gap)]
                ga, gb = (after-k) % rays, (k-before) % rays
                radius[k] = (radius[after]*gb+radius[before]*ga)/(ga+gb)
        out[i] = np.array([0.0, cy])+dirs*radius[:, None]
    return out
