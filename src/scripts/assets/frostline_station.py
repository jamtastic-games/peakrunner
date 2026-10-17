"""Original Frostline team base: a two-storey polar research station on a
mountain shelf, with a relay outpost forward on the team's right flank, a
plasma emplacement forward on its left flank and a vehicle apron with deploy
slots beside the station.

No source mesh, texture, lightmap or parser is used by this builder. Local
X/Z are horizontal, Y is up, the station's first floor is Y=0 and the shelf it
stands on is Y=G. -Z is the front (field-facing) side; facing the field the
right hand is +X.

Station: an insulated plinth lifts both storeys above the snow. Level 1 holds
the spawn hall with two inventory stations and a rear hall; Level 2 is the
command deck with the flag, open window bands and a roof hatch. Ramps along
the west and east walls join the levels, so the deck has two approaches and
nobody has to leave the way they came. Three doors reach Level 1 from three
sides: the front porch, the rear door and an east side door. The doors and
the command deck's window bands are open: players and shots go straight
through. Doors are 6 m wide and 5.5 m tall. A 10 x 10 m void in the command
deck in front of the flag (VOID) makes the hall and the deck one two-level
space, entered from either level; the roof hatch sits over it. Turrets face the field with a limited field of fire
(turret_arcs.py) rather than being walled off, and no spawn sits in a
straight line through an opening. Baffles remain only at the basement
(generator room) door and inside the relay outpost (a spawn room).

Generator: in a basement under the hall, in cut terrain cells, with exactly
two ways in: a stair down from the hall and a tunnel from a sunken stair in
the east annex, a roofed service shed on the shelf. The tunnel's lid is
covered by the station and the annex, so no terrain hole is exposed.

The relay outpost is a small hut with an inventory, a repair pad and a turret
and sensor on its roof; it is the team's second spawn area.

Interior dressing is render-only and stays within the 0.52 m player radius of
a solid surface. If the caller's mesh has a `lamps` list, light fixtures add
bake samples.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'frostline-station-v5'

G = -3.0                              # shelf ground under the station
SX, SZ0, SZ1, WALL = 14.0, -14.0, 14.0, .6
IX, IZ0, IZ1 = SX-WALL, SZ0+WALL, SZ1-WALL   # inner faces
L1, L1_CEIL, L2, L2_CEIL, ROOF = 0.0, 6.5, 7.5, 13.5, 14.5
SKIRT = G-.3
DOOR, DOOR_TOP = (-3.0, 3.0), 5.5
REAR_DOOR = (-12.0, -6.0)
# Roofed porch in front of the door (open straight through into the hall).
PORCH_X, PORCH_Z, PORCH_TOP = 3.2, -18.0, 6.0
PARTITION_Z, PART_DOOR, PART_DOOR_TOP = (6.0, 6.6), (4.0, 10.0), 5.5
PART_DOOR_W = (-10.0, -4.0)            # second partition doorway, west of the void
# West ramp: surface y = L2 at RAMP_TOP_Z down to L1 at RAMP_FOOT_Z.
RAMP_X = (-IX, -10.4)
RAMP_TOP_Z, RAMP_FOOT_Z = -10.0, 5.4
OPENING_Z = (RAMP_TOP_Z, -2.6)        # L2 floor opening over the ramp
HATCH = (-3.0, 3.0, -3.0, 3.0)        # roof hatch x0, x1, z0, z1, over the void
# Two-level void: a 10 x 10 m hole in the command deck in front of the flag,
# so the hall and the deck are one space; enter it from either level.
VOID = (-5.0, 5.0, -4.0, 6.0)
WINDOW_BAND = (9.2, 11.8)
FRONT_WINDOWS = ((-11.0, -4.0), (4.0, 11.0))
SIDE_WINDOWS = ((-10.0, -3.0), (3.0, 10.0))
FLAG = (0.0, 8.0)
PLINTH = .3
INVENTORIES = ((-6.0, 3.4), (-1.0, 3.4))
CRATES = (((1.0, -4.5), (2.4, 1.5, .8)), ((6.0, -1.0), (.8, 1.5, 2.6)))
# East ramp, the mirror of the west one: the deck's second approach.
E_RAMP_X = (10.4, IX)
E_OPENING_Z = OPENING_Z
# East side door into the rear hall and the stair down to the shelf.
EAST_DOOR, EAST_DOOR_TOP = (6.8, 12.8), 5.5
EAST_STAIR_FOOT_X = SX+6.0

# --- Basement generator room (v3) ---------------------------------------------
# Terrain holes: whole 8 m cells. With the base origins in maps/frostline.json
# (world x/z multiples of 8), local x and z must be multiples of 8 too.
B_FLOOR, B_CEIL = -7.5, L1-1.0             # floor top; underside of the hall floor
B_CELLS = (-8.0, 8.0, -8.0, 8.0)
B_IN = (-7.4, 7.4, -7.4, 7.4)
B_STAIR = (-7.4, -4.4, -7.4, 6.6)          # x0, x1, z at the hall floor, z at the basement floor
B_OPENING = (-7.4, -4.4, -7.4, 0.0)        # railed opening in the hall floor over the stair
B_DOOR = (-6.6, -1.4)                      # tunnel door, in the basement's east wall
B_BAFFLE = (4.8, 5.4, -7.4, 0.4)
GENERATOR = (0.0, B_FLOOR, 2.5)
T_CELLS = (8.0, 24.0, -8.0, 0.0)           # tunnel; it runs 0.8 m inside its cells
T_IN = (-7.2, -0.8)
T_CEIL = G-.4                              # underside of the lid
X_STAIR = (16.0, 24.0)                     # x at the tunnel floor, x at the shelf
ANNEX = (16.0, 32.6, -8.6, 0.6)            # service shed over the sunken stair (outer faces)
ANNEX_ROOF = G+4.4
ANNEX_DOOR, ANNEX_DOOR_TOP = (26.0, 30.0), G+3.4
DUCT = (14.0, 16.0, -8.6, 0.6)             # cable duct between the station and the shed
HOLES = {'basement': B_CELLS, 'tunnel': T_CELLS}
CONSOLES = (((-7.5, -12.4), (4.0, 1.1, .9)), ((7.5, -12.4), (4.0, 1.1, .9)), ((9.0, 10.0), (3.0, 1.1, 2.0)))
ROOF_TURRET = (-9.0, -9.0)
DOME = (0.0, 9.0)
# Front deck and stair down to the shelf.
DECK = (-5.0, 5.0, PORCH_Z, SZ0)
STAIR_X, STAIR_FOOT_Z = 4.0, -27.0
REAR_STAIR_FOOT_Z = SZ1+9.0           # rear stair from the back door down to the shelf
# Relay outpost (right flank, forward) and plasma emplacement (left flank).
OUTPOST, OG = (104.0, -72.0), 20.0
OH, OW, O_CEIL, O_ROOF = 7.0, .5, 4.6, 5.3
O_DOOR, O_DOOR_TOP = (-1.8, 1.8), 3.4
O_PORCH_X, O_PORCH_D = 1.9, 3.0
O_BAFFLE_Z, O_BAFFLE_X = (3.6, 4.0), 5.0
O_INVENTORY, O_REPAIR = (-3.4, -1.0), (2.8, -2.6)
EMPLACEMENT, EG = (-96.0, -58.0), 26.0
E_R0, E_R1, E_H = 6.4, 6.0, 2.4
E_RAMP = (5.4, 12.4)                   # z offsets of the access ramp's top and foot
APRON, APRON_HALF = (-32.0, 0.0), (9.0, 9.0)
APRON_TOP = G+.12
DEPLOY_SLOTS = ((-4.5, -4.5), (4.5, -4.5), (-4.5, 4.5), (4.5, 4.5))
SPAWN_LIFT = 1.2
# (x, floor, z, yaw); yaw 0 faces -Z, -pi/2 faces +X, pi/2 faces -X, pi faces +Z.
# Station spawns all stand in the front hall, near the front wall or down its
# sides, where no straight line through an open door or window reaches them
# from the field (the rear hall is open at both ends and the command deck has
# window bands all round).
STATION_SPAWNS = ((-8.0, L1, -11.6, -math.pi/2), (8.0, L1, -11.6, math.pi/2), (-9.0, L1, -6.0, math.pi),
                  (8.0, L1, -6.0, math.pi/2), (9.2, L1, -4.0, 0.0))
OUTPOST_SPAWNS = ((-3.4, -3.6, -3*math.pi/8), (2.6, 1.8, math.pi/2), (-4.8, 5.3, -math.pi/2))

HULL, DECKING, GRATE, METAL, GLOW, WOOD, SNOWPINE = 'concrete', 'panel', 'grate', 'trim', 'light', 'bark', 'leaf'


def ramp_surface(z):
    """Station ramp surface height at z (L2 at the top, L1 at the foot)."""
    t = min(max((RAMP_FOOT_Z-z)/(RAMP_FOOT_Z-RAMP_TOP_Z), 0), 1)
    return L1+(L2-L1)*t


def stair_surface(z):
    t = min(max((z-STAIR_FOOT_Z)/(PORCH_Z-STAIR_FOOT_Z), 0), 1)
    return G+(L1-G)*t


def rear_stair_surface(z):
    t = min(max((z-SZ1)/(REAR_STAIR_FOOT_Z-SZ1), 0), 1)
    return L1+(G-L1)*t


def basement_stair_surface(z):
    """Hall floor at the top (B_STAIR z0) down to the basement floor."""
    _, _, z0, z1 = B_STAIR
    t = min(max((z-z0)/(z1-z0), 0), 1)
    return L1+(B_FLOOR-L1)*t


def east_stair_surface(x):
    """East door threshold (L1 at the wall) down to the shelf."""
    t = min(max((x-SX)/(EAST_STAIR_FOOT_X-SX), 0), 1)
    return L1+(G-L1)*t


def exit_stair_surface(x):
    """Tunnel floor at X_STAIR[0] up to the shed floor at X_STAIR[1]."""
    t = min(max((x-X_STAIR[0])/(X_STAIR[1]-X_STAIR[0]), 0), 1)
    return B_FLOOR+(G-B_FLOOR)*t


def sites():
    """Terrain surfaces to blend toward around the structures:
    (name, shape, local height, falloff metres). Shapes are ('rect', x0, x1,
    z0, z1) or ('disc', x, z, r) in local coordinates. Each flat bench runs at
    least one 8 m cell past its structure so no terrain triangle rises into a
    floor, deck or ramp."""
    ox, oz = OUTPOST; ex, ez = EMPLACEMENT
    return [
        ('station', ('rect', APRON[0]-APRON_HALF[0]-9, ANNEX[1]+9, STAIR_FOOT_Z-9, REAR_STAIR_FOOT_Z+9), G-.06, 30),
        ('outpost', ('rect', ox-OH-9, ox+OH+9, oz-OH-9, oz+OH+O_PORCH_D+9), OG-.06, 26),
        ('emplacement', ('disc', ex, ez, E_R0+9), EG-.06, 22),
        ('emplacement ramp', ('rect', ex-10, ex+10, ez, ez+E_RAMP[1]+9), EG-.06, 22),
    ]


def _runs_without(lo, hi, gaps):
    out, u = [], lo
    for a, c in sorted(gaps):
        if a > u: out.append((u, a))
        u = max(u, c)
    if hi > u: out.append((u, hi))
    return out


def build(mesh, team, circuit):
    if team not in (0, 1) or not circuit: raise ValueError('team and circuit required')
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=WOOD, metal=METAL, glow=GLOW)
    wall, slab = b.wall, b.slab

    def closed_side(x, z0, z1, bottom, surface):
        """Double-faced panel under a ramp side, from `bottom` up to 0.6 m
        under the ramp surface, so nobody can walk under the ramp."""
        u0, u1 = surface(z0)-.6, surface(z1)-.6
        if u0 < bottom and u1 < bottom: return
        if u0 < bottom or u1 < bottom:
            zc = z0+(bottom-u0)*(z1-z0)/(u1-u0)
            if u0 < bottom: z0, u0 = zc, bottom
            else: z1, u1 = zc, bottom
        pts = [(x, bottom, z0), (x, u0, z0), (x, u1, z1), (x, bottom, z1)]
        pts = [q for i, q in enumerate(pts) if q != pts[i-1]]
        if len(pts) == 3: mesh.triangle(pts, HULL); mesh.triangle(pts[::-1], HULL)
        else: mesh.quad(*pts, HULL); mesh.quad(*pts[::-1], HULL)

    def shell_z(z0, z1, door, door_top, windows):
        """Front or back wall along x at z0..z1, skirt to roof."""
        wall(-SX, SX, z0, z1, SKIRT, L1, HULL)
        for u0, u1 in _runs_without(-SX, SX, [door] if door else []):
            wall(u0, u1, z0, z1, L1, L2, HULL)
        if door: wall(door[0], door[1], z0, z1, door_top, L2, HULL)
        wall(-SX, SX, z0, z1, L2, WINDOW_BAND[0], HULL)
        for u0, u1 in _runs_without(-SX, SX, windows): wall(u0, u1, z0, z1, *WINDOW_BAND, HULL)
        wall(-SX, SX, z0, z1, WINDOW_BAND[1], ROOF, HULL)

    def closed_side_x(z, x0, x1, bottom, surface):
        """closed_side for a stair that climbs along x."""
        u0, u1 = surface(x0)-.6, surface(x1)-.6
        if u0 < bottom and u1 < bottom: return
        if u0 < bottom or u1 < bottom:
            xc = x0+(bottom-u0)*(x1-x0)/(u1-u0)
            if u0 < bottom: x0, u0 = xc, bottom
            else: x1, u1 = xc, bottom
        pts = [(x0, bottom, z), (x0, u0, z), (x1, u1, z), (x1, bottom, z)]
        pts = [q for i, q in enumerate(pts) if q != pts[i-1]]
        if len(pts) == 3: mesh.triangle(pts, HULL); mesh.triangle(pts[::-1], HULL)
        else: mesh.quad(*pts, HULL); mesh.quad(*pts[::-1], HULL)

    def xramp(x0, x1, z0, z1, y0, y1, mat):
        """A stair climbing along x: surface y0 at x0 to y1 at x1, with the
        same 0.6 m closed slab as the kit's z ramps."""
        a, d, c, bb_ = (x0, y0, z0), (x0, y0, z1), (x1, y1, z1), (x1, y1, z0)
        up = lambda p: (p[0], p[1]-.6, p[2])
        mesh.quad(a, d, c, bb_, mat)
        la, ld, lc, lb = up(a), up(d), up(c), up(bb_)
        mesh.quad(la, lb, lc, ld, METAL)
        for p, q in ((a, bb_), (d, c)):
            mesh.quad(p, q, up(q), up(p), METAL); mesh.quad(up(p), up(q), q, p, METAL)

    def shell_x(x0, x1, windows, door=None):
        if not door:
            wall(x0, x1, SZ0+WALL, SZ1-WALL, SKIRT, WINDOW_BAND[0], HULL)
        else:
            wall(x0, x1, SZ0+WALL, SZ1-WALL, SKIRT, L1, HULL)
            for u0, u1 in _runs_without(SZ0+WALL, SZ1-WALL, [door]): wall(x0, x1, u0, u1, L1, L2, HULL)
            wall(x0, x1, door[0], door[1], EAST_DOOR_TOP, L2, HULL)
            wall(x0, x1, SZ0+WALL, SZ1-WALL, L2, WINDOW_BAND[0], HULL)
        for u0, u1 in _runs_without(SZ0+WALL, SZ1-WALL, windows): wall(x0, x1, u0, u1, *WINDOW_BAND, HULL)
        wall(x0, x1, SZ0+WALL, SZ1-WALL, WINDOW_BAND[1], ROOF, HULL)

    # --- Station shell ------------------------------------------------------
    shell_z(SZ0, IZ0, DOOR, DOOR_TOP, FRONT_WINDOWS)
    shell_z(IZ1, SZ1, REAR_DOOR, DOOR_TOP, FRONT_WINDOWS)
    shell_x(-SX, -IX, SIDE_WINDOWS)
    shell_x(IX, SX, SIDE_WINDOWS, EAST_DOOR)
    # Floors: L1 over the plinth with the basement stair's opening; L2 with
    # both ramp openings; roof with the hatch.
    bx0, bx1, bz0, bz1 = B_OPENING
    # The hall floor stops at the walls' inner faces: run out to the outer
    # faces, its edges would share a plane with the wall faces and flicker.
    slab(-IX, bx0, IZ0, IZ1, L1, DECKING)
    slab(bx1, IX, IZ0, IZ1, L1, DECKING)
    slab(bx0, bx1, IZ0, bz0, L1, DECKING)
    slab(bx0, bx1, bz1, IZ1, L1, DECKING)
    ox0, ox1 = RAMP_X
    ex0, ex1 = E_RAMP_X
    vx0, vx1, vz0, vz1 = VOID
    slab(ox1, vx0, IZ0, IZ1, L2, DECKING)
    slab(vx1, ex0, IZ0, IZ1, L2, DECKING)
    slab(vx0, vx1, IZ0, vz0, L2, DECKING)
    slab(vx0, vx1, vz1, IZ1, L2, DECKING)
    # Accent trim on the void's four edge faces, so the drop reads.
    for axis, plane, into, u0, u1 in (('x', vx0, 1, vz0, vz1), ('x', vx1, -1, vz0, vz1),
                                      ('z', vz0, 1, vx0, vx1), ('z', vz1, -1, vx0, vx1)):
        b.face_quad(axis, plane, into, u0, u1, L2-.45, L2-.05, accent, .03)
    for x0, x1, (z0, z1) in ((ox0, ox1, OPENING_Z), (ex0, ex1, E_OPENING_Z)):
        slab(x0, x1, IZ0, z0, L2, DECKING)
        slab(x0, x1, z1, IZ1, L2, DECKING)
    hx0, hx1, hz0, hz1 = HATCH
    slab(-SX, hx0, SZ0, SZ1, ROOF, HULL)
    slab(hx1, SX, SZ0, SZ1, ROOF, HULL)
    slab(hx0, hx1, SZ0, hz0, ROOF, HULL)
    slab(hx0, hx1, hz1, SZ1, ROOF, HULL)
    # Roof parapet and the collar round the hatch.
    for x0, x1, z0, z1 in ((-SX, SX, SZ0, SZ0+.5), (-SX, SX, SZ1-.5, SZ1), (-SX, -SX+.5, SZ0+.5, SZ1-.5),
                           (SX-.5, SX, SZ0+.5, SZ1-.5)):
        wall(x0, x1, z0, z1, ROOF, ROOF+.9, HULL)
    for x0, x1, z0, z1 in ((hx0-.4, hx1+.4, hz0-.4, hz0), (hx0-.4, hx1+.4, hz1, hz1+.4),
                           (hx0-.4, hx0, hz0, hz1), (hx1, hx1+.4, hz0, hz1)):
        wall(x0, x1, z0, z1, ROOF, ROOF+1.0, METAL)
    # Porch: side walls and roof over the front deck, open into the hall.
    dx0, dx1, dz0, dz1 = DECK
    slab(dx0, dx1, dz0, dz1, L1, GRATE)
    for s in (-1, 1):
        wall(s*PORCH_X, s*(PORCH_X+.6), PORCH_Z, SZ0, L1, PORCH_TOP, HULL)
    slab(-PORCH_X-.6, PORCH_X+.6, PORCH_Z, SZ0, PORCH_TOP+.6, HULL, .6)
    # Under the deck: closed down to the shelf.
    wall(dx0, dx1, dz0, dz0+.4, SKIRT, L1-1, HULL)
    for s in (-1, 1): wall(s*dx1-.2, s*dx1+.2, dz0, dz1, SKIRT, L1-1, HULL)
    # Stair from the shelf up to the deck, with closed sides.
    mesh.ramp(0, 2*STAIR_X, STAIR_FOOT_Z, PORCH_Z, G, L1, GRATE)
    for s in (-1, 1): closed_side(s*STAIR_X, STAIR_FOOT_Z, PORCH_Z, G-.06, stair_surface)
    rx0, rx1 = REAR_DOOR; rxc = (rx0+rx1)/2
    mesh.ramp(rxc, rx1-rx0, SZ1, REAR_STAIR_FOOT_Z, L1, G, GRATE)
    for x in (rx0, rx1): closed_side(x, SZ1, REAR_STAIR_FOOT_Z, G-.06, rear_stair_surface)
    mesh.box((rxc, DOOR_TOP+.25, SZ1+.35), (rx1-rx0+.6, .3, .7), accent, False)
    mesh.box((rxc, DOOR_TOP+.45, SZ1+.4), (1.2, .1, .6), GLOW, False)
    b.lamp((rxc, DOOR_TOP-.2, SZ1+1.5), .5)
    # Rear hall partition with its door off the hall's axis.
    for u0, u1 in _runs_without(-IX, IX, [PART_DOOR, PART_DOOR_W]):
        wall(u0, u1, *PARTITION_Z, L1, L1_CEIL, HULL)
    for door in (PART_DOOR, PART_DOOR_W):
        wall(*door, *PARTITION_Z, PART_DOOR_TOP, L1_CEIL, HULL)
    # West and east ramps between the levels; their undersides are closed off.
    mesh.ramp((ox0+ox1)/2, ox1-ox0, RAMP_TOP_Z, RAMP_FOOT_Z, L2, L1, GRATE)
    closed_side(ox1, RAMP_TOP_Z, RAMP_FOOT_Z, L1, ramp_surface)
    wall(ox0, ox1, RAMP_TOP_Z-.4, RAMP_TOP_Z, L1, L2-1, HULL)
    mesh.ramp((ex0+ex1)/2, ex1-ex0, RAMP_TOP_Z, RAMP_FOOT_Z, L2, L1, GRATE)
    closed_side(ex0, RAMP_TOP_Z, RAMP_FOOT_Z, L1, ramp_surface)
    wall(ex0, ex1, RAMP_TOP_Z-.4, RAMP_TOP_Z, L1, L2-1, HULL)
    # East side door: open into the rear hall, stair down to the shelf outside.
    xramp(SX, EAST_STAIR_FOOT_X, *EAST_DOOR, L1, G, GRATE)
    for z in EAST_DOOR: closed_side_x(z, SX, EAST_STAIR_FOOT_X, G-.06, east_stair_surface)
    ezc = sum(EAST_DOOR)/2
    mesh.box((SX+.35, EAST_DOOR_TOP+.25, ezc), (.7, .3, EAST_DOOR[1]-EAST_DOOR[0]+.6), accent, False)
    mesh.box((SX+.4, EAST_DOOR_TOP+.45, ezc), (.6, .1, 1.2), GLOW, False)
    b.lamp((SX+1.5, EAST_DOOR_TOP-.2, ezc), .5)

    # --- Basement generator room and tunnel ---------------------------------
    cx0, cx1, cz0, cz1 = B_CELLS
    ix0, ix1, iz0, iz1 = B_IN
    slab(*B_CELLS, B_FLOOR, DECKING)
    wall(cx0, ix0, cz0, cz1, B_FLOOR, B_CEIL, HULL)
    wall(ix0, ix1, cz0, iz0, B_FLOOR, B_CEIL, HULL)
    wall(ix0, ix1, iz1, cz1, B_FLOOR, B_CEIL, HULL)
    for u0, u1 in _runs_without(cz0, cz1, [B_DOOR]): wall(ix1, cx1, u0, u1, B_FLOOR, B_CEIL, HULL)
    wall(ix1, cx1, *B_DOOR, T_CEIL, B_CEIL, HULL)
    wall(*B_BAFFLE[:2], *B_BAFFLE[2:], B_FLOOR, B_CEIL, HULL)
    # Stair down from the hall, closed underneath; rails round its opening.
    sx0, sx1, sz0, sz1 = B_STAIR
    mesh.ramp((sx0+sx1)/2, sx1-sx0, sz0, sz1, L1, B_FLOOR, GRATE)
    closed_side(sx1, sz0, sz1, B_FLOOR, basement_stair_surface)
    for x0, x1, z0, z1 in ((bx0-.2, bx0, bz0, bz1), (bx1, bx1+.2, bz0, bz1), (bx0-.2, bx1+.2, bz1, bz1+.2)):
        wall(x0, x1, z0, z1, L1, L1+1.1, METAL)
    # Tunnel east to the shed's sunken stair; lid under the station and the duct.
    tx0, tx1, tz0, tz1 = T_CELLS
    slab(*T_CELLS, B_FLOOR, DECKING)
    xs0, xs1 = X_STAIR
    for z0, z1 in ((tz0, T_IN[0]), (T_IN[1], tz1)):
        wall(tx0, xs0, z0, z1, B_FLOOR, T_CEIL, HULL)
        wall(xs0, tx1, z0, z1, B_FLOOR, G, HULL)
        wall(ANNEX[0]+WALL, tx1, z0, z1, G, G+1.1, METAL)          # stairwell rails
    slab(tx0, xs0, tz0, tz1, G, HULL, .4)
    xramp(xs0, xs1, *T_IN, B_FLOOR, G, GRATE)
    wall(*DUCT, G, G+2.0, METAL)   # on the tunnel lid (sunk into it, its end shared the lid's face over the stair)
    # Service shed over the stair: door on the south face, open inside.
    ax0, ax1, az0, az1 = ANNEX
    slab(xs1, ax1-WALL, az0+WALL, az1-WALL, G, DECKING)
    top = ANNEX_ROOF-.5                                            # underside of the shed roof
    wall(ax0, ax0+WALL, az0, az1, G-.3, top, HULL)
    wall(ax1-WALL, ax1, az0, az1, G-.3, top, HULL)
    wall(ax0+WALL, ax1-WALL, az1-WALL, az1, G-.3, top, HULL)
    for u0, u1 in _runs_without(ax0+WALL, ax1-WALL, [ANNEX_DOOR]):
        wall(u0, u1, az0, az0+WALL, G-.3, top, HULL)
    wall(*ANNEX_DOOR, az0, az0+WALL, ANNEX_DOOR_TOP, top, HULL)
    slab(ax0, ax1, az0, az1, ANNEX_ROOF, HULL, .5)
    # Cover crates in the hall, consoles on the command deck.
    for (x, z), (w, h, d) in CRATES:
        mesh.box((x, L1+h/2, z), (w, h, d), METAL)
        b.face_quad('z', z-d/2, -1, x-w/2+.15, x+w/2-.15, L1+.2, L1+h-.2, accent, .02)
    for (x, z), (w, h, d) in CONSOLES:
        mesh.box((x, L2+h/2, z), (w, h, d), METAL)
        mesh.box((x, L2+h+.02, z), (w-.3, .04, d-.3), GLOW, False)
    # Flag plinth.
    mesh.box((FLAG[0], L2+PLINTH/2, FLAG[1]), (2.6, PLINTH, 2.6), METAL)
    mesh.box((FLAG[0], L2+PLINTH+.02, FLAG[1]), (1.6, .04, 1.6), accent, False)

    # --- Station interior dressing (render only) ----------------------------
    b.dress('z', IZ0, 1, _runs_without(-IX, IX, [DOOR]), L1, L1_CEIL, pilasters=False)
    b.dress('z', PARTITION_Z[0], -1, _runs_without(-IX, IX, [PART_DOOR, PART_DOOR_W]), L1, L1_CEIL, pilasters=False)
    b.dress('x', IX, -1, [(RAMP_FOOT_Z, PARTITION_Z[0])], L1, L1_CEIL, pilasters=False)
    b.dress('x', IX, -1, _runs_without(PARTITION_Z[1], IZ1, [EAST_DOOR]), L1, L1_CEIL, stripe=False, pilasters=False)
    # Basement: liners on the free walls, light strips, tunnel and shed lamps.
    b.dress('z', iz1, -1, [(ix0, ix1)], B_FLOOR, B_CEIL, pilasters=True)
    b.dress('z', iz0, 1, [(sx1, B_BAFFLE[0])], B_FLOOR, B_CEIL, pilasters=True)
    b.dress('x', ix1, -1, _runs_without(iz0, iz1, [B_DOOR]), B_FLOOR, B_CEIL, pilasters=False)
    for x in (-2.0, 2.5): b.ceiling_strip('z', x, iz0+1.0, iz1-1.0, B_CEIL, .7)
    b.ceiling_strip('x', sum(T_IN)/2, tx0+.6, xs0-.6, T_CEIL, .5, spacing=4)
    b.ceiling_strip('x', sum(T_IN)/2, ax0+1.5, ax1-1.5, ANNEX_ROOF-.5, .6, spacing=4)
    mesh.box((sum(ANNEX_DOOR)/2, ANNEX_DOOR_TOP+.25, az0-.35), (ANNEX_DOOR[1]-ANNEX_DOOR[0]+.6, .3, .7), accent, False)
    b.lamp((sum(ANNEX_DOOR)/2, ANNEX_DOOR_TOP-.2, az0-1.5), .5)
    for x, into in ((ax0, -1), (ax1, 1)):
        b.face_quad('x', x, into, az0, az1, ANNEX_ROOF-.9, ANNEX_ROOF-.3, accent, .04)
    for z, into in ((az0, -1), (az1, 1)):
        b.face_quad('z', z, into, ax0, ax1, ANNEX_ROOF-.9, ANNEX_ROOF-.3, accent, .04)
    b.dress('z', IZ1, -1, _runs_without(-IX, IX, [REAR_DOOR]), L1, L1_CEIL, stripe=False, pilasters=False)
    b.dress('x', -IX, 1, [(RAMP_FOOT_Z, IZ1)], L1, L1_CEIL, pilasters=False)
    for axis, plane, into, runs in (('z', IZ0, 1, _runs_without(-IX, IX, FRONT_WINDOWS)),
                                    ('z', IZ1, -1, _runs_without(-IX, IX, FRONT_WINDOWS)),
                                    ('x', IX, -1, _runs_without(IZ0, IZ1, SIDE_WINDOWS)),
                                    ('x', -IX, 1, _runs_without(IZ0, IZ1, SIDE_WINDOWS))):
        b.dress(axis, plane, into, runs, L2, L2_CEIL, pilasters=False)
    for x in (-4.0, 4.0): b.ceiling_strip('z', x, IZ0+1.5, VOID[2]-1, L1_CEIL, .7)
    b.ceiling_strip('x', IZ0+1.2, -6.0, 6.0, L1_CEIL, .5)
    b.ceiling_strip('x', (PARTITION_Z[1]+IZ1)/2, -IX+2, IX-2, L1_CEIL, .7)
    for x in (-7.5, 7.5, 11.0): b.ceiling_strip('z', x, IZ0+1.5, IZ1-1.5, L2_CEIL, .7)
    # Window frames and mullions inside and out.
    for z, into in ((SZ0, -1), (SZ1, 1)):
        for u0, u1 in FRONT_WINDOWS:
            b.face_box('z', z, into, u0-.15, u1+.15, WINDOW_BAND[0]-.2, WINDOW_BAND[0], .12, METAL)
            b.face_box('z', z, into, u0-.15, u1+.15, WINDOW_BAND[1], WINDOW_BAND[1]+.2, .12, METAL)
            for u in (u0, (u0+u1)/2, u1): b.face_box('z', z, into, u-.08, u+.08, *WINDOW_BAND, .1, METAL)
    for x, into in ((-SX, -1), (SX, 1)):
        for u0, u1 in SIDE_WINDOWS:
            b.face_box('x', x, into, u0-.15, u1+.15, WINDOW_BAND[0]-.2, WINDOW_BAND[0], .12, METAL)
            b.face_box('x', x, into, u0-.15, u1+.15, WINDOW_BAND[1], WINDOW_BAND[1]+.2, .12, METAL)
            for u in (u0, (u0+u1)/2, u1): b.face_box('x', x, into, u-.08, u+.08, *WINDOW_BAND, .1, METAL)

    # --- Station exterior dressing (render only) ----------------------------
    # Team band along the floor line and the roof edge, legs at the plinth.
    for z, into in ((SZ0, -1), (SZ1, 1)):
        for u0, u1 in _runs_without(-SX, SX, []):
            b.face_quad('z', z, into, u0, u1, L2-1.0, L2-.1, accent, .04)
            b.face_quad('z', z, into, u0, u1, ROOF-.7, ROOF-.1, accent, .04)
            b.face_box('z', z, into, u0, u1, SKIRT, SKIRT+.5, .15, METAL)
    for x, into in ((-SX, -1), (SX, 1)):
        b.face_quad('x', x, into, SZ0, SZ1, L2-1.0, L2-.1, accent, .04)
        b.face_quad('x', x, into, SZ0, SZ1, ROOF-.7, ROOF-.1, accent, .04)
        b.face_box('x', x, into, SZ0, SZ1, SKIRT, SKIRT+.5, .15, METAL)
    for x in (-SX-.35, -4.7, 4.7, SX+.35):
        for z in (SZ0-.35, SZ1+.35):
            # Legs under the front deck stop at its underside (their caps
            # would otherwise lie in the deck's walking surface).
            top = L1-1 if (z < 0 and abs(x) < DECK[1]) else L1
            mesh.column((x, SKIRT, z), .35, top-SKIRT, METAL, 6, solid=False)
    # Emblem over the door: a white peak above a white frost line.
    ez = SZ0
    b.face_quad('z', ez, -1, -3.0, 3.0, 8.0, 13.2, accent, .05)
    b.face_quad('z', ez, -1, -2.4, 2.4, 8.6, 9.2, HULL, .07)
    peak = [(-2.3, 9.7), (0.0, 12.6), (2.3, 9.7)]
    mesh.triangle([(x, y, ez-.07) for x, y in (peak[0], peak[1], peak[2])], HULL, False)
    mesh.triangle([(x, y, ez-.08) for x, y in ((-1.0, 11.1), (0.0, 12.6), (1.0, 11.1))], GLOW, False)
    # Porch frame, door lamp, floodlights and name light.
    b.face_box('z', PORCH_Z, -1, -PORCH_X-.6, PORCH_X+.6, PORCH_TOP, PORCH_TOP+.6, .2, accent)
    mesh.box((0, PORCH_TOP-.12, (PORCH_Z+SZ0)/2), (1.6, .1, 1.6), GLOW, False)
    b.lamp((0, PORCH_TOP-.8, (PORCH_Z+SZ0)/2), .7)
    b.lamp((0, 3.0, PORCH_Z-2.0), .5)
    for x in (-SX+1, SX-1):
        for z in (SZ0+1, SZ1-1):
            mesh.box((x, ROOF+1.6, z), (.8, .5, .8), GLOW, False)
            mesh.column((x, ROOF+.9, z), .12, .7, METAL, 6, solid=False)
            b.lamp((x, ROOF+1.2, z), .6)
    for s in (-1, 1):
        mesh.box((s*(dx1-.2), L1+1.1, (dz0+dz1)/2), (.08, 1.0, dz1-dz0), METAL, False)
    for s in (-1, 1):
        mesh.box((s*STAIR_X, (G+L1)/2+.9, (STAIR_FOOT_Z+PORCH_Z)/2), (.08, .08, PORCH_Z-STAIR_FOOT_Z+.5), METAL, False)
    # Roof: radar dome on a drum, antenna mast with a beacon lamp.
    dx, dz = DOME
    mesh.column((dx, ROOF, dz), 2.6, 1.4, METAL, 10)
    for i, (r0, r1) in enumerate(((3.2, 3.0), (3.0, 2.5), (2.5, 1.7), (1.7, .6))):
        b.prism(dx, dz, r0, r1, ROOF+1.4+i*.8, ROOF+2.2+i*.8, 12, HULL, solid=False, cap=(i == 3))
    mesh.column((-11.0, ROOF, 11.0), .18, 9.0, METAL, 6, solid=False)
    for h in (4.0, 6.5): mesh.box((-11.0, ROOF+h, 11.0), (2.4, .08, .08), METAL, False)
    mesh.box((-11.0, ROOF+9.1, 11.0), (.5, .5, .5), GLOW, False)
    b.lamp((-11.0, ROOF+8.6, 11.0), .5)

    # --- Station equipment --------------------------------------------------
    mesh.equipment('generator', GENERATOR, team, circuit)
    for x, z in INVENTORIES: mesh.equipment('inventory', (x, L1, z), team, circuit)
    mesh.equipment('turret', (ROOF_TURRET[0], ROOF, ROOF_TURRET[1]), team, circuit, 'bullet')

    # --- Relay outpost ------------------------------------------------------
    ox, oz = OUTPOST
    oy = OG
    def owall(x0, x1, z0, z1, y0, y1, mat=HULL, solid=True): wall(ox+x0, ox+x1, oz+z0, oz+z1, oy+y0, oy+y1, mat, solid)
    owall(-OH, OH, -OH, OH, -1.0, 0.0, DECKING)
    owall(-OH, OH, -OH, -OH+OW, 0, O_ROOF)
    owall(-OH, -OH+OW, -OH+OW, OH-OW, 0, O_ROOF)
    owall(OH-OW, OH, -OH+OW, OH-OW, 0, O_ROOF)
    for u0, u1 in _runs_without(-OH, OH, [O_DOOR]): owall(u0, u1, OH-OW, OH, 0, O_ROOF)
    owall(*O_DOOR, OH-OW, OH, O_DOOR_TOP, O_ROOF)
    owall(-OH, OH, -OH, OH, O_CEIL, O_ROOF)
    for s in (-1, 1): owall(s*O_PORCH_X if s > 0 else -O_PORCH_X-.5, O_PORCH_X+.5 if s > 0 else -O_PORCH_X,
                            OH, OH+O_PORCH_D, 0, O_DOOR_TOP+.6)
    owall(-O_PORCH_X-.5, O_PORCH_X+.5, OH, OH+O_PORCH_D, O_DOOR_TOP+.6, O_DOOR_TOP+1.1)
    owall(-O_PORCH_X-.5, O_PORCH_X+.5, OH, OH+O_PORCH_D, -1.0, 0.0, GRATE)
    owall(-O_BAFFLE_X, O_BAFFLE_X, *O_BAFFLE_Z, 0, O_CEIL)
    for x0, x1, z0, z1 in ((-OH, OH, -OH, -OH+.4), (-OH, OH, OH-.4, OH), (-OH, -OH+.4, -OH+.4, OH-.4),
                           (OH-.4, OH, -OH+.4, OH-.4)):
        owall(x0, x1, z0, z1, O_ROOF, O_ROOF+.9)
    b.dress('x', ox-OH+OW, 1, [(oz-OH+OW, oz+O_BAFFLE_Z[0])], oy, oy+O_CEIL, pilasters=False)
    b.dress('x', ox+OH-OW, -1, [(oz-OH+OW, oz+O_BAFFLE_Z[0])], oy, oy+O_CEIL, pilasters=False)
    b.dress('z', oz-OH+OW, 1, [(ox-OH+OW, ox+OH-OW)], oy, oy+O_CEIL, pilasters=False)
    b.ceiling_strip('x', oz-1.5, ox-OH+1.5, ox+OH-1.5, oy+O_CEIL, .6)
    for z, into in ((oz-OH, -1), (oz+OH, 1)):
        b.face_quad('z', z, into, ox-OH, ox+OH, oy+O_ROOF-.8, oy+O_ROOF-.15, accent, .04)
    for x, into in ((ox-OH, -1), (ox+OH, 1)):
        b.face_quad('x', x, into, oz-OH, oz+OH, oy+O_ROOF-.8, oy+O_ROOF-.15, accent, .04)
    mesh.box((ox, oy+O_DOOR_TOP+.3, oz+OH+O_PORCH_D/2), (1.2, .1, 1.2), GLOW, False)
    b.lamp((ox, oy+O_DOOR_TOP-.4, oz+OH+O_PORCH_D/2), .6)
    # Lattice relay mast with a lamp.
    mx, mz = ox-3.8, oz+3.8
    for dx_, dz_ in ((-.8, -.8), (.8, -.8), (.8, .8), (-.8, .8)):
        mesh.column((mx+dx_, oy+O_ROOF, mz+dz_), .1, 12.0, METAL, 4, solid=False)
    for h in range(2, 12, 2): mesh.box((mx, oy+O_ROOF+h, mz), (1.8, .08, 1.8), METAL, False)
    mesh.box((mx, oy+O_ROOF+12.3, mz), (.6, .6, .6), GLOW, False)
    b.lamp((mx, oy+O_ROOF+11.8, mz), .5)
    # Outpost exterior: corner posts, team stripes, lit window slits, roof rail.
    for sx in (-1, 1):
        for sz in (-1, 1):
            mesh.box((ox+sx*(OH+.1), oy+O_ROOF/2, oz+sz*(OH+.1)), (.5, O_ROOF+.2, .5), METAL, False)
    for z, into in ((oz-OH, -1), (oz+OH, 1)):
        for u in (-4.5, 4.5):
            b.face_quad('z', z, into, ox+u-.35, ox+u+.35, oy+.4, oy+O_ROOF-.9, accent, .05)
        if into < 0:
            for u0, u1 in ((-3.4, -1.2), (1.2, 3.4)):
                b.face_quad('z', z, into, ox+u0, ox+u1, oy+2.3, oy+2.8, GLOW, .06)
                b.face_box('z', z, into, ox+u0-.1, ox+u1+.1, oy+2.2, oy+2.9, .08, METAL)
    for x, into in ((ox-OH, -1), (ox+OH, 1)):
        for u in (-4.5, 0.0, 4.5):
            b.face_quad('x', x, into, oz+u-.35, oz+u+.35, oy+.4, oy+O_ROOF-.9, accent, .05)
        b.face_quad('x', x, into, oz-3.0, oz+1.0, oy+2.3, oy+2.8, GLOW, .06)
        b.face_box('x', x, into, oz-3.1, oz+1.1, oy+2.2, oy+2.9, .08, METAL)
    for s in (-1, 1):
        mesh.box((ox, oy+O_ROOF+1.1, oz+s*(OH-.2)), (2*OH, .08, .08), METAL, False)
        mesh.box((ox+s*(OH-.2), oy+O_ROOF+1.1, oz), (.08, .08, 2*OH), METAL, False)
    mesh.equipment('inventory', (ox+O_INVENTORY[0], oy, oz+O_INVENTORY[1]), team, circuit)
    mesh.equipment('repair', (ox+O_REPAIR[0], oy, oz+O_REPAIR[1]), team, circuit)
    mesh.equipment('turret', (ox, oy+O_ROOF, oz-2.6), team, circuit, 'bullet')
    mesh.equipment('sensor', (ox+3.6, oy+O_ROOF, oz+3.4), team, circuit)

    # --- Plasma emplacement -------------------------------------------------
    ex, ez = EMPLACEMENT
    b.prism(ex, ez, E_R0, E_R1, EG-.4, EG+E_H, 8, HULL, phase=math.pi/8)
    b.prism(ex, ez, E_R1+.05, E_R1+.05, EG+E_H-.5, EG+E_H-.1, 8, accent, phase=math.pi/8, solid=False, cap=False)
    top = EG+E_H
    mesh.ramp(ex, 3.0, ez+E_RAMP[0], ez+E_RAMP[1], top, EG, GRATE)
    surface = lambda z: top+(EG-top)*min(max((z-ez-E_RAMP[0])/(E_RAMP[1]-E_RAMP[0]), 0), 1)
    for s in (-1, 1): closed_side(ex+s*1.5, ez+E_RAMP[0], ez+E_RAMP[1], EG-.06, surface)
    mesh.equipment('turret', (ex, top, ez), team, circuit, 'plasma')

    # --- Vehicle apron (deploy slots) ---------------------------------------
    ax, az = APRON; hx, hz = APRON_HALF
    mesh.box((ax, APRON_TOP-.25, az), (2*hx, .5, 2*hz), GRATE)
    for s in (-1, 1):
        mesh.box((ax, APRON_TOP+.02, az+s*(hz-.4)), (2*hx-.4, .04, .3), accent, False)
        mesh.box((ax+s*(hx-.4), APRON_TOP+.02, az), (.3, .04, 2*hz-.4), accent, False)
    for x, z in DEPLOY_SLOTS:
        mesh.box((ax+x, APRON_TOP+.03, az+z), (3.2, .03, .2), GLOW, False)
        mesh.box((ax+x, APRON_TOP+.03, az+z), (.2, .03, 3.2), GLOW, False)
    for x in (-hx, hx):
        for z in (-hz, hz):
            mesh.column((ax+x, APRON_TOP, az+z), .12, 3.2, METAL, 6, solid=False)
            mesh.box((ax+x, APRON_TOP+3.3, az+z), (.5, .3, .5), GLOW, False)
            b.lamp((ax+x, APRON_TOP+3.0, az+z), .4)

    spawn_points = [(x, f+SPAWN_LIFT, z, yaw) for x, f, z, yaw in STATION_SPAWNS]
    spawn_points += [(ox+x, OG+SPAWN_LIFT, oz+z, yaw) for x, z, yaw in OUTPOST_SPAWNS]
    return {
        'flag': (FLAG[0], L2+PLINTH+.05, FLAG[1]),
        'spawn': spawn_points[0][:3],
        'spawn_points': spawn_points,
        'entrances': [(0, L1, PORCH_Z), ((REAR_DOOR[0]+REAR_DOOR[1])/2, L1, SZ1),
                      (EAST_STAIR_FOOT_X, G, sum(EAST_DOOR)/2), (sum(ANNEX_DOOR)/2, G, ANNEX[2]),
                      (ox, OG, oz+OH+O_PORCH_D)],
        'hall': (0, L1, -3.0),
        'command_deck': (0, L2, 0.0),
        'generator': GENERATOR,
        'gen_entrances': [((B_STAIR[0]+B_STAIR[1])/2, L1, B_STAIR[2]), (B_IN[1], B_FLOOR, sum(B_DOOR)/2)],
        'basement': (0.0, B_FLOOR, -3.0),
        'annex': ((X_STAIR[1]+ANNEX[1])/2, G, (ANNEX[2]+ANNEX[3])/2),
        'outpost': (ox, OG, oz),
        'emplacement': (ex, top, ez),
        'deploy_slots': [(ax+x, APRON_TOP, az+z) for x, z in DEPLOY_SLOTS],
    }
