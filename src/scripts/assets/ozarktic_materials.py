"""Deterministic original Ozarktic Blast surfaces: frosted pine highlands
with limestone bluffs, and armoured field bases. No reference images are
sampled.

Kit slots repainted for this pack:
  meadow / rock / soil / moss  terrain: frosted grass, limestone, snow, pine duff
  concrete  clean light composite cladding panels
  panel     dark satin deck plate with a fine light grid
  grate     steel bar grating (ramps, bridges, decks)
  trim      dark gunmetal
  ember / glacier  painted team plate with a pale stencil band and chevrons
  light     cool white lamps
  bark      pine boards and trunks
  leaf      frosted pine needles
The sky is the manifest's procedural `look.sky`; the kit's sky faces stay.
"""

MATERIALS = ('meadow', 'rock', 'soil', 'moss', 'concrete', 'panel', 'grate', 'trim',
             'ember', 'glacier', 'light', 'bark', 'leaf')
TEAM_PAINT = {'ember': (176, 58, 36), 'glacier': (36, 96, 170)}
STENCIL = (222, 224, 216)


def _clamp(v):
    return max(0, min(255, round(v)))


def texture(name, seed, noise, value_noise):
    palettes = {'meadow': (112, 132, 96), 'rock': (126, 124, 116), 'soil': (230, 236, 242),
                'moss': (84, 88, 70), 'concrete': (176, 182, 188), 'panel': (58, 62, 68),
                'grate': (116, 120, 124), 'trim': (46, 50, 58), 'light': (210, 240, 255),
                'bark': (118, 86, 58), 'leaf': (44, 66, 50), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*20
            mottle = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name == 'meadow':
                # Short winter grass, hoar frost on the tips in pale flecks.
                blade = noise(x, y//3, seed+3)
                value = grain*.6+mottle*.45+(blade-.5)*14
                if noise(x, y, seed+9) > .93: rgb, value = (214, 222, 226), grain*.2
            elif name == 'rock':
                # Pale limestone with horizontal bedding and dark solution seams.
                bed = value_noise(x/128, y/5, 16, seed+4)
                value = grain*.4+(bed-.5)*16
                if abs(value_noise(x/40, y/9, 16, seed+13)-.5) < .02: value -= 30
            elif name == 'soil':
                # Settled snow: soft wind texture and faint blue hollows.
                drift = value_noise(x/48+y/110, y/16, 16, seed+5)
                value = grain*.2+mottle*.3+(drift-.5)*14
                rgb = (rgb[0]-round((1-drift)*12), rgb[1]-round((1-drift)*7), rgb[2])
            elif name == 'moss':
                # Pine duff: needles and cones on dark earth, lightly frosted.
                needle = noise(x//2, y, seed+6)
                value = grain*.5+mottle*.5+(needle-.5)*16
                if noise(x, y, seed+15) > .96: rgb, value = (190, 198, 200), grain*.3
            elif name in TEAM_PAINT:
                # Painted plate: panel seams, a pale stencil band with the
                # team colour in chevrons, chipped to grey primer.
                value = grain*.35+mottle*.35
                if x % 64 < 2 or y % 128 < 2: value -= 34
                if 104 <= y < 152:
                    rgb, value = STENCIL, grain*.2
                    if 112 <= y < 144 and ((x+abs(y-128)) // 12) % 3 == 0: rgb, value = TEAM_PAINT[name], 0
                if noise(x//3, y//3, seed+8) > .985: rgb, value = (120, 122, 124), grain*.3
            elif name == 'concrete':
                # Clean composite cladding: large 2 m x 1 m panels, crisp
                # seams, a faint sheen gradient and the odd vent strip.
                bx, by = x % 128, y % 64
                value = grain*.15+(noise(x//128, y//64, seed+2)-.5)*10+(by-32)*.12
                if bx < 2 or by < 2: value -= 52
                elif bx < 4 or by < 4: value += 10
                if 96 <= bx < 120 and 44 <= by < 52 and (x//3) % 2: value -= 26
            elif name == 'panel':
                # Satin deck plate: 1 m tiles with a fine light grid.
                value = grain*.2+mottle*.15
                if x % 64 < 2 or y % 64 < 2: value += 26
                elif x % 64 < 3 or y % 64 < 3: value -= 8
            elif name == 'grate':
                # Steel bar grating: bearing bars with cross rods.
                value = grain*.4+mottle*.3
                if x % 8 < 3: value -= 40
                elif y % 32 < 2: value += 18
                if y % 128 < 3: value -= 30
            elif name == 'light':
                value = grain*.1+(5 if (x//16+y//16) % 2 else 0)
            elif name == 'bark':
                # Pine boards, 20 cm wide, with knots.
                board = x // 51
                value = grain*.5+(noise(board, 0, seed+3)-.5)*22
                value += 14*(value_noise(x/4, y/40, 16, seed+board)-.5)
                if x % 51 < 2: value -= 36
                if value_noise(x/12, y/12, 16, seed+11) > .83: value -= 28
            elif name == 'leaf':
                # Pine needles with frost on the outer clumps.
                clump = value_noise(x/10, y/10, 16, seed+12)
                value = grain*.9+mottle*.4
                if clump > .62: rgb, value = (196, 206, 206), grain*.3
            else:  # trim
                value = grain*.35+mottle*.3
                if y % 32 < 2: value -= 22
                if y % 32 == 12 and x % 24 < 3: value += 30
            data.extend(_clamp(c+value) for c in rgb)
            data.append(255)
    return bytes(data)
