//! Optional per-map render look: sun, ambient, exposure, height fog and a
//! procedural sky. Renderer-only data; nothing here affects simulation. Every
//! field is optional so existing packs keep their manifests and fingerprints.
use serde::Deserialize;

/// The fixed sun every lightmap bake and every map used before `look` existed
/// (points toward the light).
pub const DEFAULT_SUN: [f32; 3] = [-0.577_350_3, 0.577_350_3, -0.577_350_3];

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Look {
    /// Direction toward the sun. Baked lightmaps assume [`DEFAULT_SUN`]; a map
    /// that moves the sun must rebake (see [`Look::bake_mismatch_degrees`]).
    pub sun_direction: Option<[f32; 3]>,
    pub sun_color: Option<[f32; 3]>,
    /// Sun disc brightness in the sky; 0 hides the disc.
    pub sun_disc: Option<f32>,
    pub exposure: Option<f32>,
    pub ambient_sky: Option<[f32; 3]>,
    pub ambient_ground: Option<[f32; 3]>,
    pub height_fog: Option<HeightFog>,
    /// Present: draw a procedural gradient-and-cloud sky instead of the
    /// pack's cubemap faces.
    pub sky: Option<ProceduralSky>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeightFog {
    /// Extinction per metre at `base` height.
    pub density: f32,
    pub base: f32,
    /// Metres over which density falls by e.
    pub falloff: f32,
}

fn cloud_color() -> [f32; 3] { [0.86, 0.86, 0.84] }
fn cloud_scale() -> f32 { 1.0 }
fn sun_size() -> f32 { 0.022 }

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProceduralSky {
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    #[serde(default)]
    pub cloud_cover: f32,
    #[serde(default = "cloud_color")]
    pub cloud_color: [f32; 3],
    #[serde(default = "cloud_scale")]
    pub cloud_scale: f32,
    /// Angular radius of the sun disc, radians.
    #[serde(default = "sun_size")]
    pub sun_size: f32,
}

/// Every value the renderer needs, with defaults applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub sun_disc: f32,
    pub exposure: f32,
    pub ambient_sky: [f32; 3],
    pub ambient_ground: [f32; 3],
    /// density, base, falloff; density 0 disables.
    pub height_fog: [f32; 3],
    /// None: cubemap sky.
    pub sky: Option<ResolvedSky>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedSky {
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub cloud_cover: f32,
    pub cloud_color: [f32; 3],
    pub cloud_scale: f32,
    pub sun_size: f32,
}

// Defaults average to the flat 0.55 ambient every map used before hemisphere
// lighting, tilted slightly cool from above and warm from below.
pub const DEFAULT_AMBIENT_SKY: [f32; 3] = [0.575, 0.585, 0.61];
pub const DEFAULT_AMBIENT_GROUND: [f32; 3] = [0.53, 0.515, 0.49];

fn finite(v: &[f32]) -> bool { v.iter().all(|x| x.is_finite()) }
fn colour(c: &[f32; 3], max: f32) -> bool { finite(c) && c.iter().all(|x| (0.0..=max).contains(x)) }
fn normalized(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

impl Look {
    pub fn validate(&self) -> Result<(), String> {
        let bad = |what: &str| Err(format!("Invalid look: {what}"));
        if let Some(d) = self.sun_direction {
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if !finite(&d) || l < 0.1 || d[1] / l < -0.2 { return bad("sun_direction"); }
        }
        if self.sun_color.is_some_and(|c| !colour(&c, 4.0)) { return bad("sun_color"); }
        if self.sun_disc.is_some_and(|v| !v.is_finite() || !(0.0..=4.0).contains(&v)) { return bad("sun_disc"); }
        if self.exposure.is_some_and(|v| !v.is_finite() || !(0.25..=4.0).contains(&v)) { return bad("exposure"); }
        if self.ambient_sky.is_some_and(|c| !colour(&c, 2.0)) { return bad("ambient_sky"); }
        if self.ambient_ground.is_some_and(|c| !colour(&c, 2.0)) { return bad("ambient_ground"); }
        if let Some(f) = &self.height_fog {
            if !finite(&[f.density, f.base, f.falloff]) || !(0.0..=0.1).contains(&f.density)
                || f.base.abs() > 10000.0 || !(0.5..=2000.0).contains(&f.falloff) {
                return bad("height_fog");
            }
        }
        if let Some(s) = &self.sky {
            if !colour(&s.zenith, 2.0) || !colour(&s.horizon, 2.0) || !colour(&s.cloud_color, 2.0)
                || !s.cloud_cover.is_finite() || !(0.0..=1.0).contains(&s.cloud_cover)
                || !s.cloud_scale.is_finite() || !(0.05..=20.0).contains(&s.cloud_scale)
                || !s.sun_size.is_finite() || !(0.0..=0.2).contains(&s.sun_size) {
                return bad("sky");
            }
        }
        Ok(())
    }

    pub fn resolved(&self) -> Resolved {
        Resolved {
            sun_direction: self.sun_direction.map(normalized).unwrap_or(DEFAULT_SUN),
            sun_color: self.sun_color.unwrap_or([1.0; 3]),
            sun_disc: self.sun_disc.unwrap_or(1.0),
            exposure: self.exposure.unwrap_or(1.0),
            ambient_sky: self.ambient_sky.unwrap_or(DEFAULT_AMBIENT_SKY),
            ambient_ground: self.ambient_ground.unwrap_or(DEFAULT_AMBIENT_GROUND),
            height_fog: self.height_fog.as_ref().map(|f| [f.density, f.base, f.falloff]).unwrap_or([0.0, 0.0, 1.0]),
            sky: self.sky.as_ref().map(|s| ResolvedSky {
                zenith: s.zenith, horizon: s.horizon, cloud_cover: s.cloud_cover,
                cloud_color: s.cloud_color, cloud_scale: s.cloud_scale, sun_size: s.sun_size,
            }),
        }
    }

    /// Angle between the requested sun and the sun the lightmaps were baked
    /// with. Anything beyond a few degrees means the pack needs a rebake.
    pub fn bake_mismatch_degrees(&self) -> f32 {
        let d = self.resolved().sun_direction;
        let dot = d[0] * DEFAULT_SUN[0] + d[1] * DEFAULT_SUN[1] + d[2] * DEFAULT_SUN[2];
        dot.clamp(-1.0, 1.0).acos().to_degrees()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_look_resolves_to_the_historical_defaults() {
        let r = Look::default().resolved();
        assert_eq!(r.sun_direction, DEFAULT_SUN);
        assert_eq!(r.sun_color, [1.0; 3]);
        assert_eq!(r.exposure, 1.0);
        assert_eq!(r.height_fog[0], 0.0);
        assert!(r.sky.is_none());
        // The hemisphere ambient averages the old flat 0.55 term.
        for i in 0..3 {
            assert!(((DEFAULT_AMBIENT_SKY[i] + DEFAULT_AMBIENT_GROUND[i]) / 2.0 - 0.55).abs() < 0.03);
        }
        assert!(Look::default().validate().is_ok());
        assert!(Look::default().bake_mismatch_degrees() < 0.01);
    }

    /// A map that moves its sun without rebaking would light its terrain and
    /// entities from one side and its baked buildings from another.
    #[test]
    fn embedded_maps_keep_their_sun_consistent_with_their_bake() {
        use crate::terrain::MapId;
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
            let look = &crate::map_pack::on(map).unwrap().manifest.look;
            look.validate().unwrap();
            let off = look.bake_mismatch_degrees();
            assert!(off < 3.0, "{map:?}: look.sun_direction is {off:.1} degrees from the baked sun; rebake its lightmaps");
        }
    }

    #[test]
    fn look_parses_and_rejects_bad_values() {
        let good: Look = serde_json::from_str(r#"{"sun_direction":[1,2,0.5],"exposure":1.1,
            "height_fog":{"density":0.004,"base":180,"falloff":40},
            "sky":{"zenith":[0.2,0.35,0.6],"horizon":[0.7,0.72,0.75],"cloud_cover":0.4}}"#).unwrap();
        good.validate().unwrap();
        let r = good.resolved();
        assert!((r.sun_direction[0].powi(2) + r.sun_direction[1].powi(2) + r.sun_direction[2].powi(2) - 1.0).abs() < 1e-5);
        assert_eq!(r.sky.unwrap().cloud_color, cloud_color());
        assert!(good.bake_mismatch_degrees() > 10.0, "a moved sun must be flagged for rebake");
        assert!(serde_json::from_str::<Look>(r#"{"unknown":1}"#).is_err());
        for bad in [r#"{"exposure":9}"#, r#"{"sun_direction":[0,-1,0]}"#, r#"{"sun_direction":[0,0,0]}"#,
            r#"{"sun_color":[5,0,0]}"#, r#"{"height_fog":{"density":0.5,"base":0,"falloff":10}}"#,
            r#"{"sky":{"zenith":[0,0,0],"horizon":[0,0,0],"cloud_cover":2}}"#, r#"{"ambient_sky":[-1,0,0]}"#] {
            let look: Look = serde_json::from_str(bad).unwrap();
            assert!(look.validate().is_err(), "{bad}");
        }
    }
}
