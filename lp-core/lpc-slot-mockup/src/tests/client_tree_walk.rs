use super::fixture::Harness;

#[test]
fn client_tree_walk_prints_synced_roots() {
    let mut harness = Harness::new();

    harness.sync_full();
    harness.print_client_tree("source.project");
    harness.print_client_tree("source.shader");
    harness.print_client_tree("source.fixture");
    harness.print_client_tree("engine.fixture_node");

    assert!(harness.client.roots.contains_key("source.project"));
    assert!(harness.client.roots.contains_key("engine.fixture_node"));
}
