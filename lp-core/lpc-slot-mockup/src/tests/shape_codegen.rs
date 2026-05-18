use crate::{
    engine::{FixtureNode, OutputNode, ShaderNode},
    source::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef},
};
use lpc_model::{SlotShapeRegistry, StaticSlotShape};

#[test]
fn generated_registration_covers_static_shapes() {
    let mut registry = SlotShapeRegistry::default();

    crate::model::register_shapes(&mut registry).unwrap();

    assert_static_shape::<ProjectDef>(&registry);
    assert_static_shape::<ShaderDef>(&registry);
    assert_static_shape::<FixtureDef>(&registry);
    assert_static_shape::<OutputDef>(&registry);
    assert_static_shape::<TextureDef>(&registry);
    assert_static_shape::<FixtureNode>(&registry);
    assert_static_shape::<OutputNode>(&registry);
    assert!(!registry.contains(&ShaderNode::SHAPE_ID));
}

#[test]
fn generated_ensure_is_idempotent() {
    let mut registry = SlotShapeRegistry::default();
    lpc_model::slot_shapes::register_all_static_slot_shapes(&mut registry).unwrap();

    let first =
        crate::slot_shapes::ensure_static_slot_shape(&mut registry, ShaderDef::SHAPE_ID).unwrap();
    let second =
        crate::slot_shapes::ensure_static_slot_shape(&mut registry, ShaderDef::SHAPE_ID).unwrap();

    assert!(first);
    assert!(!second);
    assert_static_shape::<ShaderDef>(&registry);
}

fn assert_static_shape<T: StaticSlotShape>(registry: &SlotShapeRegistry) {
    assert!(registry.contains(&T::SHAPE_ID));
}
