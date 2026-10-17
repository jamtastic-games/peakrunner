# Duels

One on one, as many duels at once as there are pairs. A duel server keeps a
lobby, and plays each accepted challenge in its own arena on the same server.
Marker `duel1`. Branch `duel/arenas` (2026-10-14), released in `.20261014.7`;
the official one is Springdale Duels (`duel`, UDP 7782, 8 sparring bots).

The design (2026-10-14): a player enters a lobby where the other players
are listed, and anyone can challenge anyone to a duel; with nobody else
there, bots can be chosen too, and bots are always on the board. Many 1v1
duels run at the same time, and spectators can switch between them. Duels
start from random duel spots on a map (floating platforms and treacherous
duel maps can come later), with light armor's default loadout.

## How it plays

- **The lobby** (mode `duel_lobby`) is **Tenfold Tower** (`docs/tenfold.md`):
  everyone joins at a spawn point drawn at random, one on every floor or
  outside on its plaza, skis the hills round it and sees who's waiting. No weapons, no clock, no score; it never counts down or ends
  (`Match::open`). The score line reads `DUEL LOBBY · V: DUEL BOARD`. Inside
  the tower, ten floors of the **blade** side game (`docs/blade.md`): a blade
  in every hand, no jets, and it counts for nothing. A challenge
  still reaches you there, and accepting takes you to your duel.
- **The duel board** opens with the map-vote key (V; duel servers have no map
  votes), while playing or watching. It shows:
  - your record (duels won and lost since you joined);
  - challenges to you, with the seconds left to answer: Accept or Decline
    (keys 1 and 2 answer the oldest);
  - the challenge you made, and Take it back;
  - everyone on the server: whoever's in the lobby can be challenged, and
    whoever's dueling can be watched;
  - the bots, always ready: Rookie, Grunt, Rider, Skirmisher, Ace and Hawk
    (`duel::BOTS`; the Anchor holds ground, so it isn't offered);
  - the duels being played, each with Watch;
  - Concede and go back to the lobby (dueling), or Back to the lobby
    (watching).
- **Challenges** wait 20 s (`duels::ANSWER_TIME`). One outgoing challenge at
  a time: a new one replaces it. Challenging someone who has challenged you
  is a yes. While the board is closed, a challenge to you shows as a pulsing
  banner (`DUEL · ADA CHALLENGES YOU`) and a notice.
- **An accepted challenge** (or a bot picked) takes both to their own arena:
  a map drawn at random from the server's rotation, the challenger on Ember.
  - Each round starts from a pair of facing duel spots (below), new each
    round. The first round counts down the mode's warmup (5 s), later rounds
    3 s (`duel::ROUND_COUNTDOWN`), shown as `Round N · <map>`.
  - One life a round: whoever is left standing takes it. A death to the map
    or by your own hand gives it away; of a trade in the same moment, the
    first death counts. The next round starts 3 s later (`duel::ROUND_BREAK`).
  - First to 3 rounds (`score_limit`) wins; at the 10-minute clock the
    leader wins, or it's a draw.
  - Light armor's default kit: no stations, purchases, packs, turrets,
    deployables, dropped ammo or repair tool (`duel.json`).
- **The end**: the overlay says `DUEL OVER` and who won; the lobby hears it
  (`Ada beat Bo 3–1 on Old Holler`), the records go up, and 6 s later
  (`duels::RESULT_TIME`) both are back in the lobby. Conceding gives the
  duel to the other at once; so does leaving the server.
- **Watching**: Watch on the board, then the team key (J) goes to the next
  duel with room to watch (after the last, the first; a full one is passed
  over); the spectator camera works as usual (click for the
  next player, right-click the previous). Each arena takes as many watchers
  as the server takes spectators (`PEAKRUNNER_MAX_SPECTATORS`, 8 at most),
  lobby players watching included: 0 turns watching duels off.
  A full duel picked on the board, or every duel full, turns a would-be
  watcher away where they stand. Someone who
  came to watch keeps their spectator's place in the lobby while watching
  a duel (`Duels::held`: no newcomer can take it), and a queued one their
  place in the queue for when they're back (a place that opens meanwhile
  goes to those waiting in the lobby). A move between the lobby and an arena carries each
  person's chat and rename limits and address group (`teams::Carried`).

## Duel spots

- **Random** (`duel::random_spots`), for maps without their own: a pair 90 to
  140 m apart, facing each other. Tried around the middle of the two bases
  first (0.55 of the distance between them, at least 120 m), then anywhere
  on the map; in sight of each other first, then not. Each spot stands
  1.2 m over the ground or over skim water (Reefbreak's sea), on a slope
  under about 32°, under open sky, with 5 m clear every way, off lava and
  out of water, at least 48 m in from the map's edges. Every stock map but
  the stadiums has them (`every_duel_map_has_facing_spots_apart`). Which end
  goes to which team is drawn too. With none found, the teams' spawns serve.
- **Authored**: a map's manifest may list `duel_spots`, pairs of
  `[x, y, z, yaw]` (up to 64; `features::check`). A map with any uses only
  those: the way to floating platforms, ledges and duel maps with
  treacherous ground. None is authored yet.

## Running a duel server

- A rotation of `duel` entries makes one: `[{"playlist":"duel"}]` (every map
  but the stadiums), or `{"map": "...", "mode": "duel"}` entries, or the
  setup wizard's "Duels", or **Duel** on the game's Host screen
  (Multiplayer → Host game: "Maps for duels", and a slider for sparring
  bots in the tower). Only one duel mode, mixed with nothing else; a
  mod's variant of `duel` works (its armors, rounds and clock). `duel_lobby`
  can't be named. The lobby is Tenfold Tower when it's installed (it ships
  in every install's `packs/`; `sim::duel::lobby_map`), else the rotation's
  first map (in random order, a random one); it never rotates. The tower is
  in the server's content list like any map it needs.
- `PEAKRUNNER_MAX_PLAYERS` counts everyone: the lobby, the duelists, and
  lobby players away watching. At most 14 duels at once
  (`duels::MAX_ARENAS`). `PEAKRUNNER_BOTS` (`bots`) is how many **sparring
  bots** fight with blades in the lobby's tower (`docs/blade.md` "Sparring
  bots"), a person taking a bot's place as on any bot server; duel opponents
  come from the board as before. Navigation graphs for every rotation map
  are built at start.
- The status reads `<lobby map> - DUEL` (`Tenfold Tower - DUEL`); its players count everyone in the
  lobby, the arenas and away watching (bots in bot duels count, as bots);
  its roster lists the lobby and the arenas.
- Chat stays in its room: the lobby's, or an arena's (duelists and watchers).

## How it's built

- **Core** (`crates/core/src/duel.rs`): an arena's state (`World::duel`:
  spots, the decided round), spot picking, and the board's types. Rule set
  `Rules::Duel` (`rules.rs`): `Died` scores the round once and asks for the
  next (`Cmd::NextRound`); the lobby has no duel state and scores nothing.
  `Match::open` (the lobby), `Match::join_on` (a duelist's side),
  `Match::add_duel_bot` (`bots` 2: one a side less the people) and
  `Match::next_duel_round` (fresh spots and countdown; score, clock, tallies
  and sky carry on). Base modes `duel` and `duel_lobby`
  (`assets/mods/base/modes/`); `SupportedMode::Duel` and `DuelLobby`.
- **Server** (`crates/server/src/duels.rs`): the lobby and the arenas in the
  one game thread. Each arena is its own `Match` on the lobby's tick, its
  rounds numbered from its ID times 1000, so a move between rooms is always
  a new round to the client (prediction resets) and never goes back in time.
  Requests are applied once a tick's messages are read; departures settle a
  duel at the next step. Boards go out at 4 Hz when they change, up to
  `SERVER_CONTROL_LIMIT` (32 KiB: a full server's board passes the plain
  4 KiB control limit).
- **Protocol** (`duel1`): `ClientMsg::Challenge`, `ChallengeBot`, `Answer`,
  `Withdraw`, `Watch`, `BackToLobby`; `ServerMsg::Duels`.
- **Client** (`src/duel_hud.rs`): the board, the banner, the score line, the
  round caption and the result; V opens the board on a duel server (also
  watching), J goes to the next duel while watching.

## Tests

- Core: `sim::duel::tests` (spots on every map; a full duel with the score
  carried through the countdowns; the open lobby; a bot of the asked
  archetype; a bot that takes the round off someone standing still).
- Server: `tests::duels_play_side_by_side_and_people_come_back_to_the_lobby`
  (five people over TCP: two duels at once, a mutual challenge, a watcher
  switching duels, a bot duel, a concession and a departure, nobody's
  snapshots going back in time), `quic::tests::duels_travel_over_quic`,
  `tests/duel_quic.rs` (the real server app and a pinned game client over
  QUIC, as players join: it stays through the board's changes into a bot
  duel; the client used to end the session at the first change of the
  board, a control message, 2026-10-14),
  `rotation::tests::duel_entries_make_a_duel_server_and_mix_with_nothing`,
  `tests::a_spectator_watching_a_duel_keeps_their_place`, `duels::tests`
  (what a watcher and a duelist carry, a full duel, the next duel with
  room, a watcher's declined challenge) and `gameserver::tests::a_command_stays_with_the_one_who_sent_it`
  (a lobby slot emptied and taken again on one tick).
- Client: `duel_hud::tests` (the lines, the overlay, the board's keys).
- The lobby tower and its blades: `docs/blade.md` "Tests", including
  `tests::a_duel_servers_lobby_is_the_tower_where_blades_count_for_nothing`.
- In the game: `launch_smoke` with `QA_LOCAL=1
  QA_VOTE_ROTATION='[{"playlist":"duel"}]'`, plus `QA_CROWD=4 QA_DUELS=1
  QA_VOTE_PANEL=1` (the board with two duels running), `QA_DUEL=Rookie` (a
  bot duel) or `QA_DUEL=watch`; `QA_ON_TOP=1` keeps an unfocused
  (`QA_DRIVE`) window drawn (`src/qa_overrides.rs`). The Host screen:
  `QA_HOST=duel` opens it on Duel and `QA_HOST=duel,start` presses Host and
  join (with `PEAKRUNNER_CONFIG_DIR` on a folder whose `host/server.json`
  says `"announce": false, "upnp": false`; `peakrunner-server` beside the
  example's binary).

## Open

- Seen in a game window on 2026-10-14 (the lobby tower, the blade, its HUD,
  other players holding theirs); live on Springdale Duels since
  `.20261014.7`, where a QUIC client over the internet duelled a bot; not
  yet played by people.
- No duel spots authored yet, and no duel maps (floating platforms,
  treacherous terrain).
- Records last a session; nothing is kept.
- No offline duel against a bot.
