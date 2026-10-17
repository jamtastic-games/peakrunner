//! Platform audio output. The sounds, mixing and cue decisions live in
//! `sound.rs`; this file only feeds that mixer to the system (rodio) or the
//! browser (WebAudio), so both platforms hear identical audio. A missing or
//! failing audio device is never an error: the game simply stays silent.

use crate::sim::World;
use crate::sound::{Cue, Director, Listener, LoopTarget, Mixer, Play, LOOPS};
use crate::terrain::MapId;
use glam::Vec3;

const MASTER: f32 = 0.85;

pub struct Audio {
    muted: bool,
    director: Director,
    listener: Listener,
    plays: Vec<Play>,
    ambient_map: Option<MapId>,
    backend: Backend,
    /// The active mods' sounds by name (their numbers in the mixer), and
    /// the definitions they were loaded for.
    custom: Vec<String>,
    custom_for: usize,
}

impl Audio {
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn silent() -> Self {
        Self::with_backend(Backend::none(), true)
    }

    pub fn new() -> Self {
        Self::with_backend(Backend::open(), false)
    }

    fn with_backend(backend: Backend, muted: bool) -> Self {
        Audio {
            muted,
            director: Director::new(),
            listener: Listener { pos: Vec3::ZERO, forward: Vec3::NEG_Z },
            plays: Vec::with_capacity(64),
            ambient_map: None,
            backend,
            custom: Vec::new(),
            custom_for: 0,
        }
    }

    /// Browsers only start audio after a user gesture.
    pub fn unlock(&mut self) {
        self.backend.unlock();
        let master = if self.muted { 0.0 } else { MASTER };
        self.backend.with(|m| m.set_master(master));
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.backend.with(|m| m.set_master(if muted { 0.0 } else { MASTER }));
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    /// The map's ambient bed, loudest near its emitters. Loaded on map change.
    pub fn set_map_ambience(&mut self, map: MapId, eye: Vec3, playing: bool) {
        let pack = peakrunner_core::map_pack::on(map);
        if self.ambient_map != Some(map) {
            self.ambient_map = Some(map);
            let samples = pack.and_then(|p| p.asset("ambient.f32").ok()).map(|bytes| {
                bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect::<Vec<f32>>()
            });
            self.backend.with(|m| m.set_ambient(samples.map(|s| (s, 44_100.0))));
        }
        let indoor = self.director.indoor();
        let gain = pack.filter(|_| playing).map_or(0.0, |pack| {
            pack.manifest.ambient_emitters.iter().map(|e| {
                let distance = eye.distance(Vec3::new(e[0], e[1], e[2]));
                e[3] * (e[4] / distance.max(e[4])).min(1.0) * (1.0 - distance / e[5]).clamp(0.0, 1.0)
            }).fold(0.0_f32, f32::max)
        }) * if indoor { 0.35 } else { 1.0 };
        self.backend.with(|m| m.set_ambient_gain(gain));
    }

    /// Per-frame: move the listener, derive cues from the world, update loops.
    /// Call before `play_world` so spatial sounds use this frame's listener.
    pub fn frame(&mut self, world: &World, dt: f32, playing: bool) {
        self.refresh_custom();
        let (eye, target, _) = world.camera();
        self.listener = Listener { pos: eye, forward: (target - eye).normalize_or(Vec3::NEG_Z) };
        self.plays.clear();
        let loops = self.director.update(world, &self.listener, dt, playing, &mut self.plays);
        let indoor = self.director.indoor();
        let plays = &self.plays;
        self.backend.with(|m| {
            for p in plays { m.play(*p); }
            for (l, t) in LOOPS.iter().zip(loops) { m.set_loop(*l, t); }
            m.set_room(indoor);
        });
    }

    /// The active mods' sounds, loaded again when the definitions in force
    /// change (a server's mods were activated or dropped).
    fn refresh_custom(&mut self) {
        let now = peakrunner_core::registry::registry() as *const _ as usize;
        if now == self.custom_for { return; }
        self.custom_for = now;
        let sounds: Vec<(String, (Vec<f32>, u32))> = peakrunner_core::mods::sounds().into_iter()
            .filter_map(|(name, bytes)| crate::sound::parse_wav(&bytes).map(|c| (name, c))).collect();
        self.custom = sounds.iter().map(|(n, _)| n.clone()).collect();
        let clips: Vec<(Vec<f32>, u32)> = sounds.into_iter().map(|(_, c)| c).collect();
        self.backend.with(|m| m.set_custom(clips));
    }

    /// A mod's sound by name.
    fn custom(&self, name: &str) -> Option<u16> { self.custom.iter().position(|n| n == name).map(|k| k as u16) }

    /// A HUD/sim event for the local player (own weapon, flag, UI); a mod's
    /// sound of that name where no cue has it.
    pub fn play(&mut self, name: &str) {
        let p = match Cue::from_event(name) {
            Some(cue) => self.director.local(cue),
            None => { let Some(k) = self.custom(name) else { return }; Play { custom: Some(k), ..Play::local(Cue::Switch, 0) } }
        };
        self.backend.with(|m| m.play(p));
    }

    /// A sound at a world position: distance, pan, dullness and, for far
    /// explosions, travel delay.
    pub fn play_world(&mut self, name: &str, pos: Vec3, world: &World) {
        let Some(cue) = Cue::from_event(name) else {
            let Some(k) = self.custom(name) else { return };
            if let Some(p) = self.director.world(&self.listener, Cue::Switch, pos, 1.0) { self.backend.with(|m| m.play(Play { custom: Some(k), ..p })); }
            return;
        };
        let play = if cue == Cue::BoomNear {
            self.director.explosion(&self.listener, world, pos)
        } else {
            self.director.world(&self.listener, cue, pos, 1.0)
        };
        if let Some(p) = play { self.backend.with(|m| m.play(p)); }
    }

    /// Leaving play silences every loop at once (jet, ski, wind, spin, hums).
    pub fn set_jet(&mut self, on: bool) {
        if on { return; }
        self.backend.with(|m| for l in LOOPS { m.set_loop(l, LoopTarget { rate: 1.0, cutoff: 18_000.0, ..LoopTarget::default() }); });
    }
}

// ------------------------------------------------------------------ native

#[cfg(not(target_arch = "wasm32"))]
struct Backend {
    _sink: Option<rodio::MixerDeviceSink>,
    mixer: Option<std::sync::Arc<std::sync::Mutex<Mixer>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Backend {
    #[cfg(test)]
    fn none() -> Self { Backend { _sink: None, mixer: None } }

    fn open() -> Self {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|err| log::warn!("audio output unavailable: {err}"))
            .ok();
        let Some(sink) = sink else { return Backend { _sink: None, mixer: None } };
        let mut m = Mixer::new(RATE as f32);
        if let Some(dir) = sound_dir() {
            let used = m.load_overrides(&dir);
            if used > 0 { log::info!("using {used} custom sound files from {}", dir.display()); }
        }
        let mixer = std::sync::Arc::new(std::sync::Mutex::new(m));
        sink.mixer().add(Engine { mixer: mixer.clone(), block: vec![0.0; BLOCK * 2], at: BLOCK * 2 });
        Backend { _sink: Some(sink), mixer: Some(mixer) }
    }

    fn unlock(&mut self) {}

    fn with(&self, f: impl FnOnce(&mut Mixer)) {
        if let Some(m) = &self.mixer {
            if let Ok(mut m) = m.lock() { f(&mut m); }
        }
    }
}

/// Folder of custom sound files that replace the synthesized ones:
/// `PEAKRUNNER_SOUND_DIR`, else `sounds/` beside the executable (or in the
/// macOS bundle's Resources), else `assets/sounds` when run from `src/`.
#[cfg(not(target_arch = "wasm32"))]
fn sound_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("PEAKRUNNER_SOUND_DIR") { return Some(dir.into()); }
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    [parent.join("sounds"), parent.join("../Resources/sounds"), std::path::PathBuf::from("assets/sounds")]
        .into_iter().find(|d| d.is_dir())
}

#[cfg(not(target_arch = "wasm32"))]
const RATE: u32 = 44_100;
#[cfg(not(target_arch = "wasm32"))]
const BLOCK: usize = 512;

/// Endless stereo source that renders the shared mixer in small blocks, so the
/// lock is taken once per block rather than per sample.
#[cfg(not(target_arch = "wasm32"))]
struct Engine {
    mixer: std::sync::Arc<std::sync::Mutex<Mixer>>,
    block: Vec<f32>,
    at: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl Iterator for Engine {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.at >= self.block.len() {
            match self.mixer.lock() {
                Ok(mut m) => m.render(&mut self.block),
                Err(_) => self.block.fill(0.0),
            }
            self.at = 0;
        }
        let s = self.block[self.at];
        self.at += 1;
        Some(s)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl rodio::Source for Engine {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> rodio::ChannelCount { std::num::NonZero::new(2).expect("nonzero") }
    fn sample_rate(&self) -> rodio::SampleRate { std::num::NonZero::new(RATE).expect("nonzero") }
    fn total_duration(&self) -> Option<std::time::Duration> { None }
}

// --------------------------------------------------------------------- web

#[cfg(target_arch = "wasm32")]
struct Backend {
    ctx: Option<web_sys::AudioContext>,
    mixer: Option<std::rc::Rc<std::cell::RefCell<Mixer>>>,
    _node: Option<web_sys::ScriptProcessorNode>,
    _callback: Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::AudioProcessingEvent)>>,
}

#[cfg(target_arch = "wasm32")]
impl Backend {
    fn open() -> Self { Backend { ctx: None, mixer: None, _node: None, _callback: None } }

    /// Created on the first user gesture; any failure leaves the game silent.
    fn unlock(&mut self) {
        use wasm_bindgen::JsCast;
        if self.ctx.is_none() {
            let Ok(ctx) = web_sys::AudioContext::new() else { return };
            let Ok(node) = ctx.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(1024, 0, 2) else { return };
            let mixer = std::rc::Rc::new(std::cell::RefCell::new(Mixer::new(ctx.sample_rate())));
            let shared = mixer.clone();
            let mut inter = vec![0.0_f32; 2048];
            let mut left = vec![0.0_f32; 1024];
            let mut right = vec![0.0_f32; 1024];
            let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::AudioProcessingEvent)>::new(move |e: web_sys::AudioProcessingEvent| {
                let Ok(out) = e.output_buffer() else { return };
                let frames = out.length() as usize;
                if inter.len() < frames * 2 { inter.resize(frames * 2, 0.0); left.resize(frames, 0.0); right.resize(frames, 0.0); }
                if let Ok(mut m) = shared.try_borrow_mut() { m.render(&mut inter[..frames * 2]); } else { inter.fill(0.0); }
                for i in 0..frames { left[i] = inter[i * 2]; right[i] = inter[i * 2 + 1]; }
                let _ = out.copy_to_channel(&left[..frames], 0);
                let _ = out.copy_to_channel(&right[..frames], 1);
            });
            node.set_onaudioprocess(Some(callback.as_ref().unchecked_ref()));
            if node.connect_with_audio_node(&ctx.destination()).is_err() { return; }
            self.ctx = Some(ctx);
            self.mixer = Some(mixer);
            self._node = Some(node);
            self._callback = Some(callback);
        }
        if let Some(ctx) = &self.ctx {
            if ctx.state() == web_sys::AudioContextState::Suspended { let _ = ctx.resume(); }
        }
    }

    fn with(&self, f: impl FnOnce(&mut Mixer)) {
        if let Some(m) = &self.mixer {
            if let Ok(mut m) = m.try_borrow_mut() { f(&mut m); }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn silent_backend_accepts_everything_without_a_device() {
        let mut a = super::Audio::silent();
        let w = crate::sim::World::new();
        a.frame(&w, 1.0 / 60.0, true);
        for e in ["disc", "chain", "grenade", "boom", "flag", "capture_win", "nonsense"] {
            a.play(e);
            a.play_world(e, glam::Vec3::new(10.0, 0.0, -10.0), &w);
        }
        a.set_map_ambience(crate::terrain::MapId::OldHoller, glam::Vec3::ZERO, true);
        a.set_jet(false);
        a.set_muted(true);
        assert!(a.muted());
    }
}
