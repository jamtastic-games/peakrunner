//! Generator announcements ("Enemy generator destroyed", "Your generator is
//! down"), queued on the shared flag announcer. Like the flag calls they come
//! from diffing server-authoritative equipment state between frames, so a
//! repeated or replayed snapshot changes nothing and announces nothing.
use peakrunner_core::equipment::{self, Kind};
use peakrunner_core::sim::World;

#[derive(Clone, PartialEq, Debug)]
struct Seen {
    map: peakrunner_core::terrain::MapId,
    team: usize,
    generators: Vec<(bool, bool)>, // (hull at zero, offline)
}

fn seen(world: &World) -> Option<Seen> {
    let team = world.players.get(world.player_id)?.team.idx();
    let generators = equipment::definitions(world.map).iter().zip(&world.equipment)
        .filter(|(d, _)| d.kind == Kind::Generator).map(|(_, s)| (s.health <= 0.0, s.offline)).collect();
    Some(Seen { map: world.map, team, generators })
}

#[derive(Default)]
pub struct GeneratorWatch { last: Option<Seen> }

impl GeneratorWatch {
    pub fn update(&mut self, world: &World) -> Vec<String> {
        let now = seen(world);
        let mut out = Vec::new();
        if let (Some(was), Some(now)) = (&self.last, &now) {
            if was.map == now.map && was.team == now.team && was.generators.len() == now.generators.len() {
                let defs = equipment::definitions(world.map).iter().filter(|d| d.kind == Kind::Generator);
                let states = equipment::definitions(world.map).iter().zip(&world.equipment)
                    .filter(|(d, _)| d.kind == Kind::Generator).map(|(_, s)| s);
                for (((d, s), before), after) in defs.zip(states).zip(&was.generators).zip(&now.generators) {
                    let ours = d.team as usize == now.team;
                    if !before.0 && after.0 {
                        out.push(if ours { "Your generator is down" } else { "Enemy generator destroyed" }.to_string());
                    } else if before.1 && !after.1 && s.health < d.max_health() {
                        // A repair crossing the online threshold; a round reset
                        // restores full hull at once and is not announced.
                        out.push(if ours { "Your generator is back online" } else { "Enemy generator is back online" }.to_string());
                    }
                }
            }
        }
        self.last = now;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::Team;
    use peakrunner_core::terrain::MapId;

    fn world(team: Team) -> World {
        let mut w = World::new(); w.set_map(MapId::OldHoller); w.start_rift(true);
        w.players[0].team = team; w
    }
    fn generator(w: &World, team: u8) -> usize {
        equipment::definitions(w.map).iter().position(|d| d.kind == Kind::Generator && d.team == team).unwrap()
    }

    #[test]
    fn destruction_and_repair_are_worded_for_each_side_and_never_replayed() {
        for (viewer, gone, back) in [(Team::Ember, "Your generator is down", "Your generator is back online"),
            (Team::Glacier, "Enemy generator destroyed", "Enemy generator is back online")] {
            let mut w = world(viewer); let g = generator(&w, 0); let mut watch = GeneratorWatch::default();
            assert!(watch.update(&w).is_empty(), "the first frame only sets a baseline");
            w.equipment[g].health = 0.0; w.equipment[g].offline = true;
            assert_eq!(watch.update(&w), vec![gone.to_string()]);
            assert!(watch.update(&w).is_empty(), "a repeated snapshot announces nothing");
            w.equipment[g].health = 300.0; w.equipment[g].offline = false;
            assert_eq!(watch.update(&w), vec![back.to_string()]);
        }
    }

    #[test]
    fn a_round_reset_to_full_hull_is_silent() {
        let mut w = world(Team::Ember); let g = generator(&w, 0); let mut watch = GeneratorWatch::default();
        w.equipment[g].health = 0.0; w.equipment[g].offline = true; watch.update(&w);
        w.equipment[g] = equipment::State::new(&equipment::definitions(w.map)[g]);
        assert!(watch.update(&w).is_empty());
    }
}
