//! Frame pacing: turn the measured frame time into an even one.
//!
//! With vsync, frames are shown on whole display refreshes, but the measured
//! time between frames jitters by a millisecond or two, and a late frame
//! lands a whole refresh later. The simulation's fixed-step clock and the
//! between-steps camera follow that time exactly, so jitter in it becomes
//! jitter on screen. The pacer learns the display's refresh interval and
//! snaps each frame time to a whole number of refreshes. The rounding error
//! is kept and paid back gradually, so game time never drifts from wall
//! time.
//!
//! `FrameStats` keeps a one-second window for the F3 overlay: frame rate,
//! frame-time spread, and how many frames ran 0, 1 or 2+ simulation steps.

/// Frame times this far from a whole number of refreshes are left alone
/// (a real hitch, or the display rate changing).
const SNAP_TOLERANCE: f32 = 0.2;
/// Share of the accumulated rounding error paid back each frame, and the
/// most error kept (anything beyond is real time the clock already lost).
const PAYBACK: f32 = 0.1;
const MAX_DEBT: f32 = 0.05;
/// Recent frame times the refresh interval is estimated from (their median,
/// so hitches and catch-up frames after them don't move it).
const WINDOW: usize = 90;

pub struct FramePacer {
    recent: [f32; WINDOW],
    len: usize,
    next: usize,
    period: f32,
    debt: f32,
}

impl Default for FramePacer {
    fn default() -> Self { Self { recent: [0.0; WINDOW], len: 0, next: 0, period: 0.0, debt: 0.0 } }
}

impl FramePacer {
    /// Forget the display's rhythm (vsync toggled, a long stall).
    pub fn reset(&mut self) { *self = Self::default(); }

    /// The paced frame time for a measured one (seconds).
    pub fn dt(&mut self, raw: f32) -> f32 {
        let raw = if raw.is_finite() { raw.clamp(0.0, 0.25) } else { 0.0 };
        if raw <= 0.0 { return 1.0 / 60.0; }
        if raw > 0.1 { self.reset(); return raw.min(0.1); }
        // The refresh interval: the median of recent plausible frame times.
        if (1.0 / 500.0..=1.0 / 20.0).contains(&raw) {
            self.recent[self.next] = raw;
            self.next = (self.next + 1) % WINDOW;
            self.len = (self.len + 1).min(WINDOW);
            let mut sorted = self.recent[..self.len].to_vec();
            sorted.sort_by(f32::total_cmp);
            self.period = sorted[self.len / 2];
        }
        if self.len < 8 || self.period <= 0.0 { return raw; }
        let refreshes = (raw / self.period).round().max(1.0);
        let snapped = refreshes * self.period;
        let mut out = if (raw - snapped).abs() < self.period * SNAP_TOLERANCE { snapped } else { raw };
        // Pay back the rounding error slowly so game time tracks wall time.
        self.debt = (self.debt + raw - out).clamp(-MAX_DEBT, MAX_DEBT);
        let pay = self.debt * PAYBACK;
        out += pay;
        self.debt -= pay;
        out.max(0.0)
    }

    pub fn period(&self) -> f32 { self.period }
}

/// Rolling one-second frame statistics for the F3 overlay.
#[derive(Default)]
pub struct FrameStats {
    window: f32,
    frames: u32,
    min: f32,
    max: f32,
    steps: [u32; 3],
    /// Last completed second: (fps, min ms, max ms, frames with 0/1/2+ steps).
    pub last: Option<(f32, f32, f32, [u32; 3])>,
}

impl FrameStats {
    pub fn record(&mut self, raw_dt: f32, steps: usize) {
        if self.frames == 0 { self.min = f32::MAX; self.max = 0.0; }
        self.frames += 1;
        self.window += raw_dt;
        self.min = self.min.min(raw_dt);
        self.max = self.max.max(raw_dt);
        self.steps[steps.min(2)] += 1;
        if self.window >= 1.0 {
            self.last = Some((self.frames as f32 / self.window, self.min * 1000.0, self.max * 1000.0, self.steps));
            *self = FrameStats { last: self.last, ..Default::default() };
        }
    }

    pub fn text(&self, pacer: &FramePacer) -> String {
        match self.last {
            Some((fps, min, max, s)) => format!("{fps:.0} fps · frame {min:.1}–{max:.1} ms · display {:.1} ms · steps/frame 0:{} 1:{} 2+:{}",
                pacer.period() * 1000.0, s[0], s[1], s[2]),
            None => "measuring frames…".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jittery(period: f32, n: usize) -> Vec<f32> {
        // ±12% of the refresh interval (±2 ms at 60 Hz), deterministic.
        (0..n).map(|i| period * (1.0 + ((i * 7919 % 13) as f32 / 12.0 - 0.5) * 0.24)).collect()
    }

    #[test]
    fn jittered_vsync_frames_come_out_even_and_keep_wall_time() {
        for period in [1.0 / 60.0, 1.0 / 120.0] {
            let mut p = FramePacer::default();
            let raw = jittery(period, 600);
            let out: Vec<f32> = raw.iter().map(|&d| p.dt(d)).collect();
            let settled = &out[100..];
            let spread = settled.iter().cloned().fold(0.0f32, f32::max) - settled.iter().cloned().fold(f32::MAX, f32::min);
            assert!(spread < period * 0.05, "{period}: paced frames vary by {spread}");
            let drift = raw.iter().sum::<f32>() - out.iter().sum::<f32>();
            assert!(drift.abs() < 0.01, "game time tracks wall time: drift {drift}");
        }
    }

    #[test]
    fn a_missed_refresh_is_two_whole_refreshes_and_a_real_hitch_passes_through() {
        let mut p = FramePacer::default();
        for d in jittery(1.0 / 60.0, 200) { p.dt(d); }
        let two = p.dt(2.0 / 60.0 + 0.001);
        assert!((two - 2.0 / 60.0).abs() < 0.0015, "{two}");
        let hitch = p.dt(0.045);
        assert!(hitch > 0.04, "a 45 ms hitch is kept: {hitch}");
    }

    #[test]
    fn a_hitch_and_catch_up_frame_or_a_vsync_off_burst_do_not_break_pacing() {
        let mut p = FramePacer::default();
        for d in jittery(1.0 / 60.0, 200) { p.dt(d); }
        p.dt(0.030); p.dt(0.0035);              // a hitch, then a catch-up frame
        for _ in 0..30 { p.dt(0.0015); }         // occluded / vsync off for half a second
        for d in jittery(1.0 / 60.0, 120) { p.dt(d); }
        assert!((p.period() - 1.0 / 60.0).abs() < 0.001, "back on the 60 Hz rhythm: {}", p.period());
        let out: Vec<f32> = jittery(1.0 / 60.0, 60).into_iter().map(|d| p.dt(d)).collect();
        let spread = out.iter().cloned().fold(0.0f32, f32::max) - out.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.001, "even again: {spread}");
    }

    #[test]
    fn a_slower_display_is_followed_too() {
        let mut p = FramePacer::default();
        for d in jittery(1.0 / 144.0, 200) { p.dt(d); }
        for d in jittery(1.0 / 60.0, 120) { p.dt(d); }
        assert!((p.period() - 1.0 / 60.0).abs() < 0.001, "{}", p.period());
    }

    #[test]
    fn the_pacer_follows_a_faster_display() {
        let mut p = FramePacer::default();
        for d in jittery(1.0 / 60.0, 200) { p.dt(d); }
        for d in jittery(1.0 / 120.0, 200) { p.dt(d); }
        assert!((p.period() - 1.0 / 120.0).abs() < 0.0006, "{}", p.period());
    }
}
