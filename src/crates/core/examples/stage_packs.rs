//! Stage map folders for an install (client, server, launcher feed, image):
//!
//!   cargo run --release -p peakrunner-core --example stage_packs -- <maps source> <packs destination>
//!
//! Every map folder in the source (one holding a `map.json`) is copied to
//! `<destination>/maps/<key>/`: `map.json` and `info.json` as they are, and
//! each payload the manifest lists zlib-compressed as `<name>.zlib` (hashes
//! stay over the uncompressed bytes, so fingerprints are unchanged). Each
//! staged map is then loaded to prove it validates with the same fingerprint.
//! Refuses to overwrite an existing destination. `scripts/stage-packs.sh`
//! runs it for the default maps.
use peakrunner_core::map_pack::MapPack;
use std::path::{Path, PathBuf};

fn stage(source: &Path, dest: &Path) -> Result<Vec<String>, String> {
    let mut staged = Vec::new();
    let mut folders: Vec<PathBuf> = std::fs::read_dir(source).map_err(|e| format!("{}: {e}", source.display()))?
        .filter_map(Result::ok).map(|e| e.path()).filter(|p| p.join("map.json").is_file()).collect();
    folders.sort();
    for dir in folders {
        let key = dir.file_name().and_then(|n| n.to_str()).ok_or("map folder name is not text")?.to_string();
        if !peakrunner_core::catalog::valid_key(&key) { return Err(format!("{key:?} isn't a map key (a-z, 0-9, -)")); }
        let original = MapPack::load(&dir).map_err(|e| format!("{key}: {e}"))?;
        let out = dest.join("maps").join(&key);
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        std::fs::copy(dir.join("map.json"), out.join("map.json")).map_err(|e| e.to_string())?;
        if dir.join("info.json").is_file() {
            std::fs::copy(dir.join("info.json"), out.join("info.json")).map_err(|e| e.to_string())?;
        }
        for name in original.manifest.files.keys() {
            let raw = std::fs::read(dir.join(name)).map_err(|e| format!("{key}/{name}: {e}"))?;
            let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9);
            std::fs::write(out.join(format!("{name}.zlib")), packed).map_err(|e| e.to_string())?;
        }
        let copy = MapPack::load(&out).map_err(|e| format!("staged {key} failed validation: {e}"))?;
        if copy.fingerprint != original.fingerprint { return Err(format!("staged {key} has a different fingerprint")); }
        staged.push(format!("{key} {}", copy.fingerprint));
    }
    if staged.is_empty() { return Err(format!("no map folders in {}", source.display())); }
    Ok(staged)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source, dest] = args.as_slice() else {
        eprintln!("usage: stage_packs <maps source> <packs destination>");
        std::process::exit(2);
    };
    let dest = Path::new(dest);
    if dest.exists() {
        eprintln!("refusing to overwrite {}", dest.display());
        std::process::exit(1);
    }
    match stage(Path::new(source), dest) {
        Ok(staged) => for line in staged { println!("{line}"); },
        Err(e) => {
            eprintln!("stage_packs: {e}");
            let _ = std::fs::remove_dir_all(dest);
            std::process::exit(1);
        }
    }
}
