//! Which filesystem paths are access files.
//!
//! The fs gate refuses to return the bytes of any access file, on any link,
//! at any tier — the device store at `/.lp/access.json` and every project's
//! sidecar at `<project>/.lp/access.json` alike. The predicate is therefore
//! "the last two components are `.lp` / `access.json`", wherever they sit.
//!
//! It is deliberately generous about spelling, because a gate that a path
//! trick walks around is no gate:
//!
//! - `.` and `..` components are resolved first, so
//!   `/projects/x/.lp/../.lp/access.json` is caught;
//! - repeated and trailing slashes are ignored;
//! - the comparison ignores ASCII case, because a host server's filesystem
//!   (macOS by default) may be case-insensitive and would happily serve
//!   `/.LP/Access.JSON`.

use alloc::vec::Vec;

/// Directory name of the reserved metadata namespace.
const META_DIR: &str = ".lp";

/// File name of an access file inside it.
const ACCESS_FILE_NAME: &str = "access.json";

/// Whether `path` names an access file (see the module docs for what
/// spellings count).
#[must_use]
pub fn is_access_file_path(path: &str) -> bool {
    let components = resolved_components(path);
    match components.as_slice() {
        [.., dir, file] => {
            dir.eq_ignore_ascii_case(META_DIR) && file.eq_ignore_ascii_case(ACCESS_FILE_NAME)
        }
        _ => false,
    }
}

/// Whether `path` is a directory that could contain an access file directly,
/// i.e. a `.lp` directory (resolved the same way as [`is_access_file_path`]).
#[must_use]
pub fn is_meta_dir_path(path: &str) -> bool {
    let components = resolved_components(path);
    components
        .last()
        .is_some_and(|last| last.eq_ignore_ascii_case(META_DIR))
}

/// Path components with `.`/`..`/empty resolved away. `..` above the root
/// stays at the root, as a filesystem would.
fn resolved_components(path: &str) -> Vec<&str> {
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            name => components.push(name),
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_real_locations() {
        assert!(is_access_file_path("/.lp/access.json"));
        assert!(is_access_file_path("/projects/choker/.lp/access.json"));
        assert!(is_access_file_path(".lp/access.json"));
    }

    #[test]
    fn spellings_that_still_reach_the_file() {
        assert!(is_access_file_path("//.lp//access.json"));
        assert!(is_access_file_path("/projects/x/.lp/./access.json"));
        assert!(is_access_file_path("/projects/x/.lp/../.lp/access.json"));
        assert!(is_access_file_path("/projects/x/../../.lp/access.json"));
        assert!(is_access_file_path("/.LP/Access.JSON"));
        assert!(is_access_file_path("/.lp/access.json/"));
    }

    #[test]
    fn neighbours_are_not_access_files() {
        assert!(!is_access_file_path("/.lp/state.json"));
        assert!(!is_access_file_path("/.lp/device.json"));
        assert!(!is_access_file_path("/.lp"));
        assert!(!is_access_file_path("/access.json"));
        assert!(!is_access_file_path("/projects/x/lp/access.json"));
        assert!(!is_access_file_path("/projects/x/.lpx/access.json"));
        assert!(!is_access_file_path("/.lp/access.json.bak"));
        assert!(!is_access_file_path("/.lp/access.json/.."));
        assert!(!is_access_file_path(""));
        assert!(!is_access_file_path("/"));
    }

    #[test]
    fn meta_dirs() {
        assert!(is_meta_dir_path("/.lp"));
        assert!(is_meta_dir_path("/projects/x/.lp/"));
        assert!(is_meta_dir_path("/projects/x/.LP"));
        assert!(!is_meta_dir_path("/projects/x"));
        assert!(!is_meta_dir_path("/.lp/access.json"));
    }
}
