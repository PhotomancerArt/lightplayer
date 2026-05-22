//! Edit vocabulary, slot overlay storage, and apply.

pub(crate) mod apply;
mod artifact_edit;
mod commit_error;
mod def_draft;
mod edit_batch;
mod edit_error;
mod edit_op;
mod edit_target;
mod slot_overlay;

pub use apply::{apply_artifact_edit, apply_edit_batch, require_absolute_path};
pub use artifact_edit::ArtifactEdit;
pub use commit_error::CommitError;
pub use def_draft::DefDraft;
pub use edit_batch::{EditBatch, EditBatchId};
pub use edit_error::EditError;
pub use edit_op::EditOp;
pub use edit_target::EditTarget;
pub use slot_overlay::{SlotOverlay, SlotOverlayEntry};

#[deprecated(note = "renamed to ArtifactEdit")]
pub type ArtifactChange = ArtifactEdit;
#[deprecated(note = "renamed to EditOp")]
pub type ArtifactOp = EditOp;
#[deprecated(note = "renamed to EditTarget")]
pub type ArtifactTarget = EditTarget;
#[deprecated(note = "renamed to EditBatch")]
pub type ChangeSet = EditBatch;
#[deprecated(note = "renamed to EditBatchId")]
pub type ChangeSetId = EditBatchId;
#[deprecated(note = "renamed to EditError")]
pub type ChangeError = EditError;
#[deprecated(note = "renamed to SlotOverlay")]
pub type ChangeOverlay = SlotOverlay;
#[deprecated(note = "renamed to SlotOverlayEntry")]
pub type OverlayEntry = SlotOverlayEntry;
#[deprecated(note = "renamed to DefDraft")]
pub type SlotDraft = DefDraft;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{LpValue, SlotPath};
    use lpfs::LpPathBuf;

    #[test]
    fn edit_batch_serde_roundtrip() {
        let batch = EditBatch::new(
            EditBatchId(42),
            vec![ArtifactEdit {
                target: EditTarget::Path(LpPathBuf::from("/shader.glsl")),
                ops: vec![
                    EditOp::SetBytes("void main() {}".into()),
                    EditOp::SetSlot {
                        path: SlotPath::root(),
                        value: LpValue::String("Clock".into()),
                    },
                ],
            }],
        );

        let json = serde_json::to_string(&batch).expect("serialize");
        let back: EditBatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, batch);
    }
}
