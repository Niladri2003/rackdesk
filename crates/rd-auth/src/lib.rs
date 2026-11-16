//! Accounts: a single admin account.
//!
//! There is **no default password**. On first start the server generates a
//! random one, prints it once to the terminal, and stores only an argon2id
//! hash. The UI then forces a change on first sign-in. See
//! docs/adr/0003-random-initial-password.md.
//!
//! Sessions, rate limiting and lockout are not in this crate yet.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const ADMIN_USER: &str = "admin";
/// Characters for generated passwords: no 0/O, 1/l/I, so it's easy to read off a terminal.
const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// 20 characters from 56 symbols is about 116 bits of entropy.
pub const GENERATED_LEN: usize = 20;
pub const MIN_PASSWORD_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    WeakPassword,
    #[error("credential store: {0}")]
    Store(String),
    #[error("randomness unavailable: {0}")]
    Random(String),
}

/// What's stored on disk (`<data_dir>/credentials.json`, mode 0600).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    /// PHC string, e.g. "$argon2id$v=19$m=19456,t=2,p=1$...".
    pub password_hash: String,
    pub must_change_password: bool,
}

pub struct CredentialStore {
    path: PathBuf,
}

/// Returned by [`CredentialStore::load_or_init`] the first time only.
pub struct InitialPassword(pub String);

impl CredentialStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("credentials.json"),
        }
    }

    /// Loads the credentials, or creates the admin account with a fresh random
    /// password if none exist yet. The plaintext is returned once, so the caller
    /// can print it, and is never written to disk.
    pub fn load_or_init(&self) -> Result<(Credentials, Option<InitialPassword>), AuthError> {
        if self.path.exists() {
            let raw = fs::read(&self.path).map_err(|e| AuthError::Store(e.to_string()))?;
            let creds =
                serde_json::from_slice(&raw).map_err(|e| AuthError::Store(e.to_string()))?;
            return Ok((creds, None));
        }
        let password = generate_password()?;
        let creds = Credentials {
            username: ADMIN_USER.into(),
            password_hash: hash_password(&password)?,
            must_change_password: true,
        };
        self.save(&creds)?;
        Ok((creds, Some(InitialPassword(password))))
    }

    /// Writes atomically (temp file + rename) with owner-only permissions.
    pub fn save(&self, creds: &Credentials) -> Result<(), AuthError> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| AuthError::Store("no parent dir".into()))?;
        fs::create_dir_all(dir).map_err(|e| AuthError::Store(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let body = serde_json::to_vec_pretty(creds).map_err(|e| AuthError::Store(e.to_string()))?;
        f.write_all(&body)
            .and_then(|_| f.sync_all())
            .map_err(|e| AuthError::Store(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| AuthError::Store(e.to_string()))
    }
}

pub fn generate_password() -> Result<String, AuthError> {
    // Rejection sampling, so every character is equally likely (no modulo bias).
    let limit = 256 - (256 % ALPHABET.len());
    let mut out = String::with_capacity(GENERATED_LEN);
    let mut buf = [0u8; 64];
    while out.len() < GENERATED_LEN {
        getrandom::fill(&mut buf).map_err(|e| AuthError::Random(e.to_string()))?;
        for &b in &buf {
            if (b as usize) < limit && out.len() < GENERATED_LEN {
                out.push(ALPHABET[b as usize % ALPHABET.len()] as char);
            }
        }
    }
    Ok(out)
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AuthError::WeakPassword);
    }
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| AuthError::Random(e.to_string()))?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| AuthError::Store(e.to_string()))?;
    // Argon2::default() is argon2id with the OWASP-recommended baseline (19 MiB, t=2, p=1).
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Store(e.to_string()))
}

/// Constant-time verification (inside argon2). Returns false on any malformed hash.
pub fn verify_password(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_passwords_use_the_alphabet_and_differ() {
        let a = generate_password().unwrap();
        let b = generate_password().unwrap();
        assert_eq!(a.len(), GENERATED_LEN);
        assert!(a.bytes().all(|c| ALPHABET.contains(&c)));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_and_verify() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("x", "not a hash"));
    }

    #[test]
    fn rejects_short_passwords() {
        assert!(matches!(
            hash_password("short"),
            Err(AuthError::WeakPassword)
        ));
    }

    #[test]
    fn first_start_creates_admin_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());

        let (creds, initial) = store.load_or_init().unwrap();
        let initial = initial
            .expect("first start returns the generated password")
            .0;
        assert_eq!(creds.username, ADMIN_USER);
        assert!(creds.must_change_password);
        assert!(verify_password(&initial, &creds.password_hash));

        // The plaintext is never stored.
        let raw = std::fs::read_to_string(dir.path().join("credentials.json")).unwrap();
        assert!(!raw.contains(&initial));

        // Second start: loads, doesn't regenerate.
        let (again, none) = store.load_or_init().unwrap();
        assert!(none.is_none());
        assert_eq!(again.password_hash, creds.password_hash);
    }

    #[cfg(unix)]
    #[test]
    fn credentials_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        CredentialStore::new(dir.path()).load_or_init().unwrap();
        let mode = std::fs::metadata(dir.path().join("credentials.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
