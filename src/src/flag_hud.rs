//! Flag bearings use the same camera basis/FOV as the scene, including behind-camera targets.
use egui::{Align2, Color32, FontId, Pos2, Vec2};
use glam::Vec3;

pub(crate) fn bearing(delta: Vec3, forward: Vec3, fov: f32, aspect: f32) -> Option<Vec2> {
    if !delta.is_finite() || delta.length_squared() < 0.01 { return None; }
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);
    let depth = delta.dot(forward);
    let scale = (fov.to_radians() * 0.5).tan();
    let mut direction = Vec2::new(delta.dot(right) / (scale * aspect), -delta.dot(up) / scale);
    if depth > 0.0 && direction.x.abs() < depth * 0.85 && direction.y.abs() < depth * 0.85 { return None; }
    if direction.length_sq() < 0.001 { direction = Vec2::new(0.0, 1.0); }
    Some(direction.normalized())
}

/// Where a world point lands on screen (normalised -1..1 from the centre),
/// or None behind the camera. Same basis as `bearing`.
pub(crate) fn project(delta: Vec3, forward: Vec3, fov: f32, aspect: f32) -> Option<Vec2> {
    let depth = delta.dot(forward);
    if !delta.is_finite() || depth <= 0.1 { return None; }
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward);
    let scale = (fov.to_radians() * 0.5).tan();
    Some(Vec2::new(delta.dot(right) / (depth * scale * aspect), -delta.dot(up) / (depth * scale)))
}

/// A flag lying loose: carried by nobody and away from its stand.
fn dropped(flag: &crate::sim::Flag) -> bool { flag.carrier.is_none() && flag.pos.distance(flag.home) >= 3.0 }

pub fn draw(ui: &egui::Ui, world: &crate::sim::World) {
    let Some(player) = world.players.get(world.player_id).filter(|p| p.alive) else { return; };
    let rect = ui.max_rect();
    let (eye, forward, fov) = world.camera();
    let inset = rect.shrink2(Vec2::new(82.0, 100.0));
    if inset.width() <= 0.0 || inset.height() <= 0.0 { return; }
    // Capture & Hold has no flags in play; control points get their own markers.
    if world.mode.rules() != peakrunner_core::modes::Rules::Ctf { return; }
    for flag in &world.flags {
        // When carrying the enemy flag, point to our capture stand instead of ourselves.
        if flag.carrier == Some(world.player_id) { continue; }
        let target = if flag.team == player.team && player.carrying.is_some() { flag.home } else { flag.pos };
        let color = if flag.team == crate::sim::Team::Ember { Color32::from_rgb(237,91,57) } else { Color32::from_rgb(54,206,226) };
        let loose = dropped(flag) && target == flag.pos;
        let label = if flag.team != player.team { if loose { "ENEMY FLAG · DOWN" } else { "ENEMY FLAG" } }
            else if player.carrying.is_some() { "CAPTURE BASE" } else if loose { "OUR FLAG · DOWN" } else { "OUR FLAG" };
        let Some(direction) = bearing(target-eye, forward, fov, rect.aspect_ratio()) else {
            // On screen: a dropped flag gets a marker over it, for both teams
            // (it can be anywhere, often in the grass).
            if loose {
                if let Some(at) = project(target + Vec3::Y * 2.2 - eye, forward, fov, rect.aspect_ratio()) {
                    let tip = rect.center() + Vec2::new(at.x * rect.width() * 0.5, at.y * rect.height() * 0.5);
                    let pulse = 1.0 + 0.15 * (world.time * 5.0).sin();
                    let r = 8.0 * pulse;
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![tip + Vec2::new(0.0, r), tip + Vec2::new(-r * 0.8, -r * 0.4), tip + Vec2::new(r * 0.8, -r * 0.4)],
                        color, egui::Stroke::new(1.5, Color32::BLACK)));
                    let text = format!("{label}\n{:.0} m", eye.distance(target));
                    let at = tip - Vec2::new(0.0, 26.0);
                    ui.painter().text(at + Vec2::splat(1.0), Align2::CENTER_CENTER, &text, FontId::proportional(12.0), Color32::BLACK);
                    ui.painter().text(at, Align2::CENTER_CENTER, &text, FontId::proportional(12.0), color);
                }
            }
            continue;
        };
        let half = inset.size() * 0.5;
        let distance = (half.x / direction.x.abs().max(0.0001)).min(half.y / direction.y.abs().max(0.0001));
        let tip = inset.center() + direction * distance;
        let side = Vec2::new(-direction.y, direction.x);
        ui.painter().add(egui::Shape::convex_polygon(vec![tip, tip-direction*17.0+side*7.0, tip-direction*17.0-side*7.0], color, egui::Stroke::new(1.5,Color32::BLACK)));
        let text: Pos2 = tip - direction * 40.0;
        ui.painter().text(text+Vec2::splat(1.0), Align2::CENTER_CENTER, format!("{label}\n{:.0} m",eye.distance(target)), FontId::proportional(12.0), Color32::BLACK);
        ui.painter().text(text, Align2::CENTER_CENTER, format!("{label}\n{:.0} m",eye.distance(target)), FontId::proportional(12.0), color);
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn camera_bearings_are_not_mirrored() {
        let f = -Vec3::Z;
        assert!(bearing(f*50.0, f, 70.0, 1.6).is_none());
        assert!(bearing(Vec3::X*50.0, f, 70.0, 1.6).unwrap().x > 0.0);
        assert!(bearing(-Vec3::X*50.0, f, 70.0, 1.6).unwrap().x < 0.0);
        assert!(bearing(Vec3::Y*50.0, f, 70.0, 1.6).unwrap().y < 0.0);
        assert!(bearing(Vec3::Z*50.0, f, 70.0, 1.6).unwrap().is_finite());
    }
    #[test] fn on_screen_points_project_where_bearing_gives_up() {
        let f = -Vec3::Z;
        // Straight ahead: centre of the screen, and no edge arrow.
        assert!(bearing(f*50.0, f, 70.0, 1.6).is_none());
        assert!(project(f*50.0, f, 70.0, 1.6).unwrap().length() < 1e-4);
        // A little right and below: right of centre, below it (screen y grows down).
        let p = project(f*50.0 + Vec3::X*5.0 - Vec3::Y*3.0, f, 70.0, 1.6).unwrap();
        assert!(p.x > 0.0 && p.y > 0.0);
        assert!(project(-f*50.0, f, 70.0, 1.6).is_none(), "behind");
    }
}
