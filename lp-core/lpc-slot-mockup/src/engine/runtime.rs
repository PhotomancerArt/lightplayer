use lpc_model::{
    FrameId, ModelType, ModelValue, SlotAccess, SlotPath, SlotShapeId, SlotShapeRegistry,
    StaticSlotAccess, set_current_state_version,
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
        set_current_state_version(FrameId::new(1));

        let mut registry = SlotShapeRegistry::default();
        crate::model::register_shapes(&mut registry);

        let shader_def = ShaderDef::new();
        let shader_node = ShaderNode::from_def(&shader_def);
        registry
            .register_tree(ShaderNode::SHAPE_ID, shader_node.shape())
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

    pub fn add_shader_param_def(&mut self, frame: FrameId, name: &str, default: f32) {
        set_current_state_version(frame);
        self.shader_def.add_param_def(name, default);
    }

    pub fn set_shader_param(&mut self, frame: FrameId, name: &str, value: f32) {
        set_current_state_version(frame);
        self.shader_node.set_param(name, value);
    }

    pub fn change_shader_param_to_vec3(
        &mut self,
        frame: FrameId,
        name: &str,
        param_value: [f32; 3],
    ) {
        set_current_state_version(frame);
        self.shader_def.set_param_value_type(name, "vec3");
        self.shader_node.set_param_vec3(name, param_value);
        self.refresh_shader_node_shape();
    }

    pub fn remove_shader_param(&mut self, frame: FrameId, name: &str) {
        set_current_state_version(frame);
        self.shader_node.remove_param(name);
        self.refresh_shader_node_shape();
    }

    pub fn clear_compile_error(&mut self, frame: FrameId) {
        set_current_state_version(frame);
        self.shader_node.clear_compile_error();
    }

    pub fn switch_fixture_mapping(&mut self, frame: FrameId) {
        set_current_state_version(frame);
        self.fixture_def.switch_mapping_to_square();
        self.fixture_node.switch_mapping_preview();
    }

    pub fn disable_fixture_mapping(&mut self, frame: FrameId) {
        set_current_state_version(frame);
        self.fixture_def.disable_mapping();
        self.fixture_node.disable_mapping_preview();
    }

    pub fn clear_fixture_brightness(&mut self, frame: FrameId) {
        set_current_state_version(frame);
        self.fixture_def.clear_brightness();
    }

    pub fn remove_touch(&mut self, frame: FrameId, id: u32) {
        set_current_state_version(frame);
        self.fixture_node.remove_touch(id);
    }

    pub fn apply_slot_mutation(
        &mut self,
        frame: FrameId,
        request: WireSlotMutationRequest,
    ) -> WireSlotMutationResponse {
        set_current_state_version(frame);
        let result = self.apply_slot_mutation_result(&request);
        WireSlotMutationResponse {
            id: request.id,
            result,
        }
    }

    fn refresh_shader_node_shape(&mut self) {
        self.registry
            .replace_tree(ShaderNode::SHAPE_ID, self.shader_node.shape());
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

        let WireSlotMutationOp::SetValue(value) = &request.op;
        if !model_value_matches_type(value, &info.ty) {
            return WireSlotMutationResult::Rejected(WireSlotMutationRejection::WrongType);
        }

        match (&info.target, value) {
            (MutationTarget::ShaderExposureParam, ModelValue::F32(value)) => {
                self.shader_node.set_param("exposure", *value);
                WireSlotMutationResult::Accepted
            }
            (MutationTarget::ShaderExposureLabel, ModelValue::String(value)) => {
                self.shader_def.set_param_label("exposure", value);
                WireSlotMutationResult::Accepted
            }
            (MutationTarget::Unsupported, _) => {
                WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
            }
            _ => WireSlotMutationResult::Rejected(WireSlotMutationRejection::WrongType),
        }
    }

    fn mutation_target_info(
        &self,
        root: &str,
        path: &SlotPath,
    ) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
        let path = path.to_string();
        match root {
            "engine.shader_node" => self.shader_node_mutation_target_info(&path),
            "source.shader" => self.shader_source_mutation_target_info(&path),
            _ => Err(WireSlotMutationRejection::UnknownRoot),
        }
    }

    fn shader_node_mutation_target_info(
        &self,
        path: &str,
    ) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
        let (name, target) = match path {
            "params.exposure" => ("exposure", MutationTarget::ShaderExposureParam),
            "params.speed" => ("speed", MutationTarget::Unsupported),
            _ => return Err(WireSlotMutationRejection::UnknownPath),
        };
        Ok(MutationTargetInfo {
            target,
            shape_version: self.root_shape_version(ShaderNode::SHAPE_ID)?,
            data_version: self
                .shader_node
                .param_changed_frame(name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?,
            ty: self
                .shader_node
                .param_model_type(name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?,
        })
    }

    fn shader_source_mutation_target_info(
        &self,
        path: &str,
    ) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
        match path {
            "param_defs.exposure.label" => Ok(MutationTargetInfo {
                target: MutationTarget::ShaderExposureLabel,
                shape_version: self
                    .root_shape_version(<ShaderDef as StaticSlotAccess>::SHAPE_ID)?,
                data_version: self
                    .shader_def
                    .param_label_changed_frame("exposure")
                    .ok_or(WireSlotMutationRejection::UnknownPath)?,
                ty: ModelType::String,
            }),
            "param_defs.exposure.default" => Ok(MutationTargetInfo {
                target: MutationTarget::Unsupported,
                shape_version: self
                    .root_shape_version(<ShaderDef as StaticSlotAccess>::SHAPE_ID)?,
                data_version: self
                    .shader_def
                    .param_default_changed_frame("exposure")
                    .ok_or(WireSlotMutationRejection::UnknownPath)?,
                ty: ModelType::F32,
            }),
            _ => Err(WireSlotMutationRejection::UnknownPath),
        }
    }

    fn root_shape_version(
        &self,
        shape_id: SlotShapeId,
    ) -> Result<FrameId, WireSlotMutationRejection> {
        self.registry
            .entry(&shape_id)
            .map(|entry| entry.changed_frame)
            .ok_or(WireSlotMutationRejection::UnknownRoot)
    }
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self::new()
    }
}

struct MutationTargetInfo {
    target: MutationTarget,
    shape_version: FrameId,
    data_version: FrameId,
    ty: ModelType,
}

enum MutationTarget {
    ShaderExposureParam,
    ShaderExposureLabel,
    Unsupported,
}

fn model_value_matches_type(value: &ModelValue, ty: &ModelType) -> bool {
    matches!(
        (value, ty),
        (ModelValue::String(_), ModelType::String)
            | (ModelValue::I32(_), ModelType::I32)
            | (ModelValue::U32(_), ModelType::U32)
            | (ModelValue::F32(_), ModelType::F32)
            | (ModelValue::Bool(_), ModelType::Bool)
            | (ModelValue::Vec2(_), ModelType::Vec2)
            | (ModelValue::Vec3(_), ModelType::Vec3)
            | (ModelValue::Vec4(_), ModelType::Vec4)
            | (ModelValue::IVec2(_), ModelType::IVec2)
            | (ModelValue::IVec3(_), ModelType::IVec3)
            | (ModelValue::IVec4(_), ModelType::IVec4)
            | (ModelValue::UVec2(_), ModelType::UVec2)
            | (ModelValue::UVec3(_), ModelType::UVec3)
            | (ModelValue::UVec4(_), ModelType::UVec4)
            | (ModelValue::BVec2(_), ModelType::BVec2)
            | (ModelValue::BVec3(_), ModelType::BVec3)
            | (ModelValue::BVec4(_), ModelType::BVec4)
            | (ModelValue::Mat2x2(_), ModelType::Mat2x2)
            | (ModelValue::Mat3x3(_), ModelType::Mat3x3)
            | (ModelValue::Mat4x4(_), ModelType::Mat4x4)
            | (ModelValue::Resource(_), ModelType::Resource)
    )
}
