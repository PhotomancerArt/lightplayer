use lpc_model::{NodeLoc, NodeName};
use lpc_source::ProjectDef;
use lpc_source::node::fixture::FixtureDef;
use lpc_source::node::output::OutputDef;
use lpc_source::node::shader::ShaderDef;
use lpc_source::node::texture::TextureDef;

#[test]
fn flat_basic_example_artifacts_parse_as_source_defs() {
    let project: ProjectDef = read_basic_toml("project.toml");
    assert_eq!(project.kind, ProjectDef::KIND);
    assert_eq!(project.name.as_deref(), Some("basic"));
    assert_eq!(project.nodes.len(), 4);
    assert_eq!(
        project
            .nodes
            .get(&NodeName::parse("shader").unwrap())
            .unwrap()
            .artifact
            .to_string(),
        "shader.toml"
    );

    let texture: TextureDef = read_basic_toml("texture.toml");
    assert_eq!(texture.width, 16);
    assert_eq!(texture.height, 16);

    let shader: ShaderDef = read_basic_toml("shader.toml");
    assert_eq!(shader.glsl_path.as_str(), "shader.glsl");
    assert_eq!(shader.texture_loc, NodeLoc::from("..texture"));

    let output: OutputDef = read_basic_toml("output.toml");
    match output {
        OutputDef::GpioStrip { pin, options } => {
            assert_eq!(pin, 4);
            assert!(!options.unwrap().dithering_enabled);
        }
    }

    let fixture: FixtureDef = read_basic_toml("fixture.toml");
    assert_eq!(fixture.output_loc, NodeLoc::from("..output"));
    assert_eq!(fixture.texture_loc, NodeLoc::from("..texture"));
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
