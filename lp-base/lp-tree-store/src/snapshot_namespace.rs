//! The reserved snapshot namespace (FORMAT.md "Files and paths", "The
//! snapshot namespace"). Version 3 builds no snapshot: it reserves the path
//! so that a later core can keep snapshots there as ordinary directory
//! entries, which every core already traces, collects around and lists as
//! files, without a format change. Until then nothing but that feature may
//! write under it (`LpFsTree` refuses).

/// Where snapshots live: `/.lp/snapshots/<project>/<name>`, each a directory
/// entry naming a directory node that held the project at that moment.
pub const SNAPSHOT_DIR: &str = "/.lp/snapshots";

/// Whether `path` (absolute, normalized) is the snapshot directory or lies
/// under it.
pub fn is_snapshot_path(path: &str) -> bool {
    path.strip_prefix(SNAPSHOT_DIR)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snapshot_directory_and_what_is_under_it() {
        assert!(is_snapshot_path("/.lp/snapshots"));
        assert!(is_snapshot_path("/.lp/snapshots/p/2026-10-16/a.json"));
        assert!(!is_snapshot_path("/.lp/snapshotsx/a"));
        assert!(!is_snapshot_path("/.lp/snapshot"));
        assert!(!is_snapshot_path("/.lp/access.json"));
        assert!(!is_snapshot_path("/projects/p/.lp/snapshots/a"));
    }
}
