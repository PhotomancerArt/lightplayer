//! Project builder for creating artifact-authored test projects with a fluent API.

use alloc::{format, rc::Rc, string::String, vec, vec::Vec};
use core::cell::RefCell;
use lp_collection::VecMap;
use lpc_model::GlslOpts;
use lpc_model::nodes::clock::ClockDef;
use lpc_model::nodes::fixture::{ColorOrder, FixtureDef, MappingConfig, PathSpec, RingOrder};
use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};
use lpc_model::nodes::shader::{ShaderDef, ShaderSlotDef};
use lpc_model::nodes::texture::TextureDef;
use lpc_model::{
    Affine2d, Affine2dSlot, ArtifactSpec, AsLpPath, AssetSlot, BindingDef, BindingDefs, BindingRef,
    BusSlotRef, Dim2u, Dim2uSlot, EnumSlot, FixtureDiagnosticMode, FixtureSamplingConfig,
    HwEndpointSpec, MapSlot, NodeDef, NodeInvocation, NodeInvocationSlot, OptionSlot, ProjectDef,
    Ratio, RatioSlot, RenderOrder, RenderOrderSlot, SlotPath, SlotShapeRegistry, ValueSlot,
};
use lpfs::LpFs;
use lpfs::lp_path::LpPathBuf;

/// Builder for creating test projects
pub struct ProjectBuilder {
    fs: Rc<RefCell<dyn LpFs>>,
    name: String,
    clock_id: u32,
    texture_id: u32,
    shader_id: u32,
    output_id: u32,
    fixture_id: u32,
    nodes: Vec<(String, LpPathBuf)>,
}

/// Builder for texture nodes
pub struct TextureBuilder {
    width: u32,
    height: u32,
}

impl TextureBuilder {
    /// Set texture width
    pub fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    /// Set texture height
    pub fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }
}

/// Builder for shader nodes
pub struct ShaderBuilder {
    _texture_path: LpPathBuf,
    glsl_source: String,
    render_order: i32,
}

/// Builder for output nodes
pub struct OutputBuilder {
    endpoint: HwEndpointSpec,
    options: OutputDriverOptionsConfig,
}

/// Builder for fixture nodes
pub struct FixtureBuilder {
    texture_path: LpPathBuf,
    mapping: MappingConfig,
    color_order: ColorOrder,
    transform: [[f32; 4]; 4],
    brightness: Option<u8>,
    gamma_correction: Option<bool>,
}

impl ProjectBuilder {
    /// Create a new ProjectBuilder with default name
    pub fn new(fs: Rc<RefCell<dyn LpFs>>) -> Self {
        Self {
            fs,
            name: String::from("Test Project"),
            clock_id: 1,
            texture_id: 1,
            shader_id: 1,
            output_id: 1,
            fixture_id: 1,
            nodes: Vec::new(),
        }
    }

    /// Set project name (defaults to "Test Project")
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = String::from(name);
        self
    }

    /// Helper to write files
    fn write_file_helper(&self, path: &str, data: &[u8]) -> Result<(), lpfs::FsError> {
        self.fs.borrow().write_file(path.as_path(), data)
    }

    /// Start building a texture node (defaults to 16x16)
    pub fn texture(&mut self) -> TextureBuilder {
        TextureBuilder {
            width: 16,
            height: 16,
        }
    }

    /// Start building a shader node
    pub fn shader(&mut self, texture_path: &LpPathBuf) -> ShaderBuilder {
        ShaderBuilder {
            _texture_path: texture_path.clone(),
            glsl_source: String::from(
                "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }",
            ),
            render_order: 0,
        }
    }

    /// Start building an output node (defaults to `ws281x:rmt:D10`, no interpolation/dithering/LUT, full brightness)
    pub fn output(&mut self) -> OutputBuilder {
        OutputBuilder {
            endpoint: OutputDef::default_endpoint(),
            options: OutputDriverOptionsConfig {
                white_point: ValueSlot::new([1.0, 1.0, 1.0]),
                brightness: RatioSlot::new(Ratio(1.0)),
                interpolation_enabled: ValueSlot::new(false),
                dithering_enabled: ValueSlot::new(false),
                lut_enabled: ValueSlot::new(false),
            },
        }
    }

    /// Start building a fixture node
    pub fn fixture(
        &mut self,
        _output_path: &LpPathBuf,
        texture_path: &LpPathBuf,
    ) -> FixtureBuilder {
        FixtureBuilder {
            texture_path: texture_path.clone(),
            mapping: default_mapping(),
            color_order: ColorOrder::Rgb,
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            brightness: Some(255),
            gamma_correction: Some(false),
        }
    }

    /// Add a clock node with defaults.
    pub fn clock_basic(&mut self) -> LpPathBuf {
        let id = self.clock_id;
        self.clock_id += 1;

        let node_name = numbered_node_name("clock", id);
        let path = artifact_path_for_node(&node_name);
        let json = authored_node_json(&slot_shape_registry(), &NodeDef::Clock(ClockDef::default()));

        self.write_file_helper(path.as_str(), json.as_bytes())
            .expect("Failed to write clock artifact");
        self.register_node(node_name, path.clone());

        path
    }

    /// Add a texture node with defaults (16x16)
    pub fn texture_basic(&mut self) -> LpPathBuf {
        self.texture().add(self)
    }

    /// Add a shader node with defaults (time-based sawtooth shader)
    pub fn shader_basic(&mut self, texture_path: &LpPathBuf) -> LpPathBuf {
        self.shader(texture_path).add(self)
    }

    /// Add an output node with defaults.
    pub fn output_basic(&mut self) -> LpPathBuf {
        self.output().add(self)
    }

    /// Add a fixture node with defaults
    pub fn fixture_basic(
        &mut self,
        output_path: &LpPathBuf,
        texture_path: &LpPathBuf,
    ) -> LpPathBuf {
        self.fixture(output_path, texture_path).add(self)
    }

    /// Build completes - writes project.json and all node artifact files.
    pub fn build(self) {
        let registry = slot_shape_registry();
        let mut nodes = VecMap::new();
        for (name, path) in &self.nodes {
            let relative_path = path.as_str().trim_start_matches('/');
            nodes.insert(
                name.clone(),
                NodeInvocationSlot::new(NodeInvocation::new(ArtifactSpec::path(format!(
                    "./{relative_path}"
                )))),
            );
        }
        let project = ProjectDef {
            name: OptionSlot::some(ValueSlot::new(self.name.clone())),
            nodes: MapSlot::new(nodes),
        };
        let project_json = authored_node_json(&registry, &NodeDef::Project(project));
        self.write_file_helper("/project.json", project_json.as_bytes())
            .expect("Failed to write project.json");
    }

    fn register_node(&mut self, name: String, path: LpPathBuf) {
        self.nodes.push((name, path));
    }
}

fn artifact_path_for_node(name: &str) -> LpPathBuf {
    LpPathBuf::from(format!("/{name}.json"))
}

fn numbered_node_name(kind: &str, id: u32) -> String {
    if id == 1 {
        String::from(kind)
    } else {
        format!("{kind}_{id}")
    }
}

fn authored_node_json(registry: &SlotShapeRegistry, node: &NodeDef) -> String {
    node.write_json(registry)
        .expect("Failed to serialize authored node JSON")
}

fn slot_shape_registry() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

impl TextureBuilder {
    /// Add the texture node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.texture_id;
        builder.texture_id += 1;

        let node_name = numbered_node_name("texture", id);
        let path = artifact_path_for_node(&node_name);

        let config = TextureDef {
            size: Dim2uSlot::new(Dim2u {
                width: self.width,
                height: self.height,
            }),
            bindings: bus_input_binding_defs("visual.out"),
        };

        let json = authored_node_json(&slot_shape_registry(), &NodeDef::Texture(config));

        builder
            .write_file_helper(path.as_str(), json.as_bytes())
            .expect("Failed to write texture artifact");
        builder.register_node(node_name, path.clone());

        path
    }
}

impl ShaderBuilder {
    /// Set the GLSL source code
    pub fn glsl(mut self, source: &str) -> Self {
        self.glsl_source = String::from(source);
        self
    }

    /// Set the render order
    pub fn render_order(mut self, order: i32) -> Self {
        self.render_order = order;
        self
    }

    /// Add the shader node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.shader_id;
        builder.shader_id += 1;

        let node_name = numbered_node_name("shader", id);
        let path = artifact_path_for_node(&node_name);
        let source_path = format!("/{node_name}.glsl");
        let source_file = format!("{node_name}.glsl");

        let config = ShaderDef {
            source: AssetSlot::path(source_file),
            render_order: RenderOrderSlot::new(RenderOrder(self.render_order)),
            bindings: bus_output_binding_defs("visual.out"),
            glsl_opts: GlslOpts::default(),
            param_defs: MapSlot::default(),
            consumed_slots: default_visual_consumed_slots(),
        };

        let json = authored_node_json(&slot_shape_registry(), &NodeDef::Shader(config));

        builder
            .write_file_helper(path.as_str(), json.as_bytes())
            .expect("Failed to write shader artifact");

        builder
            .write_file_helper(&source_path, self.glsl_source.as_bytes())
            .expect("Failed to write shader GLSL file");
        builder.register_node(node_name, path.clone());

        path
    }
}

impl OutputBuilder {
    /// Set the hardware endpoint spec.
    pub fn endpoint(mut self, endpoint: HwEndpointSpec) -> Self {
        self.endpoint = endpoint;
        self
    }

    /// Set the hardware endpoint spec from text.
    pub fn endpoint_str(mut self, endpoint: &str) -> Self {
        self.endpoint = HwEndpointSpec::parse(endpoint).expect("valid output endpoint spec");
        self
    }

    /// Add the output node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.output_id;
        builder.output_id += 1;

        let node_name = numbered_node_name("output", id);
        let path = artifact_path_for_node(&node_name);

        let config = OutputDef {
            endpoint: ValueSlot::new(self.endpoint),
            bindings: bus_input_binding_defs("control.out"),
            options: OptionSlot::some(self.options),
        };

        let json = authored_node_json(&slot_shape_registry(), &NodeDef::Output(config));

        builder
            .write_file_helper(path.as_str(), json.as_bytes())
            .expect("Failed to write output artifact");
        builder.register_node(node_name, path.clone());

        path
    }
}

impl FixtureBuilder {
    /// Set the mapping configuration
    pub fn mapping(mut self, mapping: MappingConfig) -> Self {
        self.mapping = mapping;
        self
    }

    /// Set the color order
    pub fn color_order(mut self, order: ColorOrder) -> Self {
        self.color_order = order;
        self
    }

    /// Set the transform matrix
    pub fn transform(mut self, transform: [[f32; 4]; 4]) -> Self {
        self.transform = transform;
        self
    }

    /// Set the brightness level (0-255)
    pub fn brightness(mut self, brightness: u8) -> Self {
        self.brightness = Some(brightness);
        self
    }

    /// Set gamma correction (defaults to false)
    pub fn gamma_correction(mut self, enabled: bool) -> Self {
        self.gamma_correction = Some(enabled);
        self
    }

    /// Add the fixture node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.fixture_id;
        builder.fixture_id += 1;

        let node_name = numbered_node_name("fixture", id);
        let path = artifact_path_for_node(&node_name);
        let _texture_path = self.texture_path;

        let config = FixtureDef {
            render_size: lpc_model::Dim2uSlot::new(lpc_model::Dim2u {
                width: 16,
                height: 16,
            }),
            bindings: fixture_binding_defs(),
            sampling: ValueSlot::new(FixtureSamplingConfig::TextureArea),
            diagnostic_mode: ValueSlot::new(FixtureDiagnosticMode::Off),
            mapping: EnumSlot::new(self.mapping),
            color_order: ValueSlot::new(self.color_order),
            transform: Affine2dSlot::new(affine2d_from_matrix(self.transform)),
            brightness: self.brightness.map_or_else(OptionSlot::none, |brightness| {
                OptionSlot::some(ValueSlot::new(u32::from(brightness)))
            }),
            gamma_correction: self
                .gamma_correction
                .map_or_else(OptionSlot::none, |enabled| {
                    OptionSlot::some(ValueSlot::new(enabled))
                }),
        };

        let json = authored_node_json(&slot_shape_registry(), &NodeDef::Fixture(config));

        builder
            .write_file_helper(path.as_str(), json.as_bytes())
            .expect("Failed to write fixture artifact");
        builder.register_node(node_name, path.clone());

        path
    }
}

fn bus_input_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs(
        "input",
        BindingDef::source(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse(slot).expect("valid bus slot path"),
        ))),
    )
}

fn bus_output_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs(
        "output",
        BindingDef::target(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse(slot).expect("valid bus slot path"),
        ))),
    )
}

fn default_visual_consumed_slots() -> MapSlot<String, ShaderSlotDef> {
    let mut slots = VecMap::new();
    slots.insert(
        String::from("time"),
        ShaderSlotDef::value_f32("Time", "Project clock time in seconds", 0.0, None),
    );
    MapSlot::new(slots)
}

fn fixture_binding_defs() -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(
        String::from("input"),
        BindingDef::source(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse("visual.out").expect("valid bus slot path"),
        ))),
    );
    entries.insert(
        String::from("output"),
        BindingDef::target(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse("control.out").expect("valid bus slot path"),
        ))),
    );
    BindingDefs::new(entries)
}

fn single_binding_defs(slot: &str, binding: BindingDef) -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(String::from(slot), binding);
    BindingDefs::new(entries)
}

fn default_mapping() -> MappingConfig {
    let mut ring_lamp_counts = VecMap::new();
    ring_lamp_counts.insert(0, ValueSlot::new(1));

    MappingConfig::path_points_vec(
        vec![PathSpec::ring_array(
            [0.5, 0.5],
            1.0,
            0,
            1,
            MapSlot::new(ring_lamp_counts),
            0.0,
            RingOrder::InnerFirst,
        )],
        2.0,
    )
}

fn affine2d_from_matrix(matrix: [[f32; 4]; 4]) -> Affine2d {
    Affine2d {
        m00: matrix[0][0],
        m01: matrix[0][1],
        m10: matrix[1][0],
        m11: matrix[1][1],
        tx: matrix[0][3],
        ty: matrix[1][3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::NodeDef;
    use lpfs::LpFsMemory;

    #[test]
    fn test_project_builder_creates_valid_project_json() {
        let fs = Rc::new(RefCell::new(LpFsMemory::new()));
        let mut builder = ProjectBuilder::new(fs.clone());
        builder.texture_basic();
        builder.build();

        let project_json_bytes = fs.borrow().read_file("/project.json".as_path()).unwrap();
        let project_json_str = core::str::from_utf8(&project_json_bytes).unwrap();

        let def = NodeDef::read_json(&slot_shape_registry(), project_json_str).unwrap();
        let NodeDef::Project(def) = def else {
            panic!("expected project def");
        };
        assert_eq!(def.name(), Some("Test Project"));
        assert!(def.nodes.entries.contains_key("texture"));
    }
}
