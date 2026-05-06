use crate::{FrameId, ModelType, SlotMapKeyShape, SlotMeta, SlotName, SlotNameError, SlotShapeId};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

/// Child edge in the shape graph.
///
/// `Owned` children are removed when the owning root is unregistered. `Ref`
/// children point to a shape owned elsewhere.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotShapeChild {
    Owned(SlotShapeId),
    Ref(SlotShapeId),
}

impl SlotShapeChild {
    pub fn id(&self) -> &SlotShapeId {
        match self {
            Self::Owned(id) | Self::Ref(id) => id,
        }
    }
}

/// One registered shape node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotShapeNode {
    Value {
        #[serde(default)]
        meta: SlotMeta,
        ty: ModelType,
    },
    Record {
        #[serde(default)]
        meta: SlotMeta,
        fields: Vec<SlotShapeField>,
    },
    Map {
        #[serde(default)]
        meta: SlotMeta,
        key: SlotMapKeyShape,
        value: SlotShapeChild,
    },
    Enum {
        #[serde(default)]
        meta: SlotMeta,
        variants: Vec<SlotShapeVariant>,
    },
    Option {
        #[serde(default)]
        meta: SlotMeta,
        some: SlotShapeChild,
    },
}

impl SlotShapeNode {
    pub fn value(ty: ModelType) -> Self {
        Self::Value {
            meta: SlotMeta::empty(),
            ty,
        }
    }

    fn owned_children(&self) -> Vec<&SlotShapeId> {
        let mut out = Vec::new();
        match self {
            Self::Value { .. } => {}
            Self::Record { fields, .. } => {
                for field in fields {
                    if let SlotShapeChild::Owned(id) = &field.shape {
                        out.push(id);
                    }
                }
            }
            Self::Map { value, .. } => {
                if let SlotShapeChild::Owned(id) = value {
                    out.push(id);
                }
            }
            Self::Enum { variants, .. } => {
                for variant in variants {
                    if let SlotShapeChild::Owned(id) = &variant.shape {
                        out.push(id);
                    }
                }
            }
            Self::Option { some, .. } => {
                if let SlotShapeChild::Owned(id) = some {
                    out.push(id);
                }
            }
        }
        out
    }
}

/// Field shape edge inside a record node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeField {
    pub name: SlotName,
    pub shape: SlotShapeChild,
}

impl SlotShapeField {
    pub fn new(name: &str, shape: SlotShapeChild) -> Result<Self, SlotNameError> {
        Ok(Self {
            name: SlotName::parse(name)?,
            shape,
        })
    }
}

/// Variant shape edge inside an enum node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeVariant {
    pub name: SlotName,
    pub shape: SlotShapeChild,
}

impl SlotShapeVariant {
    pub fn new(name: &str, shape: SlotShapeChild) -> Result<Self, SlotNameError> {
        Ok(Self {
            name: SlotName::parse(name)?,
            shape,
        })
    }
}

/// Shape node plus the frame where that node last changed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct VersionedSlotShapeNode {
    pub node: SlotShapeNode,
    pub changed_frame: FrameId,
}

/// Registry of id-addressed slot shape nodes.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistry {
    pub ids_changed_frame: FrameId,
    shapes: BTreeMap<SlotShapeId, VersionedSlotShapeNode>,
    owned_roots: BTreeMap<SlotShapeId, BTreeSet<SlotShapeId>>,
}

impl SlotShapeRegistry {
    pub fn register_tree(
        &mut self,
        frame: FrameId,
        root: SlotShapeId,
        nodes: Vec<(SlotShapeId, SlotShapeNode)>,
    ) -> Result<(), SlotShapeRegistryError> {
        let mut pending = BTreeMap::new();
        for (id, node) in nodes {
            if pending.insert(id, node).is_some() {
                return Err(SlotShapeRegistryError::DuplicateShapeId(id));
            }
        }
        if !pending.contains_key(&root) {
            return Err(SlotShapeRegistryError::MissingRoot(root));
        }
        let mut owned = BTreeSet::new();
        collect_owned(&root, &pending, &mut owned)?;
        for id in &owned {
            if self.shapes.contains_key(id) {
                return Err(SlotShapeRegistryError::DuplicateShapeId(*id));
            }
            let node = pending
                .get(id)
                .ok_or(SlotShapeRegistryError::MissingOwnedChild(*id))?;
            self.shapes.insert(
                *id,
                VersionedSlotShapeNode {
                    node: node.clone(),
                    changed_frame: frame,
                },
            );
        }
        self.owned_roots.insert(root, owned);
        self.ids_changed_frame = frame;
        Ok(())
    }

    pub fn unregister_tree(&mut self, frame: FrameId, root: &SlotShapeId) {
        if let Some(owned) = self.owned_roots.remove(root) {
            for id in owned {
                self.shapes.remove(&id);
            }
            self.ids_changed_frame = frame;
        }
    }

    pub fn get(&self, id: &SlotShapeId) -> Option<&SlotShapeNode> {
        self.shapes.get(id).map(|entry| &entry.node)
    }

    pub fn entry(&self, id: &SlotShapeId) -> Option<&VersionedSlotShapeNode> {
        self.shapes.get(id)
    }

    pub fn snapshot(&self) -> SlotShapeRegistrySnapshot {
        SlotShapeRegistrySnapshot {
            ids_changed_frame: self.ids_changed_frame,
            shapes: self.shapes.clone(),
            owned_roots: self.owned_roots.clone(),
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.ids_changed_frame = snapshot.ids_changed_frame;
        self.shapes = snapshot.shapes;
        self.owned_roots = snapshot.owned_roots;
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeRegistrySnapshot {
    pub ids_changed_frame: FrameId,
    pub shapes: BTreeMap<SlotShapeId, VersionedSlotShapeNode>,
    pub owned_roots: BTreeMap<SlotShapeId, BTreeSet<SlotShapeId>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotShapeRegistryError {
    MissingRoot(SlotShapeId),
    MissingOwnedChild(SlotShapeId),
    DuplicateShapeId(SlotShapeId),
}

impl core::fmt::Display for SlotShapeRegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingRoot(id) => write!(f, "missing shape registry root: {id}"),
            Self::MissingOwnedChild(id) => write!(f, "missing owned child shape: {id}"),
            Self::DuplicateShapeId(id) => write!(f, "duplicate slot shape id: {id}"),
        }
    }
}

impl core::error::Error for SlotShapeRegistryError {}

fn collect_owned(
    id: &SlotShapeId,
    pending: &BTreeMap<SlotShapeId, SlotShapeNode>,
    owned: &mut BTreeSet<SlotShapeId>,
) -> Result<(), SlotShapeRegistryError> {
    if !owned.insert(*id) {
        return Ok(());
    }
    let node = pending
        .get(id)
        .ok_or(SlotShapeRegistryError::MissingOwnedChild(*id))?;
    for child in node.owned_children() {
        collect_owned(child, pending, owned)?;
    }
    Ok(())
}
