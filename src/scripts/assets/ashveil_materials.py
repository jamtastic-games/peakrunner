"""Deterministic original Ashveil surfaces: a scorched temple in rust dust.

Warm rust-brown dust and dark basalt under an orange haze, a temple of
dark cut stone with bronze trim, bone-pale skull rocks, and glow the color
of the lava. No reference images are sampled; every texel comes from the
kit's hash noise.

Kit slots are repainted for this pack:
  concrete  dark temple stone: 2 x 1 m cut blocks, staggered courses, a
            chiselled seam and worn faces
  panel     blackened iron plate: 1 m squares, rivets
  grate     dark iron grating for the tunnel
  trim      bronze, brushed along the band, bright at its edge
  ember     rust-red painted iron for the lava works
  glacier   bone: pale, mottled, with fine cracks (skull rocks)
  light     hot orange glow (the bloom mask picks bright texels of it)
  bark      volcanic rock: red-brown, layered, with dark veins
  leaf      basalt: near-black, faceted
  meadow    rust dust (terrain layer 0)
  rock      basalt cliff (layer 1)
  soil      cooled crust at the lava's edge, with glowing cracks (layer 2)
  moss      dark ash gravel (layer 3)

Layers of its own follow the kit's (`EXTRA`, the skull tower's):
  timber    dark weathered planks: 0.25 m boards, grain, knots, nail heads
  sacking   sandbag burlap: a coarse weave, a seam across each bag
  bone      the tower's skulls outside: off-white bone, soft gray stains,
            porous grain, grime run down in uneven streaks, cracks along
            wandering lines, sparse pits, rare thin seeps of old gore
  bone_grime the same bone filthier: gray, black streaks (laid over the
            skulls in broad patches and under brows and cheeks)
  bone_dark the tower's cavity: soot-blackened bone, gray where it's worn
  None of the bone has anything on a grid or a lone mark that stands out,
  so the tile doesn't show.

A structure tile spans 4 m (64 px a metre); the terrain's spans 8 m. Every
pattern repeats exactly across the 256 px tile (noise periods divide it), so
tiles meet without seams.
"""
import math

MATERIALS = ('concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark', 'leaf',
             'meadow', 'rock', 'soil', 'moss')
EXTRA = ('timber', 'sacking', 'bone', 'bone_grime', 'bone_dark')


def stretched(noise, x, y, px, py, seed):
    """Value noise wrapping every px cells across and py down (for streaks
    longer than they're wide that still tile)."""
    ix, iy = math.floor(x), math.floor(y)
    u, v = x-ix, y-iy
    u, v = u*u*(3-2*u), v*v*(3-2*v)
    a, b = noise(ix % px, iy % py, seed), noise((ix+1) % px, iy % py, seed)
    c, d = noise(ix % px, (iy+1) % py, seed), noise((ix+1) % px, (iy+1) % py, seed)
    return (a*(1-u)+b*u)*(1-v)+(c*(1-u)+d*u)*v


def clamp01(v):
    return max(0.0, min(1.0, v))


def mix3(a, b, t):
    return tuple(p+(q-p)*t for p, q in zip(a, b))


def texture(name, seed, noise, value_noise):
    palettes = {'concrete': (70, 58, 52), 'panel': (34, 31, 30), 'grate': (52, 46, 42), 'trim': (150, 104, 58),
                'ember': (128, 48, 26), 'glacier': (196, 180, 150), 'light': (255, 150, 58), 'bark': (92, 54, 38),
                'leaf': (38, 32, 30), 'meadow': (122, 84, 60), 'rock': (58, 44, 38), 'soil': (46, 32, 28),
                'moss': (78, 64, 56), 'timber': (74, 52, 38), 'sacking': (132, 112, 82),
                'bone': (224, 222, 216), 'bone_grime': (150, 148, 142), 'bone_dark': (84, 82, 80)}
    data = bytearray()
    for y in range(256):
        for x in range(256):
            grain = (noise(x, y, seed)-.5)*10
            rgb = palettes[name]
            value = grain
            if name == 'concrete':
                course, row = y//64, y % 64
                shift = 64 if course % 2 else 0
                px = (x+shift) % 128
                value = grain*.5+(noise((x+shift)//128, course, seed+11)-.5)*12 \
                    + (value_noise(x/32, y/32, 8, seed+4)-.5)*10
                if row == 0 or px == 0: value -= 34
                elif row == 1 or px == 1: value += 12
                elif row == 63: value -= 14
            elif name == 'panel':
                cx, cy = x % 64, y % 64
                value = grain*.4+(noise(x//64, y//64, seed+2)-.5)*8
                if cx == 0 or cy == 0: value -= 22
                elif cx == 1 or cy == 1: value += 10
                if (cx-6)**2+(cy-6)**2 < 5 or (cx-57)**2+(cy-57)**2 < 5: value += 22
            elif name == 'grate':
                value = grain*.3
                if x % 12 < 3: rgb, value = (14, 12, 12), 0
                elif y % 48 < 2: value += 20
            elif name == 'trim':
                value = grain*.25+(value_noise(x/128, y/2, 2, seed+5)-.5)*12
                band = y % 32
                if band == 0: value += 44
                elif band == 1: value += 16
                elif band >= 30: value -= 26
            elif name == 'ember':
                value = grain*.5+(value_noise(x/32, y/32, 8, seed+6)-.5)*22
                if noise(x//6, y//6, seed+7) > .9: rgb, value = (84, 40, 26), grain*.3
                if y % 64 == 0: value -= 24
            elif name == 'glacier':
                value = grain*.6+(value_noise(x/16, y/16, 16, seed+8)-.5)*26
                wx = (value_noise(x/32, y/32, 8, seed+9)-.5)*30
                if abs(((x+wx) % 64)-32) < .9 and noise(int(x+wx)//5, y//5, seed+10) > .5: value -= 40
            elif name == 'light':
                value = grain*.2+(18 if x % 32 < 2 else 0)
            elif name == 'bark':
                # Layered volcanic rock: warped bands, dark veins, pitting.
                warp = (value_noise(x/64, y/64, 4, seed+12)-.5)*40
                band = math.sin((y+warp)*math.tau/32)
                value = grain*.8+band*4+(value_noise(x/16, y/16, 16, seed+13)-.5)*18 \
                    + (value_noise(x/8, y/8, 32, seed+17)-.5)*10
                if abs(band) < .03 and noise(x//6, y//6, seed+18) > .4: rgb, value = (48, 30, 24), grain*.3
                if noise(x//3, y//3, seed+14) > .985: value -= 30
            elif name == 'leaf':
                value = grain*.5+(value_noise(x/32, y/32, 8, seed+15)-.5)*14+(value_noise(x/8, y/8, 32, seed+16)-.5)*8
            elif name == 'meadow':
                # Rust dust: fine grit, small pebbles, soft drifts.
                pebble = (noise(x//4, y//4, seed+21)-.5)*16
                drift = (value_noise(x/64, y/64, 4, seed+9)-.5)*16
                value = grain*.6+pebble+drift
                speck = noise(x, y, seed+15)
                if speck > .993: rgb, value = (52, 34, 28), 0
                elif speck < .004: value += 24
            elif name == 'rock':
                wander = (value_noise(y/64, x/64, 4, seed+3)-.5)*10
                col = (x+wander) % 32
                value = grain*.6+(value_noise(x/16, y/16, 16, seed+23)-.5)*14
                if col < 1.4: value -= 22
                elif col < 3: value += 12
                if y % 64 < 1 and noise(x//8, y//64, seed+12) > .5: value -= 16
            elif name == 'soil':
                value = grain*.4+(value_noise(x/32, y/32, 8, seed+13)-.5)*8
                wx = (value_noise(x/64, y/64, 4, seed+16)-.5)*22
                wy = (value_noise(x/64, y/64, 4, seed+19)-.5)*22
                sx, sy = (x+wx) % 64, (y+wy) % 128
                on_crack = min(sx, 64-sx) < 1.3 or min(sy, 128-sy) < 1.3
                if on_crack and noise(int(x+wx)//6, int(y+wy)//6, seed+8) > .38:
                    rgb, value = (230, 96, 30), 4
            elif name == 'timber':
                # Boards 16 px (0.25 m) wide along x, grain along them, a
                # dark gap between, a knot or two, nails at the board ends.
                board, row = y//16, y % 16
                grain = (value_noise(x/64, y/2, 4, seed+30+board)-.5)*22+(value_noise(x/16, y/1, 16, seed+31)-.5)*8
                value = grain+(noise(board, 0, seed+32)-.5)*18
                if row == 0: rgb, value = (24, 18, 14), 0
                elif row == 1: value -= 12
                kx = (noise(board, 1, seed+33)*256) % 256
                if (x-kx)**2+((row-8)*2)**2 < 14: value -= 22
                if x % 128 in (4, 5) and row in (7, 8): rgb, value = (40, 38, 36), 6
            elif name == 'sacking':
                # A coarse weave, threads every 3 px, a stitched seam every
                # 48 px and soft dirt.
                weave = (x % 3 == 0)*-10+(y % 3 == 0)*-8+((x//3+y//3) % 2)*6
                value = grain*.5+weave+(value_noise(x/32, y/32, 8, seed+34)-.5)*20
                if y % 48 in (0, 1) and x % 6 < 3: value -= 28
            elif name in ('bone', 'bone_grime', 'bone_dark'):
                # Even all over, fine detail only (the tile mustn't show: the
                # large stains come from where each kind of bone is laid and
                # from the light): porous grain, short uneven grime streaks
                # (v is down a wall), hairline cracks of even width along
                # wandering noise lines, sparse pits, specks of old gore.
                mid = value_noise(x/16, y/16, 16, seed+41)
                fine = value_noise(x/4, y/4, 64, seed+42)
                pores = value_noise(x/2, y/2, 128, seed+43)
                streak = stretched(noise, x/4, y/32, 64, 8, seed+44)**3*2.2
                def lines(x, y):
                    return value_noise(x/32+mid*.5, y/32+fine*.3, 8, seed+46)
                c0 = lines(x, y)
                grad = max(abs(lines(x+1, y)-c0), abs(lines(x, y+1)-c0), 1e-4)
                crack = abs(c0-.5)/grad < .7 and value_noise(x/8, y/8, 32, seed+48) > .45
                pit = noise(x//2, y//2, seed+50) > .996
                speck = noise(x, y//3, seed+52) > .997 and streak > .25
                tone = mid*.55+fine*.45
                if name == 'bone':
                    base = mix3((234, 232, 226), (190, 188, 182), clamp01((tone-.3)*1.6))
                    base = mix3(base, (120, 118, 114), clamp01(streak-.15)*.55)
                    blood, line = (110, 46, 36), (48, 46, 44)
                elif name == 'bone_grime':
                    base = mix3((172, 170, 164), (120, 118, 113), clamp01((tone-.3)*1.6))
                    base = mix3(base, (44, 42, 40), clamp01(streak-.12)*.65)
                    blood, line = (80, 28, 22), (22, 20, 19)
                else:
                    base = mix3((48, 46, 45), (78, 76, 73), clamp01((tone-.3)*1.6))
                    base = mix3(base, (18, 17, 16), clamp01(streak-.1)*.7)
                    blood, line = (56, 18, 14), (12, 11, 11)
                base = mix3(base, (base[0]*.8, base[1]*.8, base[2]*.8), clamp01((pores-.6)*4)*.6)
                if crack: base = line
                elif pit: base = mix3(line, base, .25)
                elif speck: base = blood
                rgb, value = base, grain*.3
            else:  # moss slot: dark ash gravel
                pebble = (noise(x//3, y//3, seed+25)-.5)*20
                value = grain*.6+pebble+(value_noise(x/32, y/32, 8, seed+26)-.5)*10
            data.extend(max(0, min(255, round(c+value))) for c in rgb)
            data.append(255)
    return bytes(data)
