//! Portable map identity metadata. Paths and executable scripts are not metadata.
//! This schema is the catalog prerequisite; runtime pack registration is separate.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDescriptor {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub revision: u32,
    pub modes: Vec<SupportedMode>,
    pub team_spawns: [[f32; 3]; 2],
    pub extent: f32,
    pub resolution: u16,
}

/// Only implemented rules can be advertised. Deathmatch is every player for
/// themselves; Team Deathmatch scores each enemy frag for the team.
/// A game mode: a handle into the mode registry (`crate::modes`). The
/// constants name the base modes; a mod's modes get ids when it loads.
/// Serialized as its key (`"ctf"`). Rule-set questions go through `rules()`,
/// so a mod's variant of CTF plays by CTF's rules.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SupportedMode(u16);

#[allow(non_upper_case_globals)]
impl SupportedMode {
    pub const Ctf: SupportedMode = SupportedMode(0);
    pub const CaptureAndHold: SupportedMode = SupportedMode(1);
    pub const Football: SupportedMode = SupportedMode(2);
    pub const Deathmatch: SupportedMode = SupportedMode(3);
    pub const TeamDeathmatch: SupportedMode = SupportedMode(4);
    pub const Rabbit: SupportedMode = SupportedMode(5);
    pub const TeamRabbit: SupportedMode = SupportedMode(6);
    /// A duel server's arenas and its lobby (`sim::duel`), never offered in menus.
    pub const Duel: SupportedMode = SupportedMode(7);
    pub const DuelLobby: SupportedMode = SupportedMode(8);
    /// The base modes, in menu order.
    pub const ALL: [SupportedMode; 9] = [SupportedMode::Ctf, SupportedMode::CaptureAndHold, SupportedMode::Football,
        SupportedMode::Deathmatch, SupportedMode::TeamDeathmatch, SupportedMode::Rabbit, SupportedMode::TeamRabbit,
        SupportedMode::Duel, SupportedMode::DuelLobby];
    pub fn def(self) -> &'static crate::modes::ModeDef { crate::modes::entry(self.0 as usize).def }
    pub fn rules(self) -> crate::modes::Rules { self.def().rules }
    pub fn label(self) -> &'static str { &self.def().label }
    pub fn key(self) -> &'static str { crate::modes::entry(self.0 as usize).key }
    /// Modes the menus and server rotations offer. Free-for-all Deathmatch
    /// is held back for now.
    pub fn offered(self) -> bool { self.def().offered }
    /// A fight on any map with no flags of the map's own or capture points:
    /// Deathmatch, Team Deathmatch, the rabbit modes and duels.
    pub fn deathmatch(self) -> bool { matches!(self.rules(), crate::modes::Rules::Deathmatch | crate::modes::Rules::TeamDeathmatch | crate::modes::Rules::Rabbit | crate::modes::Rules::TeamRabbit | crate::modes::Rules::Duel) }
    /// A mode key, as rotations and snapshots name it: a base mode, or one
    /// of the mods in force (a server's, or this session's).
    pub fn parse(key: &str) -> Option<Self> {
        crate::modes::session(key).or_else(|| crate::modes::BASE.iter().position(|(k, _)| *k == key).map(|i| i as u16)).map(SupportedMode)
    }
    /// Every mode in play: the base modes, then the mods' in force.
    pub fn every() -> Vec<SupportedMode> {
        (0..crate::modes::BASE.len()).map(|i| SupportedMode(i as u16))
            .chain(crate::modes::session_modes().into_iter().map(SupportedMode)).collect()
    }
    pub(crate) fn from_index(i: usize) -> Self { SupportedMode(i as u16) }
}

impl Default for SupportedMode { fn default() -> Self { SupportedMode::Ctf } }

impl std::fmt::Debug for SupportedMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.key()) }
}

impl Serialize for SupportedMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(self.key()) }
}

impl<'de> Deserialize<'de> for SupportedMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let key = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        SupportedMode::parse(&key).ok_or_else(|| serde::de::Error::custom(format!("unknown mode {key}")))
    }
}

/// Whether `map` can host `mode`: stadiums (a manifest football field) host
/// Football; other maps host CTF, and Capture & Hold when they place at
/// least two capture points. Team Deathmatch runs on every other map (the
/// stadiums only by an explicit rotation entry). Free-for-all Deathmatch is
/// implemented but not offered yet (`OFFERED`).
pub fn supports(map: crate::terrain::MapId, mode: SupportedMode) -> bool {
    pack_supports(crate::map_pack::on(map), mode)
}

/// `supports` for a pack (none: the procedural Valley).
pub fn pack_supports(pack: Option<&crate::map_pack::MapPack>, mode: SupportedMode) -> bool {
    // A lobby map hosts the duel lobby alone.
    if pack.is_some_and(|p| p.manifest.lobby) { return mode == SupportedMode::DuelLobby; }
    let stadium = pack.is_some_and(|p| p.manifest.football.is_some());
    let needs = mode.def().requires;
    stadium == needs.football_field && pack.map_or(0, |p| p.manifest.control_points.len()) >= needs.control_points
}

/// Every listed map that hosts `mode`, in menu order.
pub fn maps_for(mode: SupportedMode) -> Vec<crate::terrain::MapId> {
    crate::terrain::maps().into_iter().map(|m| m.id).filter(|&id| supports(id, mode)).collect()
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 48
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-') && !id.ends_with('-') && !id.contains("--")
}

impl MapDescriptor {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != 1 || !valid_id(&self.id) || self.revision == 0 {
            return Err("invalid map identity/schema");
        }
        if self.name.trim() != self.name || self.name.is_empty() || self.name.len() > 80
            || self.name.chars().any(|c| c.is_control() || !c.is_ascii()) {
            return Err("invalid map display name");
        }
        if self.modes.is_empty() || self.modes.len() > 8
            || self.modes.iter().collect::<BTreeSet<_>>().len() != self.modes.len() {
            return Err("invalid supported modes");
        }
        if !self.extent.is_finite() || !(64.0..=8192.0).contains(&self.extent)
            || !(2..=256).contains(&self.resolution) {
            return Err("invalid map dimensions");
        }
        if self.team_spawns.iter().any(|p| p.iter().any(|v| !v.is_finite())
            || p[0] < 0.0 || p[0] > self.extent || p[2] < 0.0 || p[2] > self.extent
            || p[1].abs() > 10000.0) {
            return Err("invalid team spawn region");
        }
        Ok(())
    }
}

/// Validate before installing a catalog: duplicate IDs must never shadow a map.
pub fn validate_catalog(maps: &[MapDescriptor]) -> Result<(), &'static str> {
    if maps.is_empty() || maps.len() > 64 { return Err("invalid catalog size"); }
    let mut ids = BTreeSet::new();
    for map in maps {
        map.validate()?;
        if !ids.insert(&map.id) { return Err("duplicate map identity"); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_rotation_slot_is_an_embedded_original_map() {
        use crate::{map_pack,terrain::{self,MapId}};
        // The lobby (Tenfold Tower) is no rotation slot.
        let listed:Vec<_>=terrain::maps().into_iter().filter(|m|map_pack::on(m.id).is_none_or(|p|!p.manifest.lobby)).collect();
        let slots=[(MapId::OldHoller,"old-holler"),(MapId::TowerComplex,"tower-complex"),
            (MapId::Cairnhold,"cairnhold"),(MapId::Frostline,"frostline"),
            (MapId::Dustreach,"dustreach"),(MapId::Longfield,"longfield"),(MapId::Highgoal,"highgoal"),(MapId::OzarkticBlast,"ozarktic-blast"),(MapId::Reefbreak,"reefbreak"),(MapId::Ashveil,"ashveil")];
        assert_eq!(listed.len(),slots.len());
        for (id,key) in slots {
            assert_eq!(MapId::parse(key),Some(id));
            assert!(listed.iter().any(|m|m.id==id),"{key} listed");
            assert!(map_pack::on(id).is_some(),"{key} is embedded");
        }
    }
    fn descriptor() -> MapDescriptor {
        MapDescriptor { schema: 1, id: "test-map".into(), name: "Test map".into(),
            revision: 1, modes: vec![SupportedMode::Ctf], team_spawns: [[20., 10., 20.], [220., 10., 220.]],
            extent: 256., resolution: 128 }
    }
    #[test]
    fn identities_are_safe_and_unambiguous() {
        for id in [crate::terrain::MapId::Valley, crate::terrain::MapId::OldHoller,
            crate::terrain::MapId::TowerComplex, crate::terrain::MapId::Cairnhold,
            crate::terrain::MapId::Frostline, crate::terrain::MapId::Dustreach, crate::terrain::MapId::Longfield, crate::terrain::MapId::Highgoal, crate::terrain::MapId::OzarkticBlast, crate::terrain::MapId::Reefbreak, crate::terrain::MapId::Ashveil] {
            assert_eq!(crate::terrain::MapId::parse(id.key()), Some(id));
            assert!(valid_id(id.key()));
        }
        assert_eq!(crate::terrain::MapId::parse("OLD-HOLLER"), Some(crate::terrain::MapId::OldHoller));
        assert_eq!(crate::terrain::MapId::parse("../old-holler"), None);
        assert_eq!(crate::terrain::MapId::parse("no-such-map"), None);
        for id in ["", "../old-holler", "a/b", "A", "a\\b", "a b", "-a", "a-", "a--b", ".", "é"] {
            assert!(!valid_id(id), "{id}");
        }
        assert!(!valid_id(&"a".repeat(49)));
        assert!(valid_id("rift-crossing-2"));
        assert!(validate_catalog(&[descriptor()]).is_ok());
        assert!(validate_catalog(&[descriptor(), descriptor()]).is_err());
    }
    #[test]
    fn reject_invalid_bounds_modes_and_versions() {
        let mut d = descriptor(); d.extent = f32::NAN; assert!(d.validate().is_err());
        let mut d = descriptor(); d.team_spawns[0][0] = 257.; assert!(d.validate().is_err());
        let mut d = descriptor(); d.team_spawns[0][1] = f32::INFINITY; assert!(d.validate().is_err());
        let mut d = descriptor(); d.resolution = 257; assert!(d.validate().is_err());
        let mut d = descriptor(); d.modes.clear(); assert!(d.validate().is_err());
        let mut d = descriptor(); d.modes.push(SupportedMode::Ctf); assert!(d.validate().is_err());
        let mut d = descriptor(); d.schema = 2; assert!(d.validate().is_err());
        let mut d = descriptor(); d.revision = 0; assert!(d.validate().is_err());
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value["modes"] = serde_json::json!(["gungame"]);
        assert!(serde_json::from_value::<MapDescriptor>(value).is_err());
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value["script"] = serde_json::json!("execute-me");
        assert!(serde_json::from_value::<MapDescriptor>(value).is_err());
    }

    #[test]
    fn each_mode_lists_only_maps_that_host_it() {
        use crate::terrain::MapId;
        // The default maps; anything else installed (or added by another
        // test) may join the lists after them.
        let defaults = |mode| maps_for(mode).into_iter().filter(|m| m.index() < crate::catalog::WELL_KNOWN.len()).collect::<Vec<_>>();
        let ctf = defaults(SupportedMode::Ctf);
        assert_eq!(ctf, vec![MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach, MapId::OzarkticBlast, MapId::Reefbreak, MapId::Ashveil]);
        assert_eq!(defaults(SupportedMode::Football), vec![MapId::Longfield, MapId::Highgoal]);
        let (cnh, every_ctf) = (maps_for(SupportedMode::CaptureAndHold), maps_for(SupportedMode::Ctf));
        assert!(!cnh.is_empty() && cnh.iter().all(|m| every_ctf.contains(m)));
    }
}
