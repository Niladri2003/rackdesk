//! The audit log: a record of who changed what.
//!
//! Every change to files, every root action through the helper, and every
//! sign-in, sign-out and password change adds 1 line to
//! `<data_dir>/audit.log`. A line is a JSON object, so it is easy to search
//! and to parse. Lines are only added. The server never rewrites or
//! shortens the file (docs/security/SECURITY.md, P2).
//!
//! A line holds paths and names. It never holds a password, a session id or
//! the content of a file.

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Who made a request. The session check puts it on the request.
#[derive(Debug, Clone)]
pub struct Actor {
    pub username: String,
    pub ip: IpAddr,
}

#[derive(Serialize)]
struct Line<'a> {
    /// UTC, as "2026-11-17T10:00:00Z".
    time: String,
    user: &'a str,
    ip: String,
    /// What was done, such as "fs.rename" or "session.sign_in".
    action: &'a str,
    /// What it was done to: a path, or 2 paths for a move or copy.
    target: &'a str,
    /// "ok", or the error code of a refused or failed request.
    result: &'a str,
}

pub struct AuditLog {
    file: Mutex<File>,
}

impl AuditLog {
    /// Opens the log for appending, and creates it for its owner only.
    pub fn open(data_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let mut options = OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Self {
            file: Mutex::new(options.open(data_dir.join("audit.log"))?),
        })
    }

    /// Adds 1 line. `result` is `Ok` or the error code.
    ///
    /// A line is written with 1 `write` call to a file opened for append, so
    /// lines from 2 requests can't mix. The call is short and is not moved
    /// off the async runtime.
    pub fn record(&self, actor: &Actor, action: &str, target: &str, result: Result<(), &str>) {
        let line = Line {
            time: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
            user: &actor.username,
            ip: actor.ip.to_string(),
            action,
            target,
            result: result.err().unwrap_or("ok"),
        };
        let Ok(mut text) = serde_json::to_string(&line) else {
            return;
        };
        text.push('\n');
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = file.write_all(text.as_bytes()) {
            // The request itself has been done. Say that its record is lost.
            tracing::error!(error = %e, "could not write to the audit log");
        }
    }
}
