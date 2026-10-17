//! Server-only policy. Clients never submit rotation entries.
use peakrunner_core::{map_catalog::SupportedMode, terrain::MapId};
use serde::Deserialize;

/// One rotation entry: a map in a mode, or a playlist that expands to every
/// supported map: `{"playlist":"ctf_cnh"}` (every CTF map in CTF, then every
/// Capture & Hold map in Capture & Hold), `{"playlist":"football"}`, or a single
/// mode (`"ctf"`, `"capture_and_hold"`).
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Map(MapEntry),
    Playlist(PlaylistEntry),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapEntry { map: String, mode: SupportedMode }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistEntry { playlist: String }

/// A playlist's (map, mode) pairs, from the maps that support each mode.
fn playlist(name: &str) -> Result<Vec<(MapId, SupportedMode)>, String> {
    use peakrunner_core::map_catalog::maps_for;
    let modes: &[SupportedMode] = match name {
        "ctf_cnh" => &[SupportedMode::Ctf, SupportedMode::CaptureAndHold],
        "ctf" => &[SupportedMode::Ctf],
        "capture_and_hold" => &[SupportedMode::CaptureAndHold],
        "football" => &[SupportedMode::Football],
        "team_deathmatch" => &[SupportedMode::TeamDeathmatch],
        "rabbit" => &[SupportedMode::Rabbit],
        "team_rabbit" => &[SupportedMode::TeamRabbit],
        // A duel server: its arenas draw from these maps (`duels`).
        "duel" => &[SupportedMode::Duel],
        _ => return Err(format!("unknown playlist {name}; use ctf_cnh, ctf, capture_and_hold, football, team_deathmatch, rabbit, team_rabbit or duel")),
    };
    Ok(modes.iter().flat_map(|&mode| maps_for(mode).into_iter().map(move |map| (map, mode))).collect())
}

/// The matches a server plays, in turn. In random order (`shuffle`), each
/// time through is a new order, never starting with the match just played.
#[derive(Clone)]
pub struct Rotation {
    maps: Vec<(MapId, SupportedMode)>,
    cursor: usize,
    /// The entries as configured (votes list them in this order).
    listed: Vec<(MapId, SupportedMode)>,
    /// Random order's stream (xorshift); None: in order.
    rng: Option<u64>,
}

impl Rotation {
    /// One map in its natural mode: Football on a stadium, CTF elsewhere.
    pub fn single(map: MapId) -> Self {
        let mode = if peakrunner_core::map_catalog::supports(map, SupportedMode::Ctf) { SupportedMode::Ctf } else { SupportedMode::Football };
        Self { maps: vec![(map, mode)], cursor: 0, listed: vec![(map, mode)], rng: None }
    }
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > 8192 { return Err("rotation exceeds 8 KiB".into()); }
        let entries: Vec<Entry> = serde_json::from_str(json).map_err(|e| format!("invalid rotation: {e}"))?;
        if entries.is_empty() || entries.len() > 32 { return Err("rotation requires 1–32 entries".into()); }
        let mut maps = Vec::new();
        for entry in entries {
            match entry {
                Entry::Playlist(p) => maps.extend(playlist(&p.playlist)?),
                Entry::Map(e) => maps.push(Self::entry(e)?),
            }
        }
        if maps.is_empty() || maps.len() > 64 { return Err("rotation must expand to 1–64 matches".into()); }
        // Duel entries make a duel server, whose arenas draw maps from them:
        // one duel mode (the base one, or a mod's variant), nothing else.
        let duels = maps.iter().filter(|(_, mode)| mode.rules() == peakrunner_core::modes::Rules::Duel).count();
        if duels > 0 && (duels < maps.len() || maps.iter().any(|(_, mode)| *mode != maps[0].1)) {
            return Err("duel entries make a duel server: one duel mode, not mixed with other modes".into());
        }
        if maps.iter().any(|(_, mode)| *mode == SupportedMode::DuelLobby) { return Err("duel_lobby is a duel server's own lobby; use duel".into()); }
        Ok(Self { listed: maps.clone(), maps, cursor: 0, rng: None })
    }
    /// Play in a random order from now on, starting from a shuffled one
    /// (`seed`: any number; the server draws it at start).
    pub fn shuffle(&mut self, seed: u64) {
        self.rng = Some(seed | 1);
        self.cursor = 0;
        self.reorder(None);
    }
    /// A new random order, not starting with `last` when there's a choice.
    fn reorder(&mut self, last: Option<(MapId, SupportedMode)>) {
        let Some(mut x) = self.rng else { return };
        let mut next = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
        for i in (1..self.maps.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            self.maps.swap(i, j);
        }
        if let Some(last) = last.filter(|l| self.maps.iter().any(|e| e != l)) {
            if self.maps[0] == last {
                let k = self.maps.iter().position(|e| *e != last).expect("another entry");
                self.maps.swap(0, k);
            }
        }
        self.rng = Some(x);
    }
    fn entry(e: MapEntry) -> Result<(MapId, SupportedMode), String> {
        {
            let id = MapId::parse(&e.map).ok_or_else(|| unknown_map(&e.map))?;
            if !cfg!(test) && id == MapId::Valley { return Err(format!("Valley is retired; {AVAILABLE}")); }
            // What the mode needs of a map (its definition's `requires`).
            use peakrunner_core::modes::Rules;
            let (def, key) = (e.mode.def(), e.mode.key());
            // Free-for-all is implemented but not offered yet; duels are
            // offered only to rotations (a duel server), not in menus.
            if !def.offered && def.rules != Rules::Duel {
                return Err(if def.rules == Rules::Deathmatch { format!("{key} (free-for-all) is not offered yet; use team_deathmatch") }
                    else { format!("{key} is not offered") });
            }
            let stadium = peakrunner_core::sim::football::has_field(id);
            if def.requires.football_field && !stadium {
                return Err(format!("{} has no football field; {key} needs a stadium map", e.map));
            }
            // A stadium hosts CTF-rules modes never; Team Deathmatch runs on
            // any map named explicitly, stadiums too (its playlist leaves them out).
            if !def.requires.football_field && stadium && def.rules == Rules::Ctf && !cfg!(test) {
                return Err(format!("{} is a football stadium; use \"mode\":\"football\"", e.map));
            }
            let points = peakrunner_core::map_pack::on(id).map_or(0, |p| p.manifest.control_points.len());
            if points < def.requires.control_points {
                return Err(format!("{} has {points} control point(s); {key} needs at least {}", e.map, def.requires.control_points));
            }
            Ok((id, e.mode))
        }
    }
    pub fn current(&self) -> MapId { self.maps[self.cursor].0 }
    pub fn current_mode(&self) -> SupportedMode { self.maps[self.cursor].1 }
    /// A duel server's rotation: its arenas' maps and duel mode (`duel`,
    /// or a mod's variant of it).
    pub fn duels(&self) -> Option<(Vec<MapId>, SupportedMode)> {
        let mode = self.current_mode();
        if mode.rules() != peakrunner_core::modes::Rules::Duel { return None; }
        let mut maps: Vec<MapId> = Vec::new();
        for (map, _) in &self.listed { if !maps.contains(map) { maps.push(*map); } }
        Some((maps, mode))
    }
    /// The distinct entries players may vote for, in configured order.
    pub fn choices(&self) -> Vec<(MapId, SupportedMode)> {
        let mut out: Vec<(MapId, SupportedMode)> = Vec::new();
        for e in &self.listed { if !out.contains(e) { out.push(*e); } }
        out
    }
    /// Move the cursor to `entry` (a passed vote), so the cycle continues from it.
    pub fn jump_to(&mut self, entry: (MapId, SupportedMode)) -> bool {
        match self.maps.iter().position(|e| *e == entry) { Some(i) => { self.cursor = i; true } None => false }
    }
    pub fn advance(&mut self) -> MapId {
        let last = self.maps[self.cursor];
        self.cursor = (self.cursor + 1) % self.maps.len();
        if self.cursor == 0 { self.reorder(Some(last)); }
        self.current()
    }
    pub fn reset(&mut self) -> MapId { self.cursor = 0; self.current() }
}

pub(crate) const AVAILABLE: &str =
    "use old-holler, tower-complex, cairnhold, frostline, dustreach, ozarktic-blast, reefbreak, ashveil, longfield or highgoal (football)";

/// The startup error for a map key that isn't known.
pub(crate) fn unknown_map(key: &str) -> String {
    format!("map is not installed: {key}; {AVAILABLE}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duel_entries_make_a_duel_server_and_mix_with_nothing() {
        let r = Rotation::parse(r#"[{"playlist":"duel"}]"#).unwrap();
        let (maps, mode) = r.duels().expect("a duel server");
        assert_eq!(mode, SupportedMode::Duel);
        assert!(maps.len() >= 8 && !maps.iter().any(|m| peakrunner_core::sim::football::has_field(*m)), "every map but the stadiums: {maps:?}");
        let two = Rotation::parse(r#"[{"map":"old-holler","mode":"duel"},{"map":"dustreach","mode":"duel"}]"#).unwrap();
        assert_eq!(two.duels().map(|d| d.0), Some(vec![MapId::OldHoller, MapId::Dustreach]));
        assert!(Rotation::parse(r#"[{"map":"old-holler","mode":"ctf"}]"#).unwrap().duels().is_none());
        for bad in [r#"[{"map":"old-holler","mode":"duel"},{"map":"dustreach","mode":"ctf"}]"#, r#"[{"playlist":"duel"},{"playlist":"rabbit"}]"#,
            r#"[{"map":"old-holler","mode":"duel_lobby"}]"#] {
            assert!(Rotation::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn ordered_rotation_wraps_and_resets() {
        let mut r = Rotation::parse(r#"[{"map":"old-holler","mode":"ctf"},{"map":"valley","mode":"ctf"}]"#).unwrap();
        assert_eq!(r.current(), MapId::OldHoller);
        assert_eq!(r.advance(), MapId::Valley);
        assert_eq!(r.advance(), MapId::OldHoller);
        r.advance(); assert_eq!(r.reset(), MapId::OldHoller);   // reset stays available, though empty servers no longer use it
    }
    #[test]
    fn random_order_plays_every_entry_each_time_through_in_a_new_order() {
        let json = r#"[{"playlist":"ctf"},{"map":"cairnhold","mode":"rabbit"},{"map":"dustreach","mode":"team_rabbit"}]"#;
        let listed = Rotation::parse(json).unwrap();
        let mut r = listed.clone();
        r.shuffle(0x5EED);
        let n = r.maps.len();
        let mut orders = Vec::new();
        let mut last = None;
        for _ in 0..6 {
            let mut lap = vec![(r.current(), r.current_mode())];
            assert_ne!(Some(lap[0]), last, "a new lap never repeats the match just played");
            for _ in 1..n { r.advance(); lap.push((r.current(), r.current_mode())); }
            last = lap.last().copied();
            let (mut a, mut b) = (lap.clone(), listed.maps.clone());
            a.sort_by_key(|e| format!("{e:?}")); b.sort_by_key(|e| format!("{e:?}"));
            assert_eq!(a, b, "each lap plays every entry once");
            orders.push(lap);
            r.advance();
        }
        orders.dedup();
        assert!(orders.len() > 1 && orders[0] != listed.maps, "the order changes");
        assert_eq!(r.choices(), listed.choices(), "votes keep the configured order");
        // A passed vote still jumps to its entry.
        assert!(r.jump_to((MapId::Cairnhold, SupportedMode::Rabbit)));
        assert_eq!((r.current(), r.current_mode()), (MapId::Cairnhold, SupportedMode::Rabbit));
        // One entry: nothing to shuffle.
        let mut one = Rotation::parse(r#"[{"map":"cairnhold","mode":"rabbit"}]"#).unwrap();
        one.shuffle(7);
        assert_eq!((one.advance(), one.advance()), (MapId::Cairnhold, MapId::Cairnhold));
    }
    #[test]
    fn invalid_policy_is_rejected_not_replaced() {
        for json in ["[]", "{}", r#"[{"map":"missing","mode":"ctf"}]"#,
            r#"[{"map":"valley","mode":"gungame"}]"#,
            r#"[{"map":"valley","mode":"ctf","script":"x"}]"#] {
            assert!(Rotation::parse(json).is_err(), "{json}");
        }
        let unknown = Rotation::parse(r#"[{"map":"old-holler","mode":"ctf"},{"map":"no-such-map","mode":"ctf"}]"#).err().unwrap();
        assert!(unknown.contains("map is not installed: no-such-map") && unknown.contains("old-holler"), "{unknown}");
        // Every CTF map is embedded; none needs a pack.
        for key in ["tower-complex", "cairnhold", "frostline", "dustreach"] {
            assert!(Rotation::parse(&format!(r#"[{{"map":"{key}","mode":"ctf"}}]"#)).is_ok(), "{key}");
        }
        // Capture & Hold needs a map with at least two control points: Old
        // Holler has three, the Valley fixture has none.
        assert!(Rotation::parse(r#"[{"map":"old-holler","mode":"capture_and_hold"}]"#).is_ok());
        // Tower Complex has three Capture & Hold towers.
        assert!(Rotation::parse(r#"[{"map":"tower-complex","mode":"capture_and_hold"}]"#).is_ok());
        // Football needs a stadium map with a declared field.
        let e = Rotation::parse(r#"[{"map":"frostline","mode":"football"}]"#).err().unwrap();
        assert!(e.contains("no football field"), "{e}");
        assert!(Rotation::parse(r#"[{"map":"longfield","mode":"football"}]"#).is_ok());
        assert!(Rotation::parse(r#"[{"map":"highgoal","mode":"football"}]"#).is_ok());
        // Cairnhold: the Ring and two flank cairns.
        assert!(Rotation::parse(r#"[{"map":"cairnhold","mode":"capture_and_hold"}]"#).is_ok());
        let cnh = Rotation::parse(r#"[{"map":"valley","mode":"capture_and_hold"}]"#).err().unwrap();
        assert!(cnh.contains("capture_and_hold needs at least 2"), "{cnh}");
        assert!(Rotation::parse(&" ".repeat(8193)).is_err());
        assert!(Rotation::parse(&format!("[{}]", vec![r#"{"map":"valley","mode":"ctf"}"#;33].join(","))).is_err());
    }

    /// The official servers' Compose file, absent from the public source
    /// (it stays with their operations): the tests below then pass vacuously.
    fn official_compose() -> Option<String> {
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/match/compose.yaml")).ok()
    }

    #[test]
    fn the_deployed_vps_rotations_parse() {
        let Some(compose) = official_compose() else { return };
        let rotations: Vec<&str> = compose.lines().filter_map(|l| l.trim().strip_prefix("PEAKRUNNER_MATCH_ROTATION: '"))
            .map(|r| r.trim_end_matches('\'')).collect();
        assert_eq!(rotations.len(), 4, "main, football, football-bots and duel (mix and bots are folded blocks)");
        for r in rotations { let parsed = Rotation::parse(r).unwrap_or_else(|e| panic!("{r}: {e}")); assert!(!parsed.maps.is_empty()); }
    }

    /// The official servers' rotations (`deploy/match/compose.yaml`) parse,
    /// both football servers play Football only, the mix plays every
    /// non-stadium map once, mix and bots play the rabbit modes in random
    /// order, and the duel server plays duels only, on every map that has
    /// them.
    #[test]
    fn the_official_rotations_parse() {
        let Some(compose) = official_compose() else { return };
        let mut rotations = Vec::new();
        let mut lines = compose.lines().peekable();
        while let Some(line) = lines.next() {
            let Some((_, value)) = line.split_once("PEAKRUNNER_MATCH_ROTATION:") else { continue };
            let value = value.trim();
            // One quoted line, or a folded block of more-indented lines.
            let json = if value == ">-" {
                let mut block = Vec::new();
                while let Some(next) = lines.next_if(|l| l.starts_with("        ")) { block.push(next.trim()); }
                block.join(" ")
            } else { value.trim_matches('\'').to_string() };
            rotations.push(Rotation::parse(&json).unwrap_or_else(|e| panic!("{json}: {e}")));
        }
        assert_eq!(rotations.len(), 6, "match, football, football-bots, mix, bots, duel");
        for football in &rotations[1..3] {
            assert!(!football.maps.is_empty() && football.maps.iter().all(|(_, m)| *m == SupportedMode::Football), "Football only");
        }
        let mix = &rotations[3];
        assert_eq!(mix.maps.len(), 8);
        let mut maps: Vec<MapId> = mix.maps.iter().map(|(m, _)| *m).collect();
        maps.sort(); maps.dedup();
        assert_eq!(maps.len(), 8, "each map once");
        for mode in [SupportedMode::Ctf, SupportedMode::CaptureAndHold, SupportedMode::TeamDeathmatch, SupportedMode::Rabbit, SupportedMode::TeamRabbit] {
            assert!(mix.maps.iter().any(|(_, m)| *m == mode), "{mode:?}");
        }
        let bots = &rotations[4];
        let count = |mode| bots.maps.iter().filter(|(_, m)| *m == mode).count();
        assert_eq!([SupportedMode::Ctf, SupportedMode::CaptureAndHold, SupportedMode::TeamDeathmatch, SupportedMode::Rabbit, SupportedMode::TeamRabbit].map(count),
            [8, 4, 8, 2, 2], "bots: CTF, C&H, TDM, Rabbit, Team Rabbit");
        assert_eq!(compose.matches("PEAKRUNNER_ROTATION_ORDER: random").count(), 2, "mix and bots in random order");
        assert!(rotations[0].maps.iter().all(|(_, m)| *m == SupportedMode::Ctf), "the main server plays CTF only");
        let duel = &rotations[5];
        let (maps, mode) = duel.duels().expect("a duel server");
        assert_eq!(mode, SupportedMode::Duel);
        assert_eq!(maps, peakrunner_core::map_catalog::maps_for(SupportedMode::Duel), "duels on every map that has them");
    }

    #[test]
    fn playlists_expand_to_every_supported_map() {
        let r = Rotation::parse(r#"[{"playlist":"ctf_cnh"}]"#).unwrap();
        let ctf = peakrunner_core::map_catalog::maps_for(SupportedMode::Ctf);
        let cnh = peakrunner_core::map_catalog::maps_for(SupportedMode::CaptureAndHold);
        assert_eq!(r.maps.len(), ctf.len() + cnh.len());
        assert!(r.maps.iter().take(ctf.len()).all(|(m, mode)| *mode == SupportedMode::Ctf && ctf.contains(m)));
        assert!(r.maps.iter().skip(ctf.len()).all(|(_, mode)| *mode == SupportedMode::CaptureAndHold));
        let f = Rotation::parse(r#"[{"playlist":"football"}]"#).unwrap();
        assert_eq!(f.maps, vec![(MapId::Longfield, SupportedMode::Football), (MapId::Highgoal, SupportedMode::Football)]);
        // Playlists and single entries mix.
        let mixed = Rotation::parse(r#"[{"map":"old-holler","mode":"ctf"},{"playlist":"football"}]"#).unwrap();
        assert_eq!(mixed.maps.len(), 3);
        // Team Deathmatch: every map but the stadiums by default; a stadium
        // only when named. Free-for-all isn't offered yet.
        let dm = Rotation::parse(r#"[{"playlist":"team_deathmatch"}]"#).unwrap();
        // (And never the lobby, Tenfold Tower.)
        let lobbies = peakrunner_core::terrain::maps().iter().filter(|m| peakrunner_core::map_pack::on(m.id).is_some_and(|p| p.manifest.lobby)).count();
        assert_eq!(dm.maps.len(), peakrunner_core::terrain::maps().len() - 2 - lobbies);
        assert!(dm.maps.iter().all(|(m, mode)| *mode == SupportedMode::TeamDeathmatch
            && ![MapId::Valley, MapId::Longfield, MapId::Highgoal].contains(m)));
        assert!(Rotation::parse(r#"[{"map":"longfield","mode":"team_deathmatch"}]"#).is_ok());
        // The rabbit modes, on every map but the stadiums.
        for (name, mode) in [("rabbit", SupportedMode::Rabbit), ("team_rabbit", SupportedMode::TeamRabbit)] {
            let r = Rotation::parse(&format!(r#"[{{"playlist":"{name}"}}]"#)).unwrap();
            assert!(r.maps.len() == dm.maps.len() && r.maps.iter().all(|(_, m)| *m == mode), "{name}");
        }
        assert!(Rotation::parse(r#"[{"map":"ozarktic-blast","mode":"deathmatch"}]"#).is_err());
        assert!(Rotation::parse(r#"[{"playlist":"deathmatch"}]"#).is_err());
        assert!(Rotation::parse(r#"[{"playlist":"gungame"}]"#).is_err());
        assert!(Rotation::parse(r#"[{"playlist":"football","extra":1}]"#).is_err());
    }
}
