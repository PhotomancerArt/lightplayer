use lpc_model::{FrameId, ModelValue, SlotData, SlotMapKey};

use crate::{engine::MockRuntime, view::MockClient};

use super::{collect_diff, full_sync, print_root};

#[test]
fn full_sync_and_incremental_patches_cover_static_and_dynamic_slots() {
    let mut runtime = MockRuntime::new();
    let mut client = MockClient::default();
    client.apply_full_sync(full_sync(&runtime));

    let shader_root = runtime.roots()[1].1;
    let printed = print_root(shader_root, &runtime.registry);
    assert!(
        printed
            .iter()
            .any(|line| line.contains("param_defs.exposure.default"))
    );

    runtime.add_shader_param_def(FrameId::new(2), "gain", 0.5);
    client.apply_patches(collect_diff(
        "source.shader",
        runtime.roots()[1].1,
        &runtime.registry,
        FrameId::new(1),
    ));
    let shader = client.roots.get("source.shader").unwrap();
    let SlotData::Record(shader_record) = shader else {
        panic!("shader source record");
    };
    let SlotData::Map(param_defs) = &shader_record.fields[4] else {
        panic!("shader param defs map");
    };
    assert!(
        param_defs
            .entries
            .contains_key(&SlotMapKey::String("gain".into()))
    );

    runtime.set_shader_param(FrameId::new(3), "exposure", 2.5);
    client.apply_patches(collect_diff(
        "engine.shader_node",
        runtime.roots()[5].1,
        &runtime.registry,
        FrameId::new(2),
    ));
    let SlotData::Record(shader_node) = client.roots.get("engine.shader_node").unwrap() else {
        panic!("shader node record");
    };
    let SlotData::Map(params) = &shader_node.fields[0] else {
        panic!("shader params map");
    };
    let SlotData::Value(exposure) = params
        .entries
        .get(&SlotMapKey::String("exposure".into()))
        .unwrap()
    else {
        panic!("exposure value");
    };
    assert_eq!(exposure.value(), &ModelValue::F32(2.5));

    runtime.remove_shader_param(FrameId::new(4), "speed");
    client.apply_patches(collect_diff(
        "engine.shader_node",
        runtime.roots()[5].1,
        &runtime.registry,
        FrameId::new(3),
    ));
    let SlotData::Record(shader_node) = client.roots.get("engine.shader_node").unwrap() else {
        panic!("shader node record");
    };
    let SlotData::Map(params) = &shader_node.fields[0] else {
        panic!("shader params map");
    };
    assert!(
        !params
            .entries
            .contains_key(&SlotMapKey::String("speed".into()))
    );

    runtime.switch_fixture_mapping(FrameId::new(5));
    client.apply_patches(collect_diff(
        "source.fixture",
        runtime.roots()[2].1,
        &runtime.registry,
        FrameId::new(4),
    ));
    let SlotData::Record(fixture) = client.roots.get("source.fixture").unwrap() else {
        panic!("fixture record");
    };
    let SlotData::Enum(mapping) = &fixture.fields[2] else {
        panic!("fixture mapping enum");
    };
    assert_eq!(mapping.variant.as_str(), "square");

    runtime.clear_fixture_brightness(FrameId::new(6));
    client.apply_patches(collect_diff(
        "source.fixture",
        runtime.roots()[2].1,
        &runtime.registry,
        FrameId::new(5),
    ));
    let SlotData::Record(fixture) = client.roots.get("source.fixture").unwrap() else {
        panic!("fixture record");
    };
    let SlotData::Option(brightness) = &fixture.fields[4] else {
        panic!("fixture brightness option");
    };
    assert!(brightness.data.is_none());

    runtime.remove_touch(FrameId::new(7), 2);
    client.apply_patches(collect_diff(
        "engine.fixture_node",
        runtime.roots()[6].1,
        &runtime.registry,
        FrameId::new(6),
    ));
    let SlotData::Record(fixture_node) = client.roots.get("engine.fixture_node").unwrap() else {
        panic!("fixture node record");
    };
    let SlotData::Map(touches) = &fixture_node.fields[0] else {
        panic!("touch map");
    };
    assert!(!touches.entries.contains_key(&SlotMapKey::U32(2)));
}
