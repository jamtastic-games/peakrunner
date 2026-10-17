# The mod format: engine classes and their entries

PeakRunner's engine provides **classes**: the kinds of things that can exist
and how each kind behaves. Every actual item in the game is an **entry** of
a class, defined in a package's files. The base game is a package like any
other (`src/assets/mods/base/`, built into every engine). A mod is a folder
of the same files, laid over the base game's.

That gives a mod three powers:
- It can **reach any entry by its id and redefine it**: change a number,
  replace it outright, or copy it into a new entry with a few changes.
- It can **recombine**. Entries refer to each other by id: a weapon names
  the projectile it fires, a projectile its blast, a turret what it fires.
  So giving one thing's capability to another is a change of id, not engine
  work.
- It can **add new entries** of any class.

Players' games receive the entries (data) when they join a server. Scripts
(`rules/`, `hooks/`) stay on the server.

The practical guide to folders, checking and sharing is
`docs/making-content.md`. The design is `docs/modding.md`.

## A mod folder

```
mods/<id>/
  mod.json            {"name": "...", "note": "...", "api": 1}
  defs.json           world numbers: movement, repair tool, map equipment, placement, blade
  armors/*.json       bodies: movement, health, what they carry
  weapons/*.json      hand weapons: what they fire and what a shot costs
  projectiles/*.json  what flies or beams: how it moves, what contact does
  blasts/*.json       bursts: damage by distance, kick
  throwables/*.json   what the grenade (G) and mine (N) keys throw
  deployables/*.json  what players place (a pack carries it)
  packs/*.json        what players wear on the back: placed, always on, switched or used
  items/*.json        what lies on the ground for whoever runs over it
  vehicles/*.json     fliers and ground vehicles: movement, hull, seats and what each seat fires
  sounds/<name>.wav   short sounds (64 KiB at most) definitions name
  effects/*.json      status effects: for a while, or while a pack holds them on
  damage/*.json       damage kinds: zones and knocks
  modes/<key>.json    game modes (variants of an existing mode)
  rules/<key>.rhai    a mode's rules script (server only)
  hooks/<name>.rhai   scripts definitions name (server only)
```

- **Class folders** hold any number of `<name>.json` files. Each file is an
  object of entries by id:

  ```json
  { "rapid-disc": { "extends": "disc", "name": "Rapid disc", "reload": 0.6 } }
  ```

  File names are free: one `weapons/all.json`, or one file per weapon.
- **Comments.** Every JSON file may hold `//` and `/* */` comments.
- **Ids** are 1 to 32 of a-z, 0-9, `_` and `-`.

## Redefining entries

Packages are laid on in order: the base game, then each mod the server runs,
in its configured order.

| You write | It means |
|---|---|
| An id that exists, with some fields | **Patch**: those fields change, the rest stay. Objects merge key by key. |
| `null` as a field's value | **Remove** that key (for example, an armor no longer carries a weapon: `"weapons": {"blaster": null}`). |
| `"replace": true` | **Start over**: the entry is exactly what you wrote, so it must be complete. |
| `"extends": "<id>"` | **Copy** another entry of the class, then patch the copy. Within one package, copies are made after that package's plain patches, in any order of ids. |
| A new id | **Add** it. It must be complete, unless it extends one. |

Every number is checked against its range, every reference against the
entries that exist, and every entry as a whole (a burst needs a blast; a
beam needs a range). A problem names the file, the entry and the field, and
the mod is refused rather than played. Run `peakrunner-server --check` on a
mod before sharing it.

## Wire numbers

Each entry travels to players as a small number. The base game's entries
keep fixed numbers, so the base game's snapshots never change. Entries a
package adds follow, package by package, ids sorted. A mod never chooses
numbers.

## Older mods

Mods written before the classes keep working. Their `defs.json` named items
at fixed paths (`weapons.disc.reload`, `armors.heavy.ammo`, `blasts.mortar`);
the engine maps each of those onto the entry it meant.

`"api"` in `mod.json` stays 1 until the full release: playtests ship the
game, its servers and the example mods together, so no setting needs a
higher level.

## The classes so far

| Class | Folder | What an entry defines |
|---|---|---|
| Armor | `armors/` | Movement numbers (predicted by every client), its energy tank, walking, damage taken and resistances by kind, mass, weapon slots, the weapons it may carry and their rounds, its kit, what the throw keys throw, a cap per team; its class key (C: switched or used like a pack, building strain that ebbs, too much giving an effect), what its touch does (mend and charge teammates; hurt, drain and affect enemies), and mounts (a weapon on the shoulder that fires by itself, a plate in front that stops shots and blasts from ahead) |
| Weapon | `weapons/` | The projectile it fires, energy or a round per shot, reload, spread, recoil, shake, aim, view model, sound, zoom, family (its versions across armors); the trigger (`single`, `spin` up, `charge` and release, `burst`, `sustained` beam), lock-on, remote detonation, and fire modes (variants the fire-mode key cycles) |
| Projectile | `projectiles/` | `straight`, `shell`, `thrown`, `beam` or `stream` (a held beam): speed, acceleration to a top speed, gravity, life, collision, water, expiry, blast or damage and its kind, bounce, pellets, homing (`seek`: turn rate, cone, range, targets, proximity fuse), guidance by the shooter's aim, marks, mines' trigger, the look, the kill feed's word |
| Blast | `blasts/` | Radius, damage falloff (linear, concentrated or quadratic), kick |
| Throwable | `throwables/` | The projectile it becomes, throw speed and strength, the pause between throws, a team limit |
| Deployable | `deployables/` | A body (`behavior`: a turret's post and gun, a plain post, a solid wall, a field its team passes, an ammo crate, an inventory or command kiosk, a pad to stand on, a gate nobody bumps into), its size and look, hull, limits (per team, per teammate, per player, per round), a turret's targeting and what it fires; and capabilities any body can carry: a `pad` throws whoever stands on it (predicted), a `teleport` links a team's units in pairs, an `aura` holds an effect on players near, a `sensor` senses enemies (turrets reach their sensed range, the HUD marks them, an alarm sounds), a `camera` is looked through from a command station, a `shock` hurts enemies who touch it |
| Damage kind | `damage/` | What a hit is made of: extra damage in the head or the back on direct hits, and how hard a hit knocks the flag off a carrier hit from behind, drops a pack or disarms. Armors resist kinds (`resist`); projectiles name theirs (`damage_kind`) |
| Effect | `effects/` | A status a player has for a while: damage or healing a second, energy, regen and movement multiples (predicted by every client), a shield paying damage from energy, visibility (fade, hidden name, hidden from sensors), what firing or a hit ends, what it blocks (firing, jets, taking a flag), stacking, cleansed at stations, armors immune, a screen tint |
| Item | `items/` | Something lying on the ground for whoever runs over it: health, energy, a share of full rounds for each weapon carried, grenades and mines, an effect; how long one a map places takes to come back; its look. Taken only when it gives something. Deaths leave them where a mode's `death_drops` say; players drop weapons and packs with the drop keys where a mode allows (`player_drops`) |
| Vehicle | `vehicles/` | A flier (`drive` `fly`: thrust along its nose, back, sideways and up, the share of gravity it cancels while piloted, its nose turning toward where the pilot looks) or a ground vehicle (`drive` `ground`: full gravity, driven forward and back, turned by the sideways keys, held on slopes up to `climb`, its tracks gripping by `grip`); drag, top speed and turn rate (predicted for the pilot), energy and regen, hull, size and mass, what it deals running into enemies (`ram`), seats by name (`pilot` drives; each seat may fire a weapon along its rider's view, and an `exposed` seat's rider sits in the open, where shots and blasts reach them), the blast it bursts in, whether a flag carrier may ride, a team limit and a price. Bought at a deployable whose entry lists `vehicles` (the library's `vehicle-pad`); the use key boards and leaves. Vehicles bump each other; shots stop on a box the size of their body. The stock library has a skiff, a transport, the Ridgerunner (a flying motorcycle, its rider exposed) and the Hogback (a tank: a cannon on its turret and a bow gun for a second rider) |
| Pack | `packs/` | Worn on the back, one at a time, worked by the deploy key: `place` its deployable (used up), `always` on, `toggle`d, or `use`d for an action (fire a weapon from the shoulder, throw, place, give an effect, a shove along the view, a borrowed armor for a while) with its energy, cooldown and charges; while it works, effects held on and energy drained; a multiple of rounds carried |

The base game's packs (`packs/base.json`) are worn ones (energy, shield,
repair, sensor jammer, ammo; their effects are in `effects/library.json`)
and ones that place a deployable (motion and pulse sensors, the jammer
pylon, turret, wall, force field, ammo station, field inventory). An
armor's `packs` lists what it may wear (none listed: any): the base light
armor lists the worn packs and the sensors, so a mod that adds a pack for
light armor restates that list with its pack in it. The stock library
(`packs/library.json`) holds packs the base modes don't offer
(`offered: false`), such as `recall`; a mod offers one with
`{ "recall": { "offered": true } }`, or extends it.

An effect with `visibility` under 0.5 (the library's `cloaked`) hides its
bearer from the other team: the server leaves them out of that team's
snapshots unless one of its players is within the effect's `shimmer` range
(or they carry a flag), and bots don't see them either.

A command station (`behavior: "command"`) lets a player standing at it look
through their team's cameras and turrets (the use key, then the view key
for the next), work a turret (it aims where they look and fires while they
hold fire), and set the team's waypoint (the deploy key), which teammates
see on their HUD. The library's `recall` pack puts a beacon down and, used
again, takes its wearer back to it.

Maps add moving parts, zones, items and units in their manifests
(`docs/map-pipeline.md`).

## Looks, sounds and bots

- **Parts.** Projectiles, weapons (the view model), deployables, vehicles,
  items, packs (on the wearer's back) and mounts may have `parts`: up to 24
  primitive shapes (`cube`, `bevel`, `sphere`, `disc`, `capsule`, `dome`),
  each placed (`at`, m: right, up, back), sized, turned (degrees: pitch,
  yaw, roll), colored or in its team's color, and glowing. They replace the
  base look; an armor's parts are drawn on its body as well. A vehicle's
  part may `aim` with a seat: it turns with that rider's view about the
  upright line through the seat (a turret), and with `elevates` tilts up
  and down about its `pivot` (a barrel). The Hogback's turret is built so. Armors also
  take a `scale` (looks only) and a `tint`; effects a `glow` round their
  bearer.
- **Sounds.** A mod's `sounds/<name>.wav` (a WAVE file of at most 64 KiB)
  plays wherever a definition names a sound by that name (a weapon's
  `sound`), when no built-in cue has it. Sounds are sent to players with
  the mod, like its other files.
- **Bots.** `bot_roles` on an armor (`offense`, `defense`, `hunter`): bots
  of those roles wear it. `bot_range` on a weapon: the distances bots fire
  it at. `bot` on a pack or class key (`combat`, `defense`, `always`): when
  bots switch it on or use it; a pack that places with `defense` is put down
  by defenders near their flag. Without hints, bots play as the base game's
  do. Bots don't fly vehicles or work command stations.

## Hooks

Armors, weapons, packs, deployables, items, effects and vehicles may name a
`hook`: a script in the mod's `hooks/<name>.rhai`, run by servers only and
never sent to players. A mode may list `hooks` that hear every event, and a
map's trigger zone may name one. A hook defines any of `on_spawn`,
`on_death`, `on_damage`, `on_fire`, `on_use`, `on_deploy`, `on_destroyed`,
`on_touch`, `on_enter`, `on_leave`, `on_pickup`, `on_buy`, `on_effect` (each
`(game, ev)`, `ev` a record) and `on_timer(game, name)`, and answers with
commands. The full list of what `game` reads and the commands is at the top
of `crates/script/src/lib.rs`; the design is `docs/modding.md` §7.6.

```rhai
// hooks/medic.rhai: tell the team who fell, and call a regroup after.
fn on_death(game, ev) {
    if ev.killer >= 0 { team_feed(game.team(ev.victim), game.name(ev.killer) + " got one of ours"); }
    after(10, "regroup");
}
fn on_timer(game, name) { if name == "regroup" { message("REGROUP AT THE STAND", 3); } }
```

## Field reference

Generated from the engine (`peakrunner-server --schema`); a test keeps it
current. `a.b` is field `b` inside object `a`; `a.*` is every value of an
object keyed by ids.

<!-- schema:start -->
## Armor (`armors/`)

| Field | Values | About |
|---|---|---|
| `movement` | an object | The skier movement model's numbers. Predicted by every client, so only data. |
| `movement.jet` | 0 to 200 | Jet acceleration (m/s², before gravity): all upward with no movement key held. |
| `movement.side` | 0 to 200 | Horizontal jet acceleration along the movement keys, taken out of the upward share. |
| `movement.drain` | 0 to 1000 | Energy per second while jetting. |
| `movement.regen` | 0 to 200 | Energy per second regained otherwise. |
| `movement.min_jet` | 0 to 100 | Energy needed to start or keep jetting. |
| `movement.jump` | 0 to 5 | Jump impulse as a share of the standard jump. |
| `movement.fade_up` | true or false | Upward thrust fades as climb speed nears the cap. |
| `movement.air_cap` | 0 to 200 | Horizontal speed (m/s) up to which the jet adds speed. |
| `movement.ski_drag` | 0 to 100 | Skiing slowdown (m/s²) on flat ground. |
| `name` | text, up to 32 | The armor's name at stations and on the HUD. |
| `about` | text, up to 200 | A line about it at stations. |
| `offered` | true or false | Offered at stations by modes that don't list their armors (default true); a mode's `armors` list wins. |
| `build` | light, medium, heavy | The body's size: its look and its armor model. |
| `walk_scale` | 0.1 to 3 | Walking speed relative to the base light armor. |
| `damage_taken` | 0.05 to 5 | Share of damage taken. |
| `mass_scale` | 0.1 to 10 | Mass relative to the base light armor (blasts kick heavier bodies less). |
| `weapon_slots` | whole number 1 to 6 | Weapons carried at once (the six slot keys). |
| `weapons` | an object keyed by weapon ids | Weapons this armor may carry, by id. |
| `weapons.*` | whole number 0 to 10000 | Full rounds for that weapon (0 for one paid with energy). |
| `kit` | a list of up to 8 weapon ids | What it carries until its wearer chooses (bots too), slot 1 first. |
| `grenade` | an object | What the grenade key throws. |
| `grenade.throwable` | a throwable id | The throwable. |
| `grenade.count` | whole number 0 to 50 | How many are carried. |
| `mine` | an object | What the mine key throws. |
| `mine.throwable` | a throwable id | The throwable. |
| `mine.count` | whole number 0 to 50 | How many are carried. |
| `resist` | an object keyed by damage kind ids | Damage taken of each kind, by id, as a share. |
| `resist.*` | 0 to 5 | The share of that kind taken (1: as usual). |
| `energy_max` | 1 to 1000 | Its energy tank (default: the world's movement.energy_max). |
| `team_cap` | whole number 0 to 64 | Most of a team that may wear it at once (0: no limit). |
| `ability` | an object | The class key: what its wearer can do with it. |
| `ability.name` | text, up to 32 | Its name on the HUD. |
| `ability.about` | text, up to 200 | A line about it at stations. |
| `ability.use` | toggle, use | Switch it on and off, or use it for its action. |
| `ability.effects` | a list of up to 8 effect ids | Toggle: effects held on while it's on. |
| `ability.drain` | 0 to 200 | Toggle: energy a second while it's on. |
| `ability.min_energy` | 0 to 1000 | Toggle: energy it needs to stay on. |
| `ability.energy` | 0 to 1000 | Use: energy each use costs. |
| `ability.cooldown` | 0 to 600 | Use: seconds before it can be used again. |
| `ability.charges` | whole number 0 to 100 | Use: uses a life holds (0: no limit). |
| `ability.strain` | 0 to 100 | Strain each use (or each second it's on) builds. |
| `ability.strain_decay` | 0 to 100 | Strain lost each second. |
| `ability.strain_limit` | 0 to 1000 | At this strain or more, the strain effect is given (0: never). |
| `ability.strain_effect` | a effect id | What too much strain gives. |
| `ability.action` | an object | Use: what it does (exactly one of fire, throw, place, recall, effect, impulse, armor). |
| `ability.action.fire` | a weapon id | Fire this weapon's projectile at the crosshair, from the shoulder. |
| `ability.action.throw` | a throwable id | Throw this throwable. |
| `ability.action.place` | a deployable id | Put this deployable down where the player aims. |
| `ability.action.recall` | a deployable id | Put this deployable down where the player aims; used again, go to it. |
| `ability.action.effect` | a effect id | Give its user this effect. |
| `ability.action.impulse` | two numbers 0 to 200 | A shove along the view, m/s forward and up (heavier bodies move less). Predicted. |
| `ability.action.armor` | a armor id | Wear this armor for action.seconds, then go back. |
| `ability.action.seconds` | 0 to 120 | How long action.armor lasts. |
| `touch` | an object | What touching another player does, at most once a cooldown per pair. |
| `touch.heal` | 0 to 100 | Health given to a teammate touched. |
| `touch.energy` | 0 to 1000 | Energy given to a teammate touched. |
| `touch.ally_effect` | a effect id | An effect given to a teammate touched. |
| `touch.damage` | 0 to 100 | Damage to an enemy touched. |
| `touch.damage_kind` | a damage kind id | Its kind (default impact). |
| `touch.steal_energy` | 0 to 1 | Share of an enemy's energy taken (and given to the toucher). |
| `touch.effect` | a effect id | An effect given to an enemy touched. |
| `touch.cooldown` | 0 to 60 | Seconds before the same two touch again. |
| `mounts` | an object keyed by names (a-z, 0-9, _ and -) | Things fixed to the body, by a name of your choosing (at most 4). |
| `mounts.*` | an object | One mount. |
| `mounts.*.at` | shoulder, back, front, belt | Where it sits. |
| `mounts.*.weapon` | a weapon id | A weapon that fires by itself at the nearest enemy in sight. |
| `mounts.*.range` | 0 to 300 | How far it reaches (m). |
| `mounts.*.cooldown` | 0.05 to 60 | Seconds between its shots (default: the weapon's reload). |
| `mounts.*.blocker` | an object | A plate stopping shots and blasts from the front. |
| `mounts.*.blocker.arc` | 0 to 90 | Degrees either side of straight ahead it covers. |
| `mounts.*.blocker.share` | 0 to 1 | Share of a blocked hit's damage that still gets through. |
| `price` | whole number 0 to 1000000 | Credits it costs where the mode has an economy. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `scale` | 0.5 to 2 | Its body's size (looks only: collisions stay shared). |
| `tint` | three numbers 0 to 1 | Its body's color (red, green, blue). |
| `bot_roles` | a list of: offense, defense, hunter | The bot roles that wear it (none named by any armor: bots wear what they always have). |
| `ability.bot` | combat, defense, always | When bots use it. |
| `packs` | a list of up to 32 pack ids | The packs it may wear, by id (empty: any its station offers). |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |
| `mounts.*.parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `mounts.*.parts.*` | an object | One part. |
| `mounts.*.parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `mounts.*.parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `mounts.*.parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `mounts.*.parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `mounts.*.parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `mounts.*.parts.*.glow` | 0 to 4 | How much it glows. |
| `mounts.*.parts.*.team` | true or false | Its team's color instead. |
| `mounts.*.parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `mounts.*.parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `mounts.*.parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Weapon (`weapons/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | The weapon's name at stations, on the HUD and in the kill feed. |
| `about` | text, up to 200 | A line about it at stations. |
| `family` | a name: a-z, 0-9, _ and - | Versions of a weapon across armors share a family; favorites name families. |
| `offered` | true or false | Offered at stations by modes that don't list their packs (default true); a mode's `packs` list wins. |
| `projectile` | a projectile id | What it fires. |
| `energy` | 0 to 1000 | Energy a shot costs; 0: a round instead. |
| `reload` | 0.016 to 30 | Seconds between shots. |
| `spread` | 0 to 100 | Random speed added to a shot (m/s). |
| `recoil` | 0 to 1 | View kick up per shot (radians). |
| `shake` | 0 to 1 | Screen shake per shot. |
| `aim` | 1 to 5000 | Metres ahead a shot from the muzzle steers onto the crosshair (default 80). |
| `bot_spread` | 0 to 50 | Random speed (m/s) added to a bot's shot. |
| `view` | disc, chaingun, launcher, mortar, rifle, railgun, plasma, blaster | The view model held; it also places the muzzle. |
| `muzzle` | three numbers -2 to 2 | Where its model's barrel ends, if not at its view's muzzle: metres right, up and back in the view model's frame; its rounds, beams and flash leave there. |
| `sound` | a name: a-z, 0-9, _ and - | The firing sound cue. |
| `zoom` | true or false | The zoom key zooms while it's held (the wheel picks one of two levels). |
| `elevation` | true or false | Its sight shows an elevation gauge: the aim's angle above the horizon, for lobbing shells. |
| `empties_energy` | true or false | Each shot also empties its holder's energy, so they can't jet right after one. |
| `loot` | none, roll, all | What a dying player's ammo pack holds of its rounds: none, 1 to 5, or all. |
| `lost_on_death` | true or false | Lost when its carrier dies (back at respawn if it's in their setup). |
| `fire` | single, spin, charge, burst, sustained | How the trigger works (default single). |
| `spin_up` | 0 to 10 | Spin: seconds the barrel takes to spin up before it fires. |
| `spin_down` | 0 to 10 | Spin: seconds it takes to spin down once released. |
| `charge` | 0 to 10 | Charge: seconds held to reach full power. |
| `charge_scale` | 0.1 to 10 | Charge: damage and speed at full power, as a multiple (default 1). |
| `overload` | 0 to 10 | Charge: seconds held past full before it hurts the shooter (0: never). |
| `overload_damage` | 0 to 1000 | Charge: what an overload does to the shooter. |
| `burst` | whole number 0 to 20 | Burst: shots per trigger pull. |
| `burst_gap` | 0 to 5 | Burst: seconds between the shots of a burst. |
| `lock` | 0 to 10 | Seconds a target must stay in the crosshair to lock on (0: no lock). |
| `lock_cone` | 0 to 90 | Degrees from the crosshair a target may be to lock on. |
| `lock_range` | 0 to 5000 | Metres away a target may be to lock on. |
| `remote` | true or false | Firing again sets off this weapon's rounds still out. |
| `modes` | a list of up to 8 objects of these fields | Other ways to fire it (the fire-mode key cycles them): each changes some of these fields. |
| `price` | whole number 0 to 1000000 | Credits it costs where the mode has an economy. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `bot_range` | two numbers 0 to 5000 | The distances (m) bots fire it at, nearest and farthest. |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Projectile (`projectiles/`)

| Field | Values | About |
|---|---|---|
| `class` | straight, shell, thrown, beam, stream | How it moves and what contact does (stream: a held beam). |
| `speed` | 0 to 5000 | Muzzle speed (m/s). |
| `inherit` | 0 to 2 | Share of the shooter's velocity kept. |
| `life` | 0.01 to 1000 | Seconds it lasts (a grenade's fuse, a beam's glow). |
| `gravity` | 0 to 5 | Share of gravity it falls with. |
| `radius` | 0 to 8 | Collision radius against the map (default 0.12). |
| `reach` | 0 to 5 | Extra reach against equipment. |
| `water` | drag, fizzle, burst | In water: slowed hard, put out, or burst where it meets the water at half damage and twice the push. |
| `expire` | explode, vanish | When its life runs out: bursts, or is gone. |
| `blast` | a blast id | Its burst against bodies. |
| `structure_blast` | a blast id | Its burst against equipment and deployables (default: blast). |
| `damage` | 0 to 10000 | A hit's damage when it doesn't burst; a beam's damage. |
| `burn` | 0 to 2000 | Damage a second to a body it overlaps in flight. It keeps flying (it still bursts on the ground). |
| `flash` | 0 to 1 | How hard a hit that doesn't burst flashes the victim's screen (default 0.5). |
| `self_damage` | 0 to 2 | Share of its own blast the shooter takes (default 1). |
| `arm` | 0 to 60 | Shells: seconds of flight before contact sets it off. |
| `elasticity` | 0 to 1 | Bounces: share of the speed into a surface kept. |
| `slide` | 0 to 1 | Thrown: share of the speed along a surface kept. |
| `keep` | 0 to 1 | Shells: speed kept after a bounce. Straight: glances off the map keeping this share. |
| `rest_speed` | 0 to 50 | Thrown: under this speed it comes to rest on a floor. |
| `damage_kind` | a damage kind id | What its hits and burst are made of (default blast). |
| `smoke` | true or false | Trails smoke. |
| `clears_mines` | true or false | Sets off resting mines along its path. |
| `range` | 0 to 5000 | Beams: how far it reaches. |
| `trigger` | an object | Thrown ones that lie in wait (mines). |
| `trigger.rest_life` | 0.1 to 3600 | Seconds it lies at rest. |
| `trigger.arm` | 0 to 60 | Seconds at rest before it arms. |
| `trigger.radius` | 0 to 50 | An enemy this near sets it off. |
| `trigger.sympathy` | 0 to 50 | A blast this near sets it off too. |
| `trigger.shot_radius` | 0 to 5 | A shot passing this close hits it. |
| `accel` | 0 to 1000 | Speeding up along its flight (m/s²). |
| `max_speed` | 0 to 5000 | The speed it speeds up to (0: no limit). |
| `seek` | an object | Homing on a target. |
| `seek.turn` | 0 to 1080 | Degrees a second it turns toward its target. |
| `seek.cone` | 0 to 180 | Degrees from the aim a target may be, chosen at launch. |
| `seek.range` | 0 to 5000 | Metres away a target may be, chosen at launch. |
| `seek.targets` | a list of: players, jetting, marked | What it homes on: players, players jetting, or what teammates marked (default players). |
| `seek.proximity` | 0 to 50 | It bursts this near its target (0: only on contact). |
| `guided` | 0 to 1080 | Degrees a second it turns to follow the shooter's aim (0: not guided). |
| `pellets` | whole number 1 to 32 | Fired as this many pellets (default 1). |
| `cone` | 0 to 45 | Degrees the pellets spread over. |
| `marks` | 0 to 120 | Beams and streams: seconds what it hits stays marked (shown) for the shooter's team; a stream keeps one mark, on what it touches while held. |
| `stream` | an object | Streams: what the held beam does each second. |
| `stream.range` | 0 to 1000 | How far it reaches (m). |
| `stream.damage` | 0 to 1000 | Damage a second to enemies. |
| `stream.drain` | 0 to 1000 | Energy a second drained from enemies. |
| `stream.heal` | 0 to 1000 | Health a second restored to teammates. |
| `stream.repair` | 0 to 1000 | Hull a second restored to the team's equipment and deployables. |
| `stream.claim` | 0 to 120 | Seconds of beam that turn an enemy deployable to the shooter's team (0: it can't). |
| `kill` | text, up to 32 | The kill feed's word for a kill by it. |
| `kill_turret` | text, up to 32 | The kill feed's word when a turret fired it (default: kill). |
| `look` | disc, tracer, grenade, turret_plasma, mortar, hand_grenade, mine, laser, rail, plasma, blaster | How it and its burst are drawn: a base projectile's look (default by class). |
| `effect` | a effect id | An effect for whoever it hits, catches in its blast, or holds in its stream. |
| `pull` | -200 to 200 | A direct hit drags its victim toward the shooter at this speed (m/s; negative: away, and up by 0.4 of it). |
| `grapple` | 0 to 200 | Any hit drags the shooter toward it at this speed (m/s). |
| `becomes` | a deployable id | Come to rest (thrown), it becomes this deployable. |
| `drops` | a item id | Come to rest (thrown), it becomes this item. |
| `model` | a name: a-z, 0-9, _ and - | Its authored model (models/projectiles/<model>/), drawn in flight in place of its look. |
| `size` | 0.25 to 8 | Its look's glow drawn this many times as big (tracers, bolts, beams; default 1). |
| `color` | three numbers 0 to 1 | Its look's glow color, or its model's paint all over (red, green, blue). |
| `trail` | an object | A trail behind it in flight, with its look or its model (looks only: it changes nothing in play). |
| `trail.style` | ribbon, smoke | A glowing ribbon that follows it, or smoke left along its path that spreads and fades. |
| `trail.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `trail.width` | 0 to 5 | The ribbon's width, or a fresh puff's size (m; 0: ribbon 0.25, smoke 0.2). |
| `trail.life` | 0 to 10 | Seconds of flight the ribbon spans, or a puff lingers (0: ribbon 0.14, smoke 1.4). |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Blast (`blasts/`)

| Field | Values | About |
|---|---|---|
| `falloff` | linear, quadratic | The damage curve (linear, or concentrated with core). |
| `radius` | 0 to 200 | Reach (m). |
| `max_damage` | 0 to 10000 | Damage at the centre (a direct hit for quadratic). |
| `edge_fraction` | 0 to 1 | Linear: share of max_damage left at the radius. |
| `impulse` | 0 to 100000 | Kick, divided by the body's mass. |
| `core` | 0 to 200 | Above 0: concentrated, half damage at core metres. |
| `edge_damage` | 0 to 10000 | Quadratic: damage term rising to the centre. |
| `core_damage` | 0 to 10000 | Quadratic: damage term rising fast near the centre. |
| `kick` | 0 to 100 | Quadratic: kick speed (m/s) at the centre. |

## Throwable (`throwables/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name on the HUD and at stations. |
| `about` | text, up to 200 | A line about it. |
| `projectile` | a projectile id | What it becomes once thrown. |
| `speed` | 0 to 200 | Full-strength throw speed (m/s). |
| `inherit` | 0 to 2 | Share of the thrower's velocity kept (default 1). |
| `time` | 0 to 10 | Seconds before the next throw. |
| `min_strength` | 0 to 1 | The weakest throw's share of full. |
| `team_max` | whole number 0 to 500 | Most a team may have out (0: no limit). |
| `match_limit` | whole number 0 to 10000 | Most a team may throw in a match (0: no limit). |

## Deployable (`deployables/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name at stations and on the HUD. |
| `about` | text, up to 200 | A line about it at stations. |
| `behavior` | turret, wall, field, ammo, inventory, post, pad, gate, command | Its body: a turret's post and gun, a plain post, a solid wall, a field its team passes, an ammo crate, an inventory kiosk, a low pad to stand on, a gate nobody bumps into, or a command kiosk. |
| `hull` | 1 to 100000 | Its health. |
| `team_limit` | whole number 0 to 200 | Most a team may have standing. |
| `per_teammate` | 0 to 10 | More a team may have standing per player on it. |
| `per_player` | whole number 0 to 50 | Most one player may have standing (0: no limit). |
| `match_limit` | whole number 0 to 10000 | Most a team may place in a match (0: no limit). |
| `turret` | an object | A turret's targeting and fire. |
| `turret.range` | 0 to 2000 | Detection range (m). |
| `turret.sensed_range` | 0 to 2000 | Detection range while a team sensor is powered. |
| `turret.speed` | 0 to 5000 | Its rounds' speed (m/s). |
| `turret.life` | 0.01 to 60 | Its rounds' life (s). |
| `turret.cooldown` | 0.016 to 60 | Seconds between shots. |
| `turret.leads` | true or false | Aims where a target will be. |
| `turret.fires` | true or false | Fires (false: only detects). |
| `turret.projectile` | a projectile id | What it fires. |
| `turret.jetting_only` | true or false | Engages only enemies using their jets (a homing round follows the one it picked). |
| `size` | three numbers 0.05 to 20 | Half its width, height and depth (m) for a box-shaped body (default: its behavior's). |
| `look` | turret, wall, field, ammo, inventory, pylon, sensor, camera, pad, gate, command, beacon | The base look it is drawn with (default: its behavior's). |
| `pad` | an object | Whoever stands on it is thrown. Predicted. |
| `pad.forward` | 0 to 200 | Speed added along `along` (m/s; heavier bodies less). |
| `pad.up` | 0 to 200 | Upward speed it gives (m/s; heavier bodies less). |
| `pad.along` | view, facing, up | Forward along the player's view, the pad's facing, or none. |
| `teleport` | an object | A team's units of this kind link in pairs, in placement order; step into one, come out at the other. |
| `teleport.cooldown` | 0 to 60 | Seconds before the same player can go through again. |
| `teleport.carrier` | true or false | A flag carrier may go through. |
| `aura` | an object | An effect held on players within range. |
| `aura.effect` | a effect id | The effect. |
| `aura.range` | 0 to 200 | How far it reaches (m). |
| `aura.targets` | team, enemies, all | Whom it touches. |
| `sensor` | an object | Senses enemies for its team: turrets reach their sensed range, and the team sees them marked. |
| `sensor.range` | 0 to 1000 | How far it senses (m). |
| `sensor.moving` | true or false | Senses only those moving. |
| `sensor.alarm` | true or false | Sounds an alarm for its team when an enemy comes in. |
| `camera` | true or false | Its team can look through it from a command station. |
| `shock` | an object | What touching it does to an enemy. |
| `shock.damage` | 0 to 200 | Damage. |
| `shock.damage_kind` | a damage kind id | Its kind (default shock). |
| `shock.effect` | a effect id | An effect given. |
| `shock.cooldown` | 0.05 to 60 | Seconds before it shocks the same player again. |
| `vehicles` | a list of up to 8 vehicle ids | A vehicle station: the vehicles bought standing on it. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `life` | 0 to 3600 | Seconds it stands before it's gone by itself (0: until destroyed). |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Damage kind (`damage/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | The kind's name. |
| `head` | 0 to 10 | Direct hits in the head: damage times this (default 1). |
| `back` | 0 to 10 | Direct hits in the back: damage times this (default 1). |
| `knock_flag` | 0 to 10000 | A direct hit from behind this hard knocks the flag off its carrier (0: never). |
| `drop_pack` | 0 to 10000 | A direct hit this hard drops the victim's pack (0: never). |
| `disarm` | 0 to 10000 | A direct hit this hard knocks the weapon out of the victim's hand (0: never). |

## Effect (`effects/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name on the HUD. |
| `about` | text, up to 200 | A line about it. |
| `seconds` | 0 to 600 | How long it lasts once given (a pack or ability holding it on renews it). |
| `damage` | 0 to 1000 | Damage a second (of damage_kind). |
| `damage_kind` | a damage kind id | What that damage is made of (default blast). |
| `heal` | 0 to 1000 | Health restored a second. |
| `energy` | -1000 to 1000 | Energy gained a second (negative: drained). |
| `regen_scale` | 0 to 10 | Energy regained, as a multiple of the armor's (default 1). |
| `movement` | an object | Movement as multiples (default 1). Predicted by every client. |
| `movement.jet` | 0 to 5 | Jet thrust. |
| `movement.side` | 0 to 5 | Sideways jet thrust. |
| `movement.air_cap` | 0 to 5 | The speed the jet adds speed up to. |
| `movement.walk` | 0 to 5 | Walking speed. |
| `movement.jump` | 0 to 5 | The jump. |
| `absorb` | 0 to 100 | A shield: energy spent per point of damage taken instead, counted before armor (0: none). |
| `absorb_kinds` | a list of up to 16 damage kind ids | The damage kinds the shield takes (empty: all). |
| `visibility` | 0 to 1 | How plainly enemies see its bearer (1: plainly; 0: not at all). |
| `hide_name` | true or false | No name tag for enemies. |
| `sensor_hidden` | true or false | Sensors, turrets and homing rounds don't see its bearer. |
| `shimmer` | 0 to 200 | Enemies this near see a shimmer anyway. |
| `reveal` | true or false | Firing or being hit ends it. |
| `blocks` | an object | What it stops while it lasts. |
| `blocks.fire` | true or false | Firing. |
| `blocks.jet` | true or false | Jetting. |
| `blocks.flag` | true or false | Taking a flag. |
| `stack` | refresh, extend, keep | Given again while it lasts: start over, add the time, or ignore. |
| `cleansed` | true or false | An inventory or ammo station ends it. |
| `immune` | a list of up to 32 armor ids | Armors it never takes hold on. |
| `tint` | four numbers 0 to 1 | The screen tint its bearer sees: red, green, blue, strength (0 to 1). |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `glow` | four numbers 0 to 1 | A glow round its bearer's body: color and strength. |

## Pack (`packs/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name at stations and on the HUD. |
| `about` | text, up to 200 | A line about it at stations. |
| `offered` | true or false | Offered at stations (default true). |
| `base_station_only` | true or false | Only a base inventory station hands it out. |
| `use` | place, always, toggle, use | What the deploy key does: place its deployable, nothing (it always works), switch it on and off, or use it. |
| `deployable` | a deployable id | Place: what it puts down. |
| `effects` | a list of up to 8 effect ids | Effects held on while it works. |
| `drain` | 0 to 200 | Energy a second while it works. |
| `drain_regen` | 0 to 10 | Energy a second while it works, as a multiple of the wearer's recharge (`movement.regen`), on top of `drain`. |
| `min_energy` | 0 to 1000 | Energy it needs to keep working. |
| `energy` | 0 to 1000 | Use: energy each use costs. |
| `cooldown` | 0 to 600 | Use: seconds before it can be used again. |
| `charges` | whole number 0 to 100 | Use: uses it holds (0: no limit). |
| `ammo_scale` | 0 to 5 | The wearer carries this multiple of their rounds (default 1). |
| `price` | whole number 0 to 1000000 | Credits it costs where the mode has an economy. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `bot` | combat, defense, always | When bots use it (a pack that places: defenders put it down near their flag). |
| `action` | an object | What using it does (exactly one of fire, throw, place, recall, effect, impulse, armor). |
| `action.fire` | a weapon id | Fire this weapon's projectile at the crosshair. |
| `action.throw` | a throwable id | Throw this throwable. |
| `action.place` | a deployable id | Put this deployable down where the player aims. |
| `action.recall` | a deployable id | Put this deployable down where the player aims; used again, go to it (it's used up; a flag carrier drops the flag). |
| `action.effect` | a effect id | Give its user this effect. |
| `action.impulse` | two numbers 0 to 200 | A shove along the view, m/s forward and up (heavier bodies move less). Predicted. |
| `action.armor` | a armor id | Wear this armor for action.seconds, then go back. |
| `action.seconds` | 0 to 120 | How long action.armor lasts. |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Item (`items/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name. |
| `about` | text, up to 200 | A line about it. |
| `health` | 0 to 100 | Health it gives. |
| `energy` | 0 to 1000 | Energy it gives. |
| `rounds` | 0 to 1 | Rounds it gives each weapon carried, as a share of full. |
| `grenades` | whole number 0 to 50 | Grenades it gives. |
| `mines` | whole number 0 to 50 | Mines it gives. |
| `effect` | a effect id | An effect it gives. |
| `respawn` | 1 to 600 | Seconds before one a map places comes back once taken. |
| `look` | kit, box, cell | How it looks. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

## Vehicle (`vehicles/`)

| Field | Values | About |
|---|---|---|
| `name` | text, up to 32 | Its name. |
| `about` | text, up to 200 | A line about it at stations. |
| `hull` | 1 to 100000 | Its health. |
| `energy_max` | 1 to 10000 | Its energy tank. |
| `regen` | 0 to 1000 | Energy a second regained. |
| `drive` | fly, ground | How it moves: `fly` (default), or `ground`: kept to the ground, turned by the sideways keys. |
| `flight` | an object | How it moves. Predicted for the pilot. |
| `flight.thrust` | 0 to 500 | Acceleration along its nose (m/s²). |
| `flight.reverse` | 0 to 500 | Acceleration backward (m/s²). |
| `flight.strafe` | 0 to 500 | A flier's acceleration sideways (m/s²). |
| `flight.lift` | 0 to 500 | A flier's acceleration up with the jet key (m/s²). |
| `flight.hover` | 0 to 1 | A flier's share of gravity it cancels while piloted. |
| `flight.drag` | 0 to 20 | Speed lost a second, as a share. |
| `flight.top_speed` | 1 to 1000 | Its top speed (m/s). |
| `flight.turn` | 1 to 1000 | Degrees a second it turns: a flier toward where its pilot looks, a ground vehicle with the sideways keys. |
| `flight.drain` | 0 to 1000 | Energy a second while thrusting. |
| `flight.climb` | 0 to 89 | Ground: the steepest slope it drives up (degrees, default 35). |
| `flight.grip` | 0 to 100 | Ground: how hard its tracks hold, sideways always and every way while it isn't driven (per second, default 8). |
| `size` | three numbers 0.2 to 20 | Half its width, height and length (m). |
| `mass` | 0.1 to 100 | Mass relative to a light-armored player. |
| `seats` | an object keyed by names (a-z, 0-9, _ and -) | Its seats by name (at most 6): `pilot` flies it; the rest ride, in name order. |
| `seats.*` | an object | One seat. |
| `seats.*.at` | three numbers -20 to 20 | Where it is (right, up, forward m). |
| `seats.*.weapon` | a weapon id | The weapon fired from it along its rider's view. |
| `seats.*.exposed` | true or false | Its rider sits in the open: seen, and hit by shots and blasts (a bike's). |
| `blast` | a blast id | The blast it bursts in when destroyed. |
| `ram` | 0 to 1000 | Damage it deals for each m/s past 10 that it runs into an enemy on foot or another vehicle at. |
| `carries_flag` | true or false | A flag carrier may ride it. |
| `team_limit` | whole number 0 to 50 | Most a team may have at once (0: no limit). |
| `price` | whole number 0 to 1000000 | Credits it costs where the mode has an economy. |
| `abandon` | 0 to 3600 | Seconds it stays with nobody aboard before it's gone (0: until destroyed). |
| `look` | skiff, transport | How it looks. |
| `hook` | a name: a-z, 0-9, _ and - | The hook (hooks/<name>.rhai, server only) that hears its events. |
| `parts` | a list of up to 24 objects of these fields | Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well). |
| `parts.*` | an object | One part. |
| `parts.*.mesh` | cube, bevel, sphere, disc, capsule, dome | Its shape. |
| `parts.*.at` | three numbers -50 to 50 | Where it is (m: right, up, back) in its owner's frame. |
| `parts.*.size` | three numbers 0.001 to 50 | Its size (m) along each axis. |
| `parts.*.turn` | three numbers -360 to 360 | Its turn (degrees: pitch, yaw, roll). |
| `parts.*.color` | three numbers 0 to 1 | Its color (red, green, blue). |
| `parts.*.glow` | 0 to 4 | How much it glows. |
| `parts.*.team` | true or false | Its team's color instead. |
| `parts.*.aim` | a name: a-z, 0-9, _ and - | A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret). |
| `parts.*.elevates` | true or false | With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel). |
| `parts.*.pivot` | three numbers -50 to 50 | With `elevates`: the point it tilts about (m: right, up, back). |

<!-- schema:end -->
