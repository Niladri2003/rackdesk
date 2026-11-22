//! Zip archives: make one from a file or folder, and extract one.
//!
//! An archive from someone else is untrusted input
//! (docs/security/SECURITY.md, F4). So the extraction here does not use the
//! library's own `extract`. Every entry is written through cap-std into a
//! new, empty folder beneath the root:
//!
//! - A path that would leave the target folder ("Zip Slip") fails the whole
//!   extraction.
//! - A symlink entry is left out. No link is ever created.
//! - Only the 9 plain permission bits are kept. Set-uid and set-gid bits
//!   are dropped.
//! - The number of entries and the number of bytes written have limits, so
//!   a small archive can't fill the disk (a "zip bomb").
//!
//! The entries are written to a hidden temporary folder, which is renamed
//! when the extraction is complete and removed when it fails.

use crate::write::{remove_entry, temp_name};
use crate::{clean_path, FsError, Root};
use cap_std::fs::{Dir, OpenOptions};
use rd_api::Conflict;
use std::io::{self, Read, Seek, Write};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    /// Files and folders in 1 archive.
    pub max_entries: usize,
    /// Bytes read into an archive, or written out of one.
    pub max_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_bytes: 20 * 1024 * 1024 * 1024,
        }
    }
}

/// Free space that an extraction must leave on the disk.
const SPACE_RESERVE: u64 = 64 * 1024 * 1024;
/// The entry that lists what could not be read, in an archive made here.
const SKIPPED_NAME: &str = "NOT-INCLUDED.txt";

impl From<ZipError> for FsError {
    fn from(e: ZipError) -> Self {
        match e {
            ZipError::Io(io) => io.into(),
            ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_) => {
                FsError::BadArchive("it uses a feature that is not supported")
            }
            ZipError::InvalidPassword => FsError::BadArchive("it is protected by a password"),
            _ => FsError::BadArchive("it is not a valid zip file"),
        }
    }
}

/// Counts what goes into or comes out of an archive.
struct Budget {
    entries: usize,
    bytes: u64,
}

impl Budget {
    fn entry(&mut self) -> Result<(), FsError> {
        self.entries = self.entries.checked_sub(1).ok_or(FsError::TooMany)?;
        Ok(())
    }
}

/// Copies up to the bytes left in the budget, and fails if there is more.
fn copy_limited(
    from: &mut impl Read,
    to: &mut impl Write,
    budget: &mut Budget,
) -> Result<(), FsError> {
    let copied = io::copy(&mut from.take(budget.bytes.saturating_add(1)), to)?;
    budget.bytes = budget.bytes.checked_sub(copied).ok_or(FsError::TooMany)?;
    Ok(())
}

fn file_options(mode: Option<u32>) -> SimpleFileOptions {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        // Allows an entry of more than 4 GiB.
        .large_file(true);
    match mode {
        Some(mode) => options.unix_permissions(mode & 0o777),
        None => options,
    }
}

#[cfg(unix)]
fn mode_of(meta: &cap_std::fs::Metadata) -> Option<u32> {
    use cap_std::fs::PermissionsExt;
    Some(meta.permissions().mode())
}

#[cfg(not(unix))]
fn mode_of(_: &cap_std::fs::Metadata) -> Option<u32> {
    None
}

/// Adds the entry `name` in `dir` to the archive under `path`, and for a
/// folder everything in it. An entry that can't be read is noted in
/// `skipped` and left out. Symlinks and special files are left out too.
fn add_entry<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    dir: &Dir,
    name: &str,
    path: &str,
    budget: &mut Budget,
    skipped: &mut Vec<String>,
) -> Result<(), FsError> {
    let meta = dir.symlink_metadata(name)?;
    if meta.is_file() {
        let mut options = OpenOptions::new();
        options.read(true);
        let Ok(mut file) = dir.open_with(name, &options) else {
            skipped.push(path.to_owned());
            return Ok(());
        };
        budget.entry()?;
        zip.start_file(path, file_options(mode_of(&meta)))?;
        copy_limited(&mut file, zip, budget)?;
    } else if meta.is_dir() {
        let Ok(inner) = dir.open_dir(name).and_then(|d| d.entries().map(|e| (d, e))) else {
            skipped.push(format!("{path}/"));
            return Ok(());
        };
        let (inner, entries) = inner;
        budget.entry()?;
        zip.add_directory(format!("{path}/"), file_options(mode_of(&meta)))?;
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        for child in names {
            add_entry(
                zip,
                &inner,
                &child,
                &format!("{path}/{child}"),
                budget,
                skipped,
            )?;
        }
    }
    Ok(())
}

fn write_zip<W: Write + Seek>(
    mut zip: ZipWriter<W>,
    dir: &Dir,
    name: &str,
    limits: &ArchiveLimits,
) -> Result<W, FsError> {
    let mut budget = Budget {
        entries: limits.max_entries,
        bytes: limits.max_bytes,
    };
    let mut skipped = Vec::new();
    add_entry(&mut zip, dir, name, name, &mut budget, &mut skipped)?;
    if !skipped.is_empty() {
        // The person who opens the archive must be able to see that it is
        // not complete.
        zip.start_file(SKIPPED_NAME, file_options(None))?;
        writeln!(
            zip,
            "The server could not read these entries. They are not in this archive:"
        )?;
        for path in skipped {
            writeln!(zip, "{path}")?;
        }
    }
    Ok(zip.finish()?)
}

impl Root {
    /// Writes a zip of a file or folder to `out`, as it is made. `out` need
    /// not support seeking, so this can feed a download.
    /// Checks that `zip_to` can start on this path. A handler calls it
    /// before it sends the first byte, so a wrong path gets an error and not
    /// a broken archive.
    pub fn zip_check(&self, ui_path: &str) -> Result<(), FsError> {
        let source = self.place(ui_path)?;
        source.dir.symlink_metadata(&source.name)?;
        Ok(())
    }

    pub fn zip_to<W: Write>(
        &self,
        ui_path: &str,
        out: W,
        limits: &ArchiveLimits,
    ) -> Result<(), FsError> {
        let source = self.place(ui_path)?;
        write_zip(
            ZipWriter::new_stream(out),
            &source.dir,
            &source.name,
            limits,
        )?;
        Ok(())
    }

    /// Makes a zip file at `to` from the file or folder at `from`. Returns
    /// the path of the archive.
    pub fn compress(
        &self,
        from: &str,
        to: &str,
        conflict: Conflict,
        limits: &ArchiveLimits,
    ) -> Result<String, FsError> {
        let source = self.place(from)?;
        source.dir.symlink_metadata(&source.name)?;
        // An archive inside its own source would be read into itself.
        if clean_path(to)?.starts_with(clean_path(from)?) {
            return Err(FsError::InvalidPath("the target is inside the source"));
        }
        let target = self.settle(self.place(to)?, conflict)?;
        let temp = temp_name("zip");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let made = (|| -> Result<(), FsError> {
            let file = target.dir.open_with(&temp, &options)?;
            let file = write_zip(
                ZipWriter::new(file.into_std()),
                &source.dir,
                &source.name,
                limits,
            )?;
            file.sync_all()?;
            Self::put_in_place(&target, &temp)
        })();
        if let Err(e) = made {
            let _ = target.dir.remove_file(&temp);
            return Err(e);
        }
        Ok(target.path())
    }

    /// Extracts a zip file into a new folder at `to`. Returns the path of
    /// that folder. See the top of this file for the rules.
    pub fn extract(
        &self,
        archive: &str,
        to: &str,
        conflict: Conflict,
        limits: &ArchiveLimits,
    ) -> Result<String, FsError> {
        let file = self.open_file(archive)?.file;
        let mut zip = ZipArchive::new(file)?;
        if zip.len() > limits.max_entries {
            return Err(FsError::TooMany);
        }
        let target = self.settle(self.place(to)?, conflict)?;
        // Stop before the disk is full, whatever sizes the archive states.
        let free = rustix::fs::fstatvfs(&target.dir)
            .map(|s| s.f_bavail.saturating_mul(s.f_frsize))
            .unwrap_or(u64::MAX);
        let mut budget = Budget {
            entries: limits.max_entries,
            bytes: limits.max_bytes.min(free.saturating_sub(SPACE_RESERVE)),
        };

        let temp = temp_name("extract");
        target.dir.create_dir(&temp)?;
        let done = target
            .dir
            .open_dir(&temp)
            .map_err(FsError::from)
            .and_then(|out| extract_into(&mut zip, &out, &mut budget))
            .and_then(|()| Self::put_in_place(&target, &temp));
        if let Err(e) = done {
            let _ = remove_entry(&target.dir, &temp);
            return Err(e);
        }
        Ok(target.path())
    }
}

fn extract_into<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    out: &Dir,
    budget: &mut Budget,
) -> Result<(), FsError> {
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        // `enclosed_name` gives no path for an entry that is absolute or
        // that climbs out with "..". Such an archive is refused as a whole:
        // it was made to attack, and the rest of it is not to be trusted.
        let unsafe_path = FsError::BadArchive("an entry has a path outside the archive");
        let path = entry.enclosed_name().ok_or(unsafe_path)?;
        let path = path
            .to_str()
            .ok_or(FsError::BadArchive("an entry name is not valid text"))?;
        // The same checks as for a path from a request.
        let rel = clean_path(path)
            .map_err(|_| FsError::BadArchive("an entry has a path that is not allowed"))?;
        if rel.as_os_str() == "." || entry.is_symlink() {
            continue;
        }
        budget.entry()?;
        if entry.is_dir() {
            out.create_dir_all(&rel)?;
            continue;
        }
        if let Some(parent) = rel.parent().filter(|p| !p.as_os_str().is_empty()) {
            out.create_dir_all(parent)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            // Plain permission bits only. No set-uid, set-gid or sticky bit.
            use cap_std::fs::OpenOptionsExt;
            options.mode(entry.unix_mode().map_or(0o644, |m| m & 0o777));
        }
        let mut file = out.open_with(&rel, &options)?;
        copy_limited(&mut entry, &mut file, budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, Root) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        fs::create_dir_all(root.join("site/css")).unwrap();
        fs::write(root.join("site/index.html"), "<h1>hi</h1>").unwrap();
        fs::write(root.join("site/css/main.css"), "body {}").unwrap();
        fs::write(base.path().join("outside.txt"), "outside").unwrap();
        (base, Root::open(&root).unwrap())
    }

    /// Writes a zip file with entries that a normal tool would not make.
    fn evil_zip(path: &std::path::Path, build: impl FnOnce(&mut ZipWriter<fs::File>)) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        build(&mut zip);
        zip.finish().unwrap();
    }

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

    fn names_in(base: &tempfile::TempDir, dir: &str) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(base.path().join("root").join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn compress_then_extract_gives_the_same_files() {
        let (base, r) = fixture();
        let limits = ArchiveLimits::default();
        assert_eq!(
            r.compress("/site", "/site.zip", Conflict::Fail, &limits)
                .unwrap(),
            "/site.zip"
        );
        assert_eq!(
            r.extract("/site.zip", "/copy", Conflict::Fail, &limits)
                .unwrap(),
            "/copy"
        );
        let root = base.path().join("root");
        assert_eq!(
            fs::read_to_string(root.join("copy/site/index.html")).unwrap(),
            "<h1>hi</h1>"
        );
        assert_eq!(
            fs::read_to_string(root.join("copy/site/css/main.css")).unwrap(),
            "body {}"
        );
        assert_eq!(names_in(&base, ""), ["copy", "site", "site.zip"]);
    }

    #[test]
    fn a_streamed_zip_can_be_extracted() {
        let (base, r) = fixture();
        let limits = ArchiveLimits::default();
        let mut bytes = Vec::new();
        r.zip_to("/site", &mut bytes, &limits).unwrap();
        fs::write(base.path().join("root/stream.zip"), bytes).unwrap();
        r.extract("/stream.zip", "/out", Conflict::Fail, &limits)
            .unwrap();
        assert_eq!(
            fs::read_to_string(base.path().join("root/out/site/css/main.css")).unwrap(),
            "body {}"
        );
    }

    #[test]
    fn entries_that_climb_out_fail_the_whole_extraction() {
        let (base, r) = fixture();
        let options = SimpleFileOptions::default();
        for (i, bad) in [
            "../evil.txt",
            "a/../../evil.txt",
            "/abs/evil.txt",
            "a\\..\\..\\evil.txt",
        ]
        .iter()
        .enumerate()
        {
            let name = format!("bad{i}.zip");
            evil_zip(&base.path().join("root").join(&name), |zip| {
                zip.start_file("fine.txt", options).unwrap();
                zip.write_all(b"fine").unwrap();
                zip.start_file(*bad, options).unwrap();
                zip.write_all(b"evil").unwrap();
            });
            let result = r.extract(
                &format!("/{name}"),
                "/out",
                Conflict::Fail,
                &ArchiveLimits::default(),
            );
            // Either the entry is refused, or it lands inside the new folder.
            if result.is_ok() {
                assert!(!base.path().join("root/evil.txt").exists(), "{bad}");
                fs::remove_dir_all(base.path().join("root/out")).unwrap();
            } else {
                assert!(
                    !base.path().join("root/out").exists(),
                    "{bad} left a partial folder"
                );
            }
            outside_is_untouched(&base);
            assert!(
                !names_in(&base, "").iter().any(|n| n.starts_with(".rd-")),
                "{bad} left a temporary folder"
            );
        }
    }

    #[test]
    fn symlink_entries_are_left_out() {
        let (base, r) = fixture();
        let options = SimpleFileOptions::default();
        evil_zip(&base.path().join("root/links.zip"), |zip| {
            zip.add_symlink("escape", "../..", options).unwrap();
            zip.add_symlink("passwd", "/etc/passwd", options).unwrap();
            zip.start_file("kept.txt", options).unwrap();
            zip.write_all(b"kept").unwrap();
        });
        r.extract(
            "/links.zip",
            "/out",
            Conflict::Fail,
            &ArchiveLimits::default(),
        )
        .unwrap();
        assert_eq!(names_in(&base, "out"), ["kept.txt"]);
        outside_is_untouched(&base);
    }

    #[cfg(unix)]
    #[test]
    fn set_uid_bits_are_dropped() {
        use std::os::unix::fs::PermissionsExt;
        let (base, r) = fixture();
        evil_zip(&base.path().join("root/suid.zip"), |zip| {
            zip.start_file(
                "tool",
                SimpleFileOptions::default().unix_permissions(0o6755),
            )
            .unwrap();
            zip.write_all(b"#!/bin/sh").unwrap();
        });
        r.extract(
            "/suid.zip",
            "/out",
            Conflict::Fail,
            &ArchiveLimits::default(),
        )
        .unwrap();
        let mode = fs::metadata(base.path().join("root/out/tool"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7000, 0, "mode is {mode:o}");
    }

    #[test]
    fn limits_stop_an_archive_that_expands_too_far() {
        let (base, r) = fixture();
        // 4 MiB of zeros compresses to a few KiB.
        evil_zip(&base.path().join("root/bomb.zip"), |zip| {
            zip.start_file("zeros.bin", file_options(None)).unwrap();
            zip.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
        });
        assert!(
            fs::metadata(base.path().join("root/bomb.zip"))
                .unwrap()
                .len()
                < 64 * 1024
        );
        let small = ArchiveLimits {
            max_bytes: 1024 * 1024,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            r.extract("/bomb.zip", "/out", Conflict::Fail, &small),
            Err(FsError::TooMany)
        ));
        assert_eq!(
            names_in(&base, ""),
            ["bomb.zip", "site"],
            "a partial result was left"
        );

        let few = ArchiveLimits {
            max_entries: 2,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            r.compress("/site", "/site.zip", Conflict::Fail, &few),
            Err(FsError::TooMany)
        ));
        assert_eq!(names_in(&base, ""), ["bomb.zip", "site"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_zip_leaves_out_symlinks_and_notes_what_it_could_not_read() {
        use std::os::unix::fs::PermissionsExt;
        let (base, r) = fixture();
        let root = base.path().join("root");
        std::os::unix::fs::symlink(base.path().join("outside.txt"), root.join("site/link.txt"))
            .unwrap();
        fs::write(root.join("site/secret.key"), "key").unwrap();
        fs::set_permissions(
            root.join("site/secret.key"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();

        let mut bytes = Vec::new();
        r.zip_to("/site", &mut bytes, &ArchiveLimits::default())
            .unwrap();
        let mut zip = ZipArchive::new(io::Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.contains("link.txt")));
        // Root can read any file, so the note is expected only for other users.
        if rustix::process::getuid().as_raw() != 0 {
            assert!(!names.iter().any(|n| n.contains("secret.key")));
            let mut note = String::new();
            zip.by_name(SKIPPED_NAME)
                .unwrap()
                .read_to_string(&mut note)
                .unwrap();
            assert!(note.contains("site/secret.key"));
        }
    }

    #[test]
    fn a_file_that_is_not_a_zip_is_refused() {
        let (base, r) = fixture();
        assert!(matches!(
            r.extract(
                "/site/index.html",
                "/out",
                Conflict::Fail,
                &ArchiveLimits::default()
            ),
            Err(FsError::BadArchive(_))
        ));
        assert!(!base.path().join("root/out").exists());
    }
}
