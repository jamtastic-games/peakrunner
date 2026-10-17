#!/usr/bin/env python3
"""The apps' icons: the website's mark (`site/public/favicon.svg`, two
slanted bars on the 64 grid) drawn as an app icon, the Launcher's with an
amber download badge and the Server's with a rack badge.

    python3 scripts/build-icons.py [OUTPUT]

OUTPUT (default `assets/icons`) gets:
  peakrunner.icns, launcher.icns   Mac bundles' AppIcon.icns
  peakrunner.ico, launcher.ico, server.ico   the Windows executables' icon
  peakrunner-mac.png, launcher-mac.png       the Mac dock while they run
  peakrunner.png, launcher.png               the window on Windows and Linux
The Mac art keeps Apple's grid (an 824 tile on 1024, with its shadow);
Windows and Linux get the tile nearly edge to edge. Needs Pillow and
macOS's iconutil; rebuilds are byte-identical. Refuses to overwrite.
"""
from pathlib import Path
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SS = 2                     # drawn at twice the size, then reduced
N = 1024 * SS
TILE = (100, 100, 924, 924)
RADIUS = 184


def px(v): return round(v * SS)


def grid(x, y):
    """A point of the favicon's 64 grid on the 824 tile."""
    return (100 + x * 824 / 64, 100 + y * 824 / 64)


BARS = [[grid(12, 49), grid(29, 15), grid(38, 15), grid(23, 49)],
        [grid(34, 49), grid(44, 29), grid(53, 29), grid(43, 49)]]


def rgb(h): return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def mask(draw_fn):
    m = Image.new('L', (N, N), 0)
    draw_fn(ImageDraw.Draw(m))
    return m


def vertical(top, bottom, stops):
    """An RGB image whose rows run through `stops` ((at 0..1, color)) from
    row `top` to row `bottom`, held beyond them."""
    col = Image.new('RGB', (1, N))
    span = max(bottom - top, 1)
    for y in range(N):
        t = min(max((y / SS - top) / span, 0.0), 1.0)
        for (a, ca), (b, cb) in zip(stops, stops[1:]):
            if t <= b:
                f = 0.0 if b == a else (t - a) / (b - a)
                col.putpixel((0, y), tuple(round(ca[i] + (cb[i] - ca[i]) * f) for i in range(3)))
                break
    return col.resize((N, N), Image.NEAREST)


def paint(base, color, alpha_mask, opacity=1.0):
    layer = color if isinstance(color, Image.Image) else Image.new('RGB', (N, N), color)
    a = alpha_mask if opacity == 1.0 else alpha_mask.point(lambda v: round(v * opacity))
    base.paste(layer, (0, 0), a)


def tile_mask(d): d.rounded_rectangle([px(v) for v in TILE], radius=px(RADIUS), fill=255)


def poly(points): return lambda d: d.polygon([(px(x), px(y)) for x, y in points], fill=255)


def along(p, q, dy):
    """The point `dy` below `p` on the edge from `p` towards `q`."""
    return (p[0] + (q[0] - p[0]) * dy / (q[1] - p[1]), p[1] + dy)


def badge(img, kind):
    def circle(r, color):
        paint(img, rgb(color), mask(lambda d: d.ellipse([px(770 - r), px(770 - r), px(770 + r), px(770 + r)], fill=255)))
    def bar(x, y, w, h, r, color):
        paint(img, rgb(color), mask(lambda d: d.rounded_rectangle([px(x), px(y), px(x + w), px(y + h)], radius=px(r), fill=255)))
    circle(150, '#0b1013')
    if kind == 'launcher':
        circle(128, '#e0a030')
        paint(img, rgb('#0b1013'), mask(poly([(745, 690), (795, 690), (795, 770), (835, 770), (770, 840), (705, 770), (745, 770)])))
        bar(700, 852, 140, 18, 9, '#0b1013')
    else:
        circle(128, '#edf2f4')
        for y in (700, 752, 804):
            bar(700, y, 140, 36, 10, '#0b1013')
            paint(img, rgb('#5ce6ff'), mask(lambda d, y=y: d.ellipse([px(804), px(y + 10), px(820), px(y + 26)], fill=255)))


def art(kind, shadow):
    """The icon on Apple's 1024 grid, as RGBA at the drawing size."""
    tile = mask(tile_mask)
    img = Image.new('RGB', (N, N), (0, 0, 0))
    paint(img, vertical(TILE[1], TILE[3], [(0, rgb('#18252d')), (1, rgb('#0b1013'))]), tile)
    bars = [mask(poly(b)) for b in BARS]
    both = ImageChops.lighter(*bars)
    paint(img, rgb('#5ce6ff'), ImageChops.multiply(both.filter(ImageFilter.GaussianBlur(px(18))), tile), 0.45)
    for b, m in zip(BARS, bars):
        top, bottom = b[1][1], b[0][1]
        paint(img, vertical(top, bottom, [(0, rgb('#c9f6ff')), (0.45, rgb('#81d7e5')), (1, rgb('#2f9fbd'))]), m)
        strip = [b[1], b[2], along(b[2], b[3], 14), along(b[1], b[0], 14)]
        paint(img, (255, 255, 255), mask(poly(strip)), 0.7)
    edge = mask(lambda d: d.rounded_rectangle([px(v) for v in TILE], radius=px(RADIUS), outline=255, width=px(4)))
    paint(img, (255, 255, 255), edge, 0.08)
    alpha = ImageChops.lighter(tile, edge)
    if kind != 'peakrunner':
        badge(img, kind)
    out = img.convert('RGBA')
    out.putalpha(alpha)
    if not shadow:
        return out
    drop = Image.new('L', (N, N), 0)
    drop.paste(tile, (0, px(12)))
    drop = drop.filter(ImageFilter.GaussianBlur(px(14))).point(lambda v: round(v * 0.35))
    under = Image.new('RGBA', (N, N), (0, 0, 0, 0))
    under.putalpha(drop)
    return Image.alpha_composite(under, out)


def mac(kind):
    return art(kind, True).resize((1024, 1024), Image.LANCZOS)


def square(kind):
    """The tile nearly edge to edge, for Windows and Linux."""
    m = 16
    crop = art(kind, False).crop((px(TILE[0] - m), px(TILE[1] - m), px(TILE[2] + m), px(TILE[3] + m)))
    return crop.resize((1024, 1024), Image.LANCZOS)


def icns(full, path):
    """Every size Finder and the dock ask for, 16 to 1024, through iconutil."""
    with tempfile.TemporaryDirectory() as tmp:
        folder = Path(tmp) / 'icon.iconset'
        folder.mkdir()
        for s in (16, 32, 128, 256, 512):
            full.resize((s, s), Image.LANCZOS).save(folder / f'icon_{s}x{s}.png')
            full.resize((2 * s, 2 * s), Image.LANCZOS).save(folder / f'icon_{s}x{s}@2x.png')
        subprocess.run(['iconutil', '-c', 'icns', '-o', str(path), str(folder)], check=True)


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'assets/icons'
    names = ['peakrunner.icns', 'launcher.icns', 'peakrunner.ico', 'launcher.ico', 'server.ico',
             'peakrunner-mac.png', 'launcher-mac.png', 'peakrunner.png', 'launcher.png']
    taken = [n for n in names if (out / n).exists()]
    if taken:
        sys.exit(f'refusing to overwrite {", ".join(taken)} in {out}')
    out.mkdir(parents=True, exist_ok=True)
    for kind in ('peakrunner', 'launcher', 'server'):
        flat = square(kind)
        flat.save(out / f'{kind}.ico', sizes=[(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
        if kind == 'server':
            continue
        full = mac(kind)
        icns(full, out / f'{kind}.icns')
        full.resize((512, 512), Image.LANCZOS).save(out / f'{kind}-mac.png', optimize=True)
        flat.resize((256, 256), Image.LANCZOS).save(out / f'{kind}.png', optimize=True)
    print(f'wrote {len(names)} icons to {out}')


if __name__ == '__main__':
    main()
