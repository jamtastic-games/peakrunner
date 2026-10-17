"""Deterministic original Longfield surfaces: a floodlit stadium. No
reference images are sampled.

Kit slots are repainted for this pack:
  concrete  poured stadium concrete with form-board seams
  panel     rows of moulded seats on grey risers
  grate     white chalk paint for field lines and end-zone rings
  trim      dark galvanised steel for pylons, masts and frames
  ember / glacier  glossy team paint with a white pinstripe
  light     floodlight and lamp glow (the map shader has no emissive term)
  bark      scoreboard face: a dark panel of lamp cells
"""

TEAM_PAINT = {'ember': (176, 44, 30), 'glacier': (30, 92, 170)}


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (150, 148, 142), 'panel': (82, 88, 98), 'grate': (236, 236, 228),
                'trim': (58, 62, 68), 'light': (255, 246, 222), 'bark': (18, 20, 24), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*18
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*26
            rgb = palettes[name]
            if name in TEAM_PAINT:
                value = grain*.25+mottling*.25
                if y % 128 in (60, 61, 66, 67): rgb, value = (236, 236, 228), grain*.2
            elif name == 'light':
                value = grain*.1
            elif name == 'grate':
                # Chalk: slightly uneven, with grass showing through in flecks.
                value = grain*.5+mottling*.3
                if noise(x, y, seed+9) > .93: rgb, value = (120, 150, 90), 0
            elif name == 'concrete':
                # 2 m form-board pours: horizontal board lines and tie holes.
                value = grain*.7+mottling*.9+(noise(x//128, y//32, seed+4)-.5)*10
                if y % 32 < 2: value -= 22
                if y % 64 == 16 and x % 64 < 3: value -= 40
            elif name == 'panel':
                # Seat rows: a seat back every 0.5 m on a grey riser.
                seat = (x % 32) < 26 and (y % 64) < 30
                value = grain*.3+mottling*.3
                if seat: rgb, value = (46, 60, 88), grain*.3+(10 if (y % 64) < 6 else 0)
                elif (y % 64) > 58: value -= 26
            elif name == 'bark':
                # Scoreboard: dark panel of round lamp cells, a few lit.
                cx, cy = x % 8-3.5, y % 8-3.5
                cell = cx*cx+cy*cy < 7
                lit = noise(x//8, y//8, seed+2) > .82
                value = grain*.1
                if cell: rgb, value = ((250, 214, 120) if lit else (44, 46, 50)), 0
            else:  # trim
                value = grain*.4+mottling*.35
                if y % 64 < 2: value -= 18
                if y % 64 == 20 and x % 24 < 3: value += 26
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
