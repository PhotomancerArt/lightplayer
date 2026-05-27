//! Edit vocabulary, artifact overlay storage, and apply.

pub(crate) mod apply;
mod artifact_edit;
mod artifact_overlay;
mod asset_edit;
mod commit_error;
mod edit_batch;
mod edit_error;
mod edit_target;
mod pending_slot_target;
mod slot_edit;

pub use apply::{apply_artifact_edit, apply_edit_batch, require_absolute_path};
pub use artifact_edit::ArtifactEdit;
pub use artifact_overlay::{ArtifactEdits, ArtifactOverlay, PendingAsset};
pub use asset_edit::AssetEdit;
pub use commit_error::CommitError;
pub use edit_batch::{EditBatch, EditBatchId};
pub use edit_error::EditError;
pub use edit_target::EditTarget;
pub use pending_slot_target::PendingSlotTarget;
pub use slot_edit::SlotEdit;

#[deprecated(note = "renamed to ArtifactEdit")]
pub type ArtifactChange = ArtifactEdit;
#[deprecated(note = "split into SlotEdit and AssetEdit")]
pub type ArtifactOp = SlotEdit;
#[deprecated(note = "renamed to EditTarget")]
pub type ArtifactTarget = EditTarget;
#[deprecated(note = "renamed to EditBatch")]
pub type ChangeSet = EditBatch;
#[deprecated(note = "renamed to EditBatchId")]
pub type ChangeSetId = EditBatchId;
#[deprecated(note = "renamed to EditError")]
pub type ChangeError = EditError;
#[deprecated(note = "renamed to ArtifactOverlay")]
pub type ChangeOverlay = ArtifactOverlay;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::SlotPath;
    use lpfs::LpPathBuf;

    #[test]
    fn edit_batch_serde_roundtrip() {
        let batch = EditBatch::new(
            EditBatchId(42),
            vec![
                ArtifactEdit::asset(
                    EditTarget::Path(LpPathBuf::from("/shader.glsl")),
                    vec![AssetEdit::ReplaceBody("void main() {}".into())],
                ),
                ArtifactEdit::slot(
                    EditTarget::Path(LpPathBuf::from("/shader.toml")),
                    vec![SlotEdit::UseEnumVariant {
                        path: SlotPath::root(),
                        variant: "Clock".into(),
                    }],
                ),
            ],
        );

        let json = serde_json::to_string(&batch).expect("serialize");
        assert!(json.contains("\"kind\":\"Asset\""));
        assert!(json.contains("\"kind\":\"Slot\""));
        let back: EditBatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, batch);
    }
}
