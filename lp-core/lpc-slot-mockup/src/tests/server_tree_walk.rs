use super::fixture::Harness;

#[test]
fn server_tree_walk_prints_runtime_and_source_roots() {
    let harness = Harness::new();

    harness.print_server_tree("source.project");
    harness.print_server_tree("source.shader");
    harness.print_server_tree("engine.fixture_node");

    let shader_lines = crate::wire::print_root(
        harness.server_root("source.shader"),
        &harness.runtime.registry,
    );
    assert!(
        shader_lines
            .iter()
            .any(|line| line.contains("param_defs.exposure.default"))
    );
}
