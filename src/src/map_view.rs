//! The mini-map and the full map (the map key, M): the map's terrain seen
//! from above, with what's on it.
//!
//! The terrain picture (`relief`) is built once per map: the heightfield
//! hillshaded from the northwest over a muted height tint, contour lines (a
//! darker one every fifth), the water, and the structures' footprints (their
//! collision, outlined; specks under about 100 square meters, the scenery,
//! left out). On it go the local player (facing), teammates,
//! enemies the HUD may show (not hidden by a snapshot or a sensor-jamming
//! effect), both flags (a carried one rings its carrier) and the control
//! points in play; the full map adds the bases' equipment, the points' names
//! and the carriers' names.
//!
//! The mini-map turns with the player and shows the ground round them;
//! flags and points out of its view sit on its rim, pointing their way. The
//! full map is north-up over the whole map. Both are drawn only: the mouse
//! and movement keep working under them.
use egui::{Align2, Color32, FontId, Pos2, Stroke, Vec2};
use glam::Vec3;
use peakrunner_core::sim::{Team, World};
use peakrunner_core::terrain::MapId;

const EMBER: Color32 = Color32::from_rgb(237, 91, 57);
const GLACIER: Color32 = Color32::from_rgb(54, 206, 226);
const NEUTRAL: Color32 = Color32::from_rgb(196, 202, 210);
const FG: Color32 = Color32::from_rgb(231, 238, 246);
const MUTED: Color32 = Color32::from_rgb(154, 168, 184);
const PANEL: Color32 = Color32::from_rgba_premultiplied(6, 10, 16, 228);
/// The terrain picture's side, in pixels (a 2 km map: 4 m a pixel).
const RELIEF_SIZE: usize = 512;
/// The mini-map: its radius on screen and how far across it sees.
const MINI_RADIUS: f32 = 84.0;
const MINI_SPAN: f32 = 420.0;

/// The terrain picture of the map in play, kept until the map changes, and
/// whether the full map is open.
#[derive(Default)]
pub struct MapView {
    map: Option<MapId>,
    texture: Option<egui::TextureHandle>,
    pub full: bool,
}

impl MapView {
    fn texture(&mut self, ctx: &egui::Context, map: MapId) -> egui::TextureId {
        if self.map != Some(map) || self.texture.is_none() {
            let image = relief(map, RELIEF_SIZE);
            self.texture = Some(ctx.load_texture("map-relief", image, egui::TextureOptions::LINEAR));
            self.map = Some(map);
        }
        self.texture.as_ref().map(|t| t.id()).unwrap_or_default()
    }
}

fn team_color(team: Team) -> Color32 { if team.idx() == 0 { EMBER } else { GLACIER } }

/// A person's dot: their team's color, or where the mode has no teams
/// (Rabbit, free-for-all) the rabbit's gold and everyone else's steel, as
/// their armor (`player_model::livery`).
fn person_color(world: &World, i: usize, team: Team) -> Color32 {
    if !world.ffa() { team_color(team) }
    else if world.is_rabbit(i) { crate::rabbit_hud::FLAG }
    else { Color32::from_rgb(196, 202, 210) }
}
fn owner_color(owner: Option<u8>) -> Color32 { match owner { Some(0) => EMBER, Some(1) => GLACIER, _ => NEUTRAL } }
fn with_alpha(c: Color32, a: u8) -> Color32 { Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a) }

/// The map's terrain from above, `n` pixels a side: x to the right, z down.
pub fn relief(map: MapId, n: usize) -> egui::ColorImage {
    let size = peakrunner_core::terrain::info(map).size;
    let px = size / n as f32;
    let h: Vec<f32> = (0..n * n).map(|i| {
        let (u, v) = (i % n, i / n);
        peakrunner_core::terrain::height_on(map, (u as f32 + 0.5) * px, (v as f32 + 0.5) * px)
    }).collect();
    // The tint's range: the 2nd to 98th percentile, so a few peaks or pits
    // don't flatten everything else.
    let mut sorted: Vec<f32> = h.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(f32::total_cmp);
    let pick = |q: f32| sorted.get(((sorted.len() as f32 - 1.0) * q) as usize).copied().unwrap_or(0.0);
    let (lo, hi) = (pick(0.02), pick(0.98).max(pick(0.02) + 1.0));
    // A contour every `step` meters: about a dozen across the range.
    let step = [5.0f32, 10.0, 20.0, 25.0, 50.0].into_iter().find(|s| (hi - lo) / s <= 14.0).unwrap_or(100.0);
    let water = water_mask(map, n, px, &h);
    let built = structure_mask(map, n, px, &h);
    let at = |u: usize, v: usize| h[v.min(n - 1) * n + u.min(n - 1)];
    let light = Vec3::new(-1.0, 1.3, -1.0).normalize();
    let mut pixels = Vec::with_capacity(n * n);
    for v in 0..n {
        for u in 0..n {
            let i = v * n + u;
            if u == 0 || v == 0 || u == n - 1 || v == n - 1 {
                // A dark rim: off the map, the mini-map shows this.
                pixels.push(Color32::from_rgb(12, 16, 22));
                continue;
            }
            // Slopes over 4 pixels: broad landforms, not every bump.
            let dx = (at(u + 2, v) - at(u.saturating_sub(2), v)) / (4.0 * px);
            let dz = (at(u, v + 2) - at(u, v.saturating_sub(2))) / (4.0 * px);
            let shade = Vec3::new(-dx, 1.0, -dz).normalize().dot(light).max(0.0);
            let t = ((h[i] - lo) / (hi - lo)).clamp(0.0, 1.0);
            let ramp = |a: [f32; 3], b: [f32; 3], k: f32| [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * k);
            let base = if t < 0.5 { ramp([50.0, 64.0, 60.0], [112.0, 120.0, 104.0], t * 2.0) }
                else { ramp([112.0, 120.0, 104.0], [180.0, 184.0, 174.0], t * 2.0 - 1.0) };
            let mut c = base.map(|x| x * (0.55 + 0.6 * shade));
            let band = |y: f32| (y / step).floor();
            let line = band(h[i]) != band(at(u + 1, v)) || band(h[i]) != band(at(u, v + 1));
            if line {
                let index = (band(h[i].max(at(u + 1, v)).max(at(u, v + 1))) as i64).rem_euclid(5) == 0;
                c = c.map(|x| x * if index { 0.62 } else { 0.8 });
            }
            match water[i] {
                WATER => c = [0, 1, 2].map(|k| c[k] * 0.3 + [36.0, 92.0, 128.0][k] * 0.7),
                LAVA => c = [0, 1, 2].map(|k| c[k] * 0.25 + [214.0, 84.0, 26.0][k] * 0.75),
                _ => {}
            }
            if built[i] {
                let edge = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)].iter()
                    .any(|(a, b)| !built[(v as i32 + b) as usize * n + (u as i32 + a) as usize]);
                c = if edge { [26.0, 30.0, 36.0] } else { [196.0, 204.0, 214.0].map(|x| x * (0.86 + 0.14 * shade)) };
            }
            pixels.push(Color32::from_rgb(c[0] as u8, c[1] as u8, c[2] as u8));
        }
    }
    egui::ColorImage::new([n, n], pixels)
}

/// What covers a pixel of the relief (`water_mask`).
const WATER: u8 = 1;
const LAVA: u8 = 2;

/// Pixels under the map's water or lava: inside a volume's footprint, with
/// the ground below its surface (lava over water where both are), and lava
/// ground wherever it lies.
fn water_mask(map: MapId, n: usize, px: f32, h: &[f32]) -> Vec<u8> {
    let mut out = vec![0; n * n];
    let Some(pack) = peakrunner_core::map_pack::on(map) else { return out };
    if pack.has_lava_ground() {
        for (i, cell) in out.iter_mut().enumerate() {
            let (x, z) = (((i % n) as f32 + 0.5) * px, ((i / n) as f32 + 0.5) * px);
            if pack.lava_cell(x, z).is_some() { *cell = LAVA; }
        }
    }
    for w in &pack.manifest.water_volumes {
        let inside: Box<dyn Fn(f32, f32) -> bool> = match (&w.rect, &w.polygon) {
            (Some([x0, z0, x1, z1]), _) => { let (x0, z0, x1, z1) = (*x0, *z0, *x1, *z1); Box::new(move |x, z| x >= x0 && x <= x1 && z >= z0 && z <= z1) }
            (None, Some(poly)) => { let poly = poly.clone(); Box::new(move |x, z| in_polygon(&poly, x, z)) }
            _ => continue,
        };
        for (i, cell) in out.iter_mut().enumerate() {
            let (x, z) = (((i % n) as f32 + 0.5) * px, ((i / n) as f32 + 0.5) * px);
            if h[i] < w.surface && inside(x, z) { *cell = (*cell).max(if w.lava { LAVA } else { WATER }); }
        }
    }
    out
}

fn in_polygon(poly: &[[f32; 2]], x: f32, z: f32) -> bool {
    let mut inside = false;
    for (k, a) in poly.iter().enumerate() {
        let b = poly[(k + 1) % poly.len()];
        if (a[1] > z) != (b[1] > z) && x < a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) { inside = !inside; }
    }
    inside
}

/// Pixels under the map's structures: its collision triangles that stand
/// above the ground, filled from above (a wall seen edge-on, along its line).
fn structure_mask(map: MapId, n: usize, px: f32, h: &[f32]) -> Vec<bool> {
    let mut out = vec![false; n * n];
    let Some(pack) = peakrunner_core::map_pack::on(map) else { return out };
    let cell = |x: f32, z: f32| -> Option<usize> {
        let (u, v) = ((x / px).floor(), (z / px).floor());
        (u >= 0.0 && v >= 0.0 && (u as usize) < n && (v as usize) < n).then(|| v as usize * n + u as usize)
    };
    for tri in pack.triangles() {
        let top = tri.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let mid = (tri[0] + tri[1] + tri[2]) / 3.0;
        let ground = cell(mid.x, mid.z).map_or(f32::INFINITY, |i| h[i]);
        if top < ground + 0.6 { continue; }
        let p = tri.map(|q| glam::Vec2::new(q.x / px, q.z / px));
        let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
        if area.abs() < 0.25 {
            // Seen edge-on: mark its longest edge.
            let (a, b) = [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])].into_iter()
                .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).unwrap();
            let steps = (a.distance(b).ceil() as usize).max(1) * 2;
            for k in 0..=steps {
                let q = a.lerp(b, k as f32 / steps as f32);
                if let Some(i) = cell(q.x * px, q.y * px) { out[i] = true; }
            }
            continue;
        }
        let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        for v in (lo.y.floor().max(0.0) as usize)..=(hi.y.ceil().max(0.0) as usize).min(n - 1) {
            for u in (lo.x.floor().max(0.0) as usize)..=(hi.x.ceil().max(0.0) as usize).min(n - 1) {
                let q = glam::Vec2::new(u as f32 + 0.5, v as f32 + 0.5);
                let w0 = (p[1] - q).perp_dot(p[2] - q);
                let w1 = (p[2] - q).perp_dot(p[0] - q);
                let w2 = (p[0] - q).perp_dot(p[1] - q);
                if (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0) { out[v * n + u] = true; }
            }
        }
    }
    // Scenery (rocks, trunks, low cover) makes small specks: keep only
    // footprints of about 100 square meters or more.
    let least = (100.0 / (px * px)).ceil() as usize;
    let mut seen = vec![false; n * n];
    for start in 0..n * n {
        if !out[start] || seen[start] { continue; }
        let (mut stack, mut blob) = (vec![start], Vec::new());
        seen[start] = true;
        while let Some(i) = stack.pop() {
            blob.push(i);
            let (u, v) = ((i % n) as i32, (i / n) as i32);
            for (du, dv) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (u + du, v + dv);
                if a < 0 || b < 0 || a >= n as i32 || b >= n as i32 { continue; }
                let j = b as usize * n + a as usize;
                if out[j] && !seen[j] { seen[j] = true; stack.push(j); }
            }
        }
        if blob.len() < least { for i in blob { out[i] = false; } }
    }
    out
}

/// What goes on the maps: everyone the HUD may show, the flags, the points.
struct Marks {
    me: Option<(Vec3, f32, Team)>,
    /// Where, the dot's color (`person_color`), and whether a foe.
    people: Vec<(Vec3, Color32, bool)>,
    /// Each flag: where it is, its color, its carrier's name, at home.
    flags: Vec<(Vec3, Color32, Option<String>, bool)>,
    points: Vec<(Vec3, String, Option<u8>)>,
}

/// `pinged`: the viewer's own Rabbit ping after spawning is running
/// (`rabbit_hud::rabbit_hidden`).
fn marks(world: &World, pinged: bool) -> Marks {
    let me = world.players.get(world.player_id).map(|p| (p.pos, p.yaw, p.team));
    let my_team = me.map(|m| m.2);
    // Enemies by the mode (`World::foe`; a spectator sees both teams'
    // people as each other's).
    let foe = |i: usize, p: &peakrunner_core::sim::Player| if my_team.is_some() { world.foe(world.player_id, i) } else { Some(p.team) != my_team || world.ffa() };
    // In Rabbit an enemy shows only where its arrow would: in range and in
    // sight (`world_overlay::in_sight`), so the rabbit can't watch every
    // hunter on the maps.
    let rabbit = my_team.is_some() && world.mode.rules() == peakrunner_core::modes::Rules::Rabbit;
    let eye = crate::drawlist::view_camera(world).0;
    let people = world.players.iter().enumerate()
        .filter(|(i, p)| *i != world.player_id && p.alive && !p.hidden()
            && (!foe(*i, p) || !peakrunner_core::sim::status::sensor_hidden(&p.effects)
                && (!rabbit || crate::world_overlay::in_sight(world, eye, *i))))
        .map(|(i, p)| (p.pos, person_color(world, i, p.team), foe(i, p))).collect();
    let flags = if world.mode.rules() == peakrunner_core::modes::Rules::Ctf {
        world.flags.iter().map(|f| {
            let carrier = f.carrier.and_then(|c| world.players.get(c)).map(|p| p.name.clone());
            (f.pos, team_color(f.team), carrier, f.carrier.is_none() && f.pos.distance(f.home) < 3.0)
        }).collect()
    } else if world.rabbit_mode() && world.rabbit.active && !(my_team.is_some() && crate::rabbit_hud::rabbit_hidden(world, pinged)) {
        // The rabbit flag (`rabbit`): Rabbit's at home on its stand. On
        // Rabbit's rabbit's back it shows hunters only during a ping, as the
        // HUD does.
        let r = &world.rabbit;
        let carrier = r.carrier.and_then(|c| world.players.get(c)).map(|p| p.name.clone());
        let home = r.carrier.is_none() && world.mode.rules() == peakrunner_core::modes::Rules::Rabbit && r.pos.distance(r.home) < 3.0;
        vec![(r.pos, crate::rabbit_hud::FLAG, carrier, home)]
    } else { Vec::new() };
    let points = world.points.iter().filter(|p| p.active).map(|p| (p.pos, p.name.clone(), p.owner)).collect();
    Marks { me, people, flags, points }
}

fn flag_glyph(painter: &egui::Painter, at: Pos2, color: Color32, scale: f32) {
    let pole = at + Vec2::new(-3.0, 0.0) * scale;
    painter.line_segment([pole + Vec2::new(0.0, 5.0) * scale, pole + Vec2::new(0.0, -8.0) * scale], Stroke::new(1.5 * scale, FG));
    painter.add(egui::Shape::convex_polygon(vec![pole + Vec2::new(0.0, -8.0) * scale, pole + Vec2::new(8.0, -5.0) * scale,
        pole + Vec2::new(0.0, -2.0) * scale], color, Stroke::new(1.0, Color32::BLACK)));
}

fn arrow(painter: &egui::Painter, at: Pos2, dir: Vec2, size: f32, color: Color32) {
    let d = dir.normalized();
    let side = Vec2::new(-d.y, d.x);
    painter.add(egui::Shape::convex_polygon(vec![at + d * size, at - d * size * 0.6 + side * size * 0.7,
        at - d * size * 0.25, at - d * size * 0.6 - side * size * 0.7], color, Stroke::new(1.0, Color32::BLACK)));
}

fn point_glyph(painter: &egui::Painter, at: Pos2, color: Color32, r: f32) {
    painter.add(egui::Shape::convex_polygon(vec![at + Vec2::new(0.0, -r), at + Vec2::new(r, 0.0), at + Vec2::new(0.0, r),
        at + Vec2::new(-r, 0.0)], with_alpha(color, 200), Stroke::new(1.2, FG)));
}

/// The mini-map, its top right corner at `top_right`: centered on you and
/// turning with you (ahead is up).
pub fn draw_minimap(ui: &egui::Ui, world: &World, view: &mut MapView, top_right: Pos2, pinged: bool) {
    let Some((pos, yaw, team)) = world.players.get(world.player_id).map(|p| (p.pos, p.yaw, p.team)) else { return };
    let texture = view.texture(ui.ctx(), world.map);
    let size = peakrunner_core::terrain::info(world.map).size;
    let r = MINI_RADIUS;
    let center = top_right + Vec2::new(-r, r);
    let mpp = MINI_SPAN / (2.0 * r);
    let forward = glam::Vec2::new(-yaw.sin(), -yaw.cos());
    let right = glam::Vec2::new(-forward.y, forward.x);
    // World (x, z) to the screen, and back.
    let to_screen = |p: Vec3| {
        let d = glam::Vec2::new(p.x - pos.x, p.z - pos.z);
        center + Vec2::new(d.dot(right), -d.dot(forward)) / mpp
    };
    let painter = ui.painter();
    painter.circle_filled(center, r + 3.0, PANEL);
    // The terrain: a fan of triangles over the disc, its texture turned.
    let mut mesh = egui::Mesh::with_texture(texture);
    let segments = 56;
    let uv = |s: Vec2| {
        let w = glam::Vec2::new(pos.x, pos.z) + right * (s.x * mpp) + forward * (-s.y * mpp);
        Pos2::new(w.x / size, w.y / size)
    };
    mesh.vertices.push(egui::epaint::Vertex { pos: center, uv: uv(Vec2::ZERO), color: Color32::WHITE });
    for k in 0..=segments {
        let a = k as f32 / segments as f32 * std::f32::consts::TAU;
        let s = Vec2::new(a.cos(), a.sin()) * r;
        mesh.vertices.push(egui::epaint::Vertex { pos: center + s, uv: uv(s), color: Color32::WHITE });
    }
    for k in 0..segments as u32 { mesh.indices.extend_from_slice(&[0, k + 1, k + 2]); }
    painter.add(egui::Shape::mesh(mesh));
    painter.circle_stroke(center, r, Stroke::new(1.5, with_alpha(FG, 90)));
    let marks = marks(world, pinged);
    // On the rim when out of view.
    let pinned = |p: Pos2| {
        let d = p - center;
        if d.length() > r - 7.0 { (center + d.normalized() * (r - 7.0), true) } else { (p, false) }
    };
    for (p, color, enemy) in &marks.people {
        let s = to_screen(*p);
        if (s - center).length() > r - 3.0 { continue; }
        if *enemy { painter.circle(s, 3.0, *color, Stroke::new(1.0, Color32::BLACK)); }
        else { painter.circle_filled(s, 3.0, *color); }
    }
    for (p, _, owner) in &marks.points {
        let (s, _) = pinned(to_screen(*p));
        point_glyph(painter, s, owner_color(*owner), 5.0);
    }
    for (p, color, carrier, _) in &marks.flags {
        let (s, edge) = pinned(to_screen(*p));
        if carrier.is_some() && !edge { painter.circle_stroke(s, 7.0, Stroke::new(1.5, *color)); }
        flag_glyph(painter, s, *color, 1.0);
    }
    // You, in the middle, facing up.
    arrow(painter, center, Vec2::new(0.0, -1.0), 7.0, team_color(team));
}

/// The full map over the middle of the screen, north-up, and the key that
/// closes it.
pub fn draw_full(ui: &egui::Ui, world: &World, view: &mut MapView, key: &str, pinged: bool) {
    let texture = view.texture(ui.ctx(), world.map);
    let info = peakrunner_core::terrain::info(world.map);
    let screen = ui.max_rect();
    let side = (screen.width().min(screen.height()) * 0.8).max(200.0);
    let rect = egui::Rect::from_center_size(screen.center() + Vec2::new(0.0, 10.0), Vec2::splat(side));
    let painter = ui.painter();
    painter.rect_filled(rect.expand(12.0), 6.0, PANEL);
    painter.image(texture, rect, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    painter.text(rect.left_top() + Vec2::new(0.0, -18.0), Align2::LEFT_BOTTOM, info.name.to_uppercase(), FontId::proportional(16.0), FG);
    painter.text(rect.right_top() + Vec2::new(0.0, -18.0), Align2::RIGHT_BOTTOM, format!("{key} close"), FontId::proportional(13.0), MUTED);
    let scale = side / info.size;
    let to_screen = |p: Vec3| rect.min + Vec2::new(p.x, p.z) * scale;
    // A faint grid every 256 m, for calling out places.
    let mut g = 256.0;
    while g < info.size {
        let k = g * scale;
        painter.line_segment([rect.min + Vec2::new(k, 0.0), rect.min + Vec2::new(k, side)], Stroke::new(1.0, Color32::from_white_alpha(18)));
        painter.line_segment([rect.min + Vec2::new(0.0, k), rect.min + Vec2::new(side, k)], Stroke::new(1.0, Color32::from_white_alpha(18)));
        g += 256.0;
    }
    // The bases' equipment: generators, turrets, stations; gray when down.
    use peakrunner_core::equipment::Kind;
    for (d, s) in peakrunner_core::equipment::definitions(world.map).iter().zip(&world.equipment) {
        let at = to_screen(d.pos());
        let color = if s.health <= 0.0 || s.offline { Color32::from_gray(110) } else { owner_color(Some(d.team)) };
        match d.kind {
            Kind::Generator => {
                painter.rect(egui::Rect::from_center_size(at, Vec2::splat(9.0)), 1.0, with_alpha(color, 220), Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
                painter.text(at, Align2::CENTER_CENTER, "G", FontId::monospace(8.0), Color32::BLACK);
            }
            Kind::Turret => {
                painter.add(egui::Shape::convex_polygon(vec![at + Vec2::new(0.0, -5.0), at + Vec2::new(4.5, 3.5), at + Vec2::new(-4.5, 3.5)],
                    with_alpha(color, 220), Stroke::new(1.0, Color32::BLACK)));
            }
            Kind::Inventory => { painter.rect_filled(egui::Rect::from_center_size(at, Vec2::splat(5.0)), 0.0, with_alpha(color, 200)); }
            Kind::Sensor => { painter.circle_stroke(at, 3.5, Stroke::new(1.5, with_alpha(color, 220))); }
            Kind::Repair => {}
        }
    }
    let marks = marks(world, pinged);
    for (p, name, owner) in &marks.points {
        let at = to_screen(*p);
        point_glyph(painter, at, owner_color(*owner), 8.0);
        painter.text(at + Vec2::new(0.0, 11.0), Align2::CENTER_TOP, name, FontId::proportional(12.0), FG);
    }
    for (p, color, enemy) in &marks.people {
        let at = to_screen(*p);
        if *enemy { painter.circle(at, 4.0, *color, Stroke::new(1.0, Color32::BLACK)); }
        else { painter.circle_filled(at, 4.0, *color); }
    }
    for (p, color, carrier, home) in &marks.flags {
        let at = to_screen(*p);
        if carrier.is_some() { painter.circle_stroke(at, 9.0, Stroke::new(2.0, *color)); }
        flag_glyph(painter, at, *color, 1.5);
        let what = match (carrier, home) {
            (Some(name), _) => format!("carried by {name}"),
            (None, true) => String::new(),
            (None, false) => "dropped".to_string(),
        };
        if !what.is_empty() { painter.text(at + Vec2::new(8.0, 4.0), Align2::LEFT_TOP, what, FontId::proportional(12.0), FG); }
    }
    if let Some((p, yaw, team)) = marks.me {
        arrow(painter, to_screen(p), Vec2::new(-yaw.sin(), -yaw.cos()), 9.0, team_color(team));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rabbit's maps show the rabbit only the hunters in range and in
    /// sight; other modes show every enemy not jammed.
    #[test]
    fn rabbit_maps_show_only_hunters_in_sight() {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_rift(true);
        let base = w.players[0].clone();
        w.players.truncate(1);
        for (name, team) in [("Near", peakrunner_core::sim::Team::Ember), ("Far", peakrunner_core::sim::Team::Glacier)] {
            let mut p = base.clone(); p.name = name.into(); p.team = team; w.players.push(p);
        }
        let ground = peakrunner_core::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        // The viewer high over the middle; one player in the open below, one
        // inside the hill.
        let place = |w: &mut World| {
            w.players[0].pos = Vec3::new(1024.0, ground + 200.0, 1024.0);
            w.players[1].pos = Vec3::new(1060.0, ground + 160.0, 1024.0);
            w.players[2].pos = Vec3::new(1030.0, ground - 40.0, 1024.0);
            for p in &mut w.players { p.alive = true; }
        };
        let shown = |w: &World| marks(w, false).people.iter().filter(|d| d.2).count();
        w.set_mode(peakrunner_core::map_catalog::SupportedMode::TeamDeathmatch);
        place(&mut w);
        assert_eq!(shown(&w), 1, "the other team's one player, wherever");
        w.set_mode(peakrunner_core::map_catalog::SupportedMode::Rabbit);
        place(&mut w);
        w.rabbit.active = true;
        w.rabbit.carrier = Some(0);
        assert!(w.foe(0, 1) && w.foe(0, 2));
        assert_eq!(shown(&w), 1, "only the hunter in sight");
        w.players[1].pos = Vec3::new(1024.0 + (crate::world_overlay::ENEMY_ARROW_RANGE + 10.0), ground + 200.0, 1024.0);
        assert_eq!(shown(&w), 0, "and none past arrow range");
        // The flag on the rabbit's back: always on the rabbit's own maps; on
        // a hunter's only during a ping, the server's or their own.
        assert_eq!(marks(&w, false).flags.len(), 1);
        w.player_id = 1;
        assert!(marks(&w, false).flags.is_empty(), "hidden between pings");
        assert_eq!(marks(&w, true).flags.len(), 1, "their own ping after spawning");
        w.rabbit.ping = 3.0;
        assert_eq!(marks(&w, false).flags.len(), 1, "a ping");
        w.rabbit.ping = 0.0;
        w.rabbit.carrier = None;
        assert_eq!(marks(&w, false).flags.len(), 1, "a loose flag always");
    }

    /// The relief shows the ground, the water and the structures where they
    /// are: Old Holler's ravine holds water, and its bases are built over.
    #[test]
    fn the_relief_shows_water_and_structures_where_they_are() {
        let map = MapId::OldHoller;
        let n = 256;
        let image = relief(map, n);
        assert_eq!(image.size, [n, n]);
        let size = peakrunner_core::terrain::info(map).size;
        let at = |x: f32, z: f32| image.pixels[(z / size * n as f32) as usize * n + (x / size * n as f32) as usize];
        // The flooded ravine's middle, west of the bridge: bluish.
        let w = at(900.0, 950.0);
        assert!(w.b() > w.r() + 20, "the ravine reads as water: {w:?}");
        // Ember's hall roof (1160, 480): the light structure tone.
        let roof = at(1170.0, 470.0);
        assert!(roof.r() > 150 && roof.g() > 150, "the base reads as built: {roof:?}");
        // Plain ground somewhere else is neither.
        let ground = at(600.0, 300.0);
        assert!(!(ground.r() > 180 && ground.g() > 185 && ground.b() > 195), "{ground:?}");
    }
}
