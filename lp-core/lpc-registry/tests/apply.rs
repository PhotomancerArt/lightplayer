use lpc_model::{
    ArtifactLocation, AssetBodySource, AssetChangeKind, AssetOverlay, AssetSource, AssetState,
    LpValue, NodeDefChangeKind, NodeDefLocation, NodeDefState, OverlayMutation, Revision, SlotEdit,
    SlotPath, SlotShapeRegistry,
};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{FsEvent, FsEventKind, LpFs, LpFsMemory, LpPath, LpPathBuf};

fn parse_ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
    ParseCtx { shapes }
}

fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
    fs.write_file_mut(LpPath::new(path), contents.as_bytes())
        .unwrap();
}

fn shader_project() -> (LpFsMemory, SlotShapeRegistry, ProjectRegistry) {
    let shapes = SlotShapeRegistry::default();
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.toml",
        r#"
kind = "Project"

[nodes.shader]
ref = "./shader.toml"
"#,
    );
    write_file(
        &mut fs,
        "/shader.toml",
        r#"
kind = "Shader"
source = { path = "shader.glsl" }
"#,
    );
    write_file(&mut fs, "/shader.glsl", "void main() {}");

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(
            &fs,
            LpPath::new("/project.toml"),
            Revision::new(1),
            &parse_ctx(&shapes),
        )
        .unwrap();
    (fs, shapes, registry)
}

fn clock_project() -> (LpFsMemory, SlotShapeRegistry, ProjectRegistry) {
    let shapes = SlotShapeRegistry::default();
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.toml",
        r#"
kind = "Project"

[nodes.clock]
ref = "./clock.toml"
"#,
    );
    write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 1.0
"#,
    );

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(
            &fs,
            LpPath::new("/project.toml"),
            Revision::new(1),
            &parse_ctx(&shapes),
        )
        .unwrap();
    (fs, shapes, registry)
}

#[test]
fn apply_body_overlay_changes_referenced_node_def_and_assets() {
    let (fs, shapes, mut registry) = shader_project();
    let ctx = parse_ctx(&shapes);
    let shader_location = ArtifactLocation::file("/shader.toml");

    let result = registry
        .apply_mutation(
            &fs,
            OverlayMutation::SetArtifactBody {
                artifact: shader_location.clone(),
                edit: AssetOverlay::ReplaceBody(br#"kind = "Clock""#.to_vec()),
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    let shader_def = NodeDefLocation::artifact_root(shader_location);
    assert_eq!(
        result.changes.defs.changed,
        vec![lpc_model::NodeDefChange::new(
            shader_def.clone(),
            NodeDefChangeKind::KindChanged {
                from: lpc_model::NodeKind::Shader,
                to: lpc_model::NodeKind::Clock,
            }
        )]
    );
    assert_eq!(
        result.changes.assets.removed,
        vec![AssetSource::artifact(ArtifactLocation::file(
            "/shader.glsl"
        ))]
    );
    assert!(matches!(
        registry.def(&shader_def).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Clock(_))
    ));
}

#[test]
fn apply_asset_overlay_changes_referenced_asset() {
    let (fs, shapes, mut registry) = shader_project();
    let ctx = parse_ctx(&shapes);
    let asset = ArtifactLocation::file("/shader.glsl");
    let asset_source = AssetSource::artifact(asset.clone());

    let result = registry
        .apply_mutation(
            &fs,
            OverlayMutation::SetArtifactBody {
                artifact: asset.clone(),
                edit: AssetOverlay::ReplaceBody(
                    b"void main() { gl_FragColor = vec4(1.0); }".to_vec(),
                ),
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    assert_eq!(
        result.changes.assets.changed,
        vec![lpc_model::AssetChange::new(
            asset_source.clone(),
            AssetChangeKind::Body
        )]
    );
    assert_eq!(
        registry.asset(&asset_source).unwrap().state,
        AssetState::Available {
            source: AssetBodySource::OverlayReplace
        }
    );
}

#[test]
fn discard_overlay_returns_inventory_to_committed_state() {
    let (fs, shapes, mut registry) = shader_project();
    let ctx = parse_ctx(&shapes);
    let asset = ArtifactLocation::file("/shader.glsl");
    let asset_source = AssetSource::artifact(asset.clone());

    registry
        .apply_mutation(
            &fs,
            OverlayMutation::SetArtifactBody {
                artifact: asset.clone(),
                edit: AssetOverlay::Delete,
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();
    assert_eq!(
        registry.asset(&asset_source).unwrap().state,
        AssetState::Deleted
    );

    let changes = registry.discard_overlay(&fs, Revision::new(3), &ctx);

    assert_eq!(
        changes.assets.changed,
        vec![lpc_model::AssetChange::new(
            asset_source.clone(),
            AssetChangeKind::LeftError
        )]
    );
    assert_eq!(
        registry.asset(&asset_source).unwrap().state,
        AssetState::Available {
            source: AssetBodySource::Committed
        }
    );
}

#[test]
fn commit_overlay_writes_artifact_without_runtime_project_change() {
    let (fs, shapes, mut registry) = shader_project();
    let ctx = parse_ctx(&shapes);
    let asset = ArtifactLocation::file("/shader.glsl");
    let asset_source = AssetSource::artifact(asset.clone());
    let body = b"void main() { gl_FragColor = vec4(0.5); }".to_vec();

    registry
        .apply_mutation(
            &fs,
            OverlayMutation::SetArtifactBody {
                artifact: asset.clone(),
                edit: AssetOverlay::ReplaceBody(body.clone()),
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    let result = registry
        .commit_overlay(&fs, Revision::new(3), &ctx)
        .unwrap();

    assert_eq!(result.artifacts.changed, vec![asset.clone()]);
    assert!(result.changes.is_empty());
    assert_eq!(fs.read_file(LpPath::new("/shader.glsl")).unwrap(), body);
    assert_eq!(
        registry.asset(&asset_source).unwrap().state,
        AssetState::Available {
            source: AssetBodySource::Committed
        }
    );
}

#[test]
fn commit_slot_overlay_writes_effective_node_def() {
    let (fs, shapes, mut registry) = clock_project();
    let ctx = parse_ctx(&shapes);
    let clock = ArtifactLocation::file("/clock.toml");

    registry
        .apply_mutation(
            &fs,
            OverlayMutation::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("controls.rate").unwrap(),
                    LpValue::F32(2.0),
                ),
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    let result = registry
        .commit_overlay(&fs, Revision::new(3), &ctx)
        .unwrap();

    let text = String::from_utf8(fs.read_file(LpPath::new("/clock.toml")).unwrap()).unwrap();
    assert_eq!(result.artifacts.changed, vec![clock]);
    assert!(result.changes.is_empty());
    assert!(text.contains("rate = 2"));
}

#[test]
fn refresh_artifacts_returns_runtime_asset_changes() {
    let (mut fs, shapes, mut registry) = shader_project();
    let ctx = parse_ctx(&shapes);
    let asset = ArtifactLocation::file("/shader.glsl");
    let asset_source = AssetSource::artifact(asset.clone());
    write_file(
        &mut fs,
        "/shader.glsl",
        "void main() { gl_FragColor = vec4(0.25); }",
    );

    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/shader.glsl"),
            kind: FsEventKind::Modify,
        }],
        Revision::new(2),
        &ctx,
    );

    assert_eq!(
        changes.assets.changed,
        vec![lpc_model::AssetChange::new(
            asset_source,
            AssetChangeKind::Body
        )]
    );
}
