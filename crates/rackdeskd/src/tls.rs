//! HTTPS certificates.
//!
//! With no certificate from the admin, the server makes a self-signed one on
//! first start and keeps it in `<data_dir>/tls/`. A browser can't verify a
//! self-signed certificate on its own, so the server prints the SHA-256
//! fingerprint at start. The admin compares it with the one the browser shows.

use anyhow::Context;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::CertificateDer;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// Apple platforms refuse server certificates valid for more than 825 days.
const VALID_FOR: Duration = Duration::from_secs(825 * 24 * 60 * 60);
/// A certificate with less time left than this is replaced at start.
const RENEW_BEFORE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// When the server uses HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TlsMode {
    /// HTTPS, except when listening on a loopback address.
    Auto,
    /// HTTPS always.
    On,
    /// Plain HTTP always. Only safe on loopback or behind a TLS proxy.
    Off,
}

#[derive(Debug, Clone)]
pub struct TlsFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
}

/// Returns the self-signed certificate in `<data_dir>/tls/`. It is created if
/// it is missing, and replaced if it is close to its end date.
///
/// `names` are the DNS names and IP addresses the certificate is valid for.
pub fn self_signed(data_dir: &Path, names: &[String]) -> anyhow::Result<TlsFiles> {
    let dir = data_dir.join("tls");
    let files = TlsFiles {
        cert: dir.join("cert.pem"),
        key: dir.join("key.pem"),
    };
    if files.key.exists() && still_valid(&files.cert) {
        return Ok(files);
    }

    let key = rcgen::KeyPair::generate().context("generating the TLS key")?;
    let mut params =
        rcgen::CertificateParams::new(names.to_vec()).context("building the certificate")?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "RackDesk self-signed");
    let now = time::OffsetDateTime::now_utc();
    // Start 1 day back, so a client whose clock is behind still accepts it.
    params.not_before = now - DAY;
    params.not_after = now + VALID_FOR;
    let cert = params
        .self_signed(&key)
        .context("signing the certificate")?;

    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    write_new(&files.key, key.serialize_pem().as_bytes(), 0o600)?;
    write_new(&files.cert, cert.pem().as_bytes(), 0o644)?;
    Ok(files)
}

/// The end date is not read from the certificate: that needs an X.509 parser.
/// The file time is used, since the server wrote the file when it made the
/// certificate.
fn still_valid(cert: &Path) -> bool {
    let Ok(written) = fs::metadata(cert).and_then(|m| m.modified()) else {
        return false;
    };
    match SystemTime::now().duration_since(written) {
        Ok(age) => age + RENEW_BEFORE < VALID_FOR,
        // The file time is in the future. Keep the certificate.
        Err(_) => true,
    }
}

/// Writes to a temp file created with `mode`, then renames it into place.
fn write_new(path: &Path, body: &[u8], mode: u32) -> anyhow::Result<()> {
    let tmp = path.with_extension("pem.tmp");
    // Remove a leftover temp file, so the new one is created with `mode`.
    let _ = fs::remove_file(&tmp);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut f = opts
        .open(&tmp)
        .with_context(|| format!("creating {}", tmp.display()))?;
    f.write_all(body)
        .and_then(|_| f.sync_all())
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// SHA-256 of the first certificate in a PEM file, as browsers show it:
/// 32 bytes in upper-case hex with `:` between them.
pub fn fingerprint(cert: &Path) -> anyhow::Result<String> {
    let der = CertificateDer::from_pem_file(cert)
        .with_context(|| format!("reading the certificate {}", cert.display()))?;
    let digest = Sha256::digest(der.as_ref());
    let hex: Vec<String> = digest.iter().map(|b| format!("{b:02X}")).collect();
    Ok(hex.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec![
            "localhost".into(),
            "127.0.0.1".into(),
            "desk.example".into(),
        ]
    }

    #[test]
    fn creates_a_certificate_and_reuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let files = self_signed(dir.path(), &names()).unwrap();
        let cert = fs::read_to_string(&files.cert).unwrap();
        let key = fs::read_to_string(&files.key).unwrap();
        assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"));

        let again = self_signed(dir.path(), &names()).unwrap();
        assert_eq!(fs::read_to_string(&again.cert).unwrap(), cert);
        assert_eq!(fs::read_to_string(&again.key).unwrap(), key);
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let files = self_signed(dir.path(), &names()).unwrap();
        let mode = fs::metadata(&files.key).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_old_certificate_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let files = self_signed(dir.path(), &names()).unwrap();
        let before = fs::read_to_string(&files.cert).unwrap();

        let old = SystemTime::now() - (VALID_FOR - RENEW_BEFORE + DAY);
        fs::File::options()
            .write(true)
            .open(&files.cert)
            .unwrap()
            .set_modified(old)
            .unwrap();

        let files = self_signed(dir.path(), &names()).unwrap();
        assert_ne!(fs::read_to_string(&files.cert).unwrap(), before);
    }

    #[test]
    fn a_missing_key_gives_a_new_pair() {
        let dir = tempfile::tempdir().unwrap();
        let files = self_signed(dir.path(), &names()).unwrap();
        let before = fs::read_to_string(&files.cert).unwrap();
        fs::remove_file(&files.key).unwrap();

        let files = self_signed(dir.path(), &names()).unwrap();
        assert!(files.key.exists());
        assert_ne!(fs::read_to_string(&files.cert).unwrap(), before);
    }

    #[test]
    fn the_fingerprint_is_32_hex_bytes_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        let files = self_signed(dir.path(), &names()).unwrap();
        let a = fingerprint(&files.cert).unwrap();
        assert_eq!(a.len(), 32 * 2 + 31);
        assert!(a
            .split(':')
            .all(|byte| byte.len() == 2 && byte.bytes().all(|c| c.is_ascii_hexdigit())));
        assert_eq!(fingerprint(&files.cert).unwrap(), a);
    }
}
