//! Canonical pending edits for a project.

use alloc::collections::BTreeMap;

use crate::{LpPathBuf, SlotPath};

use super::{ArtifactBodyEdit, ArtifactOverlay, OverlayMutation, SlotEdit, SlotOverlay};

/// Current project-wide pending edit intent.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectOverlay {
    pub artifacts: BTreeMap<LpPathBuf, ArtifactOverlay>,
}

impl ProjectOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    pub fn contains_artifact(&self, artifact_path: &LpPathBuf) -> bool {
        self.artifacts.contains_key(artifact_path)
    }

    pub fn artifact(&self, artifact_path: &LpPathBuf) -> Option<&ArtifactOverlay> {
        self.artifacts.get(artifact_path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&LpPathBuf, &ArtifactOverlay)> + '_ {
        self.artifacts
            .iter()
            .filter(|(_, overlay)| !overlay.is_empty())
    }

    pub fn put_slot_edit(&mut self, artifact_path: LpPathBuf, edit: SlotEdit) -> bool {
        let changed = match self.artifacts.get_mut(&artifact_path) {
            Some(overlay) => overlay.put_slot_edit(edit),
            None => {
                let mut slot = SlotOverlay::new();
                slot.put_edit(edit);
                self.artifacts
                    .insert(artifact_path.clone(), ArtifactOverlay::slot(slot));
                true
            }
        };
        self.remove_empty_artifact(&artifact_path);
        changed
    }

    pub fn remove_slot_edit(&mut self, artifact_path: &LpPathBuf, path: &SlotPath) -> bool {
        let changed = match self.artifacts.get_mut(artifact_path) {
            Some(ArtifactOverlay::Slot { overlay }) => overlay.remove_edit(path),
            Some(ArtifactOverlay::Body { .. }) | None => false,
        };
        self.remove_empty_artifact(artifact_path);
        changed
    }

    pub fn set_artifact_body(&mut self, artifact_path: LpPathBuf, edit: ArtifactBodyEdit) -> bool {
        let next = ArtifactOverlay::body(edit);
        if self.artifacts.get(&artifact_path) == Some(&next) {
            return false;
        }
        self.artifacts.insert(artifact_path, next);
        true
    }

    pub fn clear_artifact(&mut self, artifact_path: &LpPathBuf) -> bool {
        self.artifacts.remove(artifact_path).is_some()
    }

    pub fn clear(&mut self) -> bool {
        let changed = !self.artifacts.is_empty();
        self.artifacts.clear();
        changed
    }

    pub fn apply_mutation(&mut self, mutation: OverlayMutation) -> bool {
        match mutation {
            OverlayMutation::PutSlotEdit {
                artifact_path,
                edit,
            } => self.put_slot_edit(artifact_path, edit),
            OverlayMutation::RemoveSlotEdit {
                artifact_path,
                path,
            } => self.remove_slot_edit(&artifact_path, &path),
            OverlayMutation::SetArtifactBody {
                artifact_path,
                edit,
            } => self.set_artifact_body(artifact_path, edit),
            OverlayMutation::ClearArtifact { artifact_path } => self.clear_artifact(&artifact_path),
            OverlayMutation::Clear => self.clear(),
        }
    }

    pub fn merge_from(&mut self, other: &ProjectOverlay) {
        for (artifact_path, overlay) in other.iter() {
            match overlay {
                ArtifactOverlay::Slot { overlay } => {
                    for edit in overlay.to_apply_plan() {
                        self.put_slot_edit(artifact_path.clone(), edit);
                    }
                }
                ArtifactOverlay::Body { edit } => {
                    self.set_artifact_body(artifact_path.clone(), edit.clone());
                }
            }
        }
    }

    fn remove_empty_artifact(&mut self, artifact_path: &LpPathBuf) {
        if self
            .artifacts
            .get(artifact_path)
            .is_some_and(ArtifactOverlay::is_empty)
        {
            self.artifacts.remove(artifact_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LpValue, SlotEditOp};

    #[test]
    fn body_and_slot_overlays_are_exclusive() {
        let mut overlay = ProjectOverlay::new();
        let path = LpPathBuf::from("/shader.glsl");
        overlay.set_artifact_body(
            path.clone(),
            ArtifactBodyEdit::ReplaceBody(b"body".to_vec()),
        );
        assert!(matches!(
            overlay.artifact(&path),
            Some(ArtifactOverlay::Body { .. })
        ));

        overlay.put_slot_edit(
            path.clone(),
            SlotEdit::ensure_present(SlotPath::parse("Shader").unwrap()),
        );
        assert!(matches!(
            overlay.artifact(&path),
            Some(ArtifactOverlay::Slot { .. })
        ));
    }

    #[test]
    fn clear_empty_slot_overlay_removes_artifact() {
        let mut overlay = ProjectOverlay::new();
        let artifact_path = LpPathBuf::from("/project.toml");
        let slot_path = SlotPath::parse("nodes[clock]").unwrap();
        overlay.put_slot_edit(
            artifact_path.clone(),
            SlotEdit::assign_value(slot_path.clone(), LpValue::String("x".into())),
        );

        assert!(overlay.remove_slot_edit(&artifact_path, &slot_path));
        assert!(overlay.is_empty());
    }

    #[test]
    fn apply_mutation_updates_canonical_overlay() {
        let mut overlay = ProjectOverlay::new();
        let artifact_path = LpPathBuf::from("/project.toml");
        let slot_path = SlotPath::parse("nodes[clock]").unwrap();

        assert!(overlay.apply_mutation(OverlayMutation::PutSlotEdit {
            artifact_path: artifact_path.clone(),
            edit: SlotEdit::ensure_present(slot_path.clone()),
        }));

        let Some(ArtifactOverlay::Slot { overlay: slot }) = overlay.artifact(&artifact_path) else {
            panic!("expected slot overlay");
        };
        assert_eq!(slot.edits.get(&slot_path), Some(&SlotEditOp::EnsurePresent));
    }
}
