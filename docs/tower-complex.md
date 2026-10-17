# Tower Complex

Original PeakRunner CTF map (key `tower-complex`, `MapId::TowerComplex`).
Everything here is PeakRunner's own geometry, terrain and materials.

## Design

Each team base has five floating hulls over rolling hills. The bases are 408 m
apart (z 820 and 1228) and the flags sit about 88–95 m above the ground:

- **Central tower, front centre.** Three levels, entered by the front door.
  - Level 1: open area with cover blocks and ramps.
  - Level 2: the flag, on a plinth between two pillars.
  - Level 3: framed windows (collision panes act as glass) and inventory stations.
  - A gunmetal central shaft runs from the keel level to the roof, with one
    open face per level (keel −x, L1 front, L2 toward the flag, L3 the far
    side) and floor holes at L1, L2 and L3. Hazard striping marks only its
    edges and the L1 hole's lip.
  - **Keel level (generator).** A 16.2 × 16.2 m room inside the top of the
    keel, floor 8 m below the deck, 7 m of headroom under the L1 slab. The
    generator stands in its front-left corner. Exactly two ways in: drop down
    the shaft from Level 1 (jet back up it), or the **keel hatch** on the +x
    flank: a 4.4 m wide, 4.5 m tall airlock passage out through the keel to an
    open 8 × 9 m ledge that a jetting player reaches from the bridges or the
    field. A baffle 1.8 m inside the hatch turns the entry sideways, so no
    straight line runs from outside into the room. No turret or sensor sits
    inside, and the four engine pods moved 2 m lower to clear the room's floor.
  - Above the playable block, stepped setbacks, spires and a team banner with a
    chevron emblem take the tower to about 52 m.
- **Two turret pods, front left and front right.** Each sits on an open bridge
  across open air.
- **Entry baffles.** Solid partitions 2 m inside the front wall stand behind
  both bridge doors and the front door. Players turn sideways through two gaps
  (|x| 4.2–6.0 m) into Level 1. The L1→L2 ramp foot sits behind the left baffle
  (local z −8.8). Before this, each pod turret looked straight down its bridge,
  through its door, along a walkway and into the rear tunnel. That let it track
  players inside the tower (570 standable interior points) and in the ship room
  (60), which felt like shooting through walls. Engine line of sight was correct.
- **Armory, rear left, and ship platform, rear right.** Both connect to Level 1
  through enclosed tunnels. The armory was the generator's pod; it now holds
  an inventory station and a repair pad facing its tunnel mouth, so the left
  tunnel still leads somewhere worth going.
- **Keels.** Every hull has a tapered keel with thruster nozzles.

Blue mirrors Red. Each base has about 9,180 render and 2,570 collision
triangles (budget 4,500, see `map-pipeline.md`).

### Ways in

`scripts/assets/route_checks.py` counts entries on the committed pack with its
airborne model (open-sky decks count as reachable, plus drops and short jet
hops; see the module doc). Both teams:

| Region | Before (v5) | After (v6) |
| --- | --- | --- |
| Main floors (L1–L3) | 3: both bridge doors, front door by a hop from a bridge | 4: the same, plus up the shaft from the keel level |
| Flag level (L2) | 2: ramp from L1, shaft | 2: the same |
| Generator | 1: the left tunnel | 2: the shaft and the keel hatch |

`test-tower-complex.py` holds these at ≥3, ≥2 and exactly 2.

### Terrain

`scripts/assets/tower_complex_terrain.py` generates the heightfield from
PeakRunner's own hash noise: a midfield basin between the bases, a warped
encircling ridge broken by four passes, a hill shoulder rising behind each
base (downhill start toward the basin), rolling fractal hills everywhere,
ridged detail on high ground and bounding hills at the map edge. Ground within
the complex footprint stays at least 20 m under the deepest keel tip. Splat
weights follow slope and height (rock on steep ground).

**No external height data is used.** The terrain measures a median slope
of 19.0°, 90th percentile 35°, 2% of cells under 3°, flags 88–95 m up and a
41–47 m midfield dip. The v4 canyon it
replaces was 30% flat with a 5° median.

Five midfield peak pairs (`PEAKS` in the terrain module) are mirrored through
the field centre so neither half is favoured. They stand off the
flag-to-flag line, which stays a low lane, and add 25–85 m summits between
the passes for cover and jet launch points. With them the interior measures
median 20.3°, 90th percentile 40°, 1.7% under 3°, and the midfield dip on the
flag line is 32 m (35 m without the peaks).

### Landing pads

`scripts/assets/landing_pad.py` builds one open floating pad per team half
(`pads` in `maps/tower-complex.json`): red at (1119, 190, 904), blue mirrored
at (929, 190, 1144). The pad has a 24 m square solid deck at y 190, 50 m below
the base decks. A 0.5 m lip runs round the edge with a 6 m walk-off gap
centred on each side. The underside has a tapered keel 16 m deep, a central
thruster, four engine pods, team trim and corner light posts included in the
bake. Markings and posts are render-only, so the deck is one flat walkable
plane. All armors share the 0.52 m body, so a heavy fits with room to spare.
The terrain is held at least 34 m under each deck; the ground sits about 41 m
under the red pad and 63 m under the blue one, since the fractal ground is
not mirrored. Nearby peaks rise to about deck height as jet launch points.

The pads have no equipment. Four marked `deploy_slots` per pad, recorded in
the manifest's informational `instances` anchors, reserve spots for future
placed turrets or vehicles. The Rust manifest schema is unchanged.

### Spawns

Each team has eight authored spawn points (`spawn_points` in `map.json`,
`[x, y, z, yaw]`, player centre 1.2 m above the floor): two on each tower
level, one in the armory and one in the ship platform room. None is on the
keel level, at the shaft's L1 hole, or near the hatch ledge. The server
picks one at random on every respawn, never the same point twice in a row for
a team, and avoids points with a live enemy within 30 m when another choice
exists. Offline bots use the same points. Maps without `spawn_points` keep the
old single spawn path unchanged.

### Slick spawn fix

The v4 spawn anchor was only 0.2 m above the 1 m floor slab. The engine finds a
standing player's floor with a ray from 0.15 m above the feet, so it started
inside the slab, hit its underside and treated the player as airborne: no
ground braking, so walking coasted like ice until the next jump. Spawns now use
the 1.2 m convention every other pack uses. Probing also showed players could
walk under the low ends of both switchback ramps and get popped upward into a
coast, so solid skirts now close the space under each ramp wherever its
underside is lower than 2.4 m.

Regression tests: the Python suite checks that the support ray finds the
up-facing floor top on every standable cell, that tilted walkable faces exist
only on ramps, the flag plinth and kit equipment, that every spawn stands on
the floor with room around it, and that the under-ramp wedges are closed. The
Rust suite settles a player on every spawn point, walks and releases, and
asserts it is grounded and stops within 2.2 m.

`pod_turrets_cannot_see_into_tower_rooms` samples standable points on a 1 m
grid in the tower (all levels), both tunnels and both rear rooms. It asserts no
pod turret has a clear line to any of them. The only exception is the 2 m entry
vestibule behind the front wall. `tower_entries_route_around_the_baffles` walks a
body from each front opening around the baffles, and on to the ramp for the left
door. It also checks that walking straight on from each opening is blocked.

## Build and test

Run these from `src/`, with Python 3 and `scripts/requirements.txt` installed (numpy, Pillow):

```sh
python3 scripts/build-tower-complex.py            # -> assets/maps/tower-complex
python3 scripts/build-tower-complex.py OUT --no-bake
python3 scripts/test-tower-complex.py            # 23 tests, ~70 s (route counts)
```

The builder refuses to overwrite. Two builds produce byte-identical packs,
including `map.json`, so the fingerprint is stable. The wall-clock bake time is
printed and is not written into the manifest.

| File | Size |
| --- | --- |
| `vertices.bin` | 2.2 MB |
| `collision.bin` | 0.15 MB |
| `height.bin` | 0.13 MB |
| `weights.rgba` | 0.26 MB |
| `textures.rgba` | 10.5 MB |
| `ambient.f32` | 0.7 MB |
| **Total** | **~13.9 MB** |

The pack is embedded with `include_bytes!` for both native and WASM builds. The
macOS release binary grew from 35.9 MB to 49.2 MB.

Sources:

- Geometry: `scripts/assets/tower_complex.py`
- Materials: `scripts/assets/tower_complex_materials.py`
- Lightmap bake: `scripts/assets/lightmap_bake.py`
- Terrain: `scripts/assets/tower_complex_terrain.py`
- Map definition: `maps/tower-complex.json`
- Section views: `scripts/inspect-tower-complex.py`

## Baked lighting

`lightmap_bake.py` bakes, per texel:

- sky and ground ambient, times ambient occlusion
- the renderer's fixed sun, with shadow rays
- 108 lamp samples from the ceiling light strips

The result is 10 pages (30 texture layers), written to the existing lightmap path
(`layer.y >= 0` in `src/map.wgsl`). There is no shader change. Baking takes
about 15 s on 13 processes.

## Compatibility

Tower Complex's pack fingerprint is part of `game_protocol()` (it
introduced `maps3`; the current marker is in `crates/protocol/src/lib.rs`).

## Validation done

- Rust:
  - `cargo test --workspace --lib`: 155 passed. The tests check that the
    map is embedded, listed and non-private, that every spawn point is on its
    own team's deck with room around it, that players settle and brake on
    each spawn, that respawns vary among the team's points, that the flags
    have a deck and roof, and that they sit 60–120 m above the terrain.
  - The all-targets, WASM (existing `Lobby` warning only) and release builds
    succeed, and the app-boundary check passes.
  - The ignored connected-rotation test, with the three private packs, rotated
    two clients through all six maps and back.
- Rendered through the normal embedded path (`QA_LOCAL=1 QA_MAP=tower-complex`),
  plus `QA_FLYCAM` views of the interiors and exterior.

### Keel level (v6) validation

- Python: 23 tests pass, including the keel room's geometry (generator on the
  keel floor under the L1 slab, hatch passage open, baffle blocking every
  straight look in, walk-round on both sides, other walls closed, ledge open to
  the sky, nothing solid poking up through the floor), standing support and
  slopes on the keel floor, passage and ledge, spawn clearance and the route
  counts above. Two baked builds are byte-identical (13 lightmap pages).
- Rust: `pod_turrets_cannot_see_into_tower_rooms` now also samples the keel
  room and hatch passage; `tower_complex_generators_sit_on_the_keel_level`
  checks each generator is inside the tower, on a floor 8 m below the deck,
  under the L1 slab; `basement_bars_stay_in_their_room` covers the keel room.
- Hit bars: a bar squeezed between a generator and a ceiling less than 1 m
  above its usual spot was hidden behind the generator from anywhere a player
  stands. `world_overlay::bar_anchor` now hangs such bars beside the
  equipment on the viewer's side (Dustreach's cistern already did).

## Known gaps

- No human playtest yet: traversal, balance and the fall risk are all unproven.
- The keel level is untested in play. Its generator sits in a 5 m lane between
  the shaft and the wall, so two defenders at the shaft foot may hold it
  easily. Bots do not use the shaft or the hatch.
- The flag cloth is six strips, so seams show close up. Turrets and the flag
  are shared models, so changing them affects every map.
- There is no emissive or bloom. "Lights" are just bright textures.
- The ship platform has no gameplay role yet.
- The landing pads' deploy slots are placeholders; nothing can be placed on
  them yet. Whether a heavy can actually reach a pad by jetting from the
  ground or a peak is unverified in play.
- Red's front faces away from the fixed sun, so the exterior reads dark.
- Skiing feel on the new terrain is unverified; slope statistics are not a
  substitute for a human run.
- The client keeps its own view yaw after a server respawn; whether the new
  per-point facing is applied on a networked client is unverified.

## Open doorways and windows (2026-09-24)

Supersedes the entry-baffle and window-glazing notes above. The three Level 1
entry baffles and the Level 3 glazing panes are gone: the front door, both
bridge doors and every Level 3 window are open for shots and players. The
windows (2.8 m aperture over a 1.1 m sill) are jet-in entries. The pod
turrets face the field with a 200 degree field of fire beyond 15 m, so they
still guard their bridges but no longer see into Level 1. Only the keel
hatch (generator room) keeps its baffle. Level 3's two spawns moved to
Level 2 and the ship room, since the open windows exposed them; the new
`test_no_indoor_spawn_shows_through_an_opening_from_the_field` checks it.
`tower_entries_walk_straight_in` replaces the baffle-route test.

## Atrium rework, v7 (2026-09-24)

Supersedes the tower layout, ramp, terrain and gap notes above where they
differ. Asset id `tower-complex-v7`.

**Tower.** Levels are now L1 0 m, L2 8 m, L3 17 m, roof 26 m. Level 2 and
Level 3 are balcony rings round a 14 × 12 m void, so the atrium is open from
the Level 1 floor to the roof slab (25 m clear). Balconies have 7–8 m of
headroom. Entries: a 6 × 6.5 m front door and two 6 × 6.5 m bridge doors on
Level 1, all open onto the atrium; two new 6 × 6.5 m doors on the Level 2
east and west faces, each onto an outside 5 × 7 m jet ledge with its own
small keel; Level 3 window bands (0.4 m sill, 5.2 m head) on every face;
8 × 6.5 m rear tunnels (were 4 × 4.6) into 9 m-tall rear rooms. The L1→L2
ramp climbs the west wall and the L2→L3 ramp the front balcony; floors above
each ramp are cut back to leave 3.2 m over it. The tube now runs only from the
atrium floor down to the keel room, so the generator keeps exactly two
entrances (the tube hole, the keel hatch). The flag sits on the rear Level 2
balcony overlooking the atrium; inventory stations moved to the rear Level 3
balcony above it. The last front baffle is gone: pod turrets keep the `arc1`
field of fire and never see the atrium (sim test updated).

**Routes** (`route_checks.py`, airborne model, both teams): main floors 6,
Level 2 6, generator exactly 2. New `route_checks.flag_routes` counts
distinct (entry, approach) pairs: each way from the field into the tower
times each way from that entry into the flag's stretch of balcony. Tower
Complex scores 10 per flag (5 entries × 2 approaches: ramp arrival and the
jump from the atrium, or the two ends of the balcony from the Level 2
doors). The suite requires at least 10.

**Terrain.** Rewritten as one continuous landscape: domain-warped broad hills
(no short-wavelength peaks), meandering drainage valleys, a basin between the
bases and a shoulder behind each, then thermal erosion (talus 36°) and a light
blur. Hull, pad and tower clearances use smooth minimums, not hard caps, so
there are no flat plates. Before → after (play area): median slope 20.3° →
17.8°, 90th percentile 40° → 31.5°, under 3° 1.7% → 2.1%, local maxima over
the grid 451 → 94, isolated peaks (more than 20 m over the median within
80 m) 8 → 0, midfield dip 32 → 29 m. The shared helpers `_hash`, `_value`,
`_fbm`, `_smooth` are unchanged (Cairnhold, Frostline and Dustreach use them).

**CTF centre (2026-10-02).** The Summit is active in CTF, with a **plasma
turret** on top of its tower's crown that fights for whichever team holds it
(`turret` in the manifest; `docs/capture-and-hold.md`).

**Capture & Hold.** New shared asset `scripts/assets/cnh_tower.py`: a
tapered octagonal pylon on a plinth with a lit beacon crown, four waist-high
cover walls on the diagonals (open approaches on the axes) and eight
render-only marker posts on the 12 m ring. Ground-contact solids sink 2 m.
Three towers: Summit at the field centre, Westfall and Eastfall mirrored
through it, each on a gently levelled plateau. Manifest `control_points`
declares them (`ctf_active: false`), so Capture & Hold is available on Tower
Complex and the offline menu enables it.

**Look.** Manifest `look`: a procedural sky (clear blue zenith, pale horizon,
42% cloud), exposure 1.05, a cool sky ambient, light valley haze, and fog
colour matched to the horizon. The sun stays the baked default. The dark red
front is lit instead by baked floodlights on the bridges and ledges.

**Surfaces.** `scripts/assets/surface_checks.py` finds coplanar, same-facing,
overlapping triangles with different materials (flicker), ignoring faces
sealed back-to-back or buried inside a solid. The old pack had 84 such pairs
(67 m²); v7 has none, and the suite and the C&H tower now test for it. The
C&H test also checks every plinth and cover-wall edge meets the ground (no
hovering edges).

Collision: 2,442 triangles per base (budget 4,500), 216 per C&H tower, 5,972
for the whole map. 13 lightmap pages.

## Props and terrain shade

Tower Complex scatters original props from `scripts/assets/props.py` (theme `tower-complex`): rock outcrops of two or three boulders on the hills (solid), scrub and grass tufts (render-only). The committed pack has 26 outcrop, 700 scrub, 3200 tuft. That adds 33,963 render triangles and 2,304 solid ones (prop budget 2,500). Big props come in mirrored pairs, stay at least 80 m from each flag and 28 m off the ski lanes, and sink into the ground. The baked terrain shade map `shade.rg` (`scripts/assets/terrain_shade.py`) shadows the ground under structures, hulls and big props for the map's sun, plus terrain self-shadow and occlusion. About 1.9% of the tile is in shadow. See `docs/map-pipeline.md`.

**Cover and ground layer.** 20 mirrored cover pieces (10 debris, 10 rock cluster; 10 crouch-height, 10 full-height) stand 36–62 m beside the ski lanes and around the control points, blocking movement and shots. The grass layer adds 8024 clumps in 100 meadow patches (80,006 render-only triangles). Scenery: 28 outcrop, 1000 scrub. In total props add 111,230 render and 3,864 solid triangles (budget 4,000).

## Water

Two mirrored ponds, the **Westfall Tarn**, sit at (784, 896) and its mirror (1264, 1152): ellipses 26 × 20 m, turned 20°, carved
into the terrain by `scripts/assets/water_bodies.py` from the map definition's
`"water"` entry. The bed shelves from ankle depth at the shore through waist
depth to 3.45 m at the centre (about 1,760 m² each), a crest one grid step wide
holds the water in, and the banks take the soil splat. They are real
`water_volumes`: they slow walkers, brake skiers hard, float swimmers and cost
extra jet energy (see `docs/map-pipeline.md`). They are 85 m from the nearest ski lane, off the Westfall flank, so they are a
choice beside the route, not a block across it; flags, spawns, capture rings
and holes are all well clear (checked by `water_checks.assert_ponds`). The
underwater tint is blue-green. No prop, cover piece or bot route is placed in
the water, and the legacy render-only plane is off.

## Seam pass (2026-09-26)

Asset id `tower-complex-v8`. The Level 2 floor ends at the wall's inner face, so the Level 1 wall top is a plain sill in the Level 2 doorways instead of flickering under the floor; the jet ledges start at the wall's outer face; both ramps' floor cut-outs clear the ramp sides by 5 cm. Found with `scripts/scan-zfighting.py`; the pack now reports no mixed-material spots. See `docs/map-pipeline.md` §5.
