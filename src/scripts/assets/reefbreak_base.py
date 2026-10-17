"""Original Reefbreak team base: a cargo frigate hovering HOVER metres over a
sandbar just outside the reef, lying side-on to the atoll, with its flag
on the entry platform outside its cargo bay's port opening, mid-ship
(docs/reefbreak.md). Both
teams get the same ship; the map's point symmetry places them. Each team
also has a ground outpost (build_outpost) on an outer island behind its ship
and a reef beacon (build_beacon) beside the gangway.

The ship's look is an authored model (assets/models/structures/reefbreak-ship,
a hard-surface build by scripts/models/reefbreak_ship.py, docs/model-pipeline.md)
that the game draws from the map's models.glb (docs/map-pipeline.md,
"Authored models"). This module builds what plays, fitted to that model:

- the rooms inside it, drawn always and lit by the bake: the hold, the
  engine room, their doors and the deck hatch's shaft (`ROOMS`);
- the cuts that open the model where the rooms and their doors need it
  (`model_cuts`, `map_model`);
- collision for the outside: the hull's cross-sections sampled from the
  model (`hull_sections`), the bay's floor, roof and jambs, the
  ledges, the fins, the engine pods and nozzles and the hover pods. That
  geometry also stands in for the model where models aren't drawn (the
  browser build), so the builder records it in `mesh.standin` (vertex float
  ranges) for the pack's `standin` run.

Base-local coordinates (X/Z horizontal, Y up, the hold floor Y=0, -Z faces
the field) are the model's frame: the ship lies along X, its bow at +X and
its port side toward the field. The rooms are laid out in the hull's own
frame (hull-local: the bow at -Z, port at -X), turned by HULL_TURN into the
base frame; `to_base` converts.

Hull-local:
- Bay (BAY_Z, through the hull from flank to flank), mid-ship: the port
  opening faces the field and the starboard one is right across, so a
  capper can fly straight through. Ledges run under both openings, level
  with the bay's floor; the port one is the entry platform, where the
  gangway arrives and the flag stands on the flat floor (FLAG_HULL).
- Hold (HOLD_Z, forward of the bay): the eight spawns. Ways in: the door
  from the bay and the deck hatch (HATCH) on the spine; a hatch in its
  floor (HOLD_HATCH) drops to the station room. No hull opening shows the
  spawns to the sea.
- Station room (middle level, under the hold): the three inventory
  stations. Ways in: the hold's floor hatch, a port door facing the field,
  and the door onto the balcony.
- Balcony (middle level, under the entry platform): open to the field; a
  door into the station room, and a porthole up through the bay's floor to
  the flag's level.
- Generator room (bottom, in the keel under the bridge): the generator on
  a floor with no door, reached only from the back: players come onto its
  aft gallery from the aft room (where the stern's low-gravity column comes
  up) or the starboard side door, and drop. A fore gallery inside makes a
  ledge for defenders.
- Top: the deck and its raised spine (the hatch opens in it), the bridge
  roof with the turret and the sensor on the model's mast step.
- The gangway (base-local): the walking route, a ramp from the reef crest up
  to the entry platform, so heavy armor gets in too. The model draws it; its
  collision here stands in for it.

Nothing touches the ground but the gangway's legs, so no terrain holes are
needed. If the caller's mesh has a `lamps` list, light fixtures add bake
samples. No source geometry is used: the model is original (its prompt and
concept are in the model sources).
"""
import functools
import math
from pathlib import Path

import numpy as np

from assets import map_models
from assets.structure_kit import Builder

ASSET_ID = 'reefbreak-ship-v5'
BEACON_ID = 'reefbreak-beacon-v1'
OUTPOST_ID = 'reefbreak-outpost-v1'
MODEL = Path(__file__).resolve().parents[2]/'assets/models/structures/reefbreak-ship/model.glb'
MODEL_NAME = 'reefbreak-ship'            # its root in the map's models.glb

W = .8
T = .2                                   # room lining thickness
HOVER = 21.0                             # the top level's floor above the sea
SAND = -HOVER-.25                        # the sandbar under the ship, just awash
HULL_TURN = -math.pi/2                   # hull-local -> base-local: port (-X) faces the field (-Z)
# The model's extent, hull-local (the fins' tips, the prow's point, the
# nozzles, the hover pods' feet), and its section: the midbody's half
# section scales down onto the prow's point (PROW_TIP, Y) toward the bow
# and about the deck's middle (0, DECK) toward the transom, as
# scripts/models/reefbreak_ship.py builds it.
HULL_X, BOW_TIP, STERN_Z, KEEL = 12.1, -42.0, 39.75, -12.6
PROW_TIP, PROW_END, STERN_START, TRANSOM, STERN_SCALE, DECK = 4.6, -23.8, 28.0, 34.6, 0.8, 7.0
HULL_SIDE, MID_SECTION_KEEL = 8.2, -2.2   # the flank's half width and the keel's height over the midbody
SANDBAR = (11.3, -40.1, 39.6)            # half width, bow and stern ends of the sand levelled under it

# Rooms. The ship has three levels in places: the top (the hold, the bay and
# the flag's platform, the aft room; floor Y=0), the middle (the station room
# hung under the hold, the balcony under the platform; MID_FLOOR) and the
# bottom (the generator, down in a keel under the bridge; GEN_FLOOR).
# The hold: the inner surface of its lining, a half section (x, y) from the
# floor corner up to the ceiling's middle, mirrored; fitted inside the
# model's skin with the lining's thickness to spare.
HOLD_Z = (-23.4, -7.6)
HOLD_SECTION = ((6.5, 0.0), (6.5, 4.6), (5.4, 5.6), (2.4, 5.8))
DOOR = (-1.6, 1.6, 3.2)                  # x0, x1, top: the door between the hold and the bay
HATCH = (-1.4, 1.4, -17.0, -14.0)        # x0, x1, z0, z1: the deck hatch into the hold
HATCH_TOP = 7.42                         # its shaft's top, just under the spine's top
HOLD_HATCH = (-1.4, 1.4, -12.0, -9.0)    # x0, x1, z0, z1: the drop from the hold to the station room

# The middle level. The station room's inner faces (half width, z, its
# ceiling the hold floor's underside) inside the model's module (half width,
# z, bottom); its door in the port flank facing the field (z0, z1) with a
# ledge outside, and the doors onto the balcony at both modules' ends.
MID_FLOOR = -6.0
STATION_X, STATION_Z, STATION_TOP = 5.6, (-21.0, -8.8), -0.4
STATION_MODULE = (6.4, -21.6, -8.2, -7.0)
PORT_DOOR = (-14.6, -11.2)
PORT_LEDGE = (6.4, 8.8)                  # |x| from, to, outside the port door
MID_DOOR = (-2.0, 2.0, MID_FLOOR+3.4)    # x0, x1, top
# The balcony under the entry platform, open to the field, between the two
# modules' faces (where the station room's doorway floor meets it): x0, x1,
# z0, z1, top; and the porthole over it up through the bay's floor (x, z,
# the half width of its octagon).
BALCONY = (-13.5, 5.0, -8.15, 8.75, MID_FLOOR)
PORTHOLE = (0.0, 0.4, 1.6)

# The bottom: the generator room's inner faces (half width, z, floor,
# ceiling) inside the keel (half width, z, bottom). Its floor has no door:
# players drop to it from its aft gallery at the top level, reached from the
# back (the aft room, where the stern's low-gravity column comes up, and the
# starboard side door with its ledge, away from the field). Its fore gallery,
# at the middle level, has no door: a ledge for its defenders.
GEN_X, GEN_Z, GEN_FLOOR, GEN_TOP = 5.0, (9.6, 24.2), -11.6, 5.8
KEEL_MODULE = (6.0, 8.8, 25.0, KEEL)
FORE_GALLERY, AFT_GALLERY = (9.6, 12.6), (21.2, 24.2)
SIDE_DOOR = (21.6, 23.8, 3.2)            # z0, z1, top: starboard, onto the aft gallery
SIDE_LEDGE = (8.3, 10.6, 21.0, 24.4)     # x0, x1, z0, z1, outside it (x0: past the hull's skin)
AFT_ROOM = (3.6, 25.2, 31.0, 4.0)        # half width, z0, z1, ceiling
AFT_DOOR = (-1.6, 1.6, 3.2)              # x0, x1, top: between the aft gallery and the aft room
# The stern's low-gravity column (a `gravity` zone, a tenth of gravity:
# jets climb for a tenth of the effort) from the platform base up through a
# hatch into the aft room; and the platform base, a deck hovering low under
# the stern with the vehicle pad.
LIFT = (1.5, 27.2, 30.2)                 # half width, z0, z1
LOW_GRAVITY, LIFT_TOP = 0.1, 1.2
PLATFORM_BASE = (0.0, 33.0, 8.0, -17.0)  # x, z of its middle, its octagon's half width, top

# The bay, lined where the model's bay walls were: the inner faces of its end
# walls, its width between the openings' jambs, its ceiling, the openings'
# clear span, the jambs and the ledges below them.
BAY_Z = (-6.9, 7.2)
BAY_X = 7.9
BAY_WALL_X = 7.45
BAY_ROOF = 5.0
BAY_OPEN = (-5.9, 6.7)                   # z of the openings' clear span
JAMBS = ((7.4, 8.5, -6.9, -5.9), (7.4, 8.5, 6.7, 7.2))
LEDGE = (8.0, 11.0, -9.6, 10.4, 0.0)     # |x| from, to, z0, z1 (centered on the openings), top (the bay's floor)
PLATFORM = 13.0                          # the port ledge's outer edge: the entry platform, 2 m deeper
# The flag: in the middle of the entry platform, in line with the opening's.
FLAG_HULL = (-(LEDGE[0]+PLATFORM)/2, LEDGE[4], (LEDGE[2]+LEDGE[3])/2)
FLAG_MARK = 1.1                          # the radius of the team mark painted round it
# The fins (starboard; mirrored), swept and canted out from the engine pods'
# tops; the pods (center x, y, radius, z0, z1 with the nozzles), the lower
# nozzles in the transom, the hover pods under the keel (x, z, radius, foot).
FINS = (((9.9, 7.67, 31.2), (11.88, 13.76, 36.6), (11.88, 13.76, 39.7), (9.9, 7.67, 37.6)),)
PODS = (9.9, 5.6, 2.35, 28.7, 39.75)
LOWER_NOZZLES = (3.2, 1.6, 1.5, 33.9, 37.35)
HOVER_PODS = ((3.4, -27.5, -0.8), (4.2, 31.6, -1.2))   # x, z, the keel's height there
HOVER_R, HOVER_DROP = 2.05, 2.1

INVENTORIES = ((-3.5, MID_FLOOR, -17.6), (3.5, MID_FLOOR, -17.6), (3.5, MID_FLOOR, -11.6))   # in the station room
GEN = (-1.5, GEN_FLOOR, 17.0)            # on the generator room's floor
TURRET = (0.0, 10.75, 16.5)              # on the bridge roof
SENSOR = (0.0, 10.95, 23.2)              # on its mast step
SPAWNS = [(x, z) for z in (-21.2, -18.4) for x in (-4.5, -1.5, 1.5, 4.5)]   # in the hold, facing the bay
SPAWN_YAW = math.pi
SPAWN_LIFT = 1.2

# Base-local, outside the hull: the gangway from the reef crest to the port
# ledge, and the reef beacon's foot beside it.
GANGWAY_Y = -13.0                        # its foot on the reef crest, below the top level's floor
GANGWAY = (1.5, 5.5, -PLATFORM, -58.0)   # x0, x1, z at the platform (floor), z at the foot
GANGWAY_PAD_R = 6.0
GANGWAY_BENTS = (-25.0, -41.0)           # z of its two pairs of columns, under its edges
BEACON_AT = (44.0, -66.0)                # local (x, z) of the reef beacon's foot
# Beacon, in its own frame (ground Y=0 at its foot).
BEACON_R0, BEACON_R1, BEACON_TOP = 3.2, 2.2, 16.0
BEACON_LEDGE, BEACON_LEDGE_Y = 5.5, 11.0

HULL_MAT, DECKING, GRATE, METAL, GLOW, WOOD = 'concrete', 'panel', 'grate', 'trim', 'light', 'bark'


def to_base(p):
    """A hull-local point in base-local coordinates."""
    x, y, z = p
    c, s = math.cos(HULL_TURN), math.sin(HULL_TURN)
    return (x*c+z*s, y, -x*s+z*c)


def to_hull(p):
    """Base-local points (n, 3) in hull-local coordinates."""
    p = np.asarray(p, np.float64)
    return np.stack([p[..., 2], p[..., 1], -p[..., 0]], -1)


FLAG = to_base(FLAG_HULL)


def ramp_y(ramp, z, y_a, y_b):
    _, _, za, zb = ramp
    t = min(max((z-za)/(zb-za), 0.0), 1.0)
    return y_a+(y_b-y_a)*t


def _mirror(half):
    return list(half)+[(-x, y) for x, y in reversed(half)]


def _width(section, y):
    """A section's half width at height y (its inner surface)."""
    pts = list(section)
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if y0 <= y <= y1 and y1 > y0:
            return x0+(x1-x0)*(y-y0)/(y1-y0)
    return pts[-1][0]


def room_boxes(section, z0, z1, steps=3):
    """Boxes (hull-local) inside a room's lining, stepped under its sloped
    ceiling panels so they never reach the model's skin: what the model is
    cut away from."""
    half = list(section)
    a, b = z0-T, z1+T
    boxes = []
    for (xa, ya), (xb, yb) in zip(half, half[1:]):
        if xa == xb:
            boxes.append(((-xa-T, -.05 if ya == 0 else ya, a), (xa+T, yb, b)))
            continue
        for k in range(steps):
            y0, y1 = ya+(yb-ya)*k/steps, ya+(yb-ya)*(k+1)/steps
            x = _width(half, y1)+T
            boxes.append(((-x, y0, a), (x, y1, b)))
    xt, yt = half[-1]
    boxes.append(((-xt-T, yt, a), (xt+T, yt+T, b)))
    return boxes


def bay_box():
    """What the bay's lining replaces of the model (hull-local box), down to
    its floor's underside (the porthole's shaft below stays the model's)."""
    return ((-BAY_WALL_X, -.5, BAY_Z[0]-T-.05), (BAY_WALL_X, BAY_ROOF+.3, BAY_Z[1]+T+.05))


def openings():
    """The holes through the model and its stand-in (hull-local boxes): the
    rooms' doors and hatches, the side door and the lift's hatch."""
    d0, d1, top = DOOR
    h0, h1, hz0, hz1 = HATCH
    s0, s1, stop = SIDE_DOOR
    m0, m1, mtop = MID_DOOR
    a0, a1, atop = AFT_DOOR
    lw, lz0, lz1 = LIFT
    return [((d0, -.05, HOLD_Z[1]), (d1, top, BAY_Z[0]+1.5)),
            ((h0, HOLD_SECTION[-1][1], hz0), (h1, 9.6, hz1)),
            ((-STATION_MODULE[0]-.6, MID_FLOOR-.05, PORT_DOOR[0]), (-STATION_X, mtop, PORT_DOOR[1])),
            ((m0, MID_FLOOR-.05, STATION_Z[1]), (m1, mtop, STATION_MODULE[2]+.3)),
            ((a0, -.05, GEN_Z[1]), (a1, atop, AFT_ROOM[1])),
            ((GEN_X, -.05, s0), (9.2, stop, s1)),
            ((-lw, -3.5, lz0), (lw, .05, lz1))]


def model_cuts():
    """Everything cut from the model (base-local boxes, its frame): the rooms
    and their openings."""
    boxes = interiors()+openings()
    out = []
    for lo, hi in boxes:
        a, b = to_base(lo), to_base(hi)
        out.append((tuple(min(a[i], b[i]) for i in range(3)), tuple(max(a[i], b[i]) for i in range(3))))
    return out


@functools.lru_cache(maxsize=None)
def _source():
    return map_models.read(MODEL)


def map_model():
    """The ship as the map's models.glb carries it: the authored model with
    the rooms and openings cut away."""
    return map_models.clip(_source(), model_cuts())


def interiors():
    """The rooms' volumes with their linings (hull-local boxes)."""
    sx, (sz0, sz1), stop = STATION_X, STATION_Z, STATION_TOP
    gz0, gz1 = GEN_Z
    ax, az0, az1, atop = AFT_ROOM
    return room_boxes(HOLD_SECTION, *HOLD_Z)+[bay_box(),
        ((-sx-T, MID_FLOOR-.45, sz0-T), (sx+T, stop, sz1+T)),
        ((-GEN_X-T, GEN_FLOOR-.45, gz0-T), (GEN_X+T, GEN_TOP+T+.05, gz1+T)),
        ((-ax-T, -.45, az0-T), (ax+T, atop+T+.05, az1+T))]


def stern_scale(z):
    """How far the hull's section is scaled down at z, toward the transom."""
    return 1.0 if z <= STERN_START else 1.0+(STERN_SCALE-1.0)*(z-STERN_START)/(TRANSOM-STERN_START)


def _keep(x, y, z):
    """Crossings the hull's sections use (the hull and its two modules):
    not the ledges or the gangway, nor what stands outboard of the stern
    (the engine pods, pylons and fins), nor the hover pods, the balcony, the
    lift's rings and the platform base (each has its own collision or none),
    nor anything inside the rooms (cut away; through a recess in the skin a
    direction could otherwise stop on the model's inner parts)."""
    ledge = (z > LEDGE[2]-1) & (z < LEDGE[3]+1) & (np.abs(x) > 8.4) & (y < 1.5)
    beyond = np.abs(x) > PLATFORM+.5             # the gangway and its pad, off the port side
    side = HULL_SIDE*np.vectorize(stern_scale)(z)
    outboard = (z > STERN_START-1.0) & (np.abs(x) > side+.3)
    pods = np.zeros(np.shape(x), bool)
    for px, pz, top in HOVER_PODS:
        pods |= (np.abs(np.abs(x)-px) < HOVER_R+.5) & (np.abs(z-pz) < HOVER_R+.5) & (y < top-.05)
    balcony = (z > STATION_MODULE[2]) & (z < KEEL_MODULE[1]) & (y < -2.3)
    under_stern = (z > KEEL_MODULE[2]+.05) & (y < -2.3)
    inside = np.zeros(np.shape(x), bool)
    for lo, hi in interiors():
        inside |= (x > lo[0]) & (x < hi[0]) & (y > lo[1]) & (y < hi[1]) & (z > lo[2]) & (z < hi[2])
    return ~(ledge | beyond | outboard | pods | balcony | under_stern | inside)


# Where the hull is sampled. Stations: every 3 m along the prow, where its
# facets narrow, then sparser along the midbody, whose section only changes
# at the modules, the bridge and the stern, with a pair just either side of
# each module's ends (and where the keel's aft end starts to chamfer) so
# the stand-in drops straight down their faces;
# directions close together over the walkable top and at each corner of the
# section and the modules (the same angles at every station, since the
# sections are one shape scaled about hull_centre's point), within the
# per-base collision budget. Over the bay only the roof, from inside the
# bay. The hull's ends at the bay sit inside the bay's end walls, the roof's
# underside inside its ceiling, so the stand-in never shares their faces.
_ends = lambda a, b: (round(a-.15, 3), round(a+.15, 3), round(b-.15, 3), round(b+.15, 3))
_sm, _km = _ends(*STATION_MODULE[1:3]), _ends(*KEEL_MODULE[1:3])
FORE_STATIONS = tuple(np.round(np.concatenate([np.arange(-41.0, PROW_END-1.5, 3.0), [PROW_END, _sm[0], _sm[1], -15.0,
                                               _sm[2], _sm[3], BAY_Z[0]-T/2]]), 3))
AFT_STATIONS = (round(BAY_Z[1]+T/2, 3), _km[0], _km[1], 10.5, 12.3, 13.1, 18.5, GEN_Z[1], _km[2], _km[3], 26.7, 28.0,
                28.7, 31.5, 34.3)
BAY_STATIONS = tuple(np.round(np.append(np.arange(BAY_Z[0]-T/2, BAY_Z[1], 3.5), BAY_Z[1]+T/2), 3))
# The section's corners (deck edge to keel edge), and the modules': the
# station room's side and chamfered bottom forward, the keel's aft.
_CORNERS = (55.1, 62.5, 72.4, 104.4, 113.8, 130.4)
_mirror_angles = lambda half: np.radians(np.concatenate([half, [180.0], 360.0-np.array(half[:0:-1])]))
HULL_ANGLES = tuple(_mirror_angles(sorted([0, 10, 20, 26, 32, *_CORNERS, 121.9, 144.0, 148.3])))
# Aft, where the bridge stands: its roof's edge, its tiers' corners.
AFT_ANGLES = tuple(_mirror_angles(sorted([0, 16, 28, 35.4, 40.6, *_CORNERS, 125.2, 157.1])))
ROOF_ANGLES = tuple(np.radians(np.concatenate([np.arange(0, 76, 8), np.arange(90, 271, 45), np.arange(284, 360, 8)])))


def hull_centre(z):
    """The height the hull's sections are sampled from at station z: the
    image of (0, 2.4) on the midbody's section, scaled with it, so each
    section is star-shaped about (0, hull_centre(z)) and its corners lie on
    the same directions."""
    if z < PROW_END:
        u = max((z-BOW_TIP)/(PROW_END-BOW_TIP), 0.0)
        return PROW_TIP+(2.4-PROW_TIP)*u
    return DECK-(DECK-2.4)*stern_scale(z)


@functools.lru_cache(maxsize=None)
def hull_sections():
    """The hull's outer cross-sections (hull-local, sampled from the model):
    (fore, aft, bay roof) rings of (x, y) points at FORE_STATIONS,
    AFT_STATIONS and BAY_STATIONS."""
    m = _source()
    tris = to_hull(m.pos)[m.tri]
    fore = map_models.sections(tris, FORE_STATIONS, hull_centre, HULL_ANGLES, _keep)
    aft = map_models.sections(tris, AFT_STATIONS, hull_centre, AFT_ANGLES, _keep)
    # Over the bay, only its roof, seen from the bay's floor within the
    # openings' width; what lies below the underside is flattened onto it.
    roof_keep = lambda x, y, z: _keep(x, y, z) & (np.abs(x) <= BAY_X) & (y > BAY_ROOF-.1)
    roof = map_models.sections(tris, BAY_STATIONS, lambda z: 1.0, ROOF_ANGLES, roof_keep)
    roof[:, :, 1] = np.maximum(roof[:, :, 1], BAY_ROOF+T/2)
    roof[:, :, 0] = np.clip(roof[:, :, 0], -BAY_X+.3, BAY_X-.3)
    return fore, aft, roof


def _face(mesh, pts, normal, mat, solid=True):
    """A triangle or quad (hull-local points) wound to face `normal`."""
    a, b, c = (np.array(p, np.float64) for p in pts[:3])
    if np.dot(np.cross(b-a, c-a), normal) < 0: pts = pts[::-1]
    if len(pts) == 3: mesh.triangle(list(pts), mat, solid)
    else: mesh.quad(*pts, mat, solid)


def _tube(mesh, rings, stations, mat, caps, holes):
    """Quads between consecutive rings (hull-local), outward facing, with
    caps (fans) at the ends listed in `caps` ('first', 'last', or a point
    to close to), clipped by `holes` (boxes)."""
    tris = []
    n = rings.shape[1]
    def outward(tri, axis):
        a, b, c = (np.array(p) for p in tri)
        m = (a+b+c)/3
        return tri if np.dot(np.cross(b-a, c-a), m-axis(m)) >= 0 else tri[::-1]
    radial = lambda i: (lambda m: np.array([0.0, float(np.mean(rings[i, :, 1])), m[2]]))
    for i in range(len(stations)-1):
        za, zb = stations[i], stations[i+1]
        for k in range(n):
            j = (k+1) % n
            a = (*rings[i, k], za); b = (*rings[i, j], za)
            c = (*rings[i+1, j], zb); d = (*rings[i+1, k], zb)
            tris += [outward((a, d, c), radial(i)), outward((a, c, b), radial(i))]
    for end, how in caps:
        i = 0 if end == 'first' else len(stations)-1
        z = stations[i]
        ring = [(*p, z) for p in rings[i]]
        flat = how == 'flat'
        # Fanned from where the section was sampled from: it is star-shaped
        # about that point, so the fan never folds over itself.
        centre = (0.0, hull_centre(z), z) if flat else how
        away = np.array([0.0, 0.0, -1.0 if end == 'first' else 1.0])
        for k in range(n):
            tri = (centre, ring[k], ring[(k+1) % n])
            tris.append(outward(tri, (lambda m: m-away)) if flat else outward(tri, radial(i)))
    _emit(mesh, tris, mat, holes)


def _emit(mesh, tris, mat, holes, solid=True):
    """Triangles (hull-local) through `mesh`, minus whatever lies inside
    `holes` (boxes, split at their faces)."""
    if not tris: return
    pos = np.array(tris, np.float64).reshape(-1, 3)
    if holes:
        m = map_models.Mesh(pos, np.zeros_like(pos), np.zeros((len(pos), 2)), np.arange(len(pos)).reshape(-1, 3), '', [])
        pos = map_models.clip(m, holes).pos
    t = pos.reshape(-1, 3, 3)
    keep = np.linalg.norm(np.cross(t[:, 1]-t[:, 0], t[:, 2]-t[:, 0]), axis=1) > 1e-6   # no collapsed samples
    for tri in t[keep]:
        mesh.triangle([tuple(float(v) for v in p) for p in tri], mat, solid)


def _prism_z(mesh, cx, cy, r, z0, z1, mat, sides=8):
    """An octagonal prism along Z (hull-local), round (cx, cy), r to its
    faces' middles, closed at both ends."""
    rr = r/math.cos(math.pi/sides)
    ring = [(cx+rr*math.cos(math.pi/sides*(2*k+1)), cy+rr*math.sin(math.pi/sides*(2*k+1))) for k in range(sides)]
    for k in range(sides):
        (xa, ya), (xb, yb) = ring[k], ring[(k+1) % sides]
        mid = np.array([(xa+xb)/2-cx, (ya+yb)/2-cy, 0.0])
        _face(mesh, ((xa, ya, z0), (xb, yb, z0), (xb, yb, z1), (xa, ya, z1)), mid, mat)
    for z, n in ((z0, -1.0), (z1, 1.0)):
        pts = [(x, y, z) for x, y in ring]
        for k in range(1, sides-1):
            _face(mesh, (pts[0], pts[k], pts[k+1]), (0.0, 0.0, n), mat)


def _quad(mesh, a, b, c, d, mat, solid=True, both=False):
    mesh.quad(a, b, c, d, mat, solid)
    if both: mesh.quad(d, c, b, a, mat, False)


def _strip(mesh, p, q, z0, z1, mat, holes=(), solid=True):
    """One flat panel of a room's lining: the inner face along p->q (x, y)
    from z0 to z1, T thick outward, its edges closed, minus `holes` given
    as (s0, s1, z0, z1) with s from 0 at p to 1 at q."""
    (px, py), (qx, qy) = p, q
    ex, ey = qx-px, qy-py
    n = math.hypot(ex, ey)
    nx, ny = ey/n, -ex/n                      # outward: away from the room's middle
    if nx*(px+qx)/2+ny*((py+qy)/2-2.5) < 0: nx, ny = -nx, -ny
    ss = sorted({0.0, 1.0, *(v for h in holes for v in h[:2] if 0 < v < 1)})
    zs = sorted({z0, z1, *(v for h in holes for v in h[2:] if z0 < v < z1)})
    def inside(s, z): return any(h[0] <= s <= h[1] and h[2] <= z <= h[3] for h in holes)
    def at(s, z, off): return (px+ex*s+nx*off, py+ey*s+ny*off, z)
    cells = {(i, k) for i in range(len(ss)-1) for k in range(len(zs)-1)
             if not inside((ss[i]+ss[i+1])/2, (zs[k]+zs[k+1])/2)}
    out = np.array([nx, ny, 0.0]); along = np.array([ex, ey, 0.0])/n
    for i, k in cells:
        s0, s1, za, zb = ss[i], ss[i+1], zs[k], zs[k+1]
        # Inner face toward the room, outer face away from it.
        _face(mesh, (at(s0, za, 0), at(s0, zb, 0), at(s1, zb, 0), at(s1, za, 0)), -out, mat, solid)
        _face(mesh, (at(s0, za, T), at(s1, za, T), at(s1, zb, T), at(s0, zb, T)), out, mat, solid)
        for edge, normal, quad in (((i-1, k), -along, (at(s0, za, 0), at(s0, za, T), at(s0, zb, T), at(s0, zb, 0))),
                                   ((i+1, k), along, (at(s1, za, 0), at(s1, zb, 0), at(s1, zb, T), at(s1, za, T))),
                                   ((i, k-1), (0, 0, -1), (at(s0, za, 0), at(s1, za, 0), at(s1, za, T), at(s0, za, T))),
                                   ((i, k+1), (0, 0, 1), (at(s0, zb, 0), at(s0, zb, T), at(s1, zb, T), at(s1, zb, 0)))):
            if edge not in cells: _face(mesh, quad, np.array(normal, np.float64), mat, solid)


def _end_wall(mesh, section, z, into, mat, door=None):
    """A room's end wall at z (its inner face), T thick away from the room
    (`into` is the room's side, +1 or -1), the section's shape, with an
    optional door (x0, x1, top) at the floor."""
    half = list(section)
    ys = sorted({y for _, y in half} | ({door[2]} if door else set()))
    zi, zo = z, z-into*T
    faces = []
    for ya, yb in zip(ys, ys[1:]):
        wa, wb = _width(half, ya), _width(half, yb)
        if door and yb <= door[2]:
            pieces = [((-wa, -wb), (door[0], door[0])), ((door[1], door[1]), (wa, wb))]
        else:
            pieces = [((-wa, -wb), (wa, wb))]
        for (la, lb), (ra, rb) in pieces:
            faces.append(((la, ya), (ra, ya), (rb, yb), (lb, yb)))
    for f in faces:
        if abs(f[1][0]-f[0][0]) < 1e-6 and abs(f[2][0]-f[3][0]) < 1e-6: continue
        _face(mesh, [(x, y, zi) for x, y in f], (0, 0, into), mat)
        _face(mesh, [(x, y, zo) for x, y in f], (0, 0, -into), mat)
    if door:
        x0, x1, top = door
        for x, nx in ((x0, 1), (x1, -1)):
            _face(mesh, ((x, 0.0, zi), (x, top, zi), (x, top, zo), (x, 0.0, zo)), (nx, 0, 0), mat)
        _face(mesh, ((x0, top, zi), (x1, top, zi), (x1, top, zo), (x0, top, zo)), (0, -1, 0), mat)


def _slab_around(b, x0, x1, z0, z1, top, mat, thick, hole=None):
    """A slab with an optional rectangular hole (x0, x1, z0, z1) through it."""
    if hole is None: return b.slab(x0, x1, z0, z1, top, mat, thick)
    hx0, hx1, hz0, hz1 = hole
    b.slab(x0, x1, z0, hz0, top, mat, thick); b.slab(x0, x1, hz1, z1, top, mat, thick)
    b.slab(x0, hx0, hz0, hz1, top, mat, thick); b.slab(hx1, x1, hz0, hz1, top, mat, thick)


def _octagon_hole(b, mesh, x0, x1, z0, z1, top, mat, thick, hole):
    """A slab with an octagonal hole (x, z, the half width across its flats)
    through it: a square hole, its corners filled with wedges."""
    cx, cz, r = hole
    _slab_around(b, x0, x1, z0, z1, top, mat, thick, (cx-r, cx+r, cz-r, cz+r))
    k = r*math.tan(math.pi/8)
    for sx in (-1, 1):
        for sz in (-1, 1):
            tri = [(cx+sx*r, cz+sz*k), (cx+sx*r, cz+sz*r), (cx+sx*k, cz+sz*r)]
            for y, n in ((top, 1.0), (top-thick, -1.0)):
                _face(mesh, [(x, y, z) for x, z in tri], (0.0, n, 0.0), mat)
            (ax, az), (bx, bz) = tri[0], tri[2]
            _face(mesh, [(ax, top-thick, az), (bx, top-thick, bz), (bx, top, bz), (ax, top, az)], (-sx, 0.0, -sz), mat)


def _octagon_shaft(mesh, cx, cz, r, y0, y1, mat):
    """The inner faces of an octagonal shaft (flats r from its middle)."""
    rr = r/math.cos(math.pi/8)
    ring = [(cx+rr*math.cos(math.pi/8*(2*k+1)), cz+rr*math.sin(math.pi/8*(2*k+1))) for k in range(8)]
    for k in range(8):
        (xa, za), (xb, zb) = ring[k], ring[(k+1) % 8]
        _face(mesh, [(xa, y0, za), (xb, y0, zb), (xb, y1, zb), (xa, y1, za)], (cx-(xa+xb)/2, 0.0, cz-(za+zb)/2), mat)


def _wall_around(b, axis, p0, p1, u0, u1, y0, y1, mat, doors=()):
    """A flat wall from plane p0 to p1 (x for axis 'x', z for 'z'), from u0
    to u1 along it and y0 to y1, with door holes (ua, ub, ya, yb) cut in it."""
    put = (lambda a, c, ya, yb: b.wall(p0, p1, a, c, ya, yb, mat)) if axis == 'x' else \
          (lambda a, c, ya, yb: b.wall(a, c, p0, p1, ya, yb, mat))
    for a, c in _runs(u0, u1, [d[:2] for d in doors]): put(a, c, y0, y1)
    for ua, ub, ya, yb in doors:
        if ya > y0: put(ua, ub, y0, ya)
        if yb < y1: put(ua, ub, yb, y1)


def _box_room(b, hx, z0, z1, y0, y1, doors=None, dress=True, step=5, end_pilasters=True):
    """A box room's lining (inner faces at |x| = hx, z0 and z1, floor y0 and
    ceiling y1), T thick outward, with door holes per wall: doors maps
    '-x', '+x', '-z', '+z' to (ua, ub, ya, yb) lists. No floor or ceiling.
    Without end pilasters the end walls (z0, z1) are dressed plain, for
    galleries that meet them."""
    doors = doors or {}
    _wall_around(b, 'x', -hx-T, -hx, z0-T, z1+T, y0, y1, METAL, doors.get('-x', ()))
    _wall_around(b, 'x', hx, hx+T, z0-T, z1+T, y0, y1, METAL, doors.get('+x', ()))
    _wall_around(b, 'z', z0-T, z0, -hx, hx, y0, y1, METAL, doors.get('-z', ()))
    _wall_around(b, 'z', z1, z1+T, -hx, hx, y0, y1, METAL, doors.get('+z', ()))
    if not dress: return
    gaps = lambda key: [d[:2] for d in doors.get(key, ())]
    b.dress('x', -hx, 1, _runs(z0, z1, gaps('-x')), y0, y1, step=step)
    b.dress('x', hx, -1, _runs(z0, z1, gaps('+x')), y0, y1, step=step)
    b.dress('z', z0, 1, _runs(-hx, hx, gaps('-z')), y0, y1, step=step, pilasters=end_pilasters)
    b.dress('z', z1, -1, _runs(-hx, hx, gaps('+z')), y0, y1, step=step, pilasters=end_pilasters)


def _passage(b, axis, a, c, u0, u1, y0, top, floor=DECKING):
    """A doorway's passage through a wall's or the hull's thickness: from a
    to c across it (x for axis 'x', z for 'z'), u0 to u1 along it, its
    reveals, lintel and floor."""
    w = (lambda p0, p1, v0, v1, ya, yb: b.wall(p0, p1, v0, v1, ya, yb, METAL)) if axis == 'x' else \
        (lambda p0, p1, v0, v1, ya, yb: b.wall(v0, v1, p0, p1, ya, yb, METAL))
    w(a, c, u0-T, u0, y0, top); w(a, c, u1, u1+T, y0, top); w(a, c, u0-T, u1+T, top, top+T)
    if axis == 'x': b.slab(a, c, u0, u1, y0, floor, .4)
    else: b.slab(u0, u1, a, c, y0, floor, .4)


def _room(b, mesh, section, z0, z1, doors=(), holes=(), floor_mat=DECKING, wall_mat=METAL, floor_hole=None):
    """A lined room: floor (with an optional hole: x0, x1, z0, z1), the
    section's panels from z0 to z1 and its end walls, with doors ((end, (x0,
    x1, top))) in the end walls and holes ((panel index, s0, s1, za, zb)) in
    the panels."""
    full = _mirror(section)
    _slab_around(b, -section[0][0]-T, section[0][0]+T, z0-T, z1+T, 0.0, floor_mat, .4, floor_hole)
    for i, (p, q) in enumerate(zip(full, full[1:])):
        _strip(mesh, p, q, z0-T, z1+T, wall_mat, [h[1:] for h in holes if h[0] == i])
    for end, into in ((z0, 1), (z1, -1)):
        door = next((d for e, d in doors if e == end), None)
        _end_wall(mesh, section, end, into, wall_mat, door)


def sites():
    """Terrain to blend toward, base-local: (shape, surface(x, z) -> local
    height, falloff). The sand is levelled round the hull; the gangway's foot
    is levelled on the reef crest."""
    sand = lambda x, z: SAND
    pad = lambda x, z: GANGWAY_Y-.1
    sx, sz0, sz1 = SANDBAR
    corners = [to_base((x, 0, z)) for x in (-sx-8, sx+8) for z in (sz0-4, sz1+6)]
    xs, zs = [c[0] for c in corners], [c[2] for c in corners]
    gx = (GANGWAY[0]+GANGWAY[1])/2
    return [(('rect', min(xs), max(xs), min(zs), max(zs)), sand, 22),
            (('disc', gx, GANGWAY[3]-GANGWAY_PAD_R+2.0, GANGWAY_PAD_R+2), pad, 16)]


def _runs(lo, hi, gaps):
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
    if not hasattr(mesh, 'standin'): mesh.standin = []
    base_yaw = mesh.yaw
    mesh.yaw = base_yaw+HULL_TURN            # the hull, in its own frame
    d0, d1, dtop = DOOR
    holes = openings()

    # --- Outside: collision fitted to the model, standing in for it ---------
    start = len(mesh.vertices)
    fore, aft, roof = hull_sections()
    _tube(mesh, fore, FORE_STATIONS, METAL, [('first', (0.0, PROW_TIP, BOW_TIP)), ('last', 'flat')], holes)
    _tube(mesh, aft, AFT_STATIONS, METAL, [('first', 'flat'), ('last', 'flat')], holes)
    # The bay's roof (closed underneath), its thresholds and jambs, the ledges.
    _tube(mesh, roof, BAY_STATIONS, METAL, [], [])
    z0, z1 = BAY_Z
    for s in (-1, 1): slab(s*BAY_WALL_X, s*BAY_X, z0, z1, 0.0, DECKING, .6)
    for x0, x1, a, c in JAMBS:
        for s in (-1, 1): wall(s*x0, s*x1, a, c, 0.0, BAY_ROOF+.8, METAL)
    lx0, lx1, lz0, lz1, ltop = LEDGE
    for s, edge in ((-1, PLATFORM), (1, lx1)): slab(s*lx0, s*edge, lz0, lz1, ltop, GRATE, .5)
    for fin in FINS:
        for s in (-1, 1):
            p = [(s*x, y, z) for x, y, z in fin]
            q = [(x+s*.25, y, z) for x, y, z in p]; p = [(x-s*.25, y, z) for x, y, z in p]
            mid = np.mean(np.array(p+q), 0)
            for i in range(4):
                j = (i+1) % 4
                quad = (p[i], q[i], q[j], p[j])
                _face(mesh, quad, np.mean(np.array(quad), 0)-mid, METAL)
            _face(mesh, p, (-s, 0, 0), METAL); _face(mesh, q, (s, 0, 0), METAL)
    # The engine pods with their nozzles, the lower nozzles, the hover pods.
    for s in (-1, 1):
        cx, cy, r, a, c = PODS
        _prism_z(mesh, s*cx, cy, r, a, c, METAL)
        cx, cy, r, a, c = LOWER_NOZZLES
        _prism_z(mesh, s*cx, cy, r, a, c, METAL)
        for x, z, top in HOVER_PODS:
            b.prism(s*x, z, HOVER_R, HOVER_R, top-HOVER_DROP, top+.1, 8, METAL, phase=math.pi/8)
    # The bay's underside (the hull's bottom under it, down to the keel), with
    # the porthole's shaft through it.
    ox, oz, orr = PORTHOLE
    hole = (ox-orr, ox+orr, oz-orr, oz+orr)
    _slab_around(b, -5.4, 5.4, BAY_Z[0]-T, BAY_Z[1]+T, -.5, METAL, -.5-MID_SECTION_KEEL, hole)
    for sgn in (-1, 1): slab(sgn*5.4, sgn*BAY_WALL_X, BAY_Z[0]-T, BAY_Z[1]+T, -.5, METAL, .7)
    _octagon_shaft(mesh, ox, oz, orr-.02, MID_SECTION_KEEL, -.5, METAL)   # just inside the square hole's walls
    # The balcony under the platform and the platform base under the stern.
    bx0, bx1, bz0, bz1, btop = BALCONY
    slab(bx0, bx1, bz0, bz1, btop, GRATE, .4)
    pcx, pcz, pr, ptop = PLATFORM_BASE
    b.prism(pcx, pcz, pr/math.cos(math.pi/8), pr/math.cos(math.pi/8), ptop-1.0, ptop, 8, GRATE, phase=math.pi/8)
    mesh.standin.append((start, len(mesh.vertices)))

    # --- The hold: spawns, the door to the bay, the deck hatch, the hatch in
    # its floor down to the station room ---------------------------------------
    hz0, hz1 = HOLD_Z
    top = len(_mirror(HOLD_SECTION))//2-1         # the flat ceiling panel's index
    w = HOLD_SECTION[-1][0]
    hatch = (top, (w+HATCH[0])/(2*w), (w+HATCH[1])/(2*w), HATCH[2], HATCH[3])
    _room(b, mesh, HOLD_SECTION, hz0, hz1, doors=[(hz1, DOOR)], holes=[hatch], floor_hole=HOLD_HATCH)
    fx0, fx1, fz0, fz1 = HOLD_HATCH
    for z in (fz0-.06, fz1+.06): mesh.box(((fx0+fx1)/2, .02, z), (fx1-fx0, .04, .08), GLOW, False)
    hx = HOLD_SECTION[0][0]
    for x, into in ((-hx, 1), (hx, -1)):
        b.dress('x', x, into, [(hz0, hz1)], 0.0, HOLD_SECTION[1][1], step=5)
    for x in (-1.9, 1.9): b.ceiling_strip('z', x, hz0+.8, hz1-.8, HOLD_SECTION[-1][1], .9)
    # The hatch's shaft up to the deck, its rim lit.
    yc = HOLD_SECTION[-1][1]+T
    h0, h1, s0, s1 = HATCH
    wall(h0-T, h0, s0-T, s1+T, yc, HATCH_TOP, METAL); wall(h1, h1+T, s0-T, s1+T, yc, HATCH_TOP, METAL)
    wall(h0, h1, s0-T, s0, yc, HATCH_TOP, METAL); wall(h0, h1, s1, s1+T, yc, HATCH_TOP, METAL)
    mesh.box(((h0+h1)/2, yc-.05, s0-.06), (h1-h0, .06, .08), GLOW, False)
    mesh.box(((h0+h1)/2, yc-.05, s1+.06), (h1-h0, .06, .08), GLOW, False)
    # The doorway through to the bay: its reveals and lintel.
    for x0, x1 in ((d0-T, d0), (d1, d1+T)): wall(x0, x1, hz1+T, z0-T, 0.0, dtop, METAL)
    wall(d0-T, d1+T, hz1+T, z0-T, dtop, dtop+T, METAL)

    # --- The station room (middle level), under the hold: the stations ----
    sx, (sz0, sz1), stop = STATION_X, STATION_Z, STATION_TOP
    m0, m1, mtop = MID_DOOR
    pz0, pz1 = PORT_DOOR
    y0 = MID_FLOOR
    slab(-sx-T, sx+T, sz0-T, sz1+T, y0, DECKING, .4)
    _box_room(b, sx, sz0, sz1, y0, stop, {'-x': [(pz0, pz1, y0, mtop)], '+z': [(m0, m1, y0, mtop)]})
    for x in (-2.4, 2.4): b.ceiling_strip('z', x, sz0+.8, sz1-.8, stop, .9)
    for p in INVENTORIES: mesh.equipment('inventory', p, team, circuit)
    # The port door, facing the field, with a ledge outside; the door aft
    # onto the balcony.
    mx, _, mz1, _ = STATION_MODULE
    _passage(b, 'x', -mx-.05, -sx-T, pz0, pz1, y0, mtop)
    slab(-PORT_LEDGE[1], -mx-.05, pz0-.6, pz1+.6, y0, GRATE, .4)
    mesh.box((-PORT_LEDGE[1], y0+.08, (pz0+pz1)/2), (.08, .06, pz1-pz0+1.2), GLOW, False)
    _passage(b, 'z', sz1+T, mz1+.05, m0, m1, y0, mtop, GRATE)

    # --- The generator room (bottom), in the keel: a floor with no door ------
    gx, (gz0, gz1) = GEN_X, GEN_Z
    a0, a1, atop = AFT_DOOR
    s0, s1, stop_ = SIDE_DOOR
    slab(-gx-T, gx+T, gz0-T, gz1+T, GEN_FLOOR, DECKING, .4)
    slab(-gx-T, gx+T, gz0-T, gz1+T, GEN_TOP+T, METAL, T)
    _box_room(b, gx, gz0, gz1, GEN_FLOOR, GEN_TOP, {'+z': [(a0, a1, 0.0, atop)], '+x': [(s0, s1, 0.0, stop_)]},
              step=4, end_pilasters=False)
    # Its galleries, where the floor ends, their edges lit: the aft one from
    # the back, the fore one a ledge for its defenders.
    slab(-gx, gx, gz0, FORE_GALLERY[1], y0, GRATE, .4)
    slab(-gx, gx, AFT_GALLERY[0], gz1, 0.0, GRATE, .4)
    mesh.box((0.0, y0-.2, FORE_GALLERY[1]+.04), (2*gx, .12, .08), GLOW, False)
    mesh.box((0.0, -.2, AFT_GALLERY[0]-.04), (2*gx, .12, .08), GLOW, False)
    for x in (-2.0, 2.0): b.ceiling_strip('z', x, gz0+.8, gz1-.8, GEN_TOP, 1.0)
    for z in (gz0+3.0, (gz0+gz1)/2, gz1-3.0): b.lamp((0.0, GEN_FLOOR+3.0, z), 1.0)
    mesh.equipment('generator', GEN, team, circuit)
    # The starboard side door onto the aft gallery, through the hull, and its
    # ledge, lit along the edge.
    lx0, lx1, lz0_, lz1_ = SIDE_LEDGE
    _passage(b, 'x', gx+T, lx0, s0, s1, 0.0, stop_, GRATE)
    slab(lx0, lx1, lz0_, lz1_, 0.0, GRATE, .4)
    mesh.box((lx1, .08, (lz0_+lz1_)/2), (.08, .06, lz1_-lz0_), GLOW, False)

    # --- The aft room, where the low-gravity column comes up its floor ----
    ax, az0, az1, artop = AFT_ROOM
    lw, lz0, lz1 = LIFT
    _slab_around(b, -ax-T, ax+T, az0-T, az1+T, 0.0, DECKING, .4, (-lw, lw, lz0, lz1))
    slab(-ax-T, ax+T, az0-T, az1+T, artop+T, METAL, T)
    _box_room(b, ax, az0, az1, 0.0, artop, {'-z': [(a0, a1, 0.0, atop)]}, step=4)
    _passage(b, 'z', gz1+T, az0-T, a0, a1, 0.0, atop)
    b.ceiling_strip('z', 0.0, az0+.6, az1-.6, artop, .8)
    # The column's shaft down through the hull, its rim lit.
    keel = DECK-(DECK-MID_SECTION_KEEL)*stern_scale(lz1)
    _wall_around(b, 'x', -lw-T, -lw, lz0-T, lz1+T, keel-.4, -.4, METAL)
    _wall_around(b, 'x', lw, lw+T, lz0-T, lz1+T, keel-.4, -.4, METAL)
    _wall_around(b, 'z', lz0-T, lz0, -lw, lw, keel-.4, -.4, METAL)
    _wall_around(b, 'z', lz1, lz1+T, -lw, lw, keel-.4, -.4, METAL)
    for z in (lz0-.06, lz1+.06): mesh.box((0.0, .02, z), (2*lw, .04, .08), GLOW, False)
    # The low-gravity column from the platform base's deck up to just over
    # the aft room's floor (world-aligned, as the ship lies).
    if not hasattr(mesh, 'zones'): mesh.zones = []
    corners = np.array([mesh.point((x, y, z)) for x in (-lw, lw) for y in (PLATFORM_BASE[3], LIFT_TOP) for z in (lz0, lz1)])
    lo, hi = corners.min(0), corners.max(0)
    mesh.zones.append(dict(kind='gravity', at=[round(float(v), 3) for v in (lo+hi)/2],
                           size=[round(float(v), 3) for v in (hi-lo)/2], gravity=LOW_GRAVITY))
    b.lamp((0.0, PLATFORM_BASE[3]+2.0, (lz0+lz1)/2), 1.2)

    # --- The bay: lined end walls with the rooms' doors, ceiling and floor;
    # the flag's mark on the port platform ------------------------------------
    bay = Builder(mesh, accent, interior=DECKING, metal=METAL, glow=GLOW)
    bx = BAY_WALL_X
    _octagon_hole(b, mesh, -bx, bx, z0-T, z1+T, 0.0, DECKING, .5, PORTHOLE)
    slab(-bx, bx, z0-T, z1+T, BAY_ROOF+T, METAL, T)
    # The porthole down to the balcony, its rim lit.
    ox, oz, orr = PORTHOLE
    b.prism(ox, oz, orr/math.cos(math.pi/8)+.02, orr/math.cos(math.pi/8)+.02, 0.0, .03, 8, GLOW,
            phase=math.pi/8, solid=False, cap=False)
    # Its fore wall has the door to the hold; its aft wall is solid (the
    # generator room behind it is reached only from below and behind).
    runs = _runs(-bx, bx, [(d0, d1)])
    for zw0, zw1, face, into, wruns in ((z0-T, z0, z0, 1, runs), (z1, z1+T, z1, -1, [(-bx, bx)])):
        for a, c in wruns: wall(a, c, zw0, zw1, 0.0, BAY_ROOF, METAL)
        if wruns is runs: wall(d0, d1, zw0, zw1, dtop, BAY_ROOF, METAL)
        bay.dress('z', face, into, wruns, 0.0, BAY_ROOF, step=4)
    # The flag stands on the flat floor: only paint round it, a team disc in
    # a lit ring (drawn, not solid).
    fx, fy, fz = FLAG_HULL
    b.prism(fx, fz, FLAG_MARK, FLAG_MARK, fy, fy+.022, 24, accent, solid=False)
    b.prism(fx, fz, FLAG_MARK+.12, FLAG_MARK+.12, fy, fy+.008, 24, GLOW, solid=False)
    b.lamp((0.0, 3.2, (z0+z1)/2), 1.4)
    for x in (-4.5, 4.5):
        mesh.box((x, BAY_ROOF-.04, (z0+z1)/2), (.4, .06, z1-z0-2.0), GLOW, False)
        for z in (z0+2.5, z1-2.5): b.lamp((x, BAY_ROOF-.4, z), .6)

    # --- On top: the turret on the bridge roof, the sensor on its mast step
    mesh.equipment('turret', TURRET, team, circuit)
    mesh.equipment('sensor', SENSOR, team, circuit)
    mesh.yaw = base_yaw                      # back to the base frame

    # --- Gangway: a ramp from the reef crest up to the entry platform. The
    # model draws it (girders, lit chevrons, columns, the pad); this is its
    # collision, standing in for it where models aren't drawn.
    gx0, gx1, gz0, gz1 = GANGWAY
    gtop = LEDGE[4]
    start = len(mesh.vertices)
    mesh.ramp((gx0+gx1)/2, gx1-gx0, gz0, gz1, gtop, GANGWAY_Y, GRATE)
    gy = lambda z: ramp_y(GANGWAY, z, gtop, GANGWAY_Y)
    for x in (gx0-.1, gx1+.1):
        for k in range(8):
            za, zb = gz0+(gz1-gz0)*k/8, gz0+(gz1-gz0)*(k+1)/8
            mesh.quad((x-.05, gy(za)+1.0, za), (x+.05, gy(za)+1.0, za), (x+.05, gy(zb)+1.0, zb), (x-.05, gy(zb)+1.0, zb), METAL, False)
            mesh.quad((x-.05, gy(za)+.05, za), (x+.05, gy(za)+.05, za), (x+.05, gy(zb)+.05, zb), (x-.05, gy(zb)+.05, zb), GLOW, False)
    for z in GANGWAY_BENTS:
        for x in (gx0+.35, gx1-.35):
            wall(x-.35, x+.35, z-.35, z+.35, SAND-2.0, gy(z)-.9, METAL)
    pz_ = gz1-GANGWAY_PAD_R+2.0
    b.prism((gx0+gx1)/2, pz_, GANGWAY_PAD_R, GANGWAY_PAD_R, GANGWAY_Y-1.2, GANGWAY_Y, 12, DECKING)
    b.prism((gx0+gx1)/2, pz_, GANGWAY_PAD_R+.05, GANGWAY_PAD_R+.05, GANGWAY_Y-.25, GANGWAY_Y-.1, 12, GLOW, solid=False, cap=False)
    mesh.standin.append((start, len(mesh.vertices)))

    spawn_points = [(to_base((x, SPAWN_LIFT, z)), SPAWN_YAW+HULL_TURN) for x, z in SPAWNS]
    H = lambda p: to_base(p)
    return dict(flag=FLAG, spawn=spawn_points[0][0], spawn_points=[(*p, yaw) for p, yaw in spawn_points],
                generator=H(GEN), hold=H((0.0, 0.0, (hz0+hz1)/2)), bay=H((0.0, 0.0, -3.25)),
                deck=H((0.0, 7.45, -20.0)), roof=H((0.0, 10.75, 21.0)),
                station_room=H((0.0, MID_FLOOR, -15.0)), balcony=H((-9.0, MID_FLOOR, 0.3)),
                fore_gallery=H((0.0, MID_FLOOR, 11.0)), aft_gallery=H((0.0, 0.0, 22.7)),
                porthole=H((PORTHOLE[0], 0.0, PORTHOLE[1])),
                aft_room=H((0.0, 0.0, 26.2)), lift_base=H((0.0, PLATFORM_BASE[3], (LIFT[1]+LIFT[2])/2)),
                cargo_port=H((-BAY_X-1.3, 0.0, sum(BAY_OPEN)/2)), star_hatch=H((BAY_X+1.3, 0.0, sum(BAY_OPEN)/2)),
                side_door=H((SIDE_LEDGE[1]-1.0, 0.0, sum(SIDE_DOOR[:2])/2)), hatch=H((0.0, HATCH_TOP, sum(HATCH[2:])/2)),
                gangway=((gx0+gx1)/2, ramp_y(GANGWAY, (gz0+gz1)/2, gtop, GANGWAY_Y), (gz0+gz1)/2),
                beacon=(BEACON_AT[0], 0.0, BEACON_AT[1]))


def build_beacon(mesh, team, circuit):
    """A short reef beacon beside the flag: a tapered tower with a turret
    ledge. Local ground Y=0 at its foot; the tower runs 3 m below it."""
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=WOOD, metal=METAL, glow=GLOW)
    b.prism(0, 0, BEACON_R0, BEACON_R1, -3.0, BEACON_TOP, 8, HULL_MAT, phase=math.pi/8)
    b.prism(0, 0, BEACON_LEDGE, BEACON_LEDGE, BEACON_LEDGE_Y-.7, BEACON_LEDGE_Y, 12, METAL)
    b.prism(0, 0, BEACON_LEDGE+.05, BEACON_LEDGE+.05, BEACON_LEDGE_Y-.3, BEACON_LEDGE_Y-.18, 12, GLOW, solid=False, cap=False)
    taper = lambda y: BEACON_R0+(BEACON_R1-BEACON_R0)*(y+3.0)/(BEACON_TOP+3.0)
    for y0, y1, mat in ((3.0, 4.2, accent), (8.0, 8.2, GLOW), (13.5, 14.7, accent)):
        r = taper(y0)+.04
        b.prism(0, 0, r, r, y0, y1, 8, mat, phase=math.pi/8, solid=False, cap=False)
    b.prism(0, 0, BEACON_R1+.4, BEACON_R1+.4, BEACON_TOP, BEACON_TOP+.6, 8, METAL, phase=math.pi/8)
    b.prism(0, 0, 1.0, .8, BEACON_TOP+.6, BEACON_TOP+1.8, 8, GLOW, solid=False)
    mesh.equipment('turret', (0.0, BEACON_LEDGE_Y, -(BEACON_LEDGE-1.2)), team, circuit)
    b.lamp((0.0, BEACON_TOP+1.2, 0.0), 1.2)
    return dict(ledge=(0.0, BEACON_LEDGE_Y, BEACON_LEDGE-1.5), top=(0.0, BEACON_TOP+.6, 0.0))


# Outpost, in its own frame (floor Y=0, -Z is the door side).
OUT_X, OUT_Z0, OUT_Z1, OUT_H = 10.0, -7.0, 7.0, 4.8
OUT_DOOR = (-2.8, 2.8, 3.6)
OUT_SIDE_DOOR = (-2.0, 2.0, 3.6)           # z range on the +X wall
OUT_INV = ((-5.0, 0.0, 3.4), (5.0, 0.0, 3.4))
OUT_SPAWNS = ((-5.0, -3.0), (0.0, -3.0), (5.0, -3.0), (0.0, 1.0))


def outpost_site():
    return ('rect', -OUT_X-6, OUT_X+6, OUT_Z0-8, OUT_Z1+5)


def build_outpost(mesh, team, circuit):
    """A ground outpost on an outer island: a low bunker with a front and a
    side door, two inventory stations and four spawns. The roof is reached
    by jetting."""
    accent = ['ember', 'glacier'][team]
    b = Builder(mesh, accent, interior=WOOD, metal=METAL, glow=GLOW)
    wall, slab = b.wall, b.slab
    x0, x1, z0, z1 = -OUT_X, OUT_X, OUT_Z0, OUT_Z1
    foot = -2.5
    for a, c in _runs(x0, x1, [OUT_DOOR[:2]]):
        wall(a, c, z0, z0+W, foot, OUT_H, HULL_MAT)
    wall(OUT_DOOR[0], OUT_DOOR[1], z0, z0+W, OUT_DOOR[2], OUT_H, HULL_MAT)
    wall(x0, x1, z1-W, z1, foot, OUT_H, HULL_MAT)
    wall(x0, x0+W, z0+W, z1-W, foot, OUT_H, HULL_MAT)
    for a, c in _runs(z0+W, z1-W, [OUT_SIDE_DOOR[:2]]):
        wall(x1-W, x1, a, c, foot, OUT_H, HULL_MAT)
    wall(x1-W, x1, OUT_SIDE_DOOR[0], OUT_SIDE_DOOR[1], OUT_SIDE_DOOR[2], OUT_H, HULL_MAT)
    slab(x0, x1, z0, z1, OUT_H+.7, HULL_MAT, .7)
    slab(x0+W, x1-W, z0+W, z1-W, 0.0, DECKING, .8)
    b.dress('z', z1-W, -1, [(x0+W, x1-W)], 0.0, OUT_H)
    b.dress('x', x0+W, 1, [(z0+W, z1-W)], 0.0, OUT_H)
    b.ceiling_strip('x', 0.0, x0+2, x1-2, OUT_H, .9)
    for p in OUT_INV: mesh.equipment('inventory', p, team, circuit)
    b.face_quad('z', z0, -1, OUT_DOOR[0]-1.5, OUT_DOOR[1]+1.5, OUT_DOOR[2]+.25, OUT_H-.2, accent, off=.05)
    for u in (OUT_DOOR[0], OUT_DOOR[1]):
        b.face_box('z', z0, -1, u-.12, u+.12, 0.0, OUT_DOOR[2], .06, GLOW)
    b.face_box('z', z0, -1, x0, x1, OUT_H+.5, OUT_H+.65, .06, GLOW)
    b.lamp((0.0, OUT_DOOR[2]+.6, z0-1.0), .8)
    spawns = [((x, SPAWN_LIFT, z), 0.0) for x, z in OUT_SPAWNS]
    return dict(inside=(0.0, 0.0, 0.0), door=(0.0, 0.0, z0-1.5), roof=(0.0, OUT_H+.7, 0.0),
                spawn_points=[(*p, yaw) for p, yaw in spawns])
