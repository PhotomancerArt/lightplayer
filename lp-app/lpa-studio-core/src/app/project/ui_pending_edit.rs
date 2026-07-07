//! Save-panel change-list DTO: one labeled entry per pending overlay edit.

use crate::UiAction;

/// One pending edit in the project's save panel (plan D5): node label + slot
/// path + op/value display + phase, with a per-entry revert action.
///
/// Entries are produced by `ProjectController::pending_edits` from the same
/// edit-state join `DirtySummary` counting uses, so the list length per
/// [`UiPendingEditPhase`] equals the summary's bucket counts by construction
/// (one entry per buffer/overlay address, never per slot row — a removed map
/// entry with no surviving row is still listed). Entries carry the saved
/// (base) value they replace where the mirror knows it ([`Self::old_value`]
/// — editing-model ADR follow-up (b), display half).
#[derive(Clone, Debug, PartialEq)]
pub struct UiPendingEdit {
    /// Label of the node the edit addresses. Overlay entries whose artifact
    /// no longer reverse-maps to a synced node (stale) carry the artifact
    /// path here instead of being dropped from the list.
    pub node_label: String,
    /// Stable address string of the node the edit targets — the same string
    /// `UiNodeView::node_id` / `UiNodeHeader::path` carry, so per-node
    /// surfaces (the node detail popup) can filter the editor-level list
    /// exactly instead of matching the (non-unique) label. Stale entries
    /// carry their artifact file path here (no node address exists), which
    /// never collides with a synced node's address.
    pub node_path: String,
    /// Human-readable slot path within the node's def root (the root name
    /// itself for root-path edits). Asset body rows
    /// ([`UiPendingEditKind::AssetBody`]) carry the artifact file path here
    /// instead — they are file rows, not slot rows.
    pub slot_path_display: String,
    /// What the edit does, with the display string for assigned values.
    pub kind: UiPendingEditKind,
    /// Display string of the saved (base) value this edit replaces, from the
    /// overlay mirror's base-value map (`ProjectSync::base_value_at`):
    /// formatted leaf values, capped compact JSON for composites. `None`
    /// when the base holds nothing at the path (a structural add), when the
    /// edit is still buffered (no ack has annotated it yet), or when the
    /// server could not derive a display — rows degrade to their kind-only
    /// form.
    pub old_value: Option<String>,
    /// Which save-panel section the entry belongs to — matches the entry's
    /// [`crate::DirtySummary`] bucket exactly.
    pub phase: UiPendingEditPhase,
    /// Revert action for this entry (dispatches `SlotEditOp::Revert` at the
    /// entry's address; for failed entries this is the clear/retry
    /// affordance). `None` only for stale entries, which have no node
    /// address to dispatch through.
    pub revert: Option<UiAction>,
}

/// The operation a pending edit performs, in display form.
#[derive(Clone, Debug, PartialEq)]
pub enum UiPendingEditKind {
    /// A value assignment; `value_display` is the assigned value's display
    /// string (`format_lp_value`).
    Assign {
        /// Display string of the currently pending value.
        value_display: String,
    },
    /// A structural add (`EnsurePresent`): map entry, option body, or enum
    /// variant created with server defaults.
    Added,
    /// A structural removal (`Remove`/`RemoveValue`): map entry or option
    /// body removed from the effective def.
    Removed,
    /// A map entry key move (`MoveEntry`), visible while buffered (mid-op or
    /// Failed); an accepted move materializes into per-path add/remove acks.
    Moved {
        /// Display string of the entry's current key.
        from: String,
        /// Display string of the key the entry moves to.
        to: String,
    },
    /// A whole asset body replacement (`SetArtifactBody`), listed as a file
    /// row: the entry's path display is the artifact path, and its revert
    /// dispatches `AssetEditOp::Revert` (`ClearArtifact`).
    AssetBody {
        /// Human-readable size of the replacement body (e.g. "3.2 KB"), or
        /// "deleted" for a pending body deletion.
        detail: String,
    },
}

/// Save-panel section for a pending edit — the entry-level mirror of the
/// [`crate::DirtySummary`] buckets.
#[derive(Clone, Debug, PartialEq)]
pub enum UiPendingEditPhase {
    /// Written to project files on save.
    Persisted,
    /// Live-only (transient persistence); survives save as a pending edit.
    Live,
    /// The buffered edit failed (rejected or transport error).
    Failed {
        /// Human-readable rejection or transport reason.
        reason: String,
    },
}

impl UiPendingEdit {
    /// True when this entry belongs to the failed section.
    pub fn is_failed(&self) -> bool {
        matches!(self.phase, UiPendingEditPhase::Failed { .. })
    }
}
