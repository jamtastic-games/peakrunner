//! The rifle slot (weapon 4): bought at an inventory station, never carried
//! by default, lost on death. Which rifle you hold follows your armor:
//!
//! - Laser rifle (light armor): spends energy, fires a straight beam from
//!   your eye along the crosshair that hits instantly at any range on the
//!   map.
//! - Railgun (heavy armor): fires a slug that flies straight at 600 m/s,
//!   led at range, keeps a little of your own motion, and hits hard enough to
//!   down light armor at once. It uses its own ammunition, and each shot
//!   empties its holder's energy.
//!
//! Both can be fired from the hip; the client zooms while the zoom key is
//! held, at one of two levels the wheel picks (a view change only; name
//! tags and markers reach as much farther as the sight magnifies). A laser shot travels to clients as a short-lived
//! `Disc` of kind LASER_KIND whose `vel` holds the beam (start to end), so it
//! renders for everyone; it never moves or collides. Marker: `rifle1`.
use super::*;

pub const RIFLE_SLOT: u8 = 3;
pub const LASER_KIND: u8 = 8;
pub const RAIL_KIND: u8 = 9;

/// Fields of view through a sight (degrees), its two levels: about 4x and
/// 8x nearer than the usual 76 degree view.
pub const ZOOM_FOVS: [f32; 2] = [22.0, 11.0];

/// How many times nearer a view of `fov` degrees brings things than the
/// usual 76 degree one.
pub fn magnification(fov: f32) -> f32 { (38f32).to_radians().tan() / (fov * 0.5).to_radians().tan() }

/// The weapon kind `p` may hold asking for `wanted` (`arsenal`): one they
/// carry (`Player::loadout`) that the mode allows (`ModeDef::weapons`). A
/// refused kind keeps what's held if that's still allowed, else takes the
/// first carried one; with none, nothing fires (`World::armed`).
pub fn allowed_slot(def: &crate::modes::ModeDef, p: &Player, wanted: u8) -> u8 {
    let ok = |k: u8| def.allows(k) && p.loadout.carries(k);
    if ok(wanted) { wanted } else if ok(p.weapon) { p.weapon } else { p.loadout.kinds().find(|&k| ok(k)).unwrap_or(p.weapon) }
}

impl World {
    /// The weapon slot player `i` may hold when they ask for `wanted`.
    pub fn allowed_weapon(&self, i: usize, wanted: u8) -> u8 { allowed_slot(self.mode.def(), &self.players[i], wanted) }

    /// Whether player `i` holds a weapon the mode allows: false in a mode
    /// with no weapons (or none for their armor), where nothing fires and
    /// the hands are empty.
    pub fn armed(&self, i: usize) -> bool {
        let p = &self.players[i];
        self.mode.def().allows(p.weapon) && p.loadout.carries(p.weapon)
    }

    /// Fire player `i`'s beam weapon `w` (the shot already paid for): a line
    /// from the eye along the crosshair, hitting the first thing it meets at
    /// once; its picture leaves the muzzle as a short-lived projectile.
    pub(super) fn fire_beam(&mut self, i: usize, w: &'static crate::registry::Weapon, round: &'static crate::registry::Projectile, kind: u8, power: f32) {
        self.players[i].shots += 1;
        let p = &self.players[i];
        let dir = look_dir(p.yaw, p.pitch);
        let eye = p.pos + Vec3::Y * EYE;
        let spd = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        let muzzle = muzzle_of(eye, dir, w, camera_fov(spd));
        let muzzle = match obstacle_hit(self.map, &self.pillars, eye, muzzle, SHOT_RADIUS) {
            Some(t) => eye.lerp(muzzle, (t-0.05/eye.distance(muzzle).max(0.05)).max(0.0)),
            None => muzzle,
        };
        let team = p.team;
        self.players[i].cooldown = w.reload;
        if i == self.player_id { self.push_event(&w.sound); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push((w.sound.as_str(), muzzle)); }
        if self.predicting { return; }
        // The beam: the first thing along the crosshair from the eye.
        let end = eye+dir*round.range;
        let mut t = obstacle_hit(self.map, &self.pillars, eye, end, 0.0).unwrap_or(1.0);
        if let Some((u, _)) = self.moveable_hit(eye, end, 0.0) { t = t.min(u); }
        let mut victim = None;
        let mut deploy = None;
        let mut equipment = None;
        if let Some((u, k)) = self.deployable_hit(eye, end, 0.0, Some(team)) {
            if u < t { t = u; deploy = Some(k); }
        }
        for (idx, (obj, state)) in crate::equipment::definitions(self.map).iter().zip(&self.equipment).enumerate() {
            if obj.team as usize == team.idx() || state.health <= 0.0 { continue; }
            if let Some(u) = segment_sphere(eye, end, obj.pos(), obj.radius) {
                if u < t { t = u; equipment = Some(idx); deploy = None; }
            }
        }
        // Other players where the shooter saw them (sim::lagcomp); the
        // shooter and everything else as they are now.
        // Vehicles take beams on the hull (`vehicles`); their riders are
        // inside, but for an exposed seat's.
        let mut craft = None;
        if let Some((u, v, _)) = self.vehicle_hit(eye, end, 0.0, None) { if u < t { t = u; craft = Some(v); equipment = None; deploy = None; } }
        let rewind = self.lag.rewind(i);
        for (idx, pl) in self.players.iter().enumerate() {
            if !pl.alive || (pl.riding > 0 && !self.rides_exposed(idx)) || (!self.hostile(i, team, idx) && !self.team_fire(i, team, idx)) { continue; }
            let at = self.lag.position(idx, rewind, pl);
            if let Some(u) = segment_sphere(eye, end, at+Vec3::Y*0.9, PLAYER_RADIUS+0.45) {
                if u < t { t = u; victim = Some(idx); equipment = None; deploy = None; craft = None; }
            }
        }
        let hit = eye.lerp(end, t);
        // A resting mine in the beam goes off (anyone's: clear your own too).
        let mut mines = self.shot_mines(eye, hit);
        mines.sort_unstable();
        for k in mines.into_iter().rev() {
            let m = self.discs.remove(k);
            self.explode_direct(m.pos, m.owner, m.kind, m.team, None);
        }
        self.discs.push(Disc { pos: muzzle, vel: hit-muzzle, team, owner: i, life: round.life, kind, spin: 0.0, target: 0, power: 1.0 });
        // A marking beam leaves its mark on what it met.
        if round.marks > 0.0 { self.marks.push(super::firing::Mark { team, player: victim, pos: hit, life: round.marks, by: None }); }
        let damage = if power != 1.0 { round.damage * power } else { round.damage };
        if let Some(k) = deploy { self.damage_deployable(k, damage); }
        if let Some(idx) = equipment {
            let defs = crate::equipment::definitions(self.map);
            if self.equipment[idx].damage(&defs[idx], damage, true) { self.equipment_destroyed(&defs[idx]); }
        }
        if let Some(idx) = victim {
            // A beam with no damage (the target marker's) hurts nobody; its
            // shooter still sees the hit.
            if damage > 0.0 { self.round_hit(idx, i, damage, round.flash, &round.kill, &round.damage_kind, hit, dir); }
            else if i == self.player_id { self.hitmarker = 1.0; self.push_event("hit"); }
            self.hit_extras(idx, i, round);
        }
        if let Some(v) = craft { self.damage_vehicle(v, damage, Some(i)); }
        if round.grapple > 0.0 && t < 1.0 { self.drag(i, hit, round.grapple); }
    }

    /// What a hard direct hit of kind `k` (`damage` after zones, flying
    /// along `dir`) knocks loose from player `idx`: the flag off a carrier
    /// hit from behind (it flies on ahead, and they can't catch it at once,
    /// as after a toss), their pack, the weapon in their hand.
    fn knock(&mut self, idx: usize, k: &crate::registry::Damage, damage: f32, back: bool, dir: Vec3) {
        if self.predicting || !self.players[idx].alive { return; }
        let hard = |threshold: f32| threshold > 0.0 && damage >= threshold;
        if hard(k.knock_flag) && back {
            if let Some(team) = self.players[idx].carrying.take() {
                let p = &self.players[idx];
                let ahead = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
                let (pos, vel) = (p.pos + Vec3::Y * 1.4 + ahead * 0.6, p.vel + ahead * TOSS_SPEED * 0.6 + Vec3::Y * TOSS_LIFT);
                let flag_return = self.mode.def().flag_return;
                let f = &mut self.flags[team.idx()];
                f.carrier = None;
                f.pos = pos;
                f.vel = vel;
                f.drop_timer = flag_return;
                f.tossed_by = Some(idx);
                f.toss_guard = TOSS_GUARD;
                self.push_event("drop");
                if idx == self.player_id { self.msg("THE FLAG WAS KNOCKED OFF YOUR BACK", 1.6); }
            }
        }
        if hard(k.drop_pack) && self.players[idx].pack.take().is_some() && idx == self.player_id { self.msg("YOUR PACK WAS KNOCKED LOOSE", 1.6); }
        if hard(k.disarm) {
            let p = &mut self.players[idx];
            let held = p.weapon;
            if p.loadout.carries(held) {
                p.loadout.remove(held);
                p.weapon = p.loadout.slot(0).unwrap_or(held);
                if idx == self.player_id { self.msg("DISARMED", 1.4); }
            }
        }
    }

    /// Whether `owner`'s fire hurts teammate `victim`: the mode's
    /// `friendly_fire` above 0 (never in free-for-all, never yourself,
    /// never equipment's).
    pub(super) fn team_fire(&self, owner: usize, team: Team, victim: usize) -> bool {
        self.mode.def().friendly_fire > 0.0 && owner != victim && owner < self.players.len() && !self.ffa()
            && self.players.get(victim).is_some_and(|v| v.team == team)
    }

    /// A hit that doesn't burst (a round or a beam) on player `idx` by
    /// `owner`: `damage` of `kind` (a damage kind's id) striking at `at`
    /// along `dir`, the victim's screen flashing `flash`, and the kill feed's
    /// `weapon` should it kill. The kind may hurt more in the head or the
    /// back, and a hard enough hit knocks the flag off a carrier hit from
    /// behind, drops a pack or knocks the weapon from the hand.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn round_hit(&mut self, idx: usize, owner: usize, damage: f32, flash: f32, weapon: &str, kind: &str, at: Vec3, dir: Vec3) {
        if !self.players[idx].alive { return; }
        let k = crate::registry::registry().damage.by_id(kind);
        let victim = &self.players[idx];
        let head = at.y >= victim.pos.y + EYE - 0.35;
        let facing = Vec3::new(-victim.yaw.sin(), 0.0, -victim.yaw.cos());
        let back = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero().dot(facing) > 0.5;
        let mut damage = damage;
        if let Some(k) = k {
            if head && k.head != 1.0 { damage *= k.head; }
            if back && k.back != 1.0 { damage *= k.back; }
        }
        // A plate on the front stops a hit from ahead (`packs`).
        let share = self.blocked_share(idx, dir);
        if share < 1.0 { damage *= share; }
        // Teammates' fire, where the mode allows it, at its share.
        if owner != idx && !self.hostile(owner, self.players.get(owner).map_or(Team::Ember, |o| o.team), idx) && owner < self.players.len() {
            damage *= self.mode.def().friendly_fire;
        }
        self.hit_by = (owner < self.players.len()).then_some(owner);
        self.hurt(idx, damage, kind);
        self.hit_by = None;
        if let Some(p) = self.players.get_mut(owner) { p.hits += 1; }
        if owner == self.player_id { self.hitmarker = 1.0; self.push_event("hit"); }
        if idx == self.player_id { self.damage_flash = flash; self.push_event("pain"); }
        if let Some(k) = k { self.knock(idx, k, damage, back, dir); }
        if self.players[idx].health <= 0.0 {
            self.kill(idx, Some(owner), weapon);
            if owner == self.player_id { self.kills += 1; self.msg("FRAG", 1.1); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// A player on open ground with an enemy standing `ahead` metres in front.
    fn range(armor: ArmorClass, ahead: f32) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(MapId::OldHoller, spot+Vec3::Y*200.0).0;
        w.players[0].pos = Vec3::new(spot.x, floor+PLAYER_RADIUS, spot.z);
        w.players[0].armor = armor;
        w.refill(0);
        w.players[0].yaw = 0.0; w.players[0].pitch = 0.0;
        let mut foe = w.players[0].clone();
        foe.team = Team::Glacier;
        foe.pos = w.players[0].pos+Vec3::new(0.0, 0.0, -ahead);
        foe.pos.y = crate::terrain::support_on(MapId::OldHoller, foe.pos+Vec3::Y*200.0).0+PLAYER_RADIUS;
        w.players.push(foe);
        // Aim at the foe's chest.
        let d = (w.players[1].pos+Vec3::Y*0.9)-(w.players[0].pos+Vec3::Y*EYE);
        w.players[0].pitch = (d.y/d.length()).asin();
        w
    }

    /// The rifle of player 0's armor (laser or railgun).
    fn rifle(w: &World) -> u8 { super::arsenal::family_weapon(w.players[0].armor, "rifle").unwrap() }

    fn fire(w: &mut World) {
        w.input.weapon = rifle(w);
        w.input.fire = true;
        w.tick(STEP*2.0);
        w.input.fire = false;
    }

    /// The range in a CTF variant that allows only `weapons`.
    fn armed_with(key: &str, weapons: &[&str], armor: ArmorClass) -> World {
        let mut def = SupportedMode::Ctf.def().clone();
        def.weapons = Some(weapons.iter().map(|w| w.to_string()).collect());
        let mode = SupportedMode::from_index(crate::modes::add(key, def).unwrap());
        let mut w = range(armor, 20.0);
        w.mode = mode;
        w
    }

    /// A sight's two levels bring things about 4 and 8 times nearer.
    #[test]
    fn the_sight_has_two_levels() {
        assert_eq!(ZOOM_FOVS.map(|f| (magnification(f) * 10.0).round() / 10.0), [4.0, 8.1]);
        let mut w = range(ArmorClass::Light, 20.0);
        w.players[0].loadout.push(super::arsenal::RIFLE);
        w.players[0].weapon = super::arsenal::RIFLE;
        assert_eq!(w.zoom_power(), 1.0, "not looking through it");
        w.zoomed = true;
        assert_eq!((w.zoom_fov(), w.zoom_power()), (22.0, magnification(22.0)));
        w.zoom_level = 1;
        assert_eq!(w.zoom_fov(), 11.0);
        w.players[0].weapon = 0;
        assert_eq!(w.zoom_power(), 1.0, "a disc launcher has no sight");
    }

    /// The railgun's slug: a projectile you lead, not a beam; one hit downs
    /// light armor and takes 95% of medium's, and each shot empties the
    /// heavy's energy.
    #[test]
    fn the_railgun_slug_flies_hits_hard_and_empties_energy() {
        let r = crate::registry::registry();
        let slug = r.projectiles.by_id("rail-slug").unwrap();
        assert_eq!(slug.class, crate::registry::ProjectileClass::Straight);
        assert_eq!(slug.speed, 600.0);
        let medium = ArmorClass::by_id("medium").unwrap();
        assert!(slug.damage * super::loadout::damage_taken(ArmorClass::Light) >= 100.0, "one hit downs light armor");
        assert!((slug.damage * super::loadout::damage_taken(medium) - 95.0).abs() < 0.01, "95% of medium's");
        let mut w = range(ArmorClass::Heavy, 120.0);
        let rail = rifle(&w);
        w.players[0].loadout.push(rail);
        w.players[1].armor = ArmorClass::Light;
        assert!(w.players[0].energy > 30.0);
        fire(&mut w);
        assert!(w.players[0].energy < 0.2, "the shot emptied it (a tick of regen since): {}", w.players[0].energy);
        assert!(w.players[1].alive, "still in flight at 120 m");
        for _ in 0..40 { w.physics_step(); }
        assert!(!w.players[1].alive, "one hit downed the light");
    }

    /// The base game's target marker is a held laser: what it touches is
    /// its holder's one mark for the team (a foe, followed, or the spot),
    /// staying 12 s after letting go, and it hurts nobody; the drain gun
    /// drains a foe's energy and wears them down.
    #[test]
    fn the_target_marker_marks_and_the_drain_gun_drains() {
        let r = crate::registry::registry();
        let mut w = range(ArmorClass::Light, 60.0);
        let marker = r.weapons.index("target-marker").unwrap() as u8;
        assert!(w.players[0].loadout.push(marker));
        w.input.weapon = marker;
        w.input.fire = true;
        for _ in 0..10 { w.tick(STEP); }
        let team = w.players[0].team;
        assert!(w.players[0].stream.is_some(), "a laser, held");
        assert!(w.marked(team, 1), "marked");
        assert_eq!((w.players[1].health, w.players[0].hits), (100.0, 0), "unhurt");
        assert_eq!((w.marked_by(team), w.marked_spots(team).len()), (vec![1], 0));
        // Swept down onto the ground: the one mark moves there.
        w.players[0].pitch = -0.5;
        for _ in 0..5 { w.tick(STEP); }
        let spot = w.marked_spots(team);
        assert!(w.marked_by(team).is_empty() && spot.len() == 1, "{:?} {spot:?}", w.marked_by(team));
        assert!(spot[0].distance(w.players[0].stream.unwrap()) < 0.01);
        assert!(w.marked_spots(w.players[1].team).is_empty(), "the other team's own");
        // Let go: the laser is off, the target stays 12 s.
        w.input.fire = false;
        for _ in 0..(11.0 / STEP) as usize { w.tick(STEP); }
        assert!(w.players[0].stream.is_none());
        assert_eq!(w.marked_spots(team), spot);
        for _ in 0..(1.5 / STEP) as usize { w.tick(STEP); }
        assert!(w.marked_spots(team).is_empty(), "gone after 12 s");

        let mut w = range(ArmorClass::Heavy, 20.0);
        let drain = r.weapons.index("drain-gun").unwrap() as u8;
        assert!(w.players[0].loadout.push(drain));
        let foe_energy = w.players[1].energy;
        w.input.weapon = drain;
        w.input.fire = true;
        for _ in 0..60 { w.tick(STEP); }
        assert!(foe_energy - w.players[1].energy > 8.0, "drained to {}", w.players[1].energy);
        assert!(w.players[1].health < 100.0, "worn down");
    }

    #[test]
    fn a_mode_can_allow_any_weapons_or_none() {
        // Disc only: every other slot keeps the disc, the rifle too.
        let mut w = armed_with("test-disc-only", &["disc"], ArmorClass::Light);
        w.players[0].loadout.push(RIFLE_SLOT);
        for wanted in 0..=RIFLE_SLOT { assert_eq!(w.allowed_weapon(0, wanted), 0, "wanted {wanted}"); }
        w.input.weapon = 1;
        w.input.fire = true;
        w.tick(STEP * 2.0);
        assert_eq!((w.players[0].weapon, w.players[0].shots), (0, 1), "asked for the chaingun, fired the disc");

        // Two of them, by armor: light has the chaingun, heavy the mortar too;
        // holding a refused slot moves to the first allowed one.
        let mut w = armed_with("test-chain-mortar", &["chaingun", "mortar"], ArmorClass::Light);
        w.players[0].weapon = 0;
        assert_eq!([0, 1, 2].map(|s| w.allowed_weapon(0, s)), [1, 1, 1]);
        w.players[0].weapon = 1;
        w.players[0].armor = ArmorClass::Heavy;
        w.equip(0);
        use super::arsenal::MORTAR;
        assert_eq!([0, 1, MORTAR].map(|s| w.allowed_weapon(0, s)), [1, 1, MORTAR]);

        // Three, with the rifle only once bought.
        let mut w = armed_with("test-three", &["disc", "grenade_launcher", "laser"], ArmorClass::Light);
        assert_eq!([0, 1, 2, 3].map(|s| w.allowed_weapon(0, s)), [0, 0, 2, 0]);
        w.players[0].loadout.push(RIFLE_SLOT);
        assert_eq!(w.allowed_weapon(0, RIFLE_SLOT), RIFLE_SLOT);

        // None: nothing fires and no round is spent.
        let mut w = armed_with("test-unarmed", &[], ArmorClass::Light);
        let ammo = w.players[0].ammo.clone();
        for wanted in 0..=2 {
            w.input.weapon = wanted;
            w.input.fire = true;
            for _ in 0..30 { w.tick(STEP); }
            assert!(!w.armed(0));
        }
        assert_eq!((w.players[0].shots, w.players[0].ammo.clone(), w.players[1].health), (0, ammo, 100.0));
        assert!(w.discs.is_empty());

        // Every weapon (the base modes): the rifle rule alone.
        let w = range(ArmorClass::Light, 20.0);
        assert_eq!([0, 1, 2, 3].map(|s| w.allowed_weapon(0, s)), [0, 1, 2, 0]);
        assert!(w.armed(0));
    }

    #[test]
    fn rifles_are_bought_not_carried_and_follow_the_armor() {
        let mut w = range(ArmorClass::Light, 30.0);
        assert!(!w.players[0].loadout.carries(RIFLE_SLOT), "not carried by default");
        assert_eq!(w.allowed_weapon(0, RIFLE_SLOT), 0);
        w.players[0].loadout.push(RIFLE_SLOT);
        assert_eq!(w.allowed_weapon(0, RIFLE_SLOT), RIFLE_SLOT);
        assert_eq!(rifle(&w), RIFLE_SLOT, "light armor's is the laser");
        w.players[0].armor = ArmorClass::Heavy;
        assert_eq!(rifle(&w), super::arsenal::RAILGUN, "heavy armor's the railgun");
        // Lost on death.
        w.kill(0, None, "Fall");
        assert!(!w.players[0].loadout.carries(RIFLE_SLOT));
    }

    #[test]
    fn the_laser_hits_instantly_at_long_range_and_costs_energy() {
        for ahead in [20.0, 180.0] {
            let mut w = range(ArmorClass::Light, ahead);
            w.players[0].loadout.push(RIFLE_SLOT);
            w.players[0].energy = crate::defs::movement().energy_max;
            fire(&mut w);
            assert!(w.players[1].health <= 100.0-crate::defs::weapons().laser.damage+0.01, "{ahead} m: hit at once ({})", w.players[1].health);
            assert!(w.players[0].energy <= crate::defs::movement().energy_max-crate::defs::weapons().laser.energy+1.0, "spent energy");
            assert!(w.discs.iter().any(|d| d.kind == LASER_KIND), "a visible beam");
        }
        // Without the energy it doesn't fire.
        let mut w = range(ArmorClass::Light, 20.0);
        w.players[0].loadout.push(RIFLE_SLOT);
        w.players[0].energy = crate::defs::weapons().laser.energy-1.0;
        fire(&mut w);
        assert_eq!(w.players[1].health, 100.0);
    }

    #[test]
    fn the_railgun_slug_is_fast_but_not_instant_and_hits_hard() {
        let mut w = range(ArmorClass::Heavy, 200.0);
        let rail = rifle(&w) as usize;
        w.players[0].loadout.push(rail as u8);
        w.players[0].vel = Vec3::new(10.0, 0.0, 0.0);
        let rounds = w.players[0].ammo[rail];
        assert!(rounds > 0);
        fire(&mut w);
        assert_eq!(w.players[0].ammo[rail], rounds-1);
        let slug = w.discs.iter().find(|d| d.kind == RAIL_KIND).expect("a slug in flight").clone();
        assert!(w.players[1].health == 100.0, "not instant at 200 m");
        let sideways = slug.vel.x;
        assert!(sideways > 0.5 && sideways < 3.0, "a little of the shooter's motion: {sideways}");
        // The target stands still where the slug crosses its line (left to
        // itself it would move as a bot; the shooter's motion carries the
        // slug a metre or two aside over 200 m).
        let ahead = (w.players[1].pos.z - slug.pos.z) / slug.vel.z;
        let stand = slug.pos + slug.vel * ahead - Vec3::Y * 0.9;
        // 600 m/s: still on its way a sixth of a second in, there within half a second.
        let step = |w: &mut World| { w.physics_step(); w.players[1].pos = stand; w.players[1].vel = Vec3::ZERO; };
        for _ in 0..10 { step(&mut w); }
        assert!(w.players[1].health == 100.0, "in flight at 0.2 s");
        for _ in 0..20 { step(&mut w); }
        assert!(w.players[1].health <= 100.0-crate::defs::weapons().railgun.damage*0.5+0.01 || !w.players[1].alive, "hit hard: {}", w.players[1].health);
    }
}
