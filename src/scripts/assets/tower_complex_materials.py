"""Deterministic original tower-complex surfaces. No reference images are
sampled. This base reads as a cooler, more industrial defense outpost, with
amber/black hazard striping ('grate') on the exposed traversal hazards: the
turret bridges, ramps and the open center shaft. Team identity comes from 'ember' (red)
and 'glacier' (blue): this pack overrides them with deep banner-cloth
colours, so the builder only has to pick the team's material. 'light' is
overridden brighter so thrusters, light strips and the emblem read as glow
under the map shader, which has no emissive term. 'bark' is an unused kit
slot in this pack, repainted as the pale interior wall panel so rooms read
differently from the dark exterior hull.
"""

TEAM_CLOTH = {'ember': (150, 30, 28), 'glacier': (30, 72, 158)}


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (74, 79, 92), 'panel': (150, 154, 163),
                'grate': (46, 47, 52), 'trim': (34, 35, 39),
                'light': (214, 250, 255), 'bark': (128, 134, 144), **TEAM_CLOTH}
    hazard = (214, 158, 47)
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed) - .5) * 24
            mottling = (value_noise(x/16, y/16, 16, seed) - .5) * 34
            value = grain + mottling
            rgb = palettes[name]
            if name in TEAM_CLOTH:
                # Heavy woven banner cloth: vertical weave, a darker hem band.
                value = grain*.3 + mottling*.3 + (6 if x % 4 < 2 else -6)
                if y % 128 < 6: value -= 38
            elif name == 'light':
                value = grain*.15 + (6 if y % 16 < 8 else 0)
            elif name == 'concrete':
                # Riveted hull plating: 2 m plates (128 px at 4 m per tile),
                # an offset half-plate course, corner rivets, faint streaks.
                px, py = (x + (64 if (y // 128) % 2 else 0)) % 128, y % 128
                value = grain*.5 + mottling*.8 + (noise(x//3, y//9, seed+3) - .5) * 10
                if px < 2 or py < 2: value -= 30
                elif px < 4 or py < 4: value += 8
                if min((px-7)**2, (px-121)**2) + min((py-7)**2, (py-121)**2) < 6: value -= 34
            elif name == 'bark':
                # Interior wall panel: 1 m vertical panels with a recessed
                # seam, a horizontal joint every 2 m, and a subtle vent row.
                px, py = x % 64, y % 128
                value = grain*.35 + mottling*.4
                if px < 2: value -= 26
                elif px < 3: value += 10
                if py < 2: value -= 18
                if 56 <= py < 62 and 12 <= px < 52 and px % 4 < 2: value -= 16
            elif name == 'panel':
                # Deck plating: 2 m x 1 m plates with a fine, low-contrast
                # diamond tread, so floors read as plate rather than tile.
                px, py = x % 128, y % 64
                diamond = ((x+y) % 12 < 2) or ((x-y) % 12 < 2)
                value = grain*.45 + mottling*.5 + (5 if diamond else -2)
                if px < 2 or py < 2: value -= 26
                elif px < 3 or py < 3: value += 9
                if (px-6)**2 + (py-6)**2 < 5 or (px-121)**2 + (py-57)**2 < 5: value -= 22
            elif name == 'grate':
                # Ribbed grate structure under amber/black hazard striping,
                # for the open turret bridges.
                ribbed = grain*.6 + (14 if y % 14 < 3 else -10)
                stripe = ((x+y) % 34) < 15
                if stripe:
                    rgb = hazard
                    value = ribbed*.25 - 10
                else:
                    value = ribbed
            else:  # trim
                # Dark gunmetal hull banding: horizontal plate seams and a
                # rivet row. Hazard striping lives on 'grate' only, so it
                # marks the shaft, bridges and ramps rather than every hull.
                value = grain*.45 + mottling*.35
                if y % 32 < 2: value -= 22
                elif y % 32 < 4: value += 12
                if y % 32 == 8 and x % 12 < 2: value += 26
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
