//! Lava ground (`lava.bin`, docs/map-pipeline.md): a copy of a shipped map
//! with a patch of skim lava and a patch of lava lying on the ground. Both
//! burn at their own rates, the skim patch holds players up, the other
//! doesn't, and a shot bursts on the skim patch and splashes it.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{Disc, Team, World, LAVA_SPLASH, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::{catalog, registry};
use sha2::{Digest, Sha256};

/// Cells (x0, z0) to (x0 + 5, z0 + 5) of each patch.
const SKIM: (usize, usize) = (120, 95);
const FLOW: (usize, usize) = (132, 95);

struct Fixture { map: MapId, step: f32, skim: f32 }

/// Old Holler, copied with lava ground added: the skim patch 0.5 m over
/// its highest ground, burning 150 a second; the other on the ground of
/// each cell, burning 60.
fn fixture() -> Fixture {
    let source = catalog::entry(MapId::OldHoller).pack().expect("Old Holler installed").root.clone();
    let dir = std::env::temp_dir().join(format!("peakrunner-lava-ground-{}", std::process::id())).join("maps/lava-holler");
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(&source).unwrap().filter_map(Result::ok) {
        let name = f.file_name();
        if name == "map.json" || name == "info.json" { continue; }
        let to = dir.join(&name);
        if std::fs::hard_link(f.path(), &to).is_err() { std::fs::copy(f.path(), &to).unwrap(); }
    }
    std::fs::write(dir.join("info.json"), r#"{"name":"Lava Holler"}"#).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(source.join("map.json")).unwrap()).unwrap();
    let step = manifest["terrain_step"].as_f64().unwrap_or(8.0) as f32;
    let ground = |x: usize, z: usize| peakrunner_core::terrain::height_on(MapId::OldHoller, (x as f32 + 0.5) * step, (z as f32 + 0.5) * step);
    let patch = |(x0, z0): (usize, usize)| (z0..z0 + 6).flat_map(move |z| (x0..x0 + 6).map(move |x| (x, z)));
    let skim = patch(SKIM).map(|(x, z)| ground(x, z)).fold(f32::MIN, f32::max) + 0.5;
    let mut lava = vec![0u8; 256 * 256 * 4];
    let mut put = |x: usize, z: usize, surface: f32, burn: u8, flags: u8| {
        let h = (surface * 32.0).round() as u16;
        lava[(z * 256 + x) * 4..(z * 256 + x) * 4 + 4].copy_from_slice(&[h as u8, (h >> 8) as u8, burn, flags]);
    };
    for (x, z) in patch(SKIM) { put(x, z, skim, 150, 1); }
    for (x, z) in patch(FLOW) { put(x, z, ground(x, z) + 0.05, 60, 0); }
    std::fs::write(dir.join("lava.bin"), &lava).unwrap();
    manifest["files"]["lava.bin"] = format!("{:x}", Sha256::digest(&lava)).into();
    std::fs::write(dir.join("map.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    Fixture { map: catalog::add_folder("lava-holler", dir).expect("the copy loads"), step, skim }
}

fn world(map: MapId) -> World {
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.player_id = 0;
    let p = &mut w.players[0];
    p.team = Team::Ember; p.is_bot = false; p.remote = false; p.alive = true; p.health = 100.0; p.vel = Vec3::ZERO;
    w
}

/// Health lost standing `seconds` at the middle of cell (x, z), and where
/// the player ended up.
fn stand(f: &Fixture, x: usize, z: usize, seconds: f32) -> (f32, Vec3) {
    let mut w = world(f.map);
    let (wx, wz) = ((x as f32 + 0.5) * f.step, (z as f32 + 0.5) * f.step);
    let floor = peakrunner_core::terrain::support_on(f.map, Vec3::new(wx, 2000.0, wz)).0;
    w.players[0].pos = Vec3::new(wx, floor + PLAYER_RADIUS, wz);
    for _ in 0..(seconds / STEP).round() as usize { w.tick(STEP); }
    (100.0 - w.players[0].health, w.players[0].pos)
}

#[test]
fn lava_ground_burns_holds_and_splashes() {
    let f = fixture();
    let pack = catalog::entry(f.map).pack().unwrap();
    assert!(pack.has_lava_ground());

    // Skim lava: stood on, never sunk into, 150 a second.
    let (hurt, at) = stand(&f, SKIM.0 + 3, SKIM.1 + 3, 0.5);
    assert!(at.y - PLAYER_RADIUS > f.skim - 0.05, "standing on it at {}, the lava at {}", at.y - PLAYER_RADIUS, f.skim);
    assert!((60.0..90.0).contains(&hurt), "half a second on it burns about 75 ({hurt:.1})");

    // Lava on the ground: walked on, 60 a second.
    let (hurt, at) = stand(&f, FLOW.0 + 3, FLOW.1 + 3, 1.0);
    let ground = peakrunner_core::terrain::height_on(f.map, at.x, at.z);
    assert!((at.y - PLAYER_RADIUS - ground).abs() < 0.2, "on the ground under it");
    assert!((50.0..70.0).contains(&hurt), "a second on it burns about 60 ({hurt:.1})");

    // The ground between the patches doesn't.
    let (hurt, _) = stand(&f, SKIM.0 + 9, SKIM.1 + 3, 1.0);
    assert!(hurt < 0.5, "dry ground ({hurt:.1})");

    // A disc fired down bursts on the skim lava and splashes it.
    let mut w = world(f.map);
    w.players[0].pos = Vec3::new(500.0, 400.0, 500.0);
    let disc = registry::registry().projectiles.index("disc").unwrap() as u8;
    let splash = registry::registry().projectiles.index(LAVA_SPLASH).unwrap() as u8;
    let (wx, wz) = ((SKIM.0 as f32 + 3.0) * f.step, (SKIM.1 as f32 + 3.0) * f.step);
    w.discs.push(Disc { pos: Vec3::new(wx, f.skim + 6.0, wz), vel: Vec3::new(0.0, -80.0, 0.0), team: Team::Ember,
        owner: 0, life: 5.0, kind: disc, spin: 0.0, target: 0, power: 1.0 });
    for _ in 0..30 { w.tick(STEP); if !w.explosions.is_empty() { break; } }
    let burst = w.explosions.iter().find(|e| e.kind == disc).expect("the burst");
    assert!((burst.pos.y - f.skim).abs() < 0.5, "on the lava at {} ({})", burst.pos.y, f.skim);
    assert!(w.explosions.iter().any(|e| e.kind == splash), "a splash");
}
