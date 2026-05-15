use crate::source::{FixtureDef, NodeDef, OutputDef, ProjectDef, ShaderDef, TextureDef};
use lpc_model::{
    SlotCodec, SlotShapeRegistry,
    slot_codec::{JsonSyntaxSource, SlotReader, SlotWriter, SyntaxError, TomlSyntaxSource},
};

#[test]
fn generated_shape_codec_missing_field_uses_empty_slot_default() {
    let json = r#"{"kind": "output", "bindings": {}}"#;

    let decoded = read_node_def_json(json).unwrap();
    let Some(output) = decoded.as_output() else {
        panic!("expected output node def");
    };

    assert_eq!(output.pin(), 0);
}

#[test]
fn generated_shape_codec_unknown_field_reports_valid_fields() {
    let json = r#"{"kind": "project", "name": "basic", "surprise": true, "nodes": {}}"#;

    let error = expect_error(read_node_def_json(json));

    assert!(error.message().contains("surprise"));
    assert!(error.message().contains("nodes"));
}

#[test]
fn generated_shape_codec_reads_real_node_def_enum_toml() {
    let toml: toml::Value = toml::from_str(OUTPUT_DEF_TOML).unwrap();

    let decoded = read_node_def_toml(&toml).unwrap();

    let Some(output) = decoded.as_output() else {
        panic!("expected output node def");
    };
    assert_output_def_matches_default(output);
}

#[test]
fn generated_shape_codec_json_round_trips_real_node_def_enum() {
    let node = NodeDef::Fixture(FixtureDef::new());
    let json = write_node_def_json(&node);

    let decoded = read_node_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    let Some(fixture) = decoded.as_fixture() else {
        panic!("expected fixture node def");
    };
    assert_fixture_def_matches_default(fixture);
}

#[test]
fn generated_shape_codec_node_def_invalid_kind_reports_valid_values() {
    let error = match read_node_def_json(r#"{"kind":"Blark12"}"#) {
        Ok(_) => panic!("expected invalid kind error"),
        Err(error) => error,
    };

    assert!(error.message().contains("Blark12"));
    assert!(error.message().contains("output"));
    assert!(error.message().contains("fixture"));
    assert!(error.message().contains("shader"));
}

#[test]
fn generated_shape_codec_reads_real_project_def_authored_toml() {
    let toml: toml::Value = toml::from_str(PROJECT_DEF_TOML).unwrap();

    let decoded = read_project_def_toml(&toml).unwrap();

    assert_project_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_json_round_trips_real_project_def() {
    let project = ProjectDef::new();
    let json = write_project_def_json(&project);

    let decoded = read_project_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_project_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_reads_real_output_def_authored_toml() {
    let toml: toml::Value = toml::from_str(OUTPUT_DEF_TOML).unwrap();

    let decoded = read_output_def_toml(&toml).unwrap();

    assert_output_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_json_round_trips_real_output_def() {
    let output = OutputDef::new();
    let json = write_output_def_json(&output);

    let decoded = read_output_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_output_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_output_def_uses_default_option_leaves() {
    let json = r#"{
        "kind": "output",
        "pin": 21,
        "options": {
            "brightness": 0.5
        }
    }"#;

    let decoded = read_output_def_json(json).unwrap();
    let options = decoded.options().unwrap();

    assert_eq!(decoded.pin(), 21);
    assert_eq!(options.brightness(), 0.5);
    assert_eq!(options.lum_power(), 2.0);
    assert_eq!(options.white_point(), [0.9, 1.0, 1.0]);
    assert!(options.interpolation_enabled());
    assert!(options.dithering_enabled());
    assert!(options.lut_enabled());
}

#[test]
fn generated_shape_codec_reads_real_texture_def_authored_toml() {
    let toml: toml::Value = toml::from_str(TEXTURE_DEF_TOML).unwrap();

    let decoded = read_texture_def_toml(&toml).unwrap();

    assert_texture_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_json_round_trips_real_texture_def() {
    let texture = TextureDef::new();
    let json = write_texture_def_json(&texture);

    let decoded = read_texture_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_texture_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_reads_real_fixture_def_authored_toml() {
    let toml: toml::Value = toml::from_str(FIXTURE_DEF_TOML).unwrap();

    let decoded = read_fixture_def_toml(&toml).unwrap();

    assert_fixture_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_json_round_trips_real_fixture_def() {
    let fixture = FixtureDef::new();
    let json = write_fixture_def_json(&fixture);

    let decoded = read_fixture_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_fixture_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_reads_real_shader_def_authored_toml() {
    let toml: toml::Value = toml::from_str(SHADER_DEF_TOML).unwrap();

    let decoded = read_shader_def_toml(&toml).unwrap();

    assert_shader_def_matches_default(&decoded);
}

#[test]
fn generated_shape_codec_json_round_trips_real_shader_def() {
    let shader = ShaderDef::new();
    let json = write_shader_def_json(&shader);

    let decoded = read_shader_def_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_shader_def_matches_default(&decoded);
}

fn read_json<T: SlotCodec>(json: &str) -> Result<T, SyntaxError> {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);
    T::read_slot(reader.value())
}

fn read_toml<T: SlotCodec>(value: &toml::Value) -> Result<T, SyntaxError> {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(TomlSyntaxSource::new(value)?, &registry);
    T::read_slot(reader.value())
}

fn write_json<T: SlotCodec>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = SlotWriter::new(&mut out);
    value.write_slot(writer.value()).unwrap();
    out
}

fn read_node_def_json(json: &str) -> Result<NodeDef, SyntaxError> {
    read_json(json)
}

fn read_node_def_toml(value: &toml::Value) -> Result<NodeDef, SyntaxError> {
    read_toml(value)
}

fn write_node_def_json(node: &NodeDef) -> Vec<u8> {
    write_json(node)
}

fn read_project_def_json(json: &str) -> Result<ProjectDef, SyntaxError> {
    read_json(json)
}

fn read_project_def_toml(value: &toml::Value) -> Result<ProjectDef, SyntaxError> {
    Ok(expect_project(read_node_def_toml(value)?))
}

fn write_project_def_json(project: &ProjectDef) -> Vec<u8> {
    write_json(project)
}

fn read_output_def_json(json: &str) -> Result<OutputDef, SyntaxError> {
    read_json(json).or_else(|_| Ok(expect_output(read_node_def_json(json)?)))
}

fn read_output_def_toml(value: &toml::Value) -> Result<OutputDef, SyntaxError> {
    Ok(expect_output(read_node_def_toml(value)?))
}

fn write_output_def_json(output: &OutputDef) -> Vec<u8> {
    write_json(output)
}

fn read_texture_def_json(json: &str) -> Result<TextureDef, SyntaxError> {
    read_json(json).or_else(|_| Ok(expect_texture(read_node_def_json(json)?)))
}

fn read_texture_def_toml(value: &toml::Value) -> Result<TextureDef, SyntaxError> {
    Ok(expect_texture(read_node_def_toml(value)?))
}

fn write_texture_def_json(texture: &TextureDef) -> Vec<u8> {
    write_json(texture)
}

fn read_fixture_def_json(json: &str) -> Result<FixtureDef, SyntaxError> {
    read_json(json).or_else(|_| Ok(expect_fixture(read_node_def_json(json)?)))
}

fn read_fixture_def_toml(value: &toml::Value) -> Result<FixtureDef, SyntaxError> {
    Ok(expect_fixture(read_node_def_toml(value)?))
}

fn write_fixture_def_json(fixture: &FixtureDef) -> Vec<u8> {
    write_json(fixture)
}

fn read_shader_def_json(json: &str) -> Result<ShaderDef, SyntaxError> {
    read_json(json).or_else(|_| Ok(expect_shader(read_node_def_json(json)?)))
}

fn read_shader_def_toml(value: &toml::Value) -> Result<ShaderDef, SyntaxError> {
    Ok(expect_shader(read_node_def_toml(value)?))
}

fn write_shader_def_json(shader: &ShaderDef) -> Vec<u8> {
    write_json(shader)
}

fn expect_project(node: NodeDef) -> ProjectDef {
    match node {
        NodeDef::Project(def) => def,
        _ => panic!("expected project node definition"),
    }
}

fn expect_output(node: NodeDef) -> OutputDef {
    match node {
        NodeDef::Output(def) => def,
        _ => panic!("expected output node definition"),
    }
}

fn expect_texture(node: NodeDef) -> TextureDef {
    match node {
        NodeDef::Texture(def) => def,
        _ => panic!("expected texture node definition"),
    }
}

fn expect_fixture(node: NodeDef) -> FixtureDef {
    match node {
        NodeDef::Fixture(def) => def,
        _ => panic!("expected fixture node definition"),
    }
}

fn expect_shader(node: NodeDef) -> ShaderDef {
    match node {
        NodeDef::Shader(def) => def,
        _ => panic!("expected shader node definition"),
    }
}

fn expect_error<T>(result: Result<T, SyntaxError>) -> SyntaxError {
    match result {
        Ok(_) => panic!("expected syntax error"),
        Err(error) => error,
    }
}

fn assert_project_def_matches_default(project: &ProjectDef) {
    assert_eq!(
        project.name.data.as_ref().map(|name| name.value().as_str()),
        Some("basic")
    );
    assert_eq!(project.nodes.entries.len(), 4);
    assert_eq!(project.nodes.entries["output"].artifact(), "./output.toml");
    assert_eq!(
        project.nodes.entries["texture"].artifact(),
        "./texture.toml"
    );
    assert_eq!(
        project.nodes.entries["fixture"].artifact(),
        "./fixture.toml"
    );
    assert_eq!(project.nodes.entries["shader"].artifact(), "./shader.toml");
}

fn assert_output_def_matches_default(output: &OutputDef) {
    assert_eq!(output.pin(), 18);
    let options = output.options().unwrap();
    assert_eq!(options.lum_power(), 2.0);
    assert_eq!(options.white_point(), [0.9, 1.0, 1.0]);
    assert_eq!(options.brightness(), 1.0);
    assert!(options.interpolation_enabled());
    assert!(options.dithering_enabled());
    assert!(options.lut_enabled());
}

fn assert_texture_def_matches_default(texture: &TextureDef) {
    assert_eq!(texture.size().width, 64);
    assert_eq!(texture.size().height, 32);
    assert!(texture.bindings().is_empty());
}

fn assert_fixture_def_matches_default(fixture: &FixtureDef) {
    assert_eq!(fixture.render_size().width, 16);
    assert_eq!(fixture.render_size().height, 16);
    assert_eq!(fixture.color_order().as_str(), "grb");
    assert_eq!(fixture.transform().m00, 1.0);
    assert_eq!(fixture.brightness().map(|value| value.value()), Some(0.8));
    assert_eq!(fixture.gamma_correction(), Some(true));
    assert!(fixture.mapping().path_points_fields().is_some());
    let (paths, sample_diameter) = fixture.mapping().path_points_fields().unwrap();
    assert_eq!(sample_diameter, 2.0);
    assert!(paths.entries[&0].ring_array_fields().is_some());
    let (_, diameter, start, end, counts, offset, order) =
        paths.entries[&0].ring_array_fields().unwrap();
    assert_eq!(diameter, 1.0);
    assert_eq!(start, 0);
    assert_eq!(end, 2);
    assert_eq!(*counts.entries[&0].value(), 1);
    assert_eq!(*counts.entries[&1].value(), 96);
    assert_eq!(offset, 0.0);
    assert_eq!(order.as_str(), "inner_first");
}

fn assert_shader_def_matches_default(shader: &ShaderDef) {
    assert_eq!(shader.glsl_path(), "main.glsl");
    assert_eq!(shader.render_order(), 0);
    assert!(shader.bindings().is_empty());
    assert_eq!(shader.glsl_opts().add_sub.value().as_str(), "wrapping");
    assert_eq!(shader.glsl_opts().mul.value().as_str(), "wrapping");
    assert_eq!(shader.glsl_opts().div.value().as_str(), "reciprocal");
    assert_eq!(shader.param_defs.entries.len(), 2);
    let exposure = &shader.param_defs.entries["exposure"];
    assert_eq!(exposure.label(), "Exposure");
    assert_eq!(exposure.description(), "Output exposure multiplier");
    assert_eq!(exposure.value_type(), "f32");
    assert_eq!(exposure.default_scalar(), 1.0);
    assert_eq!(exposure.min().map(|min| min.value()), Some(0.0));
    let speed = &shader.param_defs.entries["speed"];
    assert_eq!(speed.label(), "Speed");
    assert_eq!(speed.default_scalar(), 0.25);
}

const PROJECT_DEF_TOML: &str = r#"
kind = "project"
name = "basic"

[nodes.output]
artifact = "./output.toml"

[nodes.texture]
artifact = "./texture.toml"

[nodes.fixture]
artifact = "./fixture.toml"

[nodes.shader]
artifact = "./shader.toml"
"#;

const OUTPUT_DEF_TOML: &str = r#"
kind = "output"
pin = 18

[options]
lum_power = 2.0
white_point = [0.9, 1.0, 1.0]
brightness = 1.0
interpolation_enabled = true
dithering_enabled = true
lut_enabled = true
"#;

const TEXTURE_DEF_TOML: &str = r#"
kind = "texture"

[size]
width = 64
height = 32
"#;

const FIXTURE_DEF_TOML: &str = r#"
kind = "fixture"
color_order = "grb"
gamma_correction = true

transform = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
]

[render_size]
width = 16
height = 16

[mapping]
kind = "path_points"
sample_diameter = 2.0

[mapping.paths.0]
kind = "ring_array"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 2
offset_angle = 0.0
order = "inner_first"

[mapping.paths.0.ring_lamp_counts]
0 = 1
1 = 96

[brightness]
value = 0.8
"#;

const SHADER_DEF_TOML: &str = r#"
kind = "shader"
glsl_path = "main.glsl"
render_order = 0

[glsl_opts]
add_sub = "wrapping"
mul = "wrapping"
div = "reciprocal"

[param_defs.exposure]
label = "Exposure"
description = "Output exposure multiplier"
value_type = "f32"
default = 1.0

[param_defs.exposure.min]
value = 0.0

[param_defs.speed]
label = "Speed"
description = "Animation speed"
value_type = "f32"
default = 0.25

[param_defs.speed.min]
value = 0.0
"#;
