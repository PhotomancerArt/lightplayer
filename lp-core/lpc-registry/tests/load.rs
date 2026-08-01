use lpc_model::{
    ArtifactLocation, AssetBodyOrigin, AssetLocation, AssetState, NodeDefLocation, NodeDefState,
    PROJECT_FORMAT_VERSION, Revision, SlotShapeRegistry,
};
use lpc_registry::{ParseCtx, ProjectRegistry, RegistryError};
use lpfs::{LpFsMemory, LpPath};

fn parse_ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
    ParseCtx { shapes }
}

fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
    fs.write_file_mut(LpPath::new(path), contents.as_bytes())
        .unwrap();
}

#[test]
fn load_root_discovers_root_external_and_asset_entries() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    },
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
    );
    write_file(
        &mut fs,
        "/clock.json",
        r#"
{
  "kind": "Clock"
}
"#,
    );
    write_file(
        &mut fs,
        "/shader.json",
        r#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  },
  "render_order": 0
}
"#,
    );
    write_file(&mut fs, "/shader.glsl", "void main() {}");

    let mut registry = ProjectRegistry::new();
    let result = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .unwrap();

    let root = NodeDefLocation::artifact_root(ArtifactLocation::file("/module.json"));
    let shader = NodeDefLocation::artifact_root(ArtifactLocation::file("/shader.json"));
    let clock = NodeDefLocation::artifact_root(ArtifactLocation::file("/clock.json"));
    let shader_asset = AssetLocation::artifact(ArtifactLocation::file("/shader.glsl"));

    assert_eq!(result.root, root);
    assert!(result.changes.assets.changed.is_empty());
    assert!(result.changes.assets.removed.is_empty());
    assert_eq!(registry.inventory().defs.len(), 3);
    assert!(matches!(
        registry.def(&root).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Module(_))
    ));
    assert!(matches!(
        registry.def(&shader).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Shader(_))
    ));
    assert!(matches!(
        registry.def(&clock).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Clock(_))
    ));
    assert_eq!(
        registry.asset(&shader_asset).unwrap().state,
        AssetState::Available {
            origin: AssetBodyOrigin::Committed
        }
    );
    assert_eq!(result.changes.defs.added.len(), 3);
    assert_eq!(result.changes.assets.added, vec![shader_asset]);
}

#[test]
fn load_root_reports_parse_error_for_inline_child_def() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "shader": {
      "def": {
        "kind": "Shader",
        "source": "shader.glsl"
      }
    }
  }
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    let result = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect("load records the parse error as a def entry");

    let root = NodeDefLocation::artifact_root(ArtifactLocation::file("/module.json"));
    assert_eq!(result.root, root);
    let state = &registry.def(&root).unwrap().state;
    let NodeDefState::ParseError(err) = state else {
        panic!("expected parse error for inline child def, got {state:?}");
    };
    assert!(format!("{err}").contains("def"), "{err}");
}
#[test]
fn load_root_keeps_missing_referenced_def_as_error_entry() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "shader": {
      "ref": "./missing.json"
    }
  }
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .unwrap();

    let missing = NodeDefLocation::artifact_root(ArtifactLocation::file("/missing.json"));
    assert_eq!(
        registry.def(&missing).map(|entry| &entry.state),
        Some(&NodeDefState::NotFound)
    );
}

#[test]
fn load_root_keeps_missing_referenced_asset_as_error_entry() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    }
  }
}
"#,
    );
    write_file(
        &mut fs,
        "/shader.json",
        r#"
{
  "kind": "Shader",
  "source": {
    "path": "missing.glsl"
  }
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .unwrap();

    let missing = AssetLocation::artifact(ArtifactLocation::file("/missing.glsl"));
    assert_eq!(
        registry.asset(&missing).map(|entry| &entry.state),
        Some(&AssetState::NotFound)
    );
}

#[test]
fn load_root_accepts_current_project_format() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {}
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    let result = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect("current format loads");

    let root = NodeDefLocation::artifact_root(ArtifactLocation::file("/module.json"));
    assert_eq!(result.root, root);
    assert!(matches!(
        registry.def(&root).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Module(_))
    ));
}

#[test]
fn load_root_rejects_missing_manifest_format() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"name\": \"x\"\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {}
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    let err = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect_err("missing format must be rejected");

    assert_eq!(
        err,
        RegistryError::FormatVersion {
            expected: PROJECT_FORMAT_VERSION,
            found: None,
        }
    );
    assert!(err.to_string().contains("regenerate"), "{err}");
}

#[test]
fn vendored_module_folder_loads_under_the_projects_gate() {
    // Q10 disposition: format is a container-level concept. A copied/
    // vendored module folder inside a project carries no project.json of
    // its own — it is gated by the HOST project's container manifest, and
    // the loader never re-runs the gate for child artifacts.
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 3\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {
    "plasma": { "ref": "./modules/plasma/module.json" }
  }
}
"#,
    );
    write_file(
        &mut fs,
        "/modules/plasma/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {}
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect("vendored module folder loads under the host gate");

    let child =
        NodeDefLocation::artifact_root(ArtifactLocation::file("/modules/plasma/module.json"));
    assert!(
        matches!(
            registry.def(&child).expect("child def entry").state,
            NodeDefState::Loaded(lpc_model::NodeDef::Module(_))
        ),
        "vendored module def must load without its own manifest"
    );
}

#[test]
fn load_root_rejects_missing_container_manifest() {
    // D-A: a project with no `project.json` container manifest is a HARD
    // refuse — the manifest carries the format gate, so skipping it would
    // let unversioned projects load ungated.
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {}
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    let err = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect_err("missing container manifest must be rejected");

    assert!(
        matches!(err, RegistryError::Manifest { .. }),
        "expected a manifest error, got {err:?}"
    );
    assert!(err.to_string().contains("project.json"), "{err}");
}

#[test]
fn load_root_rejects_pre_mitosis_kind_tagged_manifest() {
    // A format-2 root (single-file, kind-tagged `project.json`) must fail
    // with a diagnosable manifest error, not a deep parse failure.
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.json",
        r#"{ "kind": "Module", "format": 2, "nodes": {} }"#,
    );

    let mut registry = ProjectRegistry::new();
    let err = registry
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
        .expect_err("pre-mitosis root must be rejected");

    assert!(
        matches!(err, RegistryError::Manifest { .. }),
        "expected a manifest error, got {err:?}"
    );
    assert!(err.to_string().contains("kind"), "{err}");
}

#[test]
fn load_root_rejects_mismatched_project_format() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(&mut fs, "/project.json", "{\n  \"format\": 999\n}\n");
    write_file(
        &mut fs,
        "/module.json",
        r#"
{
  "kind": "Module",
  "nodes": {}
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    let err = registry
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(1), &ctx)
        .expect_err("mismatched format must be rejected");

    assert_eq!(
        err,
        RegistryError::FormatVersion {
            expected: PROJECT_FORMAT_VERSION,
            found: Some(999),
        }
    );
    assert!(err.to_string().contains("999"), "{err}");
}
