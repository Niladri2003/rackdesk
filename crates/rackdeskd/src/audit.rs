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

use crate::error::ApiErr;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use rd_api::AuditEntry;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
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
    /// The id of the sign-in session. It ties a terminal to its session.
    /// It is a secret and is never written to the log.
    pub session: String,
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

/// The view in Settings shows at most this many lines.
const VIEW_LINES: usize = 500;
/// The view reads at most this much from the end of the file.
const VIEW_BYTES: u64 = 1024 * 1024;

/// The newest lines of the log, newest first. A line that is not valid, such
/// as the first one of a part that starts in the middle of a line, is left
/// out.
pub fn read_recent(data_dir: &Path) -> std::io::Result<Vec<AuditEntry>> {
    let mut file = File::open(data_dir.join("audit.log"))?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(VIEW_BYTES)))?;
    let mut text = String::new();
    file.take(VIEW_BYTES)
        .read_to_string(&mut text)
        .or_else(|e| {
            // The part can start in the middle of a UTF-8 character.
            if e.kind() == std::io::ErrorKind::InvalidData {
                Ok(0)
            } else {
                Err(e)
            }
        })?;
    Ok(text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str(line).ok())
        .take(VIEW_LINES)
        .collect())
}

/// `GET /api/v1/audit`. The log can only be read here. No route changes it.
pub async fn recent(State(state): State<AppState>) -> Result<Json<Vec<AuditEntry>>, ApiErr> {
    let data_dir = state.config().data_dir.clone();
    let entries = tokio::task::spawn_blocking(move || read_recent(&data_dir))
        .await
        .map_err(ApiErr::internal)?
        .map_err(ApiErr::internal)?;
    Ok(Json(entries))
}
