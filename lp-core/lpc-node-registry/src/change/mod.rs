//! Client change vocabulary and overlay apply (ChangeSet roadmap M1).

pub(crate) mod apply;
mod artifact_change;
mod artifact_op;
mod artifact_target;
mod change_error;
mod change_set;
mod overlay;
mod slot_draft;

pub use apply::{apply_change, apply_changeset, require_absolute_path};
pub use artifact_change::ArtifactChange;
pub use artifact_op::ArtifactOp;
pub use artifact_target::ArtifactTarget;
pub use change_error::ChangeError;
pub use change_set::{ChangeSet, ChangeSetId};
pub use overlay::{ChangeOverlay, OverlayEntry};
pub use slot_draft::SlotDraft;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{LpValue, SlotPath};
    use lpfs::LpPathBuf;

    #[test]
    fn changeset_serde_roundtrip() {
        let changeset = ChangeSet::new(
            ChangeSetId(42),
            vec![ArtifactChange {
                target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
                ops: vec![
                    ArtifactOp::SetBytes("void main() {}".into()),
                    ArtifactOp::SetSlot {
                        path: SlotPath::root(),
                        value: LpValue::String("Clock".into()),
                    },
                ],
            }],
        );

        let json = serde_json::to_string(&changeset).expect("serialize");
        let back: ChangeSet = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, changeset);
    }
}
