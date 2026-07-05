//! Client-held pending slot edit awaiting a server acknowledgement.

use lpc_model::{LpValue, MutationCmdId};

/// One buffered slot edit, keyed by `ProjectSlotAddress` in the
/// `ProjectController` edit buffer and held until the server acknowledges it.
///
/// # Edit-buffer state machine
///
/// ```text
/// (field input) ──► Pending { value }            # op queued/coalescing
/// op sends       ──► InFlight { value, cmd_id }
/// ack accepted   ──► (entry removed; mirror updated via
///                     ProjectSync::apply_acked_edits — the slot now reads
///                     dirty from the overlay mirror)
/// ack rejected   ──► Failed { value, reason }    # feeds UiSlotFieldState
///                                                # `invalid`; cleared on the
///                                                # next edit or an explicit
///                                                # revert
/// op error/timeout ─► Failed { value, transport reason }
/// ```
///
/// While an entry exists, the config-slot DTO shows the buffered `value`
/// (shadowing the mirror/synced value — rubber-band protection), and the
/// entry's phase maps to the DTO dirty affordance: `Pending`/`InFlight` →
/// `Saving`, `Failed` → `Error`. Once an accepted ack removes the entry, the
/// overlay mirror is the single source of the slot's `Dirty` state (and of
/// the assigned-value shadow until the next project read catches up).
///
/// Ops run one at a time on the controller, so `Pending` and `InFlight` are
/// only observable in mid-op progressive snapshots; between ops the buffer
/// holds only `Failed` entries.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingEdit {
    /// The value the user asked for; shadows the synced value in DTOs.
    pub value: LpValue,
    /// Where the edit sits in the ack lifecycle.
    pub phase: PendingEditPhase,
}

impl PendingEdit {
    /// A freshly staged edit that has not been sent yet.
    pub fn pending(value: LpValue) -> Self {
        Self {
            value,
            phase: PendingEditPhase::Pending,
        }
    }

    /// True when the edit failed (rejected or transport error).
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

/// Ack-lifecycle phase for a [`PendingEdit`] (see the type-level diagram).
#[derive(Clone, Debug, PartialEq)]
pub enum PendingEditPhase {
    /// Staged locally; the mutation has not been sent yet.
    Pending,
    /// The mutation batch containing this edit is awaiting its ack.
    InFlight {
        /// Correlation id of the mutation command carrying this edit.
        cmd_id: MutationCmdId,
    },
    /// The server rejected the edit or the send failed; the value is
    /// preserved for display until the next edit or an explicit revert.
    Failed {
        /// Human-readable rejection or transport reason (feeds `invalid`).
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_edit_starts_pending_and_reports_failure() {
        let mut edit = PendingEdit::pending(LpValue::F32(2.0));

        assert_eq!(edit.phase, PendingEditPhase::Pending);
        assert!(!edit.is_failed());
        assert_eq!(edit.failure_reason(), None);

        edit.phase = PendingEditPhase::Failed {
            reason: "not writable".to_string(),
        };

        assert!(edit.is_failed());
        assert_eq!(edit.failure_reason(), Some("not writable"));
        assert_eq!(edit.value, LpValue::F32(2.0), "value preserved for display");
    }
}
