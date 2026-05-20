use lpc_model::{
    LpType, LpValue, SlotAccess, SlotDataAccess, SlotFactoryError, SlotMapKey, SlotPath,
    SlotShapeId, StaticSlotShape, insert_slot_map_entry_default, set_slot_value,
    set_slot_variant_default,
};
use std::string::String;

use crate::engine::ShaderNode;
use crate::source::{FixtureDef, ProjectDef, ShaderDef};

#[test]
fn shape_factory_creates_static_project_def() {
    let registry = registry();

    let object = registry
        .create_default(ProjectDef::SHAPE_ID)
        .expect("project default object");

    assert_eq!(object.shape_id(), ProjectDef::SHAPE_ID);
    let SlotDataAccess::Record(record) = object.data() else {
        panic!("expected project record");
    };
    assert!(record.field(0).is_some(), "project name field");
    assert!(record.field(1).is_some(), "project nodes field");
}

#[test]
fn shape_factory_static_defaults_are_empty_slot_defaults() {
    let registry = registry();

    let object = registry
        .create_default(ShaderDef::SHAPE_ID)
        .expect("shader default object");

    let SlotDataAccess::Record(record) = object.data() else {
        panic!("expected shader record");
    };
    let SlotDataAccess::Enum(source) = record.field(0).expect("source") else {
        panic!("expected source enum");
    };
    assert_eq!(source.variant(), "path");
    let SlotDataAccess::Value(path) = source.data() else {
        panic!("expected source path value");
    };

    assert_eq!(path.value(), LpValue::String(String::new()));
}

#[test]
fn shape_factory_creates_dynamic_shader_node_object() {
    let shader_def = ShaderDef::new();
    let shader_node = ShaderNode::from_def(&shader_def);
    let mut registry = registry();
    registry
        .register_dynamic_shape(shader_node.shape_id(), shader_node.shape())
        .expect("dynamic shader node shape");

    let object = registry
        .create_default(shader_node.shape_id())
        .expect("dynamic shader node object");

    assert_eq!(object.shape_id(), shader_node.shape_id());
    let SlotDataAccess::Record(record) = object.data() else {
        panic!("expected shader node record");
    };
    let SlotDataAccess::Record(params) = record.field(0).expect("params") else {
        panic!("expected params record");
    };
    assert!(params.field(0).is_some());
}

#[test]
fn shape_factory_created_object_can_insert_map_entry_then_mutate() {
    let registry = registry();
    let mut object = registry
        .create_default(ProjectDef::SHAPE_ID)
        .expect("project default object");

    insert_slot_map_entry_default(
        object.as_mut(),
        &registry,
        &SlotPath::parse("nodes").unwrap(),
        lpc_model::Revision::new(20),
        &SlotMapKey::String(String::from("extra")),
    )
    .unwrap();
    set_slot_value(
        object.as_mut(),
        &registry,
        &SlotPath::parse("nodes[extra].def.path").unwrap(),
        lpc_model::Revision::new(21),
        LpValue::String(String::from("./extra.toml")),
    )
    .unwrap();
}

#[test]
fn shape_factory_created_object_can_switch_enum_then_mutate_payload() {
    let registry = registry();
    let mut object = registry
        .create_default(FixtureDef::SHAPE_ID)
        .expect("fixture default object");

    set_slot_variant_default(
        object.as_mut(),
        &registry,
        &SlotPath::parse("mapping").unwrap(),
        lpc_model::Revision::new(30),
        "Square",
    )
    .unwrap();
    set_slot_value(
        object.as_mut(),
        &registry,
        &SlotPath::parse("mapping.origin").unwrap(),
        lpc_model::Revision::new(31),
        LpValue::Vec2([0.25, 0.75]),
    )
    .unwrap();
}

#[test]
fn shape_factory_reports_explicitly_uncreatable_shapes() {
    let mut registry = registry();
    let shape_id = SlotShapeId::from_static_name("mockup.uncreatable_shape");
    registry
        .register_uncreatable_shape(shape_id, lpc_model::slot::shape::value(LpType::Bool))
        .unwrap();

    let Err(error) = registry.create_default(shape_id) else {
        panic!("expected uncreatable shape error");
    };

    assert_eq!(error, SlotFactoryError::UnsupportedFactory(shape_id));
}

fn registry() -> lpc_model::SlotShapeRegistry {
    let mut registry = lpc_model::SlotShapeRegistry::default();
    crate::model::register_shapes(&mut registry).unwrap();
    registry
}
