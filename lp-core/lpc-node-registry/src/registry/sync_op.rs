//! Unified registry ingress operations.

use lpfs::FsEvent;

use crate::edit::{ArtifactEdit, EditTarget};

/// One registry sync operation (filesystem or pending-edit CRUD).
#[derive(Clone, Debug, PartialEq)]
pub enum SyncOp {
    /// Committed filesystem notification.
    Fs(FsEvent),
    /// Apply or replace pending edits for one artifact (upsert into [`super::NodeDefRegistry`] overlay).
    Apply(ArtifactEdit),
    /// Drop pending edits for one artifact target.
    Remove(EditTarget),
    /// Drop all pending edits.
    ClearPending,
    /// Promote pending overlay to committed store and clear overlay.
    Commit,
}
