//! The server's TLS identity. Operators with a CA certificate keep using it
//! (`PEAKRUNNER_TLS_CERT`/`PEAKRUNNER_TLS_KEY`, or the Docker mount at
//! `/run/peakrunner`). Everyone else gets a key and self-signed certificate
//! generated on first run and kept in the config directory, so the
//! fingerprint players pin stays the same across restarts.
use std::{io::{self, Write}, path::Path};
use peakrunner_discovery::quic::{Fingerprint, fingerprint};
use crate::wire::invalid;

pub const CERT_FILE: &str = "server-cert.pem";
pub const KEY_FILE: &str = "server-key.pem";
const DOCKER_CERT: &str = "/run/peakrunner/fullchain.pem";
const DOCKER_KEY: &str = "/run/peakrunner/privkey.pem";

pub struct ServerIdentity {
    pub cert: Vec<u8>, pub key: Vec<u8>,
    /// SHA-256 of the end-entity certificate: what an invite link pins.
    pub fingerprint: Fingerprint,
    /// Generated here (self-signed) rather than supplied by the operator.
    pub generated: bool,
}

/// The operator's certificate if one is configured, else the generated one
/// in `dir`. With `PEAKRUNNER_TLS_*` set, or `/run/peakrunner` present (the
/// Docker mount), a missing file is an error exactly as before: an official
/// deployment never falls back to a key players haven't seen.
pub fn load(dir: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> io::Result<ServerIdentity> {
    let explicit = env("PEAKRUNNER_TLS_CERT").is_some() || env("PEAKRUNNER_TLS_KEY").is_some();
    if explicit || Path::new("/run/peakrunner").exists() {
        let cert = std::fs::read(env("PEAKRUNNER_TLS_CERT").unwrap_or(DOCKER_CERT.into()))?;
        let key = std::fs::read(env("PEAKRUNNER_TLS_KEY").unwrap_or(DOCKER_KEY.into()))?;
        let fingerprint = leaf_fingerprint(&cert)?;
        return Ok(ServerIdentity { cert, key, fingerprint, generated: false });
    }
    let dir = dir.ok_or_else(|| invalid("no config directory for the server key; use --config-dir or PEAKRUNNER_TLS_CERT/KEY"))?;
    self_signed(dir)
}

/// Load the generated identity in `dir`, creating it on first use.
pub fn self_signed(dir: &Path) -> io::Result<ServerIdentity> {
    let (cert_path, key_path) = (dir.join(CERT_FILE), dir.join(KEY_FILE));
    match (cert_path.exists(), key_path.exists()) {
        (true, true) => {}
        (false, false) => {
            private_dir(dir)?;
            let generated = rcgen::generate_simple_self_signed(vec!["peakrunner-server.invalid".into()]).map_err(io::Error::other)?;
            // The key first: a crash between the two leaves a key without a
            // certificate, which is reported rather than overwritten.
            write_private(&key_path, generated.key_pair.serialize_pem().as_bytes())?;
            write_private(&cert_path, generated.cert.pem().as_bytes())?;
        }
        _ => return Err(invalid(&format!("{} has only one of {CERT_FILE} and {KEY_FILE}; restore the other or remove both \
            to make a new key (invite links and listings then change)", dir.display()))),
    }
    let cert = std::fs::read(&cert_path)?;
    let key = std::fs::read(&key_path)?;
    // Check the pair now, not at the first handshake.
    crate::quic::server_config(&cert, &key)?;
    let fingerprint = leaf_fingerprint(&cert)?;
    Ok(ServerIdentity { cert, key, fingerprint, generated: true })
}

pub(crate) fn leaf_fingerprint(pem: &[u8]) -> io::Result<Fingerprint> {
    let first = rustls_pemfile::certs(&mut io::Cursor::new(pem)).next().ok_or_else(|| invalid("no certificate in the TLS certificate file"))??;
    Ok(fingerprint(&first))
}

/// Create `dir` (owner-only on Unix) if needed.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() { return Ok(()); }
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write a new file only its owner can read (Unix); never overwrites.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_generated_key_persists_and_half_a_pair_is_an_error() {
        let dir = std::env::temp_dir().join(format!("peakrunner-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = self_signed(&dir).unwrap();
        assert!(first.generated);
        let again = self_signed(&dir).unwrap();
        assert_eq!((again.fingerprint, &again.cert), (first.fingerprint, &first.cert), "same key after a restart");
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // The fingerprint is of the DER certificate players will see.
        let der = rustls_pemfile::certs(&mut io::Cursor::new(&first.cert)).next().unwrap().unwrap();
        assert_eq!(fingerprint(&der), first.fingerprint);
        // Operator-supplied files take precedence.
        let cert_path = dir.join(CERT_FILE).to_string_lossy().into_owned();
        let key_path = dir.join(KEY_FILE).to_string_lossy().into_owned();
        let env = |k: &str| match k { "PEAKRUNNER_TLS_CERT" => Some(cert_path.clone()), "PEAKRUNNER_TLS_KEY" => Some(key_path.clone()), _ => None };
        let operator = load(None, &env).unwrap();
        assert!(!operator.generated && operator.fingerprint == first.fingerprint);
        let missing = |k: &str| (k == "PEAKRUNNER_TLS_CERT").then(|| "/nonexistent/fullchain.pem".to_string());
        assert!(load(Some(&dir), &missing).is_err(), "a configured certificate never falls back to a generated one");
        std::fs::remove_file(dir.join(CERT_FILE)).unwrap();
        assert!(self_signed(&dir).is_err(), "a lone key isn't replaced");
        std::fs::remove_file(dir.join(KEY_FILE)).unwrap();
        assert_ne!(self_signed(&dir).unwrap().fingerprint, first.fingerprint, "removing both makes a new key");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
