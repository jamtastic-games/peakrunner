"""Old Holler base (asset old-holler-base-v8).

The base follows its reference's layout: a main base dug into the ground
(the basement hall, atrium, wide ski ramps) with its generator underground,
and the flag on a tower on the roof, where the bishop tower stood (v8: the
flag tower of v7 moved back from in front of the base, its skybridge gone):

- The flag tower: an octagonal plinth on the roof, a core with four pillars
  carrying an open octagonal deck 9 m above the roof (the bishop chamber's
  floor height, so the flag stands where it did), and four slim pylons
  carrying a canopy over it with a rocket turret on top. The deck is open on every
  side: jet up onto it from the roof, or fly straight across it and take the
  flag. The roof's plasma turrets cover it.
- The look of the tower and the base's outside is an authored, hard-surface
  model (scripts/models/old_holler_models.py, the map's models.glb); the
  tower's solid geometry here stands in for it where models aren't drawn
  (`mesh.standin`).

The earlier cleanup items still hold:

- Walls meet at the corners instead of overlapping, and the roof sits on
  the walls with eaves, so no two visible faces share a plane (z-fighting).
- Each hall ramp climbs to an opening in its roof half instead of into the
  roof slab, and the outer shoulder ramps end flush with the roof edge.
- Where a ramp's underside is lower than 2.4 m above the floor beneath it,
  a solid closure stops players walking under it.
- Hall walls get liners, baseboards, cornices, pilasters and a team stripe;
  lit strips under the roof replace the old floating wall lights.
- Eight spawn points per team, 1.2 m above solid floor, facing open space.
- The generator sits in a basement under the hall with exactly two ways in:
  a stair down from the atrium floor, and a service stair that climbs from
  a passage behind the basement to a shed against the hall's back wall.
  Its ceiling is 8 m above the floor (v5), so it can be jetted in.
  Everything underground stays inside the hall's existing terrain cut
  (local |x| < 32, -56 < z < 32; the 8 m cells align with the base origin).

Local coordinates: Y up, the entrance faces local -Z. Hall floor at -10,
roof top at 8.6, ground level at 0.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'old-holler-base-v8'
FLOOR, WALL_TOP, ROOF_TOP, APRON_TOP = -10.0, 7.4, 8.6, .03
LIFT = 1.2
UNDER, HEADROOM = .6, 2.4
# Openings in each roof half above the upper end of its hall ramp.
HOLE_X, HOLE_Z = (15.5, 24.5), (10.0, 19.0)
# Basement: floor top, ceiling (the hall slab's underside) and interior box.
B_FLOOR, B_CEIL = -19.2, FLOOR-1.2       # 8 m clear
B_X, B_Z0, B_Z1 = 11.0, -6.0, 24.0
# Atrium stair: 4 m wide at x = 0, down from the hall floor to the basement.
STAIR_W, STAIR_Z1 = 4.0, 10.6          # 9.2 m drop over 16.6 m, 29 degrees
OPEN_Z1 = 1.5            # hall-floor opening ends where stair headroom passes 2.6 m
# Service route: door in the basement's back wall, passage and landing,
# then a stair along +x in the rear strip up to a shed at ground level.
DOOR_X, DOOR_TOP = (-5.0, -1.0), -13.5
STRIP_Z = (28.0, 31.6)
SERVICE_X = (-1.0, 30.2)
TUNNEL_CLEAR = 4.5
CEIL_END = 19.6          # where the rear apron ends and the shed begins
SHED_X1, SHED_ROOF, DOOR_LINTEL = 32.0, 3.4, 3.0
# The flag tower on the roof's middle, behind the atrium: its axis (x, z);
# the plinth, core and the deck's octagons by their half widths across the
# flats; the pillars and pylons on the diagonals by their distance from the
# axis. The deck is the bishop chamber's floor, 9 m over the roof.
TOWER = (0.0, 20.0)
PLINTH_R, PLINTH_FOOT, PLINTH_TOP = 7.5, ROOF_TOP, ROOF_TOP+1.6
CORE_R = 3.0
PILLAR_AT, PILLAR_R = 5.3, .55
DECK_R, DECK_TOP, DECK_T = 7.5, ROOF_TOP+9.0, 1.0
PYLON_AT, PYLON_R = 6.3, .4
CANOPY_R, CANOPY_Y, CANOPY_TOP = 6.8, DECK_TOP+8.0, DECK_TOP+9.4


def close_under(mesh, x, width, z0, z1, y0, y1, floor, mat):
    """Solid closure under an ascending ramp wherever its underside is lower
    than HEADROOM above `floor`: two side panels and an end wall."""
    rise = (y1-y0)/(z1-z0)
    under = lambda z: y0+rise*(z-z0)-UNDER
    za = max(z0, z0+(floor-(y0-UNDER))/rise)
    zb = min(z1, z0+(floor+HEADROOM-(y0-UNDER))/rise)
    if zb <= za: return
    for sx in (x-width/2, x+width/2):
        mesh.triangle([(sx, floor, za), (sx, floor, zb), (sx, under(zb), zb)], mat)
    xa, xb = x-width/2, x+width/2
    mesh.quad((xa, floor, zb), (xb, floor, zb), (xb, under(zb), zb), (xa, under(zb), zb), mat)


def roof_half(mesh, side):
    """One roof half (x 12..32.6 mirrored) with its stairwell opening."""
    x0, x1 = sorted((side*12, side*32.6))
    hx0, hx1 = sorted((side*HOLE_X[0], side*HOLE_X[1]))
    y = (WALL_TOP+ROOF_TOP)/2; t = ROOF_TOP-WALL_TOP

    def slab(a, b, c, d):
        mesh.box(((a+b)/2, y, (c+d)/2), (b-a, t, d-c), 'panel')
    slab(x0, x1, -28, HOLE_Z[0])
    slab(x0, x1, HOLE_Z[1], 28.6)
    slab(x0, hx0, HOLE_Z[0], HOLE_Z[1])
    slab(hx1, x1, HOLE_Z[0], HOLE_Z[1])


def ramp_x(mesh, z0, z1, x0, x1, y0, y1, mat, solid=True):
    """A ramp climbing along +x (the kit's ramp runs along z): walking
    surface, a 0.6 m slab beneath it and closed long sides."""
    a, b, c, d = (x0, y0, z0), (x0, y0, z1), (x1, y1, z1), (x1, y1, z0)
    aa, bb, cc, dd = [(p[0], p[1]-UNDER, p[2]) for p in (a, b, c, d)]
    mesh.quad(a, b, c, d, mat, solid)
    mesh.quad(aa, dd, cc, bb, 'trim', solid)
    mesh.quad(a, d, dd, aa, 'trim', solid); mesh.quad(b, bb, cc, c, 'trim', solid)


def service_y(x):
    """Walking surface of the service stair, from B_FLOOR up to ground."""
    return B_FLOOR+(0-B_FLOOR)*(x-SERVICE_X[0])/(SERVICE_X[1]-SERVICE_X[0])


def basement(mesh, b, lit):
    """Generator basement under the hall and its two routes (see module doc)."""
    wall = lambda x0, x1, z0, z1, y0, y1, m='concrete': b.wall(x0, x1, z0, z1, y0, y1, m)
    # Room: floor slab, side walls on the slab, north and back walls between.
    wall(-B_X-1, B_X+1, B_Z0-1, B_Z1+1, B_FLOOR-1, B_FLOOR, 'grate')
    for s in (-1, 1): wall(s*B_X, s*(B_X+1), B_Z0-1, B_Z1+1, B_FLOOR, B_CEIL)
    wall(-B_X, B_X, B_Z0-1, B_Z0, B_FLOOR, B_CEIL)
    wall(-B_X, DOOR_X[0], B_Z1, B_Z1+1, B_FLOOR, B_CEIL)
    wall(DOOR_X[1], B_X, B_Z1, B_Z1+1, B_FLOOR, B_CEIL)
    wall(*DOOR_X, B_Z1, B_Z1+1, DOOR_TOP, B_CEIL, 'trim')
    # Atrium stair, sealed underneath (the wedge meets the north wall).
    h = STAIR_W/2
    mesh.ramp(0, STAIR_W, B_Z0, STAIR_Z1, FLOOR, B_FLOOR)
    toe = B_Z0+(STAIR_Z1-B_Z0)*(FLOOR-UNDER-B_FLOOR)/(FLOOR-B_FLOOR)
    mesh.triangle([(-h, B_FLOOR, B_Z0), (-h, B_FLOOR, toe), (-h, FLOOR-UNDER, B_Z0)], 'trim')
    mesh.triangle([(h, B_FLOOR, B_Z0), (h, FLOOR-UNDER, B_Z0), (h, B_FLOOR, toe)], 'trim')
    # Rails round the atrium opening (the stair side is open).
    for s in (-1, 1): wall(s*h, s*(h+.2), B_Z0, OPEN_Z1, FLOOR, FLOOR+1, 'trim')
    wall(-h-.2, h+.2, OPEN_Z1, OPEN_Z1+.2, FLOOR, FLOOR+1, 'trim')
    # Passage out of the back door, then a landing at the service stair's foot.
    px0, px1 = DOOR_X[0]-.4, SERVICE_X[0]
    wall(px0, px1, B_Z1+1, STRIP_Z[1], B_FLOOR-1, B_FLOOR, 'grate')
    wall(px0, DOOR_X[0], B_Z1+1, STRIP_Z[1], B_FLOOR, B_CEIL)
    wall(px1, px1+.4, B_Z1+1, STRIP_Z[0]-.4, B_FLOOR, B_CEIL)
    wall(*DOOR_X, STRIP_Z[0]-.4, STRIP_Z[0], DOOR_TOP, B_CEIL, 'trim')
    wall(*DOOR_X, *STRIP_Z, DOOR_TOP, DOOR_TOP+.6, 'trim')
    # Service stair under a sloped ceiling, walled on both long sides.
    x0, x1 = SERVICE_X
    ramp_x(mesh, *STRIP_Z, x0, x1, B_FLOOR, 0, 'grate')
    ramp_x(mesh, *STRIP_Z, x0, CEIL_END, B_FLOOR+TUNNEL_CLEAR+UNDER, service_y(CEIL_END)+TUNNEL_CLEAR+UNDER, 'panel')
    wall(x0, CEIL_END, STRIP_Z[0]-.4, STRIP_Z[0], B_FLOOR-1, B_CEIL)
    wall(px0, CEIL_END, STRIP_Z[1], 32, B_FLOOR-1, APRON_TOP-.6)
    # Shed over the stair's top end, its door facing away from the hall.
    wall(CEIL_END, CEIL_END+.4, *STRIP_Z, service_y(CEIL_END)+TUNNEL_CLEAR, SHED_ROOF)
    wall(CEIL_END, SHED_X1, STRIP_Z[1], 32, B_FLOOR-1, SHED_ROOF)
    wall(CEIL_END, SHED_X1+.3, STRIP_Z[0], 32.3, SHED_ROOF, SHED_ROOF+.5, 'panel')
    wall(SHED_X1-.4, SHED_X1, *STRIP_Z, DOOR_LINTEL, SHED_ROOF, 'trim')
    wall(x1, SHED_X1, *STRIP_Z, -UNDER, 0, 'grate')
    mesh.box(((CEIL_END+SHED_X1)/2, SHED_ROOF-.6, 32.05), (SHED_X1-CEIL_END, .5, .1), lit.accent, False)
    # Dressing and light: liners on the basement walls, strips under the
    # ceilings, and a lit strip down the stair's sloped ceiling.
    lit.dress('x', -B_X, 1, [(B_Z0+.2, B_Z1-.2)], B_FLOOR, B_CEIL)
    lit.dress('x', B_X, -1, [(B_Z0+.2, B_Z1-.2)], B_FLOOR, B_CEIL)
    lit.dress('z', B_Z0, 1, [(-B_X, -h), (h, B_X)], B_FLOOR, B_CEIL)
    lit.dress('z', B_Z1, -1, [(-B_X, DOOR_X[0]), (DOOR_X[1], B_X)], B_FLOOR, B_CEIL)
    for x in (-7, 7): lit.ceiling_strip('z', x, B_Z0+2, B_Z1-2, B_CEIL, 1.1)
    lit.ceiling_strip('z', sum(DOOR_X)/2, B_Z1+1.3, STRIP_Z[0]-.8, B_CEIL, .7, spacing=2)
    lit.ceiling_strip('x', sum(STRIP_Z)/2, DOOR_X[0]+.4, DOOR_X[1]-.4, DOOR_TOP, .7, spacing=2)
    zc, w = sum(STRIP_Z)/2, .45
    y = lambda x: service_y(x)+TUNNEL_CLEAR-.07
    xs = (x0+.4, CEIL_END-.4)
    mesh.quad((xs[0], y(xs[0]), zc-w/2), (xs[1], y(xs[1]), zc-w/2), (xs[1], y(xs[1]), zc+w/2),
              (xs[0], y(xs[0]), zc+w/2), 'light', False)
    for x in range(1, int(CEIL_END), 4): lit.lamp((x+.5, y(x+.5)-.3, zc), .8)
    lit.ceiling_strip('x', zc, CEIL_END+1, SHED_X1-1, SHED_ROOF, .8)


def octagon(b, cx, cz, r, y0, y1, mat, solid=True):
    """An octagonal prism, its flats r from the axis and facing the axes."""
    rr = r/math.cos(math.pi/8)
    b.prism(cx, cz, rr, rr, y0, y1, 8, mat, phase=math.pi/8, solid=solid)


def flag_tower(mesh, b):
    """The flag tower: solid geometry that stands in for the authored model
    (see the module doc)."""
    if not hasattr(mesh, 'standin'): mesh.standin = []
    start = len(mesh.vertices)
    tx, tz = TOWER
    octagon(b, tx, tz, PLINTH_R, PLINTH_FOOT, PLINTH_TOP, 'concrete')
    octagon(b, tx, tz, CORE_R, PLINTH_TOP, DECK_TOP-DECK_T, 'concrete')
    d = PILLAR_AT*math.sqrt(.5); e = PYLON_AT*math.sqrt(.5)
    for sx in (-1, 1):
        for sz in (-1, 1):
            octagon(b, tx+sx*d, tz+sz*d, PILLAR_R, PLINTH_TOP, DECK_TOP-DECK_T, 'trim')
            octagon(b, tx+sx*e, tz+sz*e, PYLON_R, DECK_TOP, CANOPY_Y, 'trim')
    octagon(b, tx, tz, DECK_R, DECK_TOP-DECK_T, DECK_TOP, 'grate')
    octagon(b, tx, tz, CANOPY_R, CANOPY_Y, CANOPY_TOP, 'panel')
    mesh.standin.append((start, len(mesh.vertices)))
    # Light for the bake: on the deck, under the canopy.
    b.lamp((tx, DECK_TOP+3.0, tz), 1.2)
    b.lamp((tx, CANOPY_Y-.6, tz), .8)


def build(mesh, team, circuit, equipment):
    accent = 'ember' if team == 0 else 'glacier'
    b = Builder(mesh, accent, interior='panel', metal='trim', glow='light')
    under = Builder(mesh, accent, interior='concrete', metal='trim', glow='light')
    # Hall floor and walls; side walls fit between the front and back walls.
    # The floor leaves the atrium stair's opening.
    h = STAIR_W/2
    b.slab(-32, 32, -28, B_Z0, FLOOR, 'grate', 1.2)
    b.slab(-32, 32, OPEN_Z1, 28, FLOOR, 'grate', 1.2)
    for s in (-1, 1): b.slab(s*h, s*32, B_Z0, OPEN_Z1, FLOOR, 'grate', 1.2)
    basement(mesh, b, under)
    wy, wh = (FLOOR+WALL_TOP)/2, WALL_TOP-FLOOR
    for x in (-31, 31): mesh.box((x, wy, 0), (2, wh, 52), 'concrete')
    mesh.box((0, wy, 27), (64, wh, 2), 'concrete')
    for x in (-21, 21): mesh.box((x, wy, -27), (22, wh, 2), 'concrete')
    mesh.box((0, (4+WALL_TOP)/2, -27), (20, WALL_TOP-4, 2), 'panel')
    # Roof halves with ramp openings, eaves on the sides and back; the centre
    # roof carries the flag tower. The atrium stays open to the sky.
    for side in (-1, 1): roof_half(mesh, side)
    mesh.box((0, (WALL_TOP+ROOF_TOP)/2, 16.3), (24, ROOF_TOP-WALL_TOP, 24.6), 'panel')
    # Entrance ramp down into the hall, its cheeks, and the retaining walls.
    mesh.ramp(0, 18, -52, -27, 0, FLOOR)
    for x in (-9.5, 9.5): mesh.box((x, -5, -40), (1, 10, 24), 'concrete')
    for x in (-31, 31): mesh.box((x, -5, -40.5), (2, 10, 25), 'concrete')
    for x in (-20, 20): mesh.box((x, -5, -51), (20, 10, 2), 'concrete')
    # Hall ramps to the roof openings, closed underneath where too low.
    for x in (-20, 20):
        mesh.ramp(x, 9, -19, HOLE_Z[1], FLOOR, ROOF_TOP)
        close_under(mesh, x, 9, -19, HOLE_Z[1], FLOOR, ROOF_TOP, FLOOR, 'trim')
    # Outer shoulders: ski ramps from the apron up to the roof's front edge.
    for x in (-22, 22):
        mesh.ramp(x, 18, -52, -28, APRON_TOP, ROOF_TOP, 'concrete')
        close_under(mesh, x, 18, -52, -28, APRON_TOP, ROOF_TOP, APRON_TOP, 'concrete')
    # The flag tower on the roof (see flag_tower).
    flag_tower(mesh, b)
    # The outside's cladding is the authored model's (old_holler_models.py).
    # Interior: dressed hall walls and lit strips under the roof.
    # Side runs stop short of the front and back trim so corners never overlap.
    for side in (-1, 1):
        b.dress('x', side*30, -side, [(-25.8, 25.8)], FLOOR, WALL_TOP)
    b.dress('z', 26, -1, [(-30, 30)], FLOOR, WALL_TOP)
    b.dress('z', -26, 1, [(-30, -10), (10, 30)], FLOOR, WALL_TOP)
    for side in (-1, 1):
        b.ceiling_strip('z', side*27, -24, 24, WALL_TOP, 1.1)
        b.ceiling_strip('z', side*13.7, -24, 24, WALL_TOP, .9)
    b.ceiling_strip('x', 12, -10, 10, WALL_TOP, .9)
    for item in equipment:
        mesh.equipment(item['kind'], item['position'], team, circuit, item.get('weapon', 'bullet'))
    hall, roof = FLOOR+LIFT, ROOF_TOP+LIFT
    tx, tz = TOWER
    return {
        'flag': (tx, DECK_TOP+.35, tz),
        'flag_tower': (tx, DECK_TOP, tz),
        # Just off the deck's four sides.
        'tower_entries': [(tx, DECK_TOP, tz-DECK_R-.8), (tx+DECK_R+.8, DECK_TOP, tz),
                          (tx, DECK_TOP, tz+DECK_R+.8), (tx-DECK_R-.8, DECK_TOP, tz)],
        'spawn': (48, 0, -65),
        # (x, y, z, local yaw): yaw 0 faces the entrance (-Z), pi faces +Z.
        'spawn_points': [(-10, hall, -18, math.pi), (10, hall, -18, math.pi),
                         (-12, hall, 14, 0), (12, hall, 14, 0),
                         (-27, hall, -23, math.pi), (27, hall, -23, math.pi),
                         (-27, roof, 4, 0), (27, roof, 4, 0)],
        'entrances': [(0, 0, -52), (-22, APRON_TOP, -52), (22, APRON_TOP, -52)],
        'hall': (0, hall, 0),
        'roof_openings': [(-20, ROOF_TOP, sum(HOLE_Z)/2), (20, ROOF_TOP, sum(HOLE_Z)/2)],
        'generator_room': (0, B_FLOOR+LIFT, 16),
        'stair_head': (0, FLOOR, B_Z0),
        'service_door': (SHED_X1, 0, sum(STRIP_Z)/2),
    }
