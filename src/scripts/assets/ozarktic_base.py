"""Original Ozarktic Blast team base, built in two styles on one plan so
neither side has more rooms or entrances (docs/ozarktic-blast.md):

- 'bluff' (Ember, on the Giant Hill): the outbuilding is a roofed bunker.
- 'hollow' (Glacier, in the valley): the outbuilding is an open crater shaft
  with a pylon standing in it.

Shared plan, local coordinates (X/Z horizontal, Y up, surrounding ground
Y=0, -Z faces the field, +X is the right hand facing the field):
- Command hall (HALL): open front, west and rear doors; two inventory
  stations and the eight spawns. A ramp outside the west wall climbs to the
  roof, where a turret stands.
- Generator room under the hall: exactly two ways in, the stair down from
  the hall floor (STAIR, against the west wall) and the tunnel door in its
  east wall.
- Tunnel east to the outbuilding, under a low walk-over causeway lid, so
  every terrain hole cell lies under a structure. A door in its back wall
  (BRANCH_DOOR) opens on the tunnel up to the outpost (ozarktic_tunnel).
- Outbuilding (OUT): a ground-level ledge entered through its front, a ramp
  down to the tunnel level, and the third inventory station.
- Flag deck (DECK): a raised slab on pillars in front of the hall, open to
  the sky, reached by a side ramp and a bridge from the hall roof.
- The spire (build_spire) stands on natural ground in front of the base; the
  build places it separately.

Terrain holes are whole 8 m cells (HOLES). With base origins on the 8 m grid,
every local hole edge is a multiple of 8. Dressing is render-only and stays
within the 0.52 m player radius of a solid surface. If the caller's mesh has
a `lamps` list, light fixtures add bake samples. No source geometry is used.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'ozarktic-base-v1'
SPIRE_ID = 'ozarktic-spire-v1'
STYLES = ('bluff', 'hollow')

W = .8                                     # wall thickness
FOOT = -3.0                                # surface walls run this far below ground
HALL = (-16.0, 16.0, -8.0, 16.0)           # outer x0, x1, z0, z1
ROOF, ROOF_T = 8.0, .8
DOOR_FRONT = (-4.0, 4.0, 5.0)              # x0, x1, top
DOOR_WEST = (-7.0, -3.0, 5.0)              # z0, z1, top (west wall)
DOOR_REAR = (-8.0, -2.0, 5.0)              # x0, x1, top (rear wall)
GEN_FLOOR, GEN_CEIL = -8.0, -1.0           # 7 m clear under the hall floor slab
UFOOT = GEN_FLOOR-1.0
STAIR = (-15.2, -11.2, -1.5, 12.4)         # x0, x1, z at the hall floor (y 0), z at the generator floor
STAIR_HOLE_Z = 8.0                         # the hall floor is open over the stair up to here
GEN = (4.0, GEN_FLOOR, 6.0)
TUN_Z, TUN_CEIL = (0.0, 8.0), GEN_FLOOR+4.5
TUN_DOOR = (1.6, 6.4)                      # z range of the tunnel doors
TUN_LID = .3                               # causeway top over the tunnel
BRANCH_DOOR = (24.8, 31.2)                 # x range of the back-wall door onto the outpost tunnel
OUT = (40.0, 56.0, -8.0, 16.0)
LEDGE_Z = -1.0                             # the outbuilding's ground ledge runs from its front wall to here
OUT_RAMP = (51.2, 55.2, LEDGE_Z, LEDGE_Z+13.9)
OUT_DOOR = (44.0, 50.0, 4.5)               # front opening: x0, x1, top
BUNKER_ROOF = 6.0
PARAPET = .9
OUT_INV = (46.0, GEN_FLOOR, 12.0)
PYLON = (47.5, 5.0)                        # the crater's pylon (hollow style)
DECK_Y, DECK_T = 6.0, .8
DECK = (-14.0, 14.0, -42.0, -18.0)
PILLARS = ((-11.0, -21.0), (11.0, -21.0), (-11.0, -39.0), (11.0, -39.0))
PILLAR_FOOT = -14.0
FLAG = (0.0, DECK_Y, -30.0)
BRIDGE = (-3.0, 3.0)                       # hall roof front edge down to the deck's back edge
DECK_RAMP = (14.0, 18.0, -11.6, -24.0)     # x0, x1, z at the ground, z at the deck (landing beyond)
DECK_LANDING = (14.0, 18.0, -30.0, -24.0)
ROOF_RAMP = (-21.0, -17.0, 14.0, -2.6)     # x0, x1, z at the ground, z at the roof
ROOF_LANDING = (-21.0, -16.0, -8.0, -2.6)
ROOF_TURRET = (10.0, ROOF, 8.0)
INVENTORIES = ((-6.0, 0.0, 13.0), (6.0, 0.0, 13.0))
SPAWNS = [(x, z) for z in (0.0, 5.0) for x in (-4.0, 0.0, 4.0, 8.0)]   # clear of the inventories' reach
SPAWN_LIFT = 1.2
HOLES = {'hall': (-16.0, 16.0, -8.0, 16.0), 'tunnel': (16.0, 40.0, 0.0, 8.0), 'outbuilding': (40.0, 56.0, -8.0, 16.0)}
# Spire, in its own local frame (ground Y=0 at its foot).
SPIRE_BASE_R, SPIRE_CORE, SPIRE_TOP = 3.6, 2.4, 30.0   # octagon radius at the foot and the top
SPIRE_LEDGE, SPIRE_LEDGE_Y = 7.0, 14.0
SPIRE_CAP = 3.5
SPIRE_TURRET = (0.0, SPIRE_LEDGE_Y, -4.7)
SPIRE_OFFSET = -72.0                       # spire foot, local z in front of the base origin

HULL, DECKING, GRATE, METAL, GLOW, WOOD = 'concrete', 'panel', 'grate', 'trim', 'light', 'bark'


def _clamp01(v):
    return min(max(v, 0), 1)


def ramp_y(ramp, z, y_a, y_b):
    """Surface height of a straight ramp (x0, x1, z_a, z_b) at z."""
    _, _, za, zb = ramp
    return y_a+(y_b-y_a)*_clamp01((z-za)/(zb-za))


def stair_y(z): return ramp_y(STAIR, z, 0.0, GEN_FLOOR)
def out_ramp_y(z): return ramp_y(OUT_RAMP, z, 0.0, GEN_FLOOR)
def deck_ramp_y(z): return ramp_y(DECK_RAMP, z, 0.0, DECK_Y)
def roof_ramp_y(z): return ramp_y(ROOF_RAMP, z, 0.0, ROOF)
def bridge_y(z): return ramp_y((0, 0, HALL[2], DECK[3]), z, ROOF, DECK_Y)


def sites(style):
    """Terrain surfaces to blend toward around the base:
    (kind, shape, surface(x, z) -> local height, falloff metres). Shapes are
    ('rect', x0, x1, z0, z1) or ('disc', x, z, r) in local coordinates. The
    flag deck's front half is left on the natural ground (on the hill it
    falls away beneath the deck)."""
    flat = lambda x, z: -.1
    out = [('compound', ('rect', ROOF_RAMP[0]-5, OUT[1]+5, DECK_LANDING[2]-2, HALL[3]+6), flat, 18)]
    if style == 'hollow':
        # The crater sits in a shallow bowl of its own.
        out.append(('crater bowl', ('disc', (OUT[0]+OUT[1])/2, (OUT[2]+OUT[3])/2, 18.0), flat, 16))
    return out


def _runs(lo, hi, gaps):
    out, u = [], lo
    for a, c in sorted(gaps):
        if a > u: out.append((u, a))
        u = max(u, c)
    if hi > u: out.append((u, hi))
    return out


def build(mesh, team, circuit, style):
    if team not in (0, 1) or not circuit: raise ValueError('team and circuit required')
    if style not in STYLES: raise ValueError(f'style must be one of {STYLES}')
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=WOOD, metal=METAL, glow=GLOW)
    wall, slab = b.wall, b.slab

    def panel_wall(axis, plane0, plane1, u0, u1, y0, y1, openings=(), mat=HULL):
        """A solid wall between planes plane0..plane1 (thickness), spanning
        u0..u1 and y0..y1, with rectangular openings (u0, u1, y0, y1)."""
        cuts = sorted({u0, u1, *(v for o in openings for v in o[:2] if u0 < v < u1)})
        for a, c in zip(cuts, cuts[1:]):
            mid = (a+c)/2
            gaps = [(o[2], o[3]) for o in openings if o[0] <= mid <= o[1]]
            for ya, yb in _runs(y0, y1, gaps):
                if axis == 'z': wall(a, c, plane0, plane1, ya, yb, mat)
                else: wall(plane0, plane1, a, c, ya, yb, mat)

    def closed_side(x, z0, z1, surface, bottom):
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

    def ramp(r, y_a, y_b, bottom, mat=GRATE):
        x0, x1, za, zb = r
        mesh.ramp((x0+x1)/2, x1-x0, za, zb, y_a, y_b, mat)
        f = lambda z: ramp_y(r, z, y_a, y_b)
        for x in (x0, x1): closed_side(x, za, zb, f, bottom)

    hx0, hx1, hz0, hz1 = HALL
    ix0, ix1, iz0, iz1 = hx0+W, hx1-W, hz0+W, hz1-W        # hall interior

    # --- Command hall -----------------------------------------------------
    panel_wall('z', hz0, iz0, hx0, hx1, UFOOT, ROOF, [(DOOR_FRONT[0], DOOR_FRONT[1], 0.0, DOOR_FRONT[2])])
    panel_wall('z', iz1, hz1, hx0, hx1, UFOOT, ROOF, [(DOOR_REAR[0], DOOR_REAR[1], 0.0, DOOR_REAR[2])])
    panel_wall('x', hx0, ix0, iz0, iz1, UFOOT, ROOF, [(DOOR_WEST[0], DOOR_WEST[1], 0.0, DOOR_WEST[2])])
    panel_wall('x', ix1, hx1, iz0, iz1, UFOOT, ROOF, [(TUN_DOOR[0], TUN_DOOR[1], GEN_FLOOR, TUN_CEIL)])
    slab(hx0, hx1, hz0, hz1, ROOF, HULL, ROOF_T)
    # Hall floor, open over the top of the generator stair.
    sx0, sx1, sz_top, _ = STAIR
    slab(sx1, ix1, iz0, iz1, 0.0, DECKING)
    slab(ix0, sx1, iz0, sz_top, 0.0, DECKING)
    slab(ix0, sx1, STAIR_HOLE_Z, iz1, 0.0, DECKING)
    for x0, x1, z0, z1 in ((sx1, sx1+.2, sz_top, STAIR_HOLE_Z), (ix0, sx1+.2, STAIR_HOLE_Z, STAIR_HOLE_Z+.2)):
        wall(x0, x1, z0, z1, 0.0, 1.1, METAL)
    # Interior dressing and lamps.
    b.dress('z', iz1, -1, _runs(ix0, ix1, [DOOR_REAR[:2]]), 0.0, ROOF-ROOF_T)
    b.dress('z', iz0, 1, _runs(ix0, ix1, [DOOR_FRONT[:2]]), 0.0, ROOF-ROOF_T)
    b.dress('x', ix1, -1, [(iz0, iz1)], 0.0, ROOF-ROOF_T)
    b.dress('x', ix0, 1, _runs(iz0, iz1, [DOOR_WEST[:2], (sz_top, iz1)]), 0.0, ROOF-ROOF_T)
    for x in (-6.0, 6.0): b.ceiling_strip('z', x, iz0+1, iz1-1, ROOF-ROOF_T, .9)
    for x, y, z in INVENTORIES: mesh.equipment('inventory', (x, y, z), team, circuit)
    # Team banner over the front door, a sloped canopy, and light strips:
    # the roofline all round, the corners and every door frame.
    b.face_quad('z', hz0, -1, -7.0, 7.0, DOOR_FRONT[2]+.5, ROOF-.6, accent, off=.05)
    b.face_box('z', hz0, -1, hx0, hx1, ROOF-.3, ROOF+.2, .3, METAL)
    c0, c1 = (DOOR_FRONT[0]-1.5, hz0), (DOOR_FRONT[1]+1.5, hz0)
    mesh.quad((c0[0], DOOR_FRONT[2]+.3, hz0-2.2), (c1[0], DOOR_FRONT[2]+.3, hz0-2.2),
              (c1[0], DOOR_FRONT[2]+1.1, hz0-.02), (c0[0], DOOR_FRONT[2]+1.1, hz0-.02), METAL, False)
    mesh.quad((c0[0], DOOR_FRONT[2]+1.1, hz0-.02), (c1[0], DOOR_FRONT[2]+1.1, hz0-.02),
              (c1[0], DOOR_FRONT[2]+.3, hz0-2.2), (c0[0], DOOR_FRONT[2]+.3, hz0-2.2), METAL, False)
    mesh.box((0, DOOR_FRONT[2]+.25, hz0-2.2), (c1[0]-c0[0], .08, .12), GLOW, False)
    for axis, plane, into, u0, u1 in (('z', hz0, -1, hx0, hx1), ('z', hz1, 1, hx0, hx1), ('x', hx0, -1, hz0, hz1), ('x', hx1, 1, hz0, hz1)):
        b.face_box(axis, plane, into, u0, u1, ROOF-.75, ROOF-.6, .08, GLOW)
    for x, z in ((hx0, hz0), (hx1, hz0), (hx0, hz1), (hx1, hz1)):
        mesh.box((x, (ROOF+.3)/2, z), (.9, ROOF+.3, .9), METAL, False)   # proud of the roof: no shared top face
        mesh.box((x+(.47 if x > 0 else -.47), ROOF/2, z+(.47 if z > 0 else -.47)), (.06, ROOF-1.4, .06), GLOW, False)
    for axis, plane, into, u0, u1, top in (('z', hz0, -1, *DOOR_FRONT), ('z', hz1, 1, *DOOR_REAR), ('x', hx0, -1, *DOOR_WEST)):
        for u in (u0, u1): b.face_box(axis, plane, into, u-.12, u+.12, 0.0, top, .06, GLOW)
        b.face_box(axis, plane, into, u0, u1, top-.12, top+.12, .06, GLOW)

    # --- Generator room ---------------------------------------------------
    slab(ix0, ix1, iz0, iz1, GEN_FLOOR, HULL)
    ramp(STAIR, 0.0, GEN_FLOOR, GEN_FLOOR)
    mesh.equipment('generator', GEN, team, circuit)
    for x in (-2.0, 10.0): b.ceiling_strip('z', x, iz0+1, iz1-1, GEN_CEIL, .8)
    b.lamp((GEN[0], GEN_FLOOR+6.2, GEN[2]), 1.1)

    # --- Tunnel and causeway lid -----------------------------------------
    tz0, tz1 = TUN_Z
    wall(hx1, OUT[0], tz0, tz0+W, UFOOT, TUN_CEIL, HULL)
    # The back wall, open onto the outpost tunnel, with the floor through.
    bx0, bx1 = BRANCH_DOOR
    wall(hx1, bx0, tz1-W, tz1, UFOOT, TUN_CEIL, HULL)
    wall(bx1, OUT[0], tz1-W, tz1, UFOOT, TUN_CEIL, HULL)
    slab(bx0, bx1, tz1-W, tz1, GEN_FLOOR, HULL)
    slab(hx1, OUT[0], tz0+W, tz1-W, GEN_FLOOR, HULL)
    wall(hx1, OUT[0], tz0, tz1, TUN_CEIL, TUN_LID, HULL)
    for z, sign in ((tz0, -1), (tz1, 1)):
        # Sloped skirts so the lid is walked over, not stubbed on.
        a, c = (hx1, TUN_LID, z), (OUT[0], TUN_LID, z)
        d, e = (hx1, -.3, z+sign*1.6), (OUT[0], -.3, z+sign*1.6)
        if sign < 0: mesh.quad(a, c, e, d, HULL)
        else: mesh.quad(a, d, e, c, HULL)
    b.ceiling_strip('x', (tz0+tz1)/2, hx1+1, OUT[0]-1, TUN_CEIL, .7, spacing=6)

    # --- Outbuilding ------------------------------------------------------
    ox0, ox1, oz0, oz1 = OUT
    ox0i, ox1i, oz0i, oz1i = ox0+W, ox1-W, oz0+W, oz1-W
    top = BUNKER_ROOF if style == 'bluff' else PARAPET
    front = [(OUT_DOOR[0], OUT_DOOR[1], 0.0, OUT_DOOR[2] if style == 'bluff' else top)]
    panel_wall('z', oz0, oz0i, ox0, ox1, UFOOT, top, front)
    panel_wall('z', oz1i, oz1, ox0, ox1, UFOOT, top)
    panel_wall('x', ox0, ox0i, oz0i, oz1i, UFOOT, top, [(TUN_DOOR[0], TUN_DOOR[1], GEN_FLOOR, TUN_CEIL)])
    panel_wall('x', ox1i, ox1, oz0i, oz1i, UFOOT, top)
    slab(ox0i, ox1i, oz0i, oz1i, GEN_FLOOR, HULL)
    # The ground ledge inside the front wall, open to the ramp at its east end.
    slab(ox0i, OUT_RAMP[0], oz0i, LEDGE_Z, 0.0, DECKING)
    slab(OUT_RAMP[0], ox1i, oz0i, LEDGE_Z, 0.0, DECKING)
    wall(ox0i, OUT_RAMP[0], LEDGE_Z, LEDGE_Z+.2, 0.0, 1.1, METAL)
    ramp(OUT_RAMP, 0.0, GEN_FLOOR, GEN_FLOOR)
    mesh.equipment('inventory', OUT_INV, team, circuit)
    if style == 'bluff':
        slab(ox0, ox1, oz0, oz1, BUNKER_ROOF, HULL, .8)
        b.ceiling_strip('z', (ox0+ox1)/2-2, oz0i+1, oz1i-1, BUNKER_ROOF-.8, .8)
        # Retaining wall into the hillside either side of the bunker.
        for x0, x1 in ((ox1, ox1+14.0), (hx1+2, ox0)):
            wall(x0, x1, oz1-1.2, oz1, -.5, 3.4, HULL)
        b.face_quad('z', oz0, -1, OUT_DOOR[0]-2, OUT_DOOR[1]+2, OUT_DOOR[2]+.3, BUNKER_ROOF-.4, accent, off=.05)
    else:
        # The crater shaft: open to the sky, a pylon standing in the pit.
        px, pz = PYLON
        b.prism(px, pz, 1.6, 1.3, GEN_FLOOR, 7.0, 8, METAL)
        b.prism(px, pz, 1.4, 1.4, 7.0, 7.6, 8, accent, solid=False)
        b.lamp((px, 8.2, pz), 1.0)
        b.lamp((px, GEN_FLOOR+3.0, pz-2.5), .8)
        b.face_box('z', oz0, -1, ox0, ox1, top-.1, top+.1, .3, METAL)
    b.lamp(((ox0+ox1)/2, GEN_FLOOR+3.5, (oz0+oz1)/2), .9)

    # --- Roof ramp and turret ---------------------------------------------
    ramp(ROOF_RAMP, 0.0, ROOF, -.3)
    lx0, lx1, lz0, lz1 = ROOF_LANDING
    slab(lx0, lx1, lz0, lz1, ROOF, GRATE, .6)
    mesh.equipment('turret', ROOF_TURRET, team, circuit)

    # --- Flag deck, bridge and deck ramp ----------------------------------
    dx0, dx1, dz0, dz1 = DECK
    slab(dx0, dx1, dz0, dz1, DECK_Y, DECKING, DECK_T)
    for x, z in PILLARS: wall(x-.7, x+.7, z-.7, z+.7, PILLAR_FOOT, DECK_Y-DECK_T, METAL)
    for x0, x1, z0, z1 in ((dx0, dx1, dz0-.12, dz0), (dx0, dx1, dz1, dz1+.12), (dx0-.12, dx0, dz0, dz1), (dx1, dx1+.12, dz0, dz1)):
        mesh.box(((x0+x1)/2, DECK_Y-.5, (z0+z1)/2), (x1-x0, .45, z1-z0), accent, False)
        mesh.box(((x0+x1)/2, DECK_Y-.15, (z0+z1)/2), (x1-x0+.02, .1, z1-z0+.02), GLOW, False)
    # A lit ring round the flag and light strips down the deck's pillars.
    for x, z in PILLARS: mesh.box((x, DECK_Y-DECK_T-2.0, z-.72), (.12, 3.2, .06), GLOW, False)
    fx, fy, fz = FLAG
    b.prism(fx, fz, 3.2, 3.0, DECK_Y, DECK_Y+.04, 16, METAL, solid=False)
    b.lamp((fx, DECK_Y+2.0, fz), 1.0)
    mesh.ramp(0.0, BRIDGE[1]-BRIDGE[0], hz0, dz1, ROOF, DECK_Y, GRATE)
    ramp(DECK_RAMP, 0.0, DECK_Y, -.3)
    for r, ya, yb in ((DECK_RAMP, 0.0, DECK_Y), (ROOF_RAMP, 0.0, ROOF)):
        for x in (r[0]+.1, r[1]-.1):
            mesh.quad((x-.06, ya+.03, r[2]), (x+.06, ya+.03, r[2]), (x+.06, yb+.03, r[3]), (x-.06, yb+.03, r[3]), GLOW, False)
            mesh.quad((x-.06, ya+.03, r[2]), (x-.06, yb+.03, r[3]), (x+.06, yb+.03, r[3]), (x+.06, ya+.03, r[2]), GLOW, False)
    lx0, lx1, lz0, lz1 = DECK_LANDING
    slab(lx0, lx1, lz0, lz1, DECK_Y, GRATE, .6)

    spawn_points = [((x, SPAWN_LIFT, z), 0.0) for x, z in SPAWNS]
    return dict(flag=FLAG, spawn=spawn_points[0][0], spawn_points=[(*p, yaw) for p, yaw in spawn_points],
                generator=GEN, stair_top=(sum(STAIR[:2])/2, 0.0, STAIR[2]),
                tunnel=((hx1+OUT[0])/2, GEN_FLOOR, sum(TUN_Z)/2),
                branch_door=(sum(BRANCH_DOOR)/2, GEN_FLOOR, tz1),
                outbuilding=((ox0+ox1)/2, 0.0, (oz0i+LEDGE_Z)/2),
                hall=(0.0, 0.0, 4.0), deck=(0.0, DECK_Y, (dz0+dz1)/2),
                spire=(0.0, 0.0, SPIRE_OFFSET))


def build_spire(mesh, team, circuit):
    """The tall spire in front of a base: a tapered octagonal core, a turret ledge at
    SPIRE_LEDGE_Y and the team's sensor on the cap. Local ground is Y=0; the
    core and its skirt run 4 m below it."""
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=WOOD, metal=METAL, glow=GLOW)
    # A tapered octagonal core with lit rings and a team band, a round ledge
    # plate and a cap.
    b.prism(0, 0, SPIRE_BASE_R, SPIRE_CORE, -4.0, SPIRE_TOP, 8, HULL, phase=math.pi/8)
    b.prism(0, 0, SPIRE_LEDGE, SPIRE_LEDGE, SPIRE_LEDGE_Y-.8, SPIRE_LEDGE_Y, 12, METAL)
    b.prism(0, 0, SPIRE_LEDGE+.05, SPIRE_LEDGE+.05, SPIRE_LEDGE_Y-.35, SPIRE_LEDGE_Y-.2, 12, GLOW, solid=False, cap=False)
    b.prism(0, 0, SPIRE_CAP, SPIRE_CAP, SPIRE_TOP, SPIRE_TOP+1.0, 8, METAL, phase=math.pi/8)
    taper = lambda y: SPIRE_BASE_R+(SPIRE_CORE-SPIRE_BASE_R)*(y+4.0)/(SPIRE_TOP+4.0)
    for y in (4.0, 9.0, 19.0, 25.0):
        r = taper(y)+.04
        b.prism(0, 0, r, r, y, y+.25, 8, GLOW, phase=math.pi/8, solid=False, cap=False)
    for y in (6.0, 21.5):
        r = taper(y)+.03
        b.prism(0, 0, r, r, y, y+1.4, 8, accent, phase=math.pi/8, solid=False, cap=False)
    mesh.equipment('turret', SPIRE_TURRET, team, circuit)
    mesh.equipment('sensor', (0.0, SPIRE_TOP+1.0, 0.0), team, circuit)
    b.lamp((0.0, SPIRE_LEDGE_Y+1.0, SPIRE_LEDGE-.5), .9)
    return dict(ledge=(0.0, SPIRE_LEDGE_Y, SPIRE_LEDGE-1.5), cap=(0.0, SPIRE_TOP+1.0, 0.0))
