# Football and body checks

Football is PeakRunner's third mode: no weapons, one ball, passes, fumbles,
tackles and touchdowns.
Its maps are **Longfield**, a floodlit stadium, and **Highgoal**, a walled
arena whose goals stand on platforms you have to jump and jet onto. Body checks (player-to-player
collision, teammates included) come with it and apply in **every** mode.

Compatibility markers: `bump1` (collisions), `ball1` (the mode and its
ball), `ball2` (classic movement, the carrier armor and the delayed Ctrl+K,
which adds a player timer to snapshots), `maps8` (Longfield's and Highgoal's
fingerprints join the five CTF maps), `ball3` (Football by the classic
rules and engine: the throw's lift, any player setting a pass's power, the
1 s self-catch, the catch pushed along the throw's heading, the ball's
bounce and friction, dropped throwers, the 88-degree look, ends swapped at
half time), then `ball4` (empty hands: bodies left down in place of
dropped throwers; catches swept along the ball's real path), then `ball5`
(a pass in the air names its receiver, `Ball::pass_to`, for the incoming
warning). `ball2` is source only.

## Body checks (`crates/core/src/bump.rs`, all modes)

- Enemies collide as upright cylinders the size of the light armor's
  collision box (0.5 m each side of the centre, 2.3 m tall); centres within 2.3 m
  vertically touch, so a drop onto someone's head connects. Hits are swept
  across each physics step, so skiers closing at 100+ m/s can't pass through
  each other between ticks (they did before: a third to half the time at
  60–80 m/s).
  Teammates block each other too but
  only push apart: no damage, no fumbles, no tackles. A player who is down
  still blocks.
- Overlapping bodies are pushed apart, never through a wall.
- Speed along the line of impact passes almost whole from one body to the other
  (restitution 0.8): a blocker stops a skier dead and is knocked away. This is
  how defenders body-block a flag carrier.
- Light damage outside Football, from each player's velocity change `dv`:
  `floor(min((dv - 8) * 0.45, 20))`. A walking bump does nothing, a running
  head-on collision about 5, a full-speed hit 20. A fatal check credits the other
  player ("Body check").
- Server and offline only; client prediction doesn't resolve hits against other
  players, so the server's result arrives in the next snapshot.
- **Football: others drawn ahead, bots hit where you saw them** (`ball6`,
  2026-10-15). Other modes draw everyone else 0.1 s behind the newest
  snapshots, which with your own body predicted ahead put them about 0.15 s
  late in Football (1.5 to 2.3 m at football speeds): you ran through
  carriers on your screen while on the server they had moved on. Football
  now draws them as the classic engine did: each coasted on from the
  newest snapshot to your own body's moment (`football::coast`: the same
  way along the ground, falling and landing in the air, where there's no
  air control), and the loose ball flies ahead the same way. A straight run
  or a flight is drawn about where it really is, on everyone's screen, so
  people meet about where they really meet and the server judges hits
  between people where they are; a turn or a jet the guess missed slides
  over within about 0.15 s of the next snapshot (`online::place_ahead`).
  Measured with six bots a side over a 100 ms round trip
  (`football_bots_are_drawn_near_where_they_really_are`), a moving bot is
  drawn 0.13 m from where the server has it at your body's moment on
  average (median 0.05 m, 95% within 0.40 m); drawn as before, 2.06 m
  (median 1.83 m, 95% within 3.78 m).
  Bots are still compensated, as nobody is there to feel it: every Football
  command claims the snapshot its drawing coasted on from and how far
  (`Command::view`, `Command::ahead`), and when a person and a bot didn't
  touch on the server, the server coasts the bot on from its `lagcomp`
  history the same way and checks the person's body against that
  (`bump::seen_hit`), drawn no later than the present and no earlier than
  100 ms before it (`lagcomp::MAX_BEHIND_TICKS`, 6 ticks): a claim further
  back is coasted on to that (an old view coasted on not at all let a
  chaser 3 m behind a carrier meet it where it had been a quarter second
  before). Honest claims are a tick or two behind; one frame's claim goes
  with each of its commands, so from 20 frames a second up they stay within
  it, and below that a frame's last commands are drawn up to 2 ticks
  further on than they were. A touch there is a hit, judged by
  the speeds drawn: the bodies swap velocities and the usual rules follow;
  neither body is moved. A pair can't hit that way again within 0.3 s of any
  hit between them. Bots claim nothing and play the present. Tests
  `bodies_coast_along_the_ground_and_fall_through_the_air`,
  `football_others_are_drawn_ahead_with_you_and_slide_over_a_missed_turn`,
  `football_ball_is_drawn_ahead_with_you`,
  `a_bot_is_tackled_where_the_tacklers_game_drew_it` (and a person isn't),
  `a_seen_hit_happens_once`, `a_person_tackles_a_bot_where_their_game_drew_it`,
  `a_bot_is_never_hit_where_it_would_be_later`,
  `a_bot_is_never_hit_where_it_was_long_before`,
  `honest_football_claims_are_within_the_servers_bound` (measured at 60,
  30, 20, 15 and 12 frames a second) and, over real QUIC with a 100 ms
  round trip,
  `a_bot_is_tackled_where_the_chasers_game_drew_it_over_a_100_ms_round_trip`
  (a bot cutting sideways at 30 m/s, the chaser running on where it was
  drawn: tackled with the claim, never without).

## Classic movement (Football only)

Football players move by Football's own rules (`football::classic_move`),
with Football's own armor. CTF and the other modes keep the approved
movement bit for bit (`sim::step_players` branches on the mode; `armor_for`
and prediction follow the mode in snapshots).

- **Gravity** 20 m/s², constant (no high-speed float).
- **Looking** reaches 88 degrees up or down (`football::LOOK_LIMIT`, the
  classic limit; other modes 87, `sim::LOOK_LIMIT`; test
  `football_looks_up_and_down_88_degrees_and_other_modes_87`).
- **Running:** on the ground your horizontal velocity is pulled toward your
  run speed at up to 40 m/s² (ground force / mass), times a traction of
  3 × the surface's uprightness (capped at 1). Run speeds: 11 m/s forward,
  10 backward and sideways, diagonals capped at 11. The **carrier** wears a
  slower armor: 8 forward, 7 back and sideways. 11 m/s from a standstill
  takes about 0.28 s. At rest under 0.75 m/s you stop dead (MINSPEED).
- **No air control.** Nothing but the jet changes your course in the air.
- **Jump:** impulse 50 / mass 9 = 5.56 m/s straight up, scaled by the
  surface's uprightness, plus the same along your keys when they lead
  downhill. You can jump for 256 ms after touching a surface at least 0.2
  upright, once per touch (the existing `coyote` timer carries this window,
  so prediction needed no new state).
- **Skiing is hopping:** holding jump re-jumps at every landing. Each landing
  costs one movement tick (32 ms) of ground friction, 1.28 m/s toward your run
  speed, so a skier on the flat slows (18 → about 10 m/s over six hops in
  3 s) and downhill hops gain speed.
- **Jet:** 400 / mass 9 = 44.4 m/s². On the ground, or within the
  jump window, it lifts straight up. In the air with a movement key held it
  splits: 80% (`JET_SIDE_FACTOR`) pushes along the keys and 20% lifts,
  less than gravity, so a forward jet accelerates you and sinks you. Against
  your motion you always get the full 80%, an air brake. (The forward
  limit, `JET_FORWARD_VELOCITY` 250, never limits it in practice.)
- **Energy:** 60, drained 4 per 32 ms tick (125/s) while the 8/s recharge
  keeps running: a full tank lasts 0.51 s. A jet step needs the energy it
  spends (`MIN_JET_ENERGY`), so holding the jet on empty sputters but never
  pushes harder than the recharge pays for. (A first version copied a
  one-tick-in-five sputter that fired at full thrust while paying for a
  third of it: holding jet, jump and a key gained speed without end, 42 m/s
  in 5 s. Test: `holding_jet_jump_and_a_key_levels_off_instead_of_speeding_up_forever`,
  which now levels off at 17–19 m/s: running speed plus one tank's boost.)
- **Impacts** faster than 25 m/s into any surface hurt 0.005 of armor per
  m/s (0.76 health per m/s against a 0.66 armor).

Measured in the sim (`football_jet_is_a_short_hard_burst_that_turns_sideways_in_the_air`):
a hop rises 0.73 m; a standing jet held on 7.5 m (burnout 0.50 s); jump and
jet 15.1 m; a jet 0.5 s after the jump 5.6 m; a running jump and jet with the
key held only 1.3 m. Other tests: `classic_run_speeds_carrier_armor_and_ground_friction`,
`classic_air_has_no_control_but_held_keys_turn_the_jet`,
`classic_skiing_is_hopping_and_each_landing_costs_a_tick_of_friction`.

`FOOTBALL_ARMOR` keeps these numbers for the bots' route planner.

## Football rules (`crates/core/src/football.rs`)

- **No weapons.** Fire passes while you carry the ball and does nothing
  otherwise.
- **Ctrl+K** kills you for a quick respawn (`kill1`); a carried ball or flag
  drops. In Football it takes 15 s (`SUICIDE_DELAY`), only once the match
  is on, and asking again doesn't
  restart it; dying cancels it. The HUD counts it down (`suicide_in` in
  snapshots). Other modes are instant. Turrets and sensors stand idle.
  Repair kits still work.
- **The ball** appears at a kickoff spot on one team's side. Touch it to pick
  it up: the ball has to reach your body (the 0.5 m body
  radius plus the ball's 0.3, from your feet to 2.3 m up), swept over each
  step so a fast pass can't skip through you (`football::touch`). Since
  2026-10-15 (`ball6`) catches reach 3% farther every way, 0.82 m across and
  0.34 m below the feet and over the top (`CATCH_GROW`, `catch_touch`): a
  little help, since you see the ball late too; tripping on a lying body
  keeps the body's own size. The
  carrier runs in a slower armor (8 m/s instead of 11);
  jumps and jets are unchanged.
- **Passing** follows the classic rules. The receiver is the player under
  your crosshair, **either team**: the first body your look's ray meets
  within 8 km, unless something solid is in the way (`ray_body`, the
  collision box). Failing that, it's the nearest player of either team
  within 0.15 rad of your facing, **left and right only**, at any height
  and distance (the classic check, but measured across the half turn,
  which the classic one wasn't). So a defender on the line sets the power,
  and looking up with anyone far down the field in your facing throws hard:
  79 m/s for someone 150 m away, 240-250 m at 20-45 degrees up. The
  receiver only sets the power (`6.454 * sqrt(distance)`, uncapped, which
  lands about that far at a level look, so a pass can cross the field).
  With nobody to aim at it's a 35 m/s throw. The ball leaves from the
  middle of your body (the collision box's center) at **`pitch + 0.5
  cos(pitch) - 0.25`** (`throw_elevation`): the classic engine lifts every
  throw `0.5 cos(pitch)`, fading looking straight up or down, and a pass
  is tilted 0.25 back down from that. That's 14.3 degrees above a level
  look, 12.6 at 20 degrees up, 5.9 at 45. It keeps none of your own
  velocity (`PASS_INHERIT` is 0). Looking straight up (88 degrees) a pass
  leaves at 74.7 and comes down about 31 m ahead, 28 m up and 3.4 s later
  (test `a_throw_aimed_straight_up_still_goes_forward`); 35 m/s throws
  land 33 m out from a level look, 57 m at 20 degrees up, 61 m at 45.
  Until 2026-10-11 steep throws were held to 65 degrees, which threw a
  straight-up look about 47 m ahead. Tests:
  `any_player_under_the_crosshair_or_in_the_yaw_slice_sets_a_pass_s_power`,
  measurements `probe_throws` (ignored). Leading a runner or lofting onto
  a ledge is your aim. An earlier automatic lead-arc (`lead_pass`) was removed. Tests: `passes_reach_the_receiver_you_aim_at_near_and_across_the_field`,
  `throws_follow_the_aim_only_no_carried_momentum`. Online clients fly a
  loose ball on between snapshots (under gravity, resting on the ground,
  at most 0.25 s past the newest; `online::advance_ball`): drawn only where
  20-a-second snapshots put it, a 35 m/s throw jumped 1.75 m at a time.
- **Skiing bleeds speed on the flat** in Football: it's hopping, and every
  landing costs a tick of friction (see Classic movement), so a runner has to
  keep boosting. CTF skiing is unchanged.
- **Catches:** a ball nobody threw, a dropped one (a fumble), or one
  moving under 1.1 m/s, is picked up; a moving pass from an enemy is
  intercepted; anything else is caught. Until 2026-10-15 a fumble was
  intercepted too, and the screen named whoever came up with it ("X
  INTERCEPTS"); now the ball remembers it was dropped (`Ball::dropped`,
  `ball6`; test `a_fumble_is_counted_and_picked_up_never_intercepted`). Catching pushes you along the
  heading the ball left with (`Ball::facing`: its thrower's facing, a
  fumble's random heading, or 0 for a new ball), whichever way it flies by
  then, with an impulse of 2 x ball speed x 6.5 / 9 on mass 9 (about 0.16 x
  ball speed) and a little lift (test
  `a_catch_pushes_along_the_throw_s_heading_not_its_flight`), as the
  classic mod's catch did (its light and carrier armors both weigh 9). A
  15 m pass gives about 4 m/s, 60 m about 8, 160 m about 13. On the ground
  friction takes it back toward your run speed in a fraction of a second;
  caught in the air it carries you, with no air control, until you land.
  The catch is checked along the ball's real path through the step, leg
  by leg between the surfaces it struck (`pickup_along`), and takes the
  speed of the leg that reached you: before 2026-10-14 (`ball4`) a ball
  that reached you and then struck the turf in the same step was caught
  at its speed after the bounce, a smaller push or none (test
  `a_ball_that_reaches_you_before_the_turf_is_caught_at_that_speed`;
  about 1 in 100 catches in bot matches). A thrown or
  dropped ball ignores whoever let it go for 1 s (`THROWER_NO_CATCH`, the
  classic engine's; test `a_thrower_cannot_catch_their_own_throw_for_a_second`).
- **The loose ball** moves by the classic engine's item physics
  (`fly_ball`): gravity and no air drag, the ball's own friction 1.0 and
  elasticity 0.2 times the surface's (`TURF`, the ground: 1.4 and 0.6, as
  grass; `BUILT`, anything built: 1.0 and 1.0). Each touch of a surface
  takes away the speed into it, rubs `friction x that speed` off the rest
  of its motion (all of it if that's more) and bounces it back with
  `elasticity x` the impact speed (`touch_surface`): 0.12 of it off the
  turf, 0.2 off a wall. Touching something with every velocity component
  under 1.28 m/s (twice gravity's pull over the classic 32 ms tick), it
  comes to rest and stays put. So it barely bounces and skids to a stop: a
  20 m drop hops 30 cm, anything under about 2.8 m doesn't hop at all, and
  a ball skidding at 10 m/s stops in about 2 m (tests
  `the_ball_barely_bounces_and_skids_to_a_stop`,
  `touching_a_surface_cancels_the_impact_rubs_off_speed_and_bounces_a_little`).
  It used to bounce back with about half its speed and roll on.
- **Bodies down** (since 2026-10-14, `ball4`): a dead player's body stays down
  where it falls for `BODY_LIES` (10 s) after the death, their player back
  in play after the usual second (`DownedBody`, in the ball's state; the
  client keeps the ragdoll lying and sinks it over the last 1.5 s,
  `AnimTracker::corpses`). A carrier touching one fumbles 30% of the time
  for each 32 ms of contact ("X trips over a body and fumbles!"), the
  classic's rate for its dropped ball-thrower, which the body stands in for
  now that nobody holds one; a player who leaves leaves nothing but a body
  they already lay as (tests
  `a_body_stays_down_a_while_and_a_carrier_over_it_fumbles`,
  `a_player_who_leaves_leaves_nothing_behind_but_a_body`). Until
  2026-10-14 a body left a ball-thrower lying 30 s, 24.5 s after the death.
- **Hits** (enemies only):
  - The two bodies swap velocities outright.
  - Speeds are rounded to whole m/s. The slower player loses; on a tie both do.
  - A losing carrier fumbles when the closing speed is 2–14 m/s. The ball is
    thrown out at 10 m/s along a random heading, from the middle of the
    body at `pitch + 0.5 cos(pitch)` (`drop_elevation`: the engine's lift
    without a pass's tilt), and keeps the carrier's velocity, which after
    the swap is the hitter's: a knocked-loose ball flies on the way the hit
    came (`drop_ball`). The fumbler can't re-catch it for 1 s. Every game
    announces FUMBLE (`Ball::fumbles` changes), not who comes up with it. A carrier who dies, respawns, switches team or
    leaves drops it the same way but straight ahead
    (`a_knocked_loose_ball_flies_on_with_the_hit_and_a_dropped_one_goes_ahead`).
    Earlier bugs: the fumbler took the ball straight back (no guard; test
    `a_fumbling_carrier_cannot_grab_the_ball_straight_back`), and then the
    hitter did, every time (a 2.2 m pickup reach against bodies a metre
    apart after the hit: 200 of 200 trials). Pickup is now touch, below.
  - At 15 m/s or more the loser is **tackled**: the ball comes loose, 30
    damage, 2 s down (no movement, jets or throws) with the camera orbiting 8 m
    out, getting up with an empty tank. Going down, the body goes limp: a
    ragdoll on each client (`src/ragdoll.rs`) pitched onto its face by the hit,
    lying as it lands, as a dead body does. The tackler scores 1
    when the loser carried the ball.
  - Nobody hits a player who is down. No tackles while no ball is in play.
  - "Knee to the face" is emergent: diving onto someone is how you arrive
    faster than them.
- **Touchdown:** carry the ball into the enemy end zone's sphere. The team scores
  1 and the scorer 5 points. Everyone regroups at spawn after 6 s; a new ball
  appears on the conceding team's side after 10 s.
- **Match:** two 15-minute halves, a 20 s warmup
  on the first start (later rounds count down the usual 10 s),
  1 s respawns. The score limit is 40 touchdowns,
  so games effectively run on time.
- **Half time** at 15:00: a 10 s break, and the teams **swap ends**:
  spawns, end zones and kickoff spots (`World::side`; clients place the end
  zones by the ball's `half`, marker `ball3`). Everyone is sent to their new
  spawns 3 s before the break ends, the next ball goes to the side opposite
  the team that last had it (its carrier's or last thrower's; a random side
  when there's no ball or nobody has touched it), and the second half gets
  its full 15 minutes after the break (the clock gains the break). Test
  `half_time_swaps_ends_and_gives_the_ball_to_the_side_that_did_not_have_it`.
  The maps' own end-zone colors don't swap.
- **Resets:** a ball that sits still for 20 s is reset at once, and one that
  leaves the field after 2 s. Either way it goes to the side opposite
  the last team to touch it, and everyone is sent back to spawn.
- **Leaving the field** means more than `OUT_MARGIN` (3 m) past the
  manifest's `bounds`, which mark the arena's inside: over a wall and gone,
  never just against it. (The ball used to reset when it rolled against
  Highgoal's walls, whose inner faces are 0.5 m outside its bounds; test
  `a_ball_against_the_wall_stays_in_play_until_it_goes_over`.) A ball
  bounces off the surface its sweep hit (`ball_hit`: the sweeps of
  `sim::surface_hit`, shared with grenades, mines and shells, with each
  surface's material). It used to re-probe
  along its flight, which missed walls struck at a glancing angle and
  bounced it off the ground instead; still moving into the wall, it went
  through on the next step (866 of 2520 test throws at Highgoal's walls;
  now none: `passes_into_highgoal_walls_stay_inside`, and the full
  `every_wall_throw_stays_inside_highgoal`, ignored).
- **Out of bounds:** off the field you lose 0.065 damage level
  (9.8% of light armor) each second, "YOU'RE OUT OF BOUNDS!", until you
  return or die. On Longfield that's the stands; on Highgoal the walls keep
  you in.
- Personal points use the frag counter (the HUD and roster say "pts").

## The field

A map offers Football only when its manifest declares a field:

```json
"football": {
  "end_zones": [[1024, 101.2, 899], [1024, 101.2, 1149]],
  "kickoff":   [[1024, 101.0, 986.5], [1024, 101.0, 1061.5]],
  "radius": 9,
  "bounds": [896, 799, 1152, 1249]
}
```

Index 0 is Ember's. End zones are the sphere centres a carrier (body centre
1.2 m above the turf) must enter. The rotation rejects `"mode":"football"` on a map
without a field, and the offline menu disables the button. Tests and QA without a
field fall back to the flag stands.

## Longfield (`src/assets/maps/longfield/`)

Built by `scripts/build-longfield.py` from `maps/longfield.json` and
`scripts/assets/longfield_{terrain,stadium,materials}.py`; tested by
`scripts/test-longfield.py`. Key `longfield`, `MapId::Longfield`.

- A dead-flat turf field 140 x 340 m with mown stripes. End zones 250 m apart,
  9 m spheres, painted team rings. Chalk sidelines, end lines, a line every 25 m,
  and a centre line and circle.
- A point-symmetric bowl: banks rise 42 m (median slope over 25°, never over
  60°), so you can ski down them to build speed for a tackle, to a level rim.
- On the rim: five-tier stands down both long sides and behind each end,
  back walls with team banners, lit canopies, two scoreboards and four
  floodlight masts. An open goal gate (pylons, crossbar, banner, lamp strip)
  stands behind each end zone; every approach from the field and the flanks is
  clear.
- 8 spawns per team behind their own end zone, facing up the field. No
  equipment, water or props. The terrain shade is baked directly.
- 1152 collision and 2290 render triangles, 5 lightmap pages, byte-identical
  rebuilds.
- Originality: the geometry, textures and name are original.

## Highgoal (`src/assets/maps/highgoal/`)

Built by `scripts/build-highgoal.py` from `maps/highgoal.json` and
`scripts/assets/highgoal_{terrain,arena,materials}.py`; tested by
`scripts/test-highgoal.py`. Key `highgoal`, `MapId::Highgoal`.

- An enclosed sand arena, 110 x 220 m, inside 32 m walls of carved
  medallion tiles, with lamp strips along the tops and team banners on the end walls.
- **Raised goals:** at each end, a glowing team-glass slab (15 x 8 m) on two
  tapered basalt pillars, its top `RAISED_GOAL_HEIGHT` = 7 m up; the end zone is
  a 4 m sphere on it. With no air control you get up from a run: jump 9–18 m
  out, let go of the keys (held keys turn the jet sideways) and feather the
  jet so your arc just clears the edge; your run carries you onto it
  (`a_running_leap_with_a_feathered_jet_lands_on_the_raised_goal`). Jet too
  long and you sail over; jump at the edge and you meet it. A backward jet
  brakes you over it.
  You can walk under the slab between the pillars; defenders can stand on it.
- 8 spawns per team between their goal and end wall. No equipment, water or
  props. 152 collision and 726 render triangles, 2 lightmap pages,
  byte-identical rebuilds.
- Bot carriers stop jetting 60 m out and walk in, so the tank refills on the
  way; with 45 energy they run straight at the goal from the field side (else
  they wait for it at a run-up spot 16 m out), jump 9.5–12.5 m out and, keys
  released, jet only while their arc would cross the edge too low
  (`bots_leap_onto_raised_goals_and_score_on_highgoal`: 9–14 touchdowns in
  10 minutes; when they waited at the spot for a full tank after jetting in
  empty, defenders tackled them there and games ended 0–2).
- Originality: the geometry, textures and name are original.

## Client

- `src/football_hud.rs`: no labels over the end zones (SCORE HERE and YOUR
  END ZONE removed 2026-10-14; the stadiums' paint and
  posts mark them; rings drawn only on maps without painted ones), screen-edge
  arrows (to the ball or its carrier when off screen; none while you carry
  it, since the arrow to their end zone went with the labels), and a status line (YOU HAVE THE BALL · FIRE TO PASS, TACKLED — GETTING UP,
  NEXT BALL IN n). In view, the ball and its carrier carry no label: the
  loose ball's light and the carrier's flag and light mark them (labels
  removed 2026-10-13).
- **Players** carry nothing over their heads, as in the classic mod
  (2026-10-14): no arrows over enemies and no names, except the one player
  under your crosshair, named in blue (teammate) or red (enemy). That's the
  player your pass would go to. Teams are told apart by their armor, which
  the engine paints in the team's color (`docs/art-direction.md`).
- **A pass coming to you** (2026-10-14): a ball thrown at you shouldn't come
  unannounced, so it sounds a cue and lights the screen's edge on the side
  it comes from, like a damage edge (top for in front, bottom for behind,
  left and right as lit). The server names who a
  pass goes to, the player whose distance set its power, as the feed's
  "passes toward" does (`Ball::pass_to`, in snapshots, `ball5`), until the
  ball strikes a surface, rests or is held. That player's game sounds a
  heads-up as the ball leaves the hand (`incoming`: one flag flap, soft,
  deep and cloth-like: a breath of air, a crisp crack, the cloth's whump,
  then the cloth rippling out unevenly with its surface rustling and its
  edges ticking, slowing as it settles over about half a second;
  synthesized in code, test `the_pass_warning_is_one_cloth_flap`) and,
  while it flies,
  lights the screen's edge
  in the ball's gold on its side by the way you face: the top for ahead,
  the bottom for behind, the sides for left and right, a corner between,
  brighter as it nears and pulsing gently (`football_hud::incoming_glow`,
  `level_bearing`, `edge_point`; light added to the picture, a wide wash
  and a bright rim). Either team's player can be the one: whoever is under
  the crosshair sets the power. Tests:
  `a_pass_names_who_it_goes_to_until_it_lands_or_is_caught`,
  `a_pass_thrown_to_you_on_the_server_sounds_incoming`,
  `a_pass_lights_the_edge_on_its_side`; `QA_BALL=x,y,z,pass` stages one.
- Announcements come from diffing state (TOUCHDOWN EMBER, HALF TIME, FUMBLE,
  YOU HAVE THE BALL, X INTERCEPTS for a moving pass only, TACKLED), so a
  replayed snapshot announces nothing (tests
  `a_fumble_is_announced_and_its_recovery_is_no_interception`,
  `a_fumble_on_the_server_is_announced_and_its_recovery_is_no_interception`). Plays
  are server feed entries (`feed::Entry::Play`), shown in gold as PLAY.
- **The ball** is a small dark case with pale bands, a grille and a lamp. It
  stays upright, never spinning or tumbling, in flight and at rest. A loose
  ball sits in a pool of white light; nobody sees a carried ball.
- **The glowing carrier:** the carrier wears their own team's flag, taller and
  larger than a CTF flag, on a pole over their head, and it lights the ground
  round them, pulsing every 1.5 s. `drawlist::ball_lights` fills two point-light slots
  in `DrawFrame::lights`; `map.wgsl` adds them to terrain, structures and props
  (radius 5 m, an even pool with a soft rim). Players aren't lit by it.
- **Empty hands** (since 2026-10-14): the ball is carried, not a gun.
  Nobody holds anything: the
  arms swing free (`Hold::Free`), and a pass reaches forward and up over
  its cooldown (`PoseInput::throw`, `PASS_COOLDOWN`). The carrier
  (`Hold::Carrier`) tucks the ball against the ribs in the right hand, the
  elbow down by the hip, the left arm free, and wears their team's flag;
  in first person the ball rests on your right forearm, low on the right,
  and without it nothing is in view. The classic gave everyone a "Football
  Thrower" (its energy gun's shape), as PeakRunner did until then with an
  authored one (`assets/models/equipment/thrower`, built by
  `scripts/models/thrower_models.py`, kept but no longer drawn).
- **Sound** (original synthesis, `src/sound.rs`), sparse on purpose: a
  referee whistle for a new ball, a deep horn at half time, the capture sting
  for a touchdown (win or loss), the flag-pickup chime when you catch or pick
  up the ball, the return chime when your team intercepts, a plain catch for
  anyone else's nearby, a grunt when your ball is knocked loose (a thump
  where anyone else's is; online a fumble used to whoosh like a pass), a tackle
  thud, a pass whoosh, a heads-up when a pass is thrown to you, and a
  two-tone warning each second you're out of bounds. No crowd, and no held-weapon hum (there are no weapons). Resets are
  silent until the next ball's whistle. A body check thumps only for a real
  hit (3 m/s or more), at most every 0.3 s for you
  (`leaning_is_quiet_and_a_hit_thumps_once`). Online clients derive the same
  cues from snapshots (`football_hud::network_events`).

## Bots

Football bots play the game (`crates/core/src/football_bots.rs`), offline
and on servers (`PEAKRUNNER_BOTS`; Springdale Football Bots, UDP 7781, fills
6 a side, and Springdale Football on 7778 is people only, up to 6 a side;
both were 8 a side until 2026-10-14, when they went to 6 v 6: the
classic mod's sample server held 4 a side). They
go by football names (`FOOTBALL_NAMES`, one each, none twice in a match) in
Football and by their personalities' names in other modes; a rotation into
or out of Football renames them.

At each think a bot reads the play and takes a job. A pass, a fumble or a
catch makes every bot think again at once.

A carrier reads the defenders as a person would (`fb_watch`, `fb_seen`;
since 2026-10-15). It acts on what it saw a quarter second ago (15 steps), and
sees only the enemies within 65 degrees either side of the way it was facing
then. One who goes out of view it pictures where they'd be at the speed it
last saw, for 2.5 s: it can't tell if they sped up, slowed or turned. Before,
it read every enemy's exact place and speed, behind it too, and passed at the
last instant before a hit from anywhere. Its passing, its line and its bursts
(below) go by that picture; the rest of the play still reads the field
exactly. Test `a_carrier_sees_defenders_late_in_front_and_remembers_them_behind`.

- **Carry:** run for the end zone, turning up to 75 degrees either side
  for the line the enemies it knows of take longest to cut off, and keeping to it unless
  another is clearly better. With nobody within 25 m ahead the carrier
  sprints (hops with the jet, a burst's worth kept back); a hit coming from
  ahead, or anyone closing in just after a pickup, gets a burst (jump, jet
  along the run), so it's the faster body when they meet. On Highgoal the
  run goes to the run-up spot and the leap below takes over.
- **Pass:** the carrier tries throws at each teammate (three lead times,
  eight look pitches), each exactly as `pass` would make it
  (`World::pass_throw`: the crosshair or yaw-slice player sets the power),
  and plays every ball's flight against every player: a pass must reach a
  teammate in bounds, no more than 3 m over their reach, 0.45 s before any
  enemy can. It passes when pressed (an enemy it knows of reaching it within 0.45 s,
  after a quarter second with the ball) to any such throw that loses little
  ground, or after 1.2 s with the ball to one that gains plenty. Its
  personality's aim wobble is applied and the throw checked again.
- **Loose ball** (a pass, a fumble, a new ball): each team's first to it
  goes for it; the second goes too when close behind, or else to hit the
  enemy who'll get there first as it arrives. The rest play as if the team
  that gets there first already had it.
- **Catch:** a bot works out where and when the ball first comes within its
  body's reach (0.34 m under the feet to 2.64 m, 0.82 m across; it plans a
  little inside that), standing, on the run with a
  hop, or with a jump and the jet, and prefers an easy catch up to 0.5 s
  later than a jet jump. With no air control it settles under a high ball
  and jumps straight up when the climb takes as long as the ball has left,
  holding the jet only while the body would pass under it
  (`a_bot_jumps_and_jets_to_catch_a_ball_over_its_head`: 1, 3.2, 4.5 and
  7 m over the feet). Players already in the air are planned along their
  arcs (they can't steer there). Caught in the air, the catch's push
  carries the bot to the landing as it would a person (`Brain::ride`):
  its run's jet adds to the push while its way lies within 60 degrees of
  it, and otherwise it rides the push out with the jet off and turns after
  it lands. Before 2026-10-16 it jetted toward its way at once, which took
  a push thrown across or against its way off within a fifth of a second
  (`a_bot_that_catches_in_the_air_rides_the_push_to_the_landing`,
  `a_bot_that_catches_a_pass_its_way_in_the_air_jets_on_with_the_push`).
- **Offense around the carrier:** the teammates nearest the ball take on the
  enemies within 35 m (up to two, keeping a receiver): a blocker stands on
  the line from the carrier to its man and, between them or close, charges
  him. The others get open: spots 10-46 m up the field and 28 m either
  side, scored for ground gained, distance from enemies, a clear throwing
  lane, a short run, room from teammates' spots and the end zone.
- **Defense:** the two quickest to the carrier rush on an intercept course
  (one in a side of two; none at a loose ball, where the second to it goes
  already); with three or more left one plays deep, 25-50 m out from home;
  the rest mark the receivers nearest home, 3 m toward the ball and 2 m
  toward home from their man (spares hold a zone between the ball and home).
- **Charge:** within 3.5 m to 5-8 m (by aggression) of the man it's after,
  on the ground with 35 energy, a bot jumps and jets along the keys, turned
  each step toward where he'll be: the push makes it the faster body by
  far, so the hit takes him down even from behind.
- Bots raise the repair tool only while no ball is in play.

Tests: `bots_play_football_eight_a_side` (four minutes on Longfield: passes
caught, some in the air, interceptions, carriers tackled and knocked loose,
blockers' tackles, touchdowns), `football_bots_go_by_football_names_and_back`,
`bots_carry_pass_tackle_and_score_on_longfield`,
`bots_leap_onto_raised_goals_and_score_on_highgoal`. Reports (ignored;
`--release ... -- --ignored --nocapture`): `football_bot_report` (a match's
tally by play and by the jobs in each hit; `PEAKRUNNER_FB_MAP`,
`PEAKRUNNER_FB_BOTS`, `PEAKRUNNER_FB_MINUTES`), `football_pass_trace` (each
pass: who was expected to take it and when, and what happened) and
`football_possession_trace` (each carry: how long, how far, how it ended).

Measured on 2026-10-13, 8 a side for 10 minutes. Before, Longfield: 57
passes in 5 minutes, 22 caught and 20 intercepted, 3 carriers tackled, 3
touchdowns. After:

| Map | Passes | Caught (in the air) | Intercepted | Carriers tackled / knocked loose | Touchdowns |
|---|---|---|---|---|---|
| Longfield | 120 | 73 (34) | 35 | 15 / 36 | 9 |
| Highgoal | 114 | 79 (24) | 57 | 31 / 115 | 4 |

Those interceptions count fumbles recovered by the other side too; since
2026-10-15 (`ball6`) a fumble is only picked up. Highgoal's
arena is small and crowded: loose balls are fought over.

Measured on 2026-10-15, 6 a side for 10 minutes, before and after carriers
read the defenders late and only in front (both with the 3% catch reach):

| Map | Passes | Caught (in the air) | Intercepted | Carriers tackled / knocked loose | Touchdowns |
|---|---|---|---|---|---|
| Longfield, before | 124 | 77 (31) | 21 | 12 / 39 | 8 |
| Longfield, after | 88 | 46 (12) | 11 | 22 / 106 | 7 |
| Highgoal, before | 154 | 88 (33) | 28 | 28 / 148 | 3 |
| Highgoal, after | 79 | 42 (12) | 9 | 32 / 204 | 1 |

Carriers now lose the ball far more often to hits they didn't see coming,
mostly rushers from behind or the side, and pass about half as often. The
reaction time and the blind side each did about half of that (measured one
at a time).

## QA

- Local stadium match: `QA_LOCAL=1 QA_MAP=longfield QA_MODE=football` (or
  `QA_MAP=highgoal`, and `QA_BOTS=6` for bots), plus
  `QA_CAPTURE_PATH` and optional `QA_FLYCAM`. `QA_PLAYERS="Runner*:0:x,y,z:run"`
  stages a carrier (the `*`), `QA_BALL=x,y,z` a loose ball.
- `QA_MODE=football` on an offline start plays Football on any map, with end
  zones at the flag stands.

## Open items

- Human playtest of the classic movement: hop-skiing, the split jet, no air
  control, getting onto Highgoal's platforms, the carrier armor, tackle
  speeds, pass feel and score target.
- Not built: a hot-potato mode, a match mode, and players lit by the
  ball's light.
- More stadiums.
- Field lines in the stands' evening shadow read dim.
