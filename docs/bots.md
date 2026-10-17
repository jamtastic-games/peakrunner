# Bots

Bots play offline matches, and on servers that keep a match topped up with
them (`PEAKRUNNER_BOTS`, `docs/multiplayer.md` "Server bots"). All bot state
on `Player` is `#[serde(skip)]`, so snapshots and the protocol are unchanged
and no compatibility bump is needed. Football bots play by their own jobs and
go by football names (`docs/football.md` "Bots"); this page covers the other
modes.

## Route-following (`crates/core/src/bot_nav.rs`)

When a map is first played, a navigation graph is built from its collision triangles and terrain. On native builds this runs in a background thread and takes 1–5 s. Until the graph is ready, and on the web build, bots steer straight at their goals.

**Waypoints**
- Terrain waypoints sit on a 16 m grid.
- Floor waypoints are sampled on two interleaved 3 m lattices over walkable triangles.
- A floor waypoint is rejected if it is buried, sits under a slab, lacks headroom, or is pressed into a wall.

**Links**
- **Ski:** between terrain neighbours.
- **Walk:** between floor neighbours, and hops over steps and plinths.
- **Floor-to-terrain:** doorway probes.
- **Jet:** simulated with the real jet constants on 85% of a tank, including run-in speed from downhill terrain. Each arc is checked for clearance.
- **Drop:** falls of up to 160 m. Landing damage is capped.

**Pathfinding**
- Isolated pockets of fewer than 30 nodes are never snapped to.
- A* is limited to 40k nodes, and at most two plans run per tick.
- A stuck bot bans the link it failed on and replans. If no route remains, it forgets the bans.
- A route starts from a waypoint the bot can reach in a straight line.
- Bots save energy on the approach to a jet link and wait for a tank before launching.
- A jet climb starts from the ground, as it was planned: a bot that reaches the launch in the air and falling (a hop off a ski run) lands first instead of burning its tank against the fall, and its jets pressed on landing jump-boost it (`movement.jet_jump`, `docs/loadouts.md`). Plans leave the boost out, so it's margin.
- During a jet, a bot keeps thrusting until a coast would carry it over the target.

## Personalities

| Archetype | Aim error | Reaction | Style / role |
| --- | --- | --- | --- |
| Rookie | 0.11 rad | 0.75 s | Ground |
| Grunt | default | default | Ground |
| Rider | — | — | Skier |
| Skirmisher | — | — | Jetter, hunter |
| Anchor | — | — | Defense |
| Ace | 0.025 rad | 0.2 s | Jetter, hunter |
| Hawk | 0.045 rad | 0.3 s | Flyer |

Profiles also set lead, aggression (chase range 20–80 m), retreat health, kit use and route weights per link type.

In CTF, each team's first living attacker (any bot not on defense), when it has nothing more pressing (no flag carried, none to chase), goes and touches a post its team doesn't hold: a control point that flips in 2 s or less (`control::Point::capture`, Ashveil's Sanctum). Maps without one play as before.

## Flyers (Hawk)

A flyer crosses the map in the air instead of following the ground route:

- It takes off only under open sky, and only where the climb it would fly is clear for its first two seconds (its run carried on, the jump-boost and the jet's lift): a door's lintel or a porch roof ahead would pin it. Otherwise it follows the route out first.
- It cruises 28 m above the highest ground over the next 120 m, and never below the goal's height. Over the last 150 m it glides down toward the goal.
- It pulses the jet: full tank down to empty, then it coasts while energy refills (energy refills whenever the jet is off, in the air too). Holding altitude indefinitely is impossible with these numbers, so a flight is a series of arcs.
- A wall ahead makes it stop pushing and climb straight up.
- It dives on a visible target below it within 90 m, aiming up to 1.3 rad down, and brakes with the jet near the ground.
- Within 30 m of the goal and level with it, it hands back to the ground route for the landing. Near the goal (within 320 m) the route owns the approach, so the route's jet climbs aren't interrupted.
- A goal more than 30 m above it within 260 m (a floating deck) is out of one tank's reach, so it lands on the open field and takes the route up. Routes that start under a floating base fail, even for Grunts.

`flyers_cross_every_map_mostly_airborne` requires a lone Hawk from each team to reach the enemy flag within 240 s on every map, airborne for at least 40% of the run. Measured runs (seconds to flag / share airborne, Ember then Glacier):

| Map | Ember | Glacier |
| --- | --- | --- |
| Old Holler | 73 s / 79% | 65 s / 85% |
| Tower Complex | 58 s / 81% | 43 s / 75% |
| Cairnhold | 57 s / 80% | 54 s / 81% |
| Frostline | 99 s / 85% | 103 s / 85% |
| Dustreach | 74 s / 93% | 73 s / 95% |

Measured on 2026-10-09 with the jump-boost (`jumpjet1`). The jump-boost first stranded Tower Complex's Glacier flyer: taking off under open sky, its faster climb met a lintel ahead (hence the climb check); and its tower approach reached a 56 m climb's launch mid-hop and fell short of the deck on one tank (hence landing first).

**Difficulty** is chosen in the offline menu's BOTS row and saved as `bot_difficulty` in `client.json`. The options are Easy, Normal (the default), Hard and Mixed. Normal and Hard include Hawks; Mixed picks any of the seven archetypes evenly. An unknown value reads as Normal.

The line-of-sight and reaction rules are unchanged. A bot fires only at a target it can see, after its reaction time has passed.

## Tests

- `nav_graph_connects_every_spawn_to_the_enemy_flag`
- `a_lone_bot_reaches_the_enemy_flag_on_every_map`: both teams on all five maps, within 240 s.
- `difficulty_picks_are_deterministic_and_bounded`
- `flyers_cross_every_map_mostly_airborne`
- Soak report (ignored by default):
  `cargo test --release -p peakrunner-core --lib bot_navigation_soak_report -- --ignored --nocapture`
- For a map folder in development (ignored by default, nothing happens
  without `PEAKRUNNER_NAV_PACK=<map folder>`; run with `--release ... --
  --ignored --nocapture`):
  - `nav_routes_on_a_map_folder`: every spawn's route to the enemy flag
    (steps and jets), failing if one has none, and each flag's to the
    other, printed only;
  - `ctf_bots_on_a_map_folder`: a bots-only CTF match
    (`PEAKRUNNER_NAV_MINUTES`, default 10) printing every flag pickup and
    drop; with `PEAKRUNNER_NAV_WATCH=x,z,r`, also where players are near
    (x, z) every 5 s and every death there with the few seconds before it.

## Known limits

Two cases are slow but do not fail:
- On Old Holler, a lone Glacier bot takes about 165 s: it reaches the enemy base in about 60 s, then struggles with the interior stairs and a jet under a slab.
- On Tower Complex, the long climbs into the floating bases depend on skiing speed before launch.
