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
            .any(|line| line.contains("param_defs[exposure].default"))
    );

    let fixture_lines = crate::wire::print_root(
        harness.server_root("source.fixture"),
        &harness.runtime.registry,
    );
    assert!(fixture_lines.iter().any(|line| {
        line.contains("mapping.PathPoints.paths[0].RingArray.ring_lamp_counts: map")
    }));
    assert!(fixture_lines.iter().any(|line| {
        line.contains("mapping.PathPoints.paths[0].RingArray.ring_lamp_counts[0]: U32")
    }));
    assert!(
        fixture_lines
            .iter()
            .any(|line| line.contains("mapping.PathPoints.sample_diameter: F32"))
    );
}
