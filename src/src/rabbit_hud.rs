//! Rabbit and Team Rabbit HUD: the flag (or the rabbit carrying it) marked in
//! the world and at the screen edge, a status line, and announcements from
//! state changes, worded for the viewer. In Rabbit, as in the classic,
//! hunters see where the rabbit is only during a ping (`RabbitFlag::ping`,
//! or their own for `PING_SECONDS` after they spawn), and always where the
//! flag goes home. Everything comes from snapshot state; announcements diff
//! frames, so a replayed snapshot announces nothing.
use egui::{Align2, Color32, Vec2};
use glam::Vec3;
use peakrunner_core::modes::Rules;
use peakrunner_core::sim::{Team, World};

/// The rabbit flag's gold (`player_model::RABBIT_FLAG`).
pub const FLAG: Color32 = Color32::from_rgb(245, 199, 61);
/// The flag and the rabbit stay marked this far away.
const RANGE: f32 = 1200.0;

fn team_color(t: Team) -> Color32 { crate::control_hud::team_color(Some(t.idx() as u8)) }

/// Whether Rabbit's rabbit is hidden from the viewer: it shows to hunters
/// only while a ping runs (`RabbitFlag::ping`, or `pinged`: their own after
/// spawning, `RabbitWatch::pinged`), here and on the maps (`map_view`).
pub fn rabbit_hidden(world: &World, pinged: bool) -> bool {
    let r = &world.rabbit;
    world.mode.rules() == Rules::Rabbit && r.active && r.carrier.is_some_and(|c| c != world.player_id)
        && r.ping <= 0.0 && !pinged
}

/// `pinged`: the viewer's own ping after spawning is running
/// (`RabbitWatch::pinged`).
pub fn draw(ui: &egui::Ui, world: &World, pinged: bool) {
    if !world.rabbit_mode() { return; }
    let r = &world.rabbit;
    let Some(me) = world.players.get(world.player_id) else { return };
    let rect = ui.max_rect();
    let painter = ui.painter();
    let (eye, dir, fov) = crate::drawlist::view_camera(world);
    let team_mode = world.mode.rules() == Rules::TeamRabbit;
    let carrier = r.carrier.filter(|_| r.active).and_then(|c| world.players.get(c).map(|p| (c, p)));
    let mine = carrier.is_some_and(|(c, _)| c == world.player_id);
    let ours = team_mode && carrier.is_some_and(|(_, p)| p.team == me.team);
    let hidden = rabbit_hidden(world, pinged);
    let label = |at: Vec3, text: &str, color: Color32| {
        let alpha = crate::world_overlay::fade(eye.distance(at), RANGE);
        if alpha <= 0.0 { return; }
        if let Some(p) = crate::world_overlay::project(eye, dir, fov, rect, at) {
            crate::football_hud::outlined(painter, p - Vec2::new(0.0, 16.0), Align2::CENTER_BOTTOM,
                &format!("{text} · {:.0} m", eye.distance(at)), 12.0, crate::control_hud::fade(color, alpha));
        }
    };
    // The flag, or whoever carries it, labelled in the world (not yourself),
    // by role: names show only under the crosshair (`world_overlay`).
    if r.active && !mine && !hidden {
        match carrier {
            Some((_, p)) if ours => label(p.pos + Vec3::Y * 2.6, "OUR FLAG", team_color(p.team)),
            Some((_, p)) if team_mode => label(p.pos + Vec3::Y * 2.6, "ENEMY RABBIT", team_color(p.team)),
            Some((_, p)) => label(p.pos + Vec3::Y * 2.6, "KILL THE RABBIT", FLAG),
            None => label(r.pos + Vec3::Y * 3.8, "FLAG", FLAG),
        }
    }
    // Rabbit: where the flag goes home, whenever it's away.
    if !team_mode && r.active && !mine && (carrier.is_some() || r.pos.distance(r.home) > 3.0) {
        label(r.home + Vec3::Y * 3.8, "FLAG HOME", crate::control_hud::fade(FLAG, 0.7));
    }
    if !me.alive { return; }
    // Off screen: an arrow to a loose flag or to a rabbit worth chasing.
    if r.active && !mine && !ours && !hidden {
        match carrier {
            Some((_, p)) => crate::football_hud::edge_arrow(ui, eye, dir, fov, p.pos, if team_mode { "ENEMY RABBIT" } else { "RABBIT" }, FLAG),
            None => crate::football_hud::edge_arrow(ui, eye, dir, fov, r.pos, "FLAG", FLAG),
        }
    }
    let status = if mine && !team_mode {
        let next = (peakrunner_core::sim::rabbit::HELD_POINT - r.held).max(0.0).ceil();
        Some(format!("YOU ARE THE RABBIT · NEXT POINT IN {next:.0}"))
    } else if mine {
        Some("YOU HAVE THE FLAG · YOUR TEAM'S KILLS COUNT DOUBLE".to_string())
    } else if ours {
        Some("KILL BONUS ×2".to_string())
    } else if carrier.is_some() {
        Some(if team_mode { "KILL THEIR RABBIT" } else { "KILL THE RABBIT" }.to_string())
    } else if r.active {
        Some("GET TO THE FLAG".to_string())
    } else { None };
    if let Some(text) = status {
        crate::football_hud::outlined(painter, rect.center_bottom() - Vec2::new(0.0, 150.0), Align2::CENTER_BOTTOM, &text, 17.0, FLAG);
    }
}

/// Team Rabbit's top line: each team's lives left (the score limit less what
/// the other team has taken).
pub fn lives_line(world: &World) -> Option<String> {
    if world.mode.rules() != Rules::TeamRabbit { return None; }
    let (limit, s) = (world.mode.def().score_limit, world.score);
    Some(format!("Ember {} lives  ·  Glacier {} lives", limit.saturating_sub(s[1]), limit.saturating_sub(s[0])))
}

/// Announcements from state changes, worded for the viewer, and the
/// viewer's own ping after spawning in Rabbit.
#[derive(Default)]
pub struct RabbitWatch {
    last: Option<(Option<usize>, bool, Option<usize>)>,
    /// Alive last frame, the ping's seconds left, and a ping running.
    alive: bool,
    ping: f32,
    pinging: bool,
}

impl RabbitWatch {
    /// Whether the viewer's own ping (after spawning) is running.
    pub fn pinged(&self) -> bool { self.ping > 0.0 }

    pub fn update(&mut self, world: &World, dt: f32) -> Vec<String> {
        let Some(me) = world.players.get(world.player_id).filter(|_| world.rabbit_mode()) else { *self = Self::default(); return Vec::new() };
        let team_mode = world.mode.rules() == Rules::TeamRabbit;
        let r = &world.rabbit;
        let carrier = r.carrier.filter(|_| r.active);
        let hunting = !team_mode && carrier.is_some_and(|c| c != world.player_id);
        // Spawning while someone holds Rabbit's flag shows them for a while,
        // as the classic does for each new spawn.
        self.ping = (self.ping - dt).max(0.0);
        if me.alive && !self.alive && hunting && self.last.is_some() { self.ping = peakrunner_core::sim::rabbit::PING_SECONDS; }
        if !hunting { self.ping = 0.0; }
        self.alive = me.alive;
        let pinging = hunting && r.ping > 0.0;
        // Rabbit's leader, once anyone has a point.
        let leader = if team_mode { None } else { world.ffa_leader().filter(|&i| world.players[i].frags > 0) };
        let now = (carrier, r.active, leader);
        let mut out = Vec::new();
        if let Some((was_carrier, was_active, was_leader)) = self.last {
            if carrier != was_carrier {
                match carrier {
                    Some(c) if c == world.player_id => out.push(if team_mode { "YOU HAVE THE FLAG" } else { "YOU ARE THE RABBIT" }.to_string()),
                    Some(c) if team_mode => out.push(if world.players.get(c).is_some_and(|p| p.team == me.team) {
                        "YOUR TEAM HAS THE FLAG" } else { "THE ENEMY HAS THE FLAG" }.to_string()),
                    Some(c) => out.push(format!("{} IS THE RABBIT", world.display_name(c).to_uppercase())),
                    None if was_carrier.is_some() && r.active => out.push("THE FLAG IS DOWN".to_string()),
                    None => {}
                }
            }
            if team_mode && r.active && !was_active { out.push("A FLAG HAS FALLEN".to_string()); }
            // A ping after the first (that one comes with the grab).
            if pinging && !self.pinging && carrier == was_carrier { out.push("KILL THE RABBIT".to_string()); }
            if leader != was_leader {
                match leader {
                    Some(i) if i == world.player_id => out.push("YOU HAVE THE LEAD".to_string()),
                    Some(i) => out.push(format!("{} HAS TAKEN THE LEAD", world.display_name(i).to_uppercase())),
                    None => {}
                }
            }
        }
        self.last = Some(now);
        self.pinging = pinging;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::map_catalog::SupportedMode;
    use peakrunner_core::sim::rabbit::RabbitFlag;

    fn world(mode: SupportedMode) -> World {
        let mut w = World::new();
        w.set_map(peakrunner_core::terrain::MapId::OldHoller);
        w.set_mode(mode);
        w.start_match(true);
        w
    }

    #[test]
    fn rabbit_announces_the_rabbit_the_drop_and_the_lead() {
        let mut w = world(SupportedMode::Rabbit);
        let mut watch = RabbitWatch::default();
        assert!(watch.update(&w, 0.1).is_empty(), "nothing on the first frame");
        w.rabbit.carrier = Some(w.player_id);
        assert_eq!(watch.update(&w, 0.1), vec!["YOU ARE THE RABBIT"]);
        w.rabbit.carrier = None;
        w.players[1].frags = 3;
        let said = watch.update(&w, 0.1);
        assert!(said.contains(&"THE FLAG IS DOWN".to_string()) && said.iter().any(|s| s.ends_with("HAS TAKEN THE LEAD")), "{said:?}");
        assert!(watch.update(&w, 0.1).is_empty(), "a replayed frame says nothing");
    }

    #[test]
    fn hunters_get_a_ping_when_they_spawn_and_hear_each_later_one() {
        let mut w = world(SupportedMode::Rabbit);
        let other = (0..w.players.len()).find(|&j| j != w.player_id).unwrap();
        w.rabbit.carrier = Some(other);
        let mut watch = RabbitWatch::default();
        watch.update(&w, 0.1);
        assert!(!watch.pinged(), "no ping of their own while alive all along");
        // Dead, then back: their own ping for PING_SECONDS.
        w.players[w.player_id].alive = false;
        watch.update(&w, 0.1);
        w.players[w.player_id].alive = true;
        watch.update(&w, 0.1);
        assert!(watch.pinged());
        for _ in 0..70 { watch.update(&w, 0.1); }
        assert!(!watch.pinged(), "it ends");
        // The server's ping for the same rabbit is announced.
        w.rabbit.ping = 5.0;
        assert_eq!(watch.update(&w, 0.1), vec!["KILL THE RABBIT"]);
        assert!(watch.update(&w, 0.1).is_empty());
        // The rabbit themselves gets neither.
        w.rabbit.carrier = Some(w.player_id);
        watch.update(&w, 0.1);
        assert!(!watch.pinged());
    }

    #[test]
    fn team_rabbit_shows_lives_and_announces_who_has_the_flag() {
        let mut w = world(SupportedMode::TeamRabbit);
        w.score = [10, 4];
        assert_eq!(lives_line(&w).as_deref(), Some("Ember 46 lives  ·  Glacier 40 lives"));
        let mut watch = RabbitWatch::default();
        watch.update(&w, 0.1);
        let enemy = (0..w.players.len()).find(|&j| w.players[j].team != w.players[w.player_id].team).unwrap();
        w.rabbit = RabbitFlag { active: true, carrier: Some(enemy), ..RabbitFlag::default() };
        let said = watch.update(&w, 0.1);
        assert!(said.contains(&"A FLAG HAS FALLEN".to_string()) && said.contains(&"THE ENEMY HAS THE FLAG".to_string()), "{said:?}");
        assert_eq!(lives_line(&world(SupportedMode::Ctf)), None);
    }
}
