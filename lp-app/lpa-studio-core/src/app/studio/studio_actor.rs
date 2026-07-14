//! Single-owner actor around the [`StudioController`].
//!
//! One task owns the controller. Every input — user actions and timer-driven
//! refresh ticks — arrives as a [`StudioCommand`] on an ordered queue; the actor
//! drains a batch, coalesces redundant ticks, runs pending actions ahead of
//! ticks, executes, and emits **one change-gated snapshot** per processed batch.
//! This replaces the web crate's `Option<StudioController>` take/put,
//! generation counters, cancel flags, and 25 ms spin loops with queue semantics.
//!
//! # Preemption
//!
//! Preemption is class-driven priority, applied through the pull loop's explicit
//! [`CancelSignal`]. While a passive [`RefreshTick`](StudioCommand::RefreshTick)
//! pull is in flight, the actor concurrently watches the command queue; if a
//! command whose [`ActionClass`](crate::ActionClass) preempts a passive refresh
//! arrives, the actor flips the shared cancel flag. The pull observes it at the
//! next frame boundary and returns `Cancelled` cleanly (no dropped stream); the
//! preempting action then runs, and refresh resumes on the next tick.
//!
//! # Runtime neutrality
//!
//! The loop is a plain `async fn` ([`StudioActor::run`]) with no runtime
//! dependency, so tests drive it with a bare waker and wasm drives it under
//! `spawn_local`. It builds the pull's [`ProgressDeadline`] from a caller-
//! supplied timer factory (native `sleep` / wasm `setTimeout`).

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll};
use core::time::Duration;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lpa_client::{BackoffPolicy, CancelSignal, ProgressDeadline};

use crate::app::studio::console_command::ConsoleCommand;
use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::{
    CommandReceiver, CommandSender, StudioViewReceiver, StudioViewSender, command_channel,
    studio_view_channel,
};
use crate::core::log::take_pending_records;
use crate::{
    ControllerId, DeviceController, DeviceOp, ProjectRefreshOutcome, SlotEditOp, StudioController,
    UiAction, UiLogDraft, UiLogLevel, UiLogOrigin, UiStudioView, UxUpdate, UxUpdateSink,
};

/// The default passive-refresh backoff: start at 3 s (the retired flat
/// `PASSIVE_REFRESH_FAILURE_BACKOFF_MS`), double on consecutive failures, cap at
/// 30 s. The actor consults it after a failed/timed-out passive pull.
pub const PASSIVE_REFRESH_BACKOFF_BASE: Duration = Duration::from_secs(3);
/// Cap for [`PASSIVE_REFRESH_BACKOFF_BASE`] exponential backoff.
pub const PASSIVE_REFRESH_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// The surface the UI holds after spawning the actor: a sender for commands, a
/// receiver for change-gated view snapshots, and a read-only cell carrying the
/// delay the UI timer should wait before the next `RefreshTick`. This is the
/// *entire* boundary the web crate sees — no controller handle, no take/put.
pub struct StudioHandle {
    /// Enqueue commands (user actions and refresh ticks).
    pub tx: CommandSender,
    /// Receive change-gated `UiStudioView` snapshots.
    pub view: StudioViewReceiver,
    /// Shared next-tick delay (cadence + backoff), maintained by the actor. The
    /// UI timer reads it each tick via [`StudioHandle::next_refresh_delay`], so
    /// no cadence policy lives in the view layer (Q3).
    pub delay: Rc<Cell<Duration>>,
}

impl StudioHandle {
    /// The delay the UI timer should wait before enqueuing the next
    /// [`StudioCommand::RefreshTick`]: the connection's cadence interval
    /// (data in core, per Q3) plus any active passive-refresh backoff. The web
    /// shell reads this each tick, so no cadence policy lives in the view layer.
    pub fn next_refresh_delay(&self) -> Duration {
        self.delay.get()
    }
}

/// A shared, single-threaded cancel flag handed to the pull loop.
///
/// The actor flips it when a preempting command arrives; the in-flight pull
/// observes it at a frame boundary. `Rc<Cell<bool>>` is enough because the actor
/// and its pull run on one task (`?Send`).
#[derive(Clone, Default)]
struct SharedCancel {
    cancelled: Rc<Cell<bool>>,
}

impl SharedCancel {
    fn new() -> Self {
        Self::default()
    }

    fn set(&self) {
        self.cancelled.set(true);
    }

    fn reset(&self) {
        self.cancelled.set(false);
    }
}

impl CancelSignal for SharedCancel {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
}

/// The actor: owns the controller and drives it from the command queue.
pub struct StudioActor<MakeTimer> {
    controller: StudioController,
    commands: CommandReceiver,
    view_out: StudioViewSender,
    backoff: BackoffPolicy,
    /// Builds a fresh quiet-gap timer for a pull's [`ProgressDeadline`]. Native
    /// callers pass a `sleep`-backed factory; wasm callers a `setTimeout` one.
    make_timer: MakeTimer,
    /// Shared with the [`StudioHandle`]: the delay the UI timer waits before the
    /// next tick (cadence interval + current backoff), refreshed each batch.
    delay: Rc<Cell<Duration>>,
}

impl<MakeTimer, Timer> StudioActor<MakeTimer>
where
    MakeTimer: FnMut(Duration) -> Timer + Clone,
    Timer: Future<Output = ()>,
{
    /// Create an actor plus the [`StudioHandle`] the UI keeps.
    ///
    /// `make_timer` is the platform timer factory used to build each pull's
    /// progress deadline. Call [`StudioActor::run`] to drive the loop (wasm:
    /// under `spawn_local`; tests: under a bare waker).
    pub fn new(controller: StudioController, make_timer: MakeTimer) -> (Self, StudioHandle) {
        let (tx, commands) = command_channel();
        let (view_out, view) = studio_view_channel();
        // Seed the shared delay from the controller's initial cadence so the UI
        // timer has a sane first interval before the first batch runs.
        let delay = Rc::new(Cell::new(controller.refresh_cadence().interval()));
        let actor = Self {
            controller,
            commands,
            view_out,
            backoff: BackoffPolicy::new(PASSIVE_REFRESH_BACKOFF_BASE, PASSIVE_REFRESH_BACKOFF_MAX),
            make_timer,
            delay: Rc::clone(&delay),
        };
        (actor, StudioHandle { tx, view, delay })
    }

    /// Run the command loop until all senders drop or a
    /// [`StudioCommand::Shutdown`] is processed.
    ///
    /// Each iteration: await a coalesced command batch, plan it (coalesce ticks,
    /// order actions first), execute, then emit **one** snapshot if the
    /// controller's change gate says the view changed.
    pub async fn run(mut self) {
        // Emit the initial view so the UI has a first snapshot before any command.
        self.emit_if_changed();

        while let Some(batch) = self.commands.recv_coalesced().await {
            if !self.process_batch(batch).await {
                break;
            }
        }
    }

    /// Process one coalesced batch. Returns `false` when a `Shutdown` was seen
    /// (the loop should stop after this batch is fully processed).
    async fn process_batch(&mut self, batch: Vec<StudioCommand>) -> bool {
        let plan = CommandPlan::from_batch(batch);

        // Console commands are synchronous state mutations with no async work;
        // apply them first (in queue order, never coalesced) so the batch's
        // final snapshot reflects the reshaped console. Actions run next
        // (preemption-as-priority); the coalesced tick runs after. Shutdown
        // only ends the loop after the batch is processed.
        if let Some(attachment) = plan.attach_library {
            self.controller.attach_library(attachment.0);
        }
        if plan.library_changed {
            self.controller.request_library_refresh();
        }
        for command in plan.console {
            self.controller.apply_console_command(command);
        }
        for action in plan.actions {
            self.run_action(action).await;
        }
        if plan.tick {
            self.run_refresh_tick().await;
        }
        // Re-hydrate the gallery / release closed projects' locks when due
        // (attach or LibraryChanged with no action in the batch; actions
        // settle inside dispatch).
        self.controller.settle_library().await;

        self.emit_if_changed();
        self.publish_refresh_delay();
        !plan.shutdown
    }

    /// Refresh the shared next-tick delay the UI timer reads: the connection's
    /// cadence interval (core policy) plus any active backoff. Called after each
    /// processed batch so a cadence change (e.g. simulator connects) or a backoff
    /// bump takes effect on the next tick.
    fn publish_refresh_delay(&self) {
        let interval = self.controller.refresh_cadence().interval();
        self.delay
            .set(interval.saturating_add(self.backoff.current_delay()));
    }

    /// Dispatch a user action through the controller.
    ///
    /// Progressive updates emitted while a long action runs are forwarded to the
    /// view channel so intermediate state reaches the UI mid-op (matching the
    /// retired web `apply_update` path). A `View` snapshot replaces the live
    /// view; an `Activity` update mutates the latest live view in place (via
    /// [`UiStudioView::apply_activity`]) and republishes it; a `Log` is appended
    /// to that live view. The final change-gated snapshot is still emitted by
    /// `process_batch`.
    async fn run_action(&mut self, action: UiAction) {
        let publisher = self.view_out.publisher();
        // The controller's stamping clock, shared so progressive `Log` drafts
        // get real timestamps: these entries never pass through `push_log`
        // (the producer already buffered a copy for the ring).
        let clock = self.controller.clock();
        // The live view the progressive updates mutate. `Activity`/`Log` updates
        // are deltas against the most recent full `View` snapshot.
        let live: Rc<RefCell<Option<UiStudioView>>> = Rc::new(RefCell::new(None));
        let updates = UxUpdateSink::new({
            let live = Rc::clone(&live);
            move |update| match update {
                UxUpdate::View(view) => {
                    *live.borrow_mut() = Some(view.clone());
                    publisher.send(view);
                }
                UxUpdate::Activity {
                    target,
                    status,
                    activity,
                } => {
                    let mut live = live.borrow_mut();
                    if let Some(view) = live.as_mut() {
                        view.apply_activity(&target, status, activity);
                        publisher.send(view.clone());
                    }
                }
                UxUpdate::Log(draft) => {
                    if let Some(view) = live.borrow_mut().as_mut() {
                        // `push_live` applies the view's carried filter state,
                        // keeping the live view consistent with the change-gated
                        // snapshot that will replace it.
                        view.console.push_live(draft.clone().stamp(clock()));
                        publisher.send(view.clone());
                    }
                }
            }
        });
        let result = self.controller.dispatch_with_updates(action, updates).await;
        match result {
            Ok(outcome) => {
                for notice in outcome.notices {
                    self.controller_log(UiLogDraft::from_notice(notice));
                }
            }
            Err(error) => self.controller_log(UiLogDraft::from_error(error)),
        }
    }

    /// Run one passive refresh tick as a cancellable, deadline-bounded pull,
    /// concurrently watching the queue so a preempting command cancels it.
    async fn run_refresh_tick(&mut self) {
        let cancel = SharedCancel::new();
        cancel.reset();
        let deadline =
            ProgressDeadline::new(crate::PASSIVE_REFRESH_DEADLINE, self.make_timer.clone());

        // Race the pull against "a preempting command arrived". The pull borrows
        // the controller; the watcher only peeks the queue and flips `cancel`.
        let outcome = {
            let pull = self
                .controller
                .refresh_loaded_project_tick_gated(deadline, &cancel);
            let watch = watch_for_preempt(&self.commands, &cancel);
            pull_while_watching(pull, watch).await
        };

        match outcome {
            Ok(Some(ProjectRefreshOutcome::Synced(sync))) => {
                if sync.synced {
                    self.backoff.record_success();
                } else {
                    // A recorded sync failure applies backoff, just like the
                    // retired web `delay_next_project_refresh` did.
                    self.backoff.record_failure();
                }
            }
            Ok(Some(ProjectRefreshOutcome::TimedOut)) => {
                self.controller
                    .mark_passive_project_refresh_failed("passive project refresh timed out");
                self.backoff.record_failure();
            }
            // A clean cancel (preempted) is not a failure: no backoff, no mark.
            Ok(Some(ProjectRefreshOutcome::Cancelled)) => {}
            // Nothing to refresh (no loaded project / LightPlayer).
            Ok(None) => {}
            Err(error) => {
                self.controller
                    .mark_passive_project_refresh_failed(error.to_string());
                self.controller_log(UiLogDraft::from_error(error));
                self.backoff.record_failure();
            }
        }
    }

    /// The delay to apply before the next passive refresh, per the backoff
    /// policy (zero while healthy). The UI timer adds this to its cadence.
    pub fn refresh_backoff_delay(&self) -> Duration {
        self.backoff.current_delay()
    }

    fn controller_log(&mut self, draft: UiLogDraft) {
        // Route action/error logs through the controller's bounded ring so the
        // cap lives in core. `push_log` stamps the draft and marks the view
        // dirty.
        self.controller.push_log(draft);
    }

    /// Drain the global `log::` sink queue, then emit a change-gated snapshot
    /// if the controller's view changed.
    ///
    /// This is the sink's **single drain point**: `emit_if_changed` runs once
    /// at loop start and once at the end of every processed batch — which
    /// covers every command batch and every refresh tick — so records logged
    /// *during* a batch land in that batch's own snapshot, and records logged
    /// between batches are picked up by the next one.
    fn emit_if_changed(&mut self) {
        self.drain_log_sink();
        if let Some(view) = self.controller.view_if_changed() {
            self.view_out.send(view);
        }
    }

    /// Move pending [`StudioLogSink`](crate::StudioLogSink) records into the
    /// controller ring: origin `Studio`, the record target as detail, stamped
    /// by the controller clock like every other draft. If the sink dropped
    /// records to overflow since the last drain, one Warn entry reporting the
    /// count is appended after the retained records.
    fn drain_log_sink(&mut self) {
        let (records, dropped) = take_pending_records();
        for record in records {
            self.controller.push_log(record.into_draft());
        }
        if dropped > 0 {
            self.controller.push_log(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                format!("log sink dropped {dropped} records"),
            ));
        }
    }

    /// Drive exactly one coalesced batch (test-only), so a test can inspect
    /// state — e.g. backoff — after a single tick without ending the loop.
    #[cfg(test)]
    pub(crate) async fn run_one_batch_for_test(&mut self) {
        if let Some(batch) = self.commands.recv_coalesced().await {
            let _ = self.process_batch(batch).await;
        }
    }
}

/// A planned batch: the console commands to apply (in queue order, never
/// coalesced), the ordered actions to run, whether a tick should run
/// (coalesced to at most one), and whether shutdown was requested.
struct CommandPlan {
    console: Vec<ConsoleCommand>,
    actions: Vec<UiAction>,
    tick: bool,
    shutdown: bool,
    /// A library attachment to install before actions (at most one; the
    /// shell sends it once, ahead of any project action).
    attach_library: Option<crate::app::studio::studio_command::LibraryAttachment>,
    /// Coalesced cross-tab library-change pings: schedule one gallery
    /// re-hydration for the whole batch.
    library_changed: bool,
}

impl CommandPlan {
    fn from_batch(batch: Vec<StudioCommand>) -> Self {
        let mut console = Vec::new();
        let mut actions = Vec::new();
        let mut tick = false;
        let mut shutdown = false;
        let mut attach_library = None;
        let mut library_changed = false;
        for command in batch {
            match command {
                StudioCommand::AttachLibrary(attachment) => attach_library = Some(attachment),
                StudioCommand::LibraryChanged => library_changed = true,
                StudioCommand::Action(action) => push_action_coalesced(&mut actions, action),
                // Not a local console mutation: a device-level change is a
                // server round-trip, so convert it into the equivalent device
                // action here and let it ride the normal async action path.
                StudioCommand::Console(ConsoleCommand::SetDeviceLogLevel(level)) => {
                    push_action_coalesced(
                        &mut actions,
                        UiAction::from_op(
                            ControllerId::new(DeviceController::NODE_ID),
                            DeviceOp::SetLogLevel { level },
                        ),
                    );
                }
                // Never coalesced: each console command is a distinct user
                // gesture whose relative order matters (e.g. Clear between
                // two level changes).
                StudioCommand::Console(command) => console.push(command),
                // Coalesce: many queued ticks collapse to one pull.
                StudioCommand::RefreshTick => tick = true,
                StudioCommand::Shutdown => shutdown = true,
            }
        }
        Self {
            console,
            actions,
            tick,
            shutdown,
            attach_library,
            library_changed,
        }
    }
}

/// Queue an action into the plan, coalescing `SlotEditOp::SetValue` per slot
/// address so an `oninput` flood collapses to one mutation.
///
/// The rule is deliberately dumb: scanning back from the tail, while the
/// queued actions are still `SetValue`s, a queued `SetValue` for the **same
/// address** is replaced in place by the newer one (latest value wins, order
/// otherwise preserved). Any other action — a `Revert`, a structural gesture
/// (`EnsurePresent`/`RemoveValue`), `SaveOverlay`, or an unrelated op — never
/// coalesces and is a barrier: nothing coalesces across it. Structural ops
/// change what a path *means*, so each queued gesture must reach the server
/// in order.
fn push_action_coalesced(actions: &mut Vec<UiAction>, action: UiAction) {
    let Some(SlotEditOp::SetValue { address, .. }) = action.op_as::<SlotEditOp>() else {
        actions.push(action);
        return;
    };
    let address = address.clone();
    for queued in actions.iter_mut().rev() {
        match queued.op_as::<SlotEditOp>() {
            Some(SlotEditOp::SetValue {
                address: queued_address,
                ..
            }) => {
                if *queued_address == address {
                    *queued = action;
                    return;
                }
            }
            // A Revert (or any non-SetValue action) is a barrier.
            _ => break,
        }
    }
    actions.push(action);
}

/// Peek the command queue (without consuming) for a command whose class
/// preempts a passive refresh, and flip `cancel` when one appears. Resolves once
/// it has requested cancellation, or stays pending while no preempting command
/// is queued.
async fn watch_for_preempt(commands: &CommandReceiver, cancel: &SharedCancel) {
    core::future::poll_fn(|cx: &mut Context<'_>| {
        if commands.peek_any(|command| command_preempts_passive(command)) {
            cancel.set();
            return Poll::Ready(());
        }
        commands.register_waker(cx.waker());
        Poll::Pending
    })
    .await
}

/// Drive `pull` to completion while polling `watch` alongside it. `watch` never
/// resolves the outer future — its only job is the cancel side effect — so once
/// the pull finishes we return its outcome.
async fn pull_while_watching<PullFut, WatchFut>(pull: PullFut, watch: WatchFut) -> PullFut::Output
where
    PullFut: Future,
    WatchFut: Future<Output = ()>,
{
    let mut pull = pin!(pull);
    let mut watch = pin!(watch);
    let mut watch_done = false;
    core::future::poll_fn(move |cx: &mut Context<'_>| {
        // Poll the watcher first so a preempting command sets the cancel flag
        // before we next poll the pull (which checks it at its frame boundary).
        // Once the watcher resolves (it has requested cancellation) we stop
        // polling it — polling a completed future would panic.
        if !watch_done && watch.as_mut().poll(cx).is_ready() {
            watch_done = true;
        }
        pull.as_mut().poll(cx)
    })
    .await
}

fn command_preempts_passive(command: &StudioCommand) -> bool {
    match command {
        StudioCommand::Action(action) => action.class().preempts_passive_refresh(),
        // A queued console command, tick, or shutdown does not preempt an
        // in-flight pull: console mutations are display-side and can wait for
        // the batch after the pull completes.
        // An attachment is synchronous installation work, same as console;
        // a cross-tab library ping is background gallery staleness.
        StudioCommand::AttachLibrary(_)
        | StudioCommand::LibraryChanged
        | StudioCommand::Console(_)
        | StudioCommand::RefreshTick
        | StudioCommand::Shutdown => false,
    }
}

/// Poll a future once with a no-op waker and return its output if ready.
///
/// Test-only helper shared with the view-channel tests: the in-crate channels
/// resolve synchronously when data is queued, so a single poll suffices.
#[cfg(test)]
pub(crate) fn poll_now<F: Future>(future: F) -> Option<F::Output> {
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("studio_actor_tests.rs");
}
