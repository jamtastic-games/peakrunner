# Loadouts, the repair tool, the mortar and deployables

Everything in this doc applies in CTF and Capture & Hold. Football has its own
armor and no weapons (`docs/football.md`). Compatibility markers: `loadout1`,
and `throw1` for grenades and mines (`throw2`: a throw keeps all of the
thrower's velocity, so a skier's grenade flies on ahead of them).
Not deployed.

## Armor (`crates/core/src/loadout.rs`)

Three armors, chosen at an inventory station and kept across deaths
(`src/assets/mods/base/armors/base.json`).

| | Light | Medium | Heavy |
|---|---|---|---|
| Damage taken | 1x | 0.75x | 0.5x (twice the armor, 1.32 vs 0.66) |
| Walk speed | 1x (approved movement) | 0.8x | 0.55x |
| Jet | approved standard (37.3 m/s², drain 15/s, recharge 12/s, adds speed to 72 km/h) | 30 m/s², 15 sideways, adds speed to 54 km/h, drain 13/s, recharge 9/s | 23.5 m/s² (3.5 net of gravity), 8 m/s² sideways, adds speed only to 36 km/h, drain 11.3/s, recharge 6.5/s: hard to fly, barely climbs |
| Jump | standard | the same | the same |
| Mass (shoves, blast kick) | 1x | 1.4x | 2x |
| Weapon slots | 3 | 4 | 5 |
| Launcher / rifle | grenade launcher / laser | grenade launcher / none | mortar / railgun |
| Ammo: disc / repeater / launcher / railgun / plasma | 15 / 100 / 10 / – / 30 | 20 / 150 / 12 / – / 38 | 25 / 200 / 10 / 20 / 45 |
| Hand grenades / mines | 5 / 3 | 6 / 3 | 8 / 3 |

### Who carries what

The three sizes split the arsenal and the packs (`armors/<id>.weapons` and
`armors/<id>.packs`; test `the_three_armors_split_the_arsenal_and_the_packs`):

| | Light | Medium | Heavy |
|---|---|---|---|
| Disc launcher, repeater, plasma gun, blaster | yes | yes | yes |
| Drain gun, target marker | yes | yes | yes |
| Grenade launcher | yes | yes | (the mortar instead) |
| Laser rifle | **only light** | – | – |
| Mortar, railgun | – | – | **only heavy** |
| Energy, shield, repair, jammer and ammo packs | yes | yes | yes |
| Motion and pulse sensors, jammer pylon | yes | yes | yes |
| Turret, wall, force field, ammo station, field inventory | – | yes | yes |

Light is the runner and sniper; medium the all-rounder that carries four
weapons and every pack; heavy the anchor with five weapons, the mortar and
the railgun. Taking an armor that can't wear the pack you have takes the
pack off; it never comes back at a respawn in that armor. The grenade
launcher and mortar share a family, so heavy armor carries the mortar in its
place (an armor carries one weapon of a family).

**Worn packs** (`packs/base.json`, marker `packs1`): the energy pack doubles
the recharge; the repair pack, switched on, mends 6 health a second for 5
energy a second; the **shield pack**, switched on, pays hits from energy at
half an energy a point of the hit before armor (the base `shielded` effect's
`absorb`), so every armor pays the same for the same hit: a heavy, which
takes half of each hit, pays a whole energy for each point it is spared. The
shield and the **sensor jammer** drain a multiple of their wearer's recharge
(`drain_regen`: 1.125 and 1.3125, the classic's ratios), so either one
empties the tank slowly even at rest and switches off at 2 energy. Standing
still, a shield loses 1.5 energy a second on light armor, 1.1 on medium and
0.8 on heavy; a jammer 3.75, 2.8 and 2.0. Before `packs1` they drained a flat
3 and 2 a second, less than any armor recharges, and the shield cost 1.5
energy a point after armor.

**Drain gun** (`drain-gun`, the `drain-stream` beam): hold it on an enemy
within 40 m: it drains 25 of their energy a second and does 4 damage a
second, for 4 of yours. **Target marker** (`target-marker`, the
`marker-beam` stream): a held, steady orange laser out to 1000 m, for 3
energy a second, that harms nobody. Whatever it touches is its holder's one
target for the team: an enemy (followed as they move) or the spot it lands
on, moved as the laser sweeps, and kept 12 s after letting go. A marked enemy
shows on every teammate's screen at any range, through walls; a marked spot
shows as an orange TARGET reticle with its distance, through walls too. The
team's turrets reach their sensed range while it has a mark out. Snapshots
carry the viewer's team's marks (`Snapshot::marked` and `marked_spots`;
never to the other team; marker `marks1`).

Heavy's jet barely out-pulls gravity (385 on mass 18). It lifts you onto a ledge or slows a fall; it doesn't fly you
across the map.

Steering takes lift (marker `steer1`): the jet is one thrust, and the push
along the movement keys is paid out of it, so what pushes sideways no longer
lifts. From a standstill light armor leaning in keeps 15.3 m/s² of lift
against gravity's 20 and sinks, as do medium (15) and heavy (15.5), until the
push fades with speed toward its cap; past it (skiing, a blast) the whole jet
lifts again, while turning across the line of travel still costs lift. Climb
first, then lean in. Bots fly the same way: they push only for the speed their
climb needs (`bot_nav::lean_speed`).

The jump-boost (marker `jumpjet1`, `movement.jet_jump` 0.5 in `defs.json`):
jets pressed on the ground, standing, running or skiing, kick you straight up
with half a jump (4.2 m/s in light armor, scaled by mass and the armor's
`jump`) whatever your speed, so a jump-jet off a ski rises while you fly on.
It comes once per press: jets held over a landing, or sputtering back on as
an empty tank refills, don't kick again. A jump on the same takeoff takes its
place (the kick only tops the jump up to half a jump, so never stacks), and
climbing already shrinks it as it does a jump. The middle between two older
designs: one where only the jump kicked, at any speed, and one
whose jets jumped by themselves. Football's movement doesn't have it.

Skiing is heavy's real speed: momentum from a slope is kept
(`Armor::air_cap` only limits what the jet adds). Heavy armor is drawn bigger
all round.

## Weapon slots (`crates/core/src/arsenal.rs`, marker `arsenal1`)

Each armor carries a fixed number of weapons (`armors.<class>.weapon_slots`:
light 3, medium 4, heavy 5), chosen at an inventory station from the weapons
the mode allows (`ModeDef::weapons`) and the armor may carry (above). Until a
player chooses, they carry their armor's kit (the disc launcher, repeater and
launcher; medium the plasma gun too), as bots always do. The order chosen is the
slot order, and **number keys pick slot 1, 2, 3…** (the keys are "Weapon slot
N" on the Controls screen; the wheel cycles what you carry). The HUD shows
the slots over the weapon name, the one in hand lit. Changing armor keeps the
choices that fit, in order (light keeps the first three).

**Plasma gun:** a slow glowing bolt (70 m/s, no drop, 3 s) that bursts where
it meets anything (`blasts.plasma`: 4.5 m, 36 peak, a light kick; your own
burst hurts you at 60%). Fizzles in water. 0.55 s between shots; 30 rounds
in light armor, 45 in heavy.
**Blaster:** fast bolts (190 m/s, 1.4 s) on energy, never ammunition (7 a
shot, 0.3 s apart), 13 damage to whatever they hit and no splash. A bolt
glances off the map, keeping 80% of its speed (`weapons.blaster.bounce`).

## Ammunition

Every weapon has limited rounds (`Player::ammo`). Each shot spends one; an
empty weapon clicks (`dry`) and waits 0.35 s before trying again. Online
prediction spends rounds too, so an empty weapon never predicts a shot (it
used to cycle its reload animation). An empty disc launcher shows no disc in
its tray and a dark charge bar; an empty grenade launcher has no lit rounds. Rounds refill
on respawn, and while using your team's powered inventory station (hold E).
Bots restock just by reaching their station, and fall back to it when out of
disc and chaingun rounds.

**Death packs** (`World::drop_loot`, marker `loot2`): a dying player
drops a pack drawn from what they carried. Anyone who runs over it takes
what fits, and it lasts 20 s. The pack holds:

- 1–5 discs at random, never more than they had;
- all their chaingun rounds, so a picker fills up to their own max;
- 1–5 third-weapon rounds at random, never more than they had;
- 1–5 hand grenades: at least one, even if they had none, otherwise never
  more than they had;
- no mines and no railgun slugs.

Only matching kit is taken: grenade-launcher rounds go only to light and
medium armor, and mortar shells only to heavy. Test:
`the_dead_drop_a_roll_of_what_they_carried_and_it_vanishes_after_20_s`.

## Mortar (heavy armor's third weapon)

A long green lob (65 m/s, half the thrower's velocity inherited, full
gravity): about 150 m at 20° elevation and 210 m at 45° (it was 66 m at 20°
at 42 m/s). It explodes on impact. Like the grenade launcher's shell it has a
launch safety: for its first 0.35 s (`MORTAR_ARM`) it bounces off whatever it
meets, so a short shot inside a building rattles round the room first; after
that the next contact sets it off. A shell that never lands goes off after
20 s. Blast (2026-09-28, `blast4`): 1.0 over 20 m with the
disc's linear falloff: 152 at the centre, 138 at 2 m, 118 at 5 m, 85 at 10 m,
18 at the edge. It kills light armor within about 7.8 m and heavy armor
within about 5.2 m. The shock is intense: kick 5000 reaching 20 m, in green
flames. Reload 2 s. `combat::MORTAR`, `Disc::kind` 5. The grenade launcher's
shell does 0.40 over 15 m with light armour's 1.2 multiplier
(73 at the centre, 51 at 5 m, 9 at the edge; kick 2000). Until `blast4` both
used a steep concentrated curve (the mortar did 10 damage at 2 m, less than
the disc); against equipment and deployables they already used these
values (`structure_blast`), so the shield balance in `docs/weapon-damage.md`
is unchanged. Any
blast on an airborne enemy kicks 1.5x (`AIR_HIT_KICK`): a mid-air disc throws
them hard; your own disc jumps are unchanged.

**Elevation gauge** (the mortar's `elevation`): beside the crosshair, a
scale from -20° to 90° with ticks every 5°, labels every 30°, and 45° (the
longest lob on level ground) in amber; a pointer and the reading ("+35°")
show the aim's angle above the horizon, so a shot can be repeated or walked
in by angle (`elevation_gauge` in `src/app.rs`).

## Repair tool (hold Q; replaces the repair kit)

While held, a beam repairs the nearest friendly thing within 12 m under your
crosshair and in sight (a teammate, your team's equipment or deployables), or
yourself. It spends 5 energy per second (2026-09-26: was 12, which took about
three tanks to bring a generator back online; now about one) and needs 3 to
run. The line-of-sight check allows for the target's own size, so a
generator's shell no longer blocks its repair. Rates: 6 health per second on players (a full heal takes
about 16 s), 20 hull per second on equipment and deployables. Your weapon is
lowered and can't fire while the key is held. The tool stays in your hands
for as long as you hold the key (`Player::repair_out`, marker `repair1`): when
the tank runs dry it stops, its glow dims and the HUD says it is waiting for
energy, and it rests 1.5 s (`repair.rest`) while the tank refills before it
tries again. It used to restart on each tick's recharge, flickering between
the tool and the weapon on an empty tank (jetting while repairing ran it dry
quickly). Wrecked equipment comes back online
past half its hull, as before. The old E-to-repair at equipment is gone; E
still uses stations. The beam's end travels in snapshots (`Player::repair_beam`)
for rendering and the repair hum. Commands still accept the old `kit` field.
Working on yourself (`Player::repair_self`, marker `repair2`), the tool shows
as a soft green glow on your body with motes rising through it, not a stream:
the stream goes only to a teammate, equipment or a deployable. It used to be
told apart by distance, so a moving player's self-repair drew a stream back
to where their snapshot had them.

## Inventory stations

A station is a booth you **step into from the front**: it opens only within
30° of the way it faces (`loadout::FRONT_CONE`, `station_front`). Maps give
each station's `facing` (the map kit writes it; a map without one has it found
by probing which way the booth is open). Beside or behind one, the HUD says
"step in from the front". There, **press E** to open the inventory screen:
Armor, Favorites, Weapons, Packs, Miscellany. While it's open the mouse is
free, and standing there heals and recharges you and restocks rounds, grenades
and mines.

- **Armor:** 1 light, 2 medium, 3 heavy (restocks for it).
- **Favorites:** four saved setups (armor, weapons in order, pack) kept in
  `client.json` (`setups`): "Save current" stores what you have on, Use (or
  F1-F4) equips one whole (`Command::equip`; equipping it again changes
  nothing).
- **Weapons:** "N of M slots"; each weapon shows its slot and rounds, with
  Take or Leave ("Slots full" when there's no room).
- **Packs:** the ones your armor may wear: worn packs first (energy,
  shield, repair, jammer, ammo), then the ones you place (sensors, jammer
  pylon, and for medium and heavy the turret, wall, force field, ammo
  station and field inventory).

E or Esc closes it; stepping off the station or dying closes it too. A pack is
carried until deployed or you die. Purchases are server-validated intents
(`Command::buy`: `BUY_TAKE + kind`, `BUY_DROP + kind`, packs). There is no
money.

A **field inventory** (a pack, 2 per team) is a kiosk that serves the same
from its front (the face toward whoever placed it), except armor and the
station packs, and restocks like an ammo station.

What you take is your **setup** (`sim::Setup`, `Player::setup`): every
respawn gives back your armor, weapons and pack (restocked), until you take
something else, which replaces only that part. It lasts through deaths,
rounds, twists (a twist's forced armor or rifle ends with its round, yours
stays) and map changes, and applies only where the mode sells it
(`purchases`; a pack only with `deployables`, a rifle only where `weapons`
allows it). Players who never buy, and bots, spawn as before. Server and
offline state only; nothing new is sent.

## Rifles (`crates/core/src/rifles.rs`, marker `rifle1`)

A weapon kind like any other: taken into a slot at a station (it takes one),
never carried by default, lost on death and back at the next respawn once
chosen (your setup). Which rifle you hold follows your armor:

| | Laser rifle (light only) | Railgun (heavy only) |
|---|---|---|
| Shot | a straight beam from the eye along the crosshair, instant, to 2.6 km (the whole map) | a glowing slug at 600 m/s (300 until 2026-10-09), straight, led at range (0.25 s to 150 m, a third of a second to 200 m) |
| Cost | 24 energy per shot (you need 24 to fire) | 20 slugs, restocked at stations; **each shot empties your energy** (`empties_energy`), so no jets right after |
| Damage | 60 (50 before `blast4`) | 126.67: downs light armor in one hit, takes 95 of medium's 100 (one hit downs a medium already hurt), 63 of a heavy's |
| Reload | 1.1 s | 1.6 s |
| Motion | none | keeps 15% of your velocity |

Both damage enemy players and equipment, any team's deployables, and can be
fired from the hip.

- **Zoom:** hold **E** away from a station. The view narrows to
  22° (about 4× nearer) with a sight overlay, and the look slows to match;
  while zoomed, the wheel picks the second level, 11° (about 8×), and back
  (`rifles::ZOOM_FOVS`; the level is kept for the next zoom). Through the
  sight, people are told apart as much farther out as it magnifies: name
  tags and enemy arrows reach 4 or 8 times their usual ranges (teammates'
  names 150 m by eye, enemies' names 80 m, enemy arrows 250 m;
  `World::zoom_power`). The target marker's sight zooms the same way. At a
  station, E still uses the station.
- **Seeing shots:** a laser shot reaches every client as a short-lived beam.
- **Checks:** the server refuses the rifle slot until a rifle is bought. The
  weapon command accepts slot 3.
- **Sounds:** new synthesized laser and railgun cues.

## Views and stations

- **R:** toggles a third-person view over the right shoulder: the camera
  2.5 m behind your eye, 0.8 m to its right and 0.35 m up (`THIRD_PERSON_*`
  in `sim.rs`), so your body sits left of center. Each leg of the offset,
  out to the shoulder and then back, is pulled in off walls. The camera is
  off the line your shots fly, so the crosshair marks where the shot will
  land: the muzzle's line to the first thing it meets
  (`World::aim_line`, `app::third_person_aim`). It sits near the center at
  range and drifts toward your body when something is close. A zoomed
  sight is always first person. While placing a pack, R still turns it.
- **Inventory screen:** opens by itself when you step onto your inventory
  station. After you close it, E opens it again.

## Hand grenades and mines (`crates/core/src/throwables.rs`)

Press **G** (grenade) or **N** (mine) to throw one, always at full strength
(2026-09-26: the hold-to-wind-up bar was removed; players found it unreliable
and wanted full throws). The server still accepts a `throw_strength`, which
the client now always sends as 1.0. Mashing empties the belt fast: the pause
between throws is 0.1 s (`THROW_TIME`), the client queues every press, and a
request made during the pause is held (`throw_held`) and thrown when it ends.

| | Hand grenade | Mine |
|---|---|---|
| Carried (light / heavy) | 5 / 8 | 3 / 3 |
| Full throw | 20 m/s plus your velocity | 24 m/s plus your velocity |
| Goes off | 2 s after the throw | when an enemy comes within 2.5 m, once armed |
| Blast | 10 m, 76 peak (0.5 of armor) | 10 m, 98 peak (0.65 of armor) |

Both bounce dully (elasticity 0.15) and come to rest. A mine arms 1 s after
it settles (its light pulses in its team colour once armed). Teammates never
set it off, and team splash rules still apply, so your own mines can't hurt
teammates. Any blast within 4 m sets a mine off, a teammate's included (a
chain can follow), and so does a chaingun round, railgun slug or laser that
passes within 0.35 m of a resting mine: that is how you clear your own. A
team can have 12 mines out; a mine
that never settles within 8 s fizzles, and one lies out at most 15 minutes.
Respawns, inventory stations and ammo stations restock both. Kill feed:
"Grenade", "Mine". Bots don't throw them yet.

## Controls screen (`src/keybinds.rs`)

Main menu **Controls…** or the pause menu **Controls** lists every keyboard
action. Click one and press its new key; a key already in use swaps onto the
other action. Escape, Tab and Enter are reserved. Bindings save with the
client preferences (`client.json`, `keys`); unknown or bad entries fall back
to the defaults. The mouse (fire, jet) and arrow keys (movement) stay fixed.
Hints on the HUD and menu follow the bindings. Since 2026-10-06 M opens the
full map and the mine moved to N; a saved binding keeps its key.

One-shot keys (deploy, suicide, throws, purchases) latch until a simulation
tick sends them (`Input::clear_once`), so a press on a frame that runs no tick
isn't lost, and one press acts once.

## Deployables (`crates/core/src/deploy.rs`)

**Friendly fire clears them** (marker `ff1`): every team's rounds and blasts
damage deployables, so players can shoot their own team's wall or turret out
of a doorway. Force fields still let their own team's shots through, so a
blast (disc, grenade, mortar) is what wears down your own field. Tests:
`own_team_fire_and_blasts_wear_down_own_walls_and_fields`,
`own_mines_can_be_cleared_by_shooting_them_or_a_blast`.

Press **B** (was G, now the grenade) to bring up the deployer: it replaces
your weapon, and a hologram of the unit sits where your aim meets a surface
within 12 m (`DEPLOY_RANGE`). It's in your team's colour where it fits and
gray where it can't go (the reason shows under the crosshair), and vanishes
when nothing is in reach ("TOO FAR AWAY"). The wheel or **R** turns it 15° a
step; click places it; B, a weapon key or the repair tool puts the deployer
away. Client and server run the same check (`World::aim_placement`), and the
command carries the turn (`deploy_turn`), so what you see is what you get.

| | Turret | Wall | Force field | Field inventory |
|---|---|---|---|---|
| Per team at once | 4 | 8 | 8 | 2 |
| Hull | 150 | 400 | 250 | 300 |
| Shape | tripod post, 0.7 m radius | 4 x 3.2 x 0.36 m panel | 4 x 3.2 m panel between posts | 1.2 x 1.9 x 1 m kiosk |
| Blocks | players, shots | everyone and every shot | only the other team and its shots | players, shots |
| Does | chaingun rounds at enemies in sight, 70 m, 0.25 s | cover | a door your team walks through | weapons and small packs from its front; restocks |

Ammo stations: 8 per team at once (raised with walls and force fields on 2026-09-26; turrets stay at 4).

Sensors and the jammer pylon (`deployables/library.json`), placed the same
way by any armor: a **motion sensor** (6 per team) senses enemies moving
within 60 m, with an alarm; a **pulse sensor** (4) senses every enemy within
100 m; sensed enemies show on the team's screens and the team's turrets reach
their sensed range. A **jammer pylon** (3) hides teammates within 20 m from
sensors and turrets. The worn **sensor jammer** pack does the same for its
wearer while switched on.

Placement needs flat ground where you aim (at any height in reach), room for
the unit and the whole panel, and keeps 12 m from flag stands, 6 m from spawns and 3 m from
other deployables. The server says why when it refuses ("NO ROOM THERE", "YOUR
TEAM HAS THE MAXIMUM OF THOSE DEPLOYED", ...). Deployables take bullet and
splash damage from the other team, can be repaired with the repair tool, and
blow up (harmlessly) at zero. Walls and fields block player movement and
projectiles; turret and repair sight lines see through your own fields.
Snapshots carry them whole (`Snapshot::deployables`).

## Controls added

Hold Q repair · E at a station opens the inventory · B deploy · G grenade ·
N mine · M full map · Ctrl+K respawn · Ctrl+F toss the flag · 1-6 weapon slots. All
rebindable (Controls screen).

**Flag toss** (marker `toss1`, `World::toss_flag`): a carrier presses Ctrl+F
to throw the flag ahead along their aim (16 m/s plus their own velocity and a
little lift; never steeply down). It flies until it meets the map and lies
there on the dropped flag's 14 s timer. Anyone can take it in the air: a
teammate catches it, a defender returns it, and the thrower only after 1 s.

## Main menu and server rotations

- The main menu has three pages (`MenuPage`): **Singleplayer** (an offline
  match against bots), **Multiplayer** (Find match, Host game) and the
  **Library**: how to play in simple words with pictures captured in the game
  (`src/library.rs`, `assets/library/*.png`; the browser build shows the
  words only). Esc goes back a page. Controls and Graphics are on the home page.
- Singleplayer picks **mode first** (Capture the Flag, Capture & Hold,
  Football), then lists only maps that host it (`map_catalog::supports`,
  `maps_for`): stadiums host only Football; CTF maps host CTF, and Capture &
  Hold when they place at least two capture points.
- Server rotations accept playlists besides `{"map":...,"mode":...}` entries:
  `{"playlist":"ctf_cnh"}` (every CTF map in CTF, then every Capture & Hold map
  in Capture & Hold), `{"playlist":"football"}`, `"ctf"`, `"capture_and_hold"`.
  They mix with explicit entries (up to 64 matches). The official `match`
  server uses `[{"playlist":"ctf_cnh"},{"playlist":"team_deathmatch"}]`; the
  football server uses `[{"playlist":"football"}]`.
- Explicit entries are checked the same way: CTF on a stadium, Football
  without a field, or Capture & Hold without two points are refused at startup.
- The server's status label now includes the mode ("Old Holler - CTF",
  "Longfield - Football"); the website shows it.

## QA

`QA_DEPLOY="turret:0:x,y,z,yaw;wall:1:...;field:0:..."`, `QA_HEAVY=Name,me`,
`QA_MORTAR=x,y,z`, `QA_MORTAR_BLAST=x,y,z,age`, `QA_KIT=heal` (the local repair beam),
`QA_THROWN="mine:0:x,y,z;grenade:1:x,y,z"` (resting, mines armed), `QA_SHOP=1`
(inventory screen), `QA_EMPTY=1` (empty weapons), `QA_CONTROLS=1` (controls
screen over the menu).
Rifles: `QA_RIFLE=1` (or `=heavy`) buys and raises the rifle through the
server at a station, and `QA_FIRE=1` holds the trigger. The GPU capture test
`render_weapon_captures` writes `weapons-after-vm-{laser,railgun}-{idle,firing}.png`.

## Open items

- Bots don't buy heavy armor or packs, or deploy; they repair themselves and
  restock. In a soak run they fired about half as often (ammo limits and
  trips to restock).
- Deployables have no hit bars yet; they dull as they're damaged.
- Bots don't throw grenades or lay mines.
- Human playtest: armor balance, ammo counts, mortar power, deploy limits,
  throw distances and mine limits.
