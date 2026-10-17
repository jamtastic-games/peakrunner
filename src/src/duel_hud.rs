//! A duel server on screen (`duel1`; docs/duels.md): the duel board (the
//! vote key opens it there), the banner for a challenge waiting on you, and
//! the duel's score line. The board comes from the server
//! (`ServerMsg::Duels`); every move it shows is the server's, and this only
//! asks for one (`Ask`).
use egui::{Align2, Color32, FontId, RichText, Vec2};
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{duel::{Board, Place}, World};

const AMBER: Color32 = Color32::from_rgb(255, 196, 60);
const FG: Color32 = Color32::from_rgb(231, 238, 246);
const MUTED: Color32 = Color32::from_rgb(154, 168, 184);
const BG: Color32 = Color32::from_rgb(8, 13, 20);
const GREEN: Color32 = Color32::from_rgb(120, 210, 140);

/// What the player asked for on the board.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ask { Challenge(u32), Bot(String), Answer(u32, bool), Withdraw, Watch(u32), Lobby }

/// An arena: a duel being played (the base `duel` mode, or a mod's variant).
pub(crate) fn arena(world: &World) -> bool {
    world.mode.rules() == peakrunner_core::modes::Rules::Duel && world.mode != SupportedMode::DuelLobby
}

/// The two duelists' names, challenger (Ember) first.
fn duelists(world: &World) -> [String; 2] {
    [0, 1].map(|t| world.players.iter().find(|p| p.net_id != 0 && p.team.idx() == t).map(|p| p.name.clone()).unwrap_or_default())
}

/// The score line over the view in a duel ("Ada  2 – 1  Bo", the clock
/// beside it) or in the lobby (no clock); None elsewhere.
pub(crate) fn top_line(world: &World, board_key: &str) -> Option<(String, bool)> {
    if world.mode == SupportedMode::DuelLobby { return Some((format!("DUEL LOBBY · {board_key}: DUEL BOARD"), false)); }
    if !arena(world) { return None; }
    let [a, b] = duelists(world);
    Some((format!("{a}  {} – {}  {b}", world.score[0], world.score[1]), true))
}

/// Under a later round's countdown: which round, and where.
pub(crate) fn countdown_caption(world: &World) -> Option<String> {
    arena(world).then(|| format!("Round {} · {}", world.score[0] + world.score[1] + 1, peakrunner_core::terrain::info(world.map).name))
}

/// A finished duel's result for the overlay: title, score and a line.
pub(crate) fn result(world: &World) -> Option<(String, String, String)> {
    if !arena(world) { return None; }
    let names = duelists(world);
    let s = world.score;
    let me = world.players.get(world.player_id).filter(|_| !world.spectating).map(|p| p.team.idx());
    let title = match (s[0].cmp(&s[1]), me) {
        (std::cmp::Ordering::Equal, _) => "Draw".to_string(),
        (o, Some(t)) if (o == std::cmp::Ordering::Greater) == (t == 0) => "You win the duel".into(),
        (_, Some(_)) => "You lose the duel".into(),
        (o, None) => format!("{} wins", names[if o == std::cmp::Ordering::Greater { 0 } else { 1 }]),
    };
    Some((title, format!("{}  {} – {}  {}", names[0], s[0], s[1], names[1]), "Back to the lobby in a moment".into()))
}

/// The person's own line on the board.
fn me<'a>(board: &'a Board, id: u32) -> Option<&'a peakrunner_core::sim::duel::Person> { board.people.iter().find(|p| p.id == id) }

/// The name of the person with `id` on the board.
fn name(board: &Board, id: u32) -> String { board.people.iter().find(|p| p.id == id).map_or_else(|| "Someone".into(), |p| p.name.clone()) }

/// The duel board: challenges to you and from you, everyone on the server
/// (challenge whoever is in the lobby), the bots, the duels being played
/// (watch one), and the way back to the lobby. Keys 1 and 2 answer the
/// oldest challenge to you.
pub(crate) fn board_ui(ctx: &egui::Context, board: &Board, id: u32, close_key: &str) -> Option<Ask> {
    let mut ask = None;
    let mine = me(board, id);
    let place = mine.map_or(Place::Lobby, |p| p.place);
    let free = place == Place::Lobby;
    if let Some(&(from, _)) = board.invites.first() {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Num1) { ask = Some(Ask::Answer(from, true)); }
            if i.key_pressed(egui::Key::Num2) { ask = Some(Ask::Answer(from, false)); }
        });
    }
    egui::Area::new(egui::Id::new("duel_board")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new().fill(Color32::from_rgba_unmultiplied(12, 18, 28, 245))
                .stroke(egui::Stroke::new(2.0, AMBER)).corner_radius(10.0).inner_margin(20.0).show(ui, |ui| {
                    ui.set_width(600.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("DUEL BOARD").size(13.0).color(AMBER).strong());
                        if let Some(m) = mine {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("Your record {}–{}", m.wins, m.losses)).color(MUTED));
                            });
                        }
                    });
                    ui.label(RichText::new(format!("One on one in light armor, first to {} rounds. Challenge someone in the lobby, or a bot.", board.rounds)).color(MUTED));
                    ui.add_space(8.0);
                    for (k, &(from, left)) in board.invites.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} challenges you · {left} s", name(board, from))).size(17.0).color(FG).strong());
                            let keys = |text: &str| if k == 0 { format!("{} · {text}", if text == "Accept" { 1 } else { 2 }) } else { text.to_string() };
                            let yes = egui::Button::new(RichText::new(keys("Accept")).color(BG)).fill(GREEN).min_size(Vec2::new(110.0, 32.0));
                            if ui.add_enabled(free, yes).clicked() { ask = Some(Ask::Answer(from, true)); }
                            if ui.add(egui::Button::new(keys("Decline")).min_size(Vec2::new(110.0, 32.0))).clicked() { ask = Some(Ask::Answer(from, false)); }
                        });
                    }
                    if let Some((to, left)) = board.challenge {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Waiting for {} to answer · {left} s", name(board, to))).color(AMBER));
                            if ui.button("Take it back").clicked() { ask = Some(Ask::Withdraw); }
                        });
                    }
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        ui.label(RichText::new("PEOPLE").size(12.0).color(MUTED));
                        egui::Grid::new("duel_people").num_columns(4).spacing([16.0, 6.0]).show(ui, |ui| {
                            for p in &board.people {
                                let you = p.id == id;
                                ui.label(RichText::new(if you { format!("{} (you)", p.name) } else { p.name.clone() }).color(FG));
                                let duel = board.duels.iter().find(|d| d.arena == p.arena);
                                let at = match (p.place, duel) {
                                    (Place::Lobby, _) => "in the lobby".to_string(),
                                    (Place::Dueling, Some(d)) => format!("dueling · {}–{}", d.score[0], d.score[1]),
                                    (Place::Watching, Some(d)) => format!("watching {} v {}", d.names[0], d.names[1]),
                                    (Place::Dueling, None) => "dueling".into(),
                                    (Place::Watching, None) => "watching".into(),
                                };
                                ui.label(RichText::new(at).color(MUTED));
                                ui.label(RichText::new(format!("{}–{}", p.wins, p.losses)).color(MUTED));
                                if !you && p.place == Place::Lobby {
                                    let waiting = board.challenge.is_some_and(|(to, _)| to == p.id);
                                    let b = egui::Button::new(if waiting { "Challenged" } else { "Challenge" });
                                    if ui.add_enabled(free && !waiting, b).clicked() { ask = Some(Ask::Challenge(p.id)); }
                                } else if !you && p.place == Place::Dueling {
                                    if ui.add_enabled(place != Place::Dueling, egui::Button::new("Watch")).clicked() { ask = Some(Ask::Watch(p.arena)); }
                                } else { ui.label(""); }
                                ui.end_row();
                            }
                        });
                        ui.add_space(10.0);
                        ui.label(RichText::new("BOTS · always ready").size(12.0).color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for b in &board.bots {
                                if ui.add_enabled(free, egui::Button::new(RichText::new(b).size(15.0)).min_size(Vec2::new(90.0, 34.0))).clicked() {
                                    ask = Some(Ask::Bot(b.clone()));
                                }
                            }
                        });
                        ui.add_space(10.0);
                        ui.label(RichText::new("DUELS").size(12.0).color(MUTED));
                        if board.duels.is_empty() { ui.label(RichText::new("None yet").color(MUTED)); }
                        for d in &board.duels {
                            ui.horizontal(|ui| {
                                let watchers = if d.watchers > 0 { format!(" · {} watching", d.watchers) } else { String::new() };
                                ui.label(RichText::new(format!("{} {} – {} {} · {}{watchers}", d.names[0], d.score[0], d.score[1], d.names[1],
                                    peakrunner_core::terrain::info(d.map).name)).color(FG));
                                let here = mine.is_some_and(|m| m.arena == d.arena);
                                if !here && ui.add_enabled(place != Place::Dueling, egui::Button::new("Watch")).clicked() { ask = Some(Ask::Watch(d.arena)); }
                            });
                        }
                    });
                    ui.add_space(10.0);
                    match place {
                        Place::Dueling => if ui.add(egui::Button::new(RichText::new("Concede and go back to the lobby").color(FG)).min_size(Vec2::new(260.0, 36.0))).clicked() { ask = Some(Ask::Lobby); },
                        Place::Watching if mine.is_some_and(|m| m.arena != 0) => if ui.add(egui::Button::new("Back to the lobby").min_size(Vec2::new(200.0, 36.0))).clicked() { ask = Some(Ask::Lobby); },
                        _ => {}
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{close_key} or Esc closes")).color(MUTED).size(11.0));
                });
        });
    ask
}

/// While the board is closed: a pulsing banner for a challenge to you.
pub(crate) fn banner(ui: &egui::Ui, board: &Board, key: &str) {
    let Some(&(from, left)) = board.invites.first() else { return };
    let rect = ui.max_rect();
    let t = ui.ctx().input(|i| i.time) as f32;
    let pulse = 0.55 + 0.45 * (t * 4.0).sin().abs();
    let at = egui::Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 168.0), Vec2::new(560.0, 64.0));
    let p = ui.painter();
    p.rect_filled(at, 10.0, Color32::from_rgba_unmultiplied(20, 16, 6, 230));
    p.rect_stroke(at, 10.0, egui::Stroke::new(3.0, AMBER.gamma_multiply(pulse)), egui::StrokeKind::Inside);
    p.text(at.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, format!("DUEL · {} CHALLENGES YOU", name(board, from).to_uppercase()),
        FontId::proportional(22.0), AMBER);
    let more = if board.invites.len() > 1 { format!(" · {} more", board.invites.len() - 1) } else { String::new() };
    p.text(at.center_bottom() + Vec2::new(0.0, -8.0), Align2::CENTER_BOTTOM, format!("{left} s · press {key} to answer{more}"),
        FontId::proportional(14.0), FG);
    ui.ctx().request_repaint();
}

/// The spectator's hints watching a duel, or in the lobby.
pub(crate) fn spectator_hint(world: &World, next_key: &str, board_key: &str) -> Option<String> {
    if arena(world) { Some(format!("{next_key}: next duel · {board_key}: duel board")) }
    else if world.mode == SupportedMode::DuelLobby { Some(format!("{next_key}: join the lobby · {board_key}: duel board")) }
    else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::{Match, Team};
    use peakrunner_core::terrain::MapId;

    fn arena() -> World {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Duel);
        m.join_on(1, "Ada", Team::Ember).unwrap();
        m.join_on(2, "Bo", Team::Glacier).unwrap();
        m.world.score = [2, 1];
        let mut w = World::new();
        assert!(w.apply_snapshot(&m.snapshot(), 2));
        w
    }

    fn board(invites: Vec<(u32, u8)>, place: Place) -> Board {
        use peakrunner_core::sim::duel::Person;
        let person = |id, name: &str, place| Person { id, name: name.into(), place, arena: 0, wins: 0, losses: 0 };
        Board { people: vec![person(1, "Ada", Place::Lobby), person(2, "Bo", place)], invites, bots: vec!["Rookie".into()], rounds: 3, ..Default::default() }
    }

    fn press(board: &Board, me: u32, key: Option<egui::Key>) -> Option<Ask> {
        let ctx = egui::Context::default();
        let events = key.map(|key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
        let mut asked = None;
        let mut out = ctx.run_ui(egui::RawInput { events: events.into_iter().collect(), ..Default::default() }, |ui| { asked = board_ui(ui.ctx(), board, me, "V"); });
        out.textures_delta.clear();
        asked
    }

    /// 1 and 2 answer the oldest challenge, as the vote panel's keys vote.
    #[test]
    fn keys_answer_the_oldest_challenge() {
        let b = board(vec![(1, 12), (3, 18)], Place::Lobby);
        assert_eq!(press(&b, 2, Some(egui::Key::Num1)), Some(Ask::Answer(1, true)));
        assert_eq!(press(&b, 2, Some(egui::Key::Num2)), Some(Ask::Answer(1, false)));
        assert_eq!(press(&b, 2, None), None, "nothing pressed, nothing asked");
        assert_eq!(press(&board(Vec::new(), Place::Lobby), 2, Some(egui::Key::Num1)), None, "no challenge, no answer");
    }

    #[test]
    fn the_duel_reads_as_names_and_rounds() {
        let w = arena();
        assert_eq!(top_line(&w, "V"), Some(("Ada  2 – 1  Bo".into(), true)));
        assert_eq!(countdown_caption(&w).as_deref(), Some("Round 4 · Old Holler"));
        assert_eq!(result(&w).map(|r| r.0).as_deref(), Some("You lose the duel"), "Bo is 1 down");
        let mut lobby = World::new();
        lobby.mode = SupportedMode::DuelLobby;
        assert_eq!(top_line(&lobby, "V").map(|l| l.1), Some(false), "no clock in the lobby");
        assert!(top_line(&World::new(), "V").is_none());
    }
}
