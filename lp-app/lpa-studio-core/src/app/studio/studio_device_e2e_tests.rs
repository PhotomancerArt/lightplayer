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
    DeviceAction, DeviceInput, DeviceTaskFuture, DeviceTransport, DeviceTransportFuture, DevicesOp,
    GrantedLink, StudioController, UiAction,
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
}

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
        // Short budgets keep the bench quick; the SHAPE under test is the
        // deadline, never its size.
        controller.set_device_transport(Rc::new(ScriptedTransport {
            device: device.clone(),
            endpoint: endpoint.to_string(),
            granted: Rc::clone(&granted),
            chooser_grants: Rc::clone(&chooser_grants),
            revoked: Rc::clone(&revoked),
        }));

        let bench = Self {
            controller,
            clock,
            inbox,
            store,
            revoked,
            granted,
            chooser_grants,
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

    fn registry(&self) -> Vec<crate::app::places::RegisteredDevice> {
        DeviceRegistry::new(self.store.fs_handle())
            .list()
            .expect("the registry reads back")
    }
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
