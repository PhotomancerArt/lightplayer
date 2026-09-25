//! Touring and next/prev, end to end (multi-pattern plan P5, vision D13,
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
//! cargo test -p lpc-engine --test playlist_tour -- --nocapture
//! ```

use std::rc::Rc;

use lpc_engine::engine::{ButtonService, LoadedProjectRuntime};
use lpc_engine::node::ScopeRef;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_hardware::{
    HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver, default_esp32c6_hardware_manifest,
};
use lpc_model::{
    ChannelName, LpValue, NodeId, NodeRuntimeStatus, NodeUseLocation, PlaylistTour, SlotPath,
    ToLpValue, TreePath,
};
use lpc_wire::WireNodeCommand;
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

/// Frames in one half-second step at 16 ms a frame.
const HALF_SECOND: usize = 32;

#[test]
fn a_cycle_walks_the_entries_in_key_order_and_wraps() {
    let mut show = Show::boot(Authored {
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
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
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
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
fn a_pick_while_touring_jumps_and_the_tour_carries_on_from_it() {
    let mut show = Show::boot(Authored {
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
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

/// A held, frozen or absent tour is the playlist exactly as before P5: the
/// idle entry stays, an activate (a trigger's twin) plays its entry for its
/// duration, the timed advance walks the later entries, and the playlist
/// returns to idle.
#[test]
fn frozen_hold_and_absent_tours_play_the_playlist_as_before() {
    let mut runs = Vec::new();
    for tour in [
        "",
        r#""tour": { "kind": "hold", "step_seconds": 0, "fade_seconds": 0 },"#,
        r#""tour": { "kind": "cycle", "step_seconds": 0, "fade_seconds": 0.1 },"#,
        r#""tour": { "kind": "cycle", "step_seconds": -1, "fade_seconds": 0.1 },"#,
    ] {
        let mut show = Show::boot(Authored {
            tour,
            durations: true,
            ..Authored::default()
        });
        let resting = show.run(HALF_SECOND * 6);
        show.activate(GREEN);
        let triggered = show.run(HALF_SECOND * 6);
        let mut frames = resting;
        frames.extend(triggered);
        println!("tour {tour:?}: {:?}", visited(&frames));
        runs.push((tour, frames));
    }

    let (_, absent) = &runs[0];
    assert_eq!(
        visited(absent),
        [IDLE, GREEN, BLUE, RED, IDLE],
        "idle rests; each entry plays its duration; back to idle"
    );
    for (tour, frames) in &runs[1..] {
        assert_eq!(
            frames, absent,
            "tour {tour:?} must be frame-for-frame the playlist without a tour"
        );
    }
}

/// The tour follows the clock (plan A3): at rate 2× a step takes half the
/// frames, and a paused clock holds the tour where it is.
#[test]
fn the_tour_follows_the_clock_rate_and_pause() {
    let tour = r#""tour": { "kind": "cycle", "step_seconds": 1.0, "fade_seconds": 0.1 },"#;

    let mut normal = Show::boot(Authored {
        tour,
        ..Authored::default()
    });
    let normal_frames = normal.run(HALF_SECOND * 9);

    let mut fast = Show::boot(Authored {
        tour,
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
        "a paused clock freezes the tour on {playing}: {:?}",
        visited(&paused)
    );
    fast.clock_write(
        "clock.play_state",
        LpValue::String(String::from(lpc_model::PlayState::Playing.as_str())),
    );
    let resumed = visited(&fast.run(HALF_SECOND * 3));
    assert!(resumed.len() > 1, "playing again, it moves on: {resumed:?}");
}

/// With the tour on, the idle entry is an ordinary stop the tour moves on
/// from (plan A2); with it off, the playlist rests on idle as always.
#[test]
fn idle_is_an_ordinary_stop_while_touring() {
    let mut touring = Show::boot(Authored {
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        ..Authored::default()
    });
    let walk = touring.visited(HALF_SECOND * 11);
    assert_eq!(
        walk.iter().filter(|entry| **entry == IDLE).count(),
        3,
        "idle is visited each pass and left each time: {walk:?}"
    );
    assert_ne!(walk.last(), Some(&IDLE), "the tour never rests on idle");

    let mut resting = Show::boot(Authored::default());
    assert_eq!(resting.visited(HALF_SECOND * 11), [IDLE]);
}

/// Both Play-mode controls take a panel write over the authored default
/// (plan A1): the skip list, and the tour itself.
#[test]
fn an_authored_skip_is_honoured_and_a_panel_write_overrides_it() {
    let mut show = Show::boot(Authored {
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
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

    // The tour, too: a panel hold stops the walk where it is.
    show.playlist_write("playlist.tour", PlaylistTour::Hold.to_lp_value());
    let held = visited(&show.run(HALF_SECOND * 6));
    assert!(held.len() <= 2, "held: {held:?}");
}

/// Skipping the entry playing does not cut it: it plays out its step.
#[test]
fn skipping_the_playing_entry_waits_for_the_next_step() {
    let mut show = Show::boot(Authored {
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
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
        tour: r#""tour": { "kind": "cycle", "step_seconds": 0.5, "fade_seconds": 0.1 },"#,
        skip: r#""skip": [1, 2, 3, 4],"#,
        ..Authored::default()
    });
    assert_eq!(show.visited(HALF_SECOND * 6), [IDLE]);
}

/// A `Button` → `bus:trigger` wiring (the fyeah-sign pattern) steps through
/// the entries with `next_trigger_ids` / `prev_trigger_ids`, with the tour
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

// ---- fixture ----------------------------------------------------------------

/// What a test authors on the playlist.
#[derive(Default)]
struct Authored {
    /// A `"tour": …,` line, or nothing.
    tour: &'static str,
    /// A `"skip": […],` line, or nothing.
    skip: &'static str,
    /// Green, blue and red play 0.5 s each (the timed advance).
    durations: bool,
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
        let mut services = EngineServices::new(TreePath::parse("/tour.show").expect("path"));
        services.set_button_service(Some(button_service));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load tour project");
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

    fn activate(&mut self, entry: u32) {
        let playlist = self.playlist_id();
        self.rt
            .engine_mut()
            .handle_node_command(playlist, &WireNodeCommand::PlaylistActivateEntry { entry })
            .unwrap_or_else(|e| panic!("activate {entry}: {e}"));
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
    write(&fs, "/project.json", b"{\n  \"format\": 11\n}\n");
    write(
        &fs,
        "/module.json",
        br#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "next": { "ref": "./next.json" },
    "prev": { "ref": "./prev.json" },
    "list": { "ref": "./playlist.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#,
    );
    write(&fs, "/clock.json", br#"{ "kind": "Clock" }"#);
    for (name, pin, id) in [("next", "D9", 7), ("prev", "D8", 8)] {
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
  {tour}
  {skip}
  "next_trigger_ids": [7],
  "prev_trigger_ids": [8],
  "entries": {{
    "1": {{ "name": "idle", "node": {{ "ref": "./idle.json" }} }},
    "2": {{ "name": "green", {duration}"node": {{ "ref": "./green.json" }} }},
    "3": {{ "name": "blue", {duration}"node": {{ "ref": "./blue.json" }} }},
    "4": {{ "name": "red", {duration}"node": {{ "ref": "./red.json" }} }}{broken}
  }}
}}"#,
            tour = authored.tour,
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
