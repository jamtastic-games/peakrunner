"""Longfield's stadium, one team's half: end-zone ring and goal gate, field
lines, spawn line, end stand, one long-side stand with its scoreboard, and
two floodlight masts. The builder is called twice, the second time rotated
180 degrees about the field centre, so the whole bowl is point-symmetric.

Local frame: origin at the field centre on the map's datum (y is absolute),
local -Z is this team's own end. Everything is original geometry. Lines and
banners are render-only; stands, walls, gates and masts are solid so players
can land on them, and collide.
"""
import math

from assets.structure_kit import Builder
from assets.longfield_terrain import FIELD_Y, HALF_W, HALF_L, RIM_Y

ASSET_ID = 'longfield-stadium-v1'
ZONE_Z = -125.0
ZONE_R = 9.0
KICKOFF_Z = ZONE_Z+.35*250
LINE_Y = FIELD_Y+.06
PAINT = 'grate'


def flat(mesh, x0, x1, z0, z1, y, mat):
    """Up-facing render-only quad."""
    x0, x1 = sorted((x0, x1)); z0, z1 = sorted((z0, z1))
    mesh.quad((x0, y, z0), (x0, y, z1), (x1, y, z1), (x1, y, z0), mat, False)


def annulus(mesh, cx, cz, r, width, y, mat, segments=40, a0=0.0, a1=math.tau):
    for i in range(segments):
        a, b = a0+(a1-a0)*i/segments, a0+(a1-a0)*(i+1)/segments
        ring = lambda r, t: (cx+r*math.sin(t), y, cz+r*math.cos(t))
        mesh.quad(ring(r-width/2, a), ring(r+width/2, a), ring(r+width/2, b), ring(r-width/2, b), mat, False)


def front(mesh, axis, plane, facing, u0, u1, y0, y1, mat, off=.03):
    """Render-only vertical quad on a plane of constant x ('x') or z ('z'),
    facing +1/-1 along that axis."""
    p = plane+facing*off
    if axis == 'x':
        pts = [(p, y0, u0), (p, y0, u1), (p, y1, u1), (p, y1, u0)]
        if facing > 0: pts = pts[::-1]
    else:
        pts = [(u0, y0, p), (u1, y0, p), (u1, y1, p), (u0, y1, p)]
        if facing < 0: pts = pts[::-1]
    mesh.quad(*pts, mat, False)


def build(mesh, team, centre_marks):
    accent = 'ember' if team == 0 else 'glacier'
    b = Builder(mesh, accent, metal='trim', glow='light')

    # Field lines on this half: sideline, end line, a line every 25 m.
    for s in (-1, 1):
        flat(mesh, s*(HALF_W-2.25), s*(HALF_W-1.75), -(HALF_L-2), 0, LINE_Y, PAINT)
    flat(mesh, -(HALF_W-1.75), HALF_W-1.75, -(HALF_L-2.25), -(HALF_L-1.75), LINE_Y, PAINT)
    for z in range(-150, 0, 25):
        flat(mesh, -(HALF_W-1.75), HALF_W-1.75, z-.15, z+.15, LINE_Y, PAINT)
    if centre_marks:
        flat(mesh, -(HALF_W-1.75), HALF_W-1.75, -.25, .25, LINE_Y, PAINT)
        annulus(mesh, 0, 0, 12, .5, LINE_Y, PAINT)

    # End zone: team ring with a chalk inner ring, just above the lines.
    annulus(mesh, 0, ZONE_Z, ZONE_R, 1.0, LINE_Y+.02, accent)
    annulus(mesh, 0, ZONE_Z, ZONE_R-1.2, .3, LINE_Y+.02, PAINT)

    # Goal gate behind the zone: two pylons, a crossbar, a team banner and a
    # lamp strip. Wide open: nothing blocks a run into the zone.
    gz = ZONE_Z-ZONE_R-3
    for s in (-1, 1):
        mesh.column((s*12, FIELD_Y-.5, gz), .9, 15.5, 'trim', sides=8)
        mesh.column((s*12, FIELD_Y+14.6, gz), 1.3, .6, accent, sides=8, solid=False)
    mesh.box((0, FIELD_Y+14.2, gz), (25.8, 1.2, 1.2), 'trim')
    mesh.box((0, FIELD_Y+11.6, gz+.05), (20, 3.2, .25), accent, False)
    mesh.box((0, FIELD_Y+13.45, gz+.7), (22, .12, .35), 'light', False)
    for x in (-8, 0, 8):
        b.lamp((x, FIELD_Y+13.0, gz+1.2), .8)

    # Long-side stand along local +X on the bank rim: five tiers of seats in
    # four blocks, a back wall with team banners, and a canopy.
    x0 = HALF_W+57
    blocks = [(-150, -80), (-76, -4), (4, 76), (80, 150)]
    for k in range(5):
        top = RIM_Y+1.2*(k+1)
        for z0, z1 in blocks:
            b.wall(x0+3*k, x0+3*k+3, z0, z1, RIM_Y-1, top, 'concrete')
            front(mesh, 'x', x0+3*k, -1, z0, z1, top-1.2, top, 'panel')
    wall_x = x0+15
    b.wall(wall_x, wall_x+1.5, -154, 154, RIM_Y-1, RIM_Y+15, 'concrete')
    b.wall(wall_x-10, wall_x+1.5, -154, 154, RIM_Y+15, RIM_Y+15.6, 'trim')
    for z in (-110, -40, 40, 110):
        front(mesh, 'x', wall_x, -1, z-9, z+9, RIM_Y+8, RIM_Y+13.5, accent, off=.04)
    for z in range(-140, 141, 20):
        mesh.box((wall_x-5, RIM_Y+14.85, z), (6, .1, .5), 'light', False)
        b.lamp((wall_x-5, RIM_Y+14.2, z), .5)

    # Scoreboard above the back wall, facing the field, on two legs.
    sx = wall_x+3
    for z in (-14, 14):
        mesh.column((sx, RIM_Y-1, z), .8, 20, 'trim', sides=6)
    b.wall(sx-1, sx+1, -18, 18, RIM_Y+19, RIM_Y+31, 'trim')
    front(mesh, 'x', sx-1, -1, -17, 17, RIM_Y+20, RIM_Y+30, 'bark', off=.05)
    front(mesh, 'x', sx-1, -1, -17, 17, RIM_Y+30.1, RIM_Y+30.7, accent, off=.06)

    # End stand behind this end, on the rim, facing the field.
    z0 = -(HALF_L+57)
    for k in range(5):
        top = RIM_Y+1.2*(k+1)
        for u0, u1 in [(-60, -4), (4, 60)]:
            b.wall(u0, u1, z0-3*k-3, z0-3*k, RIM_Y-1, top, 'concrete')
            front(mesh, 'z', z0-3*k, 1, u0, u1, top-1.2, top, 'panel')
    b.wall(-64, 64, z0-16.5, z0-15, RIM_Y-1, RIM_Y+12, 'concrete')
    front(mesh, 'z', z0-15, 1, -24, 24, RIM_Y+5, RIM_Y+10.5, accent, off=.04)

    # Floodlight masts at this end's corners, their heads facing the field.
    for s in (-1, 1):
        mx, mz = s*118, -215
        mesh.column((mx, RIM_Y-1, mz), 1.1, 39, 'trim', sides=8)
        mesh.box((mx, RIM_Y+40, mz), (15, 7, 1.6), 'trim')
        mesh.box((mx, RIM_Y+40, mz+.95), (14, 6.2, .3), 'light', False)
        for dx in (-5, 0, 5):
            b.lamp((mx+dx, RIM_Y+40, mz+1.6), 1.2)

    # Spawn line behind the end zone, facing up the field (local +Z).
    spawn_points = [(x, FIELD_Y+1.2, z, math.pi) for z in (-150.0, -143.0) for x in (-21.0, -7.0, 7.0, 21.0)]
    return dict(flag=(0, FIELD_Y, ZONE_Z), end_zone=(0, FIELD_Y+1.2, ZONE_Z), kickoff=(0, FIELD_Y+1.0, KICKOFF_Z),
                spawn=spawn_points[0][:3], spawn_points=spawn_points)
