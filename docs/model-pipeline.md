# Model pipeline: armor, weapons and structures

Authored, textured models for the people's armor, the weapons and map
structures, made from concept art and source meshes, processed into game
models, and drawn by the engine in place of the built-in
parts. The look they
follow is `docs/art-direction.md`.

## What the engine reads

- **Files:** `assets/models/<key>/model.glb` in a checkout, `packs/models/<key>/model.glb`
  in an install (`scripts/stage-packs.sh` copies them; their `source.json`
  stays behind). Keys: `weapons/<weapon id>`, `armor/<small|medium|large>`,
  `projectiles/<model>` (named by a projectile's `model`),
  `turrets/<deployable id>` (the CTF centres' turrets) and
  `structures/<name>` (map structures, carried in their maps; below).
  A model loads on first use (`src/model.rs`); a missing or bad one is logged
  once and drawn with the built-in parts. `PEAKRUNNER_MODELS=0` turns all of
  them off (comparisons). The browser build draws the built-in parts.
- **Format:** binary glTF 2. Mesh nodes are parts, drawn with their node's
  transform; empty nodes are named points. Per material: a base color
  (factor and texture) and an emissive texture whose color is the glow and
  whose alpha is the team mask (team texels hold their shading as gray around
  0.5 and are painted in the team's color); a material named `team…` is team
  paint all over. An armor's draws also tint its bare plates
  (`ModelDraw::plates`, `player_model::TEAM_PLATES`): texels lighter than
  0.24 gray (fully from 0.42), off the mask and not glowing, take a darker tone of the
  team's color (`PLATE_TONE` in `model.wgsl`) and lose half their metal, so
  the dark undersuit and the glow stay as authored and the art needs no
  second skin per team. Positions, normals and first texture coordinates are
  floats; textures are JPEG or PNG, at most 4096 pixels a side.
- **Materials** (2026-10-13, `model.wgsl`): physically based. A material may
  carry glTF's metallic-roughness texture (roughness in green, metalness in
  blue, times its factors), an occlusion texture (red; the hard-surface
  builds share one texture for both) and a tangent-space normal map, which
  needs the primitive's `TANGENT`s (MikkTSpace, as Blender writes them). A
  file that gives no factors and no texture is drawn matte and not metal,
  as models were before. Models are lit by the map's own look (sun color,
  sky and ground ambient, exposure; `map.wgsl` lights the ground the same),
  reflect its sky, and take sun shadows from the bodies near the camera
  (`scene::ShadowGpu`), which they also cast onto the map and themselves.
- **Skins:** a model may carry one glTF skin of at most 16 joints. Its
  skinned mesh nodes bend with the bones named by the joint nodes
  (`bone_pelvis`, `bone_chest`, `bone_head`, `bone_upper_arm_l`, ...:
  `player_model::bone_frame`) by their `JOINTS_0` (unsigned bytes or shorts)
  and `WEIGHTS_0`, from the rest pose the inverse bind matrices undo; their
  node transforms are unused.
- **Axes:** meters, +Y up, -Z forward, +X right.
- **Weapons** (`player_model::push_weapon_model`, `drawlist::viewmodel_lit`):
  the origin is the right hand's grip; points `grip_r` (the origin),
  `grip_l` (the support hand) and `muzzle`. Third person holds it in the
  aim frame a little oversized; first person puts the grip at `VM_GRIP`,
  scaled by `VM_SCALE`, with the support hand on `grip_l`. Rounds, beams and
  the flash leave where the weapon's `muzzle` says its model's barrel ends
  (the view model's own muzzle for a weapon without one). A new weapon model
  needs its `muzzle` (from the grip: `VM_GRIP` plus `VM_SCALE` times the
  model's `muzzle` point less `grip_r`); `drawlist::muzzle_tests` checks every
  weapon with a model, the base game's and Sparkfall's, within a centimetre.
  Every weapon is a hard-surface build since 2026-10-13
  (`scripts/models/weapon_models.py`, the weapon named by the output's
  folder; its `source.json` names the builder): one part, `body`, its
  points those of the generated model it replaced, so grips and muzzles
  are unchanged.
- **Armor** (`player_model::push_armor`): one model per body size (the armor
  definition's `build`: `light`, `medium`, `heavy`), authored at the pose's
  size (the soles at `gait::SOLE`, 0.145 m under the player's position).
  Bodies are drawn `player_model::STATURE` (1.187) times that, grown from
  the soles: the boots stand on the ground under the collision sphere and
  the helmet's crown stays at the top of the hit volume (they were drawn at
  the pose's size until 2026-10-13, floating 0.37 m). The gait runs at the
  speed in the pose's units, so planted feet stay put once drawn. Each part
  is named for the bone it rides and made in that bone's frame:

| Part | Bone frame |
|---|---|
| `helmet` | the head: origin at the neck, face toward -Z |
| `torso` (chest, back, collar, pauldrons), `pack` (the jetpack; glows with the jet) | the chest: origin at the pelvis point, leaning with the body |
| `pelvis` | at the pelvis point, upright |
| `upper_arm_l/_r`, `forearm_l/_r`, `thigh_l/_r`, `shin_l/_r` | the limb: origin at the upper joint, -Y down the limb, X the body's right; stretched to the posed bone's length |
| `boot_l/_r` | the foot: origin at the ankle, toes toward -Z |
| `glove_l/_r` | the hand: origin at the wrist, -Y along the hand, the palm toward -Z |
| `grip_l`, `grip_r` | a hand on a weapon, made in the weapon's frame at the hand: the support hand on its left side, the firing hand round its pistol grip |

  The neck and flag stay built in, and so do the gloves unless the model has
  `glove_l/_r`. Hands on a weapon (2026-10-07): when the armor has `grip_l`,
  `grip_r` and the points `wrist_grip_l/_r` (where each one's cuff meets the
  forearm), the hands hold an authored weapon or the thrower by its own
  `grip_r` and `grip_l` points (the support palm `SUPPORT_OUT` left of
  `grip_l`), in first and third person. Each forearm runs from its glove's
  wrist point to an elbow found by two-bone reach (`player_model::ik_elbow`:
  both bones at full length, the elbow bending down and out), and two-handed
  holds blade the upper body (`BLADE`: the left shoulder forward, the head
  still ahead). First person reaches from shoulders off the lower screen
  edge (`drawlist::push_vm_arms`); the weapon sits where `sim::vm_anchor`
  puts it (since 2026-10-13, marker `muzzle3`: higher, nearer and more to
  the middle, the firing hand on the grip low on the right in view, the
  mortar lower), and that place also sets where rounds leave. Without the grip
  gloves, the built-in support hand and sleeve are drawn
  (`push_vm_support`). Named points change what else the game draws:
  - `suit`: the model carries its own undersuit, so the built-in capsules
    are left out;
  - `visor`: the helmet is closed, so no face or close-up features are drawn;
  - `nozzle_*` (in the chest frame): where each jet flame leaves, instead of
    the built-in pack's two.

  Without them, the built-in undersuit and face are drawn, and the face
  shows through an open helmet. A frame's growth for the size is smaller
  with authored armor (`bulk`). First person, the support arm wears
  `forearm_l`.

  The three armors are hard-surface builds (2026-10-05, rebuilt 2026-10-13):
  `scripts/models/armor_models.py`, one size per run (the output's folder
  names it; `build.sh` runs it from each size's `{"builder": ...}` spec).
  - **Undersuit:** one skinned part, `undersuit`, in the model's frame at
    the engine's rest pose (`skeleton()`: the pose's pelvis, neck,
    shoulders and hips, the arms out 14 degrees): the torso, blended from
    the pelvis to the chest across the waist and into the head at the
    neck, and each arm and leg as one tube bending at the elbow and knee
    (`suit()`). Its forms (`TORSO`, `LEG`, `ARM`) are scaled by the size's
    bulk (`body`, `depth`, `limb`); its quilting is in the normal map.
  - **Plates:** rigid, a part per bone as before, wrapped round the same
    forms. A plate of many facets is shaded smooth (a curved panel) with
    crisp edges; one of few stays faceted.
  - **Sleeves:** `sleeve_upper_l/_r` and `sleeve_fore_l/_r`, the
    undersuit's arm in rigid pieces, which only first person draws
    (`drawlist::push_vm_arms`); the third-person body wears the skinned
    undersuit.
  - **Hands:** the grip gloves' fingers are jointed, rounded bones with
    guards on their backs (`finger`, `seg`, `guard`).
  `render_model_captures` also writes several more views:
  - running (`armor-run`), jetting (`armor-jet`, `armor-jet-front`),
    Glacier front (`armor-front-glacier`) and close hands (`armor-hands`);
  - first person in each armor (`vm-armor-<size>`), the repair tool
    (`vm-repair-tool`) and Football's thrower (`vm-thrower`);
  - a rider in the Ridgerunner's open seat (`rider-side`, `rider-back`);
  - the over-the-shoulder view in the open, with a wall ahead and with
    one behind (`third-open`, `third-wall-ahead`, `third-wall-behind`),
    with the HUD's crosshair drawn in. The
  design is from the first round of people concepts. Its
  parts overlap round their bone origins, so it bakes them apart
  (`hardsurface.SPREAD`).
- **Projectiles** (`drawlist::push_round_model`): a projectile names its model
  (`"model": "rocket"`); it flies nose first (-Z) along its path, a disc spins
  flat, a thrown one tumbles and rests upright. The model replaces the round's
  look; one that speeds up (`accel`) gets an exhaust flame. A round's `color`
  paints its model all over, shaded by the texture's gray (the disc's blue).
  Rounds without a model keep their look, grown by `size` and recolored by
  `color`. A `trail` (ribbon or smoke) follows either. A round that drags
  (`pull`, `grapple`) trails a cable back to its shooter.
- **Turrets** (`drawlist::push_point_turrets`): the base on the tower at the
  origin, facing -Z, turned to the turret's aim.
- **Sensors** (`equipment/sensor`, `drawlist::push_sensor_head`): the radar
  head on every map sensor's mast, a hard-surface build
  (`scripts/models/sensor_models.py`).
  - **Frame:** the origin is on the mast's axis at its top.
  - **Parts:** `collar` stays put; `head` (a turntable and a slotted antenna
    array) turns about Y while the sensor is powered. It stops when
    unpowered and goes gray when wrecked.
  - **The kit's part:** it draws only the base and mast. The old bar still
    collides, undrawn (`Mesh.collision_box`), so collision is unchanged.
- **Structures** (`structures/<name>`, `drawlist::push_map_models`): a map
  structure's look, such as Reefbreak's frigate
  (`structures/reefbreak-ship`).
  - **Shipping:** it doesn't ship in `packs/models` (`stage-packs.sh` skips
    it). The map builder reads it, cuts its rooms and doors out of it, and
    writes it into the map's own `models.glb` (`docs/map-pipeline.md`,
    "Authored models").
  - **Frame:** made in the structure's frame (the builder's base-local
    frame: meters, its floor at Y=0).
  - **Spec:** a generated one is fitted by a scale along its length
    (`"scale_by": "x"`), with `"fill": false` so its openings stay open. A
    hard-surface one names its builder script (below).
  - **Team paint:** key magenta, as on weapons (a hard-surface build marks
    its team material directly).

## Making a model

Everything below runs from `src/`. Sources (concepts and generated source
meshes) are made outside this repository and kept with the model sources,
one folder per key; `<sources>` below is that folder.

1. **Concept.** Draw it in the style of `docs/art-direction.md`: weapons in
   pure side view (barrel to the left), armor pieces alone (front view; the
   jetpack from behind; the boot from the outside, toe left), on a plain
   light-gray background, team paint in key magenta `#E000E0` for weapons and
   Ember red for armor. A concept sheet holds several objects; each one's
   image is saved as `<sources>/<key>/concept.png`.
2. **3D.** Each concept's source mesh, made outside this repository, is
   saved beside it as `<sources>/<key>/raw.glb`.
3. **Spec.** `assets/models/<key>/source.json` (fields: top of
   `scripts/models/process.py`): per part its raw model, its concept and view
   (the model is turned to match the concept's outline and colors), the box it's
   fitted into, triangle and texture budgets; weapon points as fractions of
   the bounds. `python3 scripts/models/measure.py OVERLAY.png CONCEPT...`
   finds weapon grips (behind the trigger guard) and muzzles on the concepts;
   check its overlay and fix any by hand.
4. **Build.** `sh scripts/models/build.sh <sources> [KEY...]` runs Blender
   headless: orient, fit, close and remesh into one clean shell, decimate to
   budget, unwrap, bake the generated texture onto it, find the team paint
   and glow, write `model.glb`. A source that's a single thin sheet needs
   `"solidify"`; one whose decimation stalls gets coarser voxels by itself.
5. **Look.** `blender -b -P scripts/models/preview.py -- MODEL.glb OUT_PREFIX
   --views side,fp,three` for quick views; in the game,
   `cargo test --release -p peakrunner --lib render_model_captures -- --ignored --nocapture`
   writes `screenshots/models-*.png` (the three sizes on both teams, close
   and at 40 and 110 m, every weapon held and in first person;
   `PEAKRUNNER_CAPTURE_MOD=sparkfall` adds a mod's weapons). Look at them.

Budgets: weapons 12,000 triangles (8,000 for pistols), 1024-pixel textures
(2048 for the hard-surface weapons, seen up close in first person); an armor
about 40,000 triangles in all (the hard-surface ones are 35,000 to 37,000 on
one 2048 texture set, about 5 MB with their material maps). A weapon file is about 1 MB. A map structure may have up to
80,000 triangles and a 4096 texture: Reefbreak's 80 m frigate with its
gangway is 19,700 and 4.2 MB.

A structure's concept is a pure side elevation on light gray (the view its
source mesh is made from), with three-quarter, top and parts sheets to guide
it.
Reefbreak's brief and sheets are kept with the model sources
(`structures/reefbreak-ship/`).

## Hard-surface structures

A generated source mesh is soft at every edge, and step 4's voxel remesh softens
it further, so a design drawn in flat plates and sharp chines comes out
melted. Such a structure is built instead by a Blender script that models
it from the concept's side and top views:
`scripts/models/reefbreak_ship.py` for Reefbreak's frigate (the generated
model it replaced is kept with the model sources), and
`scripts/models/old_holler_models.py` for Old Holler's bases and field
structures. Both use the shared toolkit `scripts/models/hardsurface.py`
(solids, lofts, plates, beams, rings, the bake and the glTF writer).

- **Spec:** its `source.json` names the script (`{"builder": "..."}`);
  `build.sh` runs it instead of `process.py`. By hand:
  `blender -b -P scripts/models/reefbreak_ship.py -- OUT.glb [--texture N]`
  (about 25 s at 4096).
- **Shapes:** closed solids. The hull is one chined section lofted along
  the ship, scaled down to a point at the prow and about the deck toward the
  transom, so every band is one flat facet. Plates sit on the hull's bands,
  their feet sunk in and their tops inset. Blender's exact boolean cuts the
  openings the rooms need.
- **Edges:** every edge sharper than 30 degrees gets a 1-segment chamfer
  whose faces take the steel material, so edges catch the light. Faces are
  shaded flat, round parts smooth.
- **Texture:** one layout at one density (smart project, packed), then
  Cycles bakes each material's color (with large-scale variation, panel
  seams, grille and grate slots), its glow, the team mask and ambient
  occlusion, combined into the usual albedo and glow textures.
- **Roots:** `root(name)` starts a scene root; the solids made after it
  belong to it. One file holds a map's every structure, each placed by name
  (`models` in the manifest), all on one texture.
- **Physically based** (`PBR = True`, armors and weapons; off by default,
  so map structures rebuild byte-identical): besides the albedo and glow,
  `finish` bakes a metallic-roughness texture (`SURFACE`: each material's
  roughness and metalness; paint wears to bare metal at small chips) with
  the occlusion in its red channel, and a tangent-space normal map of each
  material's surface detail (`detail_tree`: orange peel and chips on paint,
  brushing on steel, a diamond quilt on the undersuit, grain on rubber),
  and writes the tangents. The occlusion leaves the albedo but for a trace
  in its crevices. Data textures are saved raw (`save_data`).
- **Skinning:** `JOINTS` (each joint's name and rest frame) and `SKINNED`
  (the roots whose mesh bends) make `write_glb` write the skin; a skinned
  solid's `weights` give each point's bones.
- **Settings** a builder may change (defaults keep the first models as they
  were built):
  - `PLATE_BACKS = False` drops each plate's hidden back face, and a face
    given the material `'hidden'` (buried, or against another solid) is
    dropped too. Neither takes texture space.
  - `PACK`: the island packing, such as
    `dict(margin=.001, margin_method='FRACTION', shape_method='AABB', rotate=True)`.
    The default 0.0015 scaled margin wastes over half a busy texture.
  - `TEXEL` (for the solids made next) and `FLAT_TEXEL` (for plates and
    rings of one flat color: glow, team, amber, glass) set relative texture
    detail. A thin ring's island is mostly hole.
  - `GRATE`: the grate's slot period and duty. Old Holler uses 0.3 m, which
    stays readable at its density.
  - `SPREAD`: bake each root this far apart along X. A file's roots overlap
    round their origins (a map's structures, an armor's parts), and baked in
    place they shade each other dark.
  - `POINTS`: named points (`(name, (x, y, z))`), written as empty nodes.

  No island may be longer than the texture is wide at the density wanted,
  so build long parts in pieces (Old Holler's 212 m bridge rails break at
  their posts). Old Holler's six roots come to about 22 px/m on the base.
- **Generated detail:** `scripts/assets/cladding.py` records a structure
  builder's `mesh.box` and `mesh.ramp` calls (and its banners, kept clear).
  It returns the outdoor faces and open tops that nothing covers.
  `scripts/models/cladding_models.py` dresses those with copings, trims,
  light lines, fins, skirts and corner guards, all within a few centimeters
  of the solid. A map picks its look with a `Style`:
  - the ground (a height, or a function of x and z);
  - the light, fin and coping materials;
  - which tall team panels the fins keep clear of.

  Dustreach's citadels, gate and ruins (`dustreach_models.py`) and
  Frostline's stations (`frostline_models.py`) are built this way, with no
  stand-in. Any map whose structures come from `structure_kit` can use it.
- **Fit:** the map builder fits rooms and collision to it as to any
  structure model (`docs/map-pipeline.md`, "Authored models"). Keep the
  script's constants for the rooms, openings and ledges in step with the
  builder's (each is noted where it binds). A model may instead be fitted
  to existing solid geometry, leaving collision as it was, as Old Holler's
  field structures are.

## Rules

- Original designs only. Never name or imitate another game, film,
  franchise or product in prompts, files or builds.
- Before a release, confirm the generators' terms allow their output in a
  commercial game.
