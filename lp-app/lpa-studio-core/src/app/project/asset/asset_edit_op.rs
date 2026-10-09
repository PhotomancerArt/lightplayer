//! Asset-level edit operations dispatched from Studio editor components.

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Client-side limit on one applied asset body, **as encoded on the wire**
/// (`lpc_model::body_bytes::encoded_len`: UTF-8 text with its escapes —
/// a shader is its own size plus a byte a line — or a binary body's base64).
///
/// Overlay mutations are single-frame on the wire. The arithmetic lives
/// with the budget (`lpc_wire::budget::MAX_ASSET_BODY_ENCODED_BYTES`): the
/// 16,384 B message budget less a 1,024 B envelope reserve, 15,360 B, under
/// the board's 16,656 B request buffer and inside one overlay-read reply.
/// Before wire 41 the limit was 10 KB raw "for base64", while the body
/// actually went as an array of numbers, so past ~4.7 KB of source a
/// request outgrew the board's buffer and was dropped unanswered. Chunked
/// mutations for larger bodies are future work.
pub const MAX_ASSET_BODY_BYTES: usize = lpc_wire::budget::MAX_ASSET_BODY_ENCODED_BYTES;

/// Whether `bytes`, encoded as an edit carries them, exceed
/// [`MAX_ASSET_BODY_BYTES`]: the one test every sender of a body applies.
pub fn asset_body_too_large(bytes: &[u8]) -> bool {
    lpc_model::body_bytes::encoded_len(bytes) > MAX_ASSET_BODY_BYTES
}

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
        // The limit is on the encoded body, so the body plus the command
        // envelope's reserve is the frame budget (lpc-wire's
        // `the_largest_body_studio_sends_fits_one_request_and_one_reply`
        // serializes it).
        assert_eq!(
            MAX_ASSET_BODY_BYTES + lpc_wire::budget::ASSET_BODY_REQUEST_ENVELOPE_RESERVE_BYTES,
            lpc_wire::budget::PROJECT_READ_FRAME_MAX_BYTES
        );
        // A body at the limit is not too large; one character more is.
        let body = "x".repeat(MAX_ASSET_BODY_BYTES - 2);
        assert!(!asset_body_too_large(body.as_bytes()));
        assert!(asset_body_too_large(format!("{body}y").as_bytes()));
    }
}
