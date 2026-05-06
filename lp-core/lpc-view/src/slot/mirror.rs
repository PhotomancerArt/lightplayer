use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use lpc_model::{
    ModelValue, SlotData, SlotPath, SlotShapeId, SlotShapeRegistry, SlotShapeRegistrySnapshot,
};
use lpc_wire::{
    WireSlotFullSync, WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection,
    WireSlotMutationRequest, WireSlotMutationResponse, WireSlotMutationResult, WireSlotPatch,
};

use super::apply::{
    SlotMirrorError, apply_patch, data_version_at, shape_version_for_root, validate_value_at,
};
use super::pending::PendingSlotMutation;

/// Authoritative client-side mirror of synced generic slot data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotMirrorView {
    pub registry: SlotShapeRegistry,
    pub root_shapes: BTreeMap<String, SlotShapeId>,
    pub roots: BTreeMap<String, SlotData>,
    pub pending: BTreeMap<WireSlotMutationId, PendingSlotMutation>,
    pub errors: BTreeMap<WireSlotMutationId, WireSlotMutationRejection>,
}

impl SlotMirrorView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_full_sync(&mut self, sync: WireSlotFullSync) {
        self.registry.apply_snapshot(sync.registry);
        self.root_shapes.clear();
        self.roots.clear();
        for root in sync.roots {
            self.root_shapes.insert(root.name.clone(), root.shape);
            self.roots.insert(root.name, root.data);
        }
    }

    pub fn apply_registry_snapshot(&mut self, snapshot: SlotShapeRegistrySnapshot) {
        self.registry.apply_snapshot(snapshot);
    }

    pub fn apply_patches(&mut self, patches: &[WireSlotPatch]) -> Result<(), SlotMirrorError> {
        for patch in patches {
            apply_patch(&mut self.roots, &self.root_shapes, &self.registry, patch)?;
        }
        Ok(())
    }

    pub fn prepare_set_value(
        &mut self,
        id: WireSlotMutationId,
        root: &str,
        path: SlotPath,
        value: ModelValue,
    ) -> Result<WireSlotMutationRequest, SlotMirrorError> {
        validate_value_at(
            self.roots.get(root).ok_or(SlotMirrorError::UnknownRoot)?,
            self.root_shapes
                .get(root)
                .ok_or(SlotMirrorError::UnknownRoot)?,
            &path,
            &value,
            &self.registry,
        )?;

        let request = WireSlotMutationRequest {
            id,
            root: root.to_string(),
            expected_shape_version: shape_version_for_root(
                root,
                &self.root_shapes,
                &self.registry,
            )?,
            expected_data_version: data_version_at(
                self.roots.get(root).ok_or(SlotMirrorError::UnknownRoot)?,
                self.root_shapes
                    .get(root)
                    .ok_or(SlotMirrorError::UnknownRoot)?,
                &path,
                &self.registry,
            )?,
            path,
            op: WireSlotMutationOp::SetValue(value),
        };
        self.pending
            .insert(id, PendingSlotMutation::new(request.clone()));
        self.errors.remove(&id);
        Ok(request)
    }

    pub fn apply_mutation_response(&mut self, response: WireSlotMutationResponse) {
        self.pending.remove(&response.id);
        match response.result {
            WireSlotMutationResult::Accepted => {
                self.errors.remove(&response.id);
            }
            WireSlotMutationResult::Rejected(rejection) => {
                self.errors.insert(response.id, rejection);
            }
        }
    }

    pub fn is_pending(&self, id: WireSlotMutationId) -> bool {
        self.pending.contains_key(&id)
    }

    pub fn error(&self, id: WireSlotMutationId) -> Option<&WireSlotMutationRejection> {
        self.errors.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        FrameId, ModelType, SlotFieldShape, SlotMeta, SlotName, SlotRecord, SlotShape, Versioned,
    };
    use lpc_wire::{WireSlotChange, WireSlotRootSnapshot};

    #[test]
    fn set_value_mutation_tracks_pending_without_local_write() {
        let mut view = fixture();
        let id = WireSlotMutationId::new(1);

        let request = view
            .prepare_set_value(
                id,
                "engine.shader_node",
                SlotPath::parse("params.exposure").unwrap(),
                ModelValue::F32(2.0),
            )
            .unwrap();

        assert!(view.is_pending(id));
        assert_eq!(request.expected_shape_version, FrameId::new(1));
        assert_eq!(request.expected_data_version, FrameId::new(3));
        assert_eq!(
            exposure_value(&view),
            &Versioned::new(FrameId::new(3), ModelValue::F32(1.0))
        );
    }

    #[test]
    fn accepted_response_clears_pending_without_local_write() {
        let mut view = fixture();
        let id = WireSlotMutationId::new(1);
        view.prepare_set_value(
            id,
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();

        view.apply_mutation_response(WireSlotMutationResponse {
            id,
            result: WireSlotMutationResult::Accepted,
        });

        assert!(!view.is_pending(id));
        assert!(view.error(id).is_none());
        assert_eq!(
            exposure_value(&view),
            &Versioned::new(FrameId::new(3), ModelValue::F32(1.0))
        );
    }

    #[test]
    fn rejected_response_records_error() {
        let mut view = fixture();
        let id = WireSlotMutationId::new(1);
        view.prepare_set_value(
            id,
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();

        view.apply_mutation_response(WireSlotMutationResponse {
            id,
            result: WireSlotMutationResult::Rejected(WireSlotMutationRejection::DataConflict {
                current_version: FrameId::new(4),
            }),
        });

        assert!(!view.is_pending(id));
        assert_eq!(
            view.error(id),
            Some(&WireSlotMutationRejection::DataConflict {
                current_version: FrameId::new(4)
            })
        );
    }

    #[test]
    fn patches_update_authoritative_mirror() {
        let mut view = fixture();
        view.apply_patches(&[WireSlotPatch {
            root: String::from("engine.shader_node"),
            path: SlotPath::parse("params.exposure").unwrap(),
            change: WireSlotChange::Replace(SlotData::Value(Versioned::new(
                FrameId::new(4),
                ModelValue::F32(2.0),
            ))),
        }])
        .unwrap();

        assert_eq!(
            exposure_value(&view),
            &Versioned::new(FrameId::new(4), ModelValue::F32(2.0))
        );
    }

    #[test]
    fn wrong_type_is_rejected_before_pending() {
        let mut view = fixture();
        let err = view
            .prepare_set_value(
                WireSlotMutationId::new(1),
                "engine.shader_node",
                SlotPath::parse("params.exposure").unwrap(),
                ModelValue::Vec3([1.0, 2.0, 3.0]),
            )
            .unwrap_err();

        assert_eq!(err, SlotMirrorError::WrongType);
        assert!(view.pending.is_empty());
    }

    fn fixture() -> SlotMirrorView {
        let shape_id = SlotShapeId::from_static_name("engine.shader_node");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_tree_with_version(FrameId::new(1), shape_id, shader_node_shape())
            .unwrap();

        let mut view = SlotMirrorView::new();
        view.apply_full_sync(WireSlotFullSync {
            registry: registry.snapshot(),
            roots: vec![WireSlotRootSnapshot {
                name: String::from("engine.shader_node"),
                shape: shape_id,
                data: shader_node_data(),
            }],
        });
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
                            shape: SlotShape::value(ModelType::F32),
                        }],
                    },
                },
                SlotFieldShape {
                    name: SlotName::parse("compile_error").unwrap(),
                    shape: SlotShape::value(ModelType::String),
                },
            ],
        }
    }

    fn shader_node_data() -> SlotData {
        SlotData::Record(SlotRecord::with_version(
            FrameId::new(1),
            vec![
                SlotData::Record(SlotRecord::with_version(
                    FrameId::new(1),
                    vec![SlotData::Value(Versioned::new(
                        FrameId::new(3),
                        ModelValue::F32(1.0),
                    ))],
                )),
                SlotData::Value(Versioned::new(
                    FrameId::new(1),
                    ModelValue::String(String::from("warning")),
                )),
            ],
        ))
    }

    fn exposure_value(view: &SlotMirrorView) -> &Versioned<ModelValue> {
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
