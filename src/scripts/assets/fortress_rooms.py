"""Original enclosed circulation primitives: floor, two walls, ceiling.

Only the two end portals are intentionally open. The same geometry supplies
rendering and collision, so a stairwell cannot silently lack its visible roof.
"""
def stairwell(mesh, x, width, z0, z1, y0, y1, clearance=6):
    if width <= 0 or clearance < 3 or z0 == z1:
        raise ValueError('invalid stairwell dimensions')
    mesh.ramp(x,width,z0,z1,y0,y1)
    # Mesh.ramp's underside is 0.6 m below its top face.
    mesh.ramp(x,width,z0,z1,y0+clearance+.6,y1+clearance+.6,'panel')
    for side in [-1,1]:
        xx=x+side*width/2
        mesh.quad((xx,y0,z0),(xx,y1,z1),
                  (xx,y1+clearance,z1),(xx,y0+clearance,z0),'concrete')


def shaft(mesh, x, z, half, y0, y1, mat='concrete', thickness=.6, openings=()):
    """Hollow square vertical shaft wall, open top and bottom.

    Unlike stairwell (which closes its own ends), a shaft has no floor or
    ceiling of its own — the caller's floor/ceiling plates at each level must
    leave a matching hole so the opening stays a fall-through gap, not a door.
    `openings` is a list of (side, ya, yb) with side in '-x','+x','-z','+z':
    that whole wall face is left open between heights ya and yb.
    """
    if half <= 0 or y1 <= y0:
        raise ValueError('invalid shaft dimensions')
    t = thickness
    sides = {'-x': ((x-half-t/2, z), (t, 2*half+t*2)), '+x': ((x+half+t/2, z), (t, 2*half+t*2)),
             '-z': ((x, z-half-t/2), (2*half, t)), '+z': ((x, z+half+t/2), (2*half, t))}
    for side, ya, yb in openings:
        if side not in sides or not y0 <= ya < yb <= y1:
            raise ValueError('invalid shaft opening')
    for side, ((cx, cz), (sx, sz)) in sides.items():
        cuts = sorted((ya, yb) for s, ya, yb in openings if s == side)
        y = y0
        for ya, yb in cuts+[(y1, y1)]:
            if ya > y: mesh.box((cx, (y+ya)/2, cz), (sx, ya-y, sz), mat)
            y = max(y, yb)
