use std::collections::BTreeMap;

use lpc_model::SlotShapeRegistry;
use lpc_model::slot_codec::{
    JsonSyntaxSource, ObjectReader, SlotJsonValue, SlotJsonWrite, SlotJsonWriter, SlotReader,
    SyntaxError, SyntaxEventSource, TomlSyntaxSource, ValueReader,
};

#[test]
fn manual_shape_codec_fixture_covers_core_shapes() {
    let bundle = sample_bundle();

    assert_eq!(bundle.project.nodes.len(), 4);
    assert!(bundle.project.name.is_some());
    assert!(matches!(bundle.nodes[0], ManualNodeDef::Output(_)));
    assert!(matches!(bundle.nodes[1], ManualNodeDef::Texture(_)));
    assert!(matches!(bundle.nodes[2], ManualNodeDef::Shader(_)));
    assert!(matches!(bundle.nodes[3], ManualNodeDef::Fixture(_)));

    let ManualNodeDef::Fixture(fixture) = &bundle.nodes[3] else {
        panic!("fixture node");
    };
    assert!(matches!(fixture.mapping, MappingConfig::PathPoints { .. }));
    assert!(fixture.disabled_mapping_probe.is_some());
    assert!(fixture.brightness.is_none());

    let ManualNodeDef::Shader(shader) = &bundle.nodes[2] else {
        panic!("shader node");
    };
    assert!(!shader.param_defs.is_empty());
    assert!(shader.param_defs["speed"].min.is_none());

    let ManualNodeDef::Output(output) = &bundle.nodes[0] else {
        panic!("output node");
    };
    assert!(output.options.is_some());
    assert!(!output.bindings.is_empty());
}

#[test]
fn manual_shape_codec_json_round_trips_source_bundle() {
    let bundle = sample_bundle();
    let json = write_bundle_json(&bundle);

    let decoded = read_bundle_json(core::str::from_utf8(&json).unwrap()).unwrap();

    assert_eq!(decoded, bundle);
}

#[test]
fn manual_shape_codec_toml_reads_with_same_manual_reader() {
    let toml: toml::Value = toml::from_str(SAMPLE_BUNDLE_TOML).unwrap();
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(TomlSyntaxSource::new(&toml).unwrap(), &registry);

    let decoded = read_bundle(&mut reader).unwrap();

    assert_eq!(decoded, sample_bundle());
}

#[test]
fn manual_shape_codec_unknown_field_reports_valid_fields() {
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
fn manual_shape_codec_invalid_discriminator_reports_valid_values() {
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
fn manual_shape_codec_missing_required_field_is_explicit() {
    let json = r#"{
        "project": {"kind": "ProjectDef", "nodes": {}},
        "node_defs": [{"kind": "OutputDef", "bindings": {}}]
    }"#;

    let error = read_bundle_json(json).unwrap_err();

    assert!(error.message().contains("missing required field `pin`"));
}

#[derive(Clone, Debug, PartialEq)]
struct ManualSourceBundle {
    project: ProjectDefLike,
    nodes: Vec<ManualNodeDef>,
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectDefLike {
    name: Option<String>,
    nodes: BTreeMap<String, NodeInvocationDefLike>,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeInvocationDefLike {
    artifact: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ManualNodeDef {
    Output(OutputDefLike),
    Texture(TextureDefLike),
    Shader(ShaderDefLike),
    Fixture(FixtureDefLike),
}

#[derive(Clone, Debug, PartialEq)]
struct OutputDefLike {
    pin: u32,
    bindings: BTreeMap<String, BindingDefLike>,
    options: Option<OutputOptionsLike>,
}

#[derive(Clone, Debug, PartialEq)]
struct OutputOptionsLike {
    lum_power: f32,
    white_point: [f32; 3],
    brightness: f32,
    interpolation_enabled: bool,
    dithering_enabled: bool,
    lut_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TextureDefLike {
    size: Dim2uLike,
    bindings: BTreeMap<String, BindingDefLike>,
}

#[derive(Clone, Debug, PartialEq)]
struct ShaderDefLike {
    glsl_path: String,
    render_order: u32,
    bindings: BTreeMap<String, BindingDefLike>,
    glsl_opts: GlslOptsLike,
    param_defs: BTreeMap<String, ShaderParamDefLike>,
}

#[derive(Clone, Debug, PartialEq)]
struct GlslOptsLike {
    add_sub: String,
    mul: String,
    div: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ShaderParamDefLike {
    label: String,
    description: String,
    value_type: String,
    default: f32,
    min: Option<ScalarHintLike>,
}

#[derive(Clone, Debug, PartialEq)]
struct ScalarHintLike {
    value: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct FixtureDefLike {
    render_size: Dim2uLike,
    bindings: BTreeMap<String, BindingDefLike>,
    mapping: MappingConfig,
    disabled_mapping_probe: Option<MappingConfig>,
    color_order: String,
    transform: Affine2dLike,
    brightness: Option<ScalarHintLike>,
    gamma_correction: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
enum MappingConfig {
    Disabled,
    Square {
        origin: [f32; 2],
        size: [f32; 2],
    },
    PathPoints {
        paths: BTreeMap<u32, PathSpec>,
        sample_diameter: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum PathSpec {
    RingArray {
        center: [f32; 2],
        diameter: f32,
        start_ring_inclusive: u32,
        end_ring_exclusive: u32,
        ring_lamp_counts: BTreeMap<u32, u32>,
        offset_angle: f32,
        order: String,
    },
    Manual,
}

#[derive(Clone, Debug, PartialEq)]
struct Dim2uLike {
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct Affine2dLike {
    matrix: [f32; 6],
}

#[derive(Clone, Debug, PartialEq)]
struct BindingDefLike {
    source: Option<BindingEndpointLike>,
    target: Option<BindingEndpointLike>,
}

#[derive(Clone, Debug, PartialEq)]
enum BindingEndpointLike {
    Ref(String),
    Value(LpValueLike),
}

#[derive(Clone, Debug, PartialEq)]
enum LpValueLike {
    F32(f32),
}

const SAMPLE_BUNDLE_TOML: &str = r#"
[project]
kind = "ProjectDef"
name = "basic"

[project.nodes.output]
artifact = "./output.toml"

[project.nodes.texture]
artifact = "./texture.toml"

[project.nodes.shader]
artifact = "./shader.toml"

[project.nodes.fixture]
artifact = "./fixture.toml"

[[node_defs]]
kind = "OutputDef"
pin = 18

[node_defs.bindings.pixels]
target = { ref = "bus#visual.out" }

[node_defs.options]
lum_power = 2.0
white_point = [0.9, 1.0, 1.0]
brightness = 0.85
interpolation_enabled = true
dithering_enabled = true
lut_enabled = false

[[node_defs]]
kind = "TextureDef"

[node_defs.size]
width = 64
height = 32

[node_defs.bindings.input]
source = { value = 0.25 }

[[node_defs]]
kind = "ShaderDef"
glsl_path = "main.glsl"
render_order = 0

[node_defs.bindings.output]
target = { ref = "..fixture#mapping" }

[node_defs.glsl_opts]
add_sub = "Wrapping"
mul = "Wrapping"
div = "Reciprocal"

[node_defs.param_defs.exposure]
label = "Exposure"
description = "Output exposure multiplier"
value_type = "f32"
default = 1.0

[node_defs.param_defs.exposure.min]
value = 0.0

[node_defs.param_defs.speed]
label = "Speed"
description = "Animation speed"
value_type = "f32"
default = 0.25

[[node_defs]]
kind = "FixtureDef"
color_order = "Grb"
gamma_correction = true

[node_defs.render_size]
width = 16
height = 16

[node_defs.bindings.texture]
source = { ref = "..texture#output" }

[node_defs.mapping]
kind = "PathPoints"
sample_diameter = 2.0

[node_defs.mapping.paths.0]
kind = "RingArray"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 2
offset_angle = 0.0
order = "InnerFirst"

[node_defs.mapping.paths.0.ring_lamp_counts]
0 = 1
1 = 96

[node_defs.disabled_mapping_probe]
kind = "Disabled"

[node_defs.transform]
matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
"#;

fn sample_bundle() -> ManualSourceBundle {
    let mut project_nodes = BTreeMap::new();
    project_nodes.insert(
        "output".to_string(),
        NodeInvocationDefLike::new("./output.toml"),
    );
    project_nodes.insert(
        "texture".to_string(),
        NodeInvocationDefLike::new("./texture.toml"),
    );
    project_nodes.insert(
        "shader".to_string(),
        NodeInvocationDefLike::new("./shader.toml"),
    );
    project_nodes.insert(
        "fixture".to_string(),
        NodeInvocationDefLike::new("./fixture.toml"),
    );

    let mut output_bindings = BTreeMap::new();
    output_bindings.insert(
        "pixels".to_string(),
        BindingDefLike::target(BindingEndpointLike::Ref("bus#visual.out".to_string())),
    );

    let mut texture_bindings = BTreeMap::new();
    texture_bindings.insert(
        "input".to_string(),
        BindingDefLike::source(BindingEndpointLike::Value(LpValueLike::F32(0.25))),
    );

    let mut shader_bindings = BTreeMap::new();
    shader_bindings.insert(
        "output".to_string(),
        BindingDefLike::target(BindingEndpointLike::Ref("..fixture#mapping".to_string())),
    );

    let mut param_defs = BTreeMap::new();
    param_defs.insert(
        "exposure".to_string(),
        ShaderParamDefLike {
            label: "Exposure".to_string(),
            description: "Output exposure multiplier".to_string(),
            value_type: "f32".to_string(),
            default: 1.0,
            min: Some(ScalarHintLike { value: 0.0 }),
        },
    );
    param_defs.insert(
        "speed".to_string(),
        ShaderParamDefLike {
            label: "Speed".to_string(),
            description: "Animation speed".to_string(),
            value_type: "f32".to_string(),
            default: 0.25,
            min: None,
        },
    );

    let mut fixture_bindings = BTreeMap::new();
    fixture_bindings.insert(
        "texture".to_string(),
        BindingDefLike::source(BindingEndpointLike::Ref("..texture#output".to_string())),
    );

    let mut ring_lamp_counts = BTreeMap::new();
    ring_lamp_counts.insert(0, 1);
    ring_lamp_counts.insert(1, 96);
    let mut paths = BTreeMap::new();
    paths.insert(
        0,
        PathSpec::RingArray {
            center: [0.5, 0.5],
            diameter: 1.0,
            start_ring_inclusive: 0,
            end_ring_exclusive: 2,
            ring_lamp_counts,
            offset_angle: 0.0,
            order: "InnerFirst".to_string(),
        },
    );

    ManualSourceBundle {
        project: ProjectDefLike {
            name: Some("basic".to_string()),
            nodes: project_nodes,
        },
        nodes: vec![
            ManualNodeDef::Output(OutputDefLike {
                pin: 18,
                bindings: output_bindings,
                options: Some(OutputOptionsLike {
                    lum_power: 2.0,
                    white_point: [0.9, 1.0, 1.0],
                    brightness: 0.85,
                    interpolation_enabled: true,
                    dithering_enabled: true,
                    lut_enabled: false,
                }),
            }),
            ManualNodeDef::Texture(TextureDefLike {
                size: Dim2uLike {
                    width: 64,
                    height: 32,
                },
                bindings: texture_bindings,
            }),
            ManualNodeDef::Shader(ShaderDefLike {
                glsl_path: "main.glsl".to_string(),
                render_order: 0,
                bindings: shader_bindings,
                glsl_opts: GlslOptsLike {
                    add_sub: "Wrapping".to_string(),
                    mul: "Wrapping".to_string(),
                    div: "Reciprocal".to_string(),
                },
                param_defs,
            }),
            ManualNodeDef::Fixture(FixtureDefLike {
                render_size: Dim2uLike {
                    width: 16,
                    height: 16,
                },
                bindings: fixture_bindings,
                mapping: MappingConfig::PathPoints {
                    paths,
                    sample_diameter: 2.0,
                },
                disabled_mapping_probe: Some(MappingConfig::Disabled),
                color_order: "Grb".to_string(),
                transform: Affine2dLike {
                    matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                },
                brightness: None,
                gamma_correction: Some(true),
            }),
        ],
    }
}

fn read_bundle_json(json: &str) -> Result<ManualSourceBundle, SyntaxError> {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);
    read_bundle(&mut reader)
}

fn read_bundle<S>(reader: &mut SlotReader<'_, S>) -> Result<ManualSourceBundle, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["project", "node_defs"];
    let mut project = None;
    let mut nodes = None;

    let mut object = reader.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "project" => project = Some(read_project_def(prop.value())?),
            "node_defs" => nodes = Some(read_node_defs(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(ManualSourceBundle {
        project: project.ok_or_else(|| object.missing_required_field("project"))?,
        nodes: nodes.ok_or_else(|| object.missing_required_field("node_defs"))?,
    })
}

fn read_project_def<S>(value: ValueReader<'_, '_, S>) -> Result<ProjectDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "name", "nodes"];
    let mut object = value.object()?;
    let _kind = object.expect_discriminator("kind", &["ProjectDef"])?;
    let mut name = None;
    let mut nodes = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "name" => name = Some(prop.value().string()?),
            "nodes" => nodes = Some(prop.value().string_key_map(read_node_invocation)?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(ProjectDefLike {
        name,
        nodes: nodes.ok_or_else(|| object.missing_required_field("nodes"))?,
    })
}

fn read_node_defs<S>(value: ValueReader<'_, '_, S>) -> Result<Vec<ManualNodeDef>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut nodes = Vec::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        nodes.push(read_node_def(item)?);
    }
    Ok(nodes)
}

fn read_node_def<S>(value: ValueReader<'_, '_, S>) -> Result<ManualNodeDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator(
        "kind",
        &["OutputDef", "TextureDef", "ShaderDef", "FixtureDef"],
    )?;
    match kind.as_str() {
        "OutputDef" => read_output_def_body(object).map(ManualNodeDef::Output),
        "TextureDef" => read_texture_def_body(object).map(ManualNodeDef::Texture),
        "ShaderDef" => read_shader_def_body(object).map(ManualNodeDef::Shader),
        "FixtureDef" => read_fixture_def_body(object).map(ManualNodeDef::Fixture),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_output_def_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<OutputDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "pin", "bindings", "options"];
    let mut pin = None;
    let mut bindings = BTreeMap::new();
    let mut options = None;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "pin" => pin = Some(prop.value().u32()?),
            "bindings" => bindings = prop.value().string_key_map(read_binding_def)?,
            "options" => options = Some(read_output_options(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(OutputDefLike {
        pin: pin.ok_or_else(|| object.missing_required_field("pin"))?,
        bindings,
        options,
    })
}

fn read_texture_def_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<TextureDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "size", "bindings"];
    let mut size = None;
    let mut bindings = BTreeMap::new();

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "size" => size = Some(read_dim2u(prop.value())?),
            "bindings" => bindings = prop.value().string_key_map(read_binding_def)?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(TextureDefLike {
        size: size.ok_or_else(|| object.missing_required_field("size"))?,
        bindings,
    })
}

fn read_shader_def_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<ShaderDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "kind",
        "glsl_path",
        "render_order",
        "bindings",
        "glsl_opts",
        "param_defs",
    ];
    let mut glsl_path = None;
    let mut render_order = None;
    let mut bindings = BTreeMap::new();
    let mut glsl_opts = None;
    let mut param_defs = BTreeMap::new();

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "glsl_path" => glsl_path = Some(prop.value().string()?),
            "render_order" => render_order = Some(prop.value().u32()?),
            "bindings" => bindings = prop.value().string_key_map(read_binding_def)?,
            "glsl_opts" => glsl_opts = Some(read_glsl_opts(prop.value())?),
            "param_defs" => param_defs = prop.value().string_key_map(read_shader_param_def)?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(ShaderDefLike {
        glsl_path: glsl_path.ok_or_else(|| object.missing_required_field("glsl_path"))?,
        render_order: render_order.ok_or_else(|| object.missing_required_field("render_order"))?,
        bindings,
        glsl_opts: glsl_opts.ok_or_else(|| object.missing_required_field("glsl_opts"))?,
        param_defs,
    })
}

fn read_fixture_def_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<FixtureDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "kind",
        "render_size",
        "bindings",
        "mapping",
        "disabled_mapping_probe",
        "color_order",
        "transform",
        "brightness",
        "gamma_correction",
    ];
    let mut render_size = None;
    let mut bindings = BTreeMap::new();
    let mut mapping = None;
    let mut disabled_mapping_probe = None;
    let mut color_order = None;
    let mut transform = None;
    let mut brightness = None;
    let mut gamma_correction = None;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "render_size" => render_size = Some(read_dim2u(prop.value())?),
            "bindings" => bindings = prop.value().string_key_map(read_binding_def)?,
            "mapping" => mapping = Some(read_mapping_config(prop.value())?),
            "disabled_mapping_probe" => {
                disabled_mapping_probe = Some(read_mapping_config(prop.value())?)
            }
            "color_order" => color_order = Some(prop.value().string()?),
            "transform" => transform = Some(read_affine2d(prop.value())?),
            "brightness" => brightness = Some(read_scalar_hint(prop.value())?),
            "gamma_correction" => gamma_correction = Some(prop.value().bool()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(FixtureDefLike {
        render_size: render_size.ok_or_else(|| object.missing_required_field("render_size"))?,
        bindings,
        mapping: mapping.ok_or_else(|| object.missing_required_field("mapping"))?,
        disabled_mapping_probe,
        color_order: color_order.ok_or_else(|| object.missing_required_field("color_order"))?,
        transform: transform.ok_or_else(|| object.missing_required_field("transform"))?,
        brightness,
        gamma_correction,
    })
}

fn read_node_invocation<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<NodeInvocationDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["artifact"];
    let mut artifact = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "artifact" => artifact = Some(prop.value().string()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(NodeInvocationDefLike {
        artifact: artifact.ok_or_else(|| object.missing_required_field("artifact"))?,
    })
}

fn read_output_options<S>(value: ValueReader<'_, '_, S>) -> Result<OutputOptionsLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "lum_power",
        "white_point",
        "brightness",
        "interpolation_enabled",
        "dithering_enabled",
        "lut_enabled",
    ];
    let mut lum_power = None;
    let mut white_point = None;
    let mut brightness = None;
    let mut interpolation_enabled = None;
    let mut dithering_enabled = None;
    let mut lut_enabled = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "lum_power" => lum_power = Some(prop.value().f32()?),
            "white_point" => white_point = Some(prop.value().f32_array()?),
            "brightness" => brightness = Some(prop.value().f32()?),
            "interpolation_enabled" => interpolation_enabled = Some(prop.value().bool()?),
            "dithering_enabled" => dithering_enabled = Some(prop.value().bool()?),
            "lut_enabled" => lut_enabled = Some(prop.value().bool()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(OutputOptionsLike {
        lum_power: lum_power.ok_or_else(|| object.missing_required_field("lum_power"))?,
        white_point: white_point.ok_or_else(|| object.missing_required_field("white_point"))?,
        brightness: brightness.ok_or_else(|| object.missing_required_field("brightness"))?,
        interpolation_enabled: interpolation_enabled
            .ok_or_else(|| object.missing_required_field("interpolation_enabled"))?,
        dithering_enabled: dithering_enabled
            .ok_or_else(|| object.missing_required_field("dithering_enabled"))?,
        lut_enabled: lut_enabled.ok_or_else(|| object.missing_required_field("lut_enabled"))?,
    })
}

fn read_glsl_opts<S>(value: ValueReader<'_, '_, S>) -> Result<GlslOptsLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["add_sub", "mul", "div"];
    let mut add_sub = None;
    let mut mul = None;
    let mut div = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "add_sub" => add_sub = Some(prop.value().string()?),
            "mul" => mul = Some(prop.value().string()?),
            "div" => div = Some(prop.value().string()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GlslOptsLike {
        add_sub: add_sub.ok_or_else(|| object.missing_required_field("add_sub"))?,
        mul: mul.ok_or_else(|| object.missing_required_field("mul"))?,
        div: div.ok_or_else(|| object.missing_required_field("div"))?,
    })
}

fn read_shader_param_def<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<ShaderParamDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["label", "description", "value_type", "default", "min"];
    let mut label = None;
    let mut description = None;
    let mut value_type = None;
    let mut default = None;
    let mut min = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "label" => label = Some(prop.value().string()?),
            "description" => description = Some(prop.value().string()?),
            "value_type" => value_type = Some(prop.value().string()?),
            "default" => default = Some(prop.value().f32()?),
            "min" => min = Some(read_scalar_hint(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(ShaderParamDefLike {
        label: label.ok_or_else(|| object.missing_required_field("label"))?,
        description: description.ok_or_else(|| object.missing_required_field("description"))?,
        value_type: value_type.ok_or_else(|| object.missing_required_field("value_type"))?,
        default: default.ok_or_else(|| object.missing_required_field("default"))?,
        min,
    })
}

fn read_scalar_hint<S>(value: ValueReader<'_, '_, S>) -> Result<ScalarHintLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["value"];
    let mut value_field = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "value" => value_field = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(ScalarHintLike {
        value: value_field.ok_or_else(|| object.missing_required_field("value"))?,
    })
}

fn read_mapping_config<S>(value: ValueReader<'_, '_, S>) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["Disabled", "Square", "PathPoints"])?;
    match kind.as_str() {
        "Disabled" => {
            object.finish()?;
            Ok(MappingConfig::Disabled)
        }
        "Square" => read_square_mapping_body(object),
        "PathPoints" => read_path_points_mapping_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_square_mapping_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "origin", "size"];
    let mut origin = None;
    let mut size = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "origin" => origin = Some(prop.value().f32_array()?),
            "size" => size = Some(prop.value().f32_array()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::Square {
        origin: origin.ok_or_else(|| object.missing_required_field("origin"))?,
        size: size.ok_or_else(|| object.missing_required_field("size"))?,
    })
}

fn read_path_points_mapping_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "paths", "sample_diameter"];
    let mut paths = None;
    let mut sample_diameter = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "paths" => paths = Some(prop.value().u32_key_map(read_path_spec)?),
            "sample_diameter" => sample_diameter = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::PathPoints {
        paths: paths.ok_or_else(|| object.missing_required_field("paths"))?,
        sample_diameter: sample_diameter
            .ok_or_else(|| object.missing_required_field("sample_diameter"))?,
    })
}

fn read_path_spec<S>(value: ValueReader<'_, '_, S>) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["RingArray", "Manual"])?;
    match kind.as_str() {
        "RingArray" => read_ring_array_path_body(object),
        "Manual" => {
            object.finish()?;
            Ok(PathSpec::Manual)
        }
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_ring_array_path_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "kind",
        "center",
        "diameter",
        "start_ring_inclusive",
        "end_ring_exclusive",
        "ring_lamp_counts",
        "offset_angle",
        "order",
    ];
    let mut center = None;
    let mut diameter = None;
    let mut start_ring_inclusive = None;
    let mut end_ring_exclusive = None;
    let mut ring_lamp_counts = None;
    let mut offset_angle = None;
    let mut order = None;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "center" => center = Some(prop.value().f32_array()?),
            "diameter" => diameter = Some(prop.value().f32()?),
            "start_ring_inclusive" => start_ring_inclusive = Some(prop.value().u32()?),
            "end_ring_exclusive" => end_ring_exclusive = Some(prop.value().u32()?),
            "ring_lamp_counts" => {
                ring_lamp_counts = Some(prop.value().u32_key_map(|value| value.u32())?)
            }
            "offset_angle" => offset_angle = Some(prop.value().f32()?),
            "order" => order = Some(prop.value().string()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(PathSpec::RingArray {
        center: center.ok_or_else(|| object.missing_required_field("center"))?,
        diameter: diameter.ok_or_else(|| object.missing_required_field("diameter"))?,
        start_ring_inclusive: start_ring_inclusive
            .ok_or_else(|| object.missing_required_field("start_ring_inclusive"))?,
        end_ring_exclusive: end_ring_exclusive
            .ok_or_else(|| object.missing_required_field("end_ring_exclusive"))?,
        ring_lamp_counts: ring_lamp_counts
            .ok_or_else(|| object.missing_required_field("ring_lamp_counts"))?,
        offset_angle: offset_angle.ok_or_else(|| object.missing_required_field("offset_angle"))?,
        order: order.ok_or_else(|| object.missing_required_field("order"))?,
    })
}

fn read_dim2u<S>(value: ValueReader<'_, '_, S>) -> Result<Dim2uLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["width", "height"];
    let mut width = None;
    let mut height = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "width" => width = Some(prop.value().u32()?),
            "height" => height = Some(prop.value().u32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(Dim2uLike {
        width: width.ok_or_else(|| object.missing_required_field("width"))?,
        height: height.ok_or_else(|| object.missing_required_field("height"))?,
    })
}

fn read_affine2d<S>(value: ValueReader<'_, '_, S>) -> Result<Affine2dLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["matrix"];
    let mut matrix = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "matrix" => matrix = Some(prop.value().f32_array()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(Affine2dLike {
        matrix: matrix.ok_or_else(|| object.missing_required_field("matrix"))?,
    })
}

fn read_binding_def<S>(value: ValueReader<'_, '_, S>) -> Result<BindingDefLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["source", "target"];
    let mut source = None;
    let mut target = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "source" => source = Some(read_binding_endpoint(prop.value())?),
            "target" => target = Some(read_binding_endpoint(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(BindingDefLike { source, target })
}

fn read_binding_endpoint<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<BindingEndpointLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["ref", "value"];
    let mut reference = None;
    let mut value_endpoint = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "ref" => reference = Some(prop.value().string()?),
            "value" => value_endpoint = Some(read_lp_value(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    match (reference, value_endpoint) {
        (Some(reference), None) => Ok(BindingEndpointLike::Ref(reference)),
        (None, Some(value)) => Ok(BindingEndpointLike::Value(value)),
        _ => Err(object.missing_required_field("ref or value")),
    }
}

fn read_lp_value<S>(value: ValueReader<'_, '_, S>) -> Result<LpValueLike, SyntaxError>
where
    S: SyntaxEventSource,
{
    value.f32().map(LpValueLike::F32)
}

fn write_bundle_json(bundle: &ManualSourceBundle) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = SlotJsonWriter::new(&mut out);
    let mut object = writer.object().unwrap();
    write_project_def_json(object.prop("project").unwrap(), &bundle.project);
    let mut nodes = object.prop("node_defs").unwrap().array().unwrap();
    for node in &bundle.nodes {
        write_node_def_json(nodes.item().unwrap(), node);
    }
    nodes.finish().unwrap();
    object.finish().unwrap();
    out
}

fn write_project_def_json<W>(value: SlotJsonValue<'_, W>, project: &ProjectDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("ProjectDef").unwrap();
    if let Some(name) = &project.name {
        object.prop("name").unwrap().string(name).unwrap();
    }
    write_string_map(
        object.prop("nodes").unwrap(),
        &project.nodes,
        |value, invocation| {
            write_node_invocation_json(value, invocation);
        },
    );
    object.finish().unwrap();
}

fn write_node_def_json<W>(value: SlotJsonValue<'_, W>, node: &ManualNodeDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    match node {
        ManualNodeDef::Output(output) => write_output_def_json(value, output),
        ManualNodeDef::Texture(texture) => write_texture_def_json(value, texture),
        ManualNodeDef::Shader(shader) => write_shader_def_json(value, shader),
        ManualNodeDef::Fixture(fixture) => write_fixture_def_json(value, fixture),
    }
}

fn write_output_def_json<W>(value: SlotJsonValue<'_, W>, output: &OutputDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("OutputDef").unwrap();
    object.prop("pin").unwrap().u32(output.pin).unwrap();
    write_string_map(
        object.prop("bindings").unwrap(),
        &output.bindings,
        |value, binding| {
            write_binding_def_json(value, binding);
        },
    );
    if let Some(options) = &output.options {
        write_output_options_json(object.prop("options").unwrap(), options);
    }
    object.finish().unwrap();
}

fn write_texture_def_json<W>(value: SlotJsonValue<'_, W>, texture: &TextureDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("TextureDef").unwrap();
    write_dim2u_json(object.prop("size").unwrap(), &texture.size);
    write_string_map(
        object.prop("bindings").unwrap(),
        &texture.bindings,
        |value, binding| {
            write_binding_def_json(value, binding);
        },
    );
    object.finish().unwrap();
}

fn write_shader_def_json<W>(value: SlotJsonValue<'_, W>, shader: &ShaderDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("ShaderDef").unwrap();
    object
        .prop("glsl_path")
        .unwrap()
        .string(&shader.glsl_path)
        .unwrap();
    object
        .prop("render_order")
        .unwrap()
        .u32(shader.render_order)
        .unwrap();
    write_string_map(
        object.prop("bindings").unwrap(),
        &shader.bindings,
        |value, binding| {
            write_binding_def_json(value, binding);
        },
    );
    write_glsl_opts_json(object.prop("glsl_opts").unwrap(), &shader.glsl_opts);
    write_string_map(
        object.prop("param_defs").unwrap(),
        &shader.param_defs,
        |value, param| write_shader_param_def_json(value, param),
    );
    object.finish().unwrap();
}

fn write_fixture_def_json<W>(value: SlotJsonValue<'_, W>, fixture: &FixtureDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("FixtureDef").unwrap();
    write_dim2u_json(object.prop("render_size").unwrap(), &fixture.render_size);
    write_string_map(
        object.prop("bindings").unwrap(),
        &fixture.bindings,
        |value, binding| {
            write_binding_def_json(value, binding);
        },
    );
    write_mapping_config_json(object.prop("mapping").unwrap(), &fixture.mapping);
    if let Some(mapping) = &fixture.disabled_mapping_probe {
        write_mapping_config_json(object.prop("disabled_mapping_probe").unwrap(), mapping);
    }
    object
        .prop("color_order")
        .unwrap()
        .string(&fixture.color_order)
        .unwrap();
    write_affine2d_json(object.prop("transform").unwrap(), &fixture.transform);
    if let Some(brightness) = &fixture.brightness {
        write_scalar_hint_json(object.prop("brightness").unwrap(), brightness);
    }
    if let Some(gamma_correction) = fixture.gamma_correction {
        object
            .prop("gamma_correction")
            .unwrap()
            .bool(gamma_correction)
            .unwrap();
    }
    object.finish().unwrap();
}

fn write_node_invocation_json<W>(value: SlotJsonValue<'_, W>, invocation: &NodeInvocationDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object
        .prop("artifact")
        .unwrap()
        .string(&invocation.artifact)
        .unwrap();
    object.finish().unwrap();
}

fn write_output_options_json<W>(value: SlotJsonValue<'_, W>, options: &OutputOptionsLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object
        .prop("lum_power")
        .unwrap()
        .f32(options.lum_power)
        .unwrap();
    write_f32_array(object.prop("white_point").unwrap(), &options.white_point);
    object
        .prop("brightness")
        .unwrap()
        .f32(options.brightness)
        .unwrap();
    object
        .prop("interpolation_enabled")
        .unwrap()
        .bool(options.interpolation_enabled)
        .unwrap();
    object
        .prop("dithering_enabled")
        .unwrap()
        .bool(options.dithering_enabled)
        .unwrap();
    object
        .prop("lut_enabled")
        .unwrap()
        .bool(options.lut_enabled)
        .unwrap();
    object.finish().unwrap();
}

fn write_glsl_opts_json<W>(value: SlotJsonValue<'_, W>, glsl_opts: &GlslOptsLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object
        .prop("add_sub")
        .unwrap()
        .string(&glsl_opts.add_sub)
        .unwrap();
    object.prop("mul").unwrap().string(&glsl_opts.mul).unwrap();
    object.prop("div").unwrap().string(&glsl_opts.div).unwrap();
    object.finish().unwrap();
}

fn write_shader_param_def_json<W>(value: SlotJsonValue<'_, W>, param: &ShaderParamDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("label").unwrap().string(&param.label).unwrap();
    object
        .prop("description")
        .unwrap()
        .string(&param.description)
        .unwrap();
    object
        .prop("value_type")
        .unwrap()
        .string(&param.value_type)
        .unwrap();
    object.prop("default").unwrap().f32(param.default).unwrap();
    if let Some(min) = &param.min {
        write_scalar_hint_json(object.prop("min").unwrap(), min);
    }
    object.finish().unwrap();
}

fn write_scalar_hint_json<W>(value: SlotJsonValue<'_, W>, scalar_hint: &ScalarHintLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object
        .prop("value")
        .unwrap()
        .f32(scalar_hint.value)
        .unwrap();
    object.finish().unwrap();
}

fn write_mapping_config_json<W>(value: SlotJsonValue<'_, W>, mapping: &MappingConfig)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match mapping {
        MappingConfig::Disabled => {
            object.prop("kind").unwrap().string("Disabled").unwrap();
        }
        MappingConfig::Square { origin, size } => {
            object.prop("kind").unwrap().string("Square").unwrap();
            write_f32_array(object.prop("origin").unwrap(), origin);
            write_f32_array(object.prop("size").unwrap(), size);
        }
        MappingConfig::PathPoints {
            paths,
            sample_diameter,
        } => {
            object.prop("kind").unwrap().string("PathPoints").unwrap();
            write_u32_map(object.prop("paths").unwrap(), paths, |value, path| {
                write_path_spec_json(value, path);
            });
            object
                .prop("sample_diameter")
                .unwrap()
                .f32(*sample_diameter)
                .unwrap();
        }
    }
    object.finish().unwrap();
}

fn write_path_spec_json<W>(value: SlotJsonValue<'_, W>, path: &PathSpec)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match path {
        PathSpec::RingArray {
            center,
            diameter,
            start_ring_inclusive,
            end_ring_exclusive,
            ring_lamp_counts,
            offset_angle,
            order,
        } => {
            object.prop("kind").unwrap().string("RingArray").unwrap();
            write_f32_array(object.prop("center").unwrap(), center);
            object.prop("diameter").unwrap().f32(*diameter).unwrap();
            object
                .prop("start_ring_inclusive")
                .unwrap()
                .u32(*start_ring_inclusive)
                .unwrap();
            object
                .prop("end_ring_exclusive")
                .unwrap()
                .u32(*end_ring_exclusive)
                .unwrap();
            write_u32_map(
                object.prop("ring_lamp_counts").unwrap(),
                ring_lamp_counts,
                |value, count| {
                    value.u32(*count).unwrap();
                },
            );
            object
                .prop("offset_angle")
                .unwrap()
                .f32(*offset_angle)
                .unwrap();
            object.prop("order").unwrap().string(order).unwrap();
        }
        PathSpec::Manual => {
            object.prop("kind").unwrap().string("Manual").unwrap();
        }
    }
    object.finish().unwrap();
}

fn write_dim2u_json<W>(value: SlotJsonValue<'_, W>, dim: &Dim2uLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("width").unwrap().u32(dim.width).unwrap();
    object.prop("height").unwrap().u32(dim.height).unwrap();
    object.finish().unwrap();
}

fn write_affine2d_json<W>(value: SlotJsonValue<'_, W>, affine: &Affine2dLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    write_f32_array(object.prop("matrix").unwrap(), &affine.matrix);
    object.finish().unwrap();
}

fn write_binding_def_json<W>(value: SlotJsonValue<'_, W>, binding: &BindingDefLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    if let Some(source) = &binding.source {
        write_binding_endpoint_json(object.prop("source").unwrap(), source);
    }
    if let Some(target) = &binding.target {
        write_binding_endpoint_json(object.prop("target").unwrap(), target);
    }
    object.finish().unwrap();
}

fn write_binding_endpoint_json<W>(value: SlotJsonValue<'_, W>, endpoint: &BindingEndpointLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match endpoint {
        BindingEndpointLike::Ref(reference) => {
            object.prop("ref").unwrap().string(reference).unwrap();
        }
        BindingEndpointLike::Value(value) => {
            write_lp_value_json(object.prop("value").unwrap(), value);
        }
    }
    object.finish().unwrap();
}

fn write_lp_value_json<W>(value: SlotJsonValue<'_, W>, lp_value: &LpValueLike)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    match lp_value {
        LpValueLike::F32(inner) => value.f32(*inner).unwrap(),
    }
}

fn write_string_map<W, T>(
    value: SlotJsonValue<'_, W>,
    map: &BTreeMap<String, T>,
    mut write_value: impl FnMut(SlotJsonValue<'_, W>, &T),
) where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    for (key, entry) in map {
        write_value(object.prop(key).unwrap(), entry);
    }
    object.finish().unwrap();
}

fn write_u32_map<W, T>(
    value: SlotJsonValue<'_, W>,
    map: &BTreeMap<u32, T>,
    mut write_value: impl FnMut(SlotJsonValue<'_, W>, &T),
) where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    for (key, entry) in map {
        write_value(object.prop(&key.to_string()).unwrap(), entry);
    }
    object.finish().unwrap();
}

fn write_f32_array<W, const N: usize>(value: SlotJsonValue<'_, W>, values: &[f32; N])
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut array = value.array().unwrap();
    for value in values {
        array.item().unwrap().f32(*value).unwrap();
    }
    array.finish().unwrap();
}

impl NodeInvocationDefLike {
    fn new(artifact: &str) -> Self {
        Self {
            artifact: artifact.to_string(),
        }
    }
}

impl BindingDefLike {
    fn source(source: BindingEndpointLike) -> Self {
        Self {
            source: Some(source),
            target: None,
        }
    }

    fn target(target: BindingEndpointLike) -> Self {
        Self {
            source: None,
            target: Some(target),
        }
    }
}
