//! Looks, sounds and bots for a mod (docs/mod-format.md): parts are checked,
//! a mod's sounds are small WAVE files players get by name, and bots follow
//! a mod's hints: the armor for their role, a pack switched on in a fight.
//! Its own test binary: activating a mod changes the process's definitions.
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{packs::PackKind, ArmorClass, World, STEP};
use peakrunner_core::terrain::MapId;
use peakrunner_core::{catalog, mods, registry};

fn write(root: &std::path::Path, path: &str, bytes: &[u8]) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A tiny mono WAVE file.
fn wav() -> Vec<u8> {
    let samples: Vec<i16> = (0..2000).map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16).collect();
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + data).to_le_bytes()); b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&22_050u32.to_le_bytes()); b.extend_from_slice(&44_100u32.to_le_bytes()); b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes()); b.extend_from_slice(b"data"); b.extend_from_slice(&data.to_le_bytes());
    for s in samples { b.extend_from_slice(&s.to_le_bytes()); }
    b
}

#[test]
fn parts_sounds_and_bot_hints() {
    let root = std::env::temp_dir().join(format!("peakrunner-looks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/dressed";
    write(&root, &format!("{m}/mod.json"), br#"{"name": "Dressed", "note": "Looks.", "api": 1}"#);
    write(&root, &format!("{m}/weapons/look.json"), br#"{ "disc": { "sound": "zap",
        "parts": [{ "mesh": "bevel", "size": [0.1, 0.1, 0.5], "color": [0.8, 0.2, 0.2] }, { "mesh": "sphere", "at": [0, 0.1, -0.3], "size": [0.05, 0.05, 0.05], "glow": 2, "team": true }] } }"#);
    write(&root, &format!("{m}/armors/guard.json"), br#"{ "guard": { "extends": "heavy", "name": "Guard", "scale": 1.15, "tint": [0.3, 0.5, 0.3],
        "bot_roles": ["defense", "offense", "hunter"] } }"#);
    write(&root, &format!("{m}/packs/hint.json"), br#"{ "shield": { "offered": true, "bot": "combat" } }"#);
    write(&root, &format!("{m}/effects/glow.json"), br#"{ "shielded": { "glow": [0.3, 0.6, 1.0, 0.2] } }"#);
    write(&root, &format!("{m}/sounds/zap.wav"), &wav());
    // Bad parts and sounds are refused, naming the problem (one root, and
    // one test: the mods in force are the process's).
    let base = |m: &str| {
        write(&root, &format!("mods/{m}/mod.json"), br#"{"name": "Bad", "note": "Bad.", "api": 1}"#);
    };
    base("bad-mesh");
    write(&root, "mods/bad-mesh/weapons/x.json", br#"{ "disc": { "parts": [{ "mesh": "cone", "size": [1, 1, 1] }] } }"#);
    base("too-many");
    let many: Vec<String> = (0..25).map(|_| r#"{ "mesh": "cube", "size": [1, 1, 1] }"#.to_string()).collect();
    write(&root, "mods/too-many/weapons/x.json", format!(r#"{{ "disc": {{ "parts": [{}] }} }}"#, many.join(",")).as_bytes());
    base("bad-sound");
    write(&root, "mods/bad-sound/sounds/hiss.wav", b"not a wave file at all, not even close to one");
    catalog::add_root(root.clone());
    mods::activate_home(&["dressed".to_string()]).unwrap();
    let r = registry::registry();
    assert_eq!(r.weapons.by_id("disc").unwrap().parts.len(), 2);
    assert_eq!(mods::sounds().iter().map(|(n, b)| (n.as_str(), b.len() > 44)).collect::<Vec<_>>(), vec![("zap", true)]);

    // Bots wear the armor their role is given, and switch their shield pack
    // on in a fight.
    let mut w = World::new();
    w.set_map(MapId::OldHoller);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    let guard = ArmorClass::by_id("guard").unwrap();
    let bots: Vec<usize> = (0..w.players.len()).filter(|&i| w.players[i].is_bot).collect();
    assert!(!bots.is_empty());
    for &b in &bots { w.players[b].respawn = 0.0; w.players[b].alive = false; }
    w.tick(STEP);
    assert!(bots.iter().all(|&b| w.players[b].armor == guard), "every role wears it");
    let shield = PackKind::from_key("shield").unwrap();
    // One bot, and an enemy in plain sight ahead of it.
    let a = bots[0];
    let foe = *bots.iter().find(|&&b| w.players[b].team != w.players[a].team).expect("bots on both teams");
    let ground = |x: f32, z: f32| glam::Vec3::new(x, peakrunner_core::terrain::support_on(MapId::OldHoller, glam::Vec3::new(x, 400.0, z)).0 + 0.6, z);
    w.players[a].pos = ground(1060.0, 700.0);
    w.players[foe].pos = ground(1060.0, 685.0);
    w.players[a].pack = Some(shield);
    w.players[a].energy = 100.0;
    let mut on = false;
    for _ in 0..(2.0 / STEP) as usize {
        w.tick(STEP);
        if w.players[a].pack_on { on = true; break; }
    }
    assert!(on, "a bot in a fight switched its shield on");

    // The bad ones are refused, naming the problem.
    for (id, why) in [("bad-mesh", "cone"), ("too-many", "at most 24"), ("bad-sound", "WAVE")] {
        let e = mods::activate_home(&[id.to_string()]).unwrap_err();
        assert!(e.contains(why), "{id}: {e}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
