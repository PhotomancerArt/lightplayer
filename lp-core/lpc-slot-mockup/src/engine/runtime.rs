use lpc_model::{
    Revision, SlotAccess, SlotMutAccess, SlotMutationError, SlotPath, SlotShapeId,
    SlotShapeRegistry, current_revision, set_current_revision, set_slot_value, slot_data_revision,
};
use lpc_wire::{
    WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};

use crate::source::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef};

use super::{FixtureNode, OutputNode, ShaderNode};

pub struct MockRuntime {
    pub registry: SlotShapeRegistry,
    pub project: ProjectDef,
    pub shader_def: ShaderDef,
    pub fixture_def: FixtureDef,
    pub output_def: OutputDef,
    pub texture_def: TextureDef,
    pub shader_node: ShaderNode,
    pub fixture_node: FixtureNode,
    pub output_node: OutputNode,
}

impl MockRuntime {
    pub fn new() -> Self {
        set_current_revision(Revision::new(1));

        let mut registry = SlotShapeRegistry::default();
        crate::model::register_shapes(&mut registry).unwrap();

        let shader_def = ShaderDef::new();
        let shader_node = ShaderNode::from_def(&shader_def);
        // Shader runtime params are dynamic: the shape is owned by this loaded
        // node/artifact instance, not by the Rust `ShaderNode` type.
        registry
            .register_shape(shader_node.shape_id(), shader_node.shape())
            .unwrap();

        Self {
            registry,
            project: ProjectDef::new(),
            shader_node,
            fixture_def: FixtureDef::new(),
            output_def: OutputDef::new(),
            texture_def: TextureDef::new(),
            fixture_node: FixtureNode::new(),
            output_node: OutputNode::new(),
            shader_def,
        }
    }

    pub fn roots(&self) -> Vec<(&str, &dyn SlotAccess)> {
        vec![
            ("source.project", &self.project),
            ("source.shader", &self.shader_def),
            ("source.fixture", &self.fixture_def),
            ("source.output", &self.output_def),
            ("source.texture", &self.texture_def),
            ("engine.shader_node", &self.shader_node),
            ("engine.fixture_node", &self.fixture_node),
            ("engine.output_node", &self.output_node),
        ]
    }

    pub fn add_shader_param_def(&mut self, frame: Revision, name: &str, default: f32) {
        set_current_revision(frame);
        self.shader_def.add_param_def(name, default);
    }

    pub fn set_shader_param(&mut self, frame: Revision, name: &str, value: f32) {
        set_current_revision(frame);
        self.shader_node.set_param(name, value);
    }

    pub fn change_shader_param_to_vec3(
        &mut self,
        frame: Revision,
        name: &str,
        param_value: [f32; 3],
    ) {
        set_current_revision(frame);
        self.shader_def.set_param_value_type(name, "vec3");
        self.shader_node.set_param_vec3(name, param_value);
        self.refresh_shader_node_shape();
    }

    pub fn remove_shader_param(&mut self, frame: Revision, name: &str) {
        set_current_revision(frame);
        self.shader_node.remove_param(name);
        self.refresh_shader_node_shape();
    }

    pub fn clear_compile_error(&mut self, frame: Revision) {
        set_current_revision(frame);
        self.shader_node.clear_compile_error();
    }

    pub fn switch_fixture_mapping(&mut self, frame: Revision) {
        set_current_revision(frame);
        self.fixture_def.switch_mapping_to_square();
        self.fixture_node.switch_mapping_preview();
    }

    pub fn disable_fixture_mapping(&mut self, frame: Revision) {
        set_current_revision(frame);
        self.fixture_def.disable_mapping();
        self.fixture_node.disable_mapping_preview();
    }

    pub fn clear_fixture_brightness(&mut self, frame: Revision) {
        set_current_revision(frame);
        self.fixture_def.clear_brightness();
    }

    pub fn set_fixture_ring_lamp_counts(&mut self, frame: Revision, counts: Vec<u32>) {
        set_current_revision(frame);
        assert!(
            self.fixture_def.set_ring_lamp_counts(counts),
            "fixture mapping must be path_points/ring_array in the mockup"
        );
    }

    pub fn remove_touch(&mut self, frame: Revision, id: u32) {
        set_current_revision(frame);
        self.fixture_node.remove_touch(id);
    }

    pub fn apply_slot_mutation(
        &mut self,
        frame: Revision,
        request: WireSlotMutationRequest,
    ) -> WireSlotMutationResponse {
        set_current_revision(frame);
        let result = self.apply_slot_mutation_result(&request);
        WireSlotMutationResponse {
            id: request.id,
            result,
        }
    }

    fn refresh_shader_node_shape(&mut self) {
        self.registry
            .replace_shape(self.shader_node.shape_id(), self.shader_node.shape());
    }

    fn apply_slot_mutation_result(
        &mut self,
        request: &WireSlotMutationRequest,
    ) -> WireSlotMutationResult {
        let info = match self.mutation_target_info(&request.root, &request.path) {
            Ok(info) => info,
            Err(rejection) => return WireSlotMutationResult::Rejected(rejection),
        };

        if info.shape_version != request.expected_shape_version {
            return WireSlotMutationResult::Rejected(WireSlotMutationRejection::ShapeConflict {
                current_version: info.shape_version,
            });
        }
        if info.data_version != request.expected_data_version {
            return WireSlotMutationResult::Rejected(WireSlotMutationRejection::DataConflict {
                current_version: info.data_version,
            });
        }

        match &request.op {
            WireSlotMutationOp::SetValue(value) => {
                let registry = self.registry.clone();
                let root = match self.root_mut(&request.root) {
                    Ok(root) => root,
                    Err(rejection) => return WireSlotMutationResult::Rejected(rejection),
                };
                match set_slot_value(
                    root,
                    &registry,
                    &request.path,
                    current_revision(),
                    value.clone(),
                ) {
                    Ok(()) => WireSlotMutationResult::Accepted,
                    Err(error) => {
                        WireSlotMutationResult::Rejected(mutation_error_to_rejection(error))
                    }
                }
            }
        }
    }

    fn mutation_target_info(
        &self,
        root: &str,
        path: &SlotPath,
    ) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
        let root = self.root(root)?;
        Ok(MutationTargetInfo {
            shape_version: self.root_shape_version(root.shape_id())?,
            data_version: slot_data_revision(root, &self.registry, path)
                .map_err(mutation_error_to_rejection)?,
        })
    }

    fn root_shape_version(
        &self,
        shape_id: SlotShapeId,
    ) -> Result<Revision, WireSlotMutationRejection> {
        self.registry
            .entry(&shape_id)
            .map(|entry| entry.changed_at())
            .ok_or(WireSlotMutationRejection::UnknownRoot)
    }

    fn root(&self, root: &str) -> Result<&dyn SlotAccess, WireSlotMutationRejection> {
        match root {
            "source.project" => Ok(&self.project),
            "source.shader" => Ok(&self.shader_def),
            "source.fixture" => Ok(&self.fixture_def),
            "source.output" => Ok(&self.output_def),
            "source.texture" => Ok(&self.texture_def),
            "engine.shader_node" => Ok(&self.shader_node),
            "engine.fixture_node" => Ok(&self.fixture_node),
            "engine.output_node" => Ok(&self.output_node),
            _ => Err(WireSlotMutationRejection::UnknownRoot),
        }
    }

    fn root_mut(
        &mut self,
        root: &str,
    ) -> Result<&mut dyn SlotMutAccess, WireSlotMutationRejection> {
        match root {
            "source.project" => Ok(&mut self.project),
            "source.shader" => Ok(&mut self.shader_def),
            "source.fixture" => Ok(&mut self.fixture_def),
            "source.output" => Ok(&mut self.output_def),
            "source.texture" => Ok(&mut self.texture_def),
            "engine.shader_node" => Ok(&mut self.shader_node),
            _ => Err(WireSlotMutationRejection::UnknownRoot),
        }
    }
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self::new()
    }
}

struct MutationTargetInfo {
    shape_version: Revision,
    data_version: Revision,
}

fn mutation_error_to_rejection(error: SlotMutationError) -> WireSlotMutationRejection {
    match error {
        SlotMutationError::WrongType { .. } => WireSlotMutationRejection::WrongType,
        SlotMutationError::UnknownVariant { .. } | SlotMutationError::UnknownPath { .. } => {
            WireSlotMutationRejection::UnknownPath
        }
        SlotMutationError::UnsupportedTarget { .. } => WireSlotMutationRejection::UnsupportedTarget,
    }
}
