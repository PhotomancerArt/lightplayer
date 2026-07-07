//! Asset-level edit operations dispatched from Studio editor components.

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Client-side limit on one applied asset body, in raw bytes.
///
/// Overlay mutations are single-frame on the wire, bounded by
/// `lpc_wire::budget::PROJECT_READ_FRAME_MAX_BYTES` (16 KB encoded JSON per
/// server message; see `lpc-wire/src/budget.rs`). 10 KB raw leaves headroom
/// for base64 expansion (·4/3) plus the command envelope, so an accepted
/// apply can never produce an over-budget frame. Chunked mutations for
/// larger bodies are future work.
pub const MAX_ASSET_BODY_BYTES: usize = 10 * 1024;

/// An asset body edit targeting one artifact.
///
/// Editor components dispatch these as `UiAction`s against
/// `ProjectController::NODE_ID`, like [`crate::SlotEditOp`]; the op carries
/// the full [`ArtifactLocation`], so no per-asset controller id is needed.
/// Neither variant coalesces in the studio actor queue (only
/// `SlotEditOp::SetValue` coalesces): applies are explicit, whole-body
/// gestures, and both act as coalescing barriers.
#[derive(Clone, Debug, PartialEq)]
pub enum AssetEditOp {
    /// Stage `bytes` as the pending body for `artifact` and send it as a
    /// `MutationOp::SetArtifactBody` (`AssetBodyOverlay::ReplaceBody`).
    /// Bodies above [`MAX_ASSET_BODY_BYTES`] fail client-side and are never
    /// sent.
    ApplyBody {
        artifact: ArtifactLocation,
        bytes: Vec<u8>,
    },
    /// Discard the pending edit for `artifact`, locally and on the server
    /// overlay (`MutationOp::ClearArtifact`).
    Revert { artifact: ArtifactLocation },
}

impl AssetEditOp {
    /// The artifact this edit targets.
    pub fn artifact(&self) -> &ArtifactLocation {
        match self {
            Self::ApplyBody { artifact, .. } | Self::Revert { artifact } => artifact,
        }
    }
}

impl ControllerOp for AssetEditOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::ApplyBody { .. } => ActionMeta::new(
                "Apply",
                "Stage the edited asset body as a pending edit.",
                ActionPriority::Primary,
            ),
            Self::Revert { .. } => ActionMeta::new(
                "Revert",
                "Discard the pending body edit for this asset.",
                ActionPriority::Secondary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the slot-level edit ops: preempts a
        // passive refresh but not other edits, on the editor quiet-gap budget.
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/shader.glsl")
    }

    #[test]
    fn asset_edit_ops_are_editor_foreground_class() {
        let ops = [
            AssetEditOp::ApplyBody {
                artifact: test_artifact(),
                bytes: b"void main() {}".to_vec(),
            },
            AssetEditOp::Revert {
                artifact: test_artifact(),
            },
        ];

        for op in ops {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_EDITOR_ACTION_DEADLINE,
                },
                "{op:?}"
            );
            assert_eq!(op.artifact(), &test_artifact());
        }
    }

    #[test]
    fn size_limit_leaves_headroom_under_the_wire_frame_budget() {
        // Raw body expanded by base64 must fit the single-frame budget with
        // room for the command envelope.
        assert!(
            MAX_ASSET_BODY_BYTES * 4 / 3 < lpc_wire::budget::PROJECT_READ_FRAME_MAX_BYTES,
            "10 KB raw + base64 expansion must stay under the 16 KB frame budget"
        );
    }
}
