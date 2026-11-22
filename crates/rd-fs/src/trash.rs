//! The trash: delete with a way back.
//!
//! The layout follows the freedesktop.org trash specification. The trash
//! folder holds `files/`, with the trashed entries, and `info/`, with 1
//! `.trashinfo` file per entry that records where it came from.
//!
//! Moving to the trash is a rename, so it is instant and uses no space. A
//! rename can't cross to another disk. An entry on another disk than the
//! trash folder is refused with `CrossDevice`, and the UI then offers to
//! delete it for good.
//!
//! One difference from the specification: `Path=` holds the path beneath
//! the files root, not the absolute path on the server. They are the same
//! when the files root is "/".

use crate::{clean_path, to_entry, FsError, Root};
use cap_std::fs::{Dir, OpenOptions};
use rd_api::{Conflict, TrashItem};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const INFO_SUFFIX: &str = ".trashinfo";

/// The trash folder when the caller sets none: `.Trash-<uid>` at the top of
/// the files root, as the specification names it for a disk.
pub(crate) fn default_dir() -> PathBuf {
    #[cfg(unix)]
    let uid = rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let uid = 0;
    PathBuf::from(format!(".Trash-{uid}"))
}

/// Percent-encodes a path for a `.trashinfo` file. "/" stays as it is.
fn encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// "2026-11-17T01:02:03" in UTC, for the `DeletionDate=` line.
fn timestamp(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    // Days since 1970-01-01 to a calendar date (the "civil from days"
    // method by Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// An item id is a file name in the trash. It must be 1 plain name.
fn check_id(id: &str) -> Result<(), FsError> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\', '\0']) {
        return Err(FsError::InvalidPath("is not a trash item"));
    }
    Ok(())
}

/// The largest info file that is read. A real one holds a path and a date.
const MAX_INFO_BYTES: u64 = 64 * 1024;

struct TrashDirs {
    files: Dir,
    info: Dir,
}

impl Root {
    /// Sets the trash folder, as a UI path. The server uses the trash of the
    /// home folder (`~/.local/share/Trash`) when the home folder is beneath
    /// the files root.
    pub fn with_trash_dir(mut self, ui_path: &str) -> Result<Self, FsError> {
        self.trash = clean_path(ui_path)?;
        Ok(self)
    }

    fn trash_dirs(&self, create: bool) -> Result<TrashDirs, FsError> {
        if create {
            self.dir.create_dir_all(self.trash.join("files"))?;
            self.dir.create_dir_all(self.trash.join("info"))?;
            #[cfg(unix)]
            {
                // Other users of the server must not read trashed files.
                use cap_std::fs::{Permissions, PermissionsExt};
                let _ = self
                    .dir
                    .set_permissions(&self.trash, Permissions::from_mode(0o700));
            }
        }
        Ok(TrashDirs {
            files: self.dir.open_dir(self.trash.join("files"))?,
            info: self.dir.open_dir(self.trash.join("info"))?,
        })
    }

    /// Moves an entry to the trash.
    pub fn trash(&self, ui_path: &str) -> Result<(), FsError> {
        let rel = clean_path(ui_path)?;
        // The trash folder, and what is in it, is emptied, not trashed.
        if rel.starts_with(&self.trash) || self.trash.starts_with(&rel) {
            return Err(FsError::InvalidPath("is the trash, or holds it"));
        }
        let source = self.place(ui_path)?;
        source.dir.symlink_metadata(&source.name)?;
        let dirs = self.trash_dirs(true)?;

        // The info file is created first, with `create_new`. That reserves
        // the name, so 2 entries with the same name can't collide.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut name = source.name.clone();
        let mut info = None;
        for n in 1..10_000 {
            match dirs
                .info
                .open_with(format!("{name}{INFO_SUFFIX}"), &options)
            {
                Ok(file) => {
                    info = Some(file);
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    name = format!("{} ({n})", source.name);
                }
                Err(e) => return Err(e.into()),
            }
        }
        let mut info = info.ok_or(FsError::AlreadyExists)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let moved = (|| -> Result<(), FsError> {
            write!(
                info,
                "[Trash Info]\nPath={}\nDeletionDate={}\n",
                encode(&source.path()),
                timestamp(now)
            )?;
            info.sync_all()?;
            source.dir.rename(&source.name, &dirs.files, &name)?;
            Ok(())
        })();
        if moved.is_err() {
            let _ = dirs.info.remove_file(format!("{name}{INFO_SUFFIX}"));
        }
        moved
    }

    /// Everything in the trash, newest first. An empty list if there is no
    /// trash folder yet.
    pub fn trash_list(&self) -> Result<Vec<TrashItem>, FsError> {
        let dirs = match self.trash_dirs(false) {
            Ok(dirs) => dirs,
            Err(FsError::NotFound) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut items = Vec::new();
        for item in dirs.info.entries()?.flatten() {
            let file_name = item.file_name().to_string_lossy().into_owned();
            let Some(id) = file_name.strip_suffix(INFO_SUFFIX) else {
                continue;
            };
            let (Some(original_path), Ok(meta)) = (
                self.trash_origin(&dirs, id),
                dirs.files.symlink_metadata(id),
            ) else {
                continue;
            };
            let deleted_at = dirs
                .info
                .symlink_metadata(&file_name)
                .ok()
                .as_ref()
                .and_then(crate::modified_secs)
                .unwrap_or(0);
            let mut entry = to_entry(&dirs.files, id.to_owned(), &meta);
            // Show the name the entry had, not the name it has in the trash.
            entry.name = original_path.rsplit('/').next().unwrap_or(id).to_owned();
            items.push(TrashItem {
                id: id.to_owned(),
                original_path,
                deleted_at,
                entry,
            });
        }
        items.sort_by(|a, b| {
            b.deleted_at
                .cmp(&a.deleted_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(items)
    }

    /// The `Path=` value of an item, decoded.
    fn trash_origin(&self, dirs: &TrashDirs, id: &str) -> Option<String> {
        let name = format!("{id}{INFO_SUFFIX}");
        // Only a small regular file is read. A link, a device or a huge
        // file placed in the trash folder is skipped.
        let meta = dirs.info.symlink_metadata(&name).ok()?;
        if !meta.is_file() || meta.len() > MAX_INFO_BYTES {
            return None;
        }
        let text = dirs.info.read_to_string(&name).ok()?;
        let line = text.lines().find_map(|l| l.strip_prefix("Path="))?;
        decode(line.trim())
    }

    /// Moves an item back to where it was. Returns the path it has now.
    pub fn trash_restore(&self, id: &str, conflict: Conflict) -> Result<String, FsError> {
        check_id(id)?;
        let dirs = self.trash_dirs(false)?;
        dirs.files.symlink_metadata(id)?;
        let original = self.trash_origin(&dirs, id).ok_or(FsError::NotFound)?;
        // The info file is data on disk and could have been edited. The path
        // in it gets the same checks as a path from a request.
        let rel = clean_path(&original)?;
        if let Some(parent) = rel.parent().filter(|p| !p.as_os_str().is_empty()) {
            // The folder it was in may be gone. Make it again.
            self.dir.create_dir_all(parent)?;
        }
        let mut target = self.place(&original)?;
        if target.exists() {
            match conflict {
                Conflict::Fail => return Err(FsError::AlreadyExists),
                Conflict::KeepBoth => {
                    target.name = (1..10_000)
                        .map(|n| format!("{} (restored {n})", target.name))
                        .find(|c| target.dir.symlink_metadata(c).is_err())
                        .ok_or(FsError::AlreadyExists)?;
                }
                Conflict::Replace => {
                    if target.dir.symlink_metadata(&target.name)?.is_dir() {
                        target.dir.remove_dir_all(&target.name)?;
                    }
                }
            }
        }
        dirs.files.rename(id, &target.dir, &target.name)?;
        let _ = dirs.info.remove_file(format!("{id}{INFO_SUFFIX}"));
        Ok(target.path())
    }

    /// Deletes 1 item in the trash for good.
    pub fn trash_delete(&self, id: &str) -> Result<(), FsError> {
        check_id(id)?;
        let dirs = self.trash_dirs(false)?;
        if dirs.files.symlink_metadata(id)?.is_dir() {
            dirs.files.remove_dir_all(id)?;
        } else {
            dirs.files.remove_file(id)?;
        }
        let _ = dirs.info.remove_file(format!("{id}{INFO_SUFFIX}"));
        Ok(())
    }

    /// Deletes everything in the trash for good.
    pub fn trash_empty(&self) -> Result<(), FsError> {
        for item in self.trash_list()? {
            self.trash_delete(&item.id)?;
        }
        Ok(())
    }

    /// The trash folder as a UI path. Listings can hide it.
    pub fn trash_path(&self) -> String {
        crate::display_path(Path::new(&self.trash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, Root) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/guide.md"), "guide").unwrap();
        fs::write(root.join("a b%.txt"), "odd name").unwrap();
        fs::write(base.path().join("outside.txt"), "outside").unwrap();
        let r = Root::open(&root)
            .unwrap()
            .with_trash_dir("/.trash")
            .unwrap();
        (base, r)
    }

    #[test]
    fn trash_then_restore_brings_the_entry_back() {
        let (base, r) = fixture();
        let root = base.path().join("root");
        r.trash("/docs/guide.md").unwrap();
        r.trash("/a b%.txt").unwrap();
        assert!(!root.join("docs/guide.md").exists());

        let items = r.trash_list().unwrap();
        let paths: Vec<_> = items.iter().map(|i| i.original_path.as_str()).collect();
        assert!(paths.contains(&"/docs/guide.md") && paths.contains(&"/a b%.txt"));
        let info = fs::read_to_string(root.join(".trash/info/a b%.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/a%20b%25.txt\nDeletionDate="));

        // The folder it was in is gone. Restore makes it again.
        fs::remove_dir(root.join("docs")).unwrap();
        assert_eq!(
            r.trash_restore("guide.md", Conflict::Fail).unwrap(),
            "/docs/guide.md"
        );
        assert_eq!(
            fs::read_to_string(root.join("docs/guide.md")).unwrap(),
            "guide"
        );
        assert_eq!(r.trash_list().unwrap().len(), 1);
    }

    #[test]
    fn two_entries_with_one_name_stay_apart() {
        let (base, r) = fixture();
        let root = base.path().join("root");
        r.trash("/docs/guide.md").unwrap();
        fs::write(root.join("docs/guide.md"), "second").unwrap();
        r.trash("/docs/guide.md").unwrap();
        assert_eq!(r.trash_list().unwrap().len(), 2);

        r.trash_restore("guide.md", Conflict::Fail).unwrap();
        assert!(matches!(
            r.trash_restore("guide.md (1)", Conflict::Fail),
            Err(FsError::AlreadyExists)
        ));
        let kept = r.trash_restore("guide.md (1)", Conflict::KeepBoth).unwrap();
        assert_eq!(kept, "/docs/guide.md (restored 1)");
    }

    #[test]
    fn delete_and_empty_remove_for_good() {
        let (base, r) = fixture();
        r.trash("/docs").unwrap();
        r.trash("/a b%.txt").unwrap();
        r.trash_delete("docs").unwrap();
        assert_eq!(r.trash_list().unwrap().len(), 1);
        r.trash_empty().unwrap();
        assert!(r.trash_list().unwrap().is_empty());
        let left = fs::read_dir(base.path().join("root/.trash/files"))
            .unwrap()
            .count();
        assert_eq!(left, 0);
    }

    #[test]
    fn ids_and_stored_paths_can_not_leave_the_trash_or_the_root() {
        let (base, r) = fixture();
        let root = base.path().join("root");
        r.trash("/a b%.txt").unwrap();
        for id in ["..", "../info", "/etc", "a/b", "", "."] {
            assert!(r.trash_restore(id, Conflict::Replace).is_err(), "{id}");
            assert!(r.trash_delete(id).is_err(), "{id}");
        }
        // An info file that was edited to point outside the root.
        fs::write(
            root.join(".trash/info/a b%.txt.trashinfo"),
            "[Trash Info]\nPath=/../outside.txt\nDeletionDate=2026-01-01T00:00:00\n",
        )
        .unwrap();
        assert!(matches!(
            r.trash_restore("a b%.txt", Conflict::Replace),
            Err(FsError::InvalidPath(_))
        ));
        assert_eq!(
            fs::read_to_string(base.path().join("outside.txt")).unwrap(),
            "outside"
        );

        // The trash itself, and the root, can't be trashed.
        assert!(r.trash("/.trash").is_err());
        assert!(r.trash("/.trash/files").is_err());
        assert!(r.trash("/").is_err());
    }

    #[test]
    fn timestamps_are_calendar_dates_in_utc() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00");
        assert_eq!(timestamp(1_794_877_323), "2026-11-17T01:02:03");
    }
}
