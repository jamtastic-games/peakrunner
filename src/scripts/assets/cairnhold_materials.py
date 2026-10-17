"""Deterministic original Cairnhold surfaces: a fortified highland bunker in
graphite composite plate and brushed brass. No reference images are sampled.

Kit slots are repainted for this pack (v2, 2026-10-06: the bunkers as a
modern fortification; v1 was ashlar masonry, flagstones and patinated
bronze):
  concrete  graphite composite plate: 2 x 1 m panels on a grid, dark seams,
            a lit bevel on each panel's upper edge, corner fasteners and a
            darker band every third course
  panel     dark deck plate: 1 m squares, tread on alternate plates
  grate     steel grating for ramps, thresholds and treads
  trim      brushed brass: frames, bands and fittings, lit along its edges
  ember / glacier  team panels: solid paint, a brass border and a diamond
  light     warm lamp and seam glow (the map shader has no emissive term)
  bark      light interior wall panels, 1 x 2 m, vertical seams

A tile spans 4 m (64 px a meter): the kit maps a surface's two widest axes
at a quarter of a meter per unit.
"""

TEAM_PAINT = {'ember': (168, 48, 34), 'glacier': (40, 92, 166)}
BRASS = (184, 146, 82)


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (96, 100, 106), 'panel': (70, 70, 72), 'grate': (110, 110, 108),
                'trim': BRASS, 'light': (255, 222, 164), 'bark': (182, 182, 176), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*22
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name in TEAM_PAINT:
                # Solid paint, a brass border line and a diamond per 2 m cell.
                value = grain*.2+mottling*.15
                cx, cy = x % 128, y % 128
                if cx in (8, 9) or cx in (118, 119) or cy in (8, 9) or cy in (118, 119):
                    rgb, value = BRASS, grain*.2
                d = abs(cx-64)+abs(cy-64)
                if 26 <= d <= 34:
                    rgb, value = BRASS, grain*.2+6
            elif name == 'light':
                value = grain*.12+(5 if (x+y) % 24 < 12 else 0)
            elif name == 'concrete':
                # Composite plate: 2 x 1 m panels on a grid, every third course
                # a 0.25 m band; seams, a lit upper bevel, corner fasteners.
                course, row = y//64, y % 64
                band = course % 3 == 2 and row >= 48
                px = x % 128
                tone = (noise(x//128, course, seed+11)-.5)*10
                grime = (value_noise(x/32, y/24, 16, seed+4)-.5)*8+max(0, row-40)*.2
                value = grain*.25+mottling*.3+tone-grime
                if band:
                    rgb, value = (72, 76, 82), grain*.25+mottling*.2
                    if row in (48, 49): value -= 30
                elif row < 2 or px < 2 or row == 47 and course % 3 == 2:
                    value -= 44
                elif row < 4 or px < 4:
                    value += 18
                elif (px in range(8, 12) or px in range(116, 120)) and (row in range(8, 12) or row in range(36, 40)):
                    value += 10
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
                if x % 64 < 2 or y % 64 < 2: value -= 34
                elif x % 64 < 3 or y % 64 < 3: value += 10
            elif name == 'grate':
                # Steel grating: bars 8 px apart, a cross bar every 32 px.
                value = grain*.3+mottling*.25
                if x % 8 < 3 and y % 32 > 2: rgb, value = (32, 34, 38), grain*.2
                elif y % 32 < 2: value += 14
            else:  # trim
                # Brushed brass: fine streaks, lit along its band edges.
                value = grain*.2+(value_noise(x/64, y/2, 16, seed+5)-.5)*12
                if y % 32 == 0: value += 26
                elif y % 32 == 31: value -= 26
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
