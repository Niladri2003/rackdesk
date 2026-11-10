//! The API contract between `webdeskd` and the web UI.
//!
//! Every type here derives `TS`. Running `cargo test -p wd-api` regenerates
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

// ---------- Apps and windows ----------
// v1 only ships built-in apps, but the shape is fixed now so terminal apps,
// web apps and streamed GUI apps slot in later without a redesign.

/// How an app runs. This decides which kind of window content shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum AppKind {
    /// A React app shipped inside the UI bundle (Files, Settings, ...).
    Builtin { component: String },
    /// A terminal program run in a terminal window (v2), e.g. `btop`.
    Tui { command: Vec<String> },
    /// A web UI served by a local process and reverse-proxied into a window (v2).
    Web { port: u16 },
    /// A desktop GUI app streamed into a window (future).
    Gui { command: Vec<String> },
}

/// What an app needs from the server. The app store hides or greys out apps
/// that don't fit (v3).
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
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
}

/// Every error the API returns has this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiError {
    /// Machine-readable, e.g. "not_found", "permission_denied", "unauthorized".
    pub code: String,
    pub message: String,
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
}
