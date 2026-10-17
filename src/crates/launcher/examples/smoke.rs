//! Actual native launcher render check, with an explicitly isolated store.
#[allow(dead_code, unused_attributes)]
#[path = "../src/main.rs"]
mod gui;
use eframe::egui;
use std::time::Instant;
struct Probe {
    app: gui::Launcher,
    start: Instant,
    requested: bool,
}
impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        let ctx = ui.ctx();
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.iter().any(|p| p.r() > 150), "Blank launcher");
            let file = std::fs::File::create(std::env::var("QA_CAPTURE_PATH").unwrap()).unwrap();
            let mut encoder = png::Encoder::new(file, image.width() as u32, image.height() as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
            println!("PASS: native launcher rendered");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.start.elapsed().as_secs() > 24 && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        assert!(
            self.start.elapsed().as_secs() < 60,
            "Launcher screenshot timed out"
        );
        ctx.request_repaint();
    }
}
fn main() -> eframe::Result {
    std::env::var("PEAKRUNNER_LAUNCHER_DATA_DIR").expect("Set an isolated QA store");
    std::env::var("QA_CAPTURE_PATH").expect("Set screenshot output path");
    eframe::run_native(
        "PeakRunner Launcher QA",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([820., 830.]),
            renderer: peakrunner_launcher::native_renderer(),
            ..Default::default()
        },
        Box::new(|cc| {
            Ok(Box::new(Probe {
                app: gui::Launcher::new(cc),
                start: Instant::now(),
                requested: false,
            }))
        }),
    )
}
