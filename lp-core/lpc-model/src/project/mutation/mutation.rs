//! Ordered overlay mutations and portable mutation results.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{ArtifactLocation, SlotPath};

use crate::project::overlay::{AssetOverlay, SlotEdit};

/// One ordered mutation to the canonical project overlay.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum OverlayMutation {
    PutSlotEdit {
        artifact: ArtifactLocation,
        edit: SlotEdit,
    },
    RemoveSlotEdit {
        artifact: ArtifactLocation,
        path: SlotPath,
    },
    SetArtifactBody {
        artifact: ArtifactLocation,
        edit: AssetOverlay,
    },
    ClearArtifact {
        artifact: ArtifactLocation,
    },
    Clear,
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
pub struct OverlayMutationCommandId(pub u64);

impl OverlayMutationCommandId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn id(self) -> u64 {
        self.0
    }
}

/// Ordered overlay mutation command batch.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverlayMutationBatch {
    pub commands: Vec<OverlayMutationCommand>,
}

impl OverlayMutationBatch {
    pub fn new(commands: Vec<OverlayMutationCommand>) -> Self {
        Self { commands }
    }
}

/// One overlay mutation command with client correlation id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverlayMutationCommand {
    pub id: OverlayMutationCommandId,
    pub mutation: OverlayMutation,
}

/// Ordered result for an [`OverlayMutationBatch`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverlayMutationBatchResult {
    pub results: Vec<OverlayMutationCommandResult>,
}

impl OverlayMutationBatchResult {
    pub fn new(results: Vec<OverlayMutationCommandResult>) -> Self {
        Self { results }
    }
}

/// Result for one overlay mutation command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverlayMutationCommandResult {
    pub id: OverlayMutationCommandId,
    pub status: OverlayMutationCommandStatus,
}

impl OverlayMutationCommandResult {
    pub fn accepted(id: OverlayMutationCommandId, effect: OverlayMutationEffect) -> Self {
        Self {
            id,
            status: OverlayMutationCommandStatus::Accepted { effect },
        }
    }

    pub fn rejected(id: OverlayMutationCommandId, rejection: OverlayMutationRejection) -> Self {
        Self {
            id,
            status: OverlayMutationCommandStatus::Rejected { rejection },
        }
    }
}

/// Accepted or rejected overlay mutation status.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum OverlayMutationCommandStatus {
    Accepted { effect: OverlayMutationEffect },
    Rejected { rejection: OverlayMutationRejection },
}

/// Observable effect of an accepted overlay mutation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum OverlayMutationEffect {
    OverlayChanged { changed: bool },
}

/// Stable rejection for an overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OverlayMutationRejection {
    pub reason: OverlayMutationRejectionReason,
    pub message: String,
}

impl OverlayMutationRejection {
    pub fn new(reason: OverlayMutationRejectionReason, message: String) -> Self {
        Self { reason, message }
    }
}

/// Stable reason for a rejected overlay mutation command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayMutationRejectionReason {
    InvalidPath,
    EditFailed,
    Unsupported,
}
