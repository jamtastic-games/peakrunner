//! Deployables: turrets, walls, force fields and ammo stations bought at an inventory
//! station (one pack at a time) and placed where you aim (the client shows a
//! hologram from `aim_placement` while you line it up, and sends the turn). Each team may have only so many of each standing. Walls block
//! everyone and everything; force fields block only the other team and its
//! shots. An ammo station restocks teammates who use it (hold E beside it).
//! All take damage, can be repaired with the repair tool, and are
//! destroyed at zero. Server-owned; snapshots carry them whole. Marker: `loadout1`.
use super::*;

use crate::registry::Behavior;

/// What a deployable is: a deployable entry's wire number
/// (`crate::registry`); what it does placed is its entry's `behavior`. Its
/// `Debug` form is the entry's id, capitalized ("Wall").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeployKind(pub u8);

#[allow(non_upper_case_globals)]
impl DeployKind {
    /// The base game's deployables (fixed wire numbers).
    pub const Turret: DeployKind = DeployKind(0);
    pub const Wall: DeployKind = DeployKind(1);
    pub const Field: DeployKind = DeployKind(2);
    pub const Ammo: DeployKind = DeployKind(3);
    pub const Inventory: DeployKind = DeployKind(4);
    /// Every deployable in the session.
    pub fn all() -> impl Iterator<Item = DeployKind> {
        crate::registry::registry().deployables.iter().map(|(k, _, _)| DeployKind(k as u8))
    }
    /// Its entry (the base wall's, for a number no entry has).
    pub fn def(self) -> &'static crate::registry::Deployable {
        let r = crate::registry::registry();
        r.deployables.get(self.0 as usize).or_else(|| r.deployables.get(1)).expect("the base game has a wall")
    }
    pub fn behavior(self) -> Behavior { self.def().behavior }
    /// Its id, as saved favorites name it.
    pub fn key(self) -> &'static str { crate::registry::registry().deployables.id(self.0 as usize).unwrap_or("wall") }
    pub fn from_key(key: &str) -> Option<Self> { crate::registry::registry().deployables.index(key).map(|k| DeployKind(k as u8)) }
    /// Its name for people.
    pub fn name(self) -> &'static str { self.def().name.as_str() }
    /// A turret's targeting and fire, if it is one.
    pub fn turret(self) -> Option<crate::equipment::TurretProfile> {
        let t = self.def().turret.as_ref()?;
        Some(crate::equipment::TurretProfile { range: t.range, sensed_range: t.sensed_range, speed: t.speed, life: t.life,
            cooldown: t.cooldown, leads: t.leads, fires: t.fires, projectile: crate::registry::registry().projectile_kind(&t.projectile),
            jetting_only: t.jetting_only })
    }
}

impl std::fmt::Debug for DeployKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&super::loadout::capitalized(self.key())) }
}

/// A unit being lined up, and why it can't go there (None: it can).
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub unit: Deployable,
    pub problem: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deployable {
    /// Who placed it: their player slot + 1 (0: nobody's, a map's).
    #[serde(default)]
    pub owner: u8,
    pub kind: DeployKind,
    pub team: Team,
    /// Base, on the floor.
    pub pos: Vec3,
    /// Facing (a panel spans the deployer's left-right).
    pub yaw: f32,
    pub health: f32,
    pub cooldown: f32,
    /// A turret's current aim.
    pub aim: Vec3,
    /// Seconds it has stood (for an entry's `life`).
    #[serde(default)]
    pub age: f32,
}

/// Standing deployables of one kind a team may have at once, before any
/// allowance per teammate (`World::team_limit`).
pub fn team_limit(kind: DeployKind) -> usize { kind.def().team_limit as usize }

/// What a team has used up this round, against an entry's `match_limit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Used { Placed, Thrown }
pub fn max_health(kind: DeployKind) -> f32 { kind.def().hull }
/// Half-extents of an ammo station crate.
pub const AMMO_HALF: Vec3 = Vec3::new(0.55, 0.45, 0.4);
/// How close you must stand to use an ammo station (or a field inventory).
pub const AMMO_REACH: f32 = 2.5;
/// Half-extents of a field inventory: a kiosk you step up to.
pub const INVENTORY_HALF: Vec3 = Vec3::new(0.6, 0.95, 0.5);
/// A box-shaped deployable's half-extents (a turret or post is a post
/// instead): its entry's `size`, else its behavior's.
pub fn half_extents(kind: DeployKind) -> Option<Vec3> {
    let d = kind.def();
    match d.behavior {
        Behavior::Turret | Behavior::Post => None,
        _ if d.size.is_some() => d.size.map(Vec3::from),
        Behavior::Ammo => Some(AMMO_HALF),
        Behavior::Inventory | Behavior::Command => Some(INVENTORY_HALF),
        Behavior::Pad => Some(PAD_HALF),
        Behavior::Gate => Some(GATE_HALF),
        _ => Some(PANEL_HALF),
    }
}
/// A pad's half-extents: a plate 2.4 m across, 0.3 m thick.
pub const PAD_HALF: Vec3 = Vec3::new(1.2, 0.15, 1.2);
/// A gate's: a frame 2 m wide and 2.8 m tall to step into.
pub const GATE_HALF: Vec3 = Vec3::new(1.0, 1.4, 0.4);
/// A plain post (pylons, sensors, cameras): radius and height.
pub const POST_RADIUS: f32 = 0.35;
pub const POST_HEIGHT: f32 = 1.8;
/// Half-extents of a wall or field panel: 4 m wide, 3.2 m tall, 0.36 m thick.
pub const PANEL_HALF: Vec3 = Vec3::new(2.0, 1.6, 0.18);
/// A deployed turret is a squat post with this body radius.
pub const TURRET_RADIUS: f32 = 0.7;
const TURRET_HEIGHT: f32 = 1.3;
/// Keep clear of flag stands and spawns.
const FLAG_CLEAR: f32 = 12.0;
const SPAWN_CLEAR: f32 = 6.0;
const DEPLOY_SPACING: f32 = 3.0;

impl Deployable {
    /// The face a field inventory opens from: toward whoever placed it.
    pub fn front(&self) -> Vec3 { Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos()) }

    pub fn center(&self) -> Vec3 {
        match self.kind.behavior() {
            Behavior::Turret => self.pos + Vec3::Y * 0.9,
            Behavior::Post => self.pos + Vec3::Y * (POST_HEIGHT * 0.5),
            _ => self.pos + Vec3::Y * half_extents(self.kind).unwrap_or(PANEL_HALF).y,
        }
    }
    /// Blocks a body or shot of `team` (None: everything, e.g. sight lines).
    /// A gate blocks nothing.
    pub fn blocks(&self, team: Option<Team>) -> bool {
        match self.kind.behavior() { Behavior::Field => team != Some(self.team), Behavior::Gate => false, _ => true }
    }
    /// The radius of a post-shaped body (a turret's or a plain post).
    fn post_radius(&self) -> Option<f32> {
        match self.kind.behavior() { Behavior::Turret => Some(TURRET_RADIUS), Behavior::Post => Some(POST_RADIUS), _ => None }
    }
    /// Whether a body at `pos` (a player's) is inside it, or touching it.
    pub fn touches(&self, pos: Vec3, r: f32) -> bool {
        match self.post_radius() {
            Some(radius) => {
                let top = if self.kind.behavior() == Behavior::Turret { TURRET_HEIGHT } else { POST_HEIGHT };
                Vec2::new(pos.x - self.pos.x, pos.z - self.pos.z).length() <= radius + r + 0.1
                    && pos.y <= self.pos.y + top + r && pos.y >= self.pos.y - 1.0
            }
            None => {
                let half = half_extents(self.kind).unwrap_or(PANEL_HALF) + Vec3::splat(r + 0.1);
                let local = self.to_local(pos);
                local.x.abs() <= half.x && local.y.abs() <= half.y && local.z.abs() <= half.z
            }
        }
    }
    fn to_local(&self, p: Vec3) -> Vec3 {
        Mat4::from_rotation_y(-self.yaw).transform_point3(p - self.center())
    }
    /// Outward normal where a sphere of `radius` touches it at `contact`, for
    /// things that bounce off: the post's radius, or the box face struck.
    pub fn normal_at(&self, contact: Vec3, radius: f32) -> Vec3 {
        match self.kind.behavior() {
            Behavior::Turret | Behavior::Post => (contact - self.center()).normalize_or_zero(),
            _ => {
                let kind = self.kind;
                let half = half_extents(kind).unwrap_or(PANEL_HALF) + Vec3::splat(radius);
                let r = self.to_local(contact) / half;
                let face = if r.x.abs() >= r.y.abs() && r.x.abs() >= r.z.abs() { Vec3::X * r.x.signum() }
                    else if r.y.abs() >= r.z.abs() { Vec3::Y * r.y.signum() } else { Vec3::Z * r.z.signum() };
                Mat4::from_rotation_y(self.yaw).transform_vector3(face)
            }
        }
    }

    /// First fraction along `a` to `b` where a sphere of `radius` touches it.
    pub fn hit(&self, a: Vec3, b: Vec3, radius: f32) -> Option<f32> {
        match self.kind.behavior() {
            Behavior::Turret => {
                segment_sphere(a, b, self.center(), TURRET_RADIUS + radius)
            }
            Behavior::Post => segment_sphere(a, b, self.center(), POST_RADIUS.max(POST_HEIGHT * 0.5) * 0.6 + radius),
            _ => {
                let kind = self.kind;
                let half = half_extents(kind).unwrap_or(PANEL_HALF) + Vec3::splat(radius);
                segment_box(self.to_local(a), self.to_local(b), -half, half)
            }
        }
    }
}

impl World {
    /// Standing deployables of one kind `team` may have now: the entry's
    /// limit plus its allowance per player on the team.
    pub fn team_limit(&self, kind: DeployKind, team: Team) -> usize {
        let d = kind.def();
        let size = self.players.iter().filter(|p| p.team == team && (p.net_id != 0 || p.is_bot || !p.remote)).count();
        d.team_limit as usize + (d.per_teammate * size as f32).floor() as usize
    }

    /// How many of entry `id` `team` has used up this round.
    pub fn used(&self, what: Used, team: Team, id: u8) -> u16 { self.uses.get(&(what, team.idx() as u8, id)).copied().unwrap_or(0) }
    pub(super) fn count_use(&mut self, what: Used, team: Team, id: u8) { *self.uses.entry((what, team.idx() as u8, id)).or_default() += 1; }

    /// Nearest deployable hit along `a` to `b` by a sphere of `radius` that
    /// blocks `team`, as (fraction, index).
    pub fn deployable_hit(&self, a: Vec3, b: Vec3, radius: f32, team: Option<Team>) -> Option<(f32, usize)> {
        self.deployables.iter().enumerate().filter(|(_, e)| e.blocks(team))
            .filter_map(|(k, e)| e.hit(a, b, radius).map(|t| (t, k)))
            .min_by(|x, y| x.0.total_cmp(&y.0))
    }

    /// Map, pillars and deployables together.
    pub fn blocked(&self, a: Vec3, b: Vec3, radius: f32, team: Option<Team>) -> Option<f32> {
        let map = obstacle_hit(self.map, &self.pillars, a, b, radius);
        let dep = self.deployable_hit(a, b, radius, team).map(|(t, _)| t);
        let moving = self.moveable_hit(a, b, radius).map(|(t, _)| t);
        [map, dep, moving].into_iter().flatten().min_by(f32::total_cmp)
    }

    /// Push player `i` out of any deployable that blocks them.
    pub(super) fn collide_deployables(&mut self, i: usize) {
        let team = self.players[i].team;
        for k in 0..self.deployables.len() {
            let e = &self.deployables[k];
            if !e.blocks(Some(team)) { continue; }
            let p = &self.players[i];
            let r = PLAYER_RADIUS;
            match e.kind.behavior() {
                Behavior::Turret | Behavior::Post => {
                    let c = e.pos;
                    let (radius, height) = if e.kind.behavior() == Behavior::Turret { (TURRET_RADIUS, TURRET_HEIGHT) } else { (POST_RADIUS, POST_HEIGHT) };
                    let flat = Vec2::new(p.pos.x - c.x, p.pos.z - c.z);
                    let d = flat.length();
                    if d >= radius + r || p.pos.y > c.y + height + r || p.pos.y < c.y - 1.0 { continue; }
                    let n = if d > 1e-3 { flat / d } else { Vec2::X };
                    let out = Vec3::new(n.x, 0.0, n.y);
                    let p = &mut self.players[i];
                    p.pos += out * (radius + r - d);
                    let vn = p.vel.dot(out);
                    if vn < 0.0 { p.vel -= out * vn; }
                }
                // Above a pad's top: standing on it (or landing), or clear of it.
                Behavior::Pad if e.to_local(p.pos).y >= half_extents(e.kind).unwrap_or(PAD_HALF).y - 0.05 => {
                    let half = half_extents(e.kind).unwrap_or(PAD_HALF);
                    let local = e.to_local(p.pos);
                    if local.x.abs() >= half.x + r * 0.5 || local.z.abs() >= half.z + r * 0.5 || local.y >= half.y + r + 0.05 { continue; }
                    let top = e.center().y + half.y;
                    let pad = e.kind.def().pad.clone();
                    let facing = e.front();
                    let p = &mut self.players[i];
                    p.pos.y = top + r;
                    if p.vel.y < 0.0 { p.vel.y = 0.0; }
                    // Footing: ground for the next step, jumped from like it.
                    p.on_ground = true;
                    p.footing = true;
                    if let Some(pad) = pad.filter(|pad| p.vel.y <= pad.up * 0.5) {
                        let mass = loadout::mass_scale(p.armor).max(0.1);
                        let along = match pad.along {
                            crate::registry::PadAlong::View => Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos()),
                            crate::registry::PadAlong::Facing => -facing,
                            crate::registry::PadAlong::Up => Vec3::ZERO,
                        };
                        p.vel.y = pad.up / mass;
                        p.vel += along * (pad.forward / mass);
                        p.on_ground = false;
                        p.skiing = false;
                        p.coyote = 0.0;
                    }
                }
                _ => {
                    let kind = e.kind;
                    let local = e.to_local(p.pos);
                    let half = half_extents(kind).unwrap_or(PANEL_HALF) + Vec3::new(r, r, r);
                    if local.x.abs() >= half.x || local.y.abs() >= half.y + 0.5 || local.z.abs() >= half.z { continue; }
                    // Out through the nearer face: the panel's front or back,
                    // or round an end.
                    let (push, axis) = if half.z - local.z.abs() < half.x - local.x.abs() {
                        ((half.z - local.z.abs()) * local.z.signum(), Vec3::Z)
                    } else {
                        ((half.x - local.x.abs()) * local.x.signum(), Vec3::X)
                    };
                    let out = Mat4::from_rotation_y(e.yaw).transform_vector3(axis);
                    let p = &mut self.players[i];
                    p.pos += out * push;
                    let dir = out * push.signum();
                    let vn = p.vel.dot(dir);
                    if vn < 0.0 { p.vel -= dir * vn; }
                }
            }
        }
    }

    /// The deploy intent: place the carried pack where the player aims,
    /// turned by the command's `deploy_turn`, if the team has room for another
    /// and the spot is clear.
    pub(super) fn step_deploys(&mut self) {
        // Deployables take a side, so free-for-all modes have none.
        if self.predicting || !self.mode.def().deployables { return; }
        for i in 0..self.players.len() {
            let (wants, turn) = if self.players[i].is_bot { (std::mem::take(&mut self.players[i].bot_press) & 4 != 0, 0.0) }
                else if self.network_inputs.is_empty() {
                    if i == self.player_id { (self.input.deploy, self.input.deploy_turn) } else { (false, 0.0) }
                } else { self.network_inputs.get(i).map_or((false, 0.0), |c| (c.deploy, c.deploy_turn)) };
            // A pack that places (`packs`; the others work in `step_players`).
            let Some(kind) = self.players[i].pack.filter(|_| wants && self.players[i].alive).and_then(|k| k.deployable()) else { continue };
            match self.place(i, kind, turn) {
                Ok(e) => {
                    self.count_use(Used::Placed, e.team, e.kind.0);
                    let (hook, id, at) = (e.kind.def().hook.as_deref(), e.kind.key().to_string(), e.pos);
                    self.deployables.push(e);
                    self.players[i].pack = None;
                    if i == self.player_id { self.push_event("deploy"); }
                    if self.hooks.live { self.hook(hook, super::hooks::HookEvent::Deploy { player: i, deployable: id, at }); }
                }
                Err(why) => if i == self.player_id { self.msg(why, 1.6); },
            }
        }
    }

    /// Where player `i` would put down a `kind`, or why they can't.
    pub fn place(&self, i: usize, kind: DeployKind, turn: f32) -> Result<Deployable, &'static str> {
        match self.aim_placement(i, kind, turn) {
            None => Err("TOO FAR AWAY"),
            Some(Placement { problem: Some(why), .. }) => Err(why),
            Some(Placement { unit, .. }) => Ok(unit),
        }
    }

    /// The unit player `i` is lining up: where their aim meets a surface
    /// within DEPLOY_RANGE, facing their yaw plus `turn`, and the first reason
    /// it can't go there (None when it can). None when nothing is in reach.
    /// The client draws this as the hologram; the server checks it again.
    pub fn aim_placement(&self, i: usize, kind: DeployKind, turn: f32) -> Option<Placement> {
        let p = self.players.get(i)?;
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(p.yaw, p.pitch);
        let far = eye + dir * crate::defs::defs().deployables.range;
        let t = self.blocked(eye, far, 0.0, None)?;
        let spot = eye.lerp(far, t);
        let on_unit = self.deployable_hit(eye, far, 0.0, None).is_some_and(|(u, _)| u <= t + 1e-4);
        let (floor, normal) = crate::terrain::support_on(self.map, spot + Vec3::Y * 0.4);
        let flat = normal.y >= 0.85 && (floor - spot.y).abs() <= 0.6;
        let pos = Vec3::new(spot.x, if flat { floor } else { spot.y }, spot.z);
        let yaw = p.yaw + if turn.is_finite() { turn } else { 0.0 };
        let (fwd, right) = move_basis(yaw);
        let owner = (i + 1).min(u8::MAX as usize) as u8;
        let unit = Deployable { owner, kind, team: p.team, pos, yaw, health: max_health(kind), cooldown: 0.0, aim: fwd, age: 0.0 };
        let problem = (|| {
            let standing = self.deployables.iter().filter(|e| e.team == p.team && e.kind == kind).count();
            if standing >= self.team_limit(kind, p.team) { return Some("YOUR TEAM HAS THE MAXIMUM OF THOSE DEPLOYED"); }
            let per_player = kind.def().per_player as usize;
            if per_player > 0 && self.deployables.iter().filter(|e| e.owner == owner && e.kind == kind).count() >= per_player {
                return Some("YOU HAVE THE MOST OF THOSE YOU MAY PLACE");
            }
            let match_limit = kind.def().match_limit;
            if match_limit > 0 && self.used(Used::Placed, p.team, kind.0) >= match_limit {
                return Some("YOUR TEAM HAS PLACED ALL OF THOSE IT MAY THIS ROUND");
            }
            if on_unit { return Some("NO ROOM THERE"); }
            if !flat { return Some("NO FLAT GROUND THERE"); }
            let flags = [self.flags[0].home, self.flags[1].home];
            if flags.iter().any(|f| f.distance(pos) < FLAG_CLEAR) { return Some("TOO CLOSE TO A FLAG"); }
            for team in [true, false] {
                if crate::terrain::spawn_points_on(self.map, team).iter().any(|(s, _)| s.distance(pos) < SPAWN_CLEAR) {
                    return Some("TOO CLOSE TO A SPAWN");
                }
            }
            if self.deployables.iter().any(|e| e.pos.distance(pos) < DEPLOY_SPACING) { return Some("TOO CLOSE TO ANOTHER DEPLOYABLE"); }
            // Room overhead for the unit itself.
            if self.blocked(pos + Vec3::Y * 0.3, pos + Vec3::Y * 1.2, 0.0, None).is_some() { return Some("NO ROOM THERE"); }
            if matches!(kind.behavior(), Behavior::Wall | Behavior::Field) {
                // The whole panel must fit: nothing solid across it or under its ends.
                let mid = pos + Vec3::Y * PANEL_HALF.y;
                if self.blocked(mid - right * PANEL_HALF.x, mid + right * PANEL_HALF.x, 0.1, None).is_some() {
                    return Some("NO ROOM THERE");
                }
                for end in [-1.0f32, 1.0] {
                    let foot = pos + right * PANEL_HALF.x * end;
                    let under = crate::terrain::support_on(self.map, foot + Vec3::Y * 1.5).0;
                    if (under - floor).abs() > 1.0 { return Some("NO FLAT GROUND THERE"); }
                }
            }
            if self.players.iter().any(|o| o.alive && unit.hit(o.pos - Vec3::Y * 0.3, o.pos + Vec3::Y * 1.5, PLAYER_RADIUS).is_some()) {
                return Some("SOMEONE IS IN THE WAY");
            }
            None
        })();
        Some(Placement { unit, problem })
    }

    /// Damage deployable `k`; destroyed ones blow up (harmlessly) and vanish.
    pub(super) fn damage_deployable(&mut self, k: usize, amount: f32) {
        let Some(e) = self.deployables.get_mut(k) else { return };
        e.health -= amount;
    }

    /// Ammo stations: a living player holding use (E) within reach of one of
    /// their team's restocks every weapon; bots restock just by standing there.
    pub(super) fn step_ammo_stations(&mut self) {
        if self.predicting || !self.mode.def().ammo_stations { return; }
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive { continue; }
            let using = if p.is_bot { true } else if self.network_inputs.is_empty() { i == self.player_id && self.input.interact }
                else { self.network_inputs.get(i).is_some_and(|c| c.interact) };
            // A field inventory restocks too.
            let near = self.deployables.iter().any(|e| matches!(e.kind.behavior(), Behavior::Ammo | Behavior::Inventory) && e.team == p.team
                && e.center().distance(p.pos) <= AMMO_REACH);
            if using && near && (p.ammo != loadout::rounds_max(p) || p.throwables != super::throwables::max_throwables(p.armor)) {
                self.refill(i);
                World::cleanse(&mut self.players[i]);
                if i == self.player_id { self.push_event("buy"); }
            }
        }
    }

    /// Whether player `i` stands at one of their team's field inventories,
    /// stepped in from its front.
    pub fn at_field_inventory(&self, i: usize) -> bool { self.field_inventory_reach(i) == Some(true) }

    /// At a field inventory: Some(true) from its front, Some(false) beside or
    /// behind it, None away.
    pub fn field_inventory_reach(&self, i: usize) -> Option<bool> {
        let p = &self.players[i];
        let near: Vec<&Deployable> = self.deployables.iter()
            .filter(|e| e.kind.behavior() == Behavior::Inventory && e.team == p.team && e.center().distance(p.pos) <= AMMO_REACH).collect();
        if near.is_empty() { return None; }
        Some(near.iter().any(|e| super::loadout::in_front(p.pos, e.pos + e.front() * INVENTORY_HALF.z, e.front())))
    }

    /// Whether player `i` stands at one of their team's ammo stations.
    pub fn at_ammo_station(&self, i: usize) -> bool {
        let p = &self.players[i];
        self.deployables.iter().any(|e| e.kind.behavior() == Behavior::Ammo && e.team == p.team && e.center().distance(p.pos) <= AMMO_REACH)
    }

    /// Turrets track and fire; wrecks are cleared with a blast.
    pub(super) fn step_deployables(&mut self, dt: f32) {
        if self.predicting { return; }
        let mut gone = Vec::new();
        for k in 0..self.deployables.len() {
            // A unit with a life goes when it's up (`registry::Deployable::life`).
            self.deployables[k].age += dt;
            let life = self.deployables[k].kind.def().life;
            if life > 0.0 && self.deployables[k].age >= life { self.deployables[k].health = 0.0; }
            if self.deployables[k].health <= 0.0 { gone.push(k); continue; }
            let Some(profile) = self.deployables[k].kind.turret() else { continue };
            let (origin, team) = (self.deployables[k].center(), self.deployables[k].team);
            self.deployables[k].cooldown = (self.deployables[k].cooldown - dt).max(0.0);
            // Operated from a command station (`placed`): its operator aims.
            if self.operate_turret(k) { continue; }
            let hidden: Vec<bool> = (0..self.players.len()).map(|j| self.sensor_hidden(j)).collect();
            let candidates = self.players.iter().enumerate().filter(|(j, p)| p.alive && !hidden[*j])
                .map(|(index, p)| crate::equipment::Candidate { index, team: p.team.idx() as u8, pos: p.pos, vel: p.vel, jetting: p.jetting });
            let clear = |a: Vec3, b: Vec3| {
                obstacle_hit(self.map, &self.pillars, a, b, 0.0).is_none()
                    && self.deployables.iter().enumerate().all(|(j, e)| j == k || !e.blocks(Some(team)) || e.hit(a, b, 0.0).is_none())
            };
            // A team's marks, or a sensor of its own standing (`placed`),
            // let its turrets reach their sensed range.
            let marked = self.marks.iter().any(|m| m.team == team) || self.has_sensor(team);
            let Some(hit) = crate::equipment::acquire_target(origin, TURRET_RADIUS, team.idx() as u8, &profile,
                marked, candidates, clear) else { continue };
            self.deployables[k].aim = hit.aim;
            if profile.fires && self.deployables[k].cooldown <= 0.0 && self.discs.len() < 256 {
                self.discs.push(Disc { pos: hit.muzzle, vel: hit.aim * profile.speed, team, owner: MAX_PLAYERS,
                    life: profile.life, kind: profile.projectile, spin: -(k as f32) - 1.0, target: 0, power: 1.0 });
                self.deployables[k].cooldown = profile.cooldown;
            }
        }
        for k in gone.into_iter().rev() {
            let e = self.deployables.remove(k);
            if self.hooks.live {
                self.hook(e.kind.def().hook.as_deref(), super::hooks::HookEvent::Destroyed { deployable: e.kind.key().to_string(), team: e.team, at: e.pos });
            }
            self.blast_serial += 1;
            self.explosions.push(Explosion { pos: e.center(), age: 0.0, max_r: 4.0, kind: 4 });
            self.spatial_sounds.push(("boom", e.center()));
        }
    }

    /// Turrets on capture points (`control::PointTurret`): in CTF each
    /// fights for whoever holds its point, idle while nobody does (Capture &
    /// Hold leaves them be). Server only.
    pub(super) fn step_point_turrets(&mut self, dt: f32) {
        if self.predicting || self.mode.rules() != crate::modes::Rules::Ctf { return; }
        for k in 0..self.points.len() {
            let p = &self.points[k];
            let (Some(kind), Some(owner)) = (p.turret.filter(|_| p.active), p.owner) else { continue };
            let Some(mut profile) = kind.turret() else { continue };
            if p.turret_range > 0.0 { (profile.range, profile.sensed_range) = (p.turret_range, p.turret_range); }
            let team = if owner == 1 { Team::Glacier } else { Team::Ember };
            let origin = p.turret_at + Vec3::Y * 0.9;
            self.points[k].turret_cooldown = (self.points[k].turret_cooldown - dt).max(0.0);
            let hidden: Vec<bool> = (0..self.players.len()).map(|j| self.sensor_hidden(j)).collect();
            let candidates = self.players.iter().enumerate().filter(|(j, p)| p.alive && !hidden[*j])
                .map(|(index, p)| crate::equipment::Candidate { index, team: p.team.idx() as u8, pos: p.pos, vel: p.vel, jetting: p.jetting });
            let clear = |a: Vec3, b: Vec3| obstacle_hit(self.map, &self.pillars, a, b, 0.0).is_none()
                && self.deployables.iter().all(|e| !e.blocks(Some(team)) || e.hit(a, b, 0.0).is_none());
            let marked = self.marks.iter().any(|m| m.team == team) || self.has_sensor(team);
            let Some(hit) = crate::equipment::acquire_target(origin, TURRET_RADIUS, owner, &profile, marked, candidates, clear) else { continue };
            self.points[k].turret_aim = hit.aim;
            if profile.fires && self.points[k].turret_cooldown <= 0.0 && self.discs.len() < 256 {
                // A homing round follows the one it was fired at.
                let homing = crate::registry::registry().projectiles.get(profile.projectile as usize).is_some_and(|r| r.seek.is_some());
                self.discs.push(Disc { pos: hit.muzzle, vel: hit.aim * profile.speed, team, owner: MAX_PLAYERS,
                    life: profile.life, kind: profile.projectile, spin: -1000.0 - k as f32,
                    target: if homing { hit.index as u16 + 1 } else { 0 }, power: 1.0 });
                self.points[k].turret_cooldown = profile.cooldown;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    #[test]
    fn a_bounce_off_a_deployable_uses_the_face_struck() {
        let wall = Deployable { owner: 0, kind: DeployKind::Wall, team: Team::Ember, pos: Vec3::new(10.0, 0.0, 10.0),
            yaw: std::f32::consts::FRAC_PI_2, health: 100.0, cooldown: 0.0, aim: Vec3::Z, age: 0.0 };
        // Turned a quarter: its broad faces look along world X.
        let c = wall.center();
        let face = wall.normal_at(c + Vec3::new(PANEL_HALF.z + 0.12, 0.3, 1.0), 0.12);
        assert!((face - Vec3::X).length() < 1e-4, "{face}");
        let back = wall.normal_at(c + Vec3::new(-(PANEL_HALF.z + 0.12), -0.5, -1.5), 0.12);
        assert!((back + Vec3::X).length() < 1e-4, "{back}");
        let top = wall.normal_at(c + Vec3::new(0.0, PANEL_HALF.y + 0.12, 0.5), 0.12);
        assert!((top - Vec3::Y).length() < 1e-4, "{top}");
        let post = Deployable { kind: DeployKind::Turret, ..wall };
        assert!((post.normal_at(post.center() + Vec3::new(0.0, 0.0, -2.0), 0.12) + Vec3::Z).length() < 1e-4);
    }

    /// A player alone on Old Holler, standing on open ground at their spawn,
    /// carrying a pack.
    fn world(kind: DeployKind) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(MapId::OldHoller, spot + Vec3::Y * 200.0).0;
        w.players[0].pos = Vec3::new(spot.x, floor + PLAYER_RADIUS, spot.z);
        w.players[0].pack = super::packs::PackKind::from_key(kind.key());
        // Looking down at the ground a few metres ahead.
        w.players[0].pitch = -0.3;
        w
    }

    #[test]
    fn own_team_fire_and_blasts_wear_down_own_walls_and_fields() {
        for kind in [DeployKind::Wall, DeployKind::Field] {
            let mut w = world(kind);
            let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, kind, 0.0).is_ok() })
                .expect("somewhere to deploy");
            w.players[0].yaw = yaw;
            w.input.deploy = true;
            w.step_deploys();
            let e = w.deployables[0].clone();
            let full = e.health;
            if kind == DeployKind::Wall {
                // A teammate's chaingun round into the wall.
                let (fwd, _) = move_basis(yaw);
                let from = e.center() - fwd * 4.0;
                w.discs.push(crate::sim::Disc { pos: from, vel: fwd * 200.0, team: Team::Ember, owner: 0, life: 1.0, kind: 1, spin: 0.0, target: 0, power: 1.0 });
                for _ in 0..4 { w.physics_step(); }
                assert!(w.deployables[0].health < full, "own round damages own wall");
            } else {
                w.explode(e.center() + Vec3::Y * 0.5, 0, 0, Team::Ember);
                assert!(w.deployables.first().is_none_or(|d| d.health < full), "own blast damages own field");
            }
        }
    }

    #[test]
    fn a_wall_goes_down_ahead_and_blocks_everyone_a_field_only_the_enemy() {
        for kind in [DeployKind::Wall, DeployKind::Field] {
            let mut w = world(kind);
            let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, kind, 0.0).is_ok() })
                .expect("somewhere to deploy");
            w.players[0].yaw = yaw;
            w.input.deploy = true;
            w.step_deploys();
            assert_eq!(w.deployables.len(), 1, "{kind:?}");
            assert!(w.players[0].pack.is_none());
            let e = w.deployables[0].clone();
            let (fwd, _) = move_basis(yaw);
            // A shot straight through the panel.
            let a = e.center() - fwd * 3.0;
            let b = e.center() + fwd * 3.0;
            assert!(w.blocked(a, b, 0.1, Some(Team::Glacier)).is_some(), "stops enemy shots");
            assert_eq!(w.blocked(a, b, 0.1, Some(Team::Ember)).is_some(), kind == DeployKind::Wall, "own team through a field");
            // A player walking into it from in front is held back unless it's their field.
            for team in [Team::Ember, Team::Glacier] {
                w.players[0].team = team;
                w.players[0].pos = e.pos + Vec3::Y * PLAYER_RADIUS - fwd * 0.3;
                w.players[0].vel = fwd * 8.0;
                w.collide_deployables(0);
                let passes = kind == DeployKind::Field && team == Team::Ember;
                let inside = e.to_local(w.players[0].pos).z.abs() < PANEL_HALF.z + PLAYER_RADIUS - 0.01;
                assert_eq!(inside, passes, "{kind:?} {team:?}");
            }
        }
    }

    #[test]
    fn the_unit_goes_where_you_aim_turns_and_vanishes_out_of_reach() {
        let mut w = world(DeployKind::Turret);
        let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Turret, 0.0).is_ok() })
            .expect("somewhere to deploy");
        w.players[0].yaw = yaw;
        let eye = w.players[0].pos;
        let flat = |e: &Deployable| Vec2::new(e.pos.x - eye.x, e.pos.z - eye.z).length();
        // Looking lower puts it nearer; looking at the horizon puts it out of reach.
        w.players[0].pitch = -0.6;
        let near = w.aim_placement(0, DeployKind::Turret, 0.0).expect("in reach");
        w.players[0].pitch = -0.3;
        let farther = w.aim_placement(0, DeployKind::Turret, 0.0).expect("in reach");
        assert!(flat(&farther.unit) > flat(&near.unit) + 1.0, "{} vs {}", flat(&farther.unit), flat(&near.unit));
        w.players[0].pitch = 0.2;
        assert!(w.aim_placement(0, DeployKind::Turret, 0.0).is_none(), "nothing within reach: no hologram");
        assert_eq!(w.place(0, DeployKind::Turret, 0.0).err(), Some("TOO FAR AWAY"));
        // The turn rotates it, and the server places exactly the previewed unit.
        w.players[0].pitch = -0.3;
        let turned = w.aim_placement(0, DeployKind::Turret, 0.5).unwrap();
        assert!((turned.unit.yaw - (yaw + 0.5)).abs() < 1e-5);
        w.input.deploy = true;
        w.input.deploy_turn = 0.5;
        w.step_deploys();
        assert_eq!(w.deployables.last(), Some(&turned.unit));
        // A unit where one already stands is refused but still shown (gray).
        w.players[0].pack = Some(super::packs::PackKind::Turret);
        let blocked = w.aim_placement(0, DeployKind::Turret, 0.0).expect("still shown");
        assert!(blocked.problem.is_some(), "can't go there");
    }

    #[test]
    fn teams_are_limited_and_bad_spots_refused() {
        let mut w = world(DeployKind::Turret);
        let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Turret, 0.0).is_ok() }).unwrap();
        w.players[0].yaw = yaw;
        for k in 0..team_limit(DeployKind::Turret) {
            w.deployables.push(Deployable { owner: 0, kind: DeployKind::Turret, team: Team::Ember, pos: Vec3::new(10.0 + k as f32 * 5.0, 0.0, 10.0),
                yaw: 0.0, health: 150.0, cooldown: 0.0, aim: Vec3::Z, age: 0.0 });
        }
        assert_eq!(w.place(0, DeployKind::Turret, 0.0).err(), Some("YOUR TEAM HAS THE MAXIMUM OF THOSE DEPLOYED"));
        w.deployables.clear();
        w.players[0].pos = w.flags[0].home + Vec3::Y * 0.5;
        assert!(w.place(0, DeployKind::Turret, 0.0).is_err(), "not on the flag");
    }

    #[test]
    fn a_turret_shoots_enemies_takes_damage_and_blows_up() {
        let mut w = world(DeployKind::Turret);
        let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Turret, 0.0).is_ok() }).unwrap();
        w.players[0].yaw = yaw;
        w.input.deploy = true;
        w.step_deploys();
        let e = w.deployables[0].clone();
        let mut enemy = w.players[0].clone();
        enemy.team = Team::Glacier;
        enemy.pack = None;
        let (fwd, _) = move_basis(yaw);
        enemy.pos = e.pos + fwd * 20.0 + Vec3::Y * 1.0;
        w.players.push(enemy);
        w.input.deploy = false;
        for _ in 0..30 { w.step_deployables(STEP); }
        assert!(w.discs.iter().any(|d| d.owner == MAX_PLAYERS), "fired at the enemy");
        w.damage_deployable(0, 1000.0);
        w.step_deployables(STEP);
        assert!(w.deployables.is_empty());
        assert!(w.explosions.iter().any(|x| x.kind == 4));
    }

    #[test]
    fn an_ammo_station_restocks_teammates_who_use_it_and_blocks_like_a_crate() {
        let mut w = world(DeployKind::Ammo);
        let yaw = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Ammo, 0.0).is_ok() }).unwrap();
        w.players[0].yaw = yaw;
        w.input.deploy = true;
        w.step_deploys();
        let e = w.deployables[0].clone();
        assert_eq!(e.kind, DeployKind::Ammo);
        w.input.deploy = false;
        // Standing by it and using it refills; not using it, or an enemy, doesn't.
        let (fwd, _) = move_basis(yaw);
        w.players[0].pos = e.center() - fwd * 1.6;
        w.players[0].ammo.iter_mut().for_each(|n| *n = 0);
        w.step_ammo_stations();
        assert!(w.players[0].ammo.iter().all(|&n| n == 0), "hold E to use it");
        w.input.interact = true;
        w.step_ammo_stations();
        assert_eq!(w.players[0].ammo, loadout::max_ammo(w.players[0].armor));
        w.players[0].team = Team::Glacier;
        w.players[0].ammo.iter_mut().for_each(|n| *n = 0);
        w.step_ammo_stations();
        assert!(w.players[0].ammo.iter().all(|&n| n == 0), "not the other team's");
        // It's a solid crate: shots stop at it.
        assert!(w.blocked(e.center() - fwd * 3.0, e.center() + fwd * 3.0, 0.1, Some(Team::Ember)).is_some());
    }
}
