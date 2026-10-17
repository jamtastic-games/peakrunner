# Making an original map

This is the process used for Tower Complex, Cairnhold, Frostline, Dustreach,
Ozarktic Blast and Reefbreak. Follow it for every new or replacement
map. Commands run from `src/`; Python is Python 3 with
`scripts/requirements.txt` installed (numpy, Pillow).

## 0. Rules that never bend

- **Original only.** Every map is an original embedded map, with original
  names. Never copy, trace, resample, blend or fit another game's heights,
  geometry, textures, positions or audio, and never name other games, their
  maps or mods. Say so in the map's doc. There are no exceptions.
- Study notes, renders and captures stay out of the repository: never put
  them under `src/` or `docs/`, never embed them.
- Don't change approved movement physics or shaders to make a map work.
  Fix the geometry.
- Other maps' builds must stay byte-identical. If you touch a shared module
  (`structure_kit.py`, `pack_writer.py`, `lightmap_bake.py`, `landing_pad.py`,
  `fortress_rooms.py`, `build-original-map.py`), rebuild every map that uses it
  and compare all payload hashes. When several maps are built at the same
  time, treat shared modules as read-only and keep helpers map-local.

## 1. Design brief

Write down, before building:

- What players come to the map for: the idea, not the shapes.
- Base types, footprints, positions; what's floating vs. on the ground.
- Flag placement and exposure; flag-to-flag and base-to-landmark distances.
- Spawn regions and weights; equipment (gens, inventories, turrets, sensors)
  and what each covers.
- Terrain: height range, slope histogram (median, 90th percentile, % under 3°,
  % over 30°), the profile along the flag line, ski runs, routes, chokepoints.

## 2. Concept

Publish a concept page (site plan, base plan or section, a terrain sketch of
*our* design, "what we keep / what's new" in words, open questions, 2–3
original names; no names from other games, no reference images). Settle
the name and the big choices (ground vs. floating bases, flag distance, what
the landmark does, which slot it replaces) before building. When a concept
goes ahead without a review, take its recommended answers and say which.

## 3. Build

Files, by analogy with `tower-complex` and `cairnhold`:

- `maps/<id>.json`: id, name, seed, bases (team, position, yaw).
- `scripts/build-<id>.py`: imports `build-original-map.py` as `kit`, starts
  from `kit.base_pack()` (shared textures, ambience, layer layout — never
  another map's committed pack), and uses `pack_writer` for materials, the
  lightmap bake and the pack. Refuses to overwrite; `--no-bake` skips baking.
- `scripts/assets/<id>_*.py`: base, landmark, terrain, materials modules. Each
  `build(mesh, team, circuit)` returns local-space anchors (flag, spawn points,
  entrances, rooms, deploy slots).
- `scripts/test-<id>.py`: see step 5.
- Build into the ignored `out/maps/<id>/` until the map has been seen and
  approved.

Design checklist (each item has bitten us once):

- **Spawns:** 8 per team via manifest `spawn_points` (`[x,y,z,yaw]`), each
  centre **1.2 m above a solid floor** with headroom, facing open space. Lower
  spawns start the support ray inside the floor slab and the player slides
  frictionless.
- **Spawn forward clearance:** from each spawn the body's width must see 6 m
  of clear space ahead, and a half-second walk forward must stay on the same
  floor (no deck edge or ramp top). `scripts/assets/spawn_checks.py` checks the
  built pack's whole world (other structures and terrain included); every
  map's suite runs it, and the Rust spawn test walks it.
- **Open doorways and windows:** doors, bridge doors and windows are open, so
  players can shoot and fly straight in. Don't glaze windows or wall off
  doors. Baffles stay only on generator rooms and spawn rooms. Size each
  window as either an entry (taller than a 2.56 m body; count it in the route
  checks) or a firing slit.
- **Turret sightlines:** fix turrets, not openings. Every fixed turret gets a
  `facing` and `arc` (`scripts/assets/turret_arcs.py`: toward the enemy
  flag, 200 degrees). Beyond `equipment::ALL_ROUND_RANGE` (15 m) the server
  only engages targets inside that field of fire; closer in it covers every
  direction, so a turret still guards its own bridge or ramp. No turret may
  engage a standable point more than 3 m (`sightline_checks.DOOR_DEPTH`)
  past an opening's inner face. When a turret still lines up with an opening,
  move it (as the Dustreach sentry was) rather than adding a wall. Tests use
  `sightline_checks.visible` in Python and the same rule in `sim.rs`.
- **Spawns and openings:** no spawn in a room may be visible from 40-100 m
  out in the field through an open door or window
  (`spawn_checks.exposed`). Open colonnades and outdoor spawns are exempt.
- **Walkable routes:** ramps need headroom and a floor opening above them, and
  closed undersides so players can't walk under a low ramp end. Shafts need an
  open face per level. Check every route with body sweeps. The engine's body
  sweep reaches 2.64 m above a surface, so any ceiling edge over a ramp needs
  about 2.8 m of clearance (Cairnhold's stair opening had to grow for this).
- **Generator rooms:** the kit generator is 5.8 m tall and its hit bar hangs
  6.4 m over its floor, so a generator room needs about 7 m of clear height (or
  a well, as in Cairnhold). With less, the bar hangs beside the generator
  (Dustreach's cistern). Keep spawns well away from the only ways in.
- **Decoration is non-solid.** Trim, liners, markings, lamps and landing
  circles use `solid=False`, so nothing slick or snaggy sits on a floor.
- **Budget:** at most 4500 collision triangles per base, including its
  outbuildings, tunnels and pads (`scripts/assets/budgets.py`, checked by
  every map's suite); report render counts. Raised from 3000 for Dustreach's
  cistern and storehouse after measuring. At 3442 per base, Dustreach's sim tick
  averaged 141 µs (was 112 µs at 2056), worst tick ~310 µs either way,
  against a 16 667 µs tick. A mixed collision query averaged 6.2 µs (was 3.4 µs),
  because the base's 32 m buckets got denser. The densest bucket (1029
  triangles) matches Cairnhold's (1066). The eight-client load test sustained
  726 ticks in 12 s before and after. Re-measure before going past 4500 with
  the ignored `collision_budget_timing` probe in `sim.rs`, and with
  `PEAKRUNNER_LOAD_MAP=<key>` on the load test.
- **Underground:** tunnels and basements sit in terrain holes (whole 8 m
  cells, listed in the manifest's `holes`). Keep every hole cell under a
  structure, so no lid is exposed and the cut's edge keeps the flat site
  height. Otherwise the lid must meet the terrain exactly, like Cairnhold's
  trench. A generator room gets exactly two entrances and no spawn points.
- **Terrain:** original procedural, shaped like real land. It need not be
  symmetrical: hills and valleys don't have to match from one half to the
  other. Keep it fair the way Ozarktic Blast does (the same rooms, entrances,
  equipment and spawns per side, and measured routes), not by mirroring the
  ground. Match the study's slope *profile*, keep ski runs and more than one
  route, avoid large flats. Where terrain is cut for trenches or bunkers,
  holes and collision must match with no fall-through seams.
- **Materials:** a map-specific material module with team variants (banner,
  trim). Every texture replacement must write **all mip levels**; an early
  build only wrote level 0, so distant surfaces showed another map's textures.
- **Lighting:** bake lightmaps (AO, sun shadows, lamp strips) through
  `pack_writer.bake_lightmaps`. Never write build time or other non-deterministic
  values into `map.json`: it changes the fingerprint on every build. A lamp
  may carry its own color (`Builder.lamp(p, power, (r, g, b))`; without one
  it's the baker's cool white, baked exactly as before). Big closed rooms
  need **ambient zones** (`bake_lightmaps(..., zones=...)`, upright ring
  sectors with a tint, `lightmap_bake`): the baker's ambient rays mostly
  travel farther than its 4 m reach in a big room and count as open sky, so
  without a zone a room is lit nearly like the outdoors and colored lamps
  barely show (Tenfold Tower's floors, `docs/tenfold.md`).
- **Fog colour:** optional manifest `sky.fogColor` (`"r g b"`, 0–1) tints
  distance fog and the sky horizon. Omit it for the default grey (0.62). Pick
  it to match the map's sky (warm haze for desert, near-white for snow).
- **Draw distance:** optional manifest `sky.drawDistance` (metres, 480 to
  10000). The camera draws out to it when that's farther than its own
  reach, which is a little past the terrain tile (`terrain_step` x 255 x
  1.05). Set it only on a small tile with scenery past its edge (Ashveil);
  every other map leaves it out.
- **Look (optional `look` object in `map.json`, `peakrunner_core::look`):**
  renderer-only, so it never touches gameplay. Every field is optional and
  unknown keys are rejected:
  - `sun_direction` `[x,y,z]` toward the sun, and `sun_color` `[r,g,b]`.
    Lightmaps are baked with the fixed sun `[-1,1,-1]`, so a moved sun needs a
    rebake. A core test fails if it's more than 3° from the bake.
  - `sun_disc`: brightness of the visible sun in the sky (default 1, 0 hides it).
  - `exposure`: 0.25–4, default 1.
  - `ambient_sky` / `ambient_ground`: hemisphere ambient; the defaults average
    the old flat 0.55.
  - `height_fog` `{density, base, falloff}`: exponential fog that pools below
    `base` height, e.g. valley mist or a whiteout.
  - `sky` `{zenith, horizon, cloud_cover 0–1, cloud_color, cloud_scale,
    sun_size}`: a procedural gradient-and-cloud sky that replaces the cubemap
    faces. Use it to give each map a distinct sky.

  Preview a look without rebuilding: set `QA_LOOK='{"sky":{...}}'` (the same
  JSON object) on a `QA_LOCAL` capture. It applies to every map, and invalid
  JSON is ignored.

  Global renderer changes apply to every map regardless: 4× MSAA when the GPU
  supports it (a settings toggle), a close-range terrain detail layer, softer
  terrain blending from the air, hemisphere ambient, a gentle highlight
  shoulder, and a sun disc at the lighting direction.
- **Inventory stations** open only to a player stepping in from the front
  (`loadout::station_front`). The map kit writes each station entity's
  `facing` (the booth's open side, after its yaw); a station built another
  way should give `facing` too, else the engine probes for the open side.
- **Control points:** Capture & Hold towers go in the manifest's
  `control_points` (id, name, pos on the floor at the ring centre, radius, optional
  `ctf_active`, `drain` and a CTF centre's `turret`, and a post's `capture`,
  `spires` and `beacon`). Give every map at least 2 (centre plus one per side,
  mirrored), keep each ring on open, walkable ground with more than one approach,
  and keep spawns outside rings. See `docs/capture-and-hold.md`.
- **Water:** the legacy `water` plane is render-only. Water that slows players
  goes in the manifest's `water_volumes` (up to 32): each has `surface` (world
  y), exactly one footprint (`rect` `[x0, z0, x1, z1]` or `polygon`
  `[[x, z], ...]`, 3–64 points, convex or star-shaped about its centroid: the
  surface is fanned from the centroid), optional
  `depth` (volume ends that far below the surface), `flow` `[x, z]` m/s (a
  current, at most 30), and `color` (linear RGB 0–1, used for the underwater
  tint). `skim: true` makes the surface ground: players run and ski across
  it and never go under (Reefbreak's sea). `lava: true` is a skim surface
  that burns whoever touches it or skims within 0.9 m of it, `burn` damage
  a second (default 45, `water::LAVA_BURN`), drawn glowing by the map
  shader; bots don't path over it (marker `lava1`). Shots
  burst on lava instead of sinking, and a player's burst in or on it
  throws a splash that burns (projectile and blast `volcanic-splash`, up to
  30 out to 6 m, credited to the shooter; marker `lava2`). See
  `crates/core/src/water.rs`. In the shared sim, horizontal drag
  scales with immersion^1.5 (3/s fully submerged), skiing in water adds up to
  1.5/s, walking top speed drops by 60% x immersion, buoyancy floats a swimmer
  with the eye near the surface, and jetting while waist-deep costs 1.5x energy.
  Chaingun rounds and turret plasma fizzle underwater; discs burst where they
  meet the water, at half damage and twice the push (`Water::Burst`,
  `water::BURST_DAMAGE`/`BURST_PUSH`, marker `water3`: they used to drag to a
  stop and hang there until their fuse ran out); grenades are dragged hard
  (5/s). Measured on a flat patch, a skier entering at 30 m/s keeps
  29.7 m/s after 1 s dry, 20.8 ankle-deep, 2.7 waist-deep and 1.7 swimming; walking
  top speed is 11.2 dry and 7.0 waist-deep; jetting out of deep water takes 0.43 s.
  **Lava ground** (optional payload `lava.bin`, marker `lava2`): for lava
  that follows uneven ground, which flat volumes can't. One 4-byte record
  per terrain cell (256 x 256, rows along x, cells `terrain_step` wide):
  the surface (u16, metres x 32; 0: no lava), the burn (u8, damage a
  second; 0: 45) and flags (u8; bit 0: skim, so players stand on it and
  shots burst on it like a lava volume; otherwise it lies on the ground it
  covers and only burns). It burns within the same 0.9 m, bots skip it and
  the map view draws it orange (`map_pack::LavaCell`; Ashveil, test
  `crates/core/tests/lava_ground.rs`). Draw the lava itself as map
  geometry. Keep water off spawns and capture rings, keep ski lanes dry unless the
  slowdown is the point, and make sure every basin has a shore a wader can walk
  out of. `QA_WATER=surface,x0,z0,x1,z1[,flow_x,flow_z];...` stages volumes in
  a local match for previews.
  **Placing ponds:** declare bodies in the map definition's `"water"` entry
  (`centre`, `mirror`, splat `channel` for the wet bank, and `bodies`: ellipses
  with `x`, `z`, `rx`, `rz`, optional `yaw`, `depth`, `flow`, `color`), then call
  `water_bodies.apply(definition, grid)` right after the terrain grid is made,
  `water_bodies.wet_banks(...)` on the splat weights, write the returned
  `water_volumes` into the manifest with `water_enabled: false`, and pass them to
  `add_props_and_shade(..., water=...)` so no prop lands in the water. The
  carver sets the surface from the ground round the ellipse, shelves the bed
  from ankle depth at the shore through waist depth to the full depth, raises a
  crest at least one grid step wide outside the shore, and traces the outline
  along the finished shoreline so no water hangs past the bank. Put ponds off
  the ski lanes where they make a choice (existing ponds sit 77–125 m from the
  lanes). Suites call `water_checks.assert_ponds(...)` (depth, dry edge,
  clearance from flags, spawns, rings, holes and lanes, mirrored size) and the
  core test `every_maps_water_slows_skiers_who_can_jet_out` skis into every
  declared volume with the real movement code.
- **Water and lava upstairs:** a skim surface (skim water, lava volumes,
  skim lava ground) bears only bodies on it, above it or at most
  `water::SKIM_BELOW` (1.5 m) under it, so a pool or a molten pit on an
  upper floor is no floor to whoever walks the floor below. Keep a floor
  below such a pit more than 1.5 m under its surface plus a jump (Tenfold's
  pits sink into the ceiling below them).
- **Lobby maps:** a manifest's `lobby: true` makes the map a duel server's
  lobby only (`map_catalog::pack_supports`): it hosts the duel lobby and no
  other mode, so menus, rotations and votes never offer it (Tenfold Tower,
  `docs/tenfold.md`).
- **More materials:** a builder may append texture layers of its own after
  the six sky faces (every mip level, level-major, before the lightmap
  pages), raise `texture_count` to match and point vertices at them;
  Tenfold's builder does (`build-tenfold.py` `with_layers`, the manifest's
  `material_layers` records which). Only the kit's `light` layer blooms.
- **Pads:** optional `landing_pad` with named `deploy_slots` anchors for future
  player-placed turrets and vehicles.
- **Moving parts, zones, items, units and vents**
  (`crates/core/src/features.rs`): five optional manifest lists.
  - `moveables` (up to 64): boxes (`size`, half extents in metres, turned
    `yaw` degrees) whose centre follows `path` (2 to 16 points, shut to
    open) at `speed` m/s. `drive` says what moves them: `loop` (back and
    forth by itself, waiting `wait` seconds at each end: lifts and moving
    platforms), `team` (opens while a player of `team` 0 or 1 stands within
    `range` metres of where it stands shut: doors), `anyone`, or `power`
    (open while `circuit` is powered). `look`: `platform`, `door` or `lift`.
    Players stand on them and ride along, are pushed out of their sides, and
    shots stop on them. Where a part is follows from the server's tick and
    its openness alone, so clients predict it exactly.
  - `zones`: axis-aligned boxes (`at` centre, `size` half extents), or with
    a `radius` upright cylinders that tall, for every team or one (`team`). `hazard` hurts (`damage` a second of
    `damage_kind`, default `fire`), holds an `effect` on, and shows its
    `message`; `safe` lets no damage in; `portal` takes players `to` a point
    facing `facing` degrees (a flag carrier drops the flag); `gravity`
    scales how players fall inside by its `gravity` share (0.1: a tenth, so
    a jump carries ten times as high and a jet climbs for a tenth of the
    effort), predicted by clients (Reefbreak's stern columns); `updraft`
    is a pocket of hot air that pushes up by its `lift` (m/s², up to 80,
    on top of gravity's 20) over its lower part and fades to nothing over
    the top 40% of its height, with vertical air drag inside
    (`features::UPDRAFT_DRAG`), so whoever jumps or jets into it rises
    steadily and settles where the lift meets gravity (a `lift` of 30 in a
    30 m column climbs at about 6.7 m/s and holds near 23 m). Standing on
    the ground doesn't lift you. Clients predict it and draw rising embers,
    ash and a faint heat shimmer up the column (marker `updraft1`);
    `blade` is a side game's floor (`docs/blade.md`, marker `blade1`): inside,
    everyone holds a blade, the jets are off, and a death counts for nothing
    and comes back after 5 s at one of the zone's `spawns` (up to 16
    `[x, y, z, yaw degrees]`, 1.2 m over the floor); its `name` shows on the
    HUD. Blade zones work for every team alike. A map with blade zones gets
    careful collision (marker `walls1`, `docs/tenfold.md` "Traps"), and
    since nobody there has jets, its floors need no spot a body can't walk
    or jump out of: no gap between solids (or a solid and a wall) wider
    than nothing and narrower than about 1.2 m, no pocket narrower than a
    body walled by steps, and no leaning faces too steep to stand on that
    meet a wall or each other (make those solid as upright shapes,
    `collide_prism`). Run the tower's trap search, or one like it, on every
    blade floor.
  - `items`: `{ "item": "<id>", "at": [x, y, z] }`, a pickup
    (`registry::Item`) that comes back its `respawn` seconds after it's
    taken.
  - `deployables`: `{ "deployable": "<id>", "team": 0, "at": [...], "yaw":
    degrees }`, standing from the start of every round.
  - `vents` (up to 16): a vent at `at` erupts by itself, each gap between
    eruptions `every` `[least, most]` seconds (2 to 3600), after `warning`
    seconds (default 3, under the least gap) of rumble and glow. It throws
    `bombs` `[least, most]` (up to 32) of `projectile` (default
    `vent-bomb`, whose life must cover 1.2x the flight) to spots `reach`
    `[near, far]` metres away (up to 2000), in a `cone` degrees wide
    (default 360) round `toward` degrees (0 toward -Z, 90 toward +X), each
    landing after about `flight` seconds (0.5 to 20). `offset` shifts its
    schedule. The schedule and every landing spot follow from the server's
    tick alone, so every client shows the warning and marks the spots
    before the bombs land. The bombs belong to the map (`MAP_OWNER`): they
    hurt both teams and credit nobody. Keep every spot well clear of flags
    and spawns (a map's own test should check every angle and reach a vent
    allows). `vent-fireball` is a wider molten ball that burns 320 a second
    to anyone it touches in flight and keeps going (`burn`, marker
    `burn1`); the ground still bursts it. A vent's warning glows, smokes
    and marks every landing spot (`effects.rs`) and plays `Rumble`, its
    eruption `Eruption` (`docs/hud-audio.md`).

  `peakrunner-server --check` checks paths, sizes and every id against the
  definitions in force, so a map may use a mod's items and deployables when
  served with that mod. The core test `crates/core/tests/features.rs` builds
  a fixture from a copy of Old Holler with one of each.
- **Smoke** (optional manifest list `smoke`, up to 64, drawn only):
  `[x, y, z, size]` for a chimney's plume or a fire's smoke, `size` 0.1 to
  8 (about the plume's width at its base, metres). Dark puffs rise and
  spread as they drift; small fires (under 2) throw embers too
  (`effects::map_smoke`; Ashveil's skull tower). Not part of the
  simulation: no marker, and clients that don't know the field ignore it.
- **Props and terrain shade:** after the lightmap bake, call
  `pack_writer.add_props_and_shade(kit, files, manifest, theme, seed, holes,
  flags, spawn_points, control_points)`. It scatters the map's prop theme
  (`scripts/assets/props.py`) and bakes `shade.rg` (`terrain_shade.py`).
  - Props are original low-poly shapes built from the map's own materials.
    Small props (tufts, ferns, heather, scrub, saplings, bones) are
    render-only. Big props (boulders, outcrops, standing stones, logs, large
    ice shards) are solid and sunk into the ground. They come in mirrored
    pairs through the flag midpoint, stay at least 28 m off the ski lanes
    (flag to flag, flag to each control point), and take at most
    `props.SCENERY_COLLISION_TRIS` (2500) solid triangles. Scenery and cover
    together stay under `props.PROP_COLLISION_TRIS` (4000) per map, outside
    the per-base budget.
  - **Cover** (`place_cover`, the theme's `cover` list): deliberate solid
    pieces that block movement and shots, such as dry-stone walls, log piles,
    rock clusters, sandstone ruins, crates, ice ridges and plating debris.
    They stand 36–62 m to either side of each ski lane and in a ring
    34–46 m around each control point, mirrored through the flag midpoint.
    Heights are crouch cover (1.25–1.5 m) or full cover (2.6–3.6 m). Walls
    are built in 2.5 m segments that follow the ground and reach 0.5 m below
    it, so nothing hovers or leaves a gap underneath. Rock clusters overlap
    their rocks, so no wedge can trap a player. Cover keeps
    `BASE_CLEAR` off each flag (flag routes are unchanged), off lanes, rings,
    spawns and holes, 30 m apart and 4 m from any other solid prop. Each map
    gets 16–22 pieces.
  - **Ground layer** (`ground_layer`): up to `props.GROUND_TRIS` (80,000)
    render-only triangles of grass clumps (8–12 blades each). Two thirds go
    into dense meadow patches 4–9 m across, weighted towards ski lanes, flags
    and control points; the rest is scattered, thinner away from them.
  - **Instanced props (`props.bin`):** every render-only prop (grass clumps,
    tufts, ferns, heather, scrub, saplings, small ice shards, bones) is a GPU
    instance, not baked vertices. `props.Instancer` builds
    `props.INSTANCE_VARIANTS` (24) shapes per kind at size 1; each placement
    picks a variant by its shape seed and keeps its own position, yaw and
    size as a uniform scale. The renderer (`map.wgsl` `vs_prop`) applies the
    kit's `Mesh.point` yaw, recomputes the kit's planar world UVs (world / 4
    on the two axes other than the dominant normal) and shares `fs_map`, so
    lighting, terrain shade, fog, look and bloom match baked geometry. Solid
    props (big scenery, cover) stay baked: their collision is in
    `collision.bin` and they cast into `shade.rg`. Layout: header `PRP1`,
    mesh count, instance count, a mesh table (first vertex, vertex count,
    first instance, instance count), 8-float vertices (local position,
    normal, material) and 8-float instances (x, y, z, yaw, scale, mesh
    index), sorted by mesh, so each mesh is one instanced draw.
    `map_pack::PropSet` validates it at load. The payload is optional (older
    packs load without it) and is compressed by `crates/core/build.rs` and
    copied by the bundle scripts. The grass budget counts instance triangles,
    so it no longer grows the pack: Tower Complex went from 19.1 MB of
    vertices to 3.7 MB plus 0.4 MB of instances.
  - Nothing is placed near existing collision geometry, terrain holes and
    their neighbour cells, flags, spawns, capture rings, water, or the map
    edge. Density thins away from the flags and points.
  - Props keep the live-lighting path (light layer -1), so they add no
    lightmap pages. Each pack's `props` block in `map.json` records the
    counts and every big prop.
  - `shade.rg` is a 1024x1024 map (2 m texels), two bytes per texel: sun
    visibility and ambient occlusion. It comes from a soft heightfield
    ray-march, an orthographic sun depth map of every opaque structure and big
    prop (so floating hulls shadow the ground below), and overhead and concavity
    occlusion. The terrain shader and props multiply sun light by R and
    ambient by G. Bake it with the map's own sun, the same as the lightmaps.
    The payload is optional: packs without it render unshaded. Installs carry
    it compressed (`scripts/stage-packs.sh`).
  - Each map suite runs `assets/prop_checks.py` on its committed pack. It
    checks at least 12 mirrored cover pieces clear of lanes, flags, spawns,
    rings and holes, that each piece stops a shot at crouch height, that
    a player standing a metre out from any face can walk three metres away,
    that the ground layer spends its triangle budget, and that `props.bin`
    matches its manifest hash and summary with instances on the ground and
    clear of flags and rings (`check_instances`).
- **Authored models (`models.glb`, optional):** a structure may wear an
  authored model (`docs/model-pipeline.md`, `assets/models/structures/`)
  instead of the kit's look: Reefbreak's frigates do
  (`scripts/assets/reefbreak_base.py`).
  - **The payload:** binary glTF whose scene roots are named for what they
    dress, each holding its parts in that structure's frame. Encode it with
    `scripts/assets/map_models.py` (`read`, `clip`, `encode`); it keeps the
    pipeline's one material, base color and glow (alpha: team mask).
  - **Placements:** `map.json`'s `models` list places them:
    `{"model": "<root>", "position": [x, y, z], "yaw": degrees, "team": 0|1}`.
    The yaw turns them as the kit's `Mesh.yaw` turns structures; the team
    paints the model's team mask.
  - **Render only.** Collision stays in `collision.bin` and is built by the
    builder, fitted to the model:
    - `map_models.sections` samples the model's outer cross-sections along
      rays from an axis inside it. Concentrate the rays where players walk,
      and keep crossings inside your rooms out (`keep`), and those of parts
      with their own collision.
    - A hull lofted from one scaled section (a hard-surface build) is
      sampled from the image of one point on it at every station, so its
      corners fall on the same rays all along: aim rays at them
      (Reefbreak's `hull_centre` and `HULL_ANGLES`).
    - Tube those sections, and give other parts their own simple pieces.
  - **Cuts:** rooms, doors and hatches are the kit's (lined, lightmapped).
    The builder cuts the model away inside them with `map_models.clip`,
    which splits triangles at the boxes' faces so edges stay straight. A
    room's lining must stay inside the model's skin; test it by casting out
    of the lining (`map_models.hits`).
  - **Stand-in:** the outside collision geometry also stands in for the model
    where models aren't drawn: the browser build, `PEAKRUNNER_MODELS=0`, or
    a file that fails to load. Emit it as render geometry and record its
    vertex ranges (as many as you like: Reefbreak records its hull's and its
    gangway's, which the model also draws). The pack moves them to the end of the structures'
    vertices and lists the run in `map.json` as
    `"standin": [first vertex, vertex count]`. The game skips that run while
    the models draw.
    - Keep stand-in faces off the rooms' faces: sink its ends inside walls.
    - Fan caps from the point each section was sampled from, so they never
      fold.
  - **Budget:** the model's triangles don't count against collision, but its
    stand-in does (the per-base budget above). Sample only as finely as the
    walkable surfaces need.
  - **Pack size:** a 60,000-triangle ship with a 4096 texture is about
    4.4 MB. Downloads carry the file like any payload; the browser build
    neither embeds nor reads it.

## 4. Look at it before wiring

Before a map joins the default maps, preview it from a content folder of your
own: the engine's catalog (`crates/core/src/catalog.rs`) lists every map folder
it finds, validates it with the same `MapPack::load`, and needs no code change:

1. Build into the ignored `out/packs/maps/<id>/` (the folder name is the
   map's key) and add an `info.json` with its menu name and note.
2. `PEAKRUNNER_PACK_DIR=<absolute path to out/packs> QA_LOCAL=1
   QA_MAP=<id>`, plus `QA_CAPTURE_PATH` and
   `QA_FLYCAM=x,y,z,yaw,pitch,fov` from `examples/launch_smoke.rs`.
   (`PEAKRUNNER_MAP_PACK=<folder>` still loads a pack in Old Holler's place.)
3. Capture: aerial, landmark, base exterior, base interior, flag area, each
   turret emplacement, ground-level slopes, one spawn. Compute cameras from
   world-space anchors: a room corner at eye height looking across, never
   into a wall. View every image and re-aim bad ones.
4. Captures time out once the display sleeps. Run them straight after the
   build, and stop and report missing views instead of retrying for long.
5. Keep the captures out of the repository.

## 5. Tests

**Clipping.** Two things keep surfaces from flickering, and neither needs a
builder to do anything:

- The renderer uses reversed depth (a 32-bit float buffer, near plane 1, far
  plane 0; `drawlist::clip_correct`), about 0.1 mm of precision at a
  kilometre. Trim, banners and borders only a centimetre off a wall render
  cleanly at any distance. With the old mapping they flickered past ~100 m.
- `pack_writer.write_pack` runs `resolve_overlaps`: the smaller face of every
  visible, exactly coplanar, same-facing overlapping pair moves 1 cm forward
  (nothing is removed; lighting and collision are unchanged). Such pairs
  carry different baked lighting and no depth buffer can order them.

Run `scripts/scan-clipping.py [MAP...]` (numpy venv) on a finished pack: it
reports coplanar overlaps (expect none) and floors with terrain rising through
them. Scattered rocks sunk into slopes show up there and are fine; a
structure's floor or ramp under the grass is not (Old Holler's bunker ramps
ended 2 m underground until v4 of its structures).
`scripts/test-pack-writer.py` covers the overlap pass.


`scripts/test-<id>.py` must cover:

- floors and headroom along every route; no overlapping coplanar floors
- the support-ray check for every spawn
- no slick surfaces
- sightlines into rooms
- route counts with `scripts/assets/route_checks.py` on the committed pack:
  at least 3 ways into the main building, 2 onto the flag deck, exactly 2
  into the generator room. Ground maps use the walking model; floating bases
  pass `airborne=True` (open-sky decks, drops and short jet hops, see the
  module doc). Define room-shaped regions; a box around an open area scores 1.
- transform/anchor invariance
- unique equipment IDs and per-team circuits
- terrain-stat bounds and cut-seam checks
- bake determinism
- byte-identical rebuilds

Also run the other maps' suites and confirm their builds are unchanged.

Then run the stricter flicker scan on the built pack (numpy venv, from `src/`):
`scripts/scan-zfighting.py assets/maps/<id>`. It finds same-facing faces
under 0.5 cm apart that overlap, anywhere a face is not buried in collision
at every overlapping point, and exits 1 on any mixed-material spot. A new map
must report 0 mixed-material spots. Same-material spots are listed for
review; they only show where baked lighting differs across the seam.

Common causes, all found in the 2026-09-26 pass:
- a floor slab run over the top of the wall below it where a doorway leaves
  that wall top exposed (end the floor at the wall's inner face);
- ramp side plates in the plane of the floor cut-out's edge (clear the cut-out
  by 5 cm);
- a deck or cap in the plane of the beam or slab it sits beside (clip it to
  the beam's inner edge, or stop the cap at the slab's underside);
- lamps and sconces flush with the wall or lintel they hang on (offset 1 cm).

## 6. Make it a default map

Maps are folders, not code (`docs/modding.md`). A default map is one that ships
in every install's `packs/`:

- Build into `src/assets/maps/<id>/`. Two builds must be byte-identical.
- Write `src/assets/maps/<id>/info.json` by hand: `name` (menu name, at most
  48 characters), `note` (menu description), optional `bases` (the two base
  centres, `[[x,y,z],[x,y,z]]`; default: the flags) and `dust` (RGB 0..1 of
  blast dust). It is authored, not built: a rebuild that recreates the folder
  must keep it (`git restore src/assets/maps/<id>/info.json`). It is
  presentation only; the map's fingerprint stays the SHA-256 of `map.json`.
- That's the whole wiring. Menus, the Host screen, the server setup,
  playlists and votes list it from the catalog; `scripts/stage-packs.sh`
  (called by every packaging script and the server image) copies it into
  `packs/maps/<id>/` with compressed payloads; the browser build embeds the
  default folders (`crates/core/build.rs` `MAPS`, `map_pack::embedded`: add
  the id there).
- Until per-session content lands (`docs/modding.md` P2), client and server
  must hold identical folders: `game_protocol()` lists the default maps'
  fingerprints only. A new default map joins that list and bumps the
  `mapsN` marker.
- Add acceptance tests as before (spawns grounded, flags on a deck, turrets
  can't see into rooms: the `sim.rs` sightline tests) and update the tests in
  `map_catalog.rs`, `rotation.rs` and `server/src/lib.rs` that list maps.
- Update the package release-notes input and the map's own doc. Never edit
  records of a published release.
- Verify with the checks in the README's "Build from source", plus the
  ignored connected-rotation test (`rotation_cycles_connected_clients`,
  release, `-p peakrunner-server`).

## 7. Commit

Stage map sources, the embedded pack, Rust, and docs by name. Never stage
captures, studies or scratch builds (`out/`). Commit the build scripts first,
then the wiring. Nothing is deployed until a full matching client and server
release.
