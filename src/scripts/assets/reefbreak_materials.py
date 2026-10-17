"""Deterministic original Reefbreak surfaces: a tropical atoll with coral
rock, pale sand and stranded steel freighters. No reference images are
sampled.

Kit slots repainted for this pack:
  meadow / rock / soil / moss  terrain: beach grass, coral limestone, dry sand, wet sand
  concrete  painted hull plate, weathered, with rivet seams and rust streaks
  panel     deck plate with non-slip diamonds
  grate     steel bar grating (ramps, gangways)
  trim      dark gunmetal
  ember / glacier  team paint with a pale stencil band
  light     warm white lamps
  bark      driftwood planking
  leaf      palm and sea-grape greens
The sky is the manifest's procedural `look.sky`; the kit's sky faces stay.
"""

MATERIALS = ('meadow', 'rock', 'soil', 'moss', 'concrete', 'panel', 'grate', 'trim',
             'ember', 'glacier', 'light', 'bark', 'leaf')
TEAM_PAINT = {'ember': (184, 64, 40), 'glacier': (32, 104, 164)}
STENCIL = (230, 228, 214)


def _clamp(v):
    return max(0, min(255, round(v)))


def texture(name, seed, noise, value_noise):
    palettes = {'meadow': (118, 146, 82), 'rock': (196, 186, 164), 'soil': (232, 216, 176),
                'moss': (176, 160, 122), 'concrete': (206, 208, 204), 'panel': (92, 98, 96),
                'grate': (118, 122, 124), 'trim': (48, 52, 58), 'light': (255, 240, 212),
                'bark': (150, 128, 100), 'leaf': (52, 110, 58), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*20
            mottle = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name == 'meadow':
                # Coarse beach grass over sand showing through.
                blade = noise(x, y//3, seed+3)
                value = grain*.6+mottle*.5+(blade-.5)*16
                if value_noise(x/10, y/10, 16, seed+9) > .7: rgb, value = (206, 192, 152), grain*.3
            elif name == 'rock':
                # Pale coral limestone: pitted, with darker weathered pockets.
                pit = noise(x//2, y//2, seed+4)
                value = grain*.5+mottle*.5
                if pit > .9: value -= 40
                if value_noise(x/30, y/30, 16, seed+13) < .3: value -= 18
            elif name == 'soil':
                # Dry sand: fine grain and faint wind ripples.
                ripple = value_noise(x/40+y/14, y/60, 16, seed+5)
                value = grain*.35+(ripple-.5)*10
                if noise(x, y, seed+7) > .97: rgb, value = (250, 246, 236), grain*.2   # shell flecks
            elif name == 'moss':
                # Wet sand: darker, smoother, with foam-line streaks.
                value = grain*.2+mottle*.3
                if abs(value_noise(x/64, y/8, 16, seed+6)-.5) < .025: rgb, value = (224, 222, 210), grain*.2
            elif name in TEAM_PAINT:
                value = grain*.35+mottle*.35
                if x % 64 < 2 or y % 128 < 2: value -= 34
                if 104 <= y < 152:
                    rgb, value = STENCIL, grain*.2
                    if 112 <= y < 144 and ((x+abs(y-128)) // 12) % 3 == 0: rgb, value = TEAM_PAINT[name], 0
                if noise(x//3, y//3, seed+8) > .985: rgb, value = (120, 110, 100), grain*.3
            elif name == 'concrete':
                # Weathered hull plate: 2 m x 1 m plates, rivet lines and
                # rust streaks running down from the seams.
                bx, by = x % 128, y % 64
                value = grain*.2+(noise(x//128, y//64, seed+2)-.5)*12
                if bx < 2 or by < 2: value -= 46
                if (bx in (5, 122) or by in (5, 58)) and x % 6 == 0: value -= 28
                streak = value_noise(x/3, y/60, 16, seed+14)
                if streak > .78 and by > 4:
                    rgb = (168, 110, 78); value = grain*.3-(by/64)*20
            elif name == 'panel':
                # Deck plate with raised non-slip diamonds.
                value = grain*.25+mottle*.2
                dx, dy = x % 16, y % 16
                if abs(dx-8)+abs(dy-8) < 3: value += 20
                if x % 128 < 2 or y % 128 < 2: value -= 26
            elif name == 'grate':
                value = grain*.4+mottle*.3
                if x % 8 < 3: value -= 40
                elif y % 32 < 2: value += 18
                if y % 128 < 3: value -= 30
            elif name == 'light':
                value = grain*.1+(5 if (x//16+y//16) % 2 else 0)
            elif name == 'bark':
                # Sun-bleached driftwood planks.
                board = x // 40
                value = grain*.5+(noise(board, 0, seed+3)-.5)*20
                value += 12*(value_noise(x/4, y/50, 16, seed+board)-.5)
                if x % 40 < 2: value -= 34
            elif name == 'leaf':
                clump = value_noise(x/10, y/10, 16, seed+12)
                value = grain*.9+mottle*.5
                if clump > .66: rgb, value = (96, 150, 70), grain*.4
            else:  # trim
                value = grain*.35+mottle*.3
                if y % 32 < 2: value -= 22
                if y % 32 == 12 and x % 24 < 3: value += 30
            data.extend(_clamp(c+value) for c in rgb)
            data.append(255)
    return bytes(data)
