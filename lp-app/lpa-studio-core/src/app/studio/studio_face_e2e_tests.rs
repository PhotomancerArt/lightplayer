//! End-to-end node-card face flow against an in-process LightPlayer server
//! (node-card P3).
//!
//! Reuses the edit-e2e harness (`InProcessServerIo`, `drive`,
//! `project_action`): a real `LpServer` loads a clock + shader + fixture +
//! output project whose shader carries a bus-bound `speed` uniform.
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

use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, card_matching, drive, editor_dirty, project_action, project_editor,
};
use crate::{
    ControllerId, NodeCardUiState, NodeUiOp, PlaylistActivateOp, ProjectController,
    ProjectEditorOp, ProjectEditorTarget, ProjectOp, ProjectSlotAddress, SlotEditOp, StudioActor,
    StudioCommand, StudioController, StudioServerClient, UiAction, UiLogLevel, UiNodeDirtyState,
    UiNodeFace, UiNodeView, UiPanelControl, UiPanelWidget, UiPlaylistFace, UiSlotValueKind,
    UiStudioView,
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

    // -- shader face: knobs from the bound uniforms ------------------------
    let shader = node_by_kind(&snapshot, "Shader");
    let Some(UiNodeFace::Shader(face)) = &shader.face else {
        panic!("shader node derives a shader face, got {:?}", shader.face);
    };
    assert_eq!(face.controls.len(), 2, "both bound uniforms");
    let knob = control_labeled(face, "Speed");
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
    // The u32-shaped `count` uniform is a whole-number knob with no
    // authoring beyond its value shape.
    let count = control_labeled(face, "Count");
    assert_eq!(
        count.widget,
        UiPanelWidget::Knob {
            min: 1.0,
            max: 4.0,
            step: Some(1.0)
        },
        "an i32/u32 uniform snaps to whole numbers"
    );
    assert_eq!(count.value.kind, UiSlotValueKind::F32(2.0));
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
    let mapping_editor = face
        .mapping_editor
        .as_ref()
        .expect("map2d fixture derives the in-face mapping editor");
    assert_eq!(mapping_editor.source, "sign.map2d.json");

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
fn agent_collapse_preserves_the_composer_draft_end_to_end() {
    // The draft-survival contract, driven through the REAL seam: the node
    // key is the snapshot's own `header.path` (exactly what `NodePane`
    // hands the face), and the ops are the SAME sequence the web's
    // collapse control dispatches (`NodeUiOp::toggle_agent_section`). This
    // covers what the controller unit tests cannot — a key mismatch
    // between the derived DTO and the card-UI overlay would silently drop
    // the mirrored draft in the wired app while those tests stayed green.
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
    let shader = node_by_kind(&snapshot, "Shader");
    assert_eq!(
        shader.card_ui,
        NodeCardUiState::default(),
        "a fresh card starts expanded with no mirrored draft"
    );
    let node = shader.header.path.clone();

    // Collapse with a half-typed draft on hand: mirror rides first, then
    // the flip — the choreography the ShaderFace toggle dispatches.
    for op in NodeUiOp::toggle_agent_section(&node, false, "make it pulse, but slo") {
        handle.tx.send(node_ui_command(op));
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("collapse emits a snapshot");
    let card_ui = &node_by_kind(&snapshot, "Shader").card_ui;
    assert!(card_ui.agent_collapsed, "the section reads collapsed");
    assert_eq!(
        card_ui.composer_draft, "make it pulse, but slo",
        "the mirrored draft rides the DTO — the seed a remounting composer restores from"
    );

    // Expand: the flip alone (the composer was unmounted, so there is no
    // live draft to mirror) — the mirror must come back out untouched.
    for op in NodeUiOp::toggle_agent_section(&node, true, "") {
        handle.tx.send(node_ui_command(op));
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("expand emits a snapshot");
    let card_ui = &node_by_kind(&snapshot, "Shader").card_ui;
    assert!(!card_ui.agent_collapsed);
    assert_eq!(
        card_ui.composer_draft, "make it pulse, but slo",
        "collapse → expand round-trips the half-typed draft"
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

    // -- strip clicks: ACTIVE chip focuses the child, others activate -------
    let select_idle = idle
        .action
        .clone()
        .expect("the ACTIVE entry's chip carries the child select action");
    assert!(
        select_idle.op_as::<PlaylistActivateOp>().is_none(),
        "activating what already plays is a no-op — the ACTIVE chip keeps \
         the focus gesture"
    );
    let activate_cued = cued
        .action
        .clone()
        .expect("non-active entries carry the activate action");
    let activate_op = activate_cued
        .op_as::<PlaylistActivateOp>()
        .expect("non-active chip clicks are runtime activate pokes");
    assert_eq!(activate_op.entry, 2);

    handle.tx.send(StudioCommand::Action(select_idle));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("focus emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    assert!(
        playlist.children[0].focused,
        "clicking the active entry focuses its (rendered) child"
    );
    // The non-active click is a runtime poke, not a selection — covered
    // end-to-end in `playlist_entry_click_activates_on_the_real_server`.
}

#[test]
fn playlist_entry_click_activates_on_the_real_server() {
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
    let face = playlist_face(&snapshot);
    assert_eq!(face.active, Some(1));
    let activate = face.entries[1]
        .action
        .clone()
        .expect("non-active entry carries the activate action");

    // Click: the activate op rides the runtime command channel to the real
    // server (nothing staged — no overlay row, no dirty state); the
    // playlist validates and queues the switch, applying it on the next
    // engine frame (every in-process message ticks one).
    handle.tx.send(StudioCommand::Action(activate));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("dispatch emits a snapshot");
    assert_eq!(
        editor_dirty(&snapshot),
        (0, 0),
        "an activate poke stages nothing in the overlay"
    );

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    let face = playlist_face(&snapshot);
    assert_eq!(
        face.active,
        Some(2),
        "the ACTIVE placard advances to the clicked entry"
    );
    assert_eq!(
        playlist.children.len(),
        1,
        "one live surface: exactly the new active entry's child"
    );
    assert_eq!(playlist.children[0].label, "Active");
    // The chips swap roles with the placard: the newly active entry keeps
    // its child's select action, the idle entry becomes the activate poke.
    let idle_op = face.entries[0]
        .action
        .as_ref()
        .and_then(|action| action.op_as::<PlaylistActivateOp>())
        .expect("the now-inactive idle entry carries the activate action");
    assert_eq!(idle_op.entry, 1);
    assert!(
        face.entries[1]
            .action
            .as_ref()
            .is_some_and(|action| action.op_as::<PlaylistActivateOp>().is_none()),
        "the now-active entry's chip carries the child select action"
    );
}

#[test]
fn playlist_activate_rejects_an_unknown_entry_gracefully() {
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
    let face = playlist_face(&snapshot);
    let node = face.entries[1]
        .action
        .as_ref()
        .and_then(|action| action.op_as::<PlaylistActivateOp>())
        .expect("activate action carries the playlist address")
        .node
        .clone();
    let status_before = node_by_kind(&snapshot, "Playlist").header.status.clone();

    // A stale click (the entry vanished between render and dispatch): the
    // server answers a NORMAL Rejected response — a warning in the console,
    // never a transport error or a poisoned runtime status.
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PlaylistActivateOp {
            node: node.clone(),
            entry: 9,
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("rejection emits a snapshot");
    assert!(
        snapshot.console.entries.iter().any(|entry| {
            entry.level == UiLogLevel::Warn
                && entry.message.contains("Couldn't activate entry 9")
                && entry.message.contains("no loaded entry 9")
        }),
        "the rejection reason surfaces as a console warning: {:?}",
        snapshot.console.entries
    );

    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    assert_eq!(playlist.header.status, status_before, "no status poisoning");
    let face = playlist_face(&snapshot);
    assert_eq!(face.active, Some(1), "the active entry is untouched");
    assert_eq!(playlist.children.len(), 1);

    // The channel still works after a rejection: a valid activate lands.
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PlaylistActivateOp { node, entry: 2 },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("refresh emits a snapshot");
    assert_eq!(playlist_face(&snapshot).active, Some(2));
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

#[test]
fn a_bound_panel_uniform_keeps_an_interactive_control() {
    // The §4.1 regression shape (fyeah-sign): `glow` is bound to
    // `bus:glow`, `speed` is bound to nothing. The bound knob must stay a
    // working control — it derives a panel target (the command-channel write
    // path) AND keeps the editable, addressed authored default underneath
    // (modules.md R6: the authored default is what an unwritten channel
    // resolves to, so it stays reachable). Nothing pinned interactivity
    // before: the knob rendered correctly bound and dispatched nothing when
    // turned.
    //
    // Q13 (binding is publicity) also makes `speed` the negative case: with
    // the authored `panel` flag deleted, an unbound uniform has no control
    // at all.
    let server = Rc::new(RefCell::new(bound_glow_e2e_server()));
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

    let shader = node_by_kind(&snapshot, "Shader");
    let Some(UiNodeFace::Shader(face)) = &shader.face else {
        panic!("shader node derives a shader face, got {:?}", shader.face);
    };

    // The unbound uniform: not on the panel at all (Q13).
    assert!(
        face.controls.iter().all(|control| control.label != "Speed"),
        "an unbound uniform gets no knob, got {:?}",
        face.controls
            .iter()
            .map(|control| control.label.as_str())
            .collect::<Vec<_>>()
    );

    // The bound control derives its (scope, channel) write target…
    let glow = control_labeled(face, "Glow");
    assert!(
        glow.panel_target.is_some(),
        "a bound panel uniform derives a panel target end to end; aspects: {:?}",
        glow.aspects
    );
    assert!(glow.bound(), "the control wears the bound treatment");

    // …AND is still interactive. The widgets gate every gesture on an
    // editable state plus a dispatch route, so a readonly state or a missing
    // address+target is EXACTLY the inert-knob bug.
    assert!(
        glow.state.editable,
        "a bound panel control must stay editable — a readonly state makes \
         the widget dispatch nothing: {:?}",
        glow.state
    );
    assert_eq!(
        glow.address
            .as_ref()
            .map(|address| address.path.to_string()),
        Some("consumed[glow].default.some".to_string()),
        "the authored default stays addressed under the binding (R6 \
         fallback + advanced-editor edits)"
    );
    assert_eq!(
        glow.value.kind,
        UiSlotValueKind::F32(0.5),
        "the authored default value survives the binding"
    );
}

#[test]
fn a_bound_panel_uniform_inside_a_playlist_entry_stays_interactive() {
    // The EXACT fyeah-sign shape: the glow shader is not a root child — it
    // is playlist entry 1's node, so its card renders as the playlist's
    // child and its binding resolves inside the entry's sink scope. This is
    // the placement Yona actually turned the inert knob in; the flat-child
    // case above passes on its own.
    let server = Rc::new(RefCell::new(playlist_bound_glow_e2e_server()));
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
    assert_eq!(playlist.children.len(), 1, "the idle entry renders");
    let idle = &playlist.children[0];
    let Some(UiNodeFace::Shader(face)) = &idle.face else {
        panic!("idle child derives a shader face, got {:?}", idle.face);
    };

    let glow = control_labeled(face, "Glow");
    assert!(
        glow.panel_target.is_some(),
        "a bound panel uniform inside a playlist entry derives a panel \
         target; aspects: {:?}",
        glow.aspects
    );
    assert!(
        glow.state.editable,
        "the bound control stays editable inside the entry: {:?}",
        glow.state
    );
    assert_eq!(
        glow.address
            .as_ref()
            .map(|address| address.path.to_string()),
        Some("consumed[glow].default.some".to_string()),
        "the authored default stays addressed under the binding"
    );
    assert_eq!(glow.value.kind, UiSlotValueKind::F32(0.5));

    // Turn the knob for real: the EXACT op the widget dispatches
    // (`panel_or_slot_action` with a target present) must engage a writer
    // on the real server and flow back into the control as the live
    // reading. This is the interactivity assertion §4.1 was missing —
    // everything above can hold while a turned knob still does nothing.
    let target = glow.panel_target.clone().expect("checked above");
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelWriteOp {
            scope: target.scope,
            channel: target.channel.clone(),
            value: LpValue::F32(0.9),
            ttl_ms: None,
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("panel write emits a snapshot");

    let playlist = node_by_kind(&snapshot, "Playlist");
    let Some(UiNodeFace::Shader(face)) = &playlist.children[0].face else {
        panic!("idle child keeps its face");
    };
    let glow = control_labeled(face, "Glow");
    assert_eq!(
        glow.live_value.as_deref(),
        Some("0.9"),
        "the engaged writer's value flows back as the live reading"
    );
    let target = glow.panel_target.clone().expect("target survives");
    assert!(
        target.engaged,
        "the control reads engaged (drives the clear affordance)"
    );
    assert_eq!(
        editor_dirty(&snapshot),
        (0, 0),
        "a panel write stages nothing in the overlay"
    );

    // …and the clear releases it (the ↺ path).
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Channel {
                scope: target.scope,
                channel: target.channel.clone(),
            },
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("clear emits a snapshot");
    let playlist = node_by_kind(&snapshot, "Playlist");
    let Some(UiNodeFace::Shader(face)) = &playlist.children[0].face else {
        panic!("idle child keeps its face");
    };
    let glow = control_labeled(face, "Glow");
    assert!(
        !glow.panel_target.as_ref().expect("target survives").engaged,
        "clearing releases the writer"
    );
}

#[test]
fn the_root_module_card_derives_its_panel_from_scoped_channels() {
    // The flat-root reversal made the root module a real card, and this is
    // what it is FOR: its face carries the root scope's panel, derived from
    // the binding graph and the panel targets its subtree already produced
    // (`docs/design/modules.md` R8, `panel.md` P1). Nothing is mock-fed.
    let server = Rc::new(RefCell::new(bound_glow_e2e_server()));
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

    // -- one top-level card: the root module, wearing the module face -------
    let editor = project_editor(&snapshot);
    assert_eq!(editor.nodes.len(), 1, "one top-level workspace card");
    let root_card = &editor.nodes[0];
    assert_eq!(root_card.header.kind, "Module");
    assert_eq!(root_card.header.title, editor.project_name);
    let face = module_face(&snapshot);

    // -- the panel: the root scope's channels, and only those ---------------
    let scope = face.panel.target.expect("the root panel targets its scope");
    assert!(
        matches!(scope, lpc_wire::WireScopeRef::Module { .. }),
        "the root scope is a module scope, got {scope:?}"
    );
    assert_eq!(
        face.panel
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>(),
        vec!["glow"],
        "only the BOUND uniform lists — `speed` is wired to nothing, and \
         panel membership is scope publicity (Q13), never an authored flag"
    );
    let glow = &face.panel.controls[0];
    assert_eq!(glow.state, crate::UiPanelControlState::ReadDefault);
    assert_eq!(
        glow.source.as_deref(),
        Some("authored default"),
        "nothing writes the channel, so the consuming slot's own default is \
         what the control displays (R6)"
    );
    assert_eq!(glow.control.value.kind, UiSlotValueKind::F32(0.5));
    let target = glow
        .control
        .panel_target
        .clone()
        .expect("a module-panel control dispatches panel writes");
    assert_eq!(target.scope, scope);
    assert_eq!(target.channel, "glow");

    // -- one control, two cards (P1): the shader card carries the SAME one --
    let shader = node_by_kind(&snapshot, "Shader");
    assert!(
        root_card
            .children
            .iter()
            .any(|child| child.kind == "Shader"),
        "the shader card renders below the root card"
    );
    let Some(UiNodeFace::Shader(shader_face)) = &shader.face else {
        panic!("shader card keeps its own face");
    };
    assert_eq!(
        control_labeled(shader_face, "Glow").panel_target,
        Some(target.clone()),
        "the knob on the shader card and the control on the module panel \
         share one (scope, channel) identity"
    );

    // -- engaging a writer: the module panel reads Held ----------------------
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelWriteOp {
            scope: target.scope,
            channel: target.channel.clone(),
            value: LpValue::F32(0.9),
            ttl_ms: None,
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("panel write emits a snapshot");

    let face = module_face(&snapshot);
    let glow = &face.panel.controls[0];
    assert_eq!(
        glow.state,
        crate::UiPanelControlState::Engaged,
        "the engaged writer reads Held on the module panel"
    );
    assert_eq!(
        glow.control.live_value.as_deref(),
        Some("0.9"),
        "and the held value flows back as the live reading"
    );

    // -- the module's own reset: clear at scope granularity (P2) ------------
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Scope { scope },
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("panel clear emits a snapshot");

    let face = module_face(&snapshot);
    let glow = &face.panel.controls[0];
    assert_eq!(
        glow.state,
        crate::UiPanelControlState::ReadDefault,
        "resetting the module releases its writer"
    );
    assert_eq!(glow.source.as_deref(), Some("authored default"));
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

    let project_json = "{\n  \"format\": 3\n}\n";
    let module_json = r#"{
  "kind": "Module",
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
    "speed": { "source": "bus:speed" },
    "count": { "source": "bus:count" },
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
      "description": "Gradient speed multiplier"
    },
    "count": {
      "kind": "value",
      "value": "u32",
      "default": 2,
      "min": 1,
      "max": 4,
      "label": "Count",
      "description": "How many bands"
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "brightness": 200,
  "mapping": { "kind": "Map2d", "source": "sign.map2d.json" },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    let map2d_json = r#"{
  "format": 1,
  "objects": [
    { "name": "panel", "shape": { "grid": { "origin": [0, 0], "cols": 4, "rows": 4, "pitch": 10 } } }
  ]
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
        ("module.json", module_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("sign.map2d.json", map2d_json),
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

const BOUND_GLOW_PROJECT_DIR: &str = "/projects/bound-glow-e2e";

/// The fyeah-sign shape: `glow` bound to `bus:glow`, `speed` unbound (and
/// therefore, since Q13, not on any panel). Both uniforms feed the shader so the
/// compile stays honest.
const BOUND_GLOW_SHADER: &str = "layout(binding = 0) uniform float speed;\nlayout(binding = 1) uniform float glow;\n\nvec4 render(vec2 pos) {\n    return vec4(pos.x * speed, glow, 0.5, 1.0);\n}\n";

fn bound_glow_e2e_server() -> LpServer {
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

    let project_json = "{\n  \"format\": 3\n}\n";
    let module_json = r#"{
  "kind": "Module",
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
    "glow": { "source": "bus:glow" },
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
      "description": "Animation speed multiplier"
    },
    "glow": {
      "kind": "value",
      "value": "f32",
      "default": 0.5,
      "min": 0,
      "max": 1,
      "label": "Glow",
      "description": "Rainbow highlight intensity"
    }
  }
}"#;
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
        ("module.json", module_json),
        ("clock.json", clock_json),
        ("shader.json", shader_json),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
        ("shader.glsl", BOUND_GLOW_SHADER),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(
                format!("{BOUND_GLOW_PROJECT_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(BOUND_GLOW_PROJECT_DIR.as_path())
        .expect("load bound-glow-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

const PLAYLIST_BOUND_GLOW_DIR: &str = "/projects/playlist-bound-glow-e2e";

/// fyeah-sign's nesting: the bound-glow shader is playlist entry 1's node.
fn playlist_bound_glow_e2e_server() -> LpServer {
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

    let project_json = "{\n  \"format\": 3\n}\n";
    let module_json = r#"{
  "kind": "Module",
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
    let playlist_json = r#"{
  "kind": "Playlist",
  "bindings": {
    "time": { "source": "bus:time" }
  },
  "idle_entry": 1,
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } }
  }
}"#;
    let idle_json = r#"{
  "kind": "Shader",
  "source": "idle.glsl",
  "bindings": {
    "glow": { "source": "bus:glow" }
  },
  "consumed": {
    "speed": {
      "kind": "value",
      "value": "f32",
      "default": 1,
      "min": 0,
      "max": 3,
      "label": "Speed",
      "description": "Animation speed multiplier"
    },
    "glow": {
      "kind": "value",
      "value": "f32",
      "default": 0.5,
      "min": 0,
      "max": 1,
      "label": "Glow",
      "description": "Rainbow highlight intensity"
    }
  }
}"#;
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
        ("module.json", module_json),
        ("clock.json", clock_json),
        ("playlist.json", playlist_json),
        ("idle.json", idle_json),
        ("idle.glsl", BOUND_GLOW_SHADER),
        ("fixture.json", fixture_json),
        ("output.json", output_json),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(
                format!("{PLAYLIST_BOUND_GLOW_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(PLAYLIST_BOUND_GLOW_DIR.as_path())
        .expect("load playlist-bound-glow-e2e project");
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

    let project_json = "{\n  \"format\": 3\n}\n";
    let module_json = r#"{
  "kind": "Module",
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
        ("module.json", module_json),
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

/// Wrap a node-card UI mutation exactly as the web's `node_ui_action`
/// does (targeted at the node-tree editor surface; the op carries its own
/// node key).
fn node_ui_command(op: NodeUiOp) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ProjectEditorTarget::node_tree().node_id(),
        ProjectEditorOp::NodeUi(op),
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

/// The one card of `kind`, anywhere in the nested card tree. Since the
/// flat-root reversal every non-root card is a `UiNodeChild` under the root
/// module's card, so this promotes as it descends.
fn node_by_kind(view: &UiStudioView, kind: &str) -> UiNodeView {
    card_matching(view, kind, |card| card.header.kind == kind)
}

/// The root module card's face.
fn module_face(view: &UiStudioView) -> crate::UiModuleFace {
    let Some(UiNodeFace::Module(face)) = project_editor(view)
        .nodes
        .first()
        .expect("the root module card")
        .face
        .clone()
    else {
        panic!("the root card wears a module face");
    };
    face
}

fn playlist_face(view: &UiStudioView) -> UiPlaylistFace {
    let Some(UiNodeFace::Playlist(face)) = node_by_kind(view, "Playlist").face else {
        panic!("playlist face present");
    };
    face
}

/// The one panel control carrying `label` (the uniform map is key-ordered,
/// so index-addressing the controls is brittle).
fn control_labeled<'a>(face: &'a crate::UiShaderFace, label: &str) -> &'a UiPanelControl {
    face.controls
        .iter()
        .find(|control| control.label == label)
        .unwrap_or_else(|| panic!("shader face carries a {label} control"))
}

fn shader_knob(view: &UiStudioView) -> UiPanelControl {
    let Some(UiNodeFace::Shader(face)) = node_by_kind(view, "Shader").face else {
        panic!("shader face present");
    };
    control_labeled(&face, "Speed").clone()
}

fn fixture_fader(view: &UiStudioView) -> UiPanelControl {
    let Some(UiNodeFace::Fixture(face)) = node_by_kind(view, "Fixture").face else {
        panic!("fixture face present");
    };
    face.brightness
}
