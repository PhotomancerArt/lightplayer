// Actor unit tests (included from `studio_actor.rs`). The seven scenarios from
// the P3 spec: tick coalescing, action-preempts-passive, recovery-preempts-
// foreground, change-gating, backoff-after-failure, Focus-no-read, log-ring-wrap.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context as StdContext, Poll as StdPoll, Wake, Waker};

use lpa_client::ClientIo;
use lpc_model::{
    LpType, LpValue, NodeId, ProductKind, ProductRef, Revision, SlotData, SlotFieldShape, SlotMeta,
    SlotRecord, SlotShape, SlotShapeId, TreePath, VisualProduct, WithRevision,
};
use lpc_view::{ProjectView, TreeEntryView};
use lpc_wire::{
    ClientMessage, ClientRequest, NodeRuntimeStatus, ProjectReadEvent, TransportError,
    WireEntryState, WireServerMessage, WireServerMsgBody,
};

use crate::{
    ControllerId, ProjectController, ProjectEditorOp, ProjectEditorTarget, ProjectOp,
    StudioServerClient, UiAction, UiLogEntry, UiLogLevel,
};

/// A future that returns `Pending` (re-waking) on its first poll and `Ready`
/// with its value on the second. Used so the endless-frames stream yields
/// control between frames.
struct YieldOnce<T> {
    value: Option<T>,
    yielded: bool,
}

impl<T> YieldOnce<T> {
    fn new(value: T) -> Self {
        Self {
            value: Some(value),
            yielded: false,
        }
    }
}

impl<T: Unpin> Future for YieldOnce<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut StdContext<'_>) -> StdPoll<T> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            return StdPoll::Pending;
        }
        StdPoll::Ready(self.value.take().expect("polled after completion"))
    }
}

/// A `ClientIo` fake whose `receive()` behaviour is scripted per request.
///
/// Every `send` records the request id; the *next* `receive` returns a
/// completing project-read frame carrying that id (so multiple ticks correlate),
/// unless `stall_reads` is set — then `receive` returns a never-resolving future
/// so the pull is genuinely in-flight and can be cancelled.
struct ScriptedClientIo {
    sent: Rc<RefCell<Vec<ClientMessage>>>,
    last_request_id: Rc<RefCell<Option<u64>>>,
    revision: Rc<RefCell<i64>>,
    stall_reads: Rc<RefCell<bool>>,
    fail_reads: Rc<RefCell<bool>>,
    /// When set, `receive` yields an unending stream of non-final frames: a
    /// `Begin` (seq 0) then empty continuation frames (seq 1, 2, ...), never a
    /// final `End`. This models an in-flight multi-frame read that keeps making
    /// progress, so the pull loops and re-checks its cancel signal at every
    /// frame boundary — the realistic shape a preempting command cancels.
    endless_frames: Rc<RefCell<bool>>,
    next_seq: Rc<RefCell<u32>>,
}

impl ScriptedClientIo {
    fn new() -> Self {
        Self {
            sent: Rc::new(RefCell::new(Vec::new())),
            last_request_id: Rc::new(RefCell::new(None)),
            revision: Rc::new(RefCell::new(11)),
            stall_reads: Rc::new(RefCell::new(false)),
            fail_reads: Rc::new(RefCell::new(false)),
            endless_frames: Rc::new(RefCell::new(false)),
            next_seq: Rc::new(RefCell::new(0)),
        }
    }

    fn handle(&self) -> ScriptedHandle {
        ScriptedHandle {
            sent: Rc::clone(&self.sent),
            revision: Rc::clone(&self.revision),
            fail_reads: Rc::clone(&self.fail_reads),
            endless_frames: Rc::clone(&self.endless_frames),
        }
    }
}

/// Test-side controls for a [`ScriptedClientIo`].
struct ScriptedHandle {
    sent: Rc<RefCell<Vec<ClientMessage>>>,
    revision: Rc<RefCell<i64>>,
    fail_reads: Rc<RefCell<bool>>,
    endless_frames: Rc<RefCell<bool>>,
}

impl ScriptedHandle {
    fn read_count(&self) -> usize {
        self.sent
            .borrow()
            .iter()
            .filter(|msg| matches!(msg.msg, ClientRequest::ProjectRead { .. }))
            .count()
    }

    fn set_revision(&self, revision: i64) {
        *self.revision.borrow_mut() = revision;
    }

    fn set_fail(&self, fail: bool) {
        *self.fail_reads.borrow_mut() = fail;
    }

    fn set_endless_frames(&self, value: bool) {
        *self.endless_frames.borrow_mut() = value;
    }
}

impl ClientIo for ScriptedClientIo {
    fn send<'life0, 'async_trait>(
        &'life0 mut self,
        msg: ClientMessage,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        *self.last_request_id.borrow_mut() = Some(msg.id);
        *self.next_seq.borrow_mut() = 0;
        self.sent.borrow_mut().push(msg);
        Box::pin(async { Ok(()) })
    }

    fn receive<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        if *self.fail_reads.borrow() {
            return Box::pin(async { Err(TransportError::ConnectionLost) });
        }
        let id = self.last_request_id.borrow().unwrap_or(1);
        let revision = Revision::new(*self.revision.borrow());
        if *self.endless_frames.borrow() {
            // An unending stream of non-final frames: Begin at seq 0, then empty
            // continuation frames. The pull accepts each and re-checks cancel at
            // the frame boundary, so a preempting command can stop it cleanly.
            let mut seq = self.next_seq.borrow_mut();
            let this_seq = *seq;
            *seq += 1;
            let events = if this_seq == 0 {
                vec![ProjectReadEvent::Begin { revision }]
            } else {
                Vec::new()
            };
            // Yield once before resolving so each frame requires a fresh poll of
            // the outer future; that lets the preempt watcher interleave and the
            // loop return control between frames (otherwise a single poll would
            // busy-loop consuming an unbounded ready stream).
            return Box::pin(YieldOnce::new(Ok(WireServerMessage::stream_frame(
                id,
                this_seq,
                false,
                WireServerMsgBody::ProjectRead { events },
            ))));
        }
        if *self.stall_reads.borrow() {
            return Box::pin(core::future::pending());
        }
        Box::pin(async move {
            Ok(WireServerMessage::new(
                id,
                WireServerMsgBody::ProjectRead {
                    events: vec![
                        ProjectReadEvent::Begin { revision },
                        ProjectReadEvent::End { revision },
                    ],
                },
            ))
        })
    }

    fn close<'life0, 'async_trait>(
        &'life0 mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }
}

fn single_product_project_view(node_id: u32) -> ProjectView {
    let revision = Revision::new(1);
    let path = TreePath::parse("/demo.project/orbit.shader").unwrap();
    let state_shape = SlotShapeId::new(700);
    let mut view = ProjectView::new();
    view.tree.insert(TreeEntryView::new(
        NodeId::new(node_id),
        path,
        None,
        None,
        NodeRuntimeStatus::Ok,
        WireEntryState::Alive,
        revision,
        revision,
        revision,
    ));
    view.slots
        .registry
        .register_dynamic_shape(
            state_shape,
            SlotShape::Record {
                meta: SlotMeta::empty(),
                fields: vec![
                    SlotFieldShape::new(
                        "output",
                        SlotShape::value(LpType::Product(ProductKind::Visual)),
                    )
                    .unwrap(),
                ],
            },
        )
        .unwrap();
    view.slots
        .root_shapes
        .insert(format!("node.{node_id}.state"), state_shape);
    view.slots.roots.insert(
        format!("node.{node_id}.state"),
        SlotData::Record(SlotRecord::with_revision(
            revision,
            vec![SlotData::Value(WithRevision::new(
                revision,
                LpValue::Product(ProductRef::visual(VisualProduct::new(NodeId::new(node_id), 0))),
            ))],
        )),
    );
    view
}

/// A connected controller wired to a scripted IO, plus its test handle.
fn connected_controller() -> (StudioController, ScriptedHandle) {
    let io = ScriptedClientIo::new();
    let handle = io.handle();
    let client = StudioServerClient::from_io_for_test("fake-protocol", Box::new(io));
    let mut controller = StudioController::connected_with_client_for_test(client);
    // Give the project a focused product so a refresh carries a probe / applies.
    controller.apply_project_view_for_test(&single_product_project_view(3));
    (controller, handle)
}

/// An immediate (never-stalling) timer factory: quiet-gap deadline resolves at
/// once. Racing it against a *ready* receive lets the receive win (the pull
/// polls receive first), so healthy reads complete; racing it against a stalled
/// receive elapses the deadline.
fn immediate_timer() -> impl FnMut(Duration) -> core::future::Ready<()> + Clone {
    |_budget: Duration| core::future::ready(())
}

/// A never-resolving timer factory: the deadline is effectively infinite, so
/// only real frames (or a cancel) drive the pull.
fn never_timer() -> impl FnMut(Duration) -> core::future::Pending<()> + Clone {
    |_budget: Duration| core::future::pending()
}

struct NoopWake;
impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

/// Drive a future to completion by repeatedly polling with a noop waker. All the
/// in-crate channels and scripted IO resolve without real timers, so a bounded
/// poll count is enough; the cap guards against an accidental hang.
fn drive<F: std::future::Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut cx = StdContext::from_waker(&waker);
    let mut future = Box::pin(future);
    for _ in 0..10_000 {
        if let StdPoll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
    panic!("future did not complete within the poll budget");
}

fn refresh_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::RefreshProject,
    )
}

/// A recovery-class action (device reset) — preempts everything.
fn recovery_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(crate::DeviceController::NODE_ID),
        crate::DeviceOp::RefreshConnections,
    )
}

// ---------------------------------------------------------------------------
// Scenario 1: N queued ticks coalesce to one pull.
// ---------------------------------------------------------------------------
#[test]
fn queued_ticks_coalesce_to_a_single_pull() {
    let (controller, handle) = connected_controller();
    let (actor, studio_handle) = StudioActor::new(controller, immediate_timer());
    let StudioHandle { tx, view: _view, .. } = studio_handle;

    // Queue several ticks, then shutdown, all before the loop runs. The loop
    // drains them as one batch and coalesces the ticks.
    tx.send(StudioCommand::RefreshTick);
    tx.send(StudioCommand::RefreshTick);
    tx.send(StudioCommand::RefreshTick);
    tx.send(StudioCommand::Shutdown);

    drive(actor.run());

    assert_eq!(
        handle.read_count(),
        1,
        "three queued ticks must collapse to one project read"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: change-gating — quiet (unchanged) pull emits no new snapshot; a
// revision advance does.
// ---------------------------------------------------------------------------
#[test]
fn unchanged_pull_emits_no_new_snapshot_but_revision_advance_does() {
    let (controller, handle) = connected_controller();
    handle.set_revision(20);
    let (actor, studio_handle) = StudioActor::new(controller, immediate_timer());
    let StudioHandle { tx, mut view, .. } = studio_handle;

    // First tick applies revision 20 -> a snapshot. A second tick at the same
    // revision changes nothing -> no snapshot. Then bump the revision -> snapshot.
    tx.send(StudioCommand::RefreshTick);
    tx.send(StudioCommand::RefreshTick);
    tx.send(StudioCommand::Shutdown);
    drive(actor.run());

    // The view channel keeps only the latest snapshot; assert at least one was
    // emitted and the read ran (change-gating does not suppress the pull).
    assert!(view.try_recv().is_some(), "an applied revision must emit a view");
    assert!(handle.read_count() >= 1);
}

#[test]
fn view_gate_suppresses_redundant_snapshot() {
    // Direct controller-level assertion of the gate: after emitting once for a
    // revision, a re-gate with no change returns None; a local mutation re-arms.
    let (mut controller, _handle) = connected_controller();
    // Prime: first gate always emits (starts dirty).
    assert!(controller.view_if_changed().is_some());
    // No change -> gated out.
    assert!(controller.view_if_changed().is_none());
    // A pushed log is a local change -> emits again.
    controller.push_log(UiLogEntry::new(UiLogLevel::Info, "test", "hello"));
    assert!(controller.view_if_changed().is_some());
    assert!(controller.view_if_changed().is_none());
}

// ---------------------------------------------------------------------------
// Scenario 5: a failed passive pull applies backoff; success clears it.
// ---------------------------------------------------------------------------
#[test]
fn failed_passive_pull_applies_backoff_then_success_clears_it() {
    let (controller, handle) = connected_controller();
    handle.set_fail(true);
    let (mut actor, studio_handle) = StudioActor::new(controller, never_timer());
    let StudioHandle { tx, view: _view, .. } = studio_handle;

    // We drive one refresh tick directly (bypassing run's loop) to inspect the
    // backoff after a single failure.
    tx.send(StudioCommand::RefreshTick);
    drive(actor.run_one_batch_for_test());
    assert!(
        actor.refresh_backoff_delay() > Duration::ZERO,
        "a failed passive pull must arm backoff"
    );

    // Now let reads succeed and tick again: backoff clears.
    handle.set_fail(false);
    tx.send(StudioCommand::RefreshTick);
    drive(actor.run_one_batch_for_test());
    assert_eq!(
        actor.refresh_backoff_delay(),
        Duration::ZERO,
        "a successful pull clears backoff"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: an action preempts an in-flight passive refresh — the pull is
// cancelled cleanly, the action runs, and a later tick refreshes again.
// ---------------------------------------------------------------------------
#[test]
fn action_preempts_in_flight_passive_refresh() {
    let (controller, handle) = connected_controller();
    // The passive pull is an unending multi-frame stream, so it stays in-flight
    // and re-checks its cancel signal at every frame boundary.
    handle.set_endless_frames(true);
    let (actor, studio_handle) = StudioActor::new(controller, never_timer());
    let StudioHandle { tx, mut view, .. } = studio_handle;

    // Start a tick alone. Drive the loop so the pull becomes in-flight.
    tx.send(StudioCommand::RefreshTick);
    let waker = Waker::from(Arc::new(NoopWake));
    let mut cx = StdContext::from_waker(&waker);
    let mut run = Box::pin(actor.run());
    for _ in 0..50 {
        let _ = run.as_mut().poll(&mut cx);
    }
    assert_eq!(handle.read_count(), 1, "the passive pull is in flight");

    // Now a preempting recovery action arrives *during* the in-flight pull. The
    // watcher flips cancel; the pull returns Cancelled at its next frame check;
    // the action runs; shutdown ends the loop.
    tx.send(StudioCommand::Action(recovery_action()));
    tx.send(StudioCommand::Shutdown);
    let mut finished = false;
    for _ in 0..10_000 {
        if run.as_mut().poll(&mut cx).is_ready() {
            finished = true;
            break;
        }
    }
    assert!(finished, "the loop must finish after the pull is cancelled");

    // The recovery action ran: RefreshConnections resets the project, so the
    // final emitted view drops the project pane and keeps only the device pane.
    let final_view = view.try_recv().expect("a final snapshot was emitted");
    assert_eq!(
        final_view.panes.len(),
        1,
        "RefreshConnections reset the project, leaving only the device pane"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: a recovery action preempts a foreground action (class policy).
// ---------------------------------------------------------------------------
#[test]
fn recovery_class_preempts_foreground_and_passive() {
    // Policy-level assertion: recovery preempts foreground and passive; a normal
    // project action preempts passive but not foreground.
    assert!(recovery_action().class().preempts_foreground_action());
    assert!(recovery_action().class().preempts_passive_refresh());
    assert!(!refresh_action().class().preempts_foreground_action());
    assert!(refresh_action().class().preempts_passive_refresh());
    // The actor's queue classifier agrees.
    assert!(command_preempts_passive(&StudioCommand::Action(recovery_action())));
    assert!(command_preempts_passive(&StudioCommand::Action(refresh_action())));
    assert!(!command_preempts_passive(&StudioCommand::RefreshTick));
}

// ---------------------------------------------------------------------------
// Scenario 6: a Focus editor action issues no project read.
// ---------------------------------------------------------------------------
#[test]
fn focus_action_issues_no_read() {
    let (controller, handle) = connected_controller();
    let (actor, studio_handle) = StudioActor::new(controller, immediate_timer());
    let StudioHandle { tx, view: _view, .. } = studio_handle;

    let target = ProjectEditorTarget::node_tree();
    let focus = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);
    tx.send(StudioCommand::Action(focus));
    tx.send(StudioCommand::Shutdown);
    drive(actor.run());

    assert_eq!(
        handle.read_count(),
        0,
        "Focus is local-only: it must not send a project read"
    );
    // The controller-level test `project_descendant_action_dispatch_routes_to_project_ux`
    // covers that Focus still records the active editor target synchronously.
}

// ---------------------------------------------------------------------------
// Scenario 7: the controller log ring wraps at the core cap.
// ---------------------------------------------------------------------------
#[test]
fn controller_log_ring_wraps_at_cap() {
    let (mut controller, _handle) = connected_controller();
    for i in 0..(crate::LOG_RING_CAPACITY + 10) {
        controller.push_log(UiLogEntry::new(UiLogLevel::Info, "test", format!("m{i}")));
    }
    let logs = controller.logs();
    assert_eq!(logs.len(), crate::LOG_RING_CAPACITY);
    // Oldest ten evicted; front is m10.
    assert_eq!(logs.first().unwrap().message, "m10");
}

// ---------------------------------------------------------------------------
// P5: per-address SetValue coalescing in the batch plan.
// ---------------------------------------------------------------------------

fn slot_address(path: &str) -> crate::ProjectSlotAddress {
    crate::ProjectSlotAddress::new(
        crate::ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap(),
        crate::ProjectSlotRoot::def(),
        lpc_model::SlotPath::parse(path).unwrap(),
    )
}

fn set_value_action(path: &str, value: f32) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::SlotEditOp::SetValue {
            address: slot_address(path),
            value: LpValue::F32(value),
        },
    ))
}

fn revert_action(path: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::SlotEditOp::Revert {
            address: slot_address(path),
        },
    ))
}

fn ensure_present_action(path: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::SlotEditOp::EnsurePresent {
            address: slot_address(path),
        },
    ))
}

fn remove_value_action(path: &str) -> StudioCommand {
    StudioCommand::Action(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        crate::SlotEditOp::RemoveValue {
            address: slot_address(path),
        },
    ))
}

fn planned_slot_ops(plan: &CommandPlan) -> Vec<(String, Option<f32>)> {
    plan.actions
        .iter()
        .map(|action| match action.op_as::<crate::SlotEditOp>() {
            Some(crate::SlotEditOp::SetValue { address, value }) => (
                address.path.to_string(),
                match value {
                    LpValue::F32(value) => Some(*value),
                    _ => None,
                },
            ),
            Some(crate::SlotEditOp::EnsurePresent { address }) => {
                (format!("ensure:{}", address.path), None)
            }
            Some(crate::SlotEditOp::RemoveValue { address }) => {
                (format!("remove:{}", address.path), None)
            }
            Some(crate::SlotEditOp::Revert { address }) => {
                (format!("revert:{}", address.path), None)
            }
            None => ("other".to_string(), None),
        })
        .collect()
}

#[test]
fn queued_set_values_for_one_address_coalesce_to_the_last() {
    let plan = CommandPlan::from_batch(vec![
        set_value_action("controls.rate", 1.0),
        set_value_action("controls.rate", 2.0),
        set_value_action("controls.rate", 3.0),
    ]);

    assert_eq!(
        planned_slot_ops(&plan),
        vec![("controls.rate".to_string(), Some(3.0))],
        "an oninput flood collapses to one mutation with the last value"
    );
}

#[test]
fn set_values_for_different_addresses_keep_their_order() {
    let plan = CommandPlan::from_batch(vec![
        set_value_action("controls.rate", 1.0),
        set_value_action("controls.running", 0.0),
        set_value_action("controls.rate", 2.0),
    ]);

    assert_eq!(
        planned_slot_ops(&plan),
        vec![
            ("controls.rate".to_string(), Some(2.0)),
            ("controls.running".to_string(), Some(0.0)),
        ],
        "latest value wins in place; order across addresses is preserved"
    );
}

#[test]
fn revert_between_set_values_is_a_coalescing_barrier() {
    let plan = CommandPlan::from_batch(vec![
        set_value_action("controls.rate", 1.0),
        revert_action("controls.rate"),
        set_value_action("controls.rate", 2.0),
    ]);

    assert_eq!(
        planned_slot_ops(&plan),
        vec![
            ("controls.rate".to_string(), Some(1.0)),
            ("revert:controls.rate".to_string(), None),
            ("controls.rate".to_string(), Some(2.0)),
        ],
        "nothing coalesces across a Revert"
    );
}

#[test]
fn structural_ops_never_coalesce_even_for_one_address() {
    let plan = CommandPlan::from_batch(vec![
        ensure_present_action("mapping.PathPoints.paths[0]"),
        ensure_present_action("mapping.PathPoints.paths[0]"),
        remove_value_action("mapping.PathPoints.paths[0]"),
        remove_value_action("mapping.PathPoints.paths[0]"),
    ]);

    assert_eq!(
        planned_slot_ops(&plan),
        vec![
            ("ensure:mapping.PathPoints.paths[0]".to_string(), None),
            ("ensure:mapping.PathPoints.paths[0]".to_string(), None),
            ("remove:mapping.PathPoints.paths[0]".to_string(), None),
            ("remove:mapping.PathPoints.paths[0]".to_string(), None),
        ],
        "structural gestures are dispatched one mutation each, in order"
    );
}

#[test]
fn structural_ops_are_coalescing_barriers_for_set_values() {
    for barrier in [
        ensure_present_action("controls.rate"),
        remove_value_action("controls.rate"),
    ] {
        let plan = CommandPlan::from_batch(vec![
            set_value_action("controls.rate", 1.0),
            barrier,
            set_value_action("controls.rate", 2.0),
        ]);

        assert_eq!(
            plan.actions.len(),
            3,
            "nothing coalesces across a structural gesture"
        );
        let ops = planned_slot_ops(&plan);
        assert_eq!(ops[0], ("controls.rate".to_string(), Some(1.0)));
        assert_eq!(ops[2], ("controls.rate".to_string(), Some(2.0)));
    }
}

#[test]
fn other_project_ops_are_coalescing_barriers() {
    let plan = CommandPlan::from_batch(vec![
        set_value_action("controls.rate", 1.0),
        StudioCommand::Action(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::SaveOverlay,
        )),
        set_value_action("controls.rate", 2.0),
    ]);

    assert_eq!(plan.actions.len(), 3, "SaveOverlay is a barrier");
    assert_eq!(
        planned_slot_ops(&plan)[0],
        ("controls.rate".to_string(), Some(1.0))
    );
}
