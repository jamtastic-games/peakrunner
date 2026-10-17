//! Football HUD: end-zone rings (off the stadiums only), an arrow at the
//! screen edge to the ball (or its carrier) when it's off screen, a glow on
//! the screen's edge toward a pass coming to you, a status line, and play
//! announcements. Everything
//! comes from snapshot state; announcements diff frames, so a replayed
//! snapshot announces nothing.
use egui::{Align2, Color32, FontId, Stroke, Vec2};
use glam::Vec3;
use peakrunner_core::sim::football::Ball;
use peakrunner_core::sim::{Player, Team, World};

const BALL: Color32 = Color32::from_rgb(255, 214, 120);

fn team_color(t: Team) -> Color32 { crate::control_hud::team_color(Some(t.idx() as u8)) }
fn team_name(t: Team) -> &'static str { if t == Team::Ember { "EMBER" } else { "GLACIER" } }

pub(crate) fn outlined(painter: &egui::Painter, at: egui::Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    painter.text(at + Vec2::splat(1.0), align, text, FontId::proportional(size), Color32::from_black_alpha(color.a()));
    painter.text(at, align, text, FontId::proportional(size), color);
}

/// Screen-edge arrow toward `target` when it is off screen.
pub(crate) fn edge_arrow(ui: &egui::Ui, eye: Vec3, dir: Vec3, fov: f32, target: Vec3, label: &str, color: Color32) {
    let rect = ui.max_rect();
    let inset = rect.shrink2(Vec2::new(82.0, 100.0));
    if inset.width() <= 0.0 || inset.height() <= 0.0 { return; }
    let Some(direction) = crate::flag_hud::bearing(target - eye, dir, fov, rect.aspect_ratio()) else { return };
    let half = inset.size() * 0.5;
    let distance = (half.x / direction.x.abs().max(0.0001)).min(half.y / direction.y.abs().max(0.0001));
    let tip = inset.center() + direction * distance;
    let side = Vec2::new(-direction.y, direction.x);
    ui.painter().add(egui::Shape::convex_polygon(vec![tip, tip - direction * 17.0 + side * 7.0, tip - direction * 17.0 - side * 7.0],
        color, Stroke::new(1.5, Color32::BLACK)));
    outlined(ui.painter(), tip - direction * 40.0, Align2::CENTER_CENTER,
        &format!("{label}\n{:.0} m", eye.distance(target)), 12.0, color);
}

pub fn draw(ui: &egui::Ui, world: &World) {
    let ball = &world.ball;
    if !ball.active { return; }
    let rect = ui.max_rect();
    let painter = ui.painter();
    let (eye, dir, fov) = crate::drawlist::view_camera(world);
    let Some(me) = world.players.get(world.player_id) else { return };
    let radius = world.end_zone_radius();
    // End zones carry no label (removed 2026-10-14: the field's paint and
    // the goal posts mark them). Stadiums paint theirs on the field;
    // elsewhere (QA) a ring is drawn, your own only up close.
    if !peakrunner_core::sim::football::has_field(world.map) {
        for team in [Team::Ember, Team::Glacier] {
            let zone = world.end_zone(team);
            let distance = eye.distance(zone);
            let alpha = crate::world_overlay::fade(distance, 1200.0);
            if alpha <= 0.0 || (team == me.team && distance > 80.0) { continue; }
            let color = crate::control_hud::fade(team_color(team), alpha);
            for (r, w) in [(radius, 3.0), (radius * 0.7, 1.5)] {
                crate::control_hud::ring(painter, eye, dir, fov, rect, zone - Vec3::Y * 1.0, r, 0.0, 1.0, Stroke::new(w, color));
            }
        }
    }
    // The ball, loose or carried, has no label in view: its light and the
    // carrier's flag mark it. Off screen an arrow at the edge points to it,
    // unless you carry it (no arrow to their end zone since 2026-10-14).
    let carrier = ball.carrier.and_then(|c| world.players.get(c).map(|p| (c, p)));
    if !me.alive { return; }
    incoming_glow(ui, world, eye, dir);
    if ball.in_play && !ball.carried_by(world.player_id) {
        let target = carrier.map_or(ball.pos, |(_, p)| p.pos);
        edge_arrow(ui, eye, dir, fov, target, "BALL", BALL);
    }
    let bottom = rect.center_bottom() - Vec2::new(0.0, 150.0);
    let status = if me.stun > 0.0 { Some(("TACKLED — GETTING UP".to_string(), Color32::from_rgb(255, 120, 90))) }
        // Ctrl+K takes 15 s in Football.
        else if me.suicide_in > 0.0 { Some((format!("CTRL+K · RESPAWN IN {:.0}", me.suicide_in.ceil()), Color32::from_rgb(255, 90, 80))) }
        else if ball.carried_by(world.player_id) { Some(("YOU HAVE THE BALL · FIRE TO PASS".to_string(), BALL)) }
        else if !ball.in_play && ball.respawn > 0.0 {
            Some((format!("NEXT BALL IN {:.0}", ball.respawn.ceil()), Color32::from_rgb(220, 226, 236)))
        } else { None };
    if let Some((text, color)) = status { outlined(painter, bottom, Align2::CENTER_BOTTOM, &text, 17.0, color); }
}

/// A pass on its way to you (`Ball::pass_to`, until it strikes something,
/// rests or is held) lights the screen's edge on its side, by the way you
/// face: the top for ahead, the bottom for behind, the sides for left and
/// right, brighter as it nears. The "incoming" cue sounds as it leaves the
/// thrower's hand.
fn incoming_glow(ui: &egui::Ui, world: &World, eye: Vec3, dir: Vec3) {
    let ball = &world.ball;
    if world.spectating || !ball.loose() || ball.pass_to != Some(world.player_id) { return; }
    let Some(theta) = level_bearing(eye, dir, ball.pos) else { return };
    let near = (1.0 - eye.distance(ball.pos) / 120.0).clamp(0.0, 1.0);
    let pulse = 0.85 + 0.15 * (ui.input(|i| i.time) as f32 * std::f32::consts::TAU * 2.0).sin();
    edge_glow(ui.painter(), ui.max_rect(), theta, (0.7 + 0.3 * near) * pulse, BALL);
}

/// `at`'s bearing from the way the camera faces, level: 0 ahead, positive
/// to the right, ±π behind. None straight overhead or underfoot.
pub(crate) fn level_bearing(eye: Vec3, dir: Vec3, at: Vec3) -> Option<f32> {
    let ahead = Vec3::new(dir.x, 0.0, dir.z).try_normalize()?;
    let right = Vec3::new(-ahead.z, 0.0, ahead.x);
    let to = at - eye;
    let (a, r) = (to.dot(ahead), to.dot(right));
    (a.hypot(r) >= 0.5).then(|| r.atan2(a))
}

/// Where on `rect`'s edge a bearing lands: 0 the top's middle, clockwise
/// (π/2 the right's middle, ±π the bottom's), the corners at 45 degrees.
pub(crate) fn edge_point(rect: egui::Rect, theta: f32) -> egui::Pos2 {
    let (sx, sy) = (theta.sin(), -theta.cos());
    let m = sx.abs().max(sy.abs());
    rect.center() + Vec2::new(sx / m * rect.width() * 0.5, sy / m * rect.height() * 0.5)
}

/// Light on `rect`'s edge from bearing `theta` (`edge_point`), added to the
/// picture: a wide soft wash and a bright rim, each fading inward and
/// along the edge.
fn edge_glow(painter: &egui::Painter, rect: egui::Rect, theta: f32, strength: f32, color: Color32) {
    use std::f32::consts::{PI, TAU};
    const SAMPLES: usize = 120;
    let center = rect.center();
    for (spread, depth, alpha) in [(0.85_f32, 0.3_f32, 0.55_f32), (0.42, 0.1, 1.0)] {
        let mut mesh = egui::Mesh::default();
        for k in 0..=SAMPLES {
            let phi = k as f32 / SAMPLES as f32 * TAU - PI;
            let edge = edge_point(rect, phi);
            let off = (phi - theta + PI).rem_euclid(TAU) - PI;
            let a = (strength * alpha * (-(off / spread).powi(2)).exp()).clamp(0.0, 1.0);
            // Premultiplied with no alpha: added, like light.
            let lit = Color32::from_rgba_premultiplied((color.r() as f32 * a) as u8, (color.g() as f32 * a) as u8, (color.b() as f32 * a) as u8, 0);
            mesh.colored_vertex(edge, lit);
            mesh.colored_vertex(edge + (center - edge) * depth, Color32::TRANSPARENT);
        }
        for k in 0..SAMPLES as u32 {
            let i = k * 2;
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Announcements from state changes, worded for the viewer.
#[derive(Default)]
pub struct FootballWatch { last: Option<(Ball, [u32; 2], f32)> }

impl FootballWatch {
    pub fn update(&mut self, world: &World) -> Vec<String> {
        let Some(me) = world.players.get(world.player_id) else { self.last = None; return Vec::new() };
        let now = (world.ball.clone(), world.score, me.stun);
        let mut out = Vec::new();
        if let Some((was, score, stun)) = &self.last {
            if was.active && now.0.active {
                for t in [Team::Ember, Team::Glacier] {
                    if now.1[t.idx()] > score[t.idx()] { out.push(format!("TOUCHDOWN {}", team_name(t))); }
                }
                if now.0.half > was.half { out.push("HALF TIME".into()); }
                // A fumble says so, not who comes up with it.
                if now.0.fumbles != was.fumbles { out.push("FUMBLE".into()); }
                if now.0.carrier != was.carrier {
                    if now.0.carrier == Some(world.player_id) { out.push("YOU HAVE THE BALL".into()); }
                    else if let Some(c) = now.0.carrier.filter(|&c| intercepted(was, c, &world.players)) {
                        let name = world.display_name(c);
                        out.push(format!("{name} INTERCEPTS"));
                    }
                }
                if now.2 > 0.0 && *stun <= 0.0 { out.push("TACKLED".into()); }
            }
        }
        self.last = Some(now);
        out
    }
}

/// Whether `c` taking the ball as it was (`was`) is an interception, as the
/// feed words it (`pickup_along`): a pass still moving (by the last
/// snapshot's speed) from the other team. A dropped ball, a fumble's
/// included, is only picked up.
fn intercepted(was: &Ball, c: usize, players: &[Player]) -> bool {
    let team = |i: usize| players.get(i).map(|p| p.team);
    !was.dropped && was.vel.length() >= 1.1
        && was.thrown_by.is_some_and(|k| team(k).is_some() && team(c).is_some() && team(k) != team(c))
}

/// Sounds for an online client, from one snapshot to the next (offline play
/// raises the same names directly from the sim).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn network_events(old: &Ball, new: &Ball, old_players: &[Player], players: &[Player], me: usize,
                      listener: Vec3) -> (Vec<&'static str>, Vec<(&'static str, Vec3)>) {
    let (mut events, mut spatial) = (Vec::new(), Vec::new());
    if !new.active || !old.active { return (events, spatial); }
    if new.in_play && !old.in_play { events.push("whistle"); }
    if new.half > old.half { events.push("halftime"); }
    if new.carrier != old.carrier {
        if let Some(c) = new.carrier {
            // Your own catch chimes like a flag pickup, your team's
            // interception like a return; others' nearby catches are plain.
            let team = |i: usize| players.get(i).map(|p| p.team);
            let intercept = intercepted(old, c, players);
            if c == me { events.push("flag"); }
            else if intercept && team(me) == team(c) { events.push("return"); }
            else if players.get(c).is_some_and(|p| p.pos.distance(listener) < 80.0) { events.push("catch"); }
        }
    }
    // A ball knocked loose: a grunt if it was yours, a thump where it
    // happened if not, as offline (a tackle's own thud covers a tackled
    // carrier). A fumble is never a pass's whoosh.
    if new.fumbles != old.fumbles {
        let tackled = old.carrier.and_then(|k| players.get(k)).is_some_and(|p| p.stun > 0.0);
        if !tackled {
            if old.carrier == Some(me) { events.push("pain"); } else { spatial.push(("fumble", new.pos)); }
        }
    }
    if new.thrown_by.is_some() && old.thrown_by != new.thrown_by && old.carrier.is_some() && !new.dropped {
        if old.carrier == Some(me) { events.push("pass"); } else { spatial.push(("pass", new.pos)); }
    }
    // A pass on its way to you.
    if new.pass_to == Some(me) && old.pass_to != Some(me) && new.carrier.is_none() { events.push("incoming"); }
    for (i, p) in players.iter().enumerate() {
        let Some(o) = old_players.get(i).filter(|o| o.net_id == p.net_id && p.net_id != 0) else { continue };
        if p.stun > 0.0 && o.stun <= 0.0 {
            if i == me { events.push("tackle"); } else { spatial.push(("tackle", p.pos)); }
        }
    }
    (events, spatial)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pass's edge glow by the way you face: the top for ahead, the
    /// bottom for behind, the sides for left and right.
    #[test]
    fn a_pass_lights_the_edge_on_its_side() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1280.0, 800.0));
        let eye = Vec3::new(100.0, 10.0, 100.0);
        // Facing -Z (yaw 0), looking a little down.
        let dir = Vec3::new(0.0, -0.3, -1.0).normalize();
        let at = |offset: Vec3| edge_point(rect, level_bearing(eye, dir, eye + offset).unwrap());
        let close = |a: egui::Pos2, b: egui::Pos2| a.distance(b) < 1.0;
        assert!(close(at(Vec3::new(0.0, 20.0, -40.0)), egui::pos2(640.0, 0.0)), "ahead, high: the top");
        assert!(close(at(Vec3::new(0.0, -5.0, 40.0)), egui::pos2(640.0, 800.0)), "behind: the bottom");
        assert!(close(at(Vec3::new(30.0, 0.0, 0.0)), egui::pos2(1280.0, 400.0)), "right");
        assert!(close(at(Vec3::new(-30.0, 0.0, 0.0)), egui::pos2(0.0, 400.0)), "left");
        assert!(close(at(Vec3::new(30.0, 0.0, -30.0)), egui::pos2(1280.0, 0.0)), "ahead and right: that corner");
        assert!(level_bearing(eye, dir, eye + Vec3::Y * 30.0).is_none(), "straight overhead has no side");
    }

    /// Online, your game hears "incoming" when a pass to you leaves the
    /// thrower's hand, not for others' passes.
    #[test]
    fn a_pass_to_you_sounds_incoming_online() {
        let players: Vec<Player> = Vec::new();
        let held = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        let thrown = |to| Ball { active: true, in_play: true, thrown_by: Some(0), pass_to: Some(to), vel: Vec3::X, ..Ball::default() };
        let heard = |old: &Ball, new: &Ball, me| network_events(old, new, &players, &players, me, Vec3::ZERO).0.contains(&"incoming");
        assert!(heard(&held, &thrown(1), 1));
        assert!(!heard(&held, &thrown(1), 2), "someone else's");
        assert!(!heard(&thrown(1), &thrown(1), 1), "once, as it leaves");
    }

    /// Two players a side apart: 0 and 2 Ember, 1 Glacier.
    fn three() -> World {
        let mut world = World::new();
        world.set_map(peakrunner_core::terrain::MapId::Longfield);
        world.set_mode(peakrunner_core::map_catalog::SupportedMode::Football);
        world.start_match(true);
        world.players.truncate(3);
        for (p, team) in world.players.iter_mut().zip([Team::Ember, Team::Glacier, Team::Ember]) { p.team = team; p.alive = true; }
        world.player_id = 2;
        world
    }

    /// The banners as the ball goes `from` one state `to` the next.
    fn banners(world: &mut World, from: Ball, to: Ball) -> Vec<String> {
        let mut watch = FootballWatch::default();
        world.ball = from;
        watch.update(world);
        world.ball = to;
        watch.update(world)
    }

    /// A fumble says FUMBLE, not who came up with it; only a pass taken by
    /// the other side on its way says INTERCEPTS.
    #[test]
    fn a_fumble_is_announced_and_its_recovery_is_no_interception() {
        let mut world = three();
        let ball = Ball { active: true, in_play: true, ..Ball::default() };
        let held = |c| Ball { carrier: Some(c), ..ball.clone() };
        let fumbled = Ball { thrown_by: Some(0), dropped: true, fumbles: 1, vel: Vec3::X * 8.0, ..ball.clone() };
        assert_eq!(banners(&mut world, held(0), fumbled.clone()), ["FUMBLE"]);
        assert!(banners(&mut world, fumbled.clone(), Ball { fumbles: 1, ..held(1) }).is_empty(), "the other side picks it up");
        assert_eq!(banners(&mut world, held(0), Ball { fumbles: 1, ..held(1) }), ["FUMBLE"], "knocked loose and taken between snapshots");
        let pass = Ball { thrown_by: Some(0), vel: Vec3::X * 8.0, ..ball.clone() };
        assert_eq!(banners(&mut world, pass.clone(), held(1)).len(), 1, "a pass taken by the other side");
        assert!(banners(&mut world, pass.clone(), held(1))[0].ends_with("INTERCEPTS"));
        let landed = Ball { vel: Vec3::ZERO, ..pass };
        assert!(banners(&mut world, landed, held(1)).is_empty(), "a pass at rest is only picked up");
    }

    /// Online, a fumble grunts for its carrier and thumps for everyone
    /// else; it never whooshes like a pass.
    #[test]
    fn a_fumble_online_sounds_like_a_hit_not_a_pass() {
        let world = three();
        let held = Ball { active: true, in_play: true, carrier: Some(2), ..Ball::default() };
        let fumbled = Ball { carrier: None, thrown_by: Some(2), dropped: true, fumbles: 1, ..held.clone() };
        let (mine, _) = network_events(&held, &fumbled, &world.players, &world.players, 2, Vec3::ZERO);
        assert!(mine.contains(&"pain") && !mine.contains(&"pass"), "{mine:?}");
        let (theirs, near) = network_events(&held, &fumbled, &world.players, &world.players, 0, Vec3::ZERO);
        assert!(!theirs.contains(&"pass") && near.iter().any(|(s, _)| *s == "fumble") && !near.iter().any(|(s, _)| *s == "pass"), "{theirs:?} {near:?}");
    }
}

