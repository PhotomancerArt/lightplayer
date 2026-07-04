mod support;

use lpc_model::{
    ArtifactLocation, AssetBodyOverlay, AssetLocation, MutationOp, NodeDefLocation, SlotPath,
};
use lpc_registry::AssetReadError;

use support::{RegistryScenario, artifact, artifact_asset};

#[test]
fn materializes_committed_shader_source_and_fixture_assets() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let shader = scenario
        .materialize_asset_text(&artifact_asset("/idle.glsl"))
        .expect("shader source");
    assert!(shader.text.contains("vec4 render"));
    assert_eq!(shader.diagnostic_name, "/idle.glsl");

    let fixture = scenario
        .materialize_asset(&artifact_asset("/fyeah-mapping.svg"))
        .expect("fixture svg");
    assert!(fixture.bytes.starts_with(b"<?xml"));
    assert_eq!(fixture.diagnostic_name, "/fyeah-mapping.svg");
}

#[test]
fn materialization_uses_overlay_replacement_and_reports_delete() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");
    let source = artifact_asset("/idle.glsl");

    scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.glsl"),
        edit: AssetBodyOverlay::ReplaceBody(b"overlay shader".to_vec()),
    });
    let replaced = scenario
        .materialize_asset_text(&source)
        .expect("overlay text");
    assert_eq!(replaced.text, "overlay shader");

    scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.glsl"),
        edit: AssetBodyOverlay::Delete,
    });
    let err = scenario.materialize_asset_text(&source).unwrap_err();
    assert_eq!(err, AssetReadError::Deleted { location: source });
}

#[test]
fn materializes_inline_glsl_from_effective_owner_def() {
    let mut scenario = RegistryScenario::empty();
    scenario.write_file(
        "/project.json",
        r#"
{
  "kind": "Project",
  "nodes": {
    "shader": {
      "def": {
        "kind": "Shader",
        "source": {
          "glsl": "vec4 render(vec2 pos) { return vec4(1.0); }"
        }
      }
    }
  }
}
"#,
    );
    scenario.load_root("/project.json");

    let source = AssetLocation::inline(
        NodeDefLocation {
            artifact: artifact("/project.json"),
            path: SlotPath::parse("nodes[shader]").unwrap(),
        },
        SlotPath::parse("nodes[shader].source").unwrap(),
    );
    let materialized = scenario
        .materialize_asset_text(&source)
        .expect("inline source");

    assert!(materialized.text.contains("vec4 render"));
    assert_eq!(
        materialized.diagnostic_name,
        "/project.json:nodes[shader].source.glsl"
    );
}

#[test]
fn materialization_rejects_unref_and_invalid_utf8_text() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");
    let unreferenced = artifact_asset("/not-referenced.glsl");

    let err = scenario.materialize_asset(&unreferenced).unwrap_err();
    assert_eq!(
        err,
        AssetReadError::UnreferencedAsset {
            location: unreferenced
        }
    );

    scenario.apply(MutationOp::SetArtifactBody {
        artifact: ArtifactLocation::file("/idle.glsl"),
        edit: AssetBodyOverlay::ReplaceBody(vec![0xff]),
    });
    let err = scenario
        .materialize_asset_text(&artifact_asset("/idle.glsl"))
        .unwrap_err();
    assert!(matches!(err, AssetReadError::Utf8 { .. }));
}
