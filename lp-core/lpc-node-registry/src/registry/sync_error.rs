//! Errors from unified registry sync.

use crate::edit::{CommitError, EditError};

/// Failure applying a [`super::SyncOp`] batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    Edit(EditError),
    Commit(CommitError),
}

impl From<EditError> for SyncError {
    fn from(err: EditError) -> Self {
        Self::Edit(err)
    }
}

impl From<CommitError> for SyncError {
    fn from(err: CommitError) -> Self {
        Self::Commit(err)
    }
}
