use super::fixture::Harness;

#[test]
fn server_tree_walk_prints_runtime_and_source_roots() {
    let harness = Harness::new();

    harness.print_server_tree("source.project");
    harness.print_server_tree("source.shader");
    harness.print_server_tree("source.fixture");
    harness.print_server_tree("engine.fixture_node");

    let shader_lines = crate::wire::print_root(
        harness.server_root("source.shader"),
        &harness.runtime.registry,
    );
    assert!(
        shader_lines
            .iter()
            .any(|line| line.contains("consumed_slots[exposure].default"))
    );

    let fixture_lines = crate::wire::print_root(
        harness.server_root("source.fixture"),
        &harness.runtime.registry,
    );
    assert!(fixture_lines.iter().any(|line| {
        line.contains("mapping.path_points.path.ring_array.ring_lamp_counts: Array")
    }));
    assert!(fixture_lines.iter().any(|line| {
        line.contains("mapping.path_points.path.ring_array.semantic_ring_lamp_counts: Array")
    }));
    assert!(
        !fixture_lines
            .iter()
            .any(|line| line.contains("ring_lamp_counts[0]"))
    );
}
