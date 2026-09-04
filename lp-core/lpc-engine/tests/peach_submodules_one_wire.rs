//! The peach's shape as a PROJECT: two submodules, one wire.
//!
//! `body/` and `leaf/` each own a shader and a fixture wired over their own
//! scope's bare `visual.out`, and each exports its scope's `control.out` to
//! the root — where ONE output merges the two into ONE strand of 56 channels
//! (R4/R7, `docs/design/modules.md`). None of that plumbing is visible in a
//! byte-identity check or a panel walk, and it is the whole reason the
//! example is shaped this way, so it is asserted here: both fixtures reach
//! the wire, at the channels their patch documents name, and the display
//! layout a viewer draws covers every one of their lamps.
//!
//! ```bash
//! cargo test -p lpc-engine --test peach_submodules_one_wire
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::NodeEntryState;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameProbeRequest,
    OutputFrameProbeResult,
};
use lpfs::LpFsStd;

/// Lamps on the strand: the body's 44 and the leaf's 12.
const BODY_LAMPS: u32 = 44;
const LEAF_LAMPS: u32 = 12;
const WIRE_LAMPS: u32 = BODY_LAMPS + LEAF_LAMPS;

/// Ticks before the wire is read: the first frame establishes extents and
/// compiles shaders.
const TICKS: usize = 4;

#[test]
fn both_peaches_merge_both_submodules_onto_one_wire() {
    for example in ["peach-1d", "peach-2d"] {
        let mut rt = load(example);
        for tick in 0..TICKS {
            rt.tick(16)
                .unwrap_or_else(|e| panic!("examples/{example} tick {tick}: {e:?}"));
        }

        let output = output_node(&rt);
        let NodeEntryState::Alive(node) = rt
            .engine()
            .tree()
            .get(output)
            .expect("output entry")
            .state
            .value()
        else {
            panic!("{example}: the output node is not alive");
        };
        let mut placement: Vec<(u32, u32, bool)> = node
            .runtime_output_fragments()
            .iter()
            .map(|fragment| {
                (
                    fragment.offset_samples,
                    fragment.len_samples,
                    fragment.reversed,
                )
            })
            .collect();
        placement.sort_unstable();
        assert_eq!(
            placement,
            vec![(0, 66, false), (66, 36, false), (102, 66, true)],
            "{example}: body 0-21, the leaf between the halves, body 34-55 \
             backwards — two submodules, one strand"
        );

        let bytes = rt
            .engine()
            .runtime_buffers()
            .get(
                rt.engine()
                    .runtime_output_sink_buffer_id(output)
                    .expect("sink buffer"),
            )
            .expect("sink buffer")
            .value()
            .byte_len();
        assert_eq!(
            bytes,
            WIRE_LAMPS as usize * 3 * 2,
            "{example}: 56 lamps of u16 RGB"
        );
    }
}

/// The viewer's side of the same claim: the picture a simulator or a device
/// card paints covers EVERY lamp of BOTH fixtures, each indexed at the wire
/// sample its own colour was rendered into.
#[test]
fn the_published_display_layout_draws_every_lamp_of_both_fixtures() {
    for example in ["peach-1d", "peach-2d"] {
        let mut rt = load(example);
        for tick in 0..TICKS {
            rt.tick(16)
                .unwrap_or_else(|e| panic!("examples/{example} tick {tick}: {e:?}"));
        }

        let (engine, registry) = rt.read_parts();
        let result = engine.read_project_output_frame_probe(
            registry,
            OutputFrameProbeRequest {
                display_layout: ControlDisplayLayoutRead::Always,
            },
        );
        let OutputFrameProbeResult::Frame { outputs } = result;
        let entry = outputs
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{example}: one published output"));
        let ControlDisplayLayoutProbeResult::Layout(lpc_model::ControlDisplayLayout::Layout2d(
            layout,
        )) = entry.display_layout
        else {
            panic!("{example}: the published frame carries no display layout");
        };

        assert_eq!(
            layout.lamps.len(),
            WIRE_LAMPS as usize,
            "{example}: every lamp of both fixtures is drawn"
        );
        assert_eq!(
            layout
                .lamps
                .iter()
                .map(|lamp| lamp.sample_start)
                .collect::<Vec<_>>(),
            (0..WIRE_LAMPS).map(|lamp| lamp * 3).collect::<Vec<_>>(),
            "{example}: one lamp per channel, in wire order, indexed where its \
             own samples landed"
        );
    }
}

fn output_node(rt: &LoadedProjectRuntime) -> lpc_model::NodeId {
    rt.engine()
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree")
        .id
}

fn load(project: &str) -> LoadedProjectRuntime {
    let fs = LpFsStd::new(workspace_dir().join("examples").join(project));
    let root = format!("/{}.show", project.replace(['/', '-'], "_"));
    let services = EngineServices::new(TreePath::parse(&root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load examples/{project}: {e:?}"));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
