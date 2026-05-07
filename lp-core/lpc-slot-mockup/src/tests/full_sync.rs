use lpc_model::{LpValue, SlotData, SlotMapKey};

use super::fixture::{Harness, assert_map_has_key, select};

#[test]
fn full_sync_copies_server_roots_to_client() {
    let mut harness = Harness::new();

    harness.print_server_tree("source.shader");
    harness.print_server_tree("source.fixture");
    harness.sync_full();
    harness.print_client_tree("source.shader");
    harness.print_client_tree("source.fixture");

    let shader = harness.client.roots.get("source.shader").unwrap();
    assert_map_has_key(
        shader,
        "param_defs",
        SlotMapKey::String("exposure".to_string()),
    );
    assert_map_has_key(
        shader,
        "param_defs",
        SlotMapKey::String("speed".to_string()),
    );

    let fixture = harness.client.roots.get("source.fixture").unwrap();
    let ring_lamp_counts = select(
        fixture,
        "mapping.path_points.path.ring_array.ring_lamp_counts",
    );
    let SlotData::Value(value) = ring_lamp_counts else {
        panic!("ring_lamp_counts should be one slot value");
    };
    assert_eq!(
        value.value(),
        &LpValue::Array(vec![LpValue::U32(1), LpValue::U32(96)])
    );
}
