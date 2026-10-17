"""Deterministic original Old Holler surfaces: rain-darkened highland
concrete and slate. No reference images are sampled.

Kit slots repainted for this pack (rock, soil, sky, water, bark and leaf keep
the kit's original highland set):
  meadow    wet highland grass: isotropic tufts and clover patches, no
            directional streaks (the kit's meadow banded along one axis)
  moss      dark damp moss in the same isotropic style
  concrete  board-formed concrete with rain streaks and a damp lower band
  panel     slate roof and deck plates with drainage seams
  grate     galvanised grating for ramps and hall floors
  trim      dark oiled steel for frames, bands and fittings
  ember / glacier  team enamel panels with a chevron stripe
  light     cool lamp glow: a smooth lit diffuser panel, bright at its centre
            line and soft at the edges (the map shader has no emissive term;
            the old version read as a checkerboard)

Textures tile every 4 m (64 px per metre); walls use u along the wall and
v up the wall, so vertical streaks read as rain run-off.
"""

TEAM = {'ember': (176, 64, 30), 'glacier': (30, 118, 162)}


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (104, 112, 114), 'panel': (58, 66, 72), 'grate': (86, 94, 98),
                'trim': (34, 40, 45), 'light': (214, 238, 240), 'meadow': (66, 88, 42),
                'moss': (46, 66, 36), **TEAM}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*22
            mottling = (value_noise(x/16, y/16, 16, seed)-.5)*30
            rgb = palettes[name]
            if name == 'concrete':
                # Board-formed lifts every 0.5 m, tie holes, and run-off streaks
                # stretched down the wall from each lift joint.
                streak = (value_noise(x/3, y/48, 64, seed+5)-.5)*34
                value = grain*.7+mottling*.8+streak
                if y % 32 < 2: value -= 26
                elif y % 32 < 3: value += 10
                if (x % 64-32)**2+(y % 64-16)**2 < 5: value -= 36
                if y > 208: value -= (y-208)*.45
            elif name == 'panel':
                # Slate plates 1 m x 0.5 m with drainage seams and bolt heads.
                row = y//32
                bx = (x+(32 if row % 2 else 0)) % 64
                tone = int(noise((x+(32 if row % 2 else 0))//64, row, seed+7)*7)
                value = grain*.5+mottling*.6+(tone-3)*4
                if y % 32 < 2 or bx < 2: value -= 30
                if (bx-6)**2+(y % 32-6)**2 < 3: value += 26
            elif name == 'grate':
                # Diamond grating with a darker gap between bars.
                a, b = (x+y) % 16, (x-y) % 16
                value = grain*.4+mottling*.3+(18 if a < 3 or b < 3 else -16)
            elif name == 'trim':
                # Oiled steel: brushed grain along u, worn bright edges.
                value = grain*.35+(noise(x, y//8, seed+2)-.5)*16+mottling*.25
                if y % 64 < 2: value += 22
            elif name in TEAM:
                # Enamel panel with a pale chevron stripe every 1 m.
                value = grain*.25+mottling*.2
                chevron = (abs((x % 64)-32)+y) % 64
                if chevron < 10: rgb, value = (226, 222, 210), grain*.2
            elif name in ('meadow', 'moss'):
                # Isotropic: several octaves of value noise at equal scale in
                # both axes, blade-scale grain and scattered clover patches.
                broad = (value_noise(x/64, y/64, 4, seed+3)-.5)*26
                tuft = (value_noise(x/9, y/9, 29, seed+11)-.5)*22
                clover = value_noise(x/20, y/20, 13, seed+19)
                value = grain*.9+broad+tuft+mottling*.4
                if name == 'meadow':
                    k = min(1., max(0., (clover-.6)/.3))      # soft clover patches
                    rgb = tuple(round(c*(1-k*.35)+g*k*.35) for c, g in zip(rgb, (58, 94, 46)))
                if name == 'moss': value += (value_noise(x/5, y/5, 52, seed+23)-.5)*14
            else:  # light
                # Diffuser panel: bright along v's centre line, soft to the edges.
                edge = abs((y % 64)-31.5)/31.5
                value = grain*.06-edge*edge*14
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
