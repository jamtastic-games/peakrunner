#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

/// The dock's icon on a Mac (Apple's grid, its shadow drawn in), the
/// window's elsewhere.
#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../assets/icons/peakrunner-mac.png");
#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
const ICON: &[u8] = include_bytes!("../assets/icons/peakrunner.png");

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    // Hold a separate launcher-session lock even if the launcher itself exits.
    // Managed installs are immutable; no update may race a running managed game.
    let _launcher_lock = if let Some(path) = std::env::var_os("PEAKRUNNER_LAUNCHER_LOCK") {
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)
            .map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;
        file.try_lock().map_err(|e| eframe::Error::AppCreation(Box::new(std::io::Error::other(e.to_string()))))?;
        Some(file)
    } else { None };
    // Maps downloaded from servers live here (docs/modding.md, P2). Set
    // before anything asks the catalog about maps.
    if let Some(dir) = peakrunner::preferences::download_dir() { peakrunner_core::catalog::set_cache_dir(dir); }
    // The player's own maps and mods, shared with the Host game's server.
    if let Some(dir) = peakrunner::preferences::own_content_dir() { peakrunner_core::catalog::add_root(dir); }
    // PEAKRUNNER_MODS: installed mods to play offline (docs/making-content.md).
    if let Err(e) = peakrunner_core::mods::activate_home(&peakrunner_core::mods::from_env()) { eprintln!("PeakRunner: {e}; playing without mods"); }
    // Graphics screen settings that must be set when the window is made.
    let (vsync, fullscreen) = peakrunner::preferences::startup_graphics();
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    wgpu_options.surface.present_mode = if vsync { eframe::wgpu::PresentMode::AutoVsync } else { eframe::wgpu::PresentMode::AutoNoVsync };
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_fullscreen(fullscreen)
        .with_min_inner_size([960.0, 600.0])
        .with_title(if std::env::var_os("PEAKRUNNER_MAP_PACK").is_some() {"PeakRunner — private map reference"} else {"PeakRunner"});
    // Our icon in the dock or taskbar, not the toolkit's: a launcher's game
    // runs outside any app bundle (assets/icons, scripts/build-icons.py).
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) { viewport = viewport.with_icon(icon); }
    let native_options = eframe::NativeOptions {
        wgpu_options,
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "PeakRunner",
        native_options,
        Box::new(|cc| {
            peakrunner::PeakRunnerApp::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> {
                    std::io::Error::other(err).into()
                })
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("document");
        let canvas = document
            .get_element_by_id("peakrunner_canvas")
            .expect("canvas #peakrunner_canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("canvas element");
        let start = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    peakrunner::PeakRunnerApp::new(cc)
                        .map(|app| Box::new(app) as Box<dyn eframe::App>)
                        .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> {
                            std::io::Error::other(err).into()
                        })
                }),
            )
            .await;
        if let Err(err) = start {
            log::error!("PeakRunner failed to start: {err:?}");
        }
    });
}
