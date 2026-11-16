//! Slows down password guessing.
//!
//! A [`Limiter`] counts failed attempts per key. The server uses 2 limiters:
//! one keyed by client IP address and one keyed by account name. After a
//! number of failures the key is locked. Each further failure doubles the
//! lock time, up to a maximum.
//!
//! Every method takes the current time as an argument. Tests pass their own
//! time and do not sleep.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A key with no failure for this long is forgotten.
pub const FORGET_AFTER: Duration = Duration::from_secs(60 * 60);
/// Upper limit on keys held at once. It bounds memory use.
pub const MAX_KEYS: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// The failure with this number starts the first lock.
    pub lock_after: u32,
    /// Length of the first lock. Each later failure doubles it.
    pub first_lock: Duration,
    /// No lock is longer than this.
    pub max_lock: Duration,
}

impl Policy {
    /// For keys that are client IP addresses: 5 failures, then 30 s, 60 s,
    /// 120 s and so on, up to 15 min.
    pub const PER_IP: Policy = Policy {
        lock_after: 5,
        first_lock: Duration::from_secs(30),
        max_lock: Duration::from_secs(15 * 60),
    };

    /// For keys that are account names. It limits guessing from many IP
    /// addresses at once. Anyone can trigger this lock for the admin, so it
    /// starts later and is short: 10 failures, then 5 s, 10 s and so on, up
    /// to 60 s.
    pub const PER_ACCOUNT: Policy = Policy {
        lock_after: 10,
        first_lock: Duration::from_secs(5),
        max_lock: Duration::from_secs(60),
    };

    fn lock_for(&self, failures: u32) -> Option<Duration> {
        if failures < self.lock_after {
            return None;
        }
        let doublings = (failures - self.lock_after).min(20);
        let lock = self.first_lock.saturating_mul(1u32 << doublings);
        Some(lock.min(self.max_lock))
    }
}

struct Entry {
    failures: u32,
    last_failure: Instant,
    locked_until: Option<Instant>,
}

impl Entry {
    fn stale(&self, now: Instant) -> bool {
        let locked = self.locked_until.is_some_and(|until| now < until);
        !locked && now.saturating_duration_since(self.last_failure) >= FORGET_AFTER
    }
}

pub struct Limiter {
    policy: Policy,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Limiter {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Call before checking a password. `Err` holds the time left until the
    /// key may try again. Do not check the password in that case.
    pub fn check(&self, key: &str, now: Instant) -> Result<(), Duration> {
        let entries = self.lock();
        match entries.get(key).and_then(|e| e.locked_until) {
            Some(until) if now < until => Err(until - now),
            _ => Ok(()),
        }
    }

    /// Records a wrong password for this key.
    pub fn failure(&self, key: &str, now: Instant) {
        let mut entries = self.lock();
        if !entries.contains_key(key) && entries.len() >= MAX_KEYS {
            entries.retain(|_, e| !e.stale(now));
            if entries.len() >= MAX_KEYS {
                // Still full: forget the key whose last failure is oldest.
                let oldest = entries
                    .iter()
                    .min_by_key(|(_, e)| e.last_failure)
                    .map(|(k, _)| k.clone());
                if let Some(k) = oldest {
                    entries.remove(&k);
                }
            }
        }
        let entry = entries.entry(key.to_owned()).or_insert(Entry {
            failures: 0,
            last_failure: now,
            locked_until: None,
        });
        if entry.stale(now) {
            entry.failures = 0;
        }
        entry.failures = entry.failures.saturating_add(1);
        entry.last_failure = now;
        entry.locked_until = self.policy.lock_for(entry.failures).map(|d| now + d);
    }

    /// Records a correct password: the key starts again from 0 failures.
    pub fn success(&self, key: &str) {
        self.lock().remove(key);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        // A panic while the lock is held leaves the map in a valid state.
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    fn fail_times(l: &Limiter, key: &str, n: u32, now: Instant) {
        for _ in 0..n {
            l.failure(key, now);
        }
    }

    #[test]
    fn failures_below_the_limit_do_not_lock() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        fail_times(&l, "10.0.0.1", 4, now);
        assert!(l.check("10.0.0.1", now).is_ok());
    }

    #[test]
    fn the_fifth_failure_locks_for_30_seconds() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        fail_times(&l, "10.0.0.1", 5, now);
        assert_eq!(l.check("10.0.0.1", now), Err(30 * SEC));
        assert_eq!(l.check("10.0.0.1", now + 10 * SEC), Err(20 * SEC));
        assert!(l.check("10.0.0.1", now + 30 * SEC).is_ok());
    }

    #[test]
    fn each_further_failure_doubles_the_lock_up_to_the_maximum() {
        let l = Limiter::new(Policy::PER_IP);
        let mut now = Instant::now();
        fail_times(&l, "k", 5, now);
        let mut seen = Vec::new();
        for _ in 0..8 {
            let wait = l.check("k", now).unwrap_err();
            seen.push(wait.as_secs());
            now += wait;
            l.failure("k", now);
        }
        assert_eq!(seen, [30, 60, 120, 240, 480, 900, 900, 900]);
    }

    #[test]
    fn keys_are_independent() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        fail_times(&l, "10.0.0.1", 5, now);
        assert!(l.check("10.0.0.1", now).is_err());
        assert!(l.check("10.0.0.2", now).is_ok());
    }

    #[test]
    fn success_clears_the_count() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        fail_times(&l, "k", 4, now);
        l.success("k");
        fail_times(&l, "k", 4, now);
        assert!(l.check("k", now).is_ok());
    }

    #[test]
    fn old_failures_are_forgotten() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        fail_times(&l, "k", 4, now);
        let later = now + FORGET_AFTER;
        l.failure("k", later);
        assert!(l.check("k", later).is_ok());
    }

    #[test]
    fn the_account_policy_locks_later_and_for_a_shorter_time() {
        let l = Limiter::new(Policy::PER_ACCOUNT);
        let mut now = Instant::now();
        fail_times(&l, "admin", 9, now);
        assert!(l.check("admin", now).is_ok());
        l.failure("admin", now);
        let mut seen = Vec::new();
        for _ in 0..6 {
            let wait = l.check("admin", now).unwrap_err();
            seen.push(wait.as_secs());
            now += wait;
            l.failure("admin", now);
        }
        assert_eq!(seen, [5, 10, 20, 40, 60, 60]);
    }

    #[test]
    fn the_number_of_keys_is_bounded() {
        let l = Limiter::new(Policy::PER_IP);
        let now = Instant::now();
        for i in 0..(MAX_KEYS + 100) {
            l.failure(&format!("key-{i}"), now + Duration::from_millis(i as u64));
        }
        assert_eq!(l.lock().len(), MAX_KEYS);
    }

    #[test]
    fn a_very_high_failure_count_does_not_overflow() {
        let p = Policy::PER_IP;
        assert_eq!(p.lock_for(u32::MAX), Some(p.max_lock));
    }
}
