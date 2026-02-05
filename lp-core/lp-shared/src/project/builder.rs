//! Project builder for creating test projects with a fluent API

use crate::fs::LpFs;
use alloc::{format, rc::Rc, string::String, vec};
use core::cell::RefCell;
use lp_model::nodes::fixture::ColorOrder;
use lp_model::nodes::fixture::{MappingConfig, PathSpec, RingOrder};
use lp_model::nodes::{
    NodeSpecifier, fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig,
    texture::TextureConfig,
};
use lp_model::path::LpPathBuf;
use lp_model::{AsLpPath, AsLpPathBuf};

/// Builder for creating test projects
pub struct ProjectBuilder {
    fs: Rc<RefCell<dyn LpFs>>,
    uid: String,
    name: String,
    texture_id: u32,
    shader_id: u32,
    output_id: u32,
    fixture_id: u32,
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
    texture_path: LpPathBuf,
    glsl_source: String,
    render_order: i32,
}

/// Builder for output nodes
pub struct OutputBuilder {
    pin: u32,
}

/// Builder for fixture nodes
pub struct FixtureBuilder {
    output_path: LpPathBuf,
    texture_path: LpPathBuf,
    mapping: MappingConfig,
    color_order: ColorOrder,
    transform: [[f32; 4]; 4],
}

impl ProjectBuilder {
    /// Create a new ProjectBuilder with default uid and name
    pub fn new(fs: Rc<RefCell<dyn LpFs>>) -> Self {
        Self {
            fs,
            uid: String::from("test"),
            name: String::from("Test Project"),
            texture_id: 1,
            shader_id: 1,
            output_id: 1,
            fixture_id: 1,
        }
    }

    /// Set project UID (defaults to "test")
    pub fn with_uid(mut self, uid: &str) -> Self {
        self.uid = String::from(uid);
        self
    }

    /// Set project name (defaults to "Test Project")
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = String::from(name);
        self
    }

    /// Helper to write files
    fn write_file_helper(&self, path: &str, data: &[u8]) -> Result<(), crate::error::FsError> {
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
            texture_path: texture_path.clone(),
            glsl_source: String::from(
                "vec4 main(vec2 fragCoord, vec2 outputSize, float time) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }",
            ),
            render_order: 0,
        }
    }

    /// Start building an output node (defaults to GPIO pin 0)
    pub fn output(&mut self) -> OutputBuilder {
        OutputBuilder { pin: 0 }
    }

    /// Start building a fixture node
    pub fn fixture(&mut self, output_path: &LpPathBuf, texture_path: &LpPathBuf) -> FixtureBuilder {
        FixtureBuilder {
            output_path: output_path.clone(),
            texture_path: texture_path.clone(),
            mapping: MappingConfig::PathPoints {
                paths: vec![PathSpec::RingArray {
                    center: (0.5, 0.5),
                    diameter: 1.0,
                    start_ring_inclusive: 0,
                    end_ring_exclusive: 1,
                    ring_lamp_counts: vec![1],
                    offset_angle: 0.0,
                    order: RingOrder::InnerFirst,
                }],
                sample_diameter: 2.0,
            },
            color_order: ColorOrder::Rgb,
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Add a texture node with defaults (16x16)
    pub fn texture_basic(&mut self) -> LpPathBuf {
        self.texture().add(self)
    }

    /// Add a shader node with defaults (time-based sawtooth shader)
    pub fn shader_basic(&mut self, texture_path: &LpPathBuf) -> LpPathBuf {
        self.shader(texture_path).add(self)
    }

    /// Add an output node with defaults (GPIO pin 0)
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

    /// Build completes - writes project.json and all node files
    pub fn build(self) {
        // Write project.json
        let project_json = format!(r#"{{"uid": "{}", "name": "{}"}}"#, self.uid, self.name);
        self.write_file_helper("/project.json", project_json.as_bytes())
            .expect("Failed to write project.json");
        // Node files are already written by their respective add() methods
    }
}

impl TextureBuilder {
    /// Add the texture node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.texture_id;
        builder.texture_id += 1;

        let path_str = format!("/src/texture-{id}.texture");
        let node_path = format!("{path_str}/node.json");

        let config = TextureConfig {
            width: self.width,
            height: self.height,
        };

        let json = lp_model::json::to_string(&config).expect("Failed to serialize texture config");

        builder
            .write_file_helper(&node_path, json.as_bytes())
            .expect("Failed to write texture node.json");

        LpPathBuf::from(path_str)
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

        let path_str = format!("/src/shader-{id}.shader");
        let node_path = format!("{path_str}/node.json");
        let glsl_path = format!("{path_str}/main.glsl");

        let config = ShaderConfig {
            glsl_path: "main.glsl".as_path_buf(),
            texture_spec: NodeSpecifier::from(self.texture_path.as_str()),
            render_order: self.render_order,
        };

        let json = lp_model::json::to_string(&config).expect("Failed to serialize shader config");

        builder
            .write_file_helper(&node_path, json.as_bytes())
            .expect("Failed to write shader node.json");

        builder
            .write_file_helper(&glsl_path, self.glsl_source.as_bytes())
            .expect("Failed to write shader GLSL file");

        LpPathBuf::from(path_str)
    }
}

impl OutputBuilder {
    /// Set the GPIO pin
    pub fn gpio_pin(mut self, pin: u32) -> Self {
        self.pin = pin;
        self
    }

    /// Add the output node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.output_id;
        builder.output_id += 1;

        let path_str = format!("/src/output-{id}.output");
        let node_path = format!("{path_str}/node.json");

        let config = OutputConfig::GpioStrip { pin: self.pin };

        let json = lp_model::json::to_string(&config).expect("Failed to serialize output config");

        builder
            .write_file_helper(&node_path, json.as_bytes())
            .expect("Failed to write output node.json");

        LpPathBuf::from(path_str)
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

    /// Add the fixture node to the project
    pub fn add(self, builder: &mut ProjectBuilder) -> LpPathBuf {
        let id = builder.fixture_id;
        builder.fixture_id += 1;

        let path_str = format!("/src/fixture-{id}.fixture");
        let node_path = format!("{path_str}/node.json");

        let config = FixtureConfig {
            output_spec: NodeSpecifier::from(self.output_path.as_str()),
            texture_spec: NodeSpecifier::from(self.texture_path.as_str()),
            mapping: self.mapping,
            color_order: self.color_order,
            transform: self.transform,
            brightness: None,
            gamma_correction: None,
        };

        let json = lp_model::json::to_string(&config).expect("Failed to serialize fixture config");

        builder
            .write_file_helper(&node_path, json.as_bytes())
            .expect("Failed to write fixture node.json");

        LpPathBuf::from(path_str)
    }
}
