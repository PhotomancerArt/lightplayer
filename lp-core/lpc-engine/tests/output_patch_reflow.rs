//! The peach, end to end: two fixtures on one wire, one of them cut in half
//! by its patch with the second half plugged in backwards, and the other
//! landing in the gap between them.
//!
//! Wire order and fixture order stop agreeing here, which is the whole point
//! of a patch. The assertions are stated as a PERMUTATION of the unpatched
//! buffer rather than as literal colors: "the wire's third lamp is the leaf's
//! first" is the claim, and it survives any change to what colors a
//! diagnostic pattern paints.
//!
//! ```bash
//! cargo test -p lpc-engine --test output_patch_reflow
//! ```

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, Revision, TreePath};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, OutputFrameProbeRequest,
    OutputFrameProbeResult,
};
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

/// The body strand: four lamps, full brightness.
const BODY_LAMPS: u32 = 4;
/// The leaf strand: two lamps, dim, so no leaf lamp can be mistaken for a
/// body lamp of the same index.
const LEAF_LAMPS: u32 = 2;

/// Body lamps 0–1 lead the wire; lamps 2–3 are the far half, plugged in at
/// their far end twelve channels down.
const BODY_PATCH: &str = r#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 2 }, "at": { "channel": 0 } },
    { "range": { "start": 2, "count": 2 }, "at": { "channel": 4 }, "reversed": true }
  ]
}
"#;

/// The leaf plugs into the two channels between the body's halves.
const LEAF_PATCH: &str = r#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 2 }, "at": { "channel": 2 } }
  ]
}
"#;

/// The project: two diagnostic fixtures publishing to one output.
///
/// Diagnostic `led_index` rather than a shader: it paints a distinct color per
/// lamp with no visual product involved, so the buffer reads as pure placement
/// and the test needs no graphics backend.
fn project_fs(patched: bool) -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 10\n}\n")
        .expect("container manifest");
    fs.write_file(
        "/module.json".as_path(),
        br#"{ "kind": "Module", "nodes": {
  "body": { "ref": "./body.json" },
  "leaf": { "ref": "./leaf.json" },
  "output": { "ref": "./output.json" }
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

    write_fixture(&fs, "body", BODY_LAMPS, 1.0, patched);
    write_fixture(&fs, "leaf", LEAF_LAMPS, 0.25, patched);
    if patched {
        fs.write_file("/body.patch.json".as_path(), BODY_PATCH.as_bytes())
            .expect("body patch");
        fs.write_file("/leaf.patch.json".as_path(), LEAF_PATCH.as_bytes())
            .expect("leaf patch");
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
    {{ "name": "strand", "shape": {{ "grid": {{ "origin": [0.5, 0.5], "cols": {lamps}, "rows": 1, "pitch": 1 }} }} }}
  ]
}}"#
        )
        .as_bytes(),
    )
    .expect("map2d");
}

fn load(fs: &LpFsMemory) -> (Engine, ProjectRegistry) {
    let services = EngineServices::new(TreePath::parse("/peach.show").expect("path"));
    ProjectLoader::load_from_root(fs, services)
        .expect("load peach project")
        .into_parts()
}

/// Ticks before a buffer is read: the first frame establishes extents.
const SETTLE_TICKS: usize = 3;

/// The output's published samples, grouped one entry per RGB lamp.
fn published_lamps(engine: &Engine) -> Vec<[u16; 3]> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree");
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

fn settled(fs: &LpFsMemory) -> (Engine, ProjectRegistry) {
    let (mut engine, registry) = load(fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    (engine, registry)
}

fn tick(engine: &mut Engine, registry: &ProjectRegistry, times: usize) {
    for _ in 0..times {
        engine.tick(registry, 16).expect("tick");
    }
}

fn output_status(engine: &Engine) -> Option<NodeRuntimeStatus> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree");
    let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
        panic!("output node alive");
    };
    node.runtime_status()
}

fn fixture_status(engine: &Engine, name: &str) -> Option<NodeRuntimeStatus> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with(&format!("{name}.fixture")))
        .expect("fixture node in tree");
    let lpc_engine::node::NodeEntryState::Alive(node) = entry.state.value() else {
        panic!("fixture node alive");
    };
    node.runtime_status()
}

/// Unpatched, the wire is fixture order: the body's four lamps then the
/// leaf's two. This is the baseline every patched claim below is stated
/// against.
#[test]
fn without_patches_the_wire_is_fixture_order() {
    let (engine, _registry) = settled(&project_fs(false));
    let lamps = published_lamps(&engine);

    assert_eq!(lamps.len(), (BODY_LAMPS + LEAF_LAMPS) as usize);
    assert_eq!(output_status(&engine), None);
    // Distinctness is what makes the permutation assertions below meaningful.
    for (index, lamp) in lamps.iter().enumerate() {
        assert!(
            lamps.iter().skip(index + 1).all(|other| other != lamp),
            "lamp {index} is not distinguishable from a later one: {lamps:?}"
        );
    }
}

/// The peach itself. Fixture order and wire order have parted company: the
/// leaf sits between the body's halves, and the body's far half runs
/// backwards.
#[test]
fn the_patched_wire_is_the_authored_permutation_of_the_auto_flowed_one() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let patched = published_lamps(&settled(&project_fs(true)).0);

    // Auto-flow indices: 0-3 are the body's lamps, 4-5 the leaf's.
    assert_eq!(
        patched,
        vec![
            flowed[0], flowed[1], // body 0-1, where the patch anchored them
            flowed[4], flowed[5], // the leaf, interleaved into the gap
            flowed[3], flowed[2], // body 2-3, laid down end-first
        ]
    );
    assert_eq!(
        patched.len(),
        flowed.len(),
        "a patch that tiles the wire changes no lamp's existence"
    );
}

/// Clearing the patch restores auto-flow — the property an installer relies on
/// when a rig is rebuilt, and the reason the patch is its own document.
///
/// Emptying the entries is what a patching editor's "clear" writes, and it
/// must mean what deleting the file means. Anything else and the installer
/// who clears one strand of three finds the wire contested instead of plain.
#[test]
fn deleting_the_patch_restores_auto_flow() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    assert_ne!(
        published_lamps(&settled(&project_fs(true)).0),
        flowed,
        "the patch was doing something"
    );

    // Emptying the document is the "clear" a patching editor performs; the
    // reference stays, so this exercises the resolve path, not the loader.
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{ "format": 1, "entries": [] }"#,
    )
    .expect("clear body patch");
    fs.write_file_mut(
        "/leaf.patch.json".as_path(),
        br#"{ "format": 1, "entries": [] }"#,
    )
    .expect("clear leaf patch");
    apply_asset_change(
        &mut engine,
        &mut registry,
        &fs,
        &["/body.patch.json", "/leaf.patch.json"],
    );
    tick(&mut engine, &registry, SETTLE_TICKS);

    assert_eq!(
        published_lamps(&engine),
        flowed,
        "an empty patch places exactly what no patch places"
    );
}

/// The live-edit path: editing the patch document moves the wire on the next
/// tick. Nothing caches a fragment set across frames, and the fixture
/// re-resolves off the bumped patch version — this test is what says so.
#[test]
fn editing_the_patch_moves_the_wire_on_the_next_tick() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let before = published_lamps(&engine);

    // Unplug the far half's reversal, in place.
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 2 }, "at": { "channel": 0 } },
    { "range": { "start": 2, "count": 2 }, "at": { "channel": 4 } }
  ]
}"#,
    )
    .expect("edit body patch");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.patch.json"]);
    engine.tick(&registry, 16).expect("tick after the edit");

    let after = published_lamps(&engine);
    assert_eq!(
        after,
        vec![
            before[0], before[1], before[2], before[3], before[5], before[4]
        ],
        "only the far half's direction moved, and it moved on the next tick"
    );
}

/// A range with no `count` runs to the end of the fixture, and "the end"
/// moves when the MAPPING does — so a mapping edit re-resolves the patch even
/// though the patch document did not change. The strand that grew stays
/// patched; nobody edits two documents to add a lamp.
#[test]
fn a_mapping_edit_re_resolves_an_open_ended_range() {
    let mut fs = project_fs(true);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 2 }, "at": { "channel": 0 } },
    { "range": { "start": 2 }, "at": { "channel": 4 } }
  ]
}"#,
    )
    .expect("open-ended body patch");
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    assert_eq!(
        published_lamps(&engine).len(),
        (BODY_LAMPS + LEAF_LAMPS) as usize,
        "four body lamps and two leaf lamps tile six channels"
    );
    assert_eq!(output_status(&engine), None);

    // One more lamp on the body strand, mapping only.
    fs.write_file_mut(
        "/body.map2d.json".as_path(),
        br#"{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, 5.0, 1.0],
  "objects": [
    { "name": "strand", "shape": { "grid": { "origin": [0.5, 0.5], "cols": 5, "rows": 1, "pitch": 1 } } }
  ]
}"#,
    )
    .expect("grow the body strand");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.map2d.json"]);
    tick(&mut engine, &registry, 2);

    assert_eq!(
        published_lamps(&engine).len(),
        (BODY_LAMPS + LEAF_LAMPS + 1) as usize,
        "the open-ended range grew with the strand"
    );
    assert_eq!(
        output_status(&engine),
        None,
        "and it grew without opening a hole"
    );
}

/// A patch that no longer fits its fixture is reported, not obeyed: the lamps
/// fall back to auto-flow rather than being placed by a document that
/// describes a fixture this is not.
#[test]
fn a_patch_that_outgrows_its_fixture_degrades_to_auto_flow_and_reports() {
    let mut fs = project_fs(true);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 99 }, "at": { "channel": 0 } }
  ]
}"#,
    )
    .expect("overlong body patch");

    let (engine, _registry) = settled(&fs);

    let status = fixture_status(&engine, "body").expect("the fixture reports the bad patch");
    let NodeRuntimeStatus::Error(message) = status else {
        panic!("a patch that cannot be resolved is an error, not a warning");
    };
    assert!(message.contains("patch"), "{message}");

    // The leaf's patch still anchors it at channels 2–3, so the body — now
    // unpatched — auto-flows after every anchor, and channels 0–1 are a hole
    // the output warns about rather than quietly closing.
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    assert_eq!(
        published_lamps(&engine),
        vec![
            [0, 0, 0],
            [0, 0, 0],
            flowed[4],
            flowed[5],
            flowed[0],
            flowed[1],
            flowed[2],
            flowed[3],
        ],
        "every body lamp still reaches the wire, after the anchored leaf"
    );
    let Some(NodeRuntimeStatus::Warn(warning)) = output_status(&engine) else {
        panic!("the hole the failed patch left is a warning on the output");
    };
    assert!(warning.contains("0-1"), "{warning}");
}

/// A patch document newer than this build is refused at load, whole and by
/// name — the share-envelope posture, not a silent partial read.
#[test]
fn a_newer_patch_format_refuses_the_fixture_at_load() {
    let mut fs = project_fs(true);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{ "format": 4, "entries": [] }"#,
    )
    .expect("newer body patch");

    let (engine, _registry) = settled(&fs);

    let status = fixture_status(&engine, "body").expect("the fixture reports the refusal");
    let NodeRuntimeStatus::Error(message) = status else {
        panic!("a refused document is an error");
    };
    assert!(message.contains("unsupported patch format 4"), "{message}");
}

/// MANUAL flow (P5b), end to end: the body declares it and names only its
/// first two lamps, so lamps 2–3 reach NO wire — where auto-flow would have
/// re-placed them past the last anchor. This is what makes "not mapped =
/// not lit" a state a fixture can actually be in.
#[test]
fn a_manual_fixture_places_only_what_its_entries_name() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let mut fs = project_fs(true);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 3,
  "flow": "manual",
  "entries": [
    [[0, 2], -1, 0]
  ]
}"#,
    )
    .expect("manual body patch");
    fs.write_file_mut(
        "/leaf.patch.json".as_path(),
        br#"{ "format": 1, "entries": [ { "range": { "start": 0, "count": 2 }, "at": { "channel": 2 } } ] }"#,
    )
    .expect("leaf patch");

    let (engine, _registry) = settled(&fs);
    assert_eq!(
        published_lamps(&engine),
        vec![flowed[0], flowed[1], flowed[4], flowed[5]],
        "the body's unnamed lamps 2-3 are on no wire at all"
    );
    assert_eq!(
        fixture_status(&engine, "body"),
        None,
        "unmapped lamps are a state, not a fault"
    );
}

/// The unmap-all state: a manual document with NO entries lights nothing of
/// its fixture — the one place an empty document does not mean auto-flow.
#[test]
fn an_empty_manual_document_takes_the_fixture_off_the_wire() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let mut fs = project_fs(true);
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{ "format": 3, "flow": "manual", "entries": [] }"#,
    )
    .expect("unmapped body");
    fs.write_file_mut(
        "/leaf.patch.json".as_path(),
        br#"{ "format": 1, "entries": [] }"#,
    )
    .expect("cleared leaf patch");

    let (engine, _registry) = settled(&fs);
    assert_eq!(
        published_lamps(&engine),
        vec![flowed[4], flowed[5]],
        "only the auto leaf reaches the wire; the manual body is dark"
    );
}

/// Flipping the flag back is the whole undo story: the same entries under
/// `auto` flow the tail again, with no other edit.
#[test]
fn flipping_the_flag_back_to_auto_returns_the_lamps_to_flow() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let mut fs = project_fs(true);
    // The leaf sits far down the wire, so the body's own tail has somewhere
    // visible to flow INTO when the flag comes off.
    fs.write_file_mut(
        "/leaf.patch.json".as_path(),
        br#"{ "format": 1, "entries": [ { "range": { "start": 0, "count": 2 }, "at": { "channel": 10 } } ] }"#,
    )
    .expect("leaf patch");
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 3,
  "flow": "manual",
  "entries": [
    [[0, 2], -1, 0]
  ]
}"#,
    )
    .expect("manual body patch");
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    assert_eq!(
        published_lamps(&engine)[2],
        [0, 0, 0],
        "wire lamp 2 is dark while the body is manual"
    );

    // The SAME entries, `auto` — one field, no other edit.
    fs.write_file_mut(
        "/body.patch.json".as_path(),
        br#"{
  "format": 2,
  "entries": [
    [[0, 2], -1, 0]
  ]
}"#,
    )
    .expect("auto body patch");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.patch.json"]);
    tick(&mut engine, &registry, SETTLE_TICKS);

    assert_eq!(
        published_lamps(&engine)[2..4],
        [flowed[2], flowed[3]],
        "the body's tail flows again the moment the flag comes off"
    );
}

/// The output's published DISPLAY layout: where a client is told to draw each
/// lamp, and which sample of the published frame to colour it from.
///
/// This is the picture the simulator and the device card paint. It has to be
/// the WHOLE wire's picture: `sample_start` indexes the frame the output
/// published, so a producer's own lamp numbering is the wrong space the
/// moment a second producer joins or a patch moves a run.
fn published_display_layout(
    engine: &mut Engine,
    registry: &ProjectRegistry,
) -> lpc_model::ControlLayout2d {
    let result = engine.read_project_output_frame_probe(
        registry,
        OutputFrameProbeRequest {
            display_layout: ControlDisplayLayoutRead::Always,
        },
    );
    let OutputFrameProbeResult::Frame { outputs } = result;
    let entry = outputs.into_iter().next().expect("one published output");
    match entry.display_layout {
        ControlDisplayLayoutProbeResult::Layout(lpc_model::ControlDisplayLayout::Layout2d(
            layout,
        )) => layout,
        other => panic!("expected a published display layout, got {other:?}"),
    }
}

/// The x coordinate of each drawn lamp, in wire order. Every lamp in these
/// two fixtures sits at a distinct x (the body's four at 0.125…0.875 of its
/// own canvas, the leaf's two at 0.25/0.75 of its own), so this reads as
/// "which physical lamp is on which channel".
fn drawn_lamp_positions(layout: &lpc_model::ControlLayout2d) -> Vec<f32> {
    layout.lamps.iter().map(|lamp| lamp.center[0]).collect()
}

/// Even unpatched, an output is a MERGE: the leaf's lamps sit six samples
/// into the wire, and a layout stated in the leaf's own numbering would draw
/// them over the body's first two.
#[test]
fn the_published_display_layout_covers_every_fixture_on_the_wire() {
    let (mut engine, registry) = settled(&project_fs(false));
    let layout = published_display_layout(&mut engine, &registry);

    assert_eq!(
        layout
            .lamps
            .iter()
            .map(|lamp| (lamp.lamp_index, lamp.sample_start))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 3), (2, 6), (3, 9), (4, 12), (5, 15)],
        "six channels, six drawn lamps, numbered as the WIRE numbers them"
    );
    assert_eq!(
        drawn_lamp_positions(&layout),
        vec![0.125, 0.375, 0.625, 0.875, 0.25, 0.75],
        "the body's four lamps lead, then the leaf's two"
    );
}

/// The peach, from the viewer's side. Fixture order and wire order have
/// parted company — and the picture must follow the LAMPS, not the wire: every
/// lamp keeps its position and its own colour, whatever channel it was
/// plugged into.
#[test]
fn a_patched_display_layout_draws_every_lamp_at_its_own_colour() {
    let (mut flowed_engine, flowed_registry) = settled(&project_fs(false));
    let flowed_layout = published_display_layout(&mut flowed_engine, &flowed_registry);
    let flowed_colors = published_lamps(&flowed_engine);

    let (mut engine, registry) = settled(&project_fs(true));
    let layout = published_display_layout(&mut engine, &registry);
    let colors = published_lamps(&engine);

    assert_eq!(
        drawn_lamp_positions(&layout),
        vec![0.125, 0.375, 0.25, 0.75, 0.875, 0.625],
        "body 0-1, then the leaf between the halves, then the far half backwards"
    );

    // The claim that matters: pair the two projects' lamps by POSITION — the
    // same physical lamp in both — and the colour a client is told to paint
    // there must be the same colour. Nothing here mentions a channel number,
    // so it holds for any patch.
    assert_eq!(layout.lamps.len(), flowed_layout.lamps.len());
    for lamp in &layout.lamps {
        let twin = flowed_layout
            .lamps
            .iter()
            .find(|other| other.center == lamp.center)
            .expect("every patched lamp is a lamp the unpatched wire also had");
        assert_eq!(
            colors[lamp.sample_start as usize / 3],
            flowed_colors[twin.sample_start as usize / 3],
            "the lamp at {:?} must be drawn in its own colour",
            lamp.center
        );
    }
}

/// "No placements yet" is a MOMENT, not a refusal.
///
/// `Unsupported` is permanent on the wire: both client feeds stop asking for
/// geometry the instant they see one, and stand a locally synthesized layout
/// up in its place — which, for a project with more than one fixture, is one
/// fixture's lamps reading the whole wire's samples. Answering an output that
/// has simply not planned its fragments yet with `Unsupported` would therefore
/// strand a card on that wrong picture for the rest of the connection. The
/// honest answer is `Omitted`, and the very next read carries the layout.
#[test]
fn an_output_with_no_placements_yet_omits_its_layout_rather_than_refusing_it() {
    let fs = project_fs(true);
    let (mut engine, registry) = load(&fs);

    // Before the first settled frame the output has published no fragments.
    let answer = display_layout_answer(&mut engine, &registry);
    assert!(
        matches!(
            answer,
            ControlDisplayLayoutProbeResult::Omitted | ControlDisplayLayoutProbeResult::Layout(_)
        ),
        "a not-yet-placed output says nothing, it does not refuse forever: {answer:?}"
    );

    tick(&mut engine, &registry, SETTLE_TICKS);
    assert!(
        matches!(
            display_layout_answer(&mut engine, &registry),
            ControlDisplayLayoutProbeResult::Layout(_)
        ),
        "and the geometry arrives once the placements do"
    );
}

/// The display-layout half of the published-frame answer, whatever it is.
fn display_layout_answer(
    engine: &mut Engine,
    registry: &ProjectRegistry,
) -> ControlDisplayLayoutProbeResult {
    let OutputFrameProbeResult::Frame { outputs } = engine.read_project_output_frame_probe(
        registry,
        OutputFrameProbeRequest {
            display_layout: ControlDisplayLayoutRead::Always,
        },
    );
    outputs
        .into_iter()
        .next()
        .map(|entry| entry.display_layout)
        .unwrap_or(ControlDisplayLayoutProbeResult::Omitted)
}

/// A patch edit re-cuts the wire without touching any mapping — so the
/// producers' own layout revisions do not move, and a client gating on
/// `IfChanged` would keep drawing the old geometry over the new frame.
#[test]
fn re_patching_moves_the_display_layout_revision() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let before = published_display_layout(&mut engine, &registry);

    fs.write_file_mut(
        "/leaf.patch.json".as_path(),
        br#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 2 }, "at": { "channel": 2 }, "reversed": true }
  ]
}"#,
    )
    .expect("flip the leaf");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/leaf.patch.json"]);
    tick(&mut engine, &registry, 2);

    let after = published_display_layout(&mut engine, &registry);
    assert_eq!(
        drawn_lamp_positions(&after),
        vec![0.125, 0.375, 0.75, 0.25, 0.875, 0.625],
        "the leaf's two lamps swapped channels"
    );
    assert!(
        after.revision > before.revision,
        "a re-cut wire must announce itself: {:?} then {:?}",
        before.revision,
        after.revision
    );
}

fn apply_asset_change(
    engine: &mut Engine,
    registry: &mut ProjectRegistry,
    fs: &LpFsMemory,
    paths: &[&str],
) {
    let shapes = engine.slot_shapes().clone();
    let events: Vec<FsEvent> = paths
        .iter()
        .map(|path| FsEvent {
            path: LpPathBuf::from(*path),
            kind: FsEventKind::Modify,
        })
        .collect();
    let changes =
        registry.refresh_artifacts(fs, &events, Revision::new(2), &ParseCtx { shapes: &shapes });
    engine
        .apply_project_changes(fs, registry, &changes)
        .expect("apply asset change");
}

/// The layout gate reads the LINK's declared budget, not a compile-time
/// constant: the same published layout is refused on a link too small for
/// it (with both numbers in the reason) and answered without limit on a
/// link that declared none. Radiance-scale geometry never rides the
/// embedded serial frame — it rides links like these.
#[test]
fn the_declared_link_budget_gates_the_published_layout() {
    let (mut engine, registry) = settled(&project_fs(false));

    engine.set_display_layout_budget(Some(64));
    let refused = display_layout_answer(&mut engine, &registry);
    let ControlDisplayLayoutProbeResult::Unsupported { reason } = refused else {
        panic!("a 64-byte link cannot carry this layout: {refused:?}");
    };
    assert!(
        reason.contains("64"),
        "the refusal names the link budget: {reason}"
    );
    assert!(
        reason.contains("bytes"),
        "the refusal names the measured size: {reason}"
    );

    engine.set_display_layout_budget(None);
    assert!(
        matches!(
            display_layout_answer(&mut engine, &registry),
            ControlDisplayLayoutProbeResult::Layout(_)
        ),
        "an unbounded link answers the same layout"
    );
}

/// The frame-probe header carries EVERY output's layout in one unchunked
/// event, so the budget is a header TOTAL: outputs whose layouts each fit
/// individually degrade to `Unsupported` once the header is spent — and
/// their frames still flow.
#[test]
fn the_header_total_gates_layouts_across_outputs() {
    // Three outputs on the same control bus, each carrying real content:
    // since the scatter rule (D40) landed, an unpatched producer flows to
    // the DEFAULT output only, so the extra outputs get theirs by explicit
    // format-2 scatter — body split across the default and "B", leaf whole
    // on "C". Three published frames, three same-sized layouts (two lamps
    // each) in one header.
    let fs = project_fs(false);
    fs.write_file(
        "/body.patch.json".as_path(),
        br#"{
  "format": 2,
  "outputs": ["B"],
  "entries": [
    [[0, 2], -1, 0],
    [[2, 2], 0, 0]
  ]
}
"#,
    )
    .expect("scattered body patch");
    fs.write_file(
        "/leaf.patch.json".as_path(),
        br#"{
  "format": 2,
  "outputs": ["C"],
  "entries": [
    [[0, 2], 0, 0]
  ]
}
"#,
    )
    .expect("scattered leaf patch");
    for name in ["body", "leaf"] {
        // Re-point the fixtures at the patch docs (project_fs(false) wrote
        // them unpatched).
        let def = format!(
            r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": {lamps}, "height": 1 }},
  "bindings": {{ "output": {{ "target": "bus:control.out" }} }},
  "sampling": "direct",
  "diagnostic_mode": "led_index",
  "mapping": {{ "kind": "Map2d", "source": "{name}.map2d.json" }},
  "patch": {{ "kind": "File", "source": "{name}.patch.json" }},
  "color_order": "rgb",
  "brightness": {brightness},
  "gamma_correction": false
}}"#,
            lamps = if name == "body" { 4 } else { 2 },
            brightness = if name == "body" { 1.0 } else { 0.25 },
        );
        fs.write_file(format!("/{name}.json").as_path(), def.as_bytes())
            .expect("re-pointed fixture def");
    }
    fs.write_file(
        "/module.json".as_path(),
        br#"{ "kind": "Module", "nodes": {
  "body": { "ref": "./body.json" },
  "leaf": { "ref": "./leaf.json" },
  "output": { "ref": "./output.json" },
  "output2": { "ref": "./output2.json" },
  "output3": { "ref": "./output3.json" }
} }"#,
    )
    .expect("module.json");
    for (name, endpoint, output_name) in [("output2", "D11", "B"), ("output3", "D12", "C")] {
        fs.write_file(
            format!("/{name}.json").as_path(),
            format!(
                r#"{{
  "kind": "Output",
  "name": "{output_name}",
  "ports": {{ "0": {{ "endpoint": "ws281x:local:{endpoint}" }} }},
  "bindings": {{ "input": {{ "source": "bus:control.out" }} }}
}}"#
            )
            .as_bytes(),
        )
        .expect("extra output");
    }
    let (mut engine, registry) = settled(&fs);

    // Measure one layout as the wire would, then declare a budget that
    // holds two of them but not three.
    let one = match display_layout_answer(&mut engine, &registry) {
        ControlDisplayLayoutProbeResult::Layout(layout) => lpc_wire::ser_write_json_len(&layout),
        other => panic!("expected a layout to measure: {other:?}"),
    };
    engine.set_display_layout_budget(Some(one * 2 + one / 2));

    let OutputFrameProbeResult::Frame { outputs } = engine.read_project_output_frame_probe(
        &registry,
        OutputFrameProbeRequest {
            display_layout: ControlDisplayLayoutRead::Always,
        },
    );
    assert_eq!(outputs.len(), 3, "all three frames flow regardless");
    let answered = outputs
        .iter()
        .filter(|entry| {
            matches!(
                entry.display_layout,
                ControlDisplayLayoutProbeResult::Layout(_)
            )
        })
        .count();
    let refused: Vec<_> = outputs
        .iter()
        .filter_map(|entry| match &entry.display_layout {
            ControlDisplayLayoutProbeResult::Unsupported { reason } => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        (answered, refused.len()),
        (2, 1),
        "two layouts fit the header, the third degrades: {refused:?}"
    );
    assert!(
        refused[0].contains("header already carries"),
        "the refusal explains the header total: {}",
        refused[0]
    );
}
