//! Operations that change files: new folder, rename and move, copy, delete,
//! save, and the steps of an upload.
//!
//! Each one takes UI paths and resolves them beneath the root, like the read
//! operations. A new file is written under a temporary name in its target
//! folder and renamed into place when it is complete. A reader never sees a
//! half-written file, and a failure leaves the old file as it was
//! (docs/security/SECURITY.md, F3).

use crate::{clean_path, display_path, version_of, FsError, Root};
use cap_std::fs::{Dir, OpenOptions};
use rd_api::Conflict;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// A copy stops with an error after this many files and folders.
pub const MAX_COPY_ENTRIES: usize = 100_000;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A name for a temporary file. It starts with a dot, so listings hide it.
/// It is not a secret: files are created with `create_new`, which fails if
/// the name exists.
fn temp_name(purpose: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(".rd-{purpose}-{}-{nanos:x}-{n:x}.tmp", std::process::id())
}

/// A folder and a name in it: the 2 parts of a path that names an entry.
pub(crate) struct Place {
    pub dir: Dir,
    pub name: String,
    /// The folder as a UI path.
    pub dir_path: String,
}

impl Place {
    pub fn exists(&self) -> bool {
        self.dir.symlink_metadata(&self.name).is_ok()
    }

    pub fn path(&self) -> String {
        format!("{}/{}", self.dir_path.trim_end_matches('/'), self.name)
    }
}

/// "notes.txt" gives "notes (1).txt", then "notes (2).txt". For a name with
/// no dot, the number goes at the end.
fn free_name(dir: &Dir, name: &str) -> Result<String, FsError> {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    (1..10_000)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| dir.symlink_metadata(candidate).is_err())
        .ok_or(FsError::AlreadyExists)
}

fn remove_entry(dir: &Dir, name: &str) -> io::Result<()> {
    // A symlink is removed as a link. Its target is never touched.
    if dir.symlink_metadata(name)?.is_dir() {
        dir.remove_dir_all(name)
    } else {
        dir.remove_file(name)
    }
}

impl Root {
    /// Splits a path into its folder and its last name. The root itself has
    /// no such parts and is refused: it can't be renamed, deleted or
    /// replaced.
    pub(crate) fn place(&self, ui_path: &str) -> Result<Place, FsError> {
        let rel = clean_path(ui_path)?;
        let name = match rel.file_name().and_then(|n| n.to_str()) {
            Some(name) if rel != Path::new(".") => name.to_owned(),
            _ => return Err(FsError::InvalidPath("names the root, or is not valid text")),
        };
        let parent = rel.parent().filter(|p| !p.as_os_str().is_empty());
        let dir = match parent {
            Some(p) => self.dir.open_dir(p)?,
            None => self.dir.try_clone()?,
        };
        Ok(Place {
            dir,
            name,
            dir_path: display_path(parent.unwrap_or(Path::new("."))),
        })
    }

    /// Applies the conflict rule to a target. Returns the target with the
    /// name to use. With `Replace`, what is there stays until the caller
    /// has the new content ready.
    fn settle(&self, mut target: Place, conflict: Conflict) -> Result<Place, FsError> {
        if target.exists() {
            match conflict {
                Conflict::Fail => return Err(FsError::AlreadyExists),
                Conflict::KeepBoth => target.name = free_name(&target.dir, &target.name)?,
                Conflict::Replace => {}
            }
        }
        Ok(target)
    }

    /// Renames `temp` in the target folder to the target name. An existing
    /// folder is removed first: a rename can replace a file, not a folder.
    fn put_in_place(target: &Place, temp: &str) -> Result<(), FsError> {
        if let Ok(meta) = target.dir.symlink_metadata(&target.name) {
            if meta.is_dir() {
                target.dir.remove_dir_all(&target.name)?;
            }
        }
        target.dir.rename(temp, &target.dir, &target.name)?;
        Ok(())
    }

    fn check_not_inside(from: &str, to: &str) -> Result<(), FsError> {
        let from = clean_path(from)?;
        let to = clean_path(to)?;
        if to.starts_with(&from) {
            return Err(FsError::InvalidPath("the target is inside the source"));
        }
        Ok(())
    }

    /// Makes a new, empty folder. Fails if the name is in use.
    pub fn create_dir(&self, ui_path: &str) -> Result<(), FsError> {
        let place = self.place(ui_path)?;
        place.dir.create_dir(&place.name)?;
        Ok(())
    }

    /// Renames an entry, or moves it to another folder. Returns the path it
    /// has afterwards.
    pub fn rename(&self, from: &str, to: &str, conflict: Conflict) -> Result<String, FsError> {
        let source = self.place(from)?;
        source.dir.symlink_metadata(&source.name)?;
        if clean_path(from)? == clean_path(to)? {
            return Ok(source.path());
        }
        Self::check_not_inside(from, to)?;
        let target = self.settle(self.place(to)?, conflict)?;
        if let Ok(meta) = target.dir.symlink_metadata(&target.name) {
            if meta.is_dir() {
                target.dir.remove_dir_all(&target.name)?;
            }
        }
        source.dir.rename(&source.name, &target.dir, &target.name)?;
        Ok(target.path())
    }

    /// Copies a file, or a folder with all that is in it. Returns the path
    /// of the copy. Symlinks and special files inside a folder are left out.
    ///
    /// The copy is made under a temporary name and renamed when it is
    /// complete. A failure removes the partial copy.
    pub fn copy(&self, from: &str, to: &str, conflict: Conflict) -> Result<String, FsError> {
        let source = self.place(from)?;
        let meta = source.dir.symlink_metadata(&source.name)?;
        if !meta.is_dir() && !meta.is_file() {
            return Err(FsError::NotAFile);
        }
        Self::check_not_inside(from, to)?;
        let target = self.settle(self.place(to)?, conflict)?;
        let temp = temp_name("copy");
        let mut budget = MAX_COPY_ENTRIES;
        let copied = copy_entry(&source.dir, &source.name, &target.dir, &temp, &mut budget)
            .and_then(|()| Self::put_in_place(&target, &temp));
        if let Err(e) = copied {
            let _ = remove_entry(&target.dir, &temp);
            return Err(e);
        }
        Ok(target.path())
    }

    /// Deletes an entry for good. A folder is deleted with all that is in it.
    pub fn remove(&self, ui_path: &str) -> Result<(), FsError> {
        let place = self.place(ui_path)?;
        remove_entry(&place.dir, &place.name)?;
        Ok(())
    }

    /// Replaces the content of a file, or creates it.
    ///
    /// `expect` is the version the caller last read (see [`crate::OpenFile`]).
    /// If the file has another version now, nothing is written and the error
    /// is `ChangedOnDisk`. `None` writes without this check. Returns the new
    /// version.
    pub fn write_file(
        &self,
        ui_path: &str,
        data: &[u8],
        expect: Option<&str>,
    ) -> Result<String, FsError> {
        let place = self.place(ui_path)?;
        let existing = match place.dir.symlink_metadata(&place.name) {
            Ok(meta) => Some(meta),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        // A save replaces the entry with a new regular file. For a link or a
        // folder that would be a different thing than the user edited.
        if existing.as_ref().is_some_and(|m| !m.is_file()) {
            return Err(FsError::NotAFile);
        }
        if let Some(expect) = expect {
            if existing.as_ref().map(version_of).as_deref() != Some(expect) {
                return Err(FsError::ChangedOnDisk);
            }
        }

        let temp = temp_name("save");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            // The new file gets the permissions of the one it replaces.
            use cap_std::fs::{OpenOptionsExt, PermissionsExt};
            let mode = existing.as_ref().map_or(0o644, |m| m.permissions().mode());
            options.mode(mode & 0o7777);
        }
        let written = (|| -> io::Result<String> {
            let mut file = place.dir.open_with(&temp, &options)?;
            file.write_all(data)?;
            // On disk before the rename: a crash leaves the old file or the
            // new one, never a part of each.
            file.sync_all()?;
            let version = version_of(&file.metadata()?);
            place.dir.rename(&temp, &place.dir, &place.name)?;
            Ok(version)
        })();
        match written {
            Ok(version) => Ok(version),
            Err(e) => {
                let _ = place.dir.remove_file(&temp);
                Err(e.into())
            }
        }
    }

    // ---------- Uploads ----------
    //
    // An upload is a hidden temporary file in the target folder. The bytes
    // the server has are the length of that file, so no other state is
    // needed to resume. `temp` is a name made by the server, never by the
    // client.

    /// Starts an upload: creates the empty temporary file.
    pub fn upload_begin(&self, dir: &str, temp: &str) -> Result<(), FsError> {
        let dir = self.dir.open_dir(clean_path(dir)?)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        dir.open_with(temp, &options)?;
        Ok(())
    }

    /// Bytes received so far.
    pub fn upload_len(&self, dir: &str, temp: &str) -> Result<u64, FsError> {
        let dir = self.dir.open_dir(clean_path(dir)?)?;
        Ok(dir.symlink_metadata(temp)?.len())
    }

    /// Adds a chunk. `offset` must be the current length of the temporary
    /// file. A chunk sent twice, or out of order, is refused with
    /// `OffsetMismatch`, which holds the length to continue from. A chunk
    /// that would pass `size` is refused too. Returns the new length.
    pub fn upload_append(
        &self,
        dir: &str,
        temp: &str,
        offset: u64,
        data: &[u8],
        size: u64,
    ) -> Result<u64, FsError> {
        let dir = self.dir.open_dir(clean_path(dir)?)?;
        let mut options = OpenOptions::new();
        options.append(true);
        let mut file = dir.open_with(temp, &options)?;
        let len = file.metadata()?.len();
        if len != offset || offset.saturating_add(data.len() as u64) > size {
            return Err(FsError::OffsetMismatch(len));
        }
        file.write_all(data)?;
        Ok(len + data.len() as u64)
    }

    /// Ends an upload: checks the size, makes sure the bytes are on disk,
    /// and renames the temporary file to its final name. Returns the path
    /// of the file.
    pub fn upload_finish(
        &self,
        dir_path: &str,
        temp: &str,
        name: &str,
        size: u64,
        conflict: Conflict,
    ) -> Result<String, FsError> {
        let rel = clean_path(dir_path)?;
        let dir = self.dir.open_dir(&rel)?;
        let file = dir.open(temp)?;
        let len = file.metadata()?.len();
        if len != size {
            return Err(FsError::OffsetMismatch(len));
        }
        file.sync_all()?;
        drop(file);
        let target = self.settle(self.place(&format!("{dir_path}/{name}"))?, conflict)?;
        Self::put_in_place(&target, temp)?;
        Ok(target.path())
    }

    /// Removes the temporary file of an upload. A missing file is not an
    /// error: the upload may have been removed already.
    pub fn upload_abort(&self, dir: &str, temp: &str) -> Result<(), FsError> {
        let dir = self.dir.open_dir(clean_path(dir)?)?;
        match dir.remove_file(temp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

fn copy_entry(
    from_dir: &Dir,
    from: &str,
    to_dir: &Dir,
    to: &str,
    budget: &mut usize,
) -> Result<(), FsError> {
    *budget = budget.checked_sub(1).ok_or(FsError::TooMany)?;
    let meta = from_dir.symlink_metadata(from)?;
    if meta.is_file() {
        from_dir.copy(from, to_dir, to)?;
    } else if meta.is_dir() {
        to_dir.create_dir(to)?;
        let source = from_dir.open_dir(from)?;
        let target = to_dir.open_dir(to)?;
        for item in source.entries()? {
            let item = item?;
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let file_type = item.file_type()?;
            // A symlink is not followed and not copied. So a copy can't be
            // used to read a target outside the root.
            if file_type.is_file() || file_type.is_dir() {
                copy_entry(&source, &name, &target, &name, budget)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A root with a few entries, inside a folder that also holds a file
    /// outside the root.
    fn fixture() -> (tempfile::TempDir, Root) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        fs::create_dir_all(root.join("docs/deep")).unwrap();
        fs::write(root.join("docs/guide.md"), "guide").unwrap();
        fs::write(root.join("docs/deep/a.txt"), "a").unwrap();
        fs::write(root.join("notes.txt"), "notes").unwrap();
        fs::write(base.path().join("outside.txt"), "outside").unwrap();
        let r = Root::open(&root).unwrap();
        (base, r)
    }

    fn read(base: &tempfile::TempDir, path: &str) -> String {
        fs::read_to_string(base.path().join("root").join(path)).unwrap()
    }

    fn exists(base: &tempfile::TempDir, path: &str) -> bool {
        base.path().join("root").join(path).exists()
    }

    /// Names of everything in the folder that holds the root. A write that
    /// left the root would add to it or change `outside.txt`.
    fn outside_is_untouched(base: &tempfile::TempDir) {
        let mut names: Vec<_> = fs::read_dir(base.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["outside.txt", "root"]);
        assert_eq!(
            fs::read_to_string(base.path().join("outside.txt")).unwrap(),
            "outside"
        );
    }

    #[test]
    fn rename_move_copy_and_remove() {
        let (base, r) = fixture();
        r.create_dir("/new").unwrap();
        assert!(matches!(r.create_dir("/new"), Err(FsError::AlreadyExists)));

        assert_eq!(
            r.rename("/notes.txt", "/new/n.txt", Conflict::Fail)
                .unwrap(),
            "/new/n.txt"
        );
        assert_eq!(read(&base, "new/n.txt"), "notes");
        assert!(!exists(&base, "notes.txt"));

        assert_eq!(
            r.copy("/docs", "/new/docs", Conflict::Fail).unwrap(),
            "/new/docs"
        );
        assert_eq!(read(&base, "new/docs/deep/a.txt"), "a");
        assert_eq!(read(&base, "docs/deep/a.txt"), "a");

        r.remove("/new").unwrap();
        assert!(!exists(&base, "new"));
        assert!(matches!(r.remove("/"), Err(FsError::InvalidPath(_))));
    }

    #[test]
    fn conflict_rules() {
        let (base, r) = fixture();
        fs::write(base.path().join("root/docs/notes.txt"), "old").unwrap();

        assert!(matches!(
            r.copy("/notes.txt", "/docs/notes.txt", Conflict::Fail),
            Err(FsError::AlreadyExists)
        ));
        assert_eq!(read(&base, "docs/notes.txt"), "old");

        let kept = r
            .copy("/notes.txt", "/docs/notes.txt", Conflict::KeepBoth)
            .unwrap();
        assert_eq!(kept, "/docs/notes (1).txt");
        assert_eq!(read(&base, "docs/notes.txt"), "old");

        r.copy("/notes.txt", "/docs/notes.txt", Conflict::Replace)
            .unwrap();
        assert_eq!(read(&base, "docs/notes.txt"), "notes");

        // A file can replace a folder, and no temporary file is left.
        r.copy("/notes.txt", "/docs/deep", Conflict::Replace)
            .unwrap();
        assert_eq!(read(&base, "docs/deep"), "notes");
        let left: Vec<_> = fs::read_dir(base.path().join("root/docs"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with(".rd-"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_folder_can_not_be_moved_or_copied_into_itself() {
        let (_base, r) = fixture();
        for to in ["/docs/deep/docs", "/docs/x"] {
            assert!(matches!(
                r.rename("/docs", to, Conflict::Fail),
                Err(FsError::InvalidPath(_))
            ));
            assert!(matches!(
                r.copy("/docs", to, Conflict::Fail),
                Err(FsError::InvalidPath(_))
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn writes_do_not_follow_symlinks_out_of_the_root() {
        let (base, r) = fixture();
        let root = base.path().join("root");
        std::os::unix::fs::symlink(base.path(), root.join("escape")).unwrap();
        std::os::unix::fs::symlink(base.path().join("outside.txt"), root.join("link.txt")).unwrap();

        assert!(r.create_dir("/escape/made").is_err());
        assert!(r.write_file("/escape/made.txt", b"x", None).is_err());
        assert!(r.write_file("/link.txt", b"x", None).is_err());
        assert!(r
            .rename("/notes.txt", "/escape/notes.txt", Conflict::Fail)
            .is_err());
        assert!(r
            .copy("/notes.txt", "/escape/notes.txt", Conflict::Fail)
            .is_err());
        assert!(r
            .copy("/escape/outside.txt", "/stolen.txt", Conflict::Fail)
            .is_err());
        assert!(r.remove("/escape/outside.txt").is_err());
        assert!(r.upload_begin("/escape", ".rd-upload-x.part").is_err());
        outside_is_untouched(&base);

        // A copy of a folder leaves the links in it out.
        std::os::unix::fs::symlink(base.path(), root.join("docs/escape")).unwrap();
        r.copy("/docs", "/copy", Conflict::Fail).unwrap();
        assert!(!root.join("copy/escape").exists());
        assert!(root.join("copy/deep/a.txt").exists());

        // Removing a link removes the link, not what it points to.
        r.remove("/escape").unwrap();
        r.remove("/link.txt").unwrap();
        outside_is_untouched(&base);
    }

    /// Many odd paths against every write operation. Whatever each call
    /// returns, nothing outside the root may change.
    #[test]
    fn odd_paths_never_write_outside_the_root() {
        let (base, r) = fixture();
        let parts = [
            "..",
            ".",
            "",
            "/",
            "//",
            "docs",
            "notes.txt",
            "..%2f",
            "%2e%2e",
            "a\0b",
            "a\\b",
            "\\..\\..",
            "....//",
            "..;",
            " ..",
            ".. ",
            "~",
            "outside.txt",
            "root",
            "\u{202e}",
            "\n",
        ];
        // A fixed sequence of numbers, so a failure can be repeated.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        let mut path = || {
            let n = 1 + next() % 5;
            let mut p = String::new();
            for _ in 0..n {
                if next() % 4 != 0 {
                    p.push('/');
                }
                p.push_str(parts[next() % parts.len()]);
            }
            p
        };
        for _ in 0..3000 {
            let (a, b) = (path(), path());
            let _ = r.create_dir(&a);
            let _ = r.write_file(&a, b"fuzz", None);
            let _ = r.rename(&a, &b, Conflict::Replace);
            let _ = r.copy(&a, &b, Conflict::KeepBoth);
            let _ = r.upload_begin(&a, ".rd-upload-fuzz.part");
            let _ = r.upload_finish(&a, ".rd-upload-fuzz.part", &b, 0, Conflict::Replace);
            let _ = r.trash(&a);
            let _ = r.remove(&b);
        }
        outside_is_untouched(&base);
    }

    #[test]
    fn save_checks_the_version_and_keeps_the_permissions() {
        let (base, r) = fixture();
        let v1 = r.open_file("/notes.txt").unwrap().version;
        let v2 = r.write_file("/notes.txt", b"second", Some(&v1)).unwrap();
        assert_eq!(read(&base, "notes.txt"), "second");
        assert_ne!(v1, v2);
        assert_eq!(r.open_file("/notes.txt").unwrap().version, v2);

        // The old version is refused, and the file stays as it was.
        assert!(matches!(
            r.write_file("/notes.txt", b"third", Some(&v1)),
            Err(FsError::ChangedOnDisk)
        ));
        assert_eq!(read(&base, "notes.txt"), "second");

        // A file that was deleted meanwhile is a change too.
        fs::remove_file(base.path().join("root/notes.txt")).unwrap();
        assert!(matches!(
            r.write_file("/notes.txt", b"x", Some(&v2)),
            Err(FsError::ChangedOnDisk)
        ));
        assert!(matches!(
            r.write_file("/docs", b"x", None),
            Err(FsError::NotAFile)
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = base.path().join("root/run.sh");
            fs::write(&script, "old").unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();
            r.write_file("/run.sh", b"new", None).unwrap();
            assert_eq!(
                fs::metadata(&script).unwrap().permissions().mode() & 0o777,
                0o750
            );
        }
    }

    #[test]
    fn an_upload_is_appended_in_order_and_renamed_when_complete() {
        let (base, r) = fixture();
        let temp = ".rd-upload-1.part";
        r.upload_begin("/docs", temp).unwrap();
        assert_eq!(r.upload_append("/docs", temp, 0, b"hello ", 11).unwrap(), 6);

        // The same chunk again, and a chunk from the wrong place.
        assert!(matches!(
            r.upload_append("/docs", temp, 0, b"hello ", 11),
            Err(FsError::OffsetMismatch(6))
        ));
        assert!(matches!(
            r.upload_append("/docs", temp, 9, b"xx", 11),
            Err(FsError::OffsetMismatch(6))
        ));
        // A chunk that would make the file larger than announced.
        assert!(matches!(
            r.upload_append("/docs", temp, 6, b"world!!", 11),
            Err(FsError::OffsetMismatch(6))
        ));
        // Not complete yet.
        assert!(matches!(
            r.upload_finish("/docs", temp, "hello.txt", 11, Conflict::Fail),
            Err(FsError::OffsetMismatch(6))
        ));
        assert!(!exists(&base, "docs/hello.txt"));

        assert_eq!(r.upload_len("/docs", temp).unwrap(), 6);
        assert_eq!(r.upload_append("/docs", temp, 6, b"world", 11).unwrap(), 11);
        let path = r
            .upload_finish("/docs", temp, "hello.txt", 11, Conflict::Fail)
            .unwrap();
        assert_eq!(path, "/docs/hello.txt");
        assert_eq!(read(&base, "docs/hello.txt"), "hello world");
        assert!(!exists(&base, "docs/.rd-upload-1.part"));
    }

    #[test]
    fn a_cancelled_upload_leaves_nothing() {
        let (base, r) = fixture();
        let temp = ".rd-upload-2.part";
        r.upload_begin("/docs", temp).unwrap();
        r.upload_append("/docs", temp, 0, b"part", 100).unwrap();
        r.upload_abort("/docs", temp).unwrap();
        r.upload_abort("/docs", temp).unwrap();
        assert!(!exists(&base, "docs/.rd-upload-2.part"));
        assert!(r.upload_append("/docs", temp, 0, b"part", 100).is_err());
    }

    #[test]
    fn an_upload_name_can_not_hold_a_path() {
        let (base, r) = fixture();
        let temp = ".rd-upload-3.part";
        for name in ["../../outside.txt", "/outside.txt", "deep/x.txt", "..", ""] {
            r.upload_begin("/docs", temp).unwrap();
            let done = r.upload_finish("/docs", temp, name, 0, Conflict::Replace);
            if let Ok(path) = &done {
                // "deep/x.txt" is a valid path beneath the root.
                assert!(path.starts_with("/docs/"), "{name} gave {path}");
            }
            let _ = r.upload_abort("/docs", temp);
        }
        outside_is_untouched(&base);
    }
}
