//! Every file operation the UI can trigger goes through [`Root`].
//!
//! `Root` wraps a `cap_std::fs::Dir`. cap-std resolves each path *beneath*
//! that directory: `..` can't climb out, absolute paths are rejected, and
//! symlinks that point outside fail (on Linux it uses `openat2` with
//! `RESOLVE_BENEATH`). So a bug in the HTTP layer can't turn into
//! "read /etc/shadow". See docs/adr/0002-cap-std-for-paths.md.
//!
//! On top of that, [`clean_path`] rejects odd input early (NUL bytes,
//! backslashes, `..` components), so errors are clear and logs are readable.

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use rd_api::{DirEntry, DirListing, EntryKind};
use std::io;
use std::path::{Path, PathBuf};

/// Hard cap on one page of a listing. The UI pages through big folders.
pub const MAX_PAGE: u32 = 5_000;

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("invalid path: {0}")]
    InvalidPath(&'static str),
    #[error("not found")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("not a directory")]
    NotADirectory,
    #[error("io error: {0}")]
    Io(io::Error),
}

impl FsError {
    /// The `ApiError.code` for this error.
    pub fn code(&self) -> &'static str {
        match self {
            FsError::InvalidPath(_) => "invalid_path",
            FsError::NotFound => "not_found",
            FsError::PermissionDenied => "permission_denied",
            FsError::NotADirectory => "not_a_directory",
            FsError::Io(_) => "io_error",
        }
    }
}

impl From<io::Error> for FsError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => FsError::NotFound,
            io::ErrorKind::PermissionDenied => FsError::PermissionDenied,
            io::ErrorKind::NotADirectory => FsError::NotADirectory,
            _ => FsError::Io(e),
        }
    }
}

/// Turns a UI path like "/var/log/../log/nginx" into a relative path that
/// cap-std can resolve beneath the root. "/" becomes ".".
pub fn clean_path(ui_path: &str) -> Result<PathBuf, FsError> {
    if ui_path.contains('\0') {
        return Err(FsError::InvalidPath("contains a NUL byte"));
    }
    if ui_path.contains('\\') {
        return Err(FsError::InvalidPath("contains a backslash"));
    }
    if ui_path.len() > 4096 {
        return Err(FsError::InvalidPath("longer than 4096 bytes"));
    }
    let mut out = PathBuf::new();
    for part in ui_path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(FsError::InvalidPath("contains '..'")),
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    Ok(out)
}

/// The directory tree the UI is allowed to see.
pub struct Root {
    dir: Dir,
}

impl Root {
    pub fn open(path: &Path) -> Result<Self, FsError> {
        let dir = Dir::open_ambient_dir(path, ambient_authority())?;
        Ok(Self { dir })
    }

    /// Lists one page of a folder: folders first, then case-insensitive by name.
    ///
    /// This reads the whole folder to sort it. That's fine up to ~100k
    /// entries. A larger folder needs a cached, sorted index.
    pub fn list(&self, ui_path: &str, offset: u32, limit: u32) -> Result<DirListing, FsError> {
        let rel = clean_path(ui_path)?;
        let dir = self.dir.open_dir(&rel)?;
        let mut entries = Vec::new();
        for item in dir.entries()? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            // symlink_metadata semantics: describe the link itself, never follow it here.
            let meta = match dir.symlink_metadata(&name) {
                Ok(m) => m,
                // Deleted between readdir and stat: skip it.
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            entries.push(to_entry(name, &meta));
        }
        entries.sort_by(|a, b| {
            (a.kind != EntryKind::Dir)
                .cmp(&(b.kind != EntryKind::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        let total = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        let limit = limit.clamp(1, MAX_PAGE) as usize;
        let page = entries
            .into_iter()
            .skip(offset as usize)
            .take(limit)
            .collect();
        Ok(DirListing {
            path: display_path(&rel),
            entries: page,
            total,
            offset,
        })
    }
}

fn display_path(rel: &Path) -> String {
    if rel == Path::new(".") {
        "/".into()
    } else {
        format!("/{}", rel.to_string_lossy())
    }
}

fn to_entry(name: String, meta: &cap_std::fs::Metadata) -> DirEntry {
    let ft = meta.file_type();
    let kind = if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    let modified = meta
        .modified()
        .ok()
        .map(|t| t.into_std())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    #[cfg(unix)]
    let mode = {
        use cap_std::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = None;
    DirEntry {
        name,
        kind,
        size: meta.len(),
        modified,
        mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn clean_path_cases() {
        assert_eq!(clean_path("/").unwrap(), PathBuf::from("."));
        assert_eq!(clean_path("").unwrap(), PathBuf::from("."));
        assert_eq!(
            clean_path("/var//log/./nginx/").unwrap(),
            PathBuf::from("var/log/nginx")
        );
        assert!(clean_path("/var/../etc").is_err());
        assert!(clean_path("..").is_err());
        assert!(clean_path("a\0b").is_err());
        assert!(clean_path("a\\..\\b").is_err());
    }

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        fs::create_dir(root.path().join("Projects")).unwrap();
        fs::create_dir(root.path().join("docs")).unwrap();
        fs::write(root.path().join("b.txt"), "bb").unwrap();
        fs::write(root.path().join("A.txt"), "a").unwrap();
        (root, outside)
    }

    #[test]
    fn lists_folders_first_then_by_name() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let l = r.list("/", 0, 100).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["docs", "Projects", "A.txt", "b.txt"]);
        assert_eq!(l.total, 4);
        assert_eq!(l.path, "/");
    }

    #[test]
    fn pages() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let l = r.list("/", 1, 2).unwrap();
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[0].name, "Projects");
        assert_eq!(l.total, 4);
    }

    #[test]
    fn missing_folder_is_not_found() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        assert!(matches!(r.list("/nope", 0, 10), Err(FsError::NotFound)));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_root_is_listed_but_not_followed() {
        let (root, outside) = fixture();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        let r = Root::open(root.path()).unwrap();

        // The link shows up as a symlink...
        let l = r.list("/", 0, 100).unwrap();
        let link = l.entries.iter().find(|e| e.name == "escape").unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);

        // ...but opening it as a folder must not reach the outside directory.
        assert!(r.list("/escape", 0, 10).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn absolute_symlink_to_root_does_not_escape() {
        let (root, _o) = fixture();
        std::os::unix::fs::symlink("/", root.path().join("slash")).unwrap();
        let r = Root::open(root.path()).unwrap();
        assert!(r.list("/slash/etc", 0, 10).is_err());
    }
}
