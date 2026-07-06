use std::collections::BTreeMap;

use lpc_model::{
    ArtifactLocation, AssetBodyOverlay, AssetLocation, AssetState, MutationOp, NodeDefLocation,
    Revision, SlotShapeRegistry,
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

#[derive(Default)]
struct FakeRuntime {
    nodes: BTreeMap<NodeDefLocation, RuntimeNodeState>,
    assets: BTreeMap<AssetLocation, RuntimeAssetState>,
}

#[derive(Clone, Debug, PartialEq)]
struct RuntimeNodeState {
    revision: Revision,
    loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeAssetState {
    revision: Revision,
    available: bool,
}

impl FakeRuntime {
    fn apply(&mut self, registry: &ProjectRegistry, changes: &lpc_model::ProjectChangeSummary) {
        for location in &changes.defs.removed {
            self.nodes.remove(location);
        }
        for location in &changes.assets.removed {
            self.assets.remove(location);
        }
        for location in &changes.defs.added {
            self.load_node(registry, location);
        }
        for change in &changes.defs.changed {
            self.load_node(registry, &change.location);
        }
        for location in &changes.assets.added {
            self.load_asset(registry, location);
        }
        for change in &changes.assets.changed {
            self.load_asset(registry, &change.location);
        }
    }

    fn load_node(&mut self, registry: &ProjectRegistry, location: &NodeDefLocation) {
        let entry = registry.def(location).expect("node entry");
        self.nodes.insert(
            location.clone(),
            RuntimeNodeState {
                revision: entry.revision,
                loaded: entry.state.is_loaded(),
            },
        );
    }

    fn load_asset(&mut self, registry: &ProjectRegistry, source: &AssetLocation) {
        let entry = registry.asset(source).expect("asset entry");
        self.assets.insert(
            source.clone(),
            RuntimeAssetState {
                revision: entry.revision,
                available: entry.state.is_available(),
            },
        );
    }
}

#[test]
fn fake_runtime_consumes_load_apply_and_commit_change_summaries() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/project.json",
        r#"
{
  "kind": "Project",
  "format": 1,
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
    "path": "shader.glsl"
  }
}
"#,
    );
    write_file(&mut fs, "/shader.glsl", "void main() {}");

    let mut registry = ProjectRegistry::new();
    let load = registry
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(1), &ctx)
        .unwrap();
    let mut runtime = FakeRuntime::default();
    runtime.apply(&registry, &load.changes);
    assert_eq!(runtime.nodes.len(), 2);
    assert_eq!(runtime.assets.len(), 1);

    let asset = ArtifactLocation::file("/shader.glsl");
    let asset_source = AssetLocation::artifact(asset.clone());
    let apply = registry
        .mutate(
            &fs,
            MutationOp::SetArtifactBody {
                artifact: asset.clone(),
                edit: AssetBodyOverlay::ReplaceBody(b"void main() { }".to_vec()),
            },
            Revision::new(2),
            &ctx,
        )
        .unwrap();
    runtime.apply(&registry, &apply.changes);
    assert_eq!(
        runtime.assets.get(&asset_source).unwrap().revision,
        Revision::new(2)
    );
    assert_eq!(
        registry.asset(&asset_source).unwrap().state,
        AssetState::Available {
            origin: lpc_model::AssetBodyOrigin::OverlayReplace
        }
    );

    let commit = registry
        .commit_overlay(&fs, Revision::new(3), &ctx)
        .unwrap();
    assert_eq!(commit.artifact_changes.changed, vec![asset]);
}
