use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Vec2};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use eframe::egui_wgpu::Callback;
use serde::Deserialize;

use crate::audio::Audio;
use crate::drawlist::build_frame_with;
use crate::mouse;
use crate::scene::{self, SceneCallback};
use crate::sim::{MatchState, World};
use crate::terrain::{self, MapId};

const EMBER: Color32 = Color32::from_rgb(226, 74, 50);
const GLACIER: Color32 = Color32::from_rgb(62, 200, 224);
const FG: Color32 = Color32::from_rgb(231, 238, 246);
const MUTED: Color32 = Color32::from_rgb(154, 168, 184);
const BG: Color32 = Color32::from_rgb(8, 13, 20);
/// Map votes: bright enough to spot mid-fight.
const VOTE_AMBER: Color32 = Color32::from_rgb(255, 196, 60);
const SECONDARY_BUTTON_BG: Color32 = Color32::from_rgb(22, 28, 38);

/// The main menu's pages: Home (Singleplayer, Multiplayer, Library), the
/// offline match setup, the online choices, and the Library (a topic open).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MenuPage { #[default] Home, Single, Multi, Library(Option<usize>) }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Menu,
    Play,
    Pause,
    End,
    Browser,
    Lobby,
    /// Host a match: the separate server app, started from here.
    Host,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hud {
    health: f32,
    energy: f32,
    speed: f32,
    ember: u32,
    glacier: u32,
    time: f32,
    state: u8,
    weapon: u8,
    flag: i32,
    own_flag: i32,
    /// The enemy flag: 1 home, 2 carried, 3 dropped.
    #[serde(default)]
    enemy_flag: i32,
    hit: f32,
    flash: f32,
    msg: String,
    kills: u32,
    deaths: u32,
    winner: i32,
    team: u32,
    ski: u8,
    jet: u8,
    alive: u8,
    events: String,
    #[serde(default)]
    armor: u8,
    #[serde(default)]
    ammo: u16,
    #[serde(default)]
    ammo_max: u16,
    #[serde(default)]
    repairing: u8,
    #[serde(default)]
    station: u8,
    #[serde(default)]
    grenades: u8,
    #[serde(default)]
    mines: u8,
    #[serde(default)]
    grenades_max: u8,
    #[serde(default)]
    mines_max: u8,
    /// Objective caption under the score, from the world's mode.
    #[serde(skip)]
    goal: &'static str,
    /// Football: no weapons; the bottom line shows the ball instead.
    #[serde(skip)]
    football: bool,
    /// Rabbit: personal score is points, not frags.
    #[serde(skip)]
    points: bool,
    /// Holding a weapon the mode allows (false in a mode without weapons).
    #[serde(skip)]
    armed: bool,
    /// The weapons carried, slot 1 first, and which is in hand.
    #[serde(skip)]
    slots: Vec<&'static str>,
    #[serde(skip)]
    held: Option<usize>,
    /// What the grenade and mine keys throw (names; empty: nothing).
    #[serde(skip)]
    throws: [&'static str; 2],
    /// The held weapon's name in its fire mode, and its trigger's state
    /// (spinning up, charging, locking on).
    #[serde(skip)]
    held_name: &'static str,
    #[serde(skip)]
    trigger: String,
    /// Status effects on the local player (name, seconds left), and the
    /// screen tint they add up to.
    #[serde(skip)]
    effects: Vec<(&'static str, f32)>,
    #[serde(skip)]
    tint: [f32; 4],
    /// Current key bindings, for hints.
    #[serde(skip)]
    keys: crate::keybinds::Keybinds,
    /// A grenade or mine being wound up (what, seconds held).
    #[serde(skip)]
    winding: Option<(u8, f32)>,
    /// Placing a pack: why it can't go where you aim ("" when it can).
    #[serde(skip)]
    placing: Option<&'static str>,
    /// The pack worn and what its key does now ("Shield pack · on"), and
    /// the armor's class key likewise.
    #[serde(skip)]
    pack_line: String,
    #[serde(skip)]
    ability_line: String,
    /// The command view's line (cameras, turrets, waypoint).
    #[serde(skip)]
    command_line: String,
    /// Credits to spend, where the mode has an economy.
    #[serde(skip)]
    credits: Option<f32>,
    /// The vehicle ridden: its name, hull, energy and the seat.
    #[serde(skip)]
    vehicle_line: String,
    /// Looking through a rifle's zoom, and how many times nearer it brings
    /// things (`World::zoom_power`).
    #[serde(skip)]
    zoomed: bool,
    #[serde(skip)]
    zoom_power: f32,
    /// The held weapon's elevation gauge (`elevation`, the mortar's): the
    /// aim's angle above the horizon, in degrees.
    #[serde(skip)]
    elevation: Option<f32>,
    /// In the third-person view, where the shot will land on screen
    /// (`third_person_aim`): the crosshair goes there instead of the center.
    #[serde(skip)]
    aim: Option<[f32; 2]>,
    /// Deathmatch: the score line ("You 4 · Leader Ace 7"), replacing the
    /// team scores.
    #[serde(skip)]
    ffa_line: Option<String>,
    /// No clock beside the score line (a duel server's lobby).
    #[serde(skip)]
    clockless: bool,
    /// Deathmatch modes: this round's time, weather and twist.
    #[serde(skip)]
    conditions: String,
    /// The round countdown's number or GO, and the map and mode under it.
    #[serde(skip)]
    countdown: Option<String>,
    #[serde(skip)]
    countdown_caption: String,
    /// In a blade zone (`sim::blade`): stamina, a power swing's charge, the
    /// guard, reeling, and the zone's name (a tower floor's).
    #[serde(skip)]
    blade: Option<BladeHud>,
    /// The mode gives a repair tool (duels don't).
    #[serde(skip)]
    repair_tool: bool,
    /// Where you are (or fell) is a blade zone: the weapon line reads Blade.
    #[serde(skip)]
    blade_area: bool,
}

#[derive(Clone, Debug, Default)]
struct BladeHud { stamina: f32, charge: Option<f32>, guard: bool, reel: bool, place: &'static str }

/// Keyboard bindings and the screens that use them.
#[cfg(not(target_arch = "wasm32"))]
impl Controls {
    /// Show the session's newest server notice for a few seconds (once).
    fn take_notice(&mut self, lobby: &peakrunner_net::Lobby, now: f64) {
        if lobby.notices == self.notices_seen { return; }
        self.notices_seen = lobby.notices;
        if let Some(text) = lobby.notice.clone() { self.notice = Some((text, now + NOTICE_SECONDS)); }
    }
}

/// How long a server notice stays on screen.
#[cfg(not(target_arch = "wasm32"))]
const NOTICE_SECONDS: f64 = 5.0;

#[derive(Default)]
struct Controls {
    keys: crate::keybinds::Keybinds,
    /// Looking through the team's cameras and turrets from a command
    /// station: which of them (`World::viewable`), in order.
    command: Option<usize>,
    /// The command view's line on the HUD.
    command_line: String,
    /// The inventory screen is open (at your team's inventory station).
    shop_open: bool,
    /// The map-vote panel is open (V, online only).
    vote_open: bool,
    /// Grenade/mine presses not yet thrown, and the pause before the next.
    throws: std::collections::VecDeque<u8>,
    throw_wait: f32,
    /// Whether a vote was running last frame (its start plays a cue).
    vote_seen: bool,
    /// A grenade or mine being wound up: what, and seconds held.
    winding: Option<(u8, f32)>,
    /// Lining up the carried pack (the deployer replaces the weapon), its
    /// turn from your facing, the wheel travel not yet turned, and whether
    /// fire was down last frame (a click places).
    placing: bool,
    turn: f32,
    wheel: f32,
    fire_was: bool,
    /// The controls screen is open, and the action waiting for a key.
    keys_open: bool,
    /// The Graphics screen is open.
    graphics_open: bool,
    rebinding: Option<crate::keybinds::Action>,
    /// While placing: why the hologram can't go down ("" when it can).
    ghost_problem: Option<&'static str>,
    /// Stood at the inventory station last frame (it opens on arrival), and
    /// the third-person view.
    was_at_inventory: bool,
    third_person: bool,
    /// Seconds alive since the last spawn.
    alive_for: f32,
    /// The spectator camera, while watching online (`teams1`).
    #[cfg(not(target_arch = "wasm32"))]
    spectator: crate::spectate::SpectatorCam,
    /// Fire and jet last frame (spectators cycle players on presses).
    #[cfg(not(target_arch = "wasm32"))]
    spectator_buttons: (bool, bool),
    /// The server's latest notice (e.g. why a team switch was refused),
    /// shown until the given UI time; and the session's count seen so far.
    notice: Option<(String, f64)>,
    #[cfg(not(target_arch = "wasm32"))]
    notices_seen: u32,
}

struct Pad {
    x: f32,
    z: f32,
    lx: f32,
    ly: f32,
    jump: bool,
    fire: bool,
    jet: bool,
}

pub struct PeakRunnerApp {
    overlay: crate::world_overlay::OverlayState,
    /// The mini-map's and the full map's terrain picture, and whether the
    /// full map is open (`map_view`).
    map_view: crate::map_view::MapView,
    effects: crate::effects::Effects,
    announcer: crate::flag_announce::Announcer,
    countdown: crate::countdown_hud::CountdownHud,
    generator_watch: crate::generator_announce::GeneratorWatch,
    point_watch: crate::control_hud::PointWatch,
    football_watch: crate::football_hud::FootballWatch,
    rabbit_watch: crate::rabbit_hud::RabbitWatch,
    game_mode: peakrunner_core::map_catalog::SupportedMode,
    /// Which main-menu page is open.
    page: MenuPage,
    /// Library pictures, decoded once.
    pictures: crate::library::Pictures,
    chat_team: bool,
    chat_open: bool,
    chat_text: String,
    chat_error: String,
    chat_next: f64,
    world: World,
    audio: Audio,
    mode: Mode,
    ember: bool,
    map: MapId,
    hud: Option<Hud>,
    frame_aspect: f32,
    stick: [f32; 2],
    touch: bool,
    grabbed: bool,
    touch_jump: bool,
    touch_jet: bool,
    touch_fire: bool,
    touch_interact: bool,
    touch_swap: bool,
    look_pending: egui::Vec2,
    /// Even frame times (see frame_pacer.rs) and the F3 frame-timing overlay.
    pacer: crate::frame_pacer::FramePacer,
    frame_stats: crate::frame_pacer::FrameStats,
    show_frame_stats: bool,
    /// The kill feed: frags and announcements with when they arrived (s),
    /// and the feed as last seen, to spot new entries.
    kill_feed: Vec<(peakrunner_core::feed::Entry, f64)>,
    feed_seen: Vec<peakrunner_core::feed::Entry>,
    /// Seconds since the last grab request (the browser's lock arrives late).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    grab_age: f32,
    /// The window's fullscreen state last frame, to notice the system's own toggle.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    seen_fullscreen: Option<bool>,
    frame_dt: f32,
    /// The click that started the match is still down. Don't treat it as fire.
    wait_fire_release: bool,
    /// Key bindings, the inventory screen and the grenade/mine wind-up.
    ctl: Controls,
    #[cfg(not(target_arch = "wasm32"))]
    pads: Option<gilrs::Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    net: NetUi,
}

impl PeakRunnerApp {
    /// Whether F3 this frame toggles the frame-timing overlay: not at an open
    /// inventory station, where it's favorite 3 (`shop_ui`; it flipped the
    /// overlay too, and saved that).
    fn frame_stats_key(&self, ctx: &egui::Context) -> bool {
        !self.ctl.shop_open && ctx.input(|i| i.key_pressed(egui::Key::F3))
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn comms_fixture() -> Self {
        let mut world = World::new(); world.start_rift(true);
        world.feed = vec![
            peakrunner_core::feed::Entry::Chat { sender: "Ridge".into(), text: "Two incoming at the west entrance.".into() },
            peakrunner_core::feed::Entry::Frag { killer: "Ridge".into(), victim: "Echo".into(), weapon: "Disc launcher".into() },
            peakrunner_core::feed::Entry::Frag { killer: "Nova".into(), victim: "Ridge".into(), weapon: "Grenade launcher".into() },
            peakrunner_core::feed::Entry::Chat { sender: "Echo".into(), text: "On my way. Cover the flag!".into() },
        ];
        Self { overlay: Default::default(), map_view: Default::default(), effects: Default::default(), announcer: Default::default(), countdown: Default::default(), generator_watch: Default::default(), point_watch: Default::default(), football_watch: Default::default(), rabbit_watch: Default::default(), game_mode: Default::default(), page: Default::default(), pictures: Default::default(), chat_team:false, world, audio: Audio::silent(), mode: Mode::Play, ember: true, map: MapId::Valley,
            hud: None, frame_aspect: 1.6, stick: [0.;2], touch: false, grabbed: false,
            touch_jump: false, touch_jet: false, touch_fire: false, touch_interact: false,
            touch_swap: false, look_pending: Vec2::ZERO, wait_fire_release: false, ctl: Controls::default(),
            pacer: Default::default(), frame_stats: Default::default(), show_frame_stats: false, kill_feed: Vec::new(), feed_seen: Vec::new(), grab_age: 0.0, seen_fullscreen: None, frame_dt: 1.0 / 60.0,
            pads: None, net: NetUi::new(), chat_open: true, chat_text: "Nice shot!".into(), chat_error: String::new(), chat_next: 0.0 }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn capture_comms(&mut self, ctx: &egui::Context) { style_ui(ctx); self.chat_ui(ctx); }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        scene::install(cc)?;
        style_ui(&cc.egui_ctx);
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut app = Self {
            overlay: Default::default(),
            map_view: Default::default(),
            effects: Default::default(),
            announcer: Default::default(),
            countdown: Default::default(),
            generator_watch: Default::default(),
            point_watch: Default::default(),
            football_watch: Default::default(),
            rabbit_watch: Default::default(),
            game_mode: Default::default(), page: Default::default(), pictures: Default::default(),
            chat_team: false,
            chat_open: false, chat_text: String::new(), chat_error: String::new(), chat_next: 0.0,
            world: {
                let mut world = World::new();
                world.set_map(MapId::OldHoller);
                world
            },
            audio: Audio::new(),
            mode: Mode::Menu,
            ember: true,
            map: MapId::OldHoller,
            hud: None,
            frame_aspect: 16.0 / 9.0,
            stick: [0.0, 0.0],
            touch: false,
            grabbed: false,
            touch_jump: false,
            touch_jet: false,
            touch_fire: false,
            touch_interact: false,
            touch_swap: false,
            look_pending: egui::Vec2::ZERO,
            pacer: Default::default(), frame_stats: Default::default(), show_frame_stats: false, kill_feed: Vec::new(), feed_seen: Vec::new(), grab_age: 0.0, seen_fullscreen: None, frame_dt: 1.0 / 60.0,
            wait_fire_release: false,
            ctl: Controls::default(),
            #[cfg(not(target_arch = "wasm32"))]
            pads: gilrs::Gilrs::new().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            net: NetUi::load(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        app.world.set_bot_difficulty(app.net.preferences.saved.bot_difficulty);
        #[cfg(not(target_arch = "wasm32"))]
        { app.ctl.keys = app.net.preferences.saved.keys.clone(); }
        #[cfg(not(target_arch = "wasm32"))]
        { app.show_frame_stats = app.net.preferences.saved.graphics.show_fps; }
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(address) = std::env::var("PEAKRUNNER_JOIN") {
            if let Ok(name) = std::env::var("PEAKRUNNER_NAME") { app.net.name = name; }
            app.net.password = std::env::var("PEAKRUNNER_MATCH_PASSWORD").unwrap_or_default();
            app.net.spectate = std::env::var_os("PEAKRUNNER_SPECTATE").is_some();
            if encrypted_address(&address) {
                app.join_server(address.trim(), 7777);
            } else if let Some((host, port)) = address.rsplit_once(':').and_then(|(h, p)| p.parse::<u16>().ok().map(|p| (h, p))) {
                app.join_server(host, port);
            }
        }
        Ok(app)
    }

    fn step(&mut self, ctx: &egui::Context, dt: f32) {
        if ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let rect = ctx.content_rect();
        if rect.height() > 1.0 {
            self.frame_aspect = rect.width() / rect.height();
        }
        if ctx.input(|i| i.any_touches()) {
            self.touch = true;
        }
        self.world.events.clear();
        self.world.spatial_sounds.clear();
        self.poll_net(ctx);
        if !self.online() { self.world.spectating = false; self.world.spectator_camera = None; }

        let was_chat = self.chat_open;
        let keys = self.ctl.keys.clone();
        use crate::keybinds::Action;
        if self.mode != Mode::Play { self.chat_open = false; }
        if self.mode == Mode::Play && !self.chat_open && !self.ctl.shop_open {
            let public = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, keys.get(Action::ChatPublic)));
            let team = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, keys.get(Action::ChatTeam)));
            if public || team {
                self.chat_open = true;
                // Spectators have no team channel: Y opens public chat.
                self.chat_team = team && !self.world.spectating;
                self.chat_error.clear();
                // Do not insert the opening hotkey's text event into the message.
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e,egui::Event::Text(_))));
            }
        }
        if self.chat_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.chat_open = false;
            self.wait_fire_release = true;
        }
        // The inventory screen closes on Escape or the use key, when you step
        // off the station, or die.
        // A vehicle station opens its own screen (`vehicles`).
        let riding = self.world.players.get(self.world.player_id).is_some_and(|p| p.riding > 0);
        let at_vehicles = !riding && self.world.vehicle_station(self.world.player_id).is_some();
        let at_inventory = self.hud.as_ref().is_some_and(|h| (h.station == 1 || h.station == 3) && h.alive == 1)
            || std::env::var_os("QA_SHOP").is_some() || at_vehicles;
        if self.ctl.shop_open {
            let close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || i.consume_key(egui::Modifiers::NONE, keys.get(Action::Use)));
            if close || self.mode != Mode::Play || !at_inventory { self.close_shop(); }
        }
        // The vote key opens the map-vote panel online; Escape or the key closes it.
        if self.ctl.vote_open {
            let close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || i.consume_key(egui::Modifiers::NONE, keys.get(Action::Vote)));
            if close || self.mode != Mode::Play || !self.online() { self.close_vote(); }
        } else if self.mode == Mode::Play && !self.chat_open && !self.ctl.shop_open && self.online() && (!self.world.spectating || self.duel_server())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, keys.get(Action::Vote))) {
            // On a duel server the key opens the duel board instead.
            self.ctl.vote_open = true;
        }
        // The team key: play for the other side, or (spectating) join a team.
        #[cfg(not(target_arch = "wasm32"))]
        if self.mode == Mode::Play && !self.chat_open && !self.ctl.shop_open && !self.ctl.vote_open && self.online()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, keys.get(Action::Team))) {
            // Watching a duel, the team key goes to the next one.
            match &self.net.session {
                Some(session) if self.world.spectating && crate::duel_hud::arena(&self.world) => { session.watch(0); }
                _ => self.request_team_switch(),
            }
        }
        // A vote starting is announced with a cue (the banner does the rest).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let running = self.net.lobby.snapshot.as_ref().is_some_and(|s| s.vote.as_ref().is_some_and(|v| v.passed.is_none()));
            if running && !self.ctl.vote_seen { self.world.events.push_str("vote,"); }
            self.ctl.vote_seen = running;
        }
        let gameplay_input = self.mode == Mode::Play && !self.chat_open && !was_chat && !self.ctl.shop_open
            && !self.ctl.vote_open && ctx.input(|i| i.focused);
        // While a key is being rebound, Escape cancels that instead.
        // In the browser Esc releases the pointer lock itself (the key may
        // never reach the page): a lock lost while playing pauses the game.
        #[cfg(target_arch = "wasm32")]
        {
            self.grab_age += ctx.input(|i| i.unstable_dt).min(0.1);
            if self.grabbed && self.mode == Mode::Play && self.grab_age > 1.0 && !web_pointer_locked() {
                self.pause(ctx);
            }
        }
        let escape = self.ctl.rebinding.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape && self.mode == Mode::Play {
            self.pause(ctx);
        } else if escape && self.mode == Mode::Pause {
            self.resume(ctx);
        }

        if self.look_pending != egui::Vec2::ZERO {
            if gameplay_input { self.world.add_look(self.look_pending.x, self.look_pending.y); }
            self.look_pending = egui::Vec2::ZERO;
        }
        let mouse = mouse::sample(ctx, gameplay_input);
        if gameplay_input {
            self.grab(ctx, true);
            self.world.add_look(mouse.dx, mouse.dy);
        } else {
            self.grab(ctx, false);
        }
        // The weapons carried, slot 1 first (`arsenal`).
        let id = self.world.player_id;
        let loadout = self.world.players.get(id).map(|p| p.loadout).unwrap_or_default();
        if self.touch_swap && gameplay_input {
            // The next carried weapon the mode allows, in slot order.
            let usable: Vec<u8> = loadout.kinds().filter(|&k| self.world.players.get(id).is_some() && self.world.allowed_weapon(id, k) == k).collect();
            let at = usable.iter().position(|&k| k == self.world.input.weapon);
            if let Some(&next) = usable.get(at.map_or(0, |n| (n + 1) % usable.len().max(1))) { self.world.input.weapon = next; }
            self.touch_swap = false;
        }

        let pad = self.poll_pad();
        let mut mx = self.stick[0] + pad.x;
        let mut mz = self.stick[1] + pad.z;
        ctx.input(|i| {
            if !gameplay_input { return; }
            if keys.down(i, Action::Left) || i.key_down(egui::Key::ArrowLeft) { mx -= 1.0; }
            if keys.down(i, Action::Right) || i.key_down(egui::Key::ArrowRight) { mx += 1.0; }
            if keys.down(i, Action::Forward) || i.key_down(egui::Key::ArrowUp) { mz += 1.0; }
            if keys.down(i, Action::Back) || i.key_down(egui::Key::ArrowDown) { mz -= 1.0; }
            // Number keys pick slot 1, 2, 3…, whatever is in it.
            for (n, action) in Action::SLOTS.into_iter().enumerate() {
                if keys.pressed(i, action) { if let Some(kind) = loadout.slot(n) { self.world.input.weapon = kind; } }
            }
        });
        let jump = (gameplay_input && ctx.input(|i| keys.down(i, Action::Jump))) || pad.jump || self.touch_jump;
        let jet = mouse.jet || pad.jet || self.touch_jet;
        let mut mouse_fire = self.mode == Mode::Play && mouse.fire;
        if self.wait_fire_release {
            if mouse_fire {
                mouse_fire = false;
            } else {
                self.wait_fire_release = false;
            }
        }
        let fire = mouse_fire || pad.fire || self.touch_fire || (gameplay_input && std::env::var_os("QA_FIRE").is_some());
        let play = self.mode == Mode::Play;
        self.world.input.move_x = mx.clamp(-1.0, 1.0);
        self.world.input.move_z = mz.clamp(-1.0, 1.0);
        self.world.input.jump = jump;
        self.world.input.jet = jet && play;
        self.world.input.fire = fire;
        let use_pressed = gameplay_input && ctx.input(|i| keys.pressed(i, Action::Use));
        let at_command = play && self.world.at_command(self.world.player_id);
        if !at_command { self.ctl.command = None; }
        // Stepping onto your inventory station opens its screen; after
        // closing it, the use key opens it again.
        // Not on a respawn beside one: only after a second alive.
        let alive = self.hud.as_ref().is_some_and(|h| h.alive == 1);
        self.ctl.alive_for = if alive { self.ctl.alive_for+dt } else { 0.0 };
        if at_inventory && !self.ctl.was_at_inventory && self.ctl.alive_for > 1.0 && gameplay_input && !self.world.ball.active {
            self.ctl.shop_open = true;
        }
        self.ctl.was_at_inventory = at_inventory || self.ctl.alive_for <= 1.0;
        // Away from a station, holding the use key zooms a rifle.
        let at_station = self.hud.as_ref().is_some_and(|h| h.station != 0);
        self.world.zoomed = gameplay_input && !at_station && !at_command && !self.ctl.placing
            && (ctx.input(|i| keys.down(i, Action::Use)) || std::env::var_os("QA_ZOOM").is_some());
        // Through a sight, the wheel picks its level: up for the nearer one,
        // down for the first (kept for the next time you zoom).
        if self.world.zoom_active() {
            self.ctl.wheel += ctx.input(|i| i.smooth_scroll_delta.y);
            while self.ctl.wheel.abs() >= 30.0 {
                let dir = self.ctl.wheel.signum();
                self.world.zoom_level = if dir > 0.0 { 1 } else { 0 };
                self.ctl.wheel -= 30.0 * dir;
            }
        }
        // The view key: third person, unless it's turning a pack being placed
        // or moving to the next camera.
        // The map key opens or closes the full map; play goes on under it.
        if gameplay_input && ctx.input(|i| keys.pressed(i, Action::Map)) { self.map_view.full = !self.map_view.full; }
        if crate::qa_overrides::full_map() { self.map_view.full = true; }
        if gameplay_input && !self.ctl.placing && self.ctl.command.is_none() && !self.world.spectating && ctx.input(|i| keys.pressed(i, Action::View)) {
            self.ctl.third_person = !self.ctl.third_person;
        }
        self.world.third_person = self.ctl.third_person;
        self.world.input.interact = play && (ctx.input(|i| keys.down(i, Action::Use)) || self.touch_interact);
        // Use at your inventory station opens the inventory screen; riding a
        // vehicle, or beside one, it boards or leaves (`vehicles`).
        let me_at = self.world.players.get(self.world.player_id).map(|p| p.pos);
        let near_craft = me_at.is_some_and(|at| self.world.vehicles.iter().any(|c| c.pos.distance(at) <= c.radius() + 2.5));
        if use_pressed && (riding || (near_craft && !at_inventory && !at_command)) { self.world.input.board = true; }
        else if use_pressed && at_inventory && !self.world.ball.active { self.ctl.shop_open = true; }
        // At a command station (`placed`), the use key looks through the
        // team's cameras and turrets: the view key moves to the next, fire
        // fires an operated turret, the deploy key sets the team's waypoint
        // where you look, and the use key again (or stepping off) leaves.
        if use_pressed && at_command && !at_inventory {
            self.ctl.command = if self.ctl.command.is_some() { None } else { Some(0) };
        }
        self.world.view_from = None;
        self.world.input.operate = 0;
        self.ctl.command_line = String::new();
        if let Some(n) = self.ctl.command {
            let me = self.world.player_id;
            let team = self.world.players[me].team;
            let units = self.world.viewable(team);
            let mut n = n;
            if gameplay_input && ctx.input(|i| keys.pressed(i, Action::View)) && !units.is_empty() { n = (n + 1) % units.len(); }
            self.ctl.command = Some(n);
            // Standing still at the station.
            self.world.input.move_x = 0.0;
            self.world.input.move_z = 0.0;
            self.world.input.jump = false;
            self.world.input.jet = false;
            let leave = keys.name(Action::Use);
            match units.get(n.min(units.len().saturating_sub(1))).copied() {
                Some(k) => {
                    let e = &self.world.deployables[k];
                    let turret = e.kind.behavior() == peakrunner_core::registry::Behavior::Turret;
                    let eye = if turret { e.center() + glam::Vec3::Y * 0.7 } else { e.pos + glam::Vec3::Y * (crate::sim::deploy::POST_HEIGHT + 0.2) };
                    self.world.view_from = Some(eye);
                    if turret { self.world.input.operate = k as u16 + 1; }
                    if gameplay_input && ctx.input(|i| keys.pressed(i, Action::Deploy)) {
                        let (_, look, _) = self.world.camera();
                        let far = eye + look * 1500.0;
                        if let Some(t) = self.world.blocked(eye, far, 0.0, None) { self.world.input.waypoint = Some(eye.lerp(far, t)); }
                    }
                    self.ctl.command_line = format!("COMMAND · {} {} of {} · {} next · {} waypoint · {leave} leave{}", e.kind.name(), n + 1, units.len(),
                        keys.name(Action::View), keys.name(Action::Deploy), if turret { " · fire to shoot" } else { "" });
                }
                None => self.ctl.command_line = format!("COMMAND · your team has no cameras or turrets · {leave} leave"),
            }
        }
        self.world.input.repair = play && ctx.input(|i| keys.down(i, Action::Repair));
        // One-shot intents latch until a tick sends them (World::tick and the
        // online predictor clear them), so a press is never lost.
        // Deploying: the deploy key brings up the deployer and a hologram of
        // the pack where you aim (see `World::aim_placement`); click places
        // it, the wheel or the rotate key turns it, and the deploy key, a
        // weapon key or the repair tool puts the deployer away.
        // A pack that doesn't place is switched or used by the key at once.
        let pack = self.world.players.get(self.world.player_id).filter(|p| p.alive).and_then(|p| p.pack).and_then(|k| k.deployable());
        let worn = self.world.players.get(self.world.player_id).filter(|p| p.alive).and_then(|p| p.pack).is_some();
        if gameplay_input && ctx.input(|i| keys.pressed(i, Action::Deploy)) && !self.world.ball.active && self.ctl.command.is_none() {
            if pack.is_some() {
                self.ctl.placing = !self.ctl.placing;
                self.ctl.turn = 0.0;
            } else if worn { self.world.input.deploy = true; }
        }
        if self.ctl.placing {
            let leave = pack.is_none() || !play || self.world.input.repair
                || ctx.input(|i| Action::SLOTS.iter().any(|&a| keys.pressed(i, a)));
            if leave {
                self.ctl.placing = false;
                // A click still held from placing isn't a shot.
                self.wait_fire_release = true;
            } else if gameplay_input {
                let step = std::f32::consts::PI / 12.0;
                if ctx.input(|i| keys.pressed(i, Action::View)) { self.ctl.turn += step; }
                self.ctl.wheel += ctx.input(|i| i.smooth_scroll_delta.y);
                while self.ctl.wheel.abs() >= 30.0 {
                    let dir = self.ctl.wheel.signum();
                    self.ctl.turn += step * dir;
                    self.ctl.wheel -= 30.0 * dir;
                }
                self.ctl.turn = (self.ctl.turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                if fire && !self.ctl.fire_was {
                    self.world.input.deploy = true;
                    self.world.input.deploy_turn = self.ctl.turn;
                }
            }
            // The deployer doesn't shoot.
            self.world.input.fire = false;
        }
        self.ctl.fire_was = fire;
        // Ctrl+K: suicide. One press, one death: holding the
        // keys doesn't kill you again when you respawn.
        self.world.input.suicide |= play && ctx.input(|i| i.modifiers.ctrl && keys.pressed(i, Action::Suicide));
        self.world.input.toss |= play && ctx.input(|i| i.modifiers.ctrl && keys.pressed(i, Action::TossFlag));
        self.world.input.mode |= play && ctx.input(|i| keys.pressed(i, Action::FireMode));
        self.world.input.ability |= play && ctx.input(|i| keys.pressed(i, Action::Ability));
        // The drop keys (`items`).
        if play && gameplay_input {
            use peakrunner_core::sim::items::{DROP_PACK, DROP_WEAPON};
            if ctx.input(|i| keys.pressed(i, Action::DropWeapon)) { self.world.input.drop = DROP_WEAPON; }
            if ctx.input(|i| keys.pressed(i, Action::DropPack)) { self.world.input.drop = DROP_PACK; }
        }
        // Grenades and mines: every press throws one, at full strength. Presses
        // queue, so mashing the key empties your belt as fast as it's pressed:
        // one leaves every THROW_TIME (the server's pause), none are lost.
        use peakrunner_core::sim::throwables::{THROW_GRENADE, THROW_MINE};
        if gameplay_input {
            let (g, m) = (keys.get(Action::Grenade), keys.get(Action::Mine));
            ctx.input(|i| for e in &i.events {
                if let egui::Event::Key { key, pressed: true, repeat: false, .. } = e {
                    let what = if *key == g { THROW_GRENADE } else if *key == m { THROW_MINE } else { continue };
                    if self.ctl.throws.len() < 16 { self.ctl.throws.push_back(what); }
                }
            });
        } else if self.mode != Mode::Play { self.ctl.throws.clear(); }
        self.ctl.throw_wait = (self.ctl.throw_wait - dt).max(0.0);
        if self.ctl.throw_wait <= 0.0 && self.world.input.throw == 0 {
            if let Some(what) = self.ctl.throws.pop_front() {
                self.world.input.throw = what;
                self.world.input.throw_strength = 1.0;
                self.ctl.throw_wait = peakrunner_core::defs::throws().time + 0.02;
            }
        }
        self.ctl.winding = None;
        self.world.input.look_stick_x = pad.lx;
        self.world.input.look_stick_y = pad.ly;
        if !gameplay_input {
            // Keep the weapon, and a purchase or favorite made on the inventory screen
            // (a favorite only while it's open: paused or in a menu, it waits for nothing);
            // standing at the station keeps it servicing you while you shop.
            let old = std::mem::take(&mut self.world.input);
            self.world.input.weapon = old.weapon;
            self.world.input.buy = old.buy;
            self.world.input.equip = if self.ctl.shop_open { old.equip } else { None };
            self.world.input.deploy = old.deploy;
            self.world.input.deploy_turn = old.deploy_turn;
            self.world.input.interact = self.ctl.shop_open;
            self.touch_swap = false;
            self.stick = [0.0; 2];
        }
        // A QA script's keys, unfocused or not (`qa_overrides::drive`).
        #[cfg(not(target_arch = "wasm32"))]
        let driven = play && crate::qa_overrides::active() && {
            let waiting = self.world.countdown > 0.0 || (self.online() && self.net.predictor.countdown_left().is_some());
            crate::qa_overrides::drive(&mut self.world, dt, waiting)
        };

        // Spectating: the controls fly or aim the camera, and the server only
        // hears neutral inputs.
        #[cfg(not(target_arch = "wasm32"))]
        if self.online() && self.world.spectating {
            if gameplay_input { self.ctl.spectator.look(mouse.dx, mouse.dy); }
            let pressed = |a| gameplay_input && ctx.input(|i| keys.pressed(i, a));
            let (fire_was, jet_was) = self.ctl.spectator_buttons;
            let c = crate::spectate::Controls {
                move_x: self.world.input.move_x, move_z: self.world.input.move_z,
                up: self.world.input.jump, down: gameplay_input && ctx.input(|i| i.modifiers.shift),
                fast: gameplay_input && ctx.input(|i| i.modifiers.ctrl),
                next: fire && !fire_was, previous: jet && !jet_was,
                toggle_view: pressed(Action::View), free: pressed(Action::Jump),
            };
            self.ctl.spectator_buttons = (fire, jet);
            self.world.spectator_camera = Some(self.ctl.spectator.update(&self.world, c, dt));
            self.world.input = Default::default();
        } else {
            self.world.spectator_camera = None;
            self.ctl.spectator.reset();
        }
        if self.online() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(session) = &self.net.session {
                let active = (gameplay_input || driven) && !self.world.spectating;
                // The inventory screen blocks movement and fire, not its own
                // station use and purchases.
                let station = self.mode == Mode::Play && self.ctl.shop_open;
                if !self.net.predictor.advance(&mut self.world, dt, active, station, session) {
                    self.net.lobby.error = Some("Connection stalled; leave and rejoin the match.".into());
                    self.net.session.take();
                    self.net.dropped_in = false;
                    self.mode = Mode::Lobby;
                    self.world.state = MatchState::Paused;
                    self.grab(ctx, false);
                }
            }
        } else {
            self.world.tick(dt.min(0.1).max(0.0));
        }
        // The deploy hologram follows your aim every frame.
        self.world.placing = self.ctl.placing && self.mode == Mode::Play;
        self.world.deploy_ghost = None;
        self.ctl.ghost_problem = None;
        if self.world.placing {
            let me = self.world.player_id;
            if let Some(kind) = self.world.players.get(me).and_then(|p| p.pack).and_then(|k| k.deployable()) {
                match self.world.aim_placement(me, kind, self.ctl.turn) {
                    Some(g) => {
                        self.ctl.ghost_problem = Some(g.problem.unwrap_or(""));
                        self.world.deploy_ghost = Some((g.unit, g.problem.is_none()));
                    }
                    None => self.ctl.ghost_problem = Some("TOO FAR AWAY"),
                }
            }
        }
        let raw = self.world.hud_json();
        self.audio.frame(&self.world, dt, self.mode == Mode::Play);
        let mut sounds = std::mem::take(&mut self.world.spatial_sounds);
        sounds.append(&mut self.effects.sounds);
        for (name, position) in sounds.drain(..) {
            self.audio.play_world(name, position, &self.world);
        }
        self.world.spatial_sounds = sounds;
        if let Ok(mut hud) = serde_json::from_str::<Hud>(&raw) {
            hud.goal = &self.world.mode.def().goal;
            hud.repair_tool = self.world.mode.def().repair_tool;
            hud.football = self.world.ball.active;
            hud.points = self.world.mode.rules() == peakrunner_core::modes::Rules::Rabbit;
            hud.armed = self.world.players.get(self.world.player_id).is_some() && self.world.armed(self.world.player_id);
            if let Some(p) = self.world.players.get(self.world.player_id) {
                hud.slots = p.loadout.kinds().map(peakrunner_core::sim::arsenal::name).collect();
                hud.throws = [0, 1].map(|slot| peakrunner_core::sim::throwables::thrown_by(p.armor, slot).map_or("", |(t, _, _)| t.name.as_str()));
                let effects = &peakrunner_core::registry::registry().effects;
                hud.effects = p.effects.iter().filter_map(|(k, t)| effects.get(*k as usize).map(|e| (e.name.as_str(), *t))).collect();
                for (k, _) in &p.effects {
                    if let Some(e) = effects.get(*k as usize) {
                        let a = e.tint[3];
                        for c in 0..3 { hud.tint[c] = hud.tint[c] * (1.0 - a) + e.tint[c] * a; }
                        hud.tint[3] = 1.0 - (1.0 - hud.tint[3]) * (1.0 - a);
                    }
                }
                if let Some(w) = self.world.weapon_def(self.world.player_id) {
                    use peakrunner_core::registry::Fire;
                    hud.held_name = w.name.as_str();
                    let mut parts = Vec::new();
                    match w.fire {
                        Fire::Spin if p.spin < 1.0 && p.spin > 0.0 => parts.push(format!("spinning {:.0}%", p.spin * 100.0)),
                        Fire::Charge if p.charge > 0.0 => parts.push(format!("charge {:.0}%", (p.charge / w.charge.max(1e-3)).min(1.0) * 100.0)),
                        _ => {}
                    }
                    if w.lock > 0.0 && p.lock_target > 0 { parts.push(if p.lock_time >= w.lock { "LOCKED".into() } else { "locking…".into() }); }
                    if !w.modes.is_empty() { parts.push(format!("{} next mode", self.ctl.keys.name(crate::keybinds::Action::FireMode))); }
                    hud.trigger = parts.join(" · ");
                }
                hud.held = p.loadout.kinds().position(|k| k == p.weapon).filter(|_| hud.armed);
            }
            hud.blade = self.world.blade_zone_of(self.world.player_id).map(|z| {
                let b = self.world.players[self.world.player_id].blade;
                BladeHud { stamina: self.world.stamina(self.world.player_id), charge: b.charging().then(|| b.charge()), guard: b.guard,
                    reel: b.reel > 0, place: peakrunner_core::sim::features::zones(self.world.map).get(z).map_or("", |z| z.name.as_str()) }
            });
            hud.blade_area = self.world.players.get(self.world.player_id)
                .is_some_and(|p| !p.hidden() && peakrunner_core::sim::features::blade_zone(self.world.map, p.pos).is_some());
            hud.keys = self.ctl.keys.clone();
            hud.winding = self.ctl.winding;
            hud.placing = self.ctl.ghost_problem;
            hud.command_line = self.ctl.command_line.clone();
            hud.credits = self.world.mode.def().economy.as_ref().map(|_| self.world.credits_of(self.world.player_id));
            hud.vehicle_line = vehicle_line(&self.world, &self.ctl.keys);
            if let Some(p) = self.world.players.get(self.world.player_id).filter(|p| p.alive) {
                hud.pack_line = pack_line(p, &self.ctl.keys);
                hud.ability_line = ability_line(p, &self.ctl.keys);
            }
            hud.zoomed = self.world.zoom_active();
            hud.zoom_power = self.world.zoom_power();
            hud.aim = if self.world.third_person && !hud.zoomed { third_person_aim(&self.world) } else { None };
            hud.elevation = self.world.players.get(self.world.player_id).filter(|p| p.alive && hud.armed && !hud.zoomed)
                .filter(|_| self.world.weapon_def(self.world.player_id).is_some_and(|w| w.elevation)).map(|p| p.pitch.to_degrees());
            if self.world.conditions != peakrunner_core::conditions::Conditions::default() { hud.conditions = self.world.conditions.describe(); }
            // The round countdown: the server's online, the world's own offline.
            let offline = ((self.world.countdown > 0.0).then_some(self.world.countdown), self.world.state == MatchState::Playing);
            #[cfg(not(target_arch = "wasm32"))]
            let (left, playing) = if self.online() {
                (self.net.predictor.countdown_left(), self.net.predictor.phase == crate::sim::Phase::Playing)
            } else { offline };
            #[cfg(target_arch = "wasm32")]
            let (left, playing) = offline;
            let shown = matches!(self.mode, Mode::Play | Mode::Pause);
            if let Some(cue) = self.countdown.update(left.filter(|_| shown), playing && shown, dt) { self.audio.play(cue); }
            hud.countdown = self.countdown.text();
            hud.countdown_caption = peakrunner_core::sim::label(self.world.map, self.world.mode);
            if self.world.ffa() {
                let mine = self.world.players.get(self.world.player_id).map_or(0, |p| p.frags);
                hud.ffa_line = Some(match self.world.ffa_leader() {
                    Some(i) if i != self.world.player_id => format!("You {mine} · {} {}", self.world.display_name(i), self.world.players[i].frags),
                    _ => format!("You {mine} · leading"),
                });
            }
            if let Some(line) = crate::rabbit_hud::lives_line(&self.world) { hud.ffa_line = Some(line); }
            // A duel server: the duelists' score, or the lobby's line.
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some((line, clock)) = crate::duel_hud::top_line(&self.world, hud.keys.name(crate::keybinds::Action::Vote)) {
                    (hud.ffa_line, hud.clockless) = (Some(line), !clock);
                }
                if let Some(caption) = crate::duel_hud::countdown_caption(&self.world) { hud.countdown_caption = caption; }
            }
            if !hud.events.is_empty() {
                for event in hud.events.split(',') {
                    self.audio.play(event);
                }
            }
            self.audio.set_map_ambience(self.world.map,self.world.player_pos(),self.mode==Mode::Play);
            if hud.state == 3 && self.mode == Mode::Play && !self.online() {
                self.mode = Mode::End;
                self.grab(ctx, false);
            }
            self.hud = Some(hud);
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.audio.unlock();
        self.world.set_map(self.map);
        crate::qa_overrides::stage_points(&mut self.world);
        use peakrunner_core::map_catalog::SupportedMode;
        let mode = match self.game_mode {
            _ if crate::qa_overrides::football_staged() => SupportedMode::Football,
            // Deathmatch rule sets run anywhere; any other mode where the map
            // has what it needs (a field or not, enough capture points).
            m if m.deathmatch() => m,
            m if self.world.control_point_count() >= m.def().requires.control_points
                && peakrunner_core::sim::football::has_field(self.map) == m.def().requires.football_field => m,
            // A stadium otherwise hosts only Football.
            _ if peakrunner_core::sim::football::has_field(self.map) => SupportedMode::Football,
            _ => SupportedMode::Ctf,
        };
        self.world.set_mode(mode);
        // Fresh randomness per match (deathmatch conditions, spawns).
        let seed = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos());
        self.world.reseed(seed);
        self.world.start_counted_match(self.ember);
        self.audio.play("start");
        self.mode = Mode::Play;
        self.wait_fire_release = true;
        self.grab(ctx, true);
    }

    fn pause(&mut self, ctx: &egui::Context) {
        if !self.online() { self.world.set_paused(true); }
        self.mode = Mode::Pause;
        self.grab(ctx, false);
    }

    fn resume(&mut self, ctx: &egui::Context) {
        if !self.online() { self.world.set_paused(false); }
        self.mode = Mode::Play;
        self.grab(ctx, true);
    }

    fn menu(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        self.net.disconnect();
        self.world.state = MatchState::Flyby;
        self.mode = Mode::Menu;
        self.grab(ctx, false);
        self.audio.set_jet(false);
    }

    fn grab(&mut self, ctx: &egui::Context, lock: bool) {
        let was = self.grabbed;
        self.grabbed = lock && self.mode == Mode::Play && !self.chat_open;
        // The browser has no cursor grab command: it uses Pointer Lock, asked
        // for only when the state changes (a refused request is retried by
        // resuming from the pause menu, which is a click).
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if self.grabbed != was { self.grab_age = 0.0; web_pointer_lock(self.grabbed); }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = was;
        #[cfg(not(target_arch = "wasm32"))]
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(if self.grabbed {
                egui::CursorGrab::Locked
            } else {
                egui::CursorGrab::None
            }));
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!self.grabbed));
        }
    }

    fn poll_pad(&mut self) -> Pad {
        #[cfg(target_arch = "wasm32")]
        {
            return poll_web_pad();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
        let mut pad = Pad {
            x: 0.0,
            z: 0.0,
            lx: 0.0,
            ly: 0.0,
            jump: false,
            fire: false,
            jet: false,
        };
        if let Some(pads) = &mut self.pads {
            while pads.next_event().is_some() {}
            if let Some((_, gp)) = pads.gamepads().find(|(_, g)| g.is_connected()) {
                let (x, y) = deadzone(gp.value(gilrs::Axis::LeftStickX), gp.value(gilrs::Axis::LeftStickY));
                let (lx, ly) = deadzone(gp.value(gilrs::Axis::RightStickX), gp.value(gilrs::Axis::RightStickY));
                pad.x = x;
                pad.z = y;
                pad.lx = lx;
                pad.ly = -ly;
                pad.jump = gp.is_pressed(gilrs::Button::South) || gp.is_pressed(gilrs::Button::LeftTrigger2);
                pad.jet = gp.is_pressed(gilrs::Button::LeftTrigger);
                pad.fire = gp.is_pressed(gilrs::Button::RightTrigger2) || gp.is_pressed(gilrs::Button::RightTrigger);
            }
        }
        pad
        }
    }
}

fn deadzone(x: f32, y: f32) -> (f32, f32) {
    let m = x.hypot(y);
    if m < 0.15 {
        return (0.0, 0.0);
    }
    let s = (m - 0.15) / 0.85 / m;
    (x * s, y * s)
}

/// Lock or release the mouse on the game canvas (Pointer Lock). Browsers
/// only grant a lock shortly after a click or key press.
#[cfg(target_arch = "wasm32")]
fn web_pointer_lock(lock: bool) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if lock {
        if let Some(canvas) = document.get_element_by_id("peakrunner_canvas") { canvas.request_pointer_lock(); }
    } else if document.pointer_lock_element().is_some() {
        document.exit_pointer_lock();
    }
}

/// Whether the browser still has the mouse locked to the page.
#[cfg(target_arch = "wasm32")]
fn web_pointer_locked() -> bool {
    web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.pointer_lock_element().is_some())
}

#[cfg(target_arch = "wasm32")]
fn poll_web_pad() -> Pad {
    let mut pad = Pad {
        x: 0.0,
        z: 0.0,
        lx: 0.0,
        ly: 0.0,
        jump: false,
        fire: false,
        jet: false,
    };
    let Some(window) = web_sys::window() else {
        return pad;
    };
    let Ok(list) = window.navigator().get_gamepads() else {
        return pad;
    };
    for i in 0..list.length() {
        let value = list.get(i);
        if value.is_null() || value.is_undefined() {
            continue;
        }
        let Ok(gp) = value.dyn_into::<web_sys::Gamepad>() else {
            continue;
        };
        let axes = gp.axes();
        let lx = axes.get(0).as_f64().unwrap_or(0.0) as f32;
        let ly = axes.get(1).as_f64().unwrap_or(0.0) as f32;
        let rx = axes.get(2).as_f64().unwrap_or(0.0) as f32;
        let ry = axes.get(3).as_f64().unwrap_or(0.0) as f32;
        let (x, y) = deadzone(lx, ly);
        let (look_x, look_y) = deadzone(rx, ry);
        pad.x = x;
        pad.z = -y;
        pad.lx = look_x;
        pad.ly = look_y;
        let buttons = gp.buttons();
        let pressed = |idx: u32| {
            buttons
                .get(idx)
                .dyn_into::<web_sys::GamepadButton>()
                .map(|b| b.pressed())
                .unwrap_or(false)
        };
        pad.jump = pressed(0) || pressed(6);
        pad.jet = pressed(4);
        pad.fire = pressed(7) || pressed(5);
        break;
    }
    pad
}

impl eframe::App for PeakRunnerApp {
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Vsync applies live: egui-wgpu reconfigures the surface next paint.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(mut surface) = frame.wgpu_surface_config() {
            use eframe::wgpu::PresentMode;
            let want = if self.net.preferences.saved.graphics.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
            if surface.present_mode != want {
                surface.present_mode = want; frame.set_wgpu_surface_config(surface);
                self.pacer.reset();   // the display rhythm changes with vsync
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.net.settings_changed.is_some_and(|t|t.elapsed().as_millis()>=600) {
            self.net.save_settings();
        }
        // Pace the measured frame time into whole display refreshes, so the
        // fixed-step clock and the between-steps camera advance evenly.
        let raw = ctx.input(|i| i.unstable_dt);
        self.frame_dt = self.pacer.dt(raw);
        if self.frame_stats_key(ctx) {
            self.show_frame_stats = !self.show_frame_stats;
            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut g = self.net.preferences.saved.graphics;
                g.show_fps = self.show_frame_stats;
                self.net.preferences.set_graphics(g);
            }
        }
        // Remember fullscreen toggled with the system's control (or F11) too;
        // only a change after the first observed frame counts.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(full) = ctx.input(|i| i.viewport().fullscreen) {
            if self.seen_fullscreen.is_some_and(|was| was != full) && self.net.preferences.saved.graphics.fullscreen != full {
                let mut g = self.net.preferences.saved.graphics;
                g.fullscreen = full;
                self.net.preferences.set_graphics(g);
            }
            self.seen_fullscreen = Some(full);
        }
        self.step(ctx, self.frame_dt);
        self.frame_stats.record(raw, self.world.steps_last_frame as usize);
        if crate::qa_overrides::active() { self.apply_qa(); }
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let rect = ui.max_rect();
        // Render scale: the 3D scene is drawn at a share of the window's
        // pixels and stretched to fill it; the HUD stays full resolution.
        #[cfg(not(target_arch = "wasm32"))]
        let pixels = ui.ctx().pixels_per_point() * self.net.preferences.saved.graphics.scale();
        #[cfg(target_arch = "wasm32")]
        let pixels = ui.ctx().pixels_per_point();
        let frame = build_frame_with(&self.world, self.frame_aspect, self.frame_dt.max(1.0e-4), &mut self.effects);
        ui.painter().add(Callback::new_paint_callback(
            rect,
            SceneCallback {
                pixels: [
                    (rect.width() * pixels).round().max(1.0) as u32,
                    (rect.height() * pixels).round().max(1.0) as u32,
                ],
                frame,
            },
        ));

        if let Some(hud) = self.hud.clone() {
            if self.mode == Mode::Play && hud.flash > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(226, 74, 50, (hud.flash * 90.0) as u8),
                );
            }
        }
        // Eye below a water surface: a murky tint over the view.
        if self.mode == Mode::Play {
            // A QA fly-camera below the surface tints like a player's eye would.
            let eye = crate::drawlist::qa_flycam_eye().unwrap_or(self.world.camera().0);
            if let Some(c) = peakrunner_core::water::eye_under(self.world.map, &self.world.staged_water, eye) {
                let [r, g, b] = c.map(|v| (v * 255.0) as u8);
                ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(r, g, b, 120));
            }
        }

        match self.mode {
            Mode::Menu => self.menu_ui(ui),
            // QA_NOHUD: a clean frame for screenshots (no HUD, markers or weapon).
            Mode::Play if crate::qa_overrides::no_hud() => {}
            Mode::Play => {
                let dt = self.frame_dt.min(0.1);
                crate::world_overlay::draw(ui, &self.world, &mut self.overlay, dt);
                if self.show_frame_stats {
                    let rect = ui.max_rect();
                    // Top left, under the speed readout: clear of the chat box.
                    ui.painter().text(rect.left_top() + Vec2::new(22.0, 108.0), Align2::LEFT_TOP,
                        format!("F3 · {}", self.frame_stats.text(&self.pacer)).replace(" · ", "\n"), FontId::monospace(12.0), Color32::from_rgb(170, 240, 170));
                }
                crate::flag_hud::draw(ui, &self.world);
                crate::control_hud::draw(ui, &self.world);
                self.kill_feed_ui(ui);
                crate::football_hud::draw(ui, &self.world);
                crate::rabbit_hud::draw(ui, &self.world, self.rabbit_watch.pinged());
                for text in self.generator_watch.update(&self.world) { self.announcer.push(text); }
                for text in self.point_watch.update(&self.world) { self.announcer.push(text); }
                for text in self.football_watch.update(&self.world) { self.announcer.push(text); }
                for text in self.rabbit_watch.update(&self.world, dt) { self.announcer.push(text); }
                self.announcer.update(&self.world, dt);
                self.announcer.draw(ui);
                #[cfg(not(target_arch = "wasm32"))]
                if self.net.dropped_in {
                    rift_roster(ui, &self.net.lobby);
                }
                #[cfg(not(target_arch = "wasm32"))]
                let watching = self.world.spectating;
                #[cfg(target_arch = "wasm32")]
                let watching = false;
                #[cfg(not(target_arch = "wasm32"))]
                if watching { self.spectator_hud(ui); }
                self.notice_ui(ui);
                #[cfg(not(target_arch = "wasm32"))]
                if self.host_may_start() {
                    // Under the warmup banner (play_hud's message at 92).
                    ui.painter().text(ui.max_rect().center_top() + Vec2::new(0.0, 126.0), Align2::CENTER_TOP,
                        "YOU'RE THE HOST: PRESS ESC, THEN START MATCH NOW", FontId::proportional(14.0), GLACIER);
                }
                if let Some(hud) = self.hud.clone().filter(|_| !watching) {
                    let touch = self.touch;
                    let (jump, fire, jet, swap, look) = play_hud(ui, &hud, touch, &mut self.stick);
                    let corner = ui.max_rect().right_top() + Vec2::new(-22.0, 18.0);
                    crate::map_view::draw_minimap(ui, &self.world, &mut self.map_view, corner, self.rabbit_watch.pinged());
                    self.touch_jump = jump;
                    self.touch_jet = jet;
                    self.touch_fire = fire;
                    if swap {
                        self.touch_swap = true;
                    }
                    self.look_pending += look;
                }
                self.touch_interact=false;
                if self.touch && self.world.equipment_prompt().is_some() {
                    let center=ui.max_rect().center_bottom()+Vec2::new(0.0,-180.0);
                    self.touch_interact=ui.put(egui::Rect::from_center_size(center,Vec2::new(112.0,48.0)),
                        egui::Button::new("Use / repair").sense(egui::Sense::click_and_drag())).is_pointer_button_down_on();
                }
                if self.map_view.full {
                    let key = self.ctl.keys.name(crate::keybinds::Action::Map);
                    crate::map_view::draw_full(ui, &self.world, &mut self.map_view, key, self.rabbit_watch.pinged());
                }
                if self.online() && self.world.state == MatchState::Ended { self.round_over_ui(ui); }
                self.chat_ui(ui.ctx());
                if self.ctl.shop_open { self.shop_ui(ui.ctx()); }
                #[cfg(not(target_arch = "wasm32"))]
                match self.net.lobby.duels.clone() {
                    // A duel server: the board, or the banner of a challenge to you.
                    Some(board) => {
                        if self.ctl.vote_open { self.duel_board(ui.ctx(), &board); }
                        else { crate::duel_hud::banner(ui, &board, self.ctl.keys.name(crate::keybinds::Action::Vote)); }
                    }
                    None => {
                        self.vote_banner(ui);
                        if self.ctl.vote_open { self.vote_ui(ui.ctx()); }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                self.vote_banner(ui);
            }
            Mode::Pause => self.pause_ui(ui),
            Mode::End => self.end_ui(ui),
            Mode::Browser => self.browser_ui(ui),
            Mode::Lobby => self.lobby_ui(ui),
            #[cfg(not(target_arch = "wasm32"))]
            Mode::Host => self.host_ui(ui),
            #[cfg(target_arch = "wasm32")]
            Mode::Host => {}
        }
        if self.ctl.keys_open && self.mode != Mode::Play { self.controls_ui(ui.ctx()); }
        if self.ctl.graphics_open && self.mode != Mode::Play { self.graphics_ui(ui.ctx()); }
        #[cfg(not(target_arch = "wasm32"))]
        if self.online() && !self.chat_open && (ui.input(|i| i.key_down(egui::Key::Tab)) || self.mode == Mode::End) {
            self.scoreboard_ui(ui);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.03, 0.05, 0.08, 1.0]
    }
}

impl PeakRunnerApp {
    fn chat_ui(&mut self, ctx: &egui::Context) {
        if !self.chat_open { ctx.memory_mut(|m|m.surrender_focus(egui::Id::new("chat_input"))); }
        let width = (ctx.content_rect().width() - 32.0).min(440.0).max(160.0);
        let bottom = if self.touch { 224.0 } else { 112.0 };
        let mut submit = false;
        egui::Area::new(egui::Id::new("match_comms"))
            .fade_in(false)
            .anchor(Align2::LEFT_BOTTOM, [16.0, -bottom])
            .order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(BG.r(),BG.g(),BG.b(),230)).corner_radius(6).inner_margin(10).show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("MATCH COMMS").size(11.0).color(MUTED));
                        if !self.chat_open { ui.label(RichText::new(format!("Public · {}   Team · {}", self.ctl.keys.name(crate::keybinds::Action::ChatPublic),
                            self.ctl.keys.name(crate::keybinds::Action::ChatTeam))).size(11.0).color(MUTED)); }
                        if !self.chat_open && self.touch && ui.add_sized([80.0,44.0],egui::Button::new("Chat")).clicked() {
                            self.chat_open = true;
                            self.chat_team = false;
                            self.chat_error.clear();
                        }
                    });
                    egui::ScrollArea::vertical().id_salt("match_feed").max_height(if self.chat_open {160.0} else {92.0})
                        .stick_to_bottom(true).show(ui, |ui| {
                            // Chat only: frags and announcements go to the kill feed.
                            use peakrunner_core::feed::Entry;
                            let chat: Vec<&Entry> = self.world.feed.iter()
                                .filter(|e| matches!(e, Entry::Chat {..} | Entry::TeamChat {..})).collect();
                            let skip = if self.chat_open { 0 } else { chat.len().saturating_sub(4) };
                            for entry in chat.into_iter().skip(skip) {
                                let color = if matches!(entry, Entry::TeamChat {..}) { Color32::from_rgb(140, 220, 170) } else { GLACIER };
                                ui.add(egui::Label::new(RichText::new(entry.line()).color(color).size(12.0)).wrap());
                            }
                        });
                    if self.chat_open {
                        ui.separator();
                        let response = ui.add(egui::TextEdit::singleline(&mut self.chat_text)
                            .id(egui::Id::new("chat_input")).hint_text(if self.chat_team {"Message your team…"} else {"Message everyone…"}).char_limit(160).desired_width(f32::INFINITY));
                        response.request_focus();
                        submit = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                        ui.horizontal(|ui| {
                            let h=if self.touch {44.0} else {26.0};
                            submit |= ui.add_sized([56.0,h],egui::Button::new("Send")).clicked();
                            if ui.add_sized([100.0,h],egui::Button::new("Cancel · Esc")).clicked() { self.chat_open = false; self.wait_fire_release=true; }
                            ui.label(RichText::new(if self.chat_team {"Team only"} else {"All players"}).size(11.0).color(MUTED));
                        });
                        if !self.chat_error.is_empty() { ui.colored_label(EMBER, &self.chat_error); }
                    }
                });
            });
        if submit {
            let now = ctx.input(|i| i.time);
            if let Some(text) = peakrunner_core::feed::message(&self.chat_text) {
                if now < self.chat_next { self.chat_error = "Wait a moment before sending again.".into(); return; }
                let mut sent = false;
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(session) = &self.net.session { sent = if self.chat_team { session.send_team_chat(text.clone()) } else {session.send_chat(text.clone())}; }
                if !self.online() {
                    let sender = self.world.display_name(self.world.player_id);
                    let entry = if self.chat_team { peakrunner_core::feed::Entry::TeamChat { sender,text,team:self.world.players[self.world.player_id].team } }
                        else { peakrunner_core::feed::Entry::Chat { sender,text } };
                    peakrunner_core::feed::push(&mut self.world.feed, entry);
                    sent = true;
                }
                if sent {
                    self.chat_next = now + 1.1;
                    self.chat_text.clear(); self.chat_error.clear(); self.chat_open = false;
                    self.wait_fire_release = true;
                } else { self.chat_error = "Message not queued. Try again.".into(); }
            } else { self.chat_error = "Use 1–160 characters (240 UTF-8 bytes), without control characters.".into(); }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn scoreboard_ui(&self, ui: &egui::Ui) {
        let Some(snapshot) = &self.net.lobby.snapshot else { return; };
        egui::Area::new(egui::Id::new("online-scoreboard"))
            .anchor(Align2::RIGHT_TOP, Vec2::new(-18.0, 60.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(12, 18, 28, 242))
                    .corner_radius(8.0).inner_margin(16.0).show(ui, |ui| {
                        ui.set_width(560.0);
                        ui.label(RichText::new("MATCH ROSTER").color(GLACIER).size(13.0));
                        // The client world mirrors the latest snapshot (and QA staging).
                        let w = &self.world;
                        if w.mode.rules() == peakrunner_core::modes::Rules::CaptureAndHold {
                            ui.label(RichText::new(format!("{} · Ember {} – Glacier {} · first to {}",
                                w.mode.label().to_uppercase(), w.score[0], w.score[1], w.mode.def().score_limit)).color(FG));
                            ui.horizontal_wrapped(|ui| {
                                for p in w.points.iter().filter(|p| p.active) {
                                    let owner = p.owner.map_or("neutral", |o| if o == 0 { "Ember" } else { "Glacier" });
                                    ui.label(RichText::new(format!("{} · {owner}", p.name)).color(crate::control_hud::team_color(p.owner)));
                                }
                            });
                        }
                        if w.deathmatch() {
                            let (label, limit) = (w.mode.label().to_uppercase(), w.mode.def().score_limit);
                            let duel = crate::duel_hud::top_line(w, self.ctl.keys.name(crate::keybinds::Action::Vote));
                            let text = if let Some((line, _)) = duel { if crate::duel_hud::arena(w) { format!("{label} · {line} · first to {limit}") } else { line } }
                                else if let Some(lives) = crate::rabbit_hud::lives_line(w) { format!("{label} · {lives}") }
                                else if w.mode.rules() == peakrunner_core::modes::Rules::Rabbit { format!("{label} · most points by the clock") }
                                else if w.ffa() { format!("{label} · first to {limit} frags") }
                                else { format!("{label} · Ember {} – Glacier {} · first to {limit}", w.score[0], w.score[1]) };
                            ui.label(RichText::new(text).color(FG));
                        }
                        if w.conditions != peakrunner_core::conditions::Conditions::default() { ui.label(RichText::new(w.conditions.describe()).color(MUTED)); }
                        ui.label(RichText::new(format!("{} · input ack {:.0} ms", peakrunner_core::sim::label(w.map, w.mode),
                            self.net.predictor.latency_ms)).color(MUTED));
                        // One column per team (up to 14 each), best first.
                        let ping = |id: u32| snapshot.pings.iter().find(|(p, _)| *p == id)
                            .map_or_else(|| "—".into(), |(_, ms)| format!("{ms}"));
                        // Rabbit: each player's time holding the flag, and the
                        // rabbit's name in the flag's gold, as in the classic.
                        let rabbit = w.mode.rules() == peakrunner_core::modes::Rules::Rabbit;
                        let rabbit_id = w.rabbit.carrier.filter(|_| rabbit && w.rabbit.active).and_then(|c| w.players.get(c)).map(|q| q.net_id);
                        let held = |id: u32| w.rabbit.times.iter().find(|(n, _)| *n == id).map(|(_, t)| *t as u32)
                            .filter(|&t| t > 0).map_or_else(String::new, |t| format!("{}:{:02}", t / 60, t % 60));
                        ui.columns(2, |cols| {
                            for (col, (team, name, color)) in cols.iter_mut().zip([
                                (crate::sim::Team::Ember, "EMBER", EMBER), (crate::sim::Team::Glacier, "GLACIER", GLACIER)]) {
                                let mut rows: Vec<_> = snapshot.players.iter().filter(|p| p.net_id != 0 && p.team == team).collect();
                                rows.sort_by_key(|p| (std::cmp::Reverse(p.frags), p.losses, p.net_id));
                                col.label(RichText::new(format!("{name} · {}", rows.len())).color(color).strong());
                                egui::Grid::new(("match-scores", name)).striped(true).spacing(Vec2::new(10.0, 3.0)).show(col, |ui| {
                                    ui.label(RichText::new("Player").color(MUTED).size(11.0));
                                    ui.label(RichText::new(if w.ball.active || rabbit { "Pts/D" } else { "K/D" }).color(MUTED).size(11.0));
                                    if rabbit { ui.label(RichText::new("Held").color(MUTED).size(11.0)); }
                                    ui.label(RichText::new("ms").color(MUTED).size(11.0)); ui.end_row();
                                    for p in rows {
                                        let you = p.net_id == self.net.lobby.player_id;
                                        let label = if p.is_bot { format!("{} · bot", p.name) } else { p.name.clone() };
                                        let shade = if rabbit_id == Some(p.net_id) { crate::rabbit_hud::FLAG } else if you { color } else if p.is_bot { MUTED } else { FG };
                                        let text = RichText::new(label).color(shade).size(13.0);
                                        ui.label(if you { text.strong() } else { text });
                                        ui.label(RichText::new(format!("{}/{}", p.frags, p.losses)).size(13.0));
                                        if rabbit { ui.label(RichText::new(held(p.net_id)).color(MUTED).size(12.0)); }
                                        ui.label(RichText::new(ping(p.net_id)).color(MUTED).size(12.0));
                                        ui.end_row();
                                    }
                                });
                            }
                        });
                        if !snapshot.spectators.is_empty() {
                            let names: Vec<&str> = snapshot.spectators.iter().map(|s| s.name.as_str()).collect();
                            ui.label(RichText::new(format!("SPECTATORS · {} · {}", names.len(), names.join(", "))).color(MUTED).size(12.0));
                        }
                        ui.label(RichText::new("Ping = server RTT · — unavailable on local TCP").size(11.0).color(MUTED));
                    });
            });
    }

    /// The main menu: Home, Singleplayer, Multiplayer or the Library.
    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        // Escape goes back a page (unless a window is open over the menu).
        let back = !self.ctl.keys_open && !self.ctl.graphics_open && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        match self.page {
            MenuPage::Home => self.home_ui(ui),
            MenuPage::Single => { if back { self.page = MenuPage::Home; } self.single_ui(ui) }
            MenuPage::Multi => { if back { self.page = MenuPage::Home; } self.multi_ui(ui) }
            MenuPage::Library(topic) => {
                if back { self.page = if topic.is_some() { MenuPage::Library(None) } else { MenuPage::Home }; }
                self.library_ui(ui, topic)
            }
        }
    }

    /// The menu's panel: on the left, as wide as fits (up to `max`), and
    /// scrolling when taller than the window.
    fn menu_panel(ctx: &egui::Context, id: &str, max: f32, min_height: f32, add: impl FnOnce(&mut egui::Ui)) {
        let screen = ctx.content_rect();
        let width = (screen.width() - 72.0 - 48.0).clamp(300.0, max);
        egui::Area::new(egui::Id::new(id)).anchor(Align2::LEFT_CENTER, Vec2::new(36.0, 0.0)).show(ctx, |ui| {
            egui::Frame::new().fill(Color32::from_rgba_unmultiplied(10, 14, 22, 226)).corner_radius(14.0)
                .inner_margin(24.0).stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 52, 68))).show(ui, |ui| {
                ui.set_width(width);
                let tall = (screen.height() - 88.0).max(240.0);
                egui::ScrollArea::vertical().id_salt(id).max_height(tall).min_scrolled_height(min_height.min(tall)).show(ui, add);
            });
        });
    }

    /// A big menu choice: a title and a line under it, the whole card clickable.
    fn big_choice(ui: &mut egui::Ui, title: &str, line: &str) -> bool {
        let width = ui.available_width();
        let response = egui::Frame::new().fill(Color32::from_rgb(20, 28, 40)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(52, 70, 92)))
            .corner_radius(10.0).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
                ui.set_width(width - 36.0);
                ui.label(RichText::new(title).color(FG).size(26.0).strong());
                ui.label(RichText::new(line).color(MUTED).size(13.0));
            }).response.interact(egui::Sense::click());
        let hovered = response.hovered();
        if hovered { ui.painter().rect_stroke(response.rect, 10.0, egui::Stroke::new(2.0, GLACIER), egui::StrokeKind::Inside); }
        response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    fn home_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut page = None;
        let (mut keys, mut graphics) = (false, false);
        Self::menu_panel(&ctx, "menu-home", 560.0, 470.0, |ui| {
            ui.label(RichText::new("PEAKRUNNER").color(FG).size(52.0).strong());
            ui.label(RichText::new("Ski the ridgelines, jet the gaps, take their flag.").color(MUTED));
            ui.add_space(18.0);
            if Self::big_choice(ui, "Singleplayer", "Play against bots on this computer. Pick a mode, a map and the sky.") { page = Some(MenuPage::Single); }
            ui.add_space(10.0);
            if Self::big_choice(ui, "Multiplayer", "Find a server, join friends, or host your own game.") { page = Some(MenuPage::Multi); }
            ui.add_space(10.0);
            if Self::big_choice(ui, "Library", "How to play, the weapons, the maps and more, with pictures.") { page = Some(MenuPage::Library(None)); }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                keys = ui.add(secondary_button("Controls…")).clicked();
                graphics = ui.add(secondary_button("Graphics…")).clicked();
            });
            ui.add_space(10.0);
            ui.label(RichText::new("Playtest build, provided as is, without warranty. Terms, privacy and licenses: peakrunner.net/legal")
                .color(MUTED).size(11.0));
        });
        if let Some(p) = page { self.page = p; }
        if keys { self.ctl.keys_open = true; }
        if graphics { self.ctl.graphics_open = true; }
    }

    fn multi_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (mut back, mut browse) = (false, false);
        #[cfg(not(target_arch = "wasm32"))]
        let mut host = false;
        #[cfg(not(target_arch = "wasm32"))]
        let hosting = self.net.host.server.as_ref().is_some_and(|s| s.exited.is_none());
        Self::menu_panel(&ctx, "menu-multi", 560.0, 300.0, |ui| {
            ui.label(RichText::new("Multiplayer").color(FG).size(40.0).strong());
            ui.label(RichText::new("Play with other people over the internet or on your network.").color(MUTED));
            ui.add_space(18.0);
            if Self::big_choice(ui, "Find match", "Pick a server from the list, or join by an address or invite link.") { browse = true; }
            ui.add_space(10.0);
            #[cfg(not(target_arch = "wasm32"))]
            if Self::big_choice(ui, if hosting { "Your server" } else { "Host game" },
                "Start your own server on this computer and play on it; friends can join.") { host = true; }
            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new("Hosting needs the downloaded game.").color(MUTED));
            ui.add_space(16.0);
            back = ui.add(secondary_button("Back")).clicked();
        });
        if back { self.page = MenuPage::Home; }
        if browse { self.open_browser(); }
        #[cfg(not(target_arch = "wasm32"))]
        if host { self.net.host.error = None; self.mode = Mode::Host; }
    }

    fn library_ui(&mut self, ui: &mut egui::Ui, topic: Option<usize>) {
        use crate::library::{fill, Part, TOPICS};
        let ctx = ui.ctx().clone();
        let mut go = None;
        let keys = self.ctl.keys.clone();
        let pictures = &mut self.pictures;
        let picture = |ui: &mut egui::Ui, pictures: &mut crate::library::Pictures, name: &'static str, width: f32| {
            if let Some(tex) = pictures.get(&ctx, name) {
                let [w, h] = tex.size();
                let size = Vec2::new(width, width * h as f32 / w.max(1) as f32);
                ui.add(egui::Image::new(&tex).fit_to_exact_size(size).corner_radius(8.0));
            }
        };
        Self::menu_panel(&ctx, "menu-library", 900.0, f32::MAX, |ui| {
            match topic.and_then(|t| TOPICS.get(t).map(|x| (t, x))) {
                None => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Library").color(FG).size(40.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(secondary_button("Back")).clicked() { go = Some(MenuPage::Home); }
                        });
                    });
                    ui.label(RichText::new("How to play, in a few pictures. Pick one!").color(MUTED));
                    ui.add_space(12.0);
                    let card_w = 200.0;
                    let per_row = ((ui.available_width() + 8.0) / (card_w + 16.0 + 2.0 + 8.0)).floor().max(1.0) as usize;
                    for (row, topics) in TOPICS.chunks(per_row).enumerate() {
                        ui.horizontal(|ui| {
                            for (k, t) in topics.iter().enumerate() {
                                let response = egui::Frame::new().fill(Color32::from_rgb(20, 28, 40)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(52, 70, 92)))
                                    .corner_radius(10.0).inner_margin(8.0).show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.set_width(card_w);
                                            picture(ui, pictures, t.picture, card_w);
                                            ui.label(RichText::new(t.title).color(FG).size(16.0).strong());
                                            ui.label(RichText::new(t.card).color(MUTED).size(12.0));
                                        });
                                    }).response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                                if response.hovered() { ui.painter().rect_stroke(response.rect, 10.0, egui::Stroke::new(2.0, GLACIER), egui::StrokeKind::Inside); }
                                if response.clicked() { go = Some(MenuPage::Library(Some(row * per_row + k))); }
                            }
                        });
                        ui.add_space(8.0);
                    }
                }
                Some((t, page)) => {
                    ui.horizontal(|ui| {
                        if ui.add(secondary_button("Back to the Library")).clicked() { go = Some(MenuPage::Library(None)); }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if t + 1 < TOPICS.len() && ui.add(secondary_button("Next")).clicked() { go = Some(MenuPage::Library(Some(t + 1))); }
                            if t > 0 && ui.add(secondary_button("Previous")).clicked() { go = Some(MenuPage::Library(Some(t - 1))); }
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(page.title).color(FG).size(36.0).strong());
                    ui.add_space(8.0);
                    picture(ui, pictures, page.picture, ui.available_width().min(640.0));
                    ui.add_space(10.0);
                    for part in page.parts {
                        match part {
                            Part::Text(text) => {
                                ui.add(egui::Label::new(RichText::new(fill(text, &keys)).color(FG).size(17.0)).wrap());
                                ui.add_space(8.0);
                            }
                            Part::Item { picture: name, name: title, text } => {
                                ui.horizontal(|ui| {
                                    picture(ui, pictures, name, 200.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(*title).color(FG).size(19.0).strong());
                                        ui.add(egui::Label::new(RichText::new(fill(text, &keys)).color(FG).size(15.0)).wrap());
                                    });
                                });
                                ui.add_space(8.0);
                            }
                        }
                    }
                }
            }
        });
        if let Some(p) = go { self.page = p; }
    }

    /// Singleplayer: an offline match against bots (mode, map, team and
    /// bots, sky), started here.
    fn single_ui(&mut self, ui: &mut egui::Ui) {
        use peakrunner_core::map_catalog::{self, SupportedMode};
        // Maps the chosen mode can use (QA staging can widen Football and
        // Capture & Hold to any map).
        let maps_for = |mode: SupportedMode| -> Vec<terrain::MapInfo> {
            terrain::maps().into_iter().filter(|m| match mode {
                m if m.rules() == peakrunner_core::modes::Rules::Football && crate::qa_overrides::football_staged() => true,
                m if m.rules() == peakrunner_core::modes::Rules::CaptureAndHold
                    && crate::qa_overrides::staged_point_count().is_some_and(|n| n >= m.def().requires.control_points) => true,
                _ => map_catalog::supports(m.id, mode),
            }).collect()
        };
        let card = |ui: &mut egui::Ui, on: bool, title: &str, detail: &str, width: f32| -> bool {
            let fill = if on { Color32::from_rgb(34, 58, 78) } else { Color32::from_rgb(20, 26, 36) };
            let stroke = if on { egui::Stroke::new(2.0, GLACIER) } else { egui::Stroke::new(1.0, Color32::from_rgb(44, 54, 68)) };
            let response = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(8.0).inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.set_min_height(58.0);
                        ui.label(RichText::new(title).color(if on { FG } else { Color32::from_rgb(210, 216, 226) }).size(15.0).strong());
                        ui.add(egui::Label::new(RichText::new(detail).color(MUTED).size(11.5)).wrap());
                    });
                }).response.interact(egui::Sense::click());
            response.clicked()
        };
        // The panel fits the window: as wide as allowed (up to four cards a
        // row), cards laid out in rows of as many as fit, and the whole menu
        // scrolls when it's taller than the window.
        let screen = ui.ctx().content_rect();
        let panel_w = (screen.width() - 72.0 - 48.0).clamp(380.0, 840.0);
        const CARD_W: f32 = 170.0;
        let per_row = (((panel_w + 8.0) / (CARD_W + 24.0 + 2.0 + 8.0)).floor() as usize).max(1);
        egui::Area::new(egui::Id::new("menu"))
            .anchor(Align2::LEFT_CENTER, Vec2::new(36.0, 0.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(10, 14, 22, 226)).corner_radius(14.0)
                    .inner_margin(24.0).stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 52, 68))).show(ui, |ui| {
                ui.set_width(panel_w);
                let tall = (screen.height() - 88.0).max(240.0);
                egui::ScrollArea::vertical().max_height(tall).min_scrolled_height(tall).show(ui, |ui| {
                ui.label(RichText::new("PEAKRUNNER").color(FG).size(52.0).strong());
                ui.label(RichText::new("Ski the ridgelines, jet the gaps, take their flag.").color(MUTED));
                ui.add_space(14.0);

                ui.label(RichText::new("1 · GAME MODE").color(GLACIER).size(13.0));
                ui.add_space(4.0);
                // Every offered mode the engine knows (the base game's and any mod's).
                let modes: Vec<(SupportedMode, &str)> = SupportedMode::every().into_iter().filter(|m| m.offered())
                    .map(|m| (m, m.def().note.as_str())).collect();
                for row in modes.chunks(per_row) {
                    ui.horizontal(|ui| {
                        for &(mode, detail) in row {
                            let usable = !maps_for(mode).is_empty();
                            if card(ui, self.game_mode == mode, mode.label(), if usable { detail } else { "No maps for this yet." }, CARD_W) && usable {
                                self.game_mode = mode;
                            }
                        }
                    });
                }
                // Keep the map valid for the mode.
                let maps = maps_for(self.game_mode);
                if !maps.iter().any(|m| m.id == self.map) {
                    if let Some(first) = maps.first() { self.map = first.id; self.world.set_map(first.id); }
                }
                ui.add_space(12.0);

                ui.label(RichText::new("2 · MAP").color(GLACIER).size(13.0));
                ui.add_space(4.0);
                for row in maps.chunks(per_row) {
                    ui.horizontal(|ui| {
                        for spec in row {
                            let size = if spec.size >= 1000.0 { format!("{:.1} km", spec.size / 1000.0) } else { format!("{:.0} m", spec.size) };
                            if card(ui, self.map == spec.id, &format!("{} · {size}", spec.name), spec.note, CARD_W) {
                                self.map = spec.id;
                                self.world.set_map(spec.id);
                            }
                        }
                    });
                }
                ui.add_space(12.0);

                ui.label(RichText::new("3 · TEAM AND BOTS").color(GLACIER).size(13.0));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    team_button(ui, "Ember", self.ember, EMBER, || self.ember = true);
                    team_button(ui, "Glacier", !self.ember, GLACIER, || self.ember = false);
                    ui.add_space(12.0);
                    use peakrunner_core::bot_nav::Difficulty;
                    let current = self.world.bot_difficulty;
                    for difficulty in Difficulty::ALL {
                        let on = current == difficulty;
                        let fill = if on { FG } else { Color32::from_rgb(22, 28, 38) };
                        let text = if on { BG } else { FG };
                        let button = egui::Button::new(RichText::new(difficulty.label()).color(text)).fill(fill).min_size(Vec2::new(72.0, 36.0));
                        if ui.add(button).clicked() {
                            self.world.set_bot_difficulty(difficulty);
                            #[cfg(not(target_arch = "wasm32"))]
                            self.net.preferences.set_bot_difficulty(difficulty);
                        }
                    }
                });
                let about = match self.world.bot_difficulty {
                    peakrunner_core::bot_nav::Difficulty::Easy => "Bots: mostly rookies, slow to react, wide of the mark.",
                    peakrunner_core::bot_nav::Difficulty::Normal => "Bots: a spread of grunts, riders, skirmishers, anchors and hawks.",
                    peakrunner_core::bot_nav::Difficulty::Hard => "Bots: quick, accurate skiers, jetters, high-flying hawks and aces.",
                    peakrunner_core::bot_nav::Difficulty::Mixed => "Bots: any personality, from rookie to ace.",
                };
                ui.label(RichText::new(about).color(MUTED).size(12.0));
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("4 · SKY").color(GLACIER).size(13.0));
                    ui.add_space(8.0);
                    sky_picks(ui, "menu-sky", &mut self.world.forecast);
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Start match").size(20.0).color(BG)).fill(FG).min_size(Vec2::new(200.0, 46.0))).clicked() {
                        self.start(ui.ctx());
                    }
                    if ui.add(secondary_button("Back").min_size(Vec2::new(120.0, 46.0))).clicked() {
                        self.page = MenuPage::Home;
                    }
                });
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    use crate::keybinds::Action;
                    let k = &self.ctl.keys;
                    hint(ui, "Move", &[Action::Forward, Action::Left, Action::Back, Action::Right].map(|a| k.name(a)).concat());
                    hint(ui, "Ski / jump", k.name(Action::Jump));
                    hint(ui, "Jet", "Right click");
                    hint(ui, "Fire", &format!("Click · {}/{}/{} slots", k.name(Action::Slot1), k.name(Action::Slot2), k.name(Action::Slot3)));
                    hint(ui, "Repair", &format!("Hold {}", k.name(Action::Repair)));
                    hint(ui, "Inventory", &format!("{} at a station", k.name(Action::Use)));
                    hint(ui, "Grenade / mine", &format!("{} / {}", k.name(Action::Grenade), k.name(Action::Mine)));
                    hint(ui, "Deploy", k.name(Action::Deploy));
                    hint(ui, "Respawn", &format!("Ctrl+{}", k.name(Action::Suicide)));
                    hint(ui, "Toss flag", &format!("Ctrl+{}", k.name(Action::TossFlag)));
                    if ui.add(secondary_button("Controls…")).clicked() { self.ctl.keys_open = true; }
                    if ui.add(secondary_button("Graphics…")).clicked() { self.ctl.graphics_open = true; }
                });
                ui.add_space(10.0);
                ui.label(RichText::new("Playtest build, provided as is, without warranty. Terms, privacy and licenses: peakrunner.net/legal")
                    .color(MUTED).size(11.0));
                });
                    });
            });
    }

    /// Frags and announcements, top right under the minimap: each shows for
    /// KILL_FEED_SECONDS and fades over its last second; at most 5 at once.
    fn kill_feed_ui(&mut self, ui: &egui::Ui) {
        use peakrunner_core::feed::Entry;
        const KILL_FEED_SECONDS: f64 = 8.0;
        let now = ui.ctx().input(|i| i.time);
        // New entries are the tail of the feed beyond its overlap with what we saw.
        let feed = &self.world.feed;
        let overlap = (0..=self.feed_seen.len().min(feed.len())).rev()
            .find(|&k| self.feed_seen[self.feed_seen.len() - k..] == feed[..k]).unwrap_or(0);
        for e in &feed[overlap..] {
            if matches!(e, Entry::Frag {..} | Entry::Play {..}) { self.kill_feed.push((e.clone(), now)); }
        }
        self.feed_seen = feed.clone();
        self.kill_feed.retain(|(_, t)| now - t < KILL_FEED_SECONDS);
        if self.kill_feed.len() > 5 { self.kill_feed.drain(..self.kill_feed.len() - 5); }
        // Top left under the speed readout, clear of the minimap (top right).
        let rect = ui.max_rect();
        let p = ui.painter();
        for (k, (e, t)) in self.kill_feed.iter().enumerate() {
            let alpha = ((KILL_FEED_SECONDS - (now - t)) as f32).clamp(0.0, 1.0);
            let color = match e { Entry::Play {..} => Color32::from_rgb(255, 214, 120), _ => FG }.gamma_multiply(alpha);
            let at = rect.left_top() + Vec2::new(18.0, 112.0 + k as f32 * 22.0);
            let galley = p.layout_no_wrap(e.line(), FontId::proportional(13.0), color);
            let bg = egui::Rect::from_min_size(at - Vec2::new(0.0, 2.0), galley.size() + Vec2::new(12.0, 4.0));
            p.rect_filled(bg, 4.0, Color32::from_black_alpha((150.0 * alpha) as u8));
            p.galley(at + Vec2::new(6.0, 0.0), galley, color);
        }
        if !self.kill_feed.is_empty() { ui.ctx().request_repaint(); }
    }

    /// Playing on a duel server (its board has come).
    #[cfg(not(target_arch = "wasm32"))]
    fn duel_server(&self) -> bool { self.net.lobby.duels.is_some() }
    #[cfg(target_arch = "wasm32")]
    fn duel_server(&self) -> bool { false }

    /// The duel board (`duel_hud`): what it asks goes to the server, and a
    /// move closes it.
    #[cfg(not(target_arch = "wasm32"))]
    fn duel_board(&mut self, ctx: &egui::Context, board: &peakrunner_core::sim::duel::Board) {
        use crate::duel_hud::Ask;
        let me = self.net.lobby.player_id;
        let Some(ask) = crate::duel_hud::board_ui(ctx, board, me, self.ctl.keys.name(crate::keybinds::Action::Vote)) else { return };
        let Some(session) = &self.net.session else { return };
        let moving = match ask {
            Ask::Challenge(id) => { session.challenge(id); false }
            Ask::Bot(archetype) => { session.challenge_bot(&archetype); true }
            Ask::Answer(id, yes) => { session.answer_challenge(id, yes); yes }
            Ask::Withdraw => { session.withdraw(); false }
            Ask::Watch(arena) => { session.watch(arena); true }
            Ask::Lobby => { session.back_to_lobby(); true }
        };
        if moving { self.close_vote(); }
    }

    fn close_vote(&mut self) {
        if self.ctl.vote_open {
            self.ctl.vote_open = false;
            self.wait_fire_release = true;
        }
    }

    /// The map-vote panel: yes/no on a running vote, otherwise the server's
    /// rotation entries to call one for.
    #[cfg(not(target_arch = "wasm32"))]
    fn vote_ui(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = self.net.lobby.snapshot.clone() else { return };
        let me = self.net.lobby.player_id;
        let mut action: Option<Result<u16, bool>> = None;   // Ok(call entry) / Err(vote yes?)
        let running = snapshot.vote.as_ref().filter(|v| v.passed.is_none());
        if let Some(v) = running {
            if !v.voted.contains(&me) {
                ctx.input(|i| {
                    if i.key_pressed(egui::Key::Num1) { action = Some(Err(true)); }
                    if i.key_pressed(egui::Key::Num2) { action = Some(Err(false)); }
                });
            }
        }
        egui::Area::new(egui::Id::new("map_vote")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(12, 18, 28, 245))
                    .stroke(egui::Stroke::new(2.0, VOTE_AMBER)).corner_radius(10.0).inner_margin(20.0).show(ui, |ui| {
                        ui.set_width(560.0);
                        ui.label(RichText::new("MAP VOTE").size(13.0).color(VOTE_AMBER).strong());
                        if let Some(v) = running {
                            ui.label(RichText::new(peakrunner_core::sim::label(v.map, v.mode)).size(26.0).color(FG).strong());
                            ui.label(RichText::new(format!("Called by {} · {:.0} s left · Yes {} · No {} · {} players",
                                v.caller, v.left.max(0.0), v.yes, v.no, v.voters)).color(MUTED));
                            ui.add_space(12.0);
                            if v.voted.contains(&me) {
                                ui.label(RichText::new("You have voted. You can change your vote:").color(MUTED));
                            }
                            ui.horizontal(|ui| {
                                let yes = egui::Button::new(RichText::new("1 · Yes, switch").size(18.0).color(BG)).fill(Color32::from_rgb(120, 210, 140)).min_size(Vec2::new(200.0, 48.0));
                                if ui.add(yes).clicked() { action = Some(Err(true)); }
                                let no = egui::Button::new(RichText::new("2 · No, stay").size(18.0)).min_size(Vec2::new(200.0, 48.0));
                                if ui.add(no).clicked() { action = Some(Err(false)); }
                            });
                        } else {
                            ui.label(RichText::new("Call a vote to switch to another map or mode in this server's rotation. Everyone gets a minute to vote.").color(MUTED));
                            ui.add_space(10.0);
                            let current = (snapshot.map, snapshot.mode);
                            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for (i, &(map, mode)) in snapshot.vote_choices.iter().enumerate() {
                                        let here = (map, mode) == current;
                                        let text = RichText::new(format!("{}\n{}", peakrunner_core::terrain::info(map).name, mode.label())).size(14.0);
                                        let b = egui::Button::new(if here { text.color(MUTED) } else { text.color(FG) }).min_size(Vec2::new(170.0, 52.0));
                                        if ui.add_enabled(!here && snapshot.vote_wait <= 0.0, b).on_disabled_hover_text(if here { "Playing now" } else { "Not yet" }).clicked() { action = Some(Ok(i as u16)); }
                                    }
                                });
                            });
                            if snapshot.vote_wait > 0.0 {
                                let w = snapshot.vote_wait.ceil() as u32;
                                ui.label(RichText::new(format!("The last vote failed or just ended: the next can be called in {}:{:02}.", w / 60, w % 60))
                                    .color(VOTE_AMBER).size(13.0));
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new("V or Esc closes").color(MUTED).size(11.0));
                    });
            });
        if let (Some(a), Some(session)) = (action, &self.net.session) {
            match a { Ok(choice) => { session.call_vote(choice); } Err(yes) => { session.vote(yes); } }
            self.close_vote();
        }
    }

    /// The vote notice: a bright banner under the score while a vote runs,
    /// then its result for a few seconds.
    fn vote_banner(&self, ui: &egui::Ui) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(v) = self.net.lobby.snapshot.as_ref().and_then(|s| s.vote.clone()) else { return };
            let me = self.net.lobby.player_id;
            let rect = ui.max_rect();
            let (title, detail, color) = match v.passed {
                None => (format!("VOTE · {}", peakrunner_core::sim::label(v.map, v.mode)),
                    format!("{:.0} s · YES {} · NO {} · {}", v.left.max(0.0), v.yes, v.no,
                        if v.voted.contains(&me) { "you voted (V to change)".to_string() } else { format!("press {} to vote", self.ctl.keys.get(crate::keybinds::Action::Vote).name()) }),
                    VOTE_AMBER),
                Some(true) => ("VOTE PASSED".into(), format!("Switching to {}", peakrunner_core::sim::label(v.map, v.mode)), Color32::from_rgb(120, 210, 140)),
                Some(false) => ("VOTE FAILED".into(), format!("Staying here (yes {} · no {})", v.yes, v.no), MUTED),
            };
            // Pulse the frame while the vote is open, so it's hard to miss.
            let t = ui.ctx().input(|i| i.time) as f32;
            let pulse = if v.passed.is_none() { 0.55 + 0.45 * (t * 4.0).sin().abs() } else { 1.0 };
            let size = Vec2::new(560.0, 64.0);
            let at = egui::Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 168.0), size);
            let p = ui.painter();
            p.rect_filled(at, 10.0, Color32::from_rgba_unmultiplied(20, 16, 6, 230));
            p.rect_stroke(at, 10.0, egui::Stroke::new(3.0, color.gamma_multiply(pulse)), egui::StrokeKind::Inside);
            p.text(at.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, title, FontId::proportional(22.0), color);
            p.text(at.center_bottom() + Vec2::new(0.0, -8.0), Align2::CENTER_BOTTOM, detail, FontId::proportional(14.0), FG);
            if v.passed.is_none() { ui.ctx().request_repaint(); }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ui;
    }

    /// A vehicle station's screen: the vehicles it hands out, with prices
    /// where the mode has an economy. Taking one puts you at its controls.
    fn vehicle_shop(&mut self, ctx: &egui::Context, station: usize) {
        use peakrunner_core::sim::vehicles::BUY_VEHICLE;
        let r = peakrunner_core::registry::registry();
        let Some(e) = self.world.deployables.get(station) else { return };
        let def = self.world.mode.def();
        let credits = def.economy.as_ref().map(|_| self.world.credits_of(self.world.player_id));
        let team = self.world.players[self.world.player_id].team;
        let mut take = None;
        egui::Window::new(RichText::new(e.kind.name().to_uppercase()).strong()).collapsible(false).resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                if let Some(c) = credits { ui.label(RichText::new(format!("Credits {:.0}", c.floor())).color(Color32::from_rgb(240, 210, 110))); }
                for id in &e.kind.def().vehicles {
                    let Some(k) = r.vehicles.index(id) else { continue };
                    let Some(v) = r.vehicles.get(k) else { continue };
                    let out = self.world.vehicles.iter().filter(|c| c.team == team && c.kind as usize == k).count();
                    let full = v.team_limit > 0 && out >= v.team_limit as usize;
                    let dear = credits.is_some_and(|c| c < v.price as f32);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&v.name).size(15.0).color(FG));
                        ui.label(RichText::new(&v.about).size(12.0).color(MUTED));
                        let note = if v.team_limit > 0 { format!("{out}/{} out", v.team_limit) } else { String::new() };
                        let note = if credits.is_some() && v.price > 0 { format!("{note} · {} cr", v.price) } else { note };
                        ui.label(RichText::new(note).size(12.0).color(GLACIER));
                        let label = if full { "Team full" } else if dear { "Too dear" } else { "Take" };
                        if ui.add_enabled(!full && !dear, egui::Button::new(label)).clicked() { take = Some(k); }
                    });
                }
                ui.label(RichText::new("The use key boards a vehicle beside you, and leaves it.").size(12.0).color(MUTED));
            });
        if let Some(k) = take {
            self.world.input.buy = BUY_VEHICLE + k as u16;
            self.close_shop();
        }
    }

    fn close_shop(&mut self) {
        if self.ctl.shop_open {
            self.ctl.shop_open = false;
            // The click on a button isn't a shot.
            self.wait_fire_release = true;
        }
    }

    /// The inventory screen: Armor, Favorites, Weapons (as many as the
    /// armor's slots, in the order chosen: number keys pick slot 1, 2, 3…),
    /// Packs and Miscellany. Standing at the station heals you and restocks
    /// rounds, grenades and mines. A field inventory (station 3) offers the
    /// same except armor and the station packs. Purchases are intents the
    /// server checks; what you choose is what you respawn with.
    fn shop_ui(&mut self, ctx: &egui::Context) {
        use peakrunner_core::sim::{arsenal, deploy::DeployKind, loadout::*, packs::{PackKind, PackUse}, throwables::max_throwables};
        use crate::keybinds::Action;
        let Some(me) = self.world.players.get(self.world.player_id).cloned() else { return };
        // A vehicle station has its own screen.
        if !self.hud.as_ref().is_some_and(|h| h.station == 1 || h.station == 3) {
            if let Some(s) = self.world.vehicle_station(self.world.player_id) { self.vehicle_shop(ctx, s); return; }
        }
        let field = self.hud.as_ref().is_some_and(|h| h.station == 3);
        let out = |kind: DeployKind| self.world.deployables.iter().filter(|e| e.team == me.team && e.kind == kind).count();
        let registry = peakrunner_core::registry::registry();
        let max = rounds_max(&me);
        let throws = max_throwables(me.armor);
        let slots = arsenal::slots(me.armor);
        let def = self.world.mode.def();
        // The armors a station offers, in wire order; number keys pick them.
        let armors: Vec<ArmorClass> = def.offered_armors();
        let mut packs: Vec<PackKind> = PackKind::all().filter(|&k| def.offers_pack_to(k, me.armor) && (def.deployables || k.deployable().is_none())).collect();
        // Worn packs first, then those that place something, in the armor's
        // own order where it lists them.
        let listed = &me.armor.def().packs;
        packs.sort_by_key(|k| (k.deployable().is_some(), listed.iter().position(|p| p == k.key()).unwrap_or(usize::MAX)));
        // With an economy, prices and what there is to spend (`items`).
        let credits = def.economy.as_ref().map(|_| self.world.credits_of(self.world.player_id));
        let price = |n: u32, note: String| match credits {
            Some(_) if n > 0 => if note.is_empty() { format!("{n} cr") } else { format!("{note} · {n} cr") },
            _ => note,
        };
        let afford = |n: u32| credits.is_none_or(|c| c >= n as f32);
        let mut buy = BUY_NONE;
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut equip = None;
        #[cfg(not(target_arch = "wasm32"))]
        let setups = self.net.preferences.saved.setups.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let mut save: Option<usize> = None;
        ctx.input(|i| {
            let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6,
                egui::Key::Num7, egui::Key::Num8, egui::Key::Num9];
            for (key, armor) in keys.into_iter().zip(&armors) {
                if i.key_pressed(key) && !field { buy = BUY_ARMOR + armor.0 as u16; }
            }
            // A press, not the key's repeat: held, it re-equipped (and
            // sounded) at the repeat rate while the screen stays open.
            #[cfg(not(target_arch = "wasm32"))]
            for (n, key) in [egui::Key::F1, egui::Key::F2, egui::Key::F3, egui::Key::F4].into_iter().enumerate() {
                let pressed = i.events.iter().any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, repeat: false, .. } if *k == key));
                if pressed { if let Some(s) = &setups[n] { equip = Some(s.equip()); } }
            }
        });
        let mut close = false;
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.add_space(8.0);
            ui.label(RichText::new(text).size(13.0).color(GLACIER).strong());
            ui.separator();
        };
        egui::Area::new(egui::Id::new("inventory_station"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 18, 28, 240))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 70, 90)))
                    .corner_radius(10.0)
                    .inner_margin(20.0)
                    .show(ui, |ui| {
                        let width = (ctx.content_rect().width() - 64.0).clamp(280.0, 580.0);
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if field { "Field inventory" } else { "Inventory station" }).size(24.0).color(FG).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(secondary_button("Close")).clicked() { close = true; }
                            });
                        });
                        ui.label(RichText::new(if field {
                            "Restocks rounds, grenades and mines. Weapons and small packs here; armor and stations only at a base station. You respawn with what you choose until you choose again."
                        } else {
                            "Standing here heals you and restocks rounds, grenades and mines. You respawn with what you choose here until you choose again."
                        }).size(12.0).color(MUTED));
                        // A row: its shortcut, name and blurb; a note; and a button
                        // (its label, and whether it can be pressed).
                        let row = |ui: &mut egui::Ui, key: &str, name: &str, about: &str, note: &str, button: Option<(&str, bool)>| -> bool {
                            let mut clicked = false;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(key).monospace().color(MUTED));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(name).size(15.0).color(FG));
                                    ui.label(RichText::new(about).size(11.0).color(MUTED));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if let Some((label, enabled)) = button {
                                        clicked = ui.add_enabled(enabled, egui::Button::new(RichText::new(label).color(FG))
                                            .min_size(Vec2::new(96.0, 26.0))).clicked();
                                    }
                                    if !note.is_empty() { ui.label(RichText::new(note).size(12.0).color(GLACIER)); }
                                });
                            });
                            clicked
                        };
                        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 160.0).max(200.0)).show(ui, |ui| {
                            if let Some(c) = credits { heading(ui, &format!("CREDITS · {:.0}", c.floor())); }
                            heading(ui, "ARMOR");
                            let wearing = |on: bool, cost: u32| if field { ("Base station only", false) } else if on { ("Wearing", false) }
                                else if !afford(cost) { ("Too dear", false) } else { ("Take", true) };
                            for (n, armor) in armors.iter().enumerate() {
                                let a = armor.def();
                                if row(ui, &(n + 1).to_string(), &a.name, &a.about, &price(a.price, String::new()), Some(wearing(me.armor == *armor, a.price))) { buy = BUY_ARMOR + armor.0 as u16; }
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                heading(ui, "FAVORITES · F1-F4 to use");
                                for (n, setup) in setups.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("F{}", n + 1)).monospace().color(MUTED));
                                        ui.label(RichText::new(setup.as_ref().map_or("Empty".to_string(), |s| s.describe())).size(13.0)
                                            .color(if setup.is_some() { FG } else { MUTED }));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.add(egui::Button::new(RichText::new("Save current").color(FG))).clicked() { save = Some(n); }
                                            if let Some(s) = setup {
                                                if ui.add(egui::Button::new(RichText::new("Use").color(FG)).min_size(Vec2::new(56.0, 22.0))).clicked() { equip = Some(s.equip()); }
                                            }
                                        });
                                    });
                                }
                            }
                            heading(ui, &format!("WEAPONS · {} of {} slots", me.loadout.len().min(slots), slots));
                            // The weapons this armor may carry that the mode allows.
                            for (k, _, w) in registry.weapons.iter() {
                                let k = k as u8;
                                if !w.offered || !arsenal::carries(me.armor, k) || !def.allows(k) { continue; }
                                let slot = me.loadout.kinds().position(|c| c == k);
                                let energy = w.energy > 0.0;
                                let note = match slot {
                                    Some(n) if energy => format!("Slot {} · energy", n + 1),
                                    Some(n) => format!("Slot {} · {}/{}", n + 1, me.ammo[k as usize], max[k as usize]),
                                    None => String::new(),
                                };
                                let button = if slot.is_some() { ("Leave", true) } else if me.loadout.len() >= slots { ("Slots full", false) }
                                    else if !afford(w.price) { ("Too dear", false) } else { ("Take", true) };
                                let note = if slot.is_none() { price(w.price, note) } else { note };
                                if row(ui, " ", &w.name, &w.about, &note, Some(button)) {
                                    buy = if slot.is_some() { buy_drop(k) } else { buy_take(k) };
                                }
                            }
                            if !packs.is_empty() {
                                heading(ui, "PACKS · one at a time");
                                for kind in packs.iter().copied() {
                                    let carrying = me.pack == Some(kind);
                                    // A pack that places shows how many its team has out (a full
                                    // team can still carry one, to place when one comes down).
                                    let note = match kind.deployable() {
                                        Some(d) => {
                                            let limit = self.world.team_limit(d, me.team);
                                            format!("{}/{} out{}", out(d), limit, if out(d) >= limit { " (full)" } else { "" })
                                        }
                                        None => match kind.def().use_ {
                                            PackUse::Toggle => "Switched".to_string(),
                                            PackUse::Use => "Used".to_string(),
                                            _ => "Always on".to_string(),
                                        },
                                    };
                                    let button = if field && kind.def().base_station_only { ("Base station only", false) } else if carrying { ("Carrying", false) }
                                        else if !afford(kind.def().price) { ("Too dear", false) } else { ("Take", true) };
                                    let note = if carrying { note } else { price(kind.def().price, note) };
                                    if row(ui, " ", kind.name(), &kind.def().about, &note, Some(button)) { buy = buy_for(kind); }
                                }
                            }
                            heading(ui, "MISCELLANY");
                            let keys = &self.ctl.keys;
                            for (slot, action) in [(0, Action::Grenade), (1, Action::Mine)] {
                                let Some((t, _, _)) = peakrunner_core::sim::throwables::thrown_by(me.armor, slot) else { continue };
                                row(ui, " ", &format!("{}s", t.name), &format!("{} key: {}", keys.name(action), t.about),
                                    &format!("{}/{}", me.throwables[slot], throws[slot]), None);
                            }
                            row(ui, " ", "Repair tool", &format!("Hold {}. Spends energy.", keys.name(Action::Repair)), "Carried", None);
                        });
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("{} or Esc closes · 1-{} armor · F1-F4 favorites · weapon keys pick slot 1, 2, 3…",
                            self.ctl.keys.name(Action::Use), armors.len())).size(11.0).color(MUTED));
                    });
            });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(n) = save { self.net.preferences.set_setup(n, Some(crate::preferences::SavedSetup::of(&me))); }
        if buy != BUY_NONE { self.world.input.buy = buy; }
        if equip.is_some() { self.world.input.equip = equip; }
        if close { self.close_shop(); }
    }

    /// Key bindings: click an action, then press its new key. A key already
    /// in use swaps onto the action's old key. Saved with the preferences.
    fn controls_ui(&mut self, ctx: &egui::Context) {
        use crate::keybinds::{Action, Keybinds};
        if let Some(action) = self.ctl.rebinding {
            let pressed = ctx.input_mut(|i| {
                let key = i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                    _ => None,
                });
                if key.is_some() { i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_))); }
                key
            });
            if let Some(key) = pressed {
                if key != egui::Key::Escape { self.ctl.keys.bind(action, key); }
                self.ctl.rebinding = None;
                self.save_keys();
            }
        }
        let mut open = true;
        let mut reset = false;
        egui::Window::new("Controls")
            .order(egui::Order::Foreground)
            .frame(egui::Frame::new().fill(Color32::from_rgb(16, 22, 32)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 70, 90))).corner_radius(10.0).inner_margin(16.0))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new("Click an action, then press a key. Esc cancels. Mouse: left fire, right jet. Arrow keys also move.")
                    .size(12.0).color(MUTED));
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 180.0).max(200.0)).show(ui, |ui| {
                    egui::Grid::new("keybinds").num_columns(2).spacing([24.0, 6.0]).striped(true).show(ui, |ui| {
                        for action in Action::ALL {
                            ui.label(RichText::new(action.label()).color(FG));
                            let waiting = self.ctl.rebinding == Some(action);
                            let text = if waiting { "Press a key…".to_string() } else if matches!(action, Action::Suicide | Action::TossFlag) {
                                format!("Ctrl+{}", self.ctl.keys.name(action)) } else { self.ctl.keys.name(action).to_string() };
                            if ui.add(egui::Button::new(RichText::new(text).monospace()).selected(waiting).min_size(Vec2::new(120.0, 24.0))).clicked() {
                                self.ctl.rebinding = if waiting { None } else { Some(action) };
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.ctl.keys.is_default(), egui::Button::new("Reset to defaults")).clicked() { reset = true; }
                });
            });
        if reset {
            self.ctl.keys = Keybinds::default();
            self.ctl.rebinding = None;
            self.save_keys();
        }
        if !open {
            self.ctl.keys_open = false;
            self.ctl.rebinding = None;
        }
    }

    /// Graphics screen: every setting applies at once and is saved.
    fn graphics_ui(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Graphics")
            .order(egui::Order::Foreground)
            .frame(egui::Frame::new().fill(Color32::from_rgb(16, 22, 32)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(40, 70, 90))).corner_radius(10.0).inner_margin(16.0))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(440.0);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use crate::preferences::{Bloom, RENDER_SCALES};
                    let mut g = self.net.preferences.saved.graphics;
                    let row = |ui: &mut egui::Ui, title: &str, detail: &str| {
                        ui.label(RichText::new(title).color(FG).size(14.0));
                        ui.label(RichText::new(detail).color(MUTED).size(11.0));
                    };
                    row(ui, "Display mode", "You can also switch with your system's fullscreen control.");
                    let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    ui.horizontal(|ui| {
                        for (label, value) in [("Windowed", false), ("Fullscreen", true)] {
                            if ui.selectable_label(full == value, label).clicked() && full != value {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(value));
                                g.fullscreen = value;
                            }
                        }
                    });
                    ui.add_space(8.0);
                    row(ui, "Render scale", "Draw the 3D scene at fewer pixels and stretch it; the HUD stays sharp. Lower helps slower GPUs and large screens.");
                    ui.horizontal(|ui| {
                        for s in RENDER_SCALES {
                            if ui.selectable_label(g.render_scale == s, format!("{s}%")).clicked() { g.render_scale = s; }
                        }
                    });
                    ui.add_space(8.0);
                    row(ui, "Vsync", "Show each frame on a display refresh: smooth, no tearing. Off lowers latency but can tear.");
                    ui.horizontal(|ui| {
                        for (label, value) in [("On", true), ("Off", false)] {
                            if ui.selectable_label(g.vsync == value, label).clicked() { g.vsync = value; }
                        }
                    });
                    ui.add_space(8.0);
                    row(ui, "Anti-aliasing", "Smoother edges (4x multisampling when the GPU supports it).");
                    let mut aa = scene::MSAA_WANTED.load(std::sync::atomic::Ordering::Relaxed);
                    ui.horizontal(|ui| {
                        for (label, value) in [("On", true), ("Off", false)] {
                            if ui.selectable_label(aa == value, label).clicked() && aa != value {
                                aa = value;
                                scene::MSAA_WANTED.store(aa, std::sync::atomic::Ordering::Relaxed);
                                self.net.preferences.set_antialiasing(aa);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    row(ui, "Glow (bloom)", "Glow around lights, flames and the sun.");
                    let current = self.net.preferences.saved.bloom;
                    ui.horizontal(|ui| {
                        for (value, label) in [(Bloom::Off, "Off"), (Bloom::Low, "Low"), (Bloom::High, "High")] {
                            if ui.selectable_label(current == value, label).clicked() && current != value {
                                scene::BLOOM_LEVEL.store(value.level(), std::sync::atomic::Ordering::Relaxed);
                                self.net.preferences.set_bloom(value);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    row(ui, "Show FPS", "Frame rate and frame timing in matches (also F3).");
                    ui.horizontal(|ui| {
                        for (label, value) in [("On", true), ("Off", false)] {
                            if ui.selectable_label(g.show_fps == value, label).clicked() { g.show_fps = value; }
                        }
                    });
                    self.show_frame_stats = g.show_fps;
                    if g != self.net.preferences.saved.graphics { self.net.preferences.set_graphics(g); }
                }
                #[cfg(target_arch = "wasm32")]
                ui.label(RichText::new("Graphics settings are available in the desktop game.").color(MUTED));
            });
        if !open { self.ctl.graphics_open = false; }
    }

    fn save_keys(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.net.preferences.set_keys(self.ctl.keys.clone());
    }

    fn pause_ui(&mut self, ui: &mut egui::Ui) {
        egui::Area::new(egui::Id::new("pause"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(16, 22, 32, 235))
                    .corner_radius(10.0)
                    .inner_margin(22.0)
                    .show(ui, |ui| {
                        ui.set_width(280.0);
                        ui.label(RichText::new(if self.online() { "Match menu" } else { "Paused" }).size(28.0).color(FG).strong());
                        if self.online() { ui.label(RichText::new("The match continues while this menu is open.").color(MUTED)); }
                        ui.add_space(12.0);
                        #[cfg(not(target_arch = "wasm32"))]
                        if self.online() {
                            ui.label(RichText::new("Your name").color(MUTED));
                            if ui.add(egui::TextEdit::singleline(&mut self.net.name).char_limit(24)).changed() {
                                self.net.settings_edited();
                            }
                            let valid = peakrunner_core::names::validate(&self.net.name).is_some();
                            ui.label(RichText::new(peakrunner_core::names::HELP).size(11.0).color(MUTED));
                            let ready = self.net.rename_sent.is_none_or(|sent| sent.elapsed().as_secs() >= 11);
                            if ui.add_enabled(valid && ready, egui::Button::new("Apply name")).clicked() {
                                if let Some(session) = &self.net.session {
                                    if session.rename(self.net.name.clone()) { self.net.rename_sent = Some(web_time::Instant::now()); }
                                }
                            }
                            if let Some(session) = &self.net.session {
                                let lobby = session.lobby();
                                if let Some(p) = lobby.snapshot.as_ref().and_then(|s| s.players.iter().find(|p| p.net_id == lobby.player_id)) {
                                    ui.label(RichText::new(format!("Current: {}", p.name)).color(FG));
                                }
                            }
                            ui.label(RichText::new(if ready { "One change every 10 seconds." } else { "Name requested. Please wait before changing again." }).size(11.0).color(MUTED));
                            ui.add_space(8.0);
                        }
                        if big(ui, "Resume", true) {
                            self.resume(ui.ctx());
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        if self.host_may_start() && big(ui, "Start match now", false) {
                            if let (Some(session), Some(server)) = (&self.net.session, &self.net.host.server) { session.start_match(&server.key); }
                            self.resume(ui.ctx());
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        if self.online() {
                            let key = self.ctl.keys.name(crate::keybinds::Action::Team);
                            if self.world.spectating {
                                if big(ui, &format!("Join a team ({key})"), false) { self.request_team_switch(); self.resume(ui.ctx()); }
                            } else {
                                if big(ui, &format!("Switch team ({key})"), false) { self.request_team_switch(); self.resume(ui.ctx()); }
                                if big(ui, "Spectate", false) {
                                    if let Some(session) = &self.net.session { session.spectate(true); }
                                    self.resume(ui.ctx());
                                }
                            }
                        }
                        if big(ui, "Controls", false) { self.ctl.keys_open = true; }
                        if big(ui, "Graphics", false) { self.ctl.graphics_open = true; }
                        let mute = if self.audio.muted() { "Unmute" } else { "Mute" };
                        if big(ui, mute, false) {
                            let next = !self.audio.muted();
                            self.audio.set_muted(next);
                        }
                        if big(ui, "Leave rift", false) {
                            self.menu(ui.ctx());
                        }
                    });
            });
    }

    fn result_summary(&self) -> (String, String, String) { result_summary(&self.world, self.hud.as_ref()) }

    /// Between rounds online: the result over the game, nothing to click
    /// (the next round starts by itself; Esc opens the menu).
    fn round_over_ui(&self, ui: &mut egui::Ui) {
        let (title, score, detail) = self.result_summary();
        egui::Area::new(egui::Id::new("round-over")).interactable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, -60.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(16, 22, 32, 220)).corner_radius(10.0).inner_margin(24.0).show(ui, |ui| {
                    ui.set_width(320.0);
                    ui.vertical_centered(|ui| {
                        #[cfg(not(target_arch = "wasm32"))]
                        let duel = crate::duel_hud::arena(&self.world);
                        #[cfg(target_arch = "wasm32")]
                        let duel = false;
                        ui.label(RichText::new(if duel { "DUEL OVER" } else { "ROUND OVER" }).size(12.0).color(MUTED));
                        ui.label(RichText::new(title).size(44.0).color(FG).strong());
                        ui.label(RichText::new(&score).size(if score.len() > 16 { 18.0 } else { 26.0 }).color(FG));
                        ui.label(RichText::new(detail).color(MUTED));
                        ui.add_space(8.0);
                        if !duel { ui.label(RichText::new("The next round starts by itself · Esc for the menu").size(12.0).color(MUTED)); }
                    });
                });
            });
    }

    fn end_ui(&mut self, ui: &mut egui::Ui) {
        let (title, score, detail) = self.result_summary();
        egui::Area::new(egui::Id::new("end"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(16, 22, 32, 238))
                    .corner_radius(10.0)
                    .inner_margin(28.0)
                    .show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.label(RichText::new("MATCH OVER").size(12.0).color(MUTED));
                        ui.label(RichText::new(&title).size(48.0).color(FG).strong());
                        ui.label(
                            RichText::new(&score)
                                .size(if score.len() > 16 { 18.0 } else { 28.0 })
                                .color(FG),
                        );
                        ui.label(RichText::new(detail).color(MUTED));
                        ui.add_space(12.0);
                        if self.online() {
                            ui.label(RichText::new(&self.world.message).color(GLACIER));
                        } else if big(ui, "Start match", true) {
                            self.start(ui.ctx());
                        }
                        if big(ui, "Menu", false) {
                            self.menu(ui.ctx());
                        }
                    });
            });
    }
}

/// The round's result: title, score line and this player's frags (or
/// points) and deaths. Free-for-all names the leader; Team Rabbit shows
/// the lives left.
fn result_summary(w: &World, hud: Option<&Hud>) -> (String, String, String) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(duel) = crate::duel_hud::result(w) { return duel; }
    let Some(h) = hud else { return ("Match over".into(), String::new(), String::new()) };
    if w.ffa() {
        let unit = if w.rabbit_mode() { "points" } else { "frags" };
        let (title, line) = match w.ffa_leader() {
            Some(i) if i == w.player_id && !w.spectating => ("You win".to_string(), format!("{} {unit}", w.players[i].frags)),
            Some(i) => (format!("{} wins", w.display_name(i)), format!("{} {unit}", w.players[i].frags)),
            None => ("Match over".to_string(), String::new()),
        };
        return (title, line, format!("{} {unit} · {} deaths", h.kills, h.deaths));
    }
    let title = if h.winner == -1 { "Draw" }
        else if w.spectating { if h.winner == 0 { "Ember wins" } else { "Glacier wins" } }
        else if h.winner == h.team as i32 { "Victory" } else { "Defeat" };
    let score = crate::rabbit_hud::lives_line(w).unwrap_or_else(|| format!("{}  –  {}", h.ember, h.glacier));
    (title.into(), score, format!("{} frags · {} deaths", h.kills, h.deaths))
}


fn play_hud(
    ui: &mut egui::Ui,
    hud: &Hud,
    touch: bool,
    stick: &mut [f32; 2],
) -> (bool, bool, bool, bool, egui::Vec2) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let kph = (hud.speed * 3.6).round() as i32;
    let mm = (hud.time / 60.0).floor() as i32;
    let ss = (hud.time % 60.0).floor() as i32;
    painter.text(
        rect.left_top() + Vec2::new(22.0, 18.0),
        Align2::LEFT_TOP,
        "SPEED",
        FontId::proportional(12.0),
        MUTED,
    );
    painter.text(
        rect.left_top() + Vec2::new(22.0, 32.0),
        Align2::LEFT_TOP,
        format!("{kph}"),
        FontId::proportional(42.0),
        FG,
    );
    let stance = if hud.ski == 1 {
        "Skiing"
    } else if hud.jet == 1 {
        "Jet"
    } else if hud.alive == 1 {
        "Planted"
    } else {
        "Down"
    };
    painter.text(
        rect.left_top() + Vec2::new(22.0, 78.0),
        Align2::LEFT_TOP,
        stance,
        FontId::proportional(13.0),
        MUTED,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 18.0),
        Align2::CENTER_TOP,
        match &hud.ffa_line { Some(line) if hud.clockless => line.clone(), Some(line) => format!("{line}    {mm}:{ss:02}"),
            None => format!("{}    {mm}:{ss:02}    {}", hud.ember, hud.glacier) },
        FontId::proportional(26.0),
        FG,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 50.0),
        Align2::CENTER_TOP,
        if hud.conditions.is_empty() { hud.goal.to_string() } else { format!("{} · {}", hud.goal, hud.conditions) },
        FontId::proportional(12.0),
        MUTED,
    );
    if !hud.msg.is_empty() {
        let msg = hud.msg.replace("[repair]", hud.keys.name(crate::keybinds::Action::Repair)).replace("[use]", hud.keys.name(crate::keybinds::Action::Use));
        let hud_msg_len = msg.chars().count();
        painter.text(
            rect.center_top() + Vec2::new(0.0, 92.0),
            Align2::CENTER_TOP,
            &msg,
            FontId::proportional(((rect.width()-32.0)/(hud_msg_len.max(1) as f32*0.6)).clamp(14.0,26.0)),
            FG,
        );
    }
    if hud.flag >= 0 {
        painter.text(
            rect.center_top() + Vec2::new(0.0, 124.0),
            Align2::CENTER_TOP,
            "You have the flag",
            FontId::proportional(15.0),
            EMBER,
        );
    } else if hud.own_flag == 2 {
        painter.text(
            rect.center_top() + Vec2::new(0.0, 124.0),
            Align2::CENTER_TOP,
            "Your flag is taken",
            FontId::proportional(15.0),
            EMBER,
        );
    } else if hud.own_flag == 3 {
        painter.text(
            rect.center_top() + Vec2::new(0.0, 124.0),
            Align2::CENTER_TOP,
            "Your flag is down · touch it to return it",
            FontId::proportional(15.0),
            EMBER,
        );
    }
    // The enemy flag lying loose is a chance: say so (the marker shows where).
    if hud.enemy_flag == 3 && hud.flag < 0 && !hud.football {
        painter.text(
            rect.center_top() + Vec2::new(0.0, if hud.own_flag == 1 { 124.0 } else { 144.0 }),
            Align2::CENTER_TOP,
            "Their flag is down · grab it",
            FontId::proportional(15.0),
            GLACIER,
        );
    }

    let c = rect.center();
    if hud.zoomed {
        // A sight: dark surround outside a circle, fine cross hairs.
        let r = rect.height().min(rect.width())*0.42;
        let shade = Color32::from_black_alpha(215);
        painter.rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(c.x-r, rect.max.y)), 0.0, shade);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(c.x+r, rect.min.y), rect.max), 0.0, shade);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(c.x-r, rect.min.y), egui::pos2(c.x+r, c.y-r)), 0.0, shade);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(c.x-r, c.y+r), egui::pos2(c.x+r, rect.max.y)), 0.0, shade);
        for k in 0..48 {
            let a0 = k as f32/48.0*std::f32::consts::TAU; let a1 = (k+1) as f32/48.0*std::f32::consts::TAU;
            // Corner wedges between the square and the circle.
            let corner = |a: f32| egui::pos2(c.x+a.cos()*r*1.42, c.y+a.sin()*r*1.42);
            painter.add(egui::Shape::convex_polygon(vec![egui::pos2(c.x+a0.cos()*r, c.y+a0.sin()*r), corner(a0), corner(a1),
                egui::pos2(c.x+a1.cos()*r, c.y+a1.sin()*r)], shade, egui::Stroke::NONE));
        }
        painter.circle_stroke(c, r, egui::Stroke::new(2.0, Color32::from_rgb(120, 220, 255)));
        // Its power, and the wheel for the other level, inside the lower right.
        let (power, other) = (hud.zoom_power, if hud.zoom_power > 6.0 { "wheel down: 4×" } else { "wheel up: 8×" });
        let at = egui::pos2(c.x + r * 0.6, c.y + r * 0.6);
        painter.text(at, Align2::CENTER_BOTTOM, format!("{power:.0}×"), FontId::monospace(20.0), Color32::from_rgb(120, 220, 255));
        painter.text(at + Vec2::new(0.0, 4.0), Align2::CENTER_TOP, other, FontId::proportional(12.0), FG);
        for (a, b) in [(egui::vec2(-r, 0.0), egui::vec2(-8.0, 0.0)), (egui::vec2(8.0, 0.0), egui::vec2(r, 0.0)),
            (egui::vec2(0.0, -r), egui::vec2(0.0, -8.0)), (egui::vec2(0.0, 8.0), egui::vec2(0.0, r))] {
            painter.line_segment([c+a, c+b], egui::Stroke::new(1.0, Color32::from_rgb(120, 220, 255)));
        }
    }
    let hit = if hud.hit > 0.0 { EMBER } else { FG };
    // Over the shoulder, the crosshair marks where the shot lands, which
    // drifts from the center when something is close.
    let c = hud.aim.map_or(c, |[x, y]| {
        let at = c + Vec2::new(x, -y) * rect.height() * 0.5;
        egui::pos2(at.x.clamp(rect.left() + 20.0, rect.right() - 20.0), at.y.clamp(rect.top() + 20.0, rect.bottom() - 20.0))
    });
    if let Some(blade) = &hud.blade {
        // The blade's mark: a dot in a ring; a power swing's charge fills
        // the ring, the guard shows as a bar under it.
        painter.circle_filled(c, 2.0, hit);
        painter.circle_stroke(c, 11.0, egui::Stroke::new(1.0, Color32::from_white_alpha(70)));
        if let Some(charge) = blade.charge {
            let n = (48.0 * charge).ceil() as usize;
            let points: Vec<egui::Pos2> = (0..=n).map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * charge * k as f32 / n.max(1) as f32;
                c + Vec2::new(a.cos(), a.sin()) * 11.0
            }).collect();
            let color = if charge >= 1.0 { Color32::from_rgb(255, 214, 120) } else { Color32::from_rgb(255, 170, 90) };
            painter.add(egui::Shape::line(points, egui::Stroke::new(3.0, color)));
        }
        if blade.guard {
            painter.line_segment([c + Vec2::new(-16.0, 18.0), c + Vec2::new(16.0, 18.0)], egui::Stroke::new(3.0, Color32::from_rgb(120, 200, 255)));
            painter.text(c + Vec2::new(0.0, 24.0), Align2::CENTER_TOP, "GUARD", FontId::proportional(12.0), Color32::from_rgb(120, 200, 255));
        }
        if blade.reel {
            painter.text(c + Vec2::new(0.0, 24.0), Align2::CENTER_TOP, "STAGGERED", FontId::proportional(12.0), EMBER);
        }
    } else {
    for (a, b) in [
        (c + Vec2::new(-14.0, 0.0), c + Vec2::new(-6.0, 0.0)),
        (c + Vec2::new(6.0, 0.0), c + Vec2::new(14.0, 0.0)),
        (c + Vec2::new(0.0, -14.0), c + Vec2::new(0.0, -6.0)),
        (c + Vec2::new(0.0, 6.0), c + Vec2::new(0.0, 14.0)),
    ] {
        painter.line_segment([a, b], egui::Stroke::new(1.5, hit));
    }
    }
    if let Some(angle) = hud.elevation { elevation_gauge(painter, c, angle); }

    // Status effects: their tint over the view, and a line each by the bars.
    if hud.tint[3] > 0.0 {
        let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u8;
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(c(hud.tint[0]), c(hud.tint[1]), c(hud.tint[2]), c(hud.tint[3])));
    }
    for (n, (name, left)) in hud.effects.iter().enumerate() {
        let text = if *left > 2.0 { format!("{name} · {left:.0} s") } else { name.to_string() };
        painter.text(rect.left_bottom() + Vec2::new(22.0, -110.0 - 18.0 * n as f32), Align2::LEFT_BOTTOM, text, FontId::proportional(13.0), GLACIER);
    }
    bar(ui, rect.left_bottom() + Vec2::new(22.0, -78.0), "Armor", hud.health, 100.0, EMBER);
    match &hud.blade {
        Some(blade) => bar(ui, rect.left_bottom() + Vec2::new(22.0, -48.0), "Stamina", blade.stamina, 100.0, Color32::from_rgb(240, 200, 90)),
        None => bar(ui, rect.left_bottom() + Vec2::new(22.0, -48.0), "Energy", hud.energy, peakrunner_core::defs::movement().energy_max, GLACIER),
    }
    if let (Some(blade), 1) = (&hud.blade, hud.alive) {
        // The tower's floor, and how the blade works.
        if !blade.place.is_empty() {
            painter.text(rect.left_bottom() + Vec2::new(214.0, -96.0), Align2::LEFT_BOTTOM, blade.place.to_uppercase(), FontId::proportional(14.0), FG);
        }
        painter.text(rect.left_bottom() + Vec2::new(214.0, -78.0), Align2::LEFT_BOTTOM,
            "Click swing · hold for power · click fast for combos", FontId::proportional(13.0), MUTED);
        painter.text(rect.left_bottom() + Vec2::new(214.0, -60.0), Align2::LEFT_BOTTOM,
            "Right-click guard · stamina pays for both", FontId::proportional(13.0), MUTED);
        painter.text(rect.left_bottom() + Vec2::new(214.0, -42.0), Align2::LEFT_BOTTOM,
            "A side game: it counts for nothing", FontId::proportional(12.0), MUTED);
    } else if hud.alive == 1 && !hud.football {
        let armor = peakrunner_core::sim::ArmorClass(hud.armor).def().name.as_str();
        use crate::keybinds::Action;
        let k = &hud.keys;
        let (text, color) = if hud.repairing == 1 { (format!("{armor} · repairing…"), Color32::from_rgb(120, 230, 140)) }
            else if hud.repairing == 2 { (format!("{armor} · repair tool waiting for energy…"), Color32::from_rgb(120, 160, 130)) }
            else if hud.repair_tool { (format!("{armor} · hold {} to repair", k.name(Action::Repair)), FG) }
            else { (armor.to_string(), FG) };
        painter.text(rect.left_bottom() + Vec2::new(214.0, -78.0), Align2::LEFT_BOTTOM, text, FontId::proportional(13.0), color);
        if !hud.pack_line.is_empty() {
            painter.text(rect.left_bottom() + Vec2::new(214.0, -60.0), Align2::LEFT_BOTTOM,
                &hud.pack_line, FontId::proportional(13.0), GLACIER);
        }
        if !hud.ability_line.is_empty() {
            let y = if hud.pack_line.is_empty() { -60.0 } else { -42.0 };
            painter.text(rect.left_bottom() + Vec2::new(214.0, y), Align2::LEFT_BOTTOM,
                &hud.ability_line, FontId::proportional(13.0), GLACIER);
        }
        if let Some(c) = hud.credits {
            painter.text(rect.left_bottom() + Vec2::new(214.0, -96.0), Align2::LEFT_BOTTOM, format!("Credits {:.0}", c.floor()),
                FontId::proportional(13.0), Color32::from_rgb(240, 210, 110));
        }
        if !hud.vehicle_line.is_empty() {
            painter.text(rect.center_bottom() + Vec2::new(0.0, -96.0), Align2::CENTER_BOTTOM, &hud.vehicle_line,
                FontId::proportional(15.0), GLACIER);
        }
        if !hud.command_line.is_empty() {
            painter.text(rect.center_bottom() + Vec2::new(0.0, -120.0), Align2::CENTER_BOTTOM, &hud.command_line,
                FontId::proportional(15.0), Color32::from_rgb(120, 240, 150));
        }
        if let Some(problem) = hud.placing {
            let (text, color) = if problem.is_empty() { ("Click to place".to_string(), GLACIER) }
                else { (problem.to_string(), MUTED) };
            painter.text(rect.center() + Vec2::new(0.0, 34.0), Align2::CENTER_TOP, text, FontId::proportional(15.0), color);
            painter.text(rect.center() + Vec2::new(0.0, 54.0), Align2::CENTER_TOP,
                format!("Wheel or {} turns · {} puts it away", k.name(Action::View), k.name(Action::Deploy)),
                FontId::proportional(12.0), MUTED);
        }
        // What the grenade and mine keys throw.
        let throws: Vec<String> = [(hud.throws[0], hud.grenades, hud.grenades_max, Action::Grenade), (hud.throws[1], hud.mines, hud.mines_max, Action::Mine)]
            .into_iter().filter(|(name, ..)| !name.is_empty())
            .map(|(name, n, max, key)| format!("{name}s {n}/{max} ({})", k.name(key))).collect();
        painter.text(rect.left_bottom() + Vec2::new(214.0, -42.0), Align2::LEFT_BOTTOM,
            throws.join(" · "), FontId::proportional(13.0), MUTED);
        let use_key = k.name(Action::Use);
        let station = match hud.station {
            1 => format!("INVENTORY STATION · {use_key} to open"),
            3 => format!("FIELD INVENTORY · {use_key} to open"),
            4 => "INVENTORY · step in from the front".to_string(),
            2 => format!("AMMO STATION · hold {use_key} to restock"),
            _ => String::new(),
        };
        if !station.is_empty() {
            painter.text(rect.center_bottom() + Vec2::new(0.0, -120.0), Align2::CENTER_BOTTOM, station, FontId::proportional(14.0), GLACIER);
        }
    }
    // The slots: number keys pick them; the one in hand is lit.
    if !hud.football && hud.alive == 1 && hud.slots.len() > 1 {
        let font = FontId::proportional(12.0);
        let parts: Vec<(String, bool)> = hud.slots.iter().enumerate().map(|(n, name)| (format!("{} {name}", n + 1), hud.held == Some(n))).collect();
        let gap = 14.0;
        let widths: Vec<f32> = parts.iter().map(|(t, _)| painter.layout_no_wrap(t.clone(), font.clone(), FG).size().x).collect();
        let total = widths.iter().sum::<f32>() + gap * (parts.len() - 1) as f32;
        let mut x = rect.center().x - total * 0.5;
        for ((text, on), w) in parts.into_iter().zip(widths) {
            painter.text(egui::pos2(x, rect.bottom() - 84.0), Align2::LEFT_BOTTOM, text, font.clone(), if on { FG } else { MUTED });
            x += w + gap;
        }
    }
    painter.text(
        rect.center_bottom() + Vec2::new(0.0, -56.0),
        Align2::CENTER_BOTTOM,
        &if hud.football { "Football".to_string() } else {
            let name = if hud.held_name.is_empty() { peakrunner_core::sim::arsenal::name(hud.weapon) } else { hud.held_name };
            let trigger = if hud.trigger.is_empty() { String::new() } else { format!(" · {}", hud.trigger) };
            if let Some(blade) = &hud.blade {
                match blade.charge { Some(c) => format!("Blade · power {:.0}%", c * 100.0), None => "Blade".to_string() }
            }
            else if hud.blade_area { "Blade".to_string() }
            else if hud.placing.is_some() { "Deployer".to_string() } else if !hud.armed { "No weapon".to_string() }
            else { format!("{name} · {}/{}{trigger}", hud.ammo, hud.ammo_max) }
        },
        FontId::proportional(22.0),
        FG,
    );
    painter.text(
        rect.center_bottom() + Vec2::new(0.0, -32.0),
        Align2::CENTER_BOTTOM,
        if hud.football || hud.points { format!("{} pts · {} down", hud.kills, hud.deaths) } else { format!("{} frag · {} down", hud.kills, hud.deaths) },
        FontId::proportional(13.0),
        MUTED,
    );
    if !touch {
        painter.text(
            rect.right_bottom() + Vec2::new(-22.0, -40.0),
            Align2::RIGHT_BOTTOM,
            if hud.flag >= 0 {
                // Carrying a flag: how to toss it to a teammate.
                format!("Ctrl+{} toss the flag · Ctrl+{} respawn · Esc\n{}", hud.keys.name(crate::keybinds::Action::TossFlag),
                    hud.keys.name(crate::keybinds::Action::Suicide), if hud.team == 0 { "Ember" } else { "Glacier" })
            } else {
                format!("Hold {} to ski · Right click {} · Ctrl+{} respawn · Esc\n{}", hud.keys.name(crate::keybinds::Action::Jump),
                    if hud.blade.is_some() { "guard" } else { "jet" },
                    hud.keys.name(crate::keybinds::Action::Suicide), if hud.team == 0 { "Ember" } else { "Glacier" })
            },
            FontId::proportional(13.0),
            MUTED,
        );
    }
    if let Some(text) = &hud.countdown { crate::countdown_hud::draw(ui, text, &hud.countdown_caption); }

    if touch {
        touch_controls(ui, stick)
    } else {
        (false, false, false, false, egui::Vec2::ZERO)
    }
}

/// Where the held weapon's shot will land, on screen from the third-person
/// camera, as offsets from the center in half screen heights (right, up).
/// The camera sits over the shoulder, off the line the shot flies, so up
/// close the two disagree; the crosshair goes where the shot goes.
pub(crate) fn third_person_aim(world: &World) -> Option<[f32; 2]> {
    use glam::Vec3;
    // The shot's line (the muzzle, steered onto the first-person crosshair)
    // to the first thing it meets, or far along it.
    let (from, dir) = world.aim_line(world.player_id)?;
    let far = from + dir * 600.0;
    let hit = world.blocked(from, far, 0.0, None).map_or(far, |t| from.lerp(far, t));
    // Seen from the over-the-shoulder camera: offsets from the screen's
    // center in half screen heights (the projection's vertical field of view).
    let (eye, look, fov) = world.camera();
    let right = look.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(look);
    let v = hit - eye;
    let depth = v.dot(look);
    if depth < 0.1 { return None; }
    let t = (fov.to_radians() * 0.5).tan() * depth;
    Some([v.dot(right) / t, v.dot(up) / t])
}

/// The elevation gauge beside the crosshair (a weapon's `elevation`): the
/// aim's angle above the horizon on a scale from -20 to 90 degrees, ticks
/// every 5, labels every 30, and 45 (the longest lob on level ground)
/// marked in amber; a pointer and the reading at the current angle.
fn elevation_gauge(painter: &egui::Painter, c: egui::Pos2, angle: f32) {
    const AMBER: Color32 = Color32::from_rgb(224, 160, 48);
    let (lo, hi) = (-20.0f32, 90.0f32);
    let x = c.x + 72.0;
    let (bottom, top) = (c.y + 60.0, c.y - 160.0);
    let y_of = |a: f32| bottom + (a.clamp(lo, hi) - lo) / (hi - lo) * (top - bottom);
    let ink = |alpha: u8| Color32::from_rgba_unmultiplied(FG.r(), FG.g(), FG.b(), alpha);
    painter.line_segment([egui::pos2(x, y_of(lo)), egui::pos2(x, y_of(hi))], egui::Stroke::new(1.0, ink(150)));
    for step in 0..=((hi - lo) / 5.0) as i32 {
        let a = lo + step as f32 * 5.0;
        let y = y_of(a);
        let (len, color) = if a == 45.0 { (12.0, AMBER) } else if a as i32 % 30 == 0 { (10.0, ink(220)) } else { (4.0, ink(140)) };
        painter.line_segment([egui::pos2(x, y), egui::pos2(x + len, y)], egui::Stroke::new(if len > 5.0 { 1.5 } else { 1.0 }, color));
        if a == 45.0 || a as i32 % 30 == 0 {
            painter.text(egui::pos2(x + len + 4.0, y), Align2::LEFT_CENTER, format!("{a:.0}"), FontId::monospace(11.0), color);
        }
    }
    // The pointer, on the scale's left, and the reading beside it.
    let y = y_of(angle);
    painter.add(egui::Shape::convex_polygon(vec![egui::pos2(x - 2.0, y), egui::pos2(x - 11.0, y - 5.0), egui::pos2(x - 11.0, y + 5.0)],
        GLACIER, egui::Stroke::new(1.0, Color32::BLACK)));
    let reading = format!("{:+.0}°", angle);
    painter.text(egui::pos2(x - 14.0, y), Align2::RIGHT_CENTER, reading, FontId::monospace(15.0), GLACIER);
}

fn bar(ui: &egui::Ui, origin: egui::Pos2, label: &str, value: f32, max: f32, color: Color32) {
    let painter = ui.painter();
    painter.text(origin, Align2::LEFT_TOP, format!("{label}  {:.0}", value), FontId::proportional(12.0), MUTED);
    let track = egui::Rect::from_min_size(origin + Vec2::new(0.0, 16.0), Vec2::new(180.0, 6.0));
    painter.rect_filled(track, 3.0, Color32::from_rgb(28, 36, 48));
    let mut fill = track;
    let span = if max > 0.0 { max } else { 1.0 };
    fill.max.x = track.min.x + track.width() * (value / span).clamp(0.0, 1.0);
    painter.rect_filled(fill, 3.0, color);
}

fn touch_controls(ui: &mut egui::Ui, stick: &mut [f32; 2]) -> (bool, bool, bool, bool, egui::Vec2) {
    let rect = ui.max_rect();
    let zone = egui::Rect::from_min_size(rect.left_bottom() + Vec2::new(16.0, -168.0), Vec2::splat(140.0));
    let drag = ui.interact(zone, ui.id().with("move"), Sense::drag());
    ui.painter().circle_stroke(zone.center(), 56.0, egui::Stroke::new(1.0, Color32::from_white_alpha(50)));
    if drag.dragged() {
        if let Some(p) = drag.interact_pointer_pos() {
            let d = p - zone.center();
            stick[0] = (d.x / 56.0).clamp(-1.0, 1.0);
            stick[1] = (-d.y / 56.0).clamp(-1.0, 1.0);
        }
    } else if !drag.is_pointer_button_down_on() {
        stick[0] = 0.0;
        stick[1] = 0.0;
    }
    let look = egui::Rect::from_min_max(
        egui::pos2(rect.center().x, rect.top()),
        rect.right_bottom() + Vec2::new(0.0, -150.0),
    );
    let look_drag = ui.interact(look, ui.id().with("look"), Sense::drag());
    let look = if look_drag.dragged() {
        look_drag.drag_delta() * 1.6
    } else {
        egui::Vec2::ZERO
    };

    let fire = hold_button(ui, rect.right_bottom() + Vec2::new(-28.0, -28.0), "Fire", EMBER);
    let jet = hold_button(ui, rect.right_bottom() + Vec2::new(-108.0, -28.0), "Jet", GLACIER);
    let jump = hold_button(ui, rect.right_bottom() + Vec2::new(-188.0, -28.0), "Ski", FG);
    let swap = tap_button(ui, rect.right_bottom() + Vec2::new(-268.0, -28.0), "Swap");
    (jump, fire, jet, swap, look)
}

fn hold_button(ui: &mut egui::Ui, center_br: egui::Pos2, label: &str, color: Color32) -> bool {
    let rect = egui::Rect::from_center_size(center_br + Vec2::new(-32.0, -32.0), Vec2::splat(64.0));
    let resp = ui.interact(rect, ui.id().with(label), Sense::click_and_drag());
    ui.painter().circle_filled(rect.center(), 32.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 180));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), BG);
    resp.is_pointer_button_down_on()
}

fn tap_button(ui: &mut egui::Ui, center_br: egui::Pos2, label: &str) -> bool {
    let rect = egui::Rect::from_center_size(center_br + Vec2::new(-28.0, -28.0), Vec2::splat(56.0));
    let resp = ui.interact(rect, ui.id().with(label), Sense::click());
    ui.painter().circle_stroke(rect.center(), 28.0, egui::Stroke::new(1.0, FG));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), FG);
    resp.clicked()
}

impl PeakRunnerApp {
    fn online(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        { self.net.dropped_in }
        #[cfg(target_arch = "wasm32")]
        { false }
    }

    fn poll_net(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.poll_host();
            if let Some(inbox) = &self.net.inbox {
                if let Ok(result) = inbox.try_recv() {
                    self.net.listing = match result {
                        Ok(servers) => Listing::Ready(servers),
                        Err(err) => Listing::Failed(format!("Directory unavailable: {err}. Direct joining still works.")),
                    };
                    self.net.inbox = None;
                }
            }
            if let Some(inbox) = &self.net.probe_inbox {
                for (id, probe) in inbox.try_iter() { self.net.probes.insert(id, probe); }
            }
            if let Some(session) = &self.net.session { self.net.lobby = session.lobby(); }
            let now = _ctx.input(|i| i.time);
            self.ctl.take_notice(&self.net.lobby, now);
            if let Some(error) = self.net.lobby.error.clone() {
                if self.net.session.is_some() {
                    self.net.session.take();
                    self.net.dropped_in = false;
                    self.mode = Mode::Lobby;
                    self.world.state = MatchState::Paused;
                    self.audio.set_jet(false);
                    self.grab(_ctx, false);
                    self.net.lobby.error = Some(error);
                }
                return;
            }
            if self.net.session.is_some() && self.net.lobby.connected {
                if let Some(snapshot) = self.net.lobby.snapshot.clone() {
                    if !self.net.dropped_in {
                        self.net.predictor = crate::online::Online::default();
                        self.net.dropped_in = true;
                        self.world.players.clear();
                        self.world.input = Default::default();
                        self.mode = Mode::Play;
                        self.wait_fire_release = true;
                        self.audio.unlock();
                        self.audio.play("start");
                    }
                    self.net.predictor.receive(&mut self.world, &snapshot, self.net.lobby.player_id);
                    self.map = snapshot.map;
                    self.ember = self.world.players.get(self.world.player_id).is_some_and(|p| p.team == crate::sim::Team::Ember);
                    // Between rounds online the game stays in play: the mouse
                    // stays locked and the result is an overlay, so a shot
                    // fired as the round ends can't click anything (it once
                    // hit the end screen's Menu and left the match). Esc
                    // still opens the menu.
                    if self.mode == Mode::End {
                        self.mode = Mode::Play;
                        self.wait_fire_release = true;
                    }
                }
            }
        }
    }

    /// Ask the server to put this player on the other team, or (spectating)
    /// on a team; a refusal comes back as a notice.
    #[cfg(not(target_arch = "wasm32"))]
    fn request_team_switch(&mut self) {
        let Some(session) = &self.net.session else { return };
        if session.switch_team() && !self.world.spectating { self.net.predictor.expect_switch(); }
    }

    /// Spectating: who is being watched, the score, and the controls.
    #[cfg(not(target_arch = "wasm32"))]
    fn spectator_hud(&self, ui: &egui::Ui) {
        use crate::keybinds::Action;
        let w = &self.world;
        let k = |a| self.ctl.keys.name(a);
        let rect = ui.max_rect();
        let painter = ui.painter();
        if !w.message.is_empty() {
            painter.text(rect.center_top() + Vec2::new(0.0, 112.0), Align2::CENTER_TOP, &w.message, FontId::proportional(22.0), FG);
        }
        let clock = w.time_left.max(0.0) as u32;
        let score = match crate::duel_hud::top_line(w, k(Action::Vote)) {
            Some((line, true)) => format!("{line}  ·  {}:{:02}", clock / 60, clock % 60),
            Some((line, false)) => line,
            None => format!("EMBER {}  ·  {}:{:02}  ·  {} GLACIER", w.score[0], clock / 60, clock % 60, w.score[1]),
        };
        painter.text(rect.center_top() + Vec2::new(0.0, 24.0), Align2::CENTER_TOP, score, FontId::proportional(20.0), FG);
        let watched = self.ctl.spectator.watching(w);
        let color = watched.map_or(GLACIER, |p| if p.team == crate::sim::Team::Ember { EMBER } else { GLACIER });
        let base = rect.center_bottom() - Vec2::new(0.0, 28.0);
        painter.text(base - Vec2::new(0.0, 44.0), Align2::CENTER_BOTTOM, format!("SPECTATING · {}", self.ctl.spectator.label(w)),
            FontId::proportional(20.0), color);
        let join = crate::duel_hud::spectator_hint(w, k(Action::Team), k(Action::Vote))
            .unwrap_or_else(|| crate::spectate::join_hint(self.net.lobby.snapshot.as_deref(), self.net.lobby.player_id, k(Action::Team)));
        let hints = if watched.is_some() {
            format!("Click / right-click: next / previous player · {}: shoulder / orbit · {}: free camera · {join}",
                k(Action::View), k(Action::Jump))
        } else {
            format!("WASD: fly · {} / Shift: up / down · Ctrl: faster · Click: follow a player · {join}", k(Action::Jump))
        };
        painter.text(base, Align2::CENTER_BOTTOM, hints, FontId::proportional(14.0), MUTED);
    }

    /// A server notice (why a team change was refused), for a few seconds.
    fn notice_ui(&mut self, ui: &egui::Ui) {
        let now = ui.input(|i| i.time);
        let Some((text, until)) = &self.ctl.notice else { return };
        if now > *until { self.ctl.notice = None; return; }
        ui.painter().text(ui.max_rect().center_top() + Vec2::new(0.0, 148.0), Align2::CENTER_TOP,
            text.to_uppercase(), FontId::proportional(18.0), VOTE_AMBER);
    }

    fn open_browser(&mut self) {
        self.mode = Mode::Browser;
        #[cfg(not(target_arch = "wasm32"))]
        self.refresh_servers();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn refresh_servers(&mut self) {
        self.net.save_settings();
        let addr = self.net.directory.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (probe_tx, probe_rx) = std::sync::mpsc::channel();
        self.net.inbox = Some(rx);
        self.net.probe_inbox = Some(probe_rx);
        self.net.probes.clear();
        self.net.listing = Listing::Loading;
        let favorites = self.net.preferences.saved.favorites.clone();
        std::thread::spawn(move || {
            let result = peakrunner_net::browse(&addr).map_err(|err| err.to_string());
            let servers = result.as_ref().map(Vec::clone).unwrap_or_default();
            let _ = tx.send(result);
            // Then ask each server (and each favorite that isn't listed) for
            // its status, all at once; the table fills in as answers come.
            let mut targets: Vec<(String, String)> = servers.iter().filter(|s| s.host.starts_with("quic://"))
                .map(|s| (s.id.clone(), s.join_address())).collect();
            targets.extend(favorites.into_iter().filter(|f| !servers.iter().any(|s| s.id == f.id) && encrypted_address(&f.address))
                .map(|f| (f.id, f.address)));
            for (id, address) in targets {
                let probe_tx = probe_tx.clone();
                std::thread::spawn(move || { let _ = probe_tx.send((id, peakrunner_net::probe(&address).map_err(|e| e.to_string()))); });
            }
        });
    }

    fn browser_ui(&mut self, ui: &mut egui::Ui) {
        // The server list stays put near the top while it filters and fills.
        #[cfg(not(target_arch = "wasm32"))]
        let (anchor, offset) = (Align2::CENTER_TOP, Vec2::new(0.0, 36.0));
        #[cfg(target_arch = "wasm32")]
        let (anchor, offset) = (Align2::CENTER_CENTER, Vec2::ZERO);
        egui::Area::new(egui::Id::new("browser"))
            .anchor(anchor, offset)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 18, 28, 236))
                    .corner_radius(10.0)
                    .inner_margin(22.0)
                    .show(ui, |ui| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            ui.set_width(460.0);
                            ui.label(RichText::new("FIND A RIFT").size(12.0).color(GLACIER));
                            ui.label(RichText::new("Open games").size(28.0).color(FG).strong());
                            ui.add_space(8.0);
                            ui.label(RichText::new("Match listing runs in the desktop game.").color(MUTED));
                            if big(ui, "Back", false) {
                                self.mode = Mode::Menu;
                            }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        self.browser_native(ui);
                    });
            });
    }

    /// Find match: every listed server (and every favorite) in one sortable
    /// table, with what it runs and whether this game can join it.
    #[cfg(not(target_arch = "wasm32"))]
    fn browser_native(&mut self, ui: &mut egui::Ui) {
        use egui_extras::{Column, TableBuilder};
        let screen = ui.ctx().content_rect();
        ui.set_width((screen.width() - 64.0).clamp(640.0, 1180.0));
        // Text here isn't for selecting: a click anywhere on a row picks it.
        ui.style_mut().interaction.selectable_labels = false;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("FIND A RIFT").size(12.0).color(GLACIER));
                ui.label(RichText::new("Open games").size(28.0).color(FG).strong());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::TextEdit::singleline(&mut self.net.name).char_limit(24).desired_width(180.0))
                    .on_hover_text(peakrunner_core::names::HELP).changed() {
                    self.net.settings_edited();
                }
                let ok = peakrunner_core::names::validate(&self.net.name).is_some();
                ui.label(RichText::new(if ok { "Playing as" } else { "Name: 1–24 letters, numbers or spaces" })
                    .size(12.0).color(if ok { MUTED } else { EMBER }));
            });
        });

        let rows = self.net.server_rows();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.net.browse.filter).hint_text("Filter: name, map, mode or mod").desired_width(230.0));
            let modes: std::collections::BTreeSet<String> = rows.iter().map(|r| r.mode.clone()).filter(|m| !m.is_empty()).collect();
            egui::ComboBox::from_id_salt("browse-mode").width(110.0)
                .selected_text(self.net.browse.mode.clone().unwrap_or_else(|| "All modes".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.net.browse.mode, None, "All modes");
                    for mode in modes { ui.selectable_value(&mut self.net.browse.mode, Some(mode.clone()), mode); }
                });
            // Toggles that light up when on.
            for (on, label) in [(&mut self.net.browse.favorites_only, "★ Favorites only"), (&mut self.net.browse.hide_empty, "Hide empty"),
                (&mut self.net.browse.hide_full, "Hide full")] {
                if ui.selectable_label(*on, RichText::new(label).size(13.0)).clicked() { *on = !*on; }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let listed = rows.iter().filter(|r| r.listed);
                let (servers, players) = listed.fold((0, 0), |(n, p), r| (n + 1, p + r.players));
                ui.label(RichText::new(format!("{servers} servers · {players} players")).size(12.0).color(MUTED));
            });
        });
        ui.add_space(6.0);
        let visible = self.net.browse.visible(rows);
        let status = match &self.net.listing {
            Listing::Failed(err) => Some((err.clone(), EMBER)),
            Listing::Idle | Listing::Loading if visible.is_empty() => Some(("Looking for games…".into(), MUTED)),
            Listing::Ready(_) if visible.is_empty() => Some(("No servers match. Clear the filters, or start one with Host game.".into(), MUTED)),
            _ => None,
        };
        if let Some((text, color)) = status { ui.label(RichText::new(text).size(13.0).color(color)); }

        // Location only when there's room for it.
        let wide = width >= 1040.0;
        let height = (screen.height() - if self.net.browse.by_address { 440.0 } else { 340.0 }).clamp(140.0, 560.0);
        let (mut sort, selected) = (self.net.browse.sort, self.net.browse.selected.clone());
        let (mut pick, mut star, mut join) = (None, None, None);
        let mut table = TableBuilder::new(ui).id_salt("servers").striped(true).sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .min_scrolled_height(height).max_scroll_height(height).auto_shrink([false, false])
            .column(Column::exact(22.0)).column(Column::exact(32.0))
            .column(Column::remainder().at_least(150.0).clip(true))
            .column(Column::exact(44.0)).column(Column::exact(66.0))
            .column(Column::exact(150.0).clip(true)).column(Column::exact(62.0))
            .column(Column::exact(84.0).clip(true)).column(Column::exact(206.0).clip(true));
        if wide { table = table.column(Column::exact(100.0).clip(true)); }
        let headers = [(SortBy::Favorite, "★"), (SortBy::Status, ""), (SortBy::Name, "SERVER"), (SortBy::Ping, "PING"), (SortBy::Mode, "MODE"),
            (SortBy::Map, "MAP"), (SortBy::Players, "PLAYERS"), (SortBy::Version, "VERSION"), (SortBy::Mods, "MODS"), (SortBy::Location, "LOCATION")];
        table.header(22.0, |mut header| {
            for &(by, title) in headers.iter().take(if wide { 10 } else { 9 }) {
                header.col(|ui| sort_header(ui, &mut sort, by, title));
            }
        }).body(|mut body| {
            for row in &visible {
                body.row(26.0, |mut cells| {
                    cells.set_selected(selected.as_deref() == Some(row.id.as_str()));
                    let dim = matches!(row.state, RowState::Unreachable | RowState::Incompatible);
                    let text = |t: &str| RichText::new(t).size(13.0).color(if dim { MUTED } else { FG });
                    let (_, starred) = cells.col(|ui| {
                        ui.label(RichText::new(if row.favorite { "★" } else { "☆" }).size(15.0).color(if row.favorite { VOTE_AMBER } else { MUTED }));
                    });
                    if starred.clicked() { star = Some(row.clone()); }
                    cells.col(|ui| status_cell(ui, row));
                    cells.col(|ui| { ui.label(text(&row.name).strong()); });
                    cells.col(|ui| {
                        let (t, c) = match row.ping { Some(p) => (p.to_string(), if p > 150 { VOTE_AMBER } else if p <= 80 { OK_GREEN } else { FG }), None => ("—".into(), MUTED) };
                        ui.label(RichText::new(t).size(13.0).monospace().color(c));
                    });
                    cells.col(|ui| { ui.label(text(&row.mode)); });
                    cells.col(|ui| { ui.label(text(if row.map.is_empty() { "—" } else { &row.map })); });
                    cells.col(|ui| {
                        let t = if row.max == 0 { "—".into() } else { format!("{}/{}", row.players, row.max) };
                        let count = ui.label(RichText::new(t).size(13.0).monospace().color(if dim { MUTED } else if row.players > 0 { GLACIER } else { FG }));
                        if row.bots > 0 {
                            let people = row.players.saturating_sub(row.bots);
                            count.on_hover_text(format!("{people} {} and {} {}: each player who joins takes a bot's place",
                                if people == 1 { "person" } else { "people" }, row.bots, if row.bots == 1 { "bot" } else { "bots" }));
                        }
                    });
                    cells.col(|ui| {
                        let short = row.version.strip_prefix("0.1.0-playtest.").unwrap_or(&row.version);
                        let color = if row.state == RowState::Incompatible { EMBER } else { MUTED };
                        ui.label(RichText::new(if short.is_empty() { "—" } else { short }).size(12.0).monospace().color(color)).on_hover_text(&row.version);
                    });
                    cells.col(|ui| {
                        if row.mods.is_empty() && row.download == 0 {
                            let t = match row.state { RowState::Querying => "…", RowState::Unreachable | RowState::Lan => "—", _ => "base" };
                            ui.label(RichText::new(t).size(12.0).color(MUTED));
                        } else {
                            if !row.mods.is_empty() { ui.label(RichText::new(row.mods.join(", ")).size(12.5).color(MOD_ORANGE)); }
                            if row.download > 0 { ui.label(RichText::new(format!("+{}", size_text(row.download))).size(11.5).monospace().color(MUTED)); }
                        }
                    });
                    if wide { cells.col(|ui| { ui.label(RichText::new(&row.location).size(12.0).color(MUTED)); }); }
                    let response = cells.response();
                    if response.double_clicked() { join = Some(row.clone()); } else if response.clicked() { pick = Some(row.id.clone()); }
                });
            }
        });
        self.net.browse.sort = sort;
        if let Some(id) = pick { self.net.browse.selected = Some(id); }
        if let Some(row) = star { self.net.preferences.toggle_favorite(row.favorite_entry()); }

        // The selected server: where it is, what joining takes.
        let chosen = self.net.browse.selected.as_ref().and_then(|id| visible.iter().find(|r| &r.id == id)).cloned();
        ui.add_space(10.0);
        match &chosen {
            Some(row) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&row.name).size(15.0).color(FG).strong());
                    ui.label(RichText::new(&row.address).size(11.5).monospace().color(MUTED));
                    if row.community { ui.label(RichText::new("· community host, key checked by the directory").size(12.0).color(MUTED)); }
                    if !row.listed { ui.label(RichText::new("· favorite, not listed now").size(12.0).color(MUTED)); }
                });
                let note = match row.state {
                    RowState::Incompatible => Some((format!("Runs PeakRunner {}: this game can't join it until you're on the same version.", row.version), EMBER)),
                    RowState::Unreachable => Some(("Not answering right now; it may be offline.".into(), EMBER)),
                    RowState::Querying => Some(("Checking the server…".into(), MUTED)),
                    _ if !row.mods.is_empty() || row.download > 0 => Some((
                        if row.download == 0 { format!("Runs {}. You have everything it needs.", row.mods.join(", ")) }
                        else { format!("Runs {}. Joining downloads {} from it, after you agree, and keeps it for next time.",
                            if row.mods.is_empty() { "extra maps".into() } else { row.mods.join(", ") }, size_text(row.download)) }, MOD_ORANGE)),
                    _ => None,
                };
                if let Some((text, color)) = note { ui.label(RichText::new(text).size(12.5).color(color)); }
                if row.password {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("This server needs a password").size(12.5).color(VOTE_AMBER));
                        ui.add(egui::TextEdit::singleline(&mut self.net.password).password(true).desired_width(200.0));
                    });
                }
            }
            None => { ui.label(RichText::new("Select a server to see more, or double-click one to join. Click ☆ to keep it in your favorites.").size(12.5).color(MUTED)); }
        }
        ui.checkbox(&mut self.net.spectate, "Join as a spectator (watch; join a team later)");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("Back").size(15.0).color(FG)).fill(SECONDARY_BUTTON_BG).min_size(Vec2::new(90.0, 32.0))).clicked() {
                self.net.save_settings();
                self.mode = Mode::Menu;
            }
            if ui.add(egui::Button::new(RichText::new("Refresh").size(15.0).color(FG)).fill(SECONDARY_BUTTON_BG).min_size(Vec2::new(90.0, 32.0))).clicked() {
                self.refresh_servers();
            }
            let label = if self.net.browse.by_address { "Hide address" } else { "Join by address…" };
            if ui.add(egui::Button::new(RichText::new(label).size(15.0).color(FG)).fill(SECONDARY_BUTTON_BG).min_size(Vec2::new(140.0, 32.0))).clicked() {
                self.net.browse.by_address = !self.net.browse.by_address;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can = chosen.as_ref().is_some_and(ServerRow::joinable);
                let button = egui::Button::new(RichText::new("Join game").size(16.0).color(BG).strong()).fill(GLACIER).min_size(Vec2::new(150.0, 34.0));
                if ui.add_enabled(can, button).clicked() { join = chosen.clone(); }
            });
        });
        if self.net.browse.by_address {
            let mut go = false;
            ui.add_space(8.0);
            egui::Frame::new().fill(Color32::from_rgba_unmultiplied(4, 8, 14, 200)).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Server address or invite link").size(12.0).color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut self.net.direct).char_limit(2048).desired_width(380.0)).changed() { self.net.settings_edited(); }
                    go = ui.button("Join").clicked();
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Password").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.net.password).password(true).desired_width(160.0));
                    ui.label(RichText::new("Directory").size(12.0).color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut self.net.directory).char_limit(2048).desired_width(300.0)).changed() { self.net.settings_edited(); }
                });
                let note = self.net.preferences.warning.clone()
                    .unwrap_or_else(|| "quic://host:port, a peakrunner://join/ invite link, or a private IP:port. Settings save automatically; passwords are never saved.".into());
                ui.label(RichText::new(note).size(11.0).color(MUTED));
            });
            if go { let address = self.net.direct.trim().to_string(); self.join_address(&address); }
        }
        ui.add_space(4.0);
        ui.label(RichText::new("To host, use Multiplayer, then Host game, on the main menu (or the PeakRunner Server app).").size(11.0).color(MUTED));
        if let Some(row) = join { self.join_row(&row); }
    }

    /// Join a row of the server list: a listed server as listed, a favorite
    /// that isn't by the address it was saved with.
    #[cfg(not(target_arch = "wasm32"))]
    fn join_row(&mut self, row: &ServerRow) {
        if row.listed { self.join_server(&row.address, row.port); } else { self.join_address(&row.address); }
    }

    /// Join a typed address: quic:// or an invite link, else a private IP:port.
    #[cfg(not(target_arch = "wasm32"))]
    fn join_address(&mut self, address: &str) {
        if encrypted_address(address) {
            self.join_server(address, 7777);
        } else if let Some((host, port)) = address.rsplit_once(':').and_then(|(h, p)| p.parse::<u16>().ok().map(|p| (h, p))) {
            self.join_server(host, port);
        } else {
            self.net.listing = Listing::Failed("Use quic://play.peakrunner.net:7777, a peakrunner://join/ invite link, or a private IP:port".into());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn join_server(&mut self, host: &str, port: u16) {
        self.join(host, port, true);
    }

    /// Join `host`; `remember` saves it as the direct address (not for this
    /// client's own server, whose pinned local address is for this run only).
    #[cfg(not(target_arch = "wasm32"))]
    fn join(&mut self, host: &str, port: u16, remember: bool) {
        if peakrunner_core::names::validate(&self.net.name).is_none() {
            self.net.listing = Listing::Failed(format!("Your name: {}", peakrunner_core::names::HELP));
            return;
        }
        let password = if remember { self.net.password.clone() } else { self.net.host.password.clone() };
        let options = peakrunner_net::JoinOptions { spectate: self.net.spectate && remember,
            auto_download: u64::from(self.net.preferences.saved.auto_download_mb) * 1_000_000,
            player: self.net.preferences.player_id() };
        match peakrunner_net::connect_with(host, port, &self.net.name, &password, options) {
            Ok(session) => {
                self.net.host.in_own = !remember;
                if remember {
                    self.net.direct = if host.contains("://") { host.into() } else { format!("{host}:{port}") };
                    self.net.settings_edited();
                }
                self.net.save_settings();
                self.net.lobby = Default::default();
                self.net.session = Some(session);
                self.net.dropped_in = false;
                self.mode = Mode::Lobby;
            }
            Err(err) => self.net.listing = Listing::Failed(err.to_string()),
        }
    }

    fn lobby_ui(&mut self, ui: &mut egui::Ui) {
        egui::Area::new(egui::Id::new("lobby"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 18, 28, 236))
                    .corner_radius(10.0)
                    .inner_margin(22.0)
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            let lobby = self.net.lobby.clone();
                            let title = if lobby.error.is_some() {
                                "Disconnected".to_string()
                            } else if lobby.match_name.is_empty() {
                                "Connecting".to_string()
                            } else {
                                lobby.match_name.clone()
                            };
                            ui.label(RichText::new(if lobby.error.is_some() { "DISCONNECTED" } else { "IN THE RIFT" }).size(12.0).color(GLACIER));
                            ui.label(RichText::new(title).size(28.0).color(FG).strong());
                            if let Some(err) = &lobby.error {
                                ui.label(RichText::new(err).color(EMBER));
                                ui.label(RichText::new("Rejoining starts a new player session.").color(MUTED));
                                if ui.button("Reconnect").clicked() {
                                    let address = self.net.direct.clone();
                                    if self.net.host.in_own { self.net.host.joining = true; }
                                    else if encrypted_address(&address) { self.join_server(&address, 7777); }
                                    else if let Some((host, port)) = address.rsplit_once(':').and_then(|(h, p)| p.parse::<u16>().ok().map(|p| (h, p))) {
                                        self.join_server(host, port);
                                    }
                                }
                            } else if lobby.connected {
                                ui.label(RichText::new(format!("{} · {} skiers", lobby.map, lobby.players.len())).color(MUTED));
                            } else if let Some(download) = &lobby.download {
                                // The server needs maps this game lacks: ask, then show progress.
                                let mb = size_text;
                                if download.asking {
                                    let count = download.maps.len();
                                    ui.label(RichText::new(format!("This server needs {count} {} you don't have:",
                                        if count == 1 { "download" } else { "downloads" })).color(FG));
                                    for (name, bytes) in &download.maps {
                                        ui.label(RichText::new(format!("· {name} ({})", mb(*bytes))).color(FG));
                                    }
                                    ui.label(RichText::new("Maps and mods are data only and are checked before use.").size(12.0).color(MUTED));
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        if ui.button(format!("Download {} and join", mb(download.total))).clicked() {
                                            if let Some(session) = &self.net.session { session.answer_download(true); }
                                        }
                                        if ui.button("Cancel").clicked() {
                                            if let Some(session) = &self.net.session { session.answer_download(false); }
                                        }
                                    });
                                } else {
                                    let part = if download.total == 0 { 0.0 } else { download.done as f32 / download.total as f32 };
                                    ui.label(RichText::new(format!("Downloading {}…", if download.current.is_empty() { "maps" } else { &download.current })).color(FG));
                                    ui.add(egui::ProgressBar::new(part.clamp(0.0, 1.0)).text(format!("{} of {}", mb(download.done), mb(download.total))));
                                }
                            } else {
                                ui.label(RichText::new("Connecting to the server…").color(MUTED));
                            }
                            ui.add_space(8.0);
                            for name in &lobby.players {
                                ui.label(RichText::new(format!("· {name}")).color(FG));
                            }
                            ui.add_space(12.0);
                        }
                        if big(ui, "Leave", false) {
                            self.leave_lobby();
                        }
                    });
            });
    }

    fn leave_lobby(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.net.disconnect();
        self.world.state = MatchState::Flyby;
        self.audio.set_jet(false);
        #[cfg(not(target_arch = "wasm32"))]
        { self.mode = if self.net.host.in_own { Mode::Host } else { Mode::Browser }; }
        #[cfg(target_arch = "wasm32")]
        { self.mode = Mode::Browser; }
    }

    /// Playing on this client's own server while its match waits for players.
    #[cfg(not(target_arch = "wasm32"))]
    fn host_may_start(&self) -> bool {
        self.net.host.in_own && self.net.host.server.is_some() && self.online() && !self.world.spectating
            && self.net.lobby.snapshot.as_ref().is_some_and(|s| s.phase == crate::sim::Phase::Waiting)
    }

    /// Follow the hosted server: join it once it listens, and say why if it
    /// stopped first.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_host(&mut self) {
        let Some(server) = self.net.host.server.as_mut() else { return };
        let running = server.running();
        if !self.net.host.joining { return; }
        if let Some(address) = server.join_address().filter(|_| running) {
            let port = server.port;
            self.net.host.joining = false;
            self.join(&address, port, false);
            if self.net.session.is_none() {
                if let Listing::Failed(why) = &self.net.listing { self.net.host.error = Some(why.clone()); }
            }
        } else if !running {
            self.net.host.joining = false;
            self.net.host.error = Some(format!("{} Its messages are below.", server.exited.clone().unwrap_or_default()));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn host_ui(&mut self, ui: &mut egui::Ui) {
        use peakrunner_core::map_catalog::maps_for;
        let screen = ui.ctx().content_rect();
        egui::Area::new(egui::Id::new("host"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 18, 28, 236))
                    .corner_radius(10.0)
                    .inner_margin(22.0)
                    .show(ui, |ui| {
                ui.set_width(540.0_f32.min(screen.width() - 80.0));
                let tall = (screen.height() - 90.0).max(240.0);
                egui::ScrollArea::vertical().max_height(tall).min_scrolled_height(tall).show(ui, |ui| {
                ui.label(RichText::new("HOST A GAME").size(12.0).color(GLACIER));
                ui.label(RichText::new("Your own server").size(28.0).color(FG).strong());
                ui.label(RichText::new("This starts the PeakRunner Server app on this computer and puts you in its match. \
                    Start the match whenever you like, even alone; others can join any time.").size(12.0).color(MUTED));
                ui.add_space(8.0);
                let server_app = crate::host::server_app();
                let running = self.net.host.server.as_mut().is_some_and(|s| s.running());
                let net = &mut self.net;
                let mut name_edited = false;
                ui.add_enabled_ui(!running, |ui| {
                    ui.label(RichText::new("Your name").size(12.0).color(MUTED));
                    name_edited = ui.add(egui::TextEdit::singleline(&mut net.name).char_limit(24)).changed();
                    let host = &mut net.host;
                    let s = &mut host.settings;
                    ui.label(RichText::new("Server name").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.name).char_limit(64));
                    ui.label(RichText::new("Location (optional, e.g. Denver, US)").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.location).char_limit(40));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Mode").size(12.0).color(MUTED));
                    ui.horizontal_wrapped(|ui| {
                        for mode in crate::host::modes() {
                            if ui.selectable_label(s.mode == mode, mode.label()).clicked() { s.set_mode(mode); }
                        }
                    });
                    // Duels draw their map at random: no order to number.
                    let duels = s.mode == peakrunner_core::map_catalog::SupportedMode::Duel;
                    ui.label(RichText::new(if duels { "Maps for duels: click to add or remove" } else { "Maps, in play order: click to add or remove" })
                        .size(12.0).color(MUTED));
                    ui.horizontal_wrapped(|ui| {
                        for map in maps_for(s.mode) {
                            let name = terrain::info(map).name;
                            let label = match s.maps.iter().position(|&m| m == map) {
                                Some(i) if !duels => format!("{} · {name}", i + 1),
                                _ => name.to_string(),
                            };
                            if ui.selectable_label(s.maps.contains(&map), label).clicked() { s.toggle_map(map); }
                        }
                    });
                    let order: Vec<&str> = s.maps.iter().map(|&m| terrain::info(m).name).collect();
                    ui.label(RichText::new(if order.is_empty() { "Pick at least one map.".into() }
                        else if duels { format!("Duels play on: {}", order.join(", ")) }
                        else { format!("Rotation: {}", order.join(", then ")) })
                        .size(12.0).color(if order.is_empty() { EMBER } else { FG }));
                    if duels {
                        ui.label(RichText::new("A duel server: everyone meets in Tenfold Tower, ten floors of blade fights, and challenges \
                            people or bots from the duel board (V). Each duel plays on one of these maps, drawn at random, many at once.")
                            .size(11.0).color(MUTED));
                    }
                    if let Some(folder) = &host.folder {
                        ui.label(RichText::new(format!("Your own maps and mods go in {} (read when the game starts; players download them on joining).",
                            folder.join("content").display())).size(11.0).color(MUTED));
                    }
                    ui.add_space(6.0);
                    // Each round's sky: the mode's own (the map's look;
                    // deathmatch rolls its own), random, or always one.
                    sky_picks(ui, "host-sky", &mut s.forecast);
                    ui.add_space(6.0);
                    ui.add(egui::Slider::new(&mut s.max_players, 2..=peakrunner_core::sim::MAX_PLAYERS as u32).text("max players"));
                    let most = s.max_players as usize & !1;
                    s.bots = s.bots.min(most) & !1;
                    let bots = if s.mode == peakrunner_core::map_catalog::SupportedMode::Duel { "sparring bots in the tower (people take their places)" }
                        else { "bots (people take their places)" };
                    ui.add(egui::Slider::new(&mut s.bots, 0..=most).step_by(2.0).text(bots));
                    ui.label(RichText::new("Match password (optional, not saved)").size(12.0).color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut host.password).password(true).char_limit(64));
                    ui.label(RichText::new("UDP port").size(12.0).color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut host.port).char_limit(5).desired_width(80.0)).changed() {
                        s.port = host.port.trim().parse().unwrap_or(0);
                    }
                    ui.add_space(4.0);
                    // A clearly outlined box: checked announces, unchecked doesn't.
                    ui.scope(|ui| {
                        let widgets = &mut ui.visuals_mut().widgets;
                        widgets.inactive.bg_stroke = egui::Stroke::new(1.5, MUTED);
                        widgets.hovered.bg_stroke = egui::Stroke::new(1.5, FG);
                        ui.checkbox(&mut s.announce, "Announce publicly: list this server under Find match");
                    });
                    ui.label(RichText::new(if s.announce { "Anyone can find and join it." }
                        else { "Not listed: players join with the invite link shown once it's running (the server list is asked once for your public address, to write it)." }).size(11.0).color(MUTED));
                });
                if name_edited { self.net.settings_edited(); }
                ui.add_space(10.0);
                let problem = if server_app.is_none() {
                    Some("The server app isn't installed next to this game. Download a full PeakRunner package, or run the PeakRunner Server app yourself.")
                } else if peakrunner_core::names::validate(&self.net.name).is_none() {
                    Some("Your name: 1–24 letters, digits and spaces.")
                } else { self.net.host.settings.problem() };
                ui.horizontal(|ui| {
                    if running {
                        if !self.net.host.in_own || self.net.session.is_none() {
                            if ui.add(egui::Button::new(RichText::new("Join").color(BG)).fill(FG).min_size(Vec2::new(120.0, 40.0))).clicked() {
                                self.net.host.error = None;
                                self.net.host.joining = true;
                            }
                        }
                        if ui.add(secondary_button("Stop server").min_size(Vec2::new(140.0, 40.0))).clicked() {
                            if self.net.host.in_own { self.net.disconnect(); }
                            if let Some(server) = self.net.host.server.as_mut() { server.stop(); }
                            self.net.host.joining = false;
                        }
                    } else if ui.add_enabled(problem.is_none(), egui::Button::new(RichText::new("Host and join").color(BG)).fill(FG)
                        .min_size(Vec2::new(160.0, 40.0))).clicked() {
                        self.host_and_join(server_app.as_deref());
                    }
                    if ui.add(secondary_button("Back").min_size(Vec2::new(100.0, 40.0))).clicked() {
                        self.net.save_settings();
                        self.mode = Mode::Menu;
                    }
                });
                if let Some(why) = problem.filter(|_| !running) { ui.label(RichText::new(why).size(12.0).color(EMBER)); }
                if let Some(error) = &self.net.host.error { ui.label(RichText::new(error).size(12.0).color(EMBER)); }
                let Some(server) = self.net.host.server.as_ref() else { return };
                ui.add_space(8.0);
                let state = if running {
                    if self.net.host.joining { "starting".to_string() }
                    else { format!("running on UDP port {}", server.port) }
                } else { server.exited.clone().unwrap_or_default() };
                ui.label(RichText::new(format!("{} · {} · {}", server.name, if server.announce { "announced" } else { "unlisted" }, state)).color(FG));
                if let Some(link) = server.invite() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Invite link").size(12.0).color(MUTED));
                        if ui.small_button("Copy").clicked() { ui.ctx().copy_text(link.clone()); }
                    });
                    ui.add(egui::Label::new(RichText::new(&link).monospace().size(11.0).color(FG)).wrap());
                }
                ui.label(RichText::new("Closing PeakRunner stops your server. Players on the internet need UDP port forwarding \
                    to this computer; the server tries UPnP and says below whether it worked.").size(11.0).color(MUTED));
                ui.add_space(4.0);
                ui.label(RichText::new("Server messages").size(12.0).color(MUTED));
                egui::Frame::new().fill(Color32::from_rgb(8, 12, 18)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("host-log").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let lines = server.lines();
                        if lines.is_empty() { ui.label(RichText::new("…").color(MUTED)); }
                        for line in lines { ui.add(egui::Label::new(RichText::new(line).monospace().size(11.0).color(FG)).wrap()); }
                    });
                });
                });
                    });
            });
    }

    /// Start the server app on the Host screen's settings and join it.
    #[cfg(not(target_arch = "wasm32"))]
    fn host_and_join(&mut self, server_app: Option<&std::path::Path>) {
        self.net.save_settings();
        let host = &mut self.net.host;
        host.error = None;
        match (server_app, host.folder.as_deref()) {
            (Some(app), Some(folder)) => match crate::host::Hosted::start(app, folder, &host.settings, &host.password) {
                Ok(server) => { host.server = Some(server); host.joining = true; }
                Err(e) => host.error = Some(format!("Couldn't start the server: {e}")),
            },
            _ => host.error = Some("No settings folder to keep the server's files in.".into()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct NetUi {
    preferences: crate::preferences::Store,
    settings_changed: Option<web_time::Instant>,
    rename_sent: Option<web_time::Instant>,
    password: String,
    direct: String,
    predictor: crate::online::Online,
    directory: String,
    name: String,
    listing: Listing,
    inbox: Option<std::sync::mpsc::Receiver<Result<Vec<peakrunner_net::ServerAdvert>, String>>>,
    /// Each listed server's (and favorite's) ping, version, mods and download
    /// size, by listing ID, as its status replies arrive.
    probes: std::collections::HashMap<String, Result<peakrunner_net::Probe, String>>,
    probe_inbox: Option<std::sync::mpsc::Receiver<(String, Result<peakrunner_net::Probe, String>)>>,
    /// The server list's selection, filters and sort.
    browse: BrowseUi,
    session: Option<peakrunner_net::Session>,
    lobby: peakrunner_net::Lobby,
    dropped_in: bool,
    /// Join the next match as a spectator (not saved).
    spectate: bool,
    host: HostUi,
}

/// The Host screen and the server it started (`host`).
#[cfg(not(target_arch = "wasm32"))]
struct HostUi {
    settings: crate::host::Settings,
    /// The match password for the next start; never saved.
    password: String,
    port: String,
    /// `PeakRunner/host`, beside the client's settings.
    folder: Option<std::path::PathBuf>,
    server: Option<crate::host::Hosted>,
    /// Join the server as soon as it listens.
    joining: bool,
    error: Option<String>,
    /// The current session is on this client's own server.
    in_own: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl HostUi {
    fn new(folder: Option<std::path::PathBuf>) -> Self {
        let settings = crate::host::load(folder.as_deref());
        Self { port: settings.port.to_string(), settings, password: String::new(), folder, server: None,
            joining: false, error: None, in_own: false }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl NetUi {
    fn load() -> Self {
        let mut net=Self::new();
        net.preferences=crate::preferences::Store::user();
        net.name=net.preferences.saved.name.clone();
        net.directory=net.preferences.saved.directory.clone();
        net.direct=net.preferences.saved.direct.clone();
        net.host=HostUi::new(crate::host::folder(net.preferences.path.as_deref()));
        scene::MSAA_WANTED.store(net.preferences.saved.antialiasing,std::sync::atomic::Ordering::Relaxed);
        scene::BLOOM_LEVEL.store(net.preferences.saved.bloom.level(),std::sync::atomic::Ordering::Relaxed);
        net
    }

    fn settings_edited(&mut self) {self.settings_changed=Some(web_time::Instant::now());}

    fn save_settings(&mut self) {
        if self.settings_changed.take().is_some() {
            self.preferences.save(&self.name,&self.directory,&self.direct);
        }
    }

    fn disconnect(&mut self) {
        self.rename_sent = None;
        if let Some(session) = self.session.take() { session.leave(); }
        self.lobby = Default::default();
        self.predictor = Default::default();
        self.dropped_in = false;
    }

    fn new() -> Self {
        Self {
            preferences: crate::preferences::Store::memory(),
            settings_changed: None,
            rename_sent: None,
            password: String::new(),
            direct: "quic://play.peakrunner.net:7777".into(),
            predictor: Default::default(),
            directory: "https://dir.peakrunner.net/servers".into(),
            name: "Skier".into(),
            listing: Listing::Idle,
            inbox: None,
            probes: Default::default(),
            probe_inbox: None,
            browse: BrowseUi::default(),
            session: None,
            lobby: peakrunner_net::Lobby::default(),
            dropped_in: false,
            spectate: false,
            host: HostUi::new(None),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for NetUi {
    fn drop(&mut self) {self.save_settings();}
}

/// Addresses that join over encrypted QUIC: `quic://host:port`, pinned
/// `quic://host:port#sha256=<hex>`, or an invite link
/// `peakrunner://join/host:port#sha256=<hex>`. Anything else is a LAN IP:port.
#[cfg(not(target_arch = "wasm32"))]
fn encrypted_address(address: &str) -> bool {
    let address = address.trim();
    address.starts_with("quic://") || address.starts_with("peakrunner://")
}

/// The server list's colors beyond the palette: a healthy ping and a mod.
#[cfg(not(target_arch = "wasm32"))]
const OK_GREEN: Color32 = Color32::from_rgb(90, 212, 138);
#[cfg(not(target_arch = "wasm32"))]
const MOD_ORANGE: Color32 = Color32::from_rgb(240, 132, 90);

/// A server list column to sort by.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortBy { Favorite, Status, Name, Ping, Mode, Map, Players, Version, Mods, Location }

/// What the list knows of a server, best first (the status column's order).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowState { Ready, Slow, Querying, Lan, Incompatible, Unreachable }

/// One line of the server list: a listed server, or a favorite that isn't.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct ServerRow {
    id: String, name: String, address: String, port: u16, listed: bool, community: bool,
    map: String, mode: String, players: u32, max: u32, bots: u32, location: String, favorite: bool,
    state: RowState, ping: Option<u32>, version: String, password: bool, mods: Vec<String>, download: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl ServerRow {
    fn joinable(&self) -> bool { self.state != RowState::Incompatible }

    /// Bots count as players, but each person who joins takes a bot's
    /// place: full means full of people.
    fn full(&self) -> bool { self.max > 0 && self.players.saturating_sub(self.bots) >= self.max }

    /// How it's saved as a favorite: a LAN server keeps its port.
    fn favorite_entry(&self) -> crate::preferences::Favorite {
        let address = if self.listed && !encrypted_address(&self.address) { format!("{}:{}", self.address, self.port) } else { self.address.clone() };
        crate::preferences::Favorite { id: self.id.clone(), name: self.name.clone(), address }
    }

    fn probed(&mut self, probe: Option<&Result<peakrunner_net::Probe, String>>) {
        match probe {
            None => self.state = RowState::Querying,
            Some(Err(_)) => self.state = RowState::Unreachable,
            Some(Ok(p)) => {
                (self.ping, self.version, self.password) = (Some(p.ping_ms), p.version.clone(), p.password);
                // Its own count, fresher than the directory's copy.
                if p.max_players > 0 { (self.players, self.max, self.bots) = (p.players, p.max_players, p.bots); }
                (self.mods, self.download) = (p.offer.mods.clone(), p.offer.download);
                self.state = if !p.compatible { RowState::Incompatible } else if p.ping_ms > 150 { RowState::Slow } else { RowState::Ready };
            }
        }
    }
}

/// A download size: KB below a megabyte ("+0.0 MB" said nothing).
#[cfg(not(target_arch = "wasm32"))]
fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000 { format!("{:.1} MB", bytes as f64 / 1e6) } else { format!("{} KB", bytes.div_ceil(1000).max(1)) }
}

/// "Old Holler - CTF" as (map, mode).
#[cfg(not(target_arch = "wasm32"))]
fn split_label(label: &str) -> (String, String) {
    match label.rsplit_once(" - ") { Some((map, mode)) => (map.trim().into(), mode.trim().into()), None => (label.trim().into(), String::new()) }
}

/// The server list's selection, filters and sort (for this run only).
#[cfg(not(target_arch = "wasm32"))]
struct BrowseUi {
    selected: Option<String>, filter: String, mode: Option<String>,
    favorites_only: bool, hide_empty: bool, hide_full: bool,
    /// Column, and descending.
    sort: (SortBy, bool),
    /// The join-by-address panel is open.
    by_address: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for BrowseUi {
    fn default() -> Self {
        Self { selected: None, filter: String::new(), mode: None, favorites_only: false, hide_empty: false, hide_full: false,
            sort: (SortBy::Players, true), by_address: false }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl BrowseUi {
    /// The rows the filters let through, sorted.
    fn visible(&self, rows: Vec<ServerRow>) -> Vec<ServerRow> {
        let needle = self.filter.trim().to_lowercase();
        let mut shown: Vec<ServerRow> = rows.into_iter().filter(|r| {
            (!self.favorites_only || r.favorite) && (!self.hide_empty || r.players > 0) && (!self.hide_full || !r.full())
                && self.mode.as_ref().is_none_or(|m| &r.mode == m)
                && (needle.is_empty() || [&r.name, &r.map, &r.mode, &r.location].iter().any(|t| t.to_lowercase().contains(&needle))
                    || r.mods.iter().any(|m| m.to_lowercase().contains(&needle)))
        }).collect();
        sort_rows(&mut shown, self.sort.0, self.sort.1);
        shown
    }
}

/// Sort by a column (then by name); servers with no ping go last going up.
#[cfg(not(target_arch = "wasm32"))]
fn sort_rows(rows: &mut [ServerRow], by: SortBy, descending: bool) {
    rows.sort_by(|a, b| {
        let order = match by {
            SortBy::Favorite => b.favorite.cmp(&a.favorite),
            SortBy::Status => (a.state as u8).cmp(&(b.state as u8)),
            SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortBy::Ping => a.ping.unwrap_or(u32::MAX).cmp(&b.ping.unwrap_or(u32::MAX)),
            SortBy::Mode => a.mode.cmp(&b.mode),
            SortBy::Map => a.map.cmp(&b.map),
            SortBy::Players => (a.players, a.max).cmp(&(b.players, b.max)),
            SortBy::Version => a.version.cmp(&b.version),
            SortBy::Mods => (a.mods.is_empty(), a.mods.join(",")).cmp(&(b.mods.is_empty(), b.mods.join(","))),
            SortBy::Location => a.location.cmp(&b.location),
        };
        (if descending { order.reverse() } else { order }).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// A column title that sorts by it (again: the other way). The arrow and
/// the status column's dot are drawn: the UI font has no such glyphs.
#[cfg(not(target_arch = "wasm32"))]
fn sort_header(ui: &mut egui::Ui, sort: &mut (SortBy, bool), by: SortBy, title: &str) {
    let active = sort.0 == by;
    let color = if active { GLACIER } else { MUTED };
    let mut response = ui.add(egui::Label::new(RichText::new(title).size(11.0).strong().color(color)).sense(Sense::click()).selectable(false));
    if by == SortBy::Status {
        let (rect, dot) = ui.allocate_exact_size(Vec2::new(10.0, 12.0), Sense::click());
        ui.painter().circle_filled(rect.center(), 3.5, color);
        response = response.union(dot);
    }
    if active {
        let (rect, arrow) = ui.allocate_exact_size(Vec2::new(10.0, 12.0), Sense::click());
        let (c, h) = (rect.center(), 3.5);
        let points = if sort.1 { vec![c + Vec2::new(-h, -h / 2.0), c + Vec2::new(h, -h / 2.0), c + Vec2::new(0.0, h)] }
            else { vec![c + Vec2::new(-h, h / 2.0), c + Vec2::new(h, h / 2.0), c + Vec2::new(0.0, -h)] };
        ui.painter().add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
        response = response.union(arrow);
    }
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *sort = if active { (by, !sort.1) } else { (by, by == SortBy::Players) };
    }
}

/// The status dot (green ready, amber slow, gray checking, red can't join),
/// and a lock for a password.
#[cfg(not(target_arch = "wasm32"))]
fn status_cell(ui: &mut egui::Ui, row: &ServerRow) {
    let color = match row.state {
        RowState::Ready => OK_GREEN, RowState::Slow => VOTE_AMBER, RowState::Querying => Color32::from_rgb(88, 98, 110),
        RowState::Lan => GLACIER, RowState::Incompatible => EMBER, RowState::Unreachable => Color32::from_rgb(120, 58, 52),
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 12.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    if row.password { ui.label(RichText::new("🔒").size(11.0).color(VOTE_AMBER)); }
}

#[cfg(not(target_arch = "wasm32"))]
impl NetUi {
    /// The listing's servers, then favorites that aren't in it, with what
    /// each one's status said.
    fn server_rows(&self) -> Vec<ServerRow> {
        let favorites = &self.preferences.saved.favorites;
        let blank = |id: &str, name: &str, address: String| ServerRow {
            id: id.into(), name: name.into(), address, port: 7777, listed: false, community: false, map: String::new(), mode: String::new(),
            players: 0, max: 0, bots: 0, location: String::new(), favorite: favorites.iter().any(|f| f.id == id), state: RowState::Querying,
            ping: None, version: String::new(), password: false, mods: Vec::new(), download: 0,
        };
        let mut rows = Vec::new();
        if let Listing::Ready(servers) = &self.listing {
            for s in servers {
                let (map, mode) = split_label(&s.map);
                let mut row = ServerRow { port: s.port, listed: true, community: s.sha256.is_some(), map, mode, players: s.players,
                    max: s.max_players, bots: s.bots.min(s.players), location: s.location.clone(), ..blank(&s.id, &s.name, s.join_address()) };
                if s.host.starts_with("quic://") { row.probed(self.probes.get(&s.id)); } else { row.state = RowState::Lan; }
                rows.push(row);
            }
        }
        let listed: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        for f in favorites.iter().filter(|f| !listed.contains(&f.id)) {
            let mut row = ServerRow { community: f.address.contains("#sha256="), ..blank(&f.id, &f.name, f.address.clone()) };
            if !encrypted_address(&f.address) { row.state = RowState::Lan; rows.push(row); continue; }
            let probe = self.probes.get(&f.id);
            row.probed(probe);
            // Not listed: what it says about itself.
            if let Some(Ok(p)) = probe {
                if !p.name.is_empty() { row.name = p.name.clone(); }
                (row.map, row.mode) = split_label(&p.map);
                (row.players, row.max, row.bots) = (p.players, p.max_players, p.bots);
            }
            rows.push(row);
        }
        rows
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum Listing {
    Idle,
    Loading,
    Ready(Vec<peakrunner_net::ServerAdvert>),
    Failed(String),
}

#[cfg(not(target_arch = "wasm32"))]
fn rift_roster(ui: &egui::Ui, lobby: &peakrunner_net::Lobby) {
    ui.painter().text(
        ui.max_rect().center_top() + Vec2::new(0.0, 78.0),
        Align2::CENTER_TOP,
        format!("{} · {} player{} · Tab: scoreboard", lobby.match_name, lobby.players.len(), if lobby.players.len() == 1 { "" } else { "s" }),
        egui::FontId::proportional(12.0),
        MUTED,
    );
}

/// Time of day and weather drop-downs, each as the mode does, random each
/// round, or always one (`conditions::Forecast`), in one row.
fn sky_picks(ui: &mut egui::Ui, id: &str, forecast: &mut peakrunner_core::conditions::Forecast) {
    use peakrunner_core::conditions::{Pick, TimeOfDay, Weather};
    fn shown(name: &str) -> String { if name == "map" { "Map's own".into() } else { format!("{}{}", name[..1].to_uppercase(), &name[1..]) } }
    fn pick<T: Copy + PartialEq>(ui: &mut egui::Ui, id: String, label: &str, pick: &mut Pick<T>, all: &[(T, &'static str)]) {
        ui.label(RichText::new(label).size(12.0).color(MUTED));
        egui::ComboBox::from_id_salt(id).width(96.0).selected_text(shown(pick.name(all))).show_ui(ui, |ui| {
            for option in [Pick::Mode, Pick::Random].into_iter().chain(all.iter().map(|&(v, _)| Pick::Always(v))) {
                ui.selectable_value(pick, option, shown(option.name(all)));
            }
        });
    }
    let row = ui.horizontal(|ui| {
        pick(ui, format!("{id}-time"), "Time of day", &mut forecast.time, &TimeOfDay::ALL);
        ui.add_space(10.0);
        pick(ui, format!("{id}-weather"), "Weather", &mut forecast.weather, &Weather::ALL);
    });
    row.response.on_hover_text("Default keeps the map's own look (Team Deathmatch rolls its own). Random picks again each round. Only the look changes.");
}

fn team_button(ui: &mut egui::Ui, label: &str, on: bool, color: Color32, mut set: impl FnMut()) {
    let fill = if on { color } else { Color32::from_rgb(22, 28, 38) };
    let text = if on && label == "Glacier" { BG } else { FG };
    if ui.add(egui::Button::new(RichText::new(label).color(text)).fill(fill).min_size(Vec2::new(96.0, 36.0))).clicked() {
        set();
    }
}

fn hint(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(k).size(11.0).color(MUTED));
        ui.label(RichText::new(v).color(FG));
    });
}

fn secondary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).size(18.0).color(FG)).fill(SECONDARY_BUTTON_BG)
}

fn big(ui: &mut egui::Ui, label: &str, primary: bool) -> bool {
    let button = if primary {
        egui::Button::new(RichText::new(label).color(BG)).fill(FG)
    } else {
        secondary_button(label)
    };
    ui.add_sized(Vec2::new(280.0, 40.0), button).clicked()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod exit_tests {
    use super::*;

    #[test]
    fn closing_net_ui_saves_settings_but_never_password() {
        let dir=std::env::temp_dir().join(format!("peakrunner-netui-{}-{}",std::process::id(),
            web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).unwrap().as_nanos()));
        let path=dir.join("client.json");
        {
            let mut net=NetUi::new();
            net.preferences=crate::preferences::Store::load(path.clone());
            net.name="Rift Pilot".into();
            net.directory="https://example.net/servers".into();
            net.direct="quic://example.net:7777".into();
            net.password="never-store-this".into();
            net.settings_edited();
        }
        let saved=crate::preferences::Store::load(path.clone());
        assert_eq!(saved.saved.name,"Rift Pilot");
        assert_eq!(saved.saved.directory,"https://example.net/servers");
        assert_eq!(saved.saved.direct,"quic://example.net:7777");
        assert!(!std::fs::read_to_string(path).unwrap().contains("never-store-this"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }
    fn frame(app: &mut PeakRunnerApp, ctx: &egui::Context, events: Vec<egui::Event>, time: f64) {
        let mut output = ctx.run_ui(egui::RawInput { events, time:Some(time), focused:true,
            screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,Vec2::new(1280.,800.))), ..Default::default() }, |ui| {
            app.step(ui.ctx(), 0.0); app.chat_ui(ui.ctx());
        });
        output.textures_delta.clear();
    }
    #[test]
    fn an_online_round_end_keeps_playing_with_nothing_to_click() {
        // Online, the end of a round used to open the end screen, freeing the
        // mouse under a big Menu button: a shot fired as the round ended left
        // the match. Now the game stays in play; offline still shows it.
        let mut app=PeakRunnerApp::comms_fixture(); let ctx=egui::Context::default();
        app.net.dropped_in=true; app.mode=Mode::Play; app.world.state=MatchState::Ended;
        for t in 0..3 { frame(&mut app,&ctx,vec![],t as f64); }
        assert_eq!(app.mode,Mode::Play,"online: no end screen between rounds");
        let mut offline=PeakRunnerApp::comms_fixture();
        offline.net.dropped_in=false; offline.mode=Mode::Play; offline.world.state=MatchState::Ended;
        for t in 0..3 { frame(&mut offline,&ctx,vec![],t as f64); }
        assert_eq!(offline.mode,Mode::End,"offline: the end screen with Start match and Menu");
    }

    #[test]
    fn typing_chat_blocks_gameplay_and_enter_sends_escape_cancels() {
        let mut app=PeakRunnerApp::comms_fixture(); let ctx=egui::Context::default();
        app.chat_open=false; app.chat_text.clear();
        frame(&mut app,&ctx,vec![key(egui::Key::Enter),key(egui::Key::Space)],0.);
        assert!(!app.chat_open,"gameplay keys and Enter must not open chat");
        frame(&mut app,&ctx,vec![key(egui::Key::T),egui::Event::Text("t".into())],1.);
        assert!(app.chat_open);
        assert!(!app.chat_team); assert!(app.chat_text.is_empty());
        frame(&mut app,&ctx,vec![key(egui::Key::W),key(egui::Key::Space),key(egui::Key::Num3),egui::Event::Text("hello".into())],2.);
        assert_eq!(app.world.input.move_z,0.); assert!(!app.world.input.fire && !app.world.input.jump);
        assert_eq!(app.world.input.weapon,0); assert!(!app.grabbed);
        assert_eq!(app.chat_text,"hello");
        frame(&mut app,&ctx,vec![key(egui::Key::Enter)],3.);
        assert!(!app.chat_open,"Enter must submit the message");
        assert!(app.world.feed.last().unwrap().line().ends_with(": hello"));
        frame(&mut app,&ctx,vec![key(egui::Key::Y)],4.);
        assert!(app.chat_open);
        assert!(app.chat_team);
        frame(&mut app,&ctx,vec![key(egui::Key::Escape)],5.);
        assert!(!app.chat_open && app.mode==Mode::Play,"Escape closes chat, not the match");
    }

    #[test]
    #[ignore = "measurement"]
    fn probe_key_release_stop_by_direction() {
        use glam::Vec3; let hv = |v: Vec3| (v.x * v.x + v.z * v.z).sqrt();
        for k in [egui::Key::W, egui::Key::S, egui::Key::A, egui::Key::D] {
            let mut app=PeakRunnerApp::comms_fixture(); let ctx=egui::Context::default();
            app.chat_open=false;
            app.world.set_map(MapId::Longfield); app.world.set_mode(peakrunner_core::map_catalog::SupportedMode::Ctf);
            app.world.start_match(true); app.world.players.truncate(1);
            let at = Vec3::new(1000.0, 0.0, 1024.0);
            app.world.players[0].pos = Vec3::new(at.x, peakrunner_core::terrain::support_on(MapId::Longfield, at + Vec3::Y*200.0).0 + peakrunner_core::terrain::PLAYER_RADIUS + 0.02, at.z);
            let ev = |pressed| egui::Event::Key { key: k, physical_key: Some(k), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
            let run = |app: &mut PeakRunnerApp, t: f64, events: Vec<egui::Event>| { let mut out = ctx.run_ui(egui::RawInput { events, time: Some(t), focused: true,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280., 800.))), ..Default::default() }, |ui| app.step(ui.ctx(), 1.0/60.0)); out.textures_delta.clear(); };
            run(&mut app, 0.0, vec![ev(true)]);
            for f in 1..90 { run(&mut app, f as f64/60.0, vec![]); }
            let v0 = hv(app.world.players[0].vel);
            run(&mut app, 1.5, vec![ev(false)]);
            let mut f = 0; while hv(app.world.players[0].vel) > 0.1 && f < 600 { f += 1; run(&mut app, 1.5 + f as f64/60.0, vec![]); }
            println!("{k:?}: speed {v0:.2} m/s, stops in {:.2} s", f as f32/60.0);
        }
    }

    #[test]
    fn a_press_of_g_or_n_throws_at_full_strength() {
        use peakrunner_core::sim::throwables::{GRENADE_KIND, MINE_KIND, THROW_GRENADE};
        let mut app=PeakRunnerApp::comms_fixture(); let ctx=egui::Context::default();
        app.chat_open=false;
        // Three quick presses, even within one frame: three throws, none lost.
        let before = app.world.players[app.world.player_id].throwables;
        let up = egui::Event::Key { key: egui::Key::G, physical_key: Some(egui::Key::G), pressed: false, repeat: false, modifiers: egui::Modifiers::NONE };
        frame(&mut app,&ctx,vec![key(egui::Key::G),up.clone(),key(egui::Key::G),up.clone(),key(egui::Key::G),up],1.);
        assert_eq!((app.world.input.throw, app.world.input.throw_strength), (THROW_GRENADE, 1.0), "full strength");
        let idle = |app: &mut PeakRunnerApp, t: f64| { let _ = ctx.run_ui(egui::RawInput { time: Some(t), focused: true,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280., 800.))), ..Default::default() },
            |ui| app.step(ui.ctx(), 1.0/20.0)); };
        for k in 0..12 { idle(&mut app, 1.1 + k as f64 * 0.05); }
        let after = app.world.players[app.world.player_id].throwables;
        assert_eq!(before[0] - after[0], 3, "all three thrown within about 0.4 s");
        assert!(app.world.discs.iter().any(|d| d.kind == GRENADE_KIND));
        frame(&mut app,&ctx,vec![key(egui::Key::N)],2.);
        for k in 0..4 { idle(&mut app, 2.1 + k as f64 * 0.05); }
        assert!(app.world.discs.iter().any(|d| d.kind == MINE_KIND), "and a mine");
    }

    #[test]
    fn shift_space_and_keyboard_focus_do_not_open_chat() {
        let mut app=PeakRunnerApp::comms_fixture(); let ctx=egui::Context::default();
        app.chat_open=false;
        let shifted_space=egui::Event::Key {key:egui::Key::Space,physical_key:Some(egui::Key::Space),
            pressed:true,repeat:false,modifiers:egui::Modifiers::SHIFT};
        frame(&mut app,&ctx,vec![key(egui::Key::Tab)],1.);
        frame(&mut app,&ctx,vec![shifted_space],2.);
        assert!(!app.chat_open);
        frame(&mut app,&ctx,vec![key(egui::Key::Enter)],3.);
        assert!(!app.chat_open);
    }

    #[test]
    fn server_notices_show_once_for_a_few_seconds() {
        let mut ctl = Controls::default();
        let mut lobby = peakrunner_net::Lobby::default();
        ctl.take_notice(&lobby, 1.0);
        assert!(ctl.notice.is_none(), "nothing to show yet");
        lobby.notice = Some("Match is full: you're spectating".into());
        lobby.notices = 1;
        ctl.take_notice(&lobby, 2.0);
        assert_eq!(ctl.notice, Some(("Match is full: you're spectating".into(), 2.0 + NOTICE_SECONDS)));
        ctl.notice = None;
        ctl.take_notice(&lobby, 3.0);
        assert!(ctl.notice.is_none(), "the same notice isn't shown twice");
        lobby.notice = Some("A slot opened: you joined Ember".into());
        lobby.notices = 2;
        ctl.take_notice(&lobby, 4.0);
        assert!(ctl.notice.as_ref().is_some_and(|(t, _)| t.contains("slot opened")));
    }

    #[test]
    fn leaving_clears_the_snapshot_that_could_reenter_the_match() {
        let mut net = NetUi::new();
        net.lobby.connected = true;
        net.lobby.player_id = 42;
        net.lobby.snapshot = Some(std::sync::Arc::new(crate::sim::Match::new(crate::terrain::MapId::Valley).snapshot()));
        net.lobby.error = Some("old error".into());
        net.dropped_in = true;
        net.predictor.tick = 100;
        net.disconnect();
        assert!(!net.lobby.connected);
        assert!(net.lobby.snapshot.is_none());
        assert!(net.lobby.error.is_none());
        assert_eq!(net.lobby.player_id, 0);
        assert!(!net.dropped_in);
        assert!(net.session.is_none());
        assert_eq!(net.predictor.tick, 0);
        net.disconnect(); // Leaving twice is harmless.
    }

    #[test]
    fn favorites_equip_at_an_open_inventory_station() {
        // F1-F4 and Use set the favorite while the station screen stays open;
        // the next frame's input reset used to drop it before the tick.
        use peakrunner_core::sim::ArmorClass;
        let mut app = PeakRunnerApp::comms_fixture();
        let ctx = egui::Context::default();
        app.chat_open = false;
        app.world.set_map(crate::terrain::MapId::OldHoller);
        app.world.start_rift(true);
        let id = app.world.player_id;
        let defs = peakrunner_core::equipment::definitions(app.world.map);
        let team = app.world.players[id].team as u8;
        let station = defs.iter().find(|d| d.kind == peakrunner_core::equipment::Kind::Inventory && d.team == team).unwrap().pos();
        app.world.players[id].pos = station;
        app.world.players[id].vel = glam::Vec3::ZERO;
        app.world.players[id].armor = ArmorClass::Heavy;
        app.net.preferences.saved.setups[0] = Some(crate::preferences::SavedSetup::of(&app.world.players[id]));
        app.world.players[id].armor = ArmorClass::Light;
        let run = |app: &mut PeakRunnerApp, events: Vec<egui::Event>, time: f64| {
            let mut out = ctx.run_ui(egui::RawInput { events, time: Some(time), focused: true,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280., 800.))), ..Default::default() }, |ui| {
                app.step(ui.ctx(), 1.0 / 60.0);
                if app.ctl.shop_open { app.shop_ui(ui.ctx()); }
            });
            out.textures_delta.clear();
        };
        // A first frame fills the HUD that says you're at the station.
        run(&mut app, vec![], 1.0);
        assert!(app.hud.as_ref().is_some_and(|h| h.station == 1), "standing at the station");
        app.ctl.shop_open = true;
        run(&mut app, vec![key(egui::Key::F1)], 2.0);
        assert_eq!(app.world.players[id].armor, ArmorClass::Light, "pressed, not yet stepped");
        run(&mut app, vec![], 3.0);
        assert!(app.ctl.shop_open, "the screen stays open");
        assert_eq!(app.world.players[id].armor, ArmorClass::Heavy, "F1's favorite equipped on the next step");
        // Held, the key repeats: that equips nothing (it re-equipped, and
        // sounded, at the repeat rate).
        app.world.players[id].armor = ArmorClass::Light;
        let held = egui::Event::Key { key: egui::Key::F1, physical_key: Some(egui::Key::F1), pressed: true, repeat: true, modifiers: egui::Modifiers::NONE };
        run(&mut app, vec![held], 4.0);
        run(&mut app, vec![], 5.0);
        assert_eq!(app.world.players[id].armor, ArmorClass::Light, "a repeat isn't a press");
        // F3 here is favorite 3, not the frame-timing overlay as well.
        let toggles = |app: &PeakRunnerApp, time: f64| {
            let mut toggles = None;
            let mut out = ctx.run_ui(egui::RawInput { events: vec![key(egui::Key::F3)], time: Some(time), focused: true, ..Default::default() },
                |ui| toggles = Some(app.frame_stats_key(ui.ctx())));
            out.textures_delta.clear();
            toggles.unwrap()
        };
        assert!(!toggles(&app, 6.0), "at the open station: favorite 3 only");
        app.ctl.shop_open = false;
        assert!(toggles(&app, 7.0), "elsewhere: the overlay");
    }
}

fn style_ui(ctx: &egui::Context) {
    // OS light-mode notifications must not select an unstyled light palette
    // underneath the game's explicitly light labels.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = Color32::from_rgba_unmultiplied(12, 18, 28, 230);
    visuals.override_text_color = Some(FG);
    visuals.widgets.inactive.bg_fill = SECONDARY_BUTTON_BG;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 44, 58);
    visuals.widgets.active.bg_fill = EMBER;
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    #[test]
    fn system_light_mode_cannot_replace_the_game_palette() {
        let ctx=egui::Context::default();
        ctx.set_theme(egui::ThemePreference::System);
        ctx.run_ui(egui::RawInput { system_theme:Some(egui::Theme::Light), ..Default::default() }, |_| {}).textures_delta.clear();
        style_ui(&ctx);
        for theme in [egui::Theme::Light,egui::Theme::Dark,egui::Theme::Light] {
            ctx.run_ui(egui::RawInput { system_theme:Some(theme), ..Default::default() }, |_| {}).textures_delta.clear();
            assert_eq!(ctx.theme(),egui::Theme::Dark);
            assert_eq!(ctx.global_style().visuals.override_text_color,Some(FG));
            assert_eq!(ctx.global_style().visuals.widgets.inactive.bg_fill,SECONDARY_BUTTON_BG);
        }
    }

    #[test]
    fn secondary_button_label_has_readable_contrast() {
        fn luminance(c:Color32)->f32 {
            let linear=|v:u8| {let s=v as f32/255.;if s<=0.04045 {s/12.92}else{((s+0.055)/1.055).powf(2.4)}};
            0.2126*linear(c.r())+0.7152*linear(c.g())+0.0722*linear(c.b())
        }
        assert!((luminance(FG)+0.05)/(luminance(SECONDARY_BUTTON_BG)+0.05)>=4.5);
    }
}

impl PeakRunnerApp {
    /// One line of connection state for QA tools.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn qa_net_state(&self) -> String {
        let l = &self.net.lobby;
        format!("mode {:?} connected {} error {:?} snapshot tick {:?} people {} session {}", self.mode, l.connected, l.error,
            l.snapshot.as_ref().map(|s| s.tick), l.snapshot.as_ref().map_or(0, |s| s.players.iter().filter(|p| p.net_id != 0 && !p.is_bot).count()),
            self.net.session.is_some())
    }

    /// QA staging hooks; only run when some QA_ variable is set.
    fn apply_qa(&mut self) {
        crate::qa_overrides::apply(&mut self.world);
        crate::qa_overrides::apply_points(&mut self.world);
        crate::qa_overrides::apply_loadout(&mut self.world);
        if std::env::var_os("QA_SHOP").is_some() && self.mode == Mode::Play { self.ctl.shop_open = true; }
        // `QA_PAGE=single|multi|library|library:N` opens that menu page.
        if self.mode == Mode::Menu && self.page == MenuPage::Home {
            if let Ok(page) = std::env::var("QA_PAGE") {
                self.page = match page.as_str() {
                    "single" => MenuPage::Single, "multi" => MenuPage::Multi, "library" => MenuPage::Library(None),
                    p => p.strip_prefix("library:").and_then(|n| n.parse().ok()).map_or(MenuPage::Home, |n| MenuPage::Library(Some(n))),
                };
            }
        }
        // QA_HOST=<mode key>[,start]: the Host screen on that mode (e.g.
        // `duel`), and with `start` its Host and join pressed once.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(qa) = std::env::var("QA_HOST") {
            static DONE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
            static SHOWN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let mut parts = qa.split(',');
            let mode = parts.next().and_then(|key| crate::host::modes().into_iter().find(|m| m.key() == key));
            let start = parts.any(|p| p == "start");
            match DONE.load(std::sync::atomic::Ordering::Relaxed) {
                0 if self.mode == Mode::Menu => {
                    self.mode = Mode::Host;
                    if let Some(mode) = mode { self.net.host.settings.set_mode(mode); }
                    SHOWN.get_or_init(std::time::Instant::now);
                    DONE.store(1, std::sync::atomic::Ordering::Relaxed);
                }
                // Three seconds on the screen first, so a capture can show it.
                1 if start && self.mode == Mode::Host && SHOWN.get().is_some_and(|t| t.elapsed().as_secs_f32() > 3.0) => {
                    self.host_and_join(crate::host::server_app().as_deref());
                    DONE.store(2, std::sync::atomic::Ordering::Relaxed);
                }
                _ => {}
            }
        }
        if std::env::var_os("QA_CONTROLS").is_some() && self.mode == Mode::Menu { self.ctl.keys_open = true; }
        if std::env::var_os("QA_GRAPHICS").is_some() && self.mode == Mode::Menu { self.ctl.graphics_open = true; }
        if std::env::var_os("QA_FPS").is_some() { self.show_frame_stats = true; }
        if std::env::var_os("QA_VOTE_PANEL").is_some() && self.mode == Mode::Play && self.online() { self.ctl.vote_open = true; }
        // QA_DUEL=<bot archetype>: on a duel server, duel that bot once the
        // board comes; QA_DUEL=watch: watch the first duel being played.
        #[cfg(not(target_arch = "wasm32"))]
        if let (Ok(duel), Some(board), Some(session)) = (std::env::var("QA_DUEL"), &self.net.lobby.duels, &self.net.session) {
            static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            let me = self.net.lobby.player_id;
            let free = board.people.iter().any(|p| p.id == me && p.place != peakrunner_core::sim::duel::Place::Dueling && p.arena == 0);
            if free && !ASKED.load(std::sync::atomic::Ordering::Relaxed) && (duel != "watch" || !board.duels.is_empty()) {
                if duel == "watch" { session.watch(0); } else { session.challenge_bot(&duel); }
                ASKED.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        // QA_SPECTATE_FOLLOW=shoulder|orbit: a spectator follows the first player.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(view) = std::env::var("QA_SPECTATE_FOLLOW") {
            if self.world.spectating && self.ctl.spectator.view == crate::spectate::View::Free {
                self.ctl.spectator.cycle(&self.world, 1);
                if let crate::spectate::View::Follow { net_id, .. } = self.ctl.spectator.view {
                    self.ctl.spectator.view = crate::spectate::View::Follow { net_id, orbit: view == "orbit" };
                }
            }
        }
        if let Some((kind, turn)) = crate::qa_overrides::placing(&mut self.world) {
            if self.mode == Mode::Play {
                if let Some(p) = self.world.players.get_mut(self.world.player_id) { p.pack = peakrunner_core::sim::packs::PackKind::from_key(kind.key()); }
                self.ctl.placing = true;
                self.ctl.turn = turn;
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod server_list_tests {
    use super::*;

    #[test]
    fn the_server_list_filters_and_sorts_like_its_columns_say() {
        let row = |name: &str, mode: &str, players: u32, ping: Option<u32>, mods: &[&str], favorite: bool| ServerRow {
            id: name.into(), name: name.into(), address: format!("quic://{name}.example:7777"), port: 7777, listed: true, community: false,
            map: "Old Holler".into(), mode: mode.into(), players, max: 16, bots: 0, location: "New York, US".into(), favorite,
            state: if ping.is_some() { RowState::Ready } else { RowState::Unreachable }, ping, version: String::new(), password: false,
            mods: mods.iter().map(|m| m.to_string()).collect(), download: 0,
        };
        let rows = vec![row("Alpha", "CTF", 3, Some(90), &[], false), row("Bravo", "TDM", 16, Some(40), &["Spree"], true),
            row("Charlie", "CTF", 0, None, &[], true)];
        let names = |v: Vec<ServerRow>| v.into_iter().map(|r| r.name).collect::<Vec<_>>();
        let mut browse = BrowseUi::default();
        assert_eq!(names(browse.visible(rows.clone())), ["Bravo", "Alpha", "Charlie"], "most players first by default");
        browse.sort = (SortBy::Ping, false);
        assert_eq!(names(browse.visible(rows.clone())), ["Bravo", "Alpha", "Charlie"], "no ping goes last");
        browse.sort = (SortBy::Name, true);
        assert_eq!(names(browse.visible(rows.clone())), ["Charlie", "Bravo", "Alpha"]);
        let only = |f: &dyn Fn(&mut BrowseUi)| { let mut b = BrowseUi::default(); f(&mut b); names(b.visible(rows.clone())) };
        assert_eq!(only(&|b| b.favorites_only = true), ["Bravo", "Charlie"]);
        assert_eq!(only(&|b| b.hide_empty = true), ["Bravo", "Alpha"]);
        assert_eq!(only(&|b| b.hide_full = true), ["Alpha", "Charlie"]);
        // Bots count as players, but a match full of them still takes people.
        let bots = ServerRow { bots: 12, ..rows[1].clone() };
        assert!(rows[1].full() && !bots.full());
        assert_eq!(only(&|b| b.mode = Some("CTF".into())), ["Alpha", "Charlie"]);
        assert_eq!(only(&|b| b.filter = "spree".into()), ["Bravo"], "the filter finds mods too");
        assert_eq!(split_label("Old Holler at Dusk - SPREE"), ("Old Holler at Dusk".to_string(), "SPREE".to_string()));
        // A LAN server keeps its port as a favorite; an encrypted one keeps its URL.
        let lan = ServerRow { address: "192.168.1.20".into(), port: 7781, ..rows[0].clone() };
        assert_eq!(lan.favorite_entry().address, "192.168.1.20:7781");
        assert_eq!(rows[0].favorite_entry().address, "quic://Alpha.example:7777");
        assert_eq!((size_text(554), size_text(1_144), size_text(5_635_580)), ("1 KB".to_string(), "2 KB".to_string(), "5.6 MB".to_string()));
    }
}

/// The HUD's pack line: the pack worn and what its key does now.
fn pack_line(p: &peakrunner_core::sim::Player, keys: &crate::keybinds::Keybinds) -> String {
    use peakrunner_core::sim::packs::PackUse;
    let Some(kind) = p.pack.filter(|k| k.exists()) else { return String::new() };
    let d = kind.def();
    let key = keys.name(crate::keybinds::Action::Deploy);
    let name = if d.name.to_lowercase().contains("pack") { d.name.clone() } else { format!("{} pack", d.name) };
    match d.use_ {
        PackUse::Place => format!("{name} · {key} to deploy"),
        PackUse::Always => name,
        PackUse::Toggle => format!("{name} · {} ({key})", if p.pack_on { "on" } else { "off" }),
        PackUse::Use => {
            let left = if d.charges > 0 { format!(" · {} left", d.charges.saturating_sub(p.pack_uses)) } else { String::new() };
            if p.pack_clock > 0.0 { format!("{name} · {:.1} s{left}", p.pack_clock) } else { format!("{name} · {key} to use{left}") }
        }
    }
}

/// The HUD's class key line: the armor's ability and what its key does now.
fn ability_line(p: &peakrunner_core::sim::Player, keys: &crate::keybinds::Keybinds) -> String {
    use peakrunner_core::sim::packs::PackUse;
    let Some(a) = p.armor.def().ability.as_ref() else { return String::new() };
    let key = keys.name(crate::keybinds::Action::Ability);
    let strain = if a.strain_limit > 0.0 && p.strain > 0.0 { format!(" · strain {:.0}%", p.strain / a.strain_limit * 100.0) } else { String::new() };
    match a.use_ {
        PackUse::Toggle => format!("{} · {} ({key}){strain}", a.name, if p.ability_on { "on" } else { "off" }),
        _ => {
            let left = if a.charges > 0 { format!(" · {} left", a.charges.saturating_sub(p.ability_uses)) } else { String::new() };
            if p.ability_clock > 0.0 { format!("{} · {:.1} s{left}{strain}", a.name, p.ability_clock) } else { format!("{} · {key}{left}{strain}", a.name) }
        }
    }
}

/// The HUD's vehicle line: the vehicle ridden, its hull and energy, the seat.
fn vehicle_line(world: &peakrunner_core::sim::World, keys: &crate::keybinds::Keybinds) -> String {
    let Some((v, seat)) = world.riding(world.player_id) else { return String::new() };
    let Some(c) = world.vehicles.get(v) else { return String::new() };
    let Some(d) = c.def() else { return String::new() };
    let role = match seat { 0 if c.grounded() => "driving", 0 => "flying", _ => "riding" };
    format!("{} · {role} · hull {:.0}% · energy {:.0}% · {} to leave", d.name, (c.hull / d.hull * 100.0).max(0.0),
        (c.energy / d.energy_max * 100.0).max(0.0), keys.name(crate::keybinds::Action::Use))
}

#[cfg(test)]
mod result_tests {
    use super::*;
    use peakrunner_core::map_catalog::SupportedMode;

    fn ended(mode: SupportedMode) -> World {
        let mut w = World::new();
        w.set_map(peakrunner_core::terrain::MapId::OldHoller);
        w.set_mode(mode);
        w.start_match(true);
        w
    }

    fn summary(w: &World) -> (String, String, String) {
        let hud: Hud = serde_json::from_str(&w.hud_json()).unwrap();
        result_summary(w, Some(&hud))
    }

    #[test]
    fn rabbit_names_the_winner_in_points_and_team_rabbit_shows_lives() {
        let mut w = ended(SupportedMode::Rabbit);
        w.players[2].frags = 30;
        let (title, score, detail) = summary(&w);
        assert_eq!((title, score), (format!("{} wins", w.display_name(2)), "30 points".to_string()));
        assert!(detail.contains("points"), "{detail}");
        w.players[w.player_id].frags = 31;
        assert_eq!(summary(&w).0, "You win");

        let mut w = ended(SupportedMode::TeamRabbit);
        w.score = [50, 12];
        assert_eq!(summary(&w).1, "Ember 38 lives  ·  Glacier 0 lives");

        let mut w = ended(SupportedMode::Ctf);
        w.score = [3, 1];
        assert_eq!(summary(&w).1, "3  –  1", "team modes keep the score");
        assert_eq!(result_summary(&w, None).0, "Match over");
    }
}
