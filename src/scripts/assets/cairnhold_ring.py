"""The Ring: Cairnhold's neutral landmark on the central mesa.

Eight angular gunmetal pylons with lit bronze seams stand around a raised
octagonal dais with a ramp on each axis. The dais is Cairnhold's central
Capture & Hold point (v2): the shared C&H tower (assets/cnh_tower.py) stands
at its centre in place of the old brazier stone, and its 12 m capture ring is
the dais itself. Original geometry.

Built at the caller's origin (the dais base, Y=0) with no rotation; the ramps
run along the map axes and the pylons stand between them, so the layout is
symmetric under the map's 180-degree rotation.
"""
import math

from assets.structure_kit import Builder

ASSET_ID = 'cairnhold-ring-v2'

DAIS_R, DAIS_H = 12.0, 1.4
RAMP_LEN, RAMP_W = 8.0, 5.0
PYLON_R, PYLON_H = 21.0, 14.0
PYLON_W, PYLON_D = 2.6, 1.8          # tangential width, radial depth at the base
LEAN = .9                            # inward lean over the pylon's height
SITE_R = 31.0                        # ground held flat this far out (pylons + one cell)

STONE, METAL, GLOW, BRONZE, DECK = 'concrete', 'trim', 'light', 'grate', 'panel'


def build(mesh):
    b = Builder(mesh, METAL, metal=METAL, glow=GLOW)
    phase = math.pi/8                 # flat dais faces on the axes
    b.prism(0, 0, DAIS_R+1.2, DAIS_R, -1.5, DAIS_H, 8, STONE, phase=phase)
    # Bronze inlay rings; the C&H tower stands at the centre (built by the
    # map script), so the dais carries no central stone of its own.
    for r in (9.5, 5.5):
        b.prism(0, 0, r, r, DAIS_H+.005, DAIS_H+.02, 16, BRONZE, solid=False)
        b.prism(0, 0, r-.35, r-.35, DAIS_H+.02, DAIS_H+.03, 16, DECK, solid=False)
    # Four ramps, one per axis, from the ground up to the dais edge.
    apothem = DAIS_R*math.cos(phase)
    saved = mesh.yaw
    for k in range(4):
        mesh.yaw = saved+k*math.pi/2
        mesh.ramp(0, RAMP_W, apothem+RAMP_LEN, apothem, 0, DAIS_H, BRONZE)
    mesh.yaw = saved
    # Pylons between the ramps.
    pylons = []
    for k in range(8):
        a = phase+k*math.pi/4
        rx, rz = math.cos(a), math.sin(a)          # radial (outward)
        tx, tz = -rz, rx                           # tangential
        cx, cz = PYLON_R*rx, PYLON_R*rz
        def corner(r, t, y, lean):
            return (cx+rx*(r-lean)+tx*t, y, cz+rz*(r-lean)+tz*t)
        lo = [corner(sr*PYLON_D/2, st*PYLON_W/2, 0, 0) for sr, st in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        hi = [corner(sr*PYLON_D*.32, st*PYLON_W*.34, PYLON_H, LEAN) for sr, st in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        for i in range(4):
            j = (i+1) % 4
            mesh.quad(lo[i], lo[j], hi[j], hi[i], METAL)
        apex = corner(0, 0, PYLON_H+1.4, LEAN+.1)
        for i in range(4):
            mesh.triangle([hi[i], hi[(i+1) % 4], apex], METAL)
        # Stone footing.
        foot = [corner(sr*(PYLON_D/2+.5), st*(PYLON_W/2+.5), 0, 0) for sr, st in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        ftop = [(x, .9, z) for x, _, z in foot]; fbot = [(x, -1, z) for x, _, z in foot]
        for i in range(4):
            j = (i+1) % 4
            mesh.quad(fbot[i], fbot[j], ftop[j], ftop[i], STONE)
        mesh.quad(*ftop[::-1], STONE)
        # Lit bronze seams on the inward face, and a bronze collar.
        for st in (-.22, .22):
            p0 = corner(-PYLON_D/2-.03, st*PYLON_W, 1.4, 0)
            p1 = corner(-PYLON_D*.32-.03, st*PYLON_W*.68, PYLON_H-.8, LEAN*(PYLON_H-.8)/PYLON_H)
            w = .09
            mesh.quad((p0[0]+tx*w, p0[1], p0[2]+tz*w), (p1[0]+tx*w, p1[1], p1[2]+tz*w),
                      (p1[0]-tx*w, p1[1], p1[2]-tz*w), (p0[0]-tx*w, p0[1], p0[2]-tz*w), GLOW, False)
            mesh.quad((p0[0]-tx*w, p0[1], p0[2]-tz*w), (p1[0]-tx*w, p1[1], p1[2]-tz*w),
                      (p1[0]+tx*w, p1[1], p1[2]+tz*w), (p0[0]+tx*w, p0[1], p0[2]+tz*w), GLOW, False)
        for y in (4.0, 9.5):
            s = y/PYLON_H
            band = [corner(sr*(PYLON_D/2*(1-s)+PYLON_D*.32*s)+sr*.06, st*(PYLON_W/2*(1-s)+PYLON_W*.34*s)+st*.06,
                           y, LEAN*s) for sr, st in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
            top = [(x, y+.35, z) for x, _, z in band]
            for i in range(4):
                j = (i+1) % 4
                mesh.quad(band[i], band[j], top[j], top[i], BRONZE, False)
        b.lamp(corner(-PYLON_D/2-1.2, 0, 6, 0), .7)
        pylons.append((cx, 0, cz))
    return {'centre': (0, 0, 0), 'dais_top': (0, DAIS_H, 0), 'pylons': pylons}
