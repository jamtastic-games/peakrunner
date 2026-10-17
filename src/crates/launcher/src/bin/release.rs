//! Offline publisher. Never accepts a private key from environment or command text.
use peakrunner_launcher::{self as launcher, Asset, Envelope, Manifest, Result};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("check-public") && args.len() == 2 {
        let platform = &args[1];
        if !["macos-arm64", "macos-x64", "windows-x64", "linux-x64"].contains(&platform.as_str()) {
            return Err("Unsupported platform".into());
        }
        let release = launcher::verify(
            &launcher::fetch(
                &format!("{}/{platform}/latest.json", launcher::ORIGIN),
                256 * 1024,
            )?,
            &launcher::public_key()?,
            platform,
        )?;
        for asset in &release.manifest.files {
            let bytes = launcher::fetch(
                &format!("{}/blobs/{}", launcher::ORIGIN, asset.sha256),
                asset.size,
            )?;
            if bytes.len() as u64 != asset.size || launcher::digest(&bytes) != asset.sha256 {
                return Err("Public blob checksum mismatch".into());
            }
            println!("Verified {}", asset.path);
        }
        println!(
            "PASS: {platform} signed public release {} / {}",
            release.manifest.version, release.manifest.sequence
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("install-latest") && args.len() == 2 {
        let release = launcher::latest()?;
        let store = launcher::Store::open(
            args[1].clone().into(),
            launcher::public_key()?,
            launcher::platform().into(),
        )?;
        store.install(
            &release,
            |asset| {
                launcher::fetch(
                    &format!("{}/blobs/{}", launcher::ORIGIN, asset.sha256),
                    asset.size,
                )
            },
            |message| println!("{message}"),
        )?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("public-key") && args.len() == 2 {
        let key =
            Ed25519KeyPair::from_pkcs8(&fs::read(&args[1])?).map_err(|_| "Invalid signing key")?;
        println!("{}", launcher::hex(key.public_key().as_ref()));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("install-local") && args.len() == 4 {
        let public = launcher::public_key()?;
        let release = launcher::verify(&fs::read(&args[1])?, &public, launcher::platform())?;
        let store =
            launcher::Store::open(args[3].clone().into(), public, launcher::platform().into())?;
        store.install(
            &release,
            |asset| Ok(fs::read(Path::new(&args[2]).join(&asset.sha256))?),
            |message| println!("{message}"),
        )?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("keygen") && args.len() == 2 {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| "Key generation failed")?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&args[1])?;
        file.write_all(document.as_ref())?;
        file.sync_all()?;
        let key =
            Ed25519KeyPair::from_pkcs8(document.as_ref()).map_err(|_| "Invalid generated key")?;
        println!("{}", launcher::hex(key.public_key().as_ref()));
        return Ok(());
    }
    if args.len() != 8 || args[0] != "publish" {
        return Err("Usage: launcher-release keygen KEY | public-key KEY | publish KEY PLATFORM VERSION SEQUENCE PAYLOAD NOTES OUTPUT | install-local MANIFEST BLOBS STORE | check-public PLATFORM | install-latest STORE".into());
    }
    let key =
        Ed25519KeyPair::from_pkcs8(&fs::read(&args[1])?).map_err(|_| "Invalid signing key")?;
    if key.public_key().as_ref() != launcher::public_key()? {
        return Err("Signing key does not match launcher's pinned public key".into());
    }
    let platform = &args[2];
    let version = &args[3];
    let sequence = args[4].parse()?;
    let input = Path::new(&args[5]);
    let output = Path::new(&args[7]);
    if output.exists() {
        return Err("Refusing to overwrite a release directory".into());
    }
    let executable = if platform == "windows-x64" {
        "game/PeakRunner.exe"
    } else {
        "game/peakrunner"
    };
    let mut paths = Vec::new();
    walk(input, input, &mut paths)?;
    paths.sort();
    let mut files = Vec::new();
    for path in &paths {
        let bytes = fs::read(input.join(path))?;
        files.push(Asset {
            path: path.clone(),
            sha256: launcher::digest(&bytes),
            size: bytes.len() as u64,
        });
    }
    let manifest = Manifest {
        schema: 1,
        sequence,
        version: version.clone(),
        platform: platform.clone(),
        notes: fs::read_to_string(&args[6])?,
        executable: executable.into(),
        files,
    };
    let payload = serde_json::to_string(&manifest)?;
    let signature = launcher::hex(key.sign(payload.as_bytes()).as_ref());
    let bytes = serde_json::to_vec_pretty(&Envelope { payload, signature })?;
    launcher::verify(&bytes, key.public_key().as_ref(), platform)?;
    fs::create_dir_all(output.join("blobs"))?;
    for file in &manifest.files {
        fs::copy(
            input.join(&file.path),
            output.join("blobs").join(&file.sha256),
        )?;
    }
    fs::create_dir_all(output.join(platform))?;
    fs::write(output.join(platform).join("latest.json"), bytes)?;
    println!(
        "Signed {} files for {platform}, sequence {sequence}. Upload blobs before latest.json.",
        manifest.files.len()
    );
    Ok(())
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err("Payload symlinks are not allowed".into());
        }
        if kind.is_dir() {
            walk(root, &entry.path(), out)?;
        } else if kind.is_file() {
            out.push(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("Non UTF8 payload path")?
                    .replace('\\', "/"),
            );
        } else {
            return Err("Payload must contain regular files only".into());
        }
    }
    Ok(())
}
