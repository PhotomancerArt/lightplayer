//! End-to-end edit flow against an in-process LightPlayer server.
//!
//! Harness-level, no UI: a real `LpServer` (simulator session) runs behind a
//! `ClientIo` adapter that pumps every client message through
//! `LpServer::tick_and_send`. The studio actor drives the same command path
//! the web shell uses: connect → `SetValue` on a clock control (transient)
//! and a fixture slot (persisted) → observe DTO dirty states → `SaveOverlay`
//! (def file on disk gains only the persisted edit) → `RevertAllEdits`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lpa_client::ClientIo;
use lpa_server::{Graphics, LpGraphics, LpServer};
use lpc_model::{AsLpPath, LpValue};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireMessage, WireProjectCommand,
    WireServerMessage,
};
use lpfs::LpFsMemory;

use crate::{
    ControllerId, ProjectController, ProjectOp, SlotEditOp, StudioActor, StudioCommand,
    StudioController, StudioServerClient, UiAction, UiConfigSlot, UiConfigSlotBody,
    UiNodeDirtyState, UiNodeSection, UiNodeTabBody, UiStudioView, UiViewContent,
};

#[test]
fn simulator_session_edit_save_and_revert_end_to_end() {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
    let controller = StudioController::connected_with_client_for_test(client);
    let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
    let mut view = handle.view;

    // Connect the running project through the real client path so the
    // inventory read installs the node → def-artifact map.
    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert!(rate.state.live, "clock rate is a transient (live) control");
    let rate_address = rate.address.clone().expect("rate slot carries an address");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert!(!color_order.state.live, "color order is a persisted slot");
    let color_order_address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");
    assert_eq!(editor_dirty(&snapshot), (0, 0));

    // An oninput flood on the clock rate plus one persisted edit, queued into
    // one actor batch: the flood coalesces to a single mutation per address.
    let mutations_before = count_mutations(&sent);
    for value in [1.2_f32, 1.6, 2.0] {
        handle
            .tx
            .send(set_value_action(rate_address.clone(), LpValue::F32(value)));
    }
    handle.tx.send(set_value_action(
        color_order_address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edits emit a snapshot");

    assert_eq!(
        count_mutations(&sent) - mutations_before,
        2,
        "three queued rate SetValues coalesce with the color-order edit into two mutations"
    );
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
    assert!(rate.state.live);
    assert_eq!(slot_value_display(rate), "2");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert!(!color_order.state.live);
    assert_eq!(slot_value_display(color_order), "rgb");
    assert_eq!(
        editor_dirty(&snapshot),
        (1, 1),
        "one persisted and one transient slot are dirty"
    );

    // Save: the persisted color-order edit commits to fixture.json; the
    // transient rate edit stays pending (dirty-live), clock.json untouched.
    handle.tx.send(project_action(ProjectOp::SaveOverlay));
    drive(actor.run_one_batch_for_test());
    // Pull a refresh so the synced view reflects the committed def.
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("save + refresh emit a snapshot");

    let fixture_json = read_project_file(&server, "fixture.json");
    assert!(
        fixture_json.contains("\"color_order\":\"rgb\""),
        "fixture.json gained the persisted color-order edit: {fixture_json}"
    );
    let clock_json = read_project_file(&server, "clock.json");
    assert!(
        !clock_json.contains("\"rate\":2"),
        "clock.json must not gain the transient rate edit: {clock_json}"
    );
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(
        rate.state.dirty,
        UiNodeDirtyState::Dirty,
        "transient edit survives the save as dirty-live"
    );
    assert_eq!(slot_value_display(rate), "2");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "committed value synced back"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 1));

    // Revert all: the overlay clears, every slot returns to Clean, and the
    // *gated* refresh (since = last known revision) delivers the reverted
    // def values directly — no reconnect/full resync. Reverting advances the
    // effective def revisions monotonically (studio editing ADR follow-up
    // (e)), so the delta read includes the reverted roots.
    handle.tx.send(project_action(ProjectOp::RevertAllEdits));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(rate),
        "1",
        "rate reverted to the authored value through the gated refresh"
    );
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(color_order),
        "rgb",
        "revert does not undo committed file changes"
    );
    assert_eq!(editor_dirty(&snapshot), (0, 0));
}

#[test]
fn per_slot_transient_reset_reverts_value_through_gated_refresh() {
    // The per-slot Reset affordance on a transient control (the clock `rate`
    // slider): SetValue then `SlotEditOp::Revert` must bring the DTO back to
    // the authored default through a *gated* refresh, without a reconnect.
    // The intermediate refresh below syncs the mutated def into the view
    // first, so the final assertion can only pass if the refresh after the
    // revert delivers the *reverted* def root (monotonic revisions, studio
    // editing ADR follow-up (e)) — not because a stale mirror or buffer
    // entry happened to shadow the right value.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
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
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(slot_value_display(rate), "1");
    let rate_address = rate.address.clone().expect("rate slot carries an address");

    // Edit the transient control, then pull a gated refresh so the synced
    // view itself holds the edited value.
    handle
        .tx
        .send(set_value_action(rate_address.clone(), LpValue::F32(2.0)));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit + refresh emit a snapshot");
    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(slot_value_display(rate), "2");

    // Per-slot reset: revert the rate edit, then a gated refresh must show
    // the authored default again.
    handle.tx.send(revert_action(rate_address));
    drive(actor.run_one_batch_for_test());
    handle.tx.send(project_action(ProjectOp::RefreshProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert + refresh emit a snapshot");

    let rate = find_slot(&snapshot, "controls.rate");
    assert_eq!(rate.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(
        slot_value_display(rate),
        "1",
        "per-slot reset restores the authored default through the gated refresh"
    );
}

#[test]
fn set_back_to_base_normalizes_to_clean_without_overlay_fetch() {
    // Minimal-diff normalization, user scenario: pick a choice value
    // (diagnostic-mode style), use it, set it back to the authored value —
    // the edited highlight must clear. The server elides the base-equal
    // assignment (NormalizedToRemoval) and the mirror must learn that from
    // the ack alone: the overlay revision may not advance, so a corrective
    // ReadOverlay would never fire.
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
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
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Clean);
    assert_eq!(slot_value_display(color_order), "grb", "authored default");
    let address = color_order
        .address
        .clone()
        .expect("color order slot carries an address");

    // Change the choice: dirty, counted.
    handle.tx.send(set_value_action(
        address.clone(),
        LpValue::String("rgb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("edit emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(color_order.state.dirty, UiNodeDirtyState::Dirty);
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // Set it back to the authored value: the highlight clears, no overlay
    // fetch corrects the mirror — the ack effect alone must do it.
    let overlay_reads_before = count_overlay_reads(&sent);
    handle.tx.send(set_value_action(
        address,
        LpValue::String("grb".to_string()),
    ));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("set-back emits a snapshot");
    let color_order = find_slot(&snapshot, "color_order");
    assert_eq!(
        color_order.state.dirty,
        UiNodeDirtyState::Clean,
        "setting a slot back to its base value clears the edited state"
    );
    assert_eq!(slot_value_display(color_order), "grb");
    assert_eq!(editor_dirty(&snapshot), (0, 0));
    assert_eq!(
        count_overlay_reads(&sent) - overlay_reads_before,
        0,
        "the mirror is corrected by the ack effect, not a ReadOverlay"
    );
}

// --- Harness ---------------------------------------------------------------

const PROJECT_DIR: &str = "/projects/edit-e2e";

/// A real server with a loaded clock + fixture project (no shader, so the
/// simulator session runs entirely host-side).
fn edit_e2e_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );

    let files: &[(&str, &str)] = &[
        (
            "project.json",
            r#"{
  "kind": "Project",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "pixels": { "ref": "./fixture.json" }
  }
}"#,
        ),
        (
            "clock.json",
            r#"{
  "kind": "Clock",
  "controls": {
    "running": true,
    "rate": 1.0
  }
}"#,
        ),
        (
            "fixture.json",
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 10, "height": 10 },
  "bindings": {
    "input": { "source": "bus#visual.out" },
    "output": { "target": "bus#control.out" }
  }
}"#,
        ),
    ];
    for (name, body) in files {
        server
            .base_fs_mut()
            .write_file(format!("{PROJECT_DIR}/{name}").as_path(), body.as_bytes())
            .expect("write project file");
    }
    server
        .load_project(PROJECT_DIR.as_path())
        .expect("load edit-e2e project");
    server.advance_frame(16).expect("tick");
    server
}

fn read_project_file(server: &Rc<RefCell<LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .expect("read project file");
    // Normalize whitespace so assertions are formatting-independent.
    String::from_utf8(bytes)
        .expect("utf8 project file")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn project_action(op: ProjectOp) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        op,
    ))
}

fn set_value_action(address: crate::ProjectSlotAddress, value: LpValue) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    ))
}

fn revert_action(address: crate::ProjectSlotAddress) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Revert { address },
    ))
}

fn count_mutations(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::MutateOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

fn count_overlay_reads(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> usize {
    sent.borrow()
        .iter()
        .filter(|message| {
            matches!(
                &message.msg,
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::ReadOverlay { .. },
                    ..
                }
            )
        })
        .count()
}

fn editor_dirty(view: &UiStudioView) -> (usize, usize) {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");
    (editor.dirty.persisted, editor.dirty.transient)
}

/// Find a config slot anywhere in the editor DTO tree by its address path.
fn find_slot<'a>(view: &'a UiStudioView, path: &str) -> &'a UiConfigSlot {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");

    fn in_slots<'a>(slots: &'a [UiConfigSlot], path: &str) -> Option<&'a UiConfigSlot> {
        for slot in slots {
            if slot
                .address
                .as_ref()
                .is_some_and(|address| address.path.to_string() == path)
            {
                return Some(slot);
            }
            if let UiConfigSlotBody::Record(record) = &slot.body
                && let Some(found) = in_slots(&record.fields, path)
            {
                return Some(found);
            }
        }
        None
    }

    fn in_sections<'a>(sections: &'a [UiNodeSection], path: &str) -> Option<&'a UiConfigSlot> {
        sections.iter().find_map(|section| match section {
            UiNodeSection::ConfigSlots(slots) | UiNodeSection::AssetSlots(slots) => {
                in_slots(slots, path)
            }
            _ => None,
        })
    }

    fn in_children<'a>(children: &'a [crate::UiNodeChild], path: &str) -> Option<&'a UiConfigSlot> {
        children.iter().find_map(|child| {
            in_sections(&child.sections, path).or_else(|| in_children(&child.children, path))
        })
    }

    editor
        .nodes
        .iter()
        .find_map(|node| {
            node.tabs
                .iter()
                .find_map(|tab| match &tab.body {
                    UiNodeTabBody::Sections(sections) => in_sections(sections, path),
                    _ => None,
                })
                .or_else(|| in_children(&node.children, path))
        })
        .unwrap_or_else(|| panic!("config slot with path {path} should exist"))
}

fn slot_value_display(slot: &UiConfigSlot) -> &str {
    let UiConfigSlotBody::Value(value) = &slot.body else {
        panic!("expected a value body for {}", slot.label);
    };
    &value.display
}

/// `ClientIo` that pumps each client message through the in-process server's
/// `tick_and_send` and queues the produced frames for `receive`.
struct InProcessServerIo {
    server: Rc<RefCell<LpServer>>,
    inbox: Rc<RefCell<VecDeque<WireServerMessage>>>,
    sent: Rc<RefCell<Vec<ClientMessage>>>,
}

impl ClientIo for InProcessServerIo {
    fn send<'life0, 'async_trait>(
        &'life0 mut self,
        msg: ClientMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.sent.borrow_mut().push(msg.clone());
        let server = Rc::clone(&self.server);
        let inbox = Rc::clone(&self.inbox);
        Box::pin(async move {
            let mut transport = CollectTransport::default();
            server
                .borrow_mut()
                .tick_and_send(16, vec![WireMessage::Client(msg)], &mut transport)
                .await
                .map_err(|error| TransportError::Other(format!("server error: {error}")))?;
            inbox.borrow_mut().extend(transport.sent);
            Ok(())
        })
    }

    fn receive<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let response = self
            .inbox
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| TransportError::Other("in-process server inbox empty".to_string()));
        Box::pin(async move { response })
    }

    fn close<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
}

/// In-memory server transport that records every sent frame.
#[derive(Default)]
struct CollectTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for CollectTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(Vec::new())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Drive a future to completion with a self-waking waker (bounded, so a hung
/// future fails the test instead of the suite).
fn drive<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    for _ in 0..100_000 {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
    panic!("e2e future did not complete");
}
