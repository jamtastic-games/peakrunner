# Cairnhold

Cairnhold is an original PeakRunner map: ground bunkers dug into rugged
highland, an exposed flag stand behind each bunker guarded from above, a
plasma battery on one flank, and a neutral landmark on the high middle ground
between two valleys.

**Originality.** Every structure, the terrain and every texture are authored
here.

**Status.** Embedded from `src/assets/maps/cairnhold/` (key `cairnhold`,
`MapId::Cairnhold`); its fingerprint is part of `game_protocol()`.

**v2 base pass.** The bunker front is a stepped, crenellated gatehouse facade
(gatehouse 17 m, middle tiers 14 m, outer tiers 11.5 m over the 0 m floor) with a
recessed, bronze-lined portal under a lit lintel, a tall team banner with the
cairn emblem, and stone wing blocks retaining the hillside. The roof turret
now stands on the gatehouse. The flag sits on a real tower (platform 41 m, 13.5 m
over the knoll) rising from the guard hut, reached inside by the hut ramp, two
tower ramps and a roof hatch, and from outside by an attack ramp up the knoll to
a bridge onto the platform. The trench roof now climbs from the bunker roof to
the knoll with the ground held flush 10 m to either side, stone kerbs and light
slots on top. Turrets, the battery and the sensor have render-only stone and
bronze collars.

## Layout

The structures have 180-degree rotational symmetry about the map centre
(1024, 1024). Red at low Z faces +Z; blue is red rotated. the ground is not mirrored. Its large layout is point-symmetric (the profile
along the flag axis and the side forms), and the middle round the centre
landmarks stays shared out to 160–260 m. Out toward the bases each half's hills and
gullies come from its own noise (`OTHER_HALF` 2018 in the terrain module),
like real land. Fairness is measured, not mirrored: the halves' slopes,
heights and the climb into each base match within tolerances
(`scripts/assets/fairness.py`), and a lone bot reaches each flag in about
the same time (`a_lone_bot_reaches_the_enemy_flag_on_every_map`). See
`docs/map-pipeline.md`, Terrain.

| Element | Where (red, local; bunker floor = 0) | Notes |
| --- | --- | --- |
| Bunker | 32 x 40 m, floor 191 m world | Dug into the knoll's forward slope; roof flush with the hill behind |
| Facade | front 7 m of the roof, wings to x ±24 | Stepped tiers 17 / 14 / 11.5 m, crenellations, buttresses; gatehouse projects 2 m round an 8.4 x 7.6 m portal (v4) |
| Front vestibule | z −19.2 to −13, ceiling 6 m, under the facade | The 6 x 6 m front door opens straight in; no baffle |
| Hall (v4) | z −13 to 4, ceiling 13 m, roof 14 m | Two-level room: a 4 m gallery round its sides and back at hillside level (+7 m), with a 6 x 6 m doorway in each side wall onto the ground outside; the floor is entered by the front door and interior doors |
| Inventory room (2), back room | behind the hall | 6 m doorways to the 6 m ceiling (no lintels); no spawns in the hall |
| Vault | under the hall: x 3.2–15.2, z −9.8–12, floor −7.5, ceiling −1 (6.5 m) | Stone piers, bronze ribs. Exactly two ways in: a 28° stair from the hall's east side (railed opening, x 11.2–15.2, z −9.5 to −2) and the tunnel door (6.4 x 6.5 m) in its east wall |
| Generator well | x 3.2–10.4, z 3.4–12, floor −9.7 | The kit generator is 5.8 m tall and its hit bar hangs 6.4 m over the floor, so it stands in a well reached by a ramp; a walkway runs from the stair foot to the tunnel door along its east side |
| Sally port | east x 16–88 (z 4–12), then south z 4 → −36 (x 80–88) | 6.4 m wide, 6.5 m headroom (v4), sconces every 8 m. Floor climbs −7.5 → 3 (20.6°), runs level, then 3 → 8 (17.4°). Its stone lid is flush with the hillside it runs under, except two skylight wells (x 48–52 and 64–68): open drop-in shafts with a bronze grate rim, 9–11 m deep, jettable out |
| Exit house | x 80–88, z −44 to −36, on the battery bench | 5.6 m wide door in its east wall, open straight in, at the foot of the battery ramp |
| Roof turret (bullet) | top of the gatehouse, 17 m | Solid gatehouse mass below it |
| Covered trench | x 0–8, z 20–76, floor climbs 22 m (21.4°) | Roof 7 → 27.5 m; ground flush with it 10 m to each side |
| Guard hut | 24 x 24 m, floor +22 | Dug into the knoll (ground 27.5); its roof is a terrace round the tower |
| Flag tower | 14 x 20 m on the hut's east half, platform +41 | Hut ramp → floor 29 → ramp → landing 35 → ramp → roof hatch; buttressed, bronze ring. v4: the platform is fully open (no parapet or merlons), so the flag can be taken from any edge |
| Attack ramp + bridge | x −5–0, z 106 → 79 (26.6°) | From the knoll behind the hut to a bridge onto the platform's west edge |
| Sentry mast (bullet) + sensor | on the tower platform | Sentry about 10 m over the platform |
| Plasma battery | right flank (+X), bench +8 m | Octagonal bastion, parapet, ramp toward home |
| Landing/deploy ledge | left flank (−X), deck +5 m | 24 m paved deck, four marked `deploy_slots` |
| The Ring | map centre, 240 m | Octagonal dais, four axis ramps, eight bronze-seamed pylons. v4: the dais is the central Capture & Hold point; the shared tower stands at its centre |
| West / East Cairn (v4) | world (672, 1012) and (1376, 1036), in the glens (2026-10-07) | Mirrored flank Capture & Hold towers on levelled plateaus (radius 20 m) |

Flags are 1,060 m apart (532 m before the 2026-10-07 spread). Each team has eight `spawn_points` (two in the
inventory room, two in the back room, one in the guard hut, one on the battery
deck, two on the ledge), every one 1.2 m above its floor. None is in the hall,
vault or tunnel; the nearest is 18 m on foot from the stair head
(`test_no_spawn_camps_the_generator`).

Terrain sites are blended in order bunker (with the wings), apron, hut, ramp
foot, then trench last, so the trench banks stay exactly flush with its roof.

Dug-in structures cut terrain cells (102 holes, including the sally port's
cells). Their outer walls lie exactly on
the 8 m cell lines, and every boundary vertex is pinned to a height its walls
cover, so there is no gap between ground and wall. Other structures sit on
flattened benches that reach at least one cell beyond them.

## Terrain

`scripts/assets/cairnhold_terrain.py` (v2, 2026-10-07): an authored layout
plus ridged and rolling detail (each half its own) from PeakRunner's own hash
noise (shared with Tower Complex). Along the flag axis: the knoll (226 m) with
the flag stand, a dip (186 m) in front of the bunker, a forward rise (212 m),
the burn valley (184 m) and the central mesa (246 m) with the Ring. Each side,
a ridge line (crest |X| about 178 m) and beyond it a glen (floor |X| about
330 m); see "Spread and lanes" below. The detail is damped along the flag axis
and the glens so the lanes read.

| Play-area terrain cells (z ±600, x ±420 m from the centre) | Cairnhold |
| --- | --- |
| Median slope | 26.7° |
| 90th percentile | 43.3° |
| Under 3° | 1.8% |
| Over 30° | 40% |

The mesa is higher than the knolls; the ridge crests (245–258 m) stand over
both.

## Build and test

From `src/`, with Python 3 and `scripts/requirements.txt` installed (numpy, Pillow):

```sh
python3 scripts/build-cairnhold.py [OUTPUT] [--no-bake]
python3 scripts/test-cairnhold.py
```

Default output is the embedded `assets/maps/cairnhold`, and the build refuses to overwrite.
It bakes lightmaps (10 pages, about 25 s). Baked builds are byte-identical.
Pack assembly is shared with Tower Complex in `scripts/assets/pack_writer.py`,
and Tower Complex still builds byte-identical to its committed pack. Building
helpers live in `scripts/assets/structure_kit.py`.

`test-cairnhold.py` (24 tests) covers:
- floors and headroom, the trench, the hut ramp and both tower ramps
- the facade: stepped tiers, an open portal through to the baffle, the 2 m recess
- the attack ramp (slope, open sky, closed underside) and the bridge onto the platform
- no fall-through anywhere over a dug-in footprint, the trench banks flush with its roof,
  and the wing blocks and ramp foot meeting the ground
- the standing-support floor ray, no accidental slopes, no overlapping floor plates
- spawn validity, the flag plinth, transform invariance and the collision budget
- unique equipment IDs and circuits, and deterministic, distinct materials
- a whole-map turret/battery sightline check into every room
- terrain fairness (unmirrored halves), slope bounds and the forward-slope layout
- holes and wall coverage of the pinned boundary, ground under decks and the Ring
- Ring symmetry, bake determinism and byte-identical rebuilds

Collision: 2,336 triangles per base (budget then 3,000; now 4,500, see `map-pipeline.md`), 256 for the Ring; about 6,250 render triangles per base.

## Validation done, and gaps

- **Rendering:** checked in a real local match through a temporary pack
  redirect into the `cairnhold` slot, with captures of v1 (aerial, Ring,
  bunker exterior and bunker interior) and of v2 the same way (aerial, Ring,
  bunker exterior, bunker interior, flag tower and trench, battery, slopes,
  a battery-deck spawn view). The v1 camera positions were not
  saved, so the v1/v2 pairs are matched by eye, not exactly.
- **Rust acceptance tests:** `cairnhold_is_embedded_with_grounded_spawns_and_flag_decks`
  (terrain.rs) and `cairnhold_turrets_cannot_see_into_base_rooms` (sim.rs), which
  runs every turret through `equipment::acquire_target` at sensor range against
  standable points in the bunker, trench, hut and tower floors. Tolerance: the
  entry vestibule in front of each baffle and the columns under the open roof hatches.
- **Not done:** human traversal, ski feel, balance, bot pathing, audio, and
  Windows/Linux runtime.
- **Known gaps:** kit turret heads and the sensor are still shared placeholders
  inside render-only collars. The flag is the engine's. There is no bloom, so
  "glow" is a bright texture. The Ring has no mechanic yet. The attack ramp's
  closed side reads as a large plain stone wedge from the west. The interior is
  unchanged from v1. Nobody has walked or skied the new routes yet.
- **v3 vault and sally port:** 3,846 collision triangles per base (budget
  4,500). Python tests cover the vault's two ways in, the stair, the well, the
  whole tunnel (floor, headroom, walls, lids flush with the pinned ground) and
  the exit vestibule (the battery's barrel sees nothing past it; nobody on the
  bench sees up the tunnel). Rust tests walk the full body from the hall down
  the stair, through the vault, the length of the tunnel and out onto the
  battery bench, check the generators sit in cut cells 8.2 m down, and keep
  every turret's sightlines out of the vault, well, walkway, tunnel and exit
  house. The tunnel is 112 m long, so it is a defenders' and infiltrators'
  route, not a fast one; whether the stair + tunnel pair is too easy to hold
  needs a playtest. Bots don't use either.

## Open doorways (2026-09-24)

Supersedes the front-vestibule and exit-house dog-leg rows above. The bunker's
front door opens straight into the hall behind its recessed portal, and the
exit house's door opens straight in. Turrets and the battery face the field
with a 200 degree field of fire beyond 15 m, so neither door gives a turret a
line into the rooms. The route and sightline tests use the new rule.

## v4 playability pass (2026-09-24)

From a playability survey and a visual audit. Asset `cairnhold-base-v4`, Ring
`cairnhold-ring-v2`.

- **Two-level hall.** Behind the 6 m entry vestibule the hall rises to a 13 m
  ceiling (raised roof block, 14 m). A 4 m gallery runs round its sides and
  back at +7 m, level with the hillside outside, and a 6 x 6 m doorway in each
  side wall opens from that ground straight onto the gallery. So the hall is
  entered at floor level (front door) and at gallery level (both side walls),
  and jetting from the floor to the gallery is the fast way between them.
- **6 x 6 m doorways.** Front door 6 m wide to the 6 m ceiling (portal 8.4 x
  7.6 m); interior doorways 6 m wide to the ceiling with no lintels; back door
  6 m; exit-house door 5.6 m. Door frames stand 2 cm off their reveals.
- **6.5 m vault and tunnel.** The vault floor drops to −7.5 (6.5 m under the
  hall slab); the stair runs 14 m at 28°; the well floor is −9.7. The tunnel
  has 6.5 m headroom, its east climb is 28 m at 20.6°, and its lids were raised
  where they had to clear the taller tunnel (the south leg's end now meets the
  exit house's roof at 15.5 m). Two **skylight wells** (east leg x 48–52 and
  64–68) are open drop-in shafts from the hillside with a render-only bronze
  grate; they lead into the tunnel, not the vault, which keeps exactly two ways
  in (the stair and the tunnel door).
- **Fully open flag platform.** The tower platform's parapet and merlons are
  gone; only a render-only bronze edge band remains. 15 of 16 straight fly-in
  lines at +1.2 m reach the flag (the sentry mast blocks one).
  `route_checks.flag_routes` (airborne, hut + tower box, a 10 m zone round the
  flag): 10 entries and 19 routes per flag. v3 measured 14/38 and 14/31, because
  the metric counted each parapet gap as its own approach; the open edge now
  counts as fewer, wider ones. The test holds at least 8 entries and 10 routes.
- **CTF centre (2026-10-02).** The Ring is also active in CTF, with a
  **rocket turret** on top of its tower's crown that fights for whichever team
  holds it (`turret` in the manifest; `docs/capture-and-hold.md`).
- **Capture & Hold.** Three points (`control_points`, radius 12 m; only The
  Ring is also active in CTF): **The Ring** on the dais (the shared `cnh_tower` stands at its centre
  in place of the old brazier stone, the 12 m capture ring is the dais), and
  **West Cairn** / **East Cairn** at world (672, 1012) and (1376, 1036) since
  2026-10-07 (in the glens; first (900, 1010) and (1148, 1038)), mirrored
  through the centre on plateaus levelled to the mean natural ground height of
  the pair (radius 20 m). Rotation accepts `cairnhold` in
  `capture_and_hold`, and a Rust test captures the Ring and West Cairn.
- **Visual fixes.** Overlapping same-plane faces (`surface_checks.z_fighting`)
  went from 72 pairs on the base to 0 on the whole map: door-frame headers
  under the lintels, the trench stripe on its liner, the tower's buttress bands
  flush with their caps, the tier cornice ends on the buttresses, and hidden
  vault slab bottoms. Hovering wall bottoms (the audit's check, bilinear
  ground) went from 32 to 0: floor-length side banners, and the attack ramp's
  first lamp post set on flat ground past the ramp foot. The untextured slab
  the audit reported near (1041, 206, 1261) could not be found: that point is
  5.8 m under the ground in this build, and a capture of the area
  (`cairnhold-pass-audit-slab`) shows no untextured face. Tests pin the
  overlap and hovering checks to zero on the committed pack.
- **Look.** A late-afternoon highland: warm sun colour, a procedural sky with a
  slate zenith, amber horizon and gold cloud (cover 0.5), exposure 1.08, warm
  ground ambient, light valley haze and fog colour 0.78 0.71 0.60. The sun
  keeps the baked direction.
- **Counts:** 3,714 collision triangles per base (budget 4,500), 232 for the
  Ring, 216 per C&H tower, 8,308 in all; about 8,670 render triangles per
  base; 15 lightmap pages. Two baked builds are byte-identical.
- **Captures:** aerial, hall-up, gallery-door-outside, vault,
  tunnel-skylight, skylight-up, flag-platform, ring, west-cairn, sky-sun,
  audit-slab.
- **Still open:** the gallery has no ramp from the hall floor (it is reached
  by jetting or from outside); nobody has walked or skied it; bots do not use
  the gallery, skylights or tunnel.

## Props and terrain shade

Cairnhold scatters original props from `scripts/assets/props.py` (theme `cairnhold`): leaning standing stones and boulder scree (solid), heather mounds and grass tufts (render-only). The committed pack has 24 standing stone, 40 boulder, 1300 heather, 2400 tuft. That adds 42,636 render triangles and 1,800 solid ones (prop budget 2,500). Big props come in mirrored pairs, stay at least 80 m from each flag and 28 m off the ski lanes, and sink into the ground. The baked terrain shade map `shade.rg` (`scripts/assets/terrain_shade.py`) shadows the ground under structures, hulls and big props for the map's sun, plus terrain self-shadow and occlusion. About 5.7% of the tile is in shadow. See `docs/map-pipeline.md`.

**Cover and ground layer** (after the 2026-10-07 spread). 28 mirrored cover pieces (12 rock cluster, 16 stone wall; 10 crouch-height, 18 full-height) stand beside the ski lanes and around the control points, blocking movement and shots. The grass layer adds 8,075 clumps in 97 meadow patches (80,007 render-only triangles). Scenery: 24 standing stone, 40 boulder, 1900 heather. In total props add 131,079 render and 3,696 solid triangles (budget 4,000).

## Water

Two mirrored ponds, the **Peat Burn**, sit in the glens at (662, 776) and its mirror (1386, 1272): ellipses 15 × 24 m, turned −12°, with a 0.6 m/s current down the glen, carved
into the terrain by `scripts/assets/water_bodies.py` from the map definition's
`"water"` entry. The bed shelves from ankle depth at the shore through waist
depth to 2.7 m at the centre (about 1,140 m² each), a crest one grid step wide
holds the water in, and the banks take the soil splat. They are real
`water_volumes`: they slow walkers, brake skiers hard, float swimmers and cost
extra jet energy (see `docs/map-pipeline.md`). They are 119 m from the nearest flag lane, beside the glen road, so they are a
choice beside the route, not a block across it; flags, spawns, capture rings
and holes are all well clear (checked by `water_checks.assert_ponds`). The
underwater tint and surface are a dark peaty slate. No prop, cover piece or bot route is placed in
the water, and the legacy render-only plane is off.

## Seam pass (2026-09-26)

Asset `cairnhold-base-v5`. The flag tower's first ramp end cap stops at the landing slab's underside (its face shared the slab's front); trench and tunnel sconces stand 1 cm off the wall. Found with `scripts/scan-zfighting.py`; the pack now reports no mixed-material spots. See `docs/map-pipeline.md` §5.

## Modern fortification look (2026-10-06)

The bunkers keep their layout, rooms and routes. Collision is byte-identical,
so the behavior traces didn't move.

- **Surfaces** (`cairnhold_materials.py` v2):
  - walls: graphite composite plate on a 2 × 1 m grid with lit bevels;
  - floors: dark deck plate;
  - ramps and treads: steel grating;
  - trim: brushed brass in place of the patinated bronze;
  - interior walls: light panels;
  - banners: team panels with a brass border and diamond.
- **Hard-surface detail** (`scripts/models/cairnhold_models.py`, one root,
  `ch-base`, on both bases, 7,790 triangles). It's generated from the base
  builder's boxes (`scripts/models/cladding_models.py`):
  - steel copings and trims;
  - amber light lines;
  - light fins clear of the tall team banners;
  - skirts and corner guards.

  The knoll and the battery bench have their own ground heights. The Ring
  (prisms) has none.
- **Tests** (`AuthoredModels`): the model is placed on both bases, its
  copings are flush, and it hugs the solid.

## Spread and lanes (2026-10-07)

The bases were too close (too fast to get to the other base and back
again), so they now sit about twice as far apart, with different outdoor
routes, after the classic long maps. The bunkers keep their design; the whole
base unit moved and the ground between was laid out again.

- **Distance:** bunkers at world z 580 and 1468 (were 844 and 1204); flags
  1,060 m apart (were 532).
- **Lanes** (point-symmetric, so each team has the same choices):
  - **Centre:** knoll, dip, forward rise, burn valley, then over the Ring's
    mesa or round either side of it. The shortest way (about 1.1 times the
    flag distance), but into the Ring's turret.
  - **Ridge lines:** each side of the centre, a high crest (245–258 m)
    broken by a pass in front of each bunker and one by each burn valley.
    The high road: long sightlines and downhill runs off either side.
  - **Glens:** beyond each ridge line, a winding glen down the length of the
    field (floor about 155–165 m), deepest through the middle where the
    flank cairn stands, climbing at each end to the bunkers' flanks (the
    plasma battery on one, the landing ledge on the other). The low road:
    sheltered and longer, 1.4–1.6 times the flag distance.
- **Moved with it:** the West and East Cairns into the glens, the Peat Burn
  ponds into the glens.
- **Checks:** `test_ridge_lines_glens_and_passes_make_the_lanes` pins the
  crests, the passes and the glens; the terrain test's window, profile and
  flag distance follow the new field. A lone bot and a flyer still reach each
  enemy flag (`a_lone_bot_reaches_the_enemy_flag_on_every_map`,
  `flyers_cross_every_map_mostly_airborne`).
