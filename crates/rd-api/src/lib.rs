//! The API contract between `rackdeskd` and the web UI.
//!
//! Every type here derives `TS`. Running `cargo test -p rd-api` regenerates
//! `web/src/api/generated/*.ts`, so the frontend can't drift from the server.
//! No I/O and no logic beyond small helpers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ---------- Files ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

/// One row in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    /// Bytes. Sent as a number; JS loses precision only above 8 PiB.
    #[ts(type = "number")]
    pub size: u64,
    /// Unix seconds, if the filesystem reports it.
    #[ts(type = "number | null")]
    pub modified: Option<i64>,
    /// Permission bits, e.g. 0o755 (Unix only).
    pub mode: Option<u32>,
    /// For a symlink: the kind of the entry it points to. `None` for other
    /// kinds, for a broken link, and for a link that leaves the files root.
    pub target: Option<EntryKind>,
}

/// The column a listing is sorted by. Folders always come before files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
}

/// A page of a directory listing. Large folders are paged, never sent whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirListing {
    /// Path relative to the files root, always starting with "/".
    pub path: String,
    pub entries: Vec<DirEntry>,
    /// Total number of entries in the folder (for "100,000 items" and scrollbars).
    pub total: u32,
    pub offset: u32,
}

/// What to do when the target name of a copy, move, upload or restore is
/// already in use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Conflict {
    /// Refuse with the error code `already_exists`.
    #[default]
    Fail,
    /// Remove what is there and take its place.
    Replace,
    /// Keep what is there and use a free name, such as "notes (1).txt".
    KeepBoth,
}

/// Body of `POST /api/v1/fs/mkdir`, `/fs/delete` and `/fs/trash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PathRequest {
    pub path: String,
}

/// Body of `POST /api/v1/fs/rename` (also used to move) and `/fs/copy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TransferRequest {
    pub from: String,
    /// The full target path, with the new name as its last part.
    pub to: String,
    #[serde(default)]
    pub conflict: Conflict,
}

/// The answer to a change that can end at another path than the one asked
/// for, because of [`Conflict::KeepBoth`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PathResult {
    pub path: String,
}

/// One entry in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrashItem {
    /// The name inside the trash. It identifies the item.
    pub id: String,
    /// Where the entry was before it was moved to the trash.
    pub original_path: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub deleted_at: i64,
    pub entry: DirEntry,
}

/// Body of `POST /api/v1/fs/trash/restore` and `/fs/trash/delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrashRequest {
    pub id: String,
    #[serde(default)]
    pub conflict: Conflict,
}

/// The answer to `PUT /api/v1/fs/file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SaveResult {
    /// The new version tag of the file. Send it as `If-Match` with the next
    /// save.
    pub etag: String,
}

/// Body of `POST /api/v1/fs/uploads`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UploadRequest {
    /// The folder that receives the file.
    pub dir: String,
    pub name: String,
    /// Full size of the file in bytes.
    #[ts(type = "number")]
    pub size: u64,
    #[serde(default)]
    pub conflict: Conflict,
}

/// The state of an upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UploadStatus {
    pub id: String,
    /// Bytes the server has. The next chunk must start here.
    #[ts(type = "number")]
    pub offset: u64,
    #[ts(type = "number")]
    pub size: u64,
    /// Set when the upload is complete: the path of the finished file.
    pub path: Option<String>,
}

/// An entry with its owner, for the properties dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EntryDetails {
    pub entry: DirEntry,
    /// Numeric ids of the owner and the group (Unix only).
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Their names, if the server knows them.
    pub owner: Option<String>,
    pub group: Option<String>,
    /// True if this server has a root program for actions that need root. The UI
    /// then offers to try a refused change again with `elevate`.
    pub can_elevate: bool,
}

/// Body of `POST /api/v1/fs/chmod`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChmodRequest {
    pub path: String,
    /// The 9 plain permission bits, such as 0o644 (420 as a number).
    pub mode: u32,
    /// Do it through the root program.
    #[serde(default)]
    pub elevate: bool,
}

/// Body of `POST /api/v1/fs/chown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChownRequest {
    pub path: String,
    /// A user name or a numeric id. `None` leaves the owner as it is.
    pub owner: Option<String>,
    /// A group name or a numeric id. `None` leaves the group as it is.
    pub group: Option<String>,
    /// Do it through the root program.
    #[serde(default)]
    pub elevate: bool,
}

/// A filesystem, with its use. Shown in the sidebar of the file browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Disk {
    /// Where it is mounted, as a path beneath the files root.
    pub path: String,
    /// Bytes.
    #[ts(type = "number")]
    pub total: u64,
    /// Bytes that an unprivileged user can still write.
    #[ts(type = "number")]
    pub free: u64,
}

/// Starting points for the file browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Places {
    /// The home folder of the user that runs the server, if it is beneath
    /// the files root.
    pub home: Option<String>,
    pub disks: Vec<Disk>,
}

/// One file or folder found by a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchHit {
    /// The folder that holds the entry.
    pub dir: String,
    pub entry: DirEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// True if the search stopped at a limit (time, folders visited or
    /// number of hits) before it had walked the whole tree.
    pub truncated: bool,
}

// ---------- Apps and windows ----------
// Only built-in apps exist today. The other kinds are defined now so that
// adding terminal apps, web apps and streamed GUI apps does not change the
// shape of the API.

/// How an app runs. This decides which kind of window content shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum AppKind {
    /// A React app shipped inside the UI bundle (Files, Settings, ...).
    Builtin { component: String },
    /// A terminal program run in a terminal window, e.g. `btop`.
    Tui { command: Vec<String> },
    /// A web UI served by a local process and reverse-proxied into a window.
    Web { port: u16 },
    /// A desktop GUI app streamed into a window.
    Gui { command: Vec<String> },
}

/// What an app needs from the server. The app store hides or greys out apps
/// that don't fit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Requirements {
    #[ts(type = "number | null")]
    pub min_memory_bytes: Option<u64>,
    pub min_cpus: Option<u32>,
    pub gpu: GpuNeed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GpuNeed {
    #[default]
    None,
    Recommended,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppManifest {
    /// Stable id, e.g. "files".
    pub id: String,
    pub name: String,
    /// Lucide icon name, or a URL for store apps.
    pub icon: String,
    #[serde(flatten)]
    pub kind: AppKind,
    #[serde(default)]
    pub requirements: Requirements,
}

// ---------- Session ----------

/// Body of `POST /api/v1/session`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Body of `POST /api/v1/session/password`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// `Debug` is written by hand for the 2 types above, so a password can't reach
// a log line through `{:?}`.
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"<hidden>")
            .finish()
    }
}

impl std::fmt::Debug for ChangePasswordRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChangePasswordRequest")
            .field("current_password", &"<hidden>")
            .field("new_password", &"<hidden>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionInfo {
    pub username: String,
    /// True until the generated initial password has been replaced.
    pub must_change_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServerInfo {
    pub hostname: String,
    pub version: String,
    /// True if the terminal app can be used on this server.
    pub terminal: bool,
}

/// One line of the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditEntry {
    /// UTC, as "2026-11-17T10:00:00Z".
    pub time: String,
    pub user: String,
    pub ip: String,
    /// What was done, such as "fs.rename" or "session.sign_in".
    pub action: String,
    /// What it was done to.
    pub target: String,
    /// "ok", or the error code of a refused or failed request.
    pub result: String,
}

/// Every error the API returns has this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiError {
    /// Machine-readable, e.g. "not_found", "permission_denied", "unauthorized".
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_manifest_json_shape() {
        let m = AppManifest {
            id: "files".into(),
            name: "Files".into(),
            icon: "folder".into(),
            kind: AppKind::Builtin {
                component: "Files".into(),
            },
            requirements: Requirements::default(),
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["kind"], "builtin");
        assert_eq!(v["component"], "Files");
        let back: AppManifest = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn debug_output_hides_passwords() {
        let login = LoginRequest {
            username: "admin".into(),
            password: "secret-one".into(),
        };
        let change = ChangePasswordRequest {
            current_password: "secret-two".into(),
            new_password: "secret-three".into(),
        };
        let text = format!("{login:?} {change:?}");
        assert!(text.contains("admin"));
        assert!(!text.contains("secret"));
    }
}
