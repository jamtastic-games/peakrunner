"""Original Cairnhold team base: a stone bunker dug into the forward slope
behind a stepped, crenellated gatehouse facade; a covered trench sunk into the
hill, climbing to a guard hut on the knoll; a flag tower rising from the hut
with the open flag platform on top; a plasma battery on the team's right flank
and a paved landing/deploy ledge on its left flank.

No source mesh, texture, lightmap or parser is used by this builder. Local
X/Z are horizontal, Y is up, the bunker floor is Y=0. -Z is the front
(field-facing) side. Facing the field, the right hand is +X.

The bunker, trench and hut are dug into the ground: the build script cuts the
terrain cells over HOLES and pins each hole's boundary vertices to
`ring_height`, which every outer wall covers from below to above. Their outer
wall faces lie exactly on the 8 m cell lines, so the placement must put the
origin on a grid vertex in X and 4 m off one in Z (see maps/cairnhold.json).
The ground either side of the trench is held flush with its roof, so the trench
reads as sunk into the hill rather than standing on a causeway.

v3 moves the generator down into a stone vault under the hall. It has exactly
two ways in: a stair from the east side of the hall and a sally-port tunnel
that runs east, then south under the hillside to an exit house at the foot of
the plasma battery's ramp. The tunnel's lids lie flush with the ground it runs
under (the build blends the terrain to them and pins the cut's edge), except
at the exit house, which stands on the battery bench. No spawn is in the hall,
the vault or the tunnel: the spawns moved to the inventory and back rooms.

Sightlines: the bunker front door (behind its recessed portal) and the exit
house's door are open straight in, so attackers can shoot and fly through.
Turrets and the battery face the field with a limited field of fire
(turret_arcs.py) instead of being walled off. The tower is closed except for
the roof hatch over its upper ramp.

Interior dressing is render-only and stays within the 0.52 m player radius
of a solid surface. If the caller's mesh has a `lamps` list, light fixtures
add bake samples.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'cairnhold-base-v5'

# Bunker: footprint, walls, storey. Doorways are 6 m wide and run to the 6 m
# ceiling, so skiers and jetters fly straight through them (no lintel).
BX, BZ0, BZ1 = 16, -20, 20
WALL = .8
CEIL, ROOF = 6, 7
DOOR, DOOR_TOP = (-3.0, 3.0), 6.0
PART_A, PART_A_DOORS = (4.0, 4.4), [(-12, -6), (6, 12)]
PART_B, PART_B_DOOR = (12.0, 12.4), (-3, 3)
BACK_DOOR = (1, 7)
# v4 hall: behind the entry vestibule (under the facade) the hall rises to a
# 13 m ceiling as a two-level room. A 4 m gallery runs round its sides and
# back at the level of the hillside outside (ROOF), and a 6 x 6 m doorway in
# each side wall opens from the gallery straight onto that ground, so the hall
# is entered from both levels.
HALL_Z = (-13.0, PART_A[1])       # the tall part: facade back face to partition A
HALL_CEIL, HALL_ROOF = 13.0, 14.0
GALLERY_Y, GALLERY_W, GALLERY_THICK = float(ROOF), 4.0, .6
GALLERY_DOOR = (-7.0, -1.0)       # z span of each side wall's gallery doorway
# Facade: a stepped front block rising from the roof over the front 7 m, a
# gatehouse projecting 2 m around a recessed portal, and stone wing blocks
# retaining the hillside on either side.
FRONT_Z1 = -13.0                 # back face of the raised front block
GATE_X, GATE_Z = 6.0, -22.0      # gatehouse half-width and projecting face
PORTAL, PORTAL_TOP = (-4.2, 4.2), 7.6
TIERS = ((0.0, GATE_X, 17.0), (GATE_X, 11.0, 14.0), (11.0, BX, 11.5))   # |x| from, to, top
WING_X, WING_Z1 = 24.0, -12.0    # wing blocks: |x| BX..WING_X, z BZ0..WING_Z1, top ROOF
ROOF_TURRET = (0, TIERS[0][2], -17.0)
# Covered trench from the bunker's back wall up to the guard hut, its roof
# flush with the bunker roof at the bottom and the knoll at the top.
TX0, TX1, TZ0, TZ1 = 0, 8, 20, 76
TRENCH_RISE = 22.0
TRENCH_TOP = 27.5        # roof top at TZ1; ROOF at TZ0
TRENCH_WALL = 8.5        # outer wall height below the roof top
BANK = 10.0              # ground held flush with the roof this far to each side
LIGHT_SLOTS = (28, 40, 52, 64)
# Guard hut dug into the knoll; its roof is a terrace round the tower.
HX0, HX1, HZ0, HZ1 = -8, 16, 76, 100
HUT_FLOOR, HUT_CEIL, HUT_ROOF, HUT_RING = 22.0, 28.0, 29.0, 27.5
HUT_DOOR = (1, 7)
HUT_DOOR_TOP = 5.5
HATCH = (10, HX1-WALL, HZ0+4, HZ0+16)             # hut roof opening, inside the tower
RAMP = (10, HX1-WALL, HZ0+22, HZ0+4)             # hut ramp: x0, x1, low end z, high end z
# Flag tower on the hut's east half. Interior route: hut ramp -> tower floor
# (HUT_ROOF) -> R1 -> landing (TOWER_MID) -> R2 -> roof hatch -> platform.
TW = (2.0, 16.0, 76.0, 96.0)
TOWER_MID, TOWER_TOP = 35.0, 41.0
R1 = (2.8, 8.2, 80.0, 92.0)       # x0, x1, z at HUT_ROOF, z at TOWER_MID
R2 = (10.0, HX1-WALL, 92.0, 80.0)  # x0, x1, z at TOWER_MID, z at TOWER_TOP
LANDING_Z = (92.0, 95.2)
TOP_HATCH = (10.0, HX1-WALL, 80.0, 88.0)
FLAG = (6.0, 86.0)
PLINTH = .3
MAST, SENSOR = (13.5, 93.5), (4.5, 93.5)
# Exterior attack ramp from the knoll behind the hut up to a bridge onto the
# platform's west edge: x0, x1, z at the ground, z at the top.
XR = (-5.0, 0.0, 106.0, 79.0)
BRIDGE = (-5.0, TW[0], TW[2], 79.0)
PARAPET, MERLON = .5, .9
# Flank structures (ground levels are relative to the bunker floor).
BATTERY = (96, -64)
BATTERY_GROUND, BATTERY_R, BATTERY_H = 8.0, 6.2, 3.5
PAD = (-88, -36)
PAD_TOP, PAD_HALF, PAD_LIP, PAD_GAP = 5.0, 12, .5, 3
PAD_SLOTS = ((-6.5, -6.5), (6.5, -6.5), (-6.5, 6.5), (6.5, 6.5))
SPAWN_LIFT = 1.2       # player centre above the floor (support-ray convention)

# --- Vault and sally port (v3) ----------------------------------------------
# The generator moves down into a stone vault under the hall. Exactly two ways
# in: a stair from the east side of the hall, and the sally-port tunnel, which
# runs east and then south under the hillside to an exit house at the foot of
# the plasma battery's ramp. No spawn is in the hall, the vault or the tunnel.
VAULT_FLOOR, VAULT_CEIL = -7.5, -1.0       # floor top; underside of the hall floor slab (6.5 m)
VAULT = (3.2, 15.2, -9.8, 12.0)            # interior x0, x1, z0, z1
# The generator stands in a well 2.2 m below the vault floor: the kit model is
# 5.8 m tall and its hit bar hangs 3.9 m over its centre, so it needs ~7 m.
PIT_FLOOR = VAULT_FLOOR-2.2
PIT = (3.2, 10.4, 3.4, 12.0)               # well interior x0, x1, z0, z1
PIT_RAMP = (3.2, 10.4, -0.6, 3.4)          # x0, x1, z at the vault floor, z at the well floor
VAULT_GEN = (6.8, PIT_FLOOR, 7.7)
VAULT_PIERS = ((7.0, -6.5), (7.0, -3.0))
STAIR = (11.2, 15.2, -9.5, 4.5)            # x0, x1, z at the hall floor (head), z at the vault floor (foot); 28 degrees
HALL_HOLE = (11.2, 15.2, -9.5, -2.0)       # railed opening in the hall floor over the stair
TUN_H = 6.5                                # tunnel headroom (v4: was 4.5)
TUN_DOOR = (4.8, 11.2)                     # vault's east door onto the tunnel (z span)
# Two skylight light wells (drop-in grates) along the east leg: open shafts
# from the hillside down into the tunnel (x spans). They lead into the
# tunnel, not the vault, so the vault keeps exactly two ways in.
SKYLIGHTS = ((48.0, 52.0), (64.0, 68.0))
# Tunnel cells: east leg along +X (z 4..12), then south along -Z (x 80..88),
# ending in the exit house cell. Local x is on the 8 m grid, local z 4 m off it.
T_EAST = (16.0, 88.0, 4.0, 12.0)
T_SOUTH = (80.0, 88.0, -36.0, 4.0)
T_EXIT = (80.0, 88.0, -44.0, -36.0)
EXIT_GROUND = 8.0                          # the battery bench
EXIT_DOOR = (-43.0, -37.4)                 # z span of the door in the exit house's east wall
# Floor and lid (roof top, flush with the ground where the ground is higher)
# heights at the cell boundaries; linear between them. Each lid clears the
# tunnel's ceiling slab (floor + TUN_H + 0.5) by its own 0.5 m thickness.
T_EAST_FLOOR = ((16.0, VAULT_FLOOR), (17.0, VAULT_FLOOR), (45.0, 3.0), (88.0, 3.0))   # 20.6 degree climb
T_EAST_LID = ((16.0, 7.0), (24.0, 7.0), (32.0, 7.5), (40.0, 10.6), (48.0, 12.0), (56.0, 12.5),
              (64.0, 14.0), (72.0, 16.5), (80.0, 19.0), (88.0, 19.0))
T_SOUTH_FLOOR = ((-44.0, EXIT_GROUND), (-36.0, EXIT_GROUND), (-20.0, 3.0), (4.0, 3.0))
T_SOUTH_LID = ((-36.0, 15.5), (-28.0, 13.2), (-20.0, 12.8), (-12.0, 14.5), (-4.0, 16.5), (4.0, 19.0))
EXIT_ROOF = EXIT_GROUND+TUN_H+1.0

HOLES = {'bunker': (-BX, BX, BZ0, BZ1), 'trench': (TX0, TX1, TZ0, TZ1), 'hut': (HX0, HX1, HZ0, HZ1),
         'tunnel east': T_EAST, 'tunnel south': T_SOUTH, 'sally exit': T_EXIT}

HULL, INTERIOR, DECK, BRONZE, METAL, GLOW = 'concrete', 'bark', 'panel', 'grate', 'trim', 'light'


def _clamp01(v):
    return min(max(v, 0), 1)


def trench_floor(z):
    return TRENCH_RISE*_clamp01((z-TZ0)/(TZ1-TZ0))


def trench_roof(z):
    return ROOF+(TRENCH_TOP-ROOF)*_clamp01((z-TZ0)/(TZ1-TZ0))


def trench_wall_bottom(z):
    return trench_roof(z)-TRENCH_WALL


def bunker_ground(z):
    """Ground level round the bunker: the front apron at the floor, rising
    to roof level at the back of the wing blocks and staying there."""
    return -.1+(ROOF+.1)*_clamp01((z-BZ0)/(WING_Z1-BZ0))


def ramp_y(ramp, z, y_a, y_b):
    """Surface height of a straight ramp (x0, x1, z_a, z_b) at z."""
    _, _, za, zb = ramp
    return y_a+(y_b-y_a)*_clamp01((z-za)/(zb-za))


def exterior_ramp_y(z):
    return ramp_y(XR, z, HUT_RING, TOWER_TOP)


def stair_y(z):
    """Vault stair surface: hall floor (0) at the head, vault floor at the foot."""
    return ramp_y(STAIR, z, 0.0, VAULT_FLOOR)


def _lerp(points, u):
    us, ys = zip(*points)
    if u <= us[0]: return ys[0]
    if u >= us[-1]: return ys[-1]
    for (u0, y0), (u1, y1) in zip(points, points[1:]):
        if u0 <= u <= u1: return y0+(y1-y0)*(u-u0)/(u1-u0)


def east_floor(x): return _lerp(T_EAST_FLOOR, x)
def east_lid(x): return _lerp(T_EAST_LID, x)
def south_floor(z): return _lerp(T_SOUTH_FLOOR, z)
def south_lid(z): return _lerp(T_SOUTH_LID, z)


def tunnel_floor(x, z):
    """Walking surface anywhere along the sally port (vault door to exit)."""
    if z >= T_SOUTH[3]-1e-9 and x < T_SOUTH[0]: return east_floor(x)
    if z >= T_SOUTH[3]-1e-9: return east_floor(T_EAST[1])
    return south_floor(z)


def ring_height(region, x, z):
    """Pinned height for a terrain vertex on the boundary of a hole region.
    Always within the span of that region's outer walls."""
    if region == 'bunker': return bunker_ground(z)
    if region == 'trench': return trench_roof(z)
    if region == 'tunnel east': return east_lid(x)
    if region == 'tunnel south': return south_lid(z)
    if region == 'sally exit': return EXIT_GROUND
    return HUT_RING


def wall_span(region, x, z):
    """(bottom, top) of a hole region's outer walls at a boundary point."""
    if region == 'bunker': return (-4.0, ROOF)
    if region == 'trench': return (trench_wall_bottom(z), trench_roof(z))
    if region == 'hut': return (HUT_FLOOR-5, HUT_ROOF)
    if region == 'tunnel east': return (east_floor(x)-1, east_lid(x))
    if region == 'tunnel south': return (south_floor(z)-1, south_lid(z))
    return (EXIT_GROUND-1, EXIT_ROOF)


def sites():
    """Terrain surfaces to blend toward around the structures, in order:
    (kind, shape, surface(x, z) -> local height, falloff metres). Shapes are
    ('rect', x0, x1, z0, z1) or ('disc', x, z, r) in local coordinates."""
    bx, bz = BATTERY; px, pz = PAD
    xa, xb, zg, _ = XR
    return [
        ('bunker', ('rect', -WING_X, WING_X, BZ0, BZ1), lambda x, z: bunker_ground(z), 18),
        ('apron', ('rect', -WING_X-2, WING_X+2, -40, BZ0), lambda x, z: -.1, 6),
        ('hut', ('rect', HX0, HX1, HZ0, HZ1), lambda x, z: HUT_RING, 18),
        ('ramp foot', ('rect', xa-2, xb+2, HZ1, zg+4), lambda x, z: HUT_RING, 10),
        # Last of the three: its banks stay exactly flush with the roof, and its
        # clamped surface matches the bunker and hut levels where they meet.
        ('trench', ('rect', TX0-BANK, TX1+BANK, TZ0, TZ1), lambda x, z: trench_roof(z), 16),
        # The sally port's lids lie flush with the hillside they run under.
        ('tunnel east', ('rect',) + T_EAST, lambda x, z: east_lid(x), 10),
        ('tunnel south', ('rect',) + T_SOUTH, lambda x, z: south_lid(z), 10),
        # Flat benches reach at least one 8 m cell past each structure, so no
        # terrain triangle can rise into a deck, plinth or ramp.
        ('battery', ('disc', bx, bz, BATTERY_R+9), lambda x, z: BATTERY_GROUND, 14),
        ('battery ramp', ('rect', bx-10, bx+10, bz, bz+BATTERY_R+18), lambda x, z: BATTERY_GROUND, 14),
        ('sally apron', ('rect', T_EXIT[0]-4, T_EXIT[1]+8, T_EXIT[2]-6, T_EXIT[3]), lambda x, z: EXIT_GROUND, 8),
        ('pad', ('rect', px-PAD_HALF-8, px+PAD_HALF+8, pz-PAD_HALF-8, pz+PAD_HALF+8), lambda x, z: PAD_TOP-.06, 14),
    ]


def _runs_without(lo, hi, gaps):
    out, u = [], lo
    for a, c in sorted(gaps):
        if a > u: out.append((u, a))
        u = max(u, c)
    if hi > u: out.append((u, hi))
    return out


def _merlon_spans(u0, u1, width=1.0, gap=1.4):
    """Evenly spaced merlons filling [u0, u1], flush with both ends."""
    n = max(1, int((u1-u0+gap)//(width+gap)))
    if n == 1: return [((u0+u1-width)/2, (u0+u1+width)/2)]
    step = (u1-u0-width)/(n-1)
    return [(u0+i*step, u0+i*step+width) for i in range(n)]


def build(mesh, team, circuit):
    if team not in (0, 1) or not circuit: raise ValueError('team and circuit required')
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=INTERIOR, metal=METAL, glow=GLOW)
    wall, slab = b.wall, b.slab

    def crenels(axis, fixed0, fixed1, u0, u1, y):
        """Merlons along one edge: axis 'x' runs along x at z fixed0..fixed1."""
        for a, c in _merlon_spans(u0, u1):
            if axis == 'x': wall(a, c, fixed0, fixed1, y, y+MERLON, HULL)
            else: wall(fixed0, fixed1, a, c, y, y+MERLON, HULL)

    def closed_side(x, segments, surface):
        """Double-faced side panel under a ramp: for each (z0, z1, bottom),
        from the bottom up to 0.6 m under the ramp surface."""
        for z0, z1, y0 in segments:
            u0, u1 = surface(z0)-.6, surface(z1)-.6
            if u0 <= y0 and u1 <= y0: continue
            if u0 < y0 or u1 < y0:        # clip where the underside meets the bottom
                zc = z0+(y0-u0)*(z1-z0)/(u1-u0)
                if u0 < y0: z0, u0 = zc, y0
                else: z1, u1 = zc, y0
            pts = [(x, y0, z0), (x, u0, z0), (x, u1, z1), (x, y0, z1)]
            pts = [q for i, q in enumerate(pts) if q != pts[i-1]]    # a clipped end is a triangle
            if len(pts) == 3: mesh.triangle(pts, HULL); mesh.triangle(pts[::-1], HULL)
            else: mesh.quad(*pts, HULL); mesh.quad(*pts[::-1], HULL)

    # --- Bunker shell ------------------------------------------------------
    ix, iz0, iz1 = BX-WALL, BZ0+WALL, BZ1-WALL      # interior bounds
    hx0, hx1, hz0, hz1 = HALL_HOLE
    for x0, x1, z0, z1 in [(-ix, hx0, iz0, iz1), (hx0, ix, iz0, hz0), (hx0, ix, hz1, iz1)]:
        slab(x0, x1, z0, z1, 0, DECK)
    # Roof: the vestibule under the facade and the back rooms at ROOF; the
    # tall hall between them under its own raised roof.
    hz_a, hz_b = HALL_Z
    slab(-BX, BX, BZ0, hz_a, ROOF, HULL)
    slab(-BX, BX, hz_b, BZ1, ROOF, HULL)
    slab(-BX, BX, hz_a, hz_b, HALL_ROOF, HULL)
    wall(-BX, -ix, BZ0, BZ1, -4, CEIL, HULL)
    # East wall: full height above the vault door; below it, down to the vault
    # floor either side of the door.
    door_top = VAULT_FLOOR+TUN_H
    wall(ix, BX, BZ0, BZ1, door_top, CEIL, HULL)
    for z0, z1 in ((BZ0, TUN_DOOR[0]), (TUN_DOOR[1], BZ1)): wall(ix, BX, z0, z1, VAULT_FLOOR-1, door_top, HULL)
    # The tall hall's side walls, each with a gallery-level doorway onto the
    # hillside, and its back wall over partition A.
    ga, gb = GALLERY_DOOR
    for x0, x1 in ((-BX, -ix), (ix, BX)):
        wall(x0, x1, hz_a, ga, CEIL, HALL_CEIL, HULL)
        wall(x0, x1, gb, hz_b, CEIL, HALL_CEIL, HULL)
        wall(x0, x1, ga, gb, CEIL, GALLERY_Y, HULL)
    wall(-ix, ix, *PART_A, CEIL, HALL_CEIL, HULL)
    for x0, x1 in [(-ix, DOOR[0]), (DOOR[1], ix)]: wall(x0, x1, BZ0, iz0, -4, CEIL, HULL)
    if DOOR_TOP < CEIL: wall(DOOR[0], DOOR[1], BZ0, iz0, DOOR_TOP, CEIL, HULL)
    wall(DOOR[0], DOOR[1], BZ0, iz0, -4, 0, HULL)
    for x0, x1 in [(-ix, BACK_DOOR[0]), (BACK_DOOR[1], ix)]: wall(x0, x1, iz1, BZ1, -4, CEIL, HULL)
    if DOOR_TOP < CEIL: wall(*BACK_DOOR, iz1, BZ1, DOOR_TOP, CEIL, HULL)
    wall(*BACK_DOOR, iz1, BZ1, -4, 0, HULL)

    # The two interior partitions (doorways run to the ceiling).
    def partition(z0, z1, doors):
        u = -ix
        for d0, d1 in doors+[(ix, ix)]:
            if d0 > u: wall(u, d0, z0, z1, 0, CEIL, HULL)
            if d1 > d0 and DOOR_TOP < CEIL: wall(d0, d1, z0, z1, DOOR_TOP, CEIL, HULL)
            u = d1
    partition(*PART_A, PART_A_DOORS)
    partition(*PART_B, [PART_B_DOOR])

    # Gallery: a 4 m walkway round the tall hall's sides and back at hillside
    # level, reached from the gallery doorways or by jetting up from the floor.
    gx = ix-GALLERY_W
    gback = PART_A[0]-GALLERY_W
    for x0, x1, z0, z1 in ((-ix, -gx, hz_a, PART_A[0]), (gx, ix, hz_a, PART_A[0]), (-gx, gx, gback, PART_A[0])):
        slab(x0, x1, z0, z1, GALLERY_Y, DECK, thick=GALLERY_THICK)
    for x0, x1, z0, z1 in ((-gx-.08, -gx+.08, hz_a, gback+.08), (gx-.08, gx+.08, hz_a, gback+.08),
                           (-gx, gx, gback-.08, gback+.08)):       # render-only rail and kick band
        wall(x0, x1, z0, z1, GALLERY_Y+.95, GALLERY_Y+1.05, BRONZE, False)
        wall(x0, x1, z0, z1, GALLERY_Y+.01, GALLERY_Y+.2, METAL, False)
    for s in (-1, 1):
        # Bronze surround on the outside of each gallery doorway, and a lamp.
        b.face_box('x', s*BX, s, ga-.4, ga, GALLERY_Y, HALL_CEIL, .14, METAL)
        b.face_box('x', s*BX, s, gb, gb+.4, GALLERY_Y, HALL_CEIL, .14, METAL)
        b.lamp((s*(ix-1.5), GALLERY_Y+4.5, (ga+gb)/2), .6)
    b.ceiling_strip('x', (hz_a+gback)/2, -gx+1, gx-1, HALL_CEIL, .9)
    b.ceiling_strip('x', hz_a+3.0, -gx+1, gx-1, HALL_CEIL, .9)

    # Interior dressing: liners, baseboards, cornices, team stripe.
    b.dress('z', iz0, 1, _runs_without(-ix, ix, [DOOR]), 0, CEIL)
    for s in (-1, 1):
        b.dress('x', s*ix, -s, [(iz0, PART_A[0]), (PART_A[1], PART_B[0]), (PART_B[1], iz1)], 0, CEIL)
    b.dress('z', PART_A[0], -1, _runs_without(-ix, ix, PART_A_DOORS), 0, CEIL)
    b.dress('z', PART_A[1], 1, _runs_without(-ix, ix, PART_A_DOORS), 0, CEIL, stripe=False)
    b.dress('z', PART_B[0], -1, _runs_without(-ix, ix, [PART_B_DOOR]), 0, CEIL, stripe=False)
    b.dress('z', PART_B[1], 1, _runs_without(-ix, ix, [PART_B_DOOR]), 0, CEIL)
    b.dress('z', iz1, -1, _runs_without(-ix, ix, [BACK_DOOR]), 0, CEIL)
    # Bronze door frames on every interior opening.
    for z, doors in [(PART_A[0], PART_A_DOORS), (PART_B[0], [PART_B_DOOR]),
                     (iz1, [BACK_DOOR]), (iz0, [DOOR])]:
        for d0, d1 in doors:
            for into in (1, -1):      # jambs stand 2 cm off the reveal planes
                b.face_box('z', z, into, d0-.3, d0-.02, 0, DOOR_TOP-.02, .12, METAL)
                b.face_box('z', z, into, d1+.02, d1+.3, 0, DOOR_TOP-.02, .12, METAL)
                if DOOR_TOP < CEIL: b.face_box('z', z, into, d0-.3, d1+.3, DOOR_TOP-.3, DOOR_TOP, .12, METAL)

    # Ceiling lights: entry vestibule, the tall hall (above), inventory room,
    # back room.
    b.ceiling_strip('x', iz0+2.0, -ix+1, ix-1, CEIL, .5)
    b.ceiling_strip('x', (PART_A[1]+PART_B[0])/2, -ix+2, ix-2, CEIL, .7)
    b.ceiling_strip('x', (PART_B[1]+iz1)/2, -ix+2, ix-2, CEIL, .7)

    # Hall cover: squat stone blocks with bronze caps.
    for x, z in [(-5, -9), (5, -9), (0, -3)]:
        wall(x-.8, x+.8, z-.8, z+.8, 0, 2.6, HULL)
        wall(x-.9, x+.9, z-.9, z+.9, 2.6, 2.74, METAL, False)
        wall(x-.86, x+.86, z-.86, z+.86, 0, .3, METAL, False)
        wall(x-.84, x+.84, z-.84, z+.84, 1.6, 1.78, accent, False)

    # Inventory room; the back room (the old generator room) is now a muster
    # room with two spawns. The generator is down in the vault.
    for x in (-4, 4): mesh.equipment('inventory', (x, 0, 9), team, circuit)

    # --- Vault: the generator room under the hall --------------------------
    # Two ways in: the stair from the hall and the tunnel door. Nothing else.
    vx0, vx1, vz0, vz1 = VAULT
    F, C = VAULT_FLOOR, VAULT_CEIL
    P = PIT_FLOOR
    px0, px1, pz0, _ = PIT
    rz_top = PIT_RAMP[2]
    # Floors stop at the walls' inner faces, and the walls reach deeper than
    # the floor slabs, so no two bottom faces share a plane.
    slab(vx0, ix, vz0, rz_top, F, DECK)                            # vault floor south of the well
    slab(px1+WALL, ix, STAIR[3], vz1, F, DECK)                     # walkway along the east side, from the stair foot
    wall(px1, px1+WALL, rz_top, vz1, P-1.5, F, HULL)               # the well's east wall, flush with the floor
    slab(vx0, px1, pz0, vz1, P, DECK)                              # well floor
    mesh.ramp((px0+px1)/2, px1-px0, rz_top, pz0, F, P, BRONZE)     # ramp down into the well
    wall(vx0-WALL, vx0, vz0-WALL, vz1+WALL, P-1.5, C, HULL)
    wall(vx0, vx1, vz0-WALL, vz0, F-1.5, C, HULL)
    wall(vx0, vx1, vz1, vz1+WALL, P-1.5, C, HULL)
    # Stair from the hall down the east side, closed beneath its open edge,
    # railed round its opening in the hall floor.
    s0, s1, sz_head, sz_foot = STAIR
    mesh.ramp((s0+s1)/2, s1-s0, sz_head, sz_foot, 0.0, F, BRONZE)
    # Under the stair's open edge: a panel up to the hall slab near the head,
    # then up to 0.6 m under the stair surface.
    z_a = sz_head+(-1.0+.6)/((F-0.0)/(sz_foot-sz_head))
    for pts in ([(s0, F, sz_head), (s0, -1.0, sz_head), (s0, -1.0, z_a), (s0, F, z_a)],):
        mesh.quad(*pts, HULL); mesh.quad(*pts[::-1], HULL)
    closed_side(s0, [(z_a, sz_foot, F)], stair_y)
    wall(hx0-.4, hx0, hz0, hz1+.4, 0, 1.1, METAL)
    wall(hx0, hx1, hz1, hz1+.4, 0, 1.1, METAL)
    wall(hx0-.46, hx0+.06, hz0, hz1+.46, 1.1, 1.18, BRONZE, False)
    wall(hx0-.06, hx1, hz1-.06, hz1+.46, 1.1, 1.18, BRONZE, False)
    b.face_box('z', hz0, -1, hx0, hx1, -.02, .02, .3, BRONZE)       # bronze lip at the stair head
    generator = VAULT_GEN
    mesh.equipment('generator', generator, team, circuit)
    gx, _, gz = generator
    b.prism(gx, gz, 3.4, 3.4, P+.005, P+.02, 16, BRONZE, solid=False)
    b.prism(gx, gz, 3.0, 3.0, P+.02, P+.03, 16, accent, solid=False)
    b.dress('x', vx0, 1, [(pz0, vz1)], P, F, pilasters=False)
    b.dress('z', vz1, -1, [(vx0, px1)], P, F, pilasters=False)
    b.dress('x', px1, -1, [(pz0, vz1)], P, F, pilasters=False)
    b.face_box('x', px1, -1, pz0+.03, vz1-.03, F-.08, F+.02, .3, BRONZE)   # bronze lip on the well's edge
    b.lamp((gx, F-.5, gz), .7)
    # Squat stone piers with bronze collars, and bronze ribs across the vault.
    for x, z in VAULT_PIERS:
        b.prism(x, z, 1.0, 1.0, F, C, 8, HULL, phase=math.pi/8)
        b.prism(x, z, 1.2, 1.2, F, F+.4, 8, METAL, phase=math.pi/8, solid=False)
        b.prism(x, z, 1.3, 1.05, C-.6, C, 8, BRONZE, phase=math.pi/8, solid=False, cap=False)
    for z in (-7.0, -1.5, 4.0, 9.5):
        wall(vx0, s0, z-.25, z+.25, C-.35, C, BRONZE, False)
    b.dress('x', vx0, 1, [(vz0, vz1)], F, C)
    b.dress('z', vz0, 1, [(vx0, s0)], F, C)
    b.dress('z', vz1, -1, [(vx0, vx1)], F, C)
    b.dress('x', vx1, -1, [(sz_foot, TUN_DOOR[0]), (TUN_DOOR[1], vz1)], F, C)
    for x in (5.0, 9.4): b.ceiling_strip('z', x, vz0+1.5, vz1-1.5, C, .7)
    for into, z in ((-1, TUN_DOOR[0]), (-1, TUN_DOOR[1])):
        b.face_box('x', vx1, into, z-.3 if z == TUN_DOOR[0] else z, z if z == TUN_DOOR[0] else z+.3,
                   F, door_top-.02, .12, METAL)

    # --- Sally port: vault -> east under the hillside -> south to the battery
    def blk(x0, x1, z0, z1, yb, yt, mat, solid=True):
        """Box whose bottom and top heights are linear functions of x or z."""
        c = [(x0, yb(x0, z0), z0), (x1, yb(x1, z0), z0), (x1, yt(x1, z0), z0), (x0, yt(x0, z0), z0),
             (x0, yb(x0, z1), z1), (x1, yb(x1, z1), z1), (x1, yt(x1, z1), z1), (x0, yt(x0, z1), z1)]
        for face in [(0, 3, 2, 1), (4, 5, 6, 7), (0, 4, 7, 3), (1, 2, 6, 5), (0, 1, 5, 4), (3, 7, 6, 2)]:
            mesh.quad(*(c[i] for i in face), mat, solid)

    ex0, ex1, ez0, ez1 = T_EAST
    qx0, qx1, qz0, qz1 = T_SOUTH
    w = WALL
    ef = lambda x, z: east_floor(x); el = lambda x, z: east_lid(x)
    sf = lambda x, z: south_floor(z); sl = lambda x, z: south_lid(z)
    def breaks(points, lo, hi):
        return sorted({lo, hi} | {u for u, _ in points if lo < u < hi})
    xs = sorted(set(breaks(T_EAST_FLOOR+T_EAST_LID, ex0, ex1)) | {u for s in SKYLIGHTS for u in s})
    def in_skylight(a, c_): return any(s0 <= a and c_ <= s1 for s0, s1 in SKYLIGHTS)
    for a, c_ in zip(xs, xs[1:]):
        inner = min(c_, qx1-w)
        well = in_skylight(a, c_)
        # Under a skylight the side walls rise to the lid top, so the shaft's
        # rim is flush with the hillside; elsewhere the lid caps them.
        wall_top = el if well else (lambda x, z: el(x, z)-.5)
        if inner > a:   # floor and ceiling inside the walls
            blk(a, inner, ez0+w, ez1-w, lambda x, z: ef(x, z)-1, ef, DECK)
            if not well:
                blk(a, inner, ez0+w, ez1-w, lambda x, z: ef(x, z)+TUN_H, lambda x, z: ef(x, z)+TUN_H+.5, HULL)
        blk(a, c_, ez1-w, ez1, lambda x, z: ef(x, z)-1, wall_top, HULL)                         # north wall
        if a < qx0+w: blk(a, min(c_, qx0+w), ez0, ez0+w, lambda x, z: ef(x, z)-1, wall_top, HULL)
        if not well: blk(a, c_, ez0, ez1, lambda x, z: el(x, z)-.5, el, HULL)                   # lid
    blk(ex1-w, ex1, ez0, ez1-w, lambda x, z: ef(x, z)-1, lambda x, z: el(x, z)-.5, HULL)         # corner's east wall
    # Skylight wells: end walls close the void between the tunnel ceiling and
    # the lid; a render-only bronze grate rim and bars mark the drop-in.
    for s0, s1 in SKYLIGHTS:
        for x0_, x1_ in ((s0-.5, s0), (s1, s1+.5)):
            blk(x0_, x1_, ez0+w, ez1-w, lambda x, z: ef(x, z)+TUN_H, el, HULL)
        top = min(east_lid(s0), east_lid(s1))
        for z in (ez0+w+1.6, (ez0+ez1)/2, ez1-w-1.6):
            wall(s0, s1, z-.06, z+.06, top+.02, top+.1, BRONZE, False)
        b.lamp(((s0+s1)/2, east_floor(s0)+TUN_H-.5, (ez0+ez1)/2), .9)
    zs = breaks(T_SOUTH_FLOOR+T_SOUTH_LID, qz0, qz1)
    for a, c_ in zip(zs, zs[1:]):
        top = ez0+w if c_ >= qz1 else c_     # the last piece meets the east leg's floor
        blk(qx0+w, qx1-w, a, top, lambda x, z: sf(x, z)-1, sf, DECK)
        blk(qx0+w, qx1-w, a, top, lambda x, z: sf(x, z)+TUN_H, lambda x, z: sf(x, z)+TUN_H+.5, HULL)
        for x0_, x1_ in ((qx0, qx0+w), (qx1-w, qx1)):
            blk(x0_, x1_, a, c_ if x0_ == qx0 else min(c_, ez0), lambda x, z: sf(x, z)-1, lambda x, z: sl(x, z)-.5, HULL)
        blk(qx0, qx1, a, c_, lambda x, z: sl(x, z)-.5, sl, HULL)                                 # lid
    # Exit house at the battery's ramp foot: roofed, door in its east wall,
    # open straight in; the battery faces the field and cannot fire back
    # into it.
    g, roof = EXIT_GROUND, EXIT_ROOF
    ox0, ox1, oz0, oz1 = T_EXIT
    slab(ox0+w, ox1-w, oz0+w, oz1, g, DECK)
    slab(ox0, ox1, oz0, oz1, roof, HULL)
    wall(ox0, ox1, oz0, oz0+w, g-1, roof-1, HULL)
    wall(ox0, ox0+w, oz0+w, oz1, g-1, roof-1, HULL)
    for z0, z1 in _runs_without(oz0+w, oz1, [EXIT_DOOR]): wall(ox1-w, ox1, z0, z1, g-1, roof-1, HULL)
    # Dressing: liners, lamp strips and sconces the whole way.
    for x in range(20, 88, 8):
        y = east_floor(x)
        for zc, into in ((ez0+w, 1), (ez1-w, -1)):
            if x > qx0 and zc == ez0+w: continue
            wall(x-.3, x+.3, zc+into*.01, zc+into*.26, y+3.1, y+3.6, GLOW, False)
            b.lamp((x, y+3.3, zc+into*.6), .45)
    for z in range(0, -40, -8):
        y = south_floor(z)
        for xc, into in ((qx0+w, 1), (qx1-w, -1)):
            wall(xc+into*.01, xc+into*.26, z-.3, z+.3, y+3.1, y+3.6, GLOW, False)
            b.lamp((xc+into*.6, y+3.3, z), .45)
    for z in (-42.0, -38.0):
        b.ceiling_strip('x', z, ox0+1.5, ox1-1.5, g+TUN_H, .6)
    for z in EXIT_DOOR:
        b.face_box('x', ox1, 1, z-.3 if z == EXIT_DOOR[0] else z, z if z == EXIT_DOOR[0] else z+.3, g, g+TUN_H-.02, .12, METAL)
    b.face_box('x', ox1, 1, oz0, oz1, roof-.5, roof-.1, .16, METAL)
    b.lamp((ox1+2.0, g+3.8, sum(EXIT_DOOR)/2), .5)

    # --- Facade ------------------------------------------------------------
    # Gatehouse: jambs and lintel projecting round a recessed portal, and the
    # raised mass behind them over the vestibule.
    top_g = TIERS[0][2]
    p0, p1 = PORTAL
    wall(-GATE_X, p0, GATE_Z, BZ0, -.5, top_g, HULL)
    wall(p1, GATE_X, GATE_Z, BZ0, -.5, top_g, HULL)
    wall(p0, p1, GATE_Z, BZ0, PORTAL_TOP, top_g, HULL)
    wall(-GATE_X, GATE_X, BZ0, FRONT_Z1, ROOF, top_g, HULL)
    # Stepped tiers either side, flush with the front wall.
    for u0, u1, top in TIERS[1:]:
        for s in (-1, 1): wall(s*u0, s*u1, BZ0, FRONT_Z1, ROOF, top, HULL)
    # Behind the low outer tiers the tall hall's front wall rises to its roof.
    for s in (-1, 1):
        u0, u1, top = TIERS[2]
        if top < HALL_ROOF: wall(s*u0, s*u1, FRONT_Z1-.8, FRONT_Z1, top, HALL_ROOF, HULL)
    # Wing blocks retain the hillside either side of the facade.
    for s in (-1, 1): wall(s*BX, s*WING_X, BZ0, WING_Z1, -.5, ROOF, HULL)
    # Buttresses mark each step and the corners.
    for s in (-1, 1):
        wall(s*TIERS[2][0], s*(TIERS[2][0]+1.4), BZ0-1.4, BZ0, -.5, 13.0, HULL)
        wall(s*(BX-1.4), s*BX, BZ0-1.4, BZ0, -.5, 11.0, HULL)
        for x0, x1, y in [(TIERS[2][0], TIERS[2][0]+1.4, 13.0), (BX-1.4, BX, 11.0)]:
            wall(s*(x0-.06), s*(x1+.06), BZ0-1.46, BZ0-1.34, y-.5, y-.1, METAL, False)
    # Crenellations: gatehouse sides, back and front corners; tier fronts.
    crenels('z', -GATE_X, -GATE_X+.8, GATE_Z+.8, FRONT_Z1-.8, top_g)
    crenels('z', GATE_X-.8, GATE_X, GATE_Z+.8, FRONT_Z1-.8, top_g)
    crenels('x', FRONT_Z1-.8, FRONT_Z1, -GATE_X+.8, GATE_X-.8, top_g)
    for s in (-1, 1): wall(s*(GATE_X-1.0), s*GATE_X, GATE_Z, GATE_Z+.8, top_g, top_g+MERLON, HULL)
    for u0, u1, top in TIERS[1:]:
        lo = u0+(1.4 if u0 == TIERS[2][0] else .4)
        for s in (-1, 1):
            spans = _merlon_spans(lo, u1-.2)
            for a, c in spans: wall(s*a, s*c, BZ0, BZ0+.8, top, top+MERLON, HULL)
    # Cornices, base course and wing coping (render-only bronze bands).
    b.face_box('z', GATE_Z, -1, -GATE_X, GATE_X, top_g-1.1, top_g-.6, .2, METAL)
    b.face_box('z', GATE_Z, -1, -GATE_X, p0, -.1, .5, .15, METAL)
    b.face_box('z', GATE_Z, -1, p1, GATE_X, -.1, .5, .15, METAL)
    for u0, u1, top in TIERS[1:]:
        if u0 == TIERS[2][0]: u0, u1 = u0+1.4, u1-1.4     # runs between the outer tier's buttresses
        for s in (-1, 1):
            a, c = sorted((s*u0, s*u1))
            b.face_box('z', BZ0, -1, a, c, top-1.1, top-.6, .2, METAL)
    for s in (-1, 1):
        a, c = sorted((s*BX, s*WING_X))
        b.face_box('z', BZ0, -1, a, c, ROOF-.45, ROOF-.1, .15, METAL)
    # Portal: bronze-lined jambs, a lit lintel and the old door surround.
    for x0, x1 in [(p0, p0+.22), (p1-.22, p1)]:
        wall(x0, x1, GATE_Z, BZ0, 0, PORTAL_TOP, METAL, False)
    wall(p0, p1, GATE_Z, GATE_Z+.25, PORTAL_TOP-.25, PORTAL_TOP, METAL, False)
    wall(p0+.3, p1-.3, GATE_Z+.3, BZ0-.1, PORTAL_TOP-.08, PORTAL_TOP, GLOW, False)
    b.lamp((0, PORTAL_TOP-.7, (GATE_Z+BZ0)/2), .8)
    fz = BZ0
    wall(DOOR[0]-.5, DOOR[0], fz-.25, fz, 0, DOOR_TOP+.5, METAL, False)
    wall(DOOR[1], DOOR[1]+.5, fz-.25, fz, 0, DOOR_TOP+.5, METAL, False)
    wall(DOOR[0]-.5, DOOR[1]+.5, fz-.25, fz, DOOR_TOP+.02, DOOR_TOP+.5, METAL, False)
    # Banner wall: a tall team banner with the cairn emblem over the portal,
    # and banners on the middle tiers.
    gz = GATE_Z-.05
    mesh.quad((-2.6, 8.2, gz), (-2.6, 15.6, gz), (2.6, 15.6, gz), (2.6, 8.2, gz), accent, False)
    y = 11.2
    for w, h in [(2.6, .8), (2.0, .68), (1.4, .58)]:    # a cairn of three stacked stones
        mesh.quad((-w/2, y, gz-.02), (-w/2, y+h, gz-.02), (w/2, y+h, gz-.02), (w/2, y, gz-.02), GLOW, False)
        y += h+.18
    b.lamp((0, 11, GATE_Z-2.5), .8)
    for s in (-1, 1):
        a, c = sorted((s*7.2, s*9.8))
        mesh.quad((a, .1, fz-.05), (a, 12.2, fz-.05), (c, 12.2, fz-.05), (c, .1, fz-.05), accent, False)   # floor-length
        b.lamp((s*8.5, 7, fz-2.5), .5)
    for x in (p0-1.15, p1+1.15):        # sconces on the jamb faces
        wall(x-.2, x+.2, GATE_Z-.4, GATE_Z, 3.2, 3.5, METAL, False)
        wall(x-.14, x+.14, GATE_Z-.36, GATE_Z-.04, 3.5, 4.0, GLOW, False)
        b.lamp((x, 3.8, GATE_Z-.7), .6)

    # Roof turret on the gatehouse, in a stone and bronze collar.
    mesh.equipment('turret', ROOF_TURRET, team, circuit)
    def collar(x, y, z, r):
        b.prism(x, z, r+.15, r, y, y+1.25, 8, HULL, solid=False, cap=False)
        b.prism(x, z, r+.08, r+.08, y+.85, y+1.05, 8, METAL, solid=False, cap=False)
    collar(ROOF_TURRET[0], ROOF_TURRET[1], ROOF_TURRET[2], 2.2)

    # --- Covered trench, sunk into the hill -------------------------------
    mid = (TX0+TX1)/2
    mesh.ramp(mid, TX1-TX0-2, TZ0, TZ1, 0, TRENCH_RISE, BRONZE)
    mesh.ramp(mid, TX1-TX0-.1, TZ0, TZ1, trench_roof(TZ0), trench_roof(TZ1), HULL)
    for x0, x1 in [(TX0, TX0+1), (TX1-1, TX1)]:
        b.sloped_wall(x0, x1, TZ0, TZ1, trench_wall_bottom(TZ0), trench_wall_bottom(TZ1), TRENCH_WALL, HULL)
    # Liner under the roof, wall liners and sconces every 8 m up the slope.
    ceil0, ceil1 = trench_roof(TZ0)-.63, trench_roof(TZ1)-.63
    mesh.quad((TX0+1, ceil0, TZ0), (TX1-1, ceil0, TZ0), (TX1-1, ceil1, TZ1), (TX0+1, ceil1, TZ1), INTERIOR, False)
    for x, into in [(TX0+1, 1), (TX1-1, -1)]:
        p = x+into*.03
        quad = [(p, 0, TZ0), (p, TRENCH_RISE, TZ1), (p, ceil1, TZ1), (p, ceil0, TZ0)]
        mesh.quad(*(quad if into < 0 else quad[::-1]), INTERIOR, False)
        p = x+into*.05                                  # the stripe sits proud of the liner
        stripe = [(p, .75, TZ0), (p, TRENCH_RISE+.75, TZ1), (p, TRENCH_RISE+1, TZ1), (p, 1, TZ0)]
        mesh.quad(*(stripe if into < 0 else stripe[::-1]), accent, False)
    for z in range(TZ0+4, TZ1, 8):
        y = trench_floor(z)
        for x, into in [(TX0+1, 1), (TX1-1, -1)]:
            wall(x+into*.01, x+into*.26, z-.3, z+.3, y+3.1, y+3.6, GLOW, False)
            b.lamp((x+into*.6, y+3.3, z), .45)
    # Roof: stone kerbs along both edges and glowing light slots.
    def roof_quad(x0, x1, z0, z1, lift, mat):
        y0, y1 = trench_roof(z0)+lift, trench_roof(z1)+lift
        mesh.quad((x0, y0, z0), (x0, y1, z1), (x1, y1, z1), (x1, y0, z0), mat, False)
    for x0, x1 in [(TX0, TX0+.6), (TX1-.6, TX1)]:
        roof_quad(x0, x1, TZ0, TZ1, .015, METAL)
    for z in LIGHT_SLOTS:
        roof_quad(mid-.45, mid+.45, z-1.5, z+1.5, .015, METAL)
        roof_quad(mid-.25, mid+.25, z-1.3, z+1.3, .025, GLOW)

    # --- Guard hut ---------------------------------------------------------
    hx0, hx1, hz0, hz1 = HX0+WALL, HX1-WALL, HZ0+WALL, HZ1-WALL
    slab(hx0, hx1, hz0, hz1, HUT_FLOOR, DECK)
    for x0, x1 in [(HX0, hx0), (hx1, HX1)]: wall(x0, x1, HZ0, HZ1, HUT_FLOOR-5, HUT_CEIL, HULL)
    wall(hx0, hx1, hz1, HZ1, HUT_FLOOR-5, HUT_CEIL, HULL)
    for x0, x1 in [(hx0, HUT_DOOR[0]), (HUT_DOOR[1], hx1)]: wall(x0, x1, HZ0, hz0, HUT_FLOOR-5, HUT_CEIL, HULL)
    wall(*HUT_DOOR, HZ0, hz0, HUT_FLOOR+HUT_DOOR_TOP, HUT_CEIL, HULL)
    wall(*HUT_DOOR, HZ0, hz0, HUT_FLOOR-5, HUT_FLOOR, HULL)
    h0, h1, hz_a, hz_b = HATCH
    for x0, x1, z0, z1 in [(HX0, h0, HZ0, HZ1), (h0, HX1, HZ0, hz_a), (h0, HX1, hz_b, HZ1), (h1, HX1, hz_a, hz_b)]:
        slab(x0, x1, z0, z1, HUT_ROOF, HULL)
    # Hut ramp: a solid wedge from the back of the hut up through the hatch.
    r0, r1, zl, zh = RAMP
    rise = HUT_ROOF-HUT_FLOOR
    mesh.ramp((r0+r1)/2, r1-r0, zl, zh, HUT_FLOOR, HUT_ROOF, BRONZE)
    under = rise-.6
    for x in (r0, r1):   # close the wedge under the ramp on both sides
        tri = [(x, HUT_FLOOR, zl), (x, HUT_FLOOR+under, zh), (x, HUT_FLOOR, zh)]
        mesh.triangle(tri, HULL); mesh.triangle(tri[::-1], HULL)
    wall(r0, r1, zh, zh+.4, HUT_FLOOR, HUT_FLOOR+under, HULL)
    b.dress('x', hx0, 1, [(hz0, hz1)], HUT_FLOOR, HUT_CEIL)
    b.dress('z', hz1, -1, [(hx0, r0)], HUT_FLOOR, HUT_CEIL)
    b.dress('z', hz0, 1, _runs_without(hx0, hx1, [HUT_DOOR]), HUT_FLOOR, HUT_CEIL)
    b.ceiling_strip('z', 0, hz0+2, hz1-2, HUT_CEIL, .6)
    for x0, x1, z0, z1 in [(h0-.3, h0, hz_a, hz_b), (h0, h1, hz_b, hz_b+.3)]:
        wall(x0, x1, z0, z1, HUT_ROOF, HUT_ROOF+.03, BRONZE, False)
    # Terrace parapet round the hut roof outside the tower, with gaps.
    tx0, tx1, tz0, tz1 = TW
    xa, xb, zg, zt = XR
    t = .4
    for a, c in _runs_without(HX0, tx0, [(-6, -2)]):
        wall(a, c, HZ0, HZ0+t, HUT_ROOF, HUT_ROOF+MERLON, HULL)
    for a, c in _runs_without(HZ0+t, HZ1-t, [(86, 90)]):
        wall(HX0, HX0+t, a, c, HUT_ROOF, HUT_ROOF+MERLON, HULL)
    for a, c in _runs_without(HX0, HX1, [(xa, xb)]):
        wall(a, c, HZ1-t, HZ1, HUT_ROOF, HUT_ROOF+MERLON, HULL)
    wall(HX1-t, HX1, tz1, HZ1-t, HUT_ROOF, HUT_ROOF+MERLON, HULL)

    # --- Flag tower --------------------------------------------------------
    wx0, wx1, wz0, wz1 = tx0+WALL, tx1-WALL, tz0+WALL, tz1-WALL
    below = TOWER_TOP-1
    wall(tx0, wx0, tz0, tz1, HUT_ROOF, below, HULL)
    wall(wx1, tx1, tz0, tz1, HUT_ROOF, below, HULL)
    wall(wx0, wx1, tz0, wz0, HUT_ROOF, below, HULL)
    wall(wx0, wx1, wz1, tz1, HUT_ROOF, below, HULL)
    g0, g1, gz_a, gz_b = TOP_HATCH
    for x0, x1, z0, z1 in [(tx0, g0, tz0, tz1), (g0, tx1, tz0, gz_a), (g0, tx1, gz_b, tz1), (g1, tx1, gz_a, gz_b)]:
        slab(x0, x1, z0, z1, TOWER_TOP, HULL)
    # R1: floor to landing, a closed wedge against the west wall.
    a0, a1, za, zb = R1
    r1_y = lambda z: ramp_y(R1, z, HUT_ROOF, TOWER_MID)
    mesh.ramp((a0+a1)/2, a1-a0, za, zb, HUT_ROOF, TOWER_MID, BRONZE)
    closed_side(a1, [(za, zb, HUT_ROOF)], r1_y)
    wall(a0, a1, zb, zb+.4, HUT_ROOF, TOWER_MID-1, HULL)   # up to the landing slab's underside
    slab(wx0, wx1, *LANDING_Z, TOWER_MID, DECK)
    # R2: landing to the platform, climbing back over the hut hatch.
    c0, c1, zc, zd = R2
    mesh.ramp((c0+c1)/2, c1-c0, zc, zd, TOWER_MID, TOWER_TOP, BRONZE)
    for x0, x1, z0, z1 in [(g0-.3, g0, gz_a, gz_b), (g0, g1, gz_b, gz_b+.3)]:
        wall(x0, x1, z0, z1, TOWER_TOP, TOWER_TOP+.03, BRONZE, False)
    # Interior dressing and light.
    b.dress('x', wx0, 1, [(wz0, R1[2]), (LANDING_Z[0], wz1)], HUT_ROOF, below)
    b.dress('x', wx1, -1, [(wz0, wz1)], HUT_ROOF, below)
    b.dress('z', wz0, 1, [(wx0, wx1)], HUT_ROOF, below)
    b.dress('z', wz1, -1, [(wx0, wx1)], HUT_ROOF, below)
    b.ceiling_strip('z', 6, wz0+2, wz1-2, below, .7)
    b.ceiling_strip('x', (LANDING_Z[0]+LANDING_Z[1])/2, wx0+1, wx1-1, TOWER_MID-1, .5)
    # Exterior: buttresses, bronze ring and cornice, slits, banner.
    for x0, x1, z0, z1, y0 in [(tx1, tx1+1.2, 78.5, 79.9, HUT_RING-.5), (tx1, tx1+1.2, 92.1, 93.5, HUT_RING-.5),
                               (2.6, 4.0, tz0-1.2, tz0, HUT_RING-.5), (14.1, 15.5, tz0-1.2, tz0, HUT_RING-.5),
                               (4.3, 5.7, tz1, tz1+1.2, HUT_ROOF), (12.3, 13.7, tz1, tz1+1.2, HUT_ROOF)]:
        wall(x0, x1, z0, z1, y0, 37.5, HULL)
        wall(x0-.06, x1+.06, z0-.06, z1+.06, 36.9, 37.3, METAL, False)   # band below the cap, not flush with it
    for axis, plane, into, u0, u1 in [('x', tx0, -1, tz0, tz1), ('x', tx1, 1, tz0, tz1),
                                       ('z', tz0, -1, tx0, tx1), ('z', tz1, 1, tx0, tx1)]:
        b.face_box(axis, plane, into, u0, u1, 35.8, 36.2, .14, METAL)
        b.face_box(axis, plane, into, u0, u1, below-.5, below, .2, METAL)
    for x in (5.0, 13.0):
        b.face_box('z', tz0, -1, x-.2, x+.2, 31.5, 34.5, .05, GLOW)
    for z in (82, 90):
        b.face_box('x', tx1, 1, z-.2, z+.2, 31.5, 34.5, .05, GLOW)
    bz = tz0-.05
    mesh.quad((6.4, 29.8, bz), (6.4, 39.2, bz), (11.6, 39.2, bz), (11.6, 29.8, bz), accent, False)
    y = 35.6
    for w, h in [(2.4, .7), (1.8, .6), (1.3, .5)]:
        mesh.quad((9-w/2, y, bz-.02), (9-w/2, y+h, bz-.02), (9+w/2, y+h, bz-.02), (9+w/2, y, bz-.02), GLOW, False)
        y += h+.16
    b.lamp((9, 34, tz0-3), .7)

    # Platform (v4): fully open. No parapet or merlons, so the flag can be
    # taken from any edge at speed; only a render-only bronze edge band.
    top = TOWER_TOP
    pt = .5
    for axis, plane, into, u0, u1 in [('x', tx0, -1, tz0, tz1), ('x', tx1, 1, tz0, tz1),
                                       ('z', tz0, -1, tx0, tx1), ('z', tz1, 1, tx0, tx1)]:
        b.face_box(axis, plane, into, u0, u1, top-.35, top-.05, .1, BRONZE)
    # Flag plinth: exposed on top of the tower.
    fx, fz_ = FLAG
    b.prism(fx, fz_, 1.8, 1.5, top, top+PLINTH, 12, METAL)
    b.prism(fx, fz_, 1.2, 1.2, top+PLINTH, top+PLINTH+.02, 12, accent, solid=False)
    b.prism(fx, fz_, .4, .4, top+PLINTH+.02, top+PLINTH+.04, 12, GLOW, solid=False)
    b.prism(fx, fz_, 2.3, 2.3, top+.005, top+.02, 16, BRONZE, solid=False)
    b.lamp((fx, top+2.5, fz_), .5)
    # Sentry mast over the flag, and the tower's sensor.
    mx, mz = MAST
    b.prism(mx, mz, .8, .6, top, top+10, 8, METAL)
    wall(mx-2.2, mx+2.2, mz-2.2, mz+2.2, top+10, top+10.5, HULL)
    sentry = (mx, top+10.5, mz)
    mesh.equipment('turret', sentry, team, circuit)
    collar(mx, top+10.5, mz, 2.2)
    sx, sz = SENSOR
    mesh.equipment('sensor', (sx, top, sz), team, circuit)
    b.prism(sx, sz, 1.95, 1.8, top, top+1.1, 8, HULL, solid=False, cap=False)
    b.prism(sx, sz, 1.9, 1.9, top+.7, top+.9, 8, METAL, solid=False, cap=False)

    # Exterior attack ramp from the knoll to a bridge onto the platform.
    xm = (xa+xb)/2
    mesh.ramp(xm, xb-xa, zg, zt, HUT_RING, TOWER_TOP, BRONZE)
    for x in (xa, xb):
        closed_side(x, [(zt, HZ1, HUT_ROOF), (HZ1, zg, HUT_RING-.3)], exterior_ramp_y)
    wall(xa, xb, zt, zt+.4, HUT_ROOF, TOWER_TOP-.6, HULL)
    bx0, bx1, bz0, bz1 = BRIDGE
    slab(bx0, bx1, bz0, bz1, TOWER_TOP, DECK)
    wall(bx0, bx0+pt, bz0, bz1, top, top+PARAPET, HULL)
    wall(bx0+pt, bx1, bz0, bz0+pt, top, top+PARAPET, HULL)
    wall(xb, bx1, bz1-pt, bz1, top, top+PARAPET, HULL)
    for z in (zg+1.5, (zg+zt)/2, zt+3):   # bronze lamp posts: at the ramp foot, then along its outer edge
        y = HUT_RING if z > zg else exterior_ramp_y(z)
        b.prism(xa+.25, z, .1, .1, y, y+1.3, 6, METAL, solid=False)
        b.prism(xa+.25, z, .18, .18, y+1.3, y+1.5, 8, GLOW, solid=False)
        b.lamp((xa+.6, y+1.5, z), .4)

    # --- Plasma battery, right flank --------------------------------------
    cx, cz = BATTERY
    g, deck = BATTERY_GROUND, BATTERY_GROUND+BATTERY_H
    b.prism(cx, cz, BATTERY_R+1, BATTERY_R, g-2, deck, 8, HULL, phase=math.pi/8)
    b.prism(cx, cz, BATTERY_R+1.06, BATTERY_R+.9, g+1.2, g+1.6, 8, METAL, phase=math.pi/8, solid=False, cap=False)
    b.prism(cx, cz, BATTERY_R+.16, BATTERY_R+.1, deck-.55, deck-.2, 8, METAL, phase=math.pi/8, solid=False, cap=False)
    apothem = BATTERY_R*math.cos(math.pi/8)
    mesh.ramp(cx, 4, cz+apothem+10, cz+apothem, g, deck, BRONZE)
    verts = [(cx+(BATTERY_R-.3)*math.cos(math.pi/8+i*math.tau/8), cz+(BATTERY_R-.3)*math.sin(math.pi/8+i*math.tau/8))
             for i in range(8)]
    for i in range(8):
        a, c = verts[i], verts[(i+1) % 8]
        if (a[1]+c[1])/2-cz > apothem*.9: continue      # the ramp edge stays open
        b.beam(a, c, .5, deck, deck+1, HULL)
    battery = (cx, deck, cz)
    mesh.equipment('turret', battery, team, circuit, 'plasma')
    collar(cx, deck, cz, 2.2)
    b.lamp((cx, deck+2, cz+3), .4)

    # --- Landing and deploy ledge, left flank -----------------------------
    px, pz = PAD
    H, t = PAD_HALF, .5
    wall(px-H, px+H, pz-H, pz+H, PAD_TOP-1.2, PAD_TOP, DECK)
    for s in (-1, 1):
        for a, c in ((-H, -PAD_GAP), (PAD_GAP, H)):
            wall(px+a, px+c, pz+s*(H-t), pz+s*H, PAD_TOP, PAD_TOP+PAD_LIP, METAL)
        for a, c in ((-H+t, -PAD_GAP), (PAD_GAP, H-t)):
            wall(px+s*(H-t), px+s*H, pz+a, pz+c, PAD_TOP, PAD_TOP+PAD_LIP, METAL)
    def decal(x0, x1, z0, z1, mat):
        y = PAD_TOP+.02
        mesh.quad((px+x0, y, pz+z0), (px+x0, y, pz+z1), (px+x1, y, pz+z1), (px+x1, y, pz+z0), mat, False)
    k, w = 9.5, .5
    decal(-k, k, -k, -k+w, BRONZE); decal(-k, k, k-w, k, BRONZE)
    decal(-k, -k+w, -k+w, k-w, BRONZE); decal(k-w, k, -k+w, k-w, BRONZE)
    b.prism(px, pz, 3.2, 3.2, PAD_TOP+.01, PAD_TOP+.03, 16, accent, solid=False)
    slots = []
    for sx_, sz_ in PAD_SLOTS:
        hs, tt = 1.6, .22
        decal(sx_-hs, sx_+hs, sz_-hs, sz_-hs+tt, METAL); decal(sx_-hs, sx_+hs, sz_+hs-tt, sz_+hs, METAL)
        decal(sx_-hs, sx_-hs+tt, sz_-hs+tt, sz_+hs-tt, METAL); decal(sx_+hs-tt, sx_+hs, sz_-hs+tt, sz_+hs-tt, METAL)
        slots.append((px+sx_, PAD_TOP, pz+sz_))
    for sx_ in (-1, 1):
        for sz_ in (-1, 1):
            x, z = px+sx_*(H-t/2), pz+sz_*(H-t/2)
            b.prism(x, z, .12, .12, PAD_TOP+PAD_LIP, PAD_TOP+PAD_LIP+1.1, 6, METAL, solid=False)
            b.prism(x, z, .2, .2, PAD_TOP+PAD_LIP+1.1, PAD_TOP+PAD_LIP+1.35, 8, GLOW, solid=False)
            b.lamp((x, PAD_TOP+PAD_LIP+1.3, z), .5)

    lift = SPAWN_LIFT
    return {
        'flag': (fx, top+PLINTH+.05, fz_),
        'spawn': (-13.0, lift, 6.0),
        # (x, y, z, local yaw): yaw 0 faces the base front (-Z). None in the
        # hall (its east side holds the stair down to the generator): two in
        # the inventory room, two in the back room.
        'spawn_points': [(-13.0, lift, 6.0, -math.pi/2), (13.0, lift, 6.0, math.pi/2),
                         (-10.0, lift, 15.8, -math.pi/2), (10.0, lift, 15.8, math.pi/2),
                         (-3, HUT_FLOOR+lift, HZ0+12, 0),
                         (cx, deck+lift, cz+3.9, -math.pi/4),  # diagonal across the deck, not down the attack ramp
                         (px-5, PAD_TOP+lift, pz+4, 0), (px+5, PAD_TOP+lift, pz+4, 0)],
        'entrances': [(0, .2, GATE_Z), (mid, .2, TZ0), (mid, HUT_FLOOR+.2, HZ0), ((xa+xb)/2, HUT_RING+.2, zg),
                      (T_EXIT[1], EXIT_GROUND+.2, sum(EXIT_DOOR)/2)],
        'generator': generator,
        'gen_entrances': [((HALL_HOLE[0]+HALL_HOLE[1])/2, .2, HALL_HOLE[2]),
                          (VAULT[1], VAULT_FLOOR+.2, sum(TUN_DOOR)/2)],
        'stair_head': ((STAIR[0]+STAIR[1])/2, 0.0, STAIR[2]),
        'vault_view': (VAULT[0]+.8, VAULT_FLOOR+1.7, VAULT[3]-.8),
        'tunnel_view': (T_EAST[0]+3.0, east_floor(T_EAST[0]+3.0)+1.7, 8.0),
        'tunnel_corner_view': (T_SOUTH[0]+1.6, 3.0+1.7, 10.4),
        'exit': (T_EXIT[1], EXIT_GROUND, sum(EXIT_DOOR)/2),
        'roof_turret': ROOF_TURRET,
        'sentry': sentry,
        'battery': battery,
        'tower_top': (fx, top, fz_),
        'pad_deck': (px, PAD_TOP, pz),
        'deploy_slots': slots,
        'hall_view': (-12, 1.7, -13),
        'hut_view': (-6, HUT_FLOOR+1.7, HZ0+22),
        'exterior_view': (-40, 14, -70),
    }
