//! Joining a server that needs maps or mods this game lacks (docs/modding.md,
//! P2 and P4): the server's status lists them (`MatchStatus::content`), the
//! player approves the download, and each is fetched into the per-user cache
//! (`catalog::cache_dir`), checked exactly as the game loads it, and only
//! then registered. Nothing but data is ever accepted: map payloads and the
//! JSON files a mod may hold.
use peakrunner_core::{catalog, map_pack::MapPack, mods};
use peakrunner_discovery::{ContentItem, MAX_CONTENT};
use peakrunner_protocol::{ClientMsg, ServerMsg};
use peakrunner_discovery::quic::{read_control_limited, send_control};
use sha2::{Digest, Sha256};
use std::{io, path::Path, time::Duration};
use tokio::time::timeout;

/// Payload names a map may list (`map_pack`); anything else is refused.
const PAYLOADS: [&str; 10] = ["vertices.bin", "collision.bin", "height.bin", "weights.rgba", "textures.rgba", "ambient.f32", "shade.rg", "props.bin", "models.glb", "lava.bin"];
/// Largest single file accepted, and largest total one server may ask for.
const FILE_LIMIT: u64 = 128_000_000;
pub const SERVER_LIMIT: u64 = 1 << 30;
/// Default size of the download cache before the oldest maps are dropped.
pub const CACHE_LIMIT: u64 = 2 << 30;
/// Most downloads of each kind the cache keeps, well under what the engine
/// can register (`catalog`: 1,024 maps; `mods`: 256), so a cache filled with
/// tiny downloads can't leave a game unable to take another.
const CACHE_ENTRIES: [(&str, &str, usize); 2] = [("maps", "map", 400), ("mods", "mod", 100)];
/// Largest `map.json` a map may have (`map_pack`), and `info.json` the game
/// reads (`catalog`). An `info.json` may run up to the latter past what the
/// server listed: one map listed under two keys is served the first one's.
const MANIFEST_LIMIT: u64 = 8_000_000;
const INFO_LIMIT: u64 = 16 * 1024;

fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.to_string()) }

/// The bytes `items` say they hold, all told (saturating: the sizes are
/// the server's word, and a sum that wrapped would pass every limit).
pub(crate) fn total(items: &[ContentItem]) -> u64 { items.iter().fold(0, |sum, i| sum.saturating_add(i.bytes)) }

/// A server's content list, refused unless every entry is well formed.
pub(crate) fn check(items: &[ContentItem]) -> io::Result<()> {
    if items.len() > MAX_CONTENT { return Err(invalid("the server lists too many maps")); }
    let hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    for (n, item) in items.iter().enumerate() {
        if !["map", "mod"].contains(&item.kind.as_str()) || !catalog::valid_key(&item.key) || !hex(&item.fingerprint)
            || item.name.chars().count() > 48 || item.name.chars().any(char::is_control)
            // Each map or mod once (a repeat would download and register
            // again); one map may come under two keys.
            || items[..n].iter().any(|i| i.kind == item.kind && i.key == item.key) {
            return Err(invalid("the server's map list is malformed"));
        }
    }
    if total(items) > SERVER_LIMIT { return Err(invalid("the server's maps are larger than 1 GB")); }
    Ok(())
}

/// What joining a server takes, for the server list: its mods by name, and
/// the maps and mods this game would download first (none if it holds all).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer { pub mods: Vec<String>, pub missing: Vec<(String, u64)>, pub download: u64 }

/// Compare a server's content list with what this game holds, without
/// binding anything.
pub(crate) fn offer(items: &[ContentItem]) -> Offer {
    let held = |i: &ContentItem| if i.kind == "mod" { mods::by_fingerprint(&i.key, &i.fingerprint).is_some() }
        else { catalog::by_fingerprint(&i.key, &i.fingerprint).is_some() };
    let missing: Vec<(String, u64)> = items.iter().filter(|i| !held(i)).map(|i| (i.name.clone(), i.bytes)).collect();
    Offer {
        mods: items.iter().filter(|i| i.kind == "mod").map(|i| i.name.clone()).collect(),
        download: missing.iter().fold(0, |sum, m| sum.saturating_add(m.1)),
        missing,
    }
}

/// Why a server's content can't be bound yet.
pub(crate) enum Unresolved {
    /// Maps or mods this game doesn't hold: they can be downloaded.
    Missing(Vec<ContentItem>),
    /// A mod that fails its checks (a definition out of range, a bad mode).
    Invalid(String),
}

/// Bind the server's maps and put its mods in force for this session; the
/// digest to say hello with.
pub(crate) fn resolve(items: &[ContentItem]) -> Result<String, Unresolved> {
    let maps: Vec<&ContentItem> = items.iter().filter(|i| i.kind == "map").collect();
    let pairs: Vec<(&str, &str)> = maps.iter().map(|i| (i.key.as_str(), i.fingerprint.as_str())).collect();
    let mut missing: Vec<ContentItem> = match catalog::bind_session(&pairs) {
        Ok(()) => Vec::new(),
        Err(gone) => gone.into_iter().map(|i| maps[i].clone()).collect(),
    };
    let mut session = Vec::new();
    for item in items.iter().filter(|i| i.kind == "mod") {
        match mods::by_fingerprint(&item.key, &item.fingerprint) {
            Some(m) => session.push(m),
            None => missing.push(item.clone()),
        }
    }
    if !missing.is_empty() { return Err(Unresolved::Missing(missing)); }
    mods::activate(&session).map_err(Unresolved::Invalid)?;
    Ok(peakrunner_protocol::content_digest(items))
}

/// One file of a listed map: its bytes as the server stores them and whether
/// they are zlib-compressed; `None` if the server has no such file. Refused
/// before reading if it's larger than `most` bytes (what's left of the
/// size the server listed).
async fn fetch(conn: &quinn::Connection, map: &str, file: &str, most: u64) -> io::Result<Option<(Vec<u8>, bool)>> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(io::Error::other)?;
    send_control(&mut send, &ClientMsg::Fetch { map: map.into(), file: file.into() }).await?;
    let _ = send.finish();
    let header = timeout(Duration::from_secs(10), read_control_limited::<ServerMsg>(&mut recv, 1024)).await.map_err(io::Error::other)??;
    match header {
        ServerMsg::File { bytes, zlib } => {
            if bytes > FILE_LIMIT { return Err(invalid("a map file is larger than 128 MB")); }
            if bytes > most { return Err(invalid("the server is sending more than it listed")); }
            let mut data = vec![0; bytes as usize];
            timeout(Duration::from_secs(300), recv.read_exact(&mut data)).await.map_err(io::Error::other)?.map_err(io::Error::other)?;
            Ok(Some((data, zlib)))
        }
        ServerMsg::Reject { .. } => Ok(None),
        _ => Err(invalid("unexpected reply to a file request")),
    }
}

/// Download `items` from the server at `address` into the cache, reporting
/// (bytes so far, total, map name) as it goes. Each map is checked in full
/// and registered only once complete; a partial download is removed. No
/// map in `keep` (all the server needs) is trimmed from the cache after.
pub(crate) async fn download(address: &str, items: &[ContentItem], keep: &[ContentItem], progress: &(dyn Fn(u64, u64, &str) + Sync)) -> io::Result<()> {
    let cache = catalog::cache_dir().ok_or_else(|| invalid("this program has no download folder"))?.to_path_buf();
    let total = total(items);
    let (endpoint, conn) = peakrunner_discovery::quic::connect_for_download(address).await?;
    let mut done = 0u64;
    let result = async {
        for item in items {
            let partial = cache.join("maps").join(".partial").join(&item.fingerprint);
            let _ = std::fs::remove_dir_all(&partial);
            std::fs::create_dir_all(&partial)?;
            let fetched = if item.kind == "mod" { fetch_mod(&conn, item, &partial, &mut done, total, progress).await }
                else { fetch_map(&conn, item, &partial, &mut done, total, progress).await };
            if let Err(e) = fetched { let _ = std::fs::remove_dir_all(&partial); return Err(e); }
            let home = cache.join(if item.kind == "mod" { "mods" } else { "maps" }).join(&item.key).join(&item.fingerprint);
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(home.parent().unwrap())?;
            std::fs::rename(&partial, &home)?;
            if item.kind == "mod" { mods::add_cached(&item.key, home).map_err(|e| invalid(&e))?; }
            else { catalog::add_cached(&item.key, home).map_err(|e| invalid(&e))?; }
        }
        Ok(())
    }.await;
    conn.close(0u32.into(), b"downloaded");
    endpoint.wait_idle().await;
    trim(&cache, keep, CACHE_LIMIT);
    result
}

async fn fetch_map(conn: &quinn::Connection, item: &ContentItem, dir: &Path, done: &mut u64, total: u64,
                   progress: &(dyn Fn(u64, u64, &str) + Sync)) -> io::Result<()> {
    let name = item.name.as_str();
    // No more than the server listed (map.json, info.json and the payloads).
    let mut left = item.bytes;
    let (json, zlib) = fetch(conn, &item.fingerprint, "map.json", left.min(MANIFEST_LIMIT)).await?.ok_or_else(|| invalid("the server has no map.json for a map it lists"))?;
    if zlib || format!("{:x}", Sha256::digest(&json)) != item.fingerprint {
        return Err(invalid(&format!("{name}: the map the server sent isn't the one it listed")));
    }
    std::fs::write(dir.join("map.json"), &json)?;
    left -= json.len() as u64;
    *done += json.len() as u64;
    let manifest: serde_json::Value = serde_json::from_slice(&json).map_err(|_| invalid("map.json isn't JSON"))?;
    let files: Vec<String> = manifest.get("files").and_then(|f| f.as_object()).map(|f| f.keys().cloned().collect())
        .ok_or_else(|| invalid("map.json lists no files"))?;
    if files.iter().any(|f| !PAYLOADS.contains(&f.as_str())) { return Err(invalid(&format!("{name} lists a file PeakRunner doesn't use"))); }
    for file in files.iter().map(String::as_str).chain(["info.json"]) {
        let info = file == "info.json";
        let Some((data, zlib)) = fetch(conn, &item.fingerprint, file, if info { left.max(INFO_LIMIT) } else { left }).await? else {
            if info { continue; }
            return Err(invalid(&format!("{name}: the server is missing {file}")));
        };
        left = left.saturating_sub(data.len() as u64);
        if info && (zlib || data.len() as u64 > INFO_LIMIT) { continue; }
        std::fs::write(dir.join(if zlib { format!("{file}.zlib") } else { file.to_string() }), &data)?;
        *done += data.len() as u64;
        progress(*done, total, name);
    }
    let pack = MapPack::load(dir).map_err(|e| invalid(&format!("{name} failed its checks: {e}")))?;
    if pack.fingerprint != item.fingerprint { return Err(invalid(&format!("{name}: fingerprint mismatch"))); }
    Ok(())
}

/// A mod: its listing (which must hash to the fingerprint), then each file
/// it names, every one checked against its SHA-256; then the whole mod is
/// read as the game reads mods.
async fn fetch_mod(conn: &quinn::Connection, item: &ContentItem, dir: &Path, done: &mut u64, total: u64,
                   progress: &(dyn Fn(u64, u64, &str) + Sync)) -> io::Result<()> {
    let name = item.name.as_str();
    // The listing aside, no more than the server listed.
    let mut left = item.bytes;
    let (listing, zlib) = fetch(conn, &item.fingerprint, "listing.txt", 64 * 1024).await?.ok_or_else(|| invalid("the server has no listing for a mod it lists"))?;
    if zlib || listing.len() > 64 * 1024 || format!("{:x}", Sha256::digest(&listing)) != item.fingerprint {
        return Err(invalid(&format!("{name}: the mod the server sent isn't the one it listed")));
    }
    let text = String::from_utf8(listing).map_err(|_| invalid("a mod listing isn't text"))?;
    for line in text.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(sha), Some(_size), Some(path)) = (parts.next(), parts.next(), parts.next()) else { return Err(invalid("a mod listing is malformed")) };
        // Exactly what a mod may hold: its definitions, modes, classes and
        // sounds (class files used to be refused, so no mod with an engine
        // class could be downloaded).
        if !mods::allowed(path) { return Err(invalid(&format!("{name} lists a file a mod can't hold: {path}"))); }
        let (data, zlib) = fetch(conn, &item.fingerprint, path, left.min(64 * 1024)).await?.ok_or_else(|| invalid(&format!("{name}: the server is missing {path}")))?;
        left -= data.len() as u64;
        if zlib || data.len() > 64 * 1024 || format!("{:x}", Sha256::digest(&data)) != sha {
            return Err(invalid(&format!("{name}: {path} doesn't match its listing")));
        }
        let to = dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap())?;
        std::fs::write(to, &data)?;
        *done += data.len() as u64;
        progress(*done, total, name);
    }
    check_mod(dir, item)
}

/// A fetched mod, read as the game reads mods, whose files make the very
/// listing it came by: one out of order or with a wrong size hashes to the
/// fingerprint, yet names a mod the game never finds again (each try took
/// another of the slots mods are registered in).
fn check_mod(dir: &Path, item: &ContentItem) -> io::Result<()> {
    let name = item.name.as_str();
    mods::load(dir).map_err(|e| invalid(&format!("{name} failed its checks: {e}")))?;
    if mods::fingerprint_of(dir).map_err(|e| invalid(&e))? != item.fingerprint { return Err(invalid(&format!("{name}: fingerprint mismatch"))); }
    Ok(())
}

/// The bytes under `dir`, its folders' too (a mod has some).
fn size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).filter_map(|e| e.metadata().ok().map(|m| (e.path(), m)))
        .fold(0, |sum, (path, m)| sum.saturating_add(if m.is_dir() { size(&path) } else { m.len() }))
}

/// Keep the cache under `limit` bytes and CACHE_ENTRIES downloads of each
/// kind: drop the oldest maps and mods first, never one this server needs
/// (`keep`, its whole list, each by kind, key and fingerprint).
fn trim(cache: &Path, keep: &[ContentItem], limit: u64) {
    // (downloaded, bytes, folder, kind)
    let mut versions: Vec<(std::time::SystemTime, u64, std::path::PathBuf, usize)> = Vec::new();
    let mut kept = 0u64;
    for (k, (folder, kind, _)) in CACHE_ENTRIES.iter().enumerate() {
        for key in std::fs::read_dir(cache.join(folder)).into_iter().flatten().filter_map(Result::ok) {
            if key.file_name() == ".partial" { continue; }
            let key_name = key.file_name().to_string_lossy().to_string();
            for version in std::fs::read_dir(key.path()).into_iter().flatten().filter_map(Result::ok) {
                let fp = version.file_name().to_string_lossy().to_string();
                // What's kept counts as it lies on disk (a map this game ships
                // isn't in the cache at all). A copy under a key the server
                // doesn't list is fair game, whatever its fingerprint.
                if keep.iter().any(|i| i.kind == *kind && i.key == key_name && i.fingerprint == fp) { kept = kept.saturating_add(size(&version.path())); continue; }
                let used = version.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                versions.push((used, size(&version.path()), version.path(), k));
            }
        }
    }
    let mut total = versions.iter().fold(kept, |sum, v| sum.saturating_add(v.1));
    let mut count = [0usize; 2];
    for v in &versions { count[v.3] += 1; }
    versions.sort();
    for (_, bytes, path, k) in versions {
        if total <= limit && count[k] <= CACHE_ENTRIES[k].2 { continue; }
        if std::fs::remove_dir_all(&path).is_ok() { total = total.saturating_sub(bytes); count[k] -= 1; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offer_names_the_mods_and_only_what_this_game_lacks() {
        let held = catalog::entry(peakrunner_core::terrain::MapId::Highgoal);
        let item = |kind: &str, key: &str, name: &str, fingerprint: &str, bytes| ContentItem {
            kind: kind.into(), key: key.into(), name: name.into(), fingerprint: fingerprint.into(), bytes };
        let items = [
            item("map", "highgoal", "Highgoal", held.fingerprint().unwrap(), 9_000_000),
            item("map", "river-fort", "River Fort", &"ab".repeat(32), 24_000_000),
            item("mod", "quick-ctf", "Quick CTF", &"cd".repeat(32), 360),
        ];
        assert_eq!(offer(&items), Offer {
            mods: vec!["Quick CTF".into()],
            missing: vec![("River Fort".into(), 24_000_000), ("Quick CTF".into(), 360)],
            download: 24_000_360,
        });
        assert_eq!(offer(&items[..1]), Offer::default(), "a server of maps this game holds needs nothing");
    }

    fn item(kind: &str, key: &str, fingerprint: &str, bytes: u64) -> ContentItem {
        ContentItem { kind: kind.into(), key: key.into(), name: key.into(), fingerprint: fingerprint.into(), bytes }
    }

    /// Sizes are the server's word: two that wrapped past the largest number
    /// summed to a few bytes, under the 1 GB limit and the download question.
    /// And each map or mod is listed once (a repeat downloaded and registered
    /// again).
    #[test]
    fn a_content_list_s_sizes_add_up_and_name_each_thing_once() {
        let huge = [item("map", "big-one", &"ab".repeat(32), u64::MAX), item("map", "big-two", &"cd".repeat(32), 2)];
        assert_eq!(total(&huge), u64::MAX);
        assert!(check(&huge).is_err());
        assert_eq!(offer(&huge).download, u64::MAX);
        assert!(check(&[item("map", "arena", &"ab".repeat(32), 10), item("mod", "arena", &"cd".repeat(32), 10)]).is_ok(), "a map and a mod may share a key");
        for twice in [
            [item("map", "arena", &"ab".repeat(32), 10), item("map", "arena", &"ab".repeat(32), 10)],
            [item("map", "arena", &"ab".repeat(32), 10), item("map", "arena", &"cd".repeat(32), 10)],
        ] { assert!(check(&twice).is_err()); }
        assert!(check(&[item("map", "arena", &"ab".repeat(32), 10), item("map", "copy", &"ab".repeat(32), 10)]).is_ok(), "one map under two keys");
    }

    /// The cache is trimmed around everything the server needs: a map it
    /// already held (not downloaded this time) used to be dropped like any.
    /// Only under the keys it lists: a copy under another key goes (else a
    /// server could pile up copies of one map under new keys for good).
    #[test]
    fn trimming_keeps_every_map_the_server_needs() {
        let cache = std::env::temp_dir().join(format!("peakrunner-trim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let (held, other, new) = ("ab".repeat(32), "cd".repeat(32), "ef".repeat(32));
        let mut when = std::time::SystemTime::now() - Duration::from_secs(3600);
        // Oldest first: a copy of the held map, the held map, another, then
        // this download.
        for (key, fp) in [("copy", &held), ("held", &held), ("other", &other), ("new", &new)] {
            let dir = cache.join("maps").join(key).join(fp);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("map.json"), vec![0u8; 100]).unwrap();
            std::fs::File::open(&dir).unwrap().set_modified(when).unwrap();
            when += Duration::from_secs(60);
        }
        let keep = [item("map", "held", &held, 100), item("map", "new", &new, 100), item("map", "old-holler", &"0f".repeat(32), 50_000_000)];
        let there = |key: &str, fp: &str| cache.join("maps").join(key).join(fp).exists();
        // A map this game ships isn't in the cache: it takes none of its room.
        trim(&cache, &keep, 1000);
        assert!(there("other", &other) && there("copy", &held));
        trim(&cache, &keep, 250);
        assert!(there("held", &held) && there("new", &new), "the server's maps stay");
        assert!(!there("other", &other) && !there("copy", &held), "the others go");
        let _ = std::fs::remove_dir_all(&cache);
    }

    /// A mod's listing must be the one its files make, not merely hash to
    /// the fingerprint: a size off by one was taken, registered, and never
    /// found again.
    #[test]
    fn a_mod_must_make_the_listing_it_came_by() {
        let dir = std::env::temp_dir().join(format!("peakrunner-listing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let json = br#"{"name": "Tiny", "api": 1}"#;
        std::fs::write(dir.join("mod.json"), json).unwrap();
        assert!(check_mod(&dir, &item("mod", "tiny", &mods::fingerprint_of(&dir).unwrap(), 0)).is_ok());
        let lying = format!("{:x} {} mod.json\n", Sha256::digest(json), json.len() + 1);
        assert!(check_mod(&dir, &item("mod", "tiny", &format!("{:x}", Sha256::digest(lying.as_bytes())), 0)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tiny downloads are kept only so many of each kind: mods were never
    /// trimmed, so a server listing many small ones could fill the slots the
    /// game registers its downloads in, for good (they're registered again
    /// at every start).
    #[test]
    fn the_cache_keeps_only_so_many_downloads_of_each_kind() {
        let cache = std::env::temp_dir().join(format!("peakrunner-count-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let most = CACHE_ENTRIES[1].2;
        let fp = |n: usize| format!("{n:064x}");
        let mut when = std::time::SystemTime::now() - Duration::from_secs(36_000);
        for n in 0..most + 3 {
            let dir = cache.join("mods").join(format!("tiny-{n}")).join(fp(n));
            std::fs::create_dir_all(dir.join("modes")).unwrap();
            std::fs::write(dir.join("mod.json"), b"{}").unwrap();
            std::fs::write(dir.join("modes").join("x.json"), vec![0u8; 50]).unwrap();
            std::fs::File::open(&dir).unwrap().set_modified(when).unwrap();
            when += Duration::from_secs(60);
        }
        assert_eq!(size(&cache.join("mods").join("tiny-0").join(fp(0))), 52, "subfolders count");
        // The oldest is the server's: it stays (beside the most kept), and
        // the next two go.
        trim(&cache, &[item("mod", "tiny-0", &fp(0), 52)], CACHE_LIMIT);
        let there = |n: usize| cache.join("mods").join(format!("tiny-{n}")).join(fp(n)).exists();
        assert!(there(0) && !there(1) && !there(2) && there(3) && there(most + 2));
        let _ = std::fs::remove_dir_all(&cache);
    }

    /// Every file an installed map lists can be downloaded (Reefbreak's
    /// models.glb among them): a new kind of payload needs adding here too.
    #[test]
    fn every_installed_maps_payloads_download() {
        let mut seen = 0;
        for id in catalog::maps() {
            let Some(pack) = catalog::entry(id).pack() else { continue };
            for name in pack.manifest.files.keys() {
                assert!(PAYLOADS.contains(&name.as_str()), "{}: {name}", pack.manifest.name);
            }
            seen += 1;
        }
        assert!(seen >= 9, "{seen} maps");
        let reefbreak = catalog::entry(peakrunner_core::terrain::MapId::Reefbreak).pack().unwrap();
        assert!(reefbreak.manifest.files.contains_key("models.glb"));
    }
}
