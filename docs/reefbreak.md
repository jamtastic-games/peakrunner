# Reefbreak

An original CTF map (key `reefbreak`; also runs Capture & Hold and both
deathmatch modes): a tropical atoll.

Everything here is our own shapes at our own scale (`docs/map-pipeline.md`
rule 0).

## Layout (world metres; flag axis Z, Ember at low Z)

The atoll's frame is point-symmetric about the centre (island, reef ring and
channels, lagoon, spits, the sandbars under the ships); the rest is each
half's own: every outer island and blue hole has a twin of its own shape and
place (`OUTER_TWIN`, `BLUE_TWIN`; the outpost island keeps its twin's shape),
and the reef's lumps and the land's detail come from each half's noise. The
halves carry as much dry land, shallows and deep water within 2% (tested).

- **Reef ring:** radius 300 m, a broad, low crest about 8 m over the sea.
  - Four shallow channels, at 55°, 125°, 235° and 305° from the flag axis.
  - The **flags** are aboard the ships (below), 720 m apart.
- **Central island:** radius 135 m, about 26 m high, with the **lighthouse**.
  - A railed gallery at 24 m is the sniping spot, reached by jetting.
  - A keeper's yard wall with two gaps gives cover.
  - Sand spits run east and west to the ring, carrying the two Capture &
    Hold points: East Spit and West Spit.
- **Water:** one sea volume covers the map, with the surface at 60 m. It
  is **skim water** (`"skim": true`, marker `water2`): players run and ski
  across the top and never go under it.
  - Its surface is ground (`terrain::support_on`), and it counts as flat for
    skiing, so there's no slope from the seabed beneath (`contact_normal`).
  - It adds no immersion, so no wading drag or buoyancy. Skiing across keeps
    its speed: 30.00 m/s after 1 s against 29.71 on dry flat ground (test
    `reefbreak_water_is_a_surface_you_ski_across_never_under`).
  - Crossing it throws spray back and to the sides, a foam cloud when fast,
    and ripple rings; landing on it splashes (`effects.rs`, `skim`).
    Droplets are alpha-blended, not emissive, so they don't bloom. Spray is
    capped at 6 new droplets per frame across all players, nearest first,
    and ripples at 24 at once.
  - Old Holler and the other maps keep ordinary water: wading slows you and
    deep water floats you.
  - The lagoon and the shelf out to the islands are ankle-deep: skiable,
    with a little water drag.
  - Blue holes and the open sea beyond radius 640 m are deep.
- **Outer islands:** a ring of them, including a C-shaped pair on the
  flanks.
- **Boundary:** sea cliffs from radius 700 m.

## Bases (`scripts/assets/reefbreak_base.py`, asset `reefbreak-ship-v5`)

Each team has a cargo frigate that **hovers 21 m over the sea** (`HOVER`)
just outside the reef, 360 m from the centre, **lying side-on** to the
atoll: its port side faces the field.

**The look is an authored, hard-surface model.** The frigate, its gangway,
its lift and its platform base are `assets/models/structures/reefbreak-ship`,
built in Blender by `scripts/models/reefbreak_ship.py` from the original
concepts' side and top views (`docs/model-pipeline.md`, "Hard-surface
structures"). Its parts:
- a chined hull with a raked wedge prow, layered chamfered plates, the
  stepped bridge, two engine pods with swept fins and hover pods;
- a station module hung under the hold and a keel under the bridge;
- 23,100 triangles and a 4096 texture.

Its team plates (the prow's deck, the bay's frames, the aft shoulders, the pods, the fin tips, the gangway's curbs, the vehicle
pad) are painted in each team's color. The earlier model it replaced
(2026-10-03) was soft at every edge; it is kept outside the repository.

- The game draws it from the map's own `models.glb` (`docs/map-pipeline.md`,
  "Authored models"). Both ships place it, one per team.
- The builder fits what plays to the model:
  - **Collision:** the hull's cross-sections are sampled from the model
    (`hull_sections`), modules included:
    - every 3 m along the prow, sparser along the midbody;
    - a pair of stations either side of each module's ends;
    - rays aimed at the section's and the modules' corners.

    The bay's roof, jambs, ledges and underside (with the porthole's
    shaft), the fins, the engine pods and nozzles, the hover pods, the
    balcony, the platform base and the gangway are their own pieces.
  - **Rooms:** lined rooms inside its skin.
  - **Cuts:** the builder cuts the model away where the rooms, their doors
    and the hatches are (`model_cuts`).
- The collision geometry also stands in for the model where models aren't
  drawn: the browser build, or `PEAKRUNNER_MODELS=0`. Hull-local
  coordinates (bow at -Z, port at -X, top floor Y=0) are turned into the
  model's frame by `HULL_TURN`.

**Three levels in places** (chosen 2026-10-03 from three layouts; the
cutaways are an artifact, "Frigate Decks"). The top level holds the bay and
the flag. The station room and balcony below it hide the stations. The
generator sits at the bottom, reached only by dropping.

- **Top (floor 0):**
  - **Bay, mid-ship:** a lined room through the hull from flank to flank
    (14 m long, 5 m high), its floor flat. The port opening (12.6 m clear)
    faces the field; the starboard one is straight across, so a capper can
    fly in one side and out the other. Its aft wall is solid. A lit
    porthole in the middle of its floor drops to the balcony below; jet up
    it from the balcony.
  - **Flag, on the entry platform:** the port ledge, 5 m deep and 20 m long,
    centered on the opening, level with the bay's floor. The flag stands in
    its middle on the flat floor, in a painted team disc, with open sky
    above. The flags are 699 m apart (the ships' bays 720 m).
  - **Hold, forward of the bay:** eight spawns facing the bay. Ways in: the
    bay door and a hatch in the spine (drop in; jet out). A hatch in its
    floor drops to the station room. No hull opening shows the spawns to
    the sea.
  - **Aft room, by the stern:** where the low-gravity column comes up
    through its floor; a door forward onto the generator room's aft
    gallery.
- **Middle (floor -6):**
  - **Station room**, in the module under the hold: all three inventory
    stations. Ways in: the hold's floor hatch, a port door facing the field
    with a ledge outside, and the door aft onto the balcony.
  - **Balcony**, under the entry platform between the two modules: a
    grated deck 18.5 m wide and 17 m long, open to the field and under the
    hull. Fly in, or drop off the platform's edge. A door at its fore end
    into the station room, a closed armored face at its aft end (the
    keel), and the porthole overhead up into the bay.
- **Bottom (floor -11.6): the generator room**, in the keel, 17 m tall.
  - Its floor has no door, and nothing opens into it from the front (the
    balcony's door was closed on 2026-10-03: the generator was too
    exposed). Players reach it only from the back. They come onto its aft
    gallery, at the top level, from the aft room or the starboard side door
    with its ledge (away from the field), and drop 11.6 m.
  - A fore gallery inside, 5.6 m up, is a ledge for its defenders, reached
    only by flying within the room.
  - Shots come in at angles from the galleries, and defenders can build up
    deployables round the generator.
- **The stern's low-gravity column:** a `gravity` zone at a tenth of
  gravity (`LOW_GRAVITY`), marked by glowing rings. It rises from the
  platform base's deck, through a hatch, to just over the aft room's floor.
  It doesn't pull anyone: a jump carries ten times as high and a jet
  climbs for a tenth of the effort, and you float down slowly through it.
- **Platform base:** an octagonal deck hovering 4 m over the sand under the
  stern, the column's foot, with the vehicle pad. The pad is marked, but it
  does nothing until the base modes hand out vehicles.
- **Gangway:** the walking route (heavy armor too), a 16° ramp from a pad on
  the reef crest up to the entry platform in front of the bay. It has
  girders with team curbs and lit faces, lit chevrons pointing up to the
  flag, two braced pairs of columns with lit collars, and the pad ringed in
  light. The reef beacon with the second turret stands beside its foot; its
  field of fire faces the enemy, so it can't shoot into the bay.
- **On top:** the deck and its raised spine are walkable from the bow to
  the bridge. The bridge roof carries the turret, and its mast step the
  sensor.
- **Under the ship:** 8.4 m of clearance under the keel, 15 m under the
  balcony, to ski beneath.
- **Ground outpost:** on the outer island behind the ship
  (`build_outpost`): a bunker with front and side doors, two stations and
  four more spawns.

## Build and test (from `src/`, numpy venv)

- Build with `scripts/build-reefbreak.py [OUTPUT] [--no-bake]`.
  - The default output is the embedded `assets/maps/reefbreak`, and the
    script refuses to overwrite.
  - Builds are byte-identical. That was verified by rebuilding and
    comparing every file.
- Test with `scripts/test-reefbreak.py` (20 tests; set `REEFBREAK_PACK`
  to test a built pack):
  - each half its own ground, alike in land, shallows and depth;
  - how much water is wadeable against how much is dry land;
  - channels off the flag axis;
  - the flags on their entry platforms outside the port openings, the bays
    open from flank to flank, the port opening toward the field;
  - ships floating clear;
  - the ships drawn from the map's model, one per team, and the stand-in
    (hulls and gangways) as one run of the vertices;
  - spawns 1.2 m over floors;
  - the rooms' ways in (the bay's aft wall solid);
  - the generator floor reached only by dropping (no door below the
    galleries);
  - the low-gravity columns rising from the platform bases into the aft
    rooms through open hatches;
  - the porthole open from the balcony into the bay;
  - the rooms inside the model's skin;
  - the hull's collision kept out of the rooms;
  - the deck's collision within 0.25 m of the model's deck (90% of the
    spine);
  - each base within the collision budget;
  - stations, turrets and sensors per team;
  - the sea volume;
  - the gangway slope and its landing in front of the bay;
  - the ships' spawns face open floor and hide from the field;
  - no turret sees into the bay or the hold.
- The model is read from `assets/models/structures/reefbreak-ship/model.glb`.
  Rebuild it with `sh scripts/models/build.sh SOURCES
  structures/reefbreak-ship` (it runs `scripts/models/reefbreak_ship.py` in
  Blender and reads nothing from `SOURCES`), then rebuild the map.
- Props use the `tower-complex` theme (outcrops, rock clusters, wreck
  debris), so `props.py` is unchanged.

## Verified

With the three-level ship (2026-10-03):

- All workspace library tests pass with Reefbreak embedded, the 20 map
  tests, and the map-features test (now with a low-gravity zone). The behaviour
  trace was recorded again; only the Reefbreak scenario moved.
- The clipping and z-fighting scans report no spots.
- A rebuild is byte-identical, `models.glb` included.
- Each base (ship, beacon and outpost) has 4406 collision triangles,
  within the 4500 budget (tested).
- Captures of the baked pack (`launch_smoke`, v5 and v6):
  - both ships from the field;
  - the underside with the modules, the balcony and the lift;
  - the balcony;
  - the station room;
  - both generator galleries;
  - the aft room with the column's hatch;
  - the column from the platform base;
  - the porthole from the balcony and from the bay;
  - the stern, the flag on its platform, the gangway.

Not yet:

- A human playtest: the low-gravity column (a tenth of gravity; a half is
  a one-line change, `LOW_GRAVITY`), the porthole, the drops, the
  generator's defense.
- Bots: they don't use the column, the porthole or the hatches.
- The bot timings and tick cost below are from the earlier, one-level ship
  (generated model, flag in the bay) and were not run again:
  - a lone bot reached the enemy flag from both sides: Ember 75.8 s,
    Glacier 56.9 s (with the map added to the bot tests' list for that run;
    it isn't in their default list);
  - the bot soak's tick cost 334 µs mean and 1.13 ms worst with eight
    players, against 275 to 298 µs on Dustreach and Frostline.

## Pending

- Human traversal and balance with the flag mid-ship and the bay open on
  both flanks.
- The outposts' spawns: two face a wall 3.2 m ahead and three show through
  the door from 40 m (`spawn_checks`). This is older than the ship change.
- Before a release, confirm the generators' terms allow their output in a
  commercial game (`docs/model-pipeline.md`).

## Seam pass (2026-09-26)

Asset `reefbreak-ship-v2`. The forecastle deck is clipped to the bow beams' inner edges (it lay over their tops); the hull rub band starts 3 cm above the hatch ledges' undersides. Found with `scripts/scan-zfighting.py`; the pack now reports no mixed-material spots. See `docs/map-pipeline.md` §5.
