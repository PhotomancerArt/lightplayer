//! The docs-sim host: one leased studio instance behind a narrow handle.

use core::cell::Cell;
use core::future::Future;
use core::pin::Pin;
use core::time::Duration;
use std::rc::Rc;

use crate::app::project::project_controller::ProjectController;
use crate::app::studio::studio_view_channel::{CommandSender, StudioViewReceiver};
use crate::{
    ControllerId, DeviceController, DeviceOp, ProjectOp, StudioActor, StudioActorOptions,
    StudioCommand, StudioController, StudioHandle, UiAction,
};

/// A leased studio instance for one docs sim: a real controller + actor
/// whose first queued command connects a browser-worker sim and deploys a
/// compiled-in example directly (never through the library).
///
/// # Lifecycle contract (leak-critical)
///
/// Nothing in the actor/controller/pool/session chain terminates the
/// worker on drop — only [`DeviceOp::StopSimulator`] reaches
/// `worker.terminate()`. [`DocsSimHost::shutdown`] enqueues exactly that
/// followed by [`StudioCommand::Shutdown`]; the actor's batch plan runs
/// actions before honoring shutdown, so one enqueue is a complete,
/// ordered teardown that the actor task finishes **on its own** — the
/// page's tasks may die immediately after calling it. `Drop` also calls
/// `shutdown()` as a backstop, but the page should call it explicitly on
/// unmount (a dropped-but-unpolled actor future cannot process the
/// queue; only the still-spawned actor task can).
pub struct DocsSimHost {
    tx: CommandSender,
    /// The change-gated view stream; taken once by the page's view loop.
    view: Option<StudioViewReceiver>,
    delay: Rc<Cell<Duration>>,
    shutdown_sent: Cell<bool>,
}

impl DocsSimHost {
    /// Boot a docs sim host.
    ///
    /// `controller` is caller-built so platform seams stay in the shell
    /// (clock via [`StudioController::new`], device timers via
    /// [`StudioController::set_device_timers`]); the caller must **not**
    /// install the global log sink for it. `spawn` runs the actor future
    /// and must outlive the component that booted it (on the web:
    /// `wasm_bindgen_futures::spawn_local`, *not* a scope-tied spawn —
    /// teardown depends on the actor task surviving unmount long enough
    /// to process [`Self::shutdown`]'s queue). `make_timer` is the same
    /// platform timer factory the main actor takes.
    pub fn boot<Spawn, MakeTimer, Timer>(
        example_id: impl Into<String>,
        controller: StudioController,
        spawn: Spawn,
        make_timer: MakeTimer,
    ) -> Self
    where
        Spawn: FnOnce(Pin<Box<dyn Future<Output = ()>>>),
        MakeTimer: FnMut(Duration) -> Timer + Clone + 'static,
        Timer: Future<Output = ()> + 'static,
    {
        // Never drain the global log sink: that queue belongs to the app's
        // main actor, and a second drainer silently steals its records.
        let (actor, handle) = StudioActor::new_with_options(
            controller,
            make_timer,
            StudioActorOptions { drain_logs: false },
        );
        let StudioHandle { tx, view, delay } = handle;
        // The bootstrap rides the queue as the first action: connect the
        // sim, lens it, deploy the example (ProjectOp::OpenDocsExample).
        tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::OpenDocsExample {
                example_id: example_id.into(),
            },
        )));
        spawn(Box::pin(actor.run()));
        Self {
            tx,
            view: Some(view),
            delay,
            shutdown_sent: Cell::new(false),
        }
    }

    /// Take the change-gated view stream (once). The page's view loop
    /// `recv().await`s it into whatever reactive cell its embeds read.
    pub fn take_view(&mut self) -> Option<StudioViewReceiver> {
        self.view.take()
    }

    /// Dispatch a real [`UiAction`] into this host's controller — the
    /// same path a Studio surface dispatches through. Read-only embeds
    /// simply never call this.
    pub fn dispatch(&self, action: UiAction) {
        if self.shutdown_sent.get() {
            return;
        }
        self.tx.send(StudioCommand::Action(action));
    }

    /// Enqueue one refresh tick (the view pull). The page's tick loop
    /// calls this on the cadence of [`Self::next_refresh_delay`] and
    /// simply stops calling while the page/embeds are hidden — the sim
    /// firmware keeps self-ticking either way; pausing only freezes the
    /// pulled view.
    pub fn send_refresh_tick(&self) {
        if self.shutdown_sent.get() {
            return;
        }
        self.tx.send(StudioCommand::RefreshTick);
    }

    /// The delay the tick loop should wait before the next
    /// [`Self::send_refresh_tick`] (cadence + backoff, maintained by the
    /// actor).
    pub fn next_refresh_delay(&self) -> Duration {
        self.delay.get()
    }

    /// The docs "reset" chip: re-deploy the example's pristine files onto
    /// the live sim (same op as boot; the controller reuses the running
    /// worker).
    pub fn reset(&self, example_id: impl Into<String>) {
        self.dispatch(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::OpenDocsExample {
                example_id: example_id.into(),
            },
        ));
    }

    /// Tear down: stop the sim (closes the provider session —
    /// `worker.terminate()` on the web — and removes it from this host's
    /// pool), then shut the actor loop down. Idempotent; fire-and-forget
    /// (the spawned actor task completes the teardown by itself).
    pub fn shutdown(&self) {
        if self.shutdown_sent.replace(true) {
            return;
        }
        self.tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::StopSimulator,
        )));
        self.tx.send(StudioCommand::Shutdown);
    }
}

impl Drop for DocsSimHost {
    fn drop(&mut self) {
        // Backstop only — see the lifecycle contract on the type.
        self.shutdown();
    }
}
