//! Asset overlay reads through materialize.

mod common;

use common::{fixtures, overlay};
use lpc_model::{Revision, SourceFileSlot};
use lpc_node_registry::{
    ArtifactError, ArtifactReadFailure, MaterializeError, NodeDefEntry, NodeDefLocation,
    NodeDefRegistry, ParseCtx, SourceDiagnosticCtx,
};
use lpfs::LpPath;

fn diag_ctx() -> SourceDiagnosticCtx {
    SourceDiagnosticCtx {
        containing_file: String::from("/shader.toml"),
        slot_path: None,
    }
}

fn load_shader_root(registry: &mut NodeDefRegistry, fs: &dyn lpfs::LpFs) -> NodeDefLocation {
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap()
}

fn snapshot_entry(registry: &NodeDefRegistry, loc: &NodeDefLocation) -> NodeDefEntry {
    registry.get(loc).expect("entry").clone()
}

#[test]
fn c4c_replace_glsl_via_overlay_def_unchanged() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let root = load_shader_root(&mut registry, &fs);
    let before = snapshot_entry(&registry, &root);
    let slot = SourceFileSlot::from_path("./shader.glsl");

    overlay::set_pending_artifact_body_text(
        &mut registry,
        "/shader.glsl",
        "void main() { gl_FragColor = vec4(0.0, 1.0, 0.0, 1.0); }",
    );

    let effective = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert!(effective.text.contains("0.0, 1.0, 0.0"));
    assert_eq!(snapshot_entry(&registry, &root), before);
}

#[test]
fn c4a_add_asset_via_overlay_implicit_create() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    load_shader_root(&mut registry, &fs);

    overlay::set_pending_artifact_body_text(&mut registry, "/extra.glsl", "void main() {}");

    let slot = SourceFileSlot::from_path("./extra.glsl");
    let materialized = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert_eq!(materialized.text, "void main() {}");
}

#[test]
fn c4b_delete_asset_via_overlay() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    load_shader_root(&mut registry, &fs);
    let slot = SourceFileSlot::from_path("./shader.glsl");

    overlay::delete_pending_artifact_body(&mut registry, "/shader.glsl");

    let err = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap_err();
    assert_eq!(
        err,
        MaterializeError::Artifact(ArtifactError::Read(ArtifactReadFailure::Deleted))
    );
}

#[test]
fn c4d_replace_asset_without_touching_def_toml() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let root = load_shader_root(&mut registry, &fs);
    let before = snapshot_entry(&registry, &root);
    let slot = SourceFileSlot::from_path("./shader.glsl");
    let slot_revision = slot.revision();

    overlay::set_pending_artifact_body_text(
        &mut registry,
        "/shader.glsl",
        "void main() { /* draft */ }",
    );

    assert!(!registry.overlay_contains_path(LpPath::new("/shader.toml")));
    let effective = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert!(effective.text.contains("draft"));
    assert_eq!(effective.version, slot_revision);
    assert_eq!(snapshot_entry(&registry, &root), before);
}
