//! Client-held pending asset body edit awaiting a server acknowledgement.

use crate::PendingEditPhase;

/// One buffered asset body edit, keyed by `ArtifactLocation` in the
/// `ProjectController` asset edit buffer and held until the server
/// acknowledges it.
///
/// The lifecycle is the slot buffer's ([`crate::PendingEdit`]), sharing
/// [`PendingEditPhase`]:
///
/// ```text
/// apply_asset_body ──► Pending { bytes }         # staged, not sent yet
/// op sends         ──► InFlight { cmd_id }
/// ack accepted     ──► (entry removed; mirror updated via
///                      ProjectSync::apply_acked_edits — the artifact now
///                      reads dirty from the overlay mirror)
/// ack rejected     ──► Failed { reason }
/// op error/timeout ──► Failed { transport reason }
/// size guard trips ──► Failed { too-large reason }  # never sent
/// ```
///
/// `AwaitingRefresh` never occurs for asset edits: `SetArtifactBody` is a
/// whole-artifact op the server accepts as sent (effect `OverlayChanged`),
/// so there is no normalization window to bridge. A `Failed` entry keeps its
/// bytes so the rejected body stays available for display (and for the
/// oversize case, so the user's text is not lost); it clears on the next
/// apply or an explicit revert.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingAssetEdit {
    /// The full replacement body the user applied.
    pub bytes: Vec<u8>,
    /// Where the edit sits in the ack lifecycle.
    pub phase: PendingEditPhase,
}

impl PendingAssetEdit {
    /// A freshly staged body replacement that has not been sent yet.
    pub fn pending(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            phase: PendingEditPhase::Pending,
        }
    }

    /// A body replacement parked as failed without ever being sent (the
    /// client-side size guard).
    pub fn failed(bytes: Vec<u8>, reason: impl Into<String>) -> Self {
        Self {
            bytes,
            phase: PendingEditPhase::Failed {
                reason: reason.into(),
            },
        }
    }

    /// True when the edit failed (rejected, transport error, or size guard).
    pub fn is_failed(&self) -> bool {
        matches!(self.phase, PendingEditPhase::Failed { .. })
    }

    /// The failure reason, when the edit failed.
    pub fn failure_reason(&self) -> Option<&str> {
        match &self.phase {
            PendingEditPhase::Failed { reason } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_asset_edit_starts_pending_and_reports_failure() {
        let mut edit = PendingAssetEdit::pending(b"body".to_vec());

        assert_eq!(edit.phase, PendingEditPhase::Pending);
        assert!(!edit.is_failed());
        assert_eq!(edit.failure_reason(), None);

        edit.phase = PendingEditPhase::Failed {
            reason: "rejected".to_string(),
        };

        assert!(edit.is_failed());
        assert_eq!(edit.failure_reason(), Some("rejected"));
        assert_eq!(edit.bytes, b"body", "bytes preserved for display");
    }

    #[test]
    fn failed_constructor_parks_with_reason_and_bytes() {
        let edit = PendingAssetEdit::failed(b"huge".to_vec(), "too large");

        assert!(edit.is_failed());
        assert_eq!(edit.failure_reason(), Some("too large"));
        assert_eq!(edit.bytes, b"huge");
    }
}
