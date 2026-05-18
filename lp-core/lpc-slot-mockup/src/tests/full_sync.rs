use lpc_model::{SlotData, SlotMapKey};

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
        "consumed_slots",
        SlotMapKey::String("exposure".to_string()),
    );
    assert_map_has_key(
        shader,
        "consumed_slots",
        SlotMapKey::String("speed".to_string()),
    );

    let fixture = harness.client.roots.get("source.fixture").unwrap();
    let ring_lamp_counts = select(
        fixture,
        "mapping.PathPoints.paths[0].RingArray.ring_lamp_counts",
    );
    assert_map_has_key(ring_lamp_counts, "", SlotMapKey::U32(0));
    assert_map_has_key(ring_lamp_counts, "", SlotMapKey::U32(1));
    let count_zero = select(ring_lamp_counts, "[0]");
    let SlotData::Value(value) = count_zero else {
        panic!("ring_lamp_counts[0] should be one slot value");
    };
    assert_eq!(value.value(), &lpc_model::LpValue::U32(1));

    let count_one = select(ring_lamp_counts, "[1]");
    let SlotData::Value(value) = count_one else {
        panic!("ring_lamp_counts[1] should be one slot value");
    };
    assert_eq!(value.value(), &lpc_model::LpValue::U32(96));
}
