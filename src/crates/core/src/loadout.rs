//! Armor classes, ammunition, the repair tool, the mortar and inventory
//! purchases. Light armor keeps the approved movement; heavy armor has twice
//! the armor, a slow walk and a weak long jet. Every weapon
//! has limited ammunition, refilled at an inventory station. Marker: `loadout1`.
use super::*;

/// Which armor a player wears: an armor entry's wire number
/// (`crate::registry`). Chosen at an inventory station; kept across deaths.
/// Football ignores it (everyone wears the football armor). Its `Debug`
/// form is the entry's id, capitalized ("Light").
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArmorClass(pub u8);

#[allow(non_upper_case_globals)]
impl ArmorClass {
    /// The base game's light and heavy armor (fixed wire numbers).
    pub const Light: ArmorClass = ArmorClass(0);
    pub const Heavy: ArmorClass = ArmorClass(1);
    /// Its entry (the base light armor's, for a number no entry has).
    pub fn def(self) -> &'static crate::registry::Armor {
        let r = crate::registry::registry();
        r.armors.get(self.0 as usize).or_else(|| r.armors.get(0)).expect("the base game has a light armor")
    }
    pub fn id(self) -> &'static str { crate::registry::registry().armors.id(self.0 as usize).unwrap_or("light") }
    pub fn by_id(id: &str) -> Option<ArmorClass> { crate::registry::registry().armors.index(id).map(|i| ArmorClass(i as u8)) }
    /// Whether an entry has this number.
    pub fn exists(self) -> bool { crate::registry::registry().armors.get(self.0 as usize).is_some() }
}

impl std::fmt::Debug for ArmorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&capitalized(self.id())) }
}

/// "light" → "Light".
pub fn capitalized(id: &str) -> String {
    let mut c = id.chars();
    c.next().map(|first| first.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// An armor class's definition (its registry entry).
pub fn class_def(class: ArmorClass) -> &'static crate::registry::Armor { class.def() }
/// An armor's energy tank (its own, else the world's).
pub fn energy_max(class: ArmorClass) -> f32 { class.def().energy_max.unwrap_or(crate::defs::movement().energy_max) }
/// The share of damage of `kind` (a damage kind's id) an armor takes.
pub fn resist(class: ArmorClass, kind: &str) -> f32 { class.def().resist.get(kind).copied().unwrap_or(1.0) }
/// Walking speed, damage taken and mass relative to light armor.
pub fn walk_scale(class: ArmorClass) -> f32 { class_def(class).walk_scale }
pub fn damage_taken(class: ArmorClass) -> f32 { class_def(class).damage_taken }
pub fn mass_scale(class: ArmorClass) -> f32 { class_def(class).mass_scale }

/// Full rounds per weapon (by the weapon's wire number) for an armor: what
/// it carries of each weapon it may carry, 0 for the rest (and for weapons
/// paid with energy).
pub fn max_ammo(class: ArmorClass) -> Vec<u16> {
    let r = crate::registry::registry();
    let a = class.def();
    (0..r.weapons.len()).map(|k| r.weapons.id(k).and_then(|id| a.weapons.get(id)).copied().unwrap_or(0)).collect()
}

/// Full rounds per weapon for player `p`: their armor's, scaled by the
/// pack they wear (`registry::Pack::ammo_scale`).
pub fn rounds_max(p: &Player) -> Vec<u16> {
    let mut max = max_ammo(p.armor);
    let scale = p.pack.map_or(1.0, |k| k.def().ammo_scale);
    if scale != 1.0 { for n in &mut max { *n = (*n as f32 * scale).round().min(u16::MAX as f32) as u16; } }
    max
}

/// The base mortar's projectile number (its shell bounces until armed, as
/// every `shell` does: `registry::Projectile::arm`).
pub const MORTAR_KIND: u8 = 5;

/// An inventory purchase, sent as a command intent (`Command::buy`): wear
/// armor `a` (`BUY_ARMOR + a`), take weapon `w` into a free slot
/// (`BUY_TAKE + w`) or leave it (`BUY_DROP + w`), or take pack `d`
/// (`BUY_PACK + d`), each by its wire number (`crate::registry`).
pub const BUY_NONE: u16 = 0;
pub const BUY_ARMOR: u16 = 0x100;
pub const BUY_TAKE: u16 = 0x200;
pub const BUY_DROP: u16 = 0x300;
pub const BUY_PACK: u16 = 0x400;
/// The base game's purchases by name, as shortcuts.
pub const BUY_LIGHT: u16 = BUY_ARMOR;
pub const BUY_HEAVY: u16 = BUY_ARMOR + 1;
pub const BUY_TURRET: u16 = BUY_PACK;
pub const BUY_WALL: u16 = BUY_PACK + 1;
pub const BUY_FIELD: u16 = BUY_PACK + 2;
pub const BUY_AMMO: u16 = BUY_PACK + 3;
pub const BUY_INVENTORY: u16 = BUY_PACK + 4;
/// Take weapon `kind` into a free slot, or leave it.
pub fn buy_take(kind: u8) -> u16 { BUY_TAKE + kind as u16 }
pub fn buy_drop(kind: u8) -> u16 { BUY_DROP + kind as u16 }

/// A purchase, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purchase { Armor(ArmorClass), Take(u8), Drop(u8), Pack(packs::PackKind) }

/// What `buy` asks for, if it names something this session has.
pub fn purchase(buy: u16) -> Option<Purchase> {
    let r = crate::registry::registry();
    let n = (buy & 0xff) as usize;
    match buy & 0xff00 {
        BUY_ARMOR if r.armors.get(n).is_some() => Some(Purchase::Armor(ArmorClass(n as u8))),
        BUY_TAKE if r.weapons.get(n).is_some() => Some(Purchase::Take(n as u8)),
        BUY_DROP if r.weapons.get(n).is_some() => Some(Purchase::Drop(n as u8)),
        BUY_PACK if r.packs.get(n).is_some() => Some(Purchase::Pack(packs::PackKind(n as u8))),
        _ => None,
    }
}

/// Most degrees off a station's front a player may stand to use it: they
/// step in from the front, nearly square to it.
pub const FRONT_CONE: f32 = 30.0;

/// Whether `pos` is in front of a station whose use point is `at` and which
/// opens toward `front` (within `FRONT_CONE` of it, seen from 1 m behind the
/// use point).
pub fn in_front(pos: Vec3, at: Vec3, front: Vec3) -> bool {
    let pivot = at - front;
    Vec3::new(pos.x - pivot.x, 0.0, pos.z - pivot.z).normalize_or_zero().dot(front) >= FRONT_CONE.to_radians().cos()
}

/// The way inventory station `index` on `map` opens: the map's `facing` for
/// it, else found from the map itself: the mean of the directions a probe
/// from its use point finds open for 4 m (a booth is walled behind). None
/// for one standing in the open, which serves from any side. Cached.
pub fn station_front(map: MapId, index: usize) -> Option<Vec3> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    static FRONTS: LazyLock<Mutex<HashMap<(MapId, usize), Option<Vec3>>>> = LazyLock::new(Default::default);
    let d = crate::equipment::definitions(map).get(index)?;
    if let Some([x, z]) = d.facing { return Vec3::new(x, 0.0, z).try_normalize(); }
    *FRONTS.lock().unwrap_or_else(|e| e.into_inner()).entry((map, index)).or_insert_with(|| {
        const DIRECTIONS: usize = 32;
        let at = d.pos() + Vec3::Y * 0.5;
        let mut open = Vec3::ZERO;
        for k in 0..DIRECTIONS {
            let a = k as f32 / DIRECTIONS as f32 * std::f32::consts::TAU;
            let dir = Vec3::new(a.sin(), 0.0, a.cos());
            let end = at + dir * 4.0;
            let blocked = crate::terrain::segment_hit(map, at, end, 0.2).is_some()
                || crate::map_pack::on(map).and_then(|p| p.sweep(at, end, 0.2)).is_some();
            if !blocked { open += dir; }
        }
        let mean = open / DIRECTIONS as f32;
        (mean.length() > 0.2).then(|| mean.normalize())
    })
}

/// Whether a command's `buy` names a purchase.
pub fn valid_buy(buy: u16) -> bool {
    use super::vehicles::BUY_VEHICLE;
    buy == BUY_NONE || purchase(buy).is_some()
        || (BUY_VEHICLE..BUY_VEHICLE + 0x100).contains(&buy) && crate::registry::registry().vehicles.get((buy - BUY_VEHICLE) as usize).is_some()
}

/// The purchase that takes a pack.
pub fn buy_for(kind: packs::PackKind) -> u16 { BUY_PACK + kind.0 as u16 }

/// A whole setup equipped at once (a saved favorite), as a command intent:
/// armor (0 keep, else its wire number + 1), weapons, and a pack (0 none,
/// else its wire number + 1). Equipping the same twice changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equip { pub armor: u8, pub weapons: arsenal::Loadout, pub pack: u8 }

impl Equip {
    pub fn valid(&self) -> bool {
        let r = crate::registry::registry();
        (self.armor == 0 || r.armors.get(self.armor as usize - 1).is_some()) && self.weapons.valid()
            && (self.pack == 0 || r.packs.get(self.pack as usize - 1).is_some())
    }
    /// The armor it puts on, if any.
    pub fn armor(&self) -> Option<ArmorClass> { (self.armor > 0).then(|| ArmorClass(self.armor - 1)) }
    /// The pack it takes, if any.
    pub fn pack(&self) -> Option<packs::PackKind> { (self.pack > 0).then(|| packs::PackKind(self.pack - 1)) }
}

/// An ammo pack dropped where a player died, drawn from what they carried
/// (`World::drop_loot`), for anyone who runs over it, gone after
/// LOOT_SECONDS. Rounds are by weapon (wire number), so a picker takes only
/// rounds for weapons their armor carries (a mortar's shells never fit a
/// grenade launcher). Marker: `loot2`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Loot {
    pub pos: Vec3,
    /// Rounds per weapon, by wire number.
    pub ammo: Vec<u16>,
    pub age: f32,
    /// Hand grenades (never mines).
    #[serde(default)]
    pub grenades: u8,
    /// What else lies there (`items`): an item, a weapon, a pack (each its
    /// wire number + 1; 0 none), and who dropped it (their slot + 1).
    #[serde(default)]
    pub item: u8,
    #[serde(default)]
    pub weapon: u8,
    #[serde(default)]
    pub pack: u8,
    #[serde(default)]
    pub owner: u8,
    /// Dropped by a dead body still on its way down: it rides the body
    /// (`owner`) until the body comes to rest, then lies under it
    /// (`World::step_carried_loot`, marker `fall1`). It used to appear on
    /// the ground under wherever the player died, mid-air or not.
    #[serde(default)]
    pub carried: bool,
}

impl Loot {
    /// Whether anything is left to take.
    pub fn holds(&self) -> bool { self.ammo.iter().any(|&n| n > 0) || self.grenades > 0 || self.item > 0 || self.weapon > 0 || self.pack > 0 }
    /// The body it rides down, while it does.
    pub fn rider_of(&self) -> Option<usize> { self.carried.then(|| (self.owner as usize).checked_sub(1)).flatten() }
}
/// Loot draws 1..=LOOT_ROLL discs and third-weapon rounds, and up to this
/// many hand grenades (at least one), from what the dead carried.
pub const LOOT_ROLL: u16 = 5;
pub const LOOT_SECONDS: f32 = 20.0;
pub const LOOT_REACH: f32 = 2.0;

/// What a repair beam is working on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Repairing { Player(usize), Equipment(usize), Deployable(usize) }

impl World {
    /// The armor player `i` wears now (football overrides the class).
    pub fn armor_of(&self, i: usize) -> Armor {
        if self.mode.def().movement == crate::modes::MovementModel::Classic { return crate::defs::defs().armors.football; }
        class_def(self.players.get(i).map_or(ArmorClass::Light, |p| p.armor)).movement
    }

    /// Apply `amount` of damage of `kind` (a damage kind's id) to player
    /// `i`, scaled by their armor and its resistance to that kind.
    pub(super) fn hurt(&mut self, i: usize, amount: f32, kind: &str) {
        // No harm in a map's safe zone (`features`).
        if self.in_safe_zone(i) { return; }
        let armor = self.players[i].armor;
        let (taken, scale, resist) = if self.mode.def().armor_scaling {
            let taken = damage_taken(armor);
            (taken, taken * self.twist_damage(), resist(armor, kind))
        } else { (1.0, 1.0, 1.0) };
        // A shield pays from energy first, priced by the hit before armor; a
        // hit ends what being hit reveals.
        let damage = self.shielded(i, amount * scale * resist, kind, taken);
        self.players[i].health -= damage;
        self.reveal(i);
        if damage > 0.0 { self.rabbit_hurt(i, self.hit_by); }
        if self.hooks.live && damage > 0.0 {
            self.hook(self.armor_hook(i), super::hooks::HookEvent::Damage { victim: i, attacker: self.hit_by, amount: damage, kind: kind.to_string() });
        }
    }

    /// Whether player `i` may wear `class`: below its team cap among their
    /// teammates.
    pub(super) fn armor_fits(&self, i: usize, class: ArmorClass) -> bool {
        let cap = class.def().team_cap as usize;
        let team = self.players[i].team;
        cap == 0 || self.players.iter().enumerate().filter(|(j, q)| *j != i && q.team == team && q.armor == class).count() < cap
    }

    /// Full ammunition for the player's armor.
    pub(super) fn refill(&mut self, i: usize) {
        self.players[i].ammo = rounds_max(&self.players[i]);
        World::restock_throwables(&mut self.players[i]);
    }

    /// A dying player drops a pack of what they carried: 1-5 discs and 1-5
    /// third-weapon rounds (never more than they had), all their chaingun
    /// rounds, and 1-5 hand grenades (at least one, even if they had none;
    /// never more than they had otherwise). No mines, no railgun slugs.
    pub(super) fn drop_loot(&mut self, i: usize) {
        if self.predicting || !self.mode.def().loot { return; }
        let roll = |w: &mut Self, have: u16| -> u16 {
            let n = 1 + ((w.rng() * LOOT_ROLL as f32) as u16).min(LOOT_ROLL - 1);
            n.min(have)
        };
        let have = self.players[i].ammo.clone();
        let carried = self.players[i].throwables[0] as u16;
        // Each weapon the armor carries drops as its entry says: a roll of
        // 1-5 rounds, all of them, or none (in wire-number order).
        let r = crate::registry::registry();
        let armor = self.players[i].armor.def();
        let mut ammo = vec![0u16; r.weapons.len()];
        for (k, id, w) in r.weapons.iter() {
            if !armor.weapons.contains_key(id) { continue; }
            let held = have.get(k).copied().unwrap_or(0);
            ammo[k] = match w.loot { crate::registry::Loot::Roll => roll(self, held), crate::registry::Loot::All => held, crate::registry::Loot::None => 0 };
        }
        let grenades = roll(self, carried.max(1)).max(1) as u8;
        // Dropped with the body, it falls with it (`body_drop`).
        let (pos, carried, owner) = self.body_drop(i, 0);
        self.loot.push(Loot { pos, ammo, age: 0.0, grenades, carried, owner, ..Default::default() });
        if self.loot.len() > 24 { self.loot.remove(0); }
    }

    /// Loot ages out, and anyone alive running over it takes what they can carry.
    pub(super) fn step_loot(&mut self, dt: f32) {
        if self.predicting { return; }
        for l in &mut self.loot { l.age += dt; }
        self.loot.retain(|l| l.age < LOOT_SECONDS);
        self.step_carried_loot();
        for k in 0..self.loot.len() {
            for i in 0..self.players.len() {
                let p = &self.players[i];
                if !p.alive || p.pos.distance(self.loot[k].pos + Vec3::Y * 0.5) > LOOT_REACH { continue; }
                // Not straight back into the hands that dropped it (`items`).
                if self.loot[k].owner as usize == i + 1 && self.loot[k].age < super::items::OWN_DROP_WAIT { continue; }
                let max = rounds_max(p);
                let mut took = false;
                let room = (throwables::max_throwables(p.armor)[0]).saturating_sub(p.throwables[0]);
                let take = room.min(self.loot[k].grenades);
                if take > 0 {
                    self.players[i].throwables[0] += take;
                    self.loot[k].grenades -= take;
                    took = true;
                }
                // Rounds only for weapons this armor carries.
                for w in 0..self.loot[k].ammo.len().min(max.len()) {
                    let have = self.players[i].ammo.get(w).copied().unwrap_or(0);
                    let room = max[w].saturating_sub(have);
                    let take = room.min(self.loot[k].ammo[w]);
                    if take > 0 {
                        if self.players[i].ammo.len() <= w { self.players[i].ammo.resize(w + 1, 0); }
                        self.players[i].ammo[w] += take;
                        self.loot[k].ammo[w] -= take;
                        took = true;
                    }
                }
                if took && i == self.player_id { self.push_event("buy"); }
                // An item, weapon or pack lying there too (`items`).
                if self.loot[k].item > 0 || self.loot[k].weapon > 0 || self.loot[k].pack > 0 { self.take_extras(i, k); }
            }
        }
        // Chaingun rounds or third-weapon rounds nobody here can use still
        // lie there until someone can, or the pack times out.
        self.loot.retain(Loot::holds);
    }

    /// Repair tool for every player holding it, once per physics step. The
    /// tool stays out while its key is held. Running the tank dry rests it
    /// for `repair().rest` seconds, the tank refilling meanwhile, before it
    /// tries again: it used to restart on each tick's recharge, flickering
    /// between the tool and the weapon on an empty tank.
    pub(super) fn step_repair(&mut self, dt: f32) {
        if self.predicting { return; }
        let allowed = self.mode.def().repair_tool;
        for i in 0..self.players.len() {
            self.players[i].repair_beam = None;
            self.players[i].repair_self = false;
            let wants = if self.players[i].is_bot { self.players[i].bot_repair }
                else if self.network_inputs.is_empty() { i == self.player_id && self.input.repair }
                else { self.network_inputs.get(i).is_some_and(|c| c.repair) };
            let p = &mut self.players[i];
            p.repair_rest = (p.repair_rest - dt).max(0.0);
            p.repair_out = wants && p.alive && allowed;
            if !p.repair_out || p.stun > 0.0 || p.repair_rest > 0.0 { continue; }
            if p.energy < crate::defs::repair().min_energy { p.repair_rest = crate::defs::repair().rest; continue; }
            let target = self.repair_target(i);
            let spend = (crate::defs::repair().energy * dt).min(self.players[i].energy);
            let work = spend / crate::defs::repair().energy;
            let at = match target {
                Some(Repairing::Player(j)) => {
                    let q = &mut self.players[j];
                    if q.health >= 100.0 { continue; }
                    q.health = (q.health + crate::defs::repair().player * work).min(100.0);
                    q.pos + Vec3::Y * 0.9
                }
                Some(Repairing::Equipment(k)) => {
                    let d = &crate::equipment::definitions(self.map)[k];
                    if self.equipment[k].health >= d.max_health() { continue; }
                    self.equipment[k].repair(d, crate::defs::repair().hull * work);
                    d.pos()
                }
                Some(Repairing::Deployable(k)) => {
                    let e = &mut self.deployables[k];
                    let max = deploy::max_health(e.kind);
                    if e.health >= max { continue; }
                    e.health = (e.health + crate::defs::repair().hull * work).min(max);
                    e.center()
                }
                None => {
                    // Team Rabbit's rabbit can't mend themselves (`rabbit`).
                    if self.is_rabbit(i) && self.mode.rules() == crate::modes::Rules::TeamRabbit { continue; }
                    let q = &mut self.players[i];
                    if q.health >= 100.0 { continue; }
                    q.health = (q.health + crate::defs::repair().player * work).min(100.0);
                    q.repair_self = true;
                    q.pos + Vec3::Y * 0.9
                }
            };
            self.players[i].energy -= spend;
            self.players[i].repair_beam = Some(at);
        }
    }

    /// The nearest friendly thing within REPAIR_RANGE under player `i`'s
    /// crosshair and in sight: a teammate, equipment or a deployable.
    fn repair_target(&self, i: usize) -> Option<Repairing> {
        let p = &self.players[i];
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(p.yaw, p.pitch);
        let team = p.team;
        let mut best: Option<(f32, Repairing)> = None;
        let mut consider = |at: Vec3, reach: f32, what: Repairing| {
            let to = at - eye;
            let along = to.dot(dir);
            if along <= 0.0 || along > crate::defs::repair().range || (to - dir * along).length() > reach { return; }
            if best.is_some_and(|(d, _)| d <= along) { return; }
            best = Some((along, what));
        };
        for (j, o) in self.players.iter().enumerate() {
            if j != i && o.alive && !self.hostile(i, team, j) { consider(o.pos + Vec3::Y * 0.8, 1.0, Repairing::Player(j)); }
        }
        for (k, d) in crate::equipment::definitions(self.map).iter().enumerate() {
            if d.team as usize == team.idx() { consider(d.pos(), d.radius + 0.5, Repairing::Equipment(k)); }
        }
        for (k, e) in self.deployables.iter().enumerate() {
            if e.team == team { consider(e.center(), 1.6, Repairing::Deployable(k)); }
        }
        let (_, what) = best?;
        // Where to aim, and how far from that point its own surface is: the
        // first thing the beam meets may be the target itself (a generator's
        // shell is 2.8 m out from its centre).
        let (at, surface) = match what {
            Repairing::Player(j) => (self.players[j].pos + Vec3::Y * 0.8, 1.0),
            Repairing::Equipment(k) => { let d = &crate::equipment::definitions(self.map)[k]; (d.pos(), d.radius) }
            Repairing::Deployable(k) => (self.deployables[k].center(), 1.2),
        };
        let reach = eye.distance(at);
        let clear = obstacle_hit(self.map, &self.pillars, eye, at, 0.0).is_none_or(|t| t * reach > reach - surface - 0.8);
        clear.then_some(what)
    }

    /// Inventory purchases: at their team's powered inventory station a
    /// player may change armor (refilling ammunition), choose the weapons in
    /// their slots, take a deployable pack, or equip a saved favorite; a
    /// field inventory serves the same except armor and the station packs.
    /// Each choice joins their `Setup`.
    pub(super) fn step_purchases(&mut self) {
        if self.predicting || !self.mode.def().purchases { return; }
        for i in 0..self.players.len() {
            let (buy, equip) = if self.players[i].is_bot { (BUY_NONE, None) }
                else if self.network_inputs.is_empty() { if i == self.player_id { (self.input.buy, self.input.equip) } else { (BUY_NONE, None) } }
                else { self.network_inputs.get(i).map_or((BUY_NONE, None), |c| (c.buy, c.equip)) };
            if (buy == BUY_NONE && equip.is_none()) || !self.players[i].alive { continue; }
            let full = self.at_inventory(i);
            if !full && !self.at_field_inventory(i) { continue; }
            let mut bought = equip.is_some_and(|e| self.equip_favorite(i, e, full));
            bought |= buy != BUY_NONE && self.buy(i, buy, full);
            if bought && i == self.player_id { self.push_event("buy"); }
            if bought && self.hooks.live { self.hook(None, super::hooks::HookEvent::Buy { player: i, what: buy }); }
        }
    }

    /// Change player `i`'s armor (keeping the weapons that fit: each the
    /// new armor's of the same family).
    fn wear(&mut self, i: usize, class: ArmorClass) {
        let p = &mut self.players[i];
        p.setup.armor = Some(class);
        p.setup.weapons = p.setup.weapons.map(|w| w.for_armor(class).fitted(arsenal::slots(class), |_| true));
        if p.armor != class {
            p.armor = class;
            p.energy = p.energy.min(energy_max(class));
            self.ball_lost(i);
        }
        // A pack the new armor may not wear comes off (`armors/<id>.packs`).
        if self.players[i].pack.is_some_and(|k| !k.fits(class)) { self.wear_pack(i, None); }
    }

    /// The weapons player `i` has chosen, as they see them now.
    fn chosen(&self, i: usize) -> arsenal::Loadout {
        let p = &self.players[i];
        let def = self.mode.def();
        p.setup.weapons.unwrap_or_else(|| arsenal::Loadout::kit(p.armor)).for_armor(p.armor)
            .fitted(arsenal::slots(p.armor), |k| def.allows(k))
    }

    /// One purchase; false when it changes nothing.
    fn buy(&mut self, i: usize, buy: u16, full: bool) -> bool {
        match purchase(buy) {
            Some(Purchase::Armor(class)) if full && self.mode.def().offers_armor(class) => {
                if !self.armor_fits(i, class) {
                    if i == self.player_id { self.msg("YOUR TEAM HAS THE MOST OF THAT ARMOR IT MAY", 1.6); }
                    return false;
                }
                if !self.pay_for(i, class.def().price) { return false; }
                self.wear(i, class);
                self.refill(i);
                self.equip(i);
            }
            Some(Purchase::Take(kind)) => {
                let armor = self.players[i].armor;
                if !self.mode.def().allows(kind) || !arsenal::carries(armor, kind) { return false; }
                let mut chosen = self.chosen(i);
                if chosen.carries(kind) || chosen.len() >= arsenal::slots(armor) { return false; }
                let price = crate::registry::registry().weapons.get(kind as usize).map_or(0, |w| w.price);
                if !self.pay_for(i, price) { return false; }
                if !chosen.take(kind, arsenal::slots(armor)) { return false; }
                self.players[i].setup.weapons = Some(chosen);
                self.equip(i);
            }
            Some(Purchase::Drop(kind)) => {
                let mut chosen = self.chosen(i);
                if !chosen.carries(kind) { return false; }
                chosen.remove(kind);
                self.players[i].setup.weapons = Some(chosen);
                self.equip(i);
            }
            Some(Purchase::Pack(kind)) => {
                // Stations come only from a station.
                if !self.pack_offered(kind, full) || !self.mode.def().offers_pack_to(kind, self.players[i].armor) { return false; }
                if self.players[i].pack != Some(kind) && !self.pay_for(i, kind.def().price) { return false; }
                self.wear_pack(i, Some(kind));
            }
            _ => return false,
        }
        true
    }

    /// Pay `price` for player `i`'s purchase (`items`), saying so when they
    /// can't.
    fn pay_for(&mut self, i: usize, price: u32) -> bool {
        if self.pay(i, price) { return true; }
        if i == self.player_id { self.msg("NOT ENOUGH CREDITS", 1.6); }
        false
    }

    /// Whether pack `kind` is handed out here: offered, allowed by the mode
    /// (one that places needs deployables), and at a full station when only
    /// a base station hands it out.
    pub fn pack_offered(&self, kind: packs::PackKind, full: bool) -> bool {
        let d = kind.def();
        self.mode.def().offers_pack(kind) && (full || !d.base_station_only) && (self.mode.def().deployables || kind.deployable().is_none())
    }

    /// Player `i` wears `pack` from now on (and at every respawn): fresh,
    /// switched off, with its rounds when it changes how many they carry.
    fn wear_pack(&mut self, i: usize, pack: Option<packs::PackKind>) {
        let p = &mut self.players[i];
        let rounds = p.pack.map_or(1.0, |k| k.def().ammo_scale) != pack.map_or(1.0, |k| k.def().ammo_scale);
        if p.pack != pack { (p.pack_on, p.pack_clock, p.pack_uses) = (false, 0.0, 0); }
        p.pack = pack;
        p.setup.pack = pack;
        if rounds {
            let max = rounds_max(p);
            for (n, m) in p.ammo.iter_mut().zip(&max) { *n = (*n).min(*m); }
        }
    }

    /// Equip a saved favorite whole (armor only at a full station).
    fn equip_favorite(&mut self, i: usize, e: Equip, full: bool) -> bool {
        if !e.valid() { return false; }
        // With an economy, a favorite costs what it changes (`items`).
        let r = crate::registry::registry();
        let p = &self.players[i];
        let armor = e.armor().filter(|c| full && self.mode.def().offers_armor(*c) && self.armor_fits(i, *c) && *c != p.armor);
        let fresh = e.weapons.kinds().filter(|&k| !p.loadout.carries(k)).filter_map(|k| r.weapons.get(k as usize)).map(|w| w.price).sum::<u32>();
        let pack = e.pack().filter(|k| p.pack != Some(*k)).map_or(0, |k| k.def().price);
        if !self.pay_for(i, armor.map_or(0, |c| c.def().price) + fresh + pack) { return false; }
        if let Some(class) = armor { self.wear(i, class); }
        let p = &self.players[i];
        let def = self.mode.def();
        self.players[i].setup.weapons = Some(e.weapons.for_armor(p.armor).fitted(arsenal::slots(p.armor), |k| def.allows(k)));
        let pack = e.pack().filter(|k| self.pack_offered(*k, full) && self.mode.def().offers_pack_to(*k, self.players[i].armor));
        if e.pack == 0 || pack.is_some() { self.wear_pack(i, pack); }
        self.refill(i);
        self.equip(i);
        true
    }

    /// Whether player `i` stands at one of their team's powered inventory
    /// stations, stepped in from its front (`station_front`).
    pub fn at_inventory(&self, i: usize) -> bool { self.inventory_reach(i) == Some(true) }

    /// At one of their team's powered inventory stations: Some(true) from
    /// its front, Some(false) beside or behind it (it won't open), None away.
    pub fn inventory_reach(&self, i: usize) -> Option<bool> {
        let p = &self.players[i];
        let mut beside = false;
        for (k, (d, s)) in crate::equipment::definitions(self.map).iter().zip(&self.equipment).enumerate() {
            // In Deathmatch there are no sides: any station serves anyone.
            if d.kind != crate::equipment::Kind::Inventory || !(self.ffa() || d.team as usize == p.team.idx()) || !s.powered || s.health <= 0.0
                || p.pos.distance(d.pos()) > d.radius + 1.5 { continue; }
            if station_front(self.map, k).is_none_or(|f| in_front(p.pos, d.pos(), f)) { return Some(true); }
            beside = true;
        }
        beside.then_some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// Rounds by weapon wire number, padded with zeros to every weapon.
    fn rounds(first: &[u16]) -> Vec<u16> {
        let mut v = first.to_vec();
        v.resize(crate::registry::registry().weapons.len(), 0);
        v
    }

    fn solo(armor: ArmorClass) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        w.players[0].armor = armor;
        w.refill(0);
        w.equip(0);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(MapId::OldHoller, spot + Vec3::Y * 200.0).0;
        w.players[0].pos = Vec3::new(spot.x, floor + PLAYER_RADIUS, spot.z);
        w
    }

    #[test]
    fn heavy_armor_takes_half_damage_walks_slower_and_climbs_slower() {
        let mut light = solo(ArmorClass::Light);
        let mut heavy = solo(ArmorClass::Heavy);
        light.hurt(0, 40.0, "blast");
        heavy.hurt(0, 40.0, "blast");
        assert_eq!((light.players[0].health, heavy.players[0].health), (60.0, 80.0));
        for w in [&mut light, &mut heavy] {
            w.players[0].health = 100.0;
            w.input.move_z = 1.0;
            for _ in 0..90 { w.physics_step(); }
        }
        let flat = |w: &World| Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length();
        assert!(flat(&heavy) < flat(&light) * 0.7, "heavy walks {} vs {}", flat(&heavy), flat(&light));
        for w in [&mut light, &mut heavy] {
            w.input.move_z = 0.0;
            w.players[0].vel = Vec3::ZERO;
            w.players[0].energy = crate::defs::movement().energy_max;
            w.input.jet = true;
            // Jets already held: the jet alone, without the jump-boost.
            w.players[0].jet_prev = true;
            for _ in 0..60 { w.physics_step(); }
        }
        assert!(heavy.players[0].vel.y < light.players[0].vel.y * 0.4, "heavy jet barely climbs: {} vs {}",
            heavy.players[0].vel.y, light.players[0].vel.y);
        assert!(heavy.players[0].energy > light.players[0].energy, "but burns its tank slower");
        // Jetting sideways from a stand, heavy tops out far below light.
        for w in [&mut light, &mut heavy] {
            w.players[0].vel = Vec3::ZERO;
            w.players[0].energy = crate::defs::movement().energy_max;
            w.players[0].pos.y += 30.0;
            w.input.move_z = 1.0;
            for _ in 0..150 { w.physics_step(); }
        }
        let air = |w: &World| Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length();
        assert!(air(&heavy) <= class_def(ArmorClass::Heavy).movement.air_cap + 0.5 && air(&light) > 15.0, "air speed {} vs {}", air(&heavy), air(&light));
    }

    #[test]
    fn ammo_runs_out_and_clicks_until_an_inventory_restocks_it() {
        let mut w = solo(ArmorClass::Light);
        assert_eq!(w.players[0].ammo, max_ammo(ArmorClass::Light));
        w.players[0].ammo[0] = 2;
        w.input.fire = true;
        w.input.weapon = 0;
        for _ in 0..(4.0 / STEP) as usize { w.tick(STEP); }
        assert_eq!(w.players[0].ammo[0], 0);
        assert_eq!(w.players[0].shots, 2, "two discs, then nothing");
        // Heavy carries more of each family (its own launcher and rifle).
        let rounds = |a: ArmorClass, family: &str| arsenal::family_weapon(a, family).map_or(0, |k| max_ammo(a)[k as usize]);
        assert!(arsenal::KIND_KEYS.iter().all(|f| rounds(ArmorClass::Heavy, f) >= rounds(ArmorClass::Light, f))
            && rounds(ArmorClass::Heavy, "disc") > rounds(ArmorClass::Light, "disc")
            && rounds(ArmorClass::Heavy, "chaingun") > rounds(ArmorClass::Light, "chaingun"), "heavy carries more");
    }

    #[test]
    fn a_mortar_lobs_far_and_blows_up_on_impact() {
        let mut w = solo(ArmorClass::Heavy);
        w.input.weapon = arsenal::MORTAR;
        w.players[0].pitch = 0.35;
        let from = w.players[0].pos;
        w.input.fire = true;
        w.tick(STEP * 2.0);
        w.input.fire = false;
        let m = arsenal::MORTAR as usize;
        assert_eq!(w.players[0].ammo[m], max_ammo(ArmorClass::Heavy)[m] - 1);
        assert!(w.discs.iter().any(|d| d.kind == MORTAR_KIND), "a shell is in the air");
        for _ in 0..(12.0 / STEP) as usize {
            w.physics_step();
            if w.explosions.iter().any(|e| e.kind == MORTAR_KIND) { break; }
        }
        let blast = w.explosions.iter().find(|e| e.kind == MORTAR_KIND).expect("it exploded");
        let range = Vec2::new(blast.pos.x - from.x, blast.pos.z - from.z).length();
        assert!(range > 120.0, "a long lob: {range} m");
        assert!(!w.discs.iter().any(|d| d.kind == MORTAR_KIND), "on impact, nothing left lying");
        let blasts = &crate::defs::defs().blasts;
        assert!(blasts.mortar.max_damage > blasts.grenade.max_damage);
        assert!(blasts.mortar.radius > blasts.grenade.radius);
    }

    #[test]
    fn a_fresh_mortar_bounces_an_armed_one_explodes() {
        for armed in [false, true] {
            let mut w = solo(ArmorClass::Heavy);
            let at = w.players[0].pos + Vec3::new(15.0, 0.0, 0.0);
            let floor = crate::terrain::support_on(w.map, at + Vec3::Y * 5.0).0;
            let life = if armed { crate::defs::weapons().mortar.life - 1.0 } else { crate::defs::weapons().mortar.life };
            w.discs.push(Disc { pos: Vec3::new(at.x, floor + 0.6, at.z), vel: Vec3::new(0.0, -30.0, 0.0), team: Team::Ember,
                owner: 0, life, kind: MORTAR_KIND, spin: 0.0, target: 0, power: 1.0 });
            w.physics_step();
            if armed {
                assert!(w.explosions.iter().any(|e| e.kind == MORTAR_KIND), "armed: explodes on contact");
            } else {
                let d = w.discs.iter().find(|d| d.kind == MORTAR_KIND).expect("launch safety: it bounced");
                assert!(d.vel.y > 0.0, "off the floor");
                assert!(w.explosions.is_empty());
            }
        }
    }

    #[test]
    fn a_setup_bought_at_a_station_comes_back_at_every_respawn() {
        let (energy, wall) = (packs::PackKind::from_key("energy").unwrap(), packs::PackKind::Wall);
        let mut w = solo(ArmorClass::Light);
        // Whether they carry their armor's rifle (laser or railgun).
        let has_rifle = |w: &World| w.players[0].loadout.carries(arsenal::RIFLE) || w.players[0].loadout.carries(arsenal::RAILGUN);
        let setup = |w: &World| (w.players[0].armor, has_rifle(w), w.players[0].pack);
        // Never bought: spawns as before.
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(setup(&w), (ArmorClass::Light, false, None));
        let defs = crate::equipment::definitions(MapId::OldHoller);
        let station = defs[defs.iter().position(|d| d.kind == crate::equipment::Kind::Inventory && d.team == 0).unwrap()].pos();
        let shop = |w: &mut World, buys: &[u16]| {
            w.players[0].team = Team::Ember;
            w.players[0].pos = station;
            for &buy in buys { w.input.buy = buy; w.step_purchases(); }
            w.input.buy = BUY_NONE;
        };
        shop(&mut w, &[BUY_HEAVY, buy_take(arsenal::RAILGUN), BUY_WALL]);
        for _ in 0..3 {
            w.players[0].ammo.iter_mut().for_each(|n| *n = 0);
            w.kill(0, None, "Fall");
            assert_eq!((has_rifle(&w), w.players[0].pack), (false, None), "lost on death");
            w.respawn(0);
            assert_eq!(setup(&w), (ArmorClass::Heavy, true, Some(wall)));
            assert_eq!(w.players[0].ammo, max_ammo(ArmorClass::Heavy), "restocked for it");
        }
        // Changed again: each choice replaces only its own part (light
        // armor's three slots keep the first three weapons). Light armor
        // can't wear the wall or a turret (`armors/light.packs`): the wall
        // comes off, the turret is refused, the energy pack goes on.
        shop(&mut w, &[BUY_LIGHT]);
        assert_eq!(w.players[0].pack, None, "the wall came off");
        shop(&mut w, &[BUY_TURRET]);
        assert_eq!(w.players[0].pack, None, "no turret for light armor");
        shop(&mut w, &[buy_for(energy)]);
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(setup(&w), (ArmorClass::Light, false, Some(energy)));
        shop(&mut w, &[buy_drop(arsenal::LAUNCHER), buy_take(arsenal::RIFLE)]);
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(setup(&w), (ArmorClass::Light, true, Some(energy)));
        // Only what the mode sells: no packs that place things without
        // deployables (a worn one stays), nothing where there are no purchases.
        let variant = |key: &str, change: &dyn Fn(&mut crate::modes::ModeDef)| {
            let mut def = SupportedMode::Ctf.def().clone();
            change(&mut def);
            SupportedMode::from_index(crate::modes::add(key, def).unwrap())
        };
        w.mode = variant("test-setup-no-packs", &|d| d.deployables = false);
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(setup(&w), (ArmorClass::Light, true, Some(energy)), "worn, not placed");
        w.players[0].setup.pack = packs::PackKind::from_key("motion-sensor");
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(setup(&w), (ArmorClass::Light, true, None), "placed");
        w.mode = variant("test-setup-no-shop", &|d| d.purchases = false);
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!((has_rifle(&w), w.players[0].pack), (false, None));
    }

    #[test]
    fn weapons_fill_the_armor_s_slots_and_favorites_equip_whole_setups() {
        use arsenal::*;
        let mut w = solo(ArmorClass::Light);
        let defs = crate::equipment::definitions(MapId::OldHoller);
        let station = defs[defs.iter().position(|d| d.kind == crate::equipment::Kind::Inventory && d.team == 0).unwrap()].pos();
        w.players[0].team = Team::Ember;
        w.players[0].pos = station;
        let shop = |w: &mut World, buys: &[u16]| {
            for &buy in buys { w.input.buy = buy; w.step_purchases(); }
            w.input.buy = BUY_NONE;
        };
        let carried = |w: &World| w.players[0].loadout.kinds().collect::<Vec<_>>();
        assert_eq!(carried(&w), [DISC, CHAINGUN, LAUNCHER], "the default fills light armor's three slots");
        shop(&mut w, &[buy_take(PLASMA)]);
        assert_eq!(carried(&w), [DISC, CHAINGUN, LAUNCHER], "full: leave one first");
        shop(&mut w, &[buy_drop(LAUNCHER), buy_take(PLASMA)]);
        assert_eq!(carried(&w), [DISC, CHAINGUN, PLASMA], "slot 3 is the plasma gun");
        assert_eq!(w.players[0].ammo[PLASMA as usize], max_ammo(ArmorClass::Light)[PLASMA as usize], "stocked");
        shop(&mut w, &[BUY_HEAVY, buy_take(BLASTER)]);
        assert_eq!(carried(&w), [DISC, CHAINGUN, PLASMA, BLASTER], "heavy armor's fourth slot");
        shop(&mut w, &[BUY_LIGHT]);
        assert_eq!(carried(&w), [DISC, CHAINGUN, PLASMA], "back to three: the last one is left behind");
        shop(&mut w, &[BUY_HEAVY]);
        assert_eq!(carried(&w), [DISC, CHAINGUN, PLASMA]);
        // A favorite: the whole setup at once, and again changes nothing.
        // Its weapons become the armor's own of each family (the rifle is the
        // heavy's railgun).
        let favorite = Equip { armor: ArmorClass::Heavy.0 + 1, weapons: Loadout::of(&[RIFLE, PLASMA, DISC, BLASTER]),
            pack: packs::PackKind::Wall.0 + 1 };
        for _ in 0..2 {
            w.input.equip = Some(favorite);
            w.step_purchases();
            w.input.equip = None;
            assert_eq!((w.players[0].armor, carried(&w), w.players[0].pack),
                (ArmorClass::Heavy, vec![RAILGUN, PLASMA, DISC, BLASTER], Some(packs::PackKind::Wall)));
        }
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!((w.players[0].armor, carried(&w), w.players[0].pack),
            (ArmorClass::Heavy, vec![RAILGUN, PLASMA, DISC, BLASTER], Some(packs::PackKind::Wall)), "and it's the setup now");
        // A light favorite keeps the first three, and leaves the wall,
        // which light armor can't wear.
        w.players[0].pos = station;
        w.input.equip = Some(Equip { armor: ArmorClass::Light.0 + 1, ..favorite });
        w.step_purchases();
        w.input.equip = None;
        assert_eq!((carried(&w), w.players[0].pack), (vec![RIFLE, PLASMA, DISC], None));
        // A field inventory: weapons and small packs, never armor or stations.
        w.players[0].pos = Vec3::new(1060.0, w.players[0].pos.y, 700.0);
        assert!(!w.at_inventory(0) && !w.at_field_inventory(0));
        // Placed facing +z (yaw 0): from beside it, it won't open; from the front it does.
        w.deployables.push(deploy::Deployable { owner: 0, kind: deploy::DeployKind::Inventory, team: Team::Ember, pos: w.players[0].pos + Vec3::new(1.4, -0.5, 0.0),
            yaw: 0.0, health: 300.0, cooldown: 0.0, aim: Vec3::ZERO, age: 0.0 });
        assert_eq!(w.field_inventory_reach(0), Some(false), "beside it");
        w.deployables[0].pos = w.players[0].pos + Vec3::new(0.0, -0.5, -1.6);
        assert!(w.at_field_inventory(0), "in front of it");
        shop(&mut w, &[BUY_HEAVY, BUY_AMMO, BUY_INVENTORY]);
        assert_eq!((w.players[0].armor, w.players[0].pack), (ArmorClass::Light, None), "refused here");
        let sensor = packs::PackKind::from_key("motion-sensor").unwrap();
        shop(&mut w, &[buy_drop(RIFLE), buy_take(BLASTER), buy_for(sensor)]);
        assert_eq!((carried(&w), w.players[0].pack), (vec![PLASMA, DISC, BLASTER], Some(sensor)));
        w.input.equip = Some(favorite);
        w.step_purchases();
        w.input.equip = None;
        assert_eq!((w.players[0].armor, carried(&w)), (ArmorClass::Light, vec![RIFLE, PLASMA, DISC]), "a favorite keeps your armor here");
    }

    #[test]
    fn stations_open_only_to_a_player_stepping_in_from_the_front() {
        let mut w = solo(ArmorClass::Light);
        w.players[0].team = Team::Ember;
        let defs = crate::equipment::definitions(MapId::OldHoller);
        let k = defs.iter().position(|d| d.kind == crate::equipment::Kind::Inventory && d.team == 0).unwrap();
        let front = station_front(MapId::OldHoller, k).expect("the map says which way it opens");
        assert_eq!(front, Vec3::Z, "Old Holler's Ember stations open north");
        let at = defs[k].pos();
        let side = Vec3::new(front.z, 0.0, -front.x);
        for (pos, open) in [(at, true), (at + front * 1.5, true), (at + front * 2.0 + side * 0.6, true),
            (at + side * 2.5, false), (at + front * 0.5 + side * 2.0, false), (at - front * 1.5, false)] {
            w.players[0].pos = pos;
            assert_eq!(w.inventory_reach(0), Some(open), "{:?}", pos - at);
        }
        w.players[0].pos = at + front * 30.0;
        assert_eq!(w.inventory_reach(0), None, "away from it");
    }

    #[test]
    fn purchases_need_your_own_powered_inventory_station() {
        let mut w = solo(ArmorClass::Light);
        w.input.buy = BUY_HEAVY;
        w.step_purchases();
        assert_eq!(w.players[0].armor, ArmorClass::Light, "not away from a station");
        let defs = crate::equipment::definitions(MapId::OldHoller);
        let s = defs.iter().position(|d| d.kind == crate::equipment::Kind::Inventory && d.team == 0).unwrap();
        w.players[0].team = Team::Ember;
        w.players[0].pos = defs[s].pos();
        w.players[0].ammo = rounds(&[0, 0, 0, 0, 0]);
        w.step_purchases();
        assert_eq!(w.players[0].armor, ArmorClass::Heavy);
        assert_eq!(w.players[0].ammo, max_ammo(ArmorClass::Heavy), "restocked for the new armor");
        w.input.buy = BUY_WALL;
        w.step_purchases();
        assert_eq!(w.players[0].pack, Some(packs::PackKind::Wall));
        // The other team's station sells you nothing.
        w.players[0].team = Team::Glacier;
        w.input.buy = BUY_LIGHT;
        w.step_purchases();
        assert_eq!(w.players[0].armor, ArmorClass::Heavy);
        // A rifle only where the mode allows it (the railgun, in heavy armor).
        w.players[0].team = Team::Ember;
        let mut def = w.mode.def().clone();
        def.weapons = Some(vec!["disc".into(), "laser".into()]);
        w.mode = crate::map_catalog::SupportedMode::from_index(crate::modes::add("test-laser-only", def).unwrap());
        w.input.buy = buy_take(arsenal::RAILGUN);
        w.step_purchases();
        assert!(!w.players[0].loadout.carries(arsenal::RAILGUN), "no railgun in this mode");
        w.input.buy = BUY_LIGHT;
        w.step_purchases();
        w.input.buy = buy_take(arsenal::RIFLE);
        w.step_purchases();
        assert!(w.players[0].loadout.carries(arsenal::RIFLE), "the laser is");
    }

    #[test]
    fn the_dead_drop_a_roll_of_what_they_carried_and_it_vanishes_after_20_s() {
        let mut w = solo(ArmorClass::Light);
        let mut other = w.players[0].clone();
        other.team = Team::Glacier;
        other.pos += Vec3::new(30.0, 0.0, 0.0);
        w.players.push(other);
        // Across many deaths: discs and grenade rounds roll 1-5 (never more
        // than carried), all the chaingun rounds, 1+ hand grenades, no mines.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..60 {
            w.loot.clear();
            w.respawn(1);
            w.players[1].ammo = rounds(&[10, 60, 4, 0, 0]);
            w.players[1].throwables = [3, 3];
            w.kill(1, None, "Fall");
            let l = &w.loot[0];
            assert!((1..=5).contains(&l.ammo[0]) && (1..=4).contains(&l.ammo[2]), "{:?}", l.ammo);
            assert_eq!(l.ammo[1], 60);
            assert!((1..=3).contains(&l.grenades));
            seen.insert(l.ammo[0]);
        }
        assert!(seen.len() >= 4, "discs vary: {seen:?}");
        // Somebody with no grenades still drops one.
        w.loot.clear(); w.respawn(1);
        w.players[1].throwables = [0, 3];
        w.players[1].ammo = rounds(&[0, 0, 0, 0, 0]);
        w.kill(1, None, "Fall");
        assert_eq!((w.loot[0].ammo.clone(), w.loot[0].grenades), (rounds(&[0, 0, 0, 0]), 1));
        // A pack: an enemy runs over it and takes what fits.
        w.loot.clear(); w.respawn(1);
        w.players[1].ammo = rounds(&[10, 60, 4, 0, 0]);
        let at = w.players[1].pos;
        w.kill(1, None, "Fall");
        w.loot[0].ammo = rounds(&[5, 60, 3, 0]);
        w.loot[0].grenades = 2;
        w.players[0].ammo = rounds(&[13, 70, 8, 0, 0]);
        w.players[0].throwables = [4, 0];
        w.players[0].pos = at;
        w.step_loot(STEP);
        assert_eq!(w.players[0].ammo, rounds(&[15, 100, 10, 0, 0]), "discs to max, chaingun filled, grenade rounds");
        assert_eq!(w.players[0].throwables, [5, 0], "a hand grenade, no mines");
        assert_eq!((w.loot[0].ammo.clone(), w.loot[0].grenades), (rounds(&[3, 30, 1, 0]), 1), "the rest stays");
        // A heavy's mortar can't take grenade-launcher rounds.
        w.players[0].armor = ArmorClass::Heavy;
        w.players[0].ammo = rounds(&[0, 0, 0, 0, 0]);
        w.step_loot(STEP);
        assert_eq!((w.players[0].ammo[2], w.players[0].ammo[arsenal::MORTAR as usize]), (0, 0), "wrong third weapon");
        assert_eq!(w.loot[0].ammo[2], 1);
        // Untouched, it's gone after 20 s.
        w.players[0].pos += Vec3::new(50.0, 0.0, 0.0);
        for _ in 0..(LOOT_SECONDS / STEP) as usize + 2 { w.step_loot(STEP); }
        assert!(w.loot.is_empty());
    }

    #[test]
    fn an_empty_chaingun_clicks_without_touching_its_cooldown() {
        let mut w = solo(ArmorClass::Light);
        w.players[0].ammo[1] = 0;
        w.input.weapon = 1;
        w.input.fire = true;
        for _ in 0..30 {
            w.tick(STEP);
            assert_eq!(w.players[0].cooldown, 0.0, "no fake shots for the viewmodel");
        }
    }
}
