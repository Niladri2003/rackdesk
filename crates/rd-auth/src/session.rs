//! Sign-in sessions, kept in memory.
//!
//! A session id is 256 random bits. The browser holds it in an `HttpOnly`
//! cookie and the server holds the rest. Sessions are not written to disk, so
//! a restart of the server signs everyone out.
//!
//! Every method takes the current time as an argument. Tests pass their own
//! time and do not sleep.

use crate::AuthError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A session ends after this long without a request.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
/// A session ends this long after sign-in, even if it is in use.
pub const ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Upper limit on sessions held at once. It bounds memory use.
pub const MAX_SESSIONS: usize = 64;

/// Length of a session id in hex characters (32 bytes).
const ID_HEX_LEN: usize = 64;

struct Session {
    username: String,
    created: Instant,
    last_seen: Instant,
}

impl Session {
    fn expired(&self, timeouts: &Timeouts, now: Instant) -> bool {
        now.saturating_duration_since(self.last_seen) >= timeouts.idle
            || now.saturating_duration_since(self.created) >= timeouts.absolute
    }
}

#[derive(Clone, Copy)]
struct Timeouts {
    idle: Duration,
    absolute: Duration,
}

pub struct SessionStore {
    // The map uses the standard hasher, which is keyed with a random secret
    // per process. A lookup compares the full id only after a 64-bit keyed
    // hash matches, so response time does not reveal a prefix of a valid id.
    sessions: Mutex<HashMap<String, Session>>,
    timeouts: Timeouts,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(IDLE_TIMEOUT, ABSOLUTE_TIMEOUT)
    }
}

impl SessionStore {
    pub fn new(idle: Duration, absolute: Duration) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            timeouts: Timeouts { idle, absolute },
        }
    }

    /// Starts a session and returns its id. The id is the only secret: send
    /// it to the browser in the cookie and nowhere else.
    pub fn create(&self, username: &str, now: Instant) -> Result<String, AuthError> {
        let id = random_id()?;
        let mut sessions = self.lock();
        let timeouts = self.timeouts;
        sessions.retain(|_, s| !s.expired(&timeouts, now));
        while sessions.len() >= MAX_SESSIONS {
            // Full: end the session that has been unused for the longest time.
            let oldest = sessions
                .iter()
                .min_by_key(|(_, s)| s.last_seen)
                .map(|(id, _)| id.clone());
            match oldest {
                Some(id) => sessions.remove(&id),
                None => break,
            };
        }
        sessions.insert(
            id.clone(),
            Session {
                username: username.to_owned(),
                created: now,
                last_seen: now,
            },
        );
        Ok(id)
    }

    /// Returns the username of a live session and records the request time.
    /// An expired session is removed.
    pub fn touch(&self, id: &str, now: Instant) -> Option<String> {
        if id.len() != ID_HEX_LEN {
            return None;
        }
        let mut sessions = self.lock();
        let session = sessions.get_mut(id)?;
        if session.expired(&self.timeouts, now) {
            sessions.remove(id);
            return None;
        }
        session.last_seen = now;
        Some(session.username.clone())
    }

    /// True if the session exists and has not timed out. Unlike `touch`, it
    /// does not count as use of the session.
    pub fn is_live(&self, id: &str, now: Instant) -> bool {
        self.lock()
            .get(id)
            .is_some_and(|s| !s.expired(&self.timeouts, now))
    }

    /// Ends one session. Unknown ids are ignored.
    pub fn revoke(&self, id: &str) {
        self.lock().remove(id);
    }

    /// Ends every session of one account.
    pub fn revoke_user(&self, username: &str) {
        self.lock().retain(|_, s| s.username != username);
    }

    /// Ends every session.
    pub fn revoke_all(&self) {
        self.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        // A panic while the lock is held leaves the map in a valid state:
        // every change to it is a single insert or remove.
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn random_id() -> Result<String, AuthError> {
    crate::random_hex(ID_HEX_LEN / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);
    const HOUR: Duration = Duration::from_secs(60 * 60);

    #[test]
    fn ids_are_64_hex_characters_and_differ() {
        let a = random_id().unwrap();
        let b = random_id().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn a_new_session_is_valid() {
        let store = SessionStore::default();
        let now = Instant::now();
        let id = store.create("admin", now).unwrap();
        assert_eq!(store.touch(&id, now + MIN).as_deref(), Some("admin"));
    }

    #[test]
    fn unknown_and_malformed_ids_are_rejected() {
        let store = SessionStore::default();
        let now = Instant::now();
        store.create("admin", now).unwrap();
        assert!(store.touch("", now).is_none());
        assert!(store.touch("abc", now).is_none());
        assert!(store.touch(&"0".repeat(64), now).is_none());
    }

    #[test]
    fn idle_timeout_ends_the_session() {
        let store = SessionStore::default();
        let now = Instant::now();
        let id = store.create("admin", now).unwrap();
        assert!(store.touch(&id, now + 11 * HOUR).is_some());
        // 11 h + 11 h 59 min: each gap is under 12 h, so it stays valid.
        assert!(store.touch(&id, now + 22 * HOUR + 59 * MIN).is_some());
        // A gap of 12 h ends it.
        assert!(store.touch(&id, now + 34 * HOUR + 59 * MIN).is_none());
        assert!(store.is_empty());
    }

    #[test]
    fn absolute_timeout_ends_a_session_that_is_in_use() {
        let store = SessionStore::default();
        let start = Instant::now();
        let id = store.create("admin", start).unwrap();
        let mut now = start;
        while now + 6 * HOUR < start + ABSOLUTE_TIMEOUT {
            now += 6 * HOUR;
            assert!(store.touch(&id, now).is_some());
        }
        assert!(store.touch(&id, start + ABSOLUTE_TIMEOUT).is_none());
    }

    #[test]
    fn revoke_ends_one_session() {
        let store = SessionStore::default();
        let now = Instant::now();
        let a = store.create("admin", now).unwrap();
        let b = store.create("admin", now).unwrap();
        store.revoke(&a);
        assert!(store.touch(&a, now).is_none());
        assert!(store.touch(&b, now).is_some());
    }

    #[test]
    fn revoke_user_ends_all_sessions_of_that_account() {
        let store = SessionStore::default();
        let now = Instant::now();
        let a = store.create("admin", now).unwrap();
        let b = store.create("admin", now).unwrap();
        let other = store.create("other", now).unwrap();
        store.revoke_user("admin");
        assert!(store.touch(&a, now).is_none());
        assert!(store.touch(&b, now).is_none());
        assert!(store.touch(&other, now).is_some());
    }

    #[test]
    fn the_store_holds_a_bounded_number_of_sessions() {
        let store = SessionStore::default();
        let start = Instant::now();
        let first = store.create("admin", start).unwrap();
        let mut last = String::new();
        for i in 1..=MAX_SESSIONS as u64 {
            last = store
                .create("admin", start + Duration::from_secs(i))
                .unwrap();
        }
        assert_eq!(store.len(), MAX_SESSIONS);
        let now = start + HOUR;
        // The session unused for the longest time was dropped.
        assert!(store.touch(&first, now).is_none());
        assert!(store.touch(&last, now).is_some());
    }

    #[test]
    fn expired_sessions_are_removed_when_a_new_one_starts() {
        let store = SessionStore::default();
        let now = Instant::now();
        store.create("admin", now).unwrap();
        store.create("admin", now + 13 * HOUR).unwrap();
        assert_eq!(store.len(), 1);
    }
}
