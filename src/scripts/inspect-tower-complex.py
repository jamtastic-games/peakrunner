#!/usr/bin/env python3
"""Floor-plan/profile cuts for our own compiled tower-complex pack. Schematic
collision-geometry cuts,
not a textured render -- use alongside a real launch_smoke.rs GPU screenshot,
not instead of one. Not a private diagnostic (this is our original asset);
output can go anywhere.
"""
import argparse
import json
import math
from pathlib import Path
import struct
import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent


def load_triangles(pack_dir, origin, yaw_deg):
    raw = (pack_dir / 'collision.bin').read_bytes()
    floats = struct.unpack(f'<{len(raw)//4}f', raw)
    pts = np.array(floats).reshape(-1, 3)
    ox, oy, oz = origin
    yaw = math.radians(yaw_deg)
    c, s = math.cos(yaw), math.sin(yaw)
    dx, dy, dz = pts[:, 0]-ox, pts[:, 1]-oy, pts[:, 2]-oz
    lx = dx*c - dz*s
    lz = dx*s + dz*c
    local = np.stack([lx, lz, dy], axis=1)
    tris = local.reshape(-1, 3, 3)
    centroid = tris.mean(axis=1)
    # Wide enough for the tower (|x|<=12), the rear tunnels/pods (z to 29)
    # and the front turret-bridge pods (z to -29).
    near = (np.abs(centroid[:, 0]) < 20) & (np.abs(centroid[:, 1]) < 20) & (centroid[:, 2] > -32) & (centroid[:, 2] < 32)
    return tris[near]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--pack', type=Path, default=ROOT/'assets/maps/tower-complex')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    spec = json.loads((ROOT/'maps/tower-complex.json').read_text())
    base = spec['bases'][0]
    tris = load_triangles(args.pack, base['position'], base['yaw'])
    print(f'{len(tris)} collision triangles near local origin')
    flat = tris.reshape(-1, 3)
    print('local bounds (x,y,height)', flat.min(axis=0), flat.max(axis=0))

    args.output.mkdir(parents=True, exist_ok=True)
    # L1=0, L2=7, L3=14, ROOF=21; turret pods and rear pods sit at L1 (0-5).
    heights = [0, 3.5, 7, 10.5, 14, 17.5, 21]
    cols, sheet_rows = 3, 3
    per_sheet = cols*sheet_rows
    for sheet_i in range(0, len(heights), per_sheet):
        batch = heights[sheet_i:sheet_i+per_sheet]
        sheet = Image.new('RGB', (1500, 600*((len(batch)+cols-1)//cols)), '#101820')
        draw = ImageDraw.Draw(sheet)
        for index, h in enumerate(batch):
            ox = (index % cols)*500
            oy = (index//cols)*600

            def p(v, ox=ox, oy=oy):
                return (ox+250+v[0]*7, oy+430-v[1]*7)
            draw.text((ox+15, oy+15), f'Local height={h} m; wall cut at +2 m', fill='white')
            for tri in tris:
                n = np.cross(tri[1]-tri[0], tri[2]-tri[0])
                norm = np.linalg.norm(n)
                if norm < 1e-9:
                    continue
                n = n/norm
                if abs(n[2]) > 0.5 and tri[:, 2].min() >= h-.25 and tri[:, 2].max() <= h+.25:
                    draw.polygon([p(v) for v in tri], fill='#405971')
                cut = h+2
                pts2 = []
                for a, b in zip(tri, np.roll(tri, -1, axis=0)):
                    if (a[2] <= cut < b[2]) or (b[2] <= cut < a[2]):
                        t = (cut-a[2])/(b[2]-a[2])
                        pts2.append(a+(b-a)*t)
                if len(pts2) == 2:
                    draw.line([p(v) for v in pts2], fill='#ffe19b', width=2)
            for value in range(-25, 26, 5):
                draw.text(p((value, 0, 0)), str(value), fill='#6ccbd5')
        sheet.save(args.output/f'floor-sections-{sheet_i}.png')

    side = Image.new('RGB', (1600, 1000), '#101820')
    draw = ImageDraw.Draw(side)
    for index, (axis, cut) in enumerate([(0, 0), (1, 0)]):
        def p(v, index=index, axis=axis):
            horiz = v[1] if axis == 0 else v[0]
            return (index*800+400+horiz*10, 650-v[2]*10)
        draw.text((index*800+20, 20), f'Central section {"X" if axis==0 else "Y"}=0', fill='white')
        for tri in tris:
            pts2 = []
            for a, b in zip(tri, np.roll(tri, -1, axis=0)):
                if (a[axis] <= cut+.01 < b[axis]) or (b[axis] <= cut+.01 < a[axis]):
                    t = (cut+.01-a[axis])/(b[axis]-a[axis])
                    pts2.append(a+(b-a)*t)
            if len(pts2) == 2:
                draw.line([p(v) for v in pts2], fill='#ffe19b', width=2)
        for hh in range(-5, 25, 5):
            draw.text((index*800+10, 650-hh*10), str(hh), fill='#6ccbd5')
    side.save(args.output/'vertical-sections.png')
    print('wrote', args.output)


if __name__ == '__main__':
    main()
