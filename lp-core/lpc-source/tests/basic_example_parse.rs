use lpc_model::BindingEndpoint;
use lpc_model::nodes::fixture::FixtureDef;
use lpc_model::nodes::output::OutputDef;
use lpc_model::nodes::project::project_def::ProjectDef;
use lpc_model::nodes::shader::ShaderDef;

#[test]
fn flat_basic_example_artifacts_parse_as_source_defs() {
    let project: ProjectDef = read_basic_toml("project.toml");
    assert_eq!(project.kind, ProjectDef::KIND);
    assert_eq!(project.name(), Some("basic"));
    assert_eq!(project.nodes.entries.len(), 3);
    assert_eq!(
        project
            .nodes
            .entries
            .get("shader")
            .unwrap()
            .artifact
            .value(),
        "./shader.toml"
    );

    let shader: ShaderDef = read_basic_toml("shader.toml");
    assert_eq!(shader.glsl_path.value(), "shader.glsl");
    assert!(matches!(
        shader.bindings.entries()["output"].target_endpoint(),
        Some(BindingEndpoint::Bus(_))
    ));

    let output: OutputDef = read_basic_toml("output.toml");
    assert_eq!(output.pin(), 4);
    assert!(!*output.options().unwrap().dithering_enabled.value());

    let fixture: FixtureDef = read_basic_toml("fixture.toml");
    assert_eq!(fixture.render_width(), 16);
    assert_eq!(fixture.render_height(), 16);
    assert!(matches!(
        fixture.bindings.entries()["input"].source_endpoint(),
        Some(BindingEndpoint::Bus(_))
    ));
    assert!(matches!(
        fixture.bindings.entries()["output"].target_endpoint(),
        Some(BindingEndpoint::Bus(_))
    ));
}

fn read_basic_toml<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/basic")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    toml::from_str(&text).unwrap()
}
