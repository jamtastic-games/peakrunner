#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use eframe::egui::{self, Color32, RichText, Vec2};
use peakrunner_launcher::{self as updater, Release, Store};
use std::{
    process::Child,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(11, 18, 23);
const PANEL: Color32 = Color32::from_rgb(20, 31, 39);
const TEXT: Color32 = Color32::from_rgb(230, 239, 242);
const MUTED: Color32 = Color32::from_rgb(153, 175, 185);
const ACCENT: Color32 = Color32::from_rgb(117, 213, 228);
const PANEL_HI: Color32 = Color32::from_rgb(30, 44, 54);
const GOOD: Color32 = Color32::from_rgb(120, 220, 150);
const WARN: Color32 = Color32::from_rgb(240, 180, 110);

#[derive(serde::Deserialize)]
struct Directory {
    servers: Vec<Host>,
}
#[derive(serde::Deserialize)]
struct Host {
    name: String,
    map: String,
    players: u32,
    max_players: u32,
}
enum Event {
    Progress(String),
    Checked(Option<Release>, Vec<Host>, String),
    Done(std::result::Result<(), String>),
    Started(std::result::Result<Child, String>),
}
pub(crate) struct Launcher {
    store: Option<Arc<Store>>,
    installed: Option<String>,
    available: Option<Release>,
    hosts: Vec<Host>,
    status: String,
    busy: bool,
    child: Option<Child>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    checked: Instant,
    rollback_confirm: bool,
    /// The banner picture (Ashveil's skull tower at dusk), when it decoded.
    banner: Option<egui::TextureHandle>,
}

/// Where the game lives online, shown on the banner.
const LINKS: [(Icon, &str, &str); 3] = [
    (Icon::Web, "peakrunner.net", "https://peakrunner.net"),
    (Icon::Video, "YouTube", "https://www.youtube.com/@PeakrunnerCentral"),
    (Icon::Forum, "r/peakrunner", "https://www.reddit.com/r/peakrunner/"),
];

#[derive(Clone, Copy)]
enum Icon { Web, Video, Forum }

/// The banner, decoded from the PNG built into the launcher.
fn banner_image() -> Option<egui::ColorImage> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../assets/banner.png")[..])).read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    let size = [info.width as usize, info.height as usize];
    match info.color_type {
        png::ColorType::Rgb => Some(egui::ColorImage::from_rgb(size, &pixels[..info.buffer_size()])),
        png::ColorType::Rgba => Some(egui::ColorImage::from_rgba_unmultiplied(size, &pixels[..info.buffer_size()])),
        _ => None,
    }
}
impl Launcher {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.override_text_color = Some(TEXT);
        style.spacing.item_spacing = Vec2::new(12., 12.);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let (tx, rx) = mpsc::channel();
        let store = updater::data_dir()
            .and_then(|root| Store::open(root, updater::public_key()?, updater::platform().into()))
            .map(Arc::new);
        let (store, status) = match store {
            Ok(s) => (Some(s), "Checking for updates…".into()),
            Err(e) => (None, e.to_string()),
        };
        let mut app = Self {
            store,
            installed: None,
            available: None,
            hosts: Vec::new(),
            status,
            busy: false,
            child: None,
            tx,
            rx,
            checked: Instant::now(),
            rollback_confirm: false,
            banner: banner_image().map(|image| cc.egui_ctx.load_texture("banner", image, egui::TextureOptions::LINEAR)),
        };
        app.refresh_installed();
        if app.store.is_some() {
            app.check();
        }
        app
    }
    fn refresh_installed(&mut self) {
        if let Some(store) = &self.store {
            match store.current() {
                Ok(current) => self.installed = current.map(|c| c.release.manifest.version),
                Err(e) => {
                    self.installed = None;
                    self.status = format!("Installation metadata needs attention: {e}");
                }
            }
        }
    }
    fn check(&mut self) {
        if self.busy || self.child.is_some() {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        self.busy = true;
        self.checked = Instant::now();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let release = updater::latest().and_then(|r| {
                store.check_sequence(&r)?;
                Ok(r)
            });
            let hosts = updater::fetch("https://dir.peakrunner.net/servers", 256 * 1024)
                .and_then(|data| Ok(serde_json::from_slice::<Directory>(&data)?.servers))
                .map(|s| {
                    s.into_iter()
                        .filter(|s| {
                            s.name.len() <= 64
                                && s.map.len() <= 64
                                && s.players <= s.max_players
                                && s.max_players <= 128
                        })
                        .take(64)
                        .collect()
                })
                .unwrap_or_default();
            // A verified release needs no words: the card says what to do.
            let (release,status)=match release {
                Ok(r)=>(Some(r),String::new()),
                Err(e)=>match store.current().ok().flatten() {
                    Some(installed)=>(Some(installed.release),format!("Couldn't check for updates ({e}). The installed game still plays.")),
                    None=>(None,format!("Couldn't check for updates ({e}). Try again in a moment.")),
                },
            };
            let _ = tx.send(Event::Checked(release, hosts, status));
        });
    }
    fn install(&mut self) {
        let (Some(store), Some(release)) = (self.store.clone(), self.available.clone()) else {
            return;
        };
        self.busy = true;
        self.status = "Getting ready…".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = store
                .install(
                    &release,
                    |asset| {
                        updater::fetch(
                            &format!("{}/blobs/{}", updater::ORIGIN, asset.sha256),
                            asset.size,
                        )
                    },
                    |s| {
                        let _ = tx.send(Event::Progress(s));
                    },
                )
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Done(result));
        });
    }
    fn play(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        self.busy = true;
        self.status = "Checking the game's files…".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Started(store.play().map_err(|e| e.to_string())));
        });
    }
    fn rollback(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Done(store.rollback().map_err(|e| e.to_string())));
        });
    }
}
impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.painter().rect_filled(ui.max_rect(), 0., BG);
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Progress(s) => self.status = s,
                Event::Checked(release, hosts, status) => {
                    self.available = release;
                    self.hosts = hosts;
                    self.status = status;
                    self.busy = false;
                }
                Event::Done(result) => {
                    self.busy = false;
                    self.status = result.map_or_else(
                        |e| format!("The update failed: {e}. Your installed game is unchanged."),
                        |_| "Installed and verified.".into(),
                    );
                    self.refresh_installed();
                }
                Event::Started(result) => {
                    self.busy = false;
                    match result {
                        Ok(child) => {
                            self.child = Some(child);
                            self.status = "Updates wait until you quit the game.".into();
                        }
                        Err(e) => self.status = format!("Couldn't start the game: {e}"),
                    }
                }
            }
        }
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    self.child = None;
                    self.status = if exit.success() {
                        String::new()
                    } else {
                        "The game quit unexpectedly. Try Repair; game.log in the game folder says why.".into()
                    };
                }
                Err(e) => self.status = format!("Couldn't check on the game: {e}"),
                _ => {}
            }
        }
        if !self.busy && self.child.is_none() && self.checked.elapsed() > Duration::from_secs(300) {
            self.check();
        }
        if self.busy && ui.ctx().input(|i| i.viewport().close_requested()) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(200));
        qa_state(self);
        // The footer stays at the bottom, so nothing below the cards is cut off.
        egui::Panel::bottom("footer").frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(28, 12)))
            .show_separator_line(false).show(ui, |ui| self.footer(ui));
        egui::CentralPanel::no_frame().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 28, right: 28, top: 24, bottom: 4 }))
            .show(ui, |ui| {
                self.banner(ui);
                ui.add_space(14.);
                self.hero(ui);
                ui.add_space(14.);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if ui.available_width() >= 680. {
                        ui.columns(2, |cols| { self.notes(&mut cols[0]); self.servers(&mut cols[1]); });
                    } else {
                        self.notes(ui);
                        ui.add_space(12.);
                        self.servers(ui);
                    }
                });
            });
    }
}

/// A card: the panel colour, rounded, with room inside.
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new().fill(PANEL).corner_radius(12).inner_margin(20).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Left-aligned, not justified (columns justify, which spaces words out).
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), add).inner
    }).inner
}

/// A card's small capital heading.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.).strong().color(ACCENT));
}

/// What a platform is called on screen.
fn platform_name(platform: &str) -> &'static str {
    match platform {
        "macos-arm64" => "macOS · Apple Silicon",
        "macos-x64" => "macOS · Intel",
        "windows-x64" => "Windows",
        "linux-x64" => "Linux",
        _ => "Unsupported platform",
    }
}

/// QA only (`QA_LAUNCHER_STATE`): how the launcher looks with the game
/// installed (`installed`) or one release behind (`update`), for captures.
fn qa_state(app: &mut Launcher) {
    let Ok(state) = std::env::var("QA_LAUNCHER_STATE") else { return };
    let Some(release) = &app.available else { return };
    app.installed = Some(match state.as_str() {
        "update" => "0.1.0-playtest.20261008.1".into(),
        _ => release.manifest.version.clone(),
    });
}

impl Launcher {
    /// The banner: the picture with the name over it, the platform, and
    /// the game's site, channel and community.
    fn banner(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let height = (width / 3.3).clamp(160., 225.);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        match &self.banner {
            // Cropped to fill, from the picture's middle.
            Some(texture) => {
                let [w, h] = texture.size().map(|n| n as f32);
                let keep = (rect.aspect_ratio() / (w / h)).min(1.0);
                let uv = egui::Rect::from_min_max(egui::pos2(0., 0.5 - keep / 2.), egui::pos2(1., 0.5 + keep / 2.));
                egui::Image::new((texture.id(), rect.size())).uv(uv).corner_radius(14).paint_at(ui, rect);
            }
            None => { painter.rect_filled(rect, 14, PANEL); }
        }
        // Darkened from the left and the bottom, so the words read.
        let mut shade = egui::Mesh::default();
        let dark = |a: u8| Color32::from_rgba_unmultiplied(8, 12, 16, a);
        let mid = rect.left() + rect.width() * 0.62;
        for (x0, x1, a0, a1) in [(rect.left(), mid, 225u8, 0u8)] {
            let i = shade.vertices.len() as u32;
            shade.colored_vertex(egui::pos2(x0, rect.top()), dark(a0));
            shade.colored_vertex(egui::pos2(x1, rect.top()), dark(a1));
            shade.colored_vertex(egui::pos2(x1, rect.bottom()), dark(a1));
            shade.colored_vertex(egui::pos2(x0, rect.bottom()), dark(a0));
            shade.add_triangle(i, i + 1, i + 2); shade.add_triangle(i, i + 2, i + 3);
        }
        let i = shade.vertices.len() as u32;
        let fade = rect.bottom() - rect.height() * 0.45;
        shade.colored_vertex(egui::pos2(rect.left(), fade), dark(0));
        shade.colored_vertex(egui::pos2(rect.right(), fade), dark(0));
        shade.colored_vertex(rect.right_bottom(), dark(170));
        shade.colored_vertex(rect.left_bottom(), dark(170));
        shade.add_triangle(i, i + 1, i + 2); shade.add_triangle(i, i + 2, i + 3);
        painter.add(shade);
        // The name and a line under it, low on the left.
        let left = rect.left() + 26.;
        painter.text(egui::pos2(left, rect.bottom() - 74.), egui::Align2::LEFT_BOTTOM, "PEAKRUNNER",
            egui::FontId::proportional(36.), Color32::WHITE);
        painter.text(egui::pos2(left + 2., rect.bottom() - 52.), egui::Align2::LEFT_BOTTOM, "Ski the ridgelines. Jet the gaps. Take their flag.",
            egui::FontId::proportional(14.), Color32::from_rgb(232, 214, 196));
        // The platform, top right.
        let tag = painter.layout_no_wrap(platform_name(updater::platform()).into(), egui::FontId::proportional(12.), TEXT);
        let tag_rect = egui::Rect::from_min_size(egui::pos2(rect.right() - 16. - tag.size().x - 20., rect.top() + 14.), tag.size() + Vec2::new(20., 10.));
        painter.rect_filled(tag_rect, 10, Color32::from_rgba_unmultiplied(8, 12, 16, 150));
        painter.galley(tag_rect.min + Vec2::new(10., 5.), tag, TEXT);
        // The links, along the bottom under the name.
        let mut x = left;
        let y = rect.bottom() - 28.;
        for (icon, label, url) in LINKS {
            x += link_pill(ui, egui::pos2(x, y), icon, label, url) + 8.;
        }
    }

    /// The main card: what state the game is in, the one thing to do next,
    /// and the less common actions beside it.
    fn hero(&mut self, ui: &mut egui::Ui) {
        let ready = !self.busy && self.child.is_none() && self.store.is_some();
        let latest = self.available.as_ref().map(|r| r.manifest.version.clone());
        let behind = latest.is_some() && latest != self.installed;
        let (title, primary) = if self.child.is_some() {
            ("PeakRunner is running", None)
        } else if self.busy {
            (if self.installed.is_none() { "Installing" } else { "Working" }, None)
        } else if self.installed.is_none() {
            // Nothing to install until a release is known.
            if self.available.is_some() { ("Ready to install", Some(("Install", Action::Install))) } else { ("Not installed yet", Some(("Install", Action::None))) }
        } else if behind {
            ("Update available", Some(("Update", Action::Install)))
        } else {
            ("Ready to play", Some(("Play", Action::Play)))
        };
        card(ui, |ui| {
            ui.label(RichText::new(title).size(24.).strong());
            let version = match (&self.installed, &latest) {
                (Some(have), Some(new)) if behind => format!("Installed {have} · new {new}"),
                (Some(have), _) => format!("Installed {have}"),
                (None, Some(new)) => format!("Latest {new}"),
                (None, None) => "Version unknown until the update check works".into(),
            };
            ui.label(RichText::new(version).size(13.).color(MUTED));
            ui.add_space(14.);
            ui.horizontal_wrapped(|ui| {
                let (label, action) = primary.unwrap_or((if self.child.is_some() { "Playing" } else { "Please wait" }, Action::None));
                let main = egui::Button::new(RichText::new(label).size(17.).strong().color(if action == Action::None { MUTED } else { BG }))
                    .fill(if action == Action::None { PANEL_HI } else { ACCENT }).corner_radius(10).min_size(Vec2::new(176., 48.));
                if ui.add_enabled(ready && action != Action::None, main).clicked() {
                    match action { Action::Play => self.play(), Action::Install => self.install(), Action::None => {} }
                }
                // Behind the main action, quieter: they're seldom needed.
                ui.add_space(8.);
                let quiet = |text: &str| egui::Button::new(RichText::new(text).size(13.).color(MUTED)).frame(false);
                if ui.add_enabled(ready, quiet("Check for updates")).clicked() { self.check(); }
                if self.installed.is_some() && !behind && ui.add_enabled(ready && self.available.is_some(), quiet("Repair")).clicked() { self.install(); }
                if self.installed.is_some() && ui.add_enabled(ready, quiet("Restore previous")).clicked() { self.rollback_confirm = true; }
            });
            if self.rollback_confirm {
                ui.add_space(10.);
                egui::Frame::new().fill(PANEL_HI).corner_radius(8).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label("Go back to the version installed before this one? It may not match the live servers.");
                    ui.horizontal(|ui| {
                        if ui.button("Restore previous").clicked() { self.rollback_confirm = false; self.rollback(); }
                        if ui.button("Cancel").clicked() { self.rollback_confirm = false; }
                    });
                });
            }
            if self.busy || !self.status.is_empty() {
                ui.add_space(10.);
                ui.horizontal_wrapped(|ui| {
                    if self.busy { ui.spinner(); }
                    let warn = self.status.starts_with("The ") || self.status.starts_with("Couldn") || self.status.starts_with("Install");
                    ui.label(RichText::new(&self.status).size(13.).color(if warn && !self.busy { WARN } else { MUTED }));
                });
            }
        });
    }

    /// The release notes of the newest release.
    fn notes(&self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            heading(ui, "WHAT'S NEW");
            match &self.available {
                Some(r) => {
                    ui.label(RichText::new(&r.manifest.version).size(12.).color(MUTED));
                    ui.add_space(4.);
                    egui::ScrollArea::vertical().id_salt("notes").max_height(220.).show(ui, |ui| {
                        ui.label(RichText::new(&r.manifest.notes).size(13.).color(TEXT));
                    });
                }
                None => { ui.label(RichText::new("Release notes appear after the update check.").size(13.).color(MUTED)); }
            }
        });
    }

    /// The live servers, with how full each is.
    fn servers(&self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            heading(ui, "SERVERS ONLINE");
            if self.hosts.is_empty() {
                ui.label(RichText::new("No servers listed right now, or the list can't be reached.").size(13.).color(MUTED));
            }
            for (k, host) in self.hosts.iter().enumerate() {
                if k > 0 { ui.add_space(2.); }
                ui.horizontal(|ui| {
                    let busy = host.players > 0;
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4., if busy { GOOD } else { MUTED.gamma_multiply(0.5) });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        ui.label(RichText::new(&host.name).size(14.).strong());
                        let (map, mode) = host.map.split_once(" - ").unwrap_or((host.map.as_str(), ""));
                        let detail = if mode.is_empty() { map.to_string() } else { format!("{map} · {mode}") };
                        ui.label(RichText::new(detail).size(12.).color(MUTED));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} / {}", host.players, host.max_players)).size(13.)
                            .color(if busy { TEXT } else { MUTED }));
                    });
                });
            }
            ui.add_space(4.);
            ui.label(RichText::new("Pick a server from inside the game.").size(12.).color(MUTED));
        });
    }

    fn footer(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 16.;
            let note = ui.label(RichText::new("Only changed files download; a failed update keeps your working game.").size(11.).color(MUTED));
            if let Some(store) = &self.store { note.on_hover_text(format!("Game folder: {}", store.root.display())); }
            ui.hyperlink_to(RichText::new("Help").size(11.).color(ACCENT), "https://peakrunner.net/#downloads");
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action { Play, Install, None }

/// A link drawn as a pill at `at` (its left middle): an icon and a label on a
/// dark glass, brighter under the pointer; a click opens it in the browser.
/// Returns its width.
fn link_pill(ui: &mut egui::Ui, at: egui::Pos2, icon: Icon, label: &str, url: &str) -> f32 {
    let text = ui.painter().layout_no_wrap(label.into(), egui::FontId::proportional(13.), TEXT);
    let size = Vec2::new(16. + 18. + text.size().x + 14., 26.);
    let rect = egui::Rect::from_min_size(egui::pos2(at.x, at.y - size.y / 2.), size);
    let response = ui.interact(rect, ui.id().with(url), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let glass = if response.hovered() { Color32::from_rgba_unmultiplied(40, 60, 72, 220) } else { Color32::from_rgba_unmultiplied(8, 12, 16, 170) };
    painter.rect_filled(rect, 13, glass);
    let c = egui::pos2(rect.left() + 18., rect.center().y);
    match icon {
        // A globe: a ring, its equator and a meridian.
        Icon::Web => {
            let stroke = egui::Stroke::new(1.4, ACCENT);
            painter.circle_stroke(c, 6.5, stroke);
            painter.line_segment([c - Vec2::X * 6.5, c + Vec2::X * 6.5], stroke);
            painter.add(egui::Shape::ellipse_stroke(c, Vec2::new(2.8, 6.5), stroke));
        }
        // A play button.
        Icon::Video => {
            painter.rect_filled(egui::Rect::from_center_size(c, Vec2::new(16., 11.)), 3, Color32::from_rgb(230, 40, 40));
            painter.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-2.5, -3.), c + Vec2::new(3.5, 0.), c + Vec2::new(-2.5, 3.)], Color32::WHITE, egui::Stroke::NONE));
        }
        // A speech bubble for the community.
        Icon::Forum => {
            painter.circle_filled(c, 7., Color32::from_rgb(255, 86, 0));
            painter.circle_filled(c + Vec2::new(-2.4, 0.5), 1.3, Color32::WHITE);
            painter.circle_filled(c + Vec2::new(2.4, 0.5), 1.3, Color32::WHITE);
        }
    }
    painter.galley(egui::pos2(rect.left() + 32., rect.center().y - text.size().y / 2.), text, TEXT);
    if response.clicked() { ui.ctx().open_url(egui::OpenUrl::new_tab(url)); }
    size.x
}

/// The dock's icon on a Mac (Apple's grid, its shadow drawn in), the
/// window's elsewhere (assets/icons, scripts/build-icons.py).
#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../../../assets/icons/launcher-mac.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../../../assets/icons/launcher.png");

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([820., 830.]).with_min_inner_size([620., 600.]);
    // Our icon in the dock or taskbar, not the toolkit's.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) { viewport = viewport.with_icon(icon); }
    eframe::run_native(
        "PeakRunner Launcher",
        eframe::NativeOptions {
            viewport,
            renderer: updater::native_renderer(),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Launcher::new(cc)))),
    )
}
