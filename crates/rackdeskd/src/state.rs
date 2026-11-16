//! State shared by every request: the account, the sessions and the limiters.

use crate::error::ApiErr;
use rd_auth::limiter::{Limiter, Policy};
use rd_auth::session::SessionStore;
use rd_auth::{CredentialStore, Credentials};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{Mutex, Semaphore};

/// How many password hashes run at once. One hash uses about 19 MiB of
/// memory, so this bounds the memory a burst of sign-in attempts can take.
const HASHES_AT_ONCE: usize = 2;

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    /// True when browsers reach the server over HTTPS. The session cookie is
    /// then marked `Secure`.
    pub https: bool,
    /// True when a reverse proxy on the same machine or network forwards the
    /// requests. The client address is then read from `X-Forwarded-For`.
    pub behind_proxy: bool,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    config: Config,
    store: CredentialStore,
    account: Mutex<Account>,
    pub sessions: SessionStore,
    pub ip_limiter: Limiter,
    pub account_limiter: Limiter,
    hashing: Semaphore,
    /// Checked when the username is unknown, so that case takes as long as a
    /// wrong password.
    dummy_hash: String,
}

struct Account {
    creds: Credentials,
    stamp: Option<FileStamp>,
}

/// Identifies one version of the credentials file. It changes when another
/// process, such as `rackdeskd reset-password`, replaces the file.
#[derive(PartialEq, Eq, Clone, Copy)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
    inode: u64,
}

impl FileStamp {
    async fn read(path: &std::path::Path) -> Option<Self> {
        let meta = tokio::fs::metadata(path).await.ok()?;
        #[cfg(unix)]
        let inode = std::os::unix::fs::MetadataExt::ino(&meta);
        #[cfg(not(unix))]
        let inode = 0;
        Some(Self {
            modified: meta.modified().ok(),
            len: meta.len(),
            inode,
        })
    }
}

impl AppState {
    /// `creds` is the account as loaded at start-up.
    pub async fn new(config: Config, creds: Credentials) -> Result<Self, rd_auth::AuthError> {
        let store = CredentialStore::new(&config.data_dir);
        let stamp = FileStamp::read(store.path()).await;
        let dummy_hash = rd_auth::hash_password(&rd_auth::generate_password()?)?;
        Ok(Self(Arc::new(Inner {
            config,
            store,
            account: Mutex::new(Account { creds, stamp }),
            sessions: SessionStore::default(),
            ip_limiter: Limiter::new(Policy::PER_IP),
            account_limiter: Limiter::new(Policy::PER_ACCOUNT),
            hashing: Semaphore::new(HASHES_AT_ONCE),
            dummy_hash,
        })))
    }

    pub fn config(&self) -> &Config {
        &self.0.config
    }

    pub fn sessions(&self) -> &SessionStore {
        &self.0.sessions
    }

    pub fn ip_limiter(&self) -> &Limiter {
        &self.0.ip_limiter
    }

    pub fn account_limiter(&self) -> &Limiter {
        &self.0.account_limiter
    }

    /// The current account. If the credentials file was replaced since the
    /// last call, it is loaded again, and a changed password ends every
    /// session.
    pub async fn credentials(&self) -> Result<Credentials, ApiErr> {
        let mut account = self.0.account.lock().await;
        let stamp = FileStamp::read(self.0.store.path()).await;
        if stamp.is_some() && stamp != account.stamp {
            let store = CredentialStore::new(&self.0.config.data_dir);
            let loaded = tokio::task::spawn_blocking(move || store.load())
                .await
                .map_err(ApiErr::internal)?
                .map_err(ApiErr::internal)?;
            if let Some(creds) = loaded {
                if creds.password_hash != account.creds.password_hash {
                    self.0.sessions.revoke_all();
                    tracing::info!("the credentials file changed; all sessions were ended");
                }
                account.creds = creds;
                account.stamp = stamp;
            }
        }
        Ok(account.creds.clone())
    }

    /// Stores a new password hash and clears the "must change" flag.
    pub async fn set_password_hash(&self, password_hash: String) -> Result<Credentials, ApiErr> {
        let mut account = self.0.account.lock().await;
        let mut creds = account.creds.clone();
        creds.password_hash = password_hash;
        creds.must_change_password = false;
        let store = CredentialStore::new(&self.0.config.data_dir);
        let to_save = creds.clone();
        tokio::task::spawn_blocking(move || store.save(&to_save))
            .await
            .map_err(ApiErr::internal)?
            .map_err(ApiErr::internal)?;
        account.stamp = FileStamp::read(self.0.store.path()).await;
        account.creds = creds.clone();
        Ok(creds)
    }

    /// Checks a password against a hash, off the async runtime. With `None`
    /// it does the same amount of work and returns false.
    pub async fn verify_password(
        &self,
        password: String,
        hash: Option<String>,
    ) -> Result<bool, ApiErr> {
        let known = hash.is_some();
        let hash = hash.unwrap_or_else(|| self.0.dummy_hash.clone());
        let _permit = self.0.hashing.acquire().await.map_err(ApiErr::internal)?;
        let ok = tokio::task::spawn_blocking(move || rd_auth::verify_password(&password, &hash))
            .await
            .map_err(ApiErr::internal)?;
        Ok(ok && known)
    }

    /// Hashes a new password, off the async runtime.
    pub async fn hash_password(&self, password: String) -> Result<String, ApiErr> {
        let _permit = self.0.hashing.acquire().await.map_err(ApiErr::internal)?;
        tokio::task::spawn_blocking(move || rd_auth::hash_password(&password))
            .await
            .map_err(ApiErr::internal)?
            .map_err(ApiErr::internal)
    }
}
