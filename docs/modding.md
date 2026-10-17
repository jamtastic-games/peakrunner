# Moddable engine: design and plan

Status (2026-09-29): **P0 to P4 done, P5's core, and the start of P6**
(rules scripts, §7 "As built"), built on the branches
`mods/guard-rails`, `mods/modular-packs`, `mods/sessions-fetch`,
`mods/definitions`, `mods/mode-policy` and `mods/rule-sets`, merged into
`main` and released in `0.1.0-playtest.20260929.2`.
P5 as built separates the rule sets from the engine: the engine reports
`GameEvent`s (a capture, a touchdown, a carrier tackled, a frag, capture-zone
points, the clock running out) at the moment they happen and applies the
`Cmd`s the mode's `RuleSet` answers (`crates/core/src/rules.rs`); the five
built-in rule sets are Rust and play exactly as before (behaviour traces,
including clocks running out, unchanged), and P6's scripts implement the same
trait. Not built yet from P5: typed server events replacing the client's
snapshot comparisons, generic items and zones in snapshots, the widget HUD
and the bot objective vocabulary. As built: the base package
(`src/assets/mods/base/`: `defs.json`, `modes/*.json`) is built into the
engine; a mod is a folder of `mod.json`, an optional partial `defs.json` and
mode variants (`modes/<key>.json` with `base`), found in content folders,
activated by a server's config (`mods`, `PEAKRUNNER_MODS`) and delivered to
players like maps. Modes keep their rule set in Rust until P5/P6. P2 as built
checks content in the status and Hello rather than a separate `Session`
message (the rotation is fixed for a server's life), and downloads over the
game's own ALPN with a download-sized client window. The server list asks
each listed server for its status and shows its mods and what joining would
download (`peakrunner_net::offer`); the setup wizard offers the mods in
`content/` and their modes. Content folders as built (§3.1): a server reads
`content/` beside its `server.json`, the game and its Host-game server read
the Host folder's `content/`, and the official servers mount
`/data/peakrunner/content` read-only. The practical guide is
`docs/making-content.md`. This document is the agreed direction for turning
PeakRunner into an engine whose game modes, weapons, maps and rules are
content that anyone can make, host and play. It records the decisions, the
architecture, what in today's code has to change, and the phased plan.

## 1. Goal and decisions

**Goal.** The engine is fully capable and knows nothing about any
particular mode. Everything that makes a mode a mode lives in content:
- definitions (armor, weapons, items, zones);
- rules (what scores, what wins);
- maps, sounds and HUD layouts.

Servers run whatever packages they choose. The server list shows which mod a
server runs. Players join without installing anything by hand: the server
tells the client what it needs, and the client fetches what is missing after
the player approves. CTF, Capture & Hold, Team Deathmatch and Football
become packages too. This is the classic server-side mod model: most mods
need no download at all, because clients only ever need parameters and
assets, never rules.

**Decisions (2026-09-28/29):**

1. **Anyone can make and host mods.** There will be a public SDK, with docs,
   examples and a validator.
2. **Clients are data-only.** Joining a server can deliver maps,
   definitions, sounds and HUD layouts. Mod code never runs on a player's
   machine because they joined a server. Rules run on the server.
3. **The base game is a mod.** The official modes ship as packages:
   declarative JSON policy plus a short rules script. The engine primitives
   they use stay native Rust.
4. **Movement changes by parameters only.** A mod picks an engine movement
   model and tunes its numbers within published ranges. New movement models
   are engine work.
5. **Default maps ship as modular packs, not compiled into the binary.**
   There are enough for every base mode. The server verifies which maps the
   client already has and sends the rest only after the player approves.
6. **The engine tells everything what is available.** One engine catalog
   drives the menus, the setup wizard, rotations, votes and the server list.
   Quality improves by updating content, not by changing the engine.
7. **First milestone: modular maps downloaded with approval.** Tuning and
   mode-variant packages follow, then scripting.

**Is it possible?** Yes. Nothing in the code rules it out, but the game is
built the other way today (full inventory in §14):
- Modes are a closed enum, `SupportedMode`, tested at about 80 branch sites
  across core, server and client.
- Weapons, armor, equipment and throwables are Rust constants.
- Maps are a closed `MapId` enum. They are compiled into the binary, and
  `game_protocol()` hashes every map, so any content change needs a
  client release.
- Snapshots have fixed per-mode fields (`flags: [Flag; 2]`, `ball`,
  `points`).
- There is no download path.
- Mode state feeds client prediction: Football's `classic_move`, the carrier
  armor, stun, the C&H drain and fire-as-pass.

## 2. The model

| Layer | Contents | Runs where |
|---|---|---|
| **Engine** (Rust) | Fixed-step sim, movement models, collision, water, lag compensation, projectile archetypes, blasts, equipment, deployables, **items**, **zones**, **contact models**, score sources, bot skills and objectives, networking, rendering, synthesized audio, HUD widget kit | Client and server |
| **Definitions** (JSON) | Armors, weapons, projectiles, blasts, items, zones, equipment profiles, visuals, sound cues, mode policy (`ModeDef`), bot role tables, HUD layouts, scoreboard columns, playlists | Loaded by the server and sent to clients (small) |
| **Rules** (scripts) | Event logic: scoring, win checks, kickoff, halftime, regroup, announcements | The server, or a local server a player starts. In-process offline play runs only the installed default base package. Never on an online client |
| **Packages** | `package.json` plus definitions, rules, maps and sounds | Default packs installed beside every client and server; others on servers, cached on clients |

```text
 packs/ (installed defaults)  cache/ (downloaded)  dev dir (PEAKRUNNER_PACK_DIR)
          \                        |                        /
           +------------> engine catalog <-----------------+
                          | maps (+ the modes each supports), modes,
                          | definitions, playlists, capabilities
        +-----------------+-----------+-------------+--------------+
        v                 v           v             v              v
   offline menu   Host screen and   rotation and  status and   server list
   (mode, map)    setup wizard      map votes     directory    (mod, needs N MB)
```

### 2.1 The engine is the catalog

At startup the engine scans the pack folders and validates every pack. It
builds one **catalog**:
- maps, and for each map the modes it supports, found by matching a mode's
  required features (flag stands, two or more capture zones, a football
  field) against the features the map declares;
- modes, armors, weapons, items, zones, visuals, sounds, HUD layouts, bot
  role tables and playlists.

Nothing lists content in code any more. The offline menu (`app.rs` mode
cards), the Host screen (`host.rs` `MODES`), the setup wizard, rotation
playlists (`rotation.rs` `playlist`), votes, adverts and the server list all
read the catalog. `map_catalog::supports` and `maps_for` become catalog
queries.

### 2.2 The engine publishes its capabilities

The engine publishes what it can run:
- an **API level**, a single integer;
- every primitive with its parameter schema and ranges: movement models,
  item physics profiles, zone kinds, contact models, projectile archetypes,
  HUD widget kinds, mesh primitives, synth cues, and event and command
  names.

Packages declare `requires: {"api": N, "primitives": [...]}`. The catalog
loads a package, or lists it as unavailable with a reason ("needs engine
API 3"). `peakrunner-server engine capabilities --json` dumps the list. The
SDK reference and the package validator are generated from that dump, so
the documentation cannot drift from the engine.

API levels are **additive only**. A newer engine runs every package written
for an older level. Removing or changing a primitive's meaning needs a new
primitive name.

Until the full release the level stays **1** (`mods::API`): playtests ship
the engine, servers and mods together, so nothing is gated by level. Levels
start counting at the release.

### 2.3 Quality improves in content

- These are content and improve without an engine release: maps, looks and
  lighting, visuals, sound recipes and WAVs, HUD layouts, balance, mode
  rules, bot roles and playlists.
  - Default packs update through the signed launcher feed, independently of
    the executables.
  - Community packs update when a player joins.
- An engine release is for new primitives, performance, security, or
  generic renderer and audio work that lifts all content at once.
- Primitives are designed **parameter-rich** from the start, so most new
  ideas need data rather than an engine change.

## 3. Default packs and approved map delivery

### 3.1 Where packs live

| Place | Contents | Written by |
|---|---|---|
| `packs/` in the install | Default packs: the `base` package (modes, definitions, playlists) and the default maps | Launcher feed, packaging scripts. Read-only at runtime |
| macOS | `PeakRunner.app/Contents/Resources/packs/` | same |
| Windows, Linux | `packs/` beside the executable | same |
| Server image | `/opt/peakrunner/packs/` | Image build |
| A server's own content | `content/` beside `server.json` (`maps/<key>/`, `mods/<id>/`); the official servers mount `/data/peakrunner/content` read-only and name it first in `PEAKRUNNER_PACK_DIR` | Operator |
| A player's own content | `content/` in the Host game's folder, read by the game (offline too) and by the server it hosts | Player |
| Per-user cache | Downloaded packs, one folder per fingerprint | The client, after approval |
| Development | `PEAKRUNNER_PACK_DIR` (replaces `PEAKRUNNER_MAP_PACK`) | Map and mod authors |

Folders are searched in this order: `PEAKRUNNER_PACK_DIR`, the program's own
`content/`, the installed `packs/`, then (development builds) `src/assets`;
the first folder holding a key wins. The cache is searched only by
fingerprint, for a server's session.

- **Default maps:** all nine official maps: `old-holler`, `tower-complex`,
  `cairnhold`, `frostline`, `dustreach`, `ozarktic-blast`, `reefbreak`,
  `longfield`, `highgoal`. That covers CTF, C&H, TDM and Football. The list
  may be trimmed, and maps left out become downloads like any community map.
- **Payloads stay zlib-compressed on disk,** as the embedded copies are
  today. The hashes stay over the uncompressed bytes, so the fingerprints
  don't change.
- **Size:** the nine maps are ~159 MB raw and ~34 MB compressed. Taking them
  out of the binaries shrinks the client (41 MB) and the server (38 MB) by
  about that much each.
  (2026-10-08: the ten stock maps are ~286 MB raw and ~70 MB staged;
  Ashveil alone is 108 MB and 24 MB.)
- **Launcher:** its `verify()` accepts `packs/` paths (today only `game/` and
  `map/`). Its content-addressed blobs mean an unchanged map is never
  downloaded again when the launcher updates.

### 3.2 Map identity

- The **key** (`old-holler`) is a label for people and rotations.
- The **fingerprint** (SHA-256 of `map.json`) is the identity. It is already
  a Merkle root, because `map.json` lists the SHA-256 of every payload.
- Two servers can run different maps under the same key. Both live in the
  cache, and neither overwrites the other.
- Per-map data that is Rust code today moves into the manifest:
  - `terrain::all_maps`: name, note, size, base centres;
  - `terrain::overview_height`;
  - the dust colour in `src/effects.rs`;
  - the bot route-planning armor, which `bot_nav::plan_armor` picks by map
    today.

### 3.3 Join flow

1. **Before joining:** the status reply lists the content the server's
   whole rotation needs, vote choices included, as
   `{kind, key, name, fingerprint, bytes}` entries. The server list can
   already show "needs 55 MB".
2. **Resolve:** the client looks each fingerprint up in `packs/` and the
   cache.
3. **Approve:** anything missing opens a dialog: "This server needs 2 maps
   you don't have: Foo (24 MB), Bar (31 MB). **Download and join** /
   Cancel."
   - A preference auto-approves under a size limit. It is **off by
     default**.
   - Declining disconnects cleanly and downloads nothing.
4. **Fetch:** the client downloads over the fetch connection (§10.2) and
   verifies every file's hash while it streams. It installs into the cache
   atomically, with a staged folder and then a rename.
5. **Ready:** the client sends `Ready{epoch}`. The server seats the player
   only after `Ready`. Until then the session holds no player slot.
6. **Later rotations never stall:** everything the rotation and its votes
   can reach was resolved in step 1. A server that changes its rotation
   while running starts a new epoch, and players who lack new content get
   the same dialog.

### 3.4 Browser build

The wasm build is offline only. It loads the same default packs as separate
content-addressed files from the site over HTTPS. That needs an async
loading step: today every load is synchronous `OnceLock`, and `web-sys` has
no fetch features enabled. Until that exists, the wasm build may keep
embedding the default packs through `build.rs` as a documented exception.

## 4. Trust model

- **Online clients execute no mod code.** They load definitions, maps,
  sounds and HUD layouts through validating parsers only.
  - The client refuses any package whose client side contains a script or an
    unknown file type.
  - The client predicts only its own movement, using engine code and the
    data in definitions and snapshots.
- **Scripts run in exactly three places:**
  1. match servers;
  2. the server a player starts from the Host screen (`src/host.rs` starts
     the bundled `peakrunner-server`), which is how a player plays a
     third-party mod locally;
  3. in-process offline play, which runs **only the installed default base
     package** (desktop and browser).
- **Compile-time guarantee.** The script host's constructors accept only a
  `LocalInstall` value. That value is built from `packs/`, the server's
  configured folders or an explicit developer folder, and never from the
  download cache. Code that tries to run a downloaded script doesn't compile.
- **The server still owns everything security-relevant.** The script API
  exposes no identity, name, chat, team-switch bypass, admission,
  connection or file access. Rules decide scores, outcomes, item and zone
  state, and the announcements the engine sends. The engine keeps
  validating commands (§7.3).
- **Integrity without signatures.** Content is fetched by hash over a
  session whose server is already trusted, by WebPKI or an exact pin. Every
  file is checked against the manifest the server named, so a mirror or a
  cache can't substitute anything. Default packs remain signed through the
  launcher feed.

## 5. Engine primitives, capabilities and their data

Each primitive is Rust code with a published parameter schema. Values come
from definitions.

| Primitive | Today | Parameters (examples) |
|---|---|---|
| **Movement model** `skier` | The integrator in `sim::step_players` (approved CTF movement) | Armor: `jet`, `side`, `drain`, `regen`, `min_jet`, `jump`, `fade_up`, `air_cap`, `ski_drag`, mass, collision box, run speeds |
| **Movement model** `classic` | `football::classic_move` | Same armor fields plus classic tick, traction, jump window; must read its armor argument instead of `FOOTBALL_ARMOR` |
| **Projectile archetypes** | Magic `u8` kinds in `sim::shoot` and `step_discs` | `linear` (disc: speed 95, inherit 0.75), `ballistic` (grenade, mortar: fuse, arming, bounce), `bullet` (chaingun), `hitscan` (laser, rail, via `lagcomp`), `thrown` (grenade, mine) |
| **Blast** | `combat::BlastProfile` (`DISC`, `MORTAR`, `GRENADE`, …) | radius, max damage, edge fraction, impulse, core |
| **Weapon** | `sim.rs`, `rifles.rs`, `loadout.rs` | archetype, projectile, blast, reload, ammo, spread, self-damage scale, viewmodel recipe |
| **Equipment** | `equipment::Kind`, `profile()` | range, hull, shield, regen, arc, power circuit |
| **Deployable** | `deploy.rs` | kind, hull, per-team cap, placement rules |
| **Item** (new; generalizes flag and ball) | `Flag` in `sim.rs`, `Ball` in `football.rs` | pickup shape (`pole` sweep, `body` sweep), holder attach point, **holder modifiers** (armor override, what fire does: `shoot`, `throw_held`, `nothing`), drop physics, bounce (`surface_hit`), auto-return time, no-catch windows, throw targeting |
| **Zone** (new; generalizes stands, points, end zones, fields) | `control::Definition`, `football::Field`, flag stands | shape (sphere, cylinder, box), kind (`trigger`, `capturable`, `drain`, `hazard`, `bounds`), capture and decay seconds, drain rate, hazard damage |
| **Interaction table** | `sim::step_flags` order | ordered (actor, item state, zone) → action rows: return, grab, deliver, evaluated inside the loop |
| **Contact model** | `bump.rs` (`football_hit` swap vs momentum exchange) | `exchange` (restitution, damage curve) or `swap` (rounding, fumble and tackle thresholds, stun, damage) |
| **Score source** | `control::score`, frag scoring in `deathmatch.rs` | per held zone per second, per frag, per delivery |
| **Spawn policy** | `sim.rs` spawns, `deathmatch.rs` `ffa_spawn` | team points, any point farthest from enemies |
| **Conditions** | `conditions.rs` | weighted time, weather, wind and twist tables |
| **Bot objectives and skills** | `sim::think_bots`, `football.rs` bot functions | §8.5 |
| **HUD widgets** | `flag_hud.rs`, `control_hud.rs`, `football_hud.rs`, `countdown_hud.rs`, `world_overlay.rs` | `edge_arrow`, `item_marker`, `zone_ring`, `capture_bar`, `carrier_marker`, `status_line`, `banner`, `scoreboard` |
| **Visuals** | `drawlist.rs` `push_*`, `MeshId` primitives | compositions of `Cube`, `Sphere`, `Disc`, `Bevel`, `Armor`, `Decal`, `Head`, `Dome`, `Capsule` with transforms, colours, emission; archetypes `flag` (waving strips) and `ball` |
| **Sound cues** | `sound::Cue`, `Cue::from_event`, WAV overrides | synth recipe id with parameters, or a WAV (validated like `parse_wav`) |

**Ranges.** Every numeric parameter has a published range, and values
outside it fail validation. Movement and projectile parameters are the ones
that matter most: they drive client prediction and physics, so NaN, infinity
and absurd values are rejected before a pack loads.

## 6. Definition formats

Common rules:
- JSON with `deny_unknown_fields`. IDs are kebab-case and unique within a
  package; references across packages use `package:id`.
- Numbers are written with Rust's shortest round-trip `f32` form. The first
  extraction (P3) dumps today's constants, which are often expressions such
  as `100.0 * 0.50 / 0.66`, as their computed `f32` value. A test asserts
  `to_bits()` equality with the old constants.
- Texts shown to players pass the chat rules: printable, no control or bidi
  characters, length-capped.

Light armor, as it would come out of the current constants (`sim.rs`):

```json
{
  "id": "light",
  "movement": "skier",
  "jet": 37.28, "side": 22.0, "drain": 15.0, "regen": 12.0, "min_jet": 3.0,
  "jump": 1.0, "fade_up": true, "air_cap": 20.0, "ski_drag": 0.0,
  "mass": 90.0, "damage_scale": 1.0,
  "weapons": ["disc", "chaingun", "grenade-launcher"], "rifle": "laser",
  "visual": "armor-light"
}
```

The disc and its blast (`combat::DISC`):

```json
{ "id": "disc", "archetype": "linear", "speed": 95.0, "inherit": 0.75, "reload": 1.05,
  "self_damage": 0.4, "blast": "disc-blast", "visual": "disc-round", "fire_cue": "disc" }
{ "id": "disc-blast", "radius": 7.5, "max_damage": 75.757576, "edge_fraction": 0.12, "impulse": 2000.0, "core": 0.0 }
```

The CTF mode policy. `ModeDef` is sent to clients: it is what the client
needs to predict, draw and label. The rules script is referenced but never
sent.

```json
{
  "id": "ctf",
  "label": "Capture the Flag",
  "short": "CTF",
  "teams": 2,
  "requires": { "features": ["flag_stands"] },
  "clock": { "seconds": 480, "halves": 1 },
  "warmup": 10, "intermission": 10, "respawn": 3.4, "suicide_delay": 0,
  "score_to_win": 3,
  "policy": {
    "weapons": true, "throwables": true, "deployables": true, "loot": true,
    "repair_tool": true, "equipment": "on", "stations_serve": "own",
    "team_switch": "always", "spawns": "team", "conditions": false,
    "zones_active": "ctf_active"
  },
  "armor": "light",
  "items": [{ "def": "flag", "per_team": true, "at": "flag_stand" }],
  "interactions": "ctf-flag-table",
  "contact": "body-check",
  "bots": "ctf-roles",
  "hud": "ctf-hud",
  "goal_line": "Take the enemy flag to your stand",
  "rules": "rules/ctf.rhai"
}
```

A capture zone, from a map manifest's `control_points`:

```json
{ "id": "beacon", "kind": "capturable", "shape": { "cylinder": [48.0, 12.0] },
  "capture_seconds": 10, "decay": 0.5, "drain": { "radius": 60.0, "per_second": 10.0 } }
```

Map manifests keep their current format. Existing fields (`flags`,
`control_points`, `football`) are read as zone and item placements by a
loader shim, so today's packs and fingerprints stay valid. New maps may use
generic `zones` and `items` lists instead.

## 7. Rules scripts

**Mod API 3 (`.20260930.3`):** a mode's `weapons` lists the guns allowed
(any of six, or none), enforced by the server, predicted by the client
(`rifles::allowed_slot`, `World::armed`) and followed by bots. Hosts choose
each round's time of day and weather (`conditions::Forecast`), fixed or
random, as a server setting rather than mod content, so no download is
needed for it.

**As built (P6, started on `mods/scripts`):** `crates/script` runs Rhai
(1.26, `sync`, `no_module`, `no_time`) with operation, depth and size
limits and no `eval`. A mode names `"script"`; its mod holds
`rules/<key>.rhai`, which is kept out of the listing, so players
never receive it. The server registers a rules factory
(`rules::set_factory`) and each round's `World::set_mode` builds the
scripted `RuleSet`; the game never registers one (it doesn't link the crate:
`check-app-boundaries.mjs`), so offline a scripted mode plays the rules of
the mode it varies. Handlers get a copied `game` view and call validated
commands; a failing call's commands are dropped, and an unanswered clock
expiry still ends the match. The seven base rule sets are scripts too
(`src/assets/mods/base/rules/`), proved equal to the Rust ones by
`crates/script` A/B tests over 6,000 random events each; production still
runs the Rust ones. Example: `src/examples/mods/spree/`. Events are dispatched
where they happen (P5), not at the sync points below; budgets are per call
(operations), not yet timed per tick.

### 7.1 Language: Rhai

Why Rhai:
- It is pure Rust, so it builds for every target PeakRunner ships, including
  `wasm32-unknown-unknown` for the browser's offline play, and
  cross-compiles with MinGW and Docker like the rest of the workspace.
- It has a built-in sandbox: no file, network or clock access unless the
  host registers it, plus limits on operations, call depth, expression
  depth, and string, array and map sizes, and an `on_progress` hook.
- Its syntax is small and readable for people who have never written Rust.

Build it with `f32_float`, so script numbers match the engine's `f32`. Give
it vector types and functions bound to glam (`length`, `normalize`,
`distance`), so scripts don't reimplement vector maths with different
rounding. Confirm the exact feature flags in the P6 spike.

Rejected:
- Virtual machines written in C or C++ don't build for
  `wasm32-unknown-unknown` and complicate the MinGW and Docker builds.
- WebAssembly modules as mods are well sandboxed, but they force modders to
  install a compiler toolchain.

### 7.2 No synchronous hooks: events out, commands in, at sync points

Scripts never run inside the engine's per-tick loops, so they can't change
the order of anything the engine does in the middle of a phase. The engine
appends typed events to a queue. At fixed **sync points** in
`World::physics_step`, the rules get one call with that phase's events and
return commands. The engine applies the commands before the next phase, so
effects still land in the same tick.

| Sync point | After | Typical events |
|---|---|---|
| S0 | Clock and halftime (`time_left`, `check_halftime`) | `RoundStarted`, `Halftime`, `ClockExpired`, queued match events (join, leave, team switch, holder lost) |
| S1 | `step_players`, `step_bumps` | `BodyHit{a, b, closing, outcome}`, `Stunned`, `Spawned` |
| S2 | Projectiles, repairs, purchases, deploys, throws, loot, suicides | `Killed{victim, killer, cause}`, `Damaged` (reporting only) |
| S3 | Equipment, items, zones (replacing `step_flags`, `step_ball`, `step_points`) | `PickedUp{kind: pickup/catch/intercept}`, `Dropped`, `Thrown`, `Returned`, `Delivered{item, zone}`, `EnteredZone`, `LeftZone`, `ZoneCaptured`, `ZoneContested`, `ItemOutOfBounds`, `ItemIdle` |
| S4 | End of step | `Timer{name}`, optional `Tick` at ≤10 Hz |

Rules are **passed in, not stored in `World`**:
- the signature becomes `physics_step(&mut self, rules: &mut dyn Rules)`;
- `Match` holds `world` and `rules` as sibling fields, so there is no borrow
  conflict;
- offline play passes the base package's rules;
- `predict_command` takes no rules at all.

A callback is all-or-nothing: if it fails or exceeds its budget, its
commands are discarded. Match-level operations that happen outside
`physics_step` queue events for the next S0: `move_to_team`, `vacate`,
`remove_bot`, restart and rotation.

**What stays inside the loops as engine tables:**
- CTF's return, grab and capture order (a fast skier returns and captures
  in one step);
- the body-hit pair loop, where later pairs see earlier pairs' results;
- fire while holding an item, which throws.

Scripts only hear about these afterwards.

### 7.3 Events and commands

**Script API (reads).** A per-call view: players (id, team, alive, pos, vel,
armor, held item, stun), items, zones, scores, clock, phase, mode vars, and
the rules' random stream. It is copied into Rhai custom types; scripts never
hold references into `World`.

**Commands.** Each one is validated when applied, and an invalid command
does nothing:
- **Scores:** `add_score(team, n)`, `add_points(player, n)`,
  `set_var(key, value)` for HUD-bound values.
- **Rounds:** `end_round(winner, text)`, `start_timer(name, seconds)`,
  `cancel_timer(name)`.
- **Items:** `spawn_item(def, at)`, `remove_item(id)`, `attach(item, player)`,
  `drop(item, velocity)`, `reset_item(id)`, `no_catch(player, item, seconds)`.
- **Players:** `regroup(team | all)`, `respawn(player)`, `set_armor(player, def)`,
  `stun(player, seconds)`, `damage(player, amount, cause)`,
  `teleport(player, point)`.
- **Presentation:** `announce(audience, text, style)`,
  `feed(audience, text)`, `cue(audience, cue, at?)`. The audience is all,
  team or player.

**Randomness.** Rules draw from their **own random stream**, split from
`World.rng`. Reordering rule code must never shift shot spread, spawns or
bot randomness, which share `World.rng` today.

### 7.4 Budgets and sandbox

- **Baseline:** 28 players take about 21% of a core for the game thread
  and two relay workers together (measured on Apple Silicon, see
  `docs/multiplayer.md`), so a 16.7 ms tick has room to spare.
- **Budget:** rules get **≤0.25 ms per tick on average and ≤1 ms at p99**.
- **Limits:**
  - an operation cap per callback (`set_max_operations`);
  - a wall-clock guard in `on_progress`;
  - call-depth, expression-depth, string, array and map limits;
  - no `eval`; modules only from the package itself;
  - rate-limited logging.
- **Breaches:** a breach aborts the callback and discards its commands.
  Repeated breaches end the round with a notice, and the server log names the
  package.
- **Never called:** per player per tick, per projectile, per collision
  pair, per damage hit, during prediction, or while building HUD state.
- **Performance:** each package's scripts are compiled once, at load.

### 7.5 Sketches

These are illustrative, not the final API.

```rhai
// rules/ctf.rhai: the flag table in the engine did the touching; this is scoring.
fn on_delivered(ev, cmd) {
    cmd.add_score(ev.team, 1);
    cmd.add_points(ev.player, 10);
    cmd.announce("all", `${ev.player_name} captured the flag`, "capture");
    cmd.cue("team:" + ev.team, "capture_win");
    cmd.cue("enemies:" + ev.team, "capture_loss");
    if view.score(ev.team) >= mode.score_to_win { cmd.end_round(ev.team, "Flag captured"); }
}
```

```rhai
// rules/football.rhai: touchdown, regroup, kickoff to the conceding side.
fn on_delivered(ev, cmd) {                  // ball carried into the enemy end zone
    cmd.add_score(ev.team, 1);
    cmd.add_points(ev.player, 5);
    cmd.announce("all", "TOUCHDOWN", "banner");
    cmd.remove_item(ev.item);
    cmd.start_timer("regroup", 6.0);
    cmd.start_timer("kickoff", 10.0);
    state.kickoff_side = other(ev.team);
}
fn on_timer(ev, cmd) {
    if ev.name == "regroup" { cmd.regroup("all"); }
    if ev.name == "kickoff" { cmd.spawn_item("ball", `kickoff:${state.kickoff_side}`); }
}
```

### 7.6 Hooks (as built)

Rules scripts answer a mode's few events (§7.3 as built in `crates/script`).
**Hooks** extend the same model to definitions (`crates/core/src/hooks.rs`):

- An armor, weapon, pack, deployable, item, effect or vehicle may name a
  hook (`"hook": "rigger"` for `hooks/rigger.rhai`); so may a map's trigger
  zone; a mode lists hooks that hear every event (`"hooks": [...]`). Hooks
  stay on the server like `rules/`, and a server refuses to start when
  something names a hook no mod in force provides.
- Events: `on_spawn`, `on_death`, `on_damage`, `on_fire`, `on_use`,
  `on_deploy`, `on_destroyed`, `on_touch`, `on_enter`, `on_leave`,
  `on_pickup`, `on_buy`, `on_effect`, each with a record, and `on_timer`
  (`on_death` only for deaths that count: none in a blade zone or a duel
  server's lobby). They're made only when a factory is installed and something in the mode
  names a hook, so the base game pays nothing.
- Commands: the rules' scoring and presentation commands, plus damage,
  heal, effects, impulse, teleport, armor, rounds, energy, credits, a team
  feed line, the team waypoint, placing a deployable, a projectile and
  timers. Each is checked when applied.
- Unlike §7.4's sketch, `on_damage` and `on_fire` are per hit and per shot,
  for definitions that name a hook. To keep that safe: a hook's own commands
  never raise further events (no recursion), every call has the operation
  cap, and the hooks share a 4 ms budget per step (loading a script and
  applying its commands count too), checked before every call; past it,
  that step's remaining events are dropped (the call that crosses it runs
  to its end, within its operation cap). Timers due are called only while
  half of it is left for the events, the others waiting for the next
  step's. One-off changes to movement (impulse,
  teleport, armor) reach clients as corrections, as §1 allows.
- Randomness: `game.random(n)` draws from a stream seeded by the server's
  own stream and tick, so the same events in the same match draw the same
  numbers without moving the world's.

## 8. How the official modes map

### 8.1 Football

| Behaviour | Engine (data parameters) | Script |
|---|---|---|
| Classic movement, carrier speeds, jet split | `classic` model; armor parameters; holder modifier (predicted) | — |
| Ball flight, water float, surface bounce, roll and rest | Item physics profile | — |
| Touch pickup; stunned players can't pick up; the thrower can't catch their own throw | Pickup shape `body` sweep; no-catch parameters | — |
| Catch, intercept or pickup, and the catch impulse | Engine classifies and emits `PickedUp{kind}` | Feed text, cue |
| Pass targeting (receiver ray, 0.15 rad cone, power 6.454·√d, loft, `throw_elevation` easing) | `throw_held` on fire; targeting parameters | Feed text on `Thrown` |
| Drop and fumble pop | Drop primitive with the rules' random stream | — |
| Tackle and fumble resolution (rounded speeds, fumble 2–14, tackle ≥15, 2 s down, 30 damage) | `swap` contact model in `ModeDef`, evaluated in the pair loop | Tackle point, feed, cue |
| Out-of-bounds damage | `hazard` zone outside the field bounds | — |
| Ball out of the field, idle ball | `ItemOutOfBounds`, `ItemIdle` events | Reset side, regroup |
| Touchdown | `Delivered{item, zone}` | Score, points, regroup, next kickoff, win |
| Kickoff, halftime, regroup | `halves: 2` emits `Halftime`; timers; `spawn_item`, `regroup` | All the logic |
| Warmup 20 s, respawn 1 s, suicide 15 s, no team switch with the ball in play, weapons, deployables and equipment off | `ModeDef` policy | — |
| Bot leap onto raised goals, pounce on the carrier, pass to an open teammate | Bot skills | — |

### 8.2 Capture the Flag

| Behaviour | Engine | Script |
|---|---|---|
| Whole-body touch against the whole pole, swept (`touches_flag`) | Pickup shape `pole{below, top}` | — |
| Return, grab and capture order and reaches (`FLAG_RETURN_REACH`, …) | Interaction table in the loop | — |
| Carrier attach, drop placement, auto-return | Item parameters | — |
| Capture | `Delivered` | Score, points, announcement, cues, 3-capture win |

### 8.3 Capture & Hold

| Behaviour | Engine | Script |
|---|---|---|
| Capture state machine (`control::step`: 10 s uncontested, contest pauses, decay at half rate) | `capturable` zone | — |
| Drain field (predicted) | `drain` effect on the zone, carried in snapshots | — |
| 1 point per second per held point, fractional carry | Score source (keeps increments on the same ticks) | — |
| Captures, contests, 300 to win | `ZoneCaptured`, `ZoneContested` | Cues, feed, win |

### 8.4 Team Deathmatch and Deathmatch

- `teams: 2` or `teams: "ffa"`. FFA replaces the special case in
  `World::hostile`, the one enemy check every weapon uses.
- Policy: stations serve everyone, turrets idle, no deployables, spawn
  farthest from enemies, random conditions on.
- Script: frag scoring on `Killed`, the suicide penalty, 20 and 40 frag
  limits, the timeout winner.

### 8.5 Bots

Bots run in core, on servers and offline. Their goals are a fixed priority
chain per mode today (`sim::think_bots`, `football.rs` bot functions). They
become:
- **Engine objective vocabulary:** take item, deliver item to zone, return
  item, guard, capture the nearest unowned zone, hold the nearest owned zone,
  escort the holder, intercept the holder, chase item, hunt, go to point,
  retreat to station.
- **Role table per mode (data):** for each role (offense, defense, hunter),
  an ordered list of conditions and objectives.
- **Combat policy per mode (data):** whether bots hunt, aim and fire, or
  (Football) fire only to throw.
- **Skills stay engine code:** leap, pounce, pass, tackle run, ski lines.
- **Optional `bot_objective` hook** at think rate (about 100 calls/s for 28
  bots), only for new modes the vocabulary can't express. The base modes
  need none, so offline and browser bots work without scripts on the hot
  path.
- **Graphs:** `bot_nav` route graphs are keyed by map fingerprint and
  planning armor, replacing the fixed nine-slot `GRAPHS` array.

## 9. Packages

```text
ctf-plus/
  package.json
  defs/armors.json  defs/weapons.json  defs/items.json  defs/zones.json
  modes/ctf-plus.json            # ModeDef (client side)
  hud/ctf-plus.json  bots/ctf-plus.json  playlists.json
  rules/ctf-plus.rhai            # server side only
  sounds/horn.wav
  maps/<key>/map.json + payloads # the existing pack format
```

```json
{
  "schema": 1,
  "id": "ctf-plus",
  "name": "CTF Plus",
  "version": "1.2.0",
  "requires": { "api": 1, "primitives": ["skier", "item.pole", "contact.exchange"] },
  "depends": ["base@1"],
  "files": {
    "modes/ctf-plus.json": { "sha256": "…", "side": "client" },
    "rules/ctf-plus.rhai": { "sha256": "…", "side": "server" }
  }
}
```

- **Two manifests:**
  - The **server manifest** lists every file. Its hash identifies the
    package to the operator.
  - The **client manifest** lists only `side: client` files. It is
    generated, and its hash is what servers advertise and clients cache.
    Stripping the rules therefore never breaks identity.
- **Server-only packages:** a package whose client side is only definitions
  already present in `base` needs no download at all. Only its small
  definition files are sent.
- **Allowed client file types:** JSON (with a schema per folder), map
  payloads (validated by `MapPack::from_assets`), WAV (validated like
  `parse_wav`). Anything else is refused.
- **Paths:** safe relative paths only, using the launcher's `safe_path`
  rules (length, character set, no traversal, no device names).
- **Caps:**
  - per file: JSON ≤ 1 MiB, WAV ≤ 60 s, map payloads within the existing
    `MapPack` caps;
  - per package: ≤ 512 files, ≤ 256 MiB compressed;
  - per server: ≤ 512 MiB of required content.

## 10. Delivery

### 10.1 Sessions

`ServerMsg::Session{epoch, map, mode, packages, defs}` is reliable, on the
control stream. The server sends it before the first snapshot of each
round, rotation or vote change. It maps the session's small integer ids
(map, mode, armor, item, projectile and cue definitions) to catalog entries.

Snapshots then carry ids instead of Rust enum variants:
- `Snapshot.map`, `mode` and `vote_choices` become session ids;
- `PlayerQ` gains an armor definition id;
- item kinds replace `u8` projectile kinds.

`Command::valid` takes its caps from the session definitions instead of
constants. Welcome stays small, since `CONTROL_LIMIT` is 4 KiB. Large
definition tables go as package files referenced by hash, not inline.

### 10.2 Fetch

- **Connection:** a second ALPN, `peakrunner-fetch/1`, on the same UDP port
  and certificate. It needs no player slot and no password. It is separate
  from `PEAKRUNNER_SESSIONS_PER_IP`, with its own limits: concurrent fetches
  and bytes per address (IPv6 per /64), and total server bandwidth.
- **Flow control:** the game's `transport()` limits receive windows to
  8/16 KiB, about 160 KB/s at 50 ms. The fetch connection uses its own
  client config with multi-MiB windows.
- **Requests:** by SHA-256 only. The server serves a file only if it belongs
  to a package it currently lists.
- **HTTPS mirror (optional):** a server may name a content-addressed HTTPS
  mirror (`…/blobs/<sha256>`). Official servers use peakrunner.net. Hash
  checks make an untrusted mirror safe.
- **Not on legacy transports:** the WebSocket and LAN TCP paths get no
  fetch.
- **Client cache:** content-addressed, LRU, with a size cap preference.
  Default packs never count toward it.

### 10.3 Hostile-server protections (test list)

- oversize and truncated files;
- wrong hashes;
- path traversal;
- decompression bombs (the existing 128 MB inflate cap);
- slow senders (time and throughput floors);
- a server that lists more than the caps;
- a declined dialog;
- a rotation change mid-download.

## 11. Compatibility

`game_protocol()` becomes three things:

| Part | Changes when | Needs a client release |
|---|---|---|
| **Engine protocol**: `PROTOCOL` plus engine markers (wire format, engine sim) | The engine changes | Yes |
| **API level** | The engine adds primitives (additive only) | Yes, but old packages keep working |
| **Content hashes**: per package and per map, in `Session` and status | Maps, definitions or rules change | **No**: fetched on join |

- Map fingerprints leave `game_protocol()`. They are negotiated per session
  instead.
- The rule "bump a marker whenever the wire format, simulation or maps
  change" becomes "bump an engine marker whenever the wire format or
  engine simulation changes". Content changes are covered by hashes.
- The switch itself is one incompatible release (§15, P2). After it, map and
  rule updates stop requiring coordinated client and server releases.

## 12. Discovery and the server list

- **Adverts:** `ServerAdvert` gains `mod` (package name and version), `mode`
  as its own field (today it is folded into the map label as `"<Map> -
  CTF"`) and `engine`.
- **Status:** `MatchStatus` gains the content list: required maps and
  packages with sizes.
- **Deploy order:** the directory is deployed **before** servers send new
  fields, because `DirRequest` uses `deny_unknown_fields`.
- **Validation:** every new text field gets the label rules (ASCII,
  length-capped).
- **Client server list:**
  - a Mod column and a mode filter;
  - "needs N MB" from the status reply;
  - an "incompatible engine" state shown before joining (today the client
    checks nothing before joining).
- **Site server list:** shows the mod.
- **Moderation:** the directory operator can delist a server or a package
  name.

## 13. Security and legal

**Security**
- **Parsers:** every client-side parser is bounded and fuzzed: manifests,
  definitions, `ModeDef`, HUD layouts, map packs, WAV.
- **Prediction data:** anything that drives prediction or physics is
  range-checked and rejected if it is not finite.
- **Map memory:** runtime-loaded maps can inflate to 128 MB. The map registry
  caps resident maps and releases unused ones instead of leaking
  `&'static` packs forever. `map_pack::on` returns `&'static` today, and hot
  paths call it every step.
- **Text:** mod-supplied text is rendered as text and validated like chat.
  - `hud_json` moves to serde. Today it is built with `format!` and only strips
    `"`, so a `\` in a mod string would break the HUD.
  - Map manifests gain `deny_unknown_fields`. Today they silently ignore
    unknown keys.
- **Denial of service from scripts:** covered by §7.4. From downloads:
  covered by §10.2.
- **Review:** a security review of the fetch listener and the script host is
  required before the public SDK.

**Legal**
- PeakRunner is free software (`COPYRIGHT.md`): code under the GNU GPL v3,
  art and maps under CC BY-SA 4.0. Its section 7 permission leaves players'
  own maps and mods (what the game or a server loads from a map or mod
  folder) theirs to license, so no separate mod license is needed. A public
  SDK still needs:
  - **user-generated content terms** on `/legal/`: hosts are responsible
    for what their servers distribute, plus takedown and delisting;
  - an update to the privacy notes if the directory lists more fields.
- The originality rule covers official builds and default packs.
  Community packages are the host's responsibility under those terms. The
  directory can refuse names that break them.

## 14. Coupling inventory

Everything that has to change, by file. Function names are given instead of
line numbers because lines drift.

**`crates/core`**
- `map_catalog.rs`:
  - `SupportedMode` (closed enum) with `label`, `key`, `offered`,
    `deathmatch`;
  - `supports` and `maps_for`, which derive modes from manifest fields;
  - `MapDescriptor`, which nothing uses.
- `terrain.rs`: `MapId` (closed enum), `all_maps`, `maps`,
  `overview_height`, the Old Holler heights embedded in code, per-map
  statics.
- `map_pack.rs`:
  - `on` (a per-map `OnceLock` match returning `&'static`);
  - `active` (the Old Holler disk override);
  - the `embedded_map!` macro;
  - `Manifest` (no `deny_unknown_fields`).
- `build.rs`: the `MAPS` and `PAYLOADS` lists; packs zlib-embedded in the
  binary.
- `sim.rs`:
  - `World` fields `mode`, `flags`, `points`, `control_defs`, `cnh_acc`,
    `ball`, `conditions`;
  - `Player.carrying`, `stun`, `suicide_in` and the bot pass, leap and
    pounce fields;
  - `enum Team` (two teams);
  - `armor_for`, `set_mode`, `reset_points`;
  - the start banner in `start_match`;
  - `physics_step` (the countdown path, timeout, the `football()` and CTF
    gates);
  - `think_bots` (goal chain, hunting gate);
  - `step_players` (the `classic_move` branch, stun and countdown freeze,
    fire becomes pass);
  - `shoot` (weapon kinds and stats);
  - `step_discs` (projectile kinds);
  - the knockback mass rule, the respawn delay, `step_flags`, `capture`,
    `touches_flag`, `step_suicides`;
  - `hud_json` (`format!`-built, CTF flag blips);
  - `push_event` (the comma-separated event string).
- `football.rs`: `FOOTBALL_ARMOR`, `classic_move` (ignores its armor
  argument), rule constants, `Field`, `Ball`, and the `impl World` block
  (match time, kickoff, drops, out of bounds, `check_halftime`, `pass`,
  `step_ball`, `touchdown`, `football_hit`, tackles, bot leap, pounce, goal
  and pass).
- `bump.rs`: `step_bumps` (the ordered pair loop, the Football swap versus
  momentum exchange, bump damage).
- `loadout.rs`: `HEAVY_ARMOR`, `max_ammo`, `armor_of` (the mode override),
  the damage scale and twist, loot, repair tool and purchase gates, the FFA
  inventory rule.
- `deploy.rs`: the deploy and ammo-station gates.
- `throwables.rs`: the throw gate and throwable constants.
- `rifles.rs`: rifle constants.
- `combat.rs`: blast profiles.
- `equipment.rs`: turret profiles and durability.
- `teams.rs`: the team-switch rules by mode.
- `deathmatch.rs`: `hostile`, `ffa`, `deathmatch`, `ffa_spawn`, frag scoring
  and win, `apply_twist`, `twist_damage`, `deathmatch_goal`.
- `control.rs`: `step`, `drain_at`, `score`, `CNH_TARGET`, `Event`. It is
  the closest to the target shape.
- `conditions.rs`: roll weights.
- `matchplay.rs`:
  - `Snapshot` fixed fields (`map`, `mode`, `flags`, `score`, `points`,
    `ball`, `vote_choices`);
  - `Match::step` (Football warmup, hard-coded intermission);
  - `rotate_to_mode`, `restart`, `host_start`, `predict_command`;
  - `Command::valid` caps.
- `bot_nav.rs`: `plan_armor` (by map), the fixed `GRAPHS` array and `slot`.

**`crates/protocol`**
- `lib.rs`: `game_protocol` (markers plus nine fingerprints), `ClientMsg`
  and `ServerMsg` (add `Session`, `Ready`).
- `delta.rs`:
  - exactly 4 fixed items (misc, flag 0, flag 1, ball) and `LISTS = 8`,
    both hard-checked;
  - `PlayerQ` (armor as `heavy: bool`, carrying as a team code);
  - `DiscQ` (kind `u8`).
- `packets.rs`: `prepare_fitting`. Server events must be in the set it never
  trims.

**`crates/server`**
- `rotation.rs`: `playlist` (hard-coded names), per-mode validation.
- `gameserver.rs`: Hello admission (exact `game_protocol` match), the mode
  label (`"CTF"`, `"CnH"`, …), the rotation and vote loop.
- `setup.rs`: wizard choices, `saved_rotation`.
- `quic.rs`: the listener (add the fetch ALPN and its limits).

**`crates/discovery` and `crates/directory`**
- `types.rs`: `ServerAdvert`, `MatchStatus`, `DirRequest`
  (`deny_unknown_fields`).
- `quic.rs`: `transport` (8/16 KiB windows), `CONTROL_LIMIT`,
  `STATUS_LIMIT`.
- `lib.rs` and `http.rs`: label validation.
- directory `lib.rs`: `status_ok`.

**Client (`src/`)**
- `app.rs`:
  - `Hud` mode fields;
  - goal text, the offline mode choice and start;
  - scoreboard headers and columns;
  - mode cards;
  - CTF status lines, Football weapon and ammo lines;
  - shop and deploy gates;
  - the vote menu.
- `host.rs`: `MODES`, `Settings.maps` as `MapId`.
- `flag_hud.rs`, `control_hud.rs`, `football_hud.rs`: per-mode HUDs,
  including `FootballWatch`, `PointWatch` and `network_events`.
- `flag_announce.rs`, `generator_announce.rs`: client-side diff watchers.
- `online.rs`: `receive` (sounds derived by diffing snapshots), `sanitize`
  (flag, ball and point indices).
- `drawlist.rs`: flag, ball, beacon, thrower viewmodel, `ball_lights`.
- `player_model.rs`: `Hold`, carrier flag, tackle pose.
- `sound.rs`: `Cue`, `Cue::from_event`, the weapon hum muted by the ball.
- `world_overlay.rs`: the flag-carrier marker, FFA hostility.
- `qa_overrides.rs`: `QA_MODE`, `QA_BALL`, `QA_POINTS`, `QA_FLAG_DOWN`.
- `effects.rs`: per-map dust colour.

**Packaging and launcher**
- `crates/launcher/src/lib.rs`: `verify()` prefixes (`game/`, `map/`); add
  `packs/`.
- `scripts/package-client.sh`, `scripts/stage-launcher-release.sh`,
  `scripts/build-web.sh`: ship `packs/`.
- `crates/server/Dockerfile`: ship `/opt/peakrunner/packs`; drop
  `PEAKRUNNER_MAP_PACK`.
- `scripts/check-app-boundaries.mjs`: add the new crates and their rules
  (§15, crates).

## 15. Phases

Each phase is its own branch from `main` (the roadmap's workflow). Each is
shippable, and behaviour stays identical until P5. Sizes are relative: S, M,
L, XL.

| Phase | Scope | Marker | Gate | Size |
|---|---|---|---|---|
| **P0 Guard rails** | A/B trace harness (`core/src/trace.rs`, test only): bit-exact movement traces for light, heavy and classic; seeded match and offline traces per mode, bots included; 28-bot tick-budget and snapshot-size probes | none | Baselines recorded | M |
| **P1 Modular packs and the catalog** | Engine catalog over `packs/`, the cache and `PEAKRUNNER_PACK_DIR`; `MapId` becomes key plus fingerprint in a capped registry; map metadata moves into manifests; `Manifest` gains `deny_unknown_fields`; `build.rs` stops embedding (wasm exception); `bot_nav` graphs keyed by map; new `crates/content` (manifests, cache, `safe_path`, streaming verification, lifted from the launcher); packaging, launcher `verify`, Mac bundle and server image ship `packs/` | `maps13` | Every map loads from files; traces identical; launcher install and rollback; `rotation_cycles_connected_clients` | L |
| **P2 Sessions and approved transfer** | `game_protocol` split (engine protocol plus per-session content); `ServerMsg::Session`, `ClientMsg::Ready`; snapshot ids for map, mode and vote choices; fetch ALPN, client fetch config, limits; approval dialog, auto-approve preference, cache; status and directory content fields (directory first); server-list size and compatibility. → **Milestone 1: modular maps, downloaded with approval** | `session1`, `delta2`, `fetch1` | §10.3 hostile-server tests; rotation test with a missing map; 28-client test; mixed old and new directory, client and server | L+L |
| **P3 Definitions as data** | Constants from `sim.rs`, `combat.rs`, `rifles.rs`, `throwables.rs`, `loadout.rs`, `equipment.rs`, `deploy.rs`, `football.rs` move to the `base` package (`defs.rs`); `classic_move` takes its armor; `PlayerQ` armor id; `Command::valid` caps from definitions | `defs1` | `to_bits()` equality for every value; traces bit-identical; wasm check | M |
| **P4 Mode policy as data** | `ModeDef` replaces every mode branch in §14; `SupportedMode` becomes a catalog mode id; contact model as data; one round clock for offline play and `Match`; menu, Host screen, wizard, playlists and votes read the catalog; capabilities and API level published, packages checked. → **Milestone 2: data-only mods** (tuning packs, mode variants such as "CTF, 5 captures, no turrets") | `modes1` | Traces identical; countdown and rotation tests; definition fuzzing | L |
| **P5 Generic world and events** | Typed server events (serial, bounded ring, audience) replace `push_event` and the client diff watchers; `hud_json` via serde; **items, zones and vars** replace flags, ball and points; the `Rules` trait with sync points (Rust reference rules); split random stream; widget HUD; bot objective vocabulary | `events1`, `items1`, `delta3` | CTF movement bit-exact; reviewed rule-trace re-baseline; bot soak statistics within noise; **human playtest of every mode** | XL |
| **P6 Scripts** | `crates/script` (Rhai host, `LocalInstall` only); port TDM → C&H → CTF → Football; each port runs in-process against the Rust reference, with identical commands at every sync point over many seeds; budgets; **production switches to the scripted base package**; Rust rules stay as a test-only reference, then retire | none (server side) | A/B equivalence; budget benchmark (extend `collision_budget_timing`) | L |
| **P7 Public SDK** | Modding guide; reference generated from `engine capabilities --json`; `examples/mods/` (a single-zone hold mode, a CTF variant, a tuning pack, a community map); `peakrunner-server package check`; hot reload for a local server; mod license and UGC terms on `/legal/`; directory moderation; security review | none | Review sign-off | L |
| **P8 Later** | Custom meshes and textures format; more than two teams; new engine movement models on request; browser online play | — | — | — |

**As built (branch `mods/classes`, 2026-10-01).** Definitions became engine
classes rather than fixed structs: every capability (armor, weapon,
projectile, blast, throwable, deployable, damage kind, effect, pack, item,
vehicle) is a class whose entries mods patch, replace, extend or add by id,
and the base game is re-expressed on them (traces identical). Hooks (§7.6)
extend rules scripts to definitions. Maps gain moving parts, zones, items
and units. The reference is `docs/mod-format.md`; the example is
`src/examples/mods/sparkfall`.

**Crates and boundaries.**
- **`crates/content`:** serde and sha2 only. The client, the server and the
  launcher may use it.
- **`crates/script`:** core plus Rhai. Used by the server and by the client
  (offline base package only). Never used by protocol, net, directory or
  launcher.
- **`rules.rs` in core:** the `Rules` trait, `ModeDef`, events, commands and
  primitives.
- `scripts/check-app-boundaries.mjs` enforces all of the above.

**Working alongside other work.** P1 and P2 touch maps, protocol and
packaging, so they collide with map rebuilds and releases. Schedule them
between releases. P3 and P4 touch `sim.rs` broadly, so no other sim-wide
refactor should run at the same time.

## 16. Risks and open questions

**Risks**
1. **Feel change in P5.** Sync points, the split random stream and generic
   items change the order of some rule effects. CTF movement must stay
   bit-exact; rule traces are re-baselined only after review and a human
   playtest.
2. **Scope.** P1–P6 is months of work on a game still in playtest. Each
   phase ships on its own, so the work can pause after any milestone.
3. **Untrusted data.** Definitions drive prediction and physics, and maps
   inflate large. Mitigated by ranges, fuzzing and caps.
4. **Script denial of service.** Wall-clock guards behave differently on
   different hosts. Operation caps are the primary limit.
5. **Download abuse and cache growth.** Mitigated by approval, caps, per-IP
   limits and LRU eviction.
6. **Legal exposure of UGC** that official servers or the directory carry.
7. **Memory growth from runtime maps.** Mitigated by a capped registry
   instead of `&'static` leaks.

**Open questions**
- The auto-approve limit, the cache size, and the maximum required content
  per server.
- Whether pinned and invite-link servers may push content, or only listed
  and CA-certified ones.
- Who moderates directory listings and package names.
- Whether desktop offline play should also go through the bundled server
  (then the client never runs any rules, and only the browser runs the base
  package in-process).
- Package signing by authors versus hashes only.
- Which maps count as default (currently all nine).
- Whether the browser fetches packs over HTTPS or keeps embedding them.
