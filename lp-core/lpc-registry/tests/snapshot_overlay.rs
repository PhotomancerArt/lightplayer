use lpc_model::{ArtifactOverlay, MutationOp, Revision, SlotShapeRegistry};
use lpc_registry::{ParseCtx, ProjectRegistry, ProjectSnapshot, derive_overlay_between_snapshots};
use lpfs::{LpFsMemory, LpPath, LpPathBuf};

fn parse_ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
    ParseCtx { shapes }
}

#[test]
fn snapshot_overlay_can_bootstrap_project_files() {
    let shapes = SlotShapeRegistry::default();
    let ctx = parse_ctx(&shapes);
    let base = ProjectSnapshot::empty();
    let mut target = ProjectSnapshot::empty();
    target.insert(
        LpPathBuf::from("/project.json"),
        br#"
{
  "kind": "Project",
  "nodes": {
    "clock": {
      "def": {
        "kind": "Clock"
      }
    }
  }
}
"#
        .to_vec(),
    );

    let overlay = derive_overlay_between_snapshots(&base, &target);
    let fs = LpFsMemory::new();
    let mut registry = ProjectRegistry::new();
    for (artifact, artifact_overlay) in overlay.iter() {
        let ArtifactOverlay::Asset { overlay: edit } = artifact_overlay else {
            panic!("snapshot overlay should only emit body edits");
        };
        registry
            .mutate(
                &fs,
                MutationOp::SetArtifactBody {
                    artifact: artifact.clone(),
                    edit: edit.clone(),
                },
                Revision::new(1),
                &ctx,
            )
            .unwrap();
    }
    registry
        .commit_overlay(&fs, Revision::new(2), &ctx)
        .unwrap();

    let mut loaded = ProjectRegistry::new();
    loaded
        .load_root(&fs, LpPath::new("/project.json"), Revision::new(3), &ctx)
        .unwrap();
    assert_eq!(loaded.inventory().defs.len(), 2);
}
