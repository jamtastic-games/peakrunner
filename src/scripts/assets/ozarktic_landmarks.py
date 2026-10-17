"""Original neutral landmarks for Ozarktic Blast (docs/ozarktic-blast.md).

- The ship (build_ship): a strange hovering craft over the mesa. An
  octagonal hull on an open saucer ring, a domed roof, a halo ring, swept
  fins and a glowing core under its belly. Inside is one room, the map's
  hiding spot: entered only by jetting up through two belly hatches, with
  window slits in three of its walls to shoot out of. Local frame: origin
  at the centre of the room floor.
- The dry dock (build_dock): a basin in the east valley with two crane
  gantries spanning it, rails along both sides and stacked cargo containers
  for cover. A ship hovers over each basin, reached by jetting up from the
  basin floor through its belly hatches. Local frame: origin at the basin's
  centre on the rim ground, the basin running along local Z.
- The outpost (build_outpost): a small team building set into the west
  upland with two inventory stations and an open door facing its base.
  Local frame: floor Y=0, -Z is the door side.
- The perch (build_perch): a rock pillar on the low (valley) side with a
  walled sniper platform on top, reached by jetting or by a stair ramp up
  its back. Local frame: ground Y=0 at its foot, -Z faces the field.

No source geometry is used. Dressing is render-only. If the caller's mesh
has a `lamps` list, light fixtures add bake samples.
"""
import math

from assets.structure_kit import Builder

SHIP_ID = 'ozarktic-ship-v2'
PERCH_ID = 'ozarktic-perch-v1'
DOCK_ID = 'ozarktic-dock-v1'
OUTPOST_ID = 'ozarktic-outpost-v1'
HOVER = 18.0                       # ship floor above the dock rim

# Ship, all radii to octagon vertices.
R_HATCH_IN, R_HATCH_OUT = 3.0, 9.0 # the inner deck ring: its east and west octants are the hatches
R_WALL = 11.5                      # wall centre line
WALL_T = 1.0
R_DECK = 16.0                      # outer deck edge (the hull top)
R_KEEL = 10.0                      # the saucer ring's open bottom edge
KEEL_Y = -3.2
ROOM_H = 4.2                       # ceiling height
ROOF_TOP, R_ROOF_TOP = 8.2, 5.0
HATCH_OCTANTS = (0, 4)             # octant 0 faces +X, octant 4 faces -X
SLIT_OCTANTS = (2, 5, 7)           # walls with a firing slit
SLIT = (1.1, 1.9)                  # slit bottom and top above the floor
FIN_ANGLES = (math.radians(20), math.radians(128), math.radians(205), math.radians(300))

# Perch.
PERCH_H = 12.0                     # platform top above the foot
PERCH_HALF = 4.5                   # platform half-size
PERCH_WALL = 1.25                  # crouch cover
PERCH_RAMP = (-1.6, 1.6, 26.0, PERCH_HALF)   # x0, x1, z at the ground, z at the platform (up its back)

HULL, METAL, GLOW, ROCK, GRATE, PANEL = 'concrete', 'trim', 'light', 'rock', 'grate', 'panel'


def _octagon(r, i, y=0.0, phase=math.pi/8):
    a = phase+i*math.tau/8
    return (r*math.cos(a), y, r*math.sin(a))


def _octant_of(angle, phase=math.pi/8):
    """Octant index whose centre direction is nearest `angle`."""
    return round((angle-phase-math.pi/8)/(math.tau/8)) % 8


def build_ship(mesh):
    b = Builder(mesh, 'light', interior=HULL, metal=METAL, glow=GLOW)
    ring = lambda r, y: [_octagon(r, i, y) for i in range(8)]
    # Deck: an inner cap, then two rings of quads (up-facing); the hatches
    # are the missing inner-ring octants.
    cap = ring(R_HATCH_IN, 0.0)
    for i in range(8): mesh.triangle([(0, 0, 0), cap[(i+1) % 8], cap[i]], PANEL)
    for (r0, r1), skip in (((R_HATCH_IN, R_HATCH_OUT), HATCH_OCTANTS), ((R_HATCH_OUT, R_DECK), ())):
        a, c = ring(r0, 0.0), ring(r1, 0.0)
        for i in range(8):
            if i in skip: continue
            j = (i+1) % 8
            mesh.quad(a[i], a[j], c[j], c[i], PANEL)
            mesh.quad(a[i], c[i], c[j], a[j], HULL)        # underside
    # Hatch linings: short walls round each hatch so it reads as a shaft.
    for i in HATCH_OCTANTS:
        j = (i+1) % 8
        for p0, p1 in ((_octagon(R_HATCH_IN, i), _octagon(R_HATCH_OUT, i)), (_octagon(R_HATCH_IN, j), _octagon(R_HATCH_OUT, j))):
            b.beam((p0[0], p0[2]), (p1[0], p1[2]), .15, -1.2, 0.0, METAL, solid=False)
        a, c = _octagon(R_HATCH_OUT-.2, i, .02), _octagon(R_HATCH_OUT-.2, j, .02)
        mesh.box(((a[0]+c[0])/2, .03, (a[2]+c[2])/2), (1.0, .06, 1.0), GLOW, False)
    # The saucer ring under the deck, open in the middle.
    b.prism(0, 0, R_KEEL, R_DECK, KEEL_Y, 0.0, 8, HULL, phase=math.pi/8, cap=False)
    b.prism(0, 0, R_DECK+.25, R_DECK+.25, -.5, .1, 8, GLOW, phase=math.pi/8, solid=False, cap=False)
    # Walls: eight segments, three with a firing slit.
    for i in range(8):
        p0, p1 = _octagon(R_WALL, i), _octagon(R_WALL, i+1)
        spans = ((0.0, SLIT[0]), (SLIT[1], ROOM_H)) if i in SLIT_OCTANTS else ((0.0, ROOM_H),)
        for y0, y1 in spans: b.beam((p0[0], p0[2]), (p1[0], p1[2]), WALL_T, y0, y1, HULL)
        mid = ((p0[0]+p1[0])/2, (p0[2]+p1[2])/2)
        out = (mid[0]/math.hypot(*mid), mid[1]/math.hypot(*mid))
        b.beam((p0[0]+out[0]*.55, p0[2]+out[1]*.55), (p1[0]+out[0]*.55, p1[2]+out[1]*.55), .1, ROOM_H-.5, ROOM_H-.25, GLOW, solid=False)
    # Ceiling (down-facing) and the domed roof.
    ceil = ring(R_WALL+WALL_T/2, ROOM_H)
    for i in range(8): mesh.triangle([(0, ROOM_H, 0), ceil[i], ceil[(i+1) % 8]], METAL)
    b.prism(0, 0, R_WALL+WALL_T/2, R_ROOF_TOP, ROOM_H, ROOF_TOP, 8, HULL, phase=math.pi/8)
    b.prism(0, 0, 1.2, .2, ROOF_TOP, ROOF_TOP+9.0, 6, METAL, solid=False)
    for y in (ROOF_TOP+2.5, ROOF_TOP+5.5): b.prism(0, 0, .9, .9, y, y+.35, 6, GLOW, solid=False)
    b.ceiling_strip('x', 0.0, -7.0, 7.0, ROOM_H, .5)
    # The core: a glowing crystal hanging under the belly, and a halo ring.
    b.prism(0, 0, .2, 2.6, KEEL_Y-6.0, KEEL_Y-2.2, 6, GLOW, solid=False)
    b.prism(0, 0, 2.6, .2, KEEL_Y-2.2, KEEL_Y+.4, 6, GLOW, solid=False)
    b.lamp((0, KEEL_Y-3.0, 0), 1.2)
    halo = ring(R_DECK+7.0, 1.8)
    for i in range(8):
        a, c = halo[i], halo[(i+1) % 8]
        b.beam((a[0], a[2]), (c[0], c[2]), .5, 1.55, 2.05, GLOW, solid=False)
    # Swept fins: long blades raking down and out from the hull.
    for ang in FIN_ANGLES:
        x0, z0 = R_DECK*.8*math.cos(ang), R_DECK*.8*math.sin(ang)
        x1, z1 = (R_DECK+13)*math.cos(ang+.35), (R_DECK+13)*math.sin(ang+.35)
        b.beam((x0, z0), (x1, z1), .6, -1.8, .4, METAL)
        b.beam((x1*.97, z1*.97), (x1, z1), .7, -4.5, .4, METAL)
        b.beam((x0, z0), (x1, z1), .2, .4, .7, GLOW, solid=False)
        # Struts from the fins' tips to the halo.
        hx, hz = (R_DECK+7)*math.cos(ang+.2), (R_DECK+7)*math.sin(ang+.2)
        b.beam((hx, hz), (x1*.9, z1*.9), .3, .2, 1.8, METAL, solid=False)
    return dict(room=(0.0, 0.0, 0.0), hatches=[(_octagon((R_HATCH_IN+R_HATCH_OUT)/2, i+.5, 0.0)) for i in HATCH_OCTANTS],
                roof=(0.0, ROOF_TOP, 0.0), core=(0.0, KEEL_Y-4.0, 0.0))


def build_perch(mesh):
    b = Builder(mesh, 'light', interior=ROCK, metal=METAL, glow=GLOW)
    # Stacked rock drums narrowing to the platform, sunk 3 m into the ground.
    for (r0, r1, y0, y1, ph) in ((7.5, 6.2, -3.0, 4.0, .2), (6.2, 5.6, 4.0, 8.5, .9), (5.6, 5.4, 8.5, PERCH_H-.8, .5)):
        b.prism(0, 0, r0, r1, y0, y1, 7, ROCK, phase=ph)
    h = PERCH_HALF
    b.slab(-h, h, -h, h, PERCH_H, GRATE, .8)
    # Crouch walls on the three field-facing sides, open at the back where
    # the ramp arrives.
    for x0, x1, z0, z1 in ((-h, h, -h, -h+.4), (-h, -h+.4, -h, h), (h-.4, h, -h, h)):
        b.wall(x0, x1, z0, z1, PERCH_H, PERCH_H+PERCH_WALL, METAL)
    b.wall(-h, h, -h, -h+.45, PERCH_H+PERCH_WALL, PERCH_H+PERCH_WALL+.08, GLOW, solid=False)
    # The stair ramp up the back.
    x0, x1, zg, zt = PERCH_RAMP
    mesh.ramp((x0+x1)/2, x1-x0, zg, zt, 0.0, PERCH_H, GRATE)
    for x in (x0, x1):
        u0, u1 = -.6, PERCH_H-.6
        pts = [(x, -.4, zg), (x, max(u0, -.4), zg), (x, u1, zt), (x, -.4, zt)]
        mesh.quad(*pts, METAL); mesh.quad(*pts[::-1], METAL)
    b.lamp((0, PERCH_H+1.0, -h+1.0), .8)
    return dict(platform=(0.0, PERCH_H, 0.0), ramp_foot=((x0+x1)/2, 0.0, zg))


# Dry dock, local frame: basin centre on the rim ground.
BASIN = (16.0, 44.0, 7.0)          # half-width (x), half-length (z), depth
GANTRY_Z = (-22.0, 22.0)
GANTRY_X, GANTRY_H = 22.0, 24.0
RAIL_X = 19.0
CARGO = ((-28.0, -34.0, 2), (28.0, -18.0, 1), (-28.0, 20.0, 1), (29.0, 36.0, 2), (-27.0, 44.0, 1))
CONTAINER = (6.0, 2.6, 2.5)


def build_dock(mesh):
    b = Builder(mesh, 'light', interior=HULL, metal=METAL, glow=GLOW)
    hx, hz, depth = BASIN
    # Basin rim: a paved lip with a hazard edge (render only; the terrain
    # carries the collision).
    for side in (-1, 1):
        mesh.box((side*(hx+1.2), .03, 0), (2.4, .06, 2*hz+4.8), PANEL, False)
        mesh.box((side*(hx+.12), .07, 0), (.16, .08, 2*hz+4.3), 'ember', False)
        mesh.box((0, .03, side*(hz+1.2)), (2*hx, .06, 2.4), PANEL, False)
        # Rails for the gantries.
        for dx in (-.4, .4):
            mesh.box((side*RAIL_X+dx, .1, 0), (.18, .2, 2*hz+30), METAL, False)
    # Two portal gantries spanning the basin: solid legs, a walkable beam,
    # a trolley and lights.
    for gz in GANTRY_Z:
        for side in (-1, 1):
            b.wall(side*GANTRY_X-.8, side*GANTRY_X+.8, gz-1.2, gz+1.2, -1.0, GANTRY_H, METAL)
            mesh.box((side*GANTRY_X, .6, gz), (2.6, 1.2, 3.6), METAL)
        b.wall(-GANTRY_X-.8, GANTRY_X+.8, gz-1.4, gz+1.4, GANTRY_H, GANTRY_H+1.8, METAL)
        mesh.box((0, GANTRY_H+.95, gz-1.45), (2*GANTRY_X, .12, .06), GLOW, False)
        mesh.box((5.0, GANTRY_H-1.2, gz), (3.0, 2.4, 3.2), 'ember', False)
        b.lamp((0, GANTRY_H-1.0, gz), 1.0)
    # Cargo containers as cover (one or two high).
    cw, ch, cd = CONTAINER
    for x, z, n in CARGO:
        for k in range(n):
            mesh.box((x, ch/2+k*ch, z), (cw, ch, cd), 'glacier' if (k+int(z)) % 2 else 'ember')
    b.lamp((0, -depth+2.0, 0), 1.0)
    return dict(basin_floor=(0.0, -depth, 0.0), gantries=[(0.0, GANTRY_H+1.8, z) for z in GANTRY_Z])


# Outpost, local frame.
OUT_W, OUT_D, OUT_H = 8.0, 6.0, 5.5    # half-width, half-depth, roof top
OUT_DOOR = (-2.5, 2.5, 3.8)


def build_outpost(mesh, team, circuit):
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=HULL, metal=METAL, glow=GLOW)
    w, d, h, t = OUT_W, OUT_D, OUT_H, .7
    x0, x1, top = OUT_DOOR
    # The floor fills the room and the doorway, never under a wall (no
    # shared faces).
    b.slab(-w+t, w-t, -d+t, d-t, 0.0, PANEL, 1.0)
    b.slab(x0, x1, -d, -d+t, 0.0, PANEL, 1.0)
    b.wall(-w, w, d-t, d, -3.0, h, HULL)
    b.wall(-w, -w+t, -d, d-t, -3.0, h, HULL)
    b.wall(w-t, w, -d, d-t, -3.0, h, HULL)
    b.wall(-w+t, x0, -d, -d+t, -3.0, h, HULL)
    b.wall(x1, w-t, -d, -d+t, -3.0, h, HULL)
    b.wall(x0, x1, -d, -d+t, top, h, HULL)
    b.wall(x0, x1, -d, -d+t, -3.0, -1.0, HULL)
    b.slab(-w, w, -d, d, h+.6, METAL, .6)
    b.face_box('z', -d, -1, -w, w, h-.9, h-.75, .08, GLOW)
    for u in (x0, x1): b.face_box('z', -d, -1, u-.12, u+.12, 0.0, top, .06, GLOW)
    b.face_quad('z', -d, -1, x1+.8, w-1.0, 1.0, h-1.4, accent, off=.05)
    b.ceiling_strip('x', 0.0, -w+1.5, w-1.5, h, .8)
    for x in (-4.5, 4.5): mesh.equipment('inventory', (x, 0.0, d-t-2.0), team, circuit)
    return dict(door=(0.0, 0.0, -d), inside=(0.0, 0.0, 0.0))

