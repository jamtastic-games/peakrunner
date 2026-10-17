"""Deterministic original Frostline surfaces: a polar research station in a
whiteout. No reference images are sampled.

Kit slots repainted for this pack:
  meadow / rock / soil / moss  terrain: fresh snow, granite, ice crust, snowy scree
  concrete  insulated off-white cladding panels with frost at the seams
  panel     studded rubber deck flooring
  grate     galvanised steel bar grating (ramps, decks, the beacon platform)
  trim      graphite-grey structural steel
  ember / glacier  painted team cladding with a white stencil band
  light     warm sodium-amber lamps and window glow (no emissive shader term)
  bark      knotty spruce boards: interior panelling and tree trunks
  leaf      snow-laden spruce needles
Sky layers get a separate overcast whiteout (`sky`).
"""

TEAM_PAINT = {'ember': (170, 44, 34), 'glacier': (38, 84, 168)}
STENCIL = (226, 228, 222)
FOG_GREY = 158   # the renderer's fixed fog colour, 0.62
OVERCAST = 186


def _clamp(v):
    return max(0, min(255, round(v)))


def texture(name, seed, noise, value_noise):
    palettes = {'meadow': (232, 238, 244), 'rock': (92, 94, 100), 'soil': (188, 208, 222),
                'moss': (182, 186, 192), 'concrete': (214, 216, 210), 'panel': (58, 60, 62),
                'grate': (126, 130, 132), 'trim': (70, 72, 76), 'light': (255, 196, 128),
                'bark': (132, 98, 64), 'leaf': (46, 70, 54), **TEAM_PAINT}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*20
            mottle = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name == 'meadow':
                # Wind-rippled fresh snow with faint blue shadows and sparkle.
                ripple = value_noise(x/40+y/90, y/12, 16, seed+3)
                value = grain*.25+mottle*.35+(ripple-.5)*18
                if noise(x, y, seed+9) > .995: value += 22
                rgb = (rgb[0]-round((1-ripple)*10), rgb[1]-round((1-ripple)*6), rgb[2])
            elif name == 'rock':
                # Dark granite with only fine grain and faint strata. The
                # texture tiles every 8 m, so any large feature would repeat
                # as a pattern; snow-versus-rock variation comes from the
                # terrain weights and the shader's world-scale noise.
                strata = value_noise(x/128, y/6, 16, seed+4)
                value = grain*.45+(strata-.5)*10+(noise(x//2, y//2, seed+14)-.5)*8
            elif name == 'soil':
                # Wind-scoured blue ice crust with pale fracture lines.
                fracture = abs(value_noise(x/22, y/22, 16, seed+5)-.5) < .025
                value = grain*.35+mottle*.6+(26 if fracture else 0)
            elif name == 'moss':
                # Snowy scree: fine grey grit through thin snow, no large shapes.
                grit = noise(x//2, y//2, seed+6)
                value = grain*.3+(grit-.5)*18+(-14 if grit > .82 else 0)
            elif name in TEAM_PAINT:
                # Painted cladding: vertical panel seams, a white stencil band
                # across the middle and chipped paint showing grey primer.
                value = grain*.35+mottle*.35
                if x % 64 < 2: value -= 34
                if 112 <= y < 144:
                    rgb, value = STENCIL, grain*.2
                    if 120 <= y < 136 and (x//10) % 3 == 0: rgb, value = TEAM_PAINT[name], 0
                if noise(x//3, y//3, seed+8) > .985: rgb, value = (120, 122, 124), grain*.3
            elif name == 'concrete':
                # Insulated cladding: 1 m x 2 m panels, bevelled seams, rivets,
                # frost gathering along the seams.
                bx, by = x % 64, y % 128
                value = grain*.35+mottle*.4+(noise(x//64, y//128, seed+2)-.5)*10
                if bx < 2 or by < 2: value -= 46
                elif bx < 5 or by < 5: value += 14
                if (bx in (6, 58)) and (by % 16 == 8): value -= 36
                if (bx < 9 or by < 9) and value_noise(x/6, y/6, 16, seed+7) > .6: rgb = (236, 242, 248)
            elif name == 'panel':
                # Studded rubber decking in 1 m tiles.
                value = grain*.4+mottle*.3
                if x % 64 < 2 or y % 64 < 2: value -= 20
                elif (x % 16 - 8)**2+(y % 16 - 8)**2 < 9: value += 18
            elif name == 'grate':
                # Galvanised bar grating: bearing bars with cross rods.
                value = grain*.4+mottle*.3
                if x % 8 < 3: value -= 38
                elif y % 32 < 2: value += 20
                if y % 128 < 3: value -= 30
            elif name == 'light':
                value = grain*.1+(6 if (x//16+y//16) % 2 else 0)
            elif name == 'bark':
                # Knotty spruce boards, 20 cm wide.
                board = x // 51
                value = grain*.5+(noise(board, 0, seed+3)-.5)*22
                value += 14*(value_noise(x/4, y/40, 16, seed+board)-.5)
                if x % 51 < 2: value -= 38
                if value_noise(x/12, y/12, 16, seed+11) > .82: value -= 30
            elif name == 'leaf':
                # Spruce needles under clumps of snow.
                clump = value_noise(x/10, y/10, 16, seed+12)
                value = grain*.9+mottle*.4
                if clump > .56: rgb, value = (228, 234, 240), grain*.3
            else:  # trim
                value = grain*.35+mottle*.3
                if y % 32 < 2: value -= 22
                if y % 32 == 12 and x % 24 < 3: value += 34
            data.extend(_clamp(c+value) for c in rgb)
            data.append(255)
    return bytes(data)


def sky(face, seed, noise, value_noise):
    """Overcast whiteout. Side faces (0-3) run from a pale overcast top down to
    the fog colour at the horizon (row 128); face 4 is the zenith cloud deck;
    face 5 (below the horizon) is plain fog."""
    # One flat overcast tone with only faint cloud texture, so the cube's
    # seams do not show; the sides fade to the fog colour below row 72.
    data = bytearray()
    for y in range(256):
        for x in range(256):
            cloud = value_noise(x/64+face*7, y/48, 16, seed+face)
            if face == 5:
                c = (FOG_GREY,)*3
            else:
                v = OVERCAST+(cloud-.5)*6
                if face < 4:
                    t = min(max((y-72)/56, 0), 1)   # 0 above row 72, 1 at the horizon
                    v = v+(FOG_GREY-v)*t*t*(3-2*t)
                c = (v-3, v-1, v+3)
            data.extend(_clamp(v) for v in c)
            data.append(255)
    return bytes(data)
