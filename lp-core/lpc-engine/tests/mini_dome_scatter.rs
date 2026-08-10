//! The mini-dome's shipped patch files, proven against the engine: the
//! as-built permutation — five sectors and three door panels scattered
//! across two named outputs with shared ports, reversal and stride-stepped
//! rotation included — lands exactly where the hand-authored rows say.
//!
//! The expectations below are hand-computed from the port tables
//! (`out_a` = 39 + 30 + 39 lamps, `out_b` = 39 + 30) and the patch rows,
//! stated in SAMPLES (3 per lamp) as fragment placements. A failure here
//! means either the example's files or the scatter engine moved.

use lpc_engine::nodes::OutputFragment;
use lpc_engine::{EngineServices, ProjectLoader};
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

#[test]
fn the_shipped_mini_dome_permutation_places_every_run_where_authored() {
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf();
    let project_dir: PathBuf = workspace_dir.join("examples/mini-dome");
    let fs = LpFsStd::new(project_dir);
    let services = EngineServices::new(TreePath::parse("/mini_dome.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load mini-dome");
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

    // out_a ("1", ports 39 + 30 + 39): sector 2 rotated 10 lamps at port 0,
    // sector 4 at port 1 (wire 39), sector 0 at port 2 (wire 69), then the
    // doors interleaved into the port tails — door 0 at wire 30, door 2 at
    // wire 99. Samples = lamps × 3.
    assert_eq!(
        fragments_of("out_a.output"),
        vec![
            // dome: /sector/0 → wire 78 · /sector/2 → wire 0, offset 10
            (0, 207, 90, false, 0),
            (180, 0, 90, false, 30),
            (360, 117, 90, false, 0),
            // dome fragments ride in patch-document row order per producer.
            // doors: /door/0 → wire 30 · /door/2 → wire 99
            (0, 90, 27, false, 0),
            (54, 297, 27, false, 0),
        ],
    );

    // out_b ("Box 2", ports 39 + 30): sector 1 reversed at port 0, sector 3
    // at port 1 (wire 39), door 1 rotated one SIDE (3 lamps) at wire 30.
    assert_eq!(
        fragments_of("out_b.output"),
        vec![
            (90, 0, 90, true, 0),
            (270, 117, 90, false, 0),
            (27, 90, 27, false, 9),
        ],
    );
}
