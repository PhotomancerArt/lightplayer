//! The peach, end to end: two fixtures on one wire, one of them cut in half
//! by its patch with the second half plugged in backwards, and the other
//! landing in the gap between them.
//!
//! Wire order and fixture order stop agreeing here, which is the whole point
//! of patching. The assertions are stated as a PERMUTATION of the unpatched
//! buffer rather than as literal colors: "the wire's third lamp is the leaf's
//! first" is the claim, and it survives any change to what colors a
//! diagnostic pattern paints.
//!
//! Since project format 12 the patching is each fixture's map2d (format 6):
//! the body is two parts, `near` and `far`, and "patched" is the leaf's start
//! between them and an override that plugs `far` in backwards; "unpatched"
//! starts each fixture where the one before it ends (what the upgrade froze
//! auto-flow into). The patch-document statements this file held until then
//! (an open-ended range, a document that outgrows its fixture, a newer patch
//! format, manual flow) went with the documents; `off` stands in for manual
//! flow's "not mapped = not lit".
//!
//! ```bash
//! cargo test -p lpc-engine --test output_patch_reflow
//! ```

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{NodeRuntimeStatus, Revision, TreePath};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_wire::{
    GeometryDisplayLayout, KnownRevision, OutputFrameEntry, OutputFrameGeometry,
    OutputFrameProbeRequest, OutputFrameProbeResult, RevisionGateRead, RevisionGateResult,
};
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

/// The body strand: four lamps, full brightness.
const BODY_LAMPS: u32 = 4;
/// The leaf strand: two lamps, dim, so no leaf lamp can be mistaken for a
/// body lamp of the same index.
const LEAF_LAMPS: u32 = 2;

/// A fixture's map2d (format 6): `parts` as `(id, lamps)` one-row grids laid
/// end to end on a canvas as wide as the fixture, its start at lamp `start`
/// of output "1", and `overrides` (a JSON object's members, or empty).
fn map2d(parts: &[(&str, u32)], start: u32, overrides: &str) -> String {
    let lamps: u32 = parts.iter().map(|(_, lamps)| lamps).sum();
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
  "canvas": [0.0, 0.0, {lamps}.0, 1.0],
  "start": {{ "output": {{ "set": "1" }}, "port": {{ "set": 0 }}, "channel": {{ "set": {{ "lamps": {start} }} }} }},
  "overrides": {{ {overrides} }},
  "parts": [ {} ]
}}"#,
        parts.join(", ")
    )
}

/// The body: `near` (lamps 0–1) and `far` (lamps 2–3), from wire lamp 0.
fn body(overrides: &str) -> String {
    map2d(&[("near", 2), ("far", 2)], 0, overrides)
}

/// The leaf, from wire lamp `start`.
fn leaf(start: u32) -> String {
    map2d(&[("leaf", LEAF_LAMPS)], start, "")
}

/// The body's far half, plugged in at its far end four lamps down.
const FAR_REVERSED: &str = r#""far": { "port": { "set": 0 }, "channel": { "set": { "lamps": 4 } }, "direction": "reverse" }"#;

/// The project: two diagnostic fixtures publishing to one output.
///
/// Diagnostic `led_index` rather than a shader: it paints a distinct color per
/// lamp with no visual product involved, so the buffer reads as pure placement
/// and the test needs no graphics backend.
fn project_fs(patched: bool) -> LpFsMemory {
    if patched {
        // Body lamps 0–1 lead the wire; the leaf plugs into the two lamps
        // between the body's halves; the far half is plugged in backwards.
        project_fs_with(&body(FAR_REVERSED), &leaf(2))
    } else {
        project_fs_with(&body(""), &leaf(BODY_LAMPS))
    }
}

/// [`project_fs`] with these map2d documents.
fn project_fs_with(body_map2d: &str, leaf_map2d: &str) -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 12\n}\n")
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
  "name": "1",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#,
    )
    .expect("output.json");

    write_fixture(&fs, "body", BODY_LAMPS, 1.0, body_map2d);
    write_fixture(&fs, "leaf", LEAF_LAMPS, 0.25, leaf_map2d);
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
        .samples16()
        .expect("output channels are u16 samples")
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

/// Clearing the patching, live, returns the wire to fixture order: the
/// override goes and each start moves back to where the fixture before it
/// ends, and the next frames place exactly what the unpatched project places.
#[test]
fn clearing_the_patching_returns_the_wire_to_fixture_order() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    assert_ne!(
        published_lamps(&settled(&project_fs(true)).0),
        flowed,
        "the patching was doing something"
    );

    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    fs.write_file_mut("/body.map2d.json".as_path(), body("").as_bytes())
        .expect("clear the body's override");
    fs.write_file_mut("/leaf.map2d.json".as_path(), leaf(BODY_LAMPS).as_bytes())
        .expect("move the leaf back after the body");
    apply_asset_change(
        &mut engine,
        &mut registry,
        &fs,
        &["/body.map2d.json", "/leaf.map2d.json"],
    );
    tick(&mut engine, &registry, SETTLE_TICKS);

    assert_eq!(
        published_lamps(&engine),
        flowed,
        "cleared patching places exactly what the unpatched project places"
    );
}

/// The live-edit path: editing the patching moves the wire on the next
/// tick. Nothing caches a fragment set across frames, and the fixture
/// re-patches off the re-read document — this test is what says so.
#[test]
fn editing_the_patch_moves_the_wire_on_the_next_tick() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let before = published_lamps(&engine);

    // Unplug the far half's reversal, in place.
    fs.write_file_mut(
        "/body.map2d.json".as_path(),
        body(r#""far": { "port": { "set": 0 }, "channel": { "set": { "lamps": 4 } } }"#).as_bytes(),
    )
    .expect("edit the body's patching");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.map2d.json"]);
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

/// A mapping edit carries its patching with it: one more lamp on the far
/// half, and the far half, still overridden backwards, grows into the wire
/// with no hole (one document holds both since project format 12).
#[test]
fn a_mapping_edit_carries_its_patching() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    assert_eq!(
        published_lamps(&engine).len(),
        (BODY_LAMPS + LEAF_LAMPS) as usize,
        "four body lamps and two leaf lamps tile six channels"
    );
    assert_eq!(output_status(&engine), None);

    // One more lamp on the body's far half, the override unchanged.
    fs.write_file_mut(
        "/body.map2d.json".as_path(),
        map2d(&[("near", 2), ("far", 3)], 0, FAR_REVERSED).as_bytes(),
    )
    .expect("grow the far half");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.map2d.json"]);
    tick(&mut engine, &registry, 2);

    assert_eq!(
        published_lamps(&engine).len(),
        (BODY_LAMPS + LEAF_LAMPS + 1) as usize,
        "the far half grew on the wire"
    );
    assert_eq!(
        output_status(&engine),
        None,
        "and it grew without opening a hole"
    );
}

/// "Not mapped = not lit", end to end: the body's far half is `off`, so
/// lamps 2–3 reach NO wire. (Manual flow's job, with a patch document, until
/// project format 12.)
#[test]
fn an_off_part_reaches_no_wire() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let fs = project_fs_with(&body(r#""far": { "off": true }"#), &leaf(2));

    let (engine, _registry) = settled(&fs);
    assert_eq!(
        published_lamps(&engine),
        vec![flowed[0], flowed[1], flowed[4], flowed[5]],
        "the body's far half is on no wire at all"
    );
    assert_eq!(
        fixture_status(&engine, "body"),
        None,
        "unlit lamps are a state, not a fault"
    );
}

/// The unmap-all state: every part of the body `off` lights nothing of it.
#[test]
fn a_fixture_whose_parts_are_all_off_lights_nothing() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    let fs = project_fs_with(
        &body(r#""near": { "off": true }, "far": { "off": true }"#),
        &leaf(0),
    );

    let (engine, _registry) = settled(&fs);
    assert_eq!(
        published_lamps(&engine),
        vec![flowed[4], flowed[5]],
        "only the leaf reaches the wire; the body is dark"
    );
}

/// Taking `off` away is the whole undo story: the far half follows the near
/// one again, with no other edit.
#[test]
fn taking_off_away_returns_the_lamps_to_the_wire() {
    let flowed = published_lamps(&settled(&project_fs(false)).0);
    // The leaf sits far down the wire, so the body's far half has somewhere
    // visible to follow on INTO when `off` comes off.
    let mut fs = project_fs_with(&body(r#""far": { "off": true }"#), &leaf(10));
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    assert_eq!(
        published_lamps(&engine)[2],
        [0, 0, 0],
        "wire lamp 2 is dark while the far half is off"
    );

    fs.write_file_mut("/body.map2d.json".as_path(), body("").as_bytes())
        .expect("take off away");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/body.map2d.json"]);
    tick(&mut engine, &registry, SETTLE_TICKS);

    assert_eq!(
        published_lamps(&engine)[2..4],
        [flowed[2], flowed[3]],
        "the far half follows on again the moment off comes off"
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
    match published_geometry(engine, registry).display_layout {
        GeometryDisplayLayout::Layout(lpc_model::ControlDisplayLayout::Layout2d(layout)) => layout,
        other => panic!("expected a published display layout, got {other:?}"),
    }
}

/// The first output's whole geometry bundle, asked for outright.
fn published_geometry(engine: &mut Engine, registry: &ProjectRegistry) -> OutputFrameGeometry {
    let entry = output_entries(engine, registry, RevisionGateRead::Always)
        .into_iter()
        .next()
        .expect("one published output");
    match entry.geometry {
        RevisionGateResult::Changed(geometry) => geometry,
        other => panic!("expected changed geometry, got {other:?}"),
    }
}

/// Every published output's entry under one geometry gate.
fn output_entries(
    engine: &mut Engine,
    registry: &ProjectRegistry,
    geometry: RevisionGateRead,
) -> Vec<OutputFrameEntry> {
    let OutputFrameProbeResult::Frame { outputs } = engine.read_project_output_frame_probe(
        registry,
        OutputFrameProbeRequest {
            geometry,
            samples: Some(lpc_wire::WireChannelSampleFormat::U16),
        },
    );
    outputs
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
/// A client caches a refused display layout for as long as its geometry
/// revision stands. Answering an output that has simply not planned its
/// fragments yet with `Unsupported` would park a card on "no layout" until
/// the first plan moved the revision; the honest answer is `Omitted` —
/// "nothing to say this read" — and the client keeps asking until the
/// geometry arrives.
#[test]
fn an_output_with_no_placements_yet_omits_its_layout_rather_than_refusing_it() {
    let fs = project_fs(true);
    let (mut engine, registry) = load(&fs);

    // Before the first settled frame the output has published no fragments.
    let answer = geometry_answer(&mut engine, &registry);
    assert!(
        matches!(
            answer,
            None | Some(RevisionGateResult::Omitted)
                | Some(RevisionGateResult::Changed(OutputFrameGeometry {
                    display_layout: GeometryDisplayLayout::Layout(_),
                    ..
                }))
        ),
        "a not-yet-placed output says nothing, it does not refuse: {answer:?}"
    );

    tick(&mut engine, &registry, SETTLE_TICKS);
    assert!(
        matches!(
            published_geometry(&mut engine, &registry).display_layout,
            GeometryDisplayLayout::Layout(_)
        ),
        "and the geometry arrives once the placements do"
    );
}

/// The geometry half of the first published entry, whatever it is (`None`
/// when no output has published a frame at all).
fn geometry_answer(
    engine: &mut Engine,
    registry: &ProjectRegistry,
) -> Option<RevisionGateResult<OutputFrameGeometry>> {
    output_entries(engine, registry, RevisionGateRead::Always)
        .into_iter()
        .next()
        .map(|entry| entry.geometry)
}

/// The display layout inside the first output's changed geometry.
fn display_layout_answer(engine: &mut Engine, registry: &ProjectRegistry) -> GeometryDisplayLayout {
    published_geometry(engine, registry).display_layout
}

/// A patching-only edit re-cuts the wire without touching any lamp's
/// position — so the producers' own layout revisions do not move, and a
/// client gating on `IfChanged` would keep drawing the old geometry over the
/// new frame.
#[test]
fn re_patching_moves_the_display_layout_revision() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let before = published_display_layout(&mut engine, &registry);

    fs.write_file_mut(
        "/leaf.map2d.json".as_path(),
        map2d(
            &[("leaf", LEAF_LAMPS)],
            2,
            r#""leaf": { "direction": "reverse" }"#,
        )
        .as_bytes(),
    )
    .expect("flip the leaf");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/leaf.map2d.json"]);
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

/// Trigger 3 of the geometry gate — the PLACEMENTS move. A client holding
/// the wire's geometry at its revision hears `Unchanged` while nothing
/// moves, and `Changed` — with the re-cut placements — the read after a
/// patching-only edit, though no lamp moved (and so no producer's own
/// layout changed).
#[test]
fn re_patching_moves_the_geometry_revision_and_resends_the_placements() {
    let mut fs = project_fs(true);
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let before = published_geometry(&mut engine, &registry);
    let node = output_entries(&mut engine, &registry, RevisionGateRead::None)[0].node;
    let known = RevisionGateRead::IfChanged {
        known: vec![KnownRevision {
            node: Some(node),
            revision: before.revision,
        }],
    };

    tick(&mut engine, &registry, 2);
    assert_eq!(
        output_entries(&mut engine, &registry, known.clone())[0].geometry,
        RevisionGateResult::Unchanged {
            revision: before.revision
        },
        "a steady wire's geometry stands"
    );

    fs.write_file_mut(
        "/leaf.map2d.json".as_path(),
        map2d(
            &[("leaf", LEAF_LAMPS)],
            2,
            r#""leaf": { "direction": "reverse" }"#,
        )
        .as_bytes(),
    )
    .expect("flip the leaf");
    apply_asset_change(&mut engine, &mut registry, &fs, &["/leaf.map2d.json"]);
    tick(&mut engine, &registry, 2);

    let RevisionGateResult::Changed(after) = output_entries(&mut engine, &registry, known)
        .remove(0)
        .geometry
    else {
        panic!("a re-cut wire must resend its geometry");
    };
    assert!(after.revision > before.revision);
    assert_ne!(
        after.placements, before.placements,
        "the new cut rides the changed bundle"
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
    let GeometryDisplayLayout::Unsupported { reason } = refused else {
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
            GeometryDisplayLayout::Layout(_)
        ),
        "an unbounded link answers the same layout"
    );
}

/// The frame-probe header carries EVERY output's layout in one unchunked
/// event, so the budget is a header TOTAL: once the header is spent, a later
/// output's geometry is DEFERRED (`Omitted`) — its frame still flows — and
/// the next read, where the others answer `Unchanged`, carries it.
#[test]
fn the_header_total_gates_layouts_across_outputs() {
    // Three outputs on the same control bus, each carrying real content:
    // the body split across "1" and "B" (its far half overridden there), the
    // leaf whole on "C". Three published frames, three same-sized layouts
    // (two lamps each) in one header.
    let fs = project_fs_with(
        &body(
            r#""far": { "output": { "set": "B" }, "port": { "set": 0 }, "channel": { "set": { "lamps": 0 } } }"#,
        ),
        &map2d(&[("leaf", LEAF_LAMPS)], 0, "")
            .replace(r#""output": { "set": "1" }"#, r#""output": { "set": "C" }"#),
    );
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
        GeometryDisplayLayout::Layout(layout) => lpc_wire::ser_write_json_len(&layout),
        other => panic!("expected a layout to measure: {other:?}"),
    };
    engine.set_display_layout_budget(Some(one * 2 + one / 2));

    let outputs = output_entries(&mut engine, &registry, RevisionGateRead::Always);
    assert_eq!(outputs.len(), 3, "all three frames flow regardless");
    let answered: Vec<_> = outputs
        .iter()
        .filter_map(|entry| match &entry.geometry {
            RevisionGateResult::Changed(geometry) => Some(KnownRevision {
                node: Some(entry.node),
                revision: geometry.revision,
            }),
            _ => None,
        })
        .collect();
    let deferred: Vec<_> = outputs
        .iter()
        .filter(|entry| entry.geometry == RevisionGateResult::Omitted)
        .map(|entry| entry.node)
        .collect();
    assert_eq!(
        (answered.len(), deferred.len()),
        (2, 1),
        "two layouts fit the header, the third is deferred"
    );

    // The next read names what it holds: the two answered outputs are
    // `Unchanged`, which leaves the header to the deferred one.
    let outputs = output_entries(
        &mut engine,
        &registry,
        RevisionGateRead::IfChanged { known: answered },
    );
    let deferred_entry = outputs
        .iter()
        .find(|entry| entry.node == deferred[0])
        .expect("the deferred output still publishes");
    assert!(
        matches!(
            &deferred_entry.geometry,
            RevisionGateResult::Changed(OutputFrameGeometry {
                display_layout: GeometryDisplayLayout::Layout(_),
                ..
            })
        ),
        "the deferred output's geometry rides the next read: {:?}",
        deferred_entry.geometry
    );
}
