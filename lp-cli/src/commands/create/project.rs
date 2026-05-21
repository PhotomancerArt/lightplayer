//! Project creation logic
//!
//! Functions for creating new projects with sensible defaults.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

use lpc_model::nodes::fixture::{ColorOrder, FixtureDef, MappingConfig};
use lpc_model::nodes::output::OutputDef;
use lpc_model::nodes::shader::{ShaderDef, ShaderSlotDef};
use lpc_model::nodes::texture::TextureDef;
use lpc_model::{
    Affine2d, Affine2dSlot, AsLpPath, BindingDef, BindingDefs, BindingRef, BusSlotRef, Dim2u,
    Dim2uSlot, EnumSlot, FixtureSamplingConfig, HardwareEndpointSpec, MapSlot, NodeDef, OptionSlot,
    RenderOrder, RenderOrderSlot, ShaderSource, SlotPath, SlotShapeRegistry, ValueSlot,
};
use lpfs::LpFs;

use crate::messages;

/// Derive project name from directory path
///
/// Extracts the directory name and sanitizes it if needed.
pub fn derive_project_name(dir: &Path) -> String {
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

/// Create project directory structure
///
/// Creates the project directory, project.toml, and default node artifacts.
pub fn create_project_structure(dir: &Path, name: Option<&str>) -> Result<()> {
    // Create directory if doesn't exist
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;

    // Derive name from directory if not provided
    let project_name = if let Some(name) = name {
        name.to_string()
    } else {
        derive_project_name(dir)
    };

    // Create filesystem view for project directory
    let fs = lpfs::LpFsStd::new(dir.to_path_buf());

    // Create default template
    create_default_template(&fs)?;
    write_project_toml(&fs, &project_name)?;

    Ok(())
}

/// Create default project template
///
/// Creates the default project structure with a rainbow rotating color wheel shader.
/// The filesystem should already be chrooted to the project directory.
pub fn create_default_template(fs: &dyn LpFs) -> Result<()> {
    fs.write_file(
        "/clock.toml".as_path(),
        br#"kind = "Clock"
"#,
    )
    .map_err(|e| anyhow::anyhow!("Failed to write clock.toml: {e}"))?;

    // Create texture node
    let texture_config = TextureDef {
        size: Dim2uSlot::new(Dim2u {
            width: 64,
            height: 64,
        }),
        bindings: bus_input_binding_defs("visual.out"),
    };
    let texture_toml = authored_node_toml(&NodeDef::Texture(texture_config))
        .context("Failed to serialize texture def to TOML")?;
    fs.write_file("/texture.toml".as_path(), texture_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write texture.toml: {e}"))?;

    // Create shader node
    let shader_config = ShaderDef {
        source: EnumSlot::new(ShaderSource::path("shader.glsl")),
        render_order: RenderOrderSlot::new(RenderOrder(0)),
        bindings: bus_output_binding_defs("visual.out"),
        glsl_opts: lpc_model::GlslOpts::default(),
        param_defs: lpc_model::MapSlot::default(),
        consumed_slots: default_visual_consumed_slots(),
    };
    let shader_toml = authored_node_toml(&NodeDef::Shader(shader_config))
        .context("Failed to serialize shader def to TOML")?;
    fs.write_file("/shader.toml".as_path(), shader_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write shader.toml: {e}"))?;

    // Create shader GLSL
    fs.write_file(
        "/shader.glsl".as_path(),
        br#"// HSV to RGB conversion function
vec3 hsv_to_rgb(float h, float s, float v) {
    // h in [0, 1], s in [0, 1], v in [0, 1]
    float c = v * s;
    float x = c * (1.0 - abs(mod(h * 6.0, 2.0) - 1.0));
    float m = v - c;
    
    vec3 rgb;
    if (h < 1.0/6.0) {
        rgb = vec3(v, m + x, m);
    } else if (h < 2.0/6.0) {
        rgb = vec3(m + x, v, m);
    } else if (h < 3.0/6.0) {
        rgb = vec3(m, v, m + x);
    } else if (h < 4.0/6.0) {
        rgb = vec3(m, m + x, v);
    } else if (h < 5.0/6.0) {
        rgb = vec3(m + x, m, v);
    } else {
        rgb = vec3(v, m, m + x);
    }
    
    return rgb;
}

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    // Center of texture
    vec2 center = outputSize * 0.5;
    
    // Direction from center to fragment
    vec2 dir = pos - center;
    
    // Calculate angle (atan2 gives angle in [-PI, PI])
    float angle = atan(dir.y, dir.x);
    
    // Rotate angle with time (full rotation every 2 seconds)
    angle = (angle + time * 3.14159);
    
    // Normalize angle to [0, 1] for hue
    // atan returns [-PI, PI], map to [0, 1] by: (angle + PI) / (2 * PI)
    // Wrap hue to [0, 1] using mod to handle large time values
    float hue = mod((angle + 3.14159) / (2.0 * 3.14159), 1.0);
    
    // Distance from center (normalized to [0, 1])
    float maxDist = length(outputSize * 0.5);
    float dist = length(dir) / maxDist;
    
    // Clamp distance to prevent issues
    dist = min(dist, 1.0);
    
    // Value (brightness): highest at center, darker at edges
    float value = 1.0 - dist * 0.5;
    
    // Convert HSV to RGB
    vec3 rgb = hsv_to_rgb(hue, 1.0, value);
    
    // Clamp to [0, 1] and return
    return vec4(max(vec3(0.0), min(vec3(1.0), rgb)), 1.0);
}"#,
    )
    .map_err(|e| anyhow::anyhow!("Failed to write shader.glsl: {e}"))?;

    // Create output node
    let output_config = OutputDef {
        endpoint: ValueSlot::new(HardwareEndpointSpec::from_static("ws281x:rmt:D10")),
        bindings: bus_input_binding_defs("control.out"),
        options: OptionSlot::none(),
    };
    let output_toml = authored_node_toml(&NodeDef::Output(output_config))
        .context("Failed to serialize output def to TOML")?;
    fs.write_file("/output.toml".as_path(), output_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write output.toml: {e}"))?;

    // Create fixture node
    let fixture_config = FixtureDef {
        render_size: Dim2uSlot::new(Dim2u {
            width: 16,
            height: 16,
        }),
        bindings: fixture_binding_defs(),
        sampling: ValueSlot::new(FixtureSamplingConfig::TextureArea),
        mapping: EnumSlot::new(MappingConfig::path_points(MapSlot::default(), 2.0)),
        color_order: ValueSlot::new(ColorOrder::Rgb),
        transform: Affine2dSlot::new(Affine2d::identity()),
        brightness: OptionSlot::none(),
        gamma_correction: OptionSlot::none(),
    };
    let fixture_toml = authored_node_toml(&NodeDef::Fixture(fixture_config))
        .context("Failed to serialize fixture def to TOML")?;
    fs.write_file("/fixture.toml".as_path(), fixture_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write fixture.toml: {e}"))?;

    Ok(())
}

fn write_project_toml(fs: &dyn LpFs, name: &str) -> Result<()> {
    let project_toml = format!(
        r#"kind = "Project"
name = "{name}"

[nodes.output]
def = {{ path = "./output.toml" }}

[nodes.clock]
def = {{ path = "./clock.toml" }}

[nodes.texture]
def = {{ path = "./texture.toml" }}

[nodes.shader]
def = {{ path = "./shader.toml" }}

[nodes.fixture]
def = {{ path = "./fixture.toml" }}
"#
    );
    fs.write_file("/project.toml".as_path(), project_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write project.toml: {e}"))?;
    Ok(())
}

fn authored_node_toml(node: &NodeDef) -> Result<String> {
    node.write_toml(&slot_shape_registry())
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn slot_shape_registry() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn default_visual_consumed_slots() -> MapSlot<String, ShaderSlotDef> {
    let mut slots = BTreeMap::new();
    slots.insert(
        String::from("time"),
        ShaderSlotDef::value_f32("Time", "Project clock time in seconds", 0.0, None),
    );
    MapSlot::new(slots)
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

fn fixture_binding_defs() -> BindingDefs {
    let mut entries = std::collections::BTreeMap::new();
    entries.insert(
        String::from("input"),
        BindingDef::source(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse("visual.out").expect("valid visual bus slot"),
        ))),
    );
    entries.insert(
        String::from("output"),
        BindingDef::target(BindingRef::Bus(BusSlotRef::new(
            SlotPath::parse("control.out").expect("valid control bus slot"),
        ))),
    );
    BindingDefs::new(entries)
}

fn single_binding_defs(slot: &str, binding: BindingDef) -> BindingDefs {
    let mut entries = std::collections::BTreeMap::new();
    entries.insert(String::from(slot), binding);
    BindingDefs::new(entries)
}

/// Print success message with next steps
pub fn print_success_message(_dir: &Path, name: &str) {
    let next_step_cmd =
        messages::format_command(&format!("cd {name} && lp-cli dev ws://localhost:2812/"));

    messages::print_success(
        &format!("Project created successfully: {name}"),
        &[&next_step_cmd],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;
    use tempfile::TempDir;

    #[test]
    fn test_derive_project_name() {
        assert_eq!(
            derive_project_name(Path::new("/path/to/my-project")),
            "my-project"
        );
        // "." has no file_name, so it defaults to "project"
        assert_eq!(derive_project_name(Path::new("../../../..")), "project");
    }

    #[test]
    fn test_create_project_structure_with_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("my-project");

        create_project_structure(&project_dir, None).unwrap();

        assert!(project_dir.join("project.toml").exists());
        let project_toml = std::fs::read_to_string(project_dir.join("project.toml")).unwrap();
        let project_value: toml::Value = toml::from_str(&project_toml).unwrap();
        assert_eq!(project_value["name"].as_str(), Some("my-project"));
        assert!(project_value.get("uid").is_none());
        assert!(project_dir.join("shader.glsl").exists());
    }

    #[test]
    fn test_create_project_structure_with_custom_name() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("custom");

        create_project_structure(&project_dir, Some("Custom Name")).unwrap();

        let project_toml = std::fs::read_to_string(project_dir.join("project.toml")).unwrap();
        let project_value: toml::Value = toml::from_str(&project_toml).unwrap();
        assert_eq!(project_value["name"].as_str(), Some("Custom Name"));
        assert!(project_value.get("uid").is_none());
    }

    #[test]
    fn test_create_default_template() {
        let mut fs = LpFsMemory::new();

        // For memory filesystem, we need to use write_file_mut
        // In production, LpFsStd works with write_file
        create_default_template_mut(&mut fs).unwrap();

        assert!(fs.file_exists("/texture.toml".as_path()).unwrap());
        assert!(fs.file_exists("/shader.toml".as_path()).unwrap());
        assert!(fs.file_exists("/shader.glsl".as_path()).unwrap());
        assert!(fs.file_exists("/output.toml".as_path()).unwrap());
        assert!(fs.file_exists("/fixture.toml".as_path()).unwrap());
    }

    #[test]
    fn test_create_default_template_with_memory_fs() {
        let mut fs = LpFsMemory::new();

        create_default_template_mut(&mut fs).unwrap();

        // Verify texture node content
        let texture_toml = fs.read_file("/texture.toml".as_path()).unwrap();
        let texture_config =
            NodeDef::from_toml_str(std::str::from_utf8(&texture_toml).expect("UTF-8"))
                .expect("texture node TOML");
        let NodeDef::Texture(texture_config) = texture_config else {
            panic!("expected texture node TOML");
        };
        assert_eq!(texture_config.width(), 64);
        assert_eq!(texture_config.height(), 64);
        assert!(matches!(
            texture_config.bindings.entries()["input"].source_ref(),
            Some(BindingRef::Bus(_))
        ));

        // Verify shader node content
        let shader_toml = fs.read_file("/shader.toml".as_path()).unwrap();
        let shader_config =
            NodeDef::from_toml_str(std::str::from_utf8(&shader_toml).expect("UTF-8"))
                .expect("shader node TOML");
        let NodeDef::Shader(shader_config) = shader_config else {
            panic!("expected shader node TOML");
        };
        assert_eq!(
            shader_config.shader_source().path_value().unwrap().as_str(),
            "shader.glsl"
        );
        assert!(matches!(
            shader_config.bindings.entries()["output"].target_ref(),
            Some(BindingRef::Bus(_))
        ));

        // Verify GLSL exists
        let glsl = fs.read_file("/shader.glsl".as_path()).unwrap();
        let glsl_str = std::str::from_utf8(&glsl).unwrap();
        assert!(glsl_str.contains("hsv_to_rgb"));
        assert!(glsl_str.contains("vec4 render"));
    }

    // Helper function for tests that use mutable filesystem
    fn create_default_template_mut(fs: &mut LpFsMemory) -> Result<()> {
        create_default_template(fs)
    }
}
