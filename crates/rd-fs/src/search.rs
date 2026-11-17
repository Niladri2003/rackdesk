//! Search for files and folders by name.
//!
//! There is no index. A search walks the tree when it is asked for, level by
//! level from the start folder, and stops at the first limit it reaches. So
//! it uses no memory or CPU while nobody searches.

use crate::{clean_path, display_path, to_entry, FsError, Root};
use rd_api::{SearchHit, SearchResults};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct SearchLimits {
    pub max_hits: usize,
    /// Folders read before the search stops.
    pub max_dirs: usize,
    pub max_time: Duration,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_hits: 50,
            max_dirs: 20_000,
            max_time: Duration::from_millis(1500),
        }
    }
}

/// Folders at the top of a Linux system that hold no real files. A walk into
/// them is slow and its results are noise.
const PSEUDO_DIRS: &[&str] = &["proc", "sys", "dev", "run"];

impl Root {
    /// Finds entries beneath `ui_path` whose name contains `query`, without
    /// regard to letter case.
    ///
    /// Symlinks are reported but never followed, so the walk can't loop or
    /// leave the root. Folders whose name starts with a dot are reported but
    /// not entered. Folders that can't be read are skipped.
    pub fn search(
        &self,
        ui_path: &str,
        query: &str,
        limits: &SearchLimits,
    ) -> Result<SearchResults, FsError> {
        let start = clean_path(ui_path)?;
        let query = query.trim().to_lowercase();
        let mut results = SearchResults {
            hits: Vec::new(),
            truncated: false,
        };
        if query.is_empty() {
            return Ok(results);
        }
        // The start folder must exist. Folders below it may fail quietly.
        self.dir.open_dir(&start)?;

        let deadline = Instant::now() + limits.max_time;
        let mut queue: VecDeque<PathBuf> = VecDeque::from([start]);
        let mut visited = 0usize;
        while let Some(rel) = queue.pop_front() {
            if visited >= limits.max_dirs || Instant::now() >= deadline {
                results.truncated = true;
                break;
            }
            visited += 1;
            let Ok(dir) = self.dir.open_dir(&rel) else {
                continue;
            };
            let Ok(entries) = dir.entries() else {
                continue;
            };
            let at_top = rel == Path::new(".");
            for item in entries.flatten() {
                let name = item.file_name().to_string_lossy().into_owned();
                let Ok(file_type) = item.file_type() else {
                    continue;
                };
                if name.to_lowercase().contains(&query) {
                    if results.hits.len() >= limits.max_hits {
                        results.truncated = true;
                        return Ok(results);
                    }
                    if let Ok(meta) = dir.symlink_metadata(&name) {
                        results.hits.push(SearchHit {
                            dir: display_path(&rel),
                            entry: to_entry(&dir, name.clone(), &meta),
                        });
                    }
                }
                let enter = file_type.is_dir()
                    && !name.starts_with('.')
                    && !(at_top && PSEUDO_DIRS.contains(&name.as_str()));
                if enter {
                    queue.push_back(if at_top {
                        PathBuf::from(&name)
                    } else {
                        rel.join(&name)
                    });
                }
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let p = root.path();
        fs::create_dir_all(p.join("home/dev/projects/api")).unwrap();
        fs::create_dir_all(p.join("etc/nginx")).unwrap();
        fs::create_dir_all(p.join("home/dev/.cache/deep")).unwrap();
        fs::write(p.join("home/dev/projects/docker-compose.yml"), "x").unwrap();
        fs::write(p.join("home/dev/projects/api/Compose.md"), "x").unwrap();
        fs::write(p.join("etc/nginx/nginx.conf"), "x").unwrap();
        fs::write(p.join("home/dev/.cache/deep/compose.tmp"), "x").unwrap();
        root
    }

    fn found(r: &SearchResults) -> Vec<String> {
        let mut v: Vec<_> = r
            .hits
            .iter()
            .map(|h| format!("{}/{}", h.dir.trim_end_matches('/'), h.entry.name))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn finds_names_at_any_depth_without_regard_to_case() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/", "compose", &SearchLimits::default()).unwrap();
        assert_eq!(
            found(&res),
            [
                "/home/dev/projects/api/Compose.md",
                "/home/dev/projects/docker-compose.yml"
            ]
        );
        assert!(!res.truncated);
    }

    #[test]
    fn starts_from_the_given_folder() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/etc", "conf", &SearchLimits::default()).unwrap();
        assert_eq!(found(&res), ["/etc/nginx/nginx.conf"]);
        assert!(matches!(
            r.search("/nope", "x", &SearchLimits::default()),
            Err(FsError::NotFound)
        ));
        assert!(matches!(
            r.search("/../etc", "x", &SearchLimits::default()),
            Err(FsError::InvalidPath(_))
        ));
    }

    #[test]
    fn folders_are_found_too_and_hidden_folders_are_not_entered() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/", "cache", &SearchLimits::default()).unwrap();
        assert_eq!(found(&res), ["/home/dev/.cache"]);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/", "  ", &SearchLimits::default()).unwrap();
        assert!(res.hits.is_empty() && !res.truncated);
    }

    #[test]
    fn stops_at_the_hit_limit() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(root.path().join(format!("note-{i}.txt")), "").unwrap();
        }
        let r = Root::open(root.path()).unwrap();
        let limits = SearchLimits {
            max_hits: 5,
            ..SearchLimits::default()
        };
        let res = r.search("/", "note", &limits).unwrap();
        assert_eq!(res.hits.len(), 5);
        assert!(res.truncated);
    }

    #[test]
    fn stops_at_the_folder_limit() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let limits = SearchLimits {
            max_dirs: 2,
            ..SearchLimits::default()
        };
        let res = r.search("/", "compose", &limits).unwrap();
        assert!(res.truncated);
        assert!(res.hits.is_empty());
    }

    #[test]
    fn stops_at_the_time_limit() {
        let root = tree();
        let r = Root::open(root.path()).unwrap();
        let limits = SearchLimits {
            max_time: Duration::ZERO,
            ..SearchLimits::default()
        };
        assert!(r.search("/", "compose", &limits).unwrap().truncated);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_reported_but_never_followed() {
        let root = tree();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("compose-secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("compose-link")).unwrap();
        // A link to a parent folder would make an endless walk if followed.
        std::os::unix::fs::symlink("..", root.path().join("home/loop")).unwrap();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/", "compose", &SearchLimits::default()).unwrap();
        assert_eq!(
            found(&res),
            [
                "/compose-link",
                "/home/dev/projects/api/Compose.md",
                "/home/dev/projects/docker-compose.yml"
            ]
        );
        assert!(!res.truncated);
    }

    #[test]
    fn top_level_system_folders_are_not_entered() {
        let root = tree();
        fs::create_dir_all(root.path().join("proc/1")).unwrap();
        fs::write(root.path().join("proc/1/compose"), "x").unwrap();
        fs::create_dir_all(root.path().join("home/proc")).unwrap();
        fs::write(root.path().join("home/proc/compose-notes"), "x").unwrap();
        let r = Root::open(root.path()).unwrap();
        let res = r.search("/", "compose", &SearchLimits::default()).unwrap();
        assert!(found(&res).contains(&"/home/proc/compose-notes".to_string()));
        assert!(!found(&res).iter().any(|p| p.starts_with("/proc/")));
    }
}
