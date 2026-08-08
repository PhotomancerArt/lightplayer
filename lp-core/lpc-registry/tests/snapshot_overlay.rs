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
        LpPathBuf::from("/module.json"),
        br#"
{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#
        .to_vec(),
    );
    target.insert(
        LpPathBuf::from("/clock.json"),
        br#"
{
  "kind": "Clock"
}
"#
        .to_vec(),
    );

    let overlay = derive_overlay_between_snapshots(&base, &target);
    let mut fs = LpFsMemory::new();
    // The container manifest is not a node artifact, so it rides beside the
    // snapshot-derived files rather than through the overlay.
    fs.write_file_mut(LpPath::new("/project.json"), b"{\n  \"format\": 6\n}\n")
        .unwrap();
    let fs = fs;
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
        .load_root(&fs, LpPath::new("/module.json"), Revision::new(3), &ctx)
        .unwrap();
    assert_eq!(loaded.inventory().defs.len(), 2);
}
