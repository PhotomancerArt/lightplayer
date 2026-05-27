//! Unified registry ingress operations.

use lpfs::{FsEvent, LpPathBuf};

use crate::edit_model::{AssetEdit, SlotEdit};

/// One registry sync operation (filesystem or pending-edit CRUD).
#[derive(Clone, Debug, PartialEq)]
pub enum SyncOp {
    /// Committed filesystem notification.
    Fs(FsEvent),
    /// Upsert one slot edit into the overlay.
    UpsertSlot { path: LpPathBuf, op: SlotEdit },
    /// Set pending asset state for one artifact path.
    SetPendingAsset { path: LpPathBuf, asset: AssetEdit },
    /// Drop pending edits for one artifact path.
    Remove { path: LpPathBuf },
    /// Drop all pending edits.
    ClearPending,
    /// Promote pending overlay to committed store and clear overlay.
    Commit,
}
