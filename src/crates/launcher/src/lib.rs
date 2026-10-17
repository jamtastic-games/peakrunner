//! Independent, signed, file-level updater. No game simulation or privileged install.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const ORIGIN: &str = "https://peakrunner.net/updates/v1";
// Ed25519 public key only. Release signing keys must never enter this crate.
pub const PUBLIC_KEY: &str = include_str!("../release-public-key.hex");
pub const LAUNCHER_SCHEMA: u32 = 1;
pub fn native_renderer() -> eframe::Renderer {
    #[cfg(windows)]
    {
        eframe::Renderer::Wgpu
    }
    #[cfg(not(windows))]
    {
        eframe::Renderer::Glow
    }
}
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn unhex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Invalid hex").into());
    }
    Ok((0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect())
}
pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
pub fn platform() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else {
        "unsupported"
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub sequence: u64,
    pub version: String,
    pub platform: String,
    pub notes: String,
    pub executable: String,
    pub files: Vec<Asset>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Clone)]
pub struct Release {
    pub manifest: Manifest,
    pub envelope: Envelope,
    pub id: String,
}
fn safe_path(s: &str) -> bool {
    s.len() <= 180
        && !s.is_empty()
        && s.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                && ![
                    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6",
                    "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7",
                    "lpt8", "lpt9",
                ]
                .contains(
                    &part
                        .split('.')
                        .next()
                        .unwrap()
                        .to_ascii_lowercase()
                        .as_str(),
                )
        })
}
pub fn verify(bytes: &[u8], public: &[u8], target: &str) -> Result<Release> {
    if bytes.len() > 256 * 1024 {
        return Err(invalid("Manifest too large").into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(envelope.payload.as_bytes(), &unhex(&envelope.signature)?)
        .map_err(|_| invalid("Update signature failed; existing install is unchanged"))?;
    let manifest: Manifest = serde_json::from_str(&envelope.payload)?;
    if manifest.schema != LAUNCHER_SCHEMA
        || manifest.sequence == 0
        || manifest.platform != target
        || manifest.version.len() > 80
        || !safe_path(&manifest.version)
        || manifest.version.contains('/')
        || manifest.notes.len() > 16_384
        || manifest.files.is_empty()
        || manifest.files.len() > 256
        || !safe_path(&manifest.executable)
        || !manifest.executable.starts_with("game/")
    {
        return Err(invalid("Unsupported update manifest/platform").into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for f in &manifest.files {
        if !safe_path(&f.path)
            || !(f.path.starts_with("game/") || f.path.starts_with("map/"))
            || !names.insert(f.path.to_ascii_lowercase())
            || f.size == 0
            || f.size > 512 * 1024 * 1024
            || f.sha256.len() != 64
            || !f
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("Unsafe update file").into());
        }
        total = total
            .checked_add(f.size)
            .ok_or_else(|| invalid("Update size overflow"))?;
    }
    if total > 2 * 1024 * 1024 * 1024
        || !names.contains(&manifest.executable.to_ascii_lowercase())
        || !names.contains("map/map.json")
    {
        return Err(invalid("Incomplete or excessive update").into());
    }
    Ok(Release {
        id: digest(envelope.payload.as_bytes()),
        manifest,
        envelope,
    })
}
pub fn public_key() -> Result<Vec<u8>> {
    let key = unhex(PUBLIC_KEY.trim())?;
    if key.len() != 32 {
        return Err(invalid("Release key not configured").into());
    }
    Ok(key)
}
pub fn fetch(url: &str, limit: u64) -> Result<Vec<u8>> {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(if limit <= 256 * 1024 {
            10
        } else {
            120
        })))
        .build();
    let mut response = ureq::Agent::new_with_config(config).get(url).call()?;
    let mut data = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(invalid("Download exceeds declared limit").into());
    }
    Ok(data)
}
pub fn latest() -> Result<Release> {
    verify(
        &fetch(&format!("{ORIGIN}/{}/latest.json", platform()), 256 * 1024)?,
        &public_key()?,
        platform(),
    )
}
pub fn data_dir() -> Result<PathBuf> {
    // Explicit isolated store for portable installs and release QA; never changes trust roots.
    if let Some(path) = std::env::var_os("PEAKRUNNER_LAUNCHER_DATA_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(invalid("Launcher data directory must be absolute").into());
        }
        return Ok(path);
    }
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
    Ok(base
        .ok_or_else(|| invalid("No per-user application data folder"))?
        .join("PeakRunnerLauncher"))
}
fn reject_symlink(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(ancestor) {
            if meta.file_type().is_symlink() {
                return Err(invalid("Symbolic links are not permitted in the install path").into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(
                        invalid("Reparse points are not permitted in the install path").into(),
                    );
                }
            }
        }
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Selection {
    install: String,
    previous: Option<String>,
    highest_sequence: u64,
}
pub struct Store {
    pub root: PathBuf,
    _lock: File,
    public: Vec<u8>,
    target: String,
}
pub struct Installed {
    pub directory: PathBuf,
    pub release: Release,
}
impl Store {
    pub fn open(root: PathBuf, public: Vec<u8>, target: String) -> Result<Self> {
        // Canonicalize existing parent prefixes so macOS /var -> /private/var
        // is accepted, while symlinks *inside* our private store are rejected.
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        for name in ["cache", "installs", "selections"] {
            let p = root.join(name);
            reject_symlink(&p)?;
            fs::create_dir_all(p)?;
        }
        let lockpath = root.join("launcher.lock");
        reject_symlink(&lockpath)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lockpath)?;
        lock.try_lock()
            .map_err(|_| invalid("PeakRunner Launcher is already open"))?;
        Ok(Self {
            root,
            _lock: lock,
            public,
            target,
        })
    }
    pub fn game_idle(&self) -> Result<File> {
        let path = self.root.join("game.lock");
        reject_symlink(&path)?;
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        f.try_lock()
            .map_err(|_| invalid("Close the running game before updating or launching again"))?;
        Ok(f)
    }
    fn selection(&self) -> Result<Option<(u64, Selection)>> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.root.join("selections"))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(n) = name
                .strip_suffix(".json")
                .and_then(|s| s.parse::<u64>().ok())
            {
                records.push((n, entry.path()));
            }
        }
        let Some((n, p)) = records.into_iter().max_by_key(|(n, _)| *n) else {
            return Ok(None);
        };
        reject_symlink(&p)?;
        if p.metadata()?.len() > 4096 {
            return Err(invalid("Invalid install selection").into());
        }
        let s: Selection = serde_json::from_slice(&fs::read(p)?)?;
        for name in std::iter::once(&s.install).chain(s.previous.iter()) {
            if !safe_path(name) || name.contains('/') {
                return Err(invalid("Invalid install selection").into());
            }
        }
        Ok(Some((n, s)))
    }
    fn installed(&self, name: &str) -> Result<Installed> {
        let directory = self.root.join("installs").join(name);
        reject_symlink(&directory)?;
        let path = directory.join("release.json");
        reject_symlink(&path)?;
        if path.metadata()?.len() > 256 * 1024 {
            return Err(invalid("Installed manifest too large").into());
        }
        let release = verify(&fs::read(path)?, &self.public, &self.target)?;
        Ok(Installed { directory, release })
    }
    pub fn current(&self) -> Result<Option<Installed>> {
        self.selection()?
            .map(|(_, s)| self.installed(&s.install))
            .transpose()
    }
    fn file_valid(path: &Path, asset: &Asset) -> bool {
        if reject_symlink(path).is_err() {
            return false;
        }
        let Ok(meta) = fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() || meta.len() != asset.size {
            return false;
        }
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        let mut hash = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => hash.update(&buf[..n]),
                Err(_) => return false,
            }
        }
        hex(&hash.finalize()) == asset.sha256
    }
    pub fn validate_install(&self, installed: &Installed) -> Result<()> {
        for asset in &installed.release.manifest.files {
            if !Self::file_valid(&installed.directory.join(&asset.path), asset) {
                return Err(invalid("Installed files need repair; choose Repair").into());
            }
        }
        Ok(())
    }
    pub fn check_sequence(&self, release: &Release) -> Result<()> {
        if self
            .selection()?
            .is_some_and(|(_, s)| release.manifest.sequence < s.highest_sequence)
        {
            return Err(
                invalid("Server offered an older release; refusing automatic downgrade").into(),
            );
        }
        Ok(())
    }
    fn select(&self, install: String, highest_sequence: u64) -> Result<()> {
        let old = self.selection()?;
        let number = old.as_ref().map_or(1, |(n, _)| n + 1);
        let value = Selection {
            install,
            previous: old.map(|(_, s)| s.install),
            highest_sequence,
        };
        let dir = self.root.join("selections");
        let pending = dir.join(format!("{}.pending", nonce()));
        write_new(&pending, &serde_json::to_vec(&value)?)?;
        // A new immutable record avoids overwrite/rename differences on Windows.
        fs::rename(pending, dir.join(format!("{number:020}.json")))?;
        sync_directory(&dir)?;
        Ok(())
    }
    /// Build an isolated generation, reusing only verified files. Activate last.
    pub fn install(
        &self,
        release: &Release,
        mut download: impl FnMut(&Asset) -> Result<Vec<u8>>,
        mut progress: impl FnMut(String),
    ) -> Result<()> {
        let _idle = self.game_idle()?;
        // Reverify even callers constructing Release by hand cannot bypass signing.
        let release = verify(
            &serde_json::to_vec(&release.envelope)?,
            &self.public,
            &self.target,
        )?;
        self.check_sequence(&release)?;
        // A damaged installed manifest must not prevent a fresh signed repair.
        // Such an install is never trusted as a source of reusable files.
        let current = self.current().ok().flatten();
        let generation = format!("{}-{}", &release.id[..16], nonce());
        let stage = self
            .root
            .join("installs")
            .join(format!("{generation}.pending"));
        fs::create_dir(&stage)?;
        for (i, asset) in release.manifest.files.iter().enumerate() {
            progress(format!(
                "Checking file {} / {}: {}",
                i + 1,
                release.manifest.files.len(),
                asset.path
            ));
            let cache = self.root.join("cache").join(&asset.sha256);
            if !Self::file_valid(&cache, asset) {
                reject_symlink(&cache)?;
                let source = current.as_ref().map(|c| c.directory.join(&asset.path));
                let temp = self.root.join("cache").join(format!("{}.part", nonce()));
                if let Some(source) = source.filter(|p| Self::file_valid(p, asset)) {
                    fs::copy(source, &temp)?;
                } else {
                    progress(format!(
                        "Downloading {} ({:.1} MB)",
                        asset.path,
                        asset.size as f64 / 1_000_000.
                    ));
                    let data = download(asset)?;
                    if data.len() as u64 != asset.size || digest(&data) != asset.sha256 {
                        return Err(invalid(
                            "Downloaded file checksum mismatch; existing install is unchanged",
                        )
                        .into());
                    }
                    write_new(&temp, &data)?;
                }
                if cache.exists() {
                    fs::remove_file(&cache)?;
                } // Only a verified-invalid, hash-named cache file.
                fs::rename(temp, &cache)?;
            }
            let destination = stage.join(&asset.path);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::copy(&cache, &destination)?;
            // FlushFileBuffers on Windows requires a writable handle.
            OpenOptions::new()
                .write(true)
                .open(&destination)?
                .sync_all()?;
            #[cfg(unix)]
            if asset.path == release.manifest.executable {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&destination, fs::Permissions::from_mode(0o700))?;
            }
            sync_directory(destination.parent().unwrap())?;
        }
        write_new(
            &stage.join("release.json"),
            &serde_json::to_vec(&release.envelope)?,
        )?;
        sync_directory(&stage)?;
        let destination = self.root.join("installs").join(&generation);
        fs::rename(&stage, &destination)?;
        sync_directory(&self.root.join("installs"))?;
        self.validate_install(&Installed {
            directory: destination,
            release: release.clone(),
        })?;
        self.select(generation, release.manifest.sequence)?;
        Ok(())
    }
    pub fn rollback(&self) -> Result<()> {
        let _idle = self.game_idle()?;
        let (_, selection) = self
            .selection()?
            .ok_or_else(|| invalid("No installed version"))?;
        let previous = selection
            .previous
            .ok_or_else(|| invalid("No previous version to restore"))?;
        let installed = self.installed(&previous)?;
        self.validate_install(&installed)?;
        self.select(previous, selection.highest_sequence)
    }
    pub fn play(&self) -> Result<Child> {
        let idle = self.game_idle()?;
        let installed = self
            .current()?
            .ok_or_else(|| invalid("Install the game first"))?;
        self.validate_install(&installed)?;
        let logpath = self.root.join("game.log");
        reject_symlink(&logpath)?;
        let log = OpenOptions::new().create(true).append(true).open(logpath)?;
        let mut command = Command::new(
            installed
                .directory
                .join(&installed.release.manifest.executable),
        );
        command
            .current_dir(&installed.directory)
            .env("PEAKRUNNER_MAP_PACK", installed.directory.join("map"))
            .env("PEAKRUNNER_LAUNCHER_LOCK", self.root.join("game.lock"))
            .env_remove("PEAKRUNNER_JOIN")
            .stdout(log.try_clone()?)
            .stderr(log);
        drop(idle);
        Ok(command.spawn()?)
    }
}
pub fn nonce() -> String {
    // Wall clocks can return identical timestamps across concurrent threads.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests;
