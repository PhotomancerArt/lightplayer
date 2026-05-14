use std::collections::BTreeMap;

use crate::generated_slot_codec::{
    GeneratedBindingDef, GeneratedBundle, GeneratedEndpoint, GeneratedFixtureDef,
    GeneratedInvocation, GeneratedMapping, GeneratedNodeDef, GeneratedOutputDef,
    GeneratedOutputOptions, GeneratedProject, read_bundle_json, read_bundle_toml,
    write_bundle_json,
};

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
