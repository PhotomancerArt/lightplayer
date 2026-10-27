//! D40 — the scatter rule, end to end: a fixture's parts landing on
//! different outputs split its fragments across those outputs; a producer
//! with zero runs on an output is NOT a gap there; rotation is real in
//! rendering.
//!
//! The topology under test is the small-dome's shape in miniature: TWO
//! producers (`dome`, 6 lamps · `doors`, 4 lamps) scattered across TWO
//! outputs (`output`, named "1" · `output2`, named "Box 2") with interleaved
//! anchors — many-to-many, the case the peach (many-to-one) and zook
//! (one-to-many) leave uncovered. Expectations are stated byte-exactly
//! against a baseline that lays both fixtures end to end on "1".
//!
//! Since project format 12 the placement is each fixture's map2d (format 6:
//! its start and overrides), not a patch document; a run is a part, so a
//! fixture split across two outputs is two parts. (Until then this file
//! also pinned auto-flow onto the default output, which format 12 does not
//! have: a format-6 fixture lands only where its patching says.)

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, TreePath};
use lpc_registry::ProjectRegistry;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const DOME_LAMPS: u32 = 6;
const DOORS_LAMPS: u32 = 4;

/// Ticks before a buffer is read: the first frame establishes extents, and
/// output identities settle one tick after registration.
const SETTLE_TICKS: usize = 3;

/// One fixture's map2d (format 6): `parts` as `(id, lamps)` one-row grids
/// laid end to end, and `patching` (the document's `start` / `overrides`
/// members, comma-led).
fn map2d(parts: &[(&str, u32)], patching: &str) -> String {
    let mut x = 0.5;
    let parts: Vec<String> = parts
        .iter()
        .map(|(id, lamps)| {
            let part = format!(
                r#"{{ "id": "{id}", "kind": {{ "grid": {{ "origin": [{x}, 0.5], "cols": {lamps}, "rows": 1, "pitch": 1 }} }} }}"#
            );
            x += *lamps as f32;
            part
        })
        .collect();
    format!(
        r#"{{
  "format": 6,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, 10.0, 1.0]{patching},
  "parts": [ {} ]
}}"#,
        parts.join(", ")
    )
}

/// A start: lamp `lamps` of port 0 of `output`.
fn start(output: &str, lamps: u32) -> String {
    format!(
        r#",
  "start": {{ "output": {{ "set": "{output}" }}, "port": {{ "set": 0 }}, "channel": {{ "set": {{ "lamps": {lamps} }} }} }}"#
    )
}

fn project_fs(dome: &str, doors: &str) -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 12\n}\n")
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
  "name": "1",
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
    write_fixture(&fs, "dome", DOME_LAMPS, 1.0, dome);
    write_fixture(&fs, "doors", DOORS_LAMPS, 0.25, doors);
    fs
}

fn write_fixture(fs: &LpFsMemory, name: &str, lamps: u32, brightness: f32, map2d: &str) {
    fs.write_file(
        format!("/{name}.json").as_path(),
        format!(
            r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": {lamps}, "height": 1 }},
  "bindings": {{ "output": {{ "target": "bus:control.out" }} }},
  "sampling": "direct",
  "diagnostic_mode": "led_index",
  "mapping": {{ "kind": "Map2d", "source": "{name}.map2d.json" }},
  "color_order": "rgb",
  "brightness": {brightness},
  "gamma_correction": false
}}"#
        )
        .as_bytes(),
    )
    .expect("fixture def");
    fs.write_file(format!("/{name}.map2d.json").as_path(), map2d.as_bytes())
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
        .samples16()
        .expect("output channels are u16 samples")
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

/// The baseline every scatter expectation is a permutation of: dome's six
/// lamps then doors' four, end to end on "1".
fn baseline() -> Vec<[u16; 3]> {
    let (engine, _registry) = settled(&project_fs(
        &map2d(&[("dome", DOME_LAMPS)], &start("1", 0)),
        &map2d(&[("doors", DOORS_LAMPS)], &start("1", DOME_LAMPS)),
    ));
    let lamps = published_lamps(&engine, "output.output");
    assert_eq!(lamps.len(), (DOME_LAMPS + DOORS_LAMPS) as usize);
    lamps
}

/// The many-to-many case (Yona steer 2): two producers, two outputs, both
/// scattered, interleaved anchors — byte-exact on BOTH wires, reversal and
/// rotation included.
///
/// dome:  lamps 0-2 (`front`) → "1" @0 · lamps 3-5 (`back`) → "Box 2" @2, reversed
/// doors: lamps 0-1 (`left`) → "1" @3 · lamps 2-3 (`right`) → "Box 2" @0, rotated 1
#[test]
fn many_to_many_scatter_is_byte_exact_on_both_outputs() {
    let dome = map2d(
        &[("front", 3), ("back", 3)],
        &format!(
            r#"{},
  "overrides": {{ "back": {{ "output": {{ "set": "Box 2" }}, "port": {{ "set": 0 }},
    "channel": {{ "set": {{ "lamps": 2 }} }}, "direction": "reverse" }} }}"#,
            start("1", 0)
        ),
    );
    let doors = map2d(
        &[("left", 2), ("right", 2)],
        &format!(
            r#"{},
  "overrides": {{ "right": {{ "output": {{ "set": "Box 2" }}, "port": {{ "set": 0 }},
    "channel": {{ "set": {{ "lamps": 0 }} }}, "offset": 1 }} }}"#,
            start("1", 3)
        ),
    );
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(&dome, &doors));

    // "1": dome 0-2 at wire 0-2, doors 0-1 at wire 3-4.
    assert_eq!(
        published_lamps(&engine, "output.output"),
        vec![flowed[0], flowed[1], flowed[2], flowed[6], flowed[7]],
        "the first output carries the runs addressed to it"
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

/// A producer placed WHOLLY on the other output contributes no fragment and
/// no gap to the first one; its neighbour keeps its place there.
#[test]
fn a_fully_scattered_producer_leaves_no_gap_behind() {
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(
        &map2d(&[("dome", DOME_LAMPS)], &start("1", 0)),
        &map2d(&[("doors", DOORS_LAMPS)], &start("Box 2", 0)),
    ));

    assert_eq!(
        published_lamps(&engine, "output.output"),
        flowed[..DOME_LAMPS as usize].to_vec(),
        "the dome alone on the first output"
    );
    assert_eq!(
        published_lamps(&engine, "output2.output"),
        flowed[DOME_LAMPS as usize..].to_vec(),
        "the doors land whole on the named output"
    );
    assert_eq!(
        node_status(&engine, "output.output"),
        None,
        "the doors' absence from the first output is not a gap"
    );
}

/// Rotation in rendering: the kernel's `(j' + k) mod N`, applied to real
/// buffers — forward and composed with reversal (reverse first, then
/// rotate; the kernel's worked example at N = 5 is hand-checked here at
/// N = 6 and N = 4).
#[test]
fn rotation_permutes_lamps_within_the_run_window() {
    let dome = map2d(
        &[("dome", DOME_LAMPS)],
        &format!(
            r#"{},
  "overrides": {{ "dome": {{ "offset": 2 }} }}"#,
            start("1", 0)
        ),
    );
    let doors = map2d(
        &[("doors", DOORS_LAMPS)],
        &format!(
            r#"{},
  "overrides": {{ "doors": {{ "direction": "reverse", "offset": 1 }} }}"#,
            start("1", DOME_LAMPS)
        ),
    );
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(&dome, &doors));

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
/// mapping stands, those lamps are unplaced, and the fixture's status names
/// the missing output.
#[test]
fn a_dangling_output_name_degrades_and_reports() {
    let dome = map2d(
        &[("front", 3), ("back", 3)],
        &format!(
            r#"{},
  "overrides": {{ "back": {{ "output": {{ "set": "Box 9" }}, "port": {{ "set": 0 }},
    "channel": {{ "set": {{ "lamps": 0 }} }} }} }}"#,
            start("1", 0)
        ),
    );
    let flowed = baseline();
    let (engine, _registry) = settled(&project_fs(
        &dome,
        &map2d(&[("doors", DOORS_LAMPS)], &start("1", DOME_LAMPS)),
    ));

    let Some(NodeRuntimeStatus::Error(message)) = node_status(&engine, "dome.fixture") else {
        panic!("the dangling name is the fixture's error");
    };
    assert!(message.contains("Box 9"), "{message}");
    // The placed part stands; the dangling part is dark, not moved.
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

/// Two outputs claiming one name make a placement's output ambiguous: BOTH
/// wear an error and routing stays exact-match (nothing is guessed onto
/// either).
#[test]
fn duplicate_output_names_error_both_outputs() {
    let mut fs = project_fs(
        &map2d(&[("dome", DOME_LAMPS)], &start("Box 2", 0)),
        &map2d(&[("doors", DOORS_LAMPS)], &start("Box 2", DOME_LAMPS)),
    );
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
