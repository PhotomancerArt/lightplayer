use std::{fs, path::PathBuf};

use crate::source::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef};

#[test]
fn generated_basic_source_toml_round_trips_and_documents_authored_shape() {
    let evidence_dir = evidence_dir();
    if evidence_dir.exists() {
        fs::remove_dir_all(&evidence_dir).unwrap();
    }
    fs::create_dir_all(&evidence_dir).unwrap();

    println!("generating source-basic authored TOML evidence");
    write_evidence(&evidence_dir, "project.toml", &ProjectDef::new());
    write_evidence(&evidence_dir, "output.toml", &OutputDef::new());
    write_evidence(&evidence_dir, "texture.toml", &TextureDef::new());
    let shader_toml = write_evidence(&evidence_dir, "shader.toml", &ShaderDef::new());
    let fixture_toml = write_evidence(&evidence_dir, "fixture.toml", &FixtureDef::new());

    println!("round-tripping generated source TOML");
    let _project: ProjectDef = toml::from_str(&read_evidence(&evidence_dir, "project.toml"))
        .expect("project toml round-trip");
    let _output: OutputDef = toml::from_str(&read_evidence(&evidence_dir, "output.toml"))
        .expect("output toml round-trip");
    let _texture: TextureDef = toml::from_str(&read_evidence(&evidence_dir, "texture.toml"))
        .expect("texture toml round-trip");
    let _shader: ShaderDef = toml::from_str(&shader_toml).expect("shader toml round-trip");
    let _fixture: FixtureDef = toml::from_str(&fixture_toml).expect("fixture toml round-trip");

    assert!(shader_toml.contains("[param_defs.exposure]"));
    assert!(shader_toml.contains("texture_loc = \"..texture\""));
    assert!(fixture_toml.contains("kind = \"path_points\""));
    assert!(fixture_toml.contains("[mapping.points.1]"));
    assert!(fixture_toml.contains("[mapping.path]"));
    assert!(fixture_toml.contains("ring_lamp_counts = ["));
    assert!(!fixture_toml.contains("[mapping.path.ring_lamp_counts]"));
}

fn evidence_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/slot-mockup-evidence/source-basic")
}

fn write_evidence<T: serde::Serialize>(dir: &PathBuf, name: &str, value: &T) -> String {
    let toml = toml::to_string_pretty(value).unwrap();
    let path = dir.join(name);
    fs::write(&path, &toml).unwrap();
    println!("wrote {}", path.display());
    println!("{toml}");
    toml
}

fn read_evidence(dir: &PathBuf, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}
