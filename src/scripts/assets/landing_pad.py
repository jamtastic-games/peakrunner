"""Original open floating landing pad, one per team half of Tower Complex.

A 24 m square solid deck with low edge lips broken by a gap on every side,
landing markings, four marked deploy slots for future placed items (turrets,
vehicles), team trim, corner light posts, and the same tapered keel and
thruster look as the base hulls. No equipment is placed yet; `deploy_slots`
anchors only record where it will go.

Local X/Z are horizontal, Y is up, the deck top is Y=0. Markings and light
posts are render-only (non-solid) so the deck stays one flat walkable plane.
If the caller's mesh has a `lamps` list, light posts add bake samples.
"""
ASSET_ID = 'landing-pad-v1'

HALF = 12          # deck half-size: 24 m square
LIP = .5           # edge lip height above the deck
LIP_T = .5         # lip thickness
GAP = 3            # half-width of the opening in each lip
KEEL = 16          # deck underside to keel tip
SLOTS = ((-6.5, -6.5), (6.5, -6.5), (-6.5, 6.5), (6.5, 6.5))
SLOT_HALF = 1.6    # marked slot square half-size

DECK, METAL, HULL, HAZARD, GLOW = 'panel', 'trim', 'concrete', 'grate', 'light'


def build(mesh, team):
    if team not in (0, 1): raise ValueError('team required')
    accent = ['ember', 'glacier'][team]
    lamps = getattr(mesh, 'lamps', None)

    def decal(x0, x1, z0, z1, mat, y=.02):
        # Flat up-facing render-only quad lying just above the deck.
        mesh.quad((x0, y, z0), (x0, y, z1), (x1, y, z1), (x1, y, z0), mat, False)

    def hull(bottom, top, y0, y1, mat, solid=True):
        bx, bz = bottom; tx, tz = top
        for s in (-1, 1):
            mesh.quad((-bx, y0, s*bz), (bx, y0, s*bz), (tx, y1, s*tz), (-tx, y1, s*tz), mat, solid)
            mesh.quad((s*bx, y0, -bz), (s*bx, y0, bz), (s*tx, y1, tz), (s*tx, y1, -tz), mat, solid)

    def thruster(x, y, z, r):
        mesh.column((x, y-r*1.4, z), r*.75, r*1.4, GLOW, 10, top=r, solid=False)

    # Deck: one solid slab, top at Y=0.
    mesh.box((0, -.5, 0), (2*HALF, 1, 2*HALF), DECK)
    # Edge lips with a gap centred on each side. The X-side runs stop short of
    # the corners so no two lip boxes overlap.
    for s in (-1, 1):
        for a, b in ((-HALF, -GAP), (GAP, HALF)):
            mesh.box(((a+b)/2, LIP/2, s*(HALF-LIP_T/2)), (b-a, LIP, LIP_T), METAL)
        for a, b in ((-HALF+LIP_T, -GAP), (GAP, HALF-LIP_T)):
            mesh.box((s*(HALF-LIP_T/2), LIP/2, (a+b)/2), (LIP_T, LIP, b-a), METAL)
    # Team trim band around the deck edge.
    for s in (-1, 1):
        o = HALF+.03
        mesh.quad((-HALF, -.85, s*o), (HALF, -.85, s*o), (HALF, -.25, s*o), (-HALF, -.25, s*o), accent, False)
        mesh.quad((s*o, -.85, -HALF), (s*o, -.85, HALF), (s*o, -.25, HALF), (s*o, -.25, -HALF), accent, False)

    # Markings: hazard landing square, team centre disc, deploy slot outlines.
    k, w = 9.5, .5
    decal(-k, k, -k, -k+w, HAZARD); decal(-k, k, k-w, k, HAZARD)
    decal(-k, -k+w, -k+w, k-w, HAZARD); decal(k-w, k, -k+w, k-w, HAZARD)
    mesh.column((0, .01, 0), 3.2, .02, accent, 16, solid=False)
    slots = []
    for sx, sz in SLOTS:
        h, t = SLOT_HALF, .22
        decal(sx-h, sx+h, sz-h, sz-h+t, METAL); decal(sx-h, sx+h, sz+h-t, sz+h, METAL)
        decal(sx-h, sx-h+t, sz-h+t, sz+h-t, METAL); decal(sx+h-t, sx+h, sz-h+t, sz+h-t, METAL)
        mesh.column((sx, .02, sz), .18, .12, GLOW, 8, solid=False)
        slots.append((sx, 0, sz))

    # Corner light posts standing on the lips.
    for sx in (-1, 1):
        for sz in (-1, 1):
            x, z = sx*(HALF-LIP_T/2), sz*(HALF-LIP_T/2)
            mesh.column((x, LIP, z), .12, 1.1, METAL, 6, solid=False)
            mesh.column((x, LIP+1.1, z), .2, .25, GLOW, 8, solid=False)
            if lamps is not None: lamps.append((mesh.point((x, LIP+1.3, z)), .5))

    # Underside: metal skirt, tapered keel, central thruster, four engines.
    top = (HALF-.2, HALF-.2)
    mid = (HALF*.62, HALF*.62)
    hull(mid, top, -1-KEEL*.3, -1, METAL)
    hull((2, 2), mid, -1-KEEL, -1-KEEL*.3, HULL)
    thruster(0, -1-KEEL, 0, 2)
    for sx in (-1, 1):
        for sz in (-1, 1):
            x, z = sx*8.6, sz*8.6
            mesh.column((x, -2.6, z), 1.3, 1.6, METAL, 8, top=1.0)
            thruster(x, -2.6, z, .9)

    return dict(deck=(0, 0, 0), deploy_slots=slots, keel_tip=(0, -1-KEEL, 0),
                edge_gaps=[(0, 0, -HALF), (0, 0, HALF), (-HALF, 0, 0), (HALF, 0, 0)])
