"""Deterministic original Dustreach surfaces: sand-colored composite armor
plate, dark deck plate, steel grating and gunmetal, team panels, plus desert
terrain layers and a clear desert sky. No reference images are sampled.

Kit slots are repainted for this pack (v2, 2026-10-05: the citadels as a
modern desert outpost; v1 was sandstone, terracotta, timber and iron):
  concrete  sand-colored composite plate: 2 x 1 m panels on a grid, dark
            seams, a lit bevel on each panel's upper edge, corner fasteners
            and a darker 0.25 m band every third course
  panel     dark deck plate: 1 m squares, tread texture on alternate plates
  grate     steel grating for ramps, lintels and treads
  trim      gunmetal: brushed, lit along its band edges
  ember / glacier  team panels: solid paint, an inset border and a chevron
  light     warm lamp glow (the map shader has no emissive term)
  bark      light interior wall panels, 1 x 2 m, vertical seams
  meadow    pale wind-rippled sand (terrain layer 0)
  rock      layered sandstone strata (terrain layer 1)
  soil      darker ochre sand on the middle slopes (terrain layer 2)
  moss      gravel hardpan on the flats (terrain layer 3)

A tile spans 4 m (64 px a meter): the kit maps a surface's two widest axes
at a quarter of a meter per unit.
"""
import math

TEAM_PAINT = {'ember': (176, 58, 36), 'glacier': (40, 104, 176)}
WHITE = (232, 228, 218)
MATERIALS = ('concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark',
             'meadow', 'rock', 'soil', 'moss')


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (192, 174, 146), 'panel': (96, 90, 84), 'grate': (112, 110, 106),
                'trim': (54, 58, 64), 'light': (255, 232, 186), 'bark': (190, 188, 180),
                'meadow': (214, 180, 128), 'rock': (170, 118, 78), 'soil': (178, 128, 80),
                'moss': (150, 132, 108), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*22
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name in TEAM_PAINT:
                # Solid paint, an inset border line and a chevron per 2 m cell.
                value = grain*.2+mottling*.15
                cx, cy = x % 128, y % 128
                if cx in (8, 9) or cx in (118, 119) or cy in (8, 9) or cy in (118, 119):
                    rgb, value = WHITE, grain*.2-8
                u, v = cx-64, cy-64
                if abs(abs(u)*.8+v-6) < 7 and abs(u) < 34 and v > -30:
                    rgb, value = WHITE, grain*.2
            elif name == 'light':
                value = grain*.1+(4 if (x//8+y//8) % 2 else 0)
            elif name == 'concrete':
                # Composite plate: 2 x 1 m panels on a grid, every third course
                # a 0.25 m band; seams, a lit upper bevel, corner fasteners.
                course, row = y//64, y % 64
                band = course % 3 == 2 and row >= 48
                px = x % 128
                tone = (noise(x//128, course, seed+11)-.5)*10
                grime = (value_noise(x/32, y/24, 16, seed+4)-.5)*8+max(0, row-40)*.25
                value = grain*.25+mottling*.3+tone-grime
                if band:
                    rgb, value = (150, 136, 116), grain*.25+mottling*.2
                    if row in (48, 49): value -= 40
                elif row < 2 or px < 2 or row == 47 and course % 3 == 2:
                    value -= 58
                elif row < 4 or px < 4:
                    value += 16
                elif (px in range(8, 12) or px in range(116, 120)) and (row in range(8, 12) or row in range(36, 40)):
                    value -= 12
            elif name == 'bark':
                # Interior panels: 1 m wide, 2 m tall, vertical seams, a faint gloss.
                px, row = x % 64, y % 128
                value = grain*.2+mottling*.25+(noise(x//64, y//128, seed+2)-.5)*6
                if px < 2 or row < 2: value -= 46
                elif px < 3: value += 12
            elif name == 'panel':
                # Deck plate: 1 m squares, dark seams, tread on alternate plates.
                plate = (x//64+y//64) % 2
                value = grain*.35+mottling*.3+(noise(x//64, y//64, seed+7)-.5)*6
                if plate and ((x+y) % 8 < 2 or (x-y) % 8 < 2): value += 9
                if x % 64 < 2 or y % 64 < 2: value -= 40
                elif x % 64 < 3 or y % 64 < 3: value += 10
            elif name == 'grate':
                # Steel grating: bars 8 px apart, a cross bar every 32 px.
                value = grain*.3+mottling*.25
                if x % 8 < 3 and y % 32 > 2: rgb, value = (34, 36, 40), grain*.2
                elif y % 32 < 2: value += 14
            elif name == 'trim':
                # Gunmetal: brushed streaks, lit along the band edges.
                value = grain*.2+(value_noise(x/64, y/2, 16, seed+5)-.5)*10
                if y % 32 == 0: value += 30
                elif y % 32 == 31: value -= 20
            elif name == 'meadow':
                # Pale sand with fine wind ripples.
                ripple = math.sin((x*.9+y*.45+value_noise(x/20, y/20, 16, seed+9)*18)*math.tau/9)
                value = grain*.35+mottling*.5+ripple*6
            elif name == 'rock':
                # Horizontal sandstone strata with darker bands and cracks.
                band = math.sin((y+value_noise(x/24, y/24, 16, seed+3)*20)*math.tau/22)
                value = grain*.7+mottling*.8+band*14
                if noise(x//4, y//2, seed+12) > .985: value -= 40
            elif name == 'soil':
                # Ochre sand, coarser than the pale layer, wind streaks.
                value = grain*.55+mottling*.7+(value_noise(x/60, y/8, 16, seed+13)-.5)*16
            else:  # moss slot: gravel hardpan
                pebble = noise(x//3, y//3, seed+14)
                value = grain*.6+mottling*.5+(pebble-.5)*34
                if pebble > .93: rgb = (118, 104, 88)
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)


def sky(face, unit, smooth, cloud_noise):
    """Clear desert sky cube face: warm dusty horizon, deep blue zenith and a
    few high streaks of cloud. Same face order as the kit sky."""
    data = bytearray()
    for y in range(256):
        for x in range(256):
            u, v = x/255*2-1, y/255*2-1
            d = [(u, -v, -1), (1, -v, u), (-u, -v, 1), (-1, -v, -u), (u, 1, -v), (u, -1, v)][face]
            dx, dy, dz = unit(d)
            streak = sum((cloud_noise(dx*f*3+5, dy*f+2, dz*f*.6+9, 71)-.5)/2**i for i, f in enumerate([3, 7, 16]))
            cloud = smooth((streak+.02)/.34)*.55
            horizon = smooth(dy/.5) if dy > 0 else 0
            haze = (232, 206, 164)
            zenith = (70, 122, 188)
            for low, high in zip(haze, zenith):
                c = low*(1-horizon)+high*horizon
                c = c*(1-cloud*horizon)+248*cloud*horizon
                data.append(max(0, min(255, int(c))))
            data.append(255)
    return bytes(data)
