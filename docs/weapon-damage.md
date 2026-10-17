# Weapon damage

Tuning: 2026-09-19 onward. Values are expressed against light armor's 0.66
armor capacity: a 0.50 blast is 0.50 / 0.66 of a light player's health.

## PeakRunner mapping

Use the existing single, fast player class as the light-armour baseline, with
100 health representing 0.66 of armor.

| Weapon | Previous theoretical peak | New peak vs player | Radius, previous → new | Half-radius damage | Edge damage |
| --- | ---: | ---: | --- | ---: | ---: |
| Disc | 64 | 75.76 | 7.5 → 7.5 m | 42.42 | 9.09 |
| Grenade launcher | 80 | 72.73 | 9 → 15 m | 40.73 | 8.73 |
| Mortar (`blast4`, 2026-09-28) | 125 concentrated | 151.52 | 20 m | 84.85 | 18.18 |

Disc peak = `100 × 0.50 / 0.66`.
Grenade peak = `100 × 0.40 × 1.2 / 0.66`.
Mortar peak = `100 × 1.0 / 0.66` (1.0 over 20 m).
Within the radius, damage = `peak × (1 − 0.88 × distance / radius)`.
Outside it, damage is zero. The discontinuity at the boundary is deliberate,
replacing PeakRunner's previous quadratic falloff to zero.

A confirmed direct impact applies the peak **once as splash**, not direct plus
splash damage. Both weapons kill an unhealed full-health enemy in two direct
hits. Near misses are weaker and do not automatically become two-hit kills.

For other players, distance is to the surface of PeakRunner's vertical body
capsule, instead of the old point near the feet. Equipment distance uses
its existing collision sphere. Equipment gets the same normalized blast curve;
its custom health totals are unchanged.

Solid cover still blocks damage and knockback through the existing line-of-sight
test. This is binary cover, not partial coverage.

## Deliberate differences retained

- Self-damage: your own disc hurts you at 40% (`SELF_DISC_DAMAGE`, so disc
  jumps stay possible); your own grenades, mortar shells, hand grenades and
  mines hurt you in full. With the `blast4` mortar, a shell at your own
  feet kills light armor. Friendly fire on teammates stays off.
- Approved skiing, gravity, steering and jetpack settings remain unchanged.
- Blast impulses are now 2000 for discs and 1500 for grenades: maximum velocity
  changes of 22.22 and 16.67 m/s at PeakRunner's 90-unit player mass. Grenades
  previously pushed only 6 m/s. These are our selected combat tuning values.
  Both decline linearly to zero at the radius using body contact distance, push
  away from the impact, and release ground contact for an upward blast jump.
  Self-damage remains reduced, but self-knockback is not reduced.
- Weapon reloads, projectile speeds, inheritance, grenade arming/bounce/fuse
  behavior, and smoke trails are unchanged.
- Main turret plasma remains 55 direct damage, 6 m splash, slight knockback.
- Walls: the viewmodel muzzle sits up to ~1.5 m (more at wide FOV) ahead of the
  eye. Player and bot shots now start no farther than the first surface on the
  eye-to-muzzle segment, so a wall-hugging shot hits the wall on the shooter's
  side instead of spawning outside (`muzzle1` compatibility, source only).
- Bots need line of sight, using the turrets' rule (eye to chest against
  terrain, map collision and pillars). Each think tests the three nearest
  enemies within 95 m and targets the nearest one visible. The trigger re-checks
  sight before every shot, so bots never fire at walls. When sight breaks they
  drop the target and head for the last-seen spot for 2.5 s without firing; no
  blind grenade lobs. Bot AI runs only on the authority, so no compatibility change.
- Turrets and sensors acquire a target only with a clear segment from the muzzle
  (radius + 0.6 m out) to the target's chest, re-checked every tick, against
  terrain, pillars and the map collision mesh; the shot is re-checked before
  firing and bolts collide with the same geometry. Aligned open doorways are real
  sight lines: fix those in map layout, not by blinding turrets.

Tuning lives in `crates/core/src/combat.rs`; the shared authoritative explosion
path is in `sim.rs`. The gameplay compatibility marker includes `blast3:chat1`, so
old clients/servers cannot silently play together with different damage rules.
The directory protocol is unchanged. Live deployments require matching builds.

## Turret targeting

Every sensor and turret uses one rule, `equipment::acquire_target` in
`crates/core/src/equipment.rs`. It picks the nearest living enemy within range
whose chest (`CHEST_HEIGHT` 0.8 m) is visible from the barrel start, which is
body radius + `MUZZLE_GAP` 0.6 m toward the target. Leading weapons aim at the
constant-velocity intercept point. Before firing, the caller re-checks the line
from the muzzle to the aim point. Line of sight is a caller-supplied closure;
`step_equipment` passes terrain, pillars and the map collision mesh.

Numbers live in one table, `equipment::profile(kind, weapon)`:

| Profile | Range | With powered sensor | Speed | Life | Cooldown | Leads |
| --- | --- | --- | --- | --- | --- | --- |
| Sensor (detects only) | 260 m | 260 m | — | — | — | no |
| Bullet turret | 80 m | 150 m | 420 m/s | 1 s | 0.22 s | no |
| Plasma turret | 80 m | 150 m | 80 m/s | 3 s | 1.2 s | yes |

The table reproduces the numbers the inline code used; a test compares the old
rule with the shared one across Tower Complex, Cairnhold and Old Holler. Future
player-placed turrets should call the same function with their own profile row.
They will also need placed objects added to the line-of-sight closure, so a
deployed shield or turret blocks sight, and placement validation (clearance, no
placing through walls or with a line straight into an enemy room). Neither
exists yet.

**Field of fire (`arc1`).** Doorways and windows are open, so fixed turrets
are no longer walled off from the rooms behind them. Instead each turret in a
map pack has a `facing` (horizontal direction, toward the enemy flag) and an
`arc` (200 degrees), written by `scripts/assets/turret_arcs.py`. Before
`acquire_target`, `step_equipment` drops candidates the turret may not engage
(`Definition::in_arc`): anything inside `ALL_ROUND_RANGE` (15 m horizontally)
is fair game in every direction, so a turret still guards its own bridge or
ramp; beyond that only targets inside the arc count, unless they stand under
open sky (nothing overhead for 40 m): the arc protects the turret's own rooms,
not the roof, ramps and ground around it. Before `arc2`, a player on the hall
roof more than 15 m behind an Old Holler roof turret was never engaged.
Packs without `facing` keep 360-degree coverage. These are simulation
changes, so the compatibility marker carries `arc2` (was `arc1`).

## Equipment shields and hit bars

Sensors and fixed turrets carry a shield projected by their circuit's
generator. Generators, inventory stations and repair pads have none: the
generator is what attackers go for. Numbers live in
`equipment::durability(kind)` (`crates/core/src/equipment.rs`):

| Kind | Hull | Shield | Regen | Bullets vs. shield |
| --- | --- | --- | --- | --- |
| Turret | 250 | 450 | 110/s, continuous | 50% |
| Sensor | 150 | 300 | 110/s, continuous | 50% |
| Generator | 500 | none | — | — |
| Inventory / repair | 300 | none | — | — |

- Damage hits the shield first; overflow reaches the hull. Shields take half
  of bullet damage (chaingun and bullet turrets), explosives in full.
- A powered shield regenerates **continuously, including while it is being
  hit** (`SHIELD_REGEN`, 110/s). That is above the best one-player sustained
  damage, so a lone attacker cannot break it; two attackers focusing can, and so
  can killing the generator.
- When the generator goes down the circuit loses power: shields drop to zero
  at once, cannot regrow, and the equipment stops working. Its hull can then be
  destroyed directly.
- **Destroyed stays offline.** Anything whose hull reaches zero (generators,
  turrets, sensors, stations) stays offline until repaired past half its hull
  (`ONLINE_FRACTION`, 0.5). The same rule for every kind keeps "repaired" meaning
  one thing; a generator at 40% powers nothing. `offline` travels in snapshots.
- **Generators explode.** The killing blow sets off a large fireball and boom
  (explosion kind 4) that **damages nothing**. It travels as an ordinary
  explosion (blast serial plus snapshot), so a replayed snapshot never plays it
  twice. The wreck smokes and sparks while offline; the smoke thins once repair
  starts. Both teams get a centred announcement: "Enemy generator destroyed" /
  "Your generator is down", and "... back online" when a repair crosses the
  threshold (a round reset to full hull is silent).
- `always-on` circuits (maps without generators) count as permanently powered,
  so their equipment keeps a regenerating shield.
- Repair is unchanged: holding E restores hull only, never shield.
- All of this is server-authoritative.

**Splash reaches equipment past its own mount.** Blasts need a clear line to
the object's hit sphere, but the collision mesh does not tag which solid belongs
to which object, and the turret mounts are solid. Grenades bursting on the floor
beside a turret were blocked by the turret's own mount (0 of 16 sampled bursts
landed on almost every turret). A hit inside the object's footprint column
(hit radius + 1 m, down to 6 m below its centre) now counts as reaching it;
walls farther out still block (`splash_reaches` in `sim.rs`, with tests on all
five maps).

Sustained fire against a powered shield, best case (every explosive point
blank, every bullet hits; disc 75.8 per 1.05 s = 72/s, grenade 72.7 per 0.85 s
= 86/s, chaingun 8 × 50% per 0.075 s = 53/s). Time until damage first gets
through to the hull:

| Target, weapon | 1 player | 2 players | 3 players |
| --- | --- | --- | --- |
| Turret, disc | never | 11.0 s | 3.9 s |
| Turret, grenade | never | 6.4 s | 2.8 s |
| Turret, chaingun | never | never | 9.0 s |
| Sensor, disc | never | 6.8 s | 2.5 s |
| Sensor, grenade | never | 3.8 s | 1.7 s |
| Sensor, chaingun | never | never | 6.0 s |

After that the hull falls at the attackers' combined rate minus 110/s: two
grenadiers finish a 250 hull turret in about 4 s more, two disc players in about
7 s. Plasma is a turret-only weapon. With the generator down, a turret still
takes 4 discs and a sensor 2. Regenerate the table with
`cargo test -p peakrunner-core --lib print_time_to_break_table -- --ignored --nocapture`.

**Repair kits.** Each player carries one kit per life (`KITS_PER_LIFE`).
Pressing **Q** restores 60 armor over 2 s (`KIT_HEAL`, `KIT_SECONDS`), only when
alive, holding a kit, not already healing and below full armor. Taking damage
does not cancel the heal; death does. Inventory stations refill it. The client
only sends the intent (`Command::kit`); the server spends the kit and heals, and
prediction never does. The HUD shows the kit count and key beside the armor bar,
and the remaining heal while it runs.

**Hit bars.** The client draws a bar above every generator, turret and sensor
within 120 m and in line of sight: a thin shield strip (pale violet) above a
hull bar that turns green, amber, red and is framed in the owning team's
colour. An unpowered shield shows as a dashed grey strip with OFFLINE; a
destroyed object says DESTROYED. Bars flash white when the object takes damage,
and name the object within 45 m (`src/world_overlay.rs`). A bar never goes
through a ceiling: `bar_anchor` probes upward from the top of the model and
hangs the bar 0.4 m under anything it finds, and where there is no room above
the model (a low basement), it hangs the bar beside the object on the viewer's
side, so a generator's bar can't be seen from the floor above.

**Names.** Only the player under your crosshair is named, over their head
(`docs/hud-audio.md`): blue for a teammate, red for an enemy, out to 250 m.
The colour is relative to the viewer, not Ember/Glacier. The name needs a
clear line of sight through the same terrain and map-collision query the
server uses (`World::sight_clear`). That check is cosmetic: every player
position already arrives in snapshots, so hiding names behind walls neither
reveals nor protects anything.

## Laser rifle lag compensation

The laser rifle (light armor, 50 damage, 24 energy, 1.1 s reload) is the only
instant-hit weapon, and the only one the server rewinds. A command that fires
it names the server time of the view the shooter was drawing; the server tests
the beam against other players where they were then, at most 250 ms back
(`sim::lagcomp::MAX_REWIND_TICKS`), never ahead of now, and against the map,
deployables, equipment and mines as they are now. Damage, energy and reload
are unchanged. Aiming straight at a target works up to about a 120 ms round
trip; beyond that, lead by the excess. The cost is that a target can be hit
up to 250 ms after reaching cover, as the shooter saw them. Bots shoot at the
present. The railgun slug, discs, chaingun rounds, grenades and mortar shells
fly forward on the server and still need leading. Details:
`docs/multiplayer.md` (laser lag compensation).

## Verification

Regression tests cover exact peaks, half-radius and edge damage, outside-radius
immunity, monotonic falloff, direct-hit single application, two-hit kills,
multiple splash victims, wider grenade reach, building cover, equipment damage,
self/friendly damage, and preservation of the custom plasma behavior.

## Linear splash for the grenade launcher and mortar (`blast4`, 2026-09-28)

From the loadouts release until `blast4`, the grenade launcher and mortar hit
players with a steep "concentrated" curve (`peak / (1 + (d / 0.6)²)`): the
mortar did 125 at the body but 33 at 1 m and 10 at 2 m, less than the disc,
and the grenade launcher 13 at 1 m. Both now use their full values against
players, the same linear falloff as the disc (100% at the centre, 12% at the
radius): mortar 151.5 over 20 m, grenade launcher 72.7 over 15 m. They
already used these values against equipment and deployables. Kicks are
unchanged (mortar 5000, grenade launcher 2000).

| Distance | 0 | 2 m | 5 m | 7.5 m | 10 m | 15 m | 20 m |
|---|---:|---:|---:|---:|---:|---:|---:|
| Disc | 76 | 58 | 31 | 9 | – | – | – |
| Grenade launcher | 73 | 64 | 51 | 41 | 30 | 9 | – |
| Mortar | 152 | 138 | 118 | 102 | 85 | 52 | 18 |

Heavy armor takes half. The laser rifle now does 60 (was 50), at a fixed
cost of 24 energy.

Left alone: the chaingun does 8 per round at about 13 rounds a second; the
hand grenade does 0.5 over 10 m and the mine 0.65 over 10 m. Tests: `combat::tests::the_mortar_and_grenade_launcher_use_linear_splash`,
`sim::equipment_tests::a_mortar_six_metres_off_kills_light_armor_and_maims_heavy`.
