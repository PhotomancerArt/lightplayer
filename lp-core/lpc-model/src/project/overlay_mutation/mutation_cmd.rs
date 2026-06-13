use alloc::string::String;
use alloc::vec::Vec;

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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationEffect {
    /// Whether the accepted mutation changed canonical overlay state.
    OverlayChanged { changed: bool },
}

/// Stable reason for a rejected overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationRejectionReason {
    /// Mutation referenced an invalid slot or artifact path.
    InvalidPath,
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
