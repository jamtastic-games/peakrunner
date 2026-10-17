"""Old Holler field structures, cleaned (asset old-holler-structures-v4).

The kit's bunker walls overlapped each other and the floor slab at their
outer faces (visible z-fighting); here they sit on the floor and meet at the
corners. The landing pad's cross marking no longer overlaps its long bars,
and the pad reports deploy slots for future player-placed items. The bridge
and lookout tower are unchanged kit assets.

v3: the bunker's ceiling is 7 m above its floor (was 5.1 m) and its back
wall has a second 6 m opening with a ramp down to the ground, so it is no
longer a one-door box. Its floor slab and each landing pad stand on a
foundation that reaches below the ground, so no edge hovers over sloping
terrain (the pads sit at a fixed height; the terrain dips up to 0.7 m under
them).

v5: every field structure is drawn by an authored model
(scripts/models/old_holler_models.py: `oh-lookout`, `oh-bunker`, `oh-pad`,
`oh-bridge`) fitted to this geometry, which stays solid and stands in for
the models where they aren't drawn (`mesh.standin`); the bridge's deck and
ramps stay drawn. The lookout tower and
the bridge are the kit's shapes, built here so their stand-in can be
recorded; the lookout's turret and sensor stay the kit's equipment models.
"""
ASSET_ID = 'old-holler-structures-v5'
B_FLOOR_TOP, B_CEIL = .4, 7.4
# The entry ramps end just under the flattened ground the bunker stands on;
# ending 2 m below it buried most of each ramp under the grass (v4).
B_RAMP_FOOT = -.1
B_OPEN_W, B_OPEN_TOP = 6.0, 5.9


def _standin(mesh, build):
    """Run `build`, recording what it adds as stand-in geometry."""
    if not hasattr(mesh, 'standin'): mesh.standin = []
    start = len(mesh.vertices)
    out = build()
    mesh.standin.append((start, len(mesh.vertices)))
    return out


def lookout(mesh, team, circuit):
    """The kit's lookout tower: footing, shaft, ring deck, posts and roof,
    then its turret on the deck and sensor on the roof."""
    def body():
        mesh.column((0, -3, 0), 7, 5, 'concrete', 8, top=5)
        mesh.column((0, 2, 0), 3.5, 20, 'panel', 8, top=2.7)
        mesh.column((0, 22, 0), 7, .8, 'trim', 8)
        for x in (-4, 4): mesh.box((x, 25, 0), (.6, 5, .6), 'panel')
        mesh.box((0, 27.5, 0), (11, .6, 8), 'concrete')
    _standin(mesh, body)
    mesh.equipment('turret', (0, 22.8, -1), team, circuit)
    mesh.equipment('sensor', (0, 27.8, 1), team, circuit)


def bridge(mesh):
    """The kit's crossing: a grated deck on cross beams with rails, ramps at
    both ends and two piers into the banks. The deck and ramps stay drawn
    (the model clads the deck's sides); the rest stands in."""
    mesh.box((0, 0, 0), (13, 1.4, 212), 'grate')
    mesh.ramp(0, 13, -114, -106, 0, .7)
    mesh.ramp(0, 13, 106, 114, .7, 0)

    def body():
        for x in (-6.5, 6.5):
            mesh.box((x, .9, 0), (.45, 1.7, 212), 'trim')
            for z in range(-96, 100, 16): mesh.box((x, 1.8, z), (.52, .1, 2), 'light', False)
        for z in (-64, 64):
            for x in (-5.5, 5.5): mesh.box((x, -24, z), (2.2, 48, 4), 'concrete')
        for z in range(-88, 100, 16): mesh.box((0, -2.4, z), (14, 3, 1.5), 'panel')
    _standin(mesh, body)


def bunker(mesh):
    _standin(mesh, lambda: _bunker(mesh))


def _bunker(mesh):
    # Floor slab on a foundation reaching 2.8 m below the floor top.
    mesh.box((0, -1.2, 0), (22, 3.2, 20), 'grate')
    wy, wh = (B_FLOOR_TOP+B_CEIL)/2, B_CEIL-B_FLOOR_TOP
    for x in (-10, 10): mesh.box((x, wy, -1), (2, wh, 18), 'concrete')
    # Back wall with a 6 m opening and a lintel over it.
    side = (22-B_OPEN_W)/2
    for x in (-(B_OPEN_W+side)/2, (B_OPEN_W+side)/2): mesh.box((x, wy, 9), (side, wh, 2), 'concrete')
    mesh.box((0, (B_OPEN_TOP+B_CEIL)/2, 9), (B_OPEN_W, B_CEIL-B_OPEN_TOP, 2), 'trim')
    mesh.box((0, B_CEIL+.5, 0), (24, 1, 22), 'panel')
    mesh.ramp(0, 18, -15, -10, B_RAMP_FOOT, B_FLOOR_TOP)
    mesh.ramp(0, B_OPEN_W, 10, 15, B_FLOOR_TOP, B_RAMP_FOOT)
    for x in (-7, 7): mesh.box((x, B_CEIL-.7, -9), (.6, .2, 2), 'light', False)
    mesh.box((0, B_CEIL-.7, 7), (4, .2, .6), 'light', False)


def landing_pad(mesh, team):
    return _standin(mesh, lambda: _landing_pad(mesh, team))


def _landing_pad(mesh, team):
    accent = 'ember' if team == 0 else 'glacier'
    mesh.column((0, -4, 0), 10.6, 4, 'concrete', 12)
    mesh.column((0, 0, 0), 11, .35, 'trim', 12)
    mesh.column((0, .35, 0), 9.5, .12, 'grate', 12)
    for x in (-4, 4): mesh.box((x, .5, 0), (.5, .06, 10), accent, False)
    mesh.box((0, .5, 0), (7.5, .06, .5), accent, False)
    mesh.box((8, 1.1, 8), (1.6, 2.2, 1.2), 'panel')
    mesh.box((8, 1.8, 7.37), (1.1, .5, .04), 'light', False)
    deck = .47
    return {'deck': (0, deck, 0),
            'deploy_slots': [(-5.5, deck, -5.5), (5.5, deck, -5.5), (-5.5, deck, 5.5), (4.5, deck, 4.5)]}
