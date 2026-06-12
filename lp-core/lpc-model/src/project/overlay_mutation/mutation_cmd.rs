use std::prelude::rust_2015::{String, Vec};
use crate::MutationOp;
/// Ordered overlay mutation command batch.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmdBatch {
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
    pub id: MutationCmdId,
    pub mutation: MutationOp,
}

/// Result for one overlay mutation command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationCmdResult {
    pub id: MutationCmdId,
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
#[serde(rename_all = "snake_case", tag = "status")]
pub enum MutationCmdStatus {
    Accepted { effect: MutationEffect },
    Rejected { rejection: MutationRejection },
}

/// Observable effect of an accepted overlay mutation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum MutationEffect {
    OverlayChanged { changed: bool },
}

/// Stable reason for a rejected overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationRejectionReason {
    InvalidPath,
    EditFailed,
    Unsupported,
}

/// Stable rejection for an overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutationRejection {
    pub reason: MutationRejectionReason,
    pub message: String,
}

impl MutationRejection {
    pub fn new(reason: MutationRejectionReason, message: String) -> Self {
        Self { reason, message }
    }
}