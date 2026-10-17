"""Original Capture & Hold tower, shared by every map that places control
points (docs/capture-and-hold.md).

A tapered octagonal pylon on a low plinth, with a lit beacon crown that
reads from across the map, four waist-high cover walls on the ring's
diagonals and eight glowing marker posts on the 12 m capture ring. Players
capture by standing anywhere between the pylon and the posts, so the ring
floor stays open terrain with four clear approaches (north, south, east,
west) between the cover walls.

The origin is the ring centre on the ground. Every solid part that meets the
ground (plinth, cover walls) runs SINK metres below the origin, so the
structure never shows a hovering edge on sloped terrain; the caller levels
the terrain under the ring into a gentle plateau (tilt no more than
MAX_TILT), which the sink covers. Materials are kit names only, so each map's
own material pass gives the tower that map's finish.

If the caller's mesh has a `lamps` list, the beacon and marker posts append
bake samples to it.
"""
import math

ASSET_ID = 'cnh-tower-v1'

RING = 12.0          # capture radius (manifest `radius`)
SINK = 2.0           # below-ground depth of every ground-contact solid
MAX_TILT = 2.0       # metres of terrain rise across the plinth that SINK covers
PLINTH_R, PLINTH_H = 3.4, .5
PYLON_BASE, PYLON_TOP, PYLON_H = 2.3, 1.1, 15.0
COVER_R, COVER_LEN, COVER_H = 7.5, 4.2, 1.4
CROWN_Y = PLINTH_H+PYLON_H

HULL, METAL, GLOW, HAZARD = 'concrete', 'trim', 'light', 'grate'


def build(mesh, footing_top=0.0):
    """`footing_top`: height of the buried footing's top face. On a deck
    (grate or slab at y 0) pass a little below 0, or its top shares the
    deck's plane round the plinth's sloped foot."""
    lamps = getattr(mesh, 'lamps', None)

    def lamp(p, power):
        if lamps is not None: lamps.append((mesh.point(p), power))

    # Plinth: an octagon that reaches SINK below ground and PLINTH_H above.
    mesh.column((0,-SINK,0),PLINTH_R+.4,SINK+footing_top,HULL,8,top=PLINTH_R+.4)
    mesh.column((0,0,0),PLINTH_R+.4,PLINTH_H,METAL,8,top=PLINTH_R)
    # Pylon: tapered octagon with a mid collar and vertical glow seams.
    mesh.column((0,PLINTH_H,0),PYLON_BASE,PYLON_H*.55,HULL,8,top=(PYLON_BASE+PYLON_TOP)/2+.2)
    mid = PLINTH_H+PYLON_H*.55
    mesh.column((0,mid,0),(PYLON_BASE+PYLON_TOP)/2+.2,PYLON_H*.45,HULL,8,top=PYLON_TOP)
    mesh.column((0,mid-.4,0),(PYLON_BASE+PYLON_TOP)/2+.45,.5,METAL,8,solid=False)
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        dx, dz = math.cos(a), math.sin(a)
        # Seam strips just proud of the lower pylon's faces.
        r0, r1 = PYLON_BASE*.93+.04, ((PYLON_BASE+PYLON_TOP)/2+.2)*.93+.04
        p0 = (dx*r0, PLINTH_H+.6, dz*r0); p1 = (dx*r1, mid-.6, dz*r1)
        side = (-dz*.14, 0, dx*.14)
        mesh.quad((p0[0]-side[0],p0[1],p0[2]-side[2]),(p0[0]+side[0],p0[1],p0[2]+side[2]),
                  (p1[0]+side[0],p1[1],p1[2]+side[2]),(p1[0]-side[0],p1[1],p1[2]-side[2]),GLOW,False)
        # Buttress fins at the foot.
        fx, fz = dx*(PYLON_BASE+.9), dz*(PYLON_BASE+.9)
        mesh.box((fx,PLINTH_H+1.6,fz),(.35 if abs(dz)>abs(dx) else 1.8,3.2,1.8 if abs(dz)>abs(dx) else .35),METAL)
    # Crown: four canted vanes round a lit beacon core, and a finial.
    mesh.column((0,CROWN_Y,0),PYLON_TOP+.35,.6,METAL,8,top=PYLON_TOP+.9)
    for k in range(4):
        a = k*math.pi/2
        dx, dz = math.cos(a), math.sin(a)
        base = (dx*(PYLON_TOP+.6), CROWN_Y+.6, dz*(PYLON_TOP+.6))
        tip = (dx*(PYLON_TOP+1.5), CROWN_Y+4.2, dz*(PYLON_TOP+1.5))
        side = (-dz*.35, 0, dx*.35)
        mesh.quad((base[0]-side[0],base[1],base[2]-side[2]),(base[0]+side[0],base[1],base[2]+side[2]),
                  (tip[0]+side[0],tip[1],tip[2]+side[2]),(tip[0]-side[0],tip[1],tip[2]-side[2]),METAL,False)
        mesh.quad((base[0]-side[0],base[1],base[2]-side[2]),(tip[0]-side[0],tip[1],tip[2]-side[2]),
                  (tip[0]+side[0],tip[1],tip[2]+side[2]),(base[0]+side[0],base[1],base[2]+side[2]),METAL,False)
    mesh.column((0,CROWN_Y+.6,0),.95,3.0,GLOW,10,top=.6,solid=False)
    mesh.column((0,CROWN_Y+3.6,0),.6,.5,METAL,8,top=.2,solid=False)
    mesh.box((0,CROWN_Y+4.4,0),(.5,.5,.5),GLOW,False)
    for y in (CROWN_Y+1.2, CROWN_Y+2.6): lamp((0,y,0),1.2)
    # Four cover walls on the diagonals, open approaches on the axes.
    for k in range(4):
        a = math.pi/4+k*math.pi/2
        cx, cz = math.cos(a)*COVER_R, math.sin(a)*COVER_R
        tx, tz = -math.sin(a), math.cos(a)            # along the wall
        half = COVER_LEN/2
        pts = [(cx-tx*half-math.cos(a)*.35, cz-tz*half-math.sin(a)*.35),
               (cx+tx*half-math.cos(a)*.35, cz+tz*half-math.sin(a)*.35),
               (cx+tx*half+math.cos(a)*.35, cz+tz*half+math.sin(a)*.35),
               (cx-tx*half+math.cos(a)*.35, cz-tz*half+math.sin(a)*.35)]
        _prism(mesh, pts, -SINK, COVER_H, HULL)
        _prism(mesh, [(x+(cx-x)*.06, z+(cz-z)*.06) for x,z in pts], COVER_H, COVER_H+.12, METAL, solid=False)
    # Marker posts on the capture ring: render-only, so nothing snags a skier.
    for k in range(8):
        a = k*math.pi/4
        x, z = math.cos(a)*RING, math.sin(a)*RING
        mesh.column((x,-SINK,z),.16,SINK+1.1,METAL,6,top=.12,solid=False)
        mesh.box((x,1.2,z),(.26,.2,.26),GLOW,False)
        lamp((x,1.6,z),.25)
    return {'point': (0,0,0), 'beacon': (0,CROWN_Y+2,0)}


def _prism(mesh, pts, y0, y1, mat, solid=True):
    """Vertical prism over a convex quad footprint (counter-clockwise)."""
    for i in range(4):
        (ax,az),(bx,bz) = pts[i], pts[(i+1)%4]
        mesh.quad((ax,y0,az),(bx,y0,bz),(bx,y1,bz),(ax,y1,az),mat,solid)
    mesh.quad(*[(x,y1,z) for x,z in pts][::-1],mat,solid)
    mesh.quad(*[(x,y0,z) for x,z in pts],mat,solid)
