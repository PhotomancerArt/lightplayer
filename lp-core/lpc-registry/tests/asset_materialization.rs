mod support;

use lpc_model::{
    ArtifactLocation, AssetOverlay, AssetSource, NodeDefLocation, SlotPath,
};
use lpc_model::project::overlay_mutation::mutation_op::MutationOp;
use lpc_registry::MaterializeAssetError;

use support::{artifact, artifact_asset, RegistryScenario};

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
        edit: AssetOverlay::ReplaceBody(b"overlay shader".to_vec()),
    });
    let replaced = scenario
        .materialize_asset_text(&source)
        .expect("overlay text");
    assert_eq!(replaced.text, "overlay shader");

    scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.glsl"),
        edit: AssetOverlay::Delete,
    });
    let err = scenario.materialize_asset_text(&source).unwrap_err();
    assert_eq!(err, MaterializeAssetError::Deleted { source });
}

#[test]
fn materializes_inline_glsl_from_effective_owner_def() {
    let mut scenario = RegistryScenario::empty();
    scenario.write_file(
        "/project.toml",
        r#"
kind = "Project"

[nodes.shader.def]
kind = "Shader"
source = { glsl = "vec4 render(vec2 pos) { return vec4(1.0); }" }
"#,
    );
    scenario.load_root("/project.toml");

    let source = AssetSource::inline(
        NodeDefLocation {
            artifact: artifact("/project.toml"),
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
        "/project.toml:nodes[shader].source.glsl"
    );
}

#[test]
fn materialization_rejects_unref_and_invalid_utf8_text() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");
    let unreferenced = artifact_asset("/not-referenced.glsl");

    let err = scenario.materialize_asset(&unreferenced).unwrap_err();
    assert_eq!(
        err,
        MaterializeAssetError::UnreferencedAsset {
            source: unreferenced
        }
    );

    scenario.apply(MutationOp::SetArtifactBody {
        artifact: ArtifactLocation::file("/idle.glsl"),
        edit: AssetOverlay::ReplaceBody(vec![0xff]),
    });
    let err = scenario
        .materialize_asset_text(&artifact_asset("/idle.glsl"))
        .unwrap_err();
    assert!(matches!(err, MaterializeAssetError::Utf8 { .. }));
}
