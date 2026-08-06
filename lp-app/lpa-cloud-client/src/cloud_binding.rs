//! The record that ties one local project to one cloud project.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_cloud_api::HeadInfo;
use lpc_history::{ContentHash, PrefixedUid};
use lpfs::{FsError, LpFs, LpPath};
use serde::{Deserialize, Serialize};

use crate::sync_error::SyncError;

/// Where a binding lives inside a project's history root.
///
/// Inside the history root, not the package: the package is content-hashed,
/// and a sync bookkeeping file that changed the project's version hash every
/// time it was written would be its own kind of disaster. The studio round
/// may move this to the registry (the architecture sketch calls the binding
/// "registry-adjacent"); the shape is what matters, not the path.
pub const CLOUD_BINDING_PATH: &str = "/cloud-binding.json";

/// One local project's link to its cloud counterpart.
///
/// **Per project, not per folder** (D23): a folder is a local organizing
/// idea, and folder-level sync is a future bulk convenience over this
/// primitive. There is no `remote_url` field either — the project uid *is*
/// the identity and the link token, and it is preserved across copies (D17),
/// so a binding never needs to remember a second name for the same project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBinding {
    /// The cloud project this copy tracks. Always equal to the local
    /// project's own uid — identity is preserved, never re-minted.
    pub project: PrefixedUid,
    /// The head frontier as of the last exchange with the service. More
    /// than one entry means the service is holding a divergence.
    pub last_seen_heads: Vec<ContentHash>,
    /// The service event sequence number last observed.
    ///
    /// Recorded for the incremental `GetEvents { since }` path. The engine
    /// currently reads the log from 0 on every pull, because
    /// `ProjectHistory` replays from the origin and a client that spliced a
    /// suffix into a log it had not validated would be trusting arithmetic
    /// over content. Making the read incremental is an optimization, not a
    /// semantic change.
    pub last_event_seq: u64,
}

impl CloudBinding {
    /// A binding for a project that has not exchanged anything yet.
    pub fn new(project: PrefixedUid) -> Self {
        Self {
            project,
            last_seen_heads: Vec::new(),
            last_event_seq: 0,
        }
    }

    /// Read a project's binding from its history root, if it has one.
    pub fn load(history_fs: &dyn LpFs) -> Result<Option<Self>, SyncError> {
        let bytes = match history_fs.read_file(LpPath::new(CLOUD_BINDING_PATH)) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let binding =
            serde_json::from_slice(&bytes).map_err(|e| SyncError::Encode(e.to_string()))?;
        Ok(Some(binding))
    }

    /// Write a project's binding into its history root.
    pub fn save(&self, history_fs: &dyn LpFs) -> Result<(), SyncError> {
        let bytes = serde_json::to_vec(self).map_err(|e| SyncError::Encode(e.to_string()))?;
        history_fs.write_file(LpPath::new(CLOUD_BINDING_PATH), &bytes)?;
        Ok(())
    }

    /// Record the frontier reported by the service.
    pub fn observe_heads(&mut self, heads: &[HeadInfo]) {
        self.last_seen_heads = heads.iter().map(|head| head.tree).collect();
    }

    /// Whether the service was last seen holding a divergence.
    pub fn is_diverged_remotely(&self) -> bool {
        self.last_seen_heads.len() > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;
    use lpfs::LpFsMemory;

    #[test]
    fn round_trips_through_the_history_root() {
        let fs = LpFsMemory::new();
        assert_eq!(CloudBinding::load(&fs).unwrap(), None);

        let mut binding = CloudBinding::new(project());
        binding.last_seen_heads = alloc::vec![ContentHash::of(b"head")];
        binding.last_event_seq = 7;
        binding.save(&fs).unwrap();

        assert_eq!(CloudBinding::load(&fs).unwrap(), Some(binding));
    }

    #[test]
    fn observing_the_frontier_records_every_head() {
        let mut binding = CloudBinding::new(project());
        assert!(!binding.is_diverged_remotely());

        binding.observe_heads(&[
            HeadInfo {
                tree: ContentHash::of(b"mine"),
                parents: alloc::vec![],
            },
            HeadInfo {
                tree: ContentHash::of(b"theirs"),
                parents: alloc::vec![],
            },
        ]);
        assert_eq!(binding.last_seen_heads.len(), 2);
        assert!(binding.is_diverged_remotely());
    }

    fn project() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[1u8; 16])
    }
}
