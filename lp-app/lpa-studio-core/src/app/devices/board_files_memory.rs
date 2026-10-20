//! What this browser remembers about boards' files: the evidence that "its
//! files are gone" is news ([`super::FilesEvidence`]).
//!
//! A board's hello says when its last boot formatted its storage, and every
//! fresh install's first boot does. So the hello alone cannot tell a board
//! that lost its files from one that never had any (the connected feel
//! gate's finding 1). This record holds the rest of the evidence, read off
//! what Studio already sees:
//!
//! - the boards this tab saw run a project (live: a report of a loaded
//!   project, by the roster's handle);
//! - the registry rows that name a project this browser gave the board (a
//!   verified push), set by the controller from the library snapshot;
//! - the boards whose owner chose **Start fresh**, or factory reset here,
//!   by base MAC — kept in this browser's storage through a hook the web
//!   edge installs, so a reload does not bring the notice back. A board
//!   that runs a project again is forgotten here: its next loss is news.
//!
//! Nothing here reaches a board, the registry or a persisted format: the
//! fresh starts are a browser convenience, like the remembered Wi‑Fi
//! addresses.

use std::collections::BTreeSet;

use lpa_devices::DeviceId;

/// The browser storage key the fresh starts are kept under (the web edge's
/// `localStorage`). Never a persisted format of the product: losing it only
/// brings a "files are gone" notice back.
pub const FRESH_STARTS_STORAGE_KEY: &str = "lp.devices.fresh-starts.v1";

/// See the module docs.
#[derive(Clone, Debug, Default)]
pub struct BoardFilesMemory {
    /// Boards this tab has seen run a project, by the roster's handle.
    ran_a_project: BTreeSet<DeviceId>,
    /// Registry row keys whose association names a project this browser
    /// gave the board.
    given: BTreeSet<String>,
    /// Boards whose owner let their files go, by lowercase base MAC.
    fresh_starts: BTreeSet<String>,
    /// The fresh starts changed since the stored form was last taken.
    changed: bool,
}

impl BoardFilesMemory {
    /// Load the fresh starts from their stored form (a JSON list of base
    /// MACs). Anything that does not read is dropped: the cost is a notice
    /// shown again.
    pub fn load_fresh_starts(&mut self, json: &str) {
        self.fresh_starts = serde_json::from_str::<Vec<String>>(json)
            .unwrap_or_default()
            .into_iter()
            .map(|mac| mac.to_ascii_lowercase())
            .collect();
        self.changed = false;
    }

    /// The fresh starts' stored form, when they changed since the last
    /// take.
    pub fn take_changed_fresh_starts(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let list: Vec<&String> = self.fresh_starts.iter().collect();
        Some(serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()))
    }

    /// The registry rows that name a project this browser gave the board.
    pub fn set_given(&mut self, keys: impl IntoIterator<Item = String>) {
        self.given = keys.into_iter().collect();
    }

    /// `device` (base MAC `mac`) reported a project loaded: it holds files,
    /// and a fresh start chosen for it is over.
    pub fn note_running(&mut self, device: DeviceId, mac: Option<&str>) {
        self.ran_a_project.insert(device);
        if let Some(mac) = mac
            && self.fresh_starts.remove(&mac.to_ascii_lowercase())
        {
            self.changed = true;
        }
    }

    /// Its owner let the board's files go (Start fresh, or a factory reset
    /// here). Returns whether anything changed.
    pub fn start_fresh(&mut self, mac: &str) -> bool {
        let added = self.fresh_starts.insert(mac.to_ascii_lowercase());
        self.changed |= added;
        added
    }

    /// Whether this browser knows the board held files: it ran a project
    /// in this tab, this browser gave it one (`key`, its registry row), or
    /// it carried a stamped identity its hello no longer names
    /// (`identity_lost`).
    pub fn remembers(&self, device: DeviceId, key: Option<&str>, identity_lost: bool) -> bool {
        identity_lost
            || self.ran_a_project.contains(&device)
            || key.is_some_and(|key| self.given.contains(key))
    }

    /// Whether its owner let the board's files go since.
    pub fn started_fresh(&self, mac: Option<&str>) -> bool {
        mac.is_some_and(|mac| self.fresh_starts.contains(&mac.to_ascii_lowercase()))
    }

    /// Forget the boards `keep` lets go of (forgotten, merged away).
    pub fn retain(&mut self, keep: impl Fn(DeviceId) -> bool) {
        self.ran_a_project.retain(|device| keep(*device));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: &str = "A0:F2:62:87:B4:8C";

    #[test]
    fn a_board_is_remembered_by_what_this_browser_saw() {
        let mut memory = BoardFilesMemory::default();
        let board = DeviceId(4);
        assert!(!memory.remembers(board, Some("mac:a0f26287b48c"), false));
        assert!(memory.remembers(board, None, true), "an identity it lost");
        memory.set_given(["mac:a0f26287b48c".to_string()]);
        assert!(
            memory.remembers(board, Some("mac:a0f26287b48c"), false),
            "a project this browser gave it"
        );
        memory.set_given([]);
        memory.note_running(board, None);
        assert!(memory.remembers(board, None, false), "it ran one here");
    }

    #[test]
    fn a_fresh_start_is_kept_until_the_board_runs_a_project_again() {
        let mut memory = BoardFilesMemory::default();
        assert!(memory.start_fresh(MAC));
        assert!(!memory.start_fresh(MAC), "once");
        assert!(memory.started_fresh(Some("a0:f2:62:87:b4:8c")));
        let stored = memory.take_changed_fresh_starts().expect("changed");
        assert_eq!(stored, r#"["a0:f2:62:87:b4:8c"]"#);
        assert_eq!(memory.take_changed_fresh_starts(), None, "taken");

        let mut reloaded = BoardFilesMemory::default();
        reloaded.load_fresh_starts(&stored);
        assert!(reloaded.started_fresh(Some(MAC)), "a reload keeps it");
        reloaded.note_running(DeviceId(1), Some(MAC));
        assert!(!reloaded.started_fresh(Some(MAC)), "files again: over");
        assert_eq!(reloaded.take_changed_fresh_starts().as_deref(), Some("[]"));

        let mut garbled = BoardFilesMemory::default();
        garbled.load_fresh_starts("not json");
        assert!(!garbled.started_fresh(Some(MAC)));
    }
}
