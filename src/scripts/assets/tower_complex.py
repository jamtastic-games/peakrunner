"""Original small floating base: a central tower built round a three-storey
atrium, two rear support pods (armory, ship platform) on wide enclosed
tunnels, and two front turret pods on open bridges. Every structure is its
own hovering hull with a tapered keel and thruster nozzles underneath.

No source mesh, texture, lightmap or parser is used by this builder. Local
X/Z are horizontal, Y is up. -Z is the front (field-facing) side; +Z is the
rear. Caller supplies placement and ownership through mesh.origin/mesh.yaw.

The tower is a hollow block. Level 2 and Level 3 are balcony rings round a
14 x 12 m void, so the atrium rises from the Level 1 floor to the roof, about
25 m, and a jetting player can fly across it at any level. Entries:

- Level 1: a 6 m front door and two 6 m bridge doors, all open straight onto
  the atrium floor, and two 8 m rear tunnel mouths.
- Level 2: 6 m doors on the east and west faces, each onto an outside jet
  ledge.
- Level 3: open window bands on every face (fly-in balconies).
- Ramps: Level 1 to 2 along the west wall, Level 2 to 3 along the front.

The flag stands on the rear Level 2 balcony, overlooking the atrium, reached
from the ramp, from the east balcony, by jetting up from the atrium floor,
by dropping from Level 3, or straight in through either Level 2 door.

The generator sits on the keel level: a room inside the top of the tower's
keel, below Level 1, with exactly two entrances. One is the tube under the
atrium floor (drop in through the Level 1 hole, jet back up). The other is an
exterior hatch on the keel's +x flank, opening onto a ledge a jetting player
can reach from the bridges or the field; an airlock housing and an inner
baffle keep every outside line of sight out of the room (generator rooms
keep their baffles; every other opening is open).

Everything above ROOF (stepped setbacks, spine, fins, spires) and below the
keel level (lower keel, engine pods) is exterior massing. It is solid where a
jetting player could plausibly touch it and non-solid for thin decoration,
and none of it encloses a space a player can enter.

Interior dressing (wall liners, baseboards, cornices, pilasters, light
fixtures, frames) is render-only and sits at most 0.2 m proud of a solid
surface, inside the 0.52 m player radius, so it never needs collision.
Decoration never shares a plane with a solid face of another material
(no z-fighting): trims and cornices stand a few centimetres proud.

If the caller's mesh has a `lamps` list, every light fixture appends
(world_position, intensity) point samples to it for the offline light bake.
Floodlights on the bridges and ledges light the tower's outer faces, so the
side that faces away from the fixed sun is not left dark.
"""
import math

from assets.fortress_rooms import shaft

ASSET_ID = 'tower-complex-v8'

TOWER_HALF = 12
WALL = .4             # half-thickness of the tower perimeter walls
INNER = TOWER_HALF-WALL
SHAFT_HALF = 2.5
SHAFT_WALL = .6
L1, L2, L3, ROOF = 0, 8, 17, 26
VOID = (-7, 7, -6, 6)  # atrium void through the Level 2 and Level 3 floors
KEEL = -8             # generator level floor; its ceiling is the L1 slab (-1)
KEEL_IN = 8.1         # keel room inner wall faces at |x|, |z| = KEEL_IN
KEEL_WALL = .4
KEEL_CEILING = L1-1
HATCH_HALF = 2.2      # hatch passage half-width (z)
HATCH_TOP = KEEL+4.5
HATCH_OUT = 13.4      # outer end of the airlock housing (+x)
LEDGE_OUT = 16.5
KEEL_BAFFLE = (5.9, 6.3, -4.0, 4.0)   # x0, x1, z0, z1 inside the hatch
GENERATOR = (-5.2, KEEL, -5.2)
SHAFT_OPENINGS = [('-x', KEEL, KEEL+3.2)]
DOOR_TOP = 6.5        # Level 1 and Level 2 doorways: 6 m wide, 6.5 m tall
POD_Z = -30
BRIDGE_W = 6
REAR_Z0, REAR_Z1 = 26, 36
ROOM_H = 9            # rear rooms clear the 5.8 m kit generator and jet
TUNNEL_W, TUNNEL_H = 8, 7.5   # rear tunnels: 8 m wide, 6.5 m clear
FLAG = (0, 8.8)       # (x, z) on the rear Level 2 balcony
# Spawn anchors are the player's centre, not the floor: the engine's support
# ray starts 0.15 m above the feet (centre - 0.52 m radius), so a centre only
# 0.2 m above a 1 m slab starts inside it and reads the slab's underside,
# leaving the player "airborne" and frictionless (the v3/v4 slick spawn).
SPAWN_LIFT = 1.2   # same convention as every other pack: centre 1.2 m above the floor
RAMP_HEADROOM = 2.4   # under-ramp space lower than this is closed off
RAMP_CLEAR = 3.2      # floors above a ramp are cut back to leave this much over it
RAMP_SIDE_GAP = .05   # floor cut-outs clear each ramp side by this much
PLINTH = .3
SILL, HEAD = .4, 5.2  # Level 3 window aperture, above the L3 floor
MULLION_STEP = 3
LEDGE = 5             # Level 2 jet ledges reach this far out from the wall
# Window apertures per wall (along-wall ranges). The front keeps its centre
# solid behind the banner strip and accent bands on the outer face.
WINDOWS = {'-z': [(-9,-5.8),(5.8,9)], '+z': [(-9,9)], '-x': [(-9,9)], '+x': [(-9,9)]}
L1_GAPS = {'-z': [(-12,-6),(-3,3),(6,12)], '+z': [(-12,-4),(4,12)], '-x': [], '+x': []}
L2_GAPS = {'-z': [], '+z': [], '-x': [(-3,3)], '+x': [(-3,3)]}
# Ramps. L1->L2 climbs the west wall front to back; L2->L3 climbs the front
# balcony east to west. Each floor above is cut away where the ramp would
# leave less than RAMP_HEADROOM under it.
RAMP1 = dict(x=-9, width=4, z0=-11, z1=11)
RAMP2 = dict(z=-9, width=4, x0=11, x1=-11)

# Material roles. 'bark' is a kit slot this pack repaints as the interior
# wall panel; it is never used for trees here.
HULL, INTERIOR, DECK, HAZARD, METAL, GLOW = 'concrete', 'bark', 'panel', 'grate', 'trim', 'light'


def spans(lo, hi, gaps):
    out, u = [], lo
    for a, b in sorted(gaps):
        if a > u: out.append((u, a))
        u = max(u, b)
    if hi > u: out.append((u, hi))
    return out


def ramp1_height(z):
    return L1+(L2-L1)*(z-RAMP1['z0'])/(RAMP1['z1']-RAMP1['z0'])


def ramp2_height(x):
    return L2+(L3-L2)*(RAMP2['x0']-x)/(RAMP2['x0']-RAMP2['x1'])


def ramp_holes():
    """Floor cut-outs above each ramp, where it rises within RAMP_CLEAR of the
    next slab's underside (the slab is 1 m thick). A body is 2.56 m tall, so
    3.2 m keeps a jetting climber clear of the edge."""
    under2, under3 = L2-1, L3-1
    z_cut = RAMP1['z0']+(under2-RAMP_CLEAR-L1)*(RAMP1['z1']-RAMP1['z0'])/(L2-L1)
    x_cut = RAMP2['x0']-(under3-RAMP_CLEAR-L2)*(RAMP2['x0']-RAMP2['x1'])/(L3-L2)
    # 5 cm wider than the ramp, so its side plates never share a plane with
    # the slab's cut edge.
    w1, w2 = RAMP1['width']/2+RAMP_SIDE_GAP, RAMP2['width']/2+RAMP_SIDE_GAP
    return ((RAMP1['x']-w1, RAMP1['x']+w1, z_cut, RAMP1['z1']),
            (RAMP2['x1'], x_cut, RAMP2['z']-w2, RAMP2['z']+w2))


def build(mesh, team, circuit):
    if team not in (0,1) or not circuit: raise ValueError('team and circuit required')
    accent = ['ember','glacier'][team]
    lamps = getattr(mesh, 'lamps', None)

    def lamp(p, power):
        if lamps is not None: lamps.append((mesh.point(p), power))

    def slab(x0,x1,z0,z1,y,mat=DECK):
        mesh.box(((x0+x1)/2,y-.5,(z0+z1)/2),(x1-x0,1,z1-z0),mat)
    def wall(x0,x1,z0,z1,y0,y1,mat=HULL,solid=True):
        if x0>x1: x0,x1=x1,x0
        if z0>z1: z0,z1=z1,z0
        if y0>y1: y0,y1=y1,y0
        mesh.box(((x0+x1)/2,(y0+y1)/2,(z0+z1)/2),(x1-x0,y1-y0,z1-z0),mat,solid)
    def floor_ring(y,mat=DECK,holes=(),edge=TOWER_HALF):
        """A storey's floor out to `edge`: the wall's inner face where the
        wall top below shows in doorways (so the two never share a plane)."""
        T=edge
        xs=sorted({-T,T,*(v for hx0,hx1,_,_ in holes for v in (hx0,hx1))})
        for xa,xb in zip(xs,xs[1:]):
            cover=sorted((hz0,hz1) for hx0,hx1,hz0,hz1 in holes if hx0<=xa and hx1>=xb)
            z=-T
            for hz0,hz1 in cover+[(T,T)]:
                if hz0>z: slab(xa,xb,z,hz0,y,mat)
                z=max(z,hz1)
    def hull(cx,cz,bottom,top,y0,y1,mat,solid=True):
        bx,bz=bottom; tx,tz=top
        for s in (-1,1):
            mesh.quad((cx-bx,y0,cz+s*bz),(cx+bx,y0,cz+s*bz),(cx+tx,y1,cz+s*tz),(cx-tx,y1,cz+s*tz),mat,solid)
            mesh.quad((cx+s*bx,y0,cz-bz),(cx+s*bx,y0,cz+bz),(cx+s*tx,y1,cz+tz),(cx+s*tx,y1,cz-tz),mat,solid)
    def thruster(x,y,z,r):
        mesh.column((x,y-r*1.4,z),r*.75,r*1.4,GLOW,10,top=r,solid=False)
    def keel(cx,cz,half,y_top,depth,r_tip):
        hull(cx,cz,(half[0]*.72,half[1]*.72),half,y_top-depth*.3,y_top,METAL)
        hull(cx,cz,(r_tip,r_tip),(half[0]*.72,half[1]*.72),y_top-depth,y_top-depth*.3,HULL)
        thruster(cx,y_top-depth,cz,r_tip)
    def ramp_x(zc,width,x0,x1,y0,y1,mat=HAZARD):
        """Like the kit's mesh.ramp, but climbing along x."""
        w=width/2
        a,b,c,d=(x0,y0,zc-w),(x0,y0,zc+w),(x1,y1,zc+w),(x1,y1,zc-w)
        if x1>x0: mesh.quad(a,b,c,d,mat)
        else: mesh.quad(a,d,c,b,mat)
        aa,bb,cc,dd=[(p[0],p[1]-.6,p[2]) for p in (a,b,c,d)]
        mesh.quad(aa,bb,cc,dd,METAL); mesh.quad(a,aa,dd,d,METAL); mesh.quad(b,c,cc,bb,METAL)
    def floodlight(p, aim, power=1.6):
        """A small lamp housing, and a bake sample a little in front of it."""
        x,y,z=p; ax,az=aim
        mesh.box((x,y,z),(.5,.35,.5),METAL,False)
        mesh.box((x+ax*.28,y,z+az*.28),(.34 if ax==0 else .06,.22,.06 if ax==0 else .34),GLOW,False)
        lamp((x+ax*.8,y,z+az*.8),power)

    # --- Surface dressing helpers --------------------------------------------
    # A "face" is an axis-aligned wall plane described by the side of the room
    # it bounds, the plane coordinate and which way the room lies. Along-wall
    # coordinate u is x for z-walls and z for x-walls.
    def at(axis, plane, u, y):
        return (u, y, plane) if axis == 'z' else (plane, y, u)
    def face_quad(axis, plane, into, u0, u1, y0, y1, mat, off=.03):
        p = plane+into*off
        a,b,c,d = at(axis,p,u0,y0),at(axis,p,u1,y0),at(axis,p,u1,y1),at(axis,p,u0,y1)
        # Wind so the normal points into the room.
        flip = (axis == 'z') == (into < 0)
        mesh.quad(*((a,d,c,b) if flip else (a,b,c,d)), mat, False)
    def face_box(axis, plane, into, u0, u1, y0, y1, depth, mat, solid=False):
        centre = at(axis, plane+into*depth/2, (u0+u1)/2, (y0+y1)/2)
        size = (u1-u0, y1-y0, depth) if axis == 'z' else (depth, y1-y0, u1-u0)
        mesh.box(centre, size, mat, solid)
    def dress(axis, plane, into, runs, y0, y1, stripe=True, pilasters=True):
        """Liner, baseboard, cornice, accent stripe and pilasters over the
        solid runs of one wall face between floor y0 and ceiling y1."""
        for u0,u1 in runs:
            if u1-u0 < .05: continue
            face_quad(axis,plane,into,u0,u1,y0,y1,INTERIOR)
            face_box(axis,plane,into,u0,u1,y0,y0+.3,.12,METAL)
            face_box(axis,plane,into,u0,u1,y1-.28,y1,.18,METAL)
            if stripe: face_quad(axis,plane,into,u0,u1,y0+.7,y0+.95,accent,.045)
            if pilasters:
                for u in range(-9,10,6):
                    if u0+.4 <= u <= u1-.4: face_box(axis,plane,into,u-.22,u+.22,y0+.3,y1-.28,.15,METAL)
    def strip(p0, p1, y_ceiling, power, spacing=3):
        """Ceiling light strip from p0 to p1 (x, z), axis-aligned."""
        (x0,z0),(x1,z1)=p0,p1
        cx,cz=(x0+x1)/2,(z0+z1)/2; along_x = abs(x1-x0) > abs(z1-z0)
        length = abs(x1-x0) if along_x else abs(z1-z0)
        size = lambda l,w: (l,w) if along_x else (w,l)
        a,b = size(length+.4,.8); mesh.box((cx,y_ceiling-.035,cz),(a,.07,b),METAL,False)
        a,b = size(length,.45); mesh.box((cx,y_ceiling-.07,cz),(a,.06,b),GLOW,False)
        n = max(1, round(length/spacing))
        for i in range(n):
            t=(i+.5)/n; lamp((x0+(x1-x0)*t,y_ceiling-.3,z0+(z1-z0)*t), power)

    tower_faces = {'-z': ('z',-INNER,1), '+z': ('z',INNER,-1), '-x': ('x',-INNER,1), '+x': ('x',INNER,-1)}
    T, W = TOWER_HALF, WALL
    hole_r1, hole_r2 = ramp_holes()

    # --- Central tower: the atrium block ------------------------------------
    h = SHAFT_HALF
    floor_ring(L1, holes=[(-h,h,-h,h)])        # the tube hole: drop to the keel room
    floor_ring(L2, holes=[VOID, hole_r1], edge=INNER)   # L2 doorways: the L1 wall top is the sill
    floor_ring(L3, holes=[VOID, hole_r2])
    slab(-T,T,-T,T,ROOF)
    # Balcony edges: a thin accent-lit lip round the void at Level 2 and 3.
    vx0,vx1,vz0,vz1 = VOID
    for y in (L2,L3):
        for s,(u0,u1) in ((-1,(vx0,vx1)),(1,(vx0,vx1))):
            wall(u0,u1,(vz0 if s<0 else vz1)-.03,(vz0 if s<0 else vz1)+.03,y-1.02,y+.02,METAL,False)
        for x in (vx0,vx1):
            wall(x-.03,x+.03,vz0,vz1,y-1.02,y+.02,METAL,False)
        for s in (-1,1):
            wall(vx0,vx1,(vz0 if s<0 else vz1)-.05,(vz0 if s<0 else vz1)+.05,y-.72,y-.62,GLOW,False)
    # The tube below the atrium floor: gunmetal walls down to the keel room.
    # Walls stop at the keel ceiling: the L1 slab's hole edge is the rest of the
    # tube, so no wall top shares the floor's plane.
    shaft(mesh,0,0,SHAFT_HALF,KEEL,KEEL_CEILING,mat=METAL,thickness=SHAFT_WALL,openings=SHAFT_OPENINGS)
    edge = SHAFT_HALF+SHAFT_WALL
    mid = -(SHAFT_HALF+SHAFT_WALL/2)
    for s in (-1,1):   # solid hazard jambs in the keel-level opening's corners
        wall(mid-SHAFT_WALL/2,mid+SHAFT_WALL/2,s*(edge-.35),s*edge,KEEL,KEEL+3.2,HAZARD)
    wall(mid-SHAFT_WALL/2-.02,mid+SHAFT_WALL/2+.04,-edge+.03,edge-.03,KEEL+3.22,KEEL+3.55,HAZARD,False)
    for s in (-1,1):
        mesh.box((s*(SHAFT_HALF-.03),KEEL+5.4,0),(.06,.18,2*SHAFT_HALF),GLOW,False)
        mesh.box((0,KEEL+5.4,s*(SHAFT_HALF-.03)),(2*SHAFT_HALF,.18,.06),GLOW,False)
        mesh.box((0,KEEL+.015,s*(SHAFT_HALF-.2)),(2*SHAFT_HALF,.03,.4),HAZARD,False)
        mesh.box((s*(SHAFT_HALF-.2),KEEL+.016,0),(.4,.03,2*SHAFT_HALF-.8),HAZARD,False)
    lamp((0,KEEL+5,0),.9)
    # Hazard lip round the tube's hole in the atrium floor (the drop).
    for s in (-1,1):
        wall(-h-.04,h+.04,s*h-.04,s*h+.04,L1-.3,L1+.03,HAZARD,False)
        wall(s*h-.04,s*h+.04,-h+.04,h-.04,L1-.3,L1+.03,HAZARD,False)

    # --- Keel level: the generator room ------------------------------------
    # A room inside the top of the keel, under the L1 slab. Exactly two ways
    # in: the tube's keel-level face (-x) and the hatch passage on the +x
    # flank. The baffle just inside the hatch turns the entry sideways.
    K, KW = KEEL_IN, KEEL_WALL
    slab(-K-KW,K,-K-KW,K+KW,KEEL)                       # floor (the ledge slab carries x > K)
    keel_faces = {'-z': ('z',-K,1), '+z': ('z',K,-1), '-x': ('x',-K,1), '+x': ('x',K,-1)}
    for side,(axis,plane,into) in keel_faces.items():
        outer = plane-into*KW; lo, hi = min(plane,outer), max(plane,outer)
        gaps = [(-HATCH_HALF,HATCH_HALF)] if side == '+x' else []
        for u0,u1 in spans(-K-KW,K+KW,gaps):
            if axis == 'z': wall(u0,u1,lo,hi,KEEL,KEEL_CEILING)
            else: wall(lo,hi,u0,u1,KEEL,KEEL_CEILING)
        for u0,u1 in gaps:
            wall(lo,hi,u0,u1,HATCH_TOP,KEEL_CEILING)
            face_quad(axis,plane,into,u0,u1,HATCH_TOP,KEEL_CEILING,INTERIOR)
        dress(axis,plane,into,spans(-K,K,gaps),KEEL,KEEL_CEILING)
    bx0,bx1,bz0,bz1 = KEEL_BAFFLE
    wall(bx0,bx1,bz0,bz1,KEEL,KEEL_CEILING)
    dress('x',bx1,1,[(bz0,bz1)],KEEL,KEEL_CEILING,pilasters=False)
    dress('x',bx0,-1,[(bz0,bz1)],KEEL,KEEL_CEILING,pilasters=False)
    for z in (bz0,bz1): wall(bx0-.06,bx1+.06,z-.06,z+.06,KEEL,KEEL+4.5,HAZARD,False)
    for x in (-5.5,5.5): strip((x,-7),(x,7),KEEL_CEILING,.8)
    mesh.equipment('generator',GENERATOR,team,circuit)
    gxk,_,gzk = GENERATOR
    mesh.box((gxk,KEEL+.015,gzk),(6.4,.03,5.4),HAZARD,False)
    # Hatch passage and airlock housing: floor on the ledge slab, solid side
    # walls and roof from the room wall out past the keel flank.
    slab(K,LEDGE_OUT,-HATCH_HALF-2.3,HATCH_HALF+2.3,KEEL)              # passage floor + ledge
    for s in (-1,1):
        wall(K+KW,HATCH_OUT,s*HATCH_HALF,s*(HATCH_HALF+.4),KEEL,HATCH_TOP+1)
        dress('z',s*HATCH_HALF,-s,[(K+KW,HATCH_OUT)],KEEL,HATCH_TOP,pilasters=False)
    slab(K+KW,HATCH_OUT,-HATCH_HALF-.4,HATCH_HALF+.4,HATCH_TOP+1,HULL)    # roof
    mesh.box(((K+KW+HATCH_OUT)/2,HATCH_TOP-.035,0),(HATCH_OUT-K-KW-.6,.07,.8),METAL,False)
    mesh.box(((K+KW+HATCH_OUT)/2,HATCH_TOP-.07,0),(HATCH_OUT-K-KW-1,.06,.45),GLOW,False)
    for x in (K+1.5,HATCH_OUT-1.5): lamp((x,HATCH_TOP-.3,0),.5)
    # Outer frame, hazard lintel and ledge edge markings (render only).
    for s in (-1,1):
        mesh.box((HATCH_OUT+.07,(KEEL+HATCH_TOP)/2,s*(HATCH_HALF+.2)),(.14,HATCH_TOP-KEEL,.5),METAL,False)
        mesh.box(((HATCH_OUT+LEDGE_OUT)/2,KEEL+.015,s*(HATCH_HALF+2.1)),(LEDGE_OUT-HATCH_OUT,.03,.3),HAZARD,False)
    mesh.box((HATCH_OUT+.07,HATCH_TOP+.2,0),(.16,.4,2*HATCH_HALF+.9),HAZARD,False)
    mesh.box((LEDGE_OUT-.15,KEEL+.015,0),(.3,.03,2*HATCH_HALF+4.6),HAZARD,False)
    for s in (-1,1):
        mesh.column((LEDGE_OUT-.4,KEEL,s*(HATCH_HALF+2)),.12,1.6,METAL,6,top=.05,solid=False)
        mesh.box((LEDGE_OUT-.4,KEEL+1.7,s*(HATCH_HALF+2)),(.2,.2,.2),GLOW,False)
        lamp((LEDGE_OUT-.4,KEEL+2.2,s*(HATCH_HALF+2)),.3)

    # --- Tower shell ------------------------------------------------------
    def shell(y0, y1, gaps, door_top, enclosed=()):
        """Perimeter walls of one storey with door gaps capped by headers."""
        for side,(axis,plane,into) in tower_faces.items():
            outer = plane-into*2*W
            lo, hi = min(plane,outer), max(plane,outer)
            for u0,u1 in spans(-T,T,gaps[side]):
                if axis == 'z': wall(u0,u1,lo,hi,y0,y1)
                else: wall(lo,hi,u0,u1,y0,y1)
            for u0,u1 in gaps[side]:
                if axis == 'z': wall(u0,u1,lo,hi,y0+door_top,y1)
                else: wall(lo,hi,u0,u1,y0+door_top,y1)
                # Frames on both faces and a hazard band under the header.
                faces = ((plane,into),) if side in enclosed else ((plane,into),(outer,-into))
                for face,direction in faces:
                    for f0,f1,g0,g1 in ((max(u0-.3,-T),u0,y0+.02,y0+door_top),(u1,min(u1+.3,T),y0+.02,y0+door_top),
                                        (max(u0-.3,-T),min(u1+.3,T),y0+door_top+.02,y0+door_top+.32)):
                        if f1-f0 > .05: face_box(axis,face,direction,f0,f1,g0,g1,.14,METAL)
                if axis == 'z': wall(u0,u1,lo-.06,hi+.06,y0+door_top-.15,y0+door_top-.02,HAZARD,False)
                else: wall(lo-.06,hi+.06,u0,u1,y0+door_top-.15,y0+door_top-.02,HAZARD,False)
            runs = [(max(u0,-INNER),min(u1,INNER)) for u0,u1 in spans(-T,T,gaps[side])]
            dress(axis,plane,into,[r for r in runs if r[1]>r[0]],y0,y1-1)
            for u0,u1 in gaps[side]:
                face_quad(axis,plane,into,max(u0,-INNER),min(u1,INNER),y0+door_top,y1-1,INTERIOR)
    shell(L1, L2, L1_GAPS, DOOR_TOP, enclosed=('+z',))
    shell(L2, L3, L2_GAPS, DOOR_TOP)
    # Level 3: sill, head and piers around open window bands; the mullions
    # are decoration, so shots and players pass between them.
    for side,(axis,plane,into) in tower_faces.items():
        outer = plane-into*2*W; lo, hi = min(plane,outer), max(plane,outer)
        def piece(u0,u1,y0,y1):
            if axis == 'z': wall(u0,u1,lo,hi,y0,y1)
            else: wall(lo,hi,u0,u1,y0,y1)
        piece(-T,T,L3,L3+SILL); piece(-T,T,L3+HEAD,ROOF)
        piers = spans(-T,T,WINDOWS[side])
        for u0,u1 in piers: piece(u0,u1,L3+SILL,L3+HEAD)
        wmid = (lo+hi)/2
        for u0,u1 in WINDOWS[side]:
            y0,y1 = L3+SILL,L3+HEAD
            face_box(axis,outer,-into,u0,u1,y0-.12,y0+.02,.16,METAL)
            for face,direction in ((plane,into),(outer,-into)):
                face_box(axis,face,direction,u0,u1,y1-.02,y1+.12,.16,METAL)
            n = max(1, round((u1-u0)/MULLION_STEP))
            for i in range(n+1):
                u = u0+(u1-u0)*i/n
                if axis == 'z': wall(u-.1,u+.1,wmid-.2,wmid+.2,y0,y1,METAL,False)
                else: wall(wmid-.2,wmid+.2,u-.1,u+.1,y0,y1,METAL,False)
        inner_runs = [(max(u0,-INNER),min(u1,INNER)) for u0,u1 in piers]
        face_quad(axis,plane,into,-INNER,INNER,L3,L3+SILL,INTERIOR)
        for u0,u1 in inner_runs:
            if u1>u0: face_quad(axis,plane,into,u0,u1,L3+SILL,L3+HEAD,INTERIOR)
        face_quad(axis,plane,into,-INNER,INNER,L3+HEAD,ROOF-1,INTERIOR)
        face_box(axis,plane,into,-INNER,INNER,ROOF-1.28,ROOF-1,.18,METAL)

    # Ceiling light strips: under each balcony ring and across the roof.
    # Only where a slab is overhead (not over the ramp cut-outs).
    for p0,p1 in [((9.3,-9),(9.3,9)),((-9.3,-9),(-9.3,1)),((-5,-9),(5,-9)),((-5,9),(5,9))]:
        strip(p0,p1,L2-1,.7)
    for p0,p1 in [((9.3,-4),(9.3,9)),((-9.3,-5),(-9.3,9)),((-2,-9),(5,-9)),((-5,9),(5,9))]:
        strip(p0,p1,L3-1,.7)
    for x in (-6,0,6): strip((x,-9),(x,9),ROOF-1,1.0)

    # --- Atrium floor: cover and the ramps ---------------------------------
    for x,z in [(-4.5,-4.2),(4.5,-4.2),(-4.5,4.2),(4.5,4.2)]:
        mesh.box((x,1.5,z),(1.5,3,1.5),HULL)
        mesh.box((x,3.07,z),(1.78,.14,1.78),METAL,False)
        mesh.box((x,.18,z),(1.68,.36,1.68),METAL,False)
        mesh.box((x,2.3,z),(1.56,.16,1.56),accent,False)
        for dx in (-.76,.76):
            for dz in (-.76,.76): mesh.box((x+dx,1.5,z+dz),(.1,2.6,.1),HAZARD,False)
    r1, r2 = RAMP1, RAMP2
    mesh.ramp(r1['x'],r1['width'],r1['z0'],r1['z1'],L1,L2,HAZARD)
    ramp_x(r2['z'],r2['width'],r2['x0'],r2['x1'],L2,L3)
    for s in (-1,1):   # rails and posts on the room side of each ramp
        x=r1['x']+s*r1['width']/2
        mesh.quad((x,L1+.9,r1['z0']),(x,L2+.9,r1['z1']),(x,L2+1,r1['z1']),(x,L1+1,r1['z0']),METAL,False)
        z=r2['z']+s*r2['width']/2
        mesh.quad((r2['x0'],L2+.9,z),(r2['x1'],L3+.9,z),(r2['x1'],L3+1,z),(r2['x0'],L2+1,z),METAL,False)
        for i in range(6):
            t=(i+.5)/6
            mesh.box((x,L1+t*(L2-L1)+.5,r1['z0']+t*(r1['z1']-r1['z0'])),(.08,1,.08),METAL,False)
            mesh.box((r2['x0']+t*(r2['x1']-r2['x0']),L2+t*(L3-L2)+.5,z),(.08,1,.08),METAL,False)
    # Close the low wedge under each ramp: a player could walk in beneath the
    # rising underside and get pinched between it and the floor, which pops
    # them up into a frictionless coast. Solid skirt on the room side from
    # where the underside leaves the floor to where it clears RAMP_HEADROOM,
    # plus an end cap across the ramp and the wall trench.
    def skirt_z(x_in, x_wall, z_floor, z_open, y):
        top = y+RAMP_HEADROOM
        a,b,c = (x_in,y,z_floor),(x_in,y,z_open),(x_in,top,z_open)
        mesh.triangle([a,b,c],METAL); mesh.triangle([a,c,b],METAL)
        lo, hi = min(x_in,x_wall), max(x_in,x_wall)
        quad = ((lo,y,z_open),(hi,y,z_open),(hi,top,z_open),(lo,top,z_open))
        mesh.quad(*quad,METAL); mesh.quad(*quad[::-1],METAL)
    def skirt_x(z_in, z_wall, x_floor, x_open, y):
        top = y+RAMP_HEADROOM
        a,b,c = (x_floor,y,z_in),(x_open,y,z_in),(x_open,top,z_in)
        mesh.triangle([a,b,c],METAL); mesh.triangle([a,c,b],METAL)
        lo, hi = min(z_in,z_wall), max(z_in,z_wall)
        quad = ((x_open,y,lo),(x_open,y,hi),(x_open,top,hi),(x_open,top,lo))
        mesh.quad(*quad,METAL); mesh.quad(*quad[::-1],METAL)
    run1 = r1['z1']-r1['z0']; rise1 = L2-L1
    skirt_z(r1['x']+r1['width']/2,-INNER,r1['z0']+.6*run1/rise1,r1['z0']+(RAMP_HEADROOM+.6)*run1/rise1,L1)
    run2 = r2['x0']-r2['x1']; rise2 = L3-L2
    skirt_x(r2['z']+r2['width']/2,-INNER,r2['x0']-.6*run2/rise2,r2['x0']-(RAMP_HEADROOM+.6)*run2/rise2,L2)

    # --- Level 2: the flag on the rear balcony, and the side jet ledges -----
    fx, fz = FLAG
    flag = (fx,L2+PLINTH+.05,fz)
    mesh.column((fx,L2,fz),1.7,PLINTH,METAL,12,top=1.45)
    mesh.column((fx,L2+PLINTH+.004,fz),1.45,.012,accent,12,top=1.45,solid=False)
    mesh.column((fx,L2+PLINTH+.016,fz),1.05,.012,DECK,12,top=1.05,solid=False)
    mesh.column((fx,L2+PLINTH+.028,fz),.35,.012,GLOW,12,top=.35,solid=False)
    bz = INNER-.12
    mesh.quad((-2.2,L2+1.2,bz),(-2.2,L2+5.4,bz),(2.2,L2+5.4,bz),(2.2,L2+1.2,bz),accent,False)
    mesh.triangle([(-2.2,L2+1.2,bz),(2.2,L2+1.2,bz),(0,L2+.2,bz)],accent,False)
    for base_y in (2.6,3.8):
        for s in (-1,1):
            mesh.quad((s*1.5,L2+base_y,bz-.02),(0,L2+base_y+1.2,bz-.02),(0,L2+base_y+1.6,bz-.02),(s*1.5,L2+base_y+.4,bz-.02),GLOW,False)
    lamp((fx,L2+3,fz),.6)
    for s in (-1,1):
        x0,x1 = sorted((s*(T+W), s*(T+LEDGE)))   # from the wall's outer face
        slab(x0,x1,-3.5,3.5,L2,DECK)
        mesh.box((s*(T+LEDGE-.15),L2+.015,0),(.3,.03,7),HAZARD,False)
        for dz in (-3.2,3.2):
            mesh.column((s*(T+LEDGE-.4),L2,dz),.12,1.6,METAL,6,top=.05,solid=False)
            mesh.box((s*(T+LEDGE-.4),L2+1.7,dz),(.2,.2,.2),GLOW,False)
            lamp((s*(T+LEDGE-.4),L2+2.2,dz),.3)
        # The ledge's own little keel.
        hull(s*(T+LEDGE/2),0,(.8,1.2),(LEDGE/2,3.5),L2-5,L2-1,METAL)
        thruster(s*(T+LEDGE/2),L2-5,0,.5)

    # --- Level 3: paired inventory stations on the rear balcony ------------
    for x in (-5.2,5.2): mesh.equipment('inventory',(x,L3,8.8),team,circuit)

    # --- Central tower: exterior massing ------------------------------------
    R = ROOF
    tiers = [(10.2,R,R+8),(7.6,R+8,R+17),(5.2,R+17,R+24)]
    for half,y0,y1 in tiers: wall(-half,half,-half,half,y0,y1)
    wall(-5.8,5.8,-5.8,5.8,R+24,R+25.2,METAL)
    for half,_,y1 in tiers:   # setback cornices, proud of the tier top (no z-fight)
        wall(-half-.25,half+.25,-half-.25,half+.25,y1-.4,y1+.05,METAL,False)
    wall(-3,3,-11,-5,R,R+21,DECK)
    front=-11.2
    mesh.quad((-3.4,R+2,-11.08),(3.4,R+2,-11.08),(3.4,R+25,-11.08),(-3.4,R+25,-11.08),METAL,False)
    mesh.quad((-2.3,R+10,front),(2.3,R+10,front),(2.3,R+24,front),(-2.3,R+24,front),accent,False)
    mesh.triangle([(-2.3,R+10,front),(0,R+7.6,front),(2.3,R+10,front)],accent,False)
    # Emblem: two stacked upward chevrons (a peak), original to PeakRunner.
    for base_y in (R+15.2,R+18.4):
        for s in (-1,1):
            mesh.quad((s*1.7,base_y,front-.12),(s*1.7,base_y+.8,front-.12),
                      (0,base_y+2.6,front-.12),(0,base_y+1.8,front-.12),GLOW,False)
    mesh.quad((-.22,R+1,front-.12),(.22,R+1,front-.12),(.22,R+8.5,front-.12),(-.22,R+8.5,front-.12),GLOW,False)
    fz_=-TOWER_HALF-.55
    mesh.quad((-.22,L1+7.2,fz_),(.22,L1+7.2,fz_),(.22,R-1,fz_),(-.22,R-1,fz_),GLOW,False)
    for s in (-1,1):
        mesh.quad((s*4,L1+7.4,fz_),(s*5.2,L1+7.4,fz_),(s*5.2,L3-.4,fz_),(s*4,L3-.4,fz_),accent,False)
    # Belt courses mark each floor line on the block (the front keeps its
    # centre clear for the strip and bands; door openings stay clear).
    for y in (L2,L3):
        for side,(axis,plane,into) in tower_faces.items():
            outer = plane-into*2*W
            runs = [(-T-.2,-5.6),(5.6,T+.2)] if side == '-z' else [(-T-.2,T+.2)]
            if y == L2 and side in ('-x','+x'): runs = [(-T-.2,-3.5),(3.5,T+.2)]
            for u0,u1 in runs: face_box(axis,outer,-into,u0,u1,y-.2,y+.2,.2,METAL)
    for sx in (-1,1):
        for sz in (-1,1):
            wall(sx*12.3,sx*13.7,sz*12.3,sz*13.7,-3,R+7)
            wall(sx*9.9,sx*11,sz*9.9,sz*11,R,R+16,METAL)
            mesh.column((sx*6.6,R+21,sz*6.6),.35,9,METAL,6,top=.08,solid=False)
            mesh.box((sx*13,R+6,sz*13),(.3,.3,.3),GLOW,False)
    mesh.column((0,R+25.2,0),.5,9,METAL,6,top=.06,solid=False)
    mesh.box((0,R+34.2,0),(.5,.5,.5),GLOW,False)

    # Upper keel band around the keel level. Its +x face is split round the
    # hatch housing (z within the housing's outer walls, below its roof).
    kb, kt = 9.5, 12.4
    hz, hy = HATCH_HALF+.4, HATCH_TOP+1
    at_y = lambda y: kb+(kt-kb)*(y-KEEL)/(KEEL_CEILING-KEEL)
    for s in (-1,1):
        mesh.quad((-kb,KEEL,s*kb),(kb,KEEL,s*kb),(kt,KEEL_CEILING,s*kt),(-kt,KEEL_CEILING,s*kt),METAL)
    mesh.quad((-kb,KEEL,-kb),(-kb,KEEL,kb),(-kt,KEEL_CEILING,kt),(-kt,KEEL_CEILING,-kt),METAL)
    mesh.quad((kb,KEEL,-kb),(kb,KEEL,-hz),(kt,KEEL_CEILING,-hz),(kt,KEEL_CEILING,-kt),METAL)
    mesh.quad((kb,KEEL,hz),(kb,KEEL,kb),(kt,KEEL_CEILING,kt),(kt,KEEL_CEILING,hz),METAL)
    mesh.quad((at_y(hy),hy,-hz),(at_y(hy),hy,hz),(kt,KEEL_CEILING,hz),(kt,KEEL_CEILING,-hz),METAL)
    hull(0,0,(4.5,4.5),(9.5,9.5),-22,-8,HULL)
    hull(0,0,(1.3,1.3),(4.5,4.5),-34,-22,METAL)
    thruster(0,-34,0,1.3)
    for sx in (-1,1):
        for sz in (-1,1):
            # Engine pods sit fully below the keel-level floor.
            mesh.column((sx*8,-15,sz*8),1.3,5.5,METAL,8,top=2.1)
            thruster(sx*8,-15,sz*8,1.3)

    # --- Rear tunnels + support pods --------------------------------------
    def corridor_z(x,width,z0,z1,y0,y1,mat=HULL):
        slab(x-width/2,x+width/2,z0,z1,y0,DECK)
        slab(x-width/2,x+width/2,z0,z1,y1,mat)
        wall(x-width/2-.3,x-width/2+.3,z0,z1,y0,y1,mat)
        wall(x+width/2-.3,x+width/2+.3,z0,z1,y0,y1,mat)
        for s in (-1,1):
            dress('x',x+s*(width/2-.3),-s,[(z0+.4,z1)],y0,y1-1,pilasters=False)
        strip((x,z0+.5),(x,z1-.5),y1-1,.6,spacing=4.5)
    for s in (-1,1):
        corridor_z(s*8,TUNNEL_W,T,REAR_Z0,L1,L1+TUNNEL_H)
    rear_mid=(REAR_Z0+REAR_Z1)/2

    def rear_room(x0,x1,door):
        slab(x0,x1,REAR_Z0,REAR_Z1,L1); slab(x0,x1,REAR_Z0,REAR_Z1,L1+ROOM_H,HULL)
        wall(x0,x1,REAR_Z1-.4,REAR_Z1+.4,L1,L1+ROOM_H)
        wall(x0-.4,x0+.4,REAR_Z0,REAR_Z1,L1,L1+ROOM_H)
        wall(x1-.4,x1+.4,REAR_Z0,REAR_Z1,L1,L1+ROOM_H)
        for a,b in [(x0,door[0]),(door[1],x1)]:
            if b>a: wall(a,b,REAR_Z0-.4,REAR_Z0+.4,L1,L1+ROOM_H)
        wall(door[0],door[1],REAR_Z0,REAR_Z0+.4,L1+TUNNEL_H-1,L1+ROOM_H)   # header over the tunnel mouth
        cx=(x0+x1)/2
        dress('z',REAR_Z1-.4,-1,[(x0+.4,x1-.4)],L1,L1+ROOM_H-1,pilasters=False)
        dress('x',x0+.4,1,[(REAR_Z0+.4,REAR_Z1-.4)],L1,L1+ROOM_H-1,pilasters=False)
        dress('x',x1-.4,-1,[(REAR_Z0+.4,REAR_Z1-.4)],L1,L1+ROOM_H-1,pilasters=False)
        runs=[r for r in [(x0+.4,door[0]),(door[1],x1-.4)] if r[1]-r[0]>.05]
        dress('z',REAR_Z0+.4,1,runs,L1,L1+ROOM_H-1,pilasters=False)
        for dx in (-2.5,2.5): strip((cx+dx,rear_mid-4),(cx+dx,rear_mid+4),L1+ROOM_H-1,.7)
        keel(cx,rear_mid,((x1-x0)/2+.4,(REAR_Z1-REAR_Z0)/2+.4),-1,13,1.1)
        for s in (-1,1):
            mesh.quad((cx+s*((x1-x0)/2+.45),2.2,REAR_Z0),(cx+s*((x1-x0)/2+.45),2.2,REAR_Z1),
                      (cx+s*((x1-x0)/2+.45),3.0,REAR_Z1),(cx+s*((x1-x0)/2+.45),3.0,REAR_Z0),accent,False)
        mesh.quad((x0,2.2,REAR_Z1+.45),(x1,2.2,REAR_Z1+.45),(x1,3.0,REAR_Z1+.45),(x0,3.0,REAR_Z1+.45),accent,False)
        return cx

    # Armory, rear-left: an inventory station facing the tunnel mouth and a
    # repair pad. Exhaust stacks on the roof.
    gx=rear_room(-13,-3,(-12,-4))
    mesh.equipment('inventory',(-10.5,L1,rear_mid-2.2),team,circuit)
    mesh.equipment('repair',(-6,L1,rear_mid-2.2),team,circuit)
    for dx,hh in [(-2.5,6),(0,8),(2.5,6)]:
        mesh.column((gx+dx,L1+ROOM_H,REAR_Z1-2.5),.9,hh,METAL,8,top=.7)
        mesh.column((gx+dx,L1+ROOM_H+hh,REAR_Z1-2.5),.72,.12,GLOW,8,top=.72,solid=False)

    # Ship platform, rear-right: light cover; the roof reads as a landing pad
    # (markers only, no new collision).
    sx_=rear_room(3,13,(4,12))
    for x,z in [(6,rear_mid-2),(10,rear_mid)]:
        mesh.box((x,1.5,z),(1.2,3,1.2),HULL)
        mesh.box((x,3.07,z),(1.44,.14,1.44),METAL,False)
    for dx in (-4,4):
        for dz in (-4,4): mesh.box((sx_+dx,L1+ROOM_H+.08,rear_mid+dz),(.8,.1,.8),GLOW,False)
    mesh.column((sx_,L1+ROOM_H+.02,rear_mid),3.2,.06,accent,12,top=3.2,solid=False)
    mesh.column((sx_,L1+ROOM_H+.06,rear_mid),2.6,.03,DECK,12,top=2.6,solid=False)

    # --- Front turret pods on open bridges --------------------------------
    for side in (-1,1):
        px = side*9
        mesh.ramp(px,BRIDGE_W,-TOWER_HALF,POD_Z+4.2,L1,L1,HAZARD)
        for s in (-1,1):   # bridge rails and posts
            mesh.box((px+s*(BRIDGE_W/2+.05),L1+1,(-TOWER_HALF+POD_Z+4.2)/2),(.08,.08,-POD_Z-4.2-TOWER_HALF),METAL,False)
            for z in range(-14,POD_Z+4,-4): mesh.box((px+s*(BRIDGE_W/2+.05),L1+.5,z),(.1,1,.1),METAL,False)
        mesh.column((px,L1-.63,POD_Z),5,.6,METAL,8)
        mesh.column((px,L1-.95,POD_Z),5.1,.3,accent,8,solid=False)
        mesh.column((px,L1-3.2,POD_Z),3.6,2.6,HULL,8,top=5)
        mesh.column((px,L1-11,POD_Z),1,7.8,METAL,8,top=3.6)
        thruster(px,L1-11,POD_Z,1)
        for dx in (-3.4,3.4): mesh.column((px+dx,L1,POD_Z+2.5),.22,4,METAL,6,top=.05,solid=False)
        for dx in (-3.4,3.4): mesh.box((px+dx,L1+4.05,POD_Z+2.5),(.2,.2,.2),GLOW,False)
        mesh.equipment('turret',(px,L1,POD_Z),team,circuit)
        # Visual turret mount around the kit pedestal (r=2): armoured base
        # ring, bolt ring, accent collar and cable boxes. Behaviour unchanged.
        mesh.column((px,L1,POD_Z),2.6,.28,METAL,12,top=2.25,solid=False)
        for a in range(12):
            ang = a*math.tau/12
            mesh.box((px+2.12*math.cos(ang),L1+.3,POD_Z+2.12*math.sin(ang)),(.14,.06,.14),GLOW if a%3==0 else METAL,False)
        mesh.column((px,L1+1.12,POD_Z),2.04,.14,accent,12,solid=False)
        mesh.column((px,L1+1.2,POD_Z),1.25,.12,METAL,10,solid=False)
        for dx in (-1.6,1.6): mesh.box((px+dx,L1+.45,POD_Z-1.9),(.7,.9,.5),METAL,False)
        # Floodlights on the bridge light the tower's front face.
        for z,y in ((-15.5,1.6),(-21,1.6)):
            floodlight((px-side*(BRIDGE_W/2+.35),L1+y,z),(0,1),1.4)
    # Floodlights on the Level 2 ledges and the tower corners wash the side
    # and rear faces, and the upper block.
    for s in (-1,1):
        floodlight((s*(T+LEDGE-1),L2+1.2,-3),(0,-1),1.2)
        floodlight((s*(T+LEDGE-1),L2+1.2,3),(0,1),1.2)
        for sz in (-1,1): lamp((s*14.6,L3+4,sz*14.6),1.4)
    for x in (-8,0,8): lamp((x,L3+2,-15.5),1.3)

    local_yaw = {'front': 0, '-x': math.pi/2, '+x': -math.pi/2}
    return {
        'flag': flag,
        'spawn': (-2,L1+SPAWN_LIFT,9.5),
        # (x, y, z, local yaw): yaw 0 faces the base front (-Z), pi/2 faces -X.
        'spawn_points': [(-2,L1+SPAWN_LIFT,9.5,0), (2,L1+SPAWN_LIFT,9.5,0),
                         (9.3,L1+SPAWN_LIFT,1.2,local_yaw['-x']), (9.3,L1+SPAWN_LIFT,-1.2,local_yaw['-x']),
                         (4.5,L2+SPAWN_LIFT,9.6,local_yaw['+x']),
                         (8,L1+SPAWN_LIFT,30,0),
                         (-11,L1+SPAWN_LIFT,34,local_yaw['+x']), (5,L1+SPAWN_LIFT,34,local_yaw['+x'])],
        'entrances': [(0,.2,-TOWER_HALF),(-9,.2,-TOWER_HALF),(9,.2,-TOWER_HALF),(-8,.2,TOWER_HALF),(8,.2,TOWER_HALF),
                      (-TOWER_HALF,L2+.2,0),(TOWER_HALF,L2+.2,0),(-9,.2,POD_Z),(9,.2,POD_Z)],
        'generator': GENERATOR,
        'armory': (-8,L1,rear_mid),
        'keel_hatch': (HATCH_OUT+1.5,KEEL,0),
        'keel_view': (6.5,KEEL+.2,6.5),
        'ship_platform': (8,L1,rear_mid),
        'turret_left': (-9,L1,POD_Z),
        'turret_right': (9,L1,POD_Z),
        'atrium_view': (5,L1+.2,-9),
        'balcony_view': (9,L2+.2,-4),
        'level1_view': (4,L1+.2,-2),
        'level2_view': (-2,L2+.2,-8),
        'level3_view': (0,L3+.2,-9),
        'shaft_view': (4,L1+.4,0),
        'bridge_left_view': (-9,L1+.4,-18),
        'ledge_east': (T+LEDGE/2,L2+.2,0),
        'ledge_west': (-T-LEDGE/2,L2+.2,0),
    }
