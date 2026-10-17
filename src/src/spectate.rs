//! Spectator camera (client only; `teams1`). A spectator has no body: the
//! view is a free-flying camera, or follows one player, over the shoulder
//! or orbiting them. Nothing here reaches the server, which only keeps the
//! spectator's session alive with neutral inputs.
//!
//! Free camera: WASD flies along the view, the jump key rises, Shift sinks.
//! Fire follows the next player, alt-fire (jet) the previous one. While
//! following, the view key swaps shoulder and orbit views, and the jump key
//! goes back to the free camera where the view is.
use glam::Vec3;
use crate::sim::{Player, Snapshot, Team, World, SENS};

/// Free-camera speed (m/s), and with Ctrl held.
const FLY_SPEED: f32 = 45.0;
const FLY_FAST: f32 = 140.0;
/// Over-the-shoulder offsets: behind, above and to the right of the eyes.
const SHOULDER_BACK: f32 = 4.2;
const SHOULDER_UP: f32 = 0.9;
const SHOULDER_RIGHT: f32 = 0.7;
/// Orbit distance from the followed player.
const ORBIT_DISTANCE: f32 = 8.0;
const FOV: f32 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    Free,
    /// Following a player by net ID; `orbit` turns around them with the
    /// mouse instead of looking where they look.
    Follow { net_id: u32, orbit: bool },
}

/// This frame's spectator controls.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Controls {
    pub move_x: f32,
    pub move_z: f32,
    pub up: bool,
    pub down: bool,
    pub fast: bool,
    /// Presses (edges), not held buttons.
    pub next: bool,
    pub previous: bool,
    pub toggle_view: bool,
    pub free: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SpectatorCam {
    pub view: View,
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    placed: bool,
}

fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    let cp = pitch.cos();
    Vec3::new(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp)
}

/// Players who can be followed: occupied real slots (not the spectator's stand-in).
fn watchable(world: &World) -> Vec<(usize, u32)> {
    world.players.iter().enumerate()
        .filter(|(i, p)| *i < crate::sim::MAX_PLAYERS && p.net_id != 0)
        .map(|(i, p)| (i, p.net_id)).collect()
}

/// From `from` to `to`, stopped short of any map surface.
fn pulled_in(world: &World, from: Vec3, to: Vec3) -> Vec3 {
    peakrunner_core::map_pack::on(world.map)
        .and_then(|pack| pack.sweep(from, to, 0.35))
        .map_or(to, |(t, _)| from.lerp(to, (t - 0.02).max(0.0)))
}

impl SpectatorCam {
    /// Mouse look, as a player's (radians per count).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * SENS;
        self.pitch = (self.pitch - dy * SENS).clamp(-1.52, 1.52);
    }

    /// The player being followed, if still in the match.
    pub fn watching<'a>(&self, world: &'a World) -> Option<&'a Player> {
        let View::Follow { net_id, .. } = self.view else { return None };
        world.players.iter().take(crate::sim::MAX_PLAYERS).find(|p| p.net_id == net_id)
    }

    /// Follow the next (`step` 1) or previous (-1) player in slot order;
    /// from the free camera, the first or last.
    pub fn cycle(&mut self, world: &World, step: i32) {
        let list = watchable(world);
        if list.is_empty() { self.view = View::Free; return; }
        let orbit = matches!(self.view, View::Follow { orbit: true, .. });
        let at = match self.view {
            View::Follow { net_id, .. } => list.iter().position(|&(_, id)| id == net_id),
            View::Free => None,
        };
        let n = list.len() as i32;
        let next = match at {
            Some(i) => (i as i32 + step).rem_euclid(n),
            None => if step >= 0 { 0 } else { n - 1 },
        };
        self.view = View::Follow { net_id: list[next as usize].1, orbit };
    }

    /// Back to spectating from scratch (a new session, or playing again).
    pub fn reset(&mut self) { *self = Self::default(); }

    /// Advance by `dt` with `controls`, and return the view (eye, direction,
    /// field of view).
    pub fn update(&mut self, world: &World, c: Controls, dt: f32) -> (Vec3, Vec3, f32) {
        if !self.placed {
            // Start above the middle of the map, looking along it.
            let size = peakrunner_core::terrain::info(world.map).size;
            let centre = Vec3::new(size * 0.5, 0.0, size * 0.5);
            let floor = peakrunner_core::terrain::support_on(world.map, centre + Vec3::Y * 2000.0).0;
            self.pos = Vec3::new(centre.x, floor + 60.0, centre.z + size * 0.2);
            self.yaw = 0.0; self.pitch = -0.35;
            self.placed = true;
        }
        if c.next { self.cycle(world, 1); }
        if c.previous { self.cycle(world, -1); }
        if let View::Follow { net_id, orbit } = self.view {
            if c.toggle_view {
                self.view = View::Follow { net_id, orbit: !orbit };
            }
        }
        // The followed player left: fly free from here.
        if matches!(self.view, View::Follow { .. }) && self.watching(world).is_none() { self.view = View::Free; }
        let view = match self.view {
            View::Free => {
                let forward = look_dir(self.yaw, self.pitch);
                let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
                let lift = (c.up as i32 - c.down as i32) as f32;
                let wish = forward * c.move_z + right * c.move_x + Vec3::Y * lift;
                let speed = if c.fast { FLY_FAST } else { FLY_SPEED };
                let target = self.pos + wish.clamp_length_max(1.0) * speed * dt;
                // Slide along walls rather than through them; stay over the map.
                let mut next = pulled_in(world, self.pos, target);
                let size = peakrunner_core::terrain::info(world.map).size;
                next.x = next.x.clamp(0.0, size);
                next.z = next.z.clamp(0.0, size);
                let floor = peakrunner_core::terrain::support_on(world.map, next + Vec3::Y * 0.5).0;
                next.y = next.y.clamp(floor + 1.0, floor + 1500.0);
                self.pos = next;
                (self.pos, forward, FOV)
            }
            View::Follow { orbit, .. } => {
                let p = self.watching(world).expect("checked above");
                let pivot = p.pos + Vec3::Y * (crate::terrain::EYE + 0.2);
                let (eye, dir) = if orbit {
                    let dir = look_dir(self.yaw, self.pitch);
                    (pulled_in(world, pivot, pivot - dir * ORBIT_DISTANCE), dir)
                } else {
                    let dir = look_dir(p.yaw, p.pitch);
                    let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
                    let want = pivot - dir * SHOULDER_BACK + Vec3::Y * SHOULDER_UP + right * SHOULDER_RIGHT;
                    (pulled_in(world, pivot, want), dir)
                };
                // The free camera starts where this view is.
                self.pos = eye;
                if !orbit { self.yaw = p.yaw; self.pitch = p.pitch; }
                (eye, dir, FOV)
            }
        };
        if c.free { self.view = View::Free; }
        view
    }

    /// One line for the spectator HUD: who or what is being watched.
    pub fn label(&self, world: &World) -> String {
        match (self.view, self.watching(world)) {
            (View::Follow { orbit, .. }, Some(p)) => format!("Watching {} ({}) · {}", p.name,
                if p.team == Team::Ember { "Ember" } else { "Glacier" }, if orbit { "orbit" } else { "over the shoulder" }),
            _ => "Free camera".into(),
        }
    }
}

/// The spectator HUD's join line: waiting in the queue for a free slot
/// (a full match), or which key asks for one.
pub(crate) fn join_hint(snapshot: Option<&Snapshot>, id: u32, key: &str) -> String {
    let queued = snapshot.is_some_and(|s| s.spectators.iter().any(|w| w.net_id == id && w.queued));
    if queued { "Match full: you'll join a team when a slot opens".into() } else { format!("{key}: join a team") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sim::Match, terrain::MapId};

    fn spectating_world() -> World {
        let mut server = Match::new(MapId::OldHoller);
        server.join(1, "Alpha").unwrap();
        server.join(2, "Bravo").unwrap();
        server.join_spectator(3, "Watcher").unwrap();
        server.step(&[]);
        let mut world = World::new();
        assert!(!world.apply_snapshot(&server.snapshot(), 3), "not a player");
        assert!(world.apply_spectator_snapshot(&server.snapshot(), 3));
        world
    }

    #[test]
    fn a_spectator_world_has_a_stand_in_past_the_slots() {
        let world = spectating_world();
        assert!(world.spectating);
        assert_eq!(world.player_id, crate::sim::MAX_PLAYERS);
        let me = &world.players[world.player_id];
        assert!(!me.alive && me.net_id == 0, "a dead, unnamed stand-in");
        assert!(world.players[..crate::sim::MAX_PLAYERS].iter().all(|p| p.remote));
    }

    #[test]
    fn free_camera_flies_along_the_view_and_stays_over_the_map() {
        let world = spectating_world();
        let mut cam = SpectatorCam::default();
        let (start, _, _) = cam.update(&world, Controls::default(), 0.0);
        let forward = Controls { move_z: 1.0, ..Controls::default() };
        let (moved, dir, _) = cam.update(&world, forward, 0.5);
        assert!((moved - start).dot(dir) > 10.0, "flew forward: {start} -> {moved}");
        let rise = Controls { up: true, ..Controls::default() };
        let (up, _, _) = cam.update(&world, rise, 0.5);
        assert!(up.y > moved.y + 10.0);
        // Diving into the ground stops above it.
        let dive = Controls { down: true, fast: true, ..Controls::default() };
        let mut eye = up;
        for _ in 0..200 { eye = cam.update(&world, dive, 0.1).0; }
        let floor = peakrunner_core::terrain::support_on(world.map, eye + Vec3::Y * 0.5).0;
        assert!(eye.y >= floor + 0.9, "{eye} over {floor}");
        cam.look(100.0, 0.0);
        let (_, turned, _) = cam.update(&world, Controls::default(), 0.0);
        assert!(turned.distance(dir) > 0.1, "mouse turns the view");
    }

    #[test]
    fn follow_cycles_players_both_ways_and_views_from_behind() {
        let world = spectating_world();
        let mut cam = SpectatorCam::default();
        let press = |next, previous| Controls { next, previous, ..Controls::default() };
        cam.update(&world, press(true, false), 0.0);
        assert_eq!(cam.watching(&world).unwrap().name, "Alpha");
        cam.update(&world, press(true, false), 0.0);
        assert_eq!(cam.watching(&world).unwrap().name, "Bravo");
        cam.update(&world, press(true, false), 0.0);
        assert_eq!(cam.watching(&world).unwrap().name, "Alpha", "wraps around");
        cam.update(&world, press(false, true), 0.0);
        assert_eq!(cam.watching(&world).unwrap().name, "Bravo", "and back");
        // Over the shoulder: behind the player, looking where they look.
        let p = cam.watching(&world).unwrap().clone();
        let (eye, dir, _) = cam.update(&world, Controls::default(), 0.0);
        assert!(eye.distance(p.pos) < SHOULDER_BACK + 3.0);
        assert!((p.pos - eye).dot(dir) > 0.0, "the player is in front of the camera");
        assert!(cam.label(&world).contains("Bravo"));
        // Orbit: the mouse turns around them at a distance.
        cam.update(&world, Controls { toggle_view: true, ..Controls::default() }, 0.0);
        cam.look(400.0, 0.0);
        let (eye2, dir2, _) = cam.update(&world, Controls::default(), 0.0);
        assert!(eye2.distance(eye) > 1.0 && (p.pos - eye2).dot(dir2) > 0.0);
        assert!(cam.label(&world).contains("orbit"));
        // The jump key frees the camera where it is.
        cam.update(&world, Controls { free: true, ..Controls::default() }, 0.0);
        assert_eq!(cam.view, View::Free);
        let (free_eye, _, _) = cam.update(&world, Controls::default(), 0.0);
        assert!(free_eye.distance(eye2) < 0.01);
    }

    #[test]
    fn a_waiting_spectator_is_told_they_will_join() {
        let mut server = Match::new(MapId::OldHoller);
        server.join_spectator(3, "Watcher").unwrap();
        assert_eq!(join_hint(Some(&server.snapshot()), 3, "J"), "J: join a team");
        server.queue_spectator(3, true);
        assert!(join_hint(Some(&server.snapshot()), 3, "J").contains("when a slot opens"));
        assert_eq!(join_hint(None, 3, "J"), "J: join a team");
    }

    #[test]
    fn a_followed_player_leaving_frees_the_camera() {
        let mut world = spectating_world();
        let mut cam = SpectatorCam::default();
        cam.cycle(&world, 1);
        for p in &mut world.players { if p.name == "Alpha" { p.net_id = 0; } }
        cam.update(&world, Controls::default(), 0.1);
        assert_eq!(cam.view, View::Free);
    }
}
