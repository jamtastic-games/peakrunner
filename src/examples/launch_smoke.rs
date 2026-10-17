//! Launch the real client renderer and save its window, then close only this test.
//! QA_CAPTURE_PATH must be set. Optional PEAKRUNNER_JOIN exercises a live match.
//! Optional QA_CLICKS="x,y@seconds;..." clicks window points at those times (for
//! example a map, a difficulty and Start match), and QA_CAPTURE_AT delays the
//! capture from the default 8 s so an offline bot match can develop. QA_PAUSE_AT
//! moves QA_PAUSE's Escape from 4 s (e.g. after joining a hosted match).
//! QA_FRAMES=dir saves numbered frames there (QA_FRAMES_FPS, default 20) from
//! QA_FRAMES_FROM to QA_FRAMES_TO seconds, for clips; QA_DRIVE (the app's
//! scripted keys) leaves the window unfocused, so the real keyboard and mouse
//! change nothing and the cursor stays free; QA_ON_TOP=1 floats the window
//! over the others, so it's drawn (and captured) unfocused.
use std::time::Instant;
use eframe::egui;
struct Probe { app: peakrunner::PeakRunnerApp, started: Instant, requested: bool, saved: bool, paused: bool, clicked:usize, frames: usize,
    writer: Option<(std::sync::mpsc::Sender<(std::path::PathBuf, egui::ColorImage)>, Vec<std::thread::JoinHandle<()>>)> }
fn write_png(path:&std::path::Path, image:&egui::ColorImage) {
    let file=std::io::BufWriter::new(std::fs::File::create(path).expect("capture output"));
    let mut encoder=png::Encoder::new(file,image.width() as u32,image.height() as u32);
    encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);
    // Not `Fast`: its encoder now and then writes a bad checksum.
    encoder.set_compression(png::Compression::Balanced);
    let pixels:Vec<u8>=image.pixels.iter().flat_map(|p|p.to_array()).collect();
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}
impl eframe::App for Probe {
    fn raw_input_hook(&mut self, ctx:&egui::Context, input:&mut egui::RawInput) {
        let clicks:Vec<(f32,f32,f32)>=std::env::var("QA_CLICKS").unwrap_or_default().split(';').filter_map(|c| {
            let (xy,t)=c.split_once('@')?;let (x,y)=xy.split_once(',')?;
            Some((x.trim().parse().ok()?,y.trim().parse().ok()?,t.trim().parse().ok()?))
        }).collect();
        if let Some(&(x,y,t))=clicks.get(self.clicked) {
            if self.started.elapsed().as_secs_f32()>=t {
                self.clicked+=1;
                let pos=egui::pos2(x,y);
                input.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true,false] {input.events.push(egui::Event::PointerButton {
                    pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()
                });}
            }
        }
        if std::env::var_os("QA_DRIVE").is_some() { input.focused=false; }
        if std::env::var_os("QA_SYSTEM_THEME_LIGHT").is_some() {
            input.system_theme=Some(egui::Theme::Light);
        }
        if !self.paused && std::env::var_os("QA_BROWSER").is_some() && self.started.elapsed().as_secs()>=2 {
            self.paused=true;
            // Deterministic 1280x800 layout: on the Multiplayer page (QA_PAGE,
            // set below), activate the real Find match card rather than
            // changing private app state for a screenshot.
            let pos=egui::pos2(300.,ctx.content_rect().center().y-40.);
                input.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true,false] {input.events.push(egui::Event::PointerButton {
                    pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()
                });}
        }
    }
    fn logic(&mut self, ctx:&egui::Context, frame:&mut eframe::Frame) {
        if std::env::var_os("QA_SCOREBOARD").is_some() && self.started.elapsed().as_secs() >= 4 {
            ctx.input_mut(|i| { i.keys_down.insert(egui::Key::Tab); });
        }
        let chat = std::env::var("QA_CHAT").ok();
        let pause_at = std::env::var("QA_PAUSE_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(4.);
        if !self.paused && self.started.elapsed().as_secs_f32() >= pause_at && (std::env::var_os("QA_PAUSE").is_some() || chat.is_some()) {
            self.paused = true;
            let key = match chat.as_deref() { Some("team")=>egui::Key::Y, Some(_)=>egui::Key::T, None=>egui::Key::Escape };
            ctx.input_mut(|i| i.events.push(egui::Event::Key { key,
                physical_key:None, pressed:true, repeat:false, modifiers:Default::default() }));
        }
        self.app.logic(ctx,frame);
        if std::env::var_os("QA_NET_LOG").is_some() {
            let t = self.started.elapsed().as_secs();
            static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
            if t % 5 == 0 && LAST.swap(t, std::sync::atomic::Ordering::Relaxed) != t { eprintln!("[{t:>3}s] {}", self.app.qa_net_state()); }
        }
    }
    fn ui(&mut self, ui:&mut egui::Ui, frame:&mut eframe::Frame) {
        self.app.ui(ui,frame);
        let ctx=ui.ctx();
        let shots:Vec<_>=ctx.input(|i|i.events.iter().filter_map(|e|match e {
            egui::Event::Screenshot {image,user_data,..}=>Some((image.clone(),user_data.data.as_ref().and_then(|d|d.downcast_ref::<usize>().copied()))), _=>None,
        }).collect());
        for (image,frame) in shots {
            if let (Some(n),Some((writer,_)),Ok(dir))=(frame,&self.writer,std::env::var("QA_FRAMES")) {
                let _=writer.send((std::path::Path::new(&dir).join(format!("{n:05}.png")),(*image).clone()));
                continue;
            }
            assert!(image.pixels.iter().any(|p|p.r()>100),"blank client render");
            let path=std::env::var("QA_CAPTURE_PATH").expect("capture path");
            write_png(std::path::Path::new(&path),&image);
            self.saved=true;println!("PASS: real client window rendered and captured");
            // The frame writer finishes its queue first.
            if let Some((writer,threads))=self.writer.take() { drop(writer); for t in threads { let _=t.join(); } }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Ok(dir)=std::env::var("QA_FRAMES") {
            let num=|k:&str,d:f32| std::env::var(k).ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(d);
            let (fps,from,to)=(num("QA_FRAMES_FPS",20.),num("QA_FRAMES_FROM",0.),num("QA_FRAMES_TO",10.));
            if self.writer.is_none() && !self.saved {
                std::fs::create_dir_all(&dir).expect("frames folder");
                let (tx,rx)=std::sync::mpsc::channel::<(std::path::PathBuf,egui::ColorImage)>();
                // A few writers, so encoding keeps up with the frames.
                let rx=std::sync::Arc::new(std::sync::Mutex::new(rx));
                let threads=(0..3).map(|_| { let rx=rx.clone(); std::thread::spawn(move || loop {
                    let Ok((path,image))=rx.lock().unwrap().recv() else { break };
                    write_png(&path,&image);
                }) }).collect();
                self.writer=Some((tx,threads));
            }
            let t=self.started.elapsed().as_secs_f32();
            let due=((t-from)*fps).floor();
            if t>=from && t<to && due>=self.frames as f32 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(self.frames)));
                self.frames=due as usize+1;
            }
        }
        let capture_at=std::env::var("QA_CAPTURE_AT").ok().and_then(|v|v.parse::<f32>().ok()).unwrap_or(8.);
        if self.started.elapsed().as_secs_f32()>=capture_at && !self.requested {
            self.requested=true;ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        assert!(self.saved || self.started.elapsed().as_secs_f32()<capture_at+82.,"client screenshot timed out");
        ctx.request_repaint();
    }
}
fn main()->eframe::Result {
    std::env::var("QA_CAPTURE_PATH").expect("set QA_CAPTURE_PATH to a PNG output path");
    if std::env::var_os("QA_BROWSER").is_some() { std::env::set_var("QA_PAGE", "multi"); }
    env_logger::init();
    // Downloads from servers, as the game keeps them (PEAKRUNNER_CONFIG_DIR
    // puts them beside a test's own settings).
    if let Some(dir) = peakrunner::preferences::download_dir() { peakrunner_core::catalog::set_cache_dir(dir); }
    if let Err(e) = peakrunner_core::mods::activate_home(&peakrunner_core::mods::from_env()) { eprintln!("PeakRunner: {e}; playing without mods"); }
    let _local = if std::env::var_os("QA_LOCAL").is_some() {
        let map = std::env::var("QA_MAP").unwrap_or_else(|_| "old-holler".into());
        // QA_CROWD=N: N idle network players join too (a full-roster scoreboard).
        let crowd: usize = std::env::var("QA_CROWD").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let capacity = if crowd > 0 || std::env::var_os("QA_BOTS").is_some() { 16 } else { 8 };
        let mut host = peakrunner_server::GameHost::bind("127.0.0.1:0", "HUD QA", capacity, &map).unwrap();
        // QA_MODE=football: the local server runs Football on QA_MAP (a map
        // with a declared field, or a preview pack through PEAKRUNNER_MAP_PACK).
        if std::env::var("QA_MODE").is_ok_and(|m| m == "football") {
            host = host.with_rotation(&format!(r#"[{{"map":"{}","mode":"football"}}]"#, map.to_lowercase())).unwrap();
        }
        // QA_MODE=cnh_server: the local server runs Capture & Hold on QA_MAP.
        if std::env::var("QA_MODE").is_ok_and(|m| m == "cnh_server") {
            host = host.with_rotation(&format!(r#"[{{"map":"{}","mode":"capture_and_hold"}}]"#, map.to_lowercase())).unwrap();
        }
        // QA_MODE=team_deathmatch, rabbit or team_rabbit: the local server runs it on QA_MAP.
        if let Ok(m) = std::env::var("QA_MODE").map(|m| m.to_lowercase()) {
            if ["team_deathmatch", "rabbit", "team_rabbit"].contains(&m.as_str()) {
                host = host.with_rotation(&format!(r#"[{{"map":"{}","mode":"{m}"}}]"#, map.to_lowercase())).unwrap();
            }
        }
        // QA_VOTE_ROTATION='[...]': the local server's rotation (for vote screens);
        // QA_VOTE=1: the first crowd player calls a vote for its second entry.
        if let Ok(rotation) = std::env::var("QA_VOTE_ROTATION") { host = host.with_rotation(&rotation).unwrap(); }
        // QA_START=x,y,z,yaw: everyone who joins stands there first, a step
        // apart (inside Tenfold Tower, for blade captures).
        if let Some(v) = std::env::var("QA_START").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 4) {
            host = host.with_start([v[0], v[1], v[2]], v[3]);
        }
        // QA_BOTS=N: a bot server (the local player takes a bot's place).
        if let Some(n) = std::env::var("QA_BOTS").ok().and_then(|v| v.parse::<usize>().ok()) { host = host.with_bots(n); }
        std::env::set_var("PEAKRUNNER_JOIN", host.local_addr().to_string());
        let port = host.local_addr().port();
        let handle = host.spawn();
        if crowd > 0 {
            std::thread::spawn(move || {
                let names = ["Ash", "Birch", "Cedar", "Dune", "Elm", "Fern", "Gale", "Hazel", "Iris", "Juniper", "Kestrel", "Larch",
                    "Maple", "Nettle", "Oak", "Pine", "Quill", "Rowan", "Sage", "Thorn", "Umber", "Vale", "Willow", "Yarrow", "Zephyr",
                    "Alder", "Brook"];
                let clients: Vec<_> = (0..crowd.min(27)).filter_map(|i| peakrunner_net::connect("127.0.0.1", port, names[i]).ok()).collect();
                let vote = std::env::var_os("QA_VOTE").is_some();
                // QA_DUELS=1 (a duel server): the crowd pairs off into duels.
                let duels = std::env::var_os("QA_DUELS").is_some();
                for seq in 1u64.. {
                    if duels && (seq == 120 || seq == 150) {
                        for pair in clients.chunks(2).filter(|p| p.len() == 2) {
                            let (a, b) = (&pair[0], &pair[1]);
                            if seq == 120 { a.challenge(b.lobby().player_id); } else { b.answer_challenge(a.lobby().player_id, true); }
                        }
                    }
                    if vote && seq == 120 { if let Some(c) = clients.first() { c.call_vote(1); } }
                    if vote && seq == 180 { for c in clients.iter().skip(1).take(2) { c.vote(true); } if let Some(c) = clients.get(3) { c.vote(false); } }
                    for c in &clients { c.send_input(peakrunner_core::sim::Command { seq, ..Default::default() }); }
                    std::thread::sleep(std::time::Duration::from_millis(16));
                }
            });
        }
        Some(handle)
    } else { None };
    eframe::run_native("PeakRunner platform check",eframe::NativeOptions {
        // QA_ON_TOP=1: float over other windows (an unfocused QA_DRIVE window
        // that others cover isn't drawn, so it never captures).
        viewport:egui::ViewportBuilder::default().with_inner_size([1280.,800.]).with_active(std::env::var_os("QA_DRIVE").is_none())
            .with_window_level(if std::env::var_os("QA_ON_TOP").is_some() { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal }),
        renderer:eframe::Renderer::Wgpu,..Default::default()
    },Box::new(|cc|Ok(Box::new(Probe {app:peakrunner::PeakRunnerApp::new(cc).map_err(std::io::Error::other)?,started:{
        // The clock QA_CAPTURE_AT, QA_FRAMES_FROM and the like count from.
        if std::env::var_os("QA_FRAMES").is_some() { eprintln!("QA_FRAMES: the clock starts at {} ms", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())); }
        Instant::now()},requested:false,saved:false,paused:false,clicked:0,frames:0,writer:None}))))
}
