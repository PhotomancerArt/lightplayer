use alloc::string::String;
use alloc::vec::Vec;

use crate::{SlotEdit, SlotPath};

use super::MutationOp;

/// Ordered overlay mutation command batch.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmdBatch {
    /// Commands to apply in order.
    pub commands: Vec<MutationCmd>,
}

impl MutationCmdBatch {
    pub fn new(commands: Vec<MutationCmd>) -> Self {
        Self { commands }
    }
}

/// Ordered result for an [`MutationCmdBatch`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmdBatchResult {
    /// Per-command results in command order.
    pub results: Vec<MutationCmdResult>,
}

impl MutationCmdBatchResult {
    pub fn new(results: Vec<MutationCmdResult>) -> Self {
        Self { results }
    }
}

/// Client-visible id for one overlay mutation command.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct MutationCmdId(pub u64);

impl MutationCmdId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn id(self) -> u64 {
        self.0
    }
}

/// One overlay mutation command with client correlation id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmd {
    /// Client-supplied command id for result correlation.
    pub id: MutationCmdId,
    /// Mutation operation to apply.
    pub mutation: MutationOp,
}

/// Result for one overlay mutation command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmdResult {
    /// Command id copied from the input command.
    pub id: MutationCmdId,
    /// Accepted or rejected status for the command.
    pub status: MutationCmdStatus,
}

impl MutationCmdResult {
    pub fn accepted(id: MutationCmdId, effect: MutationEffect) -> Self {
        Self {
            id,
            status: MutationCmdStatus::Accepted { effect },
        }
    }

    pub fn rejected(id: MutationCmdId, rejection: MutationRejection) -> Self {
        Self {
            id,
            status: MutationCmdStatus::Rejected { rejection },
        }
    }
}

/// Accepted or rejected overlay mutation status.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationCmdStatus {
    /// Mutation was accepted and applied to the overlay.
    Accepted { effect: MutationEffect },
    /// Mutation was rejected without changing the overlay.
    Rejected { rejection: MutationRejection },
}

/// Observable effect of an accepted overlay mutation.
///
/// The effect is what the server actually stored, which may differ from the
/// sent command: minimal-diff normalization rewrites a `PutSlotEdit` that is
/// a no-op against the base (unoverlaid) artifact — assigning the base value,
/// `EnsurePresent` of a base-present target, or `Remove` of a base-absent
/// target — into a removal of the overlay entry at that path. Clients that
/// mirror the overlay from their own acks must apply the effect, not the sent
/// command, or their mirror diverges from the server without a revision bump
/// to correct it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationEffect {
    /// The mutation was applied as sent; `changed` reports whether it changed
    /// canonical overlay state.
    OverlayChanged {
        changed: bool,
        /// Display string of the base (saved) value at the edit's path, when
        /// the server derived one (leaf values formatted, composite subtrees
        /// as capped compact JSON, `None` when the target is absent in the
        /// base). Rides the ack so clients can show "old value" for their own
        /// pending edits without a follow-up overlay read. Omitted from the
        /// wire when `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_display: Option<String>,
    },
    /// A `PutSlotEdit` that was a no-op against the base artifact (assigning
    /// the base value, ensuring a base-present target, or removing a
    /// base-absent one) was normalized to removing the overlay entry at its
    /// path; `changed` reports whether an entry existed to remove (`false`:
    /// the command was a complete no-op).
    NormalizedToRemoval {
        changed: bool,
        /// Display string of the base value the edit normalized back to (see
        /// [`MutationEffect::OverlayChanged::base_display`]). The overlay
        /// entry is gone, but the annotation lets a client label the
        /// stale-view window ("reverted to X"). Omitted from the wire when
        /// `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_display: Option<String>,
    },
    /// The mutation materialized into several per-path overlay changes:
    /// either a multi-edit mutation (`MoveSlotEntry`) synthesized into
    /// per-path edits, or a structural `Remove` that normalized away and
    /// also cleared the overlay entries stranded strictly under its path.
    /// `edits` lists what was actually stored, in application order, against
    /// the command's artifact — each edit was individually normalized
    /// against the base, so an entry is either a stored [`SlotEdit`] or a
    /// removal of the overlay entry at a path. Ack-mirroring clients replay
    /// `edits` verbatim; `changed` reports whether any of them changed
    /// canonical overlay state.
    Materialized {
        edits: Vec<StoredSlotEdit>,
        changed: bool,
    },
}

impl MutationEffect {
    /// `OverlayChanged` without a base-value annotation.
    pub fn overlay_changed(changed: bool) -> Self {
        Self::OverlayChanged {
            changed,
            base_display: None,
        }
    }

    /// `NormalizedToRemoval` without a base-value annotation.
    pub fn normalized_to_removal(changed: bool) -> Self {
        Self::NormalizedToRemoval {
            changed,
            base_display: None,
        }
    }

    /// Attach a base-value display annotation to an `OverlayChanged` or
    /// `NormalizedToRemoval` effect; `Materialized` is unchanged (its
    /// annotations live per stored edit).
    pub fn with_base_display(self, annotation: Option<String>) -> Self {
        match self {
            Self::OverlayChanged { changed, .. } => Self::OverlayChanged {
                changed,
                base_display: annotation,
            },
            Self::NormalizedToRemoval { changed, .. } => Self::NormalizedToRemoval {
                changed,
                base_display: annotation,
            },
            materialized @ Self::Materialized { .. } => materialized,
        }
    }
}

/// One stored overlay change from a materialized mutation.
///
/// Produced by the move materialization and by a normalized structural
/// `Remove` clearing its stranded descendants. The two forms mirror what the
/// registry does per edit: store it
/// ([`crate::ProjectOverlay::put_slot_edit`]) or — when normalization elided
/// it, or a stale descendant of a normalized removal had to be cleared —
/// remove the overlay entry at a path
/// ([`crate::ProjectOverlay::remove_slot_edit`]).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredSlotEdit {
    /// `edit` was stored in the artifact's slot overlay.
    Put {
        edit: SlotEdit,
        /// Display string of the base (saved) value at the edit's path (see
        /// [`MutationEffect::OverlayChanged::base_display`]). `Removed`
        /// entries carry no annotation: they drop their overlay entry, so a
        /// base value would have nothing pending to describe. Omitted from
        /// the wire when `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_display: Option<String>,
    },
    /// The overlay entry at `path` (if any) was removed.
    Removed { path: SlotPath },
}

impl StoredSlotEdit {
    /// A stored `Put` without a base-value annotation.
    pub fn put(edit: SlotEdit) -> Self {
        Self::Put {
            edit,
            base_display: None,
        }
    }

    /// A stored `Put` carrying the base-value display annotation.
    pub fn put_with_base_display(edit: SlotEdit, base_display: Option<String>) -> Self {
        Self::Put { edit, base_display }
    }

    /// A removal of the overlay entry at `path`.
    pub fn removed(path: SlotPath) -> Self {
        Self::Removed { path }
    }
}

/// Stable reason for a rejected overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationRejectionReason {
    /// Mutation referenced an artifact with no resolvable node definition.
    UnknownArtifact,
    /// Mutation referenced a slot path that does not resolve in the
    /// artifact's shape.
    UnknownSlotPath,
    /// Mutation targeted a slot whose policy is not writable.
    NotWritable,
    /// Mutation assigned a value that does not match the slot's value type.
    TypeMismatch,
    /// Mutation assigned a value to a structural slot (record, map, option,
    /// enum, unit) instead of a value leaf.
    NotAValueLeaf,
    /// Mutation would move or create an entry at a target that already
    /// exists in the effective definition (occupied map key).
    TargetOccupied,
    /// Mutation was well-formed but edit application failed.
    EditFailed,
    /// Mutation is not supported by the current registry implementation.
    Unsupported,
}

/// Stable rejection for an overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutationRejection {
    /// Stable rejection category.
    pub reason: MutationRejectionReason,
    /// Human-readable rejection detail.
    pub message: String,
}

impl MutationRejection {
    pub fn new(reason: MutationRejectionReason, message: String) -> Self {
        Self { reason, message }
    }
}
