//! Last read / fs-notify outcome for a held artifact (no bytes stored).

use alloc::string::String;

use lpfs::FsError;

/// Outcome of the last materialization attempt or fs delete notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactReadState {
    /// No read since the last [`super::ArtifactEntry::revision`] bump.
    Unread,
    /// Last transient read succeeded (bytes not retained on the entry).
    ReadOk,
    /// Read failed or fs notified delete while held.
    Failed(ArtifactReadFailure),
}

/// Structured read / availability failure (distinct from acquire-time resolution).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactReadFailure {
    /// `FsChange::Delete` while entry held — watcher-sourced.
    Deleted,
    /// File not on disk at read time.
    NotFound,
    /// Filesystem or host I/O error.
    Io { message: String },
    /// Invalid path at read time.
    InvalidPath { message: String },
}

impl ArtifactReadFailure {
    pub fn from_fs_error(err: FsError) -> Self {
        match err {
            FsError::NotFound(_msg) => Self::NotFound,
            FsError::Filesystem(msg) => Self::Io { message: msg },
            FsError::InvalidPath(msg) => Self::InvalidPath { message: msg },
        }
    }
}
