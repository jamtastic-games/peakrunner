//! Gameplay wire protocol shared only by client and match server.
use serde::{Deserialize, Serialize};
use peakrunner_core::sim::{Command, Snapshot};
pub use peakrunner_discovery::{PROTOCOL, ServerAdvert};
pub mod packets;
pub mod delta;
pub const GAME_VERSION: &str = "0.1.0-playtest.20261016.3";
/// A host key (`ClientMsg::StartMatch`): 32 random bytes as lowercase hex.
pub const HOST_KEY_LEN: usize = 64;
/// The line a hosted server (`PEAKRUNNER_HOST_KEY`) prints on stdout once it
/// listens, followed by its key's SHA-256 fingerprint (hex): the client that
/// started it joins pinned to that key.
pub const HOST_READY: &str = "PeakRunner server ready for its host";

/// A different local map must never silently join a server simulating another
/// layout. Keep directory discovery independent of gameplay/map assets.
pub fn game_protocol() -> String {
    // walls1: careful collision on maps with blade zones (sim.rs
    //         `careful_collision`): a walking body pushed or snapped toward a
    //         face it touches is set off it, and never ends a move inside one
    //         it started clear of (`MapPack::settle`, `unslip`, `wall_overlap`);
    //         Tenfold's rebuilt cavern rocks, ruins and barrels.
    // blade1: maps' blade zones (`features::ZoneKind::Blade`, round zones
    //         and their spawns): a blade in every hand, no jets, stamina from
    //         the energy tank (sim::blade); snapshots carry each player's blade
    //         (`Player::blade`, the player codec's thirteenth group); skim
    //         surfaces bear only bodies near them (`water::skim_surface_at`).
    // duel1: duel servers (crates/server/src/duels.rs): a lobby and an arena
    //        per accepted challenge in one server, the board
    //        (ServerMsg::Duels) and the challenge messages; base modes
    //        `duel` and `duel_lobby`.
    // modes1: game modes are data (a rule set plus a definition, crate::modes)
    //         and travel in snapshots and votes as their key ("ctf"); a server's
    //         mods (definitions and modes) are content its players download,
    //         and the session plays by exactly the server's definitions.
    // content1: maps are no longer part of this string. A server lists the
    //           maps its rotation needs in its status (`MatchStatus::content`);
    //           a client fetches what it lacks (ClientMsg::Fetch, after the
    //           player approves) and proves in Hello which it holds
    //           (`content_digest`), so map updates need no new release.
    // maps13: maps are folders the engine's catalog finds (packs/ beside
    //         the executable) instead of being built in, and a map travels in
    //         snapshots and votes as its key ("old-holler"), not an index.
    // host1: a hosted server's host starts the match with one team empty
    //        (ClientMsg::StartMatch with the key the host gave the server; Match::host_start).
    // bounce1: the football, grenades, mines and unarmed shells bounce off the
    //          surface their sweep hit (sim::surface_hit) or the deployable
    //          face struck: the ball went through walls at a glancing angle, and
    //          grenades stuck to deployed walls in mid-air.
    // flag1: flags are touched by the whole body against the whole pole (to its
    //        finial or the first ceiling), swept across each step (sim::touches_flag).
    // maps12: overlapping coplanar render faces separated at build time
    //         (pack_writer.resolve_overlaps) and Old Holler's bunker ramps meet the
    //         ground: new Cairnhold, Dustreach, Frostline, Ozarktic Blast, Tower
    //         Complex and Old Holler packs.
    // maps11: map keys renamed (old-holler, tower-complex, cairnhold, frostline,
    //         dustreach), with matching map names on the JSON transport and new
    //         Old Holler and Tower Complex manifests.
    // maps10: Reefbreak joins the CTF maps.
    // maps9: Ozarktic Blast joins the CTF maps.
    // maps8: Highgoal, the raised-goal Football arena, joins Longfield.
    // maps7: Longfield, the Football stadium, joins the five CTF maps.
    // maps6: every rotation map is embedded (Old Holler, Tower Complex,
    // Cairnhold, Frostline and Dustreach).
    // muzzle1: player shots are clamped to the shooter's side of walls.
    // muzzle2: every weapon with a model fires from that model's barrel end
    //          (registry::Weapon::muzzle, sim::weapon_muzzle), not its view's
    //          shared muzzle.
    // muzzle3: the view models sit higher, nearer and more to the middle
    //          (sim::vm_anchor: the hand on the grip in view, the mortar
    //          lower), so every shot leaves from a new muzzle, and the disc
    //          launcher's and chaingun's barrels turn to match.
    // equipment5: generator-powered shields regenerate continuously, destroyed
    // equipment stays offline until repaired past half hull (sent in
    // snapshots), and generators explode visibly. kit1: repair kits (now the repair tool, loadout1).
    // cnh1: control points (state in snapshots, drain field in the shared
    // sim) and the Capture & Hold mode.
    // arc1: fixed turrets face the field with a limited field of fire beyond
    // 15 m (equipment::Definition::in_arc), so open doorways need no baffles.
    // water1: manifest water volumes slow movement and projectiles in the
    // shared sim (crate::water).
    // bump1: enemies collide and exchange momentum, with light damage
    // (sim::bump). ball1: the Football mode and its ball in snapshots.
    // ball2: Football players move by Football's own rules
    // (sim::football::classic_move) and carriers wear the slower armor.
    // rabbit1: the Rabbit and Team Rabbit modes (rules `rabbit`,
    // `team_rabbit`), their neutral flag in snapshots (`Snapshot::rabbit`,
    // the delta codec's fifth fixed part) and its grab, held and death events.
    // rabbit2: the classic Rabbit: hunters one side (World::foe), points per
    // 15 s held over all holds, the rabbit's kills double, pings that show
    // the rabbit (RabbitFlag::ping, ::times in snapshots), a mode's `banned`.
    // packs1: a pack's `drain_regen` (energy a second as a multiple of the
    // wearer's recharge; the shield 1.125, the jammer 1.3125), and shields
    // priced by the hit before armor (`absorb` 0.5 on the base shield).
    // players1: Hello carries the game's anonymous ID (`player`), counted
    // by servers with a stats folder (peakrunner-server stats.rs).
    // ball6 (2026-10-15): a dropped ball is never intercepted, only picked
    // up (`Ball::dropped`, in snapshots), and games announce a FUMBLE when
    // `Ball::fumbles` changes, not who came up with it. Catches reach 3%
    // past the body (`football::CATCH_GROW`). Football commands always claim
    // the snapshot their drawing coasted the others on from and how far
    // (`Command::view`, new `Command::ahead`; clients draw others ahead,
    // `football::coast`), and a person's body check hits a bot where it was
    // drawn (`bump::seen_hit`, the laser's `lagcomp` history); people are hit
    // where they are. Bot carriers read defenders a quarter second late,
    // only in front, with a 2.5 s memory (`football_bots::fb_watch`).
    // ball5 (2026-10-14): a pass in the air names who it goes to
    // (`Ball::pass_to`, in snapshots), whose game warns them with a sound
    // and a glow at the screen's edge on the ball's side.
    // ball4 (2026-10-14): nobody holds a ball-thrower; a dead player's body
    // stays down BODY_LIES in the ball's state (`Ball::bodies`, in place of
    // `throwers`) and fumbles a carrier who touches it; a catch is swept
    // along the step's flight leg by leg and takes the speed of the leg that
    // reached the catcher (`pickup_along`).
    // ball3: Football by the classic rules and engine: a pass's elevation
    // `pitch + 0.5 cos(pitch) - 0.25` from the body's middle, power from any
    // player under the crosshair or in the yaw slice, a 1 s self-catch, a
    // catch pushed along the throw's heading, the engine's ball bounce and
    // friction, dropped throwers in the ball's state (`Ball::throwers`), an
    // 88-degree look (`Command::valid` allows it), and the teams swapping
    // ends at half time (clients place the end zones by `Ball::half`).
    // kill1: the Ctrl+K suicide intent in commands.
    // loadout1: light and heavy armor, limited ammo, the mortar, the repair
    // tool (was the kit), inventory purchases and deployables (sim::loadout,
    // sim::deploy).
    // throw1: hand grenades and mines thrown with G and M (sim::throwables).
    // throw2: a throw keeps all of the thrower's velocity and leaves the hand 1 m ahead.
    // water2: skim water volumes are ground for players, with no immersion (crate::water).
    // water3: discs burst where they meet water, at half damage and twice the
    // push (registry `Water::Burst`, water::BURST_DAMAGE, BURST_PUSH).
    // ff1: any team's fire wears down deployables and sets off resting mines (clearing your own);
    //      your own explosives hurt you in full (discs 40%).
    // p28: snapshots carry 28 player slots (14 a side); a server's capacity is PEAKRUNNER_MAX_PLAYERS.
    // vote1: map/mode votes (ClientMsg::CallVote/Vote, Snapshot vote and vote_choices).
    // foot1: released movement keys brake along the ground on walkable slopes (no icy stops).
    // loot2: death packs roll 1-5 discs and third-weapon rounds, all chaingun rounds and 1+ hand grenades; pickers take only matching ammo (sim::loadout).
    // dm1: Deathmatch and Team Deathmatch, with per-round conditions in snapshots (sim::deathmatch).
    // rifle1: the rifle slot bought at stations: laser (light) and railgun (heavy) (sim::rifles).
    // arc2: outside the field of fire, turrets still engage anyone under open sky.
    // blast4: the mortar and grenade launcher use linear splash against players
    //         (152 over 20 m, 73 over 15 m); the laser rifle does 60 (combat, rifles).
    // lagcomp1: commands holding fire carry the drawn view time (Command::view); laser
    //           hits are tested against players rewound to it, at most 250 ms (sim::lagcomp).
    // teams1: team switches and spectators (ClientMsg::SwitchTeam/Spectate, Hello spectate,
    //         ServerMsg::Notice, Snapshot spectators; sim::teams).
    // delta1: quantized snapshots sent as deltas against the newest one the client
    //         acknowledged in its input datagrams, sized to the path MTU (delta, packets).
    // arsenal1: weapon slots per armor chosen at stations (light 3, heavy 4), the plasma
    //           gun and blaster, field inventories, saved favorites (Command::equip) and
    //           loadouts in snapshots (sim::arsenal); stations open only from their front
    //           (map entity `facing`, loadout::station_front).
    // toss1: Ctrl+F tosses the carried flag (Command::toss, Flag::vel; World::toss_flag).
    // classes1: the engine's classes (crate::registry): armors, weapons, projectiles and
    //           deployables travel as their entries' wire numbers (a player's armor, weapon,
    //           rounds per weapon, loadout and pack), purchases as BUY_ARMOR/TAKE/DROP/PACK
    //           plus a number (Command::buy is u16), favorites as an armor, weapons and pack
    //           by number (loadout::Equip), death packs as rounds per weapon; capture
    //           points carry their turret's aim (control::Point), and snapshots the
    //           viewer's team's marks (Snapshot::marked).
    // marks1: the target marker is a held laser (a marking stream keeps one
    //        mark per holder on what it touches, `firing::stream_tick`), and
    //        snapshots carry the viewer's team's marked spots (Snapshot::marked_spots).
    // gravity1: maps may have gravity zones (`features::ZoneKind::Gravity`)
    //           that scale how players fall inside, applied in movement on
    //           both sides.
    // rocket1: a map's fixed turrets may fire rockets (`"weapon": "rocket"`,
    //          `equipment::TurretWeapon::Rocket`): the library rocket turret's
    //          heat-seeking rounds, homing on the jetting enemy they were fired at.
    // lava1: maps may have lava (water volumes with `lava`: a surface that
    //        burns, `water::lava_burn`) and vents that erupt by themselves
    //        (`features::Vent`), throwing lava bombs the server owns as the
    //        map (`features::MAP_OWNER`, hostile to everyone).
    // burn1: a projectile may burn whoever it touches while it flies
    //        (`Projectile::burn`, a vent's fireball). It replaces flood1, the
    //        crater flood of the removed Cinderspire (never released).
    // vehicles2: ground vehicles (registry::VehicleDrive::Ground, predicted for the
    //            driver), exposed seats whose riders shots and blasts reach, vehicles
    //            bumping each other and running over enemies on foot, and shots
    //            stopped by a vehicle's turned box instead of a sphere (sim::vehicles).
    // updraft1: maps may have updraft zones (`features::ZoneKind::Updraft`), pockets
    //           of hot air whose lift fades near their top, applied in movement on
    //           both sides.
    // lava2: maps may have lava ground (`lava.bin`, `map_pack::LavaCell`) that
    //        burns and may hold players up like a lava volume; shots burst on
    //        lava instead of sinking, and a shooter's burst there throws a
    //        splash that burns (`World::lava_splash`, `sim::LAVA_SPLASH`).
    // spires1: control points may have their own capture time (a post flips at a
    //          touch), spires whose drain reaches only players they see, a drain
    //          that spares players on the ground, and a beacon height
    //          (control::Definition `capture`, `spires`, `beacon`, `Drain::airborne`);
    //          snapshots carry them (control::Point).
    // spires2: a drain may hurt too (`Drain::damage`, energy damage on the server).
    // spires3: a point may burn its holders inside a box (control::Definition
    //          `cook`, fire damage on the server); snapshots carry it (control::Point).
    // steer1: the skier jet's push along the movement keys is paid out of its
    //         lift (`Armor::side` taken from `Armor::jet`), predicted on both sides.
    // repair1: the repair tool stays out while its key is held and rests
    //          `repair.rest` seconds after running the tank dry; snapshots
    //          carry `Player::repair_out` (the player flags' spare bit).
    // jumpjet1: jets pressed on the ground, skiing or not, kick the skier up
    //           with a share of the jump (`movement.jet_jump`) whatever their
    //           speed; snapshots carry the jets held last tick (player flag 6).
    // rail2: the railgun's slug flies at 600 m/s (was 300).
    // fall1: dead bodies fall with their momentum and come to rest on the
    //        ground (sim::fall_body); what a body drops rides it down
    //        (Loot::carried, items::step_carried_loot).
    // repair2: the repair tool working on its holder is a flag of its own
    //          (Player::repair_self, the player flags' eighth bit), drawn as
    //          a glow, not a stream.
    format!("{PROTOCOL}:equipment5:blast4:chat2:names1:ping1:fov1:muzzle3:kit1:cnh1:arc2:water1:bump1:ball6:kill1:loadout1:throw2:rifle1:dm1:loot2:water2:ff1:p28:vote1:foot1:lagcomp1:teams1:delta1:flag1:bounce1:host1:maps13:content1:modes1:arsenal1:toss1:classes1:marks1:gravity1:rocket1:lava1:burn1:vehicles2:updraft1:lava2:spires3:steer1:repair1:jumpjet1:rail2:fall1:repair2:players1:water3:rabbit1:rabbit2:packs1:duel1:blade1:walls1")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMsg {
    /// `spectate`: join watching, without a player slot (`teams1`).
    /// `content`: `content_digest` of the server's content list as this
    /// client resolved it from its own maps (`content1`).
    /// `player`: the game's anonymous ID (32 hex digits, made on first run),
    /// for counting how many people play (`players1`); empty from older
    /// games and tests.
    Hello { name: String, protocol: String, password: String, #[serde(default)] spectate: bool, #[serde(default)] content: String,
        #[serde(default)] player: String },
    Input { command: Command },
    Chat { text: String },
    TeamChat { text: String },
    Rename { name: String },
    Leave,
    Status,
    /// Call a vote for rotation entry `choice` (an index into the snapshot's `vote_choices`).
    CallVote { choice: u16 },
    /// Yes or no on the running vote.
    Vote { yes: bool },
    /// Play for the other team; from a spectator, join a team (`teams1`).
    SwitchTeam {},
    /// Stop playing and watch (true), or ask to play again (false).
    Spectate { on: bool },
    /// Start the match now, even with one team empty: only with the key the
    /// server was started with, by the client that hosts it (`host1`).
    StartMatch { key: String },
    /// A file of a map the server lists in its status content, by the map's
    /// fingerprint: `map.json`, `info.json` or a payload `map.json` lists.
    /// The first message of a download connection; each further stream asks
    /// for one more file (`content1`).
    Fetch { map: String, file: String },
    /// Duel servers (`duel1`): challenge the person with this ID in the lobby.
    Challenge { player: u32 },
    /// Duel a bot of this archetype (`duel::BOTS`, by name).
    ChallengeBot { archetype: String },
    /// Yes or no to a challenge from the person with this ID.
    Answer { player: u32, yes: bool },
    /// Take back the challenge you made.
    Withdraw {},
    /// Watch duel `arena` (0: the next one after the one watched).
    Watch { arena: u32 },
    /// Back to the lobby: concede the duel you play, or stop watching.
    BackToLobby {},
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome { player_id: u32, match_name: String },
    Snapshot { state: Snapshot },
    Reject { message: String },
    Status { status: peakrunner_discovery::MatchStatus },
    /// A short note that doesn't end the session, e.g. why a team switch
    /// was refused (`teams1`).
    Notice { message: String },
    /// The answer to `Fetch`: `bytes` raw bytes follow on the stream,
    /// zlib-compressed when `zlib` (as the server stores the file).
    File { bytes: u64, zlib: bool },
    /// A duel server's board for this player (`duel1`), sent when it changes.
    Duels { board: peakrunner_core::sim::duel::Board },
}

/// What a client proves it holds when it joins: the SHA-256 of the server's
/// content list (kind, key and fingerprint of each map and mod, in order).
pub fn content_digest(items: &[peakrunner_discovery::ContentItem]) -> String {
    let named: Vec<(String, &str)> = items.iter().map(|i| (format!("{}:{}", i.kind, i.key), i.fingerprint.as_str())).collect();
    peakrunner_core::catalog::content_digest(named.iter().map(|(k, f)| (k.as_str(), *f)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn equipment_shield_state_survives_the_wire() {
        let mut m=peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::OldHoller);
        assert!(!m.world.equipment.is_empty(),"Old Holler has equipment");
        m.world.equipment[0].shield=123.5;m.world.equipment[0].health=40.;
        let text=serde_json::to_string(&super::ServerMsg::Snapshot{state:m.snapshot()}).unwrap();
        let super::ServerMsg::Snapshot{state}=serde_json::from_str(&text).unwrap() else {panic!()};
        assert_eq!((state.equipment[0].shield,state.equipment[0].health),(123.5,40.));
        m.world.equipment[1].offline=true;
        let text=serde_json::to_string(&super::ServerMsg::Snapshot{state:m.snapshot()}).unwrap();
        let super::ServerMsg::Snapshot{state}=serde_json::from_str(&text).unwrap() else {panic!()};
        assert!(state.equipment[1].offline,"the offline state survives the wire");
    }
    #[test]
    fn loadout_intents_ammo_armor_pack_and_deployables_survive_the_wire() {
        use peakrunner_core::sim::{loadout, deploy, ArmorClass};
        // Older clients called the repair intent `kit`; it still parses.
        let old: peakrunner_core::sim::Command = serde_json::from_str(
            r#"{"seq":1,"move_x":0,"move_z":0,"yaw":0,"pitch":0,"jump":false,"jet":false,"fire":false,"kit":true,"weapon":0}"#).unwrap();
        assert!(old.repair);
        let command=peakrunner_core::sim::Command{seq:1,repair:true,buy:loadout::BUY_HEAVY,deploy:true,..Default::default()};
        let text=serde_json::to_string(&command).unwrap();
        let back: peakrunner_core::sim::Command = serde_json::from_str(&text).unwrap();
        assert!(back.repair && back.deploy && back.buy==loadout::BUY_HEAVY);
        let mut m=peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::OldHoller);
        let Some(slot)=m.join(7,"kit") else {panic!()};
        let v=m.world.players[slot].pos;
        let p=&mut m.world.players[slot];
        p.armor=ArmorClass::Heavy;p.ammo=vec![3,40,2,7,11];p.loadout=peakrunner_core::sim::arsenal::Loadout::of(&[5,3,4,0]);p.pack=Some(peakrunner_core::sim::packs::PackKind::Inventory);p.repair_beam=Some(v);p.repair_out=true;
        m.world.deployables.push(deploy::Deployable{owner:0,kind:deploy::DeployKind::Wall,team:peakrunner_core::sim::Team::Glacier,
            pos:v,yaw:0.5,health:320.,cooldown:0.,aim:v,age:0.0});
        let text=serde_json::to_string(&super::ServerMsg::Snapshot{state:m.snapshot()}).unwrap();
        let super::ServerMsg::Snapshot{state}=serde_json::from_str(&text).unwrap() else {panic!()};
        let p=&state.players[slot];
        assert_eq!(p.loadout.kinds().collect::<Vec<_>>(),[5,3,4,0]);assert_eq!((p.armor,p.ammo.clone(),p.pack,p.repair_beam,p.repair_out),(ArmorClass::Heavy,vec![3,40,2,7,11],Some(peakrunner_core::sim::packs::PackKind::Inventory),Some(v),true));
        assert_eq!(state.deployables,m.world.deployables);
    }

    #[test]
    fn a_shot_view_survives_the_wire_and_bad_ones_are_refused() {
        use peakrunner_core::sim::{lagcomp::MAX_VIEW, Command};
        let shot = Command { seq: 9, fire: true, view: 123_456_789, ahead: 96, ..Default::default() };
        let (decoded, ack) = super::packets::decode_inputs(&super::packets::inputs(&[shot].into_iter().collect(), 42)).unwrap();
        assert_eq!((decoded[0].view, decoded[0].ahead, ack), (123_456_789, 96, 42), "the view, how far ahead, and the snapshot ack share the input datagram");
        let far = Command { ahead: peakrunner_core::sim::lagcomp::MAX_AHEAD + 1, ..shot };
        assert!(super::packets::decode_inputs(&super::packets::inputs(&[far].into_iter().collect(), 42)).is_err());
        let back: Command = serde_json::from_str(&serde_json::to_string(&shot).unwrap()).unwrap();
        assert_eq!(back.view, 123_456_789);
        let bad = Command { view: MAX_VIEW + 1, ..shot };
        assert!(super::packets::decode_inputs(&super::packets::inputs(&[bad].into_iter().collect(), 42)).is_err());
        // Clients name a time, never a victim or a hit.
        assert!(serde_json::from_str::<Command>(
            r#"{"seq":1,"move_x":0,"move_z":0,"yaw":0,"pitch":0,"jump":false,"jet":false,"fire":true,"weapon":3,"view":16,"victim":2}"#).is_err());
        assert!(super::game_protocol().contains(":lagcomp1"));
    }

    #[test]
    fn control_points_and_mode_survive_the_wire() {
        use peakrunner_core::{control, map_catalog::SupportedMode};
        let mut m=peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::OldHoller);
        m.world.set_mode(SupportedMode::CaptureAndHold);
        m.world.set_control_points(vec![control::Definition{id:"beacon".into(),name:"Beacon".into(),pos:[1.,2.,3.],
            radius:12.,ctf_active:false,drain:Some(control::Drain{radius:60.,rate:10.,airborne:false,damage:0.}),turret:None,
            capture:None,spires:Vec::new(),beacon:None,cook:None}]);
        m.world.points[0].owner=Some(1);m.world.points[0].progress=0.4;m.world.points[0].capturing=Some(0);m.world.points[0].contested=true;
        m.world.score=[120,45];
        let text=serde_json::to_string(&super::ServerMsg::Snapshot{state:m.snapshot()}).unwrap();
        let super::ServerMsg::Snapshot{state}=serde_json::from_str(&text).unwrap() else {panic!()};
        assert_eq!(state.mode,SupportedMode::CaptureAndHold);
        assert_eq!(state.points,m.world.points);
        assert_eq!(state.score,[120,45]);
        assert!(super::game_protocol().contains(":cnh1"));
        assert!(super::game_protocol().contains(":arc2"));
        assert!(super::game_protocol().contains(":water1:bump1:ball6"));
    }
    #[test]
    fn team_requests_carry_no_identity_and_spectators_survive_the_wire() {
        use super::ClientMsg;
        let hello: ClientMsg = serde_json::from_str(r#"{"op":"hello","name":"A","protocol":"p","password":""}"#).unwrap();
        assert!(matches!(hello, ClientMsg::Hello { spectate: false, .. }), "spectate defaults off");
        assert!(matches!(serde_json::from_str(r#"{"op":"switch_team"}"#).unwrap(), ClientMsg::SwitchTeam {}));
        assert!(matches!(serde_json::from_str(r#"{"op":"spectate","on":true}"#).unwrap(), ClientMsg::Spectate { on: true }));
        assert!(matches!(serde_json::from_str(r#"{"op":"start_match","key":"k"}"#).unwrap(), ClientMsg::StartMatch { key } if key == "k"));
        for forged in [r#"{"op":"switch_team","team":"Glacier"}"#, r#"{"op":"switch_team","player_id":3}"#] {
            assert!(serde_json::from_str::<ClientMsg>(forged).is_err());
        }
        let mut m = peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::OldHoller);
        m.join(1, "Player").unwrap();
        m.join_spectator(2, "Watcher").unwrap();
        m.step(&[]);
        let mut state = m.snapshot();
        let (_, body) = super::packets::prepare_fitting(&mut state).unwrap();
        let bytes = super::packets::fragments(state.tick, &super::packets::message(0, &body, &[]), super::packets::MIN_DATAGRAM).unwrap();
        let mut assembly = super::packets::Receiver::default();
        let back = bytes.iter().find_map(|b| assembly.accept(b).unwrap()).unwrap();
        assert_eq!(back.spectators.len(), 1);
        assert_eq!((back.spectators[0].net_id, back.spectators[0].name.as_str()), (2, "Watcher"));
        assert!(super::game_protocol().contains(":teams1"));
    }
    #[test]
    fn chat_cannot_supply_identity_or_frag_outcomes() {
        for text in [r#"{"op":"chat","text":"hello","sender":"Admin"}"#,
                     r#"{"op":"team_chat","text":"secret","team":"Glacier"}"#,
                     r#"{"op":"rename","name":"Admin","player_id":2}"#,
                     r#"{"op":"frag","killer":"Alice","victim":"Bob"}"#] {
            assert!(serde_json::from_str::<super::ClientMsg>(text).is_err());
        }
    }
}
