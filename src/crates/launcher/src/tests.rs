use super::*;
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
struct Lab {
    root: PathBuf,
    key: Ed25519KeyPair,
}
impl Lab {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("peakrunner-launcher-test-{}", nonce()));
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Self {
            root,
            key: Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap(),
        }
    }
    fn store(&self) -> Store {
        Store::open(
            self.root.clone(),
            self.key.public_key().as_ref().to_vec(),
            "linux-x64".into(),
        )
        .unwrap()
    }
    fn release(&self, n: u64) -> Release {
        let executable = format!("executable version {n}");
        let files = vec![
            Asset {
                path: "game/peakrunner".into(),
                sha256: digest(executable.as_bytes()),
                size: executable.len() as u64,
            },
            Asset {
                path: "map/map.json".into(),
                sha256: digest(b"unchanged map"),
                size: 13,
            },
        ];
        let manifest = Manifest {
            schema: 1,
            sequence: n,
            version: format!("test-{n}"),
            platform: "linux-x64".into(),
            notes: "Test notes".into(),
            executable: "game/peakrunner".into(),
            files,
        };
        self.sign(manifest)
    }
    fn sign(&self, manifest: Manifest) -> Release {
        let payload = serde_json::to_string(&manifest).unwrap();
        Release {
            id: digest(payload.as_bytes()),
            envelope: Envelope {
                signature: hex(self.key.sign(payload.as_bytes()).as_ref()),
                payload,
            },
            manifest,
        }
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn download(asset: &Asset, n: u64) -> Result<Vec<u8>> {
    Ok(if asset.path == "map/map.json" {
        b"unchanged map".to_vec()
    } else {
        format!("executable version {n}").into_bytes()
    })
}
#[test]
fn rejects_unsigned_tampered_wrong_platform_and_unsafe_paths() {
    let lab = Lab::new();
    let release = lab.release(1);
    let key = lab.key.public_key().as_ref();
    let bytes = serde_json::to_vec(&release.envelope).unwrap();
    assert!(verify(&bytes, key, "linux-x64").is_ok());
    assert!(verify(&bytes, key, "windows-x64").is_err());
    let mut tampered = release.envelope.clone();
    tampered.payload.push(' ');
    assert!(verify(&serde_json::to_vec(&tampered).unwrap(), key, "linux-x64").is_err());
    for path in [
        "../outside",
        "game/../../outside",
        "/absolute",
        "game\\outside",
        "game/C:foo",
        "game/CON.txt",
        "game/test.",
        "game//x",
    ] {
        let mut bad = release.manifest.clone();
        bad.files[0].path = path.into();
        let bad = lab.sign(bad);
        assert!(
            verify(
                &serde_json::to_vec(&bad.envelope).unwrap(),
                key,
                "linux-x64"
            )
            .is_err(),
            "{path}"
        );
    }
}
#[test]
fn changed_files_only_and_rollback_preserves_high_water_mark() {
    let lab = Lab::new();
    let store = lab.store();
    let mut count = 0;
    store
        .install(
            &lab.release(1),
            |asset| {
                count += 1;
                download(asset, 1)
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(count, 2);
    count = 0;
    store
        .install(
            &lab.release(2),
            |asset| {
                count += 1;
                download(asset, 2)
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        store.current().unwrap().unwrap().release.manifest.sequence,
        2
    );
    store.rollback().unwrap();
    assert_eq!(
        store.current().unwrap().unwrap().release.manifest.sequence,
        1
    );
    assert!(store
        .install(&lab.release(1), |a| download(a, 1), |_| {})
        .is_err());
    store
        .install(
            &lab.release(2),
            |_| panic!("verified cached files were redownloaded"),
            |_| {},
        )
        .unwrap();
}
#[test]
fn broken_download_and_interruption_leave_the_current_install_intact() {
    let lab = Lab::new();
    let store = lab.store();
    store
        .install(&lab.release(1), |a| download(a, 1), |_| {})
        .unwrap();
    for corrupt in [true, false] {
        assert!(store
            .install(
                &lab.release(2),
                |_| if corrupt {
                    Ok(b"corrupt".to_vec())
                } else {
                    Err(invalid("connection interrupted").into())
                },
                |_| {}
            )
            .is_err());
        let installed = store.current().unwrap().unwrap();
        assert_eq!(installed.release.manifest.sequence, 1);
        store.validate_install(&installed).unwrap();
    }
}
#[test]
fn repair_uses_verified_cache_and_detects_corruption_before_play() {
    let lab = Lab::new();
    let store = lab.store();
    store
        .install(&lab.release(1), |a| download(a, 1), |_| {})
        .unwrap();
    let installed = store.current().unwrap().unwrap();
    fs::write(installed.directory.join("game/peakrunner"), b"bad").unwrap();
    assert!(store.validate_install(&installed).is_err());
    assert!(store.play().is_err());
    store
        .install(
            &lab.release(1),
            |_| panic!("unnecessary network download"),
            |_| {},
        )
        .unwrap();
    store
        .validate_install(&store.current().unwrap().unwrap())
        .unwrap();
}
#[test]
fn running_game_and_second_launcher_block_updates() {
    let lab = Lab::new();
    let store = lab.store();
    assert!(Store::open(
        lab.root.clone(),
        lab.key.public_key().as_ref().to_vec(),
        "linux-x64".into()
    )
    .is_err());
    let game = store.game_idle().unwrap();
    assert!(store
        .install(&lab.release(1), |a| download(a, 1), |_| {})
        .is_err());
    drop(game);
    store
        .install(&lab.release(1), |a| download(a, 1), |_| {})
        .unwrap();
}
#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_cache_writes() {
    let lab = Lab::new();
    let store = lab.store();
    let release = lab.release(1);
    let outside = lab.root.join("untouched");
    fs::write(&outside, b"keep me").unwrap();
    std::os::unix::fs::symlink(
        &outside,
        store
            .root
            .join("cache")
            .join(&release.manifest.files[0].sha256),
    )
    .unwrap();
    assert!(store.install(&release, |a| download(a, 1), |_| {}).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"keep me");
}
#[test]
fn staging_names_do_not_collide_at_clock_resolution() {
    let names: std::collections::HashSet<_> = (0..10_000).map(|_| crate::nonce()).collect();
    assert_eq!(names.len(), 10_000);
}
