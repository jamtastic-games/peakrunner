# Ashveil

A volcanic Team Deathmatch map (key `ashveil`), with CTF sides and Capture
& Hold. A caldera under an orange haze: a spired temple on a low dome in
the middle with a cathedral dug under it, two lava tubes and two vents
leading down into it, a breached cinder cone in the north, a spatter cone
in the northeast, an old lava flow from the east wall, a fissure across the
south, lava ponds with hot air rising over them, basalt outcrops,
volcanic necks, rock arches, and the bones of three giant many-legged
beasts from before the caldera. Team 1's flag stands high in a hollow tower of three
stacked skulls in a lava moat in the west; team 0's on the flat top of a
mesa of basalt columns in the east. Two long lava valleys sweep round the
sides.

**Everything in it is our own**, and every part builds from the tree. Its
first version (2026-10-08 to 10-16) rebuilt a square of layout from
private reference material at the maintainer's choice; on 2026-10-16 the
maintainer had that square replaced ("redesign it please. show me
something that's better though, no worse"; of the cathedral: "i'd still
like the cathedral, but maybe make it different/better"). What carried
over is what was already ours: the skull tower, the valleys, the post and
its lightning, the hall that burns its holders, the warning, the hot air,
the lava rules and the textures.

Builder: `scripts/build-ashveil.py` (definition `maps/ashveil.json`):
the ground `scripts/assets/ashveil_land.py`, the valleys and the tower's
site `ashveil_ring.py`, the temple `ashveil_temple.py`, the cathedral
`ashveil_undercroft.py`, the mesa `ashveil_mesa.py`, the skull tower
`ashveil_tower.py`, the field's rock and the far land `ashveil_scatter.py`,
the forms they're built of `ashveil_shapes.py`, the surfaces
`ashveil_materials.py`, the terrain shade `ashveil_shade.py`. Tests:
`scripts/test-ashveil.py` (on the shipped pack by default). In core,
`moveables_zones_items_and_units` (`crates/core/tests/features.rs`) covers
the updraft, `crates/core/tests/lava_ground.rs` the lava ground,
`shots_burst_on_lava_and_splash_it` the splash and
`smoke_sources_load_and_bad_ones_are_refused` the smoke. Bot routes:
`PEAKRUNNER_NAV_PACK=assets/maps/ashveil cargo test --release -p
peakrunner-core --lib nav_routes_on_a_map_folder -- --ignored --nocapture`
(every spawn reaches the enemy flag).

## Layout

World frame: metres, X east, Y up, Z south; the tile's corner at the
origin; bearings are degrees from +X toward +Z.

- **Scale and the fence:** a 652.8 m tile (`terrain_step` 2.56 m). The
  play area is the square x and z 24 to 629 (`ring.fence`) under an
  invisible ceiling at 240 m. The caldera's walls rise toward the fence
  to a crest just past it, then drop into a lava sea that runs to the
  horizon (`rim`: `crest`, `drop`, `sea` at 31 m); basalt spires 110 to
  210 m tall and five giant rock arches stand in it round the island, and
  beyond them a ring of peaks and two volcanoes. All of it is backdrop,
  drawn and lit live with nothing to hit (`sky.drawDistance` 2000 m); the
  sea is drawn only, never lava ground.
- **The ground** (`ashveil_land.heights`, `land` in the definition): a
  caldera floor at about 44 m, rising toward its walls and toward both
  bases, rolling and roughened by noise on warped coordinates, with:
  - two arcs of the ring fault's **benches** north and south, where the
    ground steps up 6 to 7 m outward, smoothed into ramps where the lanes
    cross (`benches`);
  - **older hills** the flows went round (`hills`), whose dust is lighter,
    and patches of hummocky lava (`hummocks`);
  - the **dome** under the temple (flat at 56 m out to 30 m, its flanks
    smooth for skiing) and the **mesa's shelf** in the east (58 m);
  - the **north cone** (40 m over its foot, gullied, its crater breached
    toward the southeast) and its **breach flow**, a lobe of cooled lava
    running down to a pond on the dome's north flank, with a stream of
    lava down its middle;
  - the small **east cone** with a lava crater;
  - the **tongue**: an old flow from the east wall, flat-topped with
    levees and pressure ridges, a steep front at its west end: a lane to
    ski on;
  - the **fissure** across the south: a trench of lava between spatter
    ramparts, with three gaps to cross on foot, two rock arches over it
    and spatter cones along it;
  - six **ponds** of lava: on the dome's north and south flanks, one
    toward each base, and two widened stretches of the fissure;
  - the **lava tubes'** trenches and their collapse pits (`tubes`), the
    trench's floor flat 1.6 m past the tube's walls on each side.
  Slopes past the angle of repose slump (33 degrees); the ground under
  every spawn and field station is levelled for its footing.
- **The lanes:** the flags are 356 m apart along the middle, over
  the dome; a north lane past the cone and the tongue's front; a south
  lane along the fissure; the valleys round the sides (`field.lanes`;
  loose rock lies thinner along them).
- **The temple** (`ashveil_temple.py`) at (283, 311), half way between
  the flags: an octagonal platform of dark cut stone at 57 m, 52 m across,
  three steps up from the dome all round and open on every side. Eight
  obsidian spires on stepped plinths stand on its corners' bearings 21 m
  out, 32 m tall, joined 14 to 20 m up by a ring of pointed arches; from
  each a flying buttress springs down and out to a pier on the dome. A
  cage of eight bronze ribs rises from short piers over the middle to a
  crown ring with a bronze lantern and a spike, an ember hanging under
  it; the well opens under it, and two vents near the northwest and
  southeast faces. Braziers burn on the diagonals; bronze rings and a
  sixteen-point star are inlaid in the floor.
- **The cathedral** (`ashveil_undercroft.py`, our own design): an
  octagon 37 m across under the temple's floor.
  - **The gallery** runs right round it at the tubes' level (39 m), 5 m
    deep, with a peaked rail; the **nave** lies 10 m under it, and over it
    all an octagonal ribbed dome springs from the walls 7.5 m over the
    gallery and rises to the well's ring at 55.4 m, its basalt ribs
    carried on engaged columns in the walls' corners.
  - **The ways in:** the two lava tubes through tall bronze-framed doors in
    the west and east walls, onto the gallery; two **vents**, shafts 4 m
    across from the temple's floor, onto the gallery through low doors in
    the northwest and southeast walls; and **the well** from the middle of
    the temple's floor, 4.2 m across, dropping straight onto the post.
  - **The stairs:** from the west door's end of the gallery along the
    northwest wall to a landing in the corner, then along the north wall
    to the nave; the same turned half round from the east door. Each side
    has its own way down.
  - **The chapels:** four, cut into the gallery's foot on the west,
    southwest, east and northeast: tombs with a skull at their heads in
    the west and east ones, shelves of skulls between rows of stacked bones
    in the others. Lava runs in channels across the nave from a font in
    each chapel to the ring of fire round the post; lancets of lava glass
    burn in the four walls with no door; lantern rings hang from the dome.
  - **The post** (the `sanctum` control point, `maps/ashveil.json`
    `points`): an iron obelisk with burning slits on a low dais in a ring of
    fire. A touch takes it (1 s inside 3.2 m). Its holder's color glows on
    its crown and on the temple's eight spire tips, and the spires strike
    lightning at enemies flying within 130 m that one of them can see,
    draining 30 energy and 15 health a second (energy damage). A jets-only
    strike was tried on 2026-10-09 and dropped: the full strike is more
    devastating and makes people fight for the center. With the temple half
    way between the flags, they stand 180
    and 177 m off, out of reach (before, 147 and 217). In CTF it's the only
    point that runs.
  - **The hall burns its holders** (`cook`, since 2026-10-09; one team
    can't control the inside and the outside at the same time: the inside
    welcomes the other team while the holders fight them outside): every member of the team holding the post burns 20 health a
    second (fire damage) anywhere in the hall, floor to dome and wall to
    wall, the gallery and stairs included (the builder takes the box from
    `ashveil_undercroft.build`'s `hall`); the other team is safe in it. So
    the post's holders guard it from outside, and whoever takes it has to
    get out: a light armor lasts 5 s in there, a heavy 10.
  - **The warning is carved, not shown:** no HUD line. Dark stone tablets
    with tarnished bronze letters (`ashveil_undercroft.tablet`, our own
    stroke capitals, `GLYPHS`) hang where only someone who stops will read
    them (meant to be read only by a studious adventurer): inside, on a side wall of the southwest chapel, "KEEPERS
    OF THE FLAME, BEGONE. / THIS HALL CONSUMES ITS OWN / AND SPARES THE
    STRANGER."; outside, on the wall of each tube's passage under the
    temple, "BEYOND, THE FLAME BURNS / ALL WHO HOLD IT. / THE STRANGER
    ENTERS UNHARMED." (3.8 by 1.1 m, letters 15 cm tall; not solid).
- **The lava tubes:** from a collapse pit on each side (west at
  (222, 336), east at (345, 288)), a tube 7 m wide and 6.5 m high runs
  under the dome's flank to the cathedral's door. Its floor is the trench
  cut in the ground; its roof is a slab whose top follows the ground the
  tube was cut from; inside, a rough vault of rock, lanterns and lava
  seeping at the foot of its walls. Under the temple's floor the
  cathedral's passages carry them to the doors (`UC.PASSAGE`). The cut
  cells (`holes`) all lie under the temple's floor.
- **The mesa** (`ashveil_mesa.py`): team 0's base at (452, 250), a butte of
  hexagonal basalt columns 41 m across on the shelf. The flag stands on an
  open altar in the middle of its flat top, 16 m up, with nothing over it
  or within 4 m (`test_the_mesa_flag_stands_in_the_open_on_top`) and
  braziers round it; toward the temple the columns step down in terraces of
  1.6 m (a giant's staircase); on the other sides they stand sheer, a rank
  of taller pipes behind the top and broken stumps at the foot. Under the
  south side the lower columns broke away: a grotto 8.5 m high whose roof
  is the columns' ends, open to the south and onto the staircase in the
  west, holds the base's two stations. Four team 0 spawns stand at its
  east foot.
- **Capture & Hold:** three points: the post, the East Altar round the
  mesa's flag (9 m) and the Skull Gate in the tower's forecourt (8 m).
- **The field** (`ashveil_scatter.py`, `field`): basalt outcrops of
  columns, five volcanic necks (plugs of columns 20 to 30 m tall the
  softer rock wore away from), loose rock in clusters (crags, obsidian
  shards, boulders) kept off the spawns, stations, lava and tubes and
  thinner on the lanes, 44 small lava breakouts glowing in hollows of the
  floor (flows; away from the lanes), steam rising from vents by the
  craters and the fissure, and plumes on the caldera's rim.
- **The beasts** (`ashveil_beasts.py`, `beasts`; since 2026-10-16: more
  arches, as skeletons of lost beasts from long ago): the bones of giant many-legged creatures half
  sunk in the floor, solid, kept clear of spawns, stations and loose rock:
  - **the centipede** in the north between the cones: a spine of plated
    vertebrae 120 m long dipping in and out of the ash, its legs bowing up
    13 m and down into the ground in a row of arches either side, some
    broken at the knee, the mandibles of its head crossing in front;
  - **the stalker** in the southeast, on team 0's side: a mantis-like giant
    fallen on its belly facing the temple, its four pairs of walking legs
    standing as arches about 30 m tall, its two scythes raised, its abdomen
    a row of plates sinking toward the tail;
  - **the ribcage** on team 1's way out of the skull tower: nine pairs of
    ribs meeting 15 m up under a sagging spine, a bone tunnel 46 m long that
    the tower's bridge leads into and the lane to the temple runs through.
- **Rock arches:** seven natural arches in the field (`rock_arches`, some
  across the lanes to fly through) besides the fissure's two.
- **Lava:** all of it that shows burns.
  - **The pools:** eight lava volumes, the two craters and the six ponds,
    burn 150 a second; their footprints come from the ground they lie in.
  - **The flows:** the fissure's lava, the breach's stream and the
    breakouts draw with
    the lava shader, cooled to orange (light layer -5.5, `src/map.wgsl`),
    a molten core under drifting crust cooling to dark crust at the edge
    (the builder puts each vertex's share of the way out in the lightmap
    uv). The valleys' rivers and the tower's moat are full heat.
  - **Lava ground** (`lava.bin`, `maps/ashveil.json` `lava_ground`): the
    builder rasterizes the pools, the moat, the rivers and the flows over
    the terrain cells (2.56 m) and marks every cell where lava shows over
    half of it. The pools, the moat and the rivers are skim ground burning
    150 a second; the flows burn 60 a second, so a quick crossing costs
    about 15. No lava ground goes under the cut, under a floor at or just
    over the lava, or within 4 m of a spawn, a flag or a station. Bots
    path round it.
  - **Shots** burst on the lava instead of sinking, and a burst in or on
    it throws a splash that burns up to 30 more out to 6 m, credited to
    the shooter (`World::lava_splash`).
- **Hot air:** an `updraft` zone over each pool, 28 m tall with lift 30.
  It lifts a jumping player clear of the lava, and they settle about 23 m
  up: out of the craters, and from the dome's ponds onto the temple.
  Embers rise over each pool and the air shimmers there.
- **The skull tower** (`ashveil_tower.py`, `tower`): team 1's base at
  (100, 375), facing 10° north of east toward the temple, on a stone
  plinth 36 m in radius, its top at 58.4 m, ringed by a lava moat (to 50 m
  out, skim lava). Three ways cross it: a grate bridge to the east and a
  stone causeway at each side gate (96° either side of the front), where
  the side valleys end and their rivers pour into the moat under the
  causeways. A 1.3 m parapet round the plinth and a stone rim 1.3 m over
  the far bank (the bank level for 12 m past the moat) close the rest,
  open exactly as wide as the bridge and the causeways (`PARAPET_GAPS`,
  `RIM_GAPS`), and the bridge and causeways have curbs. The parapet, the
  rim and the curbs have peaked tops nobody can stand on: bots plan drops
  off any ledge they can stand on, and from flat tops they planned them
  straight into the moat (in a 20-minute bots-only match, about 35 moat
  deaths before, 7 after, most of those attackers falling from jets up
  the face);
  - three skulls stacked on one axis, tapering upward (46, 40 and 28 m
    wide) and nesting, each deep in the crown of the one below and looking
    a little down, merged into one hollow shell of bone 3 m thick, about
    105 m to the top skull's broken crown (a rusted chimney smokes over it
    to 128 m). The faces are chunky and faceted: a brow shelf over big
    round sockets, box cheeks and muzzle, a triangular nose, a bar of
    teeth;
  - inside is one cavity: the gate hall in the lowest skull opens up into
    the cavern of the middle one, and a neck leads up into the top one's
    room, open to the sky. The lowest skull's open jaw is the gate, under
    its fangs; every eye socket and nose is a hole through the shell. The
    sockets are dark, the middle skull's with fire deep in them (light
    layer -5.25, `map.wgsl` `fire`: flames climbing the walls, lit by
    nothing, not lava ground);
  - no stairs, ramps or scaffolding: bone shelves on the cavity's walls, at
    most 18.5 m apart in height (15.5, 18.5, 36.5, 47.2 at the eyes, 47.5,
    66, 77.5, 81 and 99.5 m over the plinth), with fires on them, lead from
    the hall floor to the crown (`test_tower_shelves_climb_a_jet_at_a_time_and_meet_the_wall`);
  - outside, only bone and fire: patches of half-buried skulls on the
    sides and back (the faces stay bare), rows round the foot, long bones
    and three seams of vertebrae set into the shell, long-bone buttresses,
    vertebra pillars with fires on top either side of the forecourt,
    sandbags, stakes along the moat;
  - two shared stations, on the hall floor and on the middle skull's south
    eye ledge; four team 1 spawns on the hall floor.
  - Bots climb in: from every spawn there's a route to the enemy flag. A
    bot carrying the flag down out of the eye has no planned drop (the
    engine plans drops only to the few nearest floors) and steers straight
    for home off the ledge, routing on from the ground.
  - The layout was revised on 2026-10-08 from proposals by two other
    assistants given the maintainer's concept art and these constraints.
- **Spawns:** sixteen a side. Team 1: the tower's four on its hall floor
  and twelve in the west half; team 0: the mesa's four and twelve in the
  east half. The builder picks each team's twelve on gentle ground
  between 45 and 230 m from its flag and at least 50 m nearer it than the
  other's, 24 m apart, away from lava, the tubes, the outcrops, the
  stations and the valleys' floors, facing the temple, and levels the
  ground under each (`spawn_rules`). Every spawn passes
  `spawn_checks.problems` (6 m clear ahead, the same floor for a 4.5 m
  walk).
- **Stations:** shared booths, each with a team-0 and a team-1 entry at
  the same use point (the engine ties a station to a team; an enemy can
  still disable a team's entry): four in the field (`stations`), the
  mesa's two in its grotto and the tower's two. A booth faces the temple
  unless rock stands within 6 m of its front; then it turns to its
  clearest side.
- **CTF flags:** team 0 on the mesa's altar at (452, 74.4, 250); team 1 in
  the skull tower's middle skull, on the ledge in its north eye, 47 m over
  the plinth (106 m), open to the face and to the cavern behind it
  (`test_tower_flag_stands_in_its_eye_open_to_the_face_and_the_cavern`).

## Look

- **Sky:** the sun is low in the west-southwest, about 24 degrees up
  (`sun`).
- **Haze:** orange height fog and distance fog.
- **Sky dome:** a procedural sky, dark rust at the zenith and bright amber
  at the horizon.
- **Ground:** rust dust, basalt on faces steeper than about 30 degrees,
  cooled crust round the lava, and dark ash on the flows, the cones, the
  valleys' floors, round the tubes and in drifts elsewhere; the old hills
  keep their dust.
- **Lighting:** the temple, the mesa and the booths bake with the sun and
  their braziers; the cathedral, the tubes and the tower bake on pages of
  their own, lit by their fires and lanterns (`lightmap_bake`), all warmed
  by `light_tint` since the shared baker lights with a neutral sky. Rock
  is lit live. The terrain shade map is baked at 0.64 m texels
  (`ashveil_shade.py`).
- **The tower's surfaces:** layers of Ashveil's own after the kit's
  (`ashveil_materials.EXTRA`): `bone` outside (off-white, porous, grime
  run down it in short streaks, hairline cracks, pits, specks of old
  gore), `bone_grime` (the same, gray and filthier) laid over the skulls in
  broad patches about 18 m across and under brows, cheeks and jaws,
  `bone_dark` for the cavity's walls and shelves (soot-black), dark timber
  for the moat's stakes and sacking for the sandbags. The bone textures
  are even all over, with fine detail only and nothing on a grid, and span
  8 m on the tower (the kit's planar mapping spans 4), so the tile doesn't
  show; the large-scale variation comes from the patches and the light. The tower bakes on lightmap pages of its
  own, lit by its fires and tinted only a little (`tower.light_tint`), so
  its bone stays white and gray in the warm haze; everything else keeps
  the map's warm `light_tint`.

## Build and preview

Python 3 with `scripts/requirements.txt` installed (numpy, Pillow):

```sh
rm -r assets/maps/ashveil   # the builder refuses to overwrite
python3 scripts/build-ashveil.py      # -> assets/maps/ashveil
python3 scripts/test-ashveil.py
cargo run --release -p peakrunner --bin peakrunner
```

`--no-bake` skips the lightmaps (about 2 min) for a quick look; never
commit an unbaked pack. To preview a build elsewhere, build into
`<folder>/maps/ashveil` and run with `PEAKRUNNER_PACK_DIR=<folder>`.

In the game, choose Singleplayer, then Team Deathmatch (or CTF), then
Ashveil. For captures, add `QA_LOCAL=1 QA_MAP=ashveil QA_FLYCAM=...` to
`examples/launch_smoke`.
