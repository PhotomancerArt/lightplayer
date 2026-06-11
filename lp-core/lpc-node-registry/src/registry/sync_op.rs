//! Unified registry ingress operations.

use lpc_model::{ArtifactBodyEdit, SlotEdit};
use lpfs::{FsEvent, LpPathBuf};

/// One registry sync operation (filesystem or pending-edit CRUD).
#[derive(Clone, Debug, PartialEq)]
pub enum SyncOp {
    /// Committed filesystem notification.
    Fs(FsEvent),
    /// Upsert one slot edit into the overlay.
    UpsertSlot { path: LpPathBuf, op: SlotEdit },
    /// Set pending artifact body state for one artifact path.
    SetPendingArtifactBody {
        path: LpPathBuf,
        edit: ArtifactBodyEdit,
    },
    /// Drop pending edits for one artifact path.
    Remove { path: LpPathBuf },
    /// Drop all pending edits.
    ClearPending,
    /// Promote pending overlay to committed store and clear overlay.
    Commit,
}
