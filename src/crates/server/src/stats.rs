//! How many people have played: each player who joins sends an anonymous
//! game ID (`ClientMsg::Hello::player`, a random number their game makes on
//! first run), and a server with a stats folder (`PEAKRUNNER_STATS_DIR`, the
//! official servers) keeps only a salted hash of it, per day (UTC) and in
//! total. The servers on a host share the folder: each writes what it has
//! seen (`seen-<name>.json`), then all of them combined as the public counts,
//! `public/stats.json` (`Counts`), which the website shows. The salt stays
//! in the folder, never in `public/`. No folder, no counting.
//!
//! The counting runs on its own thread, so the game loop never waits on the
//! disk; a folder that can't be used is logged and the server plays on.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The counts are written at least this often, so "today" turns over at
/// midnight UTC without a join.
const REFRESH: Duration = Duration::from_secs(60);
/// Days each server keeps its own day lists for: today and yesterday.
const DAYS_KEPT: usize = 2;

/// What one server has seen: salted hashes of game IDs, all of them and
/// by day ("YYYY-MM-DD", UTC).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    all: BTreeSet<String>,
    days: BTreeMap<String, BTreeSet<String>>,
}

/// The public counts: people who have ever played, people who played on
/// `day` (UTC), and when this was written (Unix seconds).
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub total: usize,
    pub today: usize,
    pub day: String,
    pub updated: u64,
}

/// A running counter: tell it each player who joins (`joined`).
#[derive(Clone)]
pub struct Stats {
    tx: mpsc::Sender<String>,
}

impl Stats {
    /// From `PEAKRUNNER_STATS_DIR` (unset or empty: none) for the server
    /// called `name`. A folder that can't be used is logged, not fatal.
    pub fn from_env(name: &str) -> Option<Self> {
        let dir = std::env::var("PEAKRUNNER_STATS_DIR").ok().filter(|d| !d.is_empty())?;
        match Self::spawn(PathBuf::from(&dir), name) {
            Ok(stats) => { log::info!("Counting players in {dir}"); Some(stats) }
            Err(e) => { log::warn!("Not counting players: {dir}: {e}"); None }
        }
    }

    /// Count into `dir` as the server called `name`.
    pub fn spawn(dir: PathBuf, name: &str) -> io::Result<Self> {
        fs::create_dir_all(dir.join("public"))?;
        let salt = salt(&dir)?;
        let name = slug(name);
        let file = dir.join(format!("seen-{name}.json"));
        let seen = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut counter = Counter { dir, name, file, salt, seen };
        counter.write(SystemTime::now())?;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new().name("peakrunner-stats".into()).spawn(move || counter.run(rx))?;
        Ok(Self { tx })
    }

    /// A player joined with game ID `player` (32 lowercase hex digits;
    /// anything else, such as an older game's empty one, isn't counted).
    pub fn joined(&self, player: &str) {
        if valid(player) { let _ = self.tx.send(player.to_string()); }
    }
}

/// A game ID: 32 lowercase hex digits.
pub fn valid(player: &str) -> bool {
    player.len() == 32 && player.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct Counter {
    dir: PathBuf,
    /// The server's name as a file name (`slug`).
    name: String,
    file: PathBuf,
    salt: String,
    seen: Seen,
}

impl Counter {
    fn run(mut self, rx: mpsc::Receiver<String>) {
        loop {
            let result = match rx.recv_timeout(REFRESH) {
                Ok(player) => self.record(&player, SystemTime::now()),
                Err(mpsc::RecvTimeoutError::Timeout) => self.write(SystemTime::now()),
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            };
            if let Err(e) = result { log::warn!("player counts: {e}"); }
        }
    }

    /// Note `player` at `at`; the files are written when they're new today.
    fn record(&mut self, player: &str, at: SystemTime) -> io::Result<()> {
        let key = self.key(player);
        let fresh = self.seen.days.entry(day(at)).or_default().insert(key.clone());
        self.seen.all.insert(key);
        if fresh { self.write(at) } else { Ok(()) }
    }

    /// A game ID's salted hash (128 bits, hex).
    fn key(&self, player: &str) -> String {
        hex(&Sha256::new().chain_update(self.salt.as_bytes()).chain_update(player.as_bytes()).finalize()[..16])
    }

    /// This server's file, then every server's combined as the public
    /// counts.
    fn write(&mut self, at: SystemTime) -> io::Result<()> {
        while self.seen.days.len() > DAYS_KEPT { self.seen.days.pop_first(); }
        replace(&self.file, &serde_json::to_vec(&self.seen)?, &self.name)?;
        let counts = self.combined(at)?;
        replace(&self.dir.join("public").join("stats.json"), &serde_json::to_vec_pretty(&counts)?, &self.name)
    }

    /// Every server's seen files in the folder, combined (each person once).
    fn combined(&self, at: SystemTime) -> io::Result<Counts> {
        let today = day(at);
        let (mut all, mut day_set): (BTreeSet<String>, BTreeSet<String>) = (BTreeSet::new(), BTreeSet::new());
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !(name.starts_with("seen-") && name.ends_with(".json")) { continue; }
            let Some(seen) = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Seen>(&b).ok()) else { continue };
            if let Some(d) = seen.days.get(&today) { day_set.extend(d.iter().cloned()); }
            all.extend(seen.all);
        }
        Ok(Counts { total: all.len(), today: day_set.len(), day: today, updated: at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) })
    }
}

/// The folder's salt, made on first use (32 random bytes, hex) and shared by
/// every server counting there.
fn salt(dir: &Path) -> io::Result<String> {
    let path = dir.join("salt");
    let read = |path: &Path| fs::read_to_string(path).map(|s| s.trim().to_string());
    if let Ok(salt) = read(&path) { if salt.len() >= 32 { return Ok(salt); } }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let salt = hex(&bytes);
    match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => { file.write_all(salt.as_bytes())?; Ok(salt) }
        // Another server made it first.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => read(&path),
        Err(e) => Err(e),
    }
}

/// Write `path` whole: to a temporary file beside it, then renamed over it.
/// The temporary file carries the writing server's name: servers in their
/// own containers can share a process ID, and two writing the same
/// temporary file lose one rename.
fn replace(path: &Path, bytes: &[u8], writer: &str) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{writer}-{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// A server name as a file name: lowercase letters and digits, dashes
/// between words.
fn slug(name: &str) -> String {
    let words: Vec<String> = name.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_ascii_lowercase).collect();
    if words.is_empty() { "server".into() } else { words.join("-") }
}

fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

/// The UTC date of `at`, "YYYY-MM-DD".
fn day(at: SystemTime) -> String {
    let days = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Civil date from a day count (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime { UNIX_EPOCH + Duration::from_secs(secs) }
    const ANN: &str = "0123456789abcdef0123456789abcdef";
    const BOB: &str = "fedcba9876543210fedcba9876543210";

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("peakrunner-stats-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn counter(dir: &Path, name: &str) -> Counter {
        fs::create_dir_all(dir.join("public")).unwrap();
        Counter { dir: dir.to_path_buf(), name: name.into(), file: dir.join(format!("seen-{name}.json")), salt: salt(dir).unwrap(), seen: Seen::default() }
    }

    fn public(dir: &Path) -> Counts { serde_json::from_slice(&fs::read(dir.join("public/stats.json")).unwrap()).unwrap() }

    #[test]
    fn utc_days_are_dated_right() {
        assert_eq!(day(at(0)), "1970-01-01");
        assert_eq!(day(at(951_782_400)), "2000-02-29");
        assert_eq!(day(at(1_791_763_199)), "2026-10-11");
        assert_eq!(day(at(1_791_763_200)), "2026-10-12");
    }

    #[test]
    fn each_person_counts_once_across_servers_and_days() {
        let dir = folder("people");
        let (mut a, mut b) = (counter(&dir, "a"), counter(&dir, "b"));
        let morning = at(1_791_720_000);
        a.record(ANN, morning).unwrap();
        a.record(ANN, morning).unwrap();
        b.record(ANN, morning).unwrap();
        b.record(BOB, morning).unwrap();
        assert_eq!(public(&dir), Counts { total: 2, today: 2, day: "2026-10-11".into(), updated: 1_791_720_000 });
        // The next day: Ann again; the day count starts over, the total holds.
        let next = at(1_791_806_400);
        a.record(ANN, next).unwrap();
        let counts = public(&dir);
        assert_eq!((counts.total, counts.today, counts.day.as_str()), (2, 1, "2026-10-12"));
        // Midnight passes with nobody joining: today drops to nothing.
        b.write(at(1_791_900_000)).unwrap();
        assert_eq!((public(&dir).total, public(&dir).today), (2, 0));
        // Nothing public names a game ID, or even its hash.
        let text = fs::read_to_string(dir.join("public/stats.json")).unwrap();
        assert!(!text.contains(ANN) && !text.contains(&a.key(ANN)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_kept_salted_and_old_days_are_dropped() {
        let dir = folder("salted");
        let mut a = counter(&dir, "a");
        for d in 0..4 { a.record(ANN, at(1_791_720_000 + d * 86_400)).unwrap(); }
        let kept = fs::read_to_string(dir.join("seen-a.json")).unwrap();
        assert!(!kept.contains(ANN), "only the hash is kept");
        assert_eq!(a.seen.days.len(), DAYS_KEPT);
        // The salt is made once and shared.
        assert_eq!(salt(&dir).unwrap(), a.salt);
        assert_eq!(counter(&dir, "b").key(ANN), a.key(ANN));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_real_game_ids_count() {
        assert!(valid(ANN));
        for bad in ["", "0123", "0123456789ABCDEF0123456789ABCDEF", "0123456789abcdef0123456789abcdeg", "../../etc/passwd0123456789abcdef"] {
            assert!(!valid(bad), "{bad}");
        }
        assert_eq!(slug("Springdale Central"), "springdale-central");
        assert_eq!(slug("../x"), "x");
        assert_eq!(slug("!!!"), "server");
    }

    #[test]
    fn servers_sharing_the_folder_never_trip_over_each_other() {
        // Servers in their own containers can share a process ID; threads of
        // one process stand in for them.
        let dir = folder("shared");
        let writers: Vec<_> = ["a", "b", "c"].into_iter().map(|name| {
            let mut c = counter(&dir, name);
            thread::spawn(move || (0..300).try_for_each(|_| c.write(at(1_791_720_000))))
        }).collect();
        for w in writers { w.join().unwrap().unwrap(); }
        assert_eq!(public(&dir).total, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_running_counter_writes_the_public_counts() {
        let dir = folder("running");
        let stats = Stats::spawn(dir.clone(), "Test Server").unwrap();
        stats.joined(ANN);
        stats.joined("not an id");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while public(&dir).total < 1 && std::time::Instant::now() < deadline { thread::sleep(Duration::from_millis(20)); }
        assert_eq!(public(&dir).total, 1);
        assert!(dir.join("seen-test-server.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
