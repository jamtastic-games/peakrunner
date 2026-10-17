"""Find a weapon's grips and muzzle on its side-view concept (barrel to the
left), as fractions of its bounds for `source.json` points:

    python scripts/models/measure.py OVERLAY.png CONCEPT.png [CONCEPT.png...]

Prints, per image, `grip_r`, `grip_l` and `muzzle` as [u, v, w]: u across
(the middle), v from the bottom, w from the muzzle end. The trigger guard is
the lowest small enclosed opening; the pistol grip hangs just
behind it. A support grip is a part hanging well below the body ahead of the
trigger, else the body ahead of it. The muzzle is the middle of the front
end. Writes an overlay of what it found, to check by eye.
"""
import json
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(__file__))
from blobs import label  # noqa: E402

out, files = sys.argv[1], sys.argv[2:]
tiles = []
for path in files:
    img = Image.open(path).convert("RGB")
    a = np.asarray(img).astype(np.int16)
    bg = np.median(np.concatenate([a[:6, :6].reshape(-1, 3), a[-6:, -6:].reshape(-1, 3)]), 0)
    ink = np.abs(a - bg).max(-1) > 22
    ys, xs = np.nonzero(ink)
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    W, H = x1 - x0, y1 - y0
    box = ink[y0:y1, x0:x1]
    # Openings: background inside the shape, away from the edges.
    holes, n = label(~box)
    edge = set(np.unique(np.concatenate([holes[0], holes[-1], holes[:, 0], holes[:, -1]])))
    guard = None
    for k in range(1, n + 1):
        if k in edge:
            continue
        hy, hx = np.nonzero(holes == k)
        area = len(hy)
        if not (0.0008 * W * H < area < 0.04 * W * H):
            continue
        cy, cx = hy.mean(), hx.mean()
        if cy < 0.35 * H or cx < 0.25 * W:
            continue
        # The lowest opening (a stock's openings sit higher).
        if guard is None or hy.max() > guard[4]:
            guard = (cx, cy, hx.max(), hy.min(), hy.max())
    low = np.array([np.nonzero(box[:, x])[0].max() if box[:, x].any() else 0 for x in range(W)])
    body = np.percentile(low, 40)
    if guard:
        gx = min(guard[2] + 0.045 * W, W - 1)
        gy = guard[3] + 0.25 * (low[int(gx)] - guard[3])
    else:
        gx, gy = 0.78 * W, body + 0.15 * H
    # A support grip: a part hanging well below the body ahead of the guard.
    ahead = int((guard[0] if guard else gx) - 0.08 * W)
    hang = [x for x in range(int(0.1 * W), max(ahead, 0)) if low[x] > body + 0.18 * H]
    if hang:
        lx = float(np.mean(hang))
        ly = body + 0.3 * (low[int(lx)] - body)
    else:
        lx = gx - 0.42 * W
        col = np.nonzero(box[:, int(max(lx, 0))])[0]
        ly = (col.min() + col.max()) / 2 if len(col) else body
    front = np.nonzero(box[:, :max(3, int(0.02 * W))].any(1))[0]
    my = (front.min() + front.max()) / 2
    frac = lambda x, y: [0.5, round(float(1 - y / H), 3), round(float(x / W), 3)]
    pts = {"grip_r": frac(gx, gy), "grip_l": frac(lx, ly), "muzzle": frac(0, my)}
    name = os.path.basename(os.path.dirname(path)) if path.endswith("concept.png") else path
    print(name, json.dumps(pts))
    d = ImageDraw.Draw(img)
    for (x, y), c in [((gx, gy), "red"), ((lx, ly), "blue"), ((0, my), "lime")]:
        X, Y = x0 + x, y0 + y
        d.ellipse([X - 12, Y - 12, X + 12, Y + 12], outline=c, width=6)
    d.text((10, 10), name, fill="black")
    tiles.append(img.resize((384, 384)))
cols = 5
sheet = Image.new("RGB", (384 * cols, 384 * ((len(tiles) + cols - 1) // cols)), (90, 90, 90))
for i, t in enumerate(tiles):
    sheet.paste(t, ((i % cols) * 384, (i // cols) * 384))
sheet.save(out)
