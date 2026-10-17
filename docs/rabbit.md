# Rabbit and Team Rabbit

Two stock modes built around one neutral gold flag, after the classic
Rabbit and Team Rabbit. Both are offered and run on every map without a
football field (`map_catalog::supports`); a stadium hosts them only when a
rotation entry names it. Mechanics are in `crates/core/src/rabbit.rs`,
scoring in `crates/core/src/rules.rs` (and the same rules as scripts in
`src/assets/mods/base/rules/rabbit.rhai` and `team_rabbit.rhai`), the mode
files in `src/assets/mods/base/modes/`. Compatibility marker: `rabbit1`.

## Rules

- **Rabbit** (`rabbit`, short `RAB`), by the classic rules (marker
  `rabbit2`). The flag
  starts midway between the two flag stands, on the highest floor there
  (`World::rabbit_home`). Whoever holds it is the rabbit.
  - Everyone else hunts the rabbit as one side: hunters can't hurt each
    other, the rabbit and the hunters can hurt each other (`World::foe`,
    which `World::hostile` and the HUD use). With nobody holding the flag,
    nobody can hurt anyone.
  - Taking the flag scores **1**, and a player scores **1** more each time
    their time holding it passes another **15 seconds**, all their holds
    in the round added together (`rabbit::HELD_POINT`,
    `RabbitFlag::times`).
  - A hunter's kill of the rabbit scores 1 (counted before the flag
    drops); each of the rabbit's kills scores **2**. A suicide costs
    nothing.
  - The limit is the classic's 1200 points (`score_limit`), so the leader
    (then fewest deaths) at the 10-minute clock wins.
  - Hunters see the rabbit (its label and an arrow at the screen's edge)
    for **6 seconds** when it takes the flag (`PING_SECONDS`), then again
    **24 seconds** after each ping ends (`PING_EVERY`), but only if nobody
    has hurt the rabbit in the last 24 seconds; else the try waits another
    24 (`RabbitFlag::ping`). Anyone who spawns while someone holds the flag
    sees the rabbit for 6 seconds. Hunters always see where the flag goes
    home.
  - The rabbit can't throw the flag away. Dropped (the rabbit died), it
    lies `flag_return` seconds (**45**) and goes home unless someone takes
    it.
  - No mortars or mines (`banned`); as in Deathmatch, any station serves
    anyone, turrets and sensors stand idle, and packs can't be deployed.
  - The rabbit may repair themselves like anyone.
- **Team Rabbit** (`team_rabbit`, short `TR`): two teams, each with
  **50 lives** (`score_limit`). The team score counts the other team's lives
  taken.
  - Every death costs the dead player's team one life, whoever or whatever
    did it (`GameEvent::Died`). A kill by the team holding the flag costs
    **two**.
  - There's no flag at the start. The first death drops it where it
    happened. Dropped and left `flag_return` seconds, it vanishes, and the
    next death drops a new one.
  - The carrier may throw it to a teammate (Ctrl+F), as a CTF carrier throws
    theirs; they can't take it back for a moment (`TOSS_GUARD`).
  - Taking the other team's last life wins; at the clock, the team that took
    more (a draw is possible).
  - The rabbit can't repair themselves (inventory stations don't heal
    either), so a hurt rabbit stays hurt.
- **Both:** a flag is taken by touching its pole, as in CTF. Rounds keep
  the map's own look (`conditions` false; hosts can still choose the sky).

## Bots

Bots go for a loose flag, and chase a hostile rabbit (`World::rabbit_goal`).
In Rabbit the hunters target only the rabbit, and the rabbit runs: of the
map's spawn points and flag stands, it heads for the one farthest from its
nearest chaser (`rabbit_flight`), repairing itself when it's quiet, and
falls back to a station only when out of rounds. In Team Rabbit the
carrier fights on with its team, neither repairing itself nor falling back.

## Players' view

- The flag is drawn on its pole, gold (`player_model::RABBIT_FLAG`), and on
  the rabbit's back. The minimap and full map mark it; in Rabbit, on the
  rabbit's back, only during a ping for hunters, as the HUD's label
  (`rabbit_hud::rabbit_hidden`): not always visible, but never hidden for
  too long.
- In Rabbit (not Team Rabbit) the rabbit's armor is the flag's gold on
  every plate, lit so it shows in any shade, and every hunter wears the
  same steel, the armor as drawn (`player_model::livery`; since 2026-10-14):
  the flag's color, bright, so the rabbit can't hide. The hidden
  team players are given shows nowhere: not in the armor, the weapon's
  paint, the jet flame, the shots (laser beams, mines, round models and
  rounds built from parts take a neutral trim, `drawlist::side_accent`) or
  the minimap's dots (the rabbit gold, the rest steel). Before, the team-colored armor of
  `.20261014.2` made hunters look like two sides.
- Arrows and map dots need sight (since 2026-10-14; before, the rabbit saw
  nearly the whole field through the hills, so now it sees others only as
  usual: within range and in line of sight). An enemy's arrow shows, as in the other
  modes, within 250 m (times a sight's magnification) and in clear sight,
  or when the viewer's own target marker marked them
  (`world_overlay::visible_marker`), and in Rabbit the minimap and full
  map show an enemy only then (`world_overlay::in_sight`). Before, every
  player counted as sensed, so the rabbit saw every hunter within 400 m
  through hills and on the full map, and hunters the rabbit's arrow and
  its flag on the maps outside a ping.
- `src/rabbit_hud.rs`: the flag or the rabbit is labeled in the world and at
  the screen's edge (in Rabbit the rabbit only during a ping, "KILL THE
  RABBIT · name", with "FLAG HOME" where it returns); a status line ("YOU
  ARE THE RABBIT · NEXT POINT IN 3",
  "KILL BONUS ×2", "GET TO THE FLAG"); announcements worded for the viewer
  ("X IS THE RABBIT", "KILL THE RABBIT" at each later ping, "THE FLAG IS
  DOWN", "A FLAG HAS FALLEN", "YOUR TEAM HAS THE FLAG", "X HAS TAKEN THE
  LEAD"). Team Rabbit's top line shows each team's lives left. Rabbit's
  scoreboard has each player's time held and the rabbit's name in gold.
- Online, the flag comes in every snapshot (`Snapshot::rabbit`, the delta
  codec's fifth fixed part); grabs, drops and returns play the flag sounds.
- The Library has a page for each mode.

## Rules scripts

The engine reports `on_rabbit_grab(game, player)`, `on_rabbit_held(game,
player)` and `on_died(game, victim, killer)` (every death that counts,
after any `on_frag`; `killer` -1: none) to rules scripts, and `game.rabbit` gives the
flag's holder (-1: nobody). A mod can vary either mode like any other
(`docs/making-content.md`).

## Server and QA

- Rotation entries `{"map":"<key>","mode":"rabbit"}` or `"team_rabbit"`; the
  playlists `rabbit` and `team_rabbit` cover every map but the stadiums. The
  setup wizard offers both (6 and 7, after the mixed rotation).
- `QA_LOCAL=1 QA_MODE=rabbit QA_MAP=<key>` (or `team_rabbit`) runs a local
  server in that mode.
- **Tests:** `sim::rabbit` (taking, holding, dropping, returning and
  vanishing; who may throw; bot goals; scoring; Rabbit's sides, time held
  over several holds, pings, bans and repairs; and offline bot matches in
  both modes that take the flag, hold it for points and take lives),
  `rabbit_hud::tests`, the A/B gate in `crates/script` (each script gives the
  Rust rules' commands), `rotation` and `setup` tests, and four behaviour
  traces (`offline rabbit cairnhold`, `offline team_rabbit dustreach`,
  `server team_rabbit frostline bots10`, `clock rabbit cairnhold`).

## Limits

- Rabbit's flag starts at the midpoint of the CTF stands, which suits the
  stock maps; a classic map places its own, and a map here can't yet name
  its own rabbit spot.
- In Rabbit, players still have a team internally (balance), as in
  Deathmatch; it decides nothing about who may hurt whom, and their armor
  doesn't show it.
- Not carried over from the classic: damage outside the mission area for
  the rabbit (these maps have none), stations that slowly repair (no mode
  here has them), blasts that push teammates without hurting them (here a
  teammate's blast does neither), and the rabbit bot's search for health
  and ammo pickups (it repairs itself and visits a station when dry).
