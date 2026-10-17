//! The round countdown: a large centred 10, 9, … 1, then a brief GO, with
//! the map and mode under it. Online it follows the server's Countdown phase
//! (run down smoothly between snapshots, `Online::countdown_left`); offline,
//! the world's own `countdown`. A click on each new number and the start cue
//! on GO (both existing synthesized cues).
use egui::{Align2, Color32, FontId, Vec2};

/// How long GO stays up once play begins (s).
const GO_SECONDS: f32 = 1.0;
const GO: Color32 = Color32::from_rgb(120, 230, 140);

#[derive(Default)]
pub struct CountdownHud {
    /// The whole second shown (None: no countdown).
    shown: Option<u32>,
    /// Seconds GO stays up.
    go: f32,
}

impl CountdownHud {
    /// `left`: seconds before play while a countdown runs (None otherwise);
    /// `playing`: play is on. Returns the cue to play this frame, if any.
    pub fn update(&mut self, left: Option<f32>, playing: bool, dt: f32) -> Option<&'static str> {
        self.go = (self.go - dt).max(0.0);
        match left {
            Some(left) if left > 0.0 => {
                let n = left.ceil() as u32;
                let cue = (self.shown != Some(n)).then_some("count");
                self.shown = Some(n);
                cue
            }
            // Reached zero (or play began): GO, once.
            _ if self.shown.is_some() && (playing || left.is_some()) => {
                self.shown = None;
                self.go = GO_SECONDS;
                Some("go")
            }
            // Cancelled (a team emptied) or none running.
            _ => { self.shown = None; None }
        }
    }

    /// What the countdown shows now.
    pub fn text(&self) -> Option<String> {
        match self.shown {
            Some(n) => Some(n.to_string()),
            None if self.go > 0.0 => Some("GO".into()),
            None => None,
        }
    }
}

/// Draws `text` (a number or GO) above the crosshair, with `caption` (map
/// and mode) under it.
pub fn draw(ui: &egui::Ui, text: &str, caption: &str) {
    let painter = ui.painter();
    let c = ui.max_rect().center();
    let shadow = Color32::from_black_alpha(170);
    let go = text == "GO";
    if !go {
        painter.text(c + Vec2::new(1.5, -219.0), Align2::CENTER_CENTER, "ROUND STARTS IN", FontId::proportional(15.0), shadow);
        painter.text(c + Vec2::new(0.0, -220.0), Align2::CENTER_CENTER, "ROUND STARTS IN", FontId::proportional(15.0),
            Color32::from_rgb(154, 168, 184));
    }
    painter.text(c + Vec2::new(3.0, -137.0), Align2::CENTER_CENTER, text, FontId::proportional(112.0), shadow);
    painter.text(c + Vec2::new(0.0, -140.0), Align2::CENTER_CENTER, text, FontId::proportional(112.0),
        if go { GO } else { Color32::from_rgb(231, 238, 246) });
    if !caption.is_empty() {
        painter.text(c + Vec2::new(1.5, -59.0), Align2::CENTER_CENTER, caption, FontId::proportional(20.0), shadow);
        painter.text(c + Vec2::new(0.0, -60.0), Align2::CENTER_CENTER, caption, FontId::proportional(20.0),
            Color32::from_rgb(231, 238, 246));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_second_then_says_go_once() {
        let mut hud = CountdownHud::default();
        let dt = 1.0 / 144.0;
        let (mut left, mut shown, mut cues) = (10.0f32, Vec::new(), Vec::new());
        while left > 0.0 {
            if let Some(cue) = hud.update(Some(left), false, dt) { cues.push(cue); }
            let text = hud.text().unwrap();
            if shown.last() != Some(&text) { shown.push(text); }
            left -= dt;
        }
        assert_eq!(shown, ["10", "9", "8", "7", "6", "5", "4", "3", "2", "1"]);
        assert_eq!(cues.len(), 10);
        assert!(cues.iter().all(|c| *c == "count"));
        // Zero, whether the smoothed value or the play phase gets there first.
        assert_eq!(hud.update(Some(0.0), false, dt), Some("go"));
        assert_eq!(hud.text().as_deref(), Some("GO"));
        assert_eq!(hud.update(None, true, dt), None, "GO once");
        for _ in 0..144 { hud.update(None, true, dt); }
        assert_eq!(hud.text(), None, "GO is brief");
        // Straight into play from a countdown also says GO.
        hud.update(Some(0.4), false, dt);
        assert_eq!(hud.text().as_deref(), Some("1"));
        assert_eq!(hud.update(None, true, dt), Some("go"));
    }

    #[test]
    fn a_cancelled_countdown_says_nothing() {
        let mut hud = CountdownHud::default();
        hud.update(Some(6.5), false, 0.01);
        assert_eq!(hud.text().as_deref(), Some("7"));
        // Back to waiting for an opponent: no GO.
        assert_eq!(hud.update(None, false, 0.01), None);
        assert_eq!(hud.text(), None);
        // Nothing counting, nothing shown.
        assert_eq!(hud.update(None, true, 0.01), None);
        assert_eq!(hud.text(), None);
    }
}
