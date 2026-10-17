"""Shared building helpers for original structures on top of the kit Mesh.

Local X/Z are horizontal, Y is up. Solid pieces add collision; dressing is
render-only and stays within the 0.52 m player radius of a solid surface.
Tower Complex keeps its own inline copies of these closures because its
recorded asset hash covers that source file.
"""
import math


class Builder:
    def __init__(self, mesh, accent, interior='bark', metal='trim', glow='light'):
        self.mesh, self.accent = mesh, accent
        self.interior, self.metal, self.glow = interior, metal, glow
        self.lamps = getattr(mesh, 'lamps', None)

    def lamp(self, p, power, color=None):
        """Point sample for the offline light bake (when the mesh collects
        them), in the baker's white or an (r, g, b) `color`."""
        if self.lamps is not None: self.lamps.append((self.mesh.point(p), power) if color is None else (self.mesh.point(p), power, color))

    def wall(self, x0, x1, z0, z1, y0, y1, mat, solid=True):
        x0, x1 = sorted((x0, x1)); z0, z1 = sorted((z0, z1)); y0, y1 = sorted((y0, y1))
        self.mesh.box(((x0+x1)/2, (y0+y1)/2, (z0+z1)/2), (x1-x0, y1-y0, z1-z0), mat, solid)

    def slab(self, x0, x1, z0, z1, y_top, mat, thick=1.0):
        self.wall(x0, x1, z0, z1, y_top-thick, y_top, mat)

    def pane(self, a, b, c, d):
        """Collision-only quad: invisible glazing or a guard plane."""
        pts = [self.mesh.point(p) for p in (a, b, c, d)]
        for tri in ((0, 1, 2), (0, 2, 3)): self.mesh.collision.extend(v for i in tri for v in pts[i])

    # A "face" is an axis-aligned wall plane: axis 'z' is a plane of constant
    # z (u runs along x), axis 'x' a plane of constant x (u runs along z).
    # `into` is the side of the plane the room lies on (+1 or -1).
    @staticmethod
    def at(axis, plane, u, y):
        return (u, y, plane) if axis == 'z' else (plane, y, u)

    def face_quad(self, axis, plane, into, u0, u1, y0, y1, mat, off=.03):
        p = plane+into*off
        a, b, c, d = (self.at(axis, p, u, y) for u, y in ((u0, y0), (u1, y0), (u1, y1), (u0, y1)))
        flip = (axis == 'z') == (into < 0)
        self.mesh.quad(*((a, d, c, b) if flip else (a, b, c, d)), mat, False)

    def face_box(self, axis, plane, into, u0, u1, y0, y1, depth, mat, solid=False):
        centre = self.at(axis, plane+into*depth/2, (u0+u1)/2, (y0+y1)/2)
        size = (u1-u0, y1-y0, depth) if axis == 'z' else (depth, y1-y0, u1-u0)
        self.mesh.box(centre, size, mat, solid)

    def dress(self, axis, plane, into, runs, y0, y1, stripe=True, pilasters=True, step=6):
        """Liner, baseboard, cornice, accent stripe and pilasters over the
        solid runs of one wall face between floor y0 and ceiling y1."""
        for u0, u1 in runs:
            if u1-u0 < .05: continue
            self.face_quad(axis, plane, into, u0, u1, y0, y1, self.interior)
            self.face_box(axis, plane, into, u0, u1, y0, y0+.3, .12, self.metal)
            self.face_box(axis, plane, into, u0, u1, y1-.28, y1, .18, self.metal)
            if stripe: self.face_quad(axis, plane, into, u0, u1, y0+.7, y0+.95, self.accent, .045)
            if pilasters:
                u = math.ceil((u0+.4)/step)*step
                while u <= u1-.4:
                    self.face_box(axis, plane, into, u-.22, u+.22, y0+.3, y1-.28, .15, self.metal)
                    u += step

    def ceiling_strip(self, axis, c, u0, u1, y_ceiling, power, spacing=3):
        """A lit strip under a ceiling. axis 'z': runs along z at x=c."""
        mid, length = (u0+u1)/2, u1-u0
        if axis == 'z':
            self.mesh.box((c, y_ceiling-.035, mid), (.8, .07, length+.4), self.metal, False)
            self.mesh.box((c, y_ceiling-.07, mid), (.45, .06, length), self.glow, False)
        else:
            self.mesh.box((mid, y_ceiling-.035, c), (length+.4, .07, .8), self.metal, False)
            self.mesh.box((mid, y_ceiling-.07, c), (length, .06, .45), self.glow, False)
        n = max(1, round(length/spacing))
        for i in range(n):
            u = u0+(i+.5)*length/n
            self.lamp((c, y_ceiling-.3, u) if axis == 'z' else (u, y_ceiling-.3, c), power)

    def prism(self, cx, cz, r0, r1, y0, y1, sides, mat, phase=0.0, solid=True, cap=True):
        """Regular polygon frustum, radius r0 at y0 to r1 at y1. `phase`
        rotates the vertices (math.pi/sides puts flat faces on the axes)."""
        ring = lambda r, y: [(cx+r*math.cos(phase+i*math.tau/sides), y, cz+r*math.sin(phase+i*math.tau/sides))
                             for i in range(sides)]
        lo, hi = ring(r0, y0), ring(r1, y1)
        for i in range(sides):
            j = (i+1) % sides
            self.mesh.quad(lo[i], hi[i], hi[j], lo[j], mat, solid)
        if cap:
            for i in range(sides):
                self.mesh.triangle([(cx, y1, cz), hi[(i+1) % sides], hi[i]], mat, solid)

    def beam(self, p0, p1, thick, y0, y1, mat, solid=True):
        """A wall of the given thickness between two (x, z) points."""
        (ax, az), (bx, bz) = p0, p1
        dx, dz = bx-ax, bz-az; n = math.hypot(dx, dz)
        ox, oz = -dz/n*thick/2, dx/n*thick/2
        c = [(ax+ox, az+oz), (bx+ox, bz+oz), (bx-ox, bz-oz), (ax-ox, az-oz)]
        lo = [(x, y0, z) for x, z in c]; hi = [(x, y1, z) for x, z in c]
        for i in range(4):
            j = (i+1) % 4
            self.mesh.quad(lo[i], hi[i], hi[j], lo[j], mat, solid)
        self.mesh.quad(*hi, mat, solid); self.mesh.quad(*lo[::-1], mat, solid)

    def sloped_wall(self, x0, x1, z0, z1, bottom0, bottom1, height, mat, solid=True):
        """Box running along z whose bottom rises linearly from bottom0 at z0
        to bottom1 at z1, with a constant height above it (a sheared box)."""
        x0, x1 = sorted((x0, x1))
        b = [(x0, bottom0, z0), (x1, bottom0, z0), (x1, bottom1, z1), (x0, bottom1, z1)]
        t = [(x, y+height, z) for x, y, z in b]
        m = self.mesh
        m.quad(b[0], b[3], b[2], b[1], mat, solid); m.quad(t[0], t[1], t[2], t[3], mat, solid)
        m.quad(b[0], b[1], t[1], t[0], mat, solid); m.quad(b[3], t[3], t[2], b[2], mat, solid)
        m.quad(b[0], t[0], t[3], b[3], mat, solid); m.quad(b[1], b[2], t[2], t[1], mat, solid)
