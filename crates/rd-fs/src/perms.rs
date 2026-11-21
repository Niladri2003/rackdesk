//! Owner and permission bits of an entry.
//!
//! Only the 9 plain permission bits can be set here: read, write and execute
//! for the owner, the group and others. The set-uid, set-gid and sticky bits
//! are refused. A set-uid file owned by root would give its rights to every
//! user who runs it.

use crate::{FsError, Root};

/// The bits that `set_mode` accepts.
pub const PLAIN_BITS: u32 = 0o777;

impl Root {
    /// The user id and group id that own an entry. A symlink is described,
    /// not followed.
    #[cfg(unix)]
    pub fn owner(&self, ui_path: &str) -> Result<(u32, u32), FsError> {
        use cap_std::fs::MetadataExt;
        let meta = if crate::clean_path(ui_path)? == std::path::Path::new(".") {
            self.dir.dir_metadata()?
        } else {
            let place = self.place(ui_path)?;
            place.dir.symlink_metadata(&place.name)?
        };
        Ok((meta.uid(), meta.gid()))
    }

    /// Sets the permission bits of a file or folder.
    #[cfg(unix)]
    pub fn set_mode(&self, ui_path: &str, mode: u32) -> Result<(), FsError> {
        use cap_std::fs::{Permissions, PermissionsExt};
        if mode & !PLAIN_BITS != 0 {
            return Err(FsError::InvalidPath(
                "only the 9 plain permission bits can be set",
            ));
        }
        let place = self.place(ui_path)?;
        // A change through a link would land on its target. cap-std keeps
        // the target beneath the root, but it would not be the entry that
        // the user chose.
        if place.dir.symlink_metadata(&place.name)?.is_symlink() {
            return Err(FsError::NotAFile);
        }
        place
            .dir
            .set_permissions(&place.name, Permissions::from_mode(mode))?;
        Ok(())
    }

    /// Sets the owner, the group, or both. A symlink is changed itself, not
    /// its target. The system allows this only for root, except that a user
    /// can give a file to a group they are in.
    #[cfg(unix)]
    pub fn set_owner(
        &self,
        ui_path: &str,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<(), FsError> {
        use rustix::fs::{chownat, AtFlags, Gid, Uid};
        let place = self.place(ui_path)?;
        chownat(
            &place.dir,
            &place.name,
            uid.map(Uid::from_raw),
            gid.map(Gid::from_raw),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|e| FsError::from(std::io::Error::from(e)))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn fixture() -> (tempfile::TempDir, Root) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("run.sh"), "#!/bin/sh").unwrap();
        fs::write(base.path().join("outside.txt"), "outside").unwrap();
        fs::set_permissions(
            base.path().join("outside.txt"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        (base, Root::open(&root).unwrap())
    }

    fn mode(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn set_mode_changes_plain_bits_and_refuses_special_bits() {
        let (base, r) = fixture();
        let file = base.path().join("root/run.sh");
        r.set_mode("/run.sh", 0o750).unwrap();
        assert_eq!(mode(&file), 0o750);
        for special in [0o4755, 0o2755, 0o1777, 0o10000] {
            assert!(r.set_mode("/run.sh", special).is_err(), "{special:o}");
        }
        assert_eq!(mode(&file), 0o750);
    }

    #[test]
    fn changes_do_not_reach_outside_the_root() {
        let (base, r) = fixture();
        let outside = base.path().join("outside.txt");
        std::os::unix::fs::symlink(&outside, base.path().join("root/link")).unwrap();
        std::os::unix::fs::symlink(base.path(), base.path().join("root/escape")).unwrap();
        let (uid, gid) = r.owner("/run.sh").unwrap();

        assert!(r.set_mode("/link", 0o777).is_err());
        assert!(r.set_mode("/escape/outside.txt", 0o777).is_err());
        assert!(r.set_mode("/../outside.txt", 0o777).is_err());
        assert!(r
            .set_owner("/escape/outside.txt", Some(uid), Some(gid))
            .is_err());
        assert!(r
            .set_owner("/../outside.txt", Some(uid), Some(gid))
            .is_err());
        assert!(r.owner("/escape/outside.txt").is_err());
        assert_eq!(mode(&outside), 0o644);
    }

    #[test]
    fn set_owner_to_the_current_owner_succeeds() {
        let (_base, r) = fixture();
        let (uid, gid) = r.owner("/run.sh").unwrap();
        r.set_owner("/run.sh", Some(uid), Some(gid)).unwrap();
        r.set_owner("/run.sh", None, None).unwrap();
        assert_eq!(r.owner("/run.sh").unwrap(), (uid, gid));
        assert_eq!(r.owner("/").unwrap().0, uid);
    }
}
