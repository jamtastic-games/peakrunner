# Deathmatch and Team Deathmatch

Two source-only modes. **Only Team Deathmatch is offered for now**
(`SupportedMode::offered`). Free-for-all Deathmatch is implemented and
tested, but the menu doesn't list it and server rotations reject it.

Team Deathmatch runs on every map except the football stadiums
(`map_catalog::supports`). A stadium hosts it only when a rotation entry
names it explicitly. Rules are in `crates/core/src/deathmatch.rs`; the
per-round conditions in `crates/core/src/conditions.rs`. Compatibility
marker: `dm1`.

## Rules

- **Deathmatch** (`deathmatch`): every player for themselves.
  `World::hostile(owner, team, victim)` is the one enemy check used by discs
  and every projectile, blasts, the laser, rail slugs, mines, body checks,
  frag credit and bot targeting. In Deathmatch everyone but you is hostile;
  otherwise it is the other team. First to **20 frags** wins, or the top
  fragger (then fewest deaths) at the 10-minute time limit. A suicide costs a
  frag. Any inventory station serves anyone. Turrets and sensors stand idle
  and packs can't be deployed, because they take a side. Players spawn at
  any authored point of either team, choosing among four random candidates
  the one farthest from the nearest living opponent.
- **Team Deathmatch** (`team_deathmatch`): each enemy frag scores one for the
  team. First to **40**, or the higher team at 10 minutes. Stations, turrets
  and deployables work as in CTF.
- **Both modes:** no flags or capture points. Bots hunt the nearest enemy when
  they can't see one.

## Random conditions

At each round start the server rolls `Conditions` from the world's random
stream, which is seeded from the clock at server start and mixed with the tick
at every round. The result goes to clients in every snapshot, so everyone sees
the same round. Other modes always use the default, the map's own look.

| Factor | Values (weight) |
| --- | --- |
| Time of day | dawn 1, day 2, dusk 1, night 1.6 |
| Weather | clear 3, overcast 1.5, rain 1.6, storm 0.8, snow 1, fog 1 |
| Wind | random direction; 0–1.5 m/s clear or fog, 2–6 m/s rain or snow, 9–14 m/s storm |
| Twist | none 4, *Rifles for all* 1, *Glass cannon* (×1.5 damage) 1, *Heavies only* 1 |

- **Rendering:**
  - Baked lightmaps assume the map's sun, so the sun never moves. Dawn and
    dusk warm and dim it; night turns it into a cool, dim moon, with a
    darker exposure and a dark blue ambient.
  - Changing the time of day or the weather replaces the sky with a
    procedural one: the gradient for that time, and cloud cover and
    darkness for the weather.
  - Fog colour and distance follow the conditions, and weather thickens the
    height fog.
  - Players and projectiles are lit by `entity_light`.
- **Rain and snow:** up to 640 drops (rain streaks along the wind, drifting
  snowflakes) in a box around the camera.
  - Each drop stops at the first surface below it, roofs included (a map
    collision sweep).
  - A drop with anything solid above it is never drawn, so it doesn't rain
    indoors, though it does through roof openings.
- **Twists:** *Rifles* gives every spawn its armor's rifle; *Heavies* spawns
  everyone in heavy armor (refilled); *Glass cannon* multiplies damage taken.

## Server and QA

- Rotation entries: `{"map":"<key>","mode":"team_deathmatch"}` (any map,
  stadiums included). The playlist `team_deathmatch` covers every map but
  the stadiums. The directory shows the mode as TDM.
- QA:
  - `QA_LOCAL=1 QA_MODE=team_deathmatch QA_MAP=<id>` runs a local server
    in that mode.
  - `QA_CONDITIONS="night,rain"` forces the time and weather on the client,
    as a view change only.
  - Captures used for tuning are in `screenshots/dm/` (private).
- **Tests:**
  - `sim::deathmatch::tests` covers hostility, team-kill damage, frag limits,
    suicides, varied rolls, twists, and bots fragging each other: 13 frags in
    2 minutes.
  - `conditions::tests` covers the default being the identity, roll
    coverage and night darkness.
  - `rotation::tests` covers the playlists.

## Limits

- Night can't relight baked lightmaps. It dims and cools them through
  exposure, so interiors keep their own light layout.
- There's no rain audio yet.
- Deathmatch still assigns each player a team internally, for team
  balance. In free-for-all everyone wears the same steel armor, the team
  showing nowhere on the body (`player_model::livery`, since 2026-10-14);
  names under the crosshair and markers treat everyone as an enemy.
