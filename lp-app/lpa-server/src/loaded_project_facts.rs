//! What a board says about what it plays, read off the server between
//! ticks with `&self` only: the first loaded project's name and uid, and a
//! picture of its outputs.
//!
//! The cloud relay's board side asks (its `Project` frame and its `Picture`
//! frame; the C6's frame hook and lp-cli's host board): both are made on
//! the main thread, from a hook that holds `&LpServer`. A board plays one
//! project; when several are loaded (a host), the lowest handle — the
//! first loaded — is the one it reports.
//!
//! Nothing here allocates: the facts are borrowed, and the picture is
//! appended to buffers the caller keeps (`lpc_engine`'s
//! `output_picture_lamps` / `append_output_picture`, whose doc says what
//! a sample's colour means). Nothing here logs the uid: it is a read
//! capability.

extern crate alloc;

use alloc::vec::Vec;

use crate::project::Project;
use crate::server::LpServer;

/// What a board says about the project it plays (the relay's `Project`
/// frame). Borrowed: no allocation per call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LoadedProjectFacts<'a> {
    /// project.json's `name`, else the folder's (the last component of the
    /// project's path, as the board's advertised name already is).
    pub name: &'a str,
    /// project.json's `uid`, when it has one. A read capability: never log
    /// it.
    pub uid: Option<&'a str>,
}

/// The uid is a capability: never in a log line.
impl core::fmt::Debug for LoadedProjectFacts<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoadedProjectFacts")
            .field("name", &self.name)
            .field("has_uid", &self.uid.is_some())
            .finish()
    }
}

impl<'a> LoadedProjectFacts<'a> {
    /// `project`'s facts: its manifest's identity, the folder's name when
    /// the manifest names none.
    #[must_use]
    pub fn of(project: &'a Project) -> Self {
        let identity = project.registry().identity();
        let name = identity
            .name
            .as_deref()
            .unwrap_or_else(|| folder_name(project.path().as_str()));
        Self {
            name,
            uid: identity.uid.as_deref(),
        }
    }
}

impl LpServer {
    /// The first loaded project's facts, or `None` when nothing is loaded.
    #[must_use]
    pub fn loaded_project_facts(&self) -> Option<LoadedProjectFacts<'_>> {
        self.project_manager()
            .first_loaded()
            .map(LoadedProjectFacts::of)
    }

    /// Lamps per published output of the first loaded project, at most
    /// `max_outputs`, into `lamps` (cleared first); none when nothing is
    /// loaded. See `Engine::output_picture_lamps`.
    pub fn output_picture_lamps(&self, max_outputs: usize, lamps: &mut Vec<u32>) {
        match self.project_manager().first_loaded() {
            Some(project) => project.engine().output_picture_lamps(max_outputs, lamps),
            None => lamps.clear(),
        }
    }

    /// Append `count` colour samples of the first loaded project's outputs
    /// (the ones `lamps` describes) to `rgb`; exactly `3·count` bytes, black
    /// where there is nothing to read. See `Engine::append_output_picture`.
    pub fn append_output_picture(&self, lamps: &[u32], count: u32, rgb: &mut Vec<u8>) {
        match self.project_manager().first_loaded() {
            Some(project) => project.engine().append_output_picture(lamps, count, rgb),
            None => rgb.resize(rgb.len() + count as usize * 3, 0),
        }
    }
}

/// The last non-empty component of a project's path (`/projects/basic/` →
/// `basic`).
fn folder_name(path: &str) -> &str {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_name_is_the_last_component() {
        assert_eq!(folder_name("/projects/basic"), "basic");
        assert_eq!(folder_name("/projects/basic/"), "basic");
        assert_eq!(folder_name("basic"), "basic");
    }

    #[test]
    fn debug_never_prints_the_uid() {
        let facts = LoadedProjectFacts {
            name: "Rocaille",
            uid: Some("prj7m3qk2x9z4w8v6t5r1n0p2a4c"),
        };
        let text = alloc::format!("{facts:?}");
        assert!(text.contains("Rocaille"), "{text}");
        assert!(!text.contains("prj7m3qk"), "{text}");
    }
}
