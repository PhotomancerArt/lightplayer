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
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 9\n}\n")
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
  "channels": { "0": { "endpoint": "ws281x:local:D10" } },
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
        br#"{ "format": 2, "entries": [] }"#,
    )
    .expect("newer body patch");

    let (engine, _registry) = settled(&fs);

    let status = fixture_status(&engine, "body").expect("the fixture reports the refusal");
    let NodeRuntimeStatus::Error(message) = status else {
        panic!("a refused document is an error");
    };
    assert!(message.contains("unsupported patch format 2"), "{message}");
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
