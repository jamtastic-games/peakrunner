# The blade

A side game a map can hold in its **blade zones**: inside one, every hand
holds a blade, the jets are off, and players run, jump and fight up close.
What happens there counts for nothing. Tenfold Tower, the duel servers'
lobby (`docs/tenfold.md`), is ten floors of it. Marker `blade1`. Branch
`duel/arenas` (2026-10-14), released in `.20261014.7`.

The design (2026-10-14): an open lobby building where people run around
with energy blades and attack each other, a side game that counts for
nothing (die, and spawn 5 seconds later). No flying, just running. Each left
click swings a different kind of swing: a click and release is a quick one,
a click and hold builds power for a bigger swing, and rapid clicks make
combos. Right click blocks. Every action takes stamina, blocking wears you
down even when nothing hits, and a hit takes stamina like health, then
health once stamina runs out. In the code and in the game it's a **blade**.

## How it plays

- **Inside a blade zone** the blade comes out and the jet key guards. Out of
  the zone it goes away; nothing else changes (skiing and jumping work
  everywhere).
- **Fire**:
  - a **tap** (let go within `tap`, 0.2 s) is a **quick swing**, the three
    in turn: right to left, left to right, overhead;
  - a **hold** builds **power** (`charge`, 1 s to full); letting go swings
    the **power swing**, its damage and push up to `charged` (2.2) times, its
    stamina up to `charged_cost` (1.8) times;
  - **combos**: a press made during a quick swing, or within `combo` (0.45 s)
    of one, carries the combo on, and a quick swing's follow-through gives
    way to it. A third quick swing in a combo is the **whirl** (all the way
    round); a hold after one quick swing is the **lunge** (a dash ahead with
    extra reach); a hold after two is the **slam** (a hop and a smash over a
    wide arc).
- **Guard** (the jet key, right click): blocks blows from `guard_arc` (150°)
  ahead and slows you to `guard_walk` (half speed). No swings while on
  guard. Holding it costs `guard_drain` (12) stamina a second; an empty tank
  drops it.
- **Stamina** is the energy tank, counted in points of 100. Every swing
  costs its `stamina`; it comes back at `regen` (28) a second once nothing
  has been swung or guarded for `rest` (0.7 s). An empty tank swings
  nothing.
- **A blow**:
  - unguarded, it **cuts**: its damage off health, a push (`push`, m/s) away;
  - on a guard, **stamina pays**: `guard_cost` (1) stamina per point of
    damage; what stamina can't pay comes off health and **breaks the guard**
    (reeling `reel`, 0.6 s); the swing glances off and its swinger reels a
    moment;
  - blades swinging at each other in the same step **clash**: both reel,
    nobody is hurt.
- **Reach**: `reach` (2.3 m) from the body's middle, plus a swing's own; up
  to 1.9 m above or below; never through a wall.
- **Counts for nothing**: a death in a blade zone gives no frag, takes no
  life, goes in no feed and calls no rule set; the fallen are back after
  `respawn` (5 s). In a duel server's lobby no death counts, in a zone or
  not; outside the zones (a fall off the plaza, a bump, Ctrl+K there) the
  fallen come back after the lobby's own respawn, with YOU DIED, not CUT
  DOWN. In
  the duel lobby's tower people come back, as they join,
  at a spawn point drawn at random from all of them, one on every floor and
  the plaza's (`World::lobby_spawn`: at least one on every floor and one
  outside, so the spawn point is random); sparring
  bots on their own floor; elsewhere, at one of the zone's own `spawns`, the
  one farthest from everyone (`World::blade_spawn`).
- The numbers (`defs.json` `blade`, `defs::Blade`):

| Swing | Seconds | Cuts during | Arc | Damage | Stamina | Push | Extra |
|---|---|---|---|---|---|---|---|
| quick (×3 in turn) | 0.42 | 0.08–0.2 s | 130° (overhead: a third) | 10 | 9 | 3 | |
| power | 0.7 | 0.12–0.3 s | 150° | 16 (to 35) | 14 (to 25) | 5 (to 11) | |
| whirl | 0.7 | 0.1–0.46 s | 360° | 14 | 22 | 7 | |
| lunge | 0.6 | 0.06–0.28 s | 50° | 18 | 18 | 6 | dash 14 m/s, reach +1 m |
| slam | 0.85 | 0.34–0.46 s | 220° | 22 | 26 | 10 | hop 6 m/s, reach +0.6 m |

The damage was halved on 2026-10-14 ("the fights end too quickly. lets
make it take a little bit longer"): two sparring bots now take about 4.5 s
to cut down someone standing still (2.4 s before).

## On screen

- First person (`src/blade_look.rs`, drawn with the world:
  `drawlist::push_first_person_blade`): both hands on the hilt, the blade
  glowing in one of six colors by player, a smear behind each cut. Third
  person and other players: the body's `Hold::Blade`, posed by the same
  swing functions. Sparks and a sound where every blow lands, read from each
  player's latest blow (`blade::State::struck`, `how`, `by`), and a whoosh as
  each swing starts (`effects::watch_blades`).
- HUD: the bar reads **Stamina**; the crosshair is a dot in a ring that fills
  orange with a power swing's charge; **GUARD** and **STAGGERED** under it;
  the zone's name ("Floor 6 · Foundry") and the controls by the bars; the
  weapon line reads "Blade" (with the power while charging).
- Sounds (`sound.rs`): `blade-swing`, `blade-heavy`, `blade-cut`,
  `blade-block`, `blade-clash`, synthesized like every cue.

## How it's built

- **Core** (`crates/core/src/blade.rs`, `sim::blade`): the state on each
  player (`Player::blade`: swing and steps into it, power, the press, combo,
  guard, reel, rest, the latest blow), counted in physics steps so a client
  predicts its own blade exactly. `World::blade_step` reads fire and the jet
  key each step (both sides); `blade_cuts` (server and offline, after
  everyone has moved, so meeting blades clash whoever was stepped first)
  sweeps each swing's arc against bodies in blade zones. Jets are off and the
  armor's recharge gives way to the blade's in a zone (`step_players`).
- **Zones** (`features::ZoneKind::Blade`; `docs/map-pipeline.md`): a zone may
  be round (`radius`), carries its `spawns`, and its `name` shows on the HUD.
- **Wire** (`blade1`): the player codec's thirteenth group carries the blade
  state; inputs are the existing fire and jet bits.
- **Skim surfaces** (`water::skim_surface_at`): water or lava a body stands
  on counts only within `SKIM_BELOW` (1.5 m) under the body, so an upper
  floor's pool or molten pit is no floor to the floor below it (everyone
  under one was lifted through the ceiling onto it). Maps with one level of
  skim water behave as before.

## Tests

- Core, on the real tower (`sim::blade::tests`): ten blade floors with
  grounded spawns; walking the spiral from the ground to the crown with the
  real movement; jets off and guard on inside, jets back outside; taps in
  turn and quick taps into a whirl; holds into power, the lunge and the slam;
  stamina for swings and an empty tank; a cut, a guard paying in stamina, a
  broken guard paying in health; a clash; a death that counts for nothing
  and comes back on its floor after 5 s; molten pits that kill, the floor
  below them untouched, water that slows, a wader jumping back out.
- Server (`tests::a_duel_servers_lobby_is_the_tower_where_blades_count_for_nothing`):
  over TCP, the duel lobby is the tower, cuts land and reach the snapshots,
  no frag or feed line, back on a crown spawn after 5 s.
- Protocol: blade fields in the random-state delta round trip.
- Client: `blade_look::tests` (continuous poses, smears only in cuts), the GPU
  captures `render_blade_captures` (`PEAKRUNNER_CAPTURE_DIR`, `QA_ONLY`).
- In the game: `launch_smoke` with `QA_LOCAL=1
  QA_VOTE_ROTATION='[{"playlist":"duel"}]' QA_START=1038,172.6,1024,90`
  (on the crown) and `QA_DRIVE` with `fire` and `jet`.
- Timing: `sim::blade::tower_load` (ignored; release): 28 people walking and
  swinging through the tower take about 1.1 ms a tick on average, 1.3 ms at
  worst, against 0.1 ms on Old Holler (16.7 ms budget). The tower's 12,800
  collision triangles share four 32 m columns of the collision grid.

## Sparring bots

Since 2026-10-14 bots spar with blades under parody names that cross two
famous space franchises (Jean Luc Skywalker, Luke Q, Data the Dog, Jabba
the Klingon), as the football bots' parody real players. They are kept in
one list (`blade::SPARRING_NAMES`, 26), easy to swap.

- **Seen**: a blade is drawn only while some of its holder's body (head,
  chest or hips) can be seen (`drawlist::body_in_sight`), and its sounds and
  sparks come only from your own floor or someone in sight: a fight on the
  floor above, seen up the stairwell, showed swords swinging with nobody
  holding them ("a random floating sword with no bot that liked to swing").
- **Where**: a map with blade zones in a mode without weapons (the duel
  lobby's tower, `World::sparring`). A duel server's bots (`PEAKRUNNER_BOTS`,
  `with_bots`) fill the lobby with them, a person taking a bot's place as on
  any bot server (`Match::maintain_bots`). Each has a floor, two to a floor
  from the ground up (`spar_home`), where it appears and comes back; one
  pushed out of the tower is back on its floor within 3 s.
- **How they fight** (`World::spar_wish`, in place of the route-following
  bot; `think_bots` leaves them be): the nearest on their floor, people
  first (bots spar with each other while nobody's there); close to a
  blade's length, circle, and swing when ready: mostly quick swings, a
  whirl or a slam now and then, a power swing (more often against a guard),
  the odd lunge from a few meters; guard a swing they see coming, as often
  as each bot is wary (35 to 80%); back off when winded (stamina under
  24); hop what they walk into. Each bot's pace (0.28 to 0.68 s between
  attacks) and wariness are drawn when it first takes its floor.
- **Molten metal**: never toward it for as far as a run takes to stop (4 m
  ahead, the first clear way round), away from its momentum toward it, a
  margin of 1 to 2 m off any molten edge, no hops (a slam's, or over what
  it's stuck on) within 2.6 m of it, and no lunge whose dash crosses it.
  With molten metal between it and its foe, a bot walks round the floor
  (the shorter way about the zone's middle) until the way is clear, and it
  wanders only to a spot it can walk straight to; before, either left it
  standing at a rim (2026-10-15, `docs/tenfold.md` "Traps").
- Tests: `sim::blade::tests::sparring_bots_fight_on_their_floors` (eleven
  bots on the real tower, two to a floor, their names, a person on the
  ground floor and then on the foundry's walkway cut down by them, no bot
  ever in molten metal), `tests::a_duel_servers_bots_spar_in_the_tower`
  (over TCP: named sparring bots on the lowest floors, one making room),
  and, ignored (minutes), `tower_traps::sparring_bots_never_stick` (twenty
  bots, two a floor, ten minutes: none still for 20 s with nobody near,
  none in the wall) and `nobody_gets_stuck_in_the_tower` (`docs/tenfold.md`).
  In a game window: `launch_smoke` with `QA_LOCAL=1
  QA_VOTE_ROTATION='[{"playlist":"duel"}]' QA_BOTS=4
  QA_START=1028.5,100.6,1016.2,330` (the ground floor).

## Open

- The floors are dressed once from the concept sheet (`docs/tenfold.md`);
  their layouts are still the test levels'.
