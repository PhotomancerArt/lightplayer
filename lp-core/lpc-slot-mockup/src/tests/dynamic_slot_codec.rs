use lpc_model::{
    LpValue, SlotAccess, SlotDataAccess, StaticSlotShape, slot_codec::JsonSyntaxSource,
};

use crate::{
    engine::ShaderNode,
    source::{FixtureDef, ProjectDef, ShaderDef},
};

#[test]
fn dynamic_slot_codec_reads_project_json_through_registry() {
    let registry = registry();

    let object = registry
        .read_slot_json(
            ProjectDef::SHAPE_ID,
            r#"{"name":"basic","nodes":{"shader":{"artifact":"./shader.toml"}}}"#,
        )
        .unwrap();
    let Ok(project) = object.into_any().downcast::<ProjectDef>() else {
        panic!("expected ProjectDef");
    };

    assert_eq!(project.name.data.as_ref().unwrap().value(), "basic");
    assert_eq!(
        project.nodes.entries.get("shader").unwrap().artifact(),
        "./shader.toml"
    );
}

#[test]
fn dynamic_slot_codec_reads_project_toml_through_registry() {
    let registry = registry();
    let toml: toml::Value = toml::from_str(
        r#"
name = "basic"

[nodes.shader]
artifact = "./shader.toml"
"#,
    )
    .unwrap();

    let object = registry
        .read_slot_toml(ProjectDef::SHAPE_ID, &toml)
        .unwrap();
    let Ok(project) = object.into_any().downcast::<ProjectDef>() else {
        panic!("expected ProjectDef");
    };

    assert_eq!(project.name.data.as_ref().unwrap().value(), "basic");
    assert_eq!(
        project.nodes.entries.get("shader").unwrap().artifact(),
        "./shader.toml"
    );
}

#[test]
fn dynamic_slot_codec_reads_json_event_sources() {
    let registry = registry();
    let object = registry
        .read_slot_from(
            ProjectDef::SHAPE_ID,
            JsonSyntaxSource::new(r#"{"nodes":{"shader":{"artifact":"./shader.toml"}}}"#).unwrap(),
        )
        .unwrap();
    let Ok(project) = object.into_any().downcast::<ProjectDef>() else {
        panic!("expected ProjectDef");
    };

    assert_eq!(
        project.nodes.entries.get("shader").unwrap().artifact(),
        "./shader.toml"
    );
}

#[test]
fn dynamic_slot_codec_reads_fixture_enum_payloads() {
    let registry = registry();

    let object = registry
        .read_slot_json(
            FixtureDef::SHAPE_ID,
            r#"{"mapping":{"kind":"square","origin":[0.25,0.75],"size":[0.5,0.25]}}"#,
        )
        .unwrap();
    let Ok(fixture) = object.into_any().downcast::<FixtureDef>() else {
        panic!("expected FixtureDef");
    };

    assert_eq!(
        fixture.mapping().square_fields(),
        Some(([0.25, 0.75], [0.5, 0.25]))
    );
}

#[test]
fn dynamic_slot_codec_reads_registered_dynamic_shapes() {
    let shader_def = ShaderDef::new();
    let shader_node = ShaderNode::from_def(&shader_def);
    let mut registry = registry();
    registry
        .register_dynamic_shape(shader_node.shape_id(), shader_node.shape())
        .unwrap();

    let object = registry
        .read_slot_json(
            shader_node.shape_id(),
            r#"{"params":{"exposure":1.25},"compile_error":"warning"}"#,
        )
        .unwrap();

    let SlotDataAccess::Record(shader_node_data) = object.data() else {
        panic!("expected shader node record");
    };
    let SlotDataAccess::Record(params) = shader_node_data.field(0).unwrap() else {
        panic!("expected params record");
    };
    assert_eq!(record_value(params, 0), LpValue::F32(1.25));
    assert_eq!(
        option_value(shader_node_data.field(1).unwrap()),
        Some(LpValue::String("warning".into()))
    );
}

#[test]
fn dynamic_slot_codec_rejects_unknown_fields() {
    let registry = registry();

    let Err(error) = registry.read_slot_json(ProjectDef::SHAPE_ID, r#"{"surprise":true}"#) else {
        panic!("expected unknown field error");
    };

    assert!(error.message().contains("surprise"));
    assert!(error.message().contains("nodes"));
}

#[test]
fn dynamic_slot_codec_rejects_invalid_discriminators() {
    let registry = registry();

    let Err(error) =
        registry.read_slot_json(FixtureDef::SHAPE_ID, r#"{"mapping":{"kind":"hex_grid"}}"#)
    else {
        panic!("expected discriminator error");
    };

    assert!(error.message().contains("hex_grid"));
    assert!(error.message().contains("disabled"));
    assert!(error.message().contains("square"));
    assert!(error.message().contains("path_points"));
}

fn registry() -> lpc_model::SlotShapeRegistry {
    let mut registry = lpc_model::SlotShapeRegistry::default();
    crate::model::register_shapes(&mut registry).unwrap();
    registry
}

fn record_value(record: &dyn lpc_model::SlotRecordAccess, index: usize) -> LpValue {
    match record.field(index).unwrap() {
        SlotDataAccess::Value(value) => value.value(),
        other => panic!("expected value, got {}", data_kind(other)),
    }
}

fn option_value(data: SlotDataAccess<'_>) -> Option<LpValue> {
    let SlotDataAccess::Option(option) = data else {
        panic!("expected option");
    };
    option.data().map(|data| match data {
        SlotDataAccess::Value(value) => value.value(),
        other => panic!("expected option value, got {}", data_kind(other)),
    })
}

fn data_kind(data: SlotDataAccess<'_>) -> &'static str {
    match data {
        SlotDataAccess::Unit(_) => "unit",
        SlotDataAccess::Value(_) => "value",
        SlotDataAccess::Record(_) => "record",
        SlotDataAccess::Map(_) => "map",
        SlotDataAccess::Enum(_) => "enum",
        SlotDataAccess::Option(_) => "option",
    }
}
