//! Dedicated match hosting. No client rendering, audio or directory application.
mod duels;
mod gameserver;
pub mod identity;
pub mod input_buffer;
pub mod link;
pub mod reach;
pub mod rotation;
pub mod quic;
pub mod setup;
pub mod stats;
pub mod websocket;
use peakrunner_protocol as proto;
use peakrunner_discovery::wire;
pub use gameserver::{run_from_args as run_game_server, GameHost, GameHandle};
#[cfg(test)]
use peakrunner_net::{connect, connect_as, connect_private, browse};
#[cfg(test)]
mod directory {
    pub use peakrunner_directory::lan::serve;
    pub use peakrunner_discovery::lan::*;
}
#[cfg(test)]
use directory::serve as serve_directory;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use peakrunner_core::sim::{Command, Phase};
    use std::time::{Duration, Instant};
    use std::net::TcpStream;
    use std::io::Write;

    fn wait(mut pred: impl FnMut() -> bool) {
        let start = Instant::now();
        while !pred() {
            assert!(start.elapsed() < Duration::from_secs(4), "condition timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "slow connected-socket rotation; run explicitly"]
    fn rotation_cycles_connected_clients_through_every_map_and_an_empty_server_stays() {
        use peakrunner_core::terrain::MapId;
        let maps = [MapId::OldHoller, MapId::TowerComplex,
            MapId::Cairnhold, MapId::Frostline, MapId::Dustreach];
        let policy = serde_json::to_string(&maps.iter().map(|m|
            serde_json::json!({"map":m.key(),"mode":"ctf"})).collect::<Vec<_>>()).unwrap();
        let mut host = GameHost::bind("127.0.0.1:0", "Rotation QA", 8, "old-holler")
            .unwrap().with_rotation(&policy).unwrap();
        host.accelerated_rounds = true;
        let port = host.local_addr().port();
        let handle = host.spawn();
        let a = connect_private("127.0.0.1", port, "Alpha", "").unwrap();
        let b = connect_private("127.0.0.1", port, "Beta", "").unwrap();
        wait(|| a.lobby().connected && b.lobby().connected);
        let ids = [a.lobby().player_id, b.lobby().player_id];
        let mut tick = 0;
        let mut seq = 0;
        let mut last_input = Instant::now() - Duration::from_secs(1);
        for map in maps.into_iter().chain([MapId::OldHoller, MapId::TowerComplex]) {
            wait(|| {
                let send = last_input.elapsed() >= Duration::from_millis(20);
                if send { seq += 1; last_input = Instant::now(); }
                for c in [&a,&b] {
                    assert!(c.lobby().error.is_none(), "waiting for {map:?}: {:?}", c.lobby().error);
                    if send { assert!(c.send_input(Command { seq, ..Command::default() }), "input queue full for {map:?}"); }
                }
                [&a,&b].iter().all(|c| c.lobby().snapshot.as_ref().is_some_and(|s|s.map == map))
            });
            for (i,c) in [&a,&b].iter().enumerate() {
                let lobby = c.lobby();
                assert_eq!(lobby.player_id, ids[i]);
                assert!(lobby.error.is_none());
                let state = lobby.snapshot.unwrap();
                assert!(state.tick >= tick);
                assert!(state.players.iter().all(|p|p.pos.is_finite() && p.vel.is_finite()));
                // Each new map's round counts down from its spawns.
                if tick > 0 { assert_eq!(state.phase, Phase::Countdown, "{map:?} began without a countdown"); }
            }
            tick = a.lobby().snapshot.unwrap().tick;
        }
        // A late join receives the current authoritative map and stable peers.
        let late = connect_private("127.0.0.1", port, "Late", "").unwrap();
        wait(|| late.lobby().snapshot.is_some());
        assert_eq!(late.lobby().snapshot.unwrap().map, MapId::TowerComplex);
        a.leave(); b.leave(); late.leave();
        wait(|| handle.players.load(std::sync::atomic::Ordering::Relaxed) == 0);
        // Empty, it keeps its current map instead of going back to the first.
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(handle.status.lock().unwrap().map, "Tower Complex - CTF");
    }

    /// A duel server's lobby is Tenfold Tower, and its floors are blade
    /// zones: over the network, a blade cuts, the blows reach both players'
    /// snapshots, a death counts for nothing and comes back at a spawn point.
    #[test]
    fn a_duel_servers_lobby_is_the_tower_where_blades_count_for_nothing() {
        use peakrunner_core::{map_catalog::SupportedMode, sim::{blade, duel, features}};
        let tower = duel::lobby_map().expect("Tenfold Tower is installed");
        let crown = &features::zones(tower)[9];
        let floor = crown.at[1] - crown.size[1] + 0.3;
        // Both start on the crown 2.2 m apart, the first facing along -z
        // toward the second.
        let host = GameHost::bind("127.0.0.1:0", "Blade QA", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"duel"}]"#).unwrap()
            .with_start([crown.at[0] + 14.0, floor + 0.52, crown.at[2]], 90.0);
        let port = host.local_addr().port();
        let handle = host.spawn();
        let ada = connect_private("127.0.0.1", port, "Ada", "").unwrap();
        let start = Instant::now();
        while ada.lobby().snapshot.is_none() { assert!(start.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(5)); }
        let bo = connect_private("127.0.0.1", port, "Bo", "").unwrap();
        let mut seq = 0;
        // Ada steps after Bo when a cut knocks Bo out of reach.
        let gap = |a: &peakrunner_net::Session| a.lobby().snapshot.map_or(0.0, |s| {
            let at = |n: &str| s.players.iter().find(|p| p.name == n).map(|p| p.pos);
            match (at("Ada"), at("Bo")) { (Some(a), Some(b)) => (a.z - b.z).abs(), _ => 0.0 }
        });
        let mut drive = |fire: bool| {
            seq += 1;
            let step = if gap(&ada) > 1.9 { 1.0 } else { 0.0 };
            ada.send_input(Command { seq, fire, yaw: 0.0, move_z: step, ..Command::default() });
            bo.send_input(Command { seq, yaw: std::f32::consts::PI, ..Command::default() });
            std::thread::sleep(Duration::from_millis(16));
        };
        let me = |c: &peakrunner_net::Session| { let l = c.lobby(); l.snapshot.and_then(|s| s.players.iter().find(|p| p.net_id == l.player_id && p.net_id != 0).cloned()) };
        let start = Instant::now();
        while !(me(&ada).is_some() && me(&bo).is_some_and(|p| p.alive)) { assert!(start.elapsed() < Duration::from_secs(5), "both in"); drive(false); }
        let snap = bo.lobby().snapshot.unwrap();
        assert_eq!((snap.map, snap.mode), (tower, SupportedMode::DuelLobby), "the lobby is the tower");
        assert!(features::blade_zone(tower, me(&bo).unwrap().pos) == Some(9), "on the crown: {}", me(&bo).unwrap().pos);
        // Ada taps until Bo falls; every blow reaches Bo's snapshots.
        let (mut saw_swing, mut saw_cut) = (false, false);
        let start = Instant::now();
        while me(&bo).is_some_and(|p| p.alive) {
            assert!(start.elapsed() < Duration::from_secs(12), "Bo still up: {:?}", me(&bo).map(|p| (p.health, p.pos, p.blade)));
            for k in 0..30 { drive(k < 3); }
            let ada_seen = bo.lobby().snapshot.unwrap().players.iter().find(|p| p.name == "Ada").map(|p| p.blade).unwrap();
            saw_swing |= ada_seen.swing != blade::NONE || ada_seen.t > 0;
            saw_cut |= me(&bo).is_some_and(|p| p.blade.how == blade::CUT && p.blade.struck > 0);
        }
        assert!(saw_cut, "Bo's snapshots carried the cuts");
        let snap = bo.lobby().snapshot.unwrap();
        let (a, b) = (snap.players.iter().find(|p| p.name == "Ada").unwrap(), snap.players.iter().find(|p| p.name == "Bo").unwrap());
        assert_eq!((a.frags, b.losses), (0, 0), "it counts for nothing");
        assert!(snap.feed.iter().all(|e| !matches!(e, peakrunner_core::feed::Entry::Frag { .. })), "no feed line: {:?}", snap.feed);
        let _ = saw_swing;
        // Back in five seconds, at one of the tower's spawn points.
        let fell = Instant::now();
        while !me(&bo).is_some_and(|p| p.alive) { assert!(fell.elapsed() < Duration::from_secs(8), "Bo is back"); drive(false); }
        let waited = fell.elapsed().as_secs_f32();
        assert!(waited > blade::tuning().respawn - 0.6, "back after {waited} s");
        let pos = me(&bo).unwrap().pos;
        let points = peakrunner_core::map_pack::on(tower).unwrap().manifest.spawn_points.concat();
        assert!(points.iter().any(|s| peakrunner_core::glam::Vec3::new(s[0], s[1], s[2]).distance(pos) < 1.0), "at a spawn point: {pos}");
        ada.leave(); bo.leave();
        drop(handle);
    }

    /// A duel server's bots spar in the lobby's tower, two to a floor from
    /// the ground up, under their two-galaxy names.
    #[test]
    fn a_duel_servers_bots_spar_in_the_tower() {
        use peakrunner_core::sim::{blade, features};
        let host = GameHost::bind("127.0.0.1:0", "Spar QA", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"duel"}]"#).unwrap().with_bots(4);
        let port = host.local_addr().port();
        let handle = host.spawn();
        let ada = connect_private("127.0.0.1", port, "Ada", "").unwrap();
        let start = Instant::now();
        let mut seq = 0;
        loop {
            assert!(start.elapsed() < Duration::from_secs(6), "the bots came");
            seq += 1;
            ada.send_input(Command { seq, ..Command::default() });
            std::thread::sleep(Duration::from_millis(16));
            let Some(snap) = ada.lobby().snapshot else { continue };
            let bots: Vec<_> = snap.players.iter().filter(|p| p.is_bot && p.net_id != 0 && p.alive).collect();
            if bots.len() < 3 { continue; }
            for b in &bots {
                assert!(blade::SPARRING_NAMES.contains(&b.name.as_str()), "{}", b.name);
                assert!(features::blade_zone(snap.map, b.pos).is_some_and(|z| z < 2), "{} on the lowest floors: {}", b.name, b.pos);
            }
            break;
        }
        assert_eq!(handle.status.lock().unwrap().bots, 3, "one made room for Ada");
        ada.leave();
        drop(handle);
    }

    /// A duel server: people meet in the lobby, duels play side by side in
    /// their own arenas, a watcher moves from duel to duel, a bot takes a
    /// challenge, and a concession or a departure ends a duel, everyone back
    /// in the lobby with the result.
    #[test]
    fn duels_play_side_by_side_and_people_come_back_to_the_lobby() {
        use peakrunner_core::{map_catalog::SupportedMode, sim::duel::Place};
        let host = GameHost::bind("127.0.0.1:0", "Duel QA", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"duel"},{"map":"tower-complex","mode":"duel"}]"#).unwrap();
        let port = host.local_addr().port();
        let handle = host.spawn();
        let names = ["Ada", "Bo", "Cy", "Di", "Ed"];
        let mut people: Vec<_> = names.iter().map(|n| Some(connect_private("127.0.0.1", port, n, "").unwrap())).collect();
        let mut seq = 0;
        let mut last_input = Instant::now() - Duration::from_secs(1);
        let mut ticks = vec![0u64; names.len()];
        // Wait for `done`, keeping everyone's session alive and checking that
        // no one's snapshots ever go back in time.
        let mut until = |people: &[Option<peakrunner_net::Session>], what: &str, done: &dyn Fn(&[Option<peakrunner_net::Session>]) -> bool| {
            let start = Instant::now();
            while !done(people) {
                assert!(start.elapsed() < Duration::from_secs(6), "timed out waiting for {what}");
                if last_input.elapsed() >= Duration::from_millis(20) {
                    seq += 1; last_input = Instant::now();
                    for c in people.iter().flatten() { c.send_input(Command { seq, ..Command::default() }); }
                }
                for (i, c) in people.iter().enumerate() {
                    let Some(c) = c else { continue };
                    let lobby = c.lobby();
                    assert!(lobby.error.is_none(), "{}: {:?}", names[i], lobby.error);
                    if let Some(s) = &lobby.snapshot { assert!(s.tick >= ticks[i], "{} went back in time", names[i]); ticks[i] = s.tick; }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let mode = |c: &peakrunner_net::Session| c.lobby().snapshot.map(|s| s.mode);
        let board = |c: &peakrunner_net::Session| c.lobby().duels.unwrap_or_default();
        until(&people, "everyone in the lobby", &|p| p.iter().flatten().all(|c| mode(c) == Some(SupportedMode::DuelLobby) && board(c).people.len() == 5));
        let id = |p: &[Option<peakrunner_net::Session>], k: usize| p[k].as_ref().unwrap().lobby().player_id;
        let ids: Vec<u32> = (0..5).map(|k| id(&people, k)).collect();
        fn at(p: &[Option<peakrunner_net::Session>], k: usize) -> &peakrunner_net::Session { p[k].as_ref().unwrap() }
        // Ada challenges Bo, who says yes.
        assert!(at(&people, 0).challenge(ids[1]));
        // Each session's board comes on its own: wait for the one read.
        until(&people, "Bo's invitation", &|p| board(p[1].as_ref().unwrap()).invites.iter().any(|(from, left)| *from == ids[0] && *left > 0));
        until(&people, "Ada's challenge", &|p| board(p[0].as_ref().unwrap()).challenge.map(|c| c.0) == Some(ids[1]));
        assert!(at(&people, 1).answer_challenge(ids[0], true));
        // Cy and Di challenge each other: that's a yes too.
        assert!(at(&people, 2).challenge(ids[3]));
        until(&people, "Di's invitation", &|p| !board(p[3].as_ref().unwrap()).invites.is_empty());
        assert!(at(&people, 3).challenge(ids[2]));
        until(&people, "two duels", &|p| (0..4).all(|k| mode(p[k].as_ref().unwrap()) == Some(SupportedMode::Duel))
            && board(p[4].as_ref().unwrap()).duels.len() == 2);
        let duels = board(at(&people, 4)).duels;
        assert_ne!(duels[0].arena, duels[1].arena);
        assert_eq!(duels.iter().map(|d| d.names.clone()).collect::<Vec<_>>(),
            [["Ada".to_string(), "Bo".to_string()], ["Cy".to_string(), "Di".to_string()]], "whoever challenged first, first");
        for k in 0..4 {
            let snap = at(&people, k).lobby().snapshot.unwrap();
            assert_eq!(snap.players.iter().filter(|p| p.net_id != 0).count(), 2, "{}: just the two duelists", names[k]);
        }
        // Ed watches one duel, then the next.
        assert!(at(&people, 4).watch(0));
        until(&people, "Ed watching", &|p| board(p[4].as_ref().unwrap()).people.iter().any(|q| q.id == ids[4] && q.place == Place::Watching && q.arena == duels[0].arena));
        assert!(at(&people, 4).lobby().spectating());
        assert!(at(&people, 4).watch(0));
        until(&people, "Ed watching the other", &|p| board(p[4].as_ref().unwrap()).people.iter().any(|q| q.id == ids[4] && q.arena == duels[1].arena));
        // Then takes on a bot.
        assert!(at(&people, 4).back_to_lobby());
        until(&people, "Ed back", &|p| mode(p[4].as_ref().unwrap()) == Some(SupportedMode::DuelLobby) && !p[4].as_ref().unwrap().lobby().spectating());
        assert!(at(&people, 4).challenge_bot("Rookie"));
        until(&people, "Ed's bot duel", &|p| board(p[4].as_ref().unwrap()).duels.len() == 3);
        let snap = at(&people, 4).lobby().snapshot.unwrap();
        assert!(snap.players.iter().any(|p| p.is_bot && p.name == "Rookie"), "the bot asked for");
        let status = handle.status.lock().unwrap().clone();
        assert_eq!((status.players, status.bots, status.map.as_str()), (6, 1, "Tenfold Tower - DUEL"));
        // Bo concedes: Ada wins, both back in the lobby.
        assert!(at(&people, 1).back_to_lobby());
        until(&people, "Ada and Bo back", &|p| (0..2).all(|k| mode(p[k].as_ref().unwrap()) == Some(SupportedMode::DuelLobby)));
        until(&people, "the record", &|p| board(p[0].as_ref().unwrap()).people.iter().any(|q| q.id == ids[0] && q.wins == 1 && q.losses == 0)
            && board(p[0].as_ref().unwrap()).people.iter().any(|q| q.id == ids[1] && q.losses == 1));
        assert!(at(&people, 0).lobby().snapshot.unwrap().feed.iter().any(|e| e.line().contains("Bo conceded to Ada")));
        // Di leaves the server: Cy wins and comes back.
        people[3].take().unwrap().leave();
        until(&people, "Cy back", &|p| mode(p[2].as_ref().unwrap()) == Some(SupportedMode::DuelLobby)
            && board(p[2].as_ref().unwrap()).people.iter().any(|q| q.id == ids[2] && q.wins == 1));
        // Ed's bot duel plays on, or (on a busy machine) the bot has already
        // beaten Ed, who stood still: it never went with the others. Ada's
        // board may still be a step behind Cy's.
        let ed_on = |b: &peakrunner_core::sim::duel::Board| b.duels.len() == 1 || b.people.iter().any(|q| q.id == ids[4] && q.losses == 1);
        until(&people, "Ada's board without Cy's duel", &|p| !board(p[0].as_ref().unwrap()).duels.iter().any(|d| d.names.contains(&"Cy".to_string())));
        let b = board(at(&people, 0));
        assert!(ed_on(&b), "Ed's bot duel: {b:?}");
        for c in people.into_iter().flatten() { c.leave(); }
        wait(|| handle.players.load(std::sync::atomic::Ordering::Relaxed) == 0);
    }

    /// A duel server's spectator watching a duel keeps their spectator's
    /// place: a newcomer can't take it meanwhile, and they come back to the
    /// lobby. (With the place taken, the return failed and the server
    /// dropped them as an unfinished handshake.)
    #[test]
    fn a_spectator_watching_a_duel_keeps_their_place() {
        use peakrunner_core::{map_catalog::SupportedMode, sim::duel::Place};
        let host = GameHost::bind("127.0.0.1:0", "Duel watch", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"duel"}]"#).unwrap().with_spectators(1).unwrap();
        let port = host.local_addr().port();
        let _handle = host.spawn();
        let ada = connect_private("127.0.0.1", port, "Ada", "").unwrap();
        let eve = connect_as("127.0.0.1", port, "Eve", "", true).unwrap();
        let mut seq = 0;
        // Wait for `done`, keeping the sessions alive.
        let mut until = |sessions: &[&peakrunner_net::Session], what: &str, done: &dyn Fn() -> bool| {
            let start = Instant::now();
            while !done() {
                assert!(start.elapsed() < Duration::from_secs(6), "timed out waiting for {what}");
                seq += 1;
                for c in sessions { c.send_input(Command { seq, ..Command::default() }); }
                for c in [&ada, &eve] { assert!(c.lobby().error.is_none(), "{:?}", c.lobby().error); }
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let board = |c: &peakrunner_net::Session| c.lobby().duels.unwrap_or_default();
        until(&[&ada, &eve], "both in the lobby", &|| board(&ada).people.len() == 2 && eve.lobby().spectating());
        let eve_id = eve.lobby().player_id;
        assert!(ada.challenge_bot("Rookie"));
        until(&[&ada, &eve], "Ada's bot duel", &|| board(&eve).duels.len() == 1);
        assert!(eve.watch(0));
        until(&[&ada, &eve], "Eve watching", &|| board(&eve).people.iter().any(|q| q.id == eve_id && q.place == Place::Watching && q.arena != 0));
        // The server's one spectator place is Eve's.
        let late = connect_as("127.0.0.1", port, "Late", "", true).unwrap();
        until(&[&ada, &eve, &late], "Late turned away", &|| late.lobby().error.is_some());
        assert!(late.lobby().error.unwrap().contains("no room"));
        assert!(eve.back_to_lobby());
        until(&[&ada, &eve], "Eve back", &|| eve.lobby().snapshot.is_some_and(|s| s.mode == SupportedMode::DuelLobby && s.spectators.iter().any(|w| w.net_id == eve_id)));
        // Past the handshake's 3 s, still here.
        let back = Instant::now();
        until(&[&ada, &eve], "a while back", &|| back.elapsed() > Duration::from_millis(3500));
        assert!(eve.lobby().connected && eve.lobby().spectating());
        for c in [ada, eve] { c.leave(); }
    }

    #[test]
    fn a_bot_server_lists_its_bots_as_players_and_still_takes_people() {
        let host = GameHost::bind("127.0.0.1:0", "Bot count", 4, "Valley").unwrap().with_bots(4);
        let port = host.local_addr().port();
        let handle = host.spawn();
        let status = || handle.status.lock().unwrap().clone();
        // Bots alone: every place is taken, every player a bot.
        wait(|| status().players == 4);
        let s = status();
        assert_eq!((s.players, s.max_players, s.bots, s.roster.len()), (4, 4, 4, 4));
        assert!(s.roster.iter().all(|p| p.bot));
        // A person takes a bot's place: still four players, one of them a person.
        let a = connect_private("127.0.0.1", port, "Alpha", "").unwrap();
        wait(|| status().bots == 3);
        let s = status();
        assert_eq!(s.players, 4);
        assert_eq!(s.roster.iter().filter(|p| !p.bot).map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Alpha"]);
        assert_eq!(handle.players.load(std::sync::atomic::Ordering::Relaxed), 1);
        a.leave();
        wait(|| status().bots == 4);
    }

    #[test]
    fn a_location_must_be_plain_short_text() {
        let host = || GameHost::bind("127.0.0.1:0", "Loc", 8, "Valley").unwrap();
        assert!(host().with_location("New York, US").is_ok());
        assert!(host().with_location("").is_ok());
        assert!(host().with_location("<script>").is_err());
        assert!(host().with_location(&"x".repeat(41)).is_err());
    }

    #[test]
    fn server_rotation_selects_initial_map_for_real_clients() {
        let host = GameHost::bind("127.0.0.1:0", "Rotation test", 8, "Valley").unwrap()
            .with_rotation(r#"[{"map":"cairnhold","mode":"ctf"},{"map":"old-holler","mode":"ctf"}]"#).unwrap()
            .with_location(" Dallas, US ").unwrap();
        let port = host.local_addr().port();
        let host = host.spawn();
        let a = connect_private("127.0.0.1", port, "Alpha", "").unwrap();
        let b = connect_private("127.0.0.1", port, "Beta", "").unwrap();
        wait(|| a.lobby().snapshot.is_some() && b.lobby().snapshot.is_some());
        for client in [&a, &b] {
            assert_eq!(client.lobby().snapshot.unwrap().map, peakrunner_core::terrain::MapId::Cairnhold);
        }
        wait(|| host.status.lock().unwrap().map == "Cairnhold - CTF");
        assert_eq!(host.status.lock().unwrap().location, "Dallas, US");
        a.leave(); b.leave();
        wait(|| host.players.load(std::sync::atomic::Ordering::Relaxed) == 0);
    }

    #[test]
    fn team_chat_never_reaches_the_opposing_connection() {
        let host = GameHost::bind("127.0.0.1:0", "Team chat test", 8, "Valley").unwrap();
        let port = host.local_addr().port(); let _host = host.spawn();
        let a = connect("127.0.0.1",port,"Alice").unwrap();
        wait(||a.lobby().connected);
        let enemy = connect("127.0.0.1",port,"Enemy").unwrap();
        wait(||enemy.lobby().connected);
        let friend = connect("127.0.0.1",port,"Friend").unwrap();
        wait(||friend.lobby().connected);
        assert!(a.send_team_chat("Secret route".into()));
        wait(||friend.lobby().snapshot.as_ref().is_some_and(|s|s.feed.iter().any(|e|e.line().contains("Secret route"))));
        let tick = friend.lobby().snapshot.unwrap().tick;
        wait(||enemy.lobby().snapshot.as_ref().is_some_and(|s|s.tick >= tick));
        assert!(!enemy.lobby().snapshot.unwrap().feed.iter().any(|e|e.line().contains("Secret route")));
    }

    #[test]
    fn lan_spectators_are_capped_switch_teams_and_join_through_the_client_session() {
        use peakrunner_core::sim::Team;
        let host = GameHost::bind("127.0.0.1:0", "LAN teams", 8, "Valley").unwrap().with_spectators(1).unwrap();
        let port = host.local_addr().port(); let _host = host.spawn();
        let a = connect("127.0.0.1", port, "Alpha").unwrap();
        wait(|| a.lobby().connected);
        let s = connect_as("127.0.0.1", port, "Watcher", "", true).unwrap();
        wait(|| s.lobby().connected);
        assert!(s.lobby().spectating() && !a.lobby().spectating());
        let extra = connect_as("127.0.0.1", port, "Extra", "", true).unwrap();
        wait(|| extra.lobby().error.is_some());
        assert!(extra.lobby().error.unwrap().contains("spectators"));
        // Alone, Alpha may switch (0 v 1 is within one).
        let team = |c: &peakrunner_net::Session, id: u32| c.lobby().snapshot.and_then(|x| x.players.iter().find(|p| p.net_id == id).map(|p| p.team));
        let id = a.lobby().player_id;
        assert_eq!(team(&a, id), Some(Team::Ember));
        assert!(a.switch_team());
        wait(|| team(&a, id) == Some(Team::Glacier));
        // Refusals arrive as notices, not disconnects.
        assert!(s.vote(true) && s.call_vote(0));
        wait(|| s.lobby().notices >= 1);
        assert!(s.lobby().notice.unwrap().contains("vote") && s.lobby().error.is_none());
        // The spectator takes the free place on the smaller side.
        assert!(s.switch_team());
        let sid = s.lobby().player_id;
        wait(|| team(&s, sid) == Some(Team::Ember));
        assert!(!s.lobby().spectating());
    }

    #[test]
    fn players_who_join_are_counted_once_by_their_game_id() {
        let dir = std::env::temp_dir().join(format!("peakrunner-count-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let host = GameHost::bind("127.0.0.1:0", "Counted", 8, "Valley").unwrap().with_spectators(1).unwrap()
            .with_stats(Some(crate::stats::Stats::spawn(dir.clone(), "Counted").unwrap()));
        let port = host.local_addr().port();
        let _handle = host.spawn();
        let join = |player: u128, spectate: bool| peakrunner_net::connect_with("127.0.0.1", port, "Skier", "",
            peakrunner_net::JoinOptions { spectate, auto_download: 0, player }).unwrap();
        let counts = || -> Option<crate::stats::Counts> { serde_json::from_slice(&std::fs::read(dir.join("public/stats.json")).ok()?).ok() };
        let ann = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
        // The same game twice, an older game that sends none, and a game
        // that only watches: one person played.
        let (a, b, c) = (join(ann, false), join(ann, false), join(0, false));
        let w = join(0xfedc_ba98_7654_3210_fedc_ba98_7654_3210, true);
        wait(|| [&a, &b, &c, &w].iter().all(|s| s.lobby().connected));
        wait(|| counts().is_some_and(|n| n.total == 1));
        std::thread::sleep(Duration::from_millis(300));
        let n = counts().unwrap();
        assert_eq!((n.total, n.today), (1, 1), "{n:?}");
        // The watcher takes a place and plays: now two.
        assert!(w.switch_team());
        wait(|| counts().is_some_and(|n| n.total == 2));
        drop((a, b, c, w));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn mismatched_map_content_is_rejected_before_a_slot_is_assigned() {
        use std::io::{BufRead,BufReader};
        let host=GameHost::bind("127.0.0.1:0","Content check",8,"old-holler").unwrap();
        let addr=host.local_addr();let handle=host.spawn();
        let mut stream=TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        // The right game, but not the server's maps (a different Old Holler).
        let hello=proto::ClientMsg::Hello {name:"Wrong content".into(),password:String::new(),
            protocol:proto::game_protocol(),spectate:false,content:"0".repeat(64),player:String::new()};
        let mut bytes=serde_json::to_vec(&hello).unwrap();bytes.push(b'\n');stream.write_all(&bytes).unwrap();
        let mut reply=String::new();BufReader::new(stream).read_line(&mut reply).unwrap();
        assert!(matches!(serde_json::from_str::<proto::ServerMsg>(&reply).unwrap(),
            proto::ServerMsg::Reject {message} if message == crate::gameserver::CONTENT_MISMATCH));
        assert_eq!(handle.players.load(std::sync::atomic::Ordering::Relaxed),0);
    }
    #[test]
    fn two_clients_share_identity_map_airborne_movement_and_disconnect() {
        let dir = serve_directory("127.0.0.1:0").unwrap();
        let host = GameHost::bind("127.0.0.1:0", "North Spine", 8, "Valley").unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        let lease = directory::register(&dir.addr.to_string(), "North Spine", "127.0.0.1",
            port, 0, 8, "Valley").unwrap();
        assert!(!lease.token.is_empty());
        assert_eq!(browse(&dir.addr.to_string()).unwrap()[0].port, port);
        // Identical display names must not alias identity or hide either player.
        let a = connect("127.0.0.1", port, "Skier").unwrap();
        let b = connect("127.0.0.1", port, "Skier").unwrap();
        wait(|| a.lobby().connected && b.lobby().connected && a.lobby().players.len() == 2);
        let aid = a.lobby().player_id;
        assert!(a.rename("  Pilot 42  ".into()));
        wait(|| b.lobby().players.contains(&"Pilot 42".to_string()));
        assert!(a.rename("<script>".into()));
        assert_eq!(a.lobby().player_id, aid);
        assert_ne!(aid, b.lobby().player_id);
        assert_eq!(a.lobby().map, "Valley");
        assert!(a.send_chat("Ready to play".into()));
        wait(|| b.lobby().snapshot.as_ref().is_some_and(|s| s.feed.iter().any(|e|
            matches!(e, peakrunner_core::feed::Entry::Chat { sender, text } if sender == "Pilot 42" && text == "Ready to play"))));
        // The round counts down ROUND_COUNTDOWN seconds before play.
        let mut seq = 0;
        let started = Instant::now();
        while a.lobby().snapshot.as_ref().is_none_or(|s| s.phase != Phase::Playing) {
            assert!(started.elapsed().as_secs_f32() < peakrunner_core::sim::ROUND_COUNTDOWN + 4.0, "play never began");
            seq += 1;
            assert!(a.send_input(Command { seq, ..Command::default() }));
            assert!(b.send_input(Command { seq, ..Command::default() }));
            std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
        }
        let before = a.lobby().snapshot.unwrap().players.iter().find(|p| p.net_id == aid).cloned().unwrap();
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(3600) {
            seq += 1;
            assert!(a.send_input(Command { seq, jet: true, move_x: 1.0, ..Command::default() }));
            assert!(b.send_input(Command { seq, ..Command::default() }));
            std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0));
        }
        wait(|| {
            let al = a.lobby(); let bl = b.lobby();
            assert!(al.error.is_none(), "{:?}", al.error);
            assert!(bl.error.is_none(), "{:?}", bl.error);
            let (Some(sa), Some(sb)) = (al.snapshot, bl.snapshot) else { return false; };
            if sa.tick != sb.tick { return false; }
            assert_eq!(sa.phase, Phase::Playing);
            assert_eq!(serde_json::to_string(&*sa).unwrap(), serde_json::to_string(&*sb).unwrap());
            let p = sa.players.iter().find(|p| p.net_id == aid).unwrap();
            assert!(p.pos.y > before.pos.y + 1.0, "airborne Y must be authoritative");
            assert!(p.energy < before.energy);
            assert!(p.pos.x > before.pos.x);
            true
        });
        a.leave();
        wait(|| b.lobby().players.len() == 1);
    }
    #[test]
    fn eight_real_slots_and_password_gate() {
        let host = GameHost::bind("127.0.0.1:0", "Test", 8, "Valley").unwrap().with_password("invite".into())
            .with_spectators(1).unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        let wrong = connect_private("127.0.0.1", port, "Wrong", "no").unwrap();
        wait(|| wrong.lobby().error.is_some());
        assert!(!wrong.lobby().connected);
        let mut clients = Vec::new();
        for i in 0..8 { clients.push(connect_private("127.0.0.1", port, &format!("P{i}"), "invite").unwrap()); }
        wait(|| clients.iter().all(|c| c.lobby().players.len() == 8));
        // A ninth is seated as a waiting spectator and told why.
        let ninth = connect_private("127.0.0.1", port, "Ninth", "invite").unwrap();
        wait(|| ninth.lobby().connected && ninth.lobby().notice.is_some());
        assert!(ninth.lobby().spectating() && ninth.lobby().notice.unwrap().contains("Match is full"));
        let id = ninth.lobby().player_id;
        assert!(ninth.lobby().snapshot.unwrap().spectators.iter().any(|s| s.net_id == id && s.queued));
        // With the spectator places full too, a tenth is refused.
        let tenth = connect_private("127.0.0.1", port, "Tenth", "invite").unwrap();
        wait(|| tenth.lobby().error.is_some());
        assert!(tenth.lobby().error.unwrap().contains("full") && !tenth.lobby().connected);
        let snapshot = clients[0].lobby().snapshot.unwrap();
        assert_eq!(snapshot.players.iter().filter(|p| p.net_id != 0 && p.team == peakrunner_core::sim::Team::Ember).count(), 4);
        // A player leaves: the waiting spectator takes the slot, with a notice.
        clients.remove(0).leave();
        wait(|| ninth.lobby().snapshot.is_some_and(|s| s.players.iter().any(|p| p.net_id == id)));
        wait(|| ninth.lobby().notice.is_some_and(|n| n.contains("slot opened")));
        assert!(!ninth.lobby().spectating() && ninth.lobby().error.is_none());
    }
    #[test]
    fn a_called_vote_that_passes_switches_the_map_and_mode() {
        let host = GameHost::bind("127.0.0.1:0", "Vote", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"ctf"},{"map":"reefbreak","mode":"team_deathmatch"}]"#).unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        let a = connect("127.0.0.1", port, "Caller").unwrap();
        let b = connect("127.0.0.1", port, "Voter").unwrap();
        wait(|| [&a, &b].iter().all(|c| c.lobby().snapshot.is_some_and(|s| s.players.iter().filter(|p| p.net_id != 0).count() == 2)));
        let s = a.lobby().snapshot.unwrap();
        assert_eq!(s.vote_choices.len(), 2, "the rotation's entries are offered");
        let reef = s.vote_choices.iter().position(|e| e.0 == peakrunner_core::terrain::MapId::Reefbreak).unwrap();
        // Keep both sessions alive with inputs while the vote runs.
        let mut seq = 0;
        let mut pump = |a: &peakrunner_net::Session, b: &peakrunner_net::Session| { seq += 1;
            for c in [a, b] { c.send_input(Command { seq, ..Command::default() }); } std::thread::sleep(Duration::from_millis(16)); };
        assert!(a.call_vote(reef as u16));
        for _ in 0..30 { pump(&a, &b); }
        let v = b.lobby().snapshot.unwrap().vote.clone().expect("a vote is running");
        assert_eq!((v.yes, v.passed), (1, None), "1 of 2 isn't a majority");
        assert!(b.vote(true));
        for _ in 0..60 {
            pump(&a, &b);
            if b.lobby().snapshot.is_some_and(|s| s.map == peakrunner_core::terrain::MapId::Reefbreak) { break; }
        }
        let s = b.lobby().snapshot.unwrap();
        assert_eq!((s.map, s.mode), (peakrunner_core::terrain::MapId::Reefbreak, peakrunner_core::map_catalog::SupportedMode::TeamDeathmatch));
        assert_eq!(s.vote.as_ref().and_then(|v| v.passed), Some(true), "the result shows after the switch");
    }

    #[test]
    fn delayed_input_burst_is_bounded_and_does_not_replay_stale_ticks() {
        let host = GameHost::bind("127.0.0.1:0", "Burst", 8, "Valley").unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        let client = connect("127.0.0.1", port, "Burst").unwrap();
        wait(|| client.lobby().connected);
        let start = client.lobby().snapshot.unwrap().tick;
        for seq in 1..=16 {
            assert!(client.send_input(peakrunner_core::sim::Command { seq, move_z: 1.0, ..Default::default() }));
        }
        wait(|| client.lobby().snapshot.as_ref().is_some_and(|s| s.acks.iter().any(|ack| *ack == 16)));
        let lobby = client.lobby();
        assert!(lobby.connected && lobby.error.is_none());
        assert!(lobby.snapshot.unwrap().tick - start < 16, "stale burst replayed as a sixteen-tick backlog");
    }
    #[test]
    fn forged_outcomes_oversize_and_flood_do_not_break_server() {
        let host = GameHost::bind("127.0.0.1:0", "Test", 8, "Valley").unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        for payload in [b"{\"op\":\"pose\",\"x\":999999}\n".to_vec(), vec![b'x'; 16384],
            b"{\"op\":\"hello\",\"name\":\"Spam\",\"protocol\":\"peakrunner-2\",\"password\":\"\"}\n".repeat(40)] {
            let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let _ = socket.write_all(&payload);
        }
        let good = connect("127.0.0.1", port, "Good").unwrap();
        wait(|| good.lobby().connected);
        assert_eq!(good.lobby().players, vec!["Good"]);
        assert!(GameHost::bind("0.0.0.0:0", "Public", 8, "Valley").is_err());
    }
    #[test]
    fn framing_preserves_partial_messages_and_bounds_lines() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let mut reader = wire::Framed::new(socket, 64).unwrap();
        sender.write_all(b"{\"op\":").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert!(reader.receive::<proto::ClientMsg>().unwrap().is_empty());
        sender.write_all(b"\"leave\"}\n").unwrap();
        wait(|| !reader.receive::<proto::ClientMsg>().unwrap().is_empty());
        sender.write_all(&[b'x'; 65]).unwrap();
        wait(|| reader.receive::<proto::ClientMsg>().is_err());
    }

    #[test]
    #[ignore = "12-second real-socket load test"]
    fn eight_clients_sustain_movement_and_all_weapons() { load_test(8); }

    #[test]
    #[ignore = "load test: 28 local clients for 12 s"]
    fn twenty_eight_clients_sustain_movement_and_all_weapons() { load_test(28); }

    fn load_test(n: usize) {
        // PEAKRUNNER_LOAD_MAP picks another map key, e.g. for collision-budget checks.
        let map = std::env::var("PEAKRUNNER_LOAD_MAP").unwrap_or_else(|_| "old-holler".into());
        let host = GameHost::bind("127.0.0.1:0", "Load test", n as u32, &map).unwrap();
        let port = host.local_addr().port();
        let _host = host.spawn();
        let clients: Vec<_> = (0..n).map(|i| connect("127.0.0.1", port, &format!("P{i}")).unwrap()).collect();
        wait(|| clients.iter().all(|c| c.lobby().connected));
        let start = Instant::now();
        let mut next = start;
        let mut max_bytes = 0;
        // Twelve seconds of play after the round's countdown.
        let countdown = (peakrunner_core::sim::ROUND_COUNTDOWN * 60.0) as u64;
        for seq in 1..=720 + countdown {
            for (i, c) in clients.iter().enumerate() {
                if seq % 90 == 1 { assert!(c.send_chat(format!("Match comms test {i}: {}", "abcdefghij".repeat(12)))); }
                assert!(c.send_input(Command { seq, move_z: 1.0, move_x: (i as f32 * 0.5).sin(),
                    yaw: (seq as f32 * 0.003 + i as f32).rem_euclid(std::f32::consts::TAU),
                    jump: true, jet: seq % 240 < 120, fire: true, weapon: ((seq / 120) % 3) as u8,
                    ..Command::default() }));
                let lobby = c.lobby();
                assert!(lobby.error.is_none(), "client {i}: {:?}", lobby.error);
                assert!(lobby.connected);
                let s = lobby.snapshot.unwrap();
                max_bytes = max_bytes.max(serde_json::to_vec(&*s).unwrap().len());
                assert!(s.players.iter().all(|p| p.pos.is_finite() && p.vel.is_finite()));
            }
            next += Duration::from_secs_f64(1.0 / 60.0);
            if next > Instant::now() { std::thread::sleep(next - Instant::now()); }
        }
        let snapshot = clients[0].lobby().snapshot.unwrap();
        assert_eq!(snapshot.players.iter().filter(|p| p.net_id != 0).count(), n);
        // Every client kept firing, except while dead: turrets and splash kill
        // some players, and each death costs the respawn wait.
        assert!(snapshot.players.iter().filter(|p| p.net_id != 0).all(|p| p.shots > 10 || (p.losses > 0 && p.shots > 0)),
            "shots, deaths: {:?}", snapshot.players.iter().filter(|p| p.net_id != 0).map(|p| (p.shots, p.losses)).collect::<Vec<_>>());
        assert!(snapshot.tick >= 680 + countdown, "server failed to sustain close to 60 Hz: {}", snapshot.tick);
        println!("{n}-client load test: {} ticks, {:.2}s, largest snapshot {} bytes", snapshot.tick, start.elapsed().as_secs_f32(), max_bytes);
    }
}
