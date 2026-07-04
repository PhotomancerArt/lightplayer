use lpc_model::{
    ArtifactLocation, AssetBodyOrigin, AssetContentType, AssetLocation, AssetState,
    NodeDefLocation, NodeDefState, Revision, SlotPath, SlotShapeRegistry,
};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{LpFsMemory, LpPath};

fn parse_ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
    ParseCtx { shapes }
}

fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
    fs.write_file_mut(LpPath::new(path), contents.as_bytes())
        .unwrap();
}

#[test]
fn load_root_discovers_root_external_inline_and_asset_entries() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.json",
        r#"
{
  "kind": "Project",
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    },
    "clock": {
      "def": {
        "kind": "Clock"
      }
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
    "path": "shader.glsl"
  },
  "render_order": 0
}
"#,
    );
    write_file(&mut fs, "/shader.glsl", "void main() {}");

    let mut registry = ProjectRegistry::new();
    let result = registry
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
        .unwrap();

    let root = NodeDefLocation::artifact_root(ArtifactLocation::file("/project.json"));
    let shader = NodeDefLocation::artifact_root(ArtifactLocation::file("/shader.json"));
    let inline_clock = NodeDefLocation {
        artifact: ArtifactLocation::file("/project.json"),
        path: SlotPath::parse("nodes[clock]").unwrap(),
    };
    let shader_asset = AssetLocation::artifact(ArtifactLocation::file("/shader.glsl"));

    assert_eq!(result.root, root);
    assert!(result.changes.assets.changed.is_empty());
    assert!(result.changes.assets.removed.is_empty());
    assert_eq!(registry.inventory().defs.len(), 3);
    assert!(matches!(
        registry.def(&root).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Project(_))
    ));
    assert!(matches!(
        registry.def(&shader).unwrap().state,
        NodeDefState::Loaded(lpc_model::NodeDef::Shader(_))
    ));
    assert!(matches!(
        registry.def(&inline_clock).unwrap().state,
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
fn load_root_discovers_inline_source_asset() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.json",
        r#"
{
  "kind": "Project",
  "nodes": {
    "shader": {
      "def": {
        "kind": "Shader",
        "source": {
          "glsl": "void main() {}"
        }
      }
    }
  }
}
"#,
    );

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
        .unwrap();

    let source = AssetLocation::inline(
        NodeDefLocation {
            artifact: ArtifactLocation::file("/project.json"),
            path: SlotPath::parse("nodes[shader]").unwrap(),
        },
        SlotPath::parse("nodes[shader].source").unwrap(),
    );
    let entry = registry.asset(&source).expect("inline source asset");

    assert_eq!(entry.content_type, AssetContentType::ShaderSource);
    assert_eq!(
        entry.state,
        AssetState::Available {
            origin: AssetBodyOrigin::Inline
        }
    );
}

#[test]
fn load_root_keeps_missing_referenced_def_as_error_entry() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.json",
        r#"
{
  "kind": "Project",
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
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
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
    write_file(
        &mut fs,
        "/project.json",
        r#"
{
  "kind": "Project",
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
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
        .unwrap();

    let missing = AssetLocation::artifact(ArtifactLocation::file("/missing.glsl"));
    assert_eq!(
        registry.asset(&missing).map(|entry| &entry.state),
        Some(&AssetState::NotFound)
    );
}
