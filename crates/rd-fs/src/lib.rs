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

mod archive;
mod perms;
mod search;
mod trash;
mod write;

use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
pub use rd_api::EntryKind;
use rd_api::{DirEntry, DirListing, SortKey};
use std::io;
use std::path::{Path, PathBuf};

pub use archive::ArchiveLimits;
pub use perms::PLAIN_BITS;
pub use search::SearchLimits;
pub use write::MAX_COPY_ENTRIES;

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
    #[error("not a regular file")]
    NotAFile,
    #[error("the name is already in use")]
    AlreadyExists,
    #[error("the file was changed by someone else")]
    ChangedOnDisk,
    #[error("the target is on another disk")]
    CrossDevice,
    #[error("too many entries, or too many bytes")]
    TooMany,
    #[error("the archive can't be read: {0}")]
    BadArchive(&'static str),
    /// An upload chunk does not start where the file ends. Holds the size
    /// the file has now.
    #[error("the chunk does not start at the end of the file")]
    OffsetMismatch(u64),
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
            FsError::NotAFile => "not_a_file",
            FsError::AlreadyExists => "already_exists",
            FsError::ChangedOnDisk => "changed_on_disk",
            FsError::CrossDevice => "cross_device",
            FsError::TooMany => "too_many",
            FsError::BadArchive(_) => "bad_archive",
            FsError::OffsetMismatch(_) => "offset_mismatch",
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
            io::ErrorKind::AlreadyExists => FsError::AlreadyExists,
            io::ErrorKind::CrossesDevices => FsError::CrossDevice,
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

/// How to list a folder.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub offset: u32,
    /// Entries on the page. Clamped to 1..=[`MAX_PAGE`].
    pub limit: u32,
    pub sort: SortKey,
    pub descending: bool,
    /// Include entries whose name starts with a dot.
    pub show_hidden: bool,
    /// Keep only entries whose name contains this text, without regard to
    /// letter case.
    pub filter: Option<String>,
}

impl ListOptions {
    pub fn page(offset: u32, limit: u32) -> Self {
        Self {
            offset,
            limit,
            show_hidden: true,
            ..Self::default()
        }
    }
}

/// An open regular file, ready to be read.
pub struct OpenFile {
    pub file: std::fs::File,
    /// Bytes.
    pub len: u64,
    /// Unix seconds.
    pub modified: Option<i64>,
    /// Changes when the size or the modified time of the file changes. Used
    /// as the HTTP `ETag`, and to see that a file was not changed between a
    /// read and a save.
    pub version: String,
}

/// One entry while a listing is sorted. `meta` is read only when the sort
/// order needs it, or when the entry is on the page that is returned.
struct Listed {
    name: String,
    kind: EntryKind,
    meta: Option<cap_std::fs::Metadata>,
}

/// The directory tree the UI is allowed to see.
pub struct Root {
    dir: Dir,
    /// The trash folder, as a path beneath the root.
    trash: PathBuf,
}

impl Root {
    pub fn open(path: &Path) -> Result<Self, FsError> {
        let dir = Dir::open_ambient_dir(path, ambient_authority())?;
        Ok(Self {
            dir,
            trash: trash::default_dir(),
        })
    }

    /// Lists one page of a folder. Folders come first, then the sort order
    /// from `opts`.
    ///
    /// Each call reads all names in the folder. A sort by name needs no
    /// `stat` call except for the entries on the page, so a folder with
    /// 100,000 entries lists fast. A sort by size or time reads the metadata
    /// of every entry.
    pub fn list(&self, ui_path: &str, opts: &ListOptions) -> Result<DirListing, FsError> {
        let rel = clean_path(ui_path)?;
        let dir = self.dir.open_dir(&rel)?;
        let filter = opts.filter.as_deref().map(str::to_lowercase);
        let need_meta = opts.sort != SortKey::Name;

        let mut entries = Vec::new();
        for item in dir.entries()? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !opts.show_hidden && name.starts_with('.') {
                continue;
            }
            if let Some(f) = &filter {
                if !name.to_lowercase().contains(f) {
                    continue;
                }
            }
            // The file type comes with the folder read on most filesystems.
            // It describes a symlink itself and never follows it.
            let kind = match item.file_type() {
                Ok(ft) => kind_of(ft),
                // Deleted between the folder read and now: skip it.
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let meta = if need_meta {
                match dir.symlink_metadata(&name) {
                    Ok(m) => Some(m),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.into()),
                }
            } else {
                None
            };
            entries.push(Listed { name, kind, meta });
        }

        match opts.sort {
            SortKey::Name => entries.sort_by_cached_key(|e| e.name.to_lowercase()),
            SortKey::Size => entries.sort_by_cached_key(|e| {
                let size = e.meta.as_ref().map_or(0, |m| m.len());
                (size, e.name.to_lowercase())
            }),
            SortKey::Modified => entries.sort_by_cached_key(|e| {
                let time = e.meta.as_ref().and_then(modified_secs).unwrap_or(0);
                (time, e.name.to_lowercase())
            }),
        }
        if opts.descending {
            entries.reverse();
        }
        // A stable sort keeps the order above inside each of the 2 groups.
        entries.sort_by_key(|e| e.kind != EntryKind::Dir);

        let total = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        let limit = opts.limit.clamp(1, MAX_PAGE) as usize;
        let mut page = Vec::with_capacity(limit.min(entries.len()));
        for e in entries.into_iter().skip(opts.offset as usize).take(limit) {
            let meta = match e.meta {
                Some(m) => m,
                None => match dir.symlink_metadata(&e.name) {
                    Ok(m) => m,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                    Err(err) => return Err(err.into()),
                },
            };
            page.push(to_entry(&dir, e.name, &meta));
        }
        Ok(DirListing {
            path: display_path(&rel),
            entries: page,
            total,
            offset: opts.offset,
        })
    }

    /// Describes one file or folder. A symlink is described, not followed.
    pub fn entry(&self, ui_path: &str) -> Result<DirEntry, FsError> {
        let rel = clean_path(ui_path)?;
        if rel == Path::new(".") {
            let meta = self.dir.dir_metadata()?;
            return Ok(to_entry(&self.dir, "/".into(), &meta));
        }
        let name = rel
            .file_name()
            .ok_or(FsError::InvalidPath("has no file name"))?
            .to_string_lossy()
            .into_owned();
        let parent = match rel.parent() {
            Some(p) if !p.as_os_str().is_empty() => self.dir.open_dir(p)?,
            _ => self.dir.try_clone()?,
        };
        let meta = parent.symlink_metadata(&name)?;
        Ok(to_entry(&parent, name, &meta))
    }

    /// Opens a regular file for reading. A symlink is followed only while it
    /// stays beneath the root.
    pub fn open_file(&self, ui_path: &str) -> Result<OpenFile, FsError> {
        let rel = clean_path(ui_path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            // Without this flag, opening a named pipe waits until another
            // program writes to it. The flag has no effect on regular files.
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
        }
        let file = self.dir.open_with(&rel, &options)?;
        // Checked on the open file, so the answer can't change afterwards.
        let meta = file.metadata()?;
        // Folders, devices, pipes and sockets are never read.
        if !meta.is_file() {
            return Err(FsError::NotAFile);
        }
        Ok(OpenFile {
            len: meta.len(),
            modified: modified_secs(&meta),
            version: version_of(&meta),
            file: file.into_std(),
        })
    }

    /// Total and free bytes of the filesystem that holds `ui_path`.
    pub fn disk_usage(&self, ui_path: &str) -> Result<(u64, u64), FsError> {
        let rel = clean_path(ui_path)?;
        let dir = self.dir.open_dir(&rel)?;
        let stat = rustix::fs::fstatvfs(&dir).map_err(|e| FsError::Io(e.into()))?;
        let block = stat.f_frsize;
        Ok((
            stat.f_blocks.saturating_mul(block),
            stat.f_bavail.saturating_mul(block),
        ))
    }
}

fn display_path(rel: &Path) -> String {
    if rel == Path::new(".") {
        "/".into()
    } else {
        format!("/{}", rel.to_string_lossy())
    }
}

fn kind_of(ft: cap_std::fs::FileType) -> EntryKind {
    if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

/// A short text that identifies one version of a file: its size and its
/// modified time, to the nanosecond.
fn version_of(meta: &cap_std::fs::Metadata) -> String {
    let (secs, nanos) = meta
        .modified()
        .ok()
        .map(|t| t.into_std())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or((0, 0), |d| (d.as_secs(), d.subsec_nanos()));
    format!("{:x}-{secs:x}-{nanos:x}", meta.len())
}

fn modified_secs(meta: &cap_std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .map(|t| t.into_std())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
}

/// `dir` is the folder that holds the entry. It is used to find what a
/// symlink points to, and cap-std refuses a target outside the root.
fn to_entry(dir: &Dir, name: String, meta: &cap_std::fs::Metadata) -> DirEntry {
    let kind = kind_of(meta.file_type());
    let target = if kind == EntryKind::Symlink {
        dir.metadata(&name).ok().map(|m| kind_of(m.file_type()))
    } else {
        None
    };
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
        modified: modified_secs(meta),
        mode,
        target,
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
        let l = r.list("/", &ListOptions::page(0, 100)).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["docs", "Projects", "A.txt", "b.txt"]);
        assert_eq!(l.total, 4);
        assert_eq!(l.path, "/");
    }

    #[test]
    fn pages() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let l = r.list("/", &ListOptions::page(1, 2)).unwrap();
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[0].name, "Projects");
        assert_eq!(l.total, 4);
    }

    #[test]
    fn missing_folder_is_not_found() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        assert!(matches!(
            r.list("/nope", &ListOptions::page(0, 10)),
            Err(FsError::NotFound)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_root_is_listed_but_not_followed() {
        let (root, outside) = fixture();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        let r = Root::open(root.path()).unwrap();

        // The link shows up as a symlink...
        let l = r.list("/", &ListOptions::page(0, 100)).unwrap();
        let link = l.entries.iter().find(|e| e.name == "escape").unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);

        // ...but opening it as a folder must not reach the outside directory.
        assert!(r.list("/escape", &ListOptions::page(0, 10)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn absolute_symlink_to_root_does_not_escape() {
        let (root, _o) = fixture();
        std::os::unix::fs::symlink("/", root.path().join("slash")).unwrap();
        let r = Root::open(root.path()).unwrap();
        assert!(r.list("/slash/etc", &ListOptions::page(0, 10)).is_err());
    }

    fn names(l: &DirListing) -> Vec<&str> {
        l.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn hidden_entries_are_left_out_unless_asked_for() {
        let (root, _o) = fixture();
        fs::write(root.path().join(".env"), "x").unwrap();
        let r = Root::open(root.path()).unwrap();
        let mut opts = ListOptions::page(0, 100);
        assert!(names(&r.list("/", &opts).unwrap()).contains(&".env"));
        opts.show_hidden = false;
        let l = r.list("/", &opts).unwrap();
        assert!(!names(&l).contains(&".env"));
        assert_eq!(l.total, 4);
    }

    #[test]
    fn the_filter_matches_a_part_of_the_name_without_regard_to_case() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let mut opts = ListOptions::page(0, 100);
        opts.filter = Some("TXT".into());
        let l = r.list("/", &opts).unwrap();
        assert_eq!(names(&l), ["A.txt", "b.txt"]);
        assert_eq!(l.total, 2);
    }

    #[test]
    fn sorts_by_size_with_folders_first() {
        let (root, _o) = fixture();
        fs::write(root.path().join("big.bin"), vec![0u8; 500]).unwrap();
        let r = Root::open(root.path()).unwrap();
        let mut opts = ListOptions::page(0, 100);
        opts.sort = SortKey::Size;
        assert_eq!(
            names(&r.list("/", &opts).unwrap()),
            ["docs", "Projects", "A.txt", "b.txt", "big.bin"]
        );
        opts.descending = true;
        assert_eq!(
            names(&r.list("/", &opts).unwrap())[2..],
            ["big.bin", "b.txt", "A.txt"]
        );
    }

    #[test]
    fn sorts_by_modified_time() {
        let (root, _o) = fixture();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(root.path().join("b.txt"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        let r = Root::open(root.path()).unwrap();
        let mut opts = ListOptions::page(0, 100);
        opts.sort = SortKey::Modified;
        let l = r.list("/", &opts).unwrap();
        assert_eq!(names(&l)[2..], ["b.txt", "A.txt"]);
        assert_eq!(l.entries[2].modified, Some(1_000_000));
    }

    #[test]
    fn entry_describes_a_file_a_folder_and_the_root() {
        let (root, _o) = fixture();
        fs::write(root.path().join("docs/guide.md"), "hello").unwrap();
        let r = Root::open(root.path()).unwrap();
        let file = r.entry("/docs/guide.md").unwrap();
        assert_eq!(
            (file.name.as_str(), file.kind, file.size),
            ("guide.md", EntryKind::File, 5)
        );
        assert_eq!(r.entry("/docs").unwrap().kind, EntryKind::Dir);
        assert_eq!(r.entry("/").unwrap().kind, EntryKind::Dir);
        assert!(matches!(r.entry("/nope"), Err(FsError::NotFound)));
        assert!(matches!(r.entry("/../x"), Err(FsError::InvalidPath(_))));
    }

    #[test]
    fn open_file_reads_a_regular_file() {
        use std::io::Read;
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let mut open = r.open_file("/b.txt").unwrap();
        assert_eq!(open.len, 2);
        let mut text = String::new();
        open.file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "bb");
    }

    #[test]
    fn open_file_refuses_folders_and_bad_paths() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        assert!(matches!(r.open_file("/docs"), Err(FsError::NotAFile)));
        assert!(matches!(r.open_file("/nope.txt"), Err(FsError::NotFound)));
        assert!(matches!(
            r.open_file("/../b.txt"),
            Err(FsError::InvalidPath(_))
        ));
        assert!(r.open_file("/etc/passwd").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn open_file_does_not_follow_a_symlink_out_of_the_root() {
        let (root, outside) = fixture();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            root.path().join("link.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        std::os::unix::fs::symlink("/", root.path().join("slash")).unwrap();
        let r = Root::open(root.path()).unwrap();
        assert!(r.open_file("/link.txt").is_err());
        assert!(r.open_file("/escape/secret.txt").is_err());
        assert!(r.open_file("/slash/etc/hosts").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn open_file_follows_a_symlink_that_stays_inside() {
        let (root, _o) = fixture();
        std::os::unix::fs::symlink("b.txt", root.path().join("alias.txt")).unwrap();
        let r = Root::open(root.path()).unwrap();
        assert_eq!(r.open_file("/alias.txt").unwrap().len, 2);
        let e = r.entry("/alias.txt").unwrap();
        assert_eq!(
            (e.kind, e.target),
            (EntryKind::Symlink, Some(EntryKind::File))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_root_has_no_target_kind() {
        let (root, outside) = fixture();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        let r = Root::open(root.path()).unwrap();
        let l = r.list("/", &ListOptions::page(0, 100)).unwrap();
        let link = l.entries.iter().find(|e| e.name == "escape").unwrap();
        assert_eq!((link.kind, link.target), (EntryKind::Symlink, None));
    }

    #[cfg(unix)]
    #[test]
    fn open_file_refuses_a_named_pipe_without_waiting() {
        let (root, _o) = fixture();
        let pipe = root.path().join("pipe");
        // `mkfifo` is on every Unix system. No safe Rust API makes a pipe
        // file on both Linux and macOS.
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap();
        assert!(made.success());
        let r = Root::open(root.path()).unwrap();
        assert!(matches!(r.open_file("/pipe"), Err(FsError::NotAFile)));
    }

    #[test]
    fn disk_usage_reports_free_space_within_the_total() {
        let (root, _o) = fixture();
        let r = Root::open(root.path()).unwrap();
        let (total, free) = r.disk_usage("/").unwrap();
        assert!(total > 0);
        assert!(free <= total);
        assert!(r.disk_usage("/../..").is_err());
    }

    /// The roadmap target: a folder with 100,000 entries lists in under 1 s.
    /// Slow to set up, so it runs only on request:
    /// `cargo test -p rd-fs --release -- --ignored`.
    #[test]
    #[ignore]
    fn lists_100_000_entries_in_under_a_second() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..100_000 {
            fs::write(root.path().join(format!("file-{i:06}.log")), "").unwrap();
        }
        let r = Root::open(root.path()).unwrap();
        for sort in [SortKey::Name, SortKey::Size, SortKey::Modified] {
            let mut opts = ListOptions::page(50_000, 200);
            opts.sort = sort;
            let start = std::time::Instant::now();
            let l = r.list("/", &opts).unwrap();
            let took = start.elapsed();
            println!("sort by {sort:?}: {took:?}");
            assert_eq!((l.total, l.entries.len()), (100_000, 200));
            assert!(
                took < std::time::Duration::from_secs(1),
                "{sort:?} took {took:?}"
            );
        }
    }
}
