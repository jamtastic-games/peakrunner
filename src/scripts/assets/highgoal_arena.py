"""Highgoal's arena, one team's half: a long wall, the end wall, the raised
goal, floor markings and the spawn line. Built twice, the second time rotated
180 degrees about the centre, so the arena is point-symmetric.

The goal is a glowing team slab on two stone pillars, its top
RAISED_GOAL_HEIGHT (7 m) above the sand: the football jet alone only just
gets you up there; a jump and jet together clears it easily. The end zone is
a small sphere on the slab, so a carrier has to land up there (or skim across it).

Local frame: origin at the arena centre on the map datum (y absolute), local
-Z is this team's own end. All geometry is original. Walls, pillars and the
slab are solid; markings, trim and glow are render-only.
"""
import math

from assets.structure_kit import Builder
from assets.highgoal_terrain import FLOOR_Y, HALF_W, HALF_L

ASSET_ID = 'highgoal-arena-v1'
GOAL_HEIGHT = 7.0           # must match football::RAISED_GOAL_HEIGHT
GOAL_Z = -85.0
ZONE_R = 4.0
SLAB = (15.0, .7, 8.0)      # width (x), thickness, depth (z)
PILLAR_X, PILLAR_W = 6.3, 2.4
WALL_H, WALL_T = 32.0, 2.0
KICKOFF_Z = GOAL_Z+.35*(-2*GOAL_Z)
LINE_Y = FLOOR_Y+.05
PAINT = 'grate'


def flat(mesh, x0, x1, z0, z1, y, mat):
    x0, x1 = sorted((x0, x1)); z0, z1 = sorted((z0, z1))
    mesh.quad((x0, y, z0), (x0, y, z1), (x1, y, z1), (x1, y, z0), mat, False)


def annulus(mesh, cx, cz, r, width, y, mat, segments=40):
    for i in range(segments):
        a, b = math.tau*i/segments, math.tau*(i+1)/segments
        ring = lambda r, t: (cx+r*math.sin(t), y, cz+r*math.cos(t))
        mesh.quad(ring(r-width/2, a), ring(r+width/2, a), ring(r+width/2, b), ring(r-width/2, b), mat, False)


def build(mesh, team, centre_marks):
    accent = 'ember' if team == 0 else 'glacier'
    b = Builder(mesh, accent, metal='trim', glow='light')
    top = FLOOR_Y+GOAL_HEIGHT

    # Walls: the long wall along local +X (full length) and this end wall,
    # abutting it. Carved tiles inside, a pale base course, a basalt cap and
    # a lamp strip along the top.
    x0 = HALF_W
    b.wall(x0, x0+WALL_T, -(HALF_L+WALL_T), HALF_L+WALL_T, FLOOR_Y-1, FLOOR_Y+WALL_H, 'concrete')
    b.wall(-HALF_W, HALF_W, -(HALF_L+WALL_T), -HALF_L, FLOOR_Y-1, FLOOR_Y+WALL_H, 'concrete')
    b.face_quad('x', x0, -1, -HALF_L, HALF_L, FLOOR_Y, FLOOR_Y+1.6, 'panel', off=.04)
    b.face_quad('z', -HALF_L, 1, -HALF_W, HALF_W, FLOOR_Y, FLOOR_Y+1.6, 'panel', off=.04)
    mesh.box((x0+WALL_T/2, FLOOR_Y+WALL_H+.25, 0), (WALL_T+.3, .5, 2*HALF_L+2*WALL_T+.3), 'trim', False)
    mesh.box((0, FLOOR_Y+WALL_H+.25, -(HALF_L+WALL_T/2)), (2*HALF_W, .5, WALL_T+.3), 'trim', False)
    mesh.box((x0-.12, FLOOR_Y+WALL_H-1.5, 0), (.2, .25, 2*HALF_L-2), 'light', False)
    mesh.box((0, FLOOR_Y+WALL_H-1.5, -HALF_L+.12), (2*HALF_W-2, .25, .2), 'light', False)
    for z in range(-100, 101, 20):
        b.lamp((x0-.8, FLOOR_Y+WALL_H-2, z), .9)
    for x in (-40, -20, 0, 20, 40):
        b.lamp((x, FLOOR_Y+WALL_H-2, -HALF_L+.8), .9)
    # Team banners on the end wall either side of the goal.
    for s in (-1, 1):
        b.face_quad('z', -HALF_L, 1, s*30-8, s*30+8, FLOOR_Y+14, FLOOR_Y+26, accent, off=.06)

    # The raised goal: tapered basalt pillars and a glowing glass slab with
    # lit edges, a chalk ring round the scoring zone on top.
    for s in (-1, 1):
        px = s*PILLAR_X
        b.prism(px, GOAL_Z, PILLAR_W*.95, PILLAR_W*.62, FLOOR_Y-.3, FLOOR_Y+1.2, 4, 'trim', phase=math.pi/4, cap=False)
        b.wall(px-PILLAR_W/2, px+PILLAR_W/2, GOAL_Z-PILLAR_W/2, GOAL_Z+PILLAR_W/2, FLOOR_Y-.5, top-SLAB[1], 'trim')
        b.face_quad('z', GOAL_Z+PILLAR_W/2, 1, px-.25, px+.25, FLOOR_Y+1.5, top-1.2, 'light', off=.05)
    w, t, d = SLAB
    mesh.box((0, top-t/2, GOAL_Z), (w, t, d), accent)
    for sz in (-1, 1):
        mesh.box((0, top-t/2, GOAL_Z+sz*(d/2+.09)), (w+.2, .22, .12), 'light', False)
    for sx in (-1, 1):
        mesh.box((sx*(w/2+.09), top-t/2, GOAL_Z), (.12, .22, d), 'light', False)
    annulus(mesh, 0, GOAL_Z, ZONE_R-.3, .25, top+.03, PAINT, segments=28)
    for x in (-5, 0, 5):
        b.lamp((x, top-1.2, GOAL_Z), .7)
        b.lamp((x, top+1.0, GOAL_Z), .5)

    # Floor markings: a team ring under the goal and a line across the arena
    # in front of it; the centre line and circle once.
    annulus(mesh, 0, GOAL_Z, 9.0, .8, LINE_Y, accent)
    flat(mesh, -HALF_W+2, HALF_W-2, GOAL_Z+14.8, GOAL_Z+15.2, LINE_Y, PAINT)
    if centre_marks:
        flat(mesh, -HALF_W+2, HALF_W-2, -.25, .25, LINE_Y, PAINT)
        annulus(mesh, 0, 0, 10, .5, LINE_Y, PAINT)

    # Spawn line between the goal and the end wall, facing up the arena.
    spawn_points = [(x, FLOOR_Y+1.2, z, math.pi) for z in (-102.0, -96.0) for x in (-30.0, -18.0, 18.0, 30.0)]
    return dict(flag=(0, top, GOAL_Z), end_zone=(0, top+1.2, GOAL_Z), kickoff=(0, FLOOR_Y+1.0, KICKOFF_Z),
                spawn=spawn_points[0][:3], spawn_points=spawn_points)
