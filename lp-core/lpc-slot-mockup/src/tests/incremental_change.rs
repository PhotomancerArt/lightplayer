use lpc_model::{FrameId, LpValue, SlotMapKey};

use super::fixture::{
    Harness, assert_map_has_key, assert_shader_param, assert_shader_param_lacks, select,
};

#[test]
fn incremental_changes_patch_client_state() {
    let mut harness = Harness::new();
    harness.sync_full();
    harness.print_client_tree("engine.shader_node");

    println!("server updating source.fixture#mapping.path_points.path.ring_array.ring_lamp_counts");
    harness
        .runtime
        .set_fixture_ring_lamp_counts(FrameId::new(2), vec![1, 8, 12, 16]);
    harness.print_server_tree("source.fixture");
    harness.sync_diff("source.fixture", FrameId::new(1));
    harness.print_client_tree("source.fixture");
    assert_eq!(
        select(
            harness.client.roots.get("source.fixture").unwrap(),
            "mapping.path_points.path.ring_array.ring_lamp_counts",
        ),
        &lpc_model::SlotData::Value(lpc_model::Versioned::new(
            FrameId::new(2),
            LpValue::Array(vec![
                LpValue::U32(1),
                LpValue::U32(8),
                LpValue::U32(12),
                LpValue::U32(16)
            ])
        )),
    );

    println!("server updating source.shader#param_defs[gain].default to 0.5");
    harness
        .runtime
        .add_shader_param_def(FrameId::new(3), "gain", 0.5);
    harness.print_server_tree("source.shader");
    harness.sync_diff("source.shader", FrameId::new(2));
    harness.print_client_tree("source.shader");
    assert_map_has_key(
        harness.client.roots.get("source.shader").unwrap(),
        "param_defs",
        SlotMapKey::String("gain".to_string()),
    );

    println!("server updating engine.shader_node#params.exposure to 2.5");
    harness
        .runtime
        .set_shader_param(FrameId::new(4), "exposure", 2.5);
    harness.print_server_tree("engine.shader_node");
    harness.sync_diff("engine.shader_node", FrameId::new(3));
    harness.print_client_tree("engine.shader_node");
    assert_shader_param(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "exposure",
        LpValue::F32(2.5),
    );

    println!("server removing engine.shader_node#params.speed");
    harness
        .runtime
        .remove_shader_param(FrameId::new(5), "speed");
    harness.print_server_tree("engine.shader_node");
    harness.sync_registry();
    harness.sync_diff("engine.shader_node", FrameId::new(4));
    harness.print_client_tree("engine.shader_node");
    assert_shader_param_lacks(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "speed",
    );

    println!("server updating source.fixture#mapping to square and brightness to none");
    harness.runtime.switch_fixture_mapping(FrameId::new(6));
    harness.runtime.clear_fixture_brightness(FrameId::new(7));
    harness.print_server_tree("source.fixture");
    harness.sync_diff("source.fixture", FrameId::new(5));
    harness.print_client_tree("source.fixture");

    println!("server updating source.fixture#mapping to disabled unit variant");
    harness.runtime.disable_fixture_mapping(FrameId::new(8));
    harness.print_server_tree("source.fixture");
    harness.sync_diff("source.fixture", FrameId::new(7));
    harness.print_client_tree("source.fixture");
    assert_eq!(
        select(
            harness.client.roots.get("source.fixture").unwrap(),
            "mapping.disabled",
        ),
        &lpc_model::SlotData::Unit {
            changed_frame: FrameId::new(8),
        },
    );
}
