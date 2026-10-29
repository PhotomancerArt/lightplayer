//! The format-6 test projects under `projects/test/` (mapping rebuild M2
//! P4), loaded from disk the way a device loads them, rendered by their own
//! shaders through the real engine.
//!
//! - `circular-display-6` lands as its journey ends: every lamp where
//!   [`Patching::landing`], computed here from the same file, says.
//! - `shader-oracle-6`, `lanes-6x128-6` and `segments-64-6` light every
//!   output exactly as their format ≤ 5 twins do, sample for sample.
//! - Every format-6 document there is in the writer's canonical form.
//!
//! **The permutation oracle** (as `parts_patch_tests.rs`): lamp k's colors
//! are read from the fixture's own control product (a control-product probe,
//! the fixture's lamp order, untouched by any patching) and checked lit and
//! pairwise distinct, so a placement can't pass by a color collision; then
//! every wire lamp is checked against the landing, and every lamp of the
//! fixture lit on the wire is accounted for.

extern crate std;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use std::path::{Path, PathBuf};

use lpc_mapping::map2d6;
use lpc_mapping::part::render;
use lpc_mapping::patching::{OutputName, OutputPorts, Port, PortKey, patch_mapping};
use lpc_model::{ControlExtent, ControlProduct, NodeId, NodeName, NodeRuntimeStatus, TreePath};
use lpc_wire::{
    ControlProductProbeRequest, ControlProductProbeResult, ProjectProbeRequest, ProjectProbeResult,
    ProjectReadRequest, RevisionGateRead, WireChannelSampleFormat,
};
use lpfs::LpFsStd;

use super::test_support::read_probe_results;
use super::{EngineServices, LoadedProjectRuntime, ProjectLoader};
use crate::node::NodeEntryState;

/// `circular-display-6` lands as its journey ends: rings r1 and r2 on port
/// 0 from lamp 0, r3 reversed on port 1 from lamp 0, r4 and the centre
/// following it on port 1.
#[test]
fn the_circular_display_lands_as_its_journey_ends() {
    let mut project = Project::load("circular-display-6");
    let text = project.read("fixture.map2d.json");
    let mapping = map2d6::read(&text).expect("format 6");
    let tree = render(&mapping).expect("render");
    // `output.json`: output `main`, two ports of 64 lamps.
    let ports = vec![OutputPorts {
        output: OutputName::new("main"),
        ports: vec![
            Port {
                key: PortKey(0),
                channels: Some(64 * 3),
            },
            Port {
                key: PortKey(1),
                channels: Some(64 * 3),
            },
        ],
    }];
    let patching = patch_mapping(&mapping, &tree, &ports);
    assert!(
        patching.problems.is_empty(),
        "the journey's patching has no problems: {:?}",
        patching.problems
    );

    let own = project.control_product("fixture", tree.positions().len() as u32);
    let wire = project.published("output");
    let mut placed = 0;
    for segment in tree.segments() {
        for j in 0..segment.count() {
            let k = (segment.lamps.start + j) as usize;
            let address = patching
                .landing(&segment.id, j)
                .unwrap_or_else(|| panic!("lamp {j} of {} lands", segment.id));
            let wire_lamp = (address.port.0 * 64 * 3 + address.channel) / 3;
            assert_eq!(
                wire.get(wire_lamp as usize),
                Some(&own[k]),
                "lamp {j} of {} (fixture lamp {k}) belongs at wire lamp {wire_lamp}",
                segment.id
            );
            placed += 1;
        }
    }
    let lit = wire.iter().filter(|lamp| own.contains(lamp)).count();
    assert_eq!(lit, placed, "nothing lit that the patcher didn't place");
    assert_eq!(placed, 61, "every lamp of the display");

    // The journey's own claims, by address.
    let at = |segment: &str, j: u32| {
        let address = patching
            .landing(&lpc_mapping::part::TreeId::parse(segment).expect("id"), j)
            .expect("lands");
        (address.port.0, address.channel / 3)
    };
    assert_eq!(at("circle1/r1", 0), (0, 0));
    assert_eq!(at("circle1/r2", 0), (0, 24));
    assert_eq!(at("circle1/r3", 0), (1, 11), "r3 reversed");
    assert_eq!(at("circle1/r3", 11), (1, 0));
    assert_eq!(at("circle1/r4", 0), (1, 12));
    assert_eq!(at("circle1/r5", 0), (1, 20));
    assert_eq!(project.status("fixture"), None);
}

/// `shader-oracle-6` lights its output exactly as `shader-oracle` does.
#[test]
fn shader_oracle_6_lights_exactly_as_its_twin() {
    assert_twins("shader-oracle", "shader-oracle-6", &[("output", "output")]);
}

/// `lanes-6x128-6` lights all six outputs exactly as `lanes-6x128` does.
#[test]
fn lanes_6x128_6_lights_exactly_as_its_twin() {
    let outputs: Vec<(String, String)> = (1..=6)
        .map(|n| (format!("output{n}"), format!("output{n}")))
        .collect();
    let outputs: Vec<(&str, &str)> = outputs
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_twins("lanes-6x128", "lanes-6x128-6", &outputs);
}

/// `segments-64-6` (64 segments, placed by its start) lights its output
/// exactly as `segments-64-5` (one strand of 64 instances, auto-flowed)
/// does.
#[test]
fn segments_64_6_lights_exactly_as_its_twin() {
    assert_twins("segments-64-5", "segments-64-6", &[("output", "output")]);
}

/// Every format-6 document under `projects/test/` is in the writer's
/// canonical form: it reads, and writes back byte for byte.
#[test]
fn every_format_6_test_document_is_canonical() {
    let mut seen = 0;
    for project in read_dir_sorted(&projects_test()) {
        for file in read_dir_sorted(&project) {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".map2d.json") {
                continue;
            }
            let text = std::fs::read_to_string(&file).expect("read");
            if map2d6::peek_format(&text) != Ok(map2d6::MAP2D6_FORMAT) {
                continue;
            }
            let mapping = map2d6::read(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            assert_eq!(
                map2d6::write(&mapping).expect("write"),
                text,
                "{} is not in canonical form",
                file.display()
            );
            seen += 1;
        }
    }
    // shader-oracle-6 (1), lanes-6x128-6 (6), circular-display-6 (1),
    // segments-64-6 (1).
    assert!(seen >= 9, "the walk saw {seen} format-6 documents");
}

// --- helpers ----------------------------------------------------------------

/// Load `twin` and `project`, render each for the same frames, and assert
/// each pair of outputs published the same samples (and lit something).
fn assert_twins(twin: &str, project: &str, outputs: &[(&str, &str)]) {
    let twin_rt = Project::load(twin);
    let project_rt = Project::load(project);
    for (twin_output, output) in outputs {
        let expected = twin_rt.published(twin_output);
        let actual = project_rt.published(output);
        assert!(
            expected.iter().any(|lamp| *lamp != [0; 3]),
            "{twin}/{twin_output} lights something"
        );
        assert_eq!(
            actual, expected,
            "{project}/{output} lights exactly as {twin}/{twin_output}"
        );
    }
    for fixture in project_rt.fixtures() {
        assert_eq!(
            project_rt.status(&fixture),
            None,
            "{project}/{fixture} says nothing is wrong"
        );
    }
}

/// Frames rendered before a buffer is read: output names register on the
/// first, a fixture patches against them on the next, and the plan
/// settles by the third.
const FRAMES: usize = 4;

struct Project {
    dir: PathBuf,
    rt: LoadedProjectRuntime,
}

impl Project {
    fn load(name: &str) -> Self {
        let dir = projects_test().join(name);
        let fs = LpFsStd::new(dir.clone());
        let services = EngineServices::new(TreePath::parse("/project.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services)
            .unwrap_or_else(|e| panic!("load {name}: {e:?}"));
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        for _ in 0..FRAMES {
            rt.tick(16).expect("tick");
        }
        Self { dir, rt }
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.dir.join(file)).expect("read")
    }

    fn node(&self, name: &str) -> NodeId {
        let tree = self.rt.tree();
        tree.lookup_sibling(tree.root(), NodeName::parse(name).expect("name"))
            .unwrap_or_else(|| panic!("node {name}"))
    }

    /// Every fixture node's name.
    fn fixtures(&self) -> Vec<String> {
        let tree = self.rt.tree();
        tree.entries()
            .filter(|entry| entry.path.to_string().ends_with(".fixture"))
            .map(|entry| {
                let path = entry.path.to_string();
                let leaf = path.rsplit('/').next().unwrap_or("");
                String::from(leaf.trim_end_matches(".fixture"))
            })
            .collect()
    }

    /// An output's published samples, one entry per RGB lamp.
    fn published(&self, output: &str) -> Vec<[u16; 3]> {
        let id = self.node(output);
        let buffer = self
            .rt
            .engine()
            .runtime_output_sink_buffer_id(id)
            .unwrap_or_else(|| panic!("{output}: sink buffer"));
        self.rt
            .engine()
            .runtime_buffers()
            .get(buffer)
            .expect("sink buffer")
            .value()
            .samples16()
            .expect("u16 samples")
            .chunks_exact(3)
            .map(|lamp| [lamp[0], lamp[1], lamp[2]])
            .collect()
    }

    /// A fixture's own control product: lamp k's samples, in the fixture's
    /// lamp order. Checked lit and pairwise distinct.
    fn control_product(&mut self, fixture: &str, lamps: u32) -> Vec<[u16; 3]> {
        let id = self.node(fixture);
        let (engine, registry) = self.rt.split_mut();
        let results = read_probe_results(
            engine,
            registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product: ControlProduct::new(id, 0, ControlExtent::new(1, lamps * 3)),
                        sample_format: WireChannelSampleFormat::U16,
                        geometry: RevisionGateRead::Always,
                    },
                )],
            },
        );
        let Some(ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            bytes,
            ..
        })) = results.first()
        else {
            panic!("{fixture}: a control preview, got {results:?}");
        };
        let colors: Vec<[u16; 3]> = bytes
            .chunks_exact(6)
            .map(|lamp| {
                let sample = |i: usize| u16::from_le_bytes([lamp[2 * i], lamp[2 * i + 1]]);
                [sample(0), sample(1), sample(2)]
            })
            .collect();
        assert_eq!(
            colors.len(),
            lamps as usize,
            "{fixture}: one color per lamp"
        );
        let mut seen: BTreeMap<[u16; 3], usize> = BTreeMap::new();
        for (k, lamp) in colors.iter().enumerate() {
            assert_ne!(*lamp, [0; 3], "{fixture}: lamp {k} is dark");
            if let Some(other) = seen.insert(*lamp, k) {
                panic!("{fixture}: lamps {other} and {k} share a color");
            }
        }
        colors
    }

    fn status(&self, fixture: &str) -> Option<NodeRuntimeStatus> {
        let entry = self.rt.tree().get(self.node(fixture)).expect("entry");
        let NodeEntryState::Alive(runtime) = entry.state.value() else {
            panic!("{fixture} alive");
        };
        runtime.runtime_status()
    }
}

fn projects_test() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/test")
}

fn read_dir_sorted(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries
}
