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

use crate::app::project::control_display_layout_fallback::synthesized_map2d_layout;
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
    assert_eq!(
        face.controls.len(),
        3,
        "both bound uniforms, plus the phasor's period (P7 item 5)"
    );
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
    // The phasor slot's ONE control: its period, in seconds, editing the
    // interior config slot and carrying the slot's own shaping out with
    // every gesture (P7 item 5).
    // The def's own "Speed" VALUE uniform holds the plain label, so the
    // phasor knob disambiguates with its uniform name (G2 vocab feedback).
    let period = control_labeled(face, "Phase period");
    assert_eq!(period.value.kind, UiSlotValueKind::F32(20.0));
    assert_eq!(
        period.emit,
        crate::UiPanelEmit::PhasorPeriod {
            waveform: lpc_model::Waveform::Triangle,
            phase_offset: 0.25,
        },
        "waveform and offset ride the gesture untouched"
    );
    let period_address = period.address.clone().expect("period edits are addressed");
    assert_eq!(
        period_address.path.to_string(),
        "consumed[phase].phasor.some"
    );
    assert!(
        period.panel_target.is_none(),
        "the phasor slot carries no authored binding, so the knob is card-local"
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
            max: 1.0,
            step: None
        }
    );
    assert_eq!(face.brightness.value.kind, UiSlotValueKind::F32(0.8));
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

    // -- clock: the time product plus its (unread) phasor listing -----------
    // P7 item 4. The clock used to fall back to the generic sections; since
    // `bus:time` carries a product it has a face like everything else, and
    // that face is where the timebase listing lands.
    let clock = node_by_kind(&snapshot, "Clock");
    let Some(UiNodeFace::Clock(face)) = &clock.face else {
        panic!("clock node derives a clock face, got {:?}", clock.face);
    };
    assert_eq!(face.product.kind, crate::UiProductKind::Time);
    assert_eq!(
        face.product.detail.as_deref(),
        Some("node 1 output 0"),
        "the handle names the clock's own node and output"
    );
    let transport = face
        .transport
        .clone()
        .expect("the transport block rides the face (tape-hero P2)");
    assert_eq!(
        transport.play_state,
        lpc_model::PlayState::Playing,
        "authored default: playing"
    );
    assert_eq!(transport.rate, 1.0);
    assert_eq!(transport.scrub_offset_seconds, 0.0);
    assert_eq!(
        transport.seconds, 0.0,
        "numeric seconds is probe-only; no probe answers in this harness"
    );
    let scrub_address = transport
        .scrub_address
        .clone()
        .expect("writable Debug row → dispatch address");
    // No timebase probe has answered in this harness, and "no read yet" is
    // deliberately NOT the same state as an empty listing.
    assert_eq!(face.timebase, crate::UiTimebaseState::Unread);
    assert!(face.phasors.is_empty());

    // -- scrub drag flood: the tape gesture is a SetValue flood on the
    // transport row; the face's transport block must read the staged value
    // back IMMEDIATELY (edit-buffer echo suppression — the contract that
    // keeps the tape from snapping back under the finger, tape-hero P2).
    for value in [-4.0_f32, -8.0, -12.4] {
        handle
            .tx
            .send(set_value_action(scrub_address.clone(), LpValue::F32(value)));
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("scrub edits emit a snapshot");
    let clock = node_by_kind(&snapshot, "Clock");
    let Some(UiNodeFace::Clock(face)) = &clock.face else {
        panic!("clock keeps its face through a scrub, got {:?}", clock.face);
    };
    let scrubbed = face.transport.clone().expect("block survives the edit");
    assert_eq!(
        scrubbed.scrub_offset_seconds, -12.4,
        "the staged scrub value reads back through the face at once"
    );
    assert_eq!(
        scrubbed.play_state,
        lpc_model::PlayState::Playing,
        "siblings untouched by the scrub"
    );

    // -- knob drag flood: coalesced SetValues flow back into the face -------
    for value in [1.4_f32, 1.9, 2.5] {
        handle
            .tx
            .send(set_value_action(knob_address.clone(), LpValue::F32(value)));
    }
    handle
        .tx
        .send(set_value_action(fader_address.clone(), LpValue::F32(0.12)));
    // The period knob's gesture: the SAME slot path, carrying the whole
    // re-wrapped config the emit family builds (the web dispatches exactly
    // this through `panel_or_slot_action`).
    handle.tx.send(set_value_action(
        period_address.clone(),
        lpc_model::ToLpValue::to_lp_value(&lpc_model::PhasorConfig {
            period_seconds: 8.0,
            waveform: lpc_model::Waveform::Triangle,
            phase_offset: 0.25,
        }),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    let knob = shader_knob(&snapshot);
    assert_eq!(knob.value.kind, UiSlotValueKind::F32(2.5));
    assert_eq!(knob.state.dirty, UiNodeDirtyState::Dirty);
    let period = shader_control(&snapshot, "Phase period");
    assert_eq!(
        period.value.kind,
        UiSlotValueKind::F32(8.0),
        "the knob reads the period back out of the edited record"
    );
    assert_eq!(period.state.dirty, UiNodeDirtyState::Dirty);
    let fader = fixture_fader(&snapshot);
    assert_eq!(fader.value.kind, UiSlotValueKind::F32(0.12));
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
    assert!(
        shader_json.contains("\"period_seconds\":8"),
        "shader.json gained the period edit as a whole config: {shader_json}"
    );
    assert!(
        shader_json.contains("\"waveform\":\"triangle\""),
        "and the shaping the panel may not touch survived it: {shader_json}"
    );
    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"brightness\":0.12"),
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
        "a fresh card starts with no mirrored draft"
    );
    assert!(
        shader.card_ui.agent_collapsed,
        "a fresh card starts with the agent section collapsed (G1 R-F)"
    );
    let node = shader.header.path.clone();

    // Expand it first — the flip alone, from the collapsed rest state.
    for op in NodeUiOp::toggle_agent_section(&node, true, "") {
        handle.tx.send(node_ui_command(op));
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("expand emits a snapshot");
    assert!(
        !node_by_kind(&snapshot, "Shader").card_ui.agent_collapsed,
        "expanding from the collapsed default opens the section"
    );

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

    // GV fix 5, the jerky-drag fix: the very first snapshot after the
    // dispatch — no RefreshProject, no probe — already shows the written
    // value and reads engaged. Before the local echo the knob sat at its
    // authored default until the round trip landed, which is what made a
    // drag move at probe cadence.
    let echo = view.try_recv().expect("the write itself emits a snapshot");
    let playlist = node_by_kind(&echo, "Playlist");
    let Some(UiNodeFace::Shader(face)) = &playlist.children[0].face else {
        panic!("idle child keeps its face");
    };
    let echoed = control_labeled(face, "Glow");
    assert_eq!(
        echoed.live_value.as_deref(),
        Some("0.9"),
        "the panel's own write reads back before any probe"
    );
    assert!(
        echoed
            .panel_target
            .as_ref()
            .expect("target survives")
            .engaged,
        "and the control reads engaged at once"
    );
    assert_eq!(
        editor_dirty(&echo),
        (0, 0),
        "the echo is display state — it stages nothing"
    );

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
fn the_active_playlist_entrys_controls_bubble_onto_the_module_panel() {
    // R9 / GV fix 2. A playlist entry's bindings resolve in the entry's
    // SINK scope, which matches no module panel by scope — so fyeah's root
    // panel rendered EMPTY while the idle shader card below it carried a
    // live Glow knob. The active entry's controls now ride the root panel
    // as their own group, labeled by the entry.
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

    let face = module_face(&snapshot);
    assert_eq!(
        face.panel
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>(),
        vec!["brightness"],
        "nothing is AUTHORED in the root scope itself — the flat strip \
         carries only the fixture's promoted brightness fader; the clock's \
         instrument sits in its own child group (G2 feedback 2026-08-08)"
    );
    assert_eq!(
        face.panel.groups.len(),
        2,
        "two groups: the clock's instrument, then the active entry"
    );
    assert_eq!(
        face.panel.groups[0].label, "Clock",
        "the instrument group leads (G2 feedback 2026-08-08)"
    );
    let entry_group = &face.panel.groups[1];
    assert_eq!(
        entry_group.label, "idle",
        "the group wears the ACTIVE entry's name"
    );
    let entry_scope = entry_group
        .target
        .expect("the entry group resets its own scope");
    assert!(
        entry_scope.is_sink(),
        "the group targets the entry's SINK scope, got {entry_scope:?}"
    );
    assert_eq!(
        entry_group
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>(),
        vec!["glow"],
        "only the entry's authored-bound uniform is public"
    );
    let glow = &entry_group.controls[0];
    assert_eq!(glow.state, crate::UiPanelControlState::ReadDefault);
    let target = glow
        .control
        .panel_target
        .clone()
        .expect("the bubbled control keeps its own write target");
    assert_eq!(target.scope, entry_scope);

    // One control, two cards (P1): the entry card below carries the SAME one.
    let playlist = node_by_kind(&snapshot, "Playlist");
    let Some(UiNodeFace::Shader(entry_face)) = &playlist.children[0].face else {
        panic!("the entry card keeps its shader face");
    };
    assert_eq!(
        control_labeled(entry_face, "Glow").panel_target,
        Some(target.clone()),
        "the group control and the entry card's knob share one identity"
    );

    // Engaging through the bubbled control flows back to BOTH views.
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
    let glow = &face.panel.groups[1].controls[0];
    assert_eq!(
        glow.state,
        crate::UiPanelControlState::Engaged,
        "the module panel's copy reads Held"
    );
    assert_eq!(glow.control.live_value.as_deref(), Some("0.9"));
    let playlist = node_by_kind(&snapshot, "Playlist");
    let Some(UiNodeFace::Shader(entry_face)) = &playlist.children[0].face else {
        panic!("the entry card keeps its shader face");
    };
    assert_eq!(
        control_labeled(entry_face, "Glow").live_value.as_deref(),
        Some("0.9"),
        "and so does the entry card's knob"
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
        vec!["brightness", "glow"],
        "the BOUND uniform lists plus the fixture's promoted brightness \
         fader in the FLAT strip — the clock's instrument moved to its own \
         child group (G2 feedback 2026-08-08), and `speed` is wired to \
         nothing and stays off (Q13 + the hint amendment)"
    );
    // The clock contributes EXACTLY ONE control however many channels its
    // transport rides (grouping is the whole point, P8 item 3), and that
    // control lives in its own child group wearing the clock node's name —
    // an instrument never sits in the flat strip (G2 feedback 2026-08-08).
    let clock_groups: Vec<_> = face
        .panel
        .groups
        .iter()
        .filter(|group| {
            group.controls.iter().any(|control| {
                matches!(
                    control.control.widget,
                    crate::UiPanelWidget::Transport { .. }
                )
            })
        })
        .collect();
    assert_eq!(clock_groups.len(), 1, "one clock, one instrument group");
    let clock_group = clock_groups[0];
    assert_eq!(
        clock_group.label, "Clock",
        "the instrument group wears the clock NODE's name, not a widget label"
    );
    assert_eq!(
        clock_group.controls.len(),
        1,
        "the instrument group holds exactly the one grouped Transport"
    );
    assert_eq!(clock_group.controls[0].channel, "clock.rate");
    assert!(
        clock_group.target.is_none(),
        "no group reset — it would clear the whole module scope's writers; \
         the instrument carries per-dimension clears"
    );
    let brightness = control_for_channel(&face, "brightness");
    assert!(
        matches!(
            brightness.control.widget,
            crate::UiPanelWidget::Fader { .. }
        ),
        "the promoted brightness control is the fixture's fader, got {:?}",
        brightness.control.widget
    );
    assert_eq!(
        brightness.state,
        crate::UiPanelControlState::ReadDefault,
        "nothing writes bus:brightness, so the fader reads the fixture's \
         authored default (R6)"
    );
    let glow = control_for_channel(&face, "glow");
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

    // -- the wiring drawer: the sidebar bus pane's content, relocated -------
    // Same rows the pane listed (writers → readers, focus affordances),
    // scoped to the module that owns them, and closed by default because
    // wiring is the authoring diagnostic and the panel is the product.
    let wiring = face.wiring.clone().expect("the root face carries wiring");
    assert!(
        !face.wiring_open,
        "the drawer starts closed (NodeCardUiState default)"
    );
    let glow_row = wiring
        .channels
        .iter()
        .find(|channel| channel.name == "glow")
        .expect("the glow channel is wiring on the root scope");
    assert_eq!(
        glow_row.scope,
        Some(scope),
        "the drawer lists this scope's channels only"
    );
    assert_eq!(
        glow_row.readers.len(),
        1,
        "the shader reads it: {:?}",
        glow_row.readers
    );
    assert!(
        glow_row.readers[0].focus.is_some(),
        "site rows keep their focus affordance (D7 linked navigation)"
    );

    // -- provenance: the authored §8 fields, present ones only -------------
    assert_eq!(
        face.provenance.as_deref(),
        Some("Yona \u{b7} v0.4 \u{b7} CC0-1.0"),
        "the footer joins the authored provenance fields and skips the \
         unauthored `created`"
    );

    // -- the P11 auto-save switch: present, and only on the ROOT -----------
    assert_eq!(
        face.auto_save,
        Some(true),
        "the root module presents panel auto-save, on by default, carried \
         back on the read's ServerRuntimeStatus"
    );

    // Opening the drawer is a core-owned card-UI op, exactly like the
    // other drawers — disclosure survives the next snapshot.
    handle.tx.send(node_ui_command(NodeUiOp::SetDrawer {
        node: root_card.header.path.clone(),
        drawer: crate::NodeCardDrawer::Wiring,
        open: true,
    }));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the drawer toggle emits a snapshot");
    assert!(
        module_face(&snapshot).wiring_open,
        "the wiring drawer's open state rides the face DTO"
    );

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
    let glow = control_for_channel(&face, "glow");
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
    let glow = control_for_channel(&face, "glow");
    assert_eq!(
        glow.state,
        crate::UiPanelControlState::ReadDefault,
        "resetting the module releases its writer"
    );
    assert_eq!(glow.source.as_deref(), Some("authored default"));

    // -- the P11 auto-save switch flips through the wire (no local echo) ----
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelAutoSaveOp { enabled: false },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the toggle emits a snapshot");
    assert_eq!(
        module_face(&snapshot).auto_save,
        Some(false),
        "the new value arrives on the next read's ServerRuntimeStatus — \
         nothing is applied optimistically, so a refusal can't leave the \
         switch lying"
    );
}

#[test]
fn the_panel_transport_drives_all_three_clock_channels() {
    // P8's product moment, end to end: ONE control on the module panel, and
    // each of its three dimensions writes ITS OWN channel. Nothing is
    // mock-fed — a real `LpServer` materializes the three `clock.*` fallback
    // bindings from the model's declarations, and the writes go down the
    // runtime command channel the same way a finger's would.
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

    let face = module_face(&snapshot);
    let scope = face.panel.target.expect("the root panel targets its scope");
    let transport_control = control_for_channel(&face, "clock.rate");
    assert_eq!(transport_control.control.label, "Time");
    let crate::UiPanelWidget::Transport { transport } = &transport_control.control.widget else {
        panic!(
            "the grouped control wears the Transport widget, got {:?}",
            transport_control.control.widget
        );
    };
    assert_eq!(
        transport.rate, 1.0,
        "the authored default, before any write"
    );
    assert_eq!(transport.play_state, lpc_model::PlayState::Playing);
    // Every dimension resolves to its own declared channel.
    assert_eq!(
        transport_control
            .control
            .wires
            .iter()
            .map(|wire| {
                let target = wire.panel_target.as_ref().expect("wired by declaration");
                assert_eq!(target.scope, scope, "all three resolve in the root scope");
                target.channel.as_str()
            })
            .collect::<Vec<_>>(),
        vec!["clock.rate", "clock.play_state", "clock.scrub"],
    );

    // -- the three gestures, each on its own channel ------------------------
    // The fader (f32), the run/pause button (the enum's wire tag as a
    // String — no new emit family, the `Waveform` spelling), and a strip
    // drag (f32). These are exactly the ops `TapeTransport` dispatches.
    for (channel, value) in [
        ("clock.rate", LpValue::F32(2.0)),
        (
            "clock.play_state",
            LpValue::String(lpc_model::PlayState::Paused.as_str().to_string()),
        ),
        ("clock.scrub", LpValue::F32(-4.0)),
    ] {
        handle.tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            crate::PanelWriteOp {
                scope,
                channel: channel.to_string(),
                value,
                ttl_ms: None,
            },
        )));
    }
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the panel writes emit a snapshot");

    let face = module_face(&snapshot);
    let transport_control = control_for_channel(&face, "clock.rate");
    let crate::UiPanelWidget::Transport { transport } = &transport_control.control.widget else {
        panic!("the control keeps its widget through a write");
    };
    // Each dimension converged INDEPENDENTLY — three scalar channels, three
    // echoes, no read-modify-write anywhere.
    assert_eq!(transport.rate, 2.0, "the fader moved the rate channel");
    assert_eq!(
        transport.play_state,
        lpc_model::PlayState::Paused,
        "the run/pause setpoint rode its own channel as a state noun"
    );
    assert_eq!(
        transport.scrub_offset_seconds, -4.0,
        "and the scrub echo converged on its own"
    );
    assert_eq!(
        transport_control.state,
        crate::UiPanelControlState::Engaged,
        "the anchor channel has a panel writer, so the group reads Held"
    );

    // One control, two cards (P1): the clock's own card shows the same
    // transport, and its tape carries the same per-dimension wiring — so a
    // gesture on either surface lands on the same channels.
    let clock = node_by_kind(&snapshot, "Clock");
    let Some(UiNodeFace::Clock(clock_face)) = &clock.face else {
        panic!("the clock card keeps its face");
    };
    assert_eq!(
        clock_face.transport.as_ref().map(|block| block.rate),
        Some(2.0),
        "the card's tape reads the same channel the panel wrote"
    );
    assert_eq!(
        clock_face.transport_wires(),
        transport_control.control.wires,
        "and dispatches through the same wires"
    );

    // -- the module reset releases all three ---------------------------------
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Scope { scope },
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the clear emits a snapshot");

    let face = module_face(&snapshot);
    let transport_control = control_for_channel(&face, "clock.rate");
    let crate::UiPanelWidget::Transport { transport } = &transport_control.control.widget else {
        panic!("the control keeps its widget through a clear");
    };
    assert_eq!(
        transport_control.state,
        crate::UiPanelControlState::ReadDefault
    );
    assert_eq!(
        (transport.rate, transport.play_state),
        (1.0, lpc_model::PlayState::Playing),
        "with the writers dropped, every dimension falls back to its \
         authored default (R6)"
    );
}

#[test]
fn every_gallery_example_opens_onto_a_populated_root_panel() {
    // A gallery example that opens onto an EMPTY panel teaches the wrong
    // thing about modules: the panel is the product, so each embedded
    // package must publish at least one root-scope control (whether
    // directly, or bubbled up from a playlist's active entry per R9).
    //
    // Booting each example through a real `LpServer` also compiles its
    // shaders on the device frontend, which is coverage the checked-in
    // examples otherwise lack (`docs/debt/example-shaders-not-compile-gated.md`).
    for example in crate::app::home::embedded_examples() {
        let server = Rc::new(RefCell::new(example_e2e_server(example)));
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

        let editor = project_editor(&snapshot);
        assert_eq!(
            editor.nodes.len(),
            1,
            "{}: one top-level workspace card",
            example.id
        );
        let face = module_face(&snapshot);
        let published = face.panel.controls.len()
            + face
                .panel
                .groups
                .iter()
                .map(|group| group.controls.len())
                .sum::<usize>();
        assert!(
            published > 0,
            "{}: the root panel publishes nothing — a gallery example must \
             open onto live controls, not an empty panel",
            example.id
        );
        // R-E: a nested group is a bordered box with a name on it, so a
        // group with no controls anywhere inside it is a label pointing at
        // nothing. Groups that DO publish must survive the filter, which is
        // what the `published` count above keeps honest.
        assert!(
            face.panel.groups.iter().all(|group| !group.is_empty()),
            "{}: an empty panel group reached the root card: {:?}",
            example.id,
            face.panel
                .groups
                .iter()
                .map(|group| (group.label.clone(), group.controls.len()))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_module_hero_leads_with_the_control_product_and_the_toggle_flips_it() {
    // The face-e2e project resolves BOTH primaries in its root scope (the
    // shader writes `bus:visual.out`, the fixture writes `bus:control.out`),
    // which is exactly the shape Yona's 2026-08-07 ruling is about: the
    // project's output is the LAMPS, so they lead, and the raster the
    // shader painted is one toggle away.
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
    let _ = view.try_recv().expect("connect emits a snapshot");
    // The connect read arms the product subscriptions; the probe answers on
    // the next read, so the hero has real bytes to show.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refresh emits a snapshot");

    let root_path = project_editor(&snapshot).nodes[0].header.path.clone();
    let face = module_face(&snapshot);
    assert_eq!(
        face.hero_choice,
        Some(crate::ModuleHeroProduct::Control),
        "both products resolve, so the hero is a choice — and the card's \
         default one is the lamps"
    );
    let hero = face.preview.clone().expect("the root module's hero");
    assert_eq!(hero.kind, crate::UiProductKind::Control);
    assert!(
        matches!(hero.preview, crate::UiProductPreview::ControlNative(_)),
        "the default hero draws the fixture's lamps, got {:?}",
        hero.preview
    );
    assert_eq!(
        hero.tracking,
        crate::UiProductTrackingState::Tracking,
        "the control product is always-live, so the borrowed hero says so"
    );

    // -- the upper-right toggle: one op, per card ---------------------------
    handle.tx.send(node_ui_command(NodeUiOp::SetHeroProduct {
        node: root_path.clone(),
        product: crate::ModuleHeroProduct::Visual,
    }));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the hero toggle emits a snapshot");
    let face = module_face(&snapshot);
    assert_eq!(face.hero_choice, Some(crate::ModuleHeroProduct::Visual));
    let hero = face.preview.clone().expect("the root module's hero");
    assert_eq!(hero.kind, crate::UiProductKind::Visual);
    assert!(
        matches!(hero.preview, crate::UiProductPreview::VisualSrgb8 { .. }),
        "flipped, the hero is the R7 mirror's raster, got {:?}",
        hero.preview
    );

    // -- and it is core-owned, so a full rebuild keeps it -------------------
    // A refresh rebuilds every card DTO from the server's read: view-local
    // state would be back on the lamps here.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refresh emits a snapshot");
    let face = module_face(&snapshot);
    assert_eq!(
        face.hero_choice,
        Some(crate::ModuleHeroProduct::Visual),
        "the preference is keyed by the node's address, so it survives the \
         card's remount"
    );
    assert_eq!(
        face.preview.expect("the root module's hero").kind,
        crate::UiProductKind::Visual
    );

    // -- back again: the toggle is two states of one control ---------------
    handle.tx.send(node_ui_command(NodeUiOp::SetHeroProduct {
        node: root_path,
        product: crate::ModuleHeroProduct::Control,
    }));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the hero toggle emits a snapshot");
    assert_eq!(
        module_face(&snapshot)
            .preview
            .expect("the root module's hero")
            .kind,
        crate::UiProductKind::Control
    );
}

#[test]
fn a_one_product_module_falls_back_to_whichever_product_it_has() {
    // The preference names a kind the scope does not resolve: the hero
    // falls back to the other one, in both directions, and no toggle is
    // offered — a one-product module has no choice to make. Neither
    // project routes its primaries any differently from a real one; each
    // simply leaves one primary channel unwritten.
    for visual_only in [false, true] {
        let server = Rc::new(RefCell::new(single_product_e2e_server(visual_only)));
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
        let _ = view.try_recv().expect("connect emits a snapshot");
        handle.tx.send(project_action(ProjectOp::RefreshProject));
        drive(actor.run_one_batch_for_test());
        let snapshot = view.try_recv().expect("the refresh emits a snapshot");

        let face = module_face(&snapshot);
        assert_eq!(
            face.hero_choice, None,
            "visual_only={visual_only}: one product is not a choice"
        );
        let want = if visual_only {
            crate::UiProductKind::Visual
        } else {
            crate::UiProductKind::Control
        };
        assert_eq!(
            face.preview.expect("the root module's hero").kind,
            want,
            "visual_only={visual_only}: the hero falls back to the product \
             the scope actually resolves"
        );
    }
}

/// The phase's oracle: what Studio synthesizes client-side must be what the
/// engine would have sent, field for field.
///
/// The face project's fixture is small (16 lamps), so the engine sends its
/// display layout outright. Synthesizing from the SAME document at the same
/// render extent and comparing the two layouts is the only check that keeps
/// the mirrored construction (`control_display_layout_fallback`) honest as
/// the engine's own layout builder evolves.
#[test]
fn synthesized_display_layout_matches_the_engines_own_layout() {
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
    let _ = view.try_recv().expect("connect emits a snapshot");
    // The connect read arms the product subscriptions; the probe answers on
    // the next read.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refresh emits a snapshot");

    let engine = fixture_display_layout(&snapshot)
        .expect("a 16-lamp layout is far under the wire budget, so the engine sends it");
    let doc = lpc_mapping::Map2dDoc::from_json(FACE_MAP2D).expect("the mapping document parses");

    let synthesized = synthesized_map2d_layout(&doc, engine.revision, 4, 4)
        .expect("the same document synthesizes client-side");

    assert_eq!(
        synthesized, engine,
        "client synthesis and engine layout must agree on every lamp, hint, and path span"
    );
}

/// Dome scale: the engine refuses the 1500-lamp layout (over the read-frame
/// wire budget), and the client fills it in from the mapping document
/// instead of leaving both faces reading "Control product has no display
/// layout."
#[test]
fn dome_scale_fixture_falls_back_to_a_client_synthesized_layout() {
    let example = crate::app::home::embedded_example("examples/zook-dome")
        .expect("the zook-dome example ships in the bundle");
    let server = Rc::new(RefCell::new(example_e2e_server(&example)));
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
    let _ = view.try_recv().expect("connect emits a snapshot");
    // The connect read arms the product subscriptions; the probe answers on
    // the next read.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refresh emits a snapshot");

    // The engine refused the layout (over the wire budget), and the sync
    // path fetched the mapping document itself — no card has to mount, no
    // editor has to open. The synthesized layout is already on the preview.
    let layout = fixture_display_layout(&snapshot)
        .expect("the sync fetches the document and synthesizes the layout the engine refused");
    assert_eq!(layout.lamps.len(), 1500, "every dome lamp is laid out");
    assert_eq!(
        (layout.width_hint, layout.height_hint),
        (32, 32),
        "the hints are the fixture's own render size"
    );
    assert_eq!(layout.lamps[0].lamp_index, 0);
    assert_eq!(layout.lamps[0].sample_start, 0);
    assert_eq!(layout.lamps[1499].lamp_index, 1499);
    assert_eq!(
        layout.lamps[1499].sample_start, 4497,
        "the last lamp's RGB triple starts at 1499 * 3"
    );
    assert_eq!(
        layout.paths.len(),
        5,
        "one span per repeat instance (the dome ships as 1 gapped sector x repeat 5)"
    );
    assert!(
        layout.paths.iter().all(|span| span.lamp_count == 300),
        "every instance is one physical 300-lamp strand"
    );
    assert_eq!(
        layout.paths.iter().map(|span| span.lamp_count).sum::<u32>(),
        1500,
        "the spans cover the whole chain"
    );
    for lamp in &layout.lamps {
        assert!(
            (0.0..=1.0).contains(&lamp.center[0]) && (0.0..=1.0).contains(&lamp.center[1]),
            "lamp {} fits the render target",
            lamp.lamp_index
        );
    }
}

/// The 2D display layout riding the fixture card's face preview, when there
/// is one.
///
/// This is the exact value both G1 symptoms read: a control-first module's
/// output hero re-homes onto the scope's `control.out` product and pulls its
/// preview out of the same product-keyed cache, so filling this in fills in
/// that face too.
fn fixture_display_layout(view: &UiStudioView) -> Option<lpc_model::ControlLayout2d> {
    let Some(UiNodeFace::Fixture(face)) = node_by_kind(view, "Fixture").face else {
        panic!("fixture face present");
    };
    let crate::UiProductPreview::ControlNative(preview) = face.preview.preview else {
        panic!(
            "the fixture's produced control product previews natively, got {:?}",
            face.preview.preview
        );
    };
    match preview.display_layout.as_deref() {
        Some(lpc_model::ControlDisplayLayout::Layout2d(layout)) => Some(layout.clone()),
        None => None,
    }
}

// -- harness -----------------------------------------------------------------

/// A server holding one embedded gallery example, loaded from the very
/// bytes the wasm bundle ships (`include_bytes!` of `examples/<name>/`).
fn example_e2e_server(example: &crate::app::home::EmbeddedExample) -> LpServer {
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
    let dir = format!("/projects/{}", example.id.replace('/', "-"));
    for (name, bytes) in example.files {
        server
            .base_fs_mut()
            .write_file(format!("{dir}/{name}").as_path(), bytes)
            .expect("write example file");
    }
    server
        .load_project(dir.as_path())
        .unwrap_or_else(|err| panic!("{} loads: {err}", example.id));
    server.advance_frame(16).expect("tick");
    server
}

const PROJECT_DIR: &str = "/projects/face-e2e";

/// The shader uses the panel uniform so its compile stays honest.
const FACE_SHADER: &str = "layout(binding = 0) uniform float speed;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x * speed, pos.y, 0.5, 1.0);\n}\n";

/// The face fixture's mapping document — 16 lamps, small enough that the
/// engine sends its display layout outright, which makes it the parity
/// oracle for the client-side synthesis.
const FACE_MAP2D: &str = r#"{
  "format": 1,
  "objects": [
    { "name": "panel", "shape": { "grid": { "origin": [0, 0], "cols": 4, "rows": 4, "pitch": 10 } } }
  ]
}"#;

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

    let project_json = "{\n  \"format\": 8\n}\n";
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
  "transport": { "play_state": "playing", "rate": 1.0 }
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
    },
    "phase": {
      "kind": "phasor",
      "value": "f32",
      "phasor": { "period_seconds": 20.0, "waveform": "triangle", "phase_offset": 0.25 },
      "default": 0,
      "label": "Phase",
      "description": "Cycle position (0-1)"
    }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
  "brightness": 0.8,
  "mapping": { "kind": "Map2d", "source": "sign.map2d.json" },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;
    let output_json = r#"{
  "kind": "Output",
  "channels": {
    "0": {
      "endpoint": "ws281x:local:D10"
    }
  },
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
        ("sign.map2d.json", FACE_MAP2D),
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

const SINGLE_PRODUCT_PROJECT_DIR: &str = "/projects/single-product-e2e";

/// A whole project — clock, shader, fixture, output — whose root scope
/// resolves exactly ONE primary product, for the hero's fallback rules.
///
/// Nothing is left dangling: the chain still runs end to end, one link of
/// it just rides a privately named channel instead of a primary one.
/// `visual_only` hands the fixture's lamps to `bus:lamps`, so nothing
/// writes `control.out`; otherwise the shader paints `bus:raster`, so
/// nothing writes `visual.out`.
fn single_product_e2e_server(visual_only: bool) -> LpServer {
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

    let (visual_channel, control_channel) = if visual_only {
        ("bus:visual.out", "bus:lamps")
    } else {
        ("bus:raster", "bus:control.out")
    };
    let project_json = "{\n  \"format\": 8\n}\n";
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
  "transport": { "running": true, "rate": 1.0 }
}"#;
    let shader_json = format!(
        r#"{{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {{
    "speed": {{ "source": "bus:speed" }},
    "output": {{ "target": "{visual_channel}" }}
  }},
  "consumed": {{
    "speed": {{
      "kind": "value",
      "value": "f32",
      "default": 1,
      "min": 0,
      "max": 3,
      "label": "Speed",
      "description": "Gradient speed multiplier"
    }}
  }}
}}"#
    );
    let fixture_json = format!(
        r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": 4, "height": 4 }},
  "brightness": 0.8,
  "mapping": {{ "kind": "Map2d", "source": "sign.map2d.json" }},
  "bindings": {{
    "input": {{ "source": "{visual_channel}" }},
    "output": {{ "target": "{control_channel}" }}
  }}
}}"#
    );
    let output_json = format!(
        r#"{{
  "kind": "Output",
  "channels": {{
    "0": {{ "endpoint": "ws281x:local:D10" }}
  }},
  "bindings": {{
    "input": {{ "source": "{control_channel}" }}
  }}
}}"#
    );
    let single_product_shader = "layout(binding = 0) uniform float speed;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x * speed, pos.y, 0.5, 1.0);\n}\n";
    let files: &[(&str, &str)] = &[
        ("project.json", project_json),
        ("module.json", module_json),
        ("clock.json", clock_json),
        ("shader.json", &shader_json),
        ("fixture.json", &fixture_json),
        ("sign.map2d.json", FACE_MAP2D),
        ("output.json", &output_json),
        ("shader.glsl", single_product_shader),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(
                format!("{SINGLE_PRODUCT_PROJECT_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(SINGLE_PRODUCT_PROJECT_DIR.as_path())
        .expect("load single-product-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

const BOUND_GLOW_PROJECT_DIR: &str = "/projects/bound-glow-e2e";

/// The fyeah-sign shape: `glow` bound to `bus:glow`, `speed` unbound (and
/// therefore, since Q13, not on any panel). Both uniforms feed the shader so the
/// compile stays honest.
const BOUND_GLOW_SHADER: &str = "layout(binding = 0) uniform float speed;\nlayout(binding = 1) uniform float glow;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(pos.x * speed, glow, 0.5, 1.0);\n}\n";

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

    let project_json = "{\n  \"format\": 8\n}\n";
    // Authored provenance (R14/§8): the root face's footer line is derived
    // from these, and the omitted `created` proves the join skips absent
    // fields rather than leaving a dangling separator.
    let module_json = r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" },
    "pixels": { "ref": "./fixture.json" },
    "output": { "ref": "./output.json" }
  },
  "provenance": {
    "author": "Yona",
    "version": "v0.4",
    "license": "CC0-1.0"
  }
}"#;
    let clock_json = r#"{
  "kind": "Clock",
  "transport": { "play_state": "playing", "rate": 1.0 }
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
  "channels": {
    "0": { "endpoint": "ws281x:local:D10" }
  },
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

const PALETTE_E2E_DIR: &str = "/projects/palette-e2e";

const PALETTE_E2E_SHADER: &str = "layout(binding = 0) uniform float speed;\nlayout(binding = 1) uniform sampler2D palette;\n\nvec4 render_2d(vec2 pos) {\n    return texture(palette, vec2(pos.x * speed, 0.0));\n}\n";

/// The Palette Plasma shape (M4 D5): a `palette` slot promoted to the panel
/// by `default_bind: bus:palette` + `panel: "show"` — no authored binding.
fn palette_e2e_server() -> LpServer {
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

    let project_json = "{\n  \"format\": 8\n}\n";
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
      "description": "Scroll speed"
    },
    "palette": {
      "kind": "palette",
      "value": "sampler2D",
      "default_bind": "bus:palette",
      "panel": "show",
      "label": "Palette",
      "description": "The ramp the strip reads its colors from"
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
  "channels": {
    "0": { "endpoint": "ws281x:local:D10" }
  },
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
        ("shader.glsl", PALETTE_E2E_SHADER),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(
                format!("{PALETTE_E2E_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(PALETTE_E2E_DIR.as_path())
        .expect("load palette-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

#[test]
fn a_default_bound_palette_with_panel_show_is_a_panel_write_target() {
    // D5 promotion end to end: the swatch control over a `default_bind:
    // bus:palette` + `panel: "show"` slot must carry a panel_target, so a
    // pick dispatches a PanelWrite on the channel (runtime poke, nothing
    // authored) rather than editing the authored default through the def.
    let server = Rc::new(RefCell::new(palette_e2e_server()));
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

    let swatch = shader_control(&snapshot, "Palette");
    assert_eq!(swatch.widget, UiPanelWidget::PaletteSwatch);
    let target = swatch.panel_target.clone().expect(
        "the declared `panel = \"show\"` hint promotes the default-bound \
         palette to a panel-write target (D5)",
    );
    assert_eq!(target.channel, "palette");

    // And the module panel presents the promoted channel, brightness-style.
    let face = module_face(&snapshot);
    assert!(
        face.panel
            .controls
            .iter()
            .any(|control| control.channel == "palette"),
        "the module panel lists the promoted palette channel, got {:?}",
        face.panel
            .controls
            .iter()
            .map(|control| control.channel.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_palette_panel_write_reads_back_on_the_swatch_that_wrote_it() {
    // The M4 defect: the swatch rendered the AUTHORED config, so a pick
    // changed the light and left the control showing the old ramp. The
    // summary string could not carry the fix — a GradientConfig does not
    // survive the round trip through display text — so the control carries
    // the config structurally alongside it.
    let server = Rc::new(RefCell::new(palette_e2e_server()));
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

    let swatch = shader_control(&snapshot, "Palette");
    let authored = swatch
        .shown_palette()
        .expect("a palette control shows a palette");
    assert_eq!(
        authored.gradients()[0].stops.len(),
        2,
        "the unwritten control shows its authored default"
    );
    let target = swatch.panel_target.clone().expect("D5 promotes the slot");

    // Write a four-stop ramp down the panel channel, exactly as a pick does.
    let picked = lpc_model::GradientConfig::Static(lpc_model::Gradient {
        space: lpc_model::Colorspace::Srgb,
        method: lpc_model::InterpMethod::Linear,
        stops: lpc_model::parse_stops("#000 #f80 #0af #fff").expect("a four-stop ramp parses"),
    });
    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::PanelWriteOp {
            scope: target.scope,
            channel: target.channel.clone(),
            value: lpc_model::ToLpValue::to_lp_value(&picked),
            ttl_ms: None,
        },
    )));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("panel write emits a snapshot");

    let swatch = shader_control(&snapshot, "Palette");
    assert_eq!(
        swatch.shown_palette(),
        Some(picked),
        "the swatch shows the palette it just wrote, not the authored default"
    );
    // The readout's text surface tracks the same write.
    let summary = crate::app::project::format_gradient_summary(
        &swatch.shown_palette().expect("still a palette"),
    );
    assert_eq!(
        swatch.live_value.as_deref(),
        Some(summary.as_str()),
        "the summary and the config describe the same write"
    );
    // The authored value is not lost — it still backs the detail popover.
    assert_eq!(
        swatch
            .gradient_config()
            .expect("authored config survives")
            .gradients()[0]
            .stops
            .len(),
        2,
    );
}

#[test]
fn successive_palette_writes_compose_instead_of_clobbering() {
    // The user-visible face of the same defect: the chooser derives each new
    // config from the one the control is SHOWING, so while that was the stale
    // authored value, adding a second palette to a cycle threw the first one
    // away. This walks the loop the UI walks — read the shown config, build
    // the next one from it, write — twice.
    let server = Rc::new(RefCell::new(palette_e2e_server()));
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
    let mut snapshot = view.try_recv().expect("connect emits a snapshot");
    let target = shader_control(&snapshot, "Palette")
        .panel_target
        .clone()
        .expect("D5 promotes the slot");

    let ramp = |stops: &str| lpc_model::Gradient {
        space: lpc_model::Colorspace::Srgb,
        method: lpc_model::InterpMethod::Linear,
        stops: lpc_model::parse_stops(stops).expect("stops parse"),
    };

    // Two adds, each built from whatever the control is showing at the time —
    // exactly `with_member_added(shown, picked)` in the Cycle tab.
    for added in [ramp("#f00 #ff0"), ramp("#00f #0ff")] {
        let shown = shader_control(&snapshot, "Palette")
            .shown_palette()
            .expect("a palette control shows a palette");
        let next = match shown {
            lpc_model::GradientConfig::Static(current) => lpc_model::GradientConfig::Cycle {
                set: vec![current, added],
                step_seconds: 20.0,
                fade_seconds: 0.5,
            },
            lpc_model::GradientConfig::Cycle {
                mut set,
                step_seconds,
                fade_seconds,
            } => {
                set.push(added);
                lpc_model::GradientConfig::Cycle {
                    set,
                    step_seconds,
                    fade_seconds,
                }
            }
        };
        handle.tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            crate::PanelWriteOp {
                scope: target.scope,
                channel: target.channel.clone(),
                value: lpc_model::ToLpValue::to_lp_value(&next),
                ttl_ms: None,
            },
        )));
        drive(actor.run_one_batch_for_test());
        handle.tx.send(project_action(ProjectOp::RefreshProject));
        drive(actor.run_one_batch_for_test());
        snapshot = view.try_recv().expect("panel write emits a snapshot");
    }

    let shown = shader_control(&snapshot, "Palette")
        .shown_palette()
        .expect("still a palette");
    assert_eq!(
        shown.gradients().len(),
        3,
        "the incumbent plus BOTH adds — the second add used to discard the \
         first, because it rebuilt from the stale authored config: {shown:?}"
    );
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

    let project_json = "{\n  \"format\": 8\n}\n";
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
  "transport": { "play_state": "playing", "rate": 1.0 }
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
  "channels": {
    "0": { "endpoint": "ws281x:local:D10" }
  },
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

    let project_json = "{\n  \"format\": 8\n}\n";
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
  "transport": { "play_state": "playing", "rate": 1.0 }
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
    let entry_glsl = "vec4 render_2d(vec2 pos) {\n    return vec4(pos.x, pos.y, 0.5, 1.0);\n}\n";
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
  "channels": {
    "0": {
      "endpoint": "ws281x:local:D10"
    }
  },
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

const OUTPUT_PROJECT_DIR: &str = "/projects/output-face-e2e";

/// A shader + 4x4 fixture (16 lamps) + ONE output node driving three wires:
/// 6 lamps on IO18, 4 on IO16, and the rest on IO2.
fn output_face_e2e_server() -> LpServer {
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

    let project_json = "{\n  \"format\": 8\n}\n";
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
  "transport": { "play_state": "playing", "rate": 1.0 }
}"#;
    let shader_json = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "bindings": {
    "output": { "target": "bus:visual.out" }
  },
  "consumed": {
    "speed": { "kind": "value", "value": "f32", "default": 1 }
  }
}"#;
    let fixture_json = r#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 4 },
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
  "channels": {
    "0": { "endpoint": "ws281x:local:IO18", "count": 6 },
    "1": { "endpoint": "ws281x:local:IO16", "count": 4 },
    "2": { "endpoint": "ws281x:local:IO2" }
  },
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
            .write_file(
                format!("{OUTPUT_PROJECT_DIR}/{name}").as_path(),
                body.as_bytes(),
            )
            .expect("write project file");
    }
    server
        .load_project(OUTPUT_PROJECT_DIR.as_path())
        .expect("load output-face-e2e project");
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

/// The output face end-to-end: a real multi-channel output node against a
/// real server, through the SAME view build the app renders — which is what
/// makes this test worth having, because the face's lamp extents come from
/// the studio controller's decoration pass, not from the project walk. The
/// unit tests can only prove the two halves separately.
#[test]
fn output_face_derives_multi_channel_wires_end_to_end() {
    let server = Rc::new(RefCell::new(output_face_e2e_server()));
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

    let face = output_face(&snapshot);
    assert_eq!(face.channels.len(), 3, "one row per authored wire");
    let labels: Vec<&str> = face
        .channels
        .iter()
        .map(|channel| channel.pin_label.as_str())
        .collect();
    assert_eq!(labels, ["IO18", "IO16", "IO2"], "in channel-key order");
    assert_eq!(
        face.channels
            .iter()
            .map(|channel| (channel.count, channel.slice_start))
            .collect::<Vec<_>>(),
        [(Some(6), Some(0)), (Some(4), Some(6)), (None, Some(10))]
    );
    // The decoration half: the 4x4 fixture's 16 lamps reach the face, so the
    // count-less wire can finally say what it drives.
    assert_eq!(face.total_lamps, Some(16));
    assert_eq!(
        face.channels[2].resolved_count,
        Some(6),
        "the remainder is what the counted wires left"
    );
    assert_eq!(
        face.board, None,
        "this harness has no device registry — 'no board known' is a normal state"
    );
    assert!(
        face.channels.iter().all(|channel| channel.gpio.is_none()),
        "and with no board, no pin resolves"
    );
    assert_eq!(face.input_binding.as_deref(), Some("bus:control.out"));

    // The addresses are real: editing a count rides the ordinary slot path
    // and the whole slice plan re-derives from it.
    let count_address = face.channels[0]
        .count_address
        .clone()
        .expect("a present count is addressed");
    assert_eq!(count_address.path.to_string(), "channels[0].count.some");
    handle
        .tx
        .send(set_value_action(count_address, LpValue::U32(8)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the edit emits a snapshot");

    let face = output_face(&snapshot);
    assert_eq!(face.channels[0].count, Some(8));
    assert_eq!(
        face.channels[1].slice_start,
        Some(8),
        "the following wires shift with it"
    );
    assert_eq!(
        face.channels[2].resolved_count,
        Some(4),
        "and the remainder shrinks by the same four lamps"
    );

    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    let output_json = read_output_project_file(&server, "output.json");
    assert!(
        output_json.contains("\"count\":8"),
        "the face's edit persisted through the normal save path: {output_json}"
    );
}

/// The `space` sections both visual-side cards grow (plan-B P3), against a
/// REAL projection: the slot rows have to flatten the way the derivation
/// assumes, the claimed rows have to leave the advanced drawer, and the
/// preview has to come back tagged with the space it rendered in.
#[test]
fn space_sections_derive_and_claim_their_rows_end_to_end() {
    use crate::{UiSpaceCellRole, UiSpaceFlagRole, UiSpaceSide, UiVisualSpace};

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

    // -- producer side ----------------------------------------------------
    let shader = node_by_kind(&snapshot, "Shader");
    let Some(UiNodeFace::Shader(face)) = &shader.face else {
        panic!("shader node derives a shader face");
    };
    let space = face
        .space
        .as_ref()
        .expect("the shader card's space section");
    assert_eq!(space.side, UiSpaceSide::Producer);
    assert_eq!(
        space.declared_space,
        Some(UiVisualSpace::TwoD),
        "every shader authored before this plan declares 2D"
    );
    assert_eq!(space.primary.active, "TwoD");
    assert_eq!(
        space.primary.choices.len(),
        2,
        "both declared variants reach the picker"
    );
    let in_1d = space
        .cell(UiSpaceCellRole::ProducerIn1d)
        .expect("a 2D shader carries its 1D answer cell");
    assert_eq!(
        in_1d
            .address
            .as_ref()
            .expect("the answer cell is addressed")
            .path
            .to_string(),
        "space.TwoD.in_1d",
        "the cell dispatches EnsurePresent at the real slot path"
    );
    assert!(
        !in_1d.is_choosable(),
        "centre scanline is the only declared answer today"
    );
    assert!(space.mismatch.is_none(), "the demo shader compiles");
    assert!(
        !config_row_keys(&shader).iter().any(|key| key == "space"),
        "the section CLAIMED the row: {:?}",
        config_row_keys(&shader)
    );

    // The hero preview is space-tagged now, and the card previews exactly
    // one space by default — its producer's own.
    assert_eq!(
        face.preview
            .spaces
            .iter()
            .map(|view| (view.space, view.hero))
            .collect::<Vec<_>>(),
        vec![(UiVisualSpace::TwoD, true)],
        "default = primary-space-only (D15)"
    );
    // …and once a probe has actually answered, the view carries the
    // metadata the D15 caption reads (`native · 2D` here).
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("the refresh emits a snapshot");
    let shader = node_by_kind(&snapshot, "Shader");
    let Some(UiNodeFace::Shader(face)) = &shader.face else {
        panic!("shader node derives a shader face");
    };
    let hero = &face.preview.spaces[0];
    assert!(
        matches!(hero.preview, crate::UiProductPreview::VisualSrgb8 { .. }),
        "the hero space carries the probed bytes, got {:?}",
        hero.preview
    );
    let meta = hero.meta.expect("the probe answered space metadata");
    assert_eq!(meta.primary, UiVisualSpace::TwoD);
    assert_eq!(meta.space, UiVisualSpace::TwoD);
    assert_eq!(
        meta.projection, None,
        "a 2D producer asked for 2D projects nothing"
    );

    // -- consumer side (the mirror) ---------------------------------------
    let fixture = node_by_kind(&snapshot, "Fixture");
    let Some(UiNodeFace::Fixture(face)) = &fixture.face else {
        panic!("fixture node derives a fixture face");
    };
    let space = face
        .space
        .as_ref()
        .expect("the fixture card's space section");
    assert_eq!(space.side, UiSpaceSide::Consumer);
    assert_eq!(space.declared_space, None, "a fixture states a policy");
    assert_eq!(space.primary.active, "Auto");
    assert!(
        space.cells.is_empty(),
        "Auto is the unexpanded state — a unit variant has no payload rows"
    );
    let strip = space
        .flag(UiSpaceFlagRole::StripOrderMeaningful)
        .expect("the strip-order flag");
    assert!(strip.value, "a bare strip is {{1D}} by default (D3)");
    assert_eq!(
        strip
            .address
            .as_ref()
            .expect("the flag is addressed")
            .path
            .to_string(),
        "strip_order_meaningful",
    );
    let keys = config_row_keys(&fixture);
    assert!(
        !keys
            .iter()
            .any(|key| key == "consume" || key == "strip_order_meaningful"),
        "both consumer rows left the drawer: {keys:?}"
    );
    assert!(
        keys.iter().any(|key| key == "mapping"),
        "and the drawer keeps everything the section did not claim: {keys:?}"
    );
}

/// Top-level config-row keys in a card's advanced drawer.
fn config_row_keys(node: &UiNodeView) -> Vec<String> {
    node.tabs
        .iter()
        .flat_map(|tab| match &tab.body {
            crate::UiNodeTabBody::Sections(sections) => sections.clone(),
            _ => Vec::new(),
        })
        .filter_map(|section| match section {
            crate::UiNodeSection::ConfigSlots(rows) => Some(rows),
            _ => None,
        })
        .flatten()
        .map(|row| row.key)
        .collect()
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

fn read_output_project_file(server: &Rc<RefCell<LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{OUTPUT_PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    String::from_utf8(bytes)
        .expect("utf8 project file")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn output_face(view: &UiStudioView) -> crate::UiOutputFace {
    let Some(UiNodeFace::Output(face)) = node_by_kind(view, "Output").face else {
        panic!("output face present");
    };
    face
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

/// The module panel's control for `channel` (channel-keyed — the control
/// list is dedupe-ordered, so index-addressing is brittle). Searches the
/// flat strip AND nested groups: the clock's instrument lives in its own
/// child group (G2 feedback 2026-08-08).
fn control_for_channel<'a>(
    face: &'a crate::UiModuleFace,
    channel: &str,
) -> &'a crate::UiPanelControlView {
    fn find<'a>(
        group: &'a crate::UiPanelGroup,
        channel: &str,
    ) -> Option<&'a crate::UiPanelControlView> {
        group
            .controls
            .iter()
            .find(|control| control.channel == channel)
            .or_else(|| group.groups.iter().find_map(|child| find(child, channel)))
    }
    find(&face.panel, channel).unwrap_or_else(|| panic!("module panel carries a {channel} control"))
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
    shader_control(view, "Speed")
}

fn shader_control(view: &UiStudioView, label: &str) -> UiPanelControl {
    let Some(UiNodeFace::Shader(face)) = node_by_kind(view, "Shader").face else {
        panic!("shader face present");
    };
    control_labeled(&face, label).clone()
}

fn fixture_fader(view: &UiStudioView) -> UiPanelControl {
    let Some(UiNodeFace::Fixture(face)) = node_by_kind(view, "Fixture").face else {
        panic!("fixture face present");
    };
    face.brightness
}
