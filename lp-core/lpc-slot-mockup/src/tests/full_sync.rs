use lpc_model::SlotMapKey;

use super::fixture::{Harness, assert_map_has_key};

#[test]
fn full_sync_copies_server_roots_to_client() {
    let mut harness = Harness::new();

    harness.print_server_tree("source.shader");
    harness.sync_full();
    harness.print_client_tree("source.shader");

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
}
