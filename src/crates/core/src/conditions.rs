//! Per-match conditions for modes that ask for them (`ModeDef::conditions`:
//! the deathmatch modes, or a mod's): the server rolls a time of day,
//! weather, wind and one gameplay twist at each round start and sends them
//! in every snapshot. The time and weather only change how the map is drawn
//! (lighting, sky, fog and falling rain or snow); the twist changes the rules
//! (see `sim::deathmatch`). A server may choose the time and weather, each
//! fixed or random, in every mode (`Forecast`, `World::forecast`), never a
//! twist the mode doesn't have. Otherwise a mode uses `Conditions::default()`,
//! the map's own look with no twist.
//!
//! Baked lightmaps assume the map's sun, so the sun never moves: dawn, dusk
//! and night recolour and dim it instead (at night it is the moon).
use serde::{Deserialize, Serialize};

use crate::look::{Resolved, ResolvedSky};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay { #[default] Map, Dawn, Day, Dusk, Night }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather { #[default] Clear, Overcast, Rain, Storm, Snow, Fog }

/// One rule change for the round, or none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Twist {
    #[default]
    None,
    /// Everyone spawns carrying their armor's rifle.
    Rifles,
    /// Everyone takes half again as much damage.
    GlassCannon,
    /// Everyone spawns in heavy armor.
    Heavies,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Conditions {
    pub time: TimeOfDay,
    pub weather: Weather,
    /// Wind in m/s along world X and Z: it slants rain and drifts snow.
    pub wind: [f32; 2],
    pub twist: Twist,
}

/// A host's choice for the time of day or the weather: as the mode does (the
/// map's own look, or the deathmatch modes' roll), random each round, or
/// always one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick<T> { #[default] Mode, Random, Always(T) }

/// A server's time of day and weather (`PEAKRUNNER_TIME_OF_DAY`,
/// `PEAKRUNNER_WEATHER`; "time_of_day" and "weather" in server.json).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Forecast { pub time: Pick<TimeOfDay>, pub weather: Pick<Weather> }

impl TimeOfDay {
    /// Every time as hosts name them; "map" is the map's own lighting.
    pub const ALL: [(TimeOfDay, &'static str); 5] = [(TimeOfDay::Map, "map"), (TimeOfDay::Dawn, "dawn"), (TimeOfDay::Day, "day"),
        (TimeOfDay::Dusk, "dusk"), (TimeOfDay::Night, "night")];
}

impl Weather {
    pub const ALL: [(Weather, &'static str); 6] = [(Weather::Clear, "clear"), (Weather::Overcast, "overcast"), (Weather::Rain, "rain"),
        (Weather::Storm, "storm"), (Weather::Snow, "snow"), (Weather::Fog, "fog")];
}

impl<T: Copy + PartialEq> Pick<T> {
    /// "default" (or nothing), "random", or one of `all`'s names.
    pub fn parse(text: &str, all: &[(T, &'static str)]) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "default" => Ok(Pick::Mode),
            "random" => Ok(Pick::Random),
            name => all.iter().find(|(_, n)| *n == name).map(|&(v, _)| Pick::Always(v)).ok_or_else(|| format!(
                "\"{name}\" isn't one of default, random, {}", all.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", "))),
        }
    }
    /// The name `parse` reads ("default" for `Mode`).
    pub fn name(self, all: &[(T, &'static str)]) -> &'static str {
        match self {
            Pick::Mode => "default",
            Pick::Random => "random",
            Pick::Always(v) => all.iter().find(|(x, _)| *x == v).map_or("default", |(_, n)| n),
        }
    }
}

/// Picks from `roll` values in 0..1, so the caller supplies the randomness.
fn pick<T: Copy>(r: f32, table: &[(T, f32)]) -> T {
    let total: f32 = table.iter().map(|(_, w)| w).sum();
    let mut left = r.clamp(0.0, 0.9999) * total;
    for &(v, w) in table {
        if left < w { return v; }
        left -= w;
    }
    table[table.len() - 1].0
}

impl Conditions {
    /// A random round: `r` yields values in 0..1.
    pub fn roll(r: impl FnMut() -> f32) -> Self { Self::roll_as(r, None, None) }

    /// A random round with the time or the weather chosen (the wind follows
    /// the weather). It draws the same random values either way.
    pub fn roll_as(mut r: impl FnMut() -> f32, time: Option<TimeOfDay>, weather: Option<Weather>) -> Self {
        let rolled = pick(r(), &[(TimeOfDay::Dawn, 1.0), (TimeOfDay::Day, 2.0), (TimeOfDay::Dusk, 1.0), (TimeOfDay::Night, 1.6)]);
        let time = time.unwrap_or(rolled);
        let rolled = pick(r(), &[(Weather::Clear, 3.0), (Weather::Overcast, 1.5), (Weather::Rain, 1.6),
            (Weather::Storm, 0.8), (Weather::Snow, 1.0), (Weather::Fog, 1.0)]);
        let weather = weather.unwrap_or(rolled);
        let angle = r() * std::f32::consts::TAU;
        let strength = match weather { Weather::Storm => 9.0 + 5.0 * r(), Weather::Clear | Weather::Fog => 1.5 * r(), _ => 2.0 + 4.0 * r() };
        let twist = pick(r(), &[(Twist::None, 4.0), (Twist::Rifles, 1.0), (Twist::GlassCannon, 1.0), (Twist::Heavies, 1.0)]);
        Self { time, weather, wind: [angle.cos() * strength, angle.sin() * strength], twist }
    }

    pub fn precipitation(&self) -> bool { matches!(self.weather, Weather::Rain | Weather::Storm | Weather::Snow) }

    /// Short text for the start banner and scoreboard, e.g. "Night · Rain".
    pub fn describe(&self) -> String {
        let time = match self.time { TimeOfDay::Map => None, TimeOfDay::Dawn => Some("Dawn"), TimeOfDay::Day => Some("Day"),
            TimeOfDay::Dusk => Some("Dusk"), TimeOfDay::Night => Some("Night") };
        let weather = match self.weather { Weather::Clear => "Clear", Weather::Overcast => "Overcast", Weather::Rain => "Rain",
            Weather::Storm => "Storm", Weather::Snow => "Snow", Weather::Fog => "Fog" };
        let twist = match self.twist { Twist::None => None, Twist::Rifles => Some("Rifles for all"),
            Twist::GlassCannon => Some("Glass cannon"), Twist::Heavies => Some("Heavies only") };
        [time, Some(weather), twist].into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }

    /// The map's resolved look under these conditions. The default
    /// conditions return it unchanged.
    pub fn apply(&self, base: Resolved) -> Resolved {
        if *self == Conditions::default() { return base; }
        let mut out = base;
        let mul = |c: [f32; 3], m: [f32; 3]| [c[0] * m[0], c[1] * m[1], c[2] * m[2]];
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        // Sky gradient for this time of day, used when the time or the
        // weather replaces the map's sky.
        let (zenith, horizon) = match self.time {
            TimeOfDay::Dawn => ([0.28, 0.36, 0.58], [0.95, 0.62, 0.45]),
            TimeOfDay::Dusk => ([0.18, 0.2, 0.42], [0.92, 0.45, 0.28]),
            TimeOfDay::Night => ([0.02, 0.03, 0.075], [0.09, 0.12, 0.2]),
            _ => ([0.3, 0.48, 0.78], [0.72, 0.8, 0.9]),
        };
        match self.time {
            TimeOfDay::Dawn => {
                out.sun_color = mul(base.sun_color, [1.1, 0.78, 0.6]);
                out.ambient_sky = mul(base.ambient_sky, [0.85, 0.82, 0.95]);
                out.ambient_ground = mul(base.ambient_ground, [0.95, 0.8, 0.72]);
                out.exposure = base.exposure * 0.9;
            }
            TimeOfDay::Dusk => {
                out.sun_color = mul(base.sun_color, [1.15, 0.62, 0.42]);
                out.ambient_sky = mul(base.ambient_sky, [0.75, 0.66, 0.8]);
                out.ambient_ground = mul(base.ambient_ground, [0.9, 0.66, 0.55]);
                out.exposure = base.exposure * 0.82;
            }
            TimeOfDay::Night => {
                // Moonlight: a cool, dim key light and a dark blue ambient.
                out.sun_color = [0.5, 0.6, 0.85];
                out.ambient_sky = [0.36, 0.42, 0.6];
                out.ambient_ground = [0.18, 0.19, 0.25];
                out.exposure = base.exposure * 0.7;
                out.sun_disc = 0.35;
            }
            TimeOfDay::Map | TimeOfDay::Day => {}
        }
        let replace_sky = !matches!(self.time, TimeOfDay::Map | TimeOfDay::Day) || self.weather != Weather::Clear;
        if replace_sky {
            let mut sky = base.sky.unwrap_or(ResolvedSky { zenith, horizon, cloud_cover: 0.25,
                cloud_color: [0.86, 0.86, 0.84], cloud_scale: 1.0, sun_size: 0.022 });
            if !matches!(self.time, TimeOfDay::Map | TimeOfDay::Day) || base.sky.is_none() {
                sky.zenith = zenith;
                sky.horizon = horizon;
            }
            if self.time == TimeOfDay::Night { sky.sun_size = 0.016; sky.cloud_color = [0.1, 0.11, 0.16]; }
            else if matches!(self.time, TimeOfDay::Dawn | TimeOfDay::Dusk) { sky.cloud_color = mix(horizon, [1.0, 0.9, 0.85], 0.4); }
            out.sky = Some(sky);
        }
        let grey = |c: [f32; 3], t: f32| { let l = (c[0] + c[1] + c[2]) / 3.0; mix(c, [l, l, l], t) };
        let cloud = |out: &mut Resolved, cover: f32, dark: f32, sun: f32| {
            if let Some(s) = out.sky.as_mut() {
                s.cloud_cover = s.cloud_cover.max(cover);
                s.cloud_color = mul(grey(s.cloud_color, 0.6), [dark; 3]);
                s.zenith = mul(grey(s.zenith, 0.6), [dark; 3]);
                s.horizon = mul(grey(s.horizon, 0.5), [dark; 3]);
            }
            out.sun_color = mul(out.sun_color, [sun; 3]);
            out.sun_disc *= sun * 0.5;
        };
        match self.weather {
            Weather::Clear => {}
            Weather::Overcast => cloud(&mut out, 0.85, 0.85, 0.55),
            Weather::Rain => cloud(&mut out, 0.95, 0.62, 0.4),
            Weather::Storm => { cloud(&mut out, 1.0, 0.42, 0.28); out.exposure *= 0.85; }
            Weather::Snow => cloud(&mut out, 0.95, 0.9, 0.45),
            Weather::Fog => cloud(&mut out, 0.7, 0.9, 0.5),
        }
        // Height fog thickens with weather; fog and snow hide the far field.
        let thick = match self.weather { Weather::Fog => 0.012, Weather::Storm => 0.004, Weather::Rain | Weather::Snow => 0.0025,
            Weather::Overcast => 0.0008, Weather::Clear => 0.0 };
        if thick > 0.0 {
            let [d, b, f] = base.height_fog;
            out.height_fog = if d > 0.0 { [d.max(thick), b, f.max(60.0)] } else { [thick, 0.0, 140.0] };
        }
        out
    }

    /// Distance fog under these conditions: the colour and the near and far
    /// distances, from the map's own.
    pub fn fog(&self, color: [f32; 3], near: f32, far: f32) -> ([f32; 3], f32, f32) {
        if *self == Conditions::default() { return (color, near, far); }
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let mut c = match self.time {
            TimeOfDay::Dawn => mix(color, [0.9, 0.66, 0.55], 0.45),
            TimeOfDay::Dusk => mix(color, [0.62, 0.38, 0.3], 0.5),
            TimeOfDay::Night => [0.07, 0.09, 0.15],
            _ => color,
        };
        let (mut near, mut far) = (near, far);
        let wet = match self.weather {
            Weather::Clear => None,
            Weather::Overcast => Some((0.3, 0.9)),
            Weather::Rain => Some((0.5, 0.65)),
            Weather::Storm => Some((0.6, 0.5)),
            Weather::Snow => Some((0.6, 0.55)),
            Weather::Fog => Some((0.8, 0.3)),
        };
        if let Some((grey, reach)) = wet {
            let l = (c[0] + c[1] + c[2]) / 3.0;
            let target = if self.time == TimeOfDay::Night { [l, l, l * 1.1] } else { [l * 0.95 + 0.1, l * 0.95 + 0.1, l + 0.12] };
            c = mix(c, target, grey);
            far *= reach;
            near = (near * reach).min(far * 0.6);
        }
        if self.time == TimeOfDay::Night { far *= 0.8; near = near.min(far * 0.5); }
        (c, near, far)
    }

    /// Scene brightness for entities drawn outside the map shader (players,
    /// projectiles): 1 by day.
    pub fn entity_light(&self) -> f32 {
        let t = match self.time { TimeOfDay::Night => 0.6, TimeOfDay::Dusk => 0.8, TimeOfDay::Dawn => 0.88, _ => 1.0 };
        let w = match self.weather { Weather::Rain => 0.85, Weather::Storm => 0.75, Weather::Overcast => 0.9, _ => 1.0 };
        t * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_conditions_leave_the_map_look_alone() {
        let base = crate::look::Look::default().resolved();
        assert_eq!(Conditions::default().apply(base), base);
        assert_eq!(Conditions::default().fog([0.5; 3], 200.0, 450.0), ([0.5; 3], 200.0, 450.0));
    }

    #[test]
    fn rolls_cover_every_time_and_weather_and_night_is_dark() {
        let mut seed = 7u32;
        let mut r = || { seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); (seed >> 8) as f32 / 16_777_216.0 };
        let rolls: Vec<Conditions> = (0..400).map(|_| Conditions::roll(&mut r)).collect();
        for t in [TimeOfDay::Dawn, TimeOfDay::Day, TimeOfDay::Dusk, TimeOfDay::Night] { assert!(rolls.iter().any(|c| c.time == t), "{t:?}"); }
        for w in [Weather::Clear, Weather::Overcast, Weather::Rain, Weather::Storm, Weather::Snow, Weather::Fog] {
            assert!(rolls.iter().any(|c| c.weather == w), "{w:?}");
        }
        assert!(rolls.iter().any(|c| c.twist != Twist::None) && rolls.iter().any(|c| c.twist == Twist::None));
        let base = crate::look::Look::default().resolved();
        let night = Conditions { time: TimeOfDay::Night, ..Default::default() }.apply(base);
        assert!(night.exposure < base.exposure * 0.8 && night.sky.is_some());
        assert_eq!(night.sun_direction, base.sun_direction, "the baked sun never moves");
        let fog = Conditions { weather: Weather::Fog, ..Default::default() };
        assert!(fog.fog([0.5; 3], 200.0, 450.0).2 < 200.0);
        assert!(fog.apply(base).height_fog[0] > 0.0);
        assert_eq!(Conditions { time: TimeOfDay::Night, weather: Weather::Rain, twist: Twist::Rifles, ..Default::default() }.describe(),
            "Night · Rain · Rifles for all");
    }

    #[test]
    fn hosts_name_each_time_and_weather_or_default_or_random() {
        assert_eq!(Pick::parse("", &TimeOfDay::ALL), Ok(Pick::Mode));
        assert_eq!(Pick::parse("Random", &Weather::ALL), Ok(Pick::Random));
        assert_eq!(Pick::parse(" night ", &TimeOfDay::ALL), Ok(Pick::Always(TimeOfDay::Night)));
        assert!(Pick::parse("noon", &TimeOfDay::ALL).unwrap_err().contains("dawn, day, dusk, night"));
        for (t, name) in TimeOfDay::ALL { assert_eq!(Pick::parse(name, &TimeOfDay::ALL).map(|p| p.name(&TimeOfDay::ALL)), Ok(name), "{t:?}"); }
        for (w, name) in Weather::ALL { assert_eq!(Pick::parse(name, &Weather::ALL).map(|p| p.name(&Weather::ALL)), Ok(name), "{w:?}"); }
        // Choosing draws the same values as rolling, so nothing else shifts.
        let draws = |time, weather| { let mut n = 0; Conditions::roll_as(|| { n += 1; 0.5 }, time, weather); n };
        assert_eq!(draws(None, None), draws(Some(TimeOfDay::Night), Some(Weather::Snow)));
    }
}
