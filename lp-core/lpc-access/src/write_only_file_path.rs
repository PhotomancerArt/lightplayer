//! Which filesystem paths are write-only: their bytes never leave the
//! device.
//!
//! The fs gate refuses to return the bytes of a write-only file, on any
//! link, at any tier. Two files are write-only:
//!
//! - **access files** — the device store at `/.lp/access.json` and every
//!   project's sidecar at `<project>/.lp/access.json` alike;
//! - **the network file** — `/.lp/network.json`, which holds the Wi-Fi
//!   password ([`crate::NetworkFile`]).
//!
//! The predicate is therefore "the last two components are `.lp` /
//! `access.json` or `.lp` / `network.json`", wherever they sit (a
//! `network.json` inside a project's `.lp/` is caught too: generous is the
//! safe side).
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

/// File names inside it whose bytes never leave the device: an access file
/// and the network file.
const WRITE_ONLY_FILE_NAMES: [&str; 2] = ["access.json", "network.json"];

/// Whether `path` names a write-only file — an access file or the network
/// file (see the module docs for what spellings count).
#[must_use]
pub fn is_write_only_file_path(path: &str) -> bool {
    let components = resolved_components(path);
    match components.as_slice() {
        [.., dir, file] => {
            dir.eq_ignore_ascii_case(META_DIR)
                && WRITE_ONLY_FILE_NAMES
                    .iter()
                    .any(|name| file.eq_ignore_ascii_case(name))
        }
        _ => false,
    }
}

/// Whether `path`, once resolved, is `dir` itself or lies beneath it — the
/// check that keeps a "project files" permission from being walked out of
/// with `..` (`/projects/../hardware.json` is NOT within `/projects`).
/// Leading slashes do not matter; ASCII case does (a device filesystem is
/// case-sensitive, and a case-insensitive host only widens what a
/// case-sensitive comparison already refuses).
#[must_use]
pub fn is_within_dir(path: &str, dir: &str) -> bool {
    let path = resolved_components(path);
    let dir = resolved_components(dir);
    path.len() >= dir.len() && path[..dir.len()] == dir[..]
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
    fn the_two_real_access_file_locations() {
        assert!(is_write_only_file_path("/.lp/access.json"));
        assert!(is_write_only_file_path("/projects/choker/.lp/access.json"));
        assert!(is_write_only_file_path(".lp/access.json"));
    }

    #[test]
    fn spellings_that_still_reach_an_access_file() {
        assert!(is_write_only_file_path("//.lp//access.json"));
        assert!(is_write_only_file_path("/projects/x/.lp/./access.json"));
        assert!(is_write_only_file_path(
            "/projects/x/.lp/../.lp/access.json"
        ));
        assert!(is_write_only_file_path("/projects/x/../../.lp/access.json"));
        assert!(is_write_only_file_path("/.LP/Access.JSON"));
        assert!(is_write_only_file_path("/.lp/access.json/"));
    }

    #[test]
    fn the_network_file_is_write_only() {
        assert!(is_write_only_file_path("/.lp/network.json"));
        assert!(is_write_only_file_path(".lp/network.json"));
        assert!(is_write_only_file_path("/projects/x/.lp/network.json"));
    }

    #[test]
    fn spellings_that_still_reach_the_network_file() {
        assert!(is_write_only_file_path("//.lp//network.json/"));
        assert!(is_write_only_file_path("/.lp/../.lp/network.json"));
        assert!(is_write_only_file_path("/.lp/./network.json"));
        assert!(is_write_only_file_path("/projects/../.lp/network.json"));
        assert!(is_write_only_file_path("/.LP/Network.JSON"));
    }

    #[test]
    fn neighbours_are_not_write_only_files() {
        assert!(!is_write_only_file_path("/.lp/state.json"));
        assert!(!is_write_only_file_path("/.lp/device.json"));
        assert!(!is_write_only_file_path("/.lp"));
        assert!(!is_write_only_file_path("/access.json"));
        assert!(!is_write_only_file_path("/projects/x/lp/access.json"));
        assert!(!is_write_only_file_path("/projects/x/.lpx/access.json"));
        assert!(!is_write_only_file_path("/.lp/access.json.bak"));
        assert!(!is_write_only_file_path("/.lp/access.json/.."));
        assert!(!is_write_only_file_path(""));
        assert!(!is_write_only_file_path("/"));
    }

    #[test]
    fn neighbours_of_the_network_file_are_not_write_only() {
        assert!(!is_write_only_file_path("/.lp/network.json.bak"));
        assert!(!is_write_only_file_path("/network.json"));
        assert!(!is_write_only_file_path("/.lp/networks.json"));
        assert!(!is_write_only_file_path("/.lp/network.json/.."));
        assert!(!is_write_only_file_path("/lp/network.json"));
    }

    #[test]
    fn within_dir_resolves_before_comparing() {
        assert!(is_within_dir("/projects/x/a.json", "/projects"));
        assert!(is_within_dir("/projects", "projects/"));
        assert!(is_within_dir("projects/x", "/projects/"));
        assert!(!is_within_dir("/projects/../hardware.json", "/projects"));
        assert!(!is_within_dir("/projectsx/a", "/projects"));
        assert!(!is_within_dir("/", "/projects"));
        assert!(is_within_dir("/anything", "/"));
    }
}
