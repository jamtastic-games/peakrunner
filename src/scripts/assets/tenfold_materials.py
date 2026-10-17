"""Deterministic original surfaces for Tenfold Tower. No reference images are
sampled. The kit's slots are repainted for a tower of ten themed floors, and
the tower's own layers (`tenfold_tower.EXTRA`) are painted here too:

  concrete  dark polished floor tiles      panel   pale steel wall panels
  grate     black steel grating (stairs)   trim    dark gunmetal
  ember     warm amber glass               glacier cool cyan glass (the lab)
  light     lamp and strip glow            bark    dark oiled wood
  leaf      planted foliage                moss    wet greenhouse floor
  stone     weathered sandstone blocks     marble  veined white marble
  brass     worn brass plate               crystal violet crystal
  neon      magenta neon panels            carpet  deep red cloth
  books     shelves of book spines         tile    white lab tiles
  crate     slatted cargo crates           hazard  yellow and black stripes
  rust      a red corrugated container     steelblue a blue corrugated container
  copper    copper with green patina       gold    polished gold
  screen    a dark screen of bright lines
"""
import math

PALETTES = {
    'concrete': (58, 60, 66), 'panel': (150, 152, 158), 'grate': (40, 41, 45), 'trim': (34, 36, 40),
    'ember': (232, 150, 60), 'glacier': (90, 200, 236), 'light': (255, 248, 232), 'bark': (88, 58, 36),
    'leaf': (52, 104, 44), 'moss': (60, 84, 58), 'stone': (176, 150, 112), 'marble': (226, 222, 214),
    'brass': (176, 132, 62), 'crystal': (150, 96, 230), 'neon': (236, 60, 196), 'carpet': (128, 22, 30),
    'books': (96, 62, 44), 'tile': (214, 220, 224), 'crate': (138, 104, 64),
    'hazard': (226, 176, 30), 'rust': (146, 58, 38), 'steelblue': (52, 88, 132), 'copper': (184, 104, 60),
    'gold': (222, 176, 72), 'screen': (18, 36, 54),
}
GLOW = {'ember', 'glacier', 'light', 'crystal', 'neon', 'screen'}


def texture(name, seed, noise, value_noise):
    rgb = PALETTES[name]
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*16
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*26
            value = grain*.5+mottling*.6
            col = rgb
            if name in GLOW:
                value = grain*.12+10
                if name in ('ember', 'glacier', 'crystal') and (x % 64 < 2 or y % 64 < 2): value = -36
                if name == 'neon' and y % 32 < 3: value = -60
                if name == 'screen':
                    # Lines of bright readout on a dark glass.
                    value = grain*.2
                    if y % 16 < 3 and noise(x//12, y//16, seed+9) > .35: col, value = (90, 220, 255), grain*.2
            elif name == 'concrete':
                # 1 m polished tiles with thin dark joints.
                if x % 64 < 2 or y % 64 < 2: value -= 26
                value += (noise(x//64, y//64, seed+5)-.5)*14
            elif name == 'panel':
                if y % 128 < 3 or x % 128 < 2: value -= 30
                elif y % 128 < 6: value += 10
            elif name == 'grate':
                if x % 8 < 2 or y % 8 < 2: value += 24
            elif name == 'trim':
                if y % 32 < 2: value -= 12
            elif name == 'bark':
                # Planks along x with a fine grain.
                value = grain*.3+(noise(x//2, y//16, seed+2)-.5)*18+math.sin(x*.09+y*.6)*4
                if y % 32 < 2: value -= 30
            elif name == 'leaf':
                value = grain*.9+mottling+(18 if noise(x//3, y//3, seed+4) > .6 else -6)
            elif name == 'stone':
                # Ashlar courses, each block its own shade.
                row = y//48
                px = (x+(48 if row % 2 else 0)) % 96
                value += (noise(row, (x+(48 if row % 2 else 0))//96, seed+6)-.5)*30
                if y % 48 < 3 or px < 3: value -= 40
            elif name == 'marble':
                vein = math.sin(x*.05+value_noise(x/24, y/24, 8, seed+7)*9)
                value = grain*.2+(-34 if abs(vein) < .06 else 0)+mottling*.3
            elif name == 'brass':
                value = grain*.6+mottling*.9+(noise(x//4, y, seed+8)-.5)*10
                if x % 128 < 3: value -= 30
            elif name == 'carpet':
                value = grain*.4+(8 if (x+y) % 6 < 3 else -8)
                if 20 < y % 128 < 26: col, value = (196, 160, 70), grain*.3
            elif name == 'books':
                # Rows of spines: each a band of its own color, a dark shelf.
                shelf = y % 64
                if shelf < 6: col, value = (60, 38, 24), grain*.3
                else:
                    k = (x//6+(y//64)*7) % 6
                    col = [(126, 34, 30), (34, 70, 110), (40, 96, 52), (150, 120, 60), (90, 40, 90), (130, 90, 50)][k]
                    value = grain*.4+(10 if x % 6 == 0 else 0)-(20 if shelf < 12 else 0)
            elif name == 'tile':
                if x % 32 < 2 or y % 32 < 2: value -= 34
            elif name == 'crate':
                # Slats with a dark frame and a stencil band.
                value = grain*.4+(noise(x//2, y//12, seed+3)-.5)*14
                if y % 32 < 3: value -= 34
                if x % 128 < 10 or x % 128 > 118: value -= 22
            elif name == 'hazard':
                # Diagonal stripes, worn.
                if (x+y) % 64 >= 32: col = (28, 28, 30)
                value = grain*.6+(-30 if noise(x//3, y//3, seed+10) > .9 else 0)
            elif name in ('rust', 'steelblue'):
                # Corrugated sheet: ribs across, a frame top and bottom.
                value = grain*.5+mottling*.4+math.sin(x*math.tau/16)*14
                if y % 128 < 6: value -= 30
            elif name == 'copper':
                value = grain*.5+mottling*.8
                if value_noise(x/20, y/20, 12, seed+11) > .72: col, value = (78, 142, 112), grain*.6
            elif name == 'gold':
                value = grain*.25+mottling*.35+math.sin((x+y)*.04)*10
                if y % 64 < 2: value -= 26
            elif name == 'moss':
                value = grain*.8+mottling*1.2
            data.extend(max(0, min(255, round(c+value))) for c in col)
            data.append(255)
    return bytes(data)
