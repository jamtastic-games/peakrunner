# Weapon visuals

First slice of the arsenal upgrade.
Visual only: damage, fire rates, reloads, projectile speeds and movement are
unchanged, and no compatibility marker changed.

## Viewmodels (`src/drawlist.rs`)

- **Disc launcher**: polished, not redesigned. A charge bar along the top of
  the left rail fills over the 1.05 s reload and flashes when ready, the
  muzzle has two prongs that flare on fire, and there's a team stripe.
- **Chaingun**: rebuilt. Six barrels in a shroud ring, a vented heat jacket,
  a rear motor housing and a side ammo drum. Barrels spin up while firing and
  coast down after release. Jacket heat is cosmetic only, with no overheat.
  A small orange-red muzzle burst (see Muzzle blasts).
- **Grenade launcher**: rebuilt. A wide bore, a four-chamber revolving
  cylinder, a pump grip and a stock. The existing reload timer drives one
  cylinder step and a pump rack. The fired chamber stays dark as it turns away
  and the fresh round brightens as it arrives. There is no ammo mechanic.
- **Every weapon**: a 0.25 s switch (old weapon drops, new one rises), bob
  driven by movement (a stride on foot, a low glide while skiing, a float in
  the air), and per-weapon recoil read from the cooldown (heavy shove for the
  disc, fast jitter for the chaingun, a big slow kick for grenades).
- **Third person**: each weapon has its own held model matching its
  first-person silhouette, replacing the shared dark box.

## Rounds

Rounds with an authored model (`docs/model-pipeline.md`) fly as that model.
The disc launcher's disc is painted blue (its `color`) and trails a light blue
ribbon; the mortar shell leaves green smoke along its lob. Both are data, a
projectile's `trail` (`docs/mod-format.md`): a glowing `ribbon` drawn from the
round's flight (`drawlist::push_ribbon`), or `smoke` puffs left where it
passed (`effects.rs`, in their own ring of 192 so a lob never pushes out a
blast's smoke). The disc look, for rounds without a model, has a bright rim,
a dark hub and two rim studs so the spin reads, plus its own ribbon wake. For
the chaingun, every third round is a long bright
tracer and the rest are faint streaks. Grenades have a fuse light that blinks
faster as detonation nears. Turret plasma has a flickering core in two halo
shells.

## Effects (`src/effects.rs`, client-only)

The client watches state it already has (explosion records, rounds, shot
counters) and adds effects that can outlive the 0.55 s blast record:

- **Explosions**: a white flash, flame lobes, a shockwave ring of soft
  segments, a rising smoke column, thrown debris, sparks, a ground-tinted dust
  kick (grass, snow, sand) and a scorch mark that fades in 3.5 s.
- **Generator blast**: the same layers at a larger scale. This replaces the
  old orange bubble.
- **Chaingun impacts**: a round that vanishes between frames and whose last
  step crosses a surface struck that surface, so sparks, a flash and a dust
  puff spawn there. A round that simply expires spawns nothing.
- **Casings**: chaingun casings eject (every second round) and fade in 0.9 s.
- **Muzzle blasts** (`Effects::muzzle_blast`, every player's shots, yours
  included): the disc launcher's flash stays a cold blue. The grenade
  launcher (and the mortar) fires a small orange-red explosion that throws
  the round out: a hot yellow core, a fireball rolling out after the round,
  four flame tongues round the bore, five sparks, then the grey smoke puff.
  The chaingun's is the same much smaller on every round, a spark now and
  then. Your own sits 0.12 m past the first-person barrel (placed with the
  sim's `weapon_muzzle`, where the weapon's rounds leave, since the gun draws
  over the world) at 0.75 scale.
  The viewmodels carry matching opaque cores: an orange-red cross with a hot
  centre just past the chaingun's barrels, and the grenade launcher's core,
  tongues and a fireball that follows the round out for 0.09 s.
- **Grenade trail and generator wreck smoke**: these now use the darkening
  pass.

## Darkening pass (`src/scene.rs`, `fs_smoke` in `src/shaders.wgsl`)

A second effect pipeline beside the additive one. It uses normal alpha
blending, depth test on and depth write off, and no culling (the sphere mesh
winds inward, so culling either face leaves hollow rings). It applies the same
distance fog as lit geometry, computed per draw. Draws are sorted back to
front and drawn after the lit pass and before the additive pass, so flames
glow through smoke. The map shader math is unchanged. It works on WASM:
WebGL2 supports the blend state.

## Bloom (`src/bloom.wgsl`, `Bloom` in `src/scene.rs`)

Glow is marked, not guessed from brightness: the resolved scene colour's alpha
channel carries a glow mask (glow = 1 − alpha; everything else writes 1).

- Lit draws glow in proportion to their `emit` above 0.2 (visors, thruster and
  emitter housings); faint emit such as snow motes does not.
- Additive effects (flames, flashes, plasma, sparks, fireballs) add their
  strength to the mask through a reverse-subtract alpha blend.
- Smoke's normal alpha blend dims the glow behind it.
- Map geometry glows only on the kit's `light` material (texture layer 10 in
  every pack: light strips, lamps, capture-tower and beacon glow), and only on
  its bright texels, so white walls and snow never bloom.
- Both skies mark the sun disc and corona.

After the scene resolves (so 4× MSAA works unchanged), a prefilter keeps the
masked light at half size, a dual-Kawase chain blurs it down to 1/16 (Low) or
1/32 (High) and back up additively, and the blit screen-blends it over the
scene. egui draws the HUD after the blit, so the HUD never glows. Low uses
intensity 0.7 and four levels; High 1.1 and five. Everything is Rgba8Unorm, so
WebGL2 supports it. The capture-point ring is an egui overlay and stays crisp.

## Caps

| Budget | Limit |
|---|---|
| Live particles, all kinds (fixed ring, allocated once) | 360 |
| Bullet impacts spawned per frame | 10 |
| Live casings | 40 |
| Live scorch marks | 12 |
| Additive effect draws per frame (world effects kept first) | 1,200 |
| Smoke draws per frame (farthest dropped) | 220 |
| Full explosion detail | within 220 m |
| Explosions beyond 220 m | flash and one smoke puff |
| Explosions beyond 450 m | nothing extra |

## Checks

- `cargo test -p peakrunner --lib effects`: blast spawning is capped and done
  once, distant blasts stay cheap, impact detection works, the switch sequence
  runs in order, and the chaingun spins up and coasts down.
- `render_weapon_captures` (ignored, GPU): viewmodel states, third person,
  rounds in flight, impacts, and explosions on Old Holler, Frostline and
  Dustreach, written to `screenshots/weapons-after-*.png`.
- `render_gameplay_captures`: same angles as the `before-*` set.
- `heavy_fight_budget` (ignored, GPU, run with `--release`): 8 players, 600
  frames. It reports peak draw counts, frame-build time and GPU render time.
  Measured on the Mac, same probe on both builds:

| | Before (`5713ef1`) | After |
|---|---|---|
| Peak draws | 348 lit, 299 additive | 405 lit, 277 additive, 168 smoke |
| Frame-build time, average | 0.004 ms | 0.057 ms |
| GPU render, median (heaviest frame) | 0.59 ms | 0.68 ms |
| GPU render, p95 | 0.63 ms | 0.82 ms |

## Not in this slice

- Instanced particles and textured decals (bloom has since been added; see above).
- The fired grenade chamber is the only chamber visible from the default view.

## Third-person landing and weapon-switch cues

`src/src/player_model.rs` derives two short cues per player from how its snapshot state changes frame to frame. Nothing new is sent over the network, and every client derives them the same way.

- **Landing squash.** A player lands after at least 0.15 s airborne, falling faster than 8 m/s: the hips sink and the knees fold for 0.38 s, scaled by impact speed. Shorter airborne blips, such as snapshot jitter over bumps, don't count.
- **Weapon switch.** When a player's weapon changes, the held gun dips down and in over 0.32 s. The old model shows on the way down and the new one on the way up. Respawning never counts as a switch.

For captures, `QA_PLAYERS` takes a fourth field, one of `dive`, `land@T` or `switch@T` (see `src/src/qa_overrides.rs`).
