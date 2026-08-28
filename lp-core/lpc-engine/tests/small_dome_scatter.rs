//! The small-dome's shipped patch files, proven against the engine: the
//! as-built install — fifty 119-lamp panels and one 360-lamp door
//! scattered across two named outputs with a shared port tail, reversal
//! and stride-stepped rotation included — lands exactly where the
//! generated rows say.
//!
//! At 51 entries the placement table is no longer hand-computable row by
//! row, so the full expectation is DERIVED by resolving the shipped patch
//! documents through `lpc-mapping` (an independent code path from the
//! engine's project-load → fixture → bus → scatter lowering), while the
//! interesting rows — the rotated panel, the reversed panel, the door on
//! the shared port tail — stay pinned as hand-computed literals. A failure
//! here means either the example's files or the scatter engine moved.

use lpc_engine::nodes::OutputFragment;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_mapping::{Map2dDoc, PatchDoc, PatchResolveContext, object_instance_spans, resolve,
    resolve_patch};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `(source_offset, offset, len, reversed, rotation)` in samples — the
/// placement identity of one fragment, product elided (asserted by count
/// and order: dome fragments first, doors after, per producer attach
/// order).
type Placement = (u32, u32, u32, bool, u32);

fn placements(fragments: &[OutputFragment]) -> Vec<Placement> {
    fragments
        .iter()
        .map(|fragment| {
            (
                fragment.source_offset_samples,
                fragment.offset_samples,
                fragment.len_samples,
                fragment.reversed,
                fragment.rotation_samples,
            )
        })
        .collect()
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// One fixture's expected placements per output, derived by resolving its
/// shipped map2d + patch documents through `lpc-mapping`: entry rows lower
/// to `(span.start, wire lamp, span.count, reversed, offset)`, ×3 for
/// samples, in patch-document row order.
fn expected_placements(map2d: &str, patch: &str) -> Vec<(String, Placement)> {
    let doc = Map2dDoc::from_json(&std::fs::read_to_string(map2d).expect("map2d"))
        .expect("map2d parses");
    let patch_doc =
        PatchDoc::from_json(&std::fs::read_to_string(patch).expect("patch")).expect("patch parses");
    let resolved = resolve(&doc).expect("map2d resolves");
    let spans = object_instance_spans(&doc, &resolved);
    let ctx = PatchResolveContext {
        fixture_lamp_count: spans.iter().map(|span| span.count).sum(),
        object_spans: &spans,
        allowed_outputs: None,
        default_output: None,
    };
    let resolution = resolve_patch(&ctx, &patch_doc).expect("patch resolves");
    assert!(
        resolution.refusals.is_empty(),
        "shipped patch refuses: {:?}",
        resolution.refusals
    );
    resolution
        .ranges
        .iter()
        .map(|range| {
            (
                range.output.clone().expect("shipped rows name outputs"),
                (
                    range.start * 3,
                    range.lamp * 3,
                    range.count * 3,
                    range.reversed,
                    range.offset * 3,
                ),
            )
        })
        .collect()
}

#[test]
fn the_shipped_small_dome_install_places_every_run_where_authored() {
    let workspace_dir = workspace_dir();
    let project_dir: PathBuf = workspace_dir.join("examples/small-dome");
    let fs = LpFsStd::new(project_dir.clone());
    let services = EngineServices::new(TreePath::parse("/small_dome.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load small-dome");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    // Identities register on the first tick; placements settle by the third.
    for _ in 0..3 {
        engine.tick(&registry, 16).expect("tick");
    }

    let fragments_of = |suffix: &str| -> Vec<Placement> {
        let entry = engine
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} in tree"));
        let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("{suffix} alive");
        };
        assert_eq!(node.runtime_status(), None, "{suffix} is clean");
        placements(node.runtime_output_fragments())
    };

    // The derived expectation: dome rows in patch-document row order, then
    // the door row, split by the output each row names ("1" = out_a,
    // "Box 2" = out_b) — producer attach order puts dome before doors.
    let dome = expected_placements(
        &project_dir.join("dome/dome.map2d.json").to_string_lossy(),
        &project_dir.join("dome/dome.patch.json").to_string_lossy(),
    );
    let doors = expected_placements(
        &project_dir.join("doors/doors.map2d.json").to_string_lossy(),
        &project_dir.join("doors/doors.patch.json").to_string_lossy(),
    );
    let expect = |output: &str| -> Vec<Placement> {
        dome.iter()
            .chain(doors.iter())
            .filter(|(name, _)| name == output)
            .map(|(_, placement)| *placement)
            .collect()
    };
    let expect_a = expect("1");
    let expect_b = expect("Box 2");

    // The shape of the install: 25 panels + the door on box 1, 25 panels
    // on box 2, every panel one 357-sample run, the door 1080.
    assert_eq!(expect_a.len(), 26, "box 1 carries 25 panels and the door");
    assert_eq!(expect_b.len(), 25, "box 2 carries 25 panels");
    assert_eq!(
        expect_a.iter().map(|p| p.2).sum::<u32>(),
        (25 * 119 + 360) * 3,
        "box 1 wire fully claimed"
    );
    assert_eq!(
        expect_b.iter().map(|p| p.2).sum::<u32>(),
        25 * 119 * 3,
        "box 2 wire fully claimed"
    );

    // The quirk rows, hand-computed so the derivation cannot drift with a
    // resolver bug: `/band-c/0` (object 4, instance 0 → fixture lamp 2380)
    // rotated one panel side (40 lamps) at wire 1071; `/band-b/2` (object
    // 3, instance 2 → lamp 2023) reversed at wire 595; the door (lamp 0)
    // on box 1's port-13 tail at wire 2975, rotated one leg (180 lamps).
    assert!(
        expect_a.contains(&(2380 * 3, 1071 * 3, 357, false, 40 * 3)),
        "rotated panel row missing"
    );
    assert!(
        expect_b.contains(&(2023 * 3, 595 * 3, 357, true, 0)),
        "reversed panel row missing"
    );
    assert_eq!(
        *expect_a.last().expect("door row"),
        (0, 2975 * 3, 1080, false, 180 * 3),
        "door tail row moved"
    );

    // Engine truth: the lowered fragments equal the derived rows exactly.
    assert_eq!(fragments_of("out_a.output"), expect_a);
    assert_eq!(fragments_of("out_b.output"), expect_b);
}
