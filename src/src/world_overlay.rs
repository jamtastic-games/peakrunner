//! World-anchored HUD: hit bars over destructible equipment, red arrows over
//! enemies (not in Football), a long-range marker over each flag carrier, and
//! the name of the one player under the crosshair. As in the classic, nobody
//! else is named: their armor's color tells the teams apart
//! (`player_model::TEAM_PLATES`).
//! Line-of-sight checks here are cosmetic: every position shown already arrives
//! in snapshots, so hiding a tag behind a wall reveals nothing. The carrier
//! marker ignores terrain on purpose: the edge-of-screen flag bearing already
//! gives every flag's position through walls.
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use glam::{Mat4, Vec3, Vec4};
use peakrunner_core::equipment::{Kind, State};
use peakrunner_core::sim::{Player, World};

pub const HIT_BAR_RANGE: f32 = 120.0;
/// How far the crosshair names whoever it is on (times a sight's
/// magnification).
pub const NAME_RANGE: f32 = 250.0;
/// How long a name stays after the crosshair leaves its body (s), the last
/// `NAME_FADE` of it fading.
const NAME_HOLD: f32 = 0.4;
const NAME_FADE: f32 = 0.2;
/// Red arrows over enemies need a clear line of sight.
pub const ENEMY_ARROW_RANGE: f32 = 250.0;
/// How far a sensed enemy is marked.
const SENSED_RANGE: f32 = 400.0;
/// Flag carriers are marked across any current map (the longest flag-to-flag
/// distance is 816 m), through terrain.
pub const CARRIER_MARKER_RANGE: f32 = 1500.0;
/// Fraction of a range over which an overlay fades out.
const FADE_BAND: f32 = 0.2;

const TEAMMATE_NAME: Color32 = Color32::from_rgb(96, 164, 255);
const ENEMY_NAME: Color32 = Color32::from_rgb(255, 84, 72);
const SHIELD: Color32 = Color32::from_rgb(200, 206, 255);
const OFFLINE: Color32 = Color32::from_rgb(120, 124, 132);

/// Projects a world point with the render camera. `None` behind the camera
/// or outside the viewport (plus a small margin so tags don't pop at edges).
pub fn project(eye: Vec3, dir: Vec3, fov: f32, rect: Rect, point: Vec3) -> Option<Pos2> {
    if !point.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 { return None; }
    let view = Mat4::look_to_rh(eye, dir, Vec3::Y);
    let proj = Mat4::perspective_rh(fov.to_radians(), rect.aspect_ratio(), 0.14, 10_000.0);
    let clip = proj * view * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.2 { return None; }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() > 1.05 || ndc.y.abs() > 1.05 { return None; }
    Some(Pos2::new(rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(), rect.top() + (1.0 - ndc.y) * 0.5 * rect.height()))
}

/// 1 inside the range, fading linearly to 0 over the last `FADE_BAND`.
pub fn fade(distance: f32, range: f32) -> f32 {
    if !distance.is_finite() || distance > range { return 0.0; }
    ((range - distance) / (range * FADE_BAND)).clamp(0.0, 1.0)
}

/// A name's colour, relative to the viewer (`friendly`: not their foe,
/// `World::foe`): friends blue, enemies red, whichever side the viewer plays.
pub fn name_color(friendly: bool) -> Color32 { if friendly { TEAMMATE_NAME } else { ENEMY_NAME } }

fn head(p: &Player) -> Vec3 { p.pos + Vec3::Y * 2.35 }

/// A body's extent for the crosshair: soles to crown (`player_model::STATURE`)
/// and a little wider than the shoulders.
const BODY_FEET: f32 = peakrunner_core::terrain::PLAYER_RADIUS;
const BODY_CROWN: f32 = 1.86;
const BODY_REACH: f32 = 0.45;
/// How far off a far body the crosshair may be and still name it, as an
/// angle (rad): a little more than the body itself at a distance.
const AIM_SLACK: f32 = 0.006;

/// The player under the crosshair (the view's centre line from `eye` along
/// `dir`), the nearest body along it, within `NAME_RANGE` times `reach` (a
/// sight's magnification, `World::zoom_power`) and in clear sight. Never the
/// viewer, the dead, the hidden or a cloaked enemy.
pub fn pointed_at(world: &World, eye: Vec3, dir: Vec3, reach: f32) -> Option<usize> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO { return None; }
    let range = NAME_RANGE * reach;
    let mut best: Option<(usize, f32)> = None;
    for (i, p) in world.players.iter().enumerate() {
        if i == world.player_id || !p.alive || p.hidden() || p.name.is_empty() { continue; }
        if world.foe(world.player_id, i) && peakrunner_core::sim::status::hides_name(&p.effects) { continue; }
        let at = p.pos + world.drawn_offset(i);
        let (feet, crown) = (at - Vec3::Y * BODY_FEET, at + Vec3::Y * BODY_CROWN);
        let Some((along, miss)) = ray_to_segment(eye, dir, feet, crown) else { continue };
        if along > range || miss > BODY_REACH + along * AIM_SLACK / reach { continue; }
        if best.is_some_and(|(_, d)| d <= along) { continue; }
        if !world.sight_clear(eye, eye + dir * along) { continue; }
        best = Some((i, along));
    }
    best.map(|(i, _)| i)
}

/// The ray's closest approach to the segment `a`–`b`: how far along the ray
/// (ahead of `eye` only) and by how much it misses.
fn ray_to_segment(eye: Vec3, dir: Vec3, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
    let axis = b - a;
    let w = eye - a;
    let (aa, ab, aw, bw) = (axis.dot(axis), axis.dot(dir), axis.dot(w), dir.dot(w));
    let denom = aa - ab * ab;
    // Where on the segment the ray passes closest, then where on the ray.
    let s = if denom > 1e-6 { ((aw - ab * bw) / denom).clamp(0.0, 1.0) } else { 0.0 };
    let point = a + axis * s;
    let t = (point - eye).dot(dir);
    (t > 0.0).then(|| (t, (eye + dir * t).distance(point)))
}

/// Whether enemy `i` is in plain sight from `eye` as the arrows have it:
/// within ENEMY_ARROW_RANGE (times a sight's magnification) and nothing in
/// between, or marked by the viewer's team. Rabbit's maps show foes only so
/// (`map_view`).
pub fn in_sight(world: &World, eye: Vec3, i: usize) -> bool {
    let (Some(viewer), Some(p)) = (world.players.get(world.player_id), world.players.get(i)) else { return false };
    if world.marked(viewer.team, i) { return true; }
    let top = head(p) + world.drawn_offset(i);
    eye.distance(top) < ENEMY_ARROW_RANGE * world.zoom_power() && world.sight_clear(eye, top)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    /// Carrying a flag: `flag` is that flag's team; `ally` is relative to the
    /// viewer; `tag` the name's opacity, under the crosshair.
    Carrier { flag: peakrunner_core::sim::Team, ally: bool, alpha: f32, tag: f32 },
    /// An enemy in range and in sight; `tag` is the name's opacity (0 = no name).
    Enemy { alpha: f32, tag: f32 },
    /// Only a name: a teammate under the crosshair, or in Football (no
    /// arrows) anyone under it; `friendly` relative to the viewer.
    Name { tag: f32, friendly: bool },
}

/// What to draw over player `i` and where, as seen from `eye`. Enemies need to
/// be within ENEMY_ARROW_RANGE, on screen and in line of sight, and Football
/// has no arrows, as in the classic mod; a flag carrier
/// only needs to be on screen and within CARRIER_MARKER_RANGE, and its marker
/// replaces the arrow. `named`: the player the crosshair names
/// (`NameWatch::named`) and the name's opacity; nobody else is named.
pub fn visible_marker(world: &World, eye: Vec3, dir: Vec3, fov: f32, rect: Rect, i: usize, named: Option<(usize, f32)>) -> Option<(Marker, Pos2)> {
    let viewer = world.players.get(world.player_id)?;
    let p = world.players.get(i)?;
    if i == world.player_id || !p.alive || p.hidden() { return None; }
    // Over the body as drawn (between steps offline).
    let top = head(p) + world.drawn_offset(i);
    let distance = eye.distance(top);
    if let Some(flag) = world.flags.iter().find(|f| f.carrier == Some(i)) {
        let alpha = fade(distance, CARRIER_MARKER_RANGE);
        if alpha <= 0.0 { return None; }
        let at = project(eye, dir, fov, rect, top)?;
        let tag = named.filter(|n| n.0 == i).map_or(0.0, |n| n.1);
        return Some((Marker::Carrier { flag: flag.team, ally: p.team == viewer.team, alpha, tag }, at));
    }
    // Enemies by the mode (`World::foe`): in Deathmatch everyone, whatever
    // their team; in Rabbit the rabbit and its hunters. Through a sight,
    // arrows reach as much farther as it magnifies.
    let hostile = world.foe(world.player_id, i);
    let reach = world.zoom_power();
    let tag = named.filter(|n| n.0 == i).map_or(0.0, |n| n.1);
    // A cloak hides an enemy's name and arrow, and fades a teammate's.
    let veiled = peakrunner_core::sim::status::hides_name(&p.effects);
    if veiled && hostile { return None; }
    let tag = if veiled { tag * 0.5 } else { tag };
    // An enemy their team's sensors sense is marked out to SENSED_RANGE,
    // walls or not (`placed`); one a teammate marked (the target marker:
    // `firing`), as far as it can be seen. Rabbit's sensors stand idle, so
    // its arrows need sight: hunters find the rabbit through hills only
    // during a ping (`rabbit_hud`).
    let marked = hostile && world.marked(viewer.team, i);
    let sensed = marked || hostile && world.has_sensor(viewer.team) && world.sensed(viewer.team).contains(&i);
    let marker = if !hostile || world.football() {
        if tag <= 0.0 { return None; }
        Marker::Name { tag, friendly: !hostile }
    } else {
        let alpha = if marked { 1.0 } else { fade(distance, if sensed { SENSED_RANGE.max(ENEMY_ARROW_RANGE * reach) } else { ENEMY_ARROW_RANGE * reach }) };
        if alpha <= 0.0 { return None; }
        Marker::Enemy { alpha, tag }
    };
    let at = project(eye, dir, fov, rect, top)?;
    if !sensed && !world.sight_clear(eye, top) { return None; }
    Some((marker, at))
}

fn team_color(team: peakrunner_core::sim::Team) -> Color32 {
    if team == peakrunner_core::sim::Team::Ember { Color32::from_rgb(237, 91, 57) } else { Color32::from_rgb(54, 206, 226) }
}

/// A downward chevron whose tip sits at `tip`.
fn chevron(painter: &egui::Painter, tip: Pos2, half: f32, fill: Color32, alpha: f32) {
    let h = half * 1.25;
    let pts = vec![tip, Pos2::new(tip.x + half, tip.y - h), Pos2::new(tip.x, tip.y - h * 0.55), Pos2::new(tip.x - half, tip.y - h)];
    painter.add(egui::Shape::convex_polygon(vec![pts[0], pts[1], pts[2]], with_alpha(fill, alpha), Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![pts[0], pts[2], pts[3]], with_alpha(fill, alpha), Stroke::NONE));
    painter.add(egui::Shape::closed_line(pts, Stroke::new(1.2, with_alpha(Color32::BLACK, alpha * 0.85))));
}

fn outlined(painter: &egui::Painter, at: Pos2, text: &str, size: f32, color: Color32, alpha: f32) {
    let font = FontId::proportional(size);
    painter.text(at + Vec2::new(1.0, 1.0), Align2::CENTER_BOTTOM, text, font.clone(), with_alpha(Color32::BLACK, alpha * 0.85));
    painter.text(at, Align2::CENTER_BOTTOM, text, font, with_alpha(color, alpha));
}

fn draw_player(painter: &egui::Painter, p: &Player, marker: Marker, at: Pos2, distance: f32) {
    match marker {
        Marker::Name { tag, friendly } => outlined(painter, at, &p.name, 14.0, name_color(friendly), tag),
        Marker::Enemy { alpha, tag } => {
            let half = (9.0 - distance * 0.014).clamp(5.5, 9.0);
            let tip = at + Vec2::new(0.0, -2.0);
            chevron(painter, tip, half, ENEMY_NAME, alpha);
            if tag > 0.0 { outlined(painter, tip - Vec2::new(0.0, half * 1.25 + 3.0), &p.name, 14.0, ENEMY_NAME, tag.min(alpha)); }
        }
        Marker::Carrier { flag, ally, alpha, tag } => {
            let tip = at + Vec2::new(0.0, -2.0);
            chevron(painter, tip, 14.0, team_color(flag), alpha);
            // Pennant above the chevron, in the carried flag's colour.
            let base = tip - Vec2::new(0.0, 22.0);
            painter.line_segment([base, base - Vec2::new(0.0, 18.0)], Stroke::new(2.0, with_alpha(Color32::from_gray(230), alpha)));
            painter.add(egui::Shape::convex_polygon(vec![base - Vec2::new(0.0, 18.0), base - Vec2::new(-13.0, 13.0), base - Vec2::new(0.0, 8.0)],
                with_alpha(team_color(flag), alpha), Stroke::new(1.0, with_alpha(Color32::BLACK, alpha))));
            let name_color = name_color(ally);
            let caption = if ally { "FLAG CARRIER" } else { "HAS YOUR FLAG" };
            let label = base - Vec2::new(0.0, 21.0);
            let detail = if distance > 60.0 { format!("{caption} · {distance:.0} m") } else { caption.to_string() };
            outlined(painter, label, &detail, 12.0, Color32::from_gray(225), alpha);
            if tag > 0.0 { outlined(painter, label - Vec2::new(0.0, 14.0), &p.name, 18.0, name_color, tag.min(alpha)); }
        }
    }
}

/// Top of the kit model above its entity point (generator cap, sensor vane,
/// turret head), so the ceiling probe starts clear of the model itself.
fn model_top(kind: Kind) -> f32 {
    match kind { Kind::Generator => 3.3, Kind::Sensor => 2.1, _ => 1.6 }
}

/// Where a hit bar hangs: above the equipment when there is open air over it.
/// Under a low ceiling (a generator in a basement) a bar squeezed between the
/// model and the ceiling would poke into the floor above or be hidden behind
/// the model from anywhere a player stands, so within CEILING_ROOM of the
/// usual spot it hangs beside the equipment on the viewer's side instead.
pub fn bar_anchor(map: peakrunner_core::terrain::MapId, d: &peakrunner_core::equipment::Definition, eye: Vec3) -> Vec3 {
    const CEILING_ROOM: f32 = 1.0;
    let base = d.pos();
    let lift = d.radius + 1.1;
    let top = model_top(d.kind).min(lift - 0.2) + 0.05;
    let span = lift + CEILING_ROOM - top;
    let from = base + Vec3::Y * top;
    let ceiling = peakrunner_core::map_pack::on(map)
        .and_then(|pack| pack.sweep(from, from + Vec3::Y * span, 0.0));
    match ceiling {
        None => base + Vec3::Y * lift,
        Some(_) => {
            let side = (eye - base).with_y(0.0).normalize_or(Vec3::X);
            base + side * (d.radius + 0.4) + Vec3::Y * 1.0
        }
    }
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * a.clamp(0.0, 1.0)) as u8)
}

fn health_color(fraction: f32) -> Color32 {
    if fraction > 0.6 { Color32::from_rgb(96, 214, 120) } else if fraction > 0.3 { Color32::from_rgb(240, 190, 70) } else { Color32::from_rgb(236, 82, 60) }
}

/// Client-only memory for the hit flash: last seen shield + health per
/// object; and who the crosshair names.
#[derive(Default)]
pub struct OverlayState { last: Vec<f32>, flash: Vec<f32>, names: NameWatch }

/// Who the crosshair names, kept `NAME_HOLD` after it leaves them so a name
/// doesn't flicker as the aim crosses a moving body.
#[derive(Default)]
pub struct NameWatch { last: Option<(u32, usize, f32)> }

impl NameWatch {
    /// `under`: the player under the crosshair now (`pointed_at`); `ids`
    /// the players' network IDs (a slot can change hands between frames).
    /// The named player and the name's opacity.
    pub fn named(&mut self, under: Option<usize>, ids: &[u32], dt: f32) -> Option<(usize, f32)> {
        match under {
            Some(i) => self.last = Some((ids.get(i).copied().unwrap_or(0), i, NAME_HOLD)),
            None => if let Some(l) = &mut self.last { l.2 -= dt; },
        }
        self.last = self.last.filter(|&(id, i, left)| left > 0.0 && ids.get(i) == Some(&id));
        self.last.map(|(_, i, left)| (i, (left / NAME_FADE).min(1.0)))
    }
}

impl OverlayState {
    fn note(&mut self, states: &[State], dt: f32) {
        if self.last.len() != states.len() { self.last = states.iter().map(|s| s.health + s.shield).collect(); self.flash = vec![0.0; states.len()]; }
        for (i, s) in states.iter().enumerate() {
            let total = s.health + s.shield;
            self.flash[i] = if total < self.last[i] - 0.01 { 0.25 } else { (self.flash[i] - dt).max(0.0) };
            self.last[i] = total;
        }
    }
}

pub fn draw(ui: &egui::Ui, world: &World, state: &mut OverlayState, dt: f32) {
    let rect = ui.max_rect();
    let (eye, dir, fov) = crate::drawlist::view_camera(world);
    let painter = ui.painter();
    let defs = peakrunner_core::equipment::definitions(world.map);
    state.note(&world.equipment, dt);
    for (i, (d, s)) in defs.iter().zip(&world.equipment).enumerate() {
        if !matches!(d.kind, Kind::Generator | Kind::Turret | Kind::Sensor) { continue; }
        let anchor = bar_anchor(world.map, d, eye);
        let distance = eye.distance(anchor);
        let alpha = fade(distance, HIT_BAR_RANGE);
        if alpha <= 0.0 { continue; }
        let Some(at) = project(eye, dir, fov, rect, anchor) else { continue; };
        if !world.sight_clear(eye, anchor) { continue; }
        let width = (80.0 - distance * 0.25).clamp(48.0, 80.0);
        let flash = state.flash.get(i).copied().unwrap_or(0.0);
        draw_bar(painter, at, width, d, s, alpha, flash, distance < 45.0);
    }
    // The one name: whoever the crosshair is on.
    let ids: Vec<u32> = world.players.iter().map(|p| p.net_id).collect();
    let under = world.players.get(world.player_id).filter(|p| p.alive).and_then(|p| {
        // Over the shoulder the view runs parallel to the look, and the
        // crosshair sits on what the look from the eyes meets.
        let from = if world.third_person && !world.spectating && !world.zoom_active() {
            p.pos + world.render_offset + world.net_camera_offset + Vec3::Y * peakrunner_core::terrain::EYE
        } else { eye };
        pointed_at(world, from, dir, world.zoom_power())
    });
    let named = state.names.named(under, &ids, dt);
    // Carriers last, so their larger marker draws on top.
    let mut order: Vec<usize> = (0..world.players.len()).collect();
    order.sort_by_key(|&i| world.flags.iter().any(|f| f.carrier == Some(i)));
    for i in order {
        let Some((marker, at)) = visible_marker(world, eye, dir, fov, rect, i, named) else { continue; };
        let p = &world.players[i];
        draw_player(painter, p, marker, at, eye.distance(head(p)));
    }
    // The team's waypoint, set at a command station (`placed`).
    if let Some(at) = world.players.get(world.player_id).and_then(|p| world.waypoints[p.team.idx()]) {
        if let Some(pos) = project(eye, dir, fov, rect, at + Vec3::Y * 2.0) {
            let green = Color32::from_rgb(120, 240, 150);
            chevron(painter, pos, 9.0, green, 1.0);
            outlined(painter, pos + egui::vec2(0.0, 16.0), &format!("WAYPOINT {:.0} m", eye.distance(at)), 12.0, green, 1.0);
        }
    }
    // Spots the team marked (the target marker: `firing`), walls or not.
    if let Some(viewer) = world.players.get(world.player_id) {
        for m in world.marks.iter().filter(|m| m.team == viewer.team && m.player.is_none()) {
            let Some(pos) = project(eye, dir, fov, rect, m.pos) else { continue };
            reticle(painter, pos, 9.0, TARGET, 1.0);
            outlined(painter, pos + egui::vec2(0.0, 14.0), &format!("TARGET {:.0} m", eye.distance(m.pos)), 12.0, TARGET, 1.0);
        }
    }
}

/// The target marker's color on screen (its laser's).
const TARGET: Color32 = Color32::from_rgb(255, 158, 46);

/// A target reticle centred on `at`: a ring with four ticks pointing in.
fn reticle(painter: &egui::Painter, at: Pos2, r: f32, color: Color32, alpha: f32) {
    let dark = Stroke::new(3.2, with_alpha(Color32::BLACK, alpha * 0.7));
    let line = Stroke::new(1.6, with_alpha(color, alpha));
    for stroke in [dark, line] {
        painter.circle_stroke(at, r, stroke);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let d = egui::vec2(dx, dy);
            painter.line_segment([at + d * (r + 5.0), at + d * (r - 3.0)], stroke);
        }
    }
    painter.circle_filled(at, 1.8, with_alpha(color, alpha));
}

#[allow(clippy::too_many_arguments)]
fn draw_bar(painter: &egui::Painter, at: Pos2, width: f32, d: &peakrunner_core::equipment::Definition, s: &State, alpha: f32, flash: f32, label: bool) {
    let team = if d.team == 0 { Color32::from_rgb(237, 91, 57) } else { Color32::from_rgb(54, 206, 226) };
    let max_shield = d.max_shield();
    let hull = (s.health / d.max_health()).clamp(0.0, 1.0);
    let x0 = at.x - width * 0.5;
    let mut y = at.y;
    let frame = |y: f32, h: f32| Rect::from_min_size(Pos2::new(x0 - 1.0, y - 1.0), Vec2::new(width + 2.0, h + 2.0));
    if max_shield > 0.0 {
        let r = frame(y, 4.0);
        painter.rect_filled(r, 1.0, with_alpha(Color32::from_black_alpha(170), alpha));
        if s.powered && s.health > 0.0 {
            let f = (s.shield / max_shield).clamp(0.0, 1.0);
            painter.rect_filled(Rect::from_min_size(Pos2::new(x0, y), Vec2::new(width * f, 4.0)), 0.0, with_alpha(SHIELD, alpha));
        } else {
            for k in 0..((width / 6.0) as i32) {
                let x = x0 + k as f32 * 6.0;
                painter.line_segment([Pos2::new(x, y + 2.0), Pos2::new(x + 3.0, y + 2.0)], Stroke::new(1.5, with_alpha(OFFLINE, alpha)));
            }
        }
        y += 6.0;
    }
    let r = frame(y, 7.0);
    painter.rect_filled(r, 1.0, with_alpha(Color32::from_black_alpha(170), alpha));
    painter.rect_stroke(r, 1.0, Stroke::new(1.0, with_alpha(team, alpha)), egui::StrokeKind::Outside);
    painter.rect_filled(Rect::from_min_size(Pos2::new(x0, y), Vec2::new(width * hull, 7.0)), 0.0, with_alpha(health_color(hull), alpha));
    if flash > 0.0 {
        painter.rect_filled(frame(at.y, y + 7.0 - at.y), 1.0, with_alpha(Color32::WHITE, alpha * flash * 2.4));
    }
    let status = if s.health <= 0.0 { Some(("DESTROYED", Color32::from_rgb(236, 82, 60))) }
        else if !s.powered { Some(("OFFLINE", OFFLINE)) } else { None };
    let font = FontId::proportional(11.0);
    if let Some((text, color)) = status {
        painter.text(Pos2::new(at.x, y + 10.0), Align2::CENTER_TOP, text, font.clone(), with_alpha(color, alpha));
    }
    if label {
        let name = match d.kind { Kind::Generator => "GENERATOR", Kind::Turret => "TURRET", _ => "SENSOR" };
        painter.text(Pos2::new(at.x, at.y - 3.0), Align2::CENTER_BOTTOM, name, font, with_alpha(Color32::from_gray(210), alpha * 0.9));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::{Team, World};
    use peakrunner_core::terrain::MapId;

    fn players() -> (World, usize, usize, usize) {
        let mut w = World::new(); w.set_map(MapId::OldHoller); w.start_rift(true);
        let base = w.players[0].clone();
        w.players.truncate(1);
        let mut friend = base.clone(); friend.name = "Friend".into(); friend.team = base.team;
        let mut foe = base.clone(); foe.name = "Foe".into(); foe.team = if base.team == Team::Ember { Team::Glacier } else { Team::Ember };
        w.players[0].name = "Me".into();
        w.players.push(friend); w.players.push(foe);
        (w, 0, 1, 2)
    }

    #[test]
    fn names_are_coloured_by_relation_to_the_viewer() {
        // Red always means enemy, whichever side the viewer plays.
        assert_eq!(name_color(true), TEAMMATE_NAME);
        assert_eq!(name_color(false), ENEMY_NAME);
    }

    /// Players `ahead` metres along the view from high over open ground.
    fn in_line(w: &mut World, me: usize, others: &[(usize, f32)]) -> (Vec3, Vec3) {
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        let eye = Vec3::new(1024.0, ground + 220.0, 1024.0);
        w.players[me].pos = eye - Vec3::Y * 2.35;
        for &(i, ahead) in others { w.players[i].pos = eye + Vec3::X * ahead - Vec3::Y * 1.2; }
        (eye, Vec3::X)
    }

    #[test]
    fn the_crosshair_names_only_the_body_it_is_on() {
        let (mut w, me, friend, foe) = players();
        let (eye, dir) = in_line(&mut w, me, &[(friend, 40.0), (foe, 120.0)]);
        // Both stand on the view's centre line: the nearer one is named.
        assert_eq!(pointed_at(&w, eye, dir, 1.0), Some(friend));
        // Aimed a few metres off either body: nobody.
        let off = (Vec3::X * 40.0 + Vec3::Z * 3.0).normalize();
        assert_eq!(pointed_at(&w, eye, off, 1.0), None, "a miss names no one");
        // A little off a far body still names it.
        let near_miss = (Vec3::X * 120.0 + Vec3::Z * 0.9).normalize();
        w.players[friend].alive = false;
        assert_eq!(pointed_at(&w, eye, near_miss, 1.0), Some(foe), "close enough at range");
        assert_eq!(pointed_at(&w, eye, dir, 1.0), Some(foe), "the dead are never named");
        assert_eq!(pointed_at(&w, eye, -dir, 1.0), None, "nobody behind the view");
        // Never yourself, even standing in your own view.
        w.players[foe].alive = false;
        w.players[me].pos = eye + Vec3::X * 5.0;
        assert_eq!(pointed_at(&w, eye, dir, 1.0), None);
        // Beyond the range by eye, but named through a sight.
        let (mut w, me, _, foe) = players();
        let (eye, dir) = in_line(&mut w, me, &[(foe, NAME_RANGE + 50.0)]);
        assert_eq!(pointed_at(&w, eye, dir, 1.0), None);
        let sight = peakrunner_core::sim::rifles::magnification(peakrunner_core::sim::rifles::ZOOM_FOVS[0]);
        assert_eq!(pointed_at(&w, eye, dir, sight), Some(foe), "named through the sight");
        // Never through the ground.
        let (mut w, me, _, foe) = players();
        let (eye, _) = in_line(&mut w, me, &[]);
        // Just under the hill below, within the range.
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1034.0, 1024.0);
        w.players[foe].pos = Vec3::new(1034.0, ground - 10.0, 1024.0);
        assert!(eye.distance(w.players[foe].pos) < NAME_RANGE);
        let down = (w.players[foe].pos - eye).normalize();
        assert_eq!(pointed_at(&w, eye, down, 1.0), None, "never through terrain");
        w.players[foe].pos.y = ground + 1.0;
        let down = (w.players[foe].pos - eye).normalize();
        assert_eq!(pointed_at(&w, eye, down, 1.0), Some(foe), "but on it");
        let edge = fade(NAME_RANGE - 1.0, NAME_RANGE);
        assert!(edge > 0.0 && edge < 0.2, "overlays fade near the range limit");
    }

    #[test]
    fn a_name_stays_a_moment_after_the_crosshair_leaves() {
        let mut watch = NameWatch::default();
        let ids = [10, 11, 12];
        assert_eq!(watch.named(None, &ids, 0.016), None);
        assert_eq!(watch.named(Some(2), &ids, 0.016), Some((2, 1.0)));
        // Off the body: still named, then fading, then gone.
        let held = watch.named(None, &ids, NAME_HOLD - NAME_FADE);
        assert!(matches!(held, Some((2, a)) if a > 0.99), "{held:?}");
        let fading = watch.named(None, &ids, NAME_FADE * 0.5);
        assert!(matches!(fading, Some((2, a)) if a > 0.3 && a < 0.7), "{fading:?}");
        assert_eq!(watch.named(None, &ids, NAME_FADE), None);
        // A slot that changes hands drops its name at once.
        watch.named(Some(1), &ids, 0.016);
        assert_eq!(watch.named(None, &[10, 99, 12], 0.016), None);
    }

    #[test]
    fn projection_rejects_points_behind_or_off_screen() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0));
        let eye = Vec3::ZERO; let dir = -Vec3::Z;
        let centre = project(eye, dir, 70.0, rect, -Vec3::Z * 50.0).unwrap();
        assert!((centre - rect.center()).length() < 0.5);
        assert!(project(eye, dir, 70.0, rect, Vec3::Z * 50.0).is_none(), "behind the camera");
        assert!(project(eye, dir, 70.0, rect, Vec3::new(500.0, 0.0, -50.0)).is_none(), "off the side of the screen");
        assert!(project(eye, dir, 70.0, rect, Vec3::new(10.0, 0.0, -50.0)).unwrap().x > centre.x, "right is right");
    }

    /// Generator bars in basements: visible from inside the generator room,
    /// never from the floor above. Tower Complex's keel level and Dustreach's
    /// cistern have a low ceiling over the generator, so their bars hang
    /// beside it.
    #[test]
    fn basement_bars_stay_in_their_room() {
        // (map, a viewpoint in the room, a viewpoint on the floor above), in
        // base-local coordinates (team 1 is unrotated; team 0 is turned 180).
        let cases = [(MapId::Cairnhold, Vec3::new(13.2, -1.65, -3.0), Vec3::new(6.8, 1.7, 7.7)),
                     (MapId::Dustreach, Vec3::new(-12.4, -0.2, -15.4), Vec3::new(0.0, 6.7, -5.0)),
                     (MapId::TowerComplex, Vec3::new(0.5, -6.3, -6.8), Vec3::new(-5.2, 1.7, -5.2))];
        for (map, inside, above) in cases {
            let info = peakrunner_core::terrain::info(map);
            let mut w = World::new(); w.set_map(map);
            for d in peakrunner_core::equipment::definitions(map).iter().filter(|d| d.kind == Kind::Generator) {
                let home = if d.team == 0 { info.ember } else { info.glacier };
                let s = if d.team == 0 { -1.0 } else { 1.0 };
                let world = |l: Vec3| Vec3::new(home.x + s * l.x, home.y + l.y, home.z + s * l.z);
                let (inside, above) = (world(inside), world(above));
                let seen = bar_anchor(map, d, inside);
                assert!(w.sight_clear(inside, seen), "{map:?} generator {}: bar {seen} hidden from its room", d.id);
                let from_above = bar_anchor(map, d, above);
                assert!(!w.sight_clear(above, from_above), "{map:?} generator {}: bar {from_above} visible from above", d.id);
            }
        }
    }


    /// Viewer high in open sky over the middle of the map, looking at `target`.
    fn sky_view(w: &mut World, me: usize, target: Vec3) -> (Vec3, Vec3, Rect) {
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        let eye = Vec3::new(1024.0, ground + 220.0, 1024.0);
        w.players[me].pos = eye - Vec3::Y * 2.35;
        (eye, (target - eye).normalize(), Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0)))
    }

    #[test]
    fn enemy_arrows_need_range_screen_and_sight() {
        let (mut w, me, friend, foe) = players();
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        let place = |w: &mut World, i: usize, d: f32| { w.players[i].pos = Vec3::new(1024.0 + d, ground + 220.0, 1024.0) - Vec3::Y * 2.35; };
        place(&mut w, foe, 200.0);
        let target = w.players[foe].pos + Vec3::Y * 2.35;
        let (eye, dir, rect) = sky_view(&mut w, me, target);
        match visible_marker(&w, eye, dir, 70.0, rect, foe, None) {
            Some((Marker::Enemy { alpha, tag }, _)) => { assert_eq!(alpha, 1.0); assert_eq!(tag, 0.0, "no name unless the crosshair names them"); }
            other => panic!("expected a red arrow at 200 m, got {other:?}"),
        }
        place(&mut w, foe, 40.0);
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, foe, None), Some((Marker::Enemy { tag, .. }, _)) if tag == 0.0), "close enemies get the arrow alone");
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, foe, Some((foe, 1.0))), Some((Marker::Enemy { tag, .. }, _)) if tag == 1.0), "and their name under the crosshair");
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, foe, Some((friend, 1.0))), Some((Marker::Enemy { tag, .. }, _)) if tag == 0.0), "only the one named");
        place(&mut w, foe, ENEMY_ARROW_RANGE + 5.0);
        assert!(visible_marker(&w, eye, dir, 70.0, rect, foe, None).is_none(), "no arrow beyond {ENEMY_ARROW_RANGE} m");
        place(&mut w, foe, 100.0);
        assert!(visible_marker(&w, eye, -dir, 70.0, rect, foe, None).is_none(), "no arrow off screen");
        w.players[foe].alive = false;
        assert!(visible_marker(&w, eye, dir, 70.0, rect, foe, None).is_none(), "no arrow on the dead");
        w.players[foe].alive = true;
        // Inside the hill below the viewer: terrain blocks the sight line.
        w.players[foe].pos = Vec3::new(1034.0, ground - 40.0, 1024.0);
        let down = (w.players[foe].pos - eye).normalize();
        assert!(visible_marker(&w, eye, down, 70.0, rect, foe, None).is_none(), "never through terrain");
        assert!(visible_marker(&w, eye, dir, 70.0, rect, me, None).is_none(), "nothing over yourself");
        place(&mut w, friend, 40.0);
        assert!(visible_marker(&w, eye, dir, 70.0, rect, friend, None).is_none(), "teammates carry nothing over their heads");
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, friend, Some((friend, 1.0))), Some((Marker::Name { friendly: true, .. }, _))), "but a blue name under the crosshair, never a red arrow");
    }

    #[test]
    fn football_has_no_arrows_only_the_crosshair_s_name() {
        let (mut w, me, _, foe) = players();
        w.set_mode(peakrunner_core::map_catalog::SupportedMode::Football);
        assert!(w.football());
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        w.players[foe].pos = Vec3::new(1064.0, ground + 220.0, 1024.0) - Vec3::Y * 2.35;
        let target = w.players[foe].pos + Vec3::Y * 2.35;
        let (eye, dir, rect) = sky_view(&mut w, me, target);
        assert!(visible_marker(&w, eye, dir, 70.0, rect, foe, None).is_none(), "no arrow over an enemy in Football");
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, foe, Some((foe, 1.0))), Some((Marker::Name { friendly: false, .. }, _))),
            "a red name alone under the crosshair");
    }

    #[test]
    fn rabbit_arrows_need_sight_both_ways() {
        let (mut w, me, hunter, _) = players();
        w.set_mode(peakrunner_core::map_catalog::SupportedMode::Rabbit);
        w.rabbit.active = true;
        w.rabbit.carrier = Some(me);
        assert!(w.foe(me, hunter) && w.foe(hunter, me));
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        w.players[hunter].pos = Vec3::new(1064.0, ground + 220.0, 1024.0) - Vec3::Y * 2.35;
        let target = w.players[hunter].pos + Vec3::Y * 2.35;
        let (eye, dir, rect) = sky_view(&mut w, me, target);
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, hunter, None), Some((Marker::Enemy { .. }, _))), "a hunter in sight gets the arrow");
        // Inside the hill below the rabbit.
        w.players[hunter].pos = Vec3::new(1034.0, ground - 40.0, 1024.0);
        let down = (w.players[hunter].pos - eye).normalize();
        assert!(visible_marker(&w, eye, down, 70.0, rect, hunter, None).is_none(), "the rabbit sees no hunter through terrain");
        // And the other way: a hunter looking at the rabbit in the hill.
        w.player_id = hunter;
        w.players[hunter].pos = eye - Vec3::Y * 2.35;
        w.players[me].pos = Vec3::new(1034.0, ground - 40.0, 1024.0);
        let down = (w.players[me].pos - eye).normalize();
        assert!(visible_marker(&w, eye, down, 70.0, rect, me, None).is_none(), "nor a hunter the rabbit, outside a ping");
    }

    #[test]
    fn flag_carriers_are_marked_far_and_through_terrain() {
        let (mut w, me, friend, foe) = players();
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        let ours = w.flags.iter().position(|f| f.team == w.players[me].team).unwrap();
        w.flags[ours].carrier = Some(foe);
        // Buried in the hill, far below the viewer: no arrow could show, the carrier marker does.
        w.players[foe].pos = Vec3::new(1034.0, ground - 40.0, 1024.0);
        let target = w.players[foe].pos + Vec3::Y * 2.35;
        let (eye, dir, rect) = sky_view(&mut w, me, target);
        match visible_marker(&w, eye, dir, 70.0, rect, foe, None) {
            Some((Marker::Carrier { flag, ally, alpha, tag }, _)) => { assert_eq!(flag, w.players[me].team); assert!(!ally); assert_eq!(alpha, 1.0); assert_eq!(tag, 0.0, "no name unless named"); }
            other => panic!("expected the carrier marker through terrain, got {other:?}"),
        }
        assert!(visible_marker(&w, eye, -dir, 70.0, rect, foe, None).is_none(), "still only on screen");
        // Far away: beyond any arrow or tag range, inside the carrier range.
        w.players[foe].pos = eye + dir * 700.0;
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, foe, None), Some((Marker::Carrier { .. }, _))));
        w.players[foe].pos = eye + dir * (CARRIER_MARKER_RANGE + 10.0);
        assert!(visible_marker(&w, eye, dir, 70.0, rect, foe, None).is_none());
        // Our own carrier: the ally variant, carrying the enemy's flag.
        let theirs = 1 - ours;
        w.flags[theirs].carrier = Some(friend);
        w.players[friend].pos = eye + dir * 300.0;
        assert!(matches!(visible_marker(&w, eye, dir, 70.0, rect, friend, None), Some((Marker::Carrier { ally: true, flag, .. }, _)) if flag != w.players[me].team));
    }

    #[test]
    fn walls_hide_tags_and_bars() {
        // Line of sight runs through the same map query the server uses.
        let mut w = World::new(); w.set_map(MapId::OldHoller);
        let top = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0) + 200.0;
        assert!(w.sight_clear(Vec3::new(1024.0, top, 1000.0), Vec3::new(1024.0, top, 1050.0)));
        assert!(!w.sight_clear(Vec3::new(1024.0, top, 1024.0), Vec3::new(1024.0, top - 400.0, 1024.0)), "the ground blocks sight");
    }
}
