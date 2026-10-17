//! Capture & Hold HUD: point labels (the beacon light on each point shows its
//! owner and progress; there are no ground rings), a capture bar while you stand
//! in a point, a drain-field warning, edge markers for off-screen points in
//! Capture & Hold, and point announcements. Everything comes from snapshot state; announcements diff
//! owners between frames, so replayed snapshots announce nothing.
use egui::{Align2, Color32, FontId, Pos2, Stroke, Vec2};
use glam::Vec3;
use peakrunner_core::control::Point;
use peakrunner_core::sim::World;

/// Rings and labels fade out beyond this.
pub const RING_RANGE: f32 = 450.0;
const SEGMENTS: usize = 40;
const NEUTRAL: Color32 = Color32::from_rgb(206, 210, 218);
const EMBER: Color32 = Color32::from_rgb(237, 91, 57);
const GLACIER: Color32 = Color32::from_rgb(54, 206, 226);
const DRAIN: Color32 = Color32::from_rgb(255, 84, 72);

pub fn team_color(team: Option<u8>) -> Color32 {
    match team { Some(0) => EMBER, Some(1) => GLACIER, _ => NEUTRAL }
}

fn team_name(team: u8) -> &'static str { if team == 0 { "Ember" } else { "Glacier" } }

pub(crate) fn fade(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// What the local player should be told about the point they stand in.
pub fn status_line(p: &Point, team: u8) -> String {
    let left = (1.0 - p.progress) * p.capture_seconds();
    if p.contested { return format!("{} CONTESTED", p.name.to_uppercase()); }
    match (p.owner, p.capturing) {
        (_, Some(c)) if c == team => format!("CAPTURING {} · {left:.1} s", p.name.to_uppercase()),
        (_, Some(_)) => format!("CLEARING ENEMY PROGRESS · {}", p.name.to_uppercase()),
        (Some(o), None) if o == team => format!("HOLDING {}", p.name.to_uppercase()),
        _ => format!("CAPTURING {} · {left:.1} s", p.name.to_uppercase()),
    }
}

/// Announcement for one point's owner change, worded for the viewer's team.
pub fn announcement(name: &str, before: Option<u8>, after: Option<u8>, team: u8) -> Option<String> {
    let after = after?;
    if before == Some(after) { return None; }
    Some(if after == team { format!("Your team captured the {name}") }
        else if before == Some(team) { format!("Your team lost the {name}") }
        else { format!("{} captured the {name}", team_name(after)) })
}

#[derive(Default)]
pub struct PointWatch { last: Option<(peakrunner_core::terrain::MapId, u8, Vec<Option<u8>>)> }

impl PointWatch {
    pub fn update(&mut self, world: &World) -> Vec<String> {
        let Some(me) = world.players.get(world.player_id) else { self.last = None; return Vec::new(); };
        let team = me.team.idx() as u8;
        let owners: Vec<Option<u8>> = world.points.iter().map(|p| p.owner).collect();
        let mut out = Vec::new();
        if let Some((map, was_team, was)) = &self.last {
            if *map == world.map && *was_team == team && was.len() == owners.len() {
                for ((p, before), after) in world.points.iter().zip(was).zip(&owners) {
                    if let Some(text) = announcement(&p.name, *before, *after, team) { out.push(text); }
                }
            }
        }
        self.last = Some((world.map, team, owners));
        out
    }
}

pub(crate) fn ring(painter: &egui::Painter, eye: Vec3, dir: Vec3, fov: f32, rect: egui::Rect, center: Vec3, radius: f32,
        from: f32, to: f32, stroke: Stroke) {
    let steps = ((to - from) * SEGMENTS as f32).ceil().max(1.0) as usize;
    let mut line: Vec<Pos2> = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = from + (to - from) * k as f32 / steps as f32;
        let a = t * std::f32::consts::TAU;
        let at = center + Vec3::new(a.sin() * radius, 0.0, -a.cos() * radius);
        match crate::world_overlay::project(eye, dir, fov, rect, at) {
            Some(p) => line.push(p),
            None => { if line.len() > 1 { painter.add(egui::Shape::line(std::mem::take(&mut line), stroke)); } line.clear(); }
        }
    }
    if line.len() > 1 { painter.add(egui::Shape::line(line, stroke)); }
}

pub fn draw(ui: &egui::Ui, world: &World) {
    if world.points.iter().all(|p| !p.active) { return; }
    let rect = ui.max_rect();
    let painter = ui.painter();
    let (eye, dir, fov) = crate::drawlist::view_camera(world);
    let me = world.players.get(world.player_id);
    let team = me.map_or(0, |p| p.team.idx() as u8);
    for p in world.points.iter().filter(|p| p.active) {
        let center = p.pos + Vec3::Y * 0.15;
        let alpha = crate::world_overlay::fade(eye.distance(center), RING_RANGE);
        if alpha <= 0.0 { continue; }
        // No rings on the ground: the point's beacon light shows its owner and
        // capture progress (drawlist::push_beacons).
        let owner = team_color(p.owner);
        if let Some(label) = crate::world_overlay::project(eye, dir, fov, rect, p.pos + Vec3::Y * 6.0) {
            let state = if p.contested { "CONTESTED".to_string() } else {
                p.owner.map_or("NEUTRAL".into(), |o| team_name(o).to_uppercase()) };
            let text = format!("{}\n{state} · {:.0} m", p.name.to_uppercase(), eye.distance(p.pos));
            painter.text(label + Vec2::splat(1.0), Align2::CENTER_BOTTOM, &text, FontId::proportional(13.0), fade(Color32::BLACK, alpha));
            painter.text(label, Align2::CENTER_BOTTOM, &text, FontId::proportional(13.0), fade(owner, alpha));
        }
    }
    let Some(me) = me.filter(|p| p.alive) else { return; };
    if world.mode.rules() == peakrunner_core::modes::Rules::CaptureAndHold {
        edge_markers(ui, world, eye, dir, fov);
        chips(ui, world);
    }
    let bottom = rect.center_bottom() - Vec2::new(0.0, 150.0);
    if let Some(p) = world.points.iter().find(|p| p.active && p.contains(me.pos)) {
        let w = 280.0;
        let bar = egui::Rect::from_center_size(bottom, Vec2::new(w, 10.0));
        painter.rect_filled(bar, 3.0, Color32::from_black_alpha(160));
        let fill = if p.capturing.is_some() { p.progress } else if p.owner.is_some() { 1.0 } else { 0.0 };
        let color = team_color(p.capturing.or(p.owner));
        painter.rect_filled(egui::Rect::from_min_size(bar.min, Vec2::new(w * fill, 10.0)), 3.0, color);
        let text = status_line(p, team);
        painter.text(bottom - Vec2::new(0.0, 12.0), Align2::CENTER_BOTTOM, &text, FontId::proportional(15.0), color);
    }
    if let Some(drain) = world.point_drain(world.player_id) {
        let pulse = 0.55 + 0.45 * (world.time * 5.0).sin().abs();
        let source = if drain.spire.is_some() { "ENEMY SPIRES" } else { "ENEMY-HELD POINT" };
        let hurt = if drain.damage > 0.0 { format!(" · HEALTH −{:.0}/s", drain.damage) } else { String::new() };
        let text = format!("ENERGY DRAIN −{:.0}/s{hurt} · {source}", drain.rate);
        painter.text(bottom - Vec2::new(0.0, 44.0), Align2::CENTER_BOTTOM, &text, FontId::proportional(16.0), fade(DRAIN, pulse));
        painter.rect_stroke(rect.shrink(3.0), 0.0, Stroke::new(4.0, fade(DRAIN, 0.25 * pulse)), egui::StrokeKind::Inside);
    }
}

fn edge_markers(ui: &egui::Ui, world: &World, eye: Vec3, dir: Vec3, fov: f32) {
    let rect = ui.max_rect();
    let inset = rect.shrink2(Vec2::new(82.0, 100.0));
    if inset.width() <= 0.0 || inset.height() <= 0.0 { return; }
    for p in world.points.iter().filter(|p| p.active) {
        let Some(direction) = crate::flag_hud::bearing(p.pos - eye, dir, fov, rect.aspect_ratio()) else { continue; };
        let half = inset.size() * 0.5;
        let distance = (half.x / direction.x.abs().max(0.0001)).min(half.y / direction.y.abs().max(0.0001));
        let tip = inset.center() + direction * distance;
        let side = Vec2::new(-direction.y, direction.x);
        let color = team_color(p.owner);
        ui.painter().add(egui::Shape::convex_polygon(vec![tip, tip - direction * 15.0 + side * 6.0, tip - direction * 15.0 - side * 6.0],
            color, Stroke::new(1.5, Color32::BLACK)));
        let text = format!("{}\n{:.0} m", p.name.to_uppercase(), eye.distance(p.pos));
        ui.painter().text(tip - direction * 36.0, Align2::CENTER_CENTER, text, FontId::proportional(11.0), color);
    }
}

/// Point chips under the score in Capture & Hold: one per point, owner-coloured.
fn chips(ui: &egui::Ui, world: &World) {
    let rect = ui.max_rect();
    let points: Vec<&Point> = world.points.iter().filter(|p| p.active).collect();
    let w = 64.0;
    let start = rect.center_top() + Vec2::new(-(points.len() as f32 * (w + 6.0) - 6.0) * 0.5, 70.0);
    for (i, p) in points.iter().enumerate() {
        let r = egui::Rect::from_min_size(start + Vec2::new(i as f32 * (w + 6.0), 0.0), Vec2::new(w, 18.0));
        ui.painter().rect_filled(r, 4.0, fade(team_color(p.owner), 0.85));
        if p.progress > 0.0 {
            let bar = egui::Rect::from_min_size(r.left_bottom() - Vec2::new(0.0, 3.0), Vec2::new(w * p.progress, 3.0));
            ui.painter().rect_filled(bar, 1.0, team_color(p.capturing));
        }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, p.name.to_uppercase(), FontId::proportional(11.0), Color32::from_rgb(12, 18, 28));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::control::{Definition, Point};
    use peakrunner_core::sim::Team;
    use peakrunner_core::terrain::MapId;

    fn point() -> Point {
        Point::from_def(&Definition { id: "beacon".into(), name: "Beacon".into(), pos: [0.0; 3], radius: 12.0,
            ctf_active: true, drain: None, turret: None, capture: None, spires: Vec::new(), beacon: None, cook: None }, true)
    }

    #[test]
    fn announcements_are_worded_for_the_viewer() {
        assert_eq!(announcement("Beacon", None, Some(1), 1).unwrap(), "Your team captured the Beacon");
        assert_eq!(announcement("Beacon", Some(1), Some(0), 1).unwrap(), "Your team lost the Beacon");
        assert_eq!(announcement("Beacon", None, Some(0), 1).unwrap(), "Ember captured the Beacon");
        assert_eq!(announcement("Beacon", Some(1), Some(1), 1), None);
        assert_eq!(announcement("Beacon", Some(1), None, 1), None, "a round reset is not a capture");
    }

    #[test]
    fn point_watch_announces_once_and_replays_are_silent() {
        let mut w = World::new(); w.set_map(MapId::OldHoller); w.start_rift(true);
        w.players[0].team = Team::Glacier;
        w.set_control_points(vec![Definition { id: "beacon".into(), name: "Beacon".into(), pos: [0.0; 3], radius: 12.0, ctf_active: true, drain: None, turret: None,
            capture: None, spires: Vec::new(), beacon: None, cook: None }]);
        let mut watch = PointWatch::default();
        assert!(watch.update(&w).is_empty(), "first frame only baselines");
        w.points[0].owner = Some(1);
        assert_eq!(watch.update(&w), vec!["Your team captured the Beacon".to_string()]);
        assert!(watch.update(&w).is_empty(), "the same state again announces nothing");
        w.set_map(MapId::TowerComplex);
        assert!(watch.update(&w).is_empty(), "a map change rebaselines");
    }

    #[test]
    fn status_line_tracks_capture_state() {
        let mut p = point();
        p.capturing = Some(0); p.progress = 0.4;
        assert_eq!(status_line(&p, 0), "CAPTURING BEACON · 6.0 s");
        assert_eq!(status_line(&p, 1), "CLEARING ENEMY PROGRESS · BEACON");
        p.capturing = None; p.progress = 0.0; p.owner = Some(1);
        assert_eq!(status_line(&p, 1), "HOLDING BEACON");
        p.contested = true;
        assert_eq!(status_line(&p, 1), "BEACON CONTESTED");
    }
}
