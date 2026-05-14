use crate::{
    engine::{FixtureNode, OutputNode, ShaderNode},
    source::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef},
};
use lpc_model::{SlotShapeRegistry, StaticSlotShape};

#[test]
fn generated_registration_covers_static_roots() {
    let mut registry = SlotShapeRegistry::default();

    crate::model::register_shapes(&mut registry).unwrap();

    assert_static_root::<ProjectDef>(&registry);
    assert_static_root::<ShaderDef>(&registry);
    assert_static_root::<FixtureDef>(&registry);
    assert_static_root::<OutputDef>(&registry);
    assert_static_root::<TextureDef>(&registry);
    assert_static_root::<FixtureNode>(&registry);
    assert_static_root::<OutputNode>(&registry);
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
    assert_static_root::<ShaderDef>(&registry);
}

fn assert_static_root<T: StaticSlotShape>(registry: &SlotShapeRegistry) {
    assert!(registry.contains(&T::SHAPE_ID));
}
