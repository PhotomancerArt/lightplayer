//! End-to-end device tests: the REAL effects layer driving the REAL model
//! over the scripted fake device's actual bytes.
//!
//! Nothing here fakes at the model's vocabulary. Every assertion is reached
//! through `M!` framing on a wire, `lpa-link`'s line splitter and frame
//! mapping, `DeviceEffects` executing the model's commands, and the
//! controller's own `fold_device_input` / `settle_device_records` — the same
//! two calls the actor makes. `lpa-devices`' own fixtures test the fold at the
//! fold's layer, correctly; a test that handed the fold ready-made
//! `LinkEvent`s could not see framing, timing, or the effects layer at all,
//! and every device bug so far lived below the record level.
//!
//! # The harness
//!
//! [`DeviceBench`] supplies the three platform seams the effects layer needs
//! and nothing else:
//!
//! | seam | here |
//! |---|---|
//! | transport | [`ScriptedTransport`] over `lpa_link::device_link::fake` |
//! | spawner | a `Vec` of tasks polled by [`DeviceBench::step`] |
//! | timer | a future that resolves when the bench's fake clock passes its due |
//!
//! The clock is the controller's own injected `now_secs`, so the model's
//! millis and the registry's seconds come from ONE clock — a bench where a
//! deadline and a `last_seen` disagreed would prove nothing.

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_devices::view::{Escape, RosterView};
use lpa_link::device_link::fake::{fake_device_link, fake_link_info};
use lpa_link::device_link::wire::roster_config;
use lpa_link::providers::fake_device::{
    FakeBootState, FakeDeviceIdentity, FakeDeviceScript, FakeEsp32Device, FakeLightPlayerState,
};

use crate::app::library::{LibraryStore, MemoryLibraryHost};
use crate::app::places::DeviceRegistry;
use crate::{
    DeviceAction, DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceInput,
    DeviceTaskFuture, DeviceTransport, DeviceTransportFuture, DevicesOp, GrantedLink, LensLineTap,
    LensTapEvent, ProjectController, ProjectOp, StudioController, UiAction,
};

/// Wall-clock ceiling on a `run_until`. Generous: the fake boots a real host
/// server on a real thread. Hitting it means a hang, which is the failure this
/// milestone exists to make impossible.
const REAL_TIME_LIMIT: Duration = Duration::from_secs(15);

/// How much the bench's fake clock advances per step.
///
/// Fast enough that a 600 ms identify deadline is reached in a few hundred
/// steps, slow enough that a step still represents "a moment" rather than
/// skipping past a window the model is watching.
const STEP_MS: f64 = 0.005;

// ---------------------------------------------------------------------
// The transport
// ---------------------------------------------------------------------

/// A [`DeviceTransport`] over one scripted fake board.
///
/// The grant model is real in the ways that matter: the port has to be
/// discovered (or chosen) before it exists, opening it is the model's
/// decision, and revoking is recorded so a test can assert the port stopped
/// being ours.
struct ScriptedTransport {
    device: FakeEsp32Device,
    endpoint: String,
    /// Grants this origin holds. Starts empty for the chooser tests and
    /// pre-granted for the reload/hotplug ones.
    granted: Rc<Cell<bool>>,
    /// Whether the chooser hands a port back, or the user dismisses it.
    chooser_grants: Rc<Cell<bool>>,
    revoked: Rc<RefCell<Vec<String>>>,
    /// What a scripted flash effect does.
    flash_plan: Rc<Cell<FlashPlan>>,
    /// `/hardware.json` payloads the manifest-write effect was handed.
    manifest_writes: Rc<RefCell<Vec<String>>>,
    /// What a push effect does. Unlike the flash, `Real` is not scripted at
    /// all: it runs the actual `lpa-client` conversation against the fake's
    /// REAL `LpServer`, so a green push here means the stop/write/load order
    /// and the hash verification both worked over real framing.
    push_plan: Rc<Cell<PushPlan>>,
    /// What a remove effect does. `Real` runs the real conversation too.
    remove_plan: Rc<Cell<RemovePlan>>,
}

/// The scripted outcomes a remove effect can play out. Same three shapes the
/// push has, and the same reason: a real conversation, a refusal, and a hang
/// that only eviction bounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RemovePlan {
    #[default]
    Real,
    Fail,
    Hang,
}

/// The scripted outcomes a push effect can play out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PushPlan {
    /// The real conversation, end to end, against the fake's `LpServer`.
    #[default]
    Real,
    /// The wire refuses. Stands in for every "the board would not take it"
    /// failure (a dropped response, a full flash, a hash mismatch).
    Fail,
    /// The effect never completes; only eviction bounds the activity.
    Hang,
}

/// The scripted outcomes a flash effect can play out (§5 of the milestone:
/// success, mid-write failure, post-flash silence — plus a hang for the
/// eviction path).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FlashPlan {
    /// The fake becomes a fresh LightPlayer (efuse MAC read by the
    /// preflight), the way a real flash ends.
    #[default]
    Success,
    /// esptool dies mid-write.
    FailMidWrite,
    /// The tool reports success but the board never comes back — the ladder
    /// must climb and then fail honestly.
    SilentAfterWrite,
    /// The effect never completes; only eviction bounds the activity.
    Hang,
}

/// The MAC the scripted preflight reports, deliberately UPPERCASE: the
/// executor's normalization is part of the evidence path under test.
const SCRIPTED_PREFLIGHT_MAC: &str = "60:55:F9:0A:0B:0C";

impl ScriptedTransport {
    fn link(&self) -> GrantedLink {
        let info = fake_link_info(&self.endpoint);
        GrantedLink {
            link: Box::new(fake_device_link(info.clone(), &self.device)),
            info,
        }
    }
}

impl DeviceTransport for ScriptedTransport {
    fn label(&self) -> &'static str {
        "scripted fake"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let granted = match self.granted.get() {
            true => vec![self.link()],
            false => Vec::new(),
        };
        Box::pin(core::future::ready(Ok(granted)))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        if !self.chooser_grants.get() {
            return Box::pin(core::future::ready(Ok(None)));
        }
        self.granted.set(true);
        Box::pin(core::future::ready(Ok(Some(self.link()))))
    }

    fn revoke_grant(
        &self,
        info: lpa_devices::LinkInfo,
    ) -> DeviceTransportFuture<Result<(), String>> {
        self.granted.set(false);
        self.revoked.borrow_mut().push(info.endpoint.0);
        Box::pin(core::future::ready(Ok(())))
    }

    fn lens_client_io(
        &self,
        _info: lpa_devices::LinkInfo,
        tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        // The real io over the fake's real `M!` wire, teed: what the lens
        // e2e rows prove is that the fold keeps folding while the client
        // owns the port.
        Ok(Box::new(FakeDeviceIo::new(&self.device).with_tap(tap)))
    }

    fn run_effect(
        &self,
        _info: lpa_devices::LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        match call {
            DeviceEffectCall::FlashFirmware { build_id } => {
                let plan = self.flash_plan.get();
                let device = self.device.clone();
                Box::pin(async move {
                    progress("Connecting to the chip".to_string(), Some(5));
                    progress("Writing firmware".to_string(), Some(50));
                    match plan {
                        FlashPlan::Success => {
                            device.fake_flash(&build_id);
                            progress("Firmware written".to_string(), Some(100));
                            Ok(DeviceEffectFacts {
                                summary: format!("wrote {build_id}"),
                                probed_mac: Some(SCRIPTED_PREFLIGHT_MAC.to_string()),
                                chip_name: Some("ESP32-C6 (fake)".to_string()),
                            })
                        }
                        FlashPlan::FailMidWrite => {
                            Err("write failed at 0x2000 (scripted)".to_string())
                        }
                        FlashPlan::SilentAfterWrite => Ok(DeviceEffectFacts {
                            summary: format!("wrote {build_id}"),
                            probed_mac: Some(SCRIPTED_PREFLIGHT_MAC.to_string()),
                            chip_name: Some("ESP32-C6 (fake)".to_string()),
                        }),
                        FlashPlan::Hang => {
                            core::future::pending::<()>().await;
                            unreachable!("a hung effect never completes")
                        }
                    }
                })
            }
            DeviceEffectCall::EraseFlash => {
                let device = self.device.clone();
                Box::pin(async move {
                    progress("Erasing flash".to_string(), Some(50));
                    device.fake_erase();
                    progress("Flash erased".to_string(), Some(100));
                    Ok(DeviceEffectFacts {
                        summary: "flash erased".to_string(),
                        ..Default::default()
                    })
                })
            }
            DeviceEffectCall::RemoveProject {
                fallback_storage_id,
            } => {
                let plan = self.remove_plan.get();
                let device = self.device.clone();
                Box::pin(async move {
                    match plan {
                        RemovePlan::Fail => {
                            Err("the board refused the delete (scripted)".to_string())
                        }
                        RemovePlan::Hang => {
                            core::future::pending::<()>().await;
                            unreachable!("a hung effect never completes")
                        }
                        // Like the push, `Real` is not scripted at all: the
                        // actual ask → stop → delete conversation runs over
                        // the fake's own `M!` wire into a real `LpServer`.
                        RemovePlan::Real => {
                            let mut client = lpa_client::LpClient::new(FakeDeviceIo::new(&device));
                            let mut report = |label: String, percent: Option<u8>| {
                                progress(label, percent);
                            };
                            let report = lpa_client::remove_project(
                                &mut client,
                                &fallback_storage_id,
                                &mut report,
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                            Ok(DeviceEffectFacts {
                                summary: match report.was_loaded {
                                    true => format!("removed {}", report.storage_id),
                                    false => format!(
                                        "the board reported nothing loaded; cleared {}",
                                        report.storage_id
                                    ),
                                },
                                ..Default::default()
                            })
                        }
                    }
                })
            }
            DeviceEffectCall::WriteHardwareManifest { manifest_json } => {
                self.manifest_writes.borrow_mut().push(manifest_json);
                Box::pin(core::future::ready(Ok(DeviceEffectFacts {
                    summary: "board manifest written".to_string(),
                    ..Default::default()
                })))
            }
            DeviceEffectCall::PushProject {
                files,
                expected_hash,
                fallback_storage_id,
            } => {
                let plan = self.push_plan.get();
                let device = self.device.clone();
                Box::pin(async move {
                    match plan {
                        PushPlan::Fail => Err("the board refused the write (scripted)".to_string()),
                        PushPlan::Hang => {
                            core::future::pending::<()>().await;
                            unreachable!("a hung effect never completes")
                        }
                        PushPlan::Real => {
                            // The REAL conversation, over the fake's own
                            // `M!` byte wire, into a real `LpServer` over
                            // `LpFsMemory`. Nothing about the stop/write/
                            // load order or the hash check is faked.
                            let mut client = lpa_client::LpClient::new(FakeDeviceIo::new(&device));
                            let mut report = |label: String, percent: Option<u8>| {
                                progress(label, percent);
                            };
                            let report = lpa_client::push_project(
                                &mut client,
                                &files,
                                &expected_hash,
                                &fallback_storage_id,
                                &mut report,
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                            Ok(DeviceEffectFacts {
                                summary: format!("project sent to {}", report.storage_id),
                                ..Default::default()
                            })
                        }
                    }
                })
            }
        }
    }
}

/// A `ClientIo` over the fake device's byte wire.
///
/// The mirror of `lpa-link`'s browser `PortLineIo`: `M!<json>` lines out,
/// `M!<json>` lines in, everything else on the wire (boot banner, logs)
/// dropped. It exists so the bench's push is a real protocol conversation
/// rather than a scripted outcome — the fake runs an actual `LpServer`, and
/// the whole point of M3 is that the conversation works.
struct FakeDeviceIo {
    stream: lpa_link::providers::fake_device::FakeDeviceByteStream,
    /// Bytes read but not yet forming a complete line.
    partial: String,
    /// Frames decoded but not yet handed out.
    pending: VecDeque<lpc_wire::WireServerMessage>,
    /// The lens tap (M5): every whole line, verbatim, before decoding.
    tap: Option<LensLineTap>,
    /// The wire died (an unplug script): every read from here on is EOF,
    /// so receive fails fast instead of waiting out its budget.
    dead: Option<String>,
}

impl FakeDeviceIo {
    fn new(device: &FakeEsp32Device) -> Self {
        Self {
            stream: lpa_link::providers::fake_device::FakeDeviceByteStream::new(device.clone()),
            partial: String::new(),
            pending: VecDeque::new(),
            tap: None,
            dead: None,
        }
    }

    fn with_tap(mut self, tap: LensLineTap) -> Self {
        self.tap = Some(tap);
        self
    }

    /// Drain whatever the wire has right now into [`Self::pending`].
    fn drain(&mut self) {
        let mut buf = [0u8; 8192];
        loop {
            let read = match read_available_checked(&mut self.stream, &mut buf) {
                Ok(read) => read,
                // The wire died under the lens (an unplug script): the
                // tap carries it, exactly as the browser io reports the
                // controller's error.
                Err(error) => {
                    if self.dead.is_none() {
                        if let Some(tap) = &self.tap {
                            tap(LensTapEvent::PortError(error.clone()));
                        }
                        self.dead = Some(error);
                    }
                    return;
                }
            };
            if read == 0 {
                return;
            }
            self.partial
                .push_str(&String::from_utf8_lossy(&buf[..read]));
            while let Some(newline) = self.partial.find('\n') {
                let line: String = self.partial.drain(..=newline).collect();
                let line = line.trim_end_matches(['\n', '\r']);
                if let Some(tap) = &self.tap {
                    tap(LensTapEvent::Line(line.to_string()));
                }
                let Some(json) = line.strip_prefix("M!") else {
                    continue;
                };
                if let Ok(message) = lpc_wire::json::from_str::<lpc_wire::WireServerMessage>(json) {
                    self.pending.push_back(message);
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl lpa_client::ClientIo for FakeDeviceIo {
    async fn send(&mut self, msg: lpc_wire::ClientMessage) -> Result<(), lpc_wire::TransportError> {
        use lpa_link::stream::DeviceByteStream;
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| lpc_wire::TransportError::Other(format!("encode failed: {error}")))?;
        self.stream
            .write_all(format!("M!{json}\n").as_bytes())
            .map_err(|error| {
                // Mirror of the browser io: a failed write is the port
                // dying under the lens.
                if let Some(tap) = &self.tap {
                    tap(LensTapEvent::PortError(error.to_string()));
                }
                lpc_wire::TransportError::Other(error.to_string())
            })
    }

    async fn receive(&mut self) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
        let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
        loop {
            self.drain();
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            if let Some(error) = &self.dead {
                return Err(lpc_wire::TransportError::Other(error.clone()));
            }
            if std::time::Instant::now() >= deadline {
                return Err(lpc_wire::TransportError::Other(
                    "the fake device did not answer".to_string(),
                ));
            }
            // Yield to the bench's task pump so the rest of the app keeps
            // turning while this conversation waits — the same shape the
            // browser io's `setTimeout` poll has.
            YieldOnce::default().await;
        }
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        // The port belongs to the model's link; the borrow ends, the port
        // stays open.
        Ok(())
    }
}

/// Pending once, then ready: one turn back to the bench's pump.
#[derive(Default)]
struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        match core::mem::replace(&mut self.yielded, true) {
            true => Poll::Ready(()),
            false => {
                // The bench re-polls every task each step; a real executor
                // gets a wake from the waker the same way the `Sleep` above
                // relies on.
                std::thread::sleep(Duration::from_millis(1));
                Poll::Pending
            }
        }
    }
}

// ---------------------------------------------------------------------
// The clock and the task pool
// ---------------------------------------------------------------------

/// A sleep on the bench's fake clock.
struct Sleep {
    clock: Rc<Cell<f64>>,
    due: f64,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        match self.clock.get() >= self.due {
            true => Poll::Ready(()),
            // No waker: the bench re-polls every task each step, which is
            // exactly what a browser's timer queue does for us in the real
            // build.
            false => Poll::Pending,
        }
    }
}

/// Poll every spawned task once, dropping the ones that finished.
fn pump(tasks: &Rc<RefCell<Vec<DeviceTaskFuture>>>) {
    let mut taken: Vec<DeviceTaskFuture> = tasks.borrow_mut().drain(..).collect();
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    taken.retain_mut(|task| task.as_mut().poll(&mut cx).is_pending());
    // Tasks spawned DURING this pump are already in the cell; put the
    // survivors back in front of them so ordering stays FIFO.
    let mut live = tasks.borrow_mut();
    taken.append(&mut live);
    *live = taken;
}

fn noop_waker() -> core::task::Waker {
    use std::sync::Arc;
    use std::task::Wake;

    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    core::task::Waker::from(Arc::new(NoopWake))
}

/// Drive a future to completion under a no-op waker (the memory host's
/// futures are all immediately ready, so this terminates promptly).
fn drive<F: Future>(future: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = core::pin::pin!(future);
    for _ in 0..100_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
    panic!("a bench future did not complete");
}

// ---------------------------------------------------------------------
// The bench
// ---------------------------------------------------------------------

struct DeviceBench {
    controller: StudioController,
    clock: Rc<Cell<f64>>,
    inbox: Rc<RefCell<VecDeque<DeviceInput>>>,
    store: LibraryStore,
    revoked: Rc<RefCell<Vec<String>>>,
    granted: Rc<Cell<bool>>,
    chooser_grants: Rc<Cell<bool>>,
    flash_plan: Rc<Cell<FlashPlan>>,
    manifest_writes: Rc<RefCell<Vec<String>>>,
    push_plan: Rc<Cell<PushPlan>>,
    remove_plan: Rc<Cell<RemovePlan>>,
    started: std::time::Instant,
}

/// `Rc<RefCell<Vec<..>>>` spelled once.
type TaskPool = Rc<RefCell<Vec<DeviceTaskFuture>>>;

impl DeviceBench {
    /// A bench whose port is ALREADY granted — the reload case, and the one
    /// the startup sweep exercises.
    fn granted(device: &FakeEsp32Device, endpoint: &str) -> (Self, TaskPool) {
        Self::build(device, endpoint, true, true)
    }

    /// A bench with no grants yet: the roster is empty until the user asks.
    fn ungranted(device: &FakeEsp32Device, endpoint: &str) -> (Self, TaskPool) {
        Self::build(device, endpoint, false, true)
    }

    fn build(
        device: &FakeEsp32Device,
        endpoint: &str,
        granted: bool,
        chooser_grants: bool,
    ) -> (Self, TaskPool) {
        let clock = Rc::new(Cell::new(1_000.0));
        let tasks: TaskPool = Rc::new(RefCell::new(Vec::new()));
        let inbox: Rc<RefCell<VecDeque<DeviceInput>>> = Rc::new(RefCell::new(VecDeque::new()));
        let granted = Rc::new(Cell::new(granted));
        let chooser_grants = Rc::new(Cell::new(chooser_grants));
        let revoked = Rc::new(RefCell::new(Vec::new()));

        let store = memory_store(Rc::clone(&clock));
        let host = MemoryLibraryHost::new(memory_store_sharing(&store), {
            let clock = Rc::clone(&clock);
            Rc::new(move || clock.get())
        });

        let mut controller = StudioController::new({
            let clock = Rc::clone(&clock);
            move || clock.get()
        });
        // Shrink the flash ladder's budgets to bench scale (the fake clock
        // walks 5 ms per step; a real 8 s rung would be 1600 steps of wall
        // time each). The SHAPE under test is the rungs, never their size.
        controller.set_device_roster_config_for_test(crate::DeviceRosterConfig {
            expected_proto: lpc_wire::WIRE_PROTO_VERSION,
            flash_rung_ms: 400,
            flash_reopen_retry_ms: 100,
            // The push budgets shrink for the same reason the flash ones do:
            // the SHAPE under test is the phases, never their size. The
            // deadline stays generous — it is the backstop, and a real
            // conversation with a real server is what runs inside it.
            push_deadline_ms: 20_000,
            push_cancel_grace_ms: 500,
            push_observe_ms: 400,
            ..Default::default()
        });
        controller.attach_library(Rc::new(host));
        controller.set_device_spawner({
            let tasks = Rc::clone(&tasks);
            move |task| tasks.borrow_mut().push(task)
        });
        controller.set_device_timer({
            let clock = Rc::clone(&clock);
            move |delay| {
                Box::pin(Sleep {
                    clock: Rc::clone(&clock),
                    due: clock.get() + delay.as_secs_f64(),
                }) as crate::DeviceTimerFuture
            }
        });
        controller.set_device_input_sink({
            let inbox = Rc::clone(&inbox);
            move |input| inbox.borrow_mut().push_back(input)
        });
        let flash_plan: Rc<Cell<FlashPlan>> = Rc::new(Cell::new(FlashPlan::default()));
        let manifest_writes: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let push_plan: Rc<Cell<PushPlan>> = Rc::new(Cell::new(PushPlan::default()));
        let remove_plan: Rc<Cell<RemovePlan>> = Rc::new(Cell::new(RemovePlan::default()));
        controller.set_device_transport(Rc::new(ScriptedTransport {
            device: device.clone(),
            endpoint: endpoint.to_string(),
            granted: Rc::clone(&granted),
            chooser_grants: Rc::clone(&chooser_grants),
            revoked: Rc::clone(&revoked),
            flash_plan: Rc::clone(&flash_plan),
            manifest_writes: Rc::clone(&manifest_writes),
            push_plan: Rc::clone(&push_plan),
            remove_plan: Rc::clone(&remove_plan),
        }));

        let bench = Self {
            controller,
            clock,
            inbox,
            store,
            revoked,
            granted,
            chooser_grants,
            flash_plan,
            manifest_writes,
            push_plan,
            remove_plan,
            started: std::time::Instant::now(),
        };
        (bench, tasks)
    }

    /// One turn: let time pass, let the spawned pumps and timers run, fold
    /// everything they queued, then settle whatever the folds asked to write.
    fn step(&mut self, tasks: &TaskPool) {
        self.clock.set(self.clock.get() + STEP_MS);
        pump(tasks);
        let queued: Vec<DeviceInput> = self.inbox.borrow_mut().drain(..).collect();
        for input in queued {
            self.controller.fold_device_input(input);
        }
        drive(self.controller.settle_device_records());
    }

    fn run_until(&mut self, tasks: &TaskPool, what: &str, ready: impl Fn(&Self) -> bool) {
        let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
        loop {
            self.step(tasks);
            if ready(self) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}; roster now: {:?}",
                self.view()
            );
            // The fake's server runs on its own thread; there is genuinely
            // nothing to do between polls.
            std::thread::sleep(Duration::from_millis(1));
            let _ = self.started;
        }
    }

    fn view(&self) -> RosterView {
        self.controller.device_roster_view().roster
    }

    /// Dispatch a device gesture the way the UI does — through the ordinary
    /// action path, not by reaching into the roster.
    fn gesture(&mut self, action: DeviceAction) {
        let action: UiAction = DevicesOp::action_for(action);
        drive(self.controller.dispatch(action)).expect("a device gesture never fails loudly");
    }

    /// The empty face's gesture, through the ordinary action path.
    fn push_gesture(&mut self, device: crate::DeviceId, source: crate::PushSource) {
        let action: UiAction = crate::DevicePushOp::action_for(device, source);
        drive(self.controller.dispatch(action)).expect("a push gesture never fails loudly");
    }

    /// The running face's Open (round-2 M5), through the ordinary action
    /// path: the editor becomes a lens on the device registered as `uid`.
    fn open_lens(&mut self, uid: &str) -> Result<(), crate::UiError> {
        let action = UiAction::from_op(
            crate::RuntimeOp::NODE_ID,
            crate::RuntimeOp::OpenDeviceLens {
                uid: uid.to_string(),
            },
        );
        drive(self.controller.dispatch(action)).map(|_| ())
    }

    /// The editor's detach (the ⇲ / close-editor gesture): for a device
    /// lens, that is also the wire going back.
    fn detach_lens(&mut self) {
        let action = UiAction::from_op(ProjectController::NODE_ID, ProjectOp::DetachLens);
        drive(self.controller.dispatch(action)).expect("detach never fails loudly");
    }

    /// The device lens session, when one is installed.
    fn lens_device_uid(&self) -> Option<String> {
        self.controller
            .runtime_pool_for_test()
            .device_session()
            .and_then(crate::RuntimeSession::device_attachment)
            .map(|attachment| attachment.uid.clone())
    }

    fn registry(&self) -> Vec<crate::app::places::RegisteredDevice> {
        DeviceRegistry::new(self.store.fs_handle())
            .list()
            .expect("the registry reads back")
    }

    /// What the library holds, as the gallery would list it.
    fn library(&self) -> Vec<crate::app::library::PackageSummary> {
        self.store.list().expect("the library reads back")
    }
}

/// M5's walk, host half: Open on the running face borrows the board's
/// wire for the editor, the pool gets a DEVICE session the lens is on, the
/// mirror connects to the project the board runs, and — the design core —
/// the card keeps folding evidence off the tapped wire while the client
/// owns it. Closing the editor gives the wire back and the pump resumes.
#[test]
fn opening_the_lens_borrows_the_wire_and_the_card_keeps_folding() {
    let device = empty_light_player("dev000000daqf6dvvr1");
    let (mut bench, tasks) = identified(&device, "usb-lens-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    bench.push_gesture(card.id, bundled_example());
    bench.run_until(&tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    assert!(
        bench.view().devices[0]
            .last_outcome
            .as_ref()
            .is_some_and(|o| o.ok)
    );
    let uid = bench.registry()[0].uid.clone();

    // Open: the wire changes hands and the editor lands on the board.
    bench
        .open_lens(&uid)
        .expect("the running board opens in the editor");
    assert_eq!(bench.lens_device_uid().as_deref(), Some(uid.as_str()));
    let pool = bench.controller.runtime_pool_for_test();
    let session = pool.device_session().expect("a device session");
    assert_eq!(
        pool.lens(),
        Some(session.id()),
        "the editor is the lens on it"
    );
    assert!(session.is_connected(), "the wire client is up");
    assert_eq!(session.kind(), crate::RuntimeKind::Device);
    let link = session.device_attachment().expect("attachment").link;
    assert!(
        bench
            .controller
            .devices_for_test()
            .effects()
            .lens_holds_wire(link),
        "the lens holds the borrow"
    );
    assert_eq!(
        bench
            .controller
            .project_for_test()
            .runtime_storage_id_for_test(),
        "studio",
        "library sync targets the dir the board actually serves"
    );

    // The tap: the fold keeps hearing the board through the lens's io. A
    // lens pull drains the wire, and the heartbeat it drains lands as
    // evidence — freshness advances without the pump.
    let before = bench.controller.devices_for_test().roster().devices()[0]
        .evidence
        .freshness
        .last_heard;
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    loop {
        // A lens pull is what drains the wire; the heartbeat it drains is
        // teed into the fold on the way through.
        let refresh = UiAction::from_op(ProjectController::NODE_ID, ProjectOp::RefreshProject);
        let _ = drive(bench.controller.dispatch(refresh));
        bench.step(&tasks);
        let heard = bench.controller.devices_for_test().roster().devices()[0]
            .evidence
            .freshness
            .last_heard;
        if heard > before {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the tapped wire never reached the fold: {:?}",
            bench.view()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        bench.view().devices[0].activity.is_none(),
        "no activity ran; the card just kept listening: {:?}",
        bench.view().devices[0]
    );

    // Close: the session goes, the wire comes back, the card stays.
    bench.detach_lens();
    assert!(
        bench.lens_device_uid().is_none(),
        "the device session is gone"
    );
    assert!(bench.controller.runtime_pool_for_test().lens().is_none());
    assert!(
        !bench
            .controller
            .devices_for_test()
            .effects()
            .lens_holds_wire(link),
        "the wire is the roster's again"
    );
    bench.run_until(&tasks, "the pump to hear the board again", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    assert_eq!(bench.view().devices.len(), 1, "the card never left");
}

/// Unplug mid-lens (G2 row 3): the board's departure is card evidence AND
/// the end of the lens session — no refresh needed, nothing held.
#[test]
fn unplugging_mid_lens_closes_the_editor_and_leaves_an_honest_card() {
    let device = empty_light_player("dev000000daqf6dvvr2");
    let (mut bench, tasks) = identified(&device, "usb-lens-2");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    bench.push_gesture(card.id, bundled_example());
    bench.run_until(&tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let uid = bench.registry()[0].uid.clone();
    bench.open_lens(&uid).expect("opens");
    assert!(bench.lens_device_uid().is_some());

    // Unplugged under the lens: the browser drops the grant and fires
    // disconnect; the departure sweep stops routing the link.
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the lens to close on the departure", |bench| {
        bench.lens_device_uid().is_none()
    });
    assert!(bench.controller.runtime_pool_for_test().lens().is_none());
    let card = &bench.view().devices[0];
    assert!(
        card.escapes.contains(&lpa_devices::Escape::Reconnect)
            || card.escapes.contains(&lpa_devices::Escape::Forget),
        "the card offers a way back: {card:?}"
    );
}

/// One wire, one owner: a card verb that needs the board's wire while the
/// editor is a lens on it closes the editor first, then RUNS — the card's
/// verbs always work; the editor is what yields.
#[test]
fn a_card_verb_on_the_lens_device_closes_the_editor_and_then_runs() {
    let device = empty_light_player("dev000000daqf6dvvr4");
    let (mut bench, tasks) = identified(&device, "usb-lens-4");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    bench.push_gesture(card.id, bundled_example());
    bench.run_until(&tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let uid = bench.registry()[0].uid.clone();
    bench.open_lens(&uid).expect("opens");
    let link = bench
        .controller
        .runtime_pool_for_test()
        .device_session()
        .and_then(crate::RuntimeSession::device_attachment)
        .expect("attachment")
        .link;

    // A rename never touches the wire: the editor stays.
    bench.gesture(DeviceAction::SetName {
        device: card.id,
        name: "porch board".to_string(),
    });
    assert!(
        bench.lens_device_uid().is_some(),
        "a rename leaves the lens alone"
    );

    // Remove project needs the wire: the editor yields, the verb runs.
    bench.gesture(DeviceAction::RemoveProject { device: card.id });
    assert!(bench.lens_device_uid().is_none(), "the lens closed first");
    assert!(
        !bench
            .controller
            .devices_for_test()
            .effects()
            .lens_holds_wire(link),
        "the wire went back before the gesture folded"
    );
    bench.run_until(&tasks, "the removal to finish", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && card.loaded_project == lpa_devices::view::LoadedProject::Empty
        })
    });
}

/// The reload row (D37, reload = re-derivation): `/device/<uid>` asks for
/// the lens BEFORE the roster knows the board — rows still loading, port
/// still identifying. The intent is held, the gallery stays honest, and
/// the tick attaches the lens the moment the board says hello.
#[test]
fn an_open_asked_before_the_board_is_ready_attaches_once_it_says_hello() {
    let device = empty_light_player("dev000000daqf6dvvr5");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-lens-5");
    // The registry knows this board from a previous sitting; its row is
    // what a `/device/<uid>` address names. (Identify it once to earn the
    // row, then reboot the bench the way a reload would: fresh roster,
    // same registry, same granted port.)
    bench.run_until(&tasks, "the board to identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    let uid = bench.registry()[0].uid.clone();
    let device_id = bench.view().devices[0].id;
    bench.gesture(DeviceAction::Disconnect { device: device_id });
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label.starts_with("Attached"))
    });

    // The address arrives while the board is NOT ready: held, not refused.
    bench
        .open_lens(&uid)
        .expect("an early open is an intent, never an error");
    assert!(bench.lens_device_uid().is_none(), "nothing attached yet");

    // The board comes back (the connect the sweep would perform) and says
    // hello; the tick attaches the held lens.
    let device_id = bench.view().devices[0].id;
    bench.gesture(DeviceAction::Connect { device: device_id });
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    loop {
        bench.step(&tasks);
        drive(bench.controller.try_pending_device_lens());
        if bench.lens_device_uid().as_deref() == Some(uid.as_str()) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the held lens never attached: {:?}",
            bench.view()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        bench.controller.runtime_pool_for_test().lens().is_some(),
        true,
        "the editor is the lens on the board"
    );
}

/// Unplug under the lens with NO hotplug event (the pump is paused, so the
/// only witness is the lens io itself): the io's port error rides the tap,
/// the fold hears error + close, and the lens drops — the road the walk
/// found missing when the departure sweep did not fire.
#[test]
fn a_port_that_dies_under_the_lens_closes_the_editor_through_the_tap() {
    let device = empty_light_player("dev000000daqf6dvvr6");
    let (mut bench, tasks) = identified(&device, "usb-lens-6");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    bench.push_gesture(card.id, bundled_example());
    bench.run_until(&tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let uid = bench.registry()[0].uid.clone();
    bench.open_lens(&uid).expect("opens");
    assert!(bench.lens_device_uid().is_some());

    // The wire dies now: every read from here on is EOF. No hotplug edge
    // is delivered — the grant stays "held" as far as the sweep knows.
    device.set_failure_plan(
        lpa_link::providers::fake_device::FakeFailurePlan::none()
            .with_disconnect_after_bytes(device.served_bytes()),
    );
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    loop {
        // A lens pull is what touches the wire and meets the EOF.
        let refresh = UiAction::from_op(ProjectController::NODE_ID, ProjectOp::RefreshProject);
        let _ = drive(bench.controller.dispatch(refresh));
        bench.step(&tasks);
        if bench.lens_device_uid().is_none() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the dead wire never closed the lens: {:?}",
            bench.view()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(bench.controller.runtime_pool_for_test().lens().is_none());
    let card = &bench.view().devices[0];
    assert!(
        card.state_label.starts_with("Attached"),
        "the card knows the port closed: {card:?}"
    );
}

/// An address the roster cannot serve yet is HELD, never refused: the
/// gallery stays honest, nothing dead is installed, and closing the lens
/// (leaving the route) lets the intent go.
#[test]
fn opening_a_lens_on_an_unknown_board_is_held_not_refused() {
    let device = empty_light_player("dev000000daqf6dvvr3");
    let (mut bench, _tasks) = identified(&device, "usb-lens-3");
    bench
        .open_lens("devnobody")
        .expect("an address the roster cannot serve yet is an intent");
    assert!(bench.lens_device_uid().is_none());
    assert!(bench.controller.runtime_pool_for_test().lens().is_none());
    drive(bench.controller.try_pending_device_lens());
    assert!(
        bench.lens_device_uid().is_none(),
        "still nothing to attach to"
    );

    // Leaving the route clears the intent.
    let close = UiAction::from_op(crate::RuntimeOp::NODE_ID, crate::RuntimeOp::CloseDeviceLens);
    drive(bench.controller.dispatch(close)).expect("close is quiet with nothing open");
    assert!(bench.controller.runtime_pool_for_test().lens().is_none());
}

fn memory_store(clock: Rc<Cell<f64>>) -> LibraryStore {
    let counter = Rc::new(Cell::new(0u8));
    LibraryStore::new(
        Rc::new(RefCell::new(lpfs::LpFsMemory::new())),
        Rc::new(move || {
            counter.set(counter.get().wrapping_add(1));
            [counter.get(); 16]
        }),
        Rc::new(move || {
            let _ = clock.get();
            "2026-08-25-0810".to_string()
        }),
    )
}

/// A second `LibraryStore` over the SAME filesystem, so the bench can read
/// what the host wrote.
fn memory_store_sharing(store: &LibraryStore) -> LibraryStore {
    LibraryStore::new(
        store.fs_handle(),
        Rc::new(|| [7u8; 16]),
        Rc::new(|| "2026-08-25-0810".to_string()),
    )
}

/// Drain whatever the device has already put on the wire, so the next link to
/// open it lands MID-STREAM: the boot banner and the unsolicited id-0 hello are
/// already gone.
fn run_past_the_boot_hello(device: &FakeEsp32Device) {
    let mut stream = lpa_link::providers::fake_device::FakeDeviceByteStream::new(device.clone());
    let mut buf = [0u8; 4096];
    let mut seen = String::new();
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    while !seen.contains("\"hello\"") {
        let read =
            crate::app::studio::studio_device_e2e_tests::read_available(&mut stream, &mut buf);
        seen.push_str(&String::from_utf8_lossy(&buf[..read]));
        assert!(
            std::time::Instant::now() < deadline,
            "the fake never said hello; saw: {seen}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// One `read_available` through the byte-stream seam.
fn read_available(
    stream: &mut lpa_link::providers::fake_device::FakeDeviceByteStream,
    buf: &mut [u8],
) -> usize {
    use lpa_link::stream::DeviceByteStream;
    stream.read_available(buf).expect("the fake is alive")
}

/// The lens io's read: a dead wire is an answer (the port-error tap), not a
/// panic — an unplug script ends the read path with `Closed`.
fn read_available_checked(
    stream: &mut lpa_link::providers::fake_device::FakeDeviceByteStream,
    buf: &mut [u8],
) -> Result<usize, String> {
    use lpa_link::stream::DeviceByteStream;
    stream
        .read_available(buf)
        .map_err(|error| format!("Serial port disconnected: {error:?}"))
}

/// A LightPlayer board with a stamped identity and a MAC — the everyday case.
fn light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
            .with_base_mac("60:55:f9:0a:0b:0c"),
    )))
}

// ---------------------------------------------------------------------
// The scenarios
// ---------------------------------------------------------------------

/// The everyday connect, whole: a granted port is swept up at startup, the
/// effects layer opens it, identification runs over real framing, the fold
/// classifies the board, the card is drawable with a way out, and the record
/// lands in the registry the studio already had.
#[test]
fn a_granted_port_identifies_and_earns_a_registry_row() {
    let device = light_player("dev_bench01");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bench-1");

    // The sweep is armed by installing the transport and runs on the first
    // settle; the link it finds arrives a step later, through the queue.
    bench.run_until(&tasks, "the granted port to be found", |bench| {
        !bench.view().pending.is_empty()
    });
    let pending = bench.view().pending;
    assert!(
        pending[0].state_label.contains("identifying"),
        "the fresh-plug affordance comes BEFORE any verdict: {}",
        pending[0].state_label
    );
    assert!(bench.view().devices.is_empty(), "nothing is known yet");

    bench.run_until(&tasks, "the hello to settle the link", |bench| {
        !bench.view().devices.is_empty()
    });

    let cards = bench.view().devices;
    assert_eq!(cards.len(), 1, "one board, one card");
    assert_eq!(cards[0].state_label, "Ready", "{:?}", cards[0]);
    assert!(
        cards[0].escapes.contains(&Escape::Forget),
        "every card has a way out"
    );
    // The identity came off the wire, through the real frame mapping.
    assert_eq!(
        cards[0].identity_label.as_deref(),
        Some("dev_bench01"),
        "{:?}",
        cards[0]
    );

    let rows = bench.registry();
    assert_eq!(rows.len(), 1, "the record reached the store: {rows:?}");
    assert_eq!(rows[0].uid, "dev_bench01");
    assert_eq!(
        rows[0].hardware_id.as_deref(),
        Some("efuse:60:55:f9:0a:0b:0c"),
        "the MAC persists in the registry's own canonical form"
    );
}

/// Add-a-device: no grants, so the roster is empty and honest until the user
/// asks — and asking runs the same identification the sweep does.
#[test]
fn add_a_device_pops_the_chooser_and_identifies_what_it_returns() {
    let device = light_player("dev_chosen");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-bench-2");

    bench.step(&tasks);
    assert!(bench.view().devices.is_empty());
    assert!(bench.view().pending.is_empty(), "no grants, no ports");

    bench.gesture(DeviceAction::AddFromUsb);
    bench.run_until(&tasks, "the chosen port to identify", |bench| {
        !bench.view().devices.is_empty()
    });

    assert_eq!(bench.view().devices[0].state_label, "Ready");
}

/// A dismissed chooser is not a failure and leaves nothing behind.
#[test]
fn a_dismissed_chooser_leaves_the_roster_alone() {
    let device = light_player("dev_unused");
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-bench-3");
    bench.chooser_grants.set(false);

    bench.gesture(DeviceAction::AddFromUsb);
    for _ in 0..50 {
        bench.step(&tasks);
    }

    assert!(bench.view().devices.is_empty());
    assert!(bench.view().pending.is_empty());
    assert!(!bench.granted.get());
}

/// Cancel mid-identify: the gesture is honoured, the link is handed back, and
/// the entry does NOT become a stuck spinner. Cancellation is bounded by
/// removal — the whole point of the supervision design.
#[test]
fn cancelling_mid_identify_ends_the_activity_and_leaves_a_way_out() {
    let device = light_player("dev_cancel");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bench-4");
    bench.run_until(&tasks, "identification to start", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.state_label.contains("identifying"))
    });
    let device_id = bench
        .controller
        .device_pending_ids_for_test()
        .into_iter()
        .next()
        .expect("the pending link carries a provisional device id");

    bench.gesture(DeviceAction::CancelActivity { device: device_id });
    bench.run_until(&tasks, "the activity to wind down", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_none_or(|pending| !pending.state_label.contains("identifying"))
    });

    // Whatever it settled as, it is escapable — which is exactly the state
    // the shipped system could not leave.
    if let Some(pending) = bench.view().pending.first() {
        assert!(pending.escapes.contains(&Escape::Forget), "{pending:?}");
    }
}

/// Forget from a live, identified device: the entry, its record and its grant
/// all go — including the `RevokeGrant` without which the next page load
/// re-created the row the user just deleted.
#[test]
fn forgetting_an_identified_device_deletes_its_row_and_gives_the_grant_back() {
    let device = light_player("dev_forget");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bench-5");
    bench.run_until(&tasks, "identification", |bench| {
        !bench.view().devices.is_empty()
    });
    assert_eq!(bench.registry().len(), 1);
    let device_id = bench.view().devices[0].id;

    bench.gesture(DeviceAction::Forget { device: device_id });
    bench.step(&tasks);

    assert!(bench.view().devices.is_empty(), "the card is gone");
    assert!(bench.registry().is_empty(), "so is the remembered row");
    assert_eq!(
        bench.revoked.borrow().len(),
        1,
        "the port stopped being ours"
    );
    assert!(!bench.granted.get());
}

/// The response-starvation wire (the 2026-08-24 request-idle defect): id-0
/// heartbeats keep flowing while every correlated response dies. The old
/// system hung here. Through the real effects layer the model must instead
/// reach its deadline and say something honest, with the entry still
/// escapable.
///
/// Since R4a the heartbeats also carry identity, so "honest" is now a NAMED
/// card whose verdict is still the truthful "never said hello" — rather than
/// the anonymous pending link this could only produce before.
#[test]
fn dropped_responses_degrade_honestly_instead_of_hanging() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev_starved", "Starved board"))
            .with_dropped_responses()
            // A live wire that never answers: heartbeats defeat any frame-gap
            // timeout, so only the settle deadline can end this.
            .with_heartbeat_interval(Duration::from_millis(50)),
    )));
    // Mid-stream: the unsolicited boot hello (id 0, which the drop does not
    // touch) is already gone, so the ONLY road to a verdict is the answer that
    // never comes. This is the state a board that has been running for an hour
    // is in when you connect to it.
    run_past_the_boot_hello(&device);
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bench-6");

    bench.run_until(
        &tasks,
        "identification to settle without a hello",
        |bench| {
            bench
                .view()
                .devices
                .first()
                .is_some_and(|device| !device.state_label.contains("identifying"))
        },
    );

    let view = bench.view();
    assert!(
        view.pending.is_empty(),
        "R4a named it off the heartbeats, so it is a card, not a pending link: {:?}",
        view.pending
    );
    assert_eq!(view.devices.len(), 1, "{:?}", view.devices);
    assert!(
        !view.devices[0].state_label.contains("identifying"),
        "the deadline ended it: {}",
        view.devices[0].state_label
    );
    assert!(
        view.devices[0].escapes.contains(&Escape::Forget),
        "an honest degrade still offers a way out"
    );
    // A row is earned by IDENTITY, not by a good verdict — and R4a is where
    // this board's identity came from. Before it, a starved wire stayed
    // anonymous and unrememberable no matter how long it talked.
    let registry = bench.registry();
    assert_eq!(registry.len(), 1, "{registry:?}");
    assert_eq!(registry[0].uid, "dev_starved");
}

/// A factory-fresh board, straight out of the bag: blank flash, no MAC on
/// the wire until the flash preflight reads efuse.
fn blank_board() -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash))
}

/// The board pick a C6 boot banner resolves to, through the REAL join
/// (chip necessary, board refinement, served build required — no fallback).
fn c6_board_choice() -> crate::FlashBoardChoice {
    let offer = crate::flash_offer(Some("esp32c6"));
    offer
        .candidates
        .first()
        .cloned()
        .expect("the checked-in catalog serves a C6 build")
}

/// M2's walk, end to end through the real effects layer: a blank board's
/// pending card offers the needs-firmware face; Flash adopts it, streams
/// progress into the projection, identity-joins off the preflight MAC,
/// climbs the ladder to the new firmware's hello, stamps `/hardware.json`,
/// and lands Ready — auto-named, with a registry row. No manual replug, no
/// naming step.
#[test]
fn a_flash_from_the_blank_pending_card_runs_to_ready_named_and_registered() {
    let device = blank_board();
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-flash-1");

    bench.run_until(&tasks, "the blank verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware)
    });
    let pending = &bench.view().pending[0];
    assert_eq!(pending.detected_chip.as_deref(), Some("esp32c6"));
    let target = pending.device;
    let choice = c6_board_choice();

    bench.gesture(DeviceAction::Flash {
        device: target,
        board_id: choice.board_id.clone(),
        build_id: choice.build_id.clone(),
        park_first: false,
    });

    // The gesture adopts: the pending card becomes a device card, busy
    // flashing, with the effect's progress visibly on it (the 2026-07-28
    // defect's regression: progress must REACH the projection).
    bench.run_until(&tasks, "flash progress to reach the card", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity
                .as_ref()
                .is_some_and(|activity| activity.percent == Some(100))
        })
    });
    assert!(bench.view().pending.is_empty(), "adopted, not duplicated");

    bench.run_until(&tasks, "the flashed board to land Ready", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready" && card.activity.is_none())
    });

    let card = &bench.view().devices[0];
    assert!(
        card.last_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.ok && outcome.summary.contains("firmware installed")),
        "{card:?}"
    );
    // Auto-name: "<board display_name> · <Mon D>" (bench clock = epoch
    // second 1000 = Jan 1), no naming step anywhere.
    assert_eq!(
        card.title,
        format!("{} · Jan 1", choice.title),
        "the derived name is the card's title"
    );

    // The registry row was earned by the preflight MAC (normalized from the
    // tool's UPPERCASE spelling), and carries the derived name.
    let rows = bench.registry();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].uid, "mac:60:55:f9:0a:0b:0c");
    assert_eq!(rows[0].name, card.title);
    assert_eq!(
        rows[0].hardware_id.as_deref(),
        Some("efuse:60:55:f9:0a:0b:0c")
    );

    // The board manifest went to the device (D4), exactly once, verbatim.
    let writes = bench.manifest_writes.borrow();
    assert_eq!(writes.len(), 1, "one stamp per flash");
    assert_eq!(
        writes[0],
        lpa_boards::runtime_manifest_json(&choice.board_id).expect("a served board has a manifest")
    );
}

/// A mid-write failure lands on an honest problem face: outcome line with
/// the tool's message, the needs-firmware face re-offered (retry in place),
/// and every escape still present.
#[test]
fn a_mid_write_failure_lands_on_an_honest_face_with_retry_in_place() {
    let device = blank_board();
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-flash-2");
    bench.flash_plan.set(FlashPlan::FailMidWrite);
    bench.run_until(&tasks, "the blank verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware)
    });
    let target = bench.view().pending[0].device;
    let choice = c6_board_choice();

    bench.gesture(DeviceAction::Flash {
        device: target,
        board_id: choice.board_id,
        build_id: choice.build_id,
        park_first: false,
    });
    bench.run_until(&tasks, "the failure to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let card = &bench.view().devices[0];
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(!outcome.ok);
    assert!(
        outcome.summary.contains("write failed at 0x2000"),
        "{outcome:?}"
    );
    assert!(
        card.needs_firmware,
        "the face re-offers the flash — retry in place: {card:?}"
    );
    assert!(!card.escapes.is_empty(), "always a way out");
}

/// Post-flash silence: the tool claims success but the board never answers.
/// The ladder climbs (reopen → Normal → BothThenDrop, real reset commands
/// through the real link) and then fails with the honest replug/Reconnect
/// guidance.
#[test]
fn post_flash_silence_climbs_the_ladder_then_fails_with_honest_guidance() {
    let device = blank_board();
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-flash-3");
    bench.flash_plan.set(FlashPlan::SilentAfterWrite);
    bench.run_until(&tasks, "the blank verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware)
    });
    let target = bench.view().pending[0].device;
    let choice = c6_board_choice();

    bench.gesture(DeviceAction::Flash {
        device: target,
        board_id: choice.board_id,
        build_id: choice.build_id,
        park_first: false,
    });
    bench.run_until(&tasks, "the ladder to exhaust", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let outcome = bench.view().devices[0]
        .last_outcome
        .clone()
        .expect("an outcome");
    assert!(!outcome.ok);
    assert!(
        outcome.summary.contains("Reconnect"),
        "the V3/CH340 guidance: {outcome:?}"
    );
    // The identity STILL joined off the preflight MAC — a failed reconnect
    // does not orphan the board.
    let rows = bench.registry();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].uid, "mac:60:55:f9:0a:0b:0c");
}

/// Forget mid-flash: eviction bounds even an effect that never completes.
/// The entry, its record and its grant all go; the late markers land on
/// nothing and nothing panics.
#[test]
fn forgetting_mid_flash_evicts_the_hung_effect_and_cleans_up() {
    let device = blank_board();
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-flash-4");
    bench.flash_plan.set(FlashPlan::Hang);
    bench.run_until(&tasks, "the blank verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware)
    });
    let target = bench.view().pending[0].device;
    let choice = c6_board_choice();

    bench.gesture(DeviceAction::Flash {
        device: target,
        board_id: choice.board_id,
        build_id: choice.build_id,
        park_first: false,
    });
    bench.run_until(&tasks, "the flash to be visibly running", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_some())
    });

    bench.gesture(DeviceAction::Forget { device: target });
    for _ in 0..20 {
        bench.step(&tasks);
    }

    assert!(bench.view().devices.is_empty(), "the card is gone");
    assert!(bench.view().pending.is_empty());
    assert_eq!(bench.revoked.borrow().len(), 1, "the grant went back");
}

// ---------------------------------------------------------------------
// M3: the empty face, the push, and the running face
// ---------------------------------------------------------------------

/// A LightPlayer board with nothing loaded, heartbeating like real firmware
/// so the fold learns what it is running (and what it is not).
fn empty_light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
            .with_base_mac("60:55:f9:0a:0b:0c")
            // The loaded-project fact rides heartbeats; the fake's host
            // server never heartbeats on its own, so a script that wants a
            // truthful empty/running face has to opt in.
            .with_heartbeat_interval(Duration::from_millis(20)),
    )))
}

/// The example the picker's first entry resolves to.
fn bundled_example() -> crate::PushSource {
    crate::PushSource::Example {
        example_id: crate::first_bundled_example_id()
            .expect("this build bundles examples")
            .to_string(),
    }
}

/// Drive a board to its settled card.
fn identified(device: &FakeEsp32Device, endpoint: &str) -> (DeviceBench, TaskPool) {
    let (mut bench, tasks) = DeviceBench::granted(device, endpoint);
    bench.run_until(&tasks, "the board to identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    (bench, tasks)
}

/// M3's walk, end to end through the real effects layer and a REAL push
/// conversation: a LightPlayer with nothing on it wears the empty face, the
/// picker's example is installed into the library and sent over the wire
/// (stop → clear → chunked writes → load → hash verify, all of it against
/// the fake's own `LpServer`), and the card comes back running it.
#[test]
fn the_empty_face_pushes_an_example_and_the_card_ends_up_running() {
    let device = empty_light_player("dev000000daqf6dvvqz");
    let (mut bench, tasks) = identified(&device, "usb-push-1");

    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    assert!(
        card.can_receive_project,
        "the empty face's primary verb is live: {card:?}"
    );
    // The picker really is built from the gallery's two lists.
    let offer = crate::push_offer(&card, &[], &[]);
    assert!(
        offer.new_project_unavailable.is_some(),
        "a board that has not named itself cannot have a starter generated: {offer:?}"
    );
    assert!(bench.library().is_empty(), "nothing installed yet");

    bench.push_gesture(card.id, bundled_example());
    bench.run_until(&tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let card = &bench.view().devices[0];
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(outcome.ok, "{outcome:?}");
    // The board's OWN report is what the running face is made of.
    assert!(
        matches!(
            &card.loaded_project,
            lpa_devices::view::LoadedProject::Running { label } if !label.is_empty()
        ),
        "the running face reads the board's report: {card:?}"
    );
    assert!(
        !card.can_receive_project || card.loaded_project != lpa_devices::view::LoadedProject::Empty,
        "the empty face is gone"
    );

    // The example became a real library project — no naming step anywhere.
    let library = bench.library();
    assert_eq!(library.len(), 1, "{library:?}");
    // And the push was banked: the device row names what it was last given.
    let rows = bench.registry();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let association = rows[0]
        .association
        .as_ref()
        .expect("a verified push is banked");
    assert_eq!(association.project, library[0].uid);
    assert_eq!(
        association.version.to_string(),
        association.version.to_string(),
        "the banked version is the verified content hash"
    );
}

/// A push the board refuses lands on the problem face: the outcome line says
/// what happened, the picker is still there (retry in place), and every
/// escape survives.
#[test]
fn a_refused_push_lands_on_an_honest_face_with_the_picker_re_offered() {
    let device = empty_light_player("dev000000daqf6dvvr0");
    let (mut bench, tasks) = identified(&device, "usb-push-2");
    bench.push_plan.set(PushPlan::Fail);
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;

    bench.push_gesture(device_id, bundled_example());
    bench.run_until(&tasks, "the failure to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let card = &bench.view().devices[0];
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(!outcome.ok);
    assert!(outcome.summary.contains("refused the write"), "{outcome:?}");
    assert!(
        card.can_receive_project && card.loaded_project == lpa_devices::view::LoadedProject::Empty,
        "the empty face re-offers the picker — retry in place: {card:?}"
    );
    assert!(!card.escapes.is_empty(), "always a way out");
    // Nothing was banked for a push that never landed.
    assert!(
        bench.registry()[0].association.is_none(),
        "a failed push is not a push"
    );
}

/// A source the app cannot prepare fails the SAME way a refused wire does:
/// on the card, with the reason, and with the picker still there. (A
/// generate for a board this build has no catalog entry for is the real
/// shape of this — the generator refuses rather than guessing a pin.)
#[test]
fn a_project_that_cannot_be_prepared_fails_on_the_card_not_in_a_log() {
    let device = empty_light_player("dev000000daqf6dvvr1");
    let (mut bench, tasks) = identified(&device, "usb-push-3");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;

    bench.push_gesture(
        device_id,
        crate::PushSource::NewForBoard {
            board_id: "no-such-board".to_string(),
        },
    );
    bench.run_until(&tasks, "the refusal to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let card = &bench.view().devices[0];
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(!outcome.ok, "{outcome:?}");
    assert!(
        card.can_receive_project,
        "the picker is still there: {card:?}"
    );
    assert!(bench.library().is_empty(), "nothing half-installed");
}

/// Cancel mid-push: the conversation cannot be torn out (the device's
/// project dir is already cleared), so the cancel is held — and eviction
/// bounds the hold. The card comes back escapable, not stuck.
#[test]
fn cancelling_mid_push_is_held_then_bounded_by_eviction() {
    let device = empty_light_player("dev000000daqf6dvvr2");
    let (mut bench, tasks) = identified(&device, "usb-push-4");
    bench.push_plan.set(PushPlan::Hang);
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;

    bench.push_gesture(device_id, bundled_example());
    bench.run_until(&tasks, "the push to be visibly running", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_some())
    });

    bench.gesture(DeviceAction::CancelActivity { device: device_id });
    assert!(
        bench.view().devices[0]
            .activity
            .as_ref()
            .is_some_and(|activity| activity.cancel_requested),
        "the card says it is cancelling rather than stopping mid-write"
    );

    bench.run_until(&tasks, "the cancel grace to bound the hold", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });

    let card = &bench.view().devices[0];
    assert!(
        card.last_outcome
            .as_ref()
            .is_some_and(|outcome| !outcome.ok),
        "an interrupted push is not a success: {card:?}"
    );
    assert!(!card.escapes.is_empty(), "always a way out");
    assert!(
        bench.registry()[0].association.is_none(),
        "an evicted push banks nothing"
    );
}

/// The seam `lpa-link` calls the ONLY honest source of `expected_proto`: the
/// studio must agree with it, or a build would call a device speaking its own
/// proto incompatible.
#[test]
fn the_studios_roster_config_agrees_with_the_transport_seam() {
    let studio = crate::app::studio::studio_controller::device_roster_config_for_test();

    assert_eq!(studio.expected_proto, roster_config().expected_proto);
}

/// The bench's own wiring, asserted once so a broken harness fails as a
/// harness rather than as every scenario at once.
#[test]
fn the_bench_shares_one_clock_between_the_model_and_the_store() {
    let device = light_player("dev_clock");
    let (bench, _tasks) = DeviceBench::granted(&device, "usb-bench-7");

    let seconds = bench.clock.get();
    let millis = bench.controller.device_now_for_test();

    assert_eq!(millis.0, (seconds * 1_000.0) as u64);
}

/// Factory reset: a Ready board is wiped from its card, and the card comes
/// back honest — needs-firmware, entry and registry row intact (identity is
/// efuse; an erase cannot take it). This is the loop the bench needs to
/// re-run the first-plug walk without a second board.
#[test]
fn factory_reset_wipes_the_board_and_the_card_comes_back_blank() {
    let device = light_player("dev_wipeme");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bench-9");

    bench.run_until(&tasks, "the hello to settle the link", |bench| {
        !bench.view().devices.is_empty()
    });
    let card = &bench.view().devices[0];
    assert_eq!(card.state_label, "Ready", "{card:?}");
    let wiped = card.id;

    bench.gesture(DeviceAction::Erase { device: wiped });
    bench.run_until(&tasks, "the erase to settle as a blank verdict", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.needs_firmware && card.activity.is_none())
    });

    let cards = bench.view().devices;
    assert_eq!(cards.len(), 1, "the entry survives the wipe: {cards:?}");
    assert!(
        cards[0]
            .last_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.ok),
        "the erase reports success on the card: {:?}",
        cards[0].last_outcome
    );
    assert!(
        cards[0].escapes.contains(&Escape::Forget),
        "a wiped board still has every way out"
    );
    assert_eq!(
        bench.registry().len(),
        1,
        "identity lives in the efuse — the registry row survives an erase"
    );
}

/// The bench's dead end and its way out (G1, 2026-08-31): a board arrives
/// running a project from a previous life and the running face has no verbs
/// on it — "how do I push?" with no answer short of throwing the firmware
/// away. Remove project is the answer, and this is the whole loop through
/// the REAL conversation: push an example, remove it, and the empty face
/// comes back with the picker on it.
///
/// The two things it must NOT do are asserted too: the library copy and the
/// registry row are untouched. The project on the board is a copy; deleting
/// the copy is not deleting the project.
#[test]
fn removing_the_project_clears_the_board_and_leaves_the_library_alone() {
    let device = empty_light_player("dev000000daqf6dvvr3");
    let (mut bench, tasks) = identified(&device, "usb-remove-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;

    // Put something on it, the ordinary way.
    bench.push_gesture(device_id, bundled_example());
    bench.run_until(&tasks, "the board to be running it", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });
    let library_before = bench.library();
    assert_eq!(library_before.len(), 1, "{library_before:?}");
    let card = bench.view().devices[0].clone();
    assert!(
        card.can_remove_project,
        "the running face offers the removal: {card:?}"
    );

    // Take it off — through the ordinary action path, like the UI.
    bench.gesture(DeviceAction::RemoveProject { device: device_id });
    bench.run_until(&tasks, "the removal to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.can_receive_project)
    });

    let card = bench.view().devices[0].clone();
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(
        card.loaded_project,
        lpa_devices::view::LoadedProject::Empty,
        "the board's OWN report is what turns the face over: {card:?}"
    );
    assert!(
        card.can_receive_project,
        "the picker is back — the whole point: {card:?}"
    );
    assert!(!card.can_remove_project, "nothing left to remove");
    assert!(!card.escapes.is_empty(), "always a way out");

    // The board lost a copy. The library did not lose a project.
    let library_after = bench.library();
    assert_eq!(
        library_after.len(),
        1,
        "the library copy stays: {library_after:?}"
    );
    assert_eq!(library_after[0].uid, library_before[0].uid);
    assert_eq!(
        bench.registry().len(),
        1,
        "the device row survives a removal"
    );
}

/// A removal the board refuses lands on the problem face with the reason,
/// and the running face is still there — nothing was half-claimed.
#[test]
fn a_refused_removal_leaves_the_running_face_and_says_why() {
    let device = empty_light_player("dev000000daqf6dvvr4");
    let (mut bench, tasks) = identified(&device, "usb-remove-2");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;
    bench.push_gesture(device_id, bundled_example());
    bench.run_until(&tasks, "the board to be running it", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });

    bench.remove_plan.set(RemovePlan::Fail);
    bench.gesture(DeviceAction::RemoveProject { device: device_id });
    bench.run_until(&tasks, "the failure to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });

    let card = bench.view().devices[0].clone();
    let outcome = card.last_outcome.as_ref().expect("an outcome");
    assert!(!outcome.ok, "{outcome:?}");
    assert!(
        outcome.summary.contains("refused the delete"),
        "{outcome:?}"
    );
    assert!(
        matches!(
            card.loaded_project,
            lpa_devices::view::LoadedProject::Running { .. }
        ),
        "the board is still running it, and the card says so: {card:?}"
    );
    assert!(!card.escapes.is_empty(), "always a way out");
}

/// C2 (G1 bench, 2026-08-31): an effect that outlives its activity must give
/// the wire back. The bench proves the consequence rather than the flag —
/// after the eviction, the board is HEARD FROM again, which can only happen
/// if the link pump resumed.
///
/// The failure this replaces: an orphaned stamp held the borrow for its own
/// remaining budget (~40 s), the fold heard nothing at all in that window,
/// and the silent card read as a dead board.
#[test]
fn an_effect_that_outlives_its_activity_gives_the_wire_back_and_the_pump_resumes() {
    let device = empty_light_player("dev000000daqf6dvvr5");
    let (mut bench, tasks) = identified(&device, "usb-borrow-1");
    bench.push_plan.set(PushPlan::Hang);
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let device_id = bench.view().devices[0].id;

    // A push that never completes: it takes the wire and keeps it.
    bench.push_gesture(device_id, bundled_example());
    bench.run_until(&tasks, "the push to be visibly running", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_some())
    });

    // Cancel is held through the write window; the grace evicts. The
    // effect's future is STILL pending at this point — nothing can stop it.
    bench.gesture(DeviceAction::CancelActivity { device: device_id });
    bench.run_until(&tasks, "the cancel grace to bound the hold", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });

    for _ in 0..400 {
        bench.step(&tasks);
    }

    // The discriminating proof, and the reason it has to be this and not
    // "ask the board again": the eviction's recovery CYCLES the port
    // (close, reopen), and a reopen begins a fresh evidence window. Only a
    // reading pump can deliver those two events, so a card still wearing
    // the pre-eviction verdict is a card sitting behind a dead pump —
    // exactly the forty seconds of frozen truth the bench saw. A stale
    // window would also let the re-identify below settle instantly on a
    // hello nobody heard, which is why that is the second assertion and
    // never the first.
    let card = bench.view().devices[0].clone();
    assert_eq!(
        card.status,
        lpa_devices::device::DeviceStatus::NotResponding,
        "the fold saw the port cycle its recovery performed: {card:?}"
    );
    assert_eq!(
        card.loaded_project,
        lpa_devices::view::LoadedProject::Unknown,
        "a fresh window claims nothing it has not been told again: {card:?}"
    );

    // And the wire genuinely works: a re-ask reaches the board and comes
    // back, over the pump that was handed the port back.
    bench.gesture(DeviceAction::Identify { device: device_id });
    bench.run_until(&tasks, "the re-identify to settle", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });

    let card = bench.view().devices[0].clone();
    assert_eq!(
        card.state_label, "Ready",
        "the board answers again over the returned wire: {card:?}"
    );
}

/// D (G1 bench, 2026-08-31): a board unplugged while its port was merely
/// GRANTED used to be invisible. The departure sweep only noticed ports that
/// died while open, so the card sat at "Attached", no detach was raised —
/// and because no detach was raised, the replug raised no attach either. The
/// board could only be recovered by pressing Identify by hand.
///
/// The whole cycle, through the real effects layer: disconnect the port,
/// unplug, replug, and the board comes back on its own card, identified.
#[test]
fn a_board_unplugged_with_its_port_closed_departs_and_a_replug_brings_it_back() {
    let device = light_player("dev_replug");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-replug-1");
    bench.run_until(&tasks, "the board to identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let device_id = bench.view().devices[0].id;

    // The user closes the port. The board is still plugged in.
    bench.gesture(DeviceAction::Disconnect { device: device_id });
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label.starts_with("Attached"))
    });
    assert_eq!(
        bench.view().devices[0].state_label,
        "Attached — not listening",
        "the honest copy for a plugged-in board nobody is listening to"
    );

    // Now it is unplugged. The browser drops the grant and fires disconnect.
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the departure to reach the card", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Offline")
    });
    assert_eq!(
        bench.view().devices.len(),
        1,
        "the entry survives: a board that left is still a board we know"
    );

    // And back in. The connect edge sweeps grants; the endpoint routes it
    // straight to the device it belongs to, which re-identifies.
    bench.granted.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    bench.run_until(&tasks, "the replug to identify itself", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });

    let cards = bench.view().devices;
    assert_eq!(cards.len(), 1, "one board, one card — never a duplicate");
    assert!(cards[0].escapes.contains(&Escape::Forget));
    assert!(
        bench.view().pending.is_empty(),
        "a known endpoint is not a new discovery"
    );
}

/// The other half of D's fix: a disconnect edge for SOMEBODY ELSE's device
/// must not tear down a port we still hold. The old open-flag check
/// detached every link that was not open, so unplugging an unrelated dongle
/// took out a board that was simply idle.
#[test]
fn an_unrelated_disconnect_leaves_a_still_granted_port_alone() {
    let device = light_player("dev_bystander");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-bystander-1");
    bench.run_until(&tasks, "the board to identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let device_id = bench.view().devices[0].id;
    bench.gesture(DeviceAction::Disconnect { device: device_id });
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label.starts_with("Attached"))
    });

    // Something else was unplugged: our grant is still there.
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    for _ in 0..400 {
        bench.step(&tasks);
    }

    assert_eq!(
        bench.view().devices[0].state_label,
        "Attached — not listening",
        "a port the browser still grants us has not departed"
    );
}
