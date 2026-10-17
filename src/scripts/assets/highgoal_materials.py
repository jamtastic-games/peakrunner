"""Deterministic original Highgoal surfaces: a sand arena walled in carved
stone tiles. No reference images are sampled.

Kit slots are repainted for this pack:
  concrete  2 m wall tiles, each carved with an original rosette medallion
  panel     smooth pale stone for the wall base and ledges
  grate     white chalk for the floor markings
  trim      dark basalt for the goal pillars and wall caps
  ember / glacier  glowing team glass for the goal platforms
  light     lamp and platform-edge glow
"""
import math

TEAM_GLASS = {'ember': (214, 70, 44), 'glacier': (60, 170, 236)}


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (150, 146, 140), 'panel': (182, 176, 164), 'grate': (238, 236, 228),
                'trim': (54, 54, 58), 'light': (255, 244, 220), **TEAM_GLASS}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*16
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*24
            rgb = palettes[name]
            value = grain*.5+mottling*.6
            if name in TEAM_GLASS:
                # Glass: bright, with a faint grid of panes.
                value = grain*.15+18
                if x % 64 < 2 or y % 64 < 2: value = -30
            elif name == 'light':
                value = grain*.1
            elif name == 'concrete':
                # One tile per 4 m texture repeat, split into four 2 m tiles,
                # each with a carved medallion: a ring and an eight-petal rosette.
                u, v = (x % 128)-63.5, (y % 128)-63.5
                r = math.hypot(u, v); a = math.atan2(v, u)
                carve = 0.0
                if 40 < r < 46: carve = -34
                elif 30 < r < 33: carve = -22
                elif r < 28 and abs(math.cos(4*a)) > .55+.012*r: carve = -26
                elif r < 5: carve = -30
                if x % 128 < 3 or y % 128 < 3: carve = -46
                value += carve
            elif name == 'panel':
                if y % 64 < 2: value -= 24
            elif name == 'trim':
                if y % 32 < 2: value -= 14
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
