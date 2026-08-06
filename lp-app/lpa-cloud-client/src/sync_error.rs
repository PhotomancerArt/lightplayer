//! Everything a sync operation can fail with.

use alloc::string::String;
use core::fmt;

use lpc_cloud_api::CloudError;
use lpc_history::{ContentHash, HistoryError};
use lpfs::FsError;

use crate::cloud_port::TransportError;

/// Why a sync operation stopped.
///
/// The three interesting families are deliberately separate: the service
/// *answering* with a refusal ([`SyncError::Cloud`]) is a different thing
/// from not reaching it at all ([`SyncError::Transport`] — the offline case
/// the caller retries), which is a different thing again from the local
/// library being in a state the operation will not act on
/// ([`SyncError::UncommittedLocalEdits`], [`SyncError::UnbankedVersion`]).
///
/// No `PartialEq`: `HistoryError` and `FsError` do not have one, and a
/// hand-written impl that skipped those arms would be a trap. Match on the
/// variant instead.
#[derive(Debug, Clone)]
pub enum SyncError {
    /// The service could not be reached, or answered unintelligibly. The
    /// offline case lives here; retrying is the caller's loop.
    Transport(TransportError),
    /// The service answered with a refusal.
    Cloud(CloudError),
    /// A local history/snapshot operation failed.
    History(HistoryError),
    /// A local filesystem operation failed.
    Fs(FsError),
    /// Reading or writing a client-side JSON record failed.
    Encode(String),
    /// The service answered a request with a response of the wrong shape.
    /// A bug on one side of the wire, never a user-facing condition.
    UnexpectedResponse(&'static str),
    /// The project has no local history at all — its event log is empty.
    /// [`open_shared`](crate::sync::open_shared) creates one; every other
    /// operation expects one to exist.
    NoLocalHistory,
    /// The project's history has no version yet (an origin event and no
    /// save), so there is nothing to push.
    NoLocalHead,
    /// The service has no commits for this project, so there is nothing to
    /// pull or adopt.
    NoRemoteHead,
    /// A local copy already exists where a fresh one was expected.
    LocalHistoryExists,
    /// The operation needs the project bound to a cloud project, and no
    /// [`CloudBinding`](crate::CloudBinding) is stored.
    NotBound,
    /// The working copy holds content the history has not recorded, and the
    /// requested operation would have overwritten it.
    ///
    /// **The content is not lost.** It was banked in the local snapshot
    /// store under `banked` before this error was raised — save it (or
    /// restore it by hash) and retry. This is the D18 asymmetry made
    /// structural: nothing is ever adopted over unbanked local state.
    UncommittedLocalEdits {
        /// Hash of the working copy's content, now in the snapshot store.
        banked: ContentHash,
        /// The version the history says is current.
        head: ContentHash,
    },
    /// A version this operation must not lose is not in the local snapshot
    /// store, so the operation refused to proceed. Pull first.
    UnbankedVersion(ContentHash),
    /// The service's history does not extend the local one, so there is no
    /// fast-forward to apply. Resolve the divergence with
    /// [`resolve_clobber`](crate::sync::resolve_clobber) instead.
    NotAFastForward,
    /// Content fetched for a version did not hash to that version.
    ContentMismatch {
        /// The version that was asked for.
        expected: ContentHash,
        /// What the fetched content actually hashed to.
        actual: ContentHash,
    },
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Transport(e) => write!(f, "transport: {e}"),
            SyncError::Cloud(e) => write!(f, "service refused the request: {e:?}"),
            SyncError::History(e) => write!(f, "history: {e}"),
            SyncError::Fs(e) => write!(f, "filesystem: {e}"),
            SyncError::Encode(msg) => write!(f, "encode: {msg}"),
            SyncError::UnexpectedResponse(what) => {
                write!(f, "service answered {what} with the wrong response")
            }
            SyncError::NoLocalHistory => f.write_str("the project has no local history"),
            SyncError::NoLocalHead => f.write_str("the project has no saved version yet"),
            SyncError::NoRemoteHead => f.write_str("the service has no commits for this project"),
            SyncError::LocalHistoryExists => f.write_str("a local copy already exists"),
            SyncError::NotBound => f.write_str("the project is not bound to a cloud project"),
            SyncError::UncommittedLocalEdits { banked, head } => write!(
                f,
                "uncommitted edits (banked as {}) over head {}",
                banked.short(),
                head.short()
            ),
            SyncError::UnbankedVersion(hash) => {
                write!(f, "version {} is not in the local store", hash.short())
            }
            SyncError::NotAFastForward => {
                f.write_str("the service's history is not a fast-forward of the local one")
            }
            SyncError::ContentMismatch { expected, actual } => write!(
                f,
                "content for {} hashed to {}",
                expected.short(),
                actual.short()
            ),
        }
    }
}

impl From<TransportError> for SyncError {
    fn from(e: TransportError) -> Self {
        SyncError::Transport(e)
    }
}

impl From<CloudError> for SyncError {
    fn from(e: CloudError) -> Self {
        SyncError::Cloud(e)
    }
}

impl From<HistoryError> for SyncError {
    fn from(e: HistoryError) -> Self {
        SyncError::History(e)
    }
}

impl From<FsError> for SyncError {
    fn from(e: FsError) -> Self {
        SyncError::Fs(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn transport_and_cloud_failures_stay_distinguishable() {
        let offline: SyncError = TransportError::Offline.into();
        let refused: SyncError = CloudError::NotFound.into();
        assert!(matches!(
            offline,
            SyncError::Transport(TransportError::Offline)
        ));
        assert!(matches!(refused, SyncError::Cloud(CloudError::NotFound)));
    }

    #[test]
    fn uncommitted_edits_name_both_hashes() {
        let error = SyncError::UncommittedLocalEdits {
            banked: ContentHash::of(b"working"),
            head: ContentHash::of(b"head"),
        };
        let rendered = format!("{error}");
        assert!(rendered.contains(&ContentHash::of(b"working").short()));
        assert!(rendered.contains(&ContentHash::of(b"head").short()));
    }
}
