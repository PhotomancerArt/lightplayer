//! End-to-end node-card face flow against an in-process LightPlayer server
//! (node-card P3).
//!
//! Reuses the edit-e2e harness (`InProcessServerIo`, `drive`,
//! `project_action`): a real `LpServer` loads a clock + shader + fixture +
//! output project whose shader carries a panel-flagged `speed` uniform.
//! Asserts the controller-side face derivation end-to-end — shader knob and
//! fixture fader present with real addresses — and that knob/fader
//! `SetValue` dispatches ride the SAME overlay path the slot editors use
//! (value + dirty state flow back into the face control, Save commits the
//! persisted-class edit to the def file).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpValue};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;

use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive, project_action};
use crate::{
    ControllerId, ProjectController, ProjectOp, ProjectSlotAddress, SlotEditOp, StudioActor,
    StudioCommand, StudioController, StudioServerClient, UiAction, UiNodeDirtyState, UiNodeFace,
    UiNodeView, UiPanelControl, UiPanelWidget, UiSlotValueKind, UiStudioView, UiViewContent,
};

#[test]
fn node_faces_derive_and_edit_end_to_end() {
    let server = Rc::new(RefCell::new(face_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // -- shader face: knob from the panel-flagged uniform ------------------
    let shader = node_by_kind(&snapshot, "Shader");
    let Some(UiNodeFace::Shader(face)) = &shader.face else {
        panic!("shader node derives a shader face, got {:?}", shader.face);
    };
    assert_eq!(face.controls.len(), 1, "one panel-flagged uniform");
    let knob = &face.controls[0];
    assert_eq!(knob.label, "Speed");
    assert_eq!(
        knob.widget,
        UiPanelWidget::Knob {
            min: 0.0,
            max: 3.0,
            step: None
        }
    );
    assert_eq!(knob.value.kind, UiSlotValueKind::F32(1.0));
    let knob_address = knob.address.clone().expect("knob edits are addressed");
    assert_eq!(
        knob_address.path.to_string(),
        "consumed[speed].default.some"
    );
    assert!(
        face.code_drawer.is_some(),
        "code drawer reuses the inline GLSL editor"
    );

    // -- fixture face: fader from the brightness slot meta ------------------
    let fixture = node_by_kind(&snapshot, "Fixture");
    let Some(UiNodeFace::Fixture(face)) = &fixture.face else {
        panic!(
            "fixture node derives a fixture face, got {:?}",
            fixture.face
        );
    };
    assert_eq!(
        face.brightness.widget,
        UiPanelWidget::Fader {
            min: 0.0,
            max: 255.0,
            step: Some(1.0)
        }
    );
    assert_eq!(face.brightness.value.kind, UiSlotValueKind::U32(200));
    let fader_address = face
        .brightness
        .address
        .clone()
        .expect("fader edits are addressed");
    assert_eq!(fader_address.path.to_string(), "brightness.some");

    // -- fallback: the clock keeps the generic sections ---------------------
    assert_eq!(node_by_kind(&snapshot, "Clock").face, None);

    // -- knob drag flood: coalesced SetValues flow back into the face -------
    for value in [1.4_f32, 1.9, 2.5] {
        handle
            .tx
            .send(set_value_action(knob_address.clone(), LpValue::F32(value)));
    }
    handle
        .tx
        .send(set_value_action(fader_address.clone(), LpValue::U32(31)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    let knob = shader_knob(&snapshot);
    assert_eq!(knob.value.kind, UiSlotValueKind::F32(2.5));
    assert_eq!(knob.state.dirty, UiNodeDirtyState::Dirty);
    let fader = fixture_fader(&snapshot);
    assert_eq!(fader.value.kind, UiSlotValueKind::U32(31));
    assert_eq!(fader.state.dirty, UiNodeDirtyState::Dirty);

    // -- save: both edits commit through the ONE overlay write path ---------
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let shader_json = read_project_file(&server, "shader.json");
    assert!(
        shader_json.contains("\"default\":2.5"),
        "shader.json gained the knob's persisted default edit: {shader_json}"
    );
    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"brightness\":31"),
        "fixture.json gained the fader's persisted brightness edit: {fixture_json}"
    );
    assert_eq!(shader_knob(&snapshot).state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        fixture_fader(&snapshot).state.dirty,
        UiNodeDirtyState::Clean
    );
}

#[test]
fn playlist_face_derives_and_keeps_one_live_surface() {
    let server = Rc::new(RefCell::new(playlist_e2e_server(1)));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // -- the strip: entries from the def, ACTIVE from the runtime status ----
    let playlist = node_by_kind(&snapshot, "Playlist");
    let Some(UiNodeFace::Playlist(face)) = &playlist.face else {
        panic!(
            "playlist node derives a playlist face, got {:?}",
            playlist.face
        );
    };
    assert_eq!(
        face.active,
        Some(1),
        "ACTIVE follows PlaylistState.active_entry (idle_entry on load)"
    );
    assert_eq!(face.entries.len(), 2);
    let idle = &face.entries[0];
    assert_eq!((idle.key, idle.name.as_str()), (1, "idle"));
    assert_eq!(idle.duration_ms, None);
    assert!(!idle.cue);
    let cued = &face.entries[1];
    assert_eq!((cued.key, cued.name.as_str()), (2, "active"));
    assert_eq!(cued.duration_ms, Some(4000), "authored 4 s → 4000 ms chip");
    assert!(cued.cue, "trigger_ids entry reads as a cue entry");

    // -- one live surface: exactly the ACTIVE entry's child below the card --
    assert_eq!(playlist.children.len(), 1, "only the active child renders");
    let child = &playlist.children[0];
    assert_eq!(child.label, "Idle");
    assert!(
        !child.active,
        "the child card must not wear the selection look — ACTIVE lives on \
         the strip placard"
    );
    assert!(!child.focused);

    // -- strip click: the entry dispatches the child's node-select action ---
    let select_idle = idle
        .action
        .clone()
        .expect("strip entries carry the child select action");
    let select_active = cued
        .action
        .clone()
        .expect("non-active entries carry it too");
    handle.tx.send(StudioCommand::Action(select_idle));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("focus emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    assert!(
        playlist.children[0].focused,
        "clicking the active entry focuses its (rendered) child"
    );

    // Clicking a non-active entry selects its (strip-represented) child;
    // the strip stays the single surface for it — child list unchanged.
    handle.tx.send(StudioCommand::Action(select_active));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("focus emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    assert_eq!(playlist.children.len(), 1);
    assert_eq!(playlist.children[0].label, "Idle");
}

#[test]
fn playlist_with_unresolvable_active_entry_keeps_all_children() {
    // The runtime status names entry 9, which exists neither in the strip
    // nor as a mounted child (authored dangling `idle_entry`) — the face
    // must not derive and the card falls back to today's full rendering
    // (never a blank card). The missing-status arm is unit-covered in
    // `node_face_builder` (the in-process server publishes the state root,
    // `active_entry` included, from the moment the project loads, so
    // status absence is not reachable end-to-end).
    let server = Rc::new(RefCell::new(playlist_e2e_server(9)));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let playlist = node_by_kind(&snapshot, "Playlist");
    assert_eq!(playlist.face, None, "unresolvable ACTIVE → no face");
    assert_eq!(
        playlist.children.len(),
        2,
        "fallback emits all children exactly as today"
    );
}

// -- harness -----------------------------------------------------------------

const PROJECT_DIR: &str = "/projects/face-e2e";

/// The shader uses the panel uniform so its compile stays honest.
const FACE_SHADER: &str = "layout(binding = 0) uniform float speed;\n\nvec4 render(vec2 pos) {\n    return vec4(pos.x * speed, pos.y, 0.5, 1.0);\n}\n";

fn face_e2e_server() -> LpServer {
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

    let project_json = r#"{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "controls": { "running": true, "rate": 1.0 }
}"#;
    let shader_json = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "output": { "target": "bus:visual.out" }
  },
  "consumed": {
    "speed": {
      "kind": "value",
      "value": "f32",
      "default": 1,
      "min": 0,
      "max": 3,
      "label": "Speed",
      "description": "Gradient speed multiplier",
      "panel": true
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "brightness": 200,
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    let output_json = r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": { "source": "bus:control.out" }
  }
}"#;
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
        ("shader.glsl", FACE_SHADER),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load face-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

const PLAYLIST_PROJECT_DIR: &str = "/projects/playlist-face-e2e";

/// A playlist project: clock + playlist (idle entry 1 + cue entry 2 with a
/// 4 s duration) + fixture + output. `idle_entry` is authored verbatim —
/// pass a key with no entry to exercise the unresolvable-ACTIVE fallback.
fn playlist_e2e_server(idle_entry: u32) -> LpServer {
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

    let project_json = r#"{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "playlist": { "ref": "./playlist.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "controls": { "running": true, "rate": 1.0 }
}"#;
    let playlist_json = format!(
        r#"{{
  "kind": "Playlist",
  "bindings": {{
    "time": {{ "source": "bus:time" }}
  }},
  "idle_entry": {idle_entry},
  "default_fade": 0.25,
  "entries": {{
    "1": {{ "name": "idle", "node": {{ "ref": "./idle.json" }} }},
    "2": {{
      "name": "active",
      "trigger_ids": [1],
      "duration": 4,
      "node": {{ "ref": "./active.json" }}
    }}
  }}
}}"#
    );
    let idle_json = r#"{ "kind": "Shader", "source": "idle.glsl" }"#;
    let active_json = r#"{ "kind": "Shader", "source": "active.glsl" }"#;
    let entry_glsl = "vec4 render(vec2 pos) {\n    return vec4(pos.x, pos.y, 0.5, 1.0);\n}\n";
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    let output_json = r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": { "source": "bus:control.out" }
  }
}"#;
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("clock.json", clock_json),
        ("playlist.json", playlist_json.as_str()),
        ("idle.json", idle_json),
        ("active.json", active_json),
        ("idle.glsl", entry_glsl),
        ("active.glsl", entry_glsl),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(
                format!("{PLAYLIST_PROJECT_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(PLAYLIST_PROJECT_DIR.as_path())
        .expect("load playlist-face-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

fn set_value_action(address: ProjectSlotAddress, value: LpValue) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    ))
}

fn read_project_file(server: &Rc<RefCell<LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    String::from_utf8(bytes)
        .expect("utf8 project file")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn node_by_kind<'a>(view: &'a UiStudioView, kind: &str) -> &'a UiNodeView {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");
    editor
        .nodes
        .iter()
        .find(|node| node.header.kind == kind)
        .unwrap_or_else(|| panic!("project carries a {kind} node"))
}

fn shader_knob(view: &UiStudioView) -> &UiPanelControl {
    let Some(UiNodeFace::Shader(face)) = &node_by_kind(view, "Shader").face else {
        panic!("shader face present");
    };
    &face.controls[0]
}

fn fixture_fader(view: &UiStudioView) -> &UiPanelControl {
    let Some(UiNodeFace::Fixture(face)) = &node_by_kind(view, "Fixture").face else {
        panic!("fixture face present");
    };
    &face.brightness
}
