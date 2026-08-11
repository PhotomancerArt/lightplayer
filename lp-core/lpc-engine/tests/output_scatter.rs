//! D40 — the scatter rule, end to end: a fixture's patch entries naming
//! different outputs split its fragments across those outputs; unaddressed
//! runs (and wholly-unpatched producers) flow to the DEFAULT output — the
//! first fragments-consuming output in attach order; a producer with zero
//! runs on an output is NOT a gap there; rotation is real in rendering.
//!
//! The topology under test is the mini-dome's shape in miniature: TWO
//! producers (`dome`, 6 lamps · `doors`, 4 lamps) scattered across TWO
//! outputs (`output`, unnamed = the default · `output2`, named "Box 2")
//! with interleaved anchors — many-to-many, the case the peach
//! (many-to-one) and zook (one-to-many) leave uncovered. Expectations are
//! stated byte-exactly against the auto-flow baseline, the same discipline
//! `output_patch_reflow.rs` uses.

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, TreePath};
use lpc_registry::ProjectRegistry;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const DOME_LAMPS: u32 = 6;
const DOORS_LAMPS: u32 = 4;

/// Ticks before a buffer is read: the first frame establishes extents, and
/// output identities settle one tick after registration.
const SETTLE_TICKS: usize = 3;

fn project_fs(dome_patch: Option<&str>, doors_patch: Option<&str>) -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 10\n}\n")
        .expect("container manifest");
    fs.write_file(
        "/module.json".as_path(),
        br#"{ "kind": "Module", "nodes": {
  "dome": { "ref": "./dome.json" },
  "doors": { "ref": "./doors.json" },
  "output": { "ref": "./output.json" },
  "output2": { "ref": "./output2.json" }
} }"#,
    )
    .expect("module.json");
    fs.write_file(
        "/output.json".as_path(),
        br#"{
  "kind": "Output",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    )
    .expect("output.json");
    fs.write_file(
        "/output2.json".as_path(),
        br#"{
  "kind": "Output",
  "name": "Box 2",
  "ports": { "0": { "endpoint": "ws281x:local:D11" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    )
    .expect("output2.json");
    write_fixture(&fs, "dome", DOME_LAMPS, 1.0, dome_patch.is_some());
    write_fixture(&fs, "doors", DOORS_LAMPS, 0.25, doors_patch.is_some());
    if let Some(patch) = dome_patch {
        fs.write_file("/dome.patch.json".as_path(), patch.as_bytes())
            .expect("dome patch");
    }
    if let Some(patch) = doors_patch {
        fs.write_file("/doors.patch.json".as_path(), patch.as_bytes())
            .expect("doors patch");
    }
    fs
}

fn write_fixture(fs: &LpFsMemory, name: &str, lamps: u32, brightness: f32, patched: bool) {
    let patch = if patched {
        format!(",\n  \"patch\": {{ \"kind\": \"File\", \"source\": \"{name}.patch.json\" }}")
    } else {
        String::new()
    };
    fs.write_file(
        format!("/{name}.json").as_path(),
        format!(
            r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": {lamps}, "height": 1 }},
  "bindings": {{ "output": {{ "target": "bus:control.out" }} }},
  "sampling": "direct",
  "diagnostic_mode": "led_index",
  "mapping": {{ "kind": "Map2d", "source": "{name}.map2d.json" }}{patch},
  "color_order": "rgb",
  "brightness": {brightness},
  "gamma_correction": false
}}"#
        )
        .as_bytes(),
    )
    .expect("fixture def");
    fs.write_file(
        format!("/{name}.map2d.json").as_path(),
        format!(
            r#"{{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, {lamps}.0, 1.0],
  "objects": [
    {{ "name": "strip", "shape": {{ "grid": {{ "origin": [0.5, 0.5], "cols": {lamps}, "rows": 1, "pitch": 1 }} }} }}
  ]
}}"#
        )
        .as_bytes(),
    )
    .expect("fixture map2d");
}

fn settled(fs: &LpFsMemory) -> (Engine, ProjectRegistry) {
    let services = EngineServices::new(TreePath::parse("/scatter.show").expect("path"));
    let (mut engine, registry) = ProjectLoader::load_from_root(fs, services)
        .expect("load scatter project")
        .into_parts();
    for _ in 0..SETTLE_TICKS {
        engine.tick(&registry, 16).expect("tick");
    }
    (engine, registry)
}

/// The published samples of the output whose tree path ends in `suffix`,
/// grouped one entry per RGB lamp.
fn published_lamps(engine: &Engine, suffix: &str) -> Vec<[u16; 3]> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with(suffix))
        .unwrap_or_else(|| panic!("{suffix} node in tree"));
    let buffer_id = engine
        .runtime_output_sink_buffer_id(entry.id)
        .expect("output sink buffer");
    engine
        .runtime_buffers()
        .get(buffer_id)
        .expect("sink buffer")
        .value()
        .bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<u16>>()
        .chunks_exact(3)
        .map(|lamp| [lamp[0], lamp[1], lamp[2]])
        .collect()
}

fn node_status(engine: &Engine, suffix: &str) -> Option<NodeRuntimeStatus> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with(suffix))
        .unwrap_or_else(|| panic!("{suffix} node in tree"));
    let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
        panic!("{suffix} node alive");
    };
    node.runtime_status()
}

/// The auto-flow baseline every scatter expectation is a permutation of:
/// dome's six lamps then doors' four, all on the default output.
fn baseline() -> Vec<[u16; 3]> {
    let (engine, _registry) = settled(&project_fs(None, None));
    let lamps = published_lamps(&engine, "output.output");
    assert_eq!(lamps.len(), (DOME_LAMPS + DOORS_LAMPS) as usize);
    lamps
}

/// The many-to-many case (Yona steer 2): two producers, two outputs, both
/// scattered, interleaved anchors — byte-exact on BOTH wires, reversal and
/// rotation included.
///
/// dome:  lamps 0-2 → default @0 · lamps 3-5 → "Box 2" @2, reversed
/// doors: lamps 0-1 → default @3 · lamps 2-3 → "Box 2" @0, rotated 1
#[test]
fn many_to_many_scatter_is_byte_exact_on_both_outputs() {
    let dome = r#"{
  "format": 2,
  "outputs": ["Box 2"],
  "entries": [
    [[0, 3], -1, 0],
    [[3, 3], 0, 2, "r"]
  ]
}
"#;
    let doors = r#"{
  "format": 2,
  "outputs": ["Box 2"],
  "entries": [
    [[0, 2], -1, 3],
    [[2, 2], 0, 0, "", 1]
  ]
}
"#;
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(Some(dome), Some(doors)));

    // Default output: dome 0-2 at wire 0-2, doors 0-1 at wire 3-4.
    assert_eq!(
        published_lamps(&engine, "output.output"),
        vec![flowed[0], flowed[1], flowed[2], flowed[6], flowed[7]],
        "the default output carries the runs addressed to nobody"
    );
    // "Box 2": doors 2-3 rotated one lamp within their window (rotate right:
    // [d2, d3] → [d3, d2]), then dome 3-5 laid down end-first.
    assert_eq!(
        published_lamps(&engine, "output2.output"),
        vec![flowed[9], flowed[8], flowed[5], flowed[4], flowed[3]],
        "the named output carries its interleaved, reversed, rotated runs"
    );
    assert_eq!(node_status(&engine, "output.output"), None);
    assert_eq!(node_status(&engine, "output2.output"), None);
}

/// D40's default rule, with the ORDER pinned: an unpatched producer flows
/// onto the FIRST fragments-consuming output in attach order (module
/// `nodes` key order — `output` before `output2`), and the second output
/// carries nothing without warning about it.
#[test]
fn an_unpatched_producer_flows_to_the_first_output_only() {
    let (engine, _registry) = settled(&project_fs(None, None));

    let first = published_lamps(&engine, "output.output");
    assert_eq!(first.len(), (DOME_LAMPS + DOORS_LAMPS) as usize);
    assert_eq!(
        published_lamps(&engine, "output2.output"),
        Vec::<[u16; 3]>::new(),
        "the non-default output receives no auto-flow"
    );
    assert_eq!(
        node_status(&engine, "output2.output"),
        None,
        "zero runs here is not a gap"
    );
}

/// A producer patched WHOLLY onto the other output contributes no fragment
/// and no gap to the default output; its neighbour keeps auto-flowing.
#[test]
fn a_fully_scattered_producer_leaves_no_gap_behind() {
    let doors = r#"{
  "format": 2,
  "outputs": ["Box 2"],
  "entries": [
    [[0], 0, 0]
  ]
}
"#;
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(None, Some(doors)));

    assert_eq!(
        published_lamps(&engine, "output.output"),
        flowed[..DOME_LAMPS as usize].to_vec(),
        "the dome auto-flows alone on the default output"
    );
    assert_eq!(
        published_lamps(&engine, "output2.output"),
        flowed[DOME_LAMPS as usize..].to_vec(),
        "the doors land whole on the named output"
    );
    assert_eq!(
        node_status(&engine, "output.output"),
        None,
        "the doors' absence from the default output is not a gap"
    );
}

/// Rotation in rendering: the kernel's `(j' + k) mod N`, applied to real
/// buffers — forward and composed with reversal (reverse first, then
/// rotate; the kernel's worked example at N = 5 is hand-checked here at
/// N = 6 and N = 4).
#[test]
fn rotation_permutes_lamps_within_the_run_window() {
    let dome = r#"{
  "format": 2,
  "entries": [
    [[0], -1, 0, "", 2]
  ]
}
"#;
    let doors = r#"{
  "format": 2,
  "entries": [
    [[0], -1, 6, "r", 1]
  ]
}
"#;
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(Some(dome), Some(doors)));

    let lamps = published_lamps(&engine, "output.output");
    // Dome, rotated 2 of 6: window slot (j + 2) mod 6 — a right-rotation.
    assert_eq!(
        lamps[..6],
        [
            flowed[4], flowed[5], flowed[0], flowed[1], flowed[2], flowed[3]
        ],
        "forward rotation is a right-rotation of the window"
    );
    // Doors, reversed THEN rotated 1 of 4: [d3,d2,d1,d0] → [d0,d3,d2,d1].
    assert_eq!(
        lamps[6..],
        [flowed[6], flowed[9], flowed[8], flowed[7]],
        "reversal applies before rotation — the kernel's canonical order"
    );
}

/// A run naming an output nobody carries degrades per RUN: the rest of the
/// document stands, the lamps are unplaced, and the fixture's status names
/// the missing output.
#[test]
fn a_dangling_output_name_degrades_and_reports() {
    let dome = r#"{
  "format": 2,
  "outputs": ["Box 9"],
  "entries": [
    [[0, 3], -1, 0],
    [[3, 3], 0, 0]
  ]
}
"#;
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(Some(dome), None));

    let Some(NodeRuntimeStatus::Error(message)) = node_status(&engine, "dome.fixture") else {
        panic!("the dangling name is the fixture's error");
    };
    assert!(message.contains("Box 9"), "{message}");
    // The placed half stands; the dangling half is unplaced (its lamps are
    // claimed by the document, so they do not reflow — dark, not moved).
    let lamps = published_lamps(&engine, "output.output");
    assert_eq!(
        lamps[..3],
        flowed[..3],
        "the well-addressed run still lights"
    );
    assert_eq!(
        published_lamps(&engine, "output2.output"),
        Vec::<[u16; 3]>::new(),
        "nothing lands on the innocent named output"
    );
}

/// Two outputs claiming one name make `at.output` ambiguous: BOTH wear an
/// error and routing stays exact-match (nothing is guessed onto either).
#[test]
fn duplicate_output_names_error_both_outputs() {
    let mut fs = project_fs(None, None);
    fs.write_file_mut(
        "/output.json".as_path(),
        br#"{
  "kind": "Output",
  "name": "Box 2",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    )
    .expect("renamed output");
    let (engine, _registry) = settled(&fs);

    for suffix in ["output.output", "output2.output"] {
        let Some(NodeRuntimeStatus::Error(message)) = node_status(&engine, suffix) else {
            panic!("{suffix} wears the duplicate-name error");
        };
        assert!(message.contains("Box 2"), "{suffix}: {message}");
    }
}
