//! Big centred flag announcements ("Budster has the enemy flag"). Derived on the
//! client by diffing flag carriers, flag positions and the score between frames,
//! so offline and online play share one path and the only inputs are
//! server-authoritative snapshot fields. A repeated or stale snapshot changes
//! nothing, so it announces nothing; a score reset rebaselines silently.
use std::collections::VecDeque;
use egui::{Align2, Color32, FontId, Pos2, Vec2};
use peakrunner_core::sim::{Team, World};

/// How long one announcement stays up, including its fade.
pub const SHOW_SECONDS: f32 = 3.0;
const FADE_SECONDS: f32 = 0.6;
/// With more waiting, the current one yields after this long.
pub const MIN_SECONDS: f32 = 1.2;
/// Pending announcements beyond this drop the oldest.
pub const QUEUE_CAP: usize = 3;
const TEXT: Color32 = Color32::from_rgb(232, 240, 238);

#[derive(Clone, PartialEq, Debug)]
struct Baseline {
    map: peakrunner_core::terrain::MapId,
    viewer: usize,
    team: Team,
    carriers: [Option<usize>; 2],
    home: [bool; 2],
    score: [u32; 2],
}

fn baseline(world: &World) -> Option<Baseline> {
    let viewer = world.players.get(world.player_id)?;
    let home = [0, 1].map(|i| {
        let f = &world.flags[i];
        f.carrier.is_none() && f.pos.distance(f.home) < 3.0
    });
    Some(Baseline { map: world.map, viewer: world.player_id, team: viewer.team,
        carriers: [world.flags[0].carrier, world.flags[1].carrier], home, score: world.score })
}

fn name(world: &World, i: usize) -> String {
    world.players.get(i).map(|p| p.name.trim()).filter(|n| !n.is_empty()).unwrap_or("Someone").to_string()
}

/// Announcements for the change from `was` to the world's current state, worded
/// for the viewer. Flag `i` belongs to team `i`; its carrier is on the other team.
fn announcements(was: &Baseline, now: &Baseline, world: &World) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..2 {
        let ours = world.flags[i].team == now.team;
        let whose = if ours { "your flag" } else { "the enemy flag" };
        match (was.carriers[i], now.carriers[i]) {
            (None, Some(c)) => out.push(if c == now.viewer { "You have the enemy flag".into() }
                else { format!("{} has {whose}", name(world, c)) }),
            (Some(c), None) => {
                let carrier_team = world.flags[i].team.other().idx();
                let captured = now.score[carrier_team] > was.score[carrier_team];
                let who = if c == now.viewer { "You".to_string() } else { name(world, c) };
                if captured {
                    out.push(format!("{who} captured {whose}"));
                } else if !now.home[i] {
                    out.push(format!("{who} dropped {whose}"));
                }
            }
            (None, None) if !was.home[i] && now.home[i] =>
                out.push(if ours { "Your flag was returned".into() } else { "The enemy flag was returned".into() }),
            _ => {}
        }
    }
    out
}

#[derive(Default)]
pub struct Announcer {
    last: Option<Baseline>,
    queue: VecDeque<String>,
    current: Option<(String, f32)>,
}

impl Announcer {
    /// Compare with the previous frame and queue anything new; advance timers.
    pub fn update(&mut self, world: &World, dt: f32) {
        let now = baseline(world);
        if let (Some(was), Some(now)) = (&self.last, &now) {
            let reset = was.map != now.map || was.viewer != now.viewer || was.team != now.team
                || now.score[0] < was.score[0] || now.score[1] < was.score[1];
            if reset {
                self.queue.clear(); self.current = None;
            } else {
                for text in announcements(was, now, world) { self.push(text); }
            }
        }
        self.last = now;
        if let Some((_, age)) = &mut self.current { *age += dt; }
        let done = self.current.as_ref().is_some_and(|(_, age)| *age >= SHOW_SECONDS
            || (*age >= MIN_SECONDS && !self.queue.is_empty()));
        if self.current.is_none() || done {
            self.current = self.queue.pop_front().map(|t| (t, 0.0));
        }
    }

    pub(crate) fn push(&mut self, text: String) {
        if self.queue.len() >= QUEUE_CAP { self.queue.pop_front(); }
        self.queue.push_back(text);
    }

    pub fn current(&self) -> Option<(&str, f32)> {
        self.current.as_ref().map(|(t, age)| {
            let alpha = if self.queue.is_empty() { ((SHOW_SECONDS - age) / FADE_SECONDS).clamp(0.0, 1.0) } else { 1.0 };
            (t.as_str(), alpha)
        })
    }

    pub fn draw(&self, ui: &egui::Ui) {
        let Some((text, alpha)) = self.current() else { return; };
        let rect = ui.max_rect();
        let size = ((rect.width() - 48.0) / (text.chars().count().max(1) as f32 * 0.55)).clamp(18.0, 34.0);
        let at = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.28);
        let font = FontId::proportional(size);
        let shade = Color32::from_black_alpha((alpha * 190.0) as u8);
        for off in [Vec2::new(-1.5, 0.0), Vec2::new(1.5, 0.0), Vec2::new(0.0, -1.5), Vec2::new(0.0, 1.5), Vec2::new(2.0, 2.0)] {
            ui.painter().text(at + off, Align2::CENTER_CENTER, text, font.clone(), shade);
        }
        let c = Color32::from_rgba_unmultiplied(TEXT.r(), TEXT.g(), TEXT.b(), (alpha * 255.0) as u8);
        ui.painter().text(at, Align2::CENTER_CENTER, text, font, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::terrain::MapId;

    /// Me (Ember), a teammate and an enemy on Old Holler, flags at home.
    fn world() -> World {
        let mut w = World::new(); w.set_map(MapId::OldHoller); w.start_rift(true);
        let base = w.players[0].clone();
        w.players.truncate(1);
        w.players[0].name = "Me".into(); w.players[0].team = Team::Ember;
        let mut ally = base.clone(); ally.name = "Budster".into(); ally.team = Team::Ember;
        let mut foe = base.clone(); foe.name = "Echo".into(); foe.team = Team::Glacier;
        w.players.push(ally); w.players.push(foe);
        w.player_id = 0;
        for f in &mut w.flags { f.carrier = None; f.pos = f.home; }
        w.score = [0, 0];
        w
    }
    const ALLY: usize = 1; const FOE: usize = 2;
    fn flag(w: &World, team: Team) -> usize { w.flags.iter().position(|f| f.team == team).unwrap() }

    fn step(a: &mut Announcer, w: &World) -> Vec<String> {
        let was = a.last.clone();
        a.update(w, 0.0);
        match (was, baseline(w)) { (Some(was), Some(now)) => announcements(&was, &now, w), _ => Vec::new() }
    }

    #[test]
    fn wording_is_relative_to_the_viewer_for_every_event() {
        let mut w = world(); let mut a = Announcer::default(); a.update(&w, 0.0);
        let theirs = flag(&w, Team::Glacier); let ours = flag(&w, Team::Ember);
        w.flags[theirs].carrier = Some(ALLY);
        assert_eq!(step(&mut a, &w), ["Budster has the enemy flag"]);
        w.flags[ours].carrier = Some(FOE);
        assert_eq!(step(&mut a, &w), ["Echo has your flag"]);
        w.flags[ours].carrier = None; w.flags[ours].pos = w.flags[ours].home + glam::Vec3::X * 40.0;
        assert_eq!(step(&mut a, &w), ["Echo dropped your flag"]);
        w.flags[ours].pos = w.flags[ours].home;
        assert_eq!(step(&mut a, &w), ["Your flag was returned"]);
        w.flags[theirs].carrier = None; w.flags[theirs].pos = w.flags[theirs].home; w.score[Team::Ember.idx()] += 1;
        assert_eq!(step(&mut a, &w), ["Budster captured the enemy flag"]);
        w.flags[theirs].carrier = Some(0);
        assert_eq!(step(&mut a, &w), ["You have the enemy flag"]);
        w.flags[theirs].carrier = None; w.flags[theirs].pos = w.flags[theirs].home + glam::Vec3::Z * 30.0;
        assert_eq!(step(&mut a, &w), ["You dropped the enemy flag"]);
        w.flags[theirs].pos = w.flags[theirs].home;
        assert_eq!(step(&mut a, &w), ["The enemy flag was returned"]);
        w.flags[ours].carrier = Some(FOE); step(&mut a, &w);
        w.flags[ours].carrier = None; w.flags[ours].pos = w.flags[ours].home; w.score[Team::Glacier.idx()] += 1;
        assert_eq!(step(&mut a, &w), ["Echo captured your flag"]);
    }

    #[test]
    fn the_other_team_reads_the_same_event_from_its_side() {
        let mut w = world(); w.player_id = FOE;
        let mut a = Announcer::default(); a.update(&w, 0.0);
        let theirs = flag(&w, Team::Glacier);
        w.flags[theirs].carrier = Some(ALLY);
        assert_eq!(step(&mut a, &w), ["Budster has your flag"]);
    }

    #[test]
    fn repeated_states_and_resets_announce_nothing() {
        let mut w = world(); let mut a = Announcer::default();
        let theirs = flag(&w, Team::Glacier);
        w.flags[theirs].carrier = Some(ALLY);
        a.update(&w, 0.0);
        assert!(a.current().is_none(), "the first frame only sets a baseline");
        for _ in 0..5 { a.update(&w, 0.1); }
        assert!(a.current().is_none(), "an unchanged state (a replayed snapshot) is silent");
        w.flags[theirs].carrier = None; w.flags[theirs].pos = w.flags[theirs].home;
        w.score = [0, 0]; a.last.as_mut().unwrap().score = [2, 1];
        a.update(&w, 0.0);
        assert!(a.current().is_none(), "a score reset (new round) rebaselines without announcing");
    }

    #[test]
    fn announcements_show_fade_and_queue() {
        let mut w = world(); let mut a = Announcer::default(); a.update(&w, 0.0);
        let theirs = flag(&w, Team::Glacier); let ours = flag(&w, Team::Ember);
        w.flags[theirs].carrier = Some(ALLY); a.update(&w, 0.0);
        assert_eq!(a.current().unwrap(), ("Budster has the enemy flag", 1.0));
        a.update(&w, 2.7);
        let (_, alpha) = a.current().unwrap();
        assert!(alpha > 0.0 && alpha < 1.0, "fades over the last {FADE_SECONDS} s");
        a.update(&w, 0.4);
        assert!(a.current().is_none(), "gone after {SHOW_SECONDS} s");
        // A burst queues: the current one yields after MIN_SECONDS.
        w.flags[ours].carrier = Some(FOE); a.update(&w, 0.0);
        w.flags[theirs].carrier = None; w.flags[theirs].pos = w.flags[theirs].home + glam::Vec3::X * 20.0; a.update(&w, 0.0);
        assert_eq!(a.current().unwrap().0, "Echo has your flag");
        a.update(&w, MIN_SECONDS);
        assert_eq!(a.current().unwrap().0, "Budster dropped the enemy flag");
        // Overflow drops the oldest pending, never the one on screen.
        let mut b = Announcer::default();
        b.current = Some(("on screen".into(), 0.0));
        for k in 0..6 { b.push(format!("m{k}")); }
        assert_eq!(b.queue, ["m3", "m4", "m5"]);
        assert_eq!(b.current().unwrap().0, "on screen");
    }

    /// Networked path: the carrier comes from the server snapshot, and a
    /// duplicate snapshot must not announce again (like the capture stings).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn snapshots_announce_once_and_replays_are_silent() {
        use peakrunner_core::sim::Match;
        let mut server = Match::new(MapId::Valley);
        server.join(1, "Viewer").unwrap(); server.join(2, "Budster").unwrap();
        server.tick = 1;
        let mut world = World::new(); let mut online = crate::online::Online::default();
        let mut a = Announcer::default();
        online.receive(&mut world, &server.snapshot(), 1); a.update(&world, 0.0);
        assert!(a.current().is_none());
        let carrier = 1;
        let enemy_flag = server.world.players[carrier].team.other().idx();
        server.world.flags[enemy_flag].carrier = Some(carrier);
        server.tick += 3;
        let state = server.snapshot();
        online.receive(&mut world, &state, 1); a.update(&world, 0.0);
        let text = a.current().unwrap().0.to_string();
        assert!(text.starts_with("Budster has "), "{text}");
        online.receive(&mut world, &state, 1); a.update(&world, 0.1);
        assert!(a.queue.is_empty(), "a replayed snapshot queues nothing");
    }
}
