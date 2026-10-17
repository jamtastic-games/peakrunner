//! Weapons carried in slots (marker `arsenal1`, by wire number since
//! `classes1`). Each armor has a number of weapon slots
//! (`armors/<id>.weapon_slots`: light 3, medium 4, heavy 5), filled at an
//! inventory station from the weapons the mode allows and the armor may
//! carry; number keys pick slot 1, 2, 3… in the order chosen. Until a player
//! chooses, they carry their armor's `kit` (light: disc launcher, repeater
//! and grenade launcher; medium: the plasma gun too; heavy: the mortar
//! instead of the grenade launcher), as bots do.
//!
//! `Player::weapon` is a weapon's wire number (`crate::registry`). Versions
//! of a weapon across armors share a `family`: changing armor swaps a
//! carried weapon for the new armor's of the same family (grenade launcher
//! and mortar, laser and railgun).
//!
//! Two of the base weapons are bolt weapons: the plasma gun fires a slow
//! glowing bolt that bursts on contact (a small blast) and fizzles in water;
//! the blaster fires fast bolts paid for with energy, never ammunition, that
//! glance off surfaces and hurt only what they hit.
use super::*;

/// The base game's weapons by wire number (fixed: `registry::Class::Weapon`).
pub const DISC: u8 = 0;
pub const CHAINGUN: u8 = 1;
/// The grenade launcher (heavy armor carries the mortar instead).
pub const LAUNCHER: u8 = 2;
/// The laser rifle (heavy armor carries the railgun instead).
pub const RIFLE: u8 = 3;
pub const PLASMA: u8 = 4;
pub const BLASTER: u8 = 5;
pub const MORTAR: u8 = 6;
pub const RAILGUN: u8 = 7;
/// The base game's six weapon families, as saved favorites name them.
pub const KIND_KEYS: [&str; 6] = ["disc", "chaingun", "launcher", "rifle", "plasma", "blaster"];
/// The most slots an armor may have.
pub const MAX_SLOTS: usize = 8;
/// Projectile kinds (`Disc::kind`) of the bolt weapons.
pub const PLASMA_KIND: u8 = 10;
pub const BLASTER_KIND: u8 = 11;

const EMPTY: u8 = u8::MAX;

/// Whether a weapon with this wire number exists in the session.
fn known(k: u8) -> bool { crate::registry::registry().weapons.get(k as usize).is_some() }

/// The weapons carried, slot 1 first, each at most once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loadout([u8; MAX_SLOTS]);

impl Default for Loadout {
    /// The base light armor's kit: disc, chaingun, grenade launcher.
    fn default() -> Self { Loadout::of(&[DISC, CHAINGUN, LAUNCHER]) }
}

impl Loadout {
    pub const NONE: Loadout = Loadout([EMPTY; MAX_SLOTS]);

    /// These weapons in this order; repeats and unknown ones are skipped.
    pub fn of(kinds: &[u8]) -> Self {
        let mut l = Self::NONE;
        for &k in kinds { l.push(k); }
        l
    }
    /// An armor's kit: what it carries until its wearer chooses.
    pub fn kit(armor: ArmorClass) -> Self {
        let r = crate::registry::registry();
        Self::of(&armor.def().kit.iter().filter_map(|id| r.weapons.index(id)).map(|k| k as u8).collect::<Vec<_>>())
    }
    pub fn kinds(&self) -> impl Iterator<Item = u8> + '_ { self.0.iter().copied().filter(|&k| k != EMPTY && known(k)) }
    pub fn len(&self) -> usize { self.kinds().count() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn carries(&self, kind: u8) -> bool { self.kinds().any(|k| k == kind) }
    /// The weapon in slot `n` (0-based).
    pub fn slot(&self, n: usize) -> Option<u8> { self.0.get(n).copied().filter(|&k| k != EMPTY && known(k)) }
    /// Add a weapon in the next free slot (false: already carried, unknown, or full).
    pub fn push(&mut self, kind: u8) -> bool {
        if kind == EMPTY || !known(kind) || self.carries(kind) { return false; }
        match self.0.iter().position(|&k| k == EMPTY) { Some(i) => { self.0[i] = kind; true } None => false }
    }
    /// Take a weapon into one of `slots` slots (false: full or already carried).
    pub fn take(&mut self, kind: u8, slots: usize) -> bool { self.len() < slots && self.push(kind) }
    /// Leave a weapon behind; the slots after it move up.
    pub fn remove(&mut self, kind: u8) { *self = Self::of(&self.kinds().filter(|&k| k != kind).collect::<Vec<_>>()); }
    /// Only the weapons `keep` accepts, then only the first `slots`.
    pub fn fitted(&self, slots: usize, keep: impl Fn(u8) -> bool) -> Self {
        Self::of(&self.kinds().filter(|&k| keep(k)).take(slots).collect::<Vec<_>>())
    }
    /// The same choice for `armor`: each weapon it can't carry becomes its
    /// weapon of the same family, or is left out.
    pub fn for_armor(&self, armor: ArmorClass) -> Self {
        let r = crate::registry::registry();
        Self::of(&self.kinds().filter_map(|k| if carries(armor, k) { Some(k) } else {
            r.weapons.get(k as usize).and_then(|w| family_weapon(armor, &w.family))
        }).collect::<Vec<_>>())
    }
    /// The slots as sent: wire numbers, empty slots last (`EMPTY`).
    pub fn raw(&self) -> [u8; MAX_SLOTS] { self.0 }
    /// A loadout as received; None unless slots fill from the first, with
    /// known weapons, each once.
    pub fn from_raw(raw: [u8; MAX_SLOTS]) -> Option<Self> {
        let l = Loadout(raw);
        l.valid().then_some(l)
    }
    /// Slots fill from the first, each a known weapon, none twice.
    pub fn valid(&self) -> bool {
        let n = self.0.iter().position(|&k| k == EMPTY).unwrap_or(MAX_SLOTS);
        self.0[n..].iter().all(|&k| k == EMPTY) && Self::of(&self.0[..n]).len() == n
    }
}

/// Weapon slots for an armor (1 to `MAX_SLOTS`).
pub fn slots(class: ArmorClass) -> usize { (class.def().weapon_slots as usize).clamp(1, MAX_SLOTS) }

/// Whether `armor` may carry weapon `kind`.
pub fn carries(armor: ArmorClass, kind: u8) -> bool {
    crate::registry::registry().weapons.id(kind as usize).is_some_and(|id| armor.def().weapons.contains_key(id))
}

/// The weapon of `family` that `armor` carries.
pub fn family_weapon(armor: ArmorClass, family: &str) -> Option<u8> {
    let r = crate::registry::registry();
    let a = armor.def();
    r.weapons.iter().find(|(_, id, w)| w.family == family && a.weapons.contains_key(*id)).map(|(k, _, _)| k as u8)
}

/// A weapon's id, as the mode's `weapons` list names it.
pub fn key(kind: u8) -> &'static str { crate::registry::registry().weapons.id(kind as usize).unwrap_or("") }

/// The view model weapon `kind` is drawn with.
pub fn view(kind: u8) -> crate::registry::View {
    crate::registry::registry().weapons.get(kind as usize).map_or(crate::registry::View::Disc, |w| w.view)
}

/// Its number in the renderer's per-model tables (`View::kind`).
pub fn view_kind(kind: u8) -> u8 { view(kind).kind() }

/// A weapon's name for people.
pub fn name(kind: u8) -> &'static str { crate::registry::registry().weapons.get(kind as usize).map_or("", |w| w.name.as_str()) }

impl World {
    /// What player `i` carries: their chosen weapons (`Setup::weapons`, or
    /// their armor's kit) as their armor carries them, that the mode allows,
    /// in as many slots as their armor has; a Rifles round adds the rifle.
    pub fn carried(&self, i: usize) -> Loadout {
        let p = &self.players[i];
        let def = self.mode.def();
        let chosen = if def.purchases { p.setup.weapons } else { None }.unwrap_or_else(|| Loadout::kit(p.armor));
        let mut l = chosen.for_armor(p.armor).fitted(slots(p.armor), |k| def.allows(k));
        if self.conditions.twist == crate::conditions::Twist::Rifles {
            if let Some(rifle) = family_weapon(p.armor, "rifle").filter(|&k| def.allows(k)) { l.push(rifle); }
        }
        l
    }

    /// Re-equip player `i` from their setup, keeping the weapon in hand when
    /// they still carry it.
    pub(super) fn equip(&mut self, i: usize) {
        let l = self.carried(i);
        let p = &mut self.players[i];
        p.loadout = l;
        if !l.carries(p.weapon) { p.weapon = l.slot(0).unwrap_or(p.weapon); }
    }

    /// Player `i`'s held weapon: its entry, its projectile's entry and the
    /// projectile's wire number.
    pub(super) fn held(&self, i: usize) -> Option<(&'static crate::registry::Weapon, &'static crate::registry::Projectile, u8)> {
        let r = crate::registry::registry();
        let w = self.weapon_def(i)?;
        let kind = r.projectiles.index(&w.projectile)?;
        Some((w, r.projectiles.get(kind)?, kind as u8))
    }

    /// Spend what a shot from player `i`'s held weapon costs: its energy, or
    /// a round (and all their energy, for one that `empties_energy`); false
    /// when they haven't got it.
    pub(super) fn pay_shot(&mut self, i: usize) -> bool {
        let Some((w, _, _)) = self.held(i) else { return false };
        let p = &mut self.players[i];
        if w.energy > 0.0 {
            if p.energy < w.energy { return false; }
            p.energy -= w.energy;
        } else {
            let Some(rounds) = p.ammo.get_mut(p.weapon as usize) else { return false };
            if *rounds == 0 { return false; }
            *rounds -= 1;
        }
        if w.empties_energy { p.energy = 0.0; }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// A light-armor player carrying `kinds`, an enemy `ahead` metres in
    /// front at chest height of the aim.
    fn range(ahead: f32, kinds: &[u8]) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        w.conditions = Default::default();
        w.players.truncate(1);
        w.players[0].loadout = Loadout::of(kinds);
        w.refill(0);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(MapId::OldHoller, spot + Vec3::Y * 200.0).0;
        w.players[0].pos = Vec3::new(spot.x, floor + PLAYER_RADIUS, spot.z);
        w.players[0].yaw = 0.0;
        let mut foe = w.players[0].clone();
        foe.team = Team::Glacier;
        foe.pos = w.players[0].pos + Vec3::new(0.0, 0.0, -ahead);
        foe.pos.y = crate::terrain::support_on(MapId::OldHoller, foe.pos + Vec3::Y * 200.0).0 + PLAYER_RADIUS;
        w.players.push(foe);
        let d = (w.players[1].pos + Vec3::Y * 0.9) - (w.players[0].pos + Vec3::Y * EYE);
        w.players[0].pitch = (d.y / d.length()).asin();
        w
    }

    fn fire(w: &mut World, kind: u8) {
        w.input.weapon = kind;
        w.input.fire = true;
        w.tick(STEP * 2.0);
        w.input.fire = false;
    }

    #[test]
    fn the_plasma_gun_fires_a_level_bolt_that_bursts_on_what_it_meets() {
        let mut w = range(8.0, &[PLASMA, DISC]);
        let rounds = w.players[0].ammo[PLASMA as usize];
        assert!(rounds > 0);
        fire(&mut w, PLASMA);
        assert_eq!(w.players[0].ammo[PLASMA as usize], rounds - 1, "a round spent");
        let bolt = w.discs.iter().find(|d| d.kind == PLASMA_KIND).expect("a bolt in flight").clone();
        let speed = crate::defs::weapons().plasma.speed;
        assert!(bolt.vel.length() > speed * 0.9 && bolt.vel.length() < speed * 1.2, "{}", bolt.vel.length());
        w.physics_step();
        let later = w.discs.iter().find(|d| d.kind == PLASMA_KIND).map(|d| d.vel.y);
        assert!(later.is_none_or(|y| (y - bolt.vel.y).abs() < 1e-3), "no drop");
        for _ in 0..30 { w.physics_step(); }
        let blast = crate::defs::defs().blasts.plasma;
        assert!(w.players[1].health <= 100.0 - blast.max_damage * 0.5, "a direct burst: {}", w.players[1].health);
        // The splash reaches a little way, and the shooter takes some of their own.
        let mut w = range(8.0, &[PLASMA]);
        let beside = w.players[1].pos + Vec3::new(blast.radius * 0.5, 0.3, 0.0);
        w.explode(beside, 0, PLASMA_KIND, Team::Ember);
        let hurt = 100.0 - w.players[1].health;
        assert!(hurt > blast.max_damage * blast.edge_fraction && hurt < blast.max_damage, "{hurt}");
        let far = w.players[1].pos + Vec3::new(blast.radius + 2.0, 0.3, 0.0);
        let before = w.players[1].health;
        w.explode(far, 0, PLASMA_KIND, Team::Ember);
        assert_eq!(w.players[1].health, before, "out of reach");
        let own = w.players[0].pos + Vec3::new(0.0, 0.3, 0.0);
        w.explode(own, 0, PLASMA_KIND, Team::Ember);
        let taken = 100.0 - w.players[0].health;
        assert!(taken > 0.0 && taken <= blast.max_damage * crate::defs::weapons().plasma.self_damage + 0.01, "{taken}");
        // Not carried: no shot.
        let mut w = range(25.0, &[DISC]);
        fire(&mut w, PLASMA);
        assert!(!w.discs.iter().any(|d| d.kind == PLASMA_KIND));
    }

    #[test]
    fn the_blaster_spends_energy_hits_only_what_it_meets_and_glances_off_the_ground() {
        let b = crate::defs::weapons().blaster;
        let mut w = range(20.0, &[BLASTER]);
        let ammo = w.players[0].ammo.clone();
        fire(&mut w, BLASTER);
        assert_eq!(w.players[0].ammo, ammo, "no rounds");
        assert!(w.players[0].energy < crate::defs::movement().energy_max - b.energy * 0.5, "energy spent");
        assert!(w.discs.iter().any(|d| d.kind == BLASTER_KIND));
        for _ in 0..40 { w.physics_step(); }
        assert!((w.players[1].health - (100.0 - b.damage)).abs() < 0.5, "a direct hit: {}", w.players[1].health);
        // An empty tank doesn't fire.
        let mut w = range(20.0, &[BLASTER]);
        w.players[0].energy = b.energy * 0.5;
        fire(&mut w, BLASTER);
        assert!(!w.discs.iter().any(|d| d.kind == BLASTER_KIND));
        // Aimed at the ground, a bolt comes back up.
        let mut w = range(60.0, &[BLASTER]);
        w.players[0].pitch = -0.5;
        fire(&mut w, BLASTER);
        let mut rose = false;
        for _ in 0..30 {
            w.physics_step();
            rose |= w.discs.iter().any(|d| d.kind == BLASTER_KIND && d.vel.y > 1.0);
        }
        assert!(rose, "glanced off the ground");
    }

    #[test]
    fn loadouts_keep_order_and_their_slots() {
        let mut l = Loadout::of(&[PLASMA, DISC, PLASMA, 99]);
        assert_eq!(l.kinds().collect::<Vec<_>>(), [PLASMA, DISC]);
        assert!(l.take(BLASTER, 3) && !l.take(RIFLE, 3), "three slots");
        assert!(!l.take(DISC, 6), "once each");
        l.remove(DISC);
        assert_eq!((l.slot(0), l.slot(1), l.slot(2)), (Some(PLASMA), Some(BLASTER), None));
        assert_eq!(Loadout::from_raw(l.raw()), Some(l));
        assert_eq!(Loadout::from_raw(Loadout::default().raw()), Some(Loadout::default()));
        assert_eq!(Loadout::from_raw(Loadout::NONE.raw()), Some(Loadout::NONE));
        // Repeats, unknown weapons and gaps are refused.
        let e = EMPTY;
        for bad in [[0, 0, e, e, e, e, e, e], [0, 99, e, e, e, e, e, e], [0, e, 1, e, e, e, e, e]] {
            assert_eq!(Loadout::from_raw(bad), None, "{bad:?}");
        }
        assert_eq!(Loadout::default().fitted(2, |k| k != CHAINGUN).kinds().collect::<Vec<_>>(), [DISC, LAUNCHER]);
        let medium = ArmorClass::by_id("medium").unwrap();
        assert_eq!((slots(ArmorClass::Light), slots(medium), slots(ArmorClass::Heavy)), (3, 4, 5));
        // Across armors, by family.
        assert_eq!(Loadout::kit(ArmorClass::Heavy), Loadout::of(&[DISC, CHAINGUN, MORTAR]));
        assert_eq!(Loadout::of(&[LAUNCHER, RIFLE, PLASMA]).for_armor(ArmorClass::Heavy), Loadout::of(&[MORTAR, RAILGUN, PLASMA]));
        assert_eq!(Loadout::of(&[MORTAR, RAILGUN]).for_armor(ArmorClass::Light), Loadout::of(&[LAUNCHER, RIFLE]));
        // Medium carries neither rifle nor the mortar.
        assert_eq!(Loadout::of(&[MORTAR, RAILGUN, PLASMA]).for_armor(medium), Loadout::of(&[LAUNCHER, PLASMA]));
        assert_eq!(Loadout::of(&[RIFLE, DISC]).for_armor(medium), Loadout::of(&[DISC]));
    }

    /// Who carries what (docs/loadouts.md): the laser rifle is light's, the
    /// mortar and railgun heavy's; the drain gun and target marker are
    /// everyone's. Light wears personal and sensor packs only.
    #[test]
    fn the_three_armors_split_the_arsenal_and_the_packs() {
        use super::packs::PackKind;
        let medium = ArmorClass::by_id("medium").unwrap();
        let r = crate::registry::registry();
        let carried = |a: ArmorClass| r.weapons.iter().filter(|(k, _, _)| carries(a, *k as u8)).map(|(_, id, _)| id).collect::<Vec<_>>();
        assert_eq!(carried(ArmorClass::Light), ["disc", "chaingun", "grenade_launcher", "laser", "plasma", "blaster", "drain-gun", "target-marker"]);
        assert_eq!(carried(medium), ["disc", "chaingun", "grenade_launcher", "plasma", "blaster", "drain-gun", "target-marker"]);
        assert_eq!(carried(ArmorClass::Heavy), ["disc", "chaingun", "plasma", "blaster", "mortar", "railgun", "drain-gun", "target-marker"]);
        let def = SupportedMode::Ctf.def();
        let packs = |a: ArmorClass| PackKind::all().filter(|&k| def.offers_pack_to(k, a)).map(|k| k.key()).collect::<Vec<_>>();
        assert_eq!(packs(ArmorClass::Light), ["ammo-pack", "energy", "jammer", "jammer-pylon", "motion-sensor", "pulse-sensor", "repair", "shield"]);
        let every = ["turret", "wall", "field", "ammo", "inventory", "ammo-pack", "energy", "jammer", "jammer-pylon", "motion-sensor",
            "pulse-sensor", "repair", "shield"];
        assert_eq!(packs(medium), every);
        assert_eq!(packs(ArmorClass::Heavy), every);
        assert_eq!(def.offered_armors(), [ArmorClass::Light, medium, ArmorClass::Heavy]);
    }

    #[test]
    fn players_carry_their_choice_or_the_default_within_the_mode_and_armor() {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        w.conditions = Default::default();
        assert_eq!(w.carried(0), Loadout::default());
        w.players[0].setup.weapons = Some(Loadout::of(&[PLASMA, RIFLE, BLASTER, DISC]));
        assert_eq!(w.carried(0), Loadout::of(&[PLASMA, RIFLE, BLASTER]), "light armor: three slots");
        w.players[0].armor = ArmorClass::Heavy;
        assert_eq!(w.carried(0), Loadout::of(&[PLASMA, RAILGUN, BLASTER, DISC]), "the heavy's rifle is the railgun");
        w.players[0].armor = ArmorClass::by_id("medium").unwrap();
        assert_eq!(w.carried(0), Loadout::of(&[PLASMA, BLASTER, DISC]), "medium armor: no rifle");
        w.players[0].armor = ArmorClass::Heavy;
        let mut def = SupportedMode::Ctf.def().clone();
        def.weapons = Some(vec!["disc".into(), "plasma".into(), "laser".into()]);
        w.mode = SupportedMode::from_index(crate::modes::add("test-arsenal-some", def).unwrap());
        assert_eq!(w.carried(0), Loadout::of(&[PLASMA, DISC]), "no railgun or blaster here");
        w.conditions.twist = crate::conditions::Twist::Rifles;
        w.players[0].armor = ArmorClass::Light;
        assert_eq!(w.carried(0), Loadout::of(&[PLASMA, RIFLE, DISC]), "a Rifles round adds the laser");
    }
}
