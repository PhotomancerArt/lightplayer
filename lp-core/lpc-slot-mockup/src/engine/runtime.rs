use lpc_model::{FrameId, SlotAccess, SlotShapeRegistry, set_current_state_version};

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
        Self {
            registry,
            project: ProjectDef::new(),
            shader_node: ShaderNode::from_def(&shader_def),
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

    pub fn remove_shader_param(&mut self, frame: FrameId, name: &str) {
        set_current_state_version(frame);
        self.shader_node.remove_param(name);
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

    pub fn clear_fixture_brightness(&mut self, frame: FrameId) {
        set_current_state_version(frame);
        self.fixture_def.clear_brightness();
    }

    pub fn remove_touch(&mut self, frame: FrameId, id: u32) {
        set_current_state_version(frame);
        self.fixture_node.remove_touch(id);
    }
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self::new()
    }
}
