# Scoreboard, flag bearings and distance audio

Release update: `.20260919.3` and its matching server are now public, together with
the native launcher. The previously staged FOV/capture/turret changes below shipped
in this coordinated release. See `docs/launcher.md` for verification and caveats.

Deployed to Springdale Central and the Mac, Windows and Linux downloads on
2026-09-19.

- Hold Tab: each connected player's server-measured QUIC RTT appears in ms.
  It is not client-reported or the input-ack latency. Unavailable transports
  (local TCP QA) display an em dash. Connection cleanup removes its RTT entry.
- Off-screen flag arrows use the scene camera basis and FOV, team colors and
  distance in world meters. Behind-camera targets stay on the HUD boundary.
  Carrying the enemy flag replaces its self-pointer with our capture-base target.
- Remote shots and explosions have position-based volume. Full volume within
  6 m, smooth squared falloff to silence at 120 m for shots, 180 m for explosions.
  Offline bot shots follow the same path. Local weapon/reload/hit/death feedback
  and match/flag announcements intentionally remain non-positional. Ambient
  emitters retain their existing distance handling. This adds attenuation, not
  wall occlusion, HRTF, or simulated sound travel time.
- Snapshot ordering prevents repeated snapshot audio. First join/new round does
  not play historical remote shots/explosions. Sounds are not gameplay authority.

Wire compatibility is now `ping1`; old clients and servers deliberately reject
each other. Future releases must assign a new release version and rebuild/deploy
matching server and client archives. The directory itself needs no protocol code.
Do not publish these under the existing immutable release filenames.

Verification: workspace library tests (including encrypted RTT transport), native
all-target compilation, wasm library check, dependency-boundary checks, and the
real native local-match screenshot in `screenshots/ping-flag-hud.png`.
The screenshot uses local TCP and therefore correctly shows unavailable ping.
Subjective multi-machine audio balancing remains a playtest task.
Public verification: two encrypted clients for 10 seconds, maximum snapshot gap
100 ms and input-ack gap 9 ticks. Native WAN screenshot
`screenshots/public2-wan-ping.png` shows the server-measured 40 ms RTT.

Reproduce the isolated HUD capture:

```sh
QA_LOCAL=1 QA_SCOREBOARD=1 QA_CAPTURE_PATH=screenshots/ping-flag-hud.png cargo run --example launch_smoke
```

This creates an ephemeral loopback match and closes only its own QA window.

## Mini-map and full map

Code: `src/src/map_view.rs`. A picture of the map's terrain from above
(`relief`) is built once per map and kept as a texture. It has four layers:
- the heightfield, hillshaded from the northwest (slopes over 4 pixels, so
  landforms rather than bumps) over a muted height tint;
- contour lines, about a dozen across the map's range with every fifth darker;
- the water volumes in blue and lava in orange;
- the structures' footprints, from their collision and outlined (scenery
  specks under about 100 square meters are left out).

- **Mini-map** (top right): a 168-pixel disc centered on you that turns with
  you (ahead is up) and sees about 420 m across. The kill feed (frags and
  play announcements, the last five for 8 s) sits at the top left under
  the speed readout, clear of it.
  - It shows you (an arrow), teammates, and enemies the HUD may show: those
    not hidden by a snapshot or a sensor-jamming effect, outlined. In
    Rabbit an enemy shows only in arrow range and in sight
    (`world_overlay::in_sight`; `docs/rabbit.md`).
  - It shows both flags (CTF), with a carried flag ringing its carrier, and
    the control points in play.
  - Flags and points out of view sit on the rim, in their direction.
- **Full map** (**M**, rebindable as "Full map"): the whole map north-up
  over the middle of the screen, with a faint 256 m grid.
  - It shows the same marks, larger, plus the points' names, who carries a
    flag ("carried by Ace") or "dropped".
  - It shows the bases' equipment: generators, turrets, inventory stations
    and sensors, gray when down.
  - It's drawn only, so play, movement and the mouse go on under it, and M
    closes it.
  - Throw mine moved from M to N for it.
  - `QA_FULLMAP=1` opens it for captures.
- Tests: `map_view::tests::the_relief_shows_water_and_structures_where_they_are`
  and `the_relief_shows_lava_in_orange`.

## Enemy arrows, carrier markers and flag announcements (source only, not deployed)

Code: `src/src/world_overlay.rs` (markers) and `src/src/flag_announce.rs`
(announcements). Client-only presentation built from snapshot fields the server
owns (player positions, teams, names, flag carriers, score); no wire change and
no compatibility marker change.

- **Names only under the crosshair** (since 2026-10-14, as in the classic).
  Nobody carries a name over their head; the armor's color tells the teams
  apart (`player_model::TEAM_PLATES`, `docs/model-pipeline.md`). The one
  player your crosshair is on is named over their head, blue for a teammate
  and red for an enemy (`world_overlay::pointed_at`): the nearest body along
  your look (from your eyes, so over the shoulder too, where the crosshair
  sits) within `NAME_RANGE` (250 m), soles to crown and a little wider than
  the shoulders, plus a sliver of slack at range, in clear sight. The name
  stays 0.4 s after the crosshair leaves, fading over the last 0.2 s
  (`NameWatch`). Never yourself, the dead or a cloaked enemy. In Football
  it's the same ray that picks who sets a pass's power, so the name is
  whom you're throwing to. Until 2026-10-14 every teammate within 150 m and
  every enemy within 80 m was named.
- **A pass coming to you** (Football): a heads-up sound as it's thrown and
  a gold light on the screen's edge on the ball's side by the way you face
  (top ahead, bottom behind, the sides left and right) while it flies
  (`football_hud::incoming_glow`; `docs/football.md` "Client").
- **Red arrow over enemies** (every mode but Football, which has none since
  2026-10-14, as in the classic mod). A downward chevron in the enemy name red, over
  every living enemy within `ENEMY_ARROW_RANGE` (250 m) that is on screen AND in
  line of sight (`World::sight_clear`). Never drawn through
  terrain or walls. It shrinks from 18 px wide close up to 11 px far out and fades
  over the last 20% of range. Teammates never get an arrow.
  Through a rifle's sight the arrow and name ranges grow by its magnification (about
  4x, or 8x at its second level; `World::zoom_power`), so the farther someone
  is, the more zoom it takes to tell who they are. Enemies a teammate has
  marked (the target marker) show at any range, through walls, and so do
  spots a teammate's marker laser touched: an orange TARGET reticle with its
  distance (`world_overlay`).
- **Flag carrier marker.** Replaces the arrow on a carrier: a larger
  chevron and pennant in the carried flag's team colour, the carrier's name only
  under your crosshair (blue for your team, red for the enemy), and "FLAG CARRIER" or "HAS YOUR FLAG" with
  the distance beyond 60 m. It shows out to `CARRIER_MARKER_RANGE` (1500 m, past
  every map's flag distance) on screen, **through terrain**: the edge-of-screen
  flag bearing already reveals every flag's position, so this adds no information.
- **Centred announcements.** Large pale text with a dark outline, a quarter of the
  way down the screen, for 3 s with a 0.6 s fade: "Budster has the enemy flag",
  "Echo has your flag", "You have the enemy flag", "… dropped …", "… captured …",
  "Your flag was returned", "The enemy flag was returned". Wording is relative to
  the viewer. They queue: up to 3 wait (the oldest waiting one drops on overflow)
  and, while others wait, the one on screen yields after 1.2 s. They come from
  diffing flag carriers, flag-at-home and score between frames, so a replayed
  snapshot announces nothing and a score reset (new round) rebaselines silently.
  The simulation no longer writes its own flag text into the top HUD line;
  VICTORY/DEFEAT still appear there.
- QA: a trailing `*` on a `QA_PLAYERS` name hands that stand-in the enemy flag
  after `QA_CARRY_AT` seconds (default 6), so the real announcement path fires
  before the 8 s capture.

## Sound engine (source only, not deployed)

Code: `src/src/sound.rs` (palette, mixer, director, tests) and `src/src/audio.rs`
(output only). This supersedes the distance note above: world sounds now have
pan, distance dulling and, for explosions, travel delay. Client-only; no wire or
compatibility change, and no gameplay change.

- **All original, all synthesized.** Every sound is generated at startup from
  deterministic noise, oscillators, resonators and FM; no samples from any
  game or library. 43 one-shot cues (many with 2–6 variants so rapid fire never
  repeats one clip) and 9 loops.
- **Deep, layered design (v2).** The first palette sounded thin and "DOS": bare
  sine sweeps and FM bells with no low end (shield hit, shield down and repair kit
  had 0% of their energy below 250 Hz), hiss-only ski and jet loops (56% and 16%
  above 6 kHz), and short dry tails under 0.5 s. Every cue is now layered: a sub
  (30–80 Hz) for weight, a saturated body, filtered pink-noise texture and a
  short transient. Oscillators are band-limited (polyBLEP saws), filters are
  zero-delay state-variable filters, envelopes rise and fall smoothly, and a
  gentle tanh saturation glues the layers. Flag and match cues are warm
  detuned-saw stings an octave lower instead of bells. The weight and envelope
  targets were calibrated by measurement against reference audio studied
  privately; every sound is synthesized in code, and nothing from any
  recording is copied, resampled or mixed into what ships. That pass pulled an over-muddy first attempt back
  up: weight comes from 150–500 Hz body, not pure sub. Spectral centroid,
  original → now: disc fire 176 → 460 Hz, near explosion 1390 → 350 Hz, jet
  3254 → 152 Hz, ski 7971 → 2715 Hz, chaingun 1386 → 992 Hz. The disc idle sits
  at ~300 Hz with a 10 Hz throb; the generator hum pulses at 12 Hz. Tests keep
  the weight (minimum share below 150 Hz per cue, below 500 Hz for the hums) and
  forbid aliasing buzz (under 1% above 12 kHz for tonal sounds).
- **Held-weapon idle hums.** The weapon in your hands hums quietly: the disc
  launcher a throbbing electric hum (around 300 Hz, pulsing 10 times a second) with a
  spinning whir inside it, the chaingun a low motor tick, the grenade launcher a
  mechanical settle. Only your own weapon; ducked to 30% while it fires; the
  mixer crossfades on a switch.
- **Master bus.** A stereo feedback-delay-network reverb (pre-delay, four damped
  lines, Householder mix) replaces the small metallic room: long and dark
  outdoors (RT60 2.4 s), shorter and brighter under a roof (0.9 s). Its input is
  high-passed at 140 Hz so the low end stays mono and tight. A soft-knee 20:1
  limiter at -2 dBFS keeps pile-ups from clipping.
- **Your own sounds.** Put WAV files (8/16/24/32-bit PCM or 32-bit float, any
  rate, mono or stereo) in a sounds folder and they replace the synthesized cue
  at startup; anything missing stays synthesized. Folder: `PEAKRUNNER_SOUND_DIR`,
  else `sounds/` beside the executable (macOS: `Contents/Resources/sounds/`),
  else `assets/sounds/` when run from `src/`. Names: `disc-fire.wav`, with
  optional variants `disc-fire-2.wav` … `-8.wav`; loops use `loop-idle-disc.wav`,
  `loop-jet.wav` and so on (the full list is `Cue::file_stem` and
  `Loop::file_stem` in `sound.rs`). Desktop only; the browser build always uses
  synthesis. Packaging doesn't copy a sounds folder yet. Shipped sounds must stay
  original: never drop in audio taken from another game.
- **One mixer on both platforms.** Native renders it through rodio in 512-frame
  blocks; the browser runs the same mixer in a WebAudio script node created on
  the first user gesture. A missing device (e.g. Linux "ALSA no device") leaves
  the game silent, never crashed.
- **Voices.** 32 one-shot voices plus 9 loops and the map ambient bed. Per-cue
  caps (chaingun and turret bullets 8, footsteps 4, shield and hull hits 4,
  explosions 6, others 3). When full, the least important, most finished voice is
  stolen; a newcomer never steals from a higher-priority voice. Priorities, from
  high to low: flag, capture and match cues; generator blast, hit marker, pain, kit,
  shield down; own weapons; explosions and plasma; bullets and impacts; footsteps and bounces.
  Playing a sound and rendering never allocate after startup.
- **Water.** Entering a water volume at speed plays `Splash` (a deep, short plop
  with spray and bubbles, under 0.5 s; louder with speed; positional for other
  players within 60 m). Footsteps more than 12% submerged become `Wade` (a low
  slosh, no clink). The eye under the surface tints the view with the volume's
  colour. Replace them with `splash.wav` / `wade.wav` like any other cue.
- **Vents.** A map's vents (`docs/map-pipeline.md`) play `Rumble` (a deep
  ground rumble swelling over 3.2 s, carrying 900 m) through each warning
  and `Eruption` (a 3.6 s blast, roar and debris rain, carrying 1400 m) as
  they erupt, both at the vent. Files: `vent-rumble.wav`, `eruption.wav`.
- **The blade** (`docs/blade.md`). Each swing starts with `BladeSwing` (a
  filtered rush over the blade's hum bending with the motion, 0.36 s; three
  variants) or, for a power swing, whirl or slam, `BladeHeavy` (0.62 s);
  where a blow lands, `BladeCut` (a sizzling crackle over a thud),
  `BladeBlock` (a bright crack and ringing zap) or `BladeClash` (louder and
  longer, crackling as it dies, also a broken guard), with sparks: orange
  for a cut, the blade's color on a guard, white for a clash. All carry
  70 m, read from every player's blade state (`effects::watch_blades`), but
  only from your own floor or someone in sight.
  Files: `blade-swing.wav`, `blade-heavy.wav`, `blade-cut.wav`,
  `blade-block.wav`, `blade-clash.wav`. In a blade zone the HUD's energy bar
  reads Stamina, the crosshair is a dot in a ring that fills orange with a
  power swing's charge, GUARD or STAGGERED shows under it, the zone's name
  and the controls sit by the bars, and the weapon line reads Blade.
- **Spatial.** Smooth squared falloff to each cue's range (footsteps 38 m, shots
  140–150 m, near explosions 180 m, far explosions 520 m, generator blast 420 m).
  Equal-power stereo pan from the camera. A one-pole low-pass makes distant sounds
  duller, and sounds behind you are slightly quieter and duller. Explosions beyond
  40 m arrive late at 340 m/s; gunfire is not delayed, so feedback stays instant.
  Explosions beyond 75 m switch to a rolling low "far" layer.
- **Room feel.** Under a roof or below the terrain surface (checked four times a
  second) adds a short room reverb, cuts speed wind to 20% and the map ambience to 35%.
- **What plays when.**
  - *Weapons:* layered disc, chaingun and grenade shots with per-shot variation;
    chaingun motor whine follows the same spin-up and coast-down as the barrels.
  - *Turrets:* their own bullet and plasma shots, spatialized.
  - *Movement:* footsteps at stride rate for you and the three nearest walkers;
    landing thumps scaled by impact; ski hiss and wind by speed; a jet loop
    whose pitch follows climb and speed.
  - *Equipment:* shield pings at the struck turret or sensor, a falling sweep
    when a shield collapses, hull clanks once it is down, a hum near a running
    generator (silent once offline), and the big generator blast.
  - *Other:* grenade bounces, the hum and Doppler of a disc passing within 26 m,
    weapon-switch clicks, the repair-kit shimmer, and new chimes for flag
    taken/dropped/returned and match start/end. The round countdown reuses
    the switch click each second (`count`) and the start chime on GO (`go`).
- **Tests.** Every cue and variant is deterministic, finite, bounded and
  click-free at 44.1 and 48 kHz. Loops wrap seamlessly. Voice caps and stealing
  are checked, as are pan, distance and filter maths, delay and far/generator
  layer choice, and the director's footsteps, landings, switches, kits, shields,
  hum and chaingun spin. A source scan fails the build if the sim or network code
  emits an event name with no cue.
- **Listen.** `cargo test -p peakrunner --lib render_audio_samples -- --ignored`
  writes WAVs, including the weapon idles and a `demo.wav` walkthrough, to
  `target/audio-samples/` (ignored by git).
- Not done: wall occlusion, HRTF, Doppler on anything but passing discs,
  third-person jet loops for other players, bullet ricochets. Mix balance still
  needs a human ear.

## Published revision: 20260919.3

- FOV stays at 76 degrees through 20 m/s. A smoothstep speed curve widens it
  to 88 degrees at 120 m/s; the camera and authoritative muzzle use the same
  curve. This is speed-based easing, not a delayed camera lens or physics change.
- Original synthesized 2.4-second capture motifs: rising victory sequence for
  the scoring team, descending warning for opponents. Native and WebAudio share
  PCM synthesis. Capture cues are global and do not fade with distance. Duplicate
  snapshots, initial joins and score resets do not trigger capture stings.
- Plasma turrets solve constant-velocity interception at 80 m/s, including
  muzzle offset and the three-second projectile lifetime. The earliest positive
  solution wins; unreachable targets fall back to direct aim. Acquisition still
  requires a visible enemy, and the predicted firing path must also be clear.
  Projectiles are not homing: a player changing velocity after firing can evade.
  Damage, splash, knockback, cooldowns, range and movement remain unchanged.
- Compatibility adds `equipment3` and `fov1` because authoritative aiming and
  muzzle behavior changed. Deploy matching clients and server together.
