//! End-to-end drawing tests (plan `drawing-on-the-picture`, P5) against an
//! in-process LightPlayer server: the open session's picture is drawable
//! exactly while a `touches` reader is loaded, and a stroke reported on it
//! reaches the engine as paced momentary touch-set writes.
//!
//! The fixtures are shaped like the catalog's: a root module holding a
//! clock, a real lamp fixture (`projects/test/basic`'s disc), an output, and
//! either a playlist of pattern MODULES (one reads `touches`, one does not)
//! or one shader at the root reading `touches` itself.
//!
//! The connected board's card (and a watched card, and Done) is proven on
//! the device bench: `studio_device_e2e_tests/draw_tests.rs`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, FromLpValue, PanelWriteValue, Touch};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;

use super::studio_edit_e2e_tests::project_editor;
use super::studio_pattern_picker_e2e_tests::{Session, root_panel};
use crate::{
    OfferArgs, PLAYLIST_ENTRY_PARAM, PLAYLIST_PLAY_VERB, StudioCommand, StudioController,
    UiDrawSurface, UiDrawable, UiModuleFace, UiNodeFace, UiStroke, UiStrokePointer, UiStudioView,
};

#[test]
fn the_playing_patterns_touches_reader_makes_the_picture_drawable_into_its_scope() {
    let mut session = Session::connect_to(
        playlist_server(),
        StudioController::connected_with_client_for_test,
    );
    let drawable = wait_drawable(&mut session);
    assert_eq!(drawable.surface, UiDrawSurface::Project);
    assert_eq!(drawable.target.channel, "touches");

    // The pattern's own knobs group resets the module's scope: the touches
    // go to the same one, not the root's and not the entry's sink.
    let panel = root_panel(&session.view);
    let knobs = panel
        .groups
        .iter()
        .find(|group| group.label == "Drawing")
        .expect("the playing pattern's knobs");
    assert_eq!(Some(drawable.target.scope), knobs.target);
    assert_ne!(Some(drawable.target.scope), panel.target, "not the root");
    assert!(!drawable.target.scope.is_sink());

    // The agent reads it in one line.
    let readout = session
        .actor
        .controller_mut_for_test()
        .app_agent_readout_for_test();
    assert!(
        readout
            .text
            .contains("the project's picture is drawable: a finger dragged on it draws"),
        "{}",
        readout.text
    );

    // A pattern with no reader: not drawable.
    session.press(
        PLAYLIST_PLAY_VERB,
        OfferArgs::new().with(PLAYLIST_ENTRY_PARAM, "2"),
    );
    assert!(
        session.try_refresh_until(|view| root_face(view).drawable.is_none()),
        "a pattern that reads no touches is not drawable"
    );
    let readout = session
        .actor
        .controller_mut_for_test()
        .app_agent_readout_for_test();
    assert!(!readout.text.contains("drawable"), "{}", readout.text);
}

#[test]
fn a_reader_at_the_root_is_drawable_into_the_root_scope() {
    let mut session = Session::connect_to(
        root_reader_server(),
        StudioController::connected_with_client_for_test,
    );
    let drawable = wait_drawable(&mut session);
    assert_eq!(
        Some(drawable.target.scope),
        root_panel(&session.view).target,
        "the root module's own scope"
    );
}

/// 120 Hz pointer samples write at most 30 times a second, each a momentary
/// (400 ms) touch set with the newest position in pattern space; a resting
/// finger is renewed; the release is one empty set, and then nothing.
#[test]
fn a_stroke_writes_paced_momentary_touch_sets_renews_and_releases() {
    let mut session = Session::connect_to(
        playlist_server(),
        StudioController::connected_with_client_for_test,
    );
    let drawable = wait_drawable(&mut session);
    let surface = drawable.surface;

    // The picture's centre is the lamp box's centre: the disc is centred.
    let mut writes: Vec<(f64, Vec<Touch>)> = Vec::new();
    let mut now = 0.0;
    for sample in 0..60 {
        let x = 0.5 + 0.2 * (f64::from(sample) / 60.0) as f32;
        let before = panel_writes(&session).len();
        session.send(StudioCommand::Stroke(UiStroke::new(
            surface,
            vec![UiStrokePointer {
                pointer: 9001,
                x,
                y: 0.5,
            }],
        )));
        for write in panel_writes(&session).split_off(before) {
            assert_eq!(write.scope, drawable.target.scope);
            assert_eq!(write.channel, "touches");
            assert_eq!(write.ttl_ms, Some(400), "momentary");
            writes.push((now, touches(&write.value)));
        }
        advance(&mut session, 1.0 / 120.0);
        now += 1.0 / 120.0;
    }
    // Half a second of samples: 15 writes at 30 Hz, never closer than 33 ms.
    assert!(
        (13..=16).contains(&writes.len()),
        "{} writes in half a second",
        writes.len()
    );
    for pair in writes.windows(2) {
        assert!(pair[1].0 - pair[0].0 >= 0.033 - 1e-6);
    }
    let first = &writes[0].1;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, 1, "core's own small id, not the browser's");
    assert!(
        first[0].pos[0].abs() < 1e-3 && first[0].pos[1].abs() < 1e-3,
        "the picture's centre is pattern space's origin: {:?}",
        first[0].pos
    );
    let last = &writes.last().expect("writes").1;
    assert!(last[0].pos[0] > first[0].pos[0], "the newest position");

    // The finger stops: the newest position it reached goes out on the
    // next tick (the samples after the last write were not lost) ...
    let before = panel_writes(&session).len();
    advance(&mut session, 0.04);
    session.send(StudioCommand::RefreshTick);
    let newest = panel_writes(&session).split_off(before);
    assert_eq!(newest.len(), 1, "the newest position");
    let resting = touches(&newest[0].value);
    assert!(resting[0].pos[0] >= last[0].pos[0]);
    // ... and then it rests: a renewal within 150 ms, the same set.
    let before = panel_writes(&session).len();
    advance(&mut session, 0.16);
    session.send(StudioCommand::RefreshTick);
    let renewals = panel_writes(&session).split_off(before);
    assert_eq!(renewals.len(), 1, "one renewal");
    assert_eq!(touches(&renewals[0].value), resting);

    // Up: one empty set, then nothing.
    let before = panel_writes(&session).len();
    session.send(StudioCommand::Stroke(UiStroke::released(surface)));
    for _ in 0..4 {
        advance(&mut session, 0.2);
        session.send(StudioCommand::RefreshTick);
    }
    let after = panel_writes(&session).split_off(before);
    assert_eq!(after.len(), 1, "one release: {after:?}");
    assert_eq!(after[0].value, PanelWriteValue::Map(Vec::new()));
    assert_eq!(after[0].ttl_ms, Some(400));

    // Every write was accepted: no warning in the console.
    let console = session.view.console.entries.clone();
    assert!(
        !console
            .iter()
            .any(|entry| entry.message.contains("Couldn't set panel control")),
        "{console:#?}"
    );
}

/// A tap whose down and up reach the actor in one batch is still a dab:
/// the release never swallows the write before it.
#[test]
fn a_tap_in_one_batch_writes_its_dab_and_its_release() {
    let mut session = Session::connect_to(
        root_reader_server(),
        StudioController::connected_with_client_for_test,
    );
    let drawable = wait_drawable(&mut session);
    let before = panel_writes(&session).len();
    session.send_batch(vec![
        StudioCommand::Stroke(UiStroke::new(
            drawable.surface,
            vec![UiStrokePointer {
                pointer: 5,
                x: 0.5,
                y: 0.5,
            }],
        )),
        StudioCommand::Stroke(UiStroke::released(drawable.surface)),
    ]);
    let writes = panel_writes(&session).split_off(before);
    assert_eq!(writes.len(), 2, "{writes:?}");
    assert_eq!(touches(&writes[0].value).len(), 1, "the dab");
    assert_eq!(
        writes[1].value,
        PanelWriteValue::Map(Vec::new()),
        "the release"
    );
}

/// The picture runs at the editor's pace while a finger draws and for a
/// second after it lifts, then goes back to its own: here Play over
/// Bluetooth, whose idle lens reads once a minute.
#[test]
fn the_picture_reads_at_the_editors_pace_during_a_stroke_and_a_second_after() {
    let mut session = Session::connect_to(
        root_reader_server(),
        StudioController::connected_over_bluetooth_with_client_for_test,
    );
    let drawable = wait_drawable(&mut session);
    session
        .actor
        .controller_mut_for_test()
        .set_play_views_for_test(1);
    let gap = |session: &mut Session| {
        session
            .actor
            .controller_mut_for_test()
            .lens_refresh_gap_for_test()
            .expect("the lens reads")
    };
    assert_eq!(
        gap(&mut session),
        crate::app::studio::BLE_PLAY_IDLE_REFRESH_INTERVAL
    );

    session.send(StudioCommand::Stroke(UiStroke::new(
        drawable.surface,
        vec![UiStrokePointer {
            pointer: 1,
            x: 0.25,
            y: 0.75,
        }],
    )));
    assert_eq!(gap(&mut session), crate::DEVICE_REFRESH_INTERVAL);
    advance(&mut session, 0.5);
    session.send(StudioCommand::Stroke(UiStroke::released(drawable.surface)));
    advance(&mut session, 0.9);
    assert_eq!(
        gap(&mut session),
        crate::DEVICE_REFRESH_INTERVAL,
        "a second after the finger lifts"
    );
    advance(&mut session, 0.2);
    // The board accepted the writes, so the lens is still chasing their
    // verdict for a few reads (`VERDICT_CHASE_TICKS`, as after a knob turn);
    // once that is spent the gap is Play's own idle minute again.
    for _ in 0..crate::app::studio::VERDICT_CHASE_TICKS + 2 {
        session.send(StudioCommand::RefreshTick);
        advance(&mut session, 0.3);
    }
    assert_eq!(
        gap(&mut session),
        crate::app::studio::BLE_PLAY_IDLE_REFRESH_INTERVAL
    );
}

/// Refresh until the project's picture is drawable, and say what it is.
fn wait_drawable(session: &mut Session) -> UiDrawable {
    assert!(
        session.try_refresh_until(|view| root_face(view).drawable.is_some()),
        "the picture never became drawable"
    );
    root_face(&session.view).drawable.expect("drawable")
}

fn root_face(view: &UiStudioView) -> UiModuleFace {
    match project_editor(view)
        .nodes
        .first()
        .and_then(|node| node.face.clone())
    {
        Some(UiNodeFace::Module(face)) => face,
        _ => panic!("the root card wears a module face"),
    }
}

fn advance(session: &mut Session, secs: f64) {
    session
        .actor
        .controller_mut_for_test()
        .advance_clock_for_test(secs);
}

/// Every panel write the studio sent, in order.
fn panel_writes(session: &Session) -> Vec<lpc_wire::WirePanelWriteRequest> {
    session
        .sent
        .borrow()
        .iter()
        .filter_map(|message| match &message.msg {
            lpc_wire::ClientRequest::ProjectCommand {
                command: lpc_wire::WireProjectCommand::PanelWrite { request },
                ..
            } => Some(request.clone()),
            _ => None,
        })
        .collect()
}

fn touches(value: &PanelWriteValue) -> Vec<Touch> {
    let PanelWriteValue::Map(entries) = value else {
        panic!("a touch set is a map: {value:?}");
    };
    entries
        .iter()
        .map(|(key, value)| {
            let touch = Touch::from_lp_value(value).expect("a touch");
            assert_eq!(*key, touch.id);
            touch
        })
        .collect()
}

const DIR: &str = "/projects/draw-e2e";

/// A touch set as a shader consumes it: a sentinel map of four touches.
const TOUCH_SLOT: &str = r#"{
      "kind": "map",
      "value": "lp::touch::Touch",
      "key": "u32",
      "default": 0,
      "mapping": { "kind": "sentinel", "len": 4, "key": "id", "empty_key": 0 },
      "label": "",
      "description": "fingers"
    }"#;

const TOUCH_GLSL: &str = "\
struct Touch {
    uint id;
    vec2 pos;
};

layout(binding = 0) uniform float tail;
layout(binding = 1) uniform Touch touches[4];

vec4 render_2d(vec2 pos) {
    float lit = touches[0].id == 0u ? 0.0 : 1.0;
    return vec4(lit * tail, pos.y, 0.5, 1.0);
}
";

const PLAIN_GLSL: &str = "layout(binding = 0) uniform float tail;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x * tail, pos.y, 0.5, 1.0);\n}\n";

const PATTERN_MODULE: &str = r#"{
  "kind": "Module",
  "nodes": { "shader": { "ref": "./shader.json" } }
}"#;

/// A shader binding its own `tail` knob and, when `reads_touches`, the
/// `touches` channel — both in the scope it sits in.
fn shader_json(reads_touches: bool) -> String {
    let (binding, slot) = match reads_touches {
        true => (
            r#", "touches": { "source": "bus:touches" }"#.to_string(),
            format!(r#", "touches": {TOUCH_SLOT}"#),
        ),
        false => (String::new(), String::new()),
    };
    format!(
        r#"{{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {{
    "output": {{ "target": "bus:visual.out" }},
    "tail": {{ "source": "bus:tail" }}{binding}
  }},
  "consumed": {{
    "tail": {{
      "kind": "value",
      "value": "f32",
      "default": 0.4,
      "min": 0.05,
      "max": 1,
      "label": "Tail"
    }}{slot}
  }}
}}"#
    )
}

/// The rig every fixture shares: a clock, the basic disc fixture, an
/// output.
fn rig_files() -> Vec<(String, String)> {
    let basic = |name: &str| {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../projects/test/basic")
                .join(name),
        )
        .expect("projects/test/basic")
    };
    vec![
        ("project.json".into(), "{\n  \"format\": 11\n}\n".into()),
        (
            "clock.json".into(),
            r#"{ "kind": "Clock", "transport": { "play_state": "playing", "rate": 1.0 } }"#.into(),
        ),
        ("fixture.json".into(), basic("fixture.json")),
        ("fixture.map2d.json".into(), basic("fixture.map2d.json")),
        ("output.json".into(), basic("output.json")),
    ]
}

/// A playlist of two pattern modules: `drawing` (entry 1, playing) reads
/// touches, `plain` (entry 2) does not.
fn playlist_server() -> LpServer {
    server_with(&playlist_files())
}

/// [`playlist_server`]'s project files; the device bench pushes them to a
/// board.
pub(super) fn playlist_files() -> Vec<(String, String)> {
    let mut files = rig_files();
    files.push((
        "module.json".into(),
        r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "playlist": { "ref": "./playlist.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#
        .into(),
    ));
    files.push((
        "playlist.json".into(),
        r#"{
  "kind": "Playlist",
  "bindings": { "time": { "source": "bus:time" } },
  "idle_entry": 1,
  "default_fade": 0.0,
  "entries": {
    "1": { "name": "drawing", "node": { "ref": "./modules/drawing/module.json" } },
    "2": { "name": "plain", "node": { "ref": "./modules/plain/module.json" } }
  }
}"#
        .into(),
    ));
    for (folder, reads_touches, glsl) in
        [("drawing", true, TOUCH_GLSL), ("plain", false, PLAIN_GLSL)]
    {
        files.push((
            format!("modules/{folder}/module.json"),
            PATTERN_MODULE.into(),
        ));
        files.push((
            format!("modules/{folder}/shader.json"),
            shader_json(reads_touches),
        ));
        files.push((format!("modules/{folder}/shader.glsl"), glsl.into()));
    }
    files
}

/// No playlist and no pattern module: one shader at the root reads touches.
fn root_reader_server() -> LpServer {
    let mut files = rig_files();
    files.push((
        "module.json".into(),
        r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "fixture": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#
        .into(),
    ));
    files.push(("shader.json".into(), shader_json(true)));
    files.push(("shader.glsl".into(), TOUCH_GLSL.into()));
    server_with(&files)
}

fn server_with(files: &[(String, String)]) -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(DIR.as_path())
        .expect("load the project");
    server.advance_frame(16).expect("tick");
    server
}
