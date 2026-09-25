//! The playlist switch, end to end: hold the frame, unload, load, fade —
//! never black (multi-pattern plan P4, vision D8–D10).
//!
//! A real project (clock → playlist → fixture → output, the fixture being
//! `projects/test/basic`'s mapped one) runs through
//! [`LoadedProjectRuntime::tick_with_residency`], the order every edge
//! ticks in, with only the idle entry loaded at boot. Each test reads the
//! OUTPUT node's published buffer — the bytes a device pushes to its lamps
//! — frame by frame.
//!
//! Entries:
//!
//! - `1` idle: a position-dependent static gradient, so a held frame that
//!   replayed samples in the wrong order would not match it;
//! - `2` green, `4` blue: solid colours with durations, for the timed advance;
//! - `3` broken: a `ref` to a file that does not exist (a load failure);
//! - `5` bad GLSL: loads, and its first compile fails.
//!
//! ```bash
//! cargo test -p lpc-engine --test playlist_switch -- --nocapture
//! ```

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::ScopeRef;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{NodeId, NodeRuntimeStatus, NodeUseLocation, SlotPath, TreePath};
use lpc_wire::WireNodeCommand;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const IDLE: u32 = 1;
const GREEN: u32 = 2;
const BROKEN: u32 = 3;
const BLUE: u32 = 4;
const BAD_GLSL: u32 = 5;
const ALL: [u32; 5] = [IDLE, GREEN, BROKEN, BLUE, BAD_GLSL];

#[test]
fn a_switch_holds_the_old_frame_until_the_new_entry_renders_then_fades() {
    let (fs, mut rt) = boot();
    let idle = steady_idle_frame(&fs, &mut rt);
    assert_eq!(loaded_entries(&rt), [IDLE], "only the idle entry is loaded");

    activate(&mut rt, GREEN);
    let frames = run(&fs, &mut rt, 25);

    // The switch's first frame still shows the idle entry: that frame is
    // what is held.
    assert_eq!(
        frames[0].bytes, idle,
        "the switch frame shows the old entry"
    );
    // Holding: the output is EXACTLY the held frame — the same lamps in the
    // same order — while the green entry loads and compiles.
    print_frames("idle → green", &frames, &idle);
    let held = frames
        .iter()
        .take_while(|frame| frame.bytes == idle)
        .count();
    assert!(
        held >= 3,
        "the playlist must hold across the load and the compile window \
         (capture, load + deferral, window), held {held} frames"
    );
    assert!(
        frames[..held]
            .iter()
            .any(|frame| frame.loaded == [GREEN] && frame.bytes == idle),
        "the idle frame is shown while the idle entry is no longer loaded: \
         it comes from the held buffer"
    );
    // Then the fade: strictly between the two, then green for good.
    let green = &frames.last().expect("frames").bytes;
    assert_ne!(green, &idle);
    assert!(
        frames[held..].iter().any(|frame| &frame.bytes != green),
        "a fade runs between the held frame and the new entry"
    );
    for (index, frame) in frames.iter().enumerate() {
        assert!(
            !frame.all_zero(),
            "frame {index} of the switch went black: {:?}",
            frame.loaded
        );
    }
    assert_eq!(
        loaded_entries(&rt),
        [GREEN],
        "the old entry was unloaded: one entry is ever loaded"
    );
    assert!(
        frames.iter().all(|frame| frame.loaded.len() <= 1),
        "never two entries at once"
    );
}

#[test]
fn a_load_failure_skips_to_the_next_entry_and_holds_meanwhile() {
    let (fs, mut rt) = boot();
    let idle = steady_idle_frame(&fs, &mut rt);

    activate(&mut rt, BROKEN);
    let frames = run(&fs, &mut rt, 25);
    print_frames("idle → broken (load fails) → blue", &frames, &idle);

    let held = frames
        .iter()
        .take_while(|frame| frame.bytes == idle)
        .count();
    assert!(held >= 3, "held {held} frames across the failed load");
    for (index, frame) in frames.iter().enumerate() {
        assert!(!frame.all_zero(), "frame {index} went black");
    }
    assert_eq!(
        loaded_entries(&rt),
        [BLUE],
        "the broken entry is skipped for the next authored one"
    );
    assert!(
        playlist_warning(&rt).contains("entry 3 failed"),
        "Studio sees the failure on the playlist's status: {:?}",
        playlist_warning(&rt)
    );
}

#[test]
fn a_compile_failure_skips_to_the_next_entry_and_holds_meanwhile() {
    let (fs, mut rt) = boot();
    let idle = steady_idle_frame(&fs, &mut rt);

    activate(&mut rt, BAD_GLSL);
    let frames = run(&fs, &mut rt, 40);
    print_frames("idle → bad GLSL (compile fails) → idle", &frames, &idle);

    for (index, frame) in frames.iter().enumerate() {
        assert!(!frame.all_zero(), "frame {index} went black");
    }
    assert!(
        frames.iter().any(|frame| frame.loaded == [BAD_GLSL]),
        "the bad entry loads — its GLSL fails at compile, not at load"
    );
    // No key comes after 5, so the next candidate wraps round to the
    // lowest playable one: the idle entry.
    assert_eq!(loaded_entries(&rt), [IDLE]);
    assert_eq!(
        frames.last().expect("frames").bytes,
        idle,
        "back on the idle entry, rendering for real"
    );
    assert!(playlist_warning(&rt).contains("entry 5 failed"));
}

#[test]
fn activate_loads_a_dormant_entry_and_rejects_an_unknown_one() {
    let (fs, mut rt) = boot();
    steady_idle_frame(&fs, &mut rt);
    assert!(!loaded_entries(&rt).contains(&BLUE), "blue starts dormant");

    let playlist = playlist_id(&rt);
    let err = rt
        .engine_mut()
        .handle_node_command(
            playlist,
            &WireNodeCommand::PlaylistActivateEntry { entry: 9 },
        )
        .expect_err("an unknown key is rejected");
    assert!(err.to_string().contains("no entry 9"), "{err}");

    activate(&mut rt, BLUE);
    run(&fs, &mut rt, 10);
    assert_eq!(loaded_entries(&rt), [BLUE], "the dormant entry loaded");
}

#[test]
fn the_timed_advance_walks_authored_keys_and_skips_failed_ones() {
    let (fs, mut rt) = boot();
    steady_idle_frame(&fs, &mut rt);

    // green (0.5 s) → 3 fails to load → blue (0.5 s) → 5 fails to compile
    // → back to idle, which stays.
    activate(&mut rt, GREEN);
    let first_pass = visited(&run(&fs, &mut rt, 150));
    assert_eq!(first_pass, [IDLE, GREEN, BLUE, BAD_GLSL, IDLE]);

    // Second pass: the failed entries are known, so neither is tried again.
    activate(&mut rt, GREEN);
    let second_pass = visited(&run(&fs, &mut rt, 150));
    assert_eq!(second_pass, [IDLE, GREEN, BLUE, IDLE]);
}

// ---- fixture ----------------------------------------------------------------

/// One published frame.
struct Frame {
    bytes: Vec<u8>,
    loaded: Vec<u32>,
}

impl Frame {
    fn all_zero(&self) -> bool {
        self.bytes.iter().all(|byte| *byte == 0)
    }
}

fn boot() -> (LpFsMemory, LoadedProjectRuntime) {
    let fs = project_fs();
    let services = EngineServices::new(TreePath::parse("/switch.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load switch project");
    rt.engine_mut().set_graphics(Some(std::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    )));
    (fs, rt)
}

/// Tick until the idle entry renders for real, and return its frame after
/// checking it is static (the same bytes on the next frame).
fn steady_idle_frame(fs: &LpFsMemory, rt: &mut LoadedProjectRuntime) -> Vec<u8> {
    let frames = run(fs, rt, 4);
    let last = &frames.last().expect("frames").bytes;
    assert!(
        last.iter().any(|byte| *byte != 0),
        "the idle entry renders ({} bytes; statuses: {:?})",
        last.len(),
        rt.engine()
            .tree()
            .entries()
            .map(|entry| (entry.path.to_string(), entry.status.value().clone()))
            .collect::<Vec<_>>()
    );
    let again = run(fs, rt, 1);
    assert_eq!(&again[0].bytes, last, "the idle entry is static");
    again[0].bytes.clone()
}

fn run(fs: &LpFsMemory, rt: &mut LoadedProjectRuntime, ticks: usize) -> Vec<Frame> {
    (0..ticks)
        .map(|tick| {
            rt.tick_with_residency(fs, 16)
                .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
            Frame {
                bytes: output_bytes(rt),
                loaded: loaded_entries(rt),
            }
        })
        .collect()
}

/// Frame-by-frame: what is loaded, and whether the output is the held idle
/// frame, black, or something else (the head bytes say which).
fn print_frames(label: &str, frames: &[Frame], idle: &[u8]) {
    println!("\n== {label} ==");
    println!("{:<6} {:<10} {:<8} head", "frame", "loaded", "output");
    for (index, frame) in frames.iter().enumerate() {
        let output = if frame.all_zero() {
            "BLACK"
        } else if frame.bytes == idle {
            "idle-frame"
        } else {
            "live"
        };
        println!(
            "{:<6} {:<10} {:<8} {:?}",
            index,
            format!("{:?}", frame.loaded),
            output,
            &frame.bytes[..frame.bytes.len().min(6)]
        );
    }
}

/// The entries loaded frame by frame, with repeats collapsed.
fn visited(frames: &[Frame]) -> Vec<u32> {
    let mut visited: Vec<u32> = Vec::new();
    for frame in frames {
        for entry in &frame.loaded {
            if visited.last() != Some(entry) {
                visited.push(*entry);
            }
        }
    }
    visited
}

fn activate(rt: &mut LoadedProjectRuntime, entry: u32) {
    let playlist = playlist_id(rt);
    rt.engine_mut()
        .handle_node_command(playlist, &WireNodeCommand::PlaylistActivateEntry { entry })
        .unwrap_or_else(|e| panic!("activate {entry}: {e}"));
}

fn playlist_id(rt: &LoadedProjectRuntime) -> NodeId {
    rt.engine()
        .project_runtime_index()
        .node_id(&NodeUseLocation::root().child(SlotPath::parse("nodes[list]").expect("slot")))
        .expect("playlist projected")
}

fn loaded_entries(rt: &LoadedProjectRuntime) -> Vec<u32> {
    let owner = playlist_id(rt);
    ALL.into_iter()
        .filter(|entry| {
            let scope = ScopeRef::Sink {
                owner,
                entry: *entry,
            };
            rt.engine()
                .tree()
                .entries()
                .any(|node| node.parent == Some(owner) && node.scope == Some(scope))
        })
        .collect()
}

fn playlist_warning(rt: &LoadedProjectRuntime) -> String {
    let entry = rt
        .engine()
        .tree()
        .get(playlist_id(rt))
        .expect("playlist entry");
    match entry.status.value() {
        NodeRuntimeStatus::Warn(text) => text.clone(),
        other => format!("{other:?}"),
    }
}

/// The output node's published buffer: what goes to the lamps.
fn output_bytes(rt: &LoadedProjectRuntime) -> Vec<u8> {
    let engine = rt.engine();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        if let Some(buffer) = engine.runtime_buffers().get(buffer_id) {
            return buffer.value().bytes().to_vec();
        }
    }
    Vec::new()
}

fn project_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\n  \"format\": 11\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "list": { "ref": "./playlist.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    write(
        &fs,
        "/playlist.json",
        br#"{
  "kind": "Playlist",
  "bindings": {
    "time": { "source": "bus:time" },
    "output": { "target": "bus:visual.out" }
  },
  "idle_entry": 1,
  "default_fade": 0.1,
  "entries": {
    "1": { "name": "idle", "fade_after": 0.1, "node": { "ref": "./idle.json" } },
    "2": { "name": "green", "duration": 0.5, "node": { "ref": "./green.json" } },
    "3": { "name": "broken", "duration": 0.5, "node": { "ref": "./missing.json" } },
    "4": { "name": "blue", "duration": 0.5, "node": { "ref": "./blue.json" } },
    "5": { "name": "bad", "duration": 0.5, "node": { "ref": "./bad.json" } }
  }
}"#,
    );
    shader(
        &fs,
        "idle",
        "vec4 render_2d(vec2 pos) {\n    \
         return vec4(fract(pos.x * 0.37), fract(pos.y * 0.21), 0.5, 1.0);\n}\n",
    );
    shader(
        &fs,
        "green",
        "vec4 render_2d(vec2 pos) {\n    return vec4(0.0, 1.0, 0.0, 1.0);\n}\n",
    );
    shader(
        &fs,
        "blue",
        "vec4 render_2d(vec2 pos) {\n    return vec4(0.0, 0.0, 1.0, 1.0);\n}\n",
    );
    shader(
        &fs,
        "bad",
        "vec4 render_2d(vec2 pos) {\n    return not_a_function(pos);\n}\n",
    );
    write(
        &fs,
        "/fixture.json",
        include_bytes!("../../../projects/test/basic/fixture.json"),
    );
    write(
        &fs,
        "/fixture.map2d.json",
        include_bytes!("../../../projects/test/basic/fixture.map2d.json"),
    );
    write(
        &fs,
        "/output.json",
        include_bytes!("../../../projects/test/basic/output.json"),
    );
    fs
}

fn shader(fs: &LpFsMemory, name: &str, glsl: &str) {
    write(
        fs,
        &format!("/{name}.json"),
        format!(r#"{{ "kind": "Shader", "source": "{name}.glsl" }}"#).as_bytes(),
    );
    write(fs, &format!("/{name}.glsl"), glsl.as_bytes());
}

fn write(fs: &LpFsMemory, path: &str, bytes: &[u8]) {
    fs.write_file(path.as_path(), bytes).expect("write fixture");
}
