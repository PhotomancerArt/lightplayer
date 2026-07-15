use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use lpc_model::{
    LpValue, NodeId, SlotData, SlotPath, SlotShapeId, SlotShapeRegistry, SlotShapeRegistrySnapshot,
    slot_sync_codec::read_slot_snapshot_json,
};
use lpc_wire::{WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot, WireSlotRootsSnapshot};

use super::apply::{SlotMirrorError, apply_patch, validate_value_at};

/// Authoritative client-side mirror of synced generic slot data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotMirrorView {
    pub registry: SlotShapeRegistry,
    pub root_shapes: BTreeMap<String, SlotShapeId>,
    pub roots: BTreeMap<String, SlotData>,
}

impl SlotMirrorView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_full_sync(&mut self, sync: WireSlotFullSync) -> Result<(), SlotMirrorError> {
        self.registry.apply_snapshot(sync.registry);
        // A full sync replaces the whole root set, so clear before upserting.
        self.root_shapes.clear();
        self.roots.clear();
        self.apply_roots_snapshot(WireSlotRootsSnapshot { roots: sync.roots })
    }

    /// Upsert the roots present in `sync`, retaining any root not named in the
    /// payload.
    ///
    /// A gated (revision-filtered) read sends only the roots whose content
    /// changed since the client's `since`, so this must merge rather than
    /// clear-and-rebuild. Roots of nodes removed by tree deltas are dropped
    /// separately (see [`Self::drop_roots_for_nodes`]).
    pub fn apply_roots_snapshot(
        &mut self,
        sync: WireSlotRootsSnapshot,
    ) -> Result<(), SlotMirrorError> {
        for root in sync.roots {
            let data = self.read_wire_slot_root(&root)?;
            self.root_shapes.insert(root.name.clone(), root.shape);
            self.roots.insert(root.name, data);
        }
        Ok(())
    }

    /// Drop the slot roots owned by the given nodes.
    ///
    /// Node-owned roots are keyed `node.{id}.def` and `node.{id}.state`. When a
    /// node is removed (via a tree `ChildrenChanged` delta) its roots are not
    /// re-sent, so the client drops them here to avoid retaining stale roots for
    /// nodes that no longer exist.
    pub fn drop_roots_for_nodes(&mut self, removed: &[NodeId]) {
        for id in removed {
            for name in [format!("node.{}.def", id.0), format!("node.{}.state", id.0)] {
                self.root_shapes.remove(&name);
                self.roots.remove(&name);
            }
        }
    }

    pub fn apply_registry_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.registry.apply_snapshot(snapshot);
    }

    pub fn apply_registry_page(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.registry.apply_partial_snapshot(snapshot);
    }

    /// Prune shapes whose id is not in `membership`.
    ///
    /// A gated read carries the full current shape id set (membership sync,
    /// G3/G7) only when the id set changed since the client's `since`; the
    /// client drops any locally-known shape absent from that list.
    pub fn prune_shapes(&mut self, membership: &[SlotShapeId]) {
        self.registry.retain_shapes(|id| membership.contains(id));
    }

    pub fn apply_patches(&mut self, patches: &[WireSlotPatch]) -> Result<(), SlotMirrorError> {
        for patch in patches {
            apply_patch(&mut self.roots, &self.root_shapes, &self.registry, patch)?;
        }
        Ok(())
    }

    pub fn validate_set_value(
        &self,
        root: &str,
        path: &SlotPath,
        value: &LpValue,
    ) -> Result<(), SlotMirrorError> {
        validate_value_at(
            self.roots.get(root).ok_or(SlotMirrorError::UnknownRoot)?,
            self.root_shapes
                .get(root)
                .ok_or(SlotMirrorError::UnknownRoot)?,
            path,
            value,
            &self.registry,
        )
    }

    fn read_wire_slot_root(
        &self,
        root: &WireSlotRootSnapshot,
    ) -> Result<SlotData, SlotMirrorError> {
        read_slot_snapshot_json(&self.registry, root.shape, root.data.get()).map_err(|error| {
            SlotMirrorError::InvalidRootData(format!(
                "root `{}` shape {} did not decode as slot sync snapshot ({})",
                root.name, root.shape, error
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        LpType, Revision, SlotFieldShape, SlotMeta, SlotName, SlotRecord, SlotShape, WithRevision,
    };
    use lpc_wire::{
        WireSlotChange, WireSlotData, WireSlotRootSnapshot, wire_slot_data_from_slot_access,
    };

    #[test]
    fn patches_update_authoritative_mirror() {
        let mut view = fixture();
        view.apply_patches(&[WireSlotPatch {
            root: String::from("engine.shader_node"),
            path: SlotPath::parse("params.exposure").unwrap(),
            change: WireSlotChange::Replace(
                WireSlotData::from_json_string(String::from(
                    r#"{"kind":"value","changed_at":4,"value":2.0}"#,
                ))
                .unwrap(),
            ),
        }])
        .unwrap();

        assert_eq!(
            exposure_value(&view),
            &WithRevision::new(Revision::new(4), LpValue::F32(2.0))
        );
    }

    #[test]
    fn wrong_type_is_rejected_by_validation() {
        let view = fixture();
        let err = view
            .validate_set_value(
                "engine.shader_node",
                &SlotPath::parse("params.exposure").unwrap(),
                &LpValue::Vec3([1.0, 2.0, 3.0]),
            )
            .unwrap_err();

        assert_eq!(err, SlotMirrorError::WrongType);
    }

    fn fixture() -> SlotMirrorView {
        let shape_id = SlotShapeId::from_static_name("engine.shader_node");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_shape_with_version(Revision::new(1), shape_id, shader_node_shape())
            .unwrap();

        let mut view = SlotMirrorView::new();
        view.apply_full_sync(WireSlotFullSync {
            registry: registry.snapshot(),
            roots: vec![WireSlotRootSnapshot {
                name: String::from("engine.shader_node"),
                shape: shape_id,
                data: wire_slot_data_from_slot_access(
                    &registry,
                    shape_id,
                    shader_node_data().access(),
                ),
            }],
        })
        .unwrap();
        view
    }

    fn shader_node_shape() -> SlotShape {
        SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                SlotFieldShape {
                    name: SlotName::parse("params").unwrap(),
                    shape: SlotShape::Record {
                        meta: SlotMeta::empty(),
                        fields: vec![SlotFieldShape {
                            name: SlotName::parse("exposure").unwrap(),
                            shape: SlotShape::value(LpType::F32),
                            semantics: Default::default(),
                            policy: Default::default(),
                            default_bind: None,
                        }],
                    },
                    semantics: Default::default(),
                    policy: Default::default(),
                    default_bind: None,
                },
                SlotFieldShape {
                    name: SlotName::parse("compile_error").unwrap(),
                    shape: SlotShape::value(LpType::String),
                    semantics: Default::default(),
                    policy: Default::default(),
                    default_bind: None,
                },
            ],
        }
    }

    fn shader_node_data() -> SlotData {
        SlotData::Record(SlotRecord::with_revision(
            Revision::new(1),
            vec![
                SlotData::Record(SlotRecord::with_revision(
                    Revision::new(1),
                    vec![SlotData::Value(WithRevision::new(
                        Revision::new(3),
                        LpValue::F32(1.0),
                    ))],
                )),
                SlotData::Value(WithRevision::new(
                    Revision::new(1),
                    LpValue::String(String::from("warning")),
                )),
            ],
        ))
    }

    fn exposure_value(view: &SlotMirrorView) -> &WithRevision<LpValue> {
        let SlotData::Record(root) = view.roots.get("engine.shader_node").unwrap() else {
            panic!("root record");
        };
        let SlotData::Record(params) = &root.fields[0] else {
            panic!("params record");
        };
        let SlotData::Value(value) = &params.fields[0] else {
            panic!("exposure value");
        };
        value
    }
}
