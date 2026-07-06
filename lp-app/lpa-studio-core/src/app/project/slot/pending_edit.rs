//! Client-held pending slot edit awaiting a server acknowledgement.

use lpc_model::{LpValue, MutationCmdId, SlotMapKey};

/// One buffered slot edit, keyed by `ProjectSlotAddress` in the
/// `ProjectController` edit buffer and held until the server acknowledges it.
///
/// # Edit-buffer state machine
///
/// ```text
/// (field input /
///  composite gesture) ──► Pending { op }          # op queued/coalescing
/// op sends       ──► InFlight { op, cmd_id }
/// ack accepted,
///   overlay stored ──► (entry removed; mirror updated via
///                     ProjectSync::apply_acked_edits — the slot now reads
///                     dirty from the overlay mirror)
/// ack accepted,
///   normalized to a removal
///   that changed the overlay
///                ──► AwaitingRefresh              # the mirror entry is gone
///                                                 # but the synced view still
///                                                 # holds the pre-normalization
///                                                 # effective value; the entry
///                                                 # bridges that gap (released
///                                                 # when the next project read
///                                                 # is applied)
/// ack rejected   ──► Failed { op, reason }        # feeds UiSlotFieldState
///                                                 # `invalid`; cleared on the
///                                                 # next edit or an explicit
///                                                 # revert
/// op error/timeout ─► Failed { op, transport reason }
/// ```
///
/// While an entry exists, a `SetValue` entry's DTO shows the buffered value
/// (shadowing the mirror/synced value — rubber-band protection); structural
/// entries ([`PendingEditOp::EnsurePresent`]/[`PendingEditOp::RemoveValue`])
/// carry no value and shadow nothing. Either way the entry's phase maps to
/// the DTO dirty affordance: `Pending`/`InFlight` → `Saving`, `Failed` →
/// `Error` — on the entry's own row when one survives, and on ancestor
/// composite rows through the prefix-aware join (`SlotEditJoin::state_under`),
/// which is how a gesture on a not-yet-existing path (e.g. a rejected map-add)
/// surfaces on the dispatching composite. Once an accepted ack removes the
/// entry, the overlay mirror is the single source of the slot's `Dirty` state
/// (and of the assigned-value shadow until the next project read catches up).
///
/// An accepted ack that reports the edit **normalized to a removal** which
/// changed the overlay (`MutationEffect::NormalizedToRemoval { changed: true
/// }` — e.g. a value assigned back to its base, or an add-then-remove map
/// entry cancelling) leaves the mirror with no entry at the path, while the
/// synced view still holds the pre-normalization effective value until the
/// next gated project read delivers the reverted def. Releasing the entry at
/// the ack would fall back to that stale value for one pull cycle (visible
/// value jitter), so the entry moves to `AwaitingRefresh` instead: it keeps
/// its shadow and its `Saving` affordance, and `ProjectController::
/// apply_project_view` releases it when the next project read is applied. A
/// normalization with `changed: false` altered nothing, so the entry releases
/// immediately as usual.
///
/// Ops run one at a time on the controller, so `Pending` and `InFlight` are
/// only observable in mid-op progressive snapshots; between ops the buffer
/// holds only `Failed` and `AwaitingRefresh` entries.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingEdit {
    /// What the user asked for: a value assignment (which shadows the synced
    /// value in DTOs) or a structural gesture (value-less).
    pub op: PendingEditOp,
    /// Where the edit sits in the ack lifecycle.
    pub phase: PendingEditPhase,
}

impl PendingEdit {
    /// A freshly staged value assignment that has not been sent yet.
    pub fn pending(value: LpValue) -> Self {
        Self::pending_op(PendingEditOp::SetValue { value })
    }

    /// A freshly staged edit op that has not been sent yet.
    pub fn pending_op(op: PendingEditOp) -> Self {
        Self {
            op,
            phase: PendingEditPhase::Pending,
        }
    }

    /// The buffered value when this edit assigns one; `None` for structural
    /// gestures, which have nothing to shadow.
    pub fn value(&self) -> Option<&LpValue> {
        match &self.op {
            PendingEditOp::SetValue { value } => Some(value),
            PendingEditOp::EnsurePresent
            | PendingEditOp::RemoveValue
            | PendingEditOp::MoveEntry { .. } => None,
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

/// The operation a [`PendingEdit`] buffers, mirroring the client
/// `SlotEditOp` vocabulary (minus `Revert`, which never buffers: it removes
/// entries instead).
#[derive(Clone, Debug, PartialEq)]
pub enum PendingEditOp {
    /// Assign `value` to the slot; shadows the synced value while buffered.
    SetValue { value: LpValue },
    /// Structural gesture: create/activate the slot path with server-built
    /// defaults (map entry add, option on, enum variant switch).
    EnsurePresent,
    /// Structural gesture: remove the slot path from the effective def (map
    /// entry remove, option off).
    RemoveValue,
    /// Structural gesture: move the entry at `from_key` of the map at this
    /// entry's address to `to_key` (server-materialized; the ack lists the
    /// stored per-path edits).
    MoveEntry {
        from_key: SlotMapKey,
        to_key: SlotMapKey,
    },
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
    /// The server accepted the edit but normalized it to an overlay-entry
    /// removal that changed the overlay (`NormalizedToRemoval { changed:
    /// true }`): the mirror holds nothing at the path, and the synced view
    /// is stale until the next gated read delivers the reverted def. The
    /// entry is retained — keeping its value shadow and `Saving` affordance
    /// — purely to bridge that window; it is released when the next project
    /// read is applied (`ProjectController::apply_project_view`).
    AwaitingRefresh,
    /// The server rejected the edit or the send failed; the op is
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
        assert_eq!(
            edit.value(),
            Some(&LpValue::F32(2.0)),
            "value preserved for display"
        );
    }

    #[test]
    fn structural_edits_buffer_without_a_value_shadow() {
        for op in [
            PendingEditOp::EnsurePresent,
            PendingEditOp::RemoveValue,
            PendingEditOp::MoveEntry {
                from_key: SlotMapKey::U32(0),
                to_key: SlotMapKey::U32(1),
            },
        ] {
            let edit = PendingEdit::pending_op(op.clone());

            assert_eq!(edit.phase, PendingEditPhase::Pending, "{op:?}");
            assert_eq!(edit.value(), None, "structural {op:?} shadows nothing");
        }
    }
}
