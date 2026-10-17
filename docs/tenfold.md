# Tenfold Tower

Key `tenfold`. A duel server's lobby (`docs/duels.md`): a round tower of ten
floors on a plaza, every floor a blade zone (`docs/blade.md`), joined by one
spiral stair round a glowing core; rolling hills round the plaza to ski while
you wait. Branch `duel/arenas` (2026-10-14), released in `.20261014.7`.

The brief (2026-10-14): a really tall tower of ten stories, a staircase
inside with different platforms, and every floor unique, starting from the
test levels to customize later; and a concept sheet of ten themed floors
round a central spiral (a cargo bay, a neon alley, ruins, a laboratory, a
greenhouse, a foundry, a library, a crystal cavern, a throne hall and a
domed top), with steps, pillars, walls to jump over, water to slow down in
and lava to die in.

Original geometry (`scripts/assets/tenfold_tower.py`), original procedural
terrain (`tenfold_terrain.py`) and an original material kit
(`tenfold_materials.py`); nothing from any other game is read, sampled or
fitted.

## The tower

- Round, 38 m across (wall 18 to 19 m from the axis), ten floors 8 m apart
  from the plaza's level (100 m), 72 m to the crown and a canopy 9 m over
  it. Slit windows light every floor (too narrow to pass; solid to bodies
  and shots). Three doors on the ground floor (east, north and west); the
  crown is open over a parapet, so it can be reached by jet from outside.
- **The stair**: one ramp round a 2.4 m core, 2.8 m wide, a full storey a
  turn (315°, about 21° at its middle, 31° at the core), with steps drawn on
  it and a rail on its outer edge. Every floor's **landing** is at the same
  place (+Z); a rail runs round each floor's hole but there. The ground
  floor's flight is closed underneath.
- **Floors** are a grid of 8 rings by 48 sectors (`Plan`): each cell a height
  (raised platforms, 0.3 m steps a body walks up, pits dry or holding water
  or molten metal); one mesher makes each floor's slab from it. Then each
  floor's dressing, after the concept sheet (dressed on 2026-10-14; the
  user: "dressing the test floors from your concept sheet, one at a time"
  was agreed). Each floor has its own wall lining (`liner`), ceiling (the
  floor above's underside, `Plan.ceiling`), lamps in its own color and a
  dim ambient of its own (below):

| Floor | Name | What's there |
|---|---|---|
| 1 | Cargo Bay | low amber light from hanging work lamps; crates on pallets to jump over (1.2 m) or hide behind (2.4 m); shipping containers along the wall (one stacked), barrels set against the wall; a catwalk deck 2.1 m up along the north wall behind the stair, railed, its steps stopping short of the doors; door lanes painted and kept clear; gantry beams with hoists; dark corrugated walls |
| 2 | Neon Alley | magenta and cyan light; alley walls with lit windows, doors, AC units and pipes; holographic signs out over the alleys and hung overhead; puddles (0.25 m deep) on dark ground; balconies up two flights, railed, under awnings with lanterns; cables strung between the walls |
| 3 | Ruins | warm torchlight; ashlar walls and ceiling; broken pillars (capitals on the tall, tops knocked askew on the broken), two round arches, fallen drums to vault, broken wall stubs against the wall (clear of the raised platform's ring), rubble, a raised platform, dry pits 1.2 m deep with rubble in them; vines and moss |
| 4 | Laboratory | cool white light; glass chambers on plinths ringed in hazard lines; the energy core in containment rings, a glowing ring round the hole; consoles with sloped screens to vault; server racks; cable trays; holo displays hung over the raised bays |
| 5 | Greenhouse | daylight under a skylight of glass between ribs; glass walls on a stone base; a channel 0.95 m deep flowing round the floor (it slows a wader about a quarter) with lily pads, crossed on four railed wooden bridges; planted beds with shrubs and flowers; eight trees; hanging planters; irrigation pipes |
| 6 | Foundry | orange light; three molten pits (`lava`, 400 a second: death in a moment), their rims marked in hazard stripes, between grated walkways; two platforms riding over the middle one; great gears on the walls with smaller ones meshing; pipes with valve wheels; two piston machines; steam from five vents; a crucible hung over the middle pit |
| 7 | Library | lamplight; walls of books; rows of capped shelves with ladders against some; carpets down the aisles; reading tables with green lamps and books; a globe; the mezzanine 3 m up on bookcases, behind a balustrade, with steps; chandeliers under a coffered wooden ceiling |
| 8 | Crystal Cavern | violet and cyan light; uneven rock (0.3 and 0.6 m bumps), flat by the wall and round the crystal pillars; boulders round the walls, solid as upright stones set into the wall; stalactites (some crystal) and stalagmites; crystal pillars (solid as upright stones) and clusters lit from within; a pool |
| 9 | Throne Hall | golden light; marble walls banded in gold; two rings of columns with bases and gilded capitals; a dais 1.5 m up with steps, the throne on it; armored statues on plinths; banners; braziers; a carpet to the throne; chandeliers |
| 10 | Crown | daylight; an open deck under the canopy ring, and over it a dome of gilded ribs round an oculus, the core's beacon under it; low walls capped in gold; an inlaid star; lanterns on the posts; a telescope at the parapet |

- **Zones**: each floor is a round blade zone (`radius` 18 m; the crown's
  19 m and 10 m tall) named "Floor N · Name", with four spawns (sparring
  bots come back on theirs). The map's spawn points put one on every floor
  for each team (two of the floor's own) and eight on the plaza, facing the
  tower; in the lobby people join and come back at one of them drawn at
  random, so there is always at least one on every floor and outside.
- **The lobby flag**: the manifest's `lobby: true` makes the map host the
  duel lobby and nothing else (`map_catalog::pack_supports`), so menus,
  rotations and votes never offer it. A duel server uses it when it's
  installed (`sim::duel::lobby_map`).
- **Materials**: the kit's slots repainted (dark tiles, steel panels,
  grating, glass, wood, foliage) and fifteen layers of the tower's own after
  the sky faces (`material_layers` in the manifest: stone, marble, brass,
  crystal, neon, carpet, books, tile, crate, hazard, rust, steelblue,
  copper, gold, screen).
- **Light**: 284 lamps, most in their floor's color, baked in 23 lightmap
  pages; each floor but the crown darkened and tinted by an ambient zone
  (`Plan.ambient`, `ambient_zones`; its pits get zones of their own) and its
  lamps scaled (`Plan.gain`), so a closed floor isn't lit like the outdoors
  (`docs/map-pipeline.md` "Lighting").
- **Hung from ceilings**: lamps, cables, trays, chains and trees keep under
  the floor above's underside, which drops under that floor's pits
  (`Plan.ceiling_at`, `ceiling_ring`); the build fails if any floor's
  dressing reaches into the floor above (`under_ceiling`; the greenhouse's
  skylight ribs once showed through the foundry's molten metal).
- **The doors**: the ground floor's catwalk once stood across all three
  doors (a 1.5 to 2.4 m wall inside each); it moved to the north wall when
  the floor was dressed, and the doors open onto the floor.

## Traps

Play on the live tower found places where a player and a bot got stuck:
on the blue floor under the light over a platform, and in the first floor's
stair and platform round the outer interior. Without jets, a spot a body
can't walk or jump out of holds it until it's cut down. What a search of
the tower found (`sim::blade::tower_traps`, below), on 2026-10-15:

- **Through the wall.** Pushed off one face in a corner tighter than square
  (any prop against the round wall's flat pieces), a body landed a fraction
  of a millimetre inside the other face; a sweep can't see a face it starts
  inside, so the body went on into the wall's thickness and fell down it.
  In 40 s of wandering, a fifth to three quarters of the wanderers on each
  floor ended up there.
- **Into a step.** The same rounding at a step's edge (a crystal or a prop
  beside it), and skiing's snap down to the ground under a body's middle
  while it still overhung an upper step, sank bodies into raised cells and
  stairs: the cargo bay's catwalk stair (the stuck bot that was seen; a sparring
  bot hops when it's stuck, and holding jump skis), the library's steps,
  the cavern's rock.
- **Crevices.** The cavern's boulders leant against the wall and each other,
  and its crystal pillars against their smaller ones: faces too steep to
  stand on that hold a body up between them, where it can't jump (the blue
  floor that was reported; its lamps hang over the rock). Raised cells beside
  the rocks walled pockets narrower than a body, held up on the rims.
- **Slots.** The ruins' wall stubs stood in the 1.6 m ring behind the raised
  platform, leaving 0.6 m; the cargo bay's barrels stood 0.7 to 1.2 m off
  the wall.
- **Bots at the rim.** A sparring bot whose foe or wander point lay across
  molten metal stood at the rim: it now walks round the floor until the way
  is clear, and wanders only where it can walk straight to.

Fixed in the engine for maps with blade zones (`careful_collision`, marker
`walls1`; other maps move exactly as before, their behaviour traces
unchanged): a contact's push that would reach into a face the body touches
sets it off that face too (`MapPack::push_enters`, `settle`), a snap down
never sinks it into a wall or a riser (`snap_clear`), a body never ends a
move sunk into a face it started clear of (`unslip`), and one found a hair
inside a wall backs out (`wall_overlap`). And in the map: the cavern's
boulders and crystals are solid as upright stones (`collide_prism`; drawn
as before), set into the wall, with the build failing on a gap between
rocks, or a rock and the wall, wider than nothing and narrower than 1.2 m
(`check_rock_gaps`); the rings by the wall and the ground round each
crystal stay flat; two crystal pillars moved in from the wall; the ruins'
stubs moved to 40 degrees; the barrels stand against the wall. And a
zero-area triangle at the foot of the ground floor's stair skirt, which
baked a black lightmap chart in some builds, is a triangle.

The search: wanderers walk, turn and hop at random on every floor for 40 s
each; one that stalls tries eight ways out, jumping each way, and a spot
it can't leave by 1.5 m, or a body inside the wall, fails the test
(`nobody_gets_stuck_in_the_tower`, `PEAKRUNNER_TRAP_WANDERERS`, `_SEED`).
150 a floor and 300 a floor on a second seed found nothing after the
fixes; twenty sparring bots spar ten minutes without sticking
(`sparring_bots_never_stick`). A new floor or dressing should pass both:

```sh
PEAKRUNNER_TRAP_WANDERERS=150 cargo test --release -p peakrunner-core --lib tower_traps -- --ignored --nocapture
```

## Budget and timing

14,283 collision triangles (the tower's own suite holds it under 15,000);
85,150 drawn. 28 people walking and swinging in the tower: about 1.4 ms a
tick on average, 1.8 ms at worst (`sim::blade::tower_load`, 2026-10-15;
1.1 ms without the careful collision).

## Build and test

```sh
python3 scripts/build-tenfold.py [OUTPUT] [--no-bake]     # Python 3 with scripts/requirements.txt
python3 scripts/test-tenfold.py
python3 scripts/scan-zfighting.py assets/maps/tenfold
```

The suite checks the blade floors and their spawns (grounded, room round
them), the plaza spawns (`spawn_checks`), every landing and the stair's
slope, the liquids in their pits, the collision budget and z-fighting, the
materials, and byte-identical unbaked rebuilds; two baked builds are
identical too. The z-fighting scan reports no mixed-material spots. The
core's tests walk the spiral and fight on the floors (`docs/blade.md`).

## Open

- Every floor is dressed once from the concept sheet; its layout is still
  the test level's (the ground floor's catwalk moved, for the doors, and
  the trap fixes above).
  More per floor may come later.
- The outside of the tower is still plain panels and bands.
- The browser build doesn't embed it (it plays no duels).
- Careful collision is the tower's (maps with blade zones). The same
  rounding can put a body into a wall on any map, where jets get it out;
  using it everywhere would change the approved CTF movement in corners
  (its traces move), so it stays on blade maps for now.
