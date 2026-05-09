use alloc::collections::{BTreeMap, BTreeSet};

use lpc_model::NodeKind;
use lpc_model::{LpPathBuf, NodeId, Revision};
use lpc_wire::{WireNodeSlotRoot, WireNodeStatus, WireSlotWatchSpecifier};

use super::resource_cache::ClientResourceCache;

/// Status change information surfaced by future canonical project sync.
#[derive(Debug, Clone)]
pub struct StatusChangeView {
    /// Node path.
    pub path: LpPathBuf,
    /// Previous status.
    pub old_status: WireNodeStatus,
    /// New status.
    pub new_status: WireNodeStatus,
}

/// Minimal project view shell between M2.2 demolition and M4 project view rebuild.
///
/// TODO(M4 project view rebuild): make this own the canonical node index, slot mirror, watch
/// state, and resource cache updates from canonical project sync.
pub struct ProjectView {
    /// Current revision, once canonical sync exists.
    pub revision: Revision,
    /// Minimal node index retained for callers that need a project-view shell.
    pub nodes: BTreeMap<NodeId, NodeEntryView>,
    /// Generic slot roots the client wants to watch.
    pub slot_watch_roots: BTreeSet<WireNodeSlotRoot>,
    /// Cached resource summaries and payloads.
    pub resource_cache: ClientResourceCache,
}

/// Minimal node entry retained until canonical project sync is rebuilt.
pub struct NodeEntryView {
    pub path: LpPathBuf,
    pub kind: NodeKind,
    pub status: WireNodeStatus,
    pub status_ver: Revision,
}

impl ProjectView {
    /// Create an empty project view shell.
    pub fn new() -> Self {
        Self {
            revision: Revision::default(),
            nodes: BTreeMap::new(),
            slot_watch_roots: BTreeSet::new(),
            resource_cache: ClientResourceCache::new(),
        }
    }

    /// Start watching one generic slot root.
    pub fn watch_slot_root(&mut self, root: WireNodeSlotRoot) {
        self.slot_watch_roots.insert(root);
    }

    /// Stop watching one generic slot root.
    pub fn unwatch_slot_root(&mut self, root: WireNodeSlotRoot) {
        self.slot_watch_roots.remove(&root);
    }

    /// Generate the generic slot watch specifier for future canonical sync.
    pub fn slot_watch_specifier(&self) -> WireSlotWatchSpecifier {
        if self.slot_watch_roots.is_empty() {
            WireSlotWatchSpecifier::None
        } else {
            WireSlotWatchSpecifier::ByRoots(self.slot_watch_roots.iter().copied().collect())
        }
    }
}

impl Default for ProjectView {
    fn default() -> Self {
        Self::new()
    }
}
