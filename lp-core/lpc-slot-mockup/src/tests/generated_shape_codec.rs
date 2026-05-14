use std::collections::BTreeMap;

use crate::generated_slot_codec::{
    GeneratedBindingDef, GeneratedBundle, GeneratedEndpoint, GeneratedFixtureDef,
    GeneratedInvocation, GeneratedMapping, GeneratedNodeDef, GeneratedOutputDef,
    GeneratedOutputOptions, GeneratedProject, read_bundle_json, read_bundle_toml,
    read_fixture_def_json, read_fixture_def_toml, read_output_def_json, read_output_def_toml,
    read_project_def_json, read_project_def_toml, read_shader_def_json, read_shader_def_toml,
    read_texture_def_json, read_texture_def_toml, write_bundle_json, write_fixture_def_json,
    write_output_def_json, write_project_def_json, write_shader_def_json, write_texture_def_json,
};
use crate::source::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef};

#[test]
fn generated_shape_codec_json_round_trips_bundle() {
    let bundle = sample_bundle();
    let json = write_bundle_json(&bundle);

    let decoded = read_bundle_json(std::str::from_utf8(&json).unwrap()).unwrap();

    assert_eq!(decoded, bundle);
}

#[test]
fn generated_shape_codec_toml_reads_with_same_reader() {
    let toml: toml::Value = toml::from_str(SAMPLE_BUNDLE_TOML).unwrap();

    let decoded = read_bundle_toml(&toml).unwrap();

    assert_eq!(decoded, sample_bundle());
}

#[test]
fn generated_shape_codec_invalid_discriminator_reports_valid_values() {
    let json = r#"{
        "project": {"kind": "ProjectDef", "nodes": {}},
        "node_defs": [{"kind": "Blark12"}]
    }"#;

    let error = read_bundle_json(json).unwrap_err();

    assert!(error.message().contains("Blark12"));
    assert!(error.message().contains("OutputDef"));
    assert!(error.message().contains("FixtureDef"));
}

#[test]
fn generated_shape_codec_missing_required_field_is_explicit() {
    let json = r#"{
        "project": {"kind": "ProjectDef", "nodes": {}},
        "node_defs": [{"kind": "OutputDef", "bindings": {}}]
    }"#;

    let error = read_bundle_json(json).unwrap_err();

    assert!(error.message().contains("missing required field `pin`"));
}

#[test]
fn generated_shape_codec_unknown_field_reports_valid_fields() {
    let json = r#"{
        "project": {
            "kind": "ProjectDef",
            "name": "basic",
            "surprise": true,
            "nodes": {}
        },
        "node_defs": []
    }"#;

    let error = read_bundle_json(json).unwrap_err();

    assert!(error.message().contains("surprise"));
    assert!(error.message().contains("nodes"));
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

fn sample_bundle() -> GeneratedBundle {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "output".to_string(),
        GeneratedInvocation {
            artifact: "./output.toml".to_string(),
        },
    );
    nodes.insert(
        "fixture".to_string(),
        GeneratedInvocation {
            artifact: "./fixture.toml".to_string(),
        },
    );

    let mut bindings = BTreeMap::new();
    bindings.insert(
        "pixels".to_string(),
        GeneratedBindingDef {
            source: Some(GeneratedEndpoint::Value(0.75)),
            target: Some(GeneratedEndpoint::Ref("bus#visual.out".to_string())),
        },
    );

    GeneratedBundle {
        project: GeneratedProject {
            name: Some("basic".to_string()),
            nodes,
        },
        nodes: vec![
            GeneratedNodeDef::Output(GeneratedOutputDef {
                pin: 18,
                bindings,
                options: Some(GeneratedOutputOptions {
                    white_point: [0.9, 1.0, 1.0],
                    brightness: 0.85,
                }),
            }),
            GeneratedNodeDef::Fixture(GeneratedFixtureDef {
                mapping: GeneratedMapping::Square {
                    origin: [0.1, 0.2],
                    size: [0.8, 0.7],
                },
            }),
            GeneratedNodeDef::Fixture(GeneratedFixtureDef {
                mapping: GeneratedMapping::Disabled,
            }),
        ],
    }
}

fn assert_project_def_matches_default(project: &ProjectDef) {
    assert_eq!(project.kind, ProjectDef::KIND);
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
    assert_eq!(output.kind, OutputDef::KIND);
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
    assert_eq!(texture.kind, TextureDef::KIND);
    assert_eq!(texture.size().width, 64);
    assert_eq!(texture.size().height, 32);
    assert!(texture.bindings().is_empty());
}

fn assert_fixture_def_matches_default(fixture: &FixtureDef) {
    assert_eq!(fixture.kind, FixtureDef::KIND);
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
    assert_eq!(shader.kind, ShaderDef::KIND);
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

const SAMPLE_BUNDLE_TOML: &str = r#"
[project]
kind = "ProjectDef"
name = "basic"

[project.nodes.output]
artifact = "./output.toml"

[project.nodes.fixture]
artifact = "./fixture.toml"

[[node_defs]]
kind = "OutputDef"
pin = 18

[node_defs.bindings.pixels.source]
value = 0.75

[node_defs.bindings.pixels.target]
ref = "bus#visual.out"

[node_defs.options]
white_point = [0.9, 1.0, 1.0]
brightness = 0.85

[[node_defs]]
kind = "FixtureDef"

[node_defs.mapping]
kind = "Square"
origin = [0.1, 0.2]
size = [0.8, 0.7]

[[node_defs]]
kind = "FixtureDef"

[node_defs.mapping]
kind = "Disabled"
"#;

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

[transform]
m00 = 1.0
m01 = 0.0
m10 = 0.0
m11 = 1.0
tx = 0.0
ty = 0.0

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
