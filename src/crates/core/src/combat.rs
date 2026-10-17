//! Explosive blasts: damage falloff and kick.
//! Values and deliberate PeakRunner differences: docs/weapon-damage.md.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlastProfile {
    pub radius: f32,
    pub max_damage: f32,
    pub edge_fraction: f32,
    /// Blast impulse, converted to a velocity change using player mass.
    pub impulse: f32,
    /// 0: linear falloff to `edge_fraction` at the radius. Otherwise a
    /// concentrated blast: `max_damage / (1 + (d / core)²)`, half damage at
    /// `core` metres, fading fast (the kick still reaches the full radius).
    pub core: f32,
}

impl BlastProfile {
    /// Distance from the blast to the target's collision surface, in metres.
    /// Linear blasts keep 12% damage at the edge; beyond the radius there is no damage.
    pub fn damage(self, distance: f32) -> f32 {
        if !distance.is_finite() || distance > self.radius {
            return 0.0;
        }
        if self.core > 0.0 {
            let k = distance.max(0.0) / self.core;
            return self.max_damage / (1.0 + k * k);
        }
        self.max_damage * (1.0 - (1.0 - self.edge_fraction) * distance.max(0.0) / self.radius)
    }
}

// The profiles are definitions: each projectile names its blasts
// (`projectiles/*.json`, `blasts/*.json`, docs/weapon-damage.md).

/// The blast a projectile of wire number `kind` makes, whatever its curve.
pub fn blast_of(kind: u8) -> Option<&'static crate::registry::Blast> {
    let r = crate::registry::registry();
    r.blasts.by_id(r.projectiles.get(kind as usize)?.blast.as_deref()?)
}

/// A projectile's blast against equipment and deployables (its
/// `structure_blast`, else its blast), when its curve is linear.
pub fn structure_blast(kind: u8) -> Option<BlastProfile> {
    let r = crate::registry::registry();
    let p = r.projectiles.get(kind as usize)?;
    let b = r.blasts.by_id(p.structure_blast.as_deref().or(p.blast.as_deref())?)?;
    (b.falloff == crate::registry::Falloff::Linear).then(|| b.profile())
}

/// A projectile's blast against players, when its curve is linear (the base
/// turrets' plasma keeps its own quadratic curve: `World::explode_direct`).
pub fn player_weapon_blast(kind: u8) -> Option<BlastProfile> {
    blast_of(kind).filter(|b| b.falloff == crate::registry::Falloff::Linear).map(|b| b.profile())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_armour_conversion_and_falloff() {
        for (p, peak) in [(crate::defs::defs().blasts.disc, 75.75758)] {
            assert!((p.damage(0.) - peak).abs() < 0.0001);
            assert!((p.damage(p.radius * 0.5) - peak * 0.56).abs() < 0.0001);
            assert!((p.damage(p.radius) - peak * 0.12).abs() < 0.0001);
            assert_eq!(p.damage(p.radius + 0.001), 0.);
            assert_eq!(p.damage(f32::NAN), 0.);
            assert_eq!(p.damage(f32::INFINITY), 0.);
            assert_eq!(p.damage(-1.), p.max_damage);
            let mut previous=p.max_damage;
            for i in 1..=100 {
                let damage=p.damage(p.radius*i as f32/100.);
                assert!(damage <= previous);previous=damage;
            }
        }
    }

    /// The mortar is the scariest blast in the game (it kills light
    /// armor out to about 7.8 m and still hurts at 20 m), the grenade launcher
    /// is close to the disc at the centre with twice its reach.
    #[test]
    fn the_mortar_and_grenade_launcher_use_linear_splash() {
        for (p, peak, radius) in [(crate::defs::defs().blasts.mortar, 151.51515, 20.0), (crate::defs::defs().blasts.grenade, 72.72727, 15.0)] {
            assert!((p.damage(0.0) - peak).abs() < 0.001);
            assert!((p.damage(radius) - peak * 0.12).abs() < 0.001);
            assert_eq!(p.damage(radius + 0.01), 0.0);
        }
        assert!(crate::defs::defs().blasts.mortar.damage(7.5) >= 100.0 && crate::defs::defs().blasts.mortar.damage(8.0) < 100.0, "one-shot light armor within ~7.8 m");
        assert!(crate::defs::defs().blasts.mortar.damage(5.0) * 0.5 >= 50.0 && crate::defs::defs().blasts.mortar.damage(15.0) > 50.0, "heavy armor fears it; far hits still hurt");
        assert!(crate::defs::defs().blasts.mortar.damage(2.0) > crate::defs::defs().blasts.disc.damage(2.0) * 2.0, "the mortar out-hits the disc at every distance");
        assert!(crate::defs::defs().blasts.grenade.damage(5.0) > crate::defs::defs().blasts.disc.damage(5.0), "the grenade reaches farther than the disc");
        assert!(crate::defs::defs().blasts.mortar.impulse > 1.5 * crate::defs::defs().blasts.disc.impulse, "an intense shock");
    }

    #[test]
    fn bullets_and_custom_plasma_are_not_rebalanced() {
        assert!(player_weapon_blast(1).is_none());
        assert!(player_weapon_blast(3).is_none());
    }
}
