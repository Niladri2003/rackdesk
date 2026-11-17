//! Accounts: a single admin account.
//!
//! There is **no default password**. On first start the server generates a
//! random one, prints it once to the terminal, and stores only an argon2id
//! hash. The UI then forces a change on first sign-in. See
//! docs/adr/0003-random-initial-password.md.
//!
//! Sessions are in [`session`]. The limit on password guessing is in
//! [`limiter`]. This crate has no HTTP types: the server maps these to routes.

pub mod limiter;
pub mod session;

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
/// Longer input is refused before hashing, so a huge password can't be used
/// to keep the CPU busy.
pub const MAX_PASSWORD_LEN: usize = 256;

/// Well-known passwords of 12 characters or more. Shorter ones are already
/// refused by the length rule. Compared without regard to letter case.
const COMMON_PASSWORDS: &[&str] = &[
    "123456789012",
    "1234567890123",
    "12345678901234",
    "123456789012345",
    "1234567890123456",
    "012345678901",
    "098765432109",
    "111111111111",
    "123123123123",
    "123412341234",
    "121212121212",
    "112233445566",
    "abcdefghijkl",
    "abcdefghijklm",
    "abcdefghijklmnopqrstuvwxyz",
    "abcdef123456",
    "abc123abc123",
    "password1234",
    "password12345",
    "password123456",
    "passwordpassword",
    "password1password1",
    "passw0rdpassw0rd",
    "p@ssw0rd1234",
    "p@ssword1234",
    "mypassword123",
    "changeme1234",
    "changemechangeme",
    "letmein12345",
    "letmeinletmein",
    "welcome12345",
    "welcome123456",
    "administrator",
    "administrator1",
    "administrator123",
    "admin1234567",
    "adminadminadmin",
    "admin123admin123",
    "rootrootroot",
    "toortoortoor",
    "qwertyuiop12",
    "qwertyuiop123",
    "qwertyuiop[]",
    "qwertyuiopasdf",
    "qwertyuiopasdfghjkl",
    "qwerty123456",
    "qwertyqwerty",
    "1q2w3e4r5t6y",
    "1qaz2wsx3edc",
    "zaq12wsxcde3",
    "asdfghjkl123",
    "asdfasdfasdf",
    "zxcvbnm12345",
    "iloveyou1234",
    "iloveyouiloveyou",
    "trustno1trustno1",
    "correct horse battery staple",
    "correcthorsebatterystaple",
    "rackdeskrackdesk",
    "rackdesk1234",
];

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    WeakPassword,
    #[error("password must be at most {MAX_PASSWORD_LEN} characters")]
    PasswordTooLong,
    #[error("this password is on the list of common passwords")]
    CommonPassword,
    #[error("credential store: {0}")]
    Store(String),
    #[error("randomness unavailable: {0}")]
    Random(String),
}

/// What's stored on disk (`<data_dir>/credentials.json`, mode 0600).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
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

    /// The file that holds the credentials.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the credentials. `None` means the account has not been created.
    pub fn load(&self) -> Result<Option<Credentials>, AuthError> {
        let raw = match fs::read(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AuthError::Store(e.to_string())),
        };
        serde_json::from_slice(&raw)
            .map(Some)
            .map_err(|e| AuthError::Store(e.to_string()))
    }

    /// Loads the credentials, or creates the admin account with a fresh random
    /// password if none exist yet. The plaintext is returned once, so the caller
    /// can print it, and is never written to disk.
    pub fn load_or_init(&self) -> Result<(Credentials, Option<InitialPassword>), AuthError> {
        if let Some(creds) = self.load()? {
            return Ok((creds, None));
        }
        self.reset()
            .map(|(creds, password)| (creds, Some(password)))
    }

    /// Replaces the admin password with a fresh random one, which must be
    /// changed at the next sign-in. Used on first start and for recovery.
    pub fn reset(&self) -> Result<(Credentials, InitialPassword), AuthError> {
        let password = generate_password()?;
        let creds = Credentials {
            username: ADMIN_USER.into(),
            password_hash: hash_password(&password)?,
            must_change_password: true,
        };
        self.save(&creds)?;
        Ok((creds, InitialPassword(password)))
    }

    /// Writes atomically (temp file + rename) with owner-only permissions.
    ///
    /// The new file keeps the owner of the file it replaces. Without this, a
    /// password reset run as root would leave a file the server can't read.
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
        #[cfg(unix)]
        if let Ok(old) = fs::metadata(&self.path) {
            use std::os::unix::fs::MetadataExt;
            let new = f.metadata().map_err(|e| AuthError::Store(e.to_string()))?;
            if (old.uid(), old.gid()) != (new.uid(), new.gid()) {
                std::os::unix::fs::fchown(&f, Some(old.uid()), Some(old.gid()))
                    .map_err(|e| AuthError::Store(e.to_string()))?;
            }
        }
        fs::rename(&tmp, &self.path).map_err(|e| AuthError::Store(e.to_string()))
    }
}

/// Checks a password chosen by a person. Generated passwords skip this.
pub fn check_new_password(password: &str) -> Result<(), AuthError> {
    let chars = password.chars().count();
    if chars < MIN_PASSWORD_LEN {
        return Err(AuthError::WeakPassword);
    }
    if chars > MAX_PASSWORD_LEN {
        return Err(AuthError::PasswordTooLong);
    }
    let mut all = password.chars();
    let first = all.next();
    if all.all(|c| Some(c) == first) {
        // One character repeated, such as "aaaaaaaaaaaa".
        return Err(AuthError::CommonPassword);
    }
    let lower = password.to_lowercase();
    if COMMON_PASSWORDS.contains(&lower.as_str()) {
        return Err(AuthError::CommonPassword);
    }
    Ok(())
}

/// `bytes` random bytes as lower-case hex. Used for ids that must be hard to
/// guess: 32 bytes for a session, 16 for an upload.
pub fn random_hex(bytes: usize) -> Result<String, AuthError> {
    let mut raw = vec![0u8; bytes];
    getrandom::fill(&mut raw).map_err(|e| AuthError::Random(e.to_string()))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes * 2);
    for b in raw {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    Ok(out)
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
    if password.len() > MAX_PASSWORD_LEN * 4 {
        // More bytes than the longest allowed password can have in UTF-8.
        return false;
    }
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

    #[test]
    fn new_password_rules() {
        assert!(check_new_password("a long and unusual phrase").is_ok());
        assert!(matches!(
            check_new_password("short"),
            Err(AuthError::WeakPassword)
        ));
        assert!(matches!(
            check_new_password(&"ab".repeat(MAX_PASSWORD_LEN)),
            Err(AuthError::PasswordTooLong)
        ));
        for common in [
            "password1234",
            "PASSWORD1234",
            "Qwertyuiop12",
            "aaaaaaaaaaaa",
        ] {
            assert!(
                matches!(check_new_password(common), Err(AuthError::CommonPassword)),
                "{common} should be refused"
            );
        }
    }

    #[test]
    fn the_common_password_list_is_lowercase_and_long_enough() {
        for p in COMMON_PASSWORDS {
            assert_eq!(*p, p.to_lowercase());
            assert!(p.chars().count() >= MIN_PASSWORD_LEN, "{p} is dead weight");
        }
    }

    #[test]
    fn an_overlong_password_is_never_hashed() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(!verify_password(&"x".repeat(MAX_PASSWORD_LEN * 4 + 1), &h));
    }

    #[test]
    fn reset_replaces_the_password_and_forces_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());
        let (_, first) = store.load_or_init().unwrap();
        let first = first.unwrap().0;

        let mut creds = store.load().unwrap().unwrap();
        creds.must_change_password = false;
        store.save(&creds).unwrap();

        let (_, second) = store.reset().unwrap();
        let creds = store.load().unwrap().unwrap();
        assert!(creds.must_change_password);
        assert!(verify_password(&second.0, &creds.password_hash));
        assert!(!verify_password(&first, &creds.password_hash));
    }

    #[test]
    fn load_returns_none_before_the_account_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert!(CredentialStore::new(dir.path()).load().unwrap().is_none());
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
