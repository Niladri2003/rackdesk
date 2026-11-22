//! The version of the data folder, and the steps that bring an older folder
//! up to date.
//!
//! The data folder holds the account, the audit log and the HTTPS files. A
//! file named `version` in it holds 1 number: the layout of the folder. A new
//! release that changes the layout adds a step to [`MIGRATIONS`]. At start,
//! the steps that the folder has not had yet run in order.
//!
//! A folder written by a newer release is refused. An older program could
//! misread it or damage it.

use anyhow::{bail, Context};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// One step: it changes a folder of version `n` into version `n + 1`. It must
/// be safe to run again after it failed half way.
type Migration = fn(&Path) -> io::Result<()>;

/// `MIGRATIONS[n]` changes a folder of version `n` into version `n + 1`.
const MIGRATIONS: &[Migration] = &[adopt_unversioned];

/// The layout that this program reads and writes.
pub const CURRENT: u32 = MIGRATIONS.len() as u32;

const VERSION_FILE: &str = "version";

/// Version 0 is a folder with no `version` file: a new folder, or one written
/// before the file existed. Its files already have the layout of version 1.
fn adopt_unversioned(_: &Path) -> io::Result<()> {
    Ok(())
}

/// Creates the data folder if it is missing and brings it up to date. Call
/// it before anything reads the folder.
pub fn prepare(dir: &Path) -> anyhow::Result<()> {
    prepare_with(dir, MIGRATIONS)
}

fn prepare_with(dir: &Path, migrations: &[Migration]) -> anyhow::Result<()> {
    create_private(dir).with_context(|| format!("creating {}", dir.display()))?;
    let current = migrations.len() as u32;
    let found = read_version(dir)?;
    if found > current {
        bail!(
            "the data folder {} has version {found}, and this program reads up to version {current}. \
             It was written by a newer RackDesk. Install that release again, or restore a backup of the folder",
            dir.display()
        );
    }
    for from in found..current {
        migrations[from as usize](dir).with_context(|| {
            format!(
                "updating the data folder {} from version {from} to {}",
                dir.display(),
                from + 1
            )
        })?;
        // Written after each step, so a failure in a later step does not
        // run the earlier ones again.
        write_version(dir, from + 1)
            .with_context(|| format!("writing the version of {}", dir.display()))?;
    }
    Ok(())
}

/// The folder is made for its owner only: it holds the password hash and the
/// HTTPS key. An existing folder keeps its permissions.
fn create_private(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// 0 when the file is missing. Content that is not a number is an error: a
/// guess could run steps on a folder that already had them.
fn read_version(dir: &Path) -> anyhow::Result<u32> {
    let path = dir.join(VERSION_FILE);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    raw.trim()
        .parse()
        .with_context(|| format!("{} does not hold a version number", path.display()))
}

/// Written under a temporary name and renamed, so the file is never half
/// written. The temporary file is created with `create_new`, which fails if
/// anything has that name: a link placed there can't redirect the write.
fn write_version(dir: &Path, version: u32) -> io::Result<()> {
    let tmp = dir.join(format!(".{VERSION_FILE}-{}.tmp", std::process::id()));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| writeln!(file, "{version}").and_then(|_| file.sync_all()))
        .and_then(|_| fs::rename(&tmp, dir.join(VERSION_FILE)));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// Refuses to continue as root on a data folder that another user owns.
///
/// The service user can write in its data folder. If root wrote there too,
/// that user could place a link in the folder and make root write to any
/// file on the machine. Run the command as the owner of the folder.
#[cfg(unix)]
pub fn refuse_root_on_a_folder_of_another_user(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if rustix::process::geteuid().as_raw() != 0 {
        return Ok(());
    }
    let Ok(folder) = fs::metadata(dir) else {
        // The folder does not exist yet. Root creates it and owns it.
        return Ok(());
    };
    if folder.uid() != 0 {
        bail!(
            "the data folder {} belongs to user id {}, and this command runs as root. \
             Run it as that user: sudo -u '#{}' rackdeskd --data-dir {} ...",
            dir.display(),
            folder.uid(),
            folder.uid(),
            dir.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn refuse_root_on_a_folder_of_another_user(_: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version_in(dir: &Path) -> String {
        fs::read_to_string(dir.join(VERSION_FILE)).unwrap()
    }

    fn touch_a(dir: &Path) -> io::Result<()> {
        fs::write(dir.join("a"), "a")
    }

    fn touch_b(dir: &Path) -> io::Result<()> {
        fs::write(dir.join("b"), "b")
    }

    fn fail(_: &Path) -> io::Result<()> {
        Err(io::Error::other("step failed"))
    }

    #[test]
    fn a_new_folder_gets_the_current_version() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("data");
        prepare(&dir).unwrap();
        assert_eq!(version_in(&dir).trim(), CURRENT.to_string());
    }

    #[cfg(unix)]
    #[test]
    fn a_new_folder_is_for_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("data");
        prepare(&dir).unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    /// Needs root, so it runs in `make test-linux` and not on a developer
    /// machine.
    #[cfg(unix)]
    #[test]
    fn root_is_refused_on_a_folder_of_another_user() {
        if rustix::process::geteuid().as_raw() != 0 {
            eprintln!("skipped: this test needs root");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        assert!(refuse_root_on_a_folder_of_another_user(tmp.path()).is_ok());
        std::os::unix::fs::chown(tmp.path(), Some(12345), Some(12345)).unwrap();
        assert!(refuse_root_on_a_folder_of_another_user(tmp.path()).is_err());
        assert!(refuse_root_on_a_folder_of_another_user(&tmp.path().join("missing")).is_ok());
    }

    #[test]
    fn a_folder_from_before_the_version_file_keeps_its_data() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("credentials.json"), "{}").unwrap();
        prepare(tmp.path()).unwrap();
        assert_eq!(version_in(tmp.path()).trim(), CURRENT.to_string());
        assert_eq!(
            fs::read_to_string(tmp.path().join("credentials.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn only_the_missing_steps_run() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(VERSION_FILE), "1\n").unwrap();
        prepare_with(tmp.path(), &[touch_a, touch_b]).unwrap();
        assert!(!tmp.path().join("a").exists());
        assert!(tmp.path().join("b").exists());
        assert_eq!(version_in(tmp.path()).trim(), "2");
    }

    #[test]
    fn a_newer_folder_is_refused_and_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(VERSION_FILE), "9\n").unwrap();
        let error = prepare_with(tmp.path(), &[touch_a]).unwrap_err();
        assert!(error.to_string().contains("newer RackDesk"));
        assert!(!tmp.path().join("a").exists());
        assert_eq!(version_in(tmp.path()), "9\n");
    }

    #[test]
    fn a_version_file_with_other_content_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(VERSION_FILE), "latest").unwrap();
        assert!(prepare_with(tmp.path(), &[touch_a]).is_err());
        assert!(!tmp.path().join("a").exists());
    }

    #[test]
    fn a_failed_step_keeps_the_version_of_the_last_good_step() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(prepare_with(tmp.path(), &[touch_a, fail]).is_err());
        assert_eq!(version_in(tmp.path()).trim(), "1");
    }
}
