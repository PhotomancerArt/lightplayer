//! Cycling and next/prev, end to end (multi-pattern plan P5, vision D13,
//! D17, D20; plan A1–A3).
//!
//! A real project (clock → playlist → fixture → output, the fixture being
//! `projects/test/basic`'s mapped one, plus two buttons on `bus:trigger`)
//! runs through [`LoadedProjectRuntime::tick_with_residency`] at 16 ms a
//! frame, with only the idle entry loaded at boot. The tests read which
//! entry is LOADED frame by frame — one entry is ever loaded, so that is
//! the entry the lamps are showing or fading to.
//!
//! Entries: `1` idle, `2` green, `3` blue, `4` red — solid shaders, no
//! durations unless a test authors them — and, where a test adds it, `5`
//! broken (a `ref` to a missing file).
//!
//! ```bash
//! cargo test -p lpc-engine --test playlist_cycle -- --nocapture
//! ```

use std::rc::Rc;

use lpc_engine::engine::{ButtonService, LoadedProjectRuntime};
use lpc_engine::node::ScopeRef;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_hardware::{
    HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver, default_esp32c6_hardware_manifest,
};
use lpc_model::{
    ChannelName, LpValue, NodeId, NodeRuntimeStatus, NodeUseLocation, PlaylistCycle, SlotPath,
    ToLpValue, TreePath,
};
use lpfs::{AsLpPath, LpFs, LpFsMemory};

const IDLE: u32 = 1;
const GREEN: u32 = 2;
const BLUE: u32 = 3;
const RED: u32 = 4;
const BROKEN: u32 = 5;
const ALL: [u32; 5] = [IDLE, GREEN, BLUE, RED, BROKEN];

/// The next button (`button:local:D9`, GPIO20) and the previous one
/// (`button:local:D8`, GPIO19).
const NEXT_PIN: u32 = 20;
const PREV_PIN: u32 = 19;
/// The blast button (`button:local:D4`, GPIO22): message id 9, which
/// `green_trigger` puts on green's `trigger_ids`.
const BLAST_PIN: u32 = 22;

/// Frames in one half-second step at 16 ms a frame.
const HALF_SECOND: usize = 32;

#[test]
fn a_cycle_walks_the_entries_in_key_order_and_wraps() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });

    let walk = show.visited(HALF_SECOND * 9);
    println!("cycle, 0.5 s steps: {walk:?}");

    assert_eq!(
        &walk[..6],
        [IDLE, GREEN, BLUE, RED, IDLE, GREEN],
        "key order, wrapping round"
    );
    assert!(
        show.loaded().len() <= 1,
        "one entry is ever loaded: {:?}",
        show.loaded()
    );
}

#[test]
fn a_cycle_passes_over_skipped_and_failed_entries() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        skip: r#""skip": [3],"#,
        broken_entry: true,
        ..Authored::default()
    });

    let walk = show.visited(HALF_SECOND * 10);
    println!("cycle, skip [3], 5 broken: {walk:?}");

    assert!(!walk.contains(&BLUE), "3 is skipped: {walk:?}");
    assert_eq!(
        &walk[..6],
        [IDLE, GREEN, RED, IDLE, GREEN, RED],
        "the broken entry fails once and is passed over after"
    );
    assert!(
        show.warning().contains("entry 5 failed"),
        "{}",
        show.warning()
    );
}

#[test]
fn a_pick_while_cycling_jumps_and_the_cycle_carries_on_from_it() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });
    // A quarter of the way into green's step.
    let before = show.visited(HALF_SECOND + HALF_SECOND / 4);
    assert_eq!(before, [IDLE, GREEN]);

    show.activate(RED);
    let frames = show.run(HALF_SECOND * 4);
    let after = visited(&frames);
    println!("pick red while on green: {after:?}");

    assert_eq!(
        &after[..4],
        [GREEN, RED, IDLE, GREEN],
        "red, then on from red: wrap to idle, then green"
    );
    let on_red = frames.iter().filter(|loaded| **loaded == [RED]).count();
    assert!(
        on_red >= HALF_SECOND - 4,
        "the picked entry plays a whole step from the pick, not the rest of green's: \
         {on_red} frames"
    );
}

/// A held, frozen or absent cycle holds the picked entry (playlists-on-the-
/// bus vision D7): a pick stays put whatever its `duration`, and a trigger's
/// excursion plays the entry's duration and comes back to the pick. Before
/// this, a held cycle walked the entries by their durations and settled on
/// idle (`docs/defects/2026-10-19-the-cycle-switch-off-does-not-hold.md`).
#[test]
fn frozen_hold_and_absent_cycles_hold_the_pick() {
    let mut runs = Vec::new();
    for cycle in [
        "",
        r#""cycle": { "kind": "hold", "step_seconds": 0, "fade_seconds": 0 },"#,
        r#""cycle": { "kind": "cycle", "step_seconds": 0, "fade_seconds": 0.1 },"#,
        r#""cycle": { "kind": "cycle", "step_seconds": -1, "fade_seconds": 0.1 },"#,
    ] {
        let mut show = Show::boot(Authored {
            cycle,
            durations: true,
            green_trigger: true,
            ..Authored::default()
        });
        let mut frames = show.run(HALF_SECOND * 6);
        show.activate(BLUE);
        frames.extend(show.run(HALF_SECOND * 6));
        show.press(BLAST_PIN);
        frames.extend(show.run(HALF_SECOND * 6));
        println!("cycle {cycle:?}: {:?}", visited(&frames));
        runs.push((cycle, frames));
    }

    let (_, absent) = &runs[0];
    assert_eq!(
        visited(absent),
        [IDLE, BLUE, GREEN, BLUE],
        "blue holds past its duration; the trigger's green plays 0.5 s and returns to blue"
    );
    for (cycle, frames) in &runs[1..] {
        assert_eq!(
            frames, absent,
            "cycle {cycle:?} must be frame-for-frame the playlist without a cycle"
        );
    }
}

/// A trigger while cycling is an excursion: the entry plays its duration,
/// then the cycle carries on where its clock says (it is not re-anchored).
#[test]
fn a_trigger_while_cycling_plays_its_duration_and_the_cycle_carries_on() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 2.0, "fade_seconds": 0.1 },"#,
        durations: true,
        green_trigger: true,
        ..Authored::default()
    });
    show.run(8);
    assert_eq!(show.loaded(), [IDLE]);

    show.press(BLAST_PIN);
    let walk = show.visited(HALF_SECOND * 6);
    println!("blast while idle's 2 s step runs: {walk:?}");
    assert_eq!(
        &walk[..3],
        [GREEN, IDLE, GREEN],
        "green's half second, back to idle for the rest of its step, then the cycle's green"
    );
}

/// Re-tapping the entry the cycle started from writes the same key again.
/// It is still a pick: the cycle restarts there (vision Q2).
#[test]
fn re_picking_the_same_entry_restarts_the_cycle_there() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });
    show.run(8);
    show.activate(BLUE);
    let on = show.visited(HALF_SECOND + HALF_SECOND / 2);
    assert_eq!(
        on.last(),
        Some(&RED),
        "the cycle walked on from blue: {on:?}"
    );

    show.activate(BLUE);
    let again = show.visited(HALF_SECOND / 2);
    assert_eq!(
        again.last(),
        Some(&BLUE),
        "the same key, written again: {again:?}"
    );
}

/// A pick restored before the first tick (`.lp/panel.json`, panel.md P10)
/// is settled into the playlist before it ever ticks, so the first tick's
/// residency step swaps the authored entry out before anything renders:
/// no frame ever shows the authored entry (playlists-on-the-bus Q3).
#[test]
fn a_pick_settled_before_the_first_tick_is_the_first_entry_shown() {
    let mut show = Show::boot(Authored::default());
    show.activate(BLUE);
    show.rt.engine_mut().settle_load_deciding_panel_values();

    let frames = show.run(HALF_SECOND * 2);
    println!("restored pick blue: {:?}", visited(&frames));
    assert_eq!(frames[0], [BLUE], "the first tick has already swapped");
    assert_eq!(visited(&frames), [BLUE], "and it holds");
}

/// The cycle follows the clock (plan A3): at rate 2× a step takes half the
/// frames, and a paused clock holds the cycle where it is.
#[test]
fn the_cycle_follows_the_clock_rate_and_pause() {
    let cycle = r#""cycle": { "kind": "cycle", "step_seconds": 1.0, "fade_seconds": 0.1 },"#;

    let mut normal = Show::boot(Authored {
        cycle,
        ..Authored::default()
    });
    let normal_frames = normal.run(HALF_SECOND * 9);

    let mut fast = Show::boot(Authored {
        cycle,
        ..Authored::default()
    });
    fast.clock_write("clock.rate", LpValue::F32(2.0));
    let fast_frames = fast.run(HALF_SECOND * 9);

    let normal_green = frames_on(&normal_frames, GREEN);
    let fast_green = frames_on(&fast_frames, GREEN);
    println!(
        "frames on green: rate 1× {normal_green}, rate 2× {fast_green}; \
         walks {:?} / {:?}",
        visited(&normal_frames),
        visited(&fast_frames)
    );
    assert!(
        (60..=66).contains(&normal_green),
        "a 1 s step at 1× is ~62 frames: {normal_green}"
    );
    assert!(
        (29..=34).contains(&fast_green),
        "the same step at 2× is half the frames: {fast_green}"
    );

    // Pause on the entry playing now: nothing moves while paused.
    let playing = *fast.loaded().first().expect("an entry is loaded");
    fast.clock_write(
        "clock.play_state",
        LpValue::String(String::from(lpc_model::PlayState::Paused.as_str())),
    );
    let paused = fast.run(HALF_SECOND * 8);
    assert!(
        paused.iter().all(|loaded| *loaded == [playing]),
        "a paused clock freezes the cycle on {playing}: {:?}",
        visited(&paused)
    );
    fast.clock_write(
        "clock.play_state",
        LpValue::String(String::from(lpc_model::PlayState::Playing.as_str())),
    );
    let resumed = visited(&fast.run(HALF_SECOND * 3));
    assert!(resumed.len() > 1, "playing again, it moves on: {resumed:?}");
}

/// With the cycle on, the idle entry is an ordinary stop the cycle moves on
/// from (plan A2); with it off, the playlist rests on idle as always.
#[test]
fn idle_is_an_ordinary_stop_while_cycling() {
    let mut cycling = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });
    let walk = cycling.visited(HALF_SECOND * 11);
    assert_eq!(
        walk.iter().filter(|entry| **entry == IDLE).count(),
        3,
        "idle is visited each pass and left each time: {walk:?}"
    );
    assert_ne!(walk.last(), Some(&IDLE), "the cycle never rests on idle");

    let mut resting = Show::boot(Authored::default());
    assert_eq!(resting.visited(HALF_SECOND * 11), [IDLE]);
}

/// Both Play-mode controls take a panel write over the authored default
/// (plan A1): the skip list, and the cycle itself.
#[test]
fn an_authored_skip_is_honoured_and_a_panel_write_overrides_it() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        skip: r#""skip": [2, 3],"#,
        ..Authored::default()
    });
    let authored = show.visited(HALF_SECOND * 6);
    assert_eq!(&authored[..3], [IDLE, RED, IDLE], "2 and 3 authored off");

    show.playlist_write("playlist.skip", Vec::from([RED]).to_lp_value());
    let overridden = visited(&show.run(HALF_SECOND * 8));
    println!("authored {authored:?}, panel skip [4]: {overridden:?}");
    assert!(!overridden[1..].contains(&RED), "{overridden:?}");
    assert!(
        overridden.contains(&GREEN) && overridden.contains(&BLUE),
        "the panel's list replaces the authored one: {overridden:?}"
    );

    // The cycle, too: a panel hold stops the walk where it is.
    show.playlist_write("playlist.cycle", PlaylistCycle::Hold.to_lp_value());
    let held = visited(&show.run(HALF_SECOND * 6));
    assert!(held.len() <= 2, "held: {held:?}");
}

/// Skipping the entry playing does not cut it: it plays out its step.
#[test]
fn skipping_the_playing_entry_waits_for_the_next_step() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });
    show.visited(HALF_SECOND + 4);
    assert_eq!(show.loaded(), [GREEN]);

    show.playlist_write("playlist.skip", Vec::from([GREEN]).to_lp_value());
    let frames = show.run(HALF_SECOND + HALF_SECOND / 2);
    let still_green = frames
        .iter()
        .take_while(|loaded| **loaded == [GREEN])
        .count();
    assert!(
        still_green >= HALF_SECOND - 8,
        "green plays out its step: {still_green} frames"
    );
    assert_eq!(visited(&frames), [GREEN, BLUE]);
}

/// Every entry skipped: the playlist holds what is playing.
#[test]
fn skipping_every_entry_holds_the_current_one() {
    let mut show = Show::boot(Authored {
        cycle: r#""cycle": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        skip: r#""skip": [1, 2, 3, 4],"#,
        ..Authored::default()
    });
    assert_eq!(show.visited(HALF_SECOND * 6), [IDLE]);
}

/// A `Button` → `bus:trigger` wiring (the fyeah-sign pattern) steps through
/// the entries with `next_trigger_ids` / `prev_trigger_ids`, with the cycle
/// off — and passes over a skipped entry.
#[test]
fn next_and_prev_buttons_step_through_the_entries() {
    let mut show = Show::boot(Authored {
        skip: r#""skip": [3],"#,
        ..Authored::default()
    });
    show.run(8);
    assert_eq!(show.loaded(), [IDLE]);

    let mut steps = Vec::new();
    for pin in [NEXT_PIN, NEXT_PIN, NEXT_PIN, PREV_PIN, PREV_PIN] {
        show.press(pin);
        steps.push(*show.loaded().first().expect("an entry is loaded"));
    }
    println!("next, next, next, prev, prev: {steps:?}");
    assert_eq!(
        steps,
        [GREEN, RED, IDLE, RED, GREEN],
        "next wraps from red to idle past the skipped blue; prev walks back"
    );
}

/// Nothing authored and nothing written costs a steady frame nothing that an
/// authored cycle and skip list do not: "absent" is recognised by type, not by
/// formatting and searching an error message every frame per playlist.
#[test]
fn an_absent_cycle_and_skip_cost_a_steady_frame_nothing_extra() {
    let mut per_frame = Vec::new();
    for (cycle, skip) in [
        ("", ""),
        (
            r#""cycle": { "kind": "hold", "step_seconds": 0, "fade_seconds": 0 },"#,
            r#""skip": [],"#,
        ),
    ] {
        let mut show = Show::boot(Authored {
            cycle,
            skip,
            ..Authored::default()
        });
        show.run(HALF_SECOND * 2);
        let before = alloc_count::allocs();
        for tick in 0..HALF_SECOND {
            show.rt
                .tick_with_residency(&show.fs, 16)
                .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
        }
        per_frame.push(alloc_count::allocs() - before);
    }
    println!("allocations over {HALF_SECOND} steady frames, absent vs authored: {per_frame:?}");
    assert!(
        per_frame[0] <= per_frame[1],
        "an absent cycle and skip list must allocate nothing an authored one does not: \
         {per_frame:?}"
    );
}

// ---- fixture ----------------------------------------------------------------

/// What a test authors on the playlist.
#[derive(Default)]
struct Authored {
    /// A `"cycle": …,` line, or nothing.
    cycle: &'static str,
    /// A `"skip": […],` line, or nothing.
    skip: &'static str,
    /// Green, blue and red last 0.5 s each — which only an excursion uses.
    durations: bool,
    /// Green is triggered by the blast button (message id 9).
    green_trigger: bool,
    /// Add entry 5, a `ref` to a missing file.
    broken_entry: bool,
}

struct Show {
    fs: LpFsMemory,
    rt: LoadedProjectRuntime,
    buttons: VirtualButtonDriver,
}

impl Show {
    fn boot(authored: Authored) -> Self {
        let fs = project_fs(&authored);
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let buttons = driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let mut services = EngineServices::new(TreePath::parse("/cycle.show").expect("path"));
        services.set_button_service(Some(button_service));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load cycle project");
        rt.engine_mut().set_graphics(Some(std::sync::Arc::new(
            lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
        )));
        Self { fs, rt, buttons }
    }

    /// The loaded entry after each of `ticks` frames.
    fn run(&mut self, ticks: usize) -> Vec<Vec<u32>> {
        (0..ticks)
            .map(|tick| {
                self.rt
                    .tick_with_residency(&self.fs, 16)
                    .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
                self.loaded()
            })
            .collect()
    }

    /// The entries loaded over `ticks` frames, repeats collapsed.
    fn visited(&mut self, ticks: usize) -> Vec<u32> {
        visited(&self.run(ticks))
    }

    /// Press and release a button, a few frames each (it debounces).
    fn press(&mut self, pin: u32) {
        self.buttons.set_pressed(HwAddress::gpio(pin), true);
        self.run(4);
        self.buttons.set_pressed(HwAddress::gpio(pin), false);
        self.run(8);
    }

    /// Pick `entry` from Play mode: a panel write on `playlist.entry`.
    fn activate(&mut self, entry: u32) {
        self.playlist_write(lpc_model::PLAYLIST_ENTRY_CHANNEL, LpValue::U32(entry));
    }

    /// A panel write in the playlist's scope (Play mode's control surface).
    fn playlist_write(&mut self, channel: &str, value: LpValue) {
        let scope = self.scope_of(self.playlist_id());
        self.rt
            .engine_mut()
            .panel_write(scope, ChannelName(String::from(channel)), value, None);
    }

    /// A panel write on the clock's transport channels.
    fn clock_write(&mut self, channel: &str, value: LpValue) {
        let clock = self
            .rt
            .engine()
            .project_runtime_index()
            .node_id(&NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").expect("slot")))
            .expect("clock projected");
        let scope = self.scope_of(clock);
        self.rt
            .engine_mut()
            .panel_write(scope, ChannelName(String::from(channel)), value, None);
    }

    fn scope_of(&self, node: NodeId) -> ScopeRef {
        self.rt.engine().tree().node_scope(node).expect("scope")
    }

    fn playlist_id(&self) -> NodeId {
        self.rt
            .engine()
            .project_runtime_index()
            .node_id(&NodeUseLocation::root().child(SlotPath::parse("nodes[list]").expect("slot")))
            .expect("playlist projected")
    }

    fn loaded(&self) -> Vec<u32> {
        let owner = self.playlist_id();
        ALL.into_iter()
            .filter(|entry| {
                let scope = ScopeRef::Sink {
                    owner,
                    entry: *entry,
                };
                self.rt
                    .engine()
                    .tree()
                    .entries()
                    .any(|node| node.parent == Some(owner) && node.scope == Some(scope))
            })
            .collect()
    }

    fn warning(&self) -> String {
        let entry = self
            .rt
            .engine()
            .tree()
            .get(self.playlist_id())
            .expect("playlist entry");
        match entry.status.value() {
            NodeRuntimeStatus::Warn(text) => text.clone(),
            other => format!("{other:?}"),
        }
    }
}

/// The entries loaded frame by frame, with repeats collapsed.
fn visited(frames: &[Vec<u32>]) -> Vec<u32> {
    let mut visited: Vec<u32> = Vec::new();
    for loaded in frames {
        for entry in loaded {
            if visited.last() != Some(entry) {
                visited.push(*entry);
            }
        }
    }
    visited
}

/// Frames spent with `entry` loaded, in its first full run.
fn frames_on(frames: &[Vec<u32>], entry: u32) -> usize {
    frames
        .iter()
        .skip_while(|loaded| **loaded != [entry])
        .take_while(|loaded| **loaded == [entry])
        .count()
}

fn project_fs(authored: &Authored) -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", b"{\n  \"format\": 12\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "next": { "ref": "./next.json" },
    "prev": { "ref": "./prev.json" },
    "blast": { "ref": "./blast.json" },
    "list": { "ref": "./playlist.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    for (name, pin, id) in [("next", "D9", 7), ("prev", "D8", 8), ("blast", "D4", 9)] {
        write(
            &fs,
            &format!("/{name}.json"),
            format!(
                r#"{{
  "kind": "Button",
  "endpoint": "button:local:{pin}",
  "id": {id},
  "stable_ms": 1,
  "bindings": {{ "down": {{ "target": "bus:trigger" }} }}
}}"#
            )
            .as_bytes(),
        );
    }
    let duration = if authored.durations {
        r#""duration": 0.5, "#
    } else {
        ""
    };
    let green_trigger = if authored.green_trigger {
        r#""trigger_ids": [9], "#
    } else {
        ""
    };
    let broken = if authored.broken_entry {
        r#",
    "5": { "name": "broken", "node": { "ref": "./missing.json" } }"#
    } else {
        ""
    };
    write(
        &fs,
        "/playlist.json",
        format!(
            r#"{{
  "kind": "Playlist",
  "bindings": {{
    "time": {{ "source": "bus:time" }},
    "trigger": {{ "source": "bus:trigger" }},
    "output": {{ "target": "bus:visual.out" }}
  }},
  "idle_entry": 1,
  "default_fade": 0.1,
  {cycle}
  {skip}
  "next_trigger_ids": [7],
  "prev_trigger_ids": [8],
  "entries": {{
    "1": {{ "name": "idle", "node": {{ "ref": "./idle.json" }} }},
    "2": {{ "name": "green", {green_trigger}{duration}"node": {{ "ref": "./green.json" }} }},
    "3": {{ "name": "blue", {duration}"node": {{ "ref": "./blue.json" }} }},
    "4": {{ "name": "red", {duration}"node": {{ "ref": "./red.json" }} }}{broken}
  }}
}}"#,
            cycle = authored.cycle,
            skip = authored.skip,
        )
        .as_bytes(),
    );
    shader(
        &fs,
        "idle",
        "vec4 render_2d(vec2 pos) {\n    \
         return vec4(fract(pos.x * 0.37), fract(pos.y * 0.21), 0.5, 1.0);\n}\n",
    );
    for (name, rgb) in [
        ("green", "0.0, 1.0, 0.0"),
        ("blue", "0.0, 0.0, 1.0"),
        ("red", "1.0, 0.0, 0.0"),
    ] {
        shader(
            &fs,
            name,
            &format!("vec4 render_2d(vec2 pos) {{\n    return vec4({rgb}, 1.0);\n}}\n"),
        );
    }
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

/// Allocation requests on the current thread, for the steady-frame test.
/// Thread-local, so the other tests running in parallel do not count.
mod alloc_count {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct CountingAlloc;

    thread_local! {
        static ALLOCS: Cell<u64> = const { Cell::new(0) };
    }

    #[global_allocator]
    static COUNTING: CountingAlloc = CountingAlloc;

    pub fn allocs() -> u64 {
        ALLOCS.try_with(Cell::get).unwrap_or(0)
    }

    fn bump() {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
    }

    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            bump();
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            bump();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            bump();
            unsafe { System.realloc(ptr, layout, new_size) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }
}
