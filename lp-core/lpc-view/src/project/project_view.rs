use lpc_model::NodeKind;
use lpc_model::{LpPathBuf, Revision};
use lpc_wire::NodeRuntimeStatus;

use super::resource_cache::ClientResourceCache;
use crate::slot::SlotMirrorView;
use crate::tree::NodeTreeView;

/// Status change information surfaced by future canonical project sync.
#[derive(Debug, Clone)]
pub struct StatusChangeView {
    /// Node path.
    pub path: LpPathBuf,
    /// Previous status.
    pub old_status: NodeRuntimeStatus,
    /// New status.
    pub new_status: NodeRuntimeStatus,
}

/// Node-centric client-side project mirror.
pub struct ProjectView {
    /// Last project revision applied to this mirror.
    pub revision: Revision,
    /// Runtime node tree mirror.
    pub tree: NodeTreeView,
    /// Generic authored/runtime slot data mirror.
    pub slots: SlotMirrorView,
    /// Cached resource summaries and payloads.
    pub resource_cache: ClientResourceCache,
}

/// Minimal node entry retained until canonical project sync is rebuilt.
pub struct NodeEntryView {
    pub path: LpPathBuf,
    pub kind: NodeKind,
    pub status: NodeRuntimeStatus,
    pub status_ver: Revision,
}

impl ProjectView {
    /// Create an empty project view shell.
    pub fn new() -> Self {
        Self {
            revision: Revision::default(),
            tree: NodeTreeView::new(),
            slots: SlotMirrorView::new(),
            resource_cache: ClientResourceCache::new(),
        }
    }
}

impl Default for ProjectView {
    fn default() -> Self {
        Self::new()
    }
}
