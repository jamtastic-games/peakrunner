"""Original Dustreach team base: a ruined sandstone citadel on a raised
terrace. A keep with twin pylon towers holds the spawn hall and inventories;
behind it an arcaded courtyard surrounds the sunken flag court, open to the
sky. A watch tower carries the sentry turret and sensor, a broken watch ruin
on the right flank carries the plasma turret, and a raised caravan dais on
the left flank is the landing and deploy pad.

v2 adds a second building and an underground level:
- A cistern under the keep holds the generator. It has exactly two ways in:
  a ramp down from the keep hall and one tunnel door.
- The service tunnel runs from that door under the terrace, north then west,
  into a room in the base of the watch tower. A ramp climbs from there to a
  door in the tower's back face.
- A two-level storehouse on the right, joined to the courtyard by a bridge
  through a gate in the curtain, holds a third inventory and two spawns.
Capping stays on the surface: the flag court is untouched and the tunnel only
links the generator to the tower.

No source mesh, texture, lightmap or parser is used by this builder. Local
X/Z are horizontal, Y is up, the surrounding ground is Y=0. -Z is the front
(field-facing) side; facing the field, the right hand is +X.

The build blends the ground to Y=-0.1 under every structure and every outer
wall runs 3 m below that. The underground level sits in terrain holes (HOLES,
whole 8 m cells). Every hole cell lies under the terrace or the tower, so no
lid is exposed and the cut's edge stays at the flat site height.

Sightlines: the keep's doors and the watch tower's back door are open, so
attackers can shoot and fly straight in. Turrets face the field with a
limited field of fire (turret_arcs.py) instead of being walled off. Baffles
remain only inside the storehouse, a spawn room; the cistern (generator)
is reached only by the hall stair and the tunnel.

Dressing is render-only and stays within the 0.52 m player radius of a solid
surface. If the caller's mesh has a `lamps` list, light fixtures add bake
samples.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'dustreach-citadel-v5'

TER = 5.0                                   # terrace top
TX, TZ0, TZ1 = 21.2, -18.0, 30.0            # terrace half-width and depth
FOOT = -3.0                                 # outer walls run this far below ground
# Keep: walls and open doors.
KX, KZ0, KZ1 = 14.0, -16.0, 4.0
WALL = .8
CEIL, ROOF = 11.0, 12.0
DOOR, DOOR_TOP = (-3.0, 3.0), TER+5.5         # 6 m wide, 5.5 m tall, under the 6 m hall ceiling
# Pylon towers flanking the keep front.
PYLON = (14.0, 20.0, -18.5, -12.5)          # |x| from, to, z from, to (meets the curtain)
PYLON_TOP, PYLON_CAP = 19.0, 21.0
# Courtyard: arcades along both sides, curtain walls with broken tops.
ARCADE_X, ARCADE_Z = 16.5, (8.0, 14.4, 20.8, 27.2)   # ~5.2 m clear bays
ARCADE_ROOF = 11.4
CURTAIN = 20.0                              # inner face of the side curtains
BACK_WALL = (28.8, TZ1)
BREACH = (-4.0, 4.0)
CURTAIN_TOPS = (12.4, 13.6, 11.6, 13.0, 12.2)   # broken tops, front to back
# Sunken flag court.
PIT_X, PIT_Z = 7.0, (8.0, 26.0)
PIT_FLOOR, PIT_DOWN = 2.0, (14.0, 20.0)
FLAG = (0.0, 17.0)
PLINTH = .3
# Access ramps from the ground: front to the keep door, back through the breach.
FRONT_RAMP = (-4.0, 4.0, -34.0, TZ0)        # x0, x1, z at ground, z at terrace
BACK_RAMP = (-4.0, 4.0, 46.0, TZ1)
# Watch tower at the back-left corner. Its base is a hollow room at the
# tunnel's end; the rest is solid up to the sentry deck.
TOWER = (-32.0, -TX, 15.0, 31.0)
TOWER_TOP = 20.0
SENTRY, SENSOR = (-26.0, 28.0), (-27.0, 22.8)   # sentry set back so it cannot see down through the keep's open back door
# Watch ruin on the right flank, forward.
RUIN = (72.0, -64.0)
RUIN_R, RUIN_TOP = 5.5, 7.0
RUIN_RAMP_LEN = 14.0
# Caravan dais on the left flank.
PAD = (-84.0, -26.0)
PAD_TOP, PAD_HALF, PAD_LIP, PAD_GAP = 4.0, 12.0, .5, 3.0
PAD_SLOTS = ((-6.5, -6.5), (6.5, -6.5), (-6.5, 6.5), (6.5, 6.5))
# Fallen drums and a broken wall in front of the citadel: cover on the apron.
DRUMS = ((-26.0, -46.0, 1.3), (-21.0, -50.0, 1.1), (30.0, -40.0, 1.2))
FRONT_RUIN = (18.0, -52.0, 10.0)            # centre x, z, length along x
SPAWN_LIFT = 1.2

# --- Underground level (v2) ---------------------------------------------------
# Terrain holes: whole 8 m cells. With the base origins used in maps/dustreach.json,
# local x must be a multiple of 8 and local z one less than a multiple of 8.
CIS_FLOOR, CIS_CEIL = -4.0, TER-1.0        # cistern floor top; underside of the terrace paving (8 m clear)
UFOOT = CIS_FLOOR-1.0                      # underground walls run this far below the floors
CIS = (-13.2, 13.2, -16.2, 6.2)            # cistern interior (walls to the hole edge)
GEN = (0.0, CIS_FLOOR, -5.0)
COLUMNS = ((-7.0, -12.0), (7.0, -12.0), (-7.0, 1.0), (7.0, 1.0))
HALL_HOLE = (9.3, 13.2, -5.0, 2.0)         # railed opening in the hall floor
STAIR = (9.3, 13.2, -14.2, 2.0)            # against the east wall: x0, x1, z at the cistern floor, z at the hall floor (29 deg)
TUN_CEIL = CIS_FLOOR+4.5
TUN_DOOR = (-13.2, -9.0)                   # the cistern's only tunnel door, in its north wall
TUNNEL_N = (-16.0, -8.0, 7.0, 23.0)        # cells; the tunnel runs 0.8 m inside them
TUNNEL_W = (-24.0, -16.0, 15.0, 23.0)
TOWER_HOLE = (-32.0, -24.0, 15.0, 31.0)
TOWER_ROOM_TOP = 4.5
TOWER_RAMP = (-31.2, -27.2, 18.5, 26.5)    # x0, x1, z at the room floor, z at the landing
LANDING = -0.1
TOWER_DOOR = (-30.5, -26.5)                # in the tower's back (+Z) face
HOLES = {'cistern': (-16.0, 16.0, -17.0, 7.0), 'tunnel north': TUNNEL_N,
         'tunnel west': TUNNEL_W, 'tower': TOWER_HOLE}
# --- Storehouse (v4): one 10 m hall right of the courtyard, with a mezzanine ---
# The west strip is a mezzanine at bridge level (entered through the curtain
# gate); a landing and ramp join it to the hall floor, so the room is entered
# from both levels and the rest is a single open, flyable volume.
STORE = (28.0, 48.0, 2.0, 22.0)
STORE_UP, STORE_ROOF = TER, 11.0           # mezzanine top (bridge level), roof top
MEZZ_X = 33.8                              # mezzanine: x from the west wall's inner face to here
STORE_LANDING = (MEZZ_X, 37.6, 14.8)       # x0, x1, z from which it runs to the north wall
STORE_RAMP = (MEZZ_X, 37.6, 4.8, 14.8)     # x0, x1, z at the hall floor, z at the mezzanine
STORE_DOOR_W, STORE_DOOR_W_TOP = (4.5, 10.5), STORE_UP-1.0   # under the mezzanine
STORE_DOOR_N, STORE_DOOR_N_TOP = (38.5, 44.5), 6.0
STORE_INV = (41.5, 0.0, 17.0)
CRATES = ((44.0, 12.0, 1.6), (31.0, 17.5, 1.4))
GATE_Z, GATE_TOP = (12.5, 18.5), TER+6.0   # curtain gate and bridge onto the mezzanine

HULL, INTERIOR, DECK, TIMBER, METAL, GLOW = 'concrete', 'bark', 'panel', 'grate', 'trim', 'light'


def _clamp01(v):
    return min(max(v, 0), 1)


def ramp_y(ramp, z, y_a, y_b):
    """Surface height of a straight ramp (x0, x1, z_a, z_b) at z."""
    _, _, za, zb = ramp
    return y_a+(y_b-y_a)*_clamp01((z-za)/(zb-za))


def front_ramp_y(z): return ramp_y(FRONT_RAMP, z, 0.0, TER)
def back_ramp_y(z): return ramp_y(BACK_RAMP, z, 0.0, TER)
def stair_y(z): return ramp_y(STAIR, z, CIS_FLOOR, TER)
def tower_ramp_y(z): return ramp_y(TOWER_RAMP, z, CIS_FLOOR, LANDING)
def store_ramp_y(z): return ramp_y(STORE_RAMP, z, 0.0, STORE_UP)


def pit_y(z):
    """Floor height over the sunken flag court and its two ramps."""
    z0, z1 = PIT_Z; d0, d1 = PIT_DOWN
    if z <= d0: return TER+(PIT_FLOOR-TER)*_clamp01((z-z0)/(d0-z0))
    if z >= d1: return TER+(PIT_FLOOR-TER)*_clamp01((z1-z)/(z1-d1))
    return PIT_FLOOR


def ruin_ramp():
    """The watch ruin's back ramp: x0, x1, z at the ground, z at the deck."""
    cx, cz = RUIN
    apothem = RUIN_R*math.cos(math.pi/8)
    return (cx-2.0, cx+2.0, cz+apothem+RUIN_RAMP_LEN, cz+apothem)


def sites():
    """Terrain surfaces to blend toward around the structures:
    (kind, shape, surface(x, z) -> local height, falloff metres). Shapes are
    ('rect', x0, x1, z0, z1) or ('disc', x, z, r) in local coordinates."""
    px, pz = PAD; rx, rz = RUIN
    flat = lambda x, z: -.1
    sx0, sx1, sz0, sz1 = STORE
    out = [('citadel', ('rect', TOWER[0]-4, TX+4, FRONT_RAMP[2]-4, BACK_RAMP[2]+4), flat, 20),
           ('storehouse', ('rect', TX, sx1+4, sz0-4, sz1+4), flat, 16),
           ('ruin', ('disc', rx, rz, RUIN_R+9), flat, 14),
           ('ruin ramp', ('rect', rx-8, rx+8, rz, ruin_ramp()[2]+6), flat, 12),
           ('pad', ('rect', px-PAD_HALF-6, px+PAD_HALF+6, pz-PAD_HALF-6, pz+PAD_HALF+6), flat, 14),
           ('apron ruin', ('rect', FRONT_RUIN[0]-FRONT_RUIN[2]/2-5, FRONT_RUIN[0]+FRONT_RUIN[2]/2+5,
                           FRONT_RUIN[1]-6, FRONT_RUIN[1]+6), flat, 10)]
    for x, z, r in DRUMS:
        out.append(('drum', ('disc', x, z, r+5), flat, 8))
    return out


def _runs_without(lo, hi, gaps):
    out, u = [], lo
    for a, c in sorted(gaps):
        if a > u: out.append((u, a))
        u = max(u, c)
    if hi > u: out.append((u, hi))
    return out


def _merlon_spans(u0, u1, width=1.0, gap=1.4):
    n = max(1, int((u1-u0+gap)//(width+gap)))
    if n == 1: return [((u0+u1-width)/2, (u0+u1+width)/2)]
    step = (u1-u0-width)/(n-1)
    return [(u0+i*step, u0+i*step+width) for i in range(n)]


def build(mesh, team, circuit):
    if team not in (0, 1) or not circuit: raise ValueError('team and circuit required')
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=INTERIOR, metal=METAL, glow=GLOW)
    wall, slab = b.wall, b.slab

    def closed_side(x, z0, z1, surface, bottom=-.3):
        """Double-faced panel under a ramp edge from `bottom` up to 0.6 m
        under the surface, so nothing can be walked into beneath it."""
        u0, u1 = surface(z0)-.6, surface(z1)-.6
        if u0 < bottom:
            z0 = z0+(bottom-u0)*(z1-z0)/(u1-u0); u0 = bottom
        if u1 < bottom:
            z1 = z1+(bottom-u1)*(z0-z1)/(u0-u1); u1 = bottom
        pts = [(x, bottom, z0), (x, u0, z0), (x, u1, z1), (x, bottom, z1)]
        pts = [q for i, q in enumerate(pts) if q != pts[i-1]]
        if len(pts) == 3: mesh.triangle(pts, HULL); mesh.triangle(pts[::-1], HULL)
        else: mesh.quad(*pts, HULL); mesh.quad(*pts[::-1], HULL)

    def crenels(axis, fixed0, fixed1, u0, u1, y, heights=None):
        for i, (a, c) in enumerate(_merlon_spans(u0, u1)):
            h = .9 if heights is None else heights[i % len(heights)]
            if h <= 0: continue
            if axis == 'x': wall(a, c, fixed0, fixed1, y, y+h, HULL)
            else: wall(fixed0, fixed1, a, c, y, y+h, HULL)

    def collar(x, y, z, r):
        b.prism(x, z, r+.15, r, y, y+1.2, 8, HULL, solid=False, cap=False)
        b.prism(x, z, r+.08, r+.08, y+.8, y+1.0, 8, METAL, solid=False, cap=False)

    # --- Terrace ------------------------------------------------------------
    # Outer walls from below the ground up to the terrace top, and the paving,
    # tiled without overlaps round the hole of the sunken flag court.
    # The outer walls rise to the paving top, and the paving fills only the
    # space inside them, so no paving edge shares a face with a wall above it
    # (the old overlap flickered along the curtains' outer faces).
    wall(-TX, TX, TZ0, TZ0+1, FOOT, TER, HULL)
    wall(TX-1, TX, TZ0+1, BACK_WALL[0], FOOT, TER, HULL)
    # The west side wall lets the service tunnel through into the tower.
    tw0, tw1 = TUNNEL_W[2]+.8, TUNNEL_W[3]-.8
    for za, zb in ((TZ0+1, tw0), (tw1, BACK_WALL[0])): wall(-TX, -(TX-1), za, zb, FOOT, TER, HULL)
    wall(-TX, -(TX-1), tw0, tw1, TUN_CEIL, TER, HULL)
    wall(-TX, TX, BACK_WALL[0], TZ1, FOOT, TER, HULL)
    z0, z1 = PIT_Z
    hx0, hx1, hz0, hz1 = HALL_HOLE
    px, pz0, pz1 = TX-1, TZ0+1, BACK_WALL[0]
    for x0, x1, za, zb in [(-px, hx0, pz0, KZ1), (hx1, px, pz0, KZ1), (hx0, hx1, pz0, hz0), (hx0, hx1, hz1, KZ1),
                           (-px, -PIT_X, KZ1, pz1), (PIT_X, px, KZ1, pz1),
                           (-PIT_X, PIT_X, KZ1, z0), (-PIT_X, PIT_X, z1, pz1)]:
        slab(x0, x1, za, zb, TER, DECK)

    # --- Sunken flag court ----------------------------------------------------
    d0, d1 = PIT_DOWN
    slab(-PIT_X, PIT_X, d0, d1, PIT_FLOOR, DECK)
    # 5 cm clear of the pit's side faces, so the ramps' side plates never
    # share their plane.
    mesh.ramp(0, 2*PIT_X-.1, z0, d0, TER, PIT_FLOOR, TIMBER)
    mesh.ramp(0, 2*PIT_X-.1, z1, d1, TER, PIT_FLOOR, TIMBER)
    for s in (-1, 1): wall(s*PIT_X, s*(PIT_X+.6), z0, z1, PIT_FLOOR-1, TER-1, HULL)
    fx, fz = FLAG
    b.prism(fx, fz, 1.8, 1.5, PIT_FLOOR, PIT_FLOOR+PLINTH, 12, METAL)
    b.prism(fx, fz, 1.2, 1.2, PIT_FLOOR+PLINTH, PIT_FLOOR+PLINTH+.02, 12, accent, solid=False)
    b.prism(fx, fz, .4, .4, PIT_FLOOR+PLINTH+.02, PIT_FLOOR+PLINTH+.04, 12, GLOW, solid=False)
    b.lamp((fx, PIT_FLOOR+2.4, fz), .6)
    # Sun-disc inlay round the plinth and a bronze-dark band on the pit walls.
    b.prism(fx, fz, 3.0, 3.0, PIT_FLOOR+.005, PIT_FLOOR+.02, 20, METAL, solid=False)
    b.prism(fx, fz, 2.6, 2.6, PIT_FLOOR+.02, PIT_FLOOR+.03, 20, DECK, solid=False)
    for s in (-1, 1):
        b.face_box('x', s*PIT_X, -s, d0, d1, PIT_FLOOR, PIT_FLOOR+.3, .1, METAL)
        b.face_quad('x', s*PIT_X, -s, d0, d1, PIT_FLOOR+1.0, PIT_FLOOR+1.3, accent)
        b.face_box('x', s*PIT_X, -s, z0, z1, TER-.25, TER+.02, .14, METAL)
    # Corner braziers on the rim: bowls of glow, lit for the bake.
    for x in (-PIT_X-1.4, PIT_X+1.4):
        for z in (z0-1.4, z1+1.4):
            b.prism(x, z, .5, .38, TER, TER+.9, 8, METAL)
            b.prism(x, z, .62, .5, TER+.9, TER+1.2, 8, METAL, solid=False)
            b.prism(x, z, .42, .42, TER+1.15, TER+1.22, 8, GLOW, solid=False)   # proud of the bowl's rim
            b.lamp((x, TER+1.8, z), .5)

    # --- Keep -----------------------------------------------------------------
    ix, iz0, iz1 = KX-WALL, KZ0+WALL, KZ1-WALL
    for s in (-1, 1): wall(s*ix, s*KX, KZ0, KZ1, TER, CEIL, HULL)
    for zf, zb in [(KZ0, iz0), (iz1, KZ1)]:
        for x0, x1 in [(-ix, DOOR[0]), (DOOR[1], ix)]: wall(x0, x1, zf, zb, TER, CEIL, HULL)
        wall(DOOR[0], DOOR[1], zf, zb, DOOR_TOP, CEIL, HULL)
    slab(-KX, KX, KZ0, KZ1, ROOF, HULL)
    # Dressing: plaster liners, iron base and cornice, team stripe.
    b.dress('z', iz0, 1, _runs_without(-ix, ix, [DOOR]), TER, CEIL)
    b.dress('z', iz1, -1, _runs_without(-ix, ix, [DOOR]), TER, CEIL)
    for s in (-1, 1): b.dress('x', s*ix, -s, [(iz0, iz1)], TER, CEIL)
    for z, into_list in [(iz0, (1,)), (iz1, (-1,))]:
        for into in into_list:
            b.face_box('z', z, into, DOOR[0]-.35, DOOR[0], TER, DOOR_TOP, .12, METAL)
            b.face_box('z', z, into, DOOR[1], DOOR[1]+.35, TER, DOOR_TOP, .12, METAL)
            b.face_box('z', z, into, DOOR[0]-.35, DOOR[1]+.35, DOOR_TOP, DOOR_TOP+.35, .12, METAL)
    b.ceiling_strip('x', iz0+1.6, -ix+1, ix-1, CEIL, .45)
    b.ceiling_strip('x', iz1-1.6, -ix+1, ix-1, CEIL, .45)
    for x in (-6, 6): b.ceiling_strip('z', x, iz0+3, iz1-3, CEIL, .7)
    # Hall cover and inventories. The generator is below, in the cistern.
    wall(-.8, .8, -6.8, -5.2, TER, TER+2.6, HULL)
    wall(-.9, .9, -6.9, -5.1, TER+2.6, TER+2.74, METAL, False)
    wall(-.84, .84, -6.84, -5.16, TER+1.6, TER+1.78, accent, False)
    for z in (-10.0, -4.5): mesh.equipment('inventory', (-10, TER, z), team, circuit)
    # Railed opening in the hall floor over the stair down to the cistern.
    wall(hx0-.4, hx0, hz0-.4, hz1, TER, TER+1.1, METAL)
    wall(hx0, hx1, hz0-.4, hz0, TER, TER+1.1, METAL)
    wall(hx0-.46, hx0+.06, hz0-.46, hz1, TER+1.1, TER+1.18, accent, False)
    b.face_box('z', hz1, 1, hx0, hx1, TER-.02, TER+.02, .3, METAL)   # iron lip at the stair head
    # Roof: parapet with broken crenels, and the roof turret.
    t = .5
    wall(-KX, KX, KZ0, KZ0+t, ROOF, ROOF+.6, HULL)
    wall(-KX, KX, KZ1-t, KZ1, ROOF, ROOF+.6, HULL)
    for s in (-1, 1): wall(s*(KX-t), s*KX, KZ0+t, KZ1-t, ROOF, ROOF+.6, HULL)
    crenels('x', KZ1-t, KZ1, -KX+.6, KX-.6, ROOF+.6, (.9, .9, 0, .9, .5, .9))
    roof_turret = (0.0, ROOF, -12.5)
    mesh.equipment('turret', roof_turret, team, circuit)
    collar(*roof_turret, 2.2)

    # --- Pylon towers and facade --------------------------------------------
    p0, p1, pz0, pz1 = PYLON
    for s in (-1, 1):
        a, c = sorted((s*p0, s*p1))
        wall(a, c, pz0, pz1, FOOT, PYLON_TOP, HULL)
        a2, c2 = sorted((s*(p0+.8), s*(p1-.8)))
        wall(a2, c2, pz0+.8, pz1-.8, PYLON_TOP, PYLON_CAP, HULL)
        crenels('x', pz0, pz0+.6, min(a, c)+.3, max(a, c)-.3, PYLON_TOP, (.9, 0, .9))
        # Iron bands, a tall banner and slit windows (render only).
        for y in (TER+.4, 13.0, PYLON_TOP-.6):
            b.face_box('z', pz0, -1, a, c, y, y+.35, .15, METAL)
        cx = (a+c)/2
        mesh.quad((cx-1.4, TER+2.0, pz0-.05), (cx-1.4, 16.8, pz0-.05), (cx+1.4, 16.8, pz0-.05),
                  (cx+1.4, TER+2.0, pz0-.05), accent, False)
        b.face_box('z', pz0, -1, cx-.2, cx+.2, 17.2, 18.4, .05, GLOW)
        b.lamp((cx, 15, pz0-2.5), .7)
    # Keep front between the pylons: a lintel band, sun emblem over the door.
    fz0 = KZ0
    b.face_box('z', fz0, -1, -KX, KX, CEIL-.4, CEIL, .2, METAL)
    b.face_box('z', fz0, -1, DOOR[0]-.5, DOOR[1]+.5, DOOR_TOP, DOOR_TOP+.5, .25, METAL)
    ey = CEIL-2.2
    for r, mat in [(1.9, METAL), (1.5, GLOW), (.9, accent)]:
        pts = [(r*math.cos(i*math.tau/16), ey+r*math.sin(i*math.tau/16), fz0-.06-(1.9-r)*.02) for i in range(16)]
        for i in range(16):
            mesh.triangle([(0, ey, pts[i][2]), pts[(i+1) % 16], pts[i]], mat, False)
    b.lamp((0, ey, fz0-2.5), .7)
    for x in (DOOR[0]-1.2, DOOR[1]+1.2):
        wall(x-.2, x+.2, fz0-.4, fz0, TER+3.0, TER+3.3, METAL, False)
        wall(x-.14, x+.14, fz0-.36, fz0-.04, TER+3.3, TER+3.8, GLOW, False)
        b.lamp((x, TER+3.6, fz0-.7), .5)

    # --- Courtyard: arcades and curtain walls ---------------------------------
    for s in (-1, 1):
        for z in ARCADE_Z:
            b.prism(s*ARCADE_X, z, .62, .55, TER, ARCADE_ROOF-.6, 8, HULL, phase=math.pi/8)
            b.prism(s*ARCADE_X, z, .8, .8, ARCADE_ROOF-1.0, ARCADE_ROOF-.6, 8, HULL, phase=math.pi/8, solid=False)
        a, c = sorted((s*(ARCADE_X-.7), s*CURTAIN))
        slab(a, c, KZ1, BACK_WALL[0], ARCADE_ROOF, HULL, .6)
        b.face_box('x', s*(ARCADE_X-.7), -s, KZ1, BACK_WALL[0], ARCADE_ROOF-.6, ARCADE_ROOF-.3, .1, METAL)
        b.ceiling_strip('z', s*(ARCADE_X+1.8), KZ1+2, BACK_WALL[0]-2, ARCADE_ROOF-.6, .45, spacing=5)
        # Curtain: runs from the keep's side to the back wall, broken tops.
        a, c = sorted((s*CURTAIN, s*TX))
        seg = (BACK_WALL[0]-KZ0)/len(CURTAIN_TOPS)
        gate = [GATE_Z] if s > 0 else []   # the east curtain opens onto the storehouse bridge
        for i, top in enumerate(CURTAIN_TOPS if s > 0 else CURTAIN_TOPS[::-1]):
            za, zb = KZ0+i*seg, KZ0+(i+1)*seg
            for ra, rb in _runs_without(za, zb, gate): wall(a, c, ra, rb, TER, top, HULL)
            for ga, gb in gate:
                if za < ga and gb < zb: wall(a, c, ga, gb, GATE_TOP, top, HULL)
        b.dress('x', s*CURTAIN, -s, _runs_without(KZ1, BACK_WALL[0], gate), TER, ARCADE_ROOF-.6, pilasters=False)
    # Gate frame on both faces of the east curtain.
    for plane, into in ((CURTAIN, -1), (TX, 1)):
        b.face_box('x', plane, into, GATE_Z[0]-.35, GATE_Z[0], TER, GATE_TOP, .12, METAL)
        b.face_box('x', plane, into, GATE_Z[1], GATE_Z[1]+.35, TER, GATE_TOP, .12, METAL)
        b.face_box('x', plane, into, GATE_Z[0]-.35, GATE_Z[1]+.35, GATE_TOP, GATE_TOP+.35, .12, METAL)
    # Back wall with the breach, jagged either side of it.
    bz0, bz1 = BACK_WALL
    for x0, x1, top in [(-TX, -9.0, 12.0), (-9.0, BREACH[0], 9.5), (BREACH[1], 9.0, 10.5), (9.0, TX, 12.8)]:
        wall(x0, x1, bz0, bz1, TER, top, HULL)
    for x0, x1, y in [(BREACH[0]-1.2, BREACH[0], 7.4), (BREACH[1], BREACH[1]+1.0, 8.0)]:
        wall(x0, x1, bz0, bz1, TER, y, HULL)

    # --- Access ramps ---------------------------------------------------------
    for ramp, fn in [(FRONT_RAMP, front_ramp_y), (BACK_RAMP, back_ramp_y)]:
        x0, x1, zg, zt = ramp
        mesh.ramp((x0+x1)/2, x1-x0, zg, zt, 0.0, TER, TIMBER)
        for x in (x0, x1): closed_side(x, *sorted((zg, zt)), fn)
        for x in (x0-.3, x1+.3):   # low stone kerbs (render only)
            ya, yb = fn(zg), fn(zt)
            mesh.quad((x, ya, zg), (x, yb, zt), (x, yb+.35, zt), (x, ya+.35, zg), METAL, False)
            mesh.quad((x, ya+.35, zg), (x, yb+.35, zt), (x, yb, zt), (x, ya, zg), METAL, False)

    # --- Watch tower ----------------------------------------------------------
    wx0, wx1, wz0, wz1 = TOWER
    w = WALL
    # Hollow base: the tunnel's end room, a ramp up to a landing and a door in
    # the back (+Z) face. Solid from the room ceiling to the sentry deck.
    slab(wx0+WALL, TOWER_HOLE[1], wz0+WALL, wz1-WALL, CIS_FLOOR, DECK)   # inside the walls, so no face is shared
    wall(wx0, wx1, wz0, wz0+w, UFOOT, TOWER_ROOM_TOP, HULL)
    for a, c in _runs_without(wx0, wx1, [TOWER_DOOR]): wall(a, c, wz1-w, wz1, UFOOT, TOWER_ROOM_TOP, HULL)
    wall(*TOWER_DOOR, wz1-w, wz1, UFOOT, LANDING, HULL)
    wall(wx0, wx0+w, wz0+w, wz1-w, UFOOT, TOWER_ROOM_TOP, HULL)
    wall(wx1-w, wx1, tw1, wz1-w, UFOOT, TOWER_ROOM_TOP, HULL)
    wall(wx1-w, wx1, tw0, tw1, TUN_CEIL, TOWER_ROOM_TOP, HULL)
    rx = TOWER_HOLE[1]
    wall(rx-w, rx, tw1, wz1-w, CIS_FLOOR, TOWER_ROOM_TOP, HULL)        # closes the pocket over solid ground
    wall(rx-w, rx, tw0, tw1, TUN_CEIL, TOWER_ROOM_TOP, HULL)           # header over the tunnel mouth
    wall(rx, wx1-w, tw1, TUNNEL_W[3], CIS_FLOOR, TUN_CEIL, HULL)       # tunnel's north wall inside the tower
    wall(wx0, wx1, wz0, wz1, TOWER_ROOM_TOP, TOWER_TOP, HULL)
    r0, r1, rz0, rz1 = TOWER_RAMP
    mesh.ramp((r0+r1)/2, r1-r0, rz0, rz1, CIS_FLOOR, LANDING, TIMBER)
    closed_side(r1, rz0, rz1, tower_ramp_y, bottom=CIS_FLOOR)
    wall(wx0+w, rx-w, rz1, wz1-w, CIS_FLOOR, LANDING, DECK)            # landing inside the door
    for into, x in ((1, wx0+w), (-1, rx-w)):
        b.dress('x', x, into, [(wz0+w, wz1-w)] if into > 0 else [(tw1, wz1-w)], CIS_FLOOR, TOWER_ROOM_TOP, stripe=False)
    b.dress('z', wz0+w, 1, [(wx0+w, rx-w)], CIS_FLOOR, TOWER_ROOM_TOP, stripe=False)
    b.ceiling_strip('z', (wx0+rx)/2, wz0+2, wz1-2, TOWER_ROOM_TOP, .55)
    for x in TOWER_DOOR:
        b.face_box('z', wz1, 1, x-.35 if x == TOWER_DOOR[0] else x, x if x == TOWER_DOOR[0] else x+.35,
                   LANDING, TOWER_ROOM_TOP, .12, METAL)
    b.lamp(((TOWER_DOOR[0]+TOWER_DOOR[1])/2, 3.6, wz1+1.5), .45)
    t = .5
    for x0, x1, za, zb in [(wx0, wx1, wz0, wz0+t), (wx0, wx1, wz1-t, wz1), (wx0, wx0+t, wz0+t, wz1-t),
                           (wx1-t, wx1, wz0+t, wz1-t)]:
        wall(x0, x1, za, zb, TOWER_TOP, TOWER_TOP+.6, HULL)
    crenels('x', wz0, wz0+t, wx0+.5, wx1-.5, TOWER_TOP+.6)
    crenels('z', wx0, wx0+t, wz0+.8, wz1-.8, TOWER_TOP+.6)
    for y in (TER+.4, 12.5, TOWER_TOP-.6):
        b.face_box('z', wz0, -1, wx0, wx1, y, y+.35, .15, METAL)
        b.face_box('x', wx0, -1, wz0, wz1, y, y+.35, .15, METAL)
    bz = (wz0+wz1)/2
    mesh.quad((wx0-.05, 8.0, bz+3.2), (wx0-.05, 17.5, bz+3.2), (wx0-.05, 17.5, bz-3.2),
              (wx0-.05, 8.0, bz-3.2), accent, False)
    sentry = (SENTRY[0], TOWER_TOP, SENTRY[1])
    mesh.equipment('turret', sentry, team, circuit)
    collar(*sentry, 2.2)
    mesh.equipment('sensor', (SENSOR[0], TOWER_TOP, SENSOR[1]), team, circuit)
    b.prism(SENSOR[0], SENSOR[1], 1.95, 1.8, TOWER_TOP, TOWER_TOP+1.1, 8, HULL, solid=False, cap=False)

    # --- Cistern: the generator room under the keep ---------------------------
    # Two ways in: the stair from the hall and the tunnel door. Nothing else.
    cx0, cx1, cz0, cz1 = CIS
    hole = HOLES['cistern']
    slab(*hole, CIS_FLOOR, DECK)
    wall(hole[0], hole[1], hole[2], cz0, CIS_FLOOR, CIS_CEIL, HULL)
    for a, c in _runs_without(hole[0], hole[1], [TUN_DOOR]): wall(a, c, cz1, hole[3], CIS_FLOOR, CIS_CEIL, HULL)
    wall(*TUN_DOOR, cz1, hole[3], TUN_CEIL, CIS_CEIL, HULL)
    for s in (-1, 1): wall(s*cx1, s*(cx1+WALL), cz0, cz1, CIS_FLOOR, CIS_CEIL, HULL)
    # Stair from the hall: a straight ramp under the railed opening, closed
    # beneath so nothing can be walked into under its low end.
    sx0_, sx1_, sz_lo, sz_hi = STAIR
    mesh.ramp((sx0_+sx1_)/2, sx1_-sx0_, sz_lo, sz_hi, CIS_FLOOR, TER, TIMBER)
    # Closed only where a body could fit under the stair (surface > 1.1 m up),
    # so its low end is an open 2 m walk-off onto the cistern floor.
    z_closed = sz_lo+(1.1/(TER-CIS_FLOOR))*(sz_hi-sz_lo)
    closed_side(sx0_, z_closed, sz_hi, stair_y, bottom=CIS_FLOOR)
    for x, z in COLUMNS:
        b.prism(x, z, .9, .9, CIS_FLOOR, CIS_CEIL, 8, HULL, phase=math.pi/8)
        b.prism(x, z, 1.1, 1.1, CIS_FLOOR, CIS_FLOOR+.4, 8, METAL, phase=math.pi/8, solid=False)
        b.prism(x, z, 1.15, .95, CIS_CEIL-.5, CIS_CEIL, 8, METAL, phase=math.pi/8, solid=False, cap=False)
    mesh.equipment('generator', GEN, team, circuit)
    gx, _, gz = GEN
    b.prism(gx, gz, 4.6, 4.6, CIS_FLOOR+.01, CIS_FLOOR+.03, 20, METAL, solid=False)   # basin ring
    b.prism(gx, gz, 4.2, 4.2, CIS_FLOOR+.03, CIS_FLOOR+.04, 20, accent, solid=False)
    b.dress('z', cz0, 1, [(cx0, cx1)], CIS_FLOOR, CIS_CEIL)
    b.dress('z', cz1, -1, _runs_without(cx0, cx1, [TUN_DOOR]), CIS_FLOOR, CIS_CEIL)
    b.dress('x', cx0, 1, [(cz0, cz1)], CIS_FLOOR, CIS_CEIL)
    b.dress('x', cx1, -1, [(cz0, STAIR[2]), (STAIR[3], cz1)], CIS_FLOOR, CIS_CEIL)
    for x in (-8.0, 0.0, 5.0): b.ceiling_strip('z', x, cz0+1.5, cz1-1.5, CIS_CEIL, .7)
    for x0 in (TUN_DOOR[0], TUN_DOOR[1]):
        for into in (-1, 1):
            b.face_box('z', cz1 if into < 0 else hole[3], into, x0-.3 if x0 == TUN_DOOR[0] else x0,
                       x0 if x0 == TUN_DOOR[0] else x0+.3, CIS_FLOOR, TUN_CEIL, .12, METAL)
    b.face_box('z', cz1, -1, TUN_DOOR[0]-.3, TUN_DOOR[1]+.3, TUN_CEIL, TUN_CEIL+.3, .14, METAL)
    b.face_box('z', cz1, -1, TUN_DOOR[0]+.4, TUN_DOOR[1]-.4, TUN_CEIL+.5, TUN_CEIL+.8, .06, accent)

    # --- Service tunnel: cistern -> north under the terrace -> west to the tower
    n0, n1, nz0, nz1 = TUNNEL_N
    slab(*TUNNEL_N, CIS_FLOOR, DECK)
    wall(n0, n0+WALL, nz0, tw0, CIS_FLOOR, TUN_CEIL, HULL)
    wall(n1-WALL, n1, nz0, nz1, CIS_FLOOR, TUN_CEIL, HULL)
    wall(n0, n1-WALL, tw1, nz1, CIS_FLOOR, TUN_CEIL, HULL)
    slab(*TUNNEL_N, TUN_CEIL+.5, HULL, .5)
    # The west tunnel's floor runs between its side walls only: full width it
    # would share the tower front wall's faces where it enters the tower.
    slab(TUNNEL_W[0], TUNNEL_W[1], tw0, tw1, CIS_FLOOR, DECK)
    wall(-(TX-1), n0, TUNNEL_W[2], tw0, CIS_FLOOR-1, TUN_CEIL, HULL)
    wall(-(TX-1), n0, tw1, TUNNEL_W[3], CIS_FLOOR-1, TUN_CEIL, HULL)
    slab(*TUNNEL_W, TUN_CEIL+.5, HULL, .5)
    ni0, ni1 = n0+WALL, n1-WALL
    b.dress('x', ni0, 1, [(nz0, tw0)], CIS_FLOOR, TUN_CEIL)
    b.dress('x', ni1, -1, [(nz0, tw1)], CIS_FLOOR, TUN_CEIL)
    b.dress('z', tw1, -1, [(TUNNEL_W[0], ni1)], CIS_FLOOR, TUN_CEIL)
    b.dress('z', tw0, 1, [(-(TX-1), n0)], CIS_FLOOR, TUN_CEIL)
    b.ceiling_strip('z', (ni0+ni1)/2, nz0+1, tw1-1, TUN_CEIL, .5, spacing=5)
    b.ceiling_strip('x', (tw0+tw1)/2, TUNNEL_W[0]+.5, n0-.5, TUN_CEIL, .5, spacing=5)

    # --- Storehouse: one tall hall with a mezzanine, right of the courtyard ----
    s0, s1, sz0, sz1 = STORE
    i0, i1, iz0, iz1 = s0+WALL, s1-WALL, sz0+WALL, sz1-WALL
    top = STORE_ROOF-1
    dw, dn = STORE_DOOR_W, STORE_DOOR_N
    slab(s0, s1, sz0, sz1, 0.0, DECK)
    # West wall: a ground door under the mezzanine and the gate onto it.
    for za, zb in _runs_without(sz0, sz1, [dw, GATE_Z]): wall(s0, i0, za, zb, 0.0, top, HULL)
    wall(s0, i0, *dw, STORE_DOOR_W_TOP, top, HULL)
    wall(s0, i0, *GATE_Z, 0.0, STORE_UP, HULL)
    wall(i1, s1, sz0, sz1, 0.0, top, HULL)
    wall(i0, i1, sz0, iz0, 0.0, top, HULL)
    for xa, xb in _runs_without(i0, i1, [dn]): wall(xa, xb, iz1, sz1, 0.0, top, HULL)
    wall(*dn, iz1, sz1, STORE_DOOR_N_TOP, top, HULL)
    slab(s0, s1, sz0, sz1, STORE_ROOF, HULL)
    # Mezzanine along the west wall, a landing at its north end and a ramp
    # down to the hall floor, closed beneath on both sides.
    lx0, lx1, lz0 = STORE_LANDING
    slab(i0, MEZZ_X, iz0, iz1, STORE_UP, DECK)
    slab(lx0, lx1, lz0, iz1, STORE_UP, DECK)
    r0, r1, rzg, rzt = STORE_RAMP
    mesh.ramp((r0+r1)/2, r1-r0, rzg, rzt, 0.0, STORE_UP, TIMBER)
    for x in (r0, r1): closed_side(x, rzg, rzt, store_ramp_y, bottom=0.0)
    b.face_box('x', MEZZ_X, 1, iz0, rzg, STORE_UP-.3, STORE_UP+.02, .1, METAL)   # iron edge on the mezzanine
    b.face_box('x', lx1, 1, lz0, iz1, STORE_UP-.3, STORE_UP+.02, .1, METAL)
    # Baffles (a spawn room): inside the ground door, the gate and the north door.
    wall(i0+2.4, i0+2.8, dw[0]-1.5, dw[1]+1.5, 0.0, STORE_DOOR_W_TOP, HULL)
    wall(i0+2.4, i0+2.8, iz0+2.2, GATE_Z[1]+1.5, STORE_UP, top, HULL)   # long: shields the mezzanine spawns from the gate
    wall(lx1, dn[1]+1.0, iz1-2.8, iz1-2.4, 0.0, STORE_DOOR_N_TOP, HULL)
    for x, z, h in CRATES:
        wall(x-h/2, x+h/2, z-h/2, z+h/2, 0.0, h, TIMBER)
        wall(x-h/2-.04, x+h/2+.04, z-h/2-.04, z+h/2+.04, h*.45, h*.55, METAL, False)
    mesh.equipment('inventory', STORE_INV, team, circuit)
    # Dressing: liners, light strips, the team stripe.
    b.dress('z', iz0, 1, [(i0, i1)], 0.0, top)
    b.dress('x', i1, -1, [(iz0, iz1)], 0.0, top)
    b.dress('x', i0, 1, _runs_without(iz0, iz1, [dw]), 0.0, STORE_UP-1)
    b.dress('x', i0, 1, _runs_without(iz0, iz1, [GATE_Z]), STORE_UP, top)
    b.dress('z', iz1, -1, _runs_without(i0, i1, [dn]), 0.0, top)
    for x in (40.0, 44.5): b.ceiling_strip('z', x, iz0+1.5, iz1-1.5, top, .6, spacing=5)
    b.ceiling_strip('z', (i0+MEZZ_X)/2, iz0+1.5, iz1-1.5, STORE_UP-1, .45, spacing=5)   # under the mezzanine
    # Exterior: iron bands, a parapet, banners on the field and courtyard faces.
    for y in (.4, STORE_UP-.3, top-.5):
        b.face_box('z', sz0, -1, s0, s1, y, y+.35, .15, METAL)
        b.face_box('x', s1, 1, sz0, sz1, y, y+.35, .15, METAL)
        for za, zb in _runs_without(sz0, sz1, [dw, GATE_Z]):
            b.face_box('x', s0, -1, za, zb, y, y+.35, .15, METAL)
    t = .5
    for x0, x1, za, zb in [(s0, s1, sz0, sz0+t), (s0, s1, sz1-t, sz1), (s0, s0+t, sz0+t, sz1-t), (s1-t, s1, sz0+t, sz1-t)]:
        wall(x0, x1, za, zb, STORE_ROOF, STORE_ROOF+.6, HULL)
    crenels('x', sz0, sz0+t, s0+.5, s1-.5, STORE_ROOF+.6, (.9, .9, 0, .9))
    crenels('z', s1-t, s1, sz0+.8, sz1-.8, STORE_ROOF+.6, (.9, 0, .9, .9))
    mid = (s0+s1)/2
    mesh.quad((mid-2.2, 1.5, sz0-.05), (mid-2.2, top-.8, sz0-.05), (mid+2.2, top-.8, sz0-.05),
              (mid+2.2, 1.5, sz0-.05), accent, False)
    for zz, (y0, y1) in ((dw, (0.0, STORE_DOOR_W_TOP)), (GATE_Z, (STORE_UP, top))):
        b.face_box('x', s0, -1, zz[0]-.35, zz[0], y0, y1, .12, METAL)
        b.face_box('x', s0, -1, zz[1], zz[1]+.35, y0, y1, .12, METAL)
        b.face_box('x', s0, -1, zz[0]-.35, zz[1]+.35, y1-.35, y1, .14, METAL)
        b.lamp((s0-2.0, y1-.6, (zz[0]+zz[1])/2), .45)
    b.face_box('z', sz1, 1, dn[0]-.35, dn[1]+.35, STORE_DOOR_N_TOP-.35, STORE_DOOR_N_TOP, .14, METAL)
    b.lamp(((dn[0]+dn[1])/2, STORE_DOOR_N_TOP-.6, sz1+2.0), .45)
    # Bridge from the curtain gate to the mezzanine gate.
    slab(TX, s0, GATE_Z[0], GATE_Z[1], STORE_UP, TIMBER, .6)
    for za, zb in ((GATE_Z[0]-.4, GATE_Z[0]), (GATE_Z[1], GATE_Z[1]+.4)):
        wall(TX, s0, za, zb, STORE_UP-.6, STORE_UP+1.1, METAL)
    for zc in GATE_Z:
        b.prism((TX+s0)/2, zc, .14, .14, STORE_UP+1.1, STORE_UP+2.2, 6, METAL, solid=False)
        b.prism((TX+s0)/2, zc, .22, .22, STORE_UP+2.2, STORE_UP+2.45, 8, GLOW, solid=False)
        b.lamp(((TX+s0)/2, STORE_UP+2.4, zc), .4)

    # --- Watch ruin, right flank ----------------------------------------------
    cx, cz = RUIN
    b.prism(cx, cz, RUIN_R+.8, RUIN_R, FOOT, RUIN_TOP, 8, HULL, phase=math.pi/8)
    b.prism(cx, cz, RUIN_R+.86, RUIN_R+.7, 1.2, 1.6, 8, METAL, phase=math.pi/8, solid=False, cap=False)
    r0, r1, zg, zt = ruin_ramp()
    ruin_ramp_y = lambda z: ramp_y((r0, r1, zg, zt), z, 0.0, RUIN_TOP)
    mesh.ramp(cx, r1-r0, zg, zt, 0.0, RUIN_TOP, TIMBER)
    for x in (r0, r1): closed_side(x, zt, zg, ruin_ramp_y)
    apothem = RUIN_R*math.cos(math.pi/8)
    verts = [(cx+(RUIN_R-.3)*math.cos(math.pi/8+i*math.tau/8), cz+(RUIN_R-.3)*math.sin(math.pi/8+i*math.tau/8))
             for i in range(8)]
    broken = (1.6, .8, 1.9, 0, 1.2, 2.2, 0, 1.4)       # a ruined, uneven crown
    for i in range(8):
        a, c = verts[i], verts[(i+1) % 8]
        if (a[1]+c[1])/2-cz > apothem*.9 or broken[i] <= 0: continue
        b.beam(a, c, .6, RUIN_TOP, RUIN_TOP+broken[i], HULL)
    ruin_turret = (cx, RUIN_TOP, cz)
    mesh.equipment('turret', ruin_turret, team, circuit, 'plasma')
    collar(*ruin_turret, 2.2)
    b.lamp((cx, RUIN_TOP+2, cz+3), .4)

    # --- Caravan dais: landing and deploy pad, left flank ---------------------
    px, pz = PAD
    H, tl = PAD_HALF, .5
    wall(px-H, px+H, pz-H, pz+H, PAD_TOP-1.2, PAD_TOP, DECK)
    wall(px-H+.4, px+H-.4, pz-H+.4, pz+H-.4, FOOT, PAD_TOP-1.2, HULL)
    b.face_box('z', pz-H, -1, px-H, px+H, PAD_TOP-1.2, PAD_TOP-.9, .12, METAL)
    for s in (-1, 1):
        for a, c in ((-H, -PAD_GAP), (PAD_GAP, H)):
            wall(px+a, px+c, pz+s*(H-tl), pz+s*H, PAD_TOP, PAD_TOP+PAD_LIP, HULL)
        for a, c in ((-H+tl, -PAD_GAP), (PAD_GAP, H-tl)):
            wall(px+s*(H-tl), px+s*H, pz+a, pz+c, PAD_TOP, PAD_TOP+PAD_LIP, HULL)
    def decal(x0, x1, z0_, z1_, mat):
        y = PAD_TOP+.02
        mesh.quad((px+x0, y, pz+z0_), (px+x0, y, pz+z1_), (px+x1, y, pz+z1_), (px+x1, y, pz+z0_), mat, False)
    k, w = 9.5, .5
    decal(-k, k, -k, -k+w, METAL); decal(-k, k, k-w, k, METAL)
    decal(-k, -k+w, -k+w, k-w, METAL); decal(k-w, k, -k+w, k-w, METAL)
    b.prism(px, pz, 3.2, 3.2, PAD_TOP+.01, PAD_TOP+.03, 16, accent, solid=False)
    b.prism(px, pz, 2.2, 2.2, PAD_TOP+.03, PAD_TOP+.04, 16, INTERIOR, solid=False)
    slots = []
    for sx, sz in PAD_SLOTS:
        hs, tt = 1.6, .22
        decal(sx-hs, sx+hs, sz-hs, sz-hs+tt, METAL); decal(sx-hs, sx+hs, sz+hs-tt, sz+hs, METAL)
        decal(sx-hs, sx-hs+tt, sz-hs+tt, sz+hs-tt, METAL); decal(sx+hs-tt, sx+hs, sz-hs+tt, sz+hs-tt, METAL)
        slots.append((px+sx, PAD_TOP, pz+sz))
    for sx in (-1, 1):
        for sz in (-1, 1):
            x, z = px+sx*(H-tl/2), pz+sz*(H-tl/2)
            b.prism(x, z, .12, .12, PAD_TOP+PAD_LIP, PAD_TOP+PAD_LIP+1.1, 6, METAL, solid=False)
            b.prism(x, z, .2, .2, PAD_TOP+PAD_LIP+1.1, PAD_TOP+PAD_LIP+1.35, 8, GLOW, solid=False)
            b.lamp((x, PAD_TOP+PAD_LIP+1.3, z), .5)

    # --- Apron cover: fallen drums and a broken wall --------------------------
    for x, z, r in DRUMS:
        b.prism(x, z, r, r, FOOT+2, 1.4, 10, HULL)
        b.prism(x, z, r+.1, r+.1, .5, .7, 10, METAL, solid=False, cap=False)
    fx_, fz_, length = FRONT_RUIN
    wall(fx_-length/2, fx_-length/6, fz_-.7, fz_+.7, FOOT+2, 4.6, HULL)
    wall(fx_-length/6, fx_+length/6, fz_-.7, fz_+.7, FOOT+2, 3.0, HULL)
    wall(fx_+length/6, fx_+length/2, fz_-.7, fz_+.7, FOOT+2, 1.6, HULL)

    lift = SPAWN_LIFT
    return {
        'flag': (fx, PIT_FLOOR+PLINTH+.05, fz),
        'spawn': (18.2, TER+lift, 17.6),
        # (x, y, z, local yaw): yaw 0 faces the base front (-Z). No spawn is in
        # the keep hall or the cistern: the stair down to the generator starts
        # in the hall, so every spawn is at least ~30 m on foot from its head.
        'spawn_points': [(18.2, TER+lift, 17.6, math.pi/2), (-18.2, TER+lift, 17.6, -math.pi/2),
                         (42.0, lift, 6.0, math.pi), (46.0, lift, 8.0, math.pi),
                         (32.6, STORE_UP+lift, 4.0, math.pi), (32.6, STORE_UP+lift, 20.2, 0.0),
                         (-18.2, TER+lift, 11.2, -math.pi/2), (px-5, PAD_TOP+lift, pz+4, 0.0)],
        'entrances': [(0.0, TER+.2, KZ0), (0.0, TER+.2, KZ1), (0.0, .2, FRONT_RAMP[2]), (0.0, .2, BACK_RAMP[2]),
                      (STORE[0], .2, sum(STORE_DOOR_W)/2), (sum(STORE_DOOR_N)/2, .2, STORE[3]),
                      (STORE[0], STORE_UP+.2, sum(GATE_Z)/2),
                      (sum(TOWER_DOOR)/2, LANDING+.2, TOWER[3])],
        'generator': GEN,
        'gen_entrances': [((HALL_HOLE[0]+HALL_HOLE[1])/2, TER+.2, HALL_HOLE[3]),
                          (sum(TUN_DOOR)/2, CIS_FLOOR+.2, CIS[3])],
        'roof_turret': roof_turret,
        'sentry': sentry,
        'ruin_turret': ruin_turret,
        'pad_deck': (px, PAD_TOP, pz),
        'deploy_slots': slots,
        'hall_view': (-11.5, TER+1.7, -11.8),
        'court_view': (-17.5, TER+1.7, 26.5),
        'exterior_view': (-46.0, 16.0, -78.0),
        'cistern_view': (-12.4, CIS_FLOOR+1.8, -15.4),
        'tunnel_view': (-12.0, CIS_FLOOR+1.7, 8.0),
        'tower_room_view': (-30.6, CIS_FLOOR+1.8, 16.4),
        'tower_exit': (sum(TOWER_DOOR)/2, LANDING, TOWER[3]),
        'store_view': (29.6, STORE_UP+1.7, 3.6),
        'store_ground_view': (46.4, 1.7, 3.6),
        'store_inventory': STORE_INV,
    }

