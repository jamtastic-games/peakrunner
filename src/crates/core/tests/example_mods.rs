//! Every example mod shipped in server packages (`examples/mods/`) loads as
//! the engine reads mods. Motor Pool once asked for engine API 5 after the
//! API went back to 1 (`mods::API`), so no server could run it.
#[test]
fn every_example_mod_loads() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mods");
    let mut bad = Vec::new();
    for e in std::fs::read_dir(&examples).unwrap().filter_map(Result::ok) {
        if !e.path().is_dir() { continue; }
        if let Err(why) = peakrunner_core::mods::load(&e.path()) { bad.push(format!("{}: {why}", e.file_name().to_string_lossy())); }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
