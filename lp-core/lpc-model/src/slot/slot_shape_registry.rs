use crate::{FrameId, SlotShape, SlotShapeId, current_state_version};
use alloc::collections::BTreeMap;

/// Shape root plus the frame where that root last changed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct VersionedSlotShape {
    pub node: SlotShape,
    pub changed_frame: FrameId,
}

/// Registry of id-addressed slot shape roots.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistry {
    pub ids_changed_frame: FrameId,
    shapes: BTreeMap<SlotShapeId, VersionedSlotShape>,
}

impl SlotShapeRegistry {
    pub fn register_tree(
        &mut self,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        self.register_tree_with_version(current_state_version(), root, shape)
    }

    pub fn register_tree_with_version(
        &mut self,
        frame: FrameId,
        root: SlotShapeId,
        shape: SlotShape,
    ) -> Result<(), SlotShapeRegistryError> {
        if self.shapes.contains_key(&root) {
            return Err(SlotShapeRegistryError::DuplicateShapeId(root));
        }
        self.shapes.insert(
            root,
            VersionedSlotShape {
                node: shape,
                changed_frame: frame,
            },
        );
        self.ids_changed_frame = frame;
        Ok(())
    }

    pub fn unregister_tree(&mut self, root: &SlotShapeId) {
        self.unregister_tree_with_version(current_state_version(), root);
    }

    pub fn unregister_tree_with_version(&mut self, frame: FrameId, root: &SlotShapeId) {
        if self.shapes.remove(root).is_some() {
            self.ids_changed_frame = frame;
        }
    }

    pub fn get(&self, id: &SlotShapeId) -> Option<&SlotShape> {
        self.shapes.get(id).map(|entry| &entry.node)
    }

    pub fn entry(&self, id: &SlotShapeId) -> Option<&VersionedSlotShape> {
        self.shapes.get(id)
    }

    pub fn snapshot(&self) -> SlotShapeRegistrySnapshot {
        SlotShapeRegistrySnapshot {
            ids_changed_frame: self.ids_changed_frame,
            shapes: self.shapes.clone(),
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_changed_frame = snapshot.ids_changed_frame;
        self.shapes = snapshot.shapes;
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistrySnapshot {
    pub ids_changed_frame: FrameId,
    pub shapes: BTreeMap<SlotShapeId, VersionedSlotShape>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotShapeRegistryError {
    DuplicateShapeId(SlotShapeId),
}

impl core::fmt::Display for SlotShapeRegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateShapeId(id) => write!(f, "duplicate slot shape id: {id}"),
        }
    }
}

impl core::error::Error for SlotShapeRegistryError {}
