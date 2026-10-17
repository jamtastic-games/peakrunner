//! Authoritative match state and the shared movement predictor.
use super::*;
#[path = "teams.rs"]
pub mod teams;

pub const MAX_PLAYERS: usize = 28;
/// Server bots carry net IDs from here up (never given to connections).
pub const BOT_ID_BASE: u32 = 0xF000_0000;
/// Seconds every round counts down before play, on every map and mode: after
/// an intermission, a rotation or a vote's map switch, offline too. Players
/// wait at their spawns, frozen, with the clock and scores reset. (Football's
/// first start keeps `football::WARMUP_SECONDS` instead.)
pub const ROUND_COUNTDOWN: f32 = 10.0;

/// A connected person, not a server bot or an empty slot.
pub fn human(p: &Player) -> bool { p.net_id != 0 && !p.is_bot }

/// An input, never an outcome. No position, velocity, damage or client delta time.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub seq: u64,
    pub move_x: f32,
    pub move_z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub jump: bool,
    pub jet: bool,
    pub fire: bool,
    #[serde(default)]
    pub interact: bool,
    /// Repair tool held (Q). Older clients sent it as `kit`.
    #[serde(default, alias = "kit")]
    pub repair: bool,
    /// A purchase (`loadout::BUY_*`).
    #[serde(default)]
    pub buy: u16,
    #[serde(default)]
    pub deploy: bool,
    #[serde(default)]
    pub deploy_turn: f32,
    #[serde(default)]
    pub suicide: bool,
    /// Throw a grenade or mine (`throwables::THROW_*`) at this strength (0..1).
    #[serde(default)]
    pub throw: u8,
    #[serde(default)]
    pub throw_strength: f32,
    /// The weapon kind held (`arsenal`).
    pub weapon: u8,
    /// Equip a saved favorite at a station (`loadout::Equip`), once.
    /// (Always written: input datagrams are postcard, which can't skip fields.)
    #[serde(default)]
    pub equip: Option<crate::sim::loadout::Equip>,
    /// Toss the flag you carry (`World::toss_flag`), once.
    #[serde(default)]
    pub toss: bool,
    /// The fire-mode key: the held weapon's next mode (`firing`), once.
    #[serde(default)]
    pub mode: bool,
    /// The class key (`packs`), once.
    #[serde(default)]
    pub ability: bool,
    /// At a command station: the team turret operated (its deployable
    /// index + 1; 0 none), and a team waypoint set, once (`placed`).
    #[serde(default)]
    pub operate: u16,
    #[serde(default)]
    pub waypoint: Option<Vec3>,
    /// Drop the weapon in hand or the pack worn (`items::DROP_*`), once.
    #[serde(default)]
    pub drop: u8,
    /// Board the nearest vehicle or leave the one ridden (`vehicles`), once.
    #[serde(default)]
    pub board: bool,
    /// While fire is held: the server time of the other players the client
    /// was drawing, in sixteenths of a tick (`lagcomp`; 0: none); always in
    /// Football, there the snapshot its drawing coasted on from (`ahead`). A
    /// claim the server clamps, never a hit.
    #[serde(default)]
    pub view: u64,
    /// Football: how far past `view` the client drew the others, coasting
    /// them on (`football::coast`), in sixteenths of a tick (`lagcomp`).
    #[serde(default)]
    pub ahead: u16,
}

impl Command {
    pub fn valid(&self) -> bool {
        self.seq > 0 && self.seq < u64::MAX && self.move_x.is_finite()
            && self.move_z.is_finite() && self.yaw.is_finite() && self.pitch.is_finite()
            && self.move_x.abs() <= 1.0 && self.move_z.abs() <= 1.0
            && self.yaw.abs() <= 100_000.0 && self.pitch.abs() <= crate::sim::football::LOOK_LIMIT && (self.weapon as usize) < crate::registry::registry().weapons.len()
            && crate::sim::loadout::valid_buy(self.buy) && self.equip.is_none_or(|e| e.valid())
            && self.throw <= crate::sim::throwables::THROW_MINE
            && self.deploy_turn.is_finite() && self.deploy_turn.abs() <= std::f32::consts::TAU
            && self.throw_strength.is_finite() && (0.0..=1.0).contains(&self.throw_strength)
            && self.view <= crate::sim::lagcomp::MAX_VIEW && self.waypoint.is_none_or(|p| p.is_finite() && p.abs().max_element() < 100_000.0)
            && self.drop <= crate::sim::items::DROP_PACK && self.ahead <= crate::sim::lagcomp::MAX_AHEAD
    }
    pub fn input(self) -> Input {
        Input { move_x: self.move_x, move_z: self.move_z, jump: self.jump,
            jet: self.jet, fire: self.fire, interact:self.interact, repair: self.repair, buy: self.buy, deploy: self.deploy, deploy_turn: self.deploy_turn, suicide: self.suicide, throw: self.throw, throw_strength: self.throw_strength, weapon: self.weapon, equip: self.equip, toss: self.toss, mode: self.mode, ability: self.ability, operate: self.operate, waypoint: self.waypoint, drop: self.drop, board: self.board, ..Input::default() }
    }
}

/// `into` with any one-shot action from `other` it lacks: the first throw,
/// buy, deploy (with its turn) and suicide in a batch of commands survive a
/// server tick that keeps only the newest command.
pub fn merge_one_shots(mut into: Command, other: &Command) -> Command {
    if into.throw == 0 && other.throw != 0 { into.throw = other.throw; into.throw_strength = other.throw_strength; }
    if into.buy == 0 && other.buy != 0 { into.buy = other.buy; }
    if into.equip.is_none() { into.equip = other.equip; }
    into.toss |= other.toss;
    into.mode |= other.mode;
    into.ability |= other.ability;
    if into.waypoint.is_none() { into.waypoint = other.waypoint; }
    if into.drop == 0 { into.drop = other.drop; }
    into.board |= other.board;
    if !into.deploy && other.deploy { into.deploy = true; into.deploy_turn = other.deploy_turn; }
    into.suicide |= other.suicide;
    into
}

/// The held state of a command: the same with its one-shot actions removed.
pub fn without_one_shots(mut c: Command) -> Command {
    c.throw = 0; c.throw_strength = 0.0; c.buy = 0; c.equip = None; c.toss = false; c.mode = false; c.ability = false; c.waypoint = None; c.drop = 0; c.board = false; c.deploy = false; c.suicide = false;
    c
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase { Waiting, Countdown, Playing, Intermission }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    /// Server transport RTT in milliseconds, keyed by connection-assigned player ID.
    pub pings: Vec<(u32, u32)>,
    pub feed: Vec<crate::feed::Entry>,
    pub equipment: Vec<crate::equipment::State>,
    pub blast_serial: u64,
    pub tick: u64,
    pub round: u32,
    pub phase: Phase,
    pub phase_left: f32,
    pub map: MapId,
    pub players: Vec<Player>,
    pub acks: Vec<u64>,
    pub discs: Vec<Disc>,
    pub explosions: Vec<Explosion>,
    pub smoke: Vec<SmokePuff>,
    pub flags: [Flag; 2],
    pub score: [u32; 2],
    pub time_left: f32,
    #[serde(default)]
    pub mode: crate::map_catalog::SupportedMode,
    #[serde(default)]
    pub points: Vec<crate::control::Point>,
    #[serde(default)]
    pub ball: crate::sim::football::Ball,
    /// Rabbit and Team Rabbit: the neutral flag (`rabbit1`).
    #[serde(default)]
    pub rabbit: crate::sim::rabbit::RabbitFlag,
    #[serde(default)]
    pub deployables: Vec<crate::sim::deploy::Deployable>,
    #[serde(default)]
    pub loot: Vec<crate::sim::loadout::Loot>,
    /// Deathmatch time of day, weather and twist; the default elsewhere.
    #[serde(default)]
    pub conditions: crate::conditions::Conditions,
    /// A map/mode vote in progress or just decided (marker `vote1`).
    #[serde(default)]
    pub vote: Option<crate::vote::Vote>,
    /// The rotation entries a vote may pick (server-set).
    #[serde(default)]
    pub vote_choices: Vec<(MapId, crate::map_catalog::SupportedMode)>,
    /// Seconds until a new vote may be called (after a failed one, minutes).
    #[serde(default)]
    pub vote_wait: f32,
    /// The viewer's team's waypoint, set at a command station (`placed`).
    #[serde(default)]
    pub waypoint: Option<Vec3>,
    /// The players the viewer's team has marked (`firing`), by slot, and
    /// the spots.
    #[serde(default)]
    pub marked: Vec<u8>,
    #[serde(default)]
    pub marked_spots: Vec<Vec3>,
    /// The viewer's team's credits, where the economy is shared (`items`).
    #[serde(default)]
    pub credits: f32,
    /// Vehicles in play (`vehicles`).
    #[serde(default)]
    pub vehicles: Vec<crate::sim::vehicles::Craft>,
    /// The map's moveables' openness and items' clocks (`features`).
    #[serde(default)]
    pub moveables: Vec<f32>,
    #[serde(default)]
    pub map_items: Vec<f32>,
    /// People watching without a player slot (marker `teams1`).
    #[serde(default)]
    pub spectators: Vec<teams::Spectator>,
}

pub struct Match {
    rename_next: Vec<u64>,
    chat_next: Vec<u64>,
    pub world: World,
    pub tick: u64,
    pub round: u32,
    pub phase: Phase,
    pub phase_left: f32,
    pub acks: Vec<u64>,
    /// Map/mode votes; carried across map changes.
    pub votes: crate::vote::Votes,
    /// Server bots: the match is topped up to this many players with bots,
    /// and a joining person takes a bot's place (0: no bots).
    pub bots: usize,
    /// Connected people watching without a slot (`teams`).
    pub spectators: Vec<teams::Spectator>,
    /// Most spectators this server takes (at most `teams::MAX_SPECTATORS`).
    pub spectator_cap: usize,
    /// Team-change cooldowns; carried across map changes and empty resets.
    switches: teams::Switches,
    /// The host started the match (`host_start`): it plays with one team
    /// empty until nobody is left. Carried across map changes.
    pub host_started: bool,
    /// Always in play, with no clock, score or rounds: a duel server's lobby
    /// (`duel`). Carried across map changes and empty resets.
    pub open: bool,
    /// A duel's later round is counting down: its score and clock carry on
    /// (`next_duel_round`).
    duel_round: bool,
}

impl Match {
    /// Server-owned round boundary: retain connection slots/identities and clocks,
    /// but never carry projectiles, equipment, scores or movement between maps.
    pub fn rotate_to(&mut self, map: MapId) {
        let mode = self.world.mode;
        self.rotate_to_mode(map, mode);
    }

    /// Server rotation boundary with the entry's mode.
    pub fn rotate_to_mode(&mut self, map: MapId, mode: crate::map_catalog::SupportedMode) {
        let mut next = Self::new(map);
        next.world.set_mode(mode);
        next.world.forecast = self.world.forecast;
        // Carry the random stream over, so rotations don't replay a round.
        next.world.reseed(self.world.rng_state() ^ self.tick as u32);
        next.world.roll_conditions();
        next.tick = self.tick;
        next.round = self.round;
        // A new map is a new round, counted down from the new spawns (the
        // players respawn below, once). An idle server keeps waiting.
        if self.phase != Phase::Waiting {
            next.round = self.round + 1;
            next.phase = Phase::Countdown;
            next.phase_left = ROUND_COUNTDOWN;
        }
        next.acks.clone_from(&self.acks);
        next.chat_next.clone_from(&self.chat_next);
        next.rename_next.clone_from(&self.rename_next);
        next.votes = std::mem::take(&mut self.votes);
        next.bots = self.bots;
        next.spectators = std::mem::take(&mut self.spectators);
        next.spectator_cap = self.spectator_cap;
        next.switches = std::mem::take(&mut self.switches);
        next.host_started = self.host_started;
        next.open = self.open;
        for (slot, old) in self.world.players.iter().enumerate() {
            // Bots are re-added by the new match (`maintain_bots`).
            if !human(old) { continue; }
            // A fresh, locally simulated player as `join` makes (the empty
            // slots of a new match are remote placeholders, which ignore
            // input: after a map change nobody could move, jet or fire).
            let ember = old.team == Team::Ember;
            let mut p = make_player(old.team, false, next.world.stand(ember),
                if ember { std::f32::consts::PI } else { 0.0 }, BotRole::Offense);
            p.net_id = old.net_id; p.name = old.name.clone();
            // Their setup goes with them to the next map.
            p.setup = old.setup;
            next.world.players[slot] = p;
            next.world.respawn(slot);
        }
        *self = next;
    }

    pub fn new(map: MapId) -> Self {
        let mut world = World::new();
        world.set_map(map);
        world.start_rift(true);
        world.players.clear();
        for _ in 0..MAX_PLAYERS {
            let mut p = make_player(Team::Ember, false, world.stand(true), 0.0, BotRole::Offense);
            p.alive = false;
            p.remote = true;
            world.players.push(p);
        }
        world.network_inputs = vec![Input::default(); MAX_PLAYERS];
        Self { world, tick: 0, round: 0, phase: Phase::Waiting, phase_left: 0.0,
            acks: vec![0; MAX_PLAYERS], chat_next: vec![0; MAX_PLAYERS], rename_next: vec![0; MAX_PLAYERS],
            votes: Default::default(), bots: 0,
            spectators: Vec::new(), spectator_cap: teams::MAX_SPECTATORS, switches: Default::default(), host_started: false,
            open: false, duel_round: false }
    }

    /// A hosted server's host starts the match without waiting for both
    /// teams: the countdown begins now, and play goes on with one team
    /// empty until everyone has left. The server checks it's the host.
    pub fn host_start(&mut self) -> Result<(), &'static str> {
        if self.phase != Phase::Waiting { return Err("The match has already started"); }
        if !self.world.players.iter().any(human) { return Err("Join a team to start the match"); }
        self.host_started = true;
        self.begin_countdown(self.world.mode.def().warmup);
        Ok(())
    }

    pub fn join(&mut self, id: u32, name: &str) -> Option<usize> {
        let count = |team| self.world.players.iter().filter(|p| human(p) && p.team == team).count();
        let team = if count(Team::Ember) <= count(Team::Glacier) { Team::Ember } else { Team::Glacier };
        self.join_on(id, name, team)
    }

    /// `join` on `team`: a duel server seats each duelist on their own side.
    pub fn join_on(&mut self, id: u32, name: &str, team: Team) -> Option<usize> {
        let name = crate::names::validate(name)?;
        if id == 0 || self.world.players.iter().any(|p| p.net_id == id) { return None; }
        // A free slot, or else a bot's (one on the joining team first).
        let slot = match self.world.players.iter().position(|p| p.net_id == 0) {
            Some(slot) => slot,
            None => {
                let bot = self.world.players.iter().position(|p| p.is_bot && p.net_id != 0 && p.team == team)
                    .or_else(|| self.world.players.iter().position(|p| p.is_bot && p.net_id != 0))?;
                self.remove_bot(bot);
                bot
            }
        };
        let mut p = make_player(team, false, self.world.stand(team == Team::Ember),
            if team == Team::Ember { std::f32::consts::PI } else { 0.0 }, BotRole::Offense);
        p.net_id = id;
        p.name = name.into();
        p.credits = self.world.starting_credits();
        self.world.players[slot] = p;
        self.world.respawn(slot);
        self.acks[slot] = 0;
        self.chat_next[slot] = 0;
        self.rename_next[slot] = 0;
        Some(slot)
    }

    /// Slot comes from the connection, never from a client-supplied player ID.
    pub fn rename(&mut self, slot: usize, raw: &str) -> bool {
        let Some(name) = crate::names::validate(raw) else { return false; };
        let Some(p) = self.world.players.get_mut(slot).filter(|p| p.net_id != 0) else { return false; };
        if self.tick < self.rename_next[slot] { return false; }
        if p.name == name { return true; }
        p.name = name.into();
        self.rename_next[slot] = self.tick.saturating_add(600);
        true
    }

    /// Identity comes only from the authenticated connection's occupied slot.
    /// Top each team up with bots so it holds `bots / 2` players, and take
    /// bots out where people now fill the places.
    pub fn maintain_bots(&mut self) {
        if self.bots == 0 && !self.world.players.iter().any(|p| p.is_bot && p.net_id != 0) { return; }
        let half = self.bots / 2;
        for team in [Team::Ember, Team::Glacier] {
            let people = self.world.players.iter().filter(|p| human(p) && p.team == team).count();
            let want = half.saturating_sub(people);
            let mut have: Vec<usize> = self.world.players.iter().enumerate()
                .filter(|(_, p)| p.is_bot && p.net_id != 0 && p.team == team).map(|(i, _)| i).collect();
            // Too many: dead bots go first.
            have.sort_by_key(|&i| self.world.players[i].alive);
            for &slot in have.iter().take(have.len().saturating_sub(want)) { self.remove_bot(slot); }
            for _ in have.len()..want {
                let Some(slot) = self.world.players.iter().position(|p| p.net_id == 0) else { break };
                self.add_bot(slot, team);
            }
        }
    }

    /// A duel's bot opponent on Glacier, of the archetype a board offered
    /// (`duel::BOTS`). It stays: `bots` is 2, one a side less the people.
    pub fn add_duel_bot(&mut self, archetype: &str) -> Option<usize> {
        let profile = *crate::bot_nav::ARCHETYPES.iter().find(|p| p.name == archetype)?;
        let slot = self.world.players.iter().position(|p| p.net_id == 0)?;
        self.bots = 2;
        self.add_bot(slot, Team::Glacier);
        let p = &mut self.world.players[slot];
        (p.bot_profile, p.bot_role, p.name) = (profile, BotRole::Hunter, profile.name.into());
        Some(slot)
    }

    /// A line in the feed (a duel server's lobby hears how duels end).
    pub fn announce(&mut self, text: String) { self.world.play(text); }

    /// A duel's next round (`duel`): both back at a fresh pair of spots,
    /// held for `duel::ROUND_COUNTDOWN`; the duel's score, clock, tallies
    /// and sky carry on.
    fn next_duel_round(&mut self) {
        let (score, clock, conditions) = (self.world.score, self.world.time_left, self.world.conditions);
        let tallies: Vec<(u32, u32, u32, u32)> = self.world.players.iter().map(|p| (p.frags, p.losses, p.hits, p.shots)).collect();
        if let Some(d) = self.world.duel.as_mut() { d.next_round = None; }
        self.world.pick_duel_spots();
        self.world.conditions = conditions;
        self.begin_countdown(crate::sim::duel::ROUND_COUNTDOWN);
        self.world.conditions = conditions;
        (self.world.score, self.world.time_left) = (score, clock);
        for (p, t) in self.world.players.iter_mut().zip(tallies) { (p.frags, p.losses, p.hits, p.shots) = t; }
        self.duel_round = true;
    }

    fn add_bot(&mut self, slot: usize, team: Team) {
        let ember = team == Team::Ember;
        let role = if slot % 3 == 0 { BotRole::Defense } else { BotRole::Offense };
        let mut p = make_player(team, true, self.world.stand(ember), if ember { std::f32::consts::PI } else { 0.0 }, role);
        p.net_id = BOT_ID_BASE + slot as u32;
        self.world.players[slot] = p;
        self.world.assign_personality(slot);
        self.world.respawn(slot);
        self.acks[slot] = 0;
    }

    fn remove_bot(&mut self, slot: usize) {
        if let Some(team) = self.world.players[slot].carrying.take() {
            self.world.drop_flag(team, self.world.players[slot].pos);
        }
        self.world.ball_lost(slot);
        self.world.leaver_body(slot);
        self.world.unseat(slot);
        let p = &mut self.world.players[slot];
        p.net_id = 0; p.name.clear(); p.alive = false; p.remote = true; p.is_bot = false;
        self.world.discs.retain(|d| d.owner != slot);
        self.world.network_inputs[slot] = Input::default();
    }

    /// Player in `slot` calls a vote for rotation entry `choice`.
    pub fn call_vote(&mut self, slot: usize, choice: usize) -> Result<(), &'static str> {
        let Some(p) = self.world.players.get(slot).filter(|p| p.net_id != 0) else { return Err("not in the match") };
        let (id, name) = (p.net_id, p.name.clone());
        let players = self.world.players.iter().filter(|p| human(p)).count() as u32;
        self.votes.call(id, &name, choice, self.tick, players, (self.world.map, self.world.mode))?;
        crate::feed::push(&mut self.world.feed, crate::feed::Entry::Play {
            text: format!("{name} called a vote: {}", self.votes.current.as_ref().map_or(String::new(), |v| label(v.map, v.mode))) });
        Ok(())
    }

    /// Player in `slot` votes yes or no on the running vote.
    pub fn cast_vote(&mut self, slot: usize, yes: bool) -> bool {
        let Some(id) = self.world.players.get(slot).map(|p| p.net_id).filter(|&id| id != 0) else { return false };
        self.votes.cast(id, yes)
    }

    pub fn chat(&mut self, slot: usize, text: &str) -> bool {
        self.chat_channel(slot, text, false)
    }

    pub fn chat_channel(&mut self, slot: usize, text: &str, team_only: bool) -> bool {
        let Some(p) = self.world.players.get(slot).filter(|p| p.net_id != 0) else { return false; };
        let Some(text) = crate::feed::message(text) else { return false; };
        if self.tick < self.chat_next[slot] { return false; }
        self.chat_next[slot] = self.tick.saturating_add(60);
        let entry = if team_only { crate::feed::Entry::TeamChat { sender:p.name.clone(), text, team:p.team } }
            else { crate::feed::Entry::Chat { sender:p.name.clone(), text } };
        crate::feed::push(&mut self.world.feed, entry);
        true
    }

    pub fn leave(&mut self, slot: usize) {
        self.votes.forget(self.world.players[slot].net_id);
        self.vacate(slot);
    }

    /// Empty `slot` (a departure, or a player turning spectator).
    fn vacate(&mut self, slot: usize) {
        if let Some(team) = self.world.players[slot].carrying.take() {
            self.world.drop_flag(team, self.world.players[slot].pos);
        }
        self.world.ball_lost(slot);
        self.world.leaver_body(slot);
        self.world.unseat(slot);
        let p = &mut self.world.players[slot];
        p.net_id = 0;
        p.name.clear();
        p.alive = false;
        p.remote = true;
        self.world.discs.retain(|d| d.owner != slot);
        self.world.network_inputs[slot] = Input::default();
        self.acks[slot] = 0;
        if self.world.players.iter().all(|p| p.net_id == 0) {
            // Fresh session, but keep the transport/healthcheck clock monotonic.
            let tick = self.tick;
            let mode = self.world.mode;
            let bots = self.bots;
            // Spectators stay connected (with their addresses and cooldowns).
            let spectators = std::mem::take(&mut self.spectators);
            let (cap, switches) = (self.spectator_cap, std::mem::take(&mut self.switches));
            let votes = self.votes.carry_addresses();
            let (open, forecast) = (self.open, self.world.forecast);
            *self = Self::new(self.world.map);
            self.world.set_mode(mode);
            (self.open, self.world.forecast) = (open, forecast);
            self.tick = tick;
            self.bots = bots;
            (self.spectators, self.spectator_cap, self.switches, self.votes) = (spectators, cap, switches, votes);
        }
    }

    fn balance_teams(&mut self) {
        loop {
            // People; on a bot server, bots too (they top each team up, so a
            // person who took a bot's place on the other side stays there).
            let counts = self.team_sizes(None);
            if counts[0].abs_diff(counts[1]) <= 1 { break; }
            let larger = if counts[0] > counts[1] { Team::Ember } else { Team::Glacier };
            // Avoid carriers first, then prefer dead players and newest arrivals.
            let slot = self.world.players.iter().enumerate()
                .filter(|(_, p)| human(p) && p.team == larger)
                .min_by_key(|(_, p)| (p.carrying.is_some(), p.alive, std::cmp::Reverse(p.net_id)))
                .map(|(i, _)| i).unwrap();
            self.move_to_team(slot, larger.other());
        }
    }

    fn restart(&mut self) {
        self.world.equipment=crate::equipment::fresh(self.world.map);
        self.world.deployables.clear();
        self.world.vehicles.clear();
        for p in &mut self.world.players { p.riding = 0; }
        self.world.place_map_deployables();
        self.world.uses.clear();
        self.world.loot.clear();
        self.world.discs.clear();
        self.world.explosions.clear();
        self.world.smoke.clear();
        self.world.place_flags();
        self.world.reset_points();
        self.world.reseed(self.tick as u32);
        self.world.reset_credits();
        let twist = self.world.conditions.twist;
        self.world.roll_conditions();
        // A twist's forced kit ends with its round; the player's own setup
        // stays (`Setup`), with the new round's weapons.
        for i in 0..self.world.players.len() {
            if twist == crate::conditions::Twist::Heavies {
                let p = &mut self.world.players[i];
                p.armor = p.setup.armor.unwrap_or(ArmorClass::Light);
            }
            self.world.equip(i);
        }
        self.world.score = [0, 0];
        self.world.time_left = self.world.match_time();
        self.world.state = MatchState::Playing;
        for i in 0..MAX_PLAYERS {
            if self.world.players[i].net_id != 0 {
                self.world.respawn(i);
                let p = &mut self.world.players[i];
                p.frags = 0; p.losses = 0; p.hits = 0; p.shots = 0; p.jump_prev = false; p.jet_prev = false;
            }
        }
    }

    /// A new round: everyone back at their spawns (the only teleport), held
    /// there for `seconds`. The round number changes here, not when play
    /// begins, so clients reset their prediction at the teleport and carry it
    /// straight on into play.
    fn begin_countdown(&mut self, seconds: f32) {
        self.restart();
        self.round += 1;
        self.phase = Phase::Countdown;
        self.phase_left = seconds;
    }

    /// Exactly one server-owned timestep. Packet volume cannot advance time.
    pub fn step(&mut self, commands: &[Option<Command>]) {
        self.tick += 1;
        self.world.events.clear();
        self.world.spatial_sounds.clear();
        let present: Vec<u32> = self.world.players.iter().filter(|p| human(p)).map(|p| p.net_id).collect();
        self.votes.step(STEP, &present, self.tick);
        // Bots first: a bot server plays even with nobody connected.
        self.maintain_bots();
        if self.world.players.iter().all(|p| p.net_id == 0) { return; }
        // Once per tick, after the server processes the whole departure batch.
        self.balance_teams();
        self.world.time += STEP;
        let both = [Team::Ember, Team::Glacier].iter().all(|team|
            self.world.players.iter().any(|p| p.net_id != 0 && p.team == *team));
        // A host's start holds while anyone is playing; once everyone has
        // gone, the next match waits for both teams (or the host) again.
        if self.host_started && !self.world.players.iter().any(human) { self.host_started = false; }
        let both = both || self.host_started;
        // A duel lobby is always in play.
        if self.open { self.phase = Phase::Playing; self.phase_left = 0.0; }
        else { match self.phase {
            Phase::Playing if !both => {
                // An abandoned team cannot concede a string of uncontested rounds.
                self.restart(); self.phase = Phase::Waiting; self.phase_left = 0.0;
            }
            Phase::Waiting if both => {
                // Football's first start keeps its 20 s warmup; every
                // other round start counts down ROUND_COUNTDOWN.
                self.begin_countdown(self.world.mode.def().warmup);
            }
            Phase::Countdown if !both => { self.phase = Phase::Waiting; self.phase_left = 0.0; }
            Phase::Countdown => {
                self.phase_left -= STEP;
                // Play begins at zero (on the whole tick: no float drift),
                // from where everyone already stands.
                if self.phase_left <= STEP * 0.5 { self.phase = Phase::Playing; self.phase_left = 0.0; self.duel_round = false; }
            }
            Phase::Intermission => {
                self.phase_left -= STEP;
                if self.phase_left <= 0.0 {
                    if both { self.begin_countdown(ROUND_COUNTDOWN); }
                    else { self.restart(); self.round += 1; self.phase = Phase::Waiting; self.phase_left = 0.0; }
                }
            }
            _ => {}
        } }
        let mut views = vec![(0, 0); MAX_PLAYERS];
        let def = self.world.mode.def();
        let limit = self.world.look_limit();
        for (i, p) in self.world.players.iter_mut().enumerate() {
            let command = commands.get(i).copied().flatten().unwrap_or_default();
            let input = if command.valid() && p.net_id != 0 { command } else { Command::default() };
            // People only: bots shoot at the present.
            if human(p) { views[i] = (input.view, input.ahead); }
            if input.seq > 0 {
                self.acks[i] = self.acks[i].max(input.seq);
                p.yaw = input.yaw;
                p.pitch = input.pitch.clamp(-limit, limit);
                // The rifle once bought, and the mode's weapons (sim::rifles).
                p.weapon = crate::sim::rifles::allowed_slot(def, p, input.weapon);
            }
            self.world.network_inputs[i] = input.input();
            if self.phase == Phase::Countdown || self.phase == Phase::Intermission {
                self.world.network_inputs[i] = Input::default();
            }
        }
        // The world holds everyone still (bots too) and stops its clock.
        self.world.countdown = if self.phase == Phase::Countdown { self.phase_left } else { 0.0 };
        self.world.lag.begin_claims(self.tick, views);
        if self.phase != Phase::Intermission {
            // The world steps to this tick (`features` moves by it).
            self.world.steps = self.tick.saturating_sub(1);
            self.world.physics_step();
            if self.phase != Phase::Playing || self.open {
                // Warmup and countdown never capture or score (a duel's
                // later rounds keep the duel's score and clock), and a
                // duel lobby never does.
                if self.world.points.iter().any(|p| p.owner.is_some() || p.progress > 0.0) {
                    self.world.reset_points();
                }
                if !self.duel_round {
                    self.world.score = [0, 0];
                    self.world.time_left = self.world.match_time();
                }
                self.world.state = MatchState::Playing;
            } else if self.world.state == MatchState::Ended {
                self.phase = Phase::Intermission;
                self.phase_left = 10.0;
            } else if self.world.duel_round_due(STEP) {
                self.next_duel_round();
            }
        }
        self.world.lag.record(self.tick, &self.world.players);
    }

    /// The public snapshot: team chat is left out.
    pub fn snapshot(&self) -> Snapshot { self.snapshot_filtered(None) }

    /// Team chat only reaches `team` (none: nobody).
    fn snapshot_filtered(&self, team: Option<Team>) -> Snapshot {
        let feed = self.world.feed.iter().filter(|e| match e {
            crate::feed::Entry::TeamChat { team: recipient, .. } => Some(*recipient) == team,
            _ => true,
        }).cloned().collect();
        Snapshot { pings:Vec::new(), feed, equipment:self.world.equipment.clone(),blast_serial: self.world.blast_serial, tick: self.tick, round: self.round, phase: self.phase,
            phase_left: self.phase_left, map: self.world.map, players: self.players_seen_by(team),
            acks: self.acks.clone(), discs: self.world.discs.clone(),
            explosions: self.world.explosions.clone(), smoke: self.world.smoke.clone(),
            flags: self.world.flags.clone(), score: self.world.score, time_left: self.world.time_left,
            mode: self.world.mode, points: self.world.points.clone(), ball: self.world.ball.clone(), rabbit: self.world.rabbit.clone(), deployables: self.world.deployables.clone(), loot: self.world.loot.clone(),
            conditions: self.world.conditions, vote: self.votes.current.clone(), vote_choices: self.votes.choices.clone(), vote_wait: self.votes.wait(self.tick),
            spectators: self.spectators.clone(), waypoint: team.and_then(|t| self.world.waypoints[t.idx()]),
            marked: team.map_or_else(Vec::new, |t| self.world.marked_by(t)),
            marked_spots: team.map_or_else(Vec::new, |t| self.world.marked_spots(t)),
            credits: team.map_or(0.0, |t| self.world.credits[t.idx()]),
            moveables: self.world.moveable_open.clone(), map_items: self.world.map_item_clocks.clone(), vehicles: self.world.vehicles.clone() }
    }

    /// The players as `team` sees them (None: spectators): an enemy all but
    /// invisible (`status::UNSEEN`) and with none of `team` within their
    /// shimmer range is left out, put far below the map (`HIDDEN_Y`).
    fn players_seen_by(&self, team: Option<Team>) -> Vec<Player> {
        // (A view is per team, so in free-for-all a player's own team still
        // sees them: their own client needs its own body.)
        let mut players = self.world.players.clone();
        let watchers: Vec<(Team, Vec3)> = self.world.players.iter().filter(|p| p.alive && p.net_id != 0).map(|p| (p.team, p.pos)).collect();
        for p in &mut players {
            // A flag carrier is always seen.
            if !p.alive || p.effects.is_empty() || Some(p.team) == team || p.carrying.is_some() { continue; }
            if crate::sim::status::visibility(&p.effects) >= crate::sim::status::UNSEEN { continue; }
            let near = crate::sim::status::shimmer(&p.effects);
            let seen = team.is_some_and(|t| watchers.iter().any(|(wt, at)| *wt == t && at.distance(p.pos) <= near));
            if seen { continue; }
            p.pos = Vec3::new(0.0, crate::sim::HIDDEN_Y, 0.0);
            p.vel = Vec3::ZERO;
            p.repair_beam = None;
            p.repair_out = false;
            p.stream = None;
        }
        players
    }

    pub fn snapshot_for(&self, slot: usize) -> Snapshot {
        let team = self.world.players.get(slot).filter(|p|p.net_id != 0).map(|p|p.team);
        self.snapshot_filtered(team)
    }
}

#[cfg(test)]
mod one_shot_tests {
    use super::*;
    #[test]
    fn a_press_in_a_skipped_command_survives_and_acts_once() {
        let press = Command { seq: 5, throw: 1, throw_strength: 0.7, buy: 3, deploy: true, deploy_turn: 0.5, ..Command::default() };
        let later = Command { seq: 6, move_z: 1.0, ..Command::default() };
        let merged = merge_one_shots(later, &press);
        assert_eq!((merged.throw, merged.buy, merged.deploy, merged.seq), (1, 3, true, 6));
        assert!((merged.throw_strength - 0.7).abs() < 1e-6 && (merged.deploy_turn - 0.5).abs() < 1e-6);
        let held = without_one_shots(merged);
        assert_eq!((held.throw, held.buy, held.deploy, held.suicide, held.move_z), (0, 0, false, false, 1.0));
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    #[test]
    fn a_bot_server_plays_by_itself_and_people_take_bots_places() {
        let mut game = Match::new(MapId::OldHoller);
        game.bots = 8;
        let none = vec![None; MAX_PLAYERS];
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 420 { game.step(&none); }
        let bots = |g: &Match| g.world.players.iter().filter(|p| p.is_bot && p.net_id != 0).count();
        let on = |g: &Match, t: Team| g.world.players.iter().filter(|p| p.net_id != 0 && p.team == t).count();
        assert_eq!((bots(&game), on(&game, Team::Ember), on(&game, Team::Glacier)), (8, 4, 4));
        assert_eq!(game.phase, Phase::Playing, "bots alone play a round");
        let moved = game.world.players.iter().filter(|p| p.is_bot && p.vel.length() > 1.0).count();
        assert!(moved >= 2, "server bots fly their own inputs: {moved} moving");
        // A person joins: a bot makes way, and teams stay 4 v 4.
        let slot = game.join(7, "Human").unwrap();
        game.step(&none);
        assert!(human(&game.world.players[slot]));
        assert_eq!((bots(&game), on(&game, Team::Ember), on(&game, Team::Glacier)), (7, 4, 4));
        // Leaving brings a bot back; a map change keeps the bots.
        game.leave(slot);
        game.step(&none);
        assert_eq!(bots(&game), 8);
        game.rotate_to_mode(MapId::Reefbreak, crate::map_catalog::SupportedMode::Ctf);
        game.step(&none);
        assert_eq!(bots(&game), 8);
        // Even a full bot server takes a person.
        let mut full = Match::new(MapId::OldHoller);
        full.bots = MAX_PLAYERS;
        full.step(&none);
        assert_eq!(bots(&full), MAX_PLAYERS);
        assert!(full.join(9, "Late").is_some());
    }

    #[test]
    fn after_a_map_change_players_move_jet_and_fire() {
        let mut game = Match::new(MapId::OldHoller);
        let a = game.join(11, "Runner").unwrap();
        game.join(22, "Other").unwrap();
        game.rotate_to_mode(MapId::Reefbreak, crate::map_catalog::SupportedMode::Ctf);
        assert!(!game.world.players[a].remote, "a networked player, not a remote placeholder");
        game.phase = Phase::Playing;
        let start = game.world.players[a].pos;
        let mut commands = vec![None; MAX_PLAYERS];
        for seq in 1..=120u64 {
            commands[a] = Some(Command { seq, move_z: 1.0, jet: true, fire: seq == 60, ..Command::default() });
            game.step(&commands);
        }
        let p = &game.world.players[a];
        assert!(p.pos.distance(start) > 3.0, "moved: {:?} -> {:?}", start, p.pos);
        assert!(p.shots > 0, "fired");
    }

    #[test]
    fn map_transition_preserves_slots_clocks_and_rate_limits_not_world_state() {
        let mut game = Match::new(MapId::Valley);
        game.join(11, "First").unwrap();
        game.join(22, "Second").unwrap();
        game.join(33, "Third").unwrap();
        game.leave(0); // Noncontiguous occupied slots must stay stable.
        game.tick = 500; game.round = 7;
        game.phase = Phase::Intermission; game.phase_left = STEP;
        game.acks[1] = 123;
        assert!(game.chat(1, "Before rotation"));
        game.world.score = [3, 2];
        game.world.players[1].frags = 9;
        game.world.players[1].vel = Vec3::splat(70.);
        let team = game.world.players[1].team;
        game.rotate_to(MapId::OldHoller);
        assert_eq!(game.world.map, MapId::OldHoller);
        // The new map's round counts down from the new spawns.
        assert_eq!(game.tick, 500); assert_eq!(game.round, 8);
        assert_eq!((game.phase, game.phase_left), (Phase::Countdown, ROUND_COUNTDOWN));
        assert_eq!(game.world.players[0].net_id, 0);
        assert_eq!(game.world.players[1].net_id, 22);
        assert_eq!(game.world.players[2].net_id, 33);
        assert_eq!(game.world.players[1].name, "Second");
        assert_eq!(game.world.players[1].team, team);
        assert_eq!(game.acks[1], 123);
        assert!(!game.chat(1, "Too soon"));
        assert_eq!(game.world.score, [0, 0]);
        assert_eq!(game.world.players[1].frags, 0);
        assert_eq!(game.world.players[1].vel, Vec3::ZERO);
        assert!(game.world.discs.is_empty()); assert!(game.world.feed.is_empty());
        game.step(&[]);
        assert_eq!(game.round, 8); assert_eq!(game.phase, Phase::Countdown);
        game.leave(1); game.leave(2);
        assert_eq!(game.phase, Phase::Waiting); assert_eq!(game.round, 0);
        assert!(game.tick > 500);
    }
}

#[cfg(test)]
mod countdown_tests {
    use super::*;
    use crate::map_catalog::SupportedMode;
    const COUNTDOWN_TICKS: usize = (ROUND_COUNTDOWN / STEP) as usize;

    /// Everyone pushing every control at once, every tick.
    fn mash(game: &Match, seq: u64) -> Vec<Option<Command>> {
        (0..MAX_PLAYERS).map(|i| (game.world.players[i].net_id != 0).then_some(Command { seq, move_z: 1.0, move_x: 0.5,
            jet: true, jump: true, fire: true, interact: true, repair: true, throw: 1, throw_strength: 1.0, ..Command::default() })).collect()
    }

    /// Runs one whole countdown from its first tick with everyone mashing:
    /// nobody leaves their spawn, fires, dies or scores, the clock waits,
    /// and play starts on the tick it reaches zero, with no second teleport.
    fn count_down(game: &mut Match, seq: &mut u64, seconds: f32) {
        assert_eq!(game.phase, Phase::Countdown);
        assert!((game.phase_left - seconds).abs() < 1e-4, "{} s, not {seconds}", game.phase_left);
        let round = game.round;
        let spawns: Vec<Vec3> = game.world.players.iter().map(|p| p.pos).collect();
        let mut ticks = 0;
        while game.phase == Phase::Countdown {
            let last: Vec<Vec3> = game.world.players.iter().map(|p| p.pos).collect();
            *seq += 1;
            let commands = mash(game, *seq);
            game.step(&commands);
            ticks += 1;
            assert!(ticks <= (seconds / STEP).round() as usize, "still counting down after {ticks} ticks");
            if game.phase == Phase::Playing {
                // The first playing tick moves from where everyone stood.
                for (i, p) in game.world.players.iter().enumerate().filter(|(_, p)| p.net_id != 0) {
                    assert!(p.pos.distance(last[i]) < 1.0, "slot {i} teleported as play began");
                }
                break;
            }
            assert_eq!(game.world.score, [0, 0]);
            assert_eq!(game.world.time_left, game.world.match_time());
            assert!(game.world.discs.is_empty() && game.world.flags.iter().all(|f| f.carrier.is_none()));
            for (i, p) in game.world.players.iter().enumerate().filter(|(_, p)| p.net_id != 0) {
                assert!(p.alive && p.shots == 0, "slot {i} acted during the countdown");
                let drift = Vec3::new(p.pos.x - spawns[i].x, 0.0, p.pos.z - spawns[i].z).length();
                assert!(drift < 0.5, "slot {i} moved {drift} m during the countdown");
            }
        }
        assert_eq!(ticks, (seconds / STEP).round() as usize, "play begins exactly at zero");
        assert_eq!(game.phase, Phase::Playing);
        assert_eq!(game.round, round, "the round number changes at the countdown's start");
    }

    fn pair(map: MapId, mode: SupportedMode) -> Match {
        let mut game = Match::new(map);
        game.world.set_mode(mode);
        game.join(1, "Alpha").unwrap();
        game.join(2, "Bravo").unwrap();
        game
    }

    #[test]
    fn every_round_start_counts_down_ten_seconds_at_the_spawns() {
        let mut game = pair(MapId::OldHoller, SupportedMode::Ctf);
        let mut seq = 0;
        // First start: waiting, then both teams present.
        game.step(&[]);
        assert_eq!(game.round, 1);
        count_down(&mut game, &mut seq, ROUND_COUNTDOWN);
        // Play is live: inputs move people.
        let a = game.world.players[0].pos;
        for _ in 0..60 {
            seq += 1;
            let c: Vec<_> = (0..MAX_PLAYERS).map(|_| Some(Command { seq, move_z: 1.0, ..Command::default() })).collect();
            game.step(&c);
        }
        assert!(game.world.players[0].pos.distance(a) > 1.0);
        // After an intermission (with no rotation), from the spawns again.
        game.world.time_left = STEP * 0.5;
        game.step(&[]);
        assert_eq!(game.phase, Phase::Intermission);
        for _ in 0..600 { game.step(&[]); }
        assert_eq!(game.round, 2);
        count_down(&mut game, &mut seq, ROUND_COUNTDOWN);
        // A rotation at the intermission's end (the server's order: rotate, then step).
        game.world.time_left = STEP * 0.5;
        game.step(&[]);
        while game.phase_left > STEP { game.step(&[]); }
        game.rotate_to_mode(MapId::Cairnhold, SupportedMode::Ctf);
        assert_eq!((game.world.map, game.round), (MapId::Cairnhold, 3));
        count_down(&mut game, &mut seq, ROUND_COUNTDOWN);
        // A passed vote switches mid-round: a fresh countdown on the new map.
        for _ in 0..30 { seq += 1; let c = mash(&game, seq); game.step(&c); }
        game.rotate_to_mode(MapId::Reefbreak, SupportedMode::TeamDeathmatch);
        assert_eq!((game.world.map, game.round), (MapId::Reefbreak, 4));
        count_down(&mut game, &mut seq, ROUND_COUNTDOWN);
    }

    #[test]
    fn football_keeps_its_first_warmup_then_counts_down_ten() {
        let mut game = pair(MapId::Longfield, SupportedMode::Football);
        let mut seq = 0;
        game.step(&[]);
        count_down(&mut game, &mut seq, crate::sim::football::WARMUP_SECONDS);
        game.world.time_left = STEP * 0.5;
        game.step(&[]);
        assert_eq!(game.phase, Phase::Intermission);
        for _ in 0..600 { game.step(&[]); }
        count_down(&mut game, &mut seq, ROUND_COUNTDOWN);
    }

    #[test]
    fn an_emptied_team_stops_the_countdown_and_nobody_scores_meanwhile() {
        let mut game = pair(MapId::OldHoller, SupportedMode::Ctf);
        game.step(&[]);
        // Standing on the enemy flag during the countdown takes nothing.
        let glacier = game.world.players.iter().position(|p| p.net_id != 0 && p.team == Team::Glacier).unwrap();
        let ember = 1 - glacier;
        for _ in 0..120 {
            game.world.players[ember].pos = game.world.flags[1].pos + Vec3::Y;
            game.step(&[]);
        }
        assert_eq!(game.phase, Phase::Countdown);
        assert!(game.world.flags[1].carrier.is_none() && game.world.score == [0, 0]);
        game.leave(glacier);
        game.step(&[]);
        assert_eq!((game.phase, game.phase_left), (Phase::Waiting, 0.0));
        // A new opponent starts a full countdown again.
        game.join(3, "Charlie").unwrap();
        game.step(&[]);
        assert_eq!((game.phase, game.phase_left), (Phase::Countdown, ROUND_COUNTDOWN));
    }

    #[test]
    fn server_bots_hold_still_during_the_countdown() {
        let mut game = Match::new(MapId::OldHoller);
        game.bots = 8;
        game.step(&[]);
        assert_eq!(game.phase, Phase::Countdown);
        let spawns: Vec<Vec3> = game.world.players.iter().map(|p| p.pos).collect();
        for _ in 0..COUNTDOWN_TICKS - 10 { game.step(&[]); }
        assert_eq!(game.phase, Phase::Countdown);
        for (i, p) in game.world.players.iter().enumerate().filter(|(_, p)| p.is_bot && p.net_id != 0) {
            let drift = Vec3::new(p.pos.x - spawns[i].x, 0.0, p.pos.z - spawns[i].z).length();
            assert!(drift < 0.5 && p.shots == 0, "bot {i} moved {drift} m");
        }
    }

    #[test]
    fn a_snapshot_mirrors_the_countdown_and_clears_it_for_play() {
        let mut game = pair(MapId::Valley, SupportedMode::Ctf);
        game.step(&[]);
        let mut client = World::new();
        assert!(client.apply_snapshot(&game.snapshot(), 1));
        assert_eq!(client.countdown, game.phase_left);
        assert!(client.message.is_empty(), "the HUD shows the countdown, not a message");
        game.phase = Phase::Playing;
        assert!(client.apply_snapshot(&game.snapshot(), 1));
        assert_eq!(client.countdown, 0.0);
    }

    #[test]
    fn an_offline_match_counts_down_frozen_then_plays() {
        let mut world = World::new();
        world.set_map(MapId::OldHoller);
        world.start_counted_match(true);
        assert_eq!(world.countdown, ROUND_COUNTDOWN);
        let clock = world.time_left;
        let spawns: Vec<Vec3> = world.players.iter().map(|p| p.pos).collect();
        world.input.move_z = 1.0; world.input.jet = true; world.input.fire = true;
        // Frame by frame, like the client: ten seconds of held keys.
        for _ in 0..(COUNTDOWN_TICKS - 6) / 2 { world.tick(2.0 * STEP); }
        assert!(world.countdown > 0.0 && world.countdown < 0.2, "{} s left", world.countdown);
        assert_eq!(world.time_left, clock, "the match clock waits");
        for (i, p) in world.players.iter().enumerate() {
            let drift = Vec3::new(p.pos.x - spawns[i].x, 0.0, p.pos.z - spawns[i].z).length();
            assert!(drift < 0.5 && p.shots == 0, "player {i} (bot: {}) moved {drift} m", p.is_bot);
        }
        for _ in 0..60 { world.tick(STEP); }
        assert_eq!(world.countdown, 0.0);
        assert!(world.time_left < clock);
        let me = &world.players[world.player_id];
        assert!(Vec3::new(me.pos.x - spawns[0].x, 0.0, me.pos.z - spawns[0].z).length() > 0.5, "moves once play begins");
    }
}

impl World {
    pub fn apply_snapshot(&mut self, snapshot: &Snapshot, id: u32) -> bool {
        let Some(slot) = snapshot.players.iter().position(|p| p.net_id == id) else { return false; };
        if !self.apply_shared(snapshot) { return false; }
        self.spectating = false;
        self.player_id = slot;
        self.players[slot].remote = false;
        self.input.jump_prev = self.players[slot].jump_prev;
        // Their team's marks (`firing`), until the next snapshot.
        let team = self.players[slot].team;
        self.marks = snapshot.marked.iter().filter_map(|&j| self.players.get(j as usize).map(|q| (j as usize, q.pos)))
            .map(|(j, pos)| crate::sim::firing::Mark { team, player: Some(j), pos: pos + Vec3::Y * 0.9, life: 1.0, by: None })
            .chain(snapshot.marked_spots.iter().map(|&pos| crate::sim::firing::Mark { team, player: None, pos, life: 1.0, by: None })).collect();
        self.kills = self.players[slot].frags;
        self.deaths = self.players[slot].losses;
        true
    }

    /// A snapshot for spectator `id` (`teams1`): every player is remote, and
    /// the local "player" is a dead stand-in past the real slots, so code
    /// that reads `players[player_id]` still works. False unless `id` is
    /// one of the snapshot's spectators.
    pub fn apply_spectator_snapshot(&mut self, snapshot: &Snapshot, id: u32) -> bool {
        if !snapshot.spectators.iter().any(|s| s.net_id == id) { return false; }
        let viewer = self.players.get(MAX_PLAYERS).filter(|_| self.spectating).cloned();
        if !self.apply_shared(snapshot) { return false; }
        let viewer = viewer.unwrap_or_else(|| {
            let mut p = make_player(Team::Ember, false, self.stand(true), 0.0, BotRole::Offense);
            p.alive = false; p.remote = true;
            p
        });
        self.players.push(viewer);
        self.player_id = MAX_PLAYERS;
        self.spectating = true;
        self.kills = 0;
        self.deaths = 0;
        true
    }

    /// Everything in a snapshot but whose view it is.
    fn apply_shared(&mut self, snapshot: &Snapshot) -> bool {
        // Checked before anything changes: a refused snapshot leaves the world as it was.
        if snapshot.equipment.len()!=crate::equipment::definitions(snapshot.map).len() {return false;}
        if self.map != snapshot.map { self.set_map(snapshot.map); }
        self.equipment=snapshot.equipment.clone();
        self.feed=snapshot.feed.clone();
        self.network_inputs.clear();
        self.blast_serial = snapshot.blast_serial;
        self.players = snapshot.players.clone();
        for p in &mut self.players { p.remote = true; }
        self.discs = snapshot.discs.clone();
        self.explosions = snapshot.explosions.clone();
        self.smoke = snapshot.smoke.clone();
        self.flags = snapshot.flags.clone();
        self.mode = snapshot.mode;
        self.points = snapshot.points.clone();
        self.ball = snapshot.ball.clone();
        self.rabbit = snapshot.rabbit.clone();
        self.deployables = snapshot.deployables.clone();
        if let Some(p) = self.players.get(self.player_id) {
            self.waypoints[p.team.idx()] = snapshot.waypoint;
            self.credits[p.team.idx()] = snapshot.credits;
        }
        self.marks.clear();
        self.steps = snapshot.tick;
        self.moveable_open = snapshot.moveables.clone();
        self.vehicles = snapshot.vehicles.clone();
        self.map_item_clocks = snapshot.map_items.clone();
        self.loot = snapshot.loot.clone();
        self.conditions = snapshot.conditions;
        self.score = snapshot.score;
        self.time_left = snapshot.time_left;
        self.state = if snapshot.phase == Phase::Intermission { MatchState::Ended } else { MatchState::Playing };
        self.countdown = if snapshot.phase == Phase::Countdown { snapshot.phase_left.max(0.0) } else { 0.0 };
        self.message = match snapshot.phase {
            Phase::Waiting => "WARMUP — WAITING FOR AN OPPONENT".into(),
            // The HUD's big centred countdown says it.
            Phase::Countdown => String::new(),
            Phase::Intermission => format!("NEXT ROUND IN {:.0}", snapshot.phase_left.ceil()),
            Phase::Playing => String::new(),
        };
        true
    }

    /// Reuse the exact movement integrator, but never predict damage or flag outcomes.
    pub fn predict_command(&mut self, command: Command) {
        if self.state != MatchState::Playing || self.players.is_empty() { return; }
        let slot = self.player_id;
        if !self.players[slot].alive { return; }
        self.input = command.input();
        self.input.jump_prev = self.players[slot].jump_prev;
        self.players[slot].yaw = command.yaw;
        self.players[slot].pitch = command.pitch;
        self.players[slot].weapon = self.allowed_weapon(slot, command.weapon);
        let discs = self.discs.len();
        self.predicting = true;
        // The map's moving parts, as the server steps them (`features`).
        self.steps += 1;
        self.step_moveables(STEP);
        self.step_players(STEP);
        // The vehicle they fly, as the server flies it (`vehicles`).
        self.step_vehicles(STEP);
        self.predicting = false;
        self.players[slot].jump_prev = command.jump;
        self.discs.truncate(discs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_s_setup_outlasts_twists_rounds_and_map_changes() {
        use crate::{conditions::Twist, map_catalog::SupportedMode, sim::{arsenal::{Loadout, RAILGUN}, packs::PackKind, rifles::RIFLE_SLOT, Setup}};
        let mut game = Match::new(MapId::OldHoller);
        game.world.set_mode(SupportedMode::Ctf);
        let slot = game.join(41, "Setup").unwrap();
        game.world.players[slot].setup = Setup { armor: Some(ArmorClass::Heavy), weapons: Some(Loadout::of(&[0, 1, 2, RIFLE_SLOT])), pack: Some(PackKind::Field) };
        // A twist round ends: its forced kit goes, the player's own stays.
        for twist in [Twist::Heavies, Twist::Rifles] {
            game.world.conditions.twist = twist;
            game.world.players[slot].armor = ArmorClass::Heavy;
            game.world.players[slot].loadout.push(RAILGUN);
            game.restart();
            assert_eq!((game.world.players[slot].armor, game.world.players[slot].loadout.carries(RAILGUN)), (ArmorClass::Heavy, true), "{twist:?}");
        }
        // The next map: the same setup at the first spawn.
        game.rotate_to_mode(MapId::Frostline, SupportedMode::Ctf);
        let p = &game.world.players[slot];
        assert_eq!((p.setup.armor, p.armor, p.loadout.carries(RAILGUN), p.pack), (Some(ArmorClass::Heavy), ArmorClass::Heavy, true, Some(PackKind::Field)));
    }

    #[test]
    fn a_server_s_weather_lasts_across_rotations() {
        use crate::{conditions::{Conditions, Forecast, Pick, Twist}, map_catalog::SupportedMode};
        let mut game = Match::new(MapId::OldHoller);
        game.world.forecast = Forecast { time: Pick::Random, weather: Pick::Random };
        game.world.reseed(7);
        game.world.roll_conditions();
        let mut seen = std::collections::HashSet::new();
        for map in [MapId::OldHoller, MapId::OldHoller, MapId::OldHoller, MapId::OldHoller, MapId::OldHoller, MapId::OldHoller] {
            game.tick += 977;
            game.rotate_to_mode(map, SupportedMode::Ctf);
            assert_eq!(game.world.forecast.time, Pick::Random, "carried to the next map");
            assert_ne!(game.world.conditions, Conditions::default());
            assert_eq!(game.world.conditions.twist, Twist::None);
            seen.insert(format!("{:?}", game.world.conditions));
        }
        assert!(seen.len() > 1, "each map rolls again");
    }

    #[test]
    fn team_chat_is_filtered_before_serialization_and_shares_rate_limit() {
        let mut m = Match::new(MapId::Valley);
        let a = m.join(1,"Alice").unwrap();
        let enemy = m.join(2,"Enemy").unwrap();
        let friend = m.join(3,"Friend").unwrap();
        assert!(m.chat_channel(a,"Secret route",true));
        assert!(!m.chat(a,"Bypass cooldown"));
        assert_eq!(m.snapshot_for(a).feed.len(),1);
        assert_eq!(m.snapshot_for(friend).feed.len(),1);
        assert!(m.snapshot_for(enemy).feed.is_empty());
        assert!(m.snapshot_for(99).feed.is_empty());
        assert!(m.snapshot().feed.is_empty());
        m.tick += 60;
        assert!(!m.chat_channel(a,"\nBad",true));
        assert!(m.chat(a,"Public hello"));
        assert_eq!(m.snapshot_for(enemy).feed.len(),1);
        m.world.players[friend].team = m.world.players[enemy].team;
        assert_eq!(m.snapshot_for(friend).feed.len(),1);
    }

    #[test]
    fn a_cloaked_enemy_is_left_out_of_the_other_team_s_view() {
        let mut m = Match::new(MapId::Valley);
        let a = m.join(1, "Alice").unwrap();
        let enemy = m.join(2, "Enemy").unwrap();
        let friend = m.join(3, "Friend").unwrap();
        for i in [a, enemy, friend] { m.world.players[i].alive = true; }
        m.world.players[friend].team = m.world.players[a].team;
        m.world.players[enemy].pos = m.world.players[a].pos + Vec3::new(40.0, 0.0, 0.0);
        let cloak = crate::registry::registry().effects.index("cloaked").unwrap() as u8;
        assert!(m.world.give_effect(a, cloak));
        let at = m.world.players[a].pos;
        let seen = |m: &Match, viewer: usize| m.snapshot_for(viewer).players[a].pos;
        assert_eq!(seen(&m, a), at, "they see themselves");
        assert_eq!(seen(&m, friend), at, "and their team sees them");
        assert!(m.snapshot_for(enemy).players[a].hidden(), "the enemy doesn't");
        assert!(m.snapshot_for(99).players[a].hidden(), "nor spectators");
        // Close enough to see the shimmer.
        m.world.players[enemy].pos = at + Vec3::new(8.0, 0.0, 0.0);
        assert_eq!(seen(&m, enemy), at);
        // A flag carrier is always seen.
        m.world.players[enemy].pos = at + Vec3::new(40.0, 0.0, 0.0);
        m.world.players[a].carrying = Some(m.world.players[enemy].team);
        assert_eq!(seen(&m, enemy), at);
    }

    #[test]
    fn rename_is_validated_rate_limited_and_preserves_identity() {
        let mut m = Match::new(MapId::Valley);
        assert!(m.join(99, "Bad<script>").is_none());
        let a = m.join(1, "Alice").unwrap();
        let b = m.join(2, "Bob").unwrap();
        m.world.players[a].frags = 7;
        assert!(!m.rename(a, "\nAdmin"));
        assert!(!m.rename(99, "Admin"));
        assert!(m.rename(a, "  Pilot 42  "));
        assert_eq!(m.world.players[a].name, "Pilot 42");
        assert_eq!(m.world.players[a].net_id, 1);
        assert_eq!(m.world.players[a].frags, 7);
        assert_eq!(m.world.players[b].name, "Bob");
        assert!(!m.rename(a, "Spam"));
        m.tick += 600;
        assert!(m.rename(a, "New Pilot"));
        m.leave(a);
        assert!(!m.rename(a, "Ghost"));
        let reused = m.join(3, "Newcomer").unwrap();
        assert!(m.rename(reused, "Fresh"));
    }

    #[test]
    fn chat_is_authoritative_bounded_rate_limited_and_resets_when_empty() {
        let mut m = Match::new(MapId::Valley);
        let a = m.join(11, "Alice").unwrap();
        let b = m.join(12, "Bob").unwrap();
        assert!(!m.chat(7, "forged"));
        assert!(!m.chat(a, "bad\nmessage"));
        assert!(m.chat(a, "hello"));
        assert!(!m.chat(a, "spam"));
        assert!(m.chat(b, "ready"));
        m.tick += 60;
        assert!(m.chat(a, "go"));
        let mut client = World::new();
        for _ in 0..3 { assert!(client.apply_snapshot(&m.snapshot(), 11)); }
        assert_eq!(client.feed.len(), 3, "repeated snapshots must not duplicate events");
        assert_eq!(client.feed[0], crate::feed::Entry::Chat { sender: "Alice".into(), text: "hello".into() });
        m.leave(a); m.leave(b);
        assert!(m.world.feed.is_empty());
    }

    #[test]
    fn frags_capture_names_and_weapon_once_even_after_slot_reuse() {
        let mut m = duel();
        m.world.players[0].name = "Alice".into();
        m.world.players[1].name = "Bob".into();
        m.world.kill(1, Some(0), "Disc launcher");
        m.world.kill(1, Some(0), "Disc launcher");
        assert_eq!(m.world.feed.len(), 1);
        m.leave(1); m.join(3, "Charlie").unwrap();
        assert_eq!(m.world.feed[0], crate::feed::Entry::Frag {
            killer: "Alice".into(), victim: "Bob".into(), weapon: "Disc launcher".into() });
        m.world.kill(1, None, "Fall");
        assert!(m.world.feed[1].line().contains("Environment fragged Charlie · Fall"));
    }

    fn duel() -> Match {
        let mut game = Match::new(MapId::Valley);
        game.join(1, "Skier").unwrap();
        game.join(2, "Skier").unwrap();
        game.phase = Phase::Playing;
        game.round = 1;
        game
    }

    #[test]
    fn departure_balances_teams_without_resetting_active_match() {
        let mut game = duel();
        for id in 3..=6 { game.join(id, "Skier").unwrap(); }
        game.world.score = [2, 1];
        game.world.players[4].frags = 7;
        game.leave(1);
        game.leave(3);
        game.step(&[]);
        let count = |team| game.world.players.iter().filter(|p| p.net_id != 0 && p.team == team).count();
        assert_eq!((count(Team::Ember), count(Team::Glacier)), (2, 2));
        assert_eq!(game.world.players[4].team, Team::Glacier);
        assert_eq!(game.world.players[4].frags, 7);
        assert_eq!(game.world.score, [2, 1]);
        assert_eq!(game.phase, Phase::Playing);
    }

    #[test]
    fn balance_prefers_non_carriers_and_drops_flags_if_unavoidable() {
        let mut game = duel();
        game.join(3, "Carrier").unwrap();
        game.world.players[2].carrying = Some(Team::Glacier);
        game.world.flags[1].carrier = Some(2);
        game.leave(1);
        game.balance_teams();
        assert_eq!(game.world.players[0].team, Team::Glacier);
        assert_eq!(game.world.players[2].team, Team::Ember);
        assert_eq!(game.world.flags[1].carrier, Some(2));

        // Exercise the fallback independently of the normal one-flag limit.
        game.world.players[0].team = Team::Ember;
        game.world.players[0].carrying = Some(Team::Ember);
        game.world.flags[0].carrier = Some(0);
        game.balance_teams();
        assert_eq!(game.world.players[2].team, Team::Glacier);
        assert_eq!(game.world.flags[1].carrier, None);
        assert_eq!(game.world.players[2].carrying, None);
    }

    #[test]
    fn empty_server_resets_every_phase_and_keeps_health_clock_advancing() {
        for phase in [Phase::Waiting, Phase::Countdown, Phase::Playing, Phase::Intermission] {
            let mut game = duel();
            game.phase = phase;
            game.phase_left = 9.0;
            game.world.score = [2, 1];
            game.world.time_left = 12.0;
            game.tick = 999;
            game.acks[0] = 300;
            game.leave(0);
            game.leave(1);
            assert_eq!(game.phase, Phase::Waiting);
            assert_eq!(game.phase_left, 0.0);
            assert_eq!(game.round, 0);
            assert_eq!(game.world.score, [0, 0]);
            assert_eq!(game.world.time_left, crate::map_catalog::SupportedMode::Ctf.def().clock);
            assert!(game.acks.iter().all(|ack| *ack == 0));
            assert!(game.world.discs.is_empty() && game.world.smoke.is_empty() && game.world.explosions.is_empty());
            assert!(game.world.flags.iter().all(|f| f.carrier.is_none() && f.pos == f.home));
            game.step(&[]);
            assert_eq!(game.tick, 1000);
            game.join(7, "Fresh").unwrap();
            game.join(8, "Fresh").unwrap();
            game.step(&[]);
            assert_eq!(game.phase, Phase::Countdown);
        }
    }

    #[test]
    fn server_and_predictor_use_the_same_movement_and_controls() {
        for direction in [-1.0, 1.0] {
            let mut game = duel();
            let mut client = World::new();
            client.apply_snapshot(&game.snapshot(), 1);
            let origin = client.players[0].pos;
            for seq in 1..121 {
                let cmd = Command { seq, move_x: direction, move_z: 1.0, yaw: 0.0,
                    jet: seq < 70, jump: true, ..Command::default() };
                game.step(&[Some(cmd)]);
                client.predict_command(cmd);
                let a = &game.world.players[0]; let b = &client.players[0];
                assert!(a.pos.distance(b.pos) < 0.0001, "movement diverged at {seq}");
                assert!(a.vel.distance(b.vel) < 0.0001);
                assert!((a.energy - b.energy).abs() < 0.0001);
            }
            assert!((client.players[0].pos.x - origin.x) * direction > 1.0);
            assert!(client.players[0].pos.z < origin.z);
        }
    }

    #[test]
    fn all_weapons_damage_on_server_and_respawn_on_server() {
        for weapon in 0..3 {
            let mut game = duel();
            game.world.players[0].pos = Vec3::new(128.0, 150.0, 128.0);
            game.world.players[1].pos = Vec3::new(128.0, 150.0, 110.0);
            for p in &mut game.world.players[..2] { p.on_ground = false; p.health = 8.0; p.cooldown = 0.0; }
            // A direct impact for each projectile type, using the production collision path.
            game.world.discs.push(Disc { pos: Vec3::new(128.0, 150.7, 111.0),
                vel: Vec3::new(0.0, 0.0, -95.0), team: Team::Ember, owner: 0,
                life: if weapon == 2 { 1.5 } else { 1.0 }, kind: weapon, spin: 0.0, target: 0, power: 1.0 });
            game.step(&[]);
            assert!(!game.world.players[1].alive, "weapon {weapon} did not kill");
            assert_eq!(game.world.players[0].frags, 1);
            assert_eq!(game.world.players[1].losses, 1);
            let snap = game.snapshot();
            let mut a = World::new(); let mut b = World::new();
            assert!(a.apply_snapshot(&snap, 1)); assert!(b.apply_snapshot(&snap, 2));
            assert!(!a.players[1].alive && !b.players[1].alive);
            for _ in 0..210 { game.step(&[]); }
            assert!(game.world.players[1].alive);
            assert_eq!(game.world.players[1].health, 100.0);
        }
    }

    #[test]
    fn fire_rate_energy_and_invalid_inputs_are_not_client_outcomes() {
        let mut game = duel();
        game.world.players[0].cooldown = 0.0;
        for seq in 1..=120 {
            game.step(&[Some(Command { seq, fire: true, jet: true, ..Command::default() })]);
        }
        assert!(game.world.players[0].shots <= 2);
        assert!(game.world.players[0].energy < crate::defs::movement().energy_max);
        let invalid = [Command { seq: 1, move_x: 2.0, ..Command::default() },
            Command { seq: 1, yaw: f32::NAN, ..Command::default() },
            Command { seq: 1, weapon: crate::registry::registry().weapons.len() as u8, ..Command::default() },
            Command { seq: 1, buy: 7, ..Command::default() },
            Command { seq: 1, buy: crate::sim::loadout::BUY_TAKE + 99, ..Command::default() },
            Command { seq: 1, equip: Some(crate::sim::loadout::Equip { armor: 40, ..Default::default() }), ..Command::default() },
            // A gap in the slots.
            Command { seq: 1, equip: Some(crate::sim::loadout::Equip { weapons: serde_json::from_str("[0,255,1,255,255,255,255,255]").unwrap(),
                ..Default::default() }), ..Command::default() }];
        for c in invalid { assert!(!c.valid()); }
        assert!(serde_json::from_str::<Command>(r#"{"seq":1,"position":[999,999,999]}"#).is_err());
    }

    #[test]
    fn flag_capture_disconnect_and_round_restart_are_shared() {
        let mut game = duel();
        game.world.players[0].pos = game.world.flags[1].pos;
        game.step(&[]);
        assert_eq!(game.world.flags[1].carrier, Some(0));
        game.leave(0);
        assert_eq!(game.world.flags[1].carrier, None);
        assert!(game.world.flags[1].drop_timer > 0.0);
        assert_eq!(game.join(3, "New player"), Some(0));
        for _ in 0..3 {
            game.world.players[0].pos = game.world.flags[1].pos;
            game.step(&[]);
            game.world.players[0].pos = game.world.flags[0].home;
            game.step(&[]);
        }
        assert_eq!(game.world.score, [3, 0]);
        assert_eq!(game.phase, Phase::Intermission);
        let round = game.round;
        for _ in 0..605 { game.step(&[]); }
        assert_eq!(game.phase, Phase::Countdown);
        assert_eq!(game.round, round + 1);
        assert_eq!(game.world.score, [0, 0]);
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize { game.step(&[]); }
        assert_eq!(game.phase, Phase::Playing);
        assert_eq!(game.round, round + 1);
        assert_eq!(game.world.players[0].net_id, 3);
        game.world.time_left = STEP * 0.5;
        game.step(&[]);
        assert_eq!(game.phase, Phase::Intermission);
    }

    #[test]
    fn late_join_snapshot_keeps_airborne_height_flags_and_health() {
        let mut game = duel();
        game.world.players[0].pos.y += 40.0;
        game.world.players[0].health = 37.0;
        game.world.flags[1].carrier = Some(0);
        game.world.players[0].carrying = Some(Team::Glacier);
        let slot = game.join(3, "Late").unwrap();
        assert_eq!(slot, 2);
        let mut client = World::new();
        client.apply_snapshot(&game.snapshot(), 3);
        assert_eq!(client.player_id, 2);
        assert_eq!(client.players[0].pos, game.world.players[0].pos);
        assert_eq!(client.players[0].health, 37.0);
        assert_eq!(client.flags[1].carrier, Some(0));
    }

    #[test]
    fn abandoned_match_returns_to_warmup_and_requires_new_countdown() {
        let mut game = duel();
        game.world.score = [2, 1];
        game.leave(1);
        game.step(&[]);
        assert_eq!(game.phase, Phase::Waiting);
        assert_eq!(game.world.score, [0, 0]);
        game.join(3, "Replacement").unwrap();
        game.step(&[]);
        assert_eq!(game.phase, Phase::Countdown);
    }

    #[test]
    fn a_host_alone_starts_the_match_and_it_plays_on_until_everyone_leaves() {
        let mut game = Match::new(MapId::Valley);
        assert_eq!(game.host_start(), Err("Join a team to start the match"), "nobody to play");
        let slot = game.join(1, "Host").unwrap();
        for _ in 0..30 { game.step(&[]); }
        assert_eq!(game.phase, Phase::Waiting, "alone, a match waits for the other team");
        game.host_start().unwrap();
        assert_eq!(game.phase, Phase::Countdown);
        assert_eq!(game.host_start(), Err("The match has already started"));
        for _ in 0..((ROUND_COUNTDOWN / STEP) as usize + 2) { game.step(&[]); }
        assert_eq!(game.phase, Phase::Playing, "the countdown runs with one team empty");
        for _ in 0..30 { game.step(&[]); }
        assert_eq!(game.phase, Phase::Playing, "and play goes on alone");
        // A map change keeps the host's start.
        game.phase = Phase::Intermission;
        game.rotate_to_mode(MapId::Valley, crate::map_catalog::SupportedMode::Ctf);
        assert!(game.host_started);
        for _ in 0..((ROUND_COUNTDOWN / STEP) as usize + 2) { game.step(&[]); }
        assert_eq!(game.phase, Phase::Playing, "the next map starts without asking again");
        // Once everyone has gone, the next player waits for a start again.
        game.leave(slot);
        game.step(&[]);
        assert_eq!(game.phase, Phase::Waiting);
        assert!(!game.host_started);
        game.join(2, "Later").unwrap();
        for _ in 0..30 { game.step(&[]); }
        assert_eq!(game.phase, Phase::Waiting);
    }
}

/// "Reefbreak · Team Deathmatch".
pub fn label(map: MapId, mode: crate::map_catalog::SupportedMode) -> String {
    format!("{} · {}", crate::terrain::info(map).name, mode.label())
}
