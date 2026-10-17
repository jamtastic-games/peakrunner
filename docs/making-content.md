# Making content

PeakRunner's engine loads its content from folders at run time; nothing you
add is compiled in. A mistake in a folder is reported and that item is left
out: it never crashes the game or the server. The design behind this is
`docs/modding.md`; this page is the practical guide.

## Where content lives

A **content folder** holds `maps/<key>/` and `mods/<id>/` folders. The
engine reads these, most preferred first; the first folder that has a key
wins:

| Folder | Who reads it | Edit it? |
|---|---|---|
| Folders named by `PEAKRUNNER_PACK_DIR` (several, separated like `PATH`) | The game and servers started with it | Yes: authors and testing |
| A server's `content/`, beside its `server.json` | That dedicated server | Yes: what the server adds |
| The game's own `content/`, in the Host game's folder: `~/Library/Application Support/PeakRunner/host/content` (Mac), `%APPDATA%\PeakRunner\host\content` (Windows), `~/.config/PeakRunner/host/content` (Linux) | The game (offline too) and the server it hosts | Yes: a player's own maps and mods |
| `packs/` beside the game (`PeakRunner.app/Contents/Resources/packs` on a Mac) or the server | The default maps and the base game an install ships | No: updates replace it |
| `src/assets/` in the source tree | Development builds (the default maps' sources, `mods/base/`) | Maintainers |

Maps a server downloads for a player are kept apart, in a cache the game
manages (one folder per version); never edit it.

```sh
peakrunner-server --check                               # this server's content/
peakrunner-server --check $HOME/peakrunner-content      # any content folder
export PEAKRUNNER_PACK_DIR=$HOME/peakrunner-content     # read it, before the rest
```

Folders are read when the program starts: restart the game or server after
adding content.

## Add a map

1. Pick a **key**: 1 to 32 lowercase letters, digits and inner hyphens
   (`river-fort`). The folder name is the key; it is how rotations, votes and
   server configs name the map. Never reuse a default map's key for a
   different map.
2. Build the map with the map pipeline (`docs/map-pipeline.md`) into
   `<content folder>/maps/<key>/`: `map.json` plus the payloads it lists
   (`vertices.bin`, `collision.bin`, `height.bin`, `weights.rgba`,
   `textures.rgba`, `ambient.f32`, optional `shade.rg`, `props.bin`,
   `lava.bin` (lava lying on the ground) and `models.glb`, the authored
   models a map may place). Each
   payload may also be stored zlib-compressed as `<name>.zlib`. Every file's
   SHA-256 must match `map.json`; the pipeline writes them.
3. Write `<content folder>/maps/<key>/info.json`:

   ```json
   {
     "name": "River Fort",
     "note": "Two forts across a fast river; bridges and a flooded tunnel.",
     "dust": [0.42, 0.38, 0.30]
   }
   ```

   | Field | Meaning | Default |
   |---|---|---|
   | `name` | Menu name, 1 to 48 plain characters | `map.json`'s `name`, else the key |
   | `note` | Menu description, up to 400 characters | none |
   | `bases` | The two base centres, `[[x,y,z],[x,y,z]]` | the flag positions |
   | `dust` | Colour of blast dust, three numbers 0 to 1 | a soil brown |

   Unknown fields are refused, so typos show up in the check.
4. Check it: `peakrunner-server --check <content folder>`. Each map is
   validated exactly as the game loads it and listed with the modes it can
   host; the command exits 1 if anything is wrong.
5. Play it: put it in the game's own `content/` (or start the game with
   `PEAKRUNNER_PACK_DIR` set). The map appears under every mode its
   `map.json` supports:
   - **CTF**, **Team Deathmatch**, **Rabbit** and **Team Rabbit**: every
     map without a football field;
   - **Capture & Hold**: two or more `control_points`;
   - **Football**: a `football` field (then Football only).

What the engine needs from `map.json` (flags, spawns, equipment, capture
points, water and lava, vents, football field, look) is described in
`docs/map-pipeline.md`, `docs/capture-and-hold.md` and `docs/football.md`.

## Share a map by hosting it

Players don't need to install your map. Put it in the server's content
folder (a dedicated server's `content/` beside `server.json`; the Host
screen's server reads the game's own `content/`) and name its key in the
rotation:

```json
{"rotation": [{"map": "river-fort", "mode": "ctf"}, {"playlist": "ctf_cnh"}]}
```

Anyone who joins and lacks it is asked "This server needs 1 map you don't
have: River Fort (24.0 MB)"; after they choose Download and join, the game
fetches it from your server, checks every file, and keeps it for next time.
Declining downloads nothing. LAN servers can't send maps, so LAN players need
the folder themselves. A rotation may use at most 32 different maps. The
official servers add content the same way.

## Make a mod

A mod changes how the game plays without touching code: numbers (gravity,
weapons, armor, equipment), new armors, weapons, gadgets and fliers built
from the engine's classes, and new modes. It is a folder, `mods/<id>/` in
your content folder (the folder name is the mod's id: 1 to 32 lowercase
letters, digits and inner hyphens), holding only these files:

| File | What | Required |
|---|---|---|
| `mod.json` | `{"name": "Low Gravity", "note": "What it does.", "api": 1}` (`api` stays 1 until the full release) | Yes |
| `defs.json` | Only the world numbers you change | No |
| `armors/`, `weapons/`, `projectiles/`, `blasts/`, `throwables/`, `deployables/`, `damage/`, `effects/`, `packs/`, `items/`, `vehicles/` | Entries of the engine's classes: patches of the base game's, or new ones | No |
| `sounds/<name>.wav` | Short sounds (64 KiB at most) your entries name | No |
| `modes/<key>.json` | A new mode, a variant of an existing one | No |
| `rules/<key>.rhai`, `hooks/<name>.rhai` | Server scripts: a mode's rules, and hooks definitions name (never sent to players) | No |

Ready-made examples to copy are in `src/examples/mods/`: small ones
(`low-gravity`, `quick-ctf`, `sidearms`, `spree`, `disc-turret`,
`drain-gun`, `guardian`) and a whole game on top of the base one
(`sparkfall`: nine classes, gadgets, fliers, a team purse and scoring).

### Change definitions

The base game's items are entries in class folders of
`src/assets/mods/base/`: `armors/` (light, heavy, football), `weapons/`
(disc, chaingun, grenade launcher, mortar, laser, railgun, plasma gun,
blaster), `projectiles/`, `blasts/`, `throwables/` (grenade, mine) and
`deployables/` (turret, wall, force field, ammo station, field inventory).
Its world numbers are in `defs.json`: `movement` (gravity, mass, jumps,
speed limits, walking, air and ski control, the energy tank), `repair`,
`equipment` (sensors, turrets, hulls, shields), `placement` and `blade` (the
blade of maps' blade zones: its swings, guard and stamina; `docs/blade.md`).

A mod names an entry by its id and changes only what it writes; a new id
adds an entry, and `"extends"` copies one first:

```json
// weapons/discs.json
{ "disc": { "reload": 0.8 },
  "rapid-disc": { "extends": "disc", "name": "Rapid disc", "reload": 0.5 } }
```

```json
// defs.json
{ "movement": { "gravity": 12.0 } }
```

The whole format (patching, replacing, extending, removing with `null`,
every field and its range) is `docs/mod-format.md`, and
`peakrunner-server --schema` prints the field list. Older mods' `defs.json`
item paths (`"weapons": { "disc": { "reload": 0.8 } }`) still work.
Misspelled or unknown fields and out-of-range numbers are refused with the
file, entry and field named.

### Recombine what the engine can do

Every capability is a class, and every entry names the ones it uses by id,
so a mod mixes them freely. Five short walkthroughs (the full files are in
`src/examples/mods/`):

**Put the disc on a turret** (`disc-turret`): a deployable that extends the
base turret and fires the disc launcher's round, and a pack that places it.

```json
// deployables/disc-turret.json
{ "disc-turret": { "extends": "turret", "name": "Disc turret",
    "turret": { "range": 60.0, "sensed_range": 80.0, "speed": 95.0, "life": 5.0, "cooldown": 1.6,
      "leads": true, "fires": true, "projectile": "disc" } } }
// packs/disc-turret.json
{ "disc-turret": { "name": "Disc turret", "use": "place", "deployable": "disc-turret" } }
```

**Change a held beam** (`drain-gun`): the base game's drain gun is a
`sustained` weapon firing a `stream` projectile (a held beam that drains
energy). The example patches only the beam's `stream`: farther, and it
mends teammates and repairs equipment. Giving a weapon to an armor is one
line in the armor (`"weapons": { "drain-gun": 0 }`, 0 for a weapon paid with
energy); taking it away is `null`.

**An armor with a plate and a class key** (`guardian`): `"extends":
"heavy"`, a `mounts` entry with a `blocker` (stops 60% of what comes from
ahead), and an `ability` used with C that gives an effect for ten seconds.

**A pack**: `use` says what the deploy key does. `"use": "toggle"` with
`effects` and a `drain` (energy a second, or `drain_regen`, a multiple of
the wearer's recharge) is a shield or a cloak (a shield effect's `absorb` is
the energy a point of the hit costs, before armor); `"use": "use"` with an
`action` fires a weapon from the shoulder, throws, places, gives an effect,
shoves along the view, or recalls to a beacon:

```json
{ "booster": { "name": "Booster", "use": "use", "energy": 10.0, "cooldown": 2.0, "charges": 2,
    "action": { "impulse": [12.0, 6.0] } } }
```

**A hook** (`guardian`): the armor names `"hook": "guardian"`, and
`hooks/guardian.rhai` answers its events with commands, on the server only:

```rhai
fn on_death(game, ev) {
    if ev.killer >= 0 { team_feed(game.team(ev.victim), "Guardian " + game.name(ev.victim) + " fell"); }
}
```

Beyond these: damage kinds and resistances, status effects, items, map
features (moving parts, zones), vehicles, looks built from parts, sounds,
an economy and bot hints, each in `docs/mod-format.md`.

### Add a mode

A mode is a variant of one of the game's modes (`ctf`, `capture_and_hold`,
`football`, `team_deathmatch`, `deathmatch`, `rabbit`, `team_rabbit`, `duel`)
or of another mode in your mod. A variant of `duel` makes a duel server when
a rotation names only it (`docs/duels.md`).
Name the one you vary as `base` and change any of its fields; the file name is
the new mode's key:

```json
{ "base": "ctf", "label": "Quick CTF", "short": "QCTF",
  "note": "Five captures to win, five-minute rounds.", "goal": "First to 5 captures",
  "start_message": "QUICK CTF — FIRST TO FIVE", "clock": 300.0, "score_limit": 5 }
```

Fields (`src/assets/mods/base/modes/ctf.json` shows them all): `label`,
`short`, `note`, `goal`, `start_message`; `clock` (seconds) and `halves`;
`warmup`, `respawn`, `suicide_delay` (seconds); `score_limit`; `teams`
(`two` or `free_for_all`, which goes with deathmatch rules);
`control_points` (`all`, `ctf_active`, `none`); `conditions` (random time,
weather and twist); `movement` (`skier` or `classic`); `armor_scaling`;
switches for `equipment`, `deployables`, `ammo_stations`, `throwables`,
`loot`, `repair_tool`, `purchases`; `weapons`; `banned` (weapons and
throwables taken out of play by id, whatever the rest allows: Rabbit's
`["mortar", "mine"]`); `requires`
(`football_field`, `control_points`: what a map needs to host it);
`flag_return` (seconds a dropped flag lies before going home, 14);
`friendly_fire` (the share of damage teammates' fire does to each other, 0);
`spawn_armor` (`setup`, the armor each player chose, or `random`, one of
those stations offer, drawn at every spawn); `armors` and `packs` (by id:
what its stations offer, in that order, whether or not an entry is marked
`offered`; without a list, every `offered` entry, armors light to heavy);
`player_drops`
(the drop keys let players drop the weapon in hand and their pack for
others); `death_drops` (`pack`, `weapon`, and `items` by id: what a death
leaves for anyone to take); and `economy` (credits: `shared` by the team or
each player's, `start`, `max`, `per_minute`, and what a `frag`, a `capture`
and a `return` earn; armors, weapons and packs cost their `price`, and a
player's setup comes back at respawn only if it can be paid for again,
else they spawn with the kit). The rule set (what
scores and what wins) comes from the mode you vary.

`weapons` lists the guns players may use, by id: the base game's `disc`,
`chaingun`, `grenade_launcher`, `laser` (light armor only), `mortar` and
`railgun` (heavy armor only), `plasma`, `blaster`, `drain-gun` and
`target-marker`, any a mod adds, or none
(`[]`). Left out, every weapon in the session. Players
choose among them at stations (as many as their armor's slots); bots use
only these, and a mode with none leaves the hands empty.
Grenades and mines follow `throwables`:

```json
{ "base": "ctf", "label": "Disc CTF", "short": "DCTF",
  "weapons": ["disc"], "throwables": true, "purchases": false }
```

### Write rules (a script)

Changing numbers and switches can't make a new way to score. For that, a
mode names a **rules script**: `"script": "spree"` in `modes/<key>.json`,
with the script in the mod's `rules/spree.rhai`. The script decides what scores, what wins and what is announced;
the engine still runs everything else (movement, weapons, flags, capture
points, the ball) from the mode it varies. `src/examples/mods/spree/` is a
whole example, and `src/assets/mods/base/rules/` holds the base game's own
rules written the same way.

Scripts are [Rhai](https://rhai.rs). Define any of these (a missing one does
nothing):

```rhai
fn on_flag_captured(game, player) { }        // the flag is already home
fn on_touchdown(game, player) { }
fn on_carrier_tackled(game, winner, loser) { }
fn on_frag(game, killer, victim) { }         // killer == victim: a suicide
fn on_died(game, victim, killer) { }         // every death that counts (not in a blade zone or a duel lobby); killer -1: none
fn on_rabbit_grab(game, player) { }          // took the rabbit flag
fn on_rabbit_held(game, player) { }          // held it 15 seconds more in all
fn on_zone_points(game, gained) { }          // gained: [ember, glacier]
fn on_clock_expired(game) { }                // unanswered, the match ends anyway
```

`game` reads the match: `score` ([ember, glacier]), `limit`, `playing`,
`ffa`, `player_count`, `touchdown_points`, `tackle_points`, `rabbit` (who
holds the rabbit flag, -1: nobody), and per player
`team(i)` (0 Ember, 1 Glacier), `frags(i)`, `losses(i)`, `name(i)`,
`is_you(i)` (whether `name(i)` is "You", the unnamed player offline, so a
line can say "You score" rather than "You scores"),
`hostile(killer, victim)`, plus `leader()` and `team_name(t)`. `this` is a
map that lasts the round, for your own state (streaks, counts). Commands:
`add_score(team, points)`, `set_score(team, points)`, `add_frags(player, n)`,
`end_match(text, seconds)`, `message(text, seconds)`, `feed(text)`,
`sound(name)` (announcer cues: `capture_win`, `point`, `horn`, ...) and
`shake(amount)`.

What keeps scripts safe:

- They run on the **server only**. They are not in the files players
  download, and the game can't run them (it doesn't contain a script
  engine). Offline, a scripted mode plays the rules of the mode it varies.
- They can't touch files, the network or the clock, and each call is
  limited (100,000 operations, depth and sizes). A call that fails or runs
  over has all its commands dropped, and the server logs why.
- `peakrunner-server --check` compiles every script and reports mistakes
  with the line; a server won't start with a script that doesn't compile.

### Check, play and host it

```sh
peakrunner-server --check $HOME/peakrunner-content                  # every map and mod
PEAKRUNNER_PACK_DIR=$HOME/peakrunner-content PEAKRUNNER_MODS=quick-ctf peakrunner   # offline
```

With the mod in the game's own `content/mods/`, `PEAKRUNNER_MODS=quick-ctf`
alone is enough.

For a server, put the mod in its `content/mods/` and run
`peakrunner-server --setup`: the wizard lists the mods it finds, asks which
to run, and offers their modes (marked "(mod)") and your maps. By hand, list
the mods in `server.json` and use their modes in the rotation (or set
`PEAKRUNNER_MODS`):

```json
{"mods": ["low-gravity", "quick-ctf"], "rotation": [{"map": "old-holler", "mode": "quick-ctf"}]}
```

Find match shows each server's mods and how much joining downloads.
Players who join download the mods like maps (after they approve) and play
that match with exactly your server's definitions; when they leave, their
game is its own again. A mod is data only: anything besides these JSON files
is refused. Change any file and it is a new mod to everyone (its fingerprint
covers every file).

## Rules that keep everyone safe

- Content is data only: maps, definitions and modes (and later sounds and
  layouts). A server never sends code to a player's game.
- Every file is checked against its SHA-256 before use; changed files are
  refused even after loading.
- A map's identity is its fingerprint (the SHA-256 of `map.json`), not its
  key. Change a map and it is a new map to everyone; players who join a
  server running it download the new version.
- Keep content original: no material taken from other games.
