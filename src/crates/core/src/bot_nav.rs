//! Bot navigation: a waypoint graph built once per map from the collision mesh
//! and terrain, and A* over it. Links are walk (floors and ramps), ski
//! (terrain), jet (climbs sized from the real jet energy and thrust numbers)
//! and drop. Server-side only; nothing here is networked.


/// Stadium maps only host Football, so their bots plan with its armor.
fn plan_armor(map: MapId) -> crate::sim::Armor {
    if crate::sim::football::has_field(map) { crate::defs::defs().armors.football } else { crate::sim::standard_armor() }
}
/// Whether jet links on `map` are planned with steering paid out of the lift
/// (the skier model); Football's own movement keeps its plans as they were.
fn plan_lift(map: MapId) -> bool { !crate::sim::football::has_field(map) }
use crate::terrain::{MapId, PLAYER_RADIUS};
use glam::Vec3;
use std::collections::{BinaryHeap, HashMap};

/// Waypoints sit this far above their floor: a standing player's centre.
pub(crate) const STAND: f32 = PLAYER_RADIUS + 0.1;
const TERRAIN_STRIDE: f32 = 16.0;
const FLOOR_STRIDE: f32 = 3.0;
const CELL: f32 = 16.0;
/// Walkable surfaces: steeper faces are walls.
const WALKABLE_NY: f32 = 0.75;
/// Longest drop a bot takes on purpose. Landing damage is capped at 11
/// health, so even a long fall off a floating deck is survivable.
const MAX_DROP: f32 = 160.0;
/// Fraction of a full tank a bot plans a climb with; it waits to recharge.
pub(crate) const JET_PLAN_ENERGY: f32 = 0.85;
/// Fastest run-in speed a ground launch plans with, m/s: a skier off a
/// downhill run, well under the speeds players reach.
const RUN_IN_MAX: f32 = 38.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link { Walk, Ski, Jet, Drop }

#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge { pub to: u32, pub cost: f32, pub link: Link }

pub(crate) struct NavGraph {
    pub nodes: Vec<Vec3>,
    pub terrain: Vec<bool>,
    pub edges: Vec<Vec<Edge>>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Waypoints in tiny pockets (fixture tops, sealed nooks) that positions
    /// never snap to.
    pocket: Vec<bool>,
}

fn cell(v: f32) -> i32 { (v / CELL).floor() as i32 }

/// The graph for `map`, building it here if needed (tests, tools). `None` for
/// maps without a pack.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn graph(map: MapId) -> Option<&'static NavGraph> {
    if map == MapId::Valley { return None; }
    crate::catalog::entry(map).nav.get_or_init(|| build(map)).as_ref()
}

/// The graph for `map` if it's ready, never blocking the simulation: the
/// first request starts a background build (one to five seconds per map), and
/// bots steer straight at their goals until it lands. Web builds have no
/// threads, so bots there keep the straight-line steering.
pub(crate) fn ready(map: MapId) -> Option<&'static NavGraph> {
    if map == MapId::Valley { return None; }
    let entry = crate::catalog::entry(map);
    if let Some(g) = entry.nav.get() { return g.as_ref(); }
    #[cfg(not(target_arch = "wasm32"))]
    if !entry.nav_started.swap(true, std::sync::atomic::Ordering::AcqRel) {
        std::thread::spawn(move || { entry.nav.get_or_init(|| build(map)); });
    }
    None
}

/// Build the graphs of `maps` one after another on a thread of their own,
/// skipping any already built or under way: a bot server calls it as it
/// starts, so no map's first round builds its graph during play (Ashveil's
/// took most of a minute). Bots steer straight until their map's is ready.
pub fn prepare(maps: &[MapId]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let maps: Vec<MapId> = maps.iter().copied().filter(|&m| m != MapId::Valley).collect();
        if maps.is_empty() { return; }
        let _ = std::thread::Builder::new().name("bot-nav".into()).spawn(move || {
            for map in maps {
                let entry = crate::catalog::entry(map);
                if !entry.nav_started.swap(true, std::sync::atomic::Ordering::AcqRel) {
                    entry.nav.get_or_init(|| build(map));
                }
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = maps;
}

/// Height a full-tank jet reaches from standstill, and whether it can clear
/// `dy` while also covering `dx`. Mirrors the jet branch of the movement step
/// (`jet_arc`).
#[cfg(test)]
pub(crate) fn jet_can_reach(dx: f32, dy: f32, energy: f32) -> bool { jet_arc(dx, dy, energy, 0.0, crate::sim::standard_armor(), true).is_some() }

/// How far over its target a coast must carry a jet climb before it is done
/// climbing; the climb rate it plans a long climb's arrival with (m/s); the
/// speed gap (m/s) at which it pushes with its whole push; and the rise
/// (m/s²) a climber keeps while it pushes.
const LEAN_CLEAR: f32 = 2.5;
const CLIMB_RATE: f32 = 12.0;
const LEAN_GAIN: f32 = 4.0;
const CLIMB_ACCEL: f32 = 10.0;

/// Whether a jet climb `below` metres under its target (negative: above
/// it), rising at `vy`, still has to climb: a coast alone falls short of it.
pub(crate) fn climbing(below: f32, vy: f32) -> bool {
    vy.max(0.0).powi(2) / (2.0 * crate::defs::movement().gravity) < below + LEAN_CLEAR
}

/// The speed toward its target that a jet climb `below` metres under it and
/// `across` metres short of it, rising at `vy`, wants: enough to arrive over
/// it as the climb ends, at most the speed the push adds, and that once it is
/// done climbing. Steering is paid for out of the jet's lift (the movement
/// step), so a climber pushes only for the speed it needs: a steep climb goes
/// up nearly straight, drifting in, while a shallow one leans in at once.
pub(crate) fn lean_speed(below: f32, across: f32, vy: f32, armor: &crate::sim::Armor) -> f32 {
    if !climbing(below, vy) { return armor.air_cap; }
    let rise = (armor.jet - crate::defs::movement().gravity).max(1.0);
    let up = below.max(0.0);
    let t = (2.0 * up / rise).sqrt().max(up / CLIMB_RATE).max(0.1);
    (across.max(0.0) / t).min(armor.air_cap)
}

/// The share of its push a jet climber spends closing a speed `gap` (m/s)
/// to `lean_speed`: in proportion, none within the movement keys' dead zone,
/// and while it is still `climbing`, no more than leaves it the lift to rise.
pub(crate) fn lean_share(gap: f32, climbing: bool, armor: &crate::sim::Armor) -> f32 {
    let mut share = (gap / LEAN_GAIN).min(1.0);
    if climbing {
        share = share.min((armor.jet - crate::defs::movement().gravity - CLIMB_ACCEL) / armor.side.max(0.1));
    }
    if share > 0.2 { share } else { 0.0 }
}

/// The flight a bot flies for a jet link: arriving at the launch point
/// already moving at `v0` horizontally toward the target (a skier off a
/// downhill run), it holds jet and steers at the target for the speed it
/// wants (`lean_speed`), until it is over it with height to spare. Returns the path as (along, up) points, or `None`
/// if a tank can't get it there. Mirrors the jet branch of the movement
/// step: the push along the keys fades near the cap and, with `lift`, takes
/// its share of the thrust from the lift, which fades near the climb cap.
/// Without it the plan pushes all the way, full lift and push together.
pub(crate) fn jet_arc(dx: f32, dy: f32, energy: f32, v0: f32, armor: crate::sim::Armor, lift: bool) -> Option<Vec<(f32, f32)>> {
    let dt = 1.0 / 60.0;
    let (mut x, mut y, mut vx, mut vy, mut e) = (0.0f32, 0.0f32, v0, 0.0f32, energy);
    let mut reached_x = dx <= 0.0;
    let mut out = vec![(0.0, 0.0)];
    for step in 0..(15.0 / dt) as u32 {
        let jet = e >= armor.min_jet;
        vy -= crate::sim::gravity_for_speed(vx) * dt;
        if jet {
            // Thrust only adds speed up to the cap; it never trims a faster run-in.
            let share = if reached_x { 0.0 } else if !lift { 1.0 }
                else { lean_share(lean_speed(dy - y, dx - x, vy, &armor) - vx, climbing(dy - y, vy), &armor) };
            let push = if share > 0.0 && vx < armor.air_cap {
                let push = armor.side * crate::sim::jet_falloff_to(vx, armor.air_cap) * share;
                if lift { push.min(armor.jet) } else { push }
            } else { 0.0 };
            let fade = if armor.fade_up { crate::sim::jet_falloff(vy.max(0.0)) } else { 1.0 };
            let up = if lift { armor.jet - push } else { armor.jet };
            vy += up * fade * dt;
            vx = (vx + push * dt).min(armor.air_cap.max(vx));
            e -= armor.drain * dt;
        }
        x = (x + vx * dt).min(dx.max(x));
        y += vy * dt;
        if step % 12 == 0 { out.push((x, y)); }
        if x >= dx { reached_x = true; }
        // Over the target, with height to spare: it can stop pushing and land.
        if reached_x && y >= dy + 1.5 { out.push((dx, y)); out.push((dx, dy)); return Some(out); }
        // Falling below the target before getting over it: too far.
        if vy < 0.0 && !jet && y < dy + 1.5 { return None; }
    }
    None
}

fn clear(map: MapId, pack: &crate::map_pack::MapPack, a: Vec3, b: Vec3, radius: f32) -> bool {
    pack.sweep(a, b, radius).is_none() && crate::terrain::segment_hit(map, a, b, 0.2).is_none()
}

fn body_clear(pack: &crate::map_pack::MapPack, a: Vec3, b: Vec3) -> bool {
    pack.body_sweep(a, b).is_none()
}

/// A walkable straight line: the body's four spheres, slightly slimmed so a
/// waypoint that starts grazing a wall or cover block isn't read as blocked,
/// while a real wall between the ends still is. Tested both ways.
fn walk_clear(pack: &crate::map_pack::MapPack, a: Vec3, b: Vec3) -> bool {
    walk_line(pack, a, b) || hop_line(pack, a, b)
}

/// A hop: a small step, plinth or lip crossed with a jump. Clear if a body
/// lifted by a jump's height gets across and can rise and settle at the ends.
fn hop_line(pack: &crate::map_pack::MapPack, a: Vec3, b: Vec3) -> bool {
    const HOP: f32 = 1.4;
    if (b.y - a.y).abs() > HOP - 0.2 { return false; }
    let up = Vec3::Y * HOP;
    walk_line(pack, a, a + up) && walk_line(pack, a + up, b + up) && walk_line(pack, b + up, b)
}

/// A waypoint pressed into a wall: the slim body, walked in from half a
/// metre away on any side, stops before reaching it. Sweeps ignore contacts
/// that start overlapped, so a plain sweep from the point can't see this.
fn embedded(pack: &crate::map_pack::MapPack, p: Vec3) -> bool {
    let slim = PLAYER_RADIUS * 0.8;
    (0..8).any(|k| {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec3::new(a.cos(), 0.0, a.sin()) * 0.5;
        [0.0, 0.9].iter().any(|&h| {
            let at = p + Vec3::Y * h;
            pack.sweep(at - d, at, slim).is_some()
        })
    })
}

fn walk_line(pack: &crate::map_pack::MapPack, a: Vec3, b: Vec3) -> bool {
    let slim = PLAYER_RADIUS * 0.8;
    [(a, b), (b, a)].iter().all(|&(from, to)| {
        [0.0, 0.5, 1.0, crate::terrain::EYE].iter().all(|&h| {
            let rise = Vec3::Y * h;
            pack.sweep(from + rise, to + rise, slim).is_none()
        })
    })
}

fn build(map: MapId) -> Option<NavGraph> {
    let pack = crate::map_pack::on(map)?;
    let size = crate::terrain::info(map).size;
    let mut nodes: Vec<Vec3> = Vec::new();
    let mut terrain: Vec<bool> = Vec::new();

    // Terrain waypoints on a coarse grid, skipping cut cells, lava (it
    // burns: bots go round it) and anything standing inside a structure.
    let n = (size / TERRAIN_STRIDE) as i32;
    let mut tgrid: HashMap<(i32, i32), u32> = HashMap::new();
    let lava: Vec<&crate::water::Volume> = crate::water::volumes(map).iter().filter(|v| v.lava).collect();
    for iz in 1..n {
        for ix in 1..n {
            let (x, z) = (ix as f32 * TERRAIN_STRIDE, iz as f32 * TERRAIN_STRIDE);
            if pack.hole(x, z) || lava.iter().any(|v| v.covers(x, z)) || pack.lava_cell(x, z).is_some() { continue; }
            let p = Vec3::new(x, crate::terrain::height_on(map, x, z) + STAND, z);
            if !body_clear(pack, p, p + Vec3::Y * 0.01) { continue; }
            tgrid.insert((ix, iz), nodes.len() as u32);
            nodes.push(p);
            terrain.push(true);
        }
    }

    // Structure floors: every walkable up-facing triangle sampled on a world
    // grid, keeping points where a body fits and the surface is the support.
    // Two interleaved lattices (the second offset half a step), so narrow
    // gaps such as baffled doors get a waypoint however the grid falls.
    let mut fkey: HashMap<(i32, i32, i32, u8), u32> = HashMap::new();
    for (lattice, off) in [(0u8, 0.0f32), (1, FLOOR_STRIDE * 0.5)] {
        for tri in pack.triangles() {
            let nrm = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
            let len = nrm.length();
            if len < 1e-4 { continue; }
            let nrm = nrm / len;
            let nrm = if nrm.y < 0.0 { -nrm } else { nrm };
            if nrm.y < WALKABLE_NY { continue; }
            let lo = tri[0].min(tri[1]).min(tri[2]);
            let hi = tri[0].max(tri[1]).max(tri[2]);
            for gx in ((lo.x - off) / FLOOR_STRIDE).ceil() as i32..=((hi.x - off) / FLOOR_STRIDE).floor() as i32 {
                for gz in ((lo.z - off) / FLOOR_STRIDE).ceil() as i32..=((hi.z - off) / FLOOR_STRIDE).floor() as i32 {
                    let (x, z) = (gx as f32 * FLOOR_STRIDE + off, gz as f32 * FLOOR_STRIDE + off);
                    if !inside_xz(x, z, tri) { continue; }
                    let y = tri[0].y - (nrm.x * (x - tri[0].x) + nrm.z * (z - tri[0].z)) / nrm.y;
                    let key = (gx, gz, (y / 2.0).round() as i32, lattice);
                    if fkey.contains_key(&key) { continue; }
                    let p = Vec3::new(x, y + STAND, z);
                    // Buried under terrain that isn't cut away.
                    if !pack.hole(x, z) && y < crate::terrain::height_on(map, x, z) - 0.5 { continue; }
                    match pack.floor(p) { Some((fy, _)) if (fy - y).abs() < 0.3 => {}, _ => continue }
                    // Headroom above the surface itself: rejects slab undersides
                    // and faces buried inside solids.
                    let base = Vec3::new(x, y + 0.03, z);
                    if pack.sweep(base, base + Vec3::Y * 2.4, 0.0).is_some() { continue; }
                    if !body_clear(pack, p, p + Vec3::Y * 0.01) { continue; }
                    if embedded(pack, p) { continue; }
                    fkey.insert(key, nodes.len() as u32);
                    nodes.push(p);
                    terrain.push(false);
                }
            }
        }
    }

    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, p) in nodes.iter().enumerate() {
        grid.entry((cell(p.x), cell(p.z))).or_default().push(i as u32);
    }
    let mut g = NavGraph { nodes, terrain, edges: Vec::new(), grid, pocket: Vec::new() };
    g.edges = vec![Vec::new(); g.nodes.len()];
    let add = |edges: &mut Vec<Vec<Edge>>, a: u32, b: u32, link: Link, cost: f32| {
        if !edges[a as usize].iter().any(|e| e.to == b) {
            edges[a as usize].push(Edge { to: b, cost, link });
        }
    };

    // Ski links between neighbouring terrain waypoints: any descent, climbs
    // up to about 50 degrees, with no ridge or structure between them. In
    // key order, like the floors below: hash order differs per process, and
    // with it each waypoint's edge order and so the routes bots pick.
    let mut tkeys: Vec<_> = tgrid.iter().map(|(k, v)| (*k, *v)).collect();
    tkeys.sort();
    for &((ix, iz), a) in &tkeys {
        for (dx, dz) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
            let Some(&b) = tgrid.get(&(ix + dx, iz + dz)) else { continue };
            let (pa, pb) = (g.nodes[a as usize], g.nodes[b as usize]);
            let d = pa.distance(pb);
            let lift = Vec3::Y * 0.9;
            if !clear(map, pack, pa + lift, pb + lift, 0.5) { continue; }
            let rise = pb.y - pa.y;
            let flat = Vec3::new(pb.x - pa.x, 0.0, pb.z - pa.z).length();
            let full = crate::defs::movement().energy_max * JET_PLAN_ENERGY;
            // Too steep to ski up: jet up it if a tank covers the climb.
            let reach = |dy: f32| jet_arc(flat, dy, full, 0.0, plan_armor(map), plan_lift(map)).is_some();
            if rise <= flat * 1.19 { add(&mut g.edges, a, b, Link::Ski, d); }
            else if reach(rise) { add(&mut g.edges, a, b, Link::Jet, 4.0 + d * 1.5); }
            if -rise <= flat * 1.19 { add(&mut g.edges, b, a, Link::Ski, d); }
            else if reach(-rise) { add(&mut g.edges, b, a, Link::Jet, 4.0 + d * 1.5); }
        }
    }

    // Walk links between neighbouring floor waypoints (ramps up to ~32 deg).
    let mut boundary = vec![false; g.nodes.len()];
    let mut fkeys: Vec<_> = fkey.iter().map(|(k, v)| (*k, *v)).collect();
    fkeys.sort();
    for &((gx, gz, lvl, lat), a) in &fkeys {
        let pa = g.nodes[a as usize];
        // Cross-lattice neighbours: the four half-step diagonals.
        let (ox, oz) = if lat == 0 { (-1, -1) } else { (0, 0) };
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            for dl in -2..=2 {
                let Some(&b) = fkey.get(&(gx + ox + dx, gz + oz + dz, lvl + dl, 1 - lat)) else { continue };
                let pb = g.nodes[b as usize];
                let flat = Vec3::new(pb.x - pa.x, 0.0, pb.z - pa.z).length();
                if (pb.y - pa.y).abs() > flat * 0.62 + 0.3 { continue; }
                let lift = Vec3::Y * 0.35;
                if walk_clear(pack, pa + lift, pb + lift) {
                    add(&mut g.edges, a, b, Link::Walk, pa.distance(pb) * 1.1);
                    add(&mut g.edges, b, a, Link::Walk, pa.distance(pb) * 1.1);
                }
            }
        }
        let mut around = 0;
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 { continue; }
                let mut found = false;
                for dl in -2..=2 {
                    let Some(&b) = fkey.get(&(gx + dx, gz + dz, lvl + dl, lat)) else { continue };
                    let pb = g.nodes[b as usize];
                    let flat = Vec3::new(pb.x - pa.x, 0.0, pb.z - pa.z).length();
                    if (pb.y - pa.y).abs() > flat * 0.62 + 0.3 { continue; }
                    found = true;
                    let lift = Vec3::Y * 0.35;
                    if walk_clear(pack, pa + lift, pb + lift) {
                        add(&mut g.edges, a, b, Link::Walk, pa.distance(pb) * 1.1);
                        add(&mut g.edges, b, a, Link::Walk, pa.distance(pb) * 1.1);
                    }
                }
                if found { around += 1; }
            }
        }
        boundary[a as usize] = around < 8;
    }

    let near = |g: &NavGraph, p: Vec3, r: f32| -> Vec<u32> {
        let mut out = Vec::new();
        for cx in cell(p.x - r)..=cell(p.x + r) {
            for cz in cell(p.z - r)..=cell(p.z + r) {
                if let Some(ids) = g.grid.get(&(cx, cz)) {
                    for &i in ids {
                        let q = g.nodes[i as usize];
                        if Vec3::new(q.x - p.x, 0.0, q.z - p.z).length() <= r { out.push(i); }
                    }
                }
            }
        }
        out
    };

    // Floor edges step onto nearby terrain and floors (doors, ramp feet,
    // bunker mouths). Candidates are independent, so they're checked in
    // parallel and merged in order: the graph comes out the same every time.
    let edge_nodes: Vec<u32> = (0..g.nodes.len() as u32).filter(|&a| !g.terrain[a as usize] && boundary[a as usize]).collect();
    let steps = par_map(&edge_nodes, |&a| {
        let pa = g.nodes[a as usize];
        let mut out = Vec::new();
        let mut floors = 0;
        let mut cands = near(&g, pa, TERRAIN_STRIDE * 1.2);
        cands.sort_by(|x, y| g.nodes[*x as usize].distance(pa).total_cmp(&g.nodes[*y as usize].distance(pa)));
        for b in cands {
            let pb = g.nodes[b as usize];
            // Other floors only within a short step, and only a few: doorways
            // narrower than the waypoint grid still get a link through them.
            if !g.terrain[b as usize] {
                if b == a || pa.distance(pb) > 9.0 || floors >= 10 || g.edges[a as usize].iter().any(|e| e.to == b) { continue; }
                floors += 1;
            }
            let flat = Vec3::new(pb.x - pa.x, 0.0, pb.z - pa.z).length();
            // Floors link at ramp slopes; coarse terrain waypoints get more slack.
            let slope = if g.terrain[b as usize] { 0.9 } else { 0.62 };
            if (pb.y - pa.y).abs() > flat * slope + 0.4 { continue; }
            let lift = Vec3::Y * 0.5;
            if walk_clear(pack, pa + lift, pb + lift) && crate::terrain::segment_hit(map, pa + lift, pb + lift, 0.1).is_none() {
                out.push((a, b, pa.distance(pb) * 1.1, Link::Walk));
                out.push((b, a, pa.distance(pb) * 1.1, Link::Walk));
            }
        }
        out
    });
    for list in steps { for (x, y, c, l) in list { add(&mut g.edges, x, y, l, c); } }

    // Off-grid probes from floor edges: step out in twelve directions to
    // points between waypoints, so gaps narrower than the grid (baffled
    // doors, railings) still connect whichever way the grid happens to fall.
    let probes = par_map(&edge_nodes, |&a| {
        let pa = g.nodes[a as usize];
        let lift = Vec3::Y * 0.35;
        let mut out = Vec::new();
        for k in 0..12 {
            let ang = k as f32 * std::f32::consts::TAU / 12.0;
            let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
            for d in [2.0f32, 3.5, 5.0] {
                let q = pa + dir * d;
                let Some((fy, _)) = pack.floor(Vec3::new(q.x, pa.y + 0.9, q.z)) else { break };
                let qs = Vec3::new(q.x, fy + STAND, q.z);
                if (qs.y - pa.y).abs() > d * 0.62 + 0.3 || !body_clear(pack, qs, qs + Vec3::Y * 0.01) { break; }
                if !walk_clear(pack, pa + lift, qs + lift) { break; }
                let best = near(&g, qs, 2.4).into_iter()
                    .filter(|&b| b != a && (g.nodes[b as usize].y - qs.y).abs() < 0.9)
                    .min_by(|x, y| g.nodes[*x as usize].distance(qs).total_cmp(&g.nodes[*y as usize].distance(qs)));
                if let Some(b) = best {
                    let pb = g.nodes[b as usize];
                    if walk_clear(pack, qs + lift, pb + lift) {
                        let c = (pa.distance(qs) + qs.distance(pb)) * 1.1;
                        out.push((a, b, c, Link::Walk));
                        out.push((b, a, c, Link::Walk));
                    }
                }
            }
        }
        out
    });
    for list in probes { for (x, y, c, l) in list { add(&mut g.edges, x, y, l, c); } }

    // Jet climbs onto floor edges (ledges, balconies, bridges, floating decks)
    // from lower terrain or floors, and drops off floor edges. Each target
    // keeps only its nearest few working launch points to bound build time.
    let full = crate::defs::movement().energy_max * JET_PLAN_ENERGY;
    let links = par_map(&edge_nodes, |&t| {
        let pt = g.nodes[t as usize];
        let mut out: Vec<(u32, u32, f32, Link)> = Vec::new();
        let lower = |s: u32| { let dy = pt.y - g.nodes[s as usize].y; dy > 2.0 && dy < 70.0 };
        // A candidate launch: for ground, the speed a skier carries in off the
        // terrain behind it; then the flown arc (or, for short hops, a rise and
        // a level crossing that threads doors) must be clear.
        let try_link = |out: &mut Vec<(u32, u32, f32, Link)>, s: u32| -> bool {
            let ps = g.nodes[s as usize];
            let flat = Vec3::new(pt.x - ps.x, 0.0, pt.z - ps.z).length();
            let dir = Vec3::new(pt.x - ps.x, 0.0, pt.z - ps.z).normalize_or_zero();
            let v0 = if g.terrain[s as usize] { run_in(map, ps, dir) } else { 0.0 };
            let Some(arc) = jet_arc(flat, pt.y - ps.y, full, v0, plan_armor(map), plan_lift(map)) else { return false };
            let at = |(u, h): (f32, f32)| ps + dir * u + Vec3::Y * (h + 0.3);
            let arc_ok = arc.windows(2).all(|w| {
                let (a, b) = (at(w[0]), at(w[1]));
                body_clear(pack, a, b) && crate::terrain::segment_hit(map, a, b, 0.3).is_none()
            });
            let ok = arc_ok || (flat < 30.0 && [2.5f32, 0.6].iter().any(|&h| {
                let top = (pt.y + h).max(ps.y + 1.5);
                let up = Vec3::new(ps.x, top, ps.z);
                let over = Vec3::new(pt.x, top, pt.z);
                body_clear(pack, ps + Vec3::Y * 0.3, up) && body_clear(pack, up, over)
                    && body_clear(pack, over, pt + Vec3::Y * 0.2)
            }));
            if ok { out.push((s, t, 4.0 + ps.distance(pt) * 1.5, Link::Jet)); }
            ok
        };
        let by_distance = |mut v: Vec<u32>| { v.sort_by(|a, b| g.nodes[*a as usize].distance(pt).total_cmp(&g.nodes[*b as usize].distance(pt))); v };
        let floors = by_distance(near(&g, pt, 40.0).into_iter().filter(|&s| s != t && !g.terrain[s as usize] && lower(s)).collect());
        let mut kept = 0;
        for &s in floors.iter().take(8) { if try_link(&mut out, s) { kept += 1; if kept >= 3 { break; } } }
        let ground = by_distance(near(&g, pt, 60.0).into_iter().filter(|&s| g.terrain[s as usize]
            && { let dy = pt.y - g.nodes[s as usize].y; dy > -40.0 && dy < 70.0 }).collect());
        let mut kept_ground = 0;
        for &s in ground.iter().take(10) { if try_link(&mut out, s) { kept_ground += 1; if kept_ground >= 2 { break; } } }
        // Floating decks high over their ground: nothing close enough, so
        // look for a long ski-in launch from farther out, under open sky.
        if kept_ground == 0 && pt.y - crate::terrain::height_on(map, pt.x, pt.z) > 25.0
            && pack.sweep(pt + Vec3::Y, pt + Vec3::Y * 30.0, 0.4).is_none() {
            let far = by_distance(near(&g, pt, 250.0).into_iter().filter(|&s| g.terrain[s as usize]
                && { let dy = pt.y - g.nodes[s as usize].y; dy > -40.0 && dy < 70.0 }).collect());
            let mut tries = 0;
            for &s in &far {
                let ps = g.nodes[s as usize];
                let dir = Vec3::new(pt.x - ps.x, 0.0, pt.z - ps.z).normalize_or_zero();
                let flat = Vec3::new(pt.x - ps.x, 0.0, pt.z - ps.z).length();
                if jet_arc(flat, pt.y - ps.y, full, run_in(map, ps, dir), plan_armor(map), plan_lift(map)).is_none() { continue; }
                if try_link(&mut out, s) { break; }
                tries += 1;
                if tries >= 12 { break; }
            }
        }
        // Drops: step off this edge onto something below.
        let mut drops: Vec<(f32, u32)> = near(&g, pt, 24.0).into_iter()
            .filter(|&b| b != t)
            .map(|b| (g.nodes[b as usize].distance(pt), b))
            .filter(|&(_, b)| { let dy = pt.y - g.nodes[b as usize].y; dy > 2.0 && dy <= MAX_DROP })
            .collect();
        drops.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Floors and ground separately, so a deck's own lower floors never
        // crowd out the way down to the terrain far below.
        for (ground, tries, keep) in [(false, 6, 3), (true, 6, 1)] {
            let mut kept = 0;
            for &(d, b) in drops.iter().filter(|(_, b)| g.terrain[*b as usize] == ground).take(tries) {
                let pb = g.nodes[b as usize];
                let over = Vec3::new(pb.x, pt.y + 0.4, pb.z);
                if clear(map, pack, pt + Vec3::Y * 0.4, over, 0.55) && clear(map, pack, over, pb + Vec3::Y * 0.3, 0.55) {
                    out.push((t, b, d + (pt.y - pb.y).max(0.0) * 0.2, Link::Drop));
                    kept += 1;
                    if kept >= keep { break; }
                }
            }
        }
        out
    });
    for list in links { for (x, y, c, l) in list { add(&mut g.edges, x, y, l, c); } }
    // Pockets: undirected groups too small to matter.
    let mut root: Vec<u32> = (0..g.nodes.len() as u32).collect();
    fn find(r: &mut [u32], mut x: u32) -> u32 { while r[x as usize] != x { r[x as usize] = r[r[x as usize] as usize]; x = r[x as usize]; } x }
    for (a, es) in g.edges.iter().enumerate() {
        for e in es { let (ra, rb) = (find(&mut root, a as u32), find(&mut root, e.to)); if ra != rb { root[ra as usize] = rb; } }
    }
    let roots: Vec<u32> = (0..g.nodes.len() as u32).map(|i| find(&mut root, i)).collect();
    let mut size: HashMap<u32, usize> = HashMap::new();
    for &r in &roots { *size.entry(r).or_default() += 1; }
    g.pocket = roots.iter().map(|r| size[r] < 30).collect();
    Some(g)
}

/// Speed a skier can carry into `at` heading `dir`: from the highest ground on
/// the line behind it, at about 80% of the frictionless energy (skiing is
/// nearly frictionless), capped.
fn run_in(map: MapId, at: Vec3, dir: Vec3) -> f32 {
    let mut drop = 0.0f32;
    for k in 1..=20 {
        let p = at - dir * (k as f32 * 16.0);
        drop = drop.max(crate::terrain::height_on(map, p.x, p.z) + STAND - at.y);
    }
    (1.6 * crate::defs::movement().gravity * drop).sqrt().min(RUN_IN_MAX)
}

/// Map `f` over `items`, split across CPU cores on native builds. Results
/// keep input order, so graph building stays deterministic.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // One core is left to the simulation: a graph built in the middle of
        // a match (a bot server held to two cores) used to starve it.
        let n = std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(1).clamp(1, 8);
        if n > 1 && items.len() > 64 {
            let chunk = items.len().div_ceil(n);
            let f = &f;
            return std::thread::scope(|sc| {
                let hs: Vec<_> = items.chunks(chunk).map(|c| sc.spawn(move || c.iter().map(f).collect::<Vec<R>>())).collect();
                hs.into_iter().flat_map(|h| h.join().expect("nav worker")).collect()
            });
        }
    }
    items.iter().map(f).collect()
}

fn inside_xz(x: f32, z: f32, t: &[Vec3; 3]) -> bool {
    let s = |a: Vec3, b: Vec3| (b.x - a.x) * (z - a.z) - (b.z - a.z) * (x - a.x);
    let (d0, d1, d2) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
    (d0 >= -1e-3 && d1 >= -1e-3 && d2 >= -1e-3) || (d0 <= 1e-3 && d1 <= 1e-3 && d2 <= 1e-3)
}

impl NavGraph {
    /// Waypoint nearest `p`, weighting height so a floor above or below loses
    /// to one at the same level.
    pub(crate) fn nearest(&self, p: Vec3) -> Option<u32> {
        let mut best: Option<(f32, u32)> = None;
        for r in [1, 2, 4] {
            for cx in cell(p.x) - r..=cell(p.x) + r {
                for cz in cell(p.z) - r..=cell(p.z) + r {
                    let Some(ids) = self.grid.get(&(cx, cz)) else { continue };
                    for &i in ids {
                        if self.edges[i as usize].is_empty() || self.pocket[i as usize] { continue; }
                        let q = self.nodes[i as usize];
                        let score = Vec3::new(q.x - p.x, 0.0, q.z - p.z).length() + (q.y - p.y).abs() * 3.0;
                        if best.is_none_or(|(s, _)| score < s) { best = Some((score, i)); }
                    }
                }
            }
            if best.is_some() { break; }
        }
        best.map(|(_, i)| i)
    }

    /// The nearest waypoint a body at `p` can move to in a straight line,
    /// so a route never starts on the far side of a wall. Falls back to the
    /// plain nearest when none nearby is in the clear.
    pub(crate) fn nearest_in_reach(&self, map: MapId, p: Vec3) -> Option<u32> {
        let Some(pack) = crate::map_pack::on(map) else { return self.nearest(p) };
        let mut near: Vec<(f32, u32)> = Vec::new();
        for cx in cell(p.x) - 1..=cell(p.x) + 1 {
            for cz in cell(p.z) - 1..=cell(p.z) + 1 {
                let Some(ids) = self.grid.get(&(cx, cz)) else { continue };
                for &i in ids {
                    if self.edges[i as usize].is_empty() || self.pocket[i as usize] { continue; }
                    let q = self.nodes[i as usize];
                    near.push((Vec3::new(q.x - p.x, 0.0, q.z - p.z).length() + (q.y - p.y).abs() * 3.0, i));
                }
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let lift = Vec3::Y * 0.6;
        // The closest dozen, then terrain waypoints (few, and outside).
        near.iter().enumerate().filter(|&(k, &(_, i))| k < 12 || self.terrain[i as usize]).map(|(_, n)| n).find(|&&(_, i)| {
            let q = self.nodes[i as usize];
            clear(map, pack, p + lift, q + lift, 0.3)
        }).map(|&(_, i)| i).or_else(|| self.nearest(p))
    }

    /// A* from `from` to `to`. `weights` scales each link type's cost: a
    /// skier prefers terrain, a jetter doesn't mind climbs. Visits at most
    /// `budget` nodes so one search can't stall a tick.
    pub(crate) fn path(&self, from: u32, to: u32, weights: [f32; 4], budget: usize) -> Option<Vec<u32>> {
        self.path_avoiding(from, to, weights, budget, &[])
    }

    /// As `path`, never using the links in `banned` (ones a bot got stuck on).
    pub(crate) fn path_avoiding(&self, from: u32, to: u32, weights: [f32; 4], budget: usize, banned: &[(u32, u32)]) -> Option<Vec<u32>> {
        #[derive(PartialEq)]
        struct Open(f32, u32);
        impl Eq for Open {}
        impl PartialOrd for Open { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) } }
        impl Ord for Open { fn cmp(&self, o: &Self) -> std::cmp::Ordering { o.0.total_cmp(&self.0) } }
        let goal = self.nodes[to as usize];
        let n = self.nodes.len();
        let mut g = vec![f32::INFINITY; n];
        let mut came = vec![u32::MAX; n];
        let mut closed = vec![false; n];
        let mut open = BinaryHeap::new();
        g[from as usize] = 0.0;
        open.push(Open(self.nodes[from as usize].distance(goal) * 0.8, from));
        let mut visited = 0;
        while let Some(Open(_, cur)) = open.pop() {
            if std::mem::replace(&mut closed[cur as usize], true) { continue; }
            if cur == to {
                let mut out = vec![cur];
                let mut c = cur;
                while came[c as usize] != u32::MAX { c = came[c as usize]; out.push(c); }
                out.reverse();
                return Some(out);
            }
            visited += 1;
            if visited > budget { return None; }
            let gc = g[cur as usize];
            for e in &self.edges[cur as usize] {
                if banned.contains(&(cur, e.to)) { continue; }
                let ng = gc + e.cost * weights[e.link as usize];
                if ng < g[e.to as usize] {
                    g[e.to as usize] = ng;
                    came[e.to as usize] = cur;
                    open.push(Open(ng + self.nodes[e.to as usize].distance(goal) * 0.8, e.to));
                }
            }
        }
        None
    }

    pub(crate) fn link(&self, a: u32, b: u32) -> Link {
        self.edges[a as usize].iter().find(|e| e.to == b).map_or(Link::Walk, |e| e.link)
    }
}

/// A bot's temperament and skill. Data only: `sim` reads it when the bot
/// thinks, aims and moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotProfile {
    pub name: &'static str,
    /// Standard deviation of aim error, radians.
    pub aim_error: f32,
    /// Seconds an enemy must stay in sight before the bot fires.
    pub reaction: f32,
    /// Fraction of the target's motion it leads (0 = aims where you are).
    pub lead: f32,
    /// 0 plays the objective, 1 chases every fight it sees.
    pub aggression: f32,
    /// Health below which it breaks off to heal.
    pub retreat_health: f32,
    pub uses_kit: bool,
    /// Link-cost weights by style: [walk, ski, jet, drop].
    pub route: [f32; 4],
    pub style: MoveStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveStyle { Ground, Skier, Jetter, Flyer }

pub const ARCHETYPES: [BotProfile; 7] = [
    BotProfile { name: "Rookie", aim_error: 0.11, reaction: 0.75, lead: 0.3, aggression: 0.3,
        retreat_health: 0.0, uses_kit: false, route: [1.0, 1.2, 2.5, 1.5], style: MoveStyle::Ground },
    BotProfile { name: "Grunt", aim_error: 0.07, reaction: 0.5, lead: 0.6, aggression: 0.5,
        retreat_health: 25.0, uses_kit: true, route: [1.0, 1.0, 1.8, 1.2], style: MoveStyle::Ground },
    BotProfile { name: "Rider", aim_error: 0.055, reaction: 0.4, lead: 0.75, aggression: 0.4,
        retreat_health: 30.0, uses_kit: true, route: [1.2, 0.6, 1.6, 1.0], style: MoveStyle::Skier },
    BotProfile { name: "Skirmisher", aim_error: 0.05, reaction: 0.35, lead: 0.8, aggression: 0.85,
        retreat_health: 20.0, uses_kit: true, route: [1.0, 0.9, 0.8, 0.8], style: MoveStyle::Jetter },
    BotProfile { name: "Anchor", aim_error: 0.04, reaction: 0.3, lead: 0.85, aggression: 0.35,
        retreat_health: 40.0, uses_kit: true, route: [1.0, 1.0, 1.2, 1.0], style: MoveStyle::Ground },
    BotProfile { name: "Ace", aim_error: 0.025, reaction: 0.2, lead: 0.95, aggression: 0.7,
        retreat_health: 35.0, uses_kit: true, route: [1.0, 0.7, 0.9, 0.9], style: MoveStyle::Jetter },
    // Crosses the map high on managed jet arcs and dives on targets below.
    BotProfile { name: "Hawk", aim_error: 0.045, reaction: 0.3, lead: 0.85, aggression: 0.7,
        retreat_health: 25.0, uses_kit: true, route: [1.0, 0.8, 0.9, 0.9], style: MoveStyle::Flyer },
];

/// Offline difficulty: which archetypes fill the bot slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Difficulty { Easy, #[default] Normal, Hard, Mixed }

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard, Difficulty::Mixed];
    pub fn label(self) -> &'static str {
        match self { Difficulty::Easy => "Easy", Difficulty::Normal => "Normal", Difficulty::Hard => "Hard", Difficulty::Mixed => "Mixed" }
    }
    /// Archetype weights, indexing `ARCHETYPES`.
    fn weights(self) -> [f32; 7] {
        match self {
            Difficulty::Easy => [0.55, 0.35, 0.1, 0.0, 0.0, 0.0, 0.0],
            Difficulty::Normal => [0.1, 0.3, 0.2, 0.15, 0.15, 0.0, 0.1],
            Difficulty::Hard => [0.0, 0.1, 0.15, 0.2, 0.15, 0.25, 0.15],
            Difficulty::Mixed => [1.0 / 7.0; 7],
        }
    }
    /// Deterministic pick from a uniform `roll` in [0, 1).
    pub fn pick(self, roll: f32) -> BotProfile {
        let w = self.weights();
        let total: f32 = w.iter().sum();
        let mut acc = 0.0;
        for (i, wi) in w.iter().enumerate() {
            acc += wi / total;
            if roll < acc { return ARCHETYPES[i]; }
        }
        ARCHETYPES[w.iter().rposition(|&x| x > 0.0).unwrap_or(0)]
    }
}

impl Default for BotProfile {
    fn default() -> Self { ARCHETYPES[1] }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPS: [MapId; 5] = [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold,
        MapId::Frostline, MapId::Dustreach];

    #[test]
    fn jet_reach_matches_the_energy_budget() {
        let full = crate::defs::movement().energy_max * JET_PLAN_ENERGY;
        assert!(jet_can_reach(0.0, 20.0, full), "a straight climb of 20 m");
        assert!(jet_can_reach(30.0, 20.0, full), "a 30 m hop onto a 20 m ledge");
        assert!(!jet_can_reach(0.0, 120.0, full), "no 120 m climb on one tank");
        assert!(!jet_can_reach(0.0, 20.0, crate::sim::standard_armor().min_jet), "no climb on an empty tank");
    }

    #[test]
    fn nav_graph_connects_every_spawn_to_the_enemy_flag() {
        for map in MAPS {
            let t = std::time::Instant::now();
            let g = graph(map).expect("embedded map");
            let built = t.elapsed();
            let links: usize = g.edges.iter().map(|e| e.len()).sum();
            let flags = crate::map_pack::on(map).unwrap().manifest.flags;
            for (team, ember) in [(0usize, true), (1, false)] {
                let goal = g.nearest(Vec3::from_array(flags[1 - team])).expect("flag waypoint");
                for (spawn, _) in crate::terrain::spawn_points_on(map, ember) {
                    let start = g.nearest(spawn).expect("spawn waypoint");
                    assert!(g.path(start, goal, [1.0; 4], usize::MAX).is_some(),
                        "{map:?}: no route from spawn {spawn:?} to the enemy flag");
                }
            }
            println!("{map:?}: {} waypoints, {links} links, built in {built:?}", g.nodes.len());
        }
    }

    /// Routes on a map folder in development (`PEAKRUNNER_NAV_PACK`, a map
    /// folder outside the catalog): every spawn reaches the enemy flag, and
    /// the routes (and each flag's to the other, and to each control point)
    /// are printed. Ignored by default; does nothing
    /// without the variable.
    #[test]
    #[ignore]
    fn nav_routes_on_a_map_folder() {
        let Ok(dir) = std::env::var("PEAKRUNNER_NAV_PACK") else { return };
        let map = crate::catalog::add_folder("nav-check", dir.into()).expect("the folder loads");
        let t = std::time::Instant::now();
        let g = graph(map).expect("a graph");
        println!("{} waypoints, built in {:?}", g.nodes.len(), t.elapsed());
        let flags = crate::map_pack::on(map).unwrap().manifest.flags;
        let mut failed = 0;
        for (team, ember) in [(0usize, true), (1, false)] {
            let flag = Vec3::from_array(flags[1 - team]);
            let goal = g.nearest(flag).expect("flag waypoint");
            println!("team {team}: enemy flag {flag:?}, its waypoint {:?}", g.nodes[goal as usize]);
            for (spawn, _) in crate::terrain::spawn_points_on(map, ember) {
                let start = g.nearest(spawn).expect("spawn waypoint");
                match g.path(start, goal, [1.0; 4], usize::MAX) {
                    Some(route) => {
                        let jets = route.windows(2).filter(|w| g.edges[w[0] as usize].iter()
                            .any(|e| e.to == w[1] && e.link == Link::Jet)).count();
                        println!("  from {spawn:?}: {} steps, {jets} jets", route.len());
                    }
                    None => {
                        failed += 1;
                        let n = start as usize;
                        let links: Vec<_> = g.edges[n].iter().map(|e| (e.link, g.nodes[e.to as usize])).collect();
                        println!("  from {spawn:?}: NO ROUTE (waypoint {:?}, pocket {}, links {links:?})", g.nodes[n], g.pocket[n]);
                    }
                }
            }
        }
        // A carrier's way home, from each flag to the other: printed only (a
        // bot with no route steers straight at its goal, so a carrier walks
        // off a perch with no planned drop and routes on from below).
        for (from, to) in [(0usize, 1usize), (1, 0)] {
            let (a, b) = (g.nearest(Vec3::from_array(flags[from])).unwrap(), g.nearest(Vec3::from_array(flags[to])).unwrap());
            match g.path(a, b, [1.0; 4], usize::MAX) {
                Some(route) => println!("flag {from} to flag {to}: {} steps", route.len()),
                None => println!("flag {from} to flag {to}: no planned route"),
            }
        }
        // Each control point from each team's first spawn: printed only.
        for p in &crate::map_pack::on(map).unwrap().manifest.control_points {
            let goal = g.nearest(Vec3::from_array(p.pos) + Vec3::Y * 1.2).unwrap();
            for ember in [true, false] {
                let (spawn, _) = crate::terrain::spawn_points_on(map, ember)[0];
                let route = g.path(g.nearest(spawn).unwrap(), goal, [1.0; 4], usize::MAX);
                println!("point {} (waypoint {:?}) from {spawn:?}: {}", p.id, g.nodes[goal as usize],
                    route.map_or("no planned route".into(), |r| format!("{} steps", r.len())));
            }
        }
        assert_eq!(failed, 0, "spawns without a route to the enemy flag");
    }

    /// A bots-only CTF match on a map folder in development
    /// (`PEAKRUNNER_NAV_PACK`, as above; `PEAKRUNNER_NAV_MINUTES`, default
    /// 10; `PEAKRUNNER_NAV_MODE=cnh` for Capture & Hold): prints every flag
    /// event, the control points' holders and the score. Ignored by default;
    /// does nothing without the variable.
    #[test]
    #[ignore]
    fn ctf_bots_on_a_map_folder() {
        let Ok(dir) = std::env::var("PEAKRUNNER_NAV_PACK") else { return };
        let minutes: usize = std::env::var("PEAKRUNNER_NAV_MINUTES").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
        let map = crate::catalog::add_folder("ctf-check", dir.into()).expect("the folder loads");
        let _ = graph(map);
        let mut g = crate::sim::Match::new(map);
        // `PEAKRUNNER_NAV_MODE=cnh` plays Capture & Hold instead.
        let cnh = std::env::var("PEAKRUNNER_NAV_MODE").is_ok_and(|m| m == "cnh");
        g.world.set_mode(if cnh { crate::map_catalog::SupportedMode::CaptureAndHold } else { crate::map_catalog::SupportedMode::Ctf });
        // `PEAKRUNNER_NAV_SEED` (a number) plays another match.
        g.world.reseed(std::env::var("PEAKRUNNER_NAV_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(0x5EED_00C7));
        g.bots = 8;
        let none = vec![None; crate::sim::MAX_PLAYERS];
        let mut carried = [None::<usize>; 2];
        // `PEAKRUNNER_NAV_WATCH=x,z,r`: every 5 s, where each living player
        // within r m (across) of (x, z) is, and every death there.
        let watch: Option<Vec<f32>> = std::env::var("PEAKRUNNER_NAV_WATCH").ok()
            .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect());
        let mut alive: Vec<bool> = Vec::new();
        let mut trail: Vec<Vec<Vec3>> = Vec::new();
        for second in 0..minutes * 60 {
            for step in 0..60 {
                g.step(&none);
                if let Some(w) = watch.as_ref().filter(|w| w.len() == 3) {
                    let inside = |p: &crate::sim::Player| glam::Vec2::new(p.pos.x - w[0], p.pos.z - w[1]).length() < w[2];
                    alive.resize(g.world.players.len(), false);
                    trail.resize(g.world.players.len(), Vec::new());
                    for (i, p) in g.world.players.iter().enumerate() {
                        if step % 20 == 0 && p.alive {
                            trail[i].push(p.pos);
                            if trail[i].len() > 8 { trail[i].remove(0); }
                        }
                        if alive[i] && !p.alive && inside(p) {
                            println!("{second:>4}s {:?} {i} died at {:.0?} after {:.0?}", p.team, p.pos, trail[i]);
                        }
                        if !p.alive { trail[i].clear(); }
                        alive[i] = p.alive;
                        if step == 0 && second % 5 == 0 && p.alive && inside(p) {
                            println!("{second:>4}s {:?} {i} at {:.0?} energy {:.0} health {:.0}", p.team, p.pos, p.energy, p.health);
                        }
                    }
                }
                for (k, f) in g.world.flags.iter().enumerate() {
                    if f.carrier != carried[k] {
                        println!("{second:>4}s flag {k}: carrier {:?} at {:.0?}", f.carrier, f.pos);
                        carried[k] = f.carrier;
                    }
                }
            }
            if second == 1 {
                let roster: Vec<_> = g.world.players.iter().map(|p| (p.name.as_str(), p.team, p.bot_role_name())).collect();
                println!("roster {roster:?}");
            }
            if second % 30 == 29 {
                let w = &g.world;
                let near = |k: usize| w.players.iter().filter(|p| p.alive && p.pos.distance(w.flags[k].home) < 60.0).count();
                println!("{second:>4}s players within 60 m of flag 0: {}, of flag 1: {}; flags at {:.0?} {:.0?}",
                    near(0), near(1), w.flags[0].pos, w.flags[1].pos);
                if w.points.iter().any(|p| p.active) {
                    let held: Vec<_> = w.points.iter().filter(|p| p.active).map(|p| (p.name.as_str(), p.owner)).collect();
                    println!("       points {held:?}, score {:?}", w.score);
                }
                if std::env::var("PEAKRUNNER_NAV_TRACK").is_ok() {
                    for (i, p) in w.players.iter().enumerate().filter(|(_, p)| p.alive && p.team == crate::sim::Team::Ember) {
                        println!("       ember {i} at {:.0?} energy {:.0}", p.pos, p.energy);
                    }
                }
            }
        }
        println!("{}", g.world.hud_json());
    }

    #[test]
    fn difficulty_picks_are_deterministic_and_bounded() {
        for d in Difficulty::ALL {
            for k in 0..100 {
                let r = k as f32 / 100.0;
                let p = d.pick(r);
                assert_eq!(p, d.pick(r));
                assert!(p.aim_error > 0.0 && p.aim_error < 0.2 && p.reaction > 0.0 && p.reaction < 1.0);
            }
        }
        assert!(Difficulty::Easy.pick(0.0).aim_error > Difficulty::Hard.pick(0.99).aim_error);
    }
}

