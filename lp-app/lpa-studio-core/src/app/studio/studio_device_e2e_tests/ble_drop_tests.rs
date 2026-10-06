//! A Bluetooth link that drops under the editor, end to end: the board
//! restarts (a power cut, a requested reboot), the radio link goes away,
//! and the same board comes back on a NEW link a few seconds later. The
//! editor must hold behind "Reconnecting…" and resume on the new link —
//! the 2026-10-05 desk check found it closing after 45 s instead.
//!
//! The third drop is the quiet one: the board walks out of range, and the
//! radio link stays "connected" while nothing comes back over it. The
//! first pull to time out there must raise the same curtain, never a red
//! transport error (the 2026-10-06 desk walk of #880).

use super::*;

/// The board's Bluetooth device id (Web Bluetooth's opaque id).
const BLE_DEVICE_ID: &str = "QkxFLWRyb3A";

/// A Bluetooth board whose presence the test controls: while `present` is
/// false the platform reports no connected device, which is how the
/// departure sweep learns the link went away.
struct DroppableBleBoard {
    device: FakeEsp32Device,
    present: Rc<Cell<bool>>,
    /// What the radio carries right now (see [`Air`]).
    air: Rc<Cell<Air>>,
}

/// What the radio between Studio and the board carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Air {
    /// Every request is answered.
    Clear,
    /// The board is out of range: the GATT link has not dropped yet, but
    /// nothing comes back over it. The page's lp-link notices first (its
    /// stall time, 3.5 s on Bluetooth, is under a request's 5 s budget) and
    /// says so on the tap; then the request in flight times out.
    OutOfRange,
    /// The link is healthy (the board's link layer still answers) but the
    /// board's server never replies to this request: a real error.
    Unanswered,
}

impl crate::BleLinkSource for DroppableBleBoard {
    fn present(&self) -> Vec<GrantedLink> {
        if !self.present.get() {
            return Vec::new();
        }
        let info = crate::ble_link_info(BLE_DEVICE_ID, "LP-b48c");
        vec![GrantedLink {
            link: Box::new(fake_device_link(info.clone(), &self.device)),
            info,
        }]
    }

    fn restore(&self) {}

    fn request(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        Box::pin(core::future::ready(Ok(None)))
    }

    fn forget(&self, _device_id: &str) -> DeviceTransportFuture<Result<(), String>> {
        Box::pin(core::future::ready(Ok(())))
    }

    fn client_io(
        &self,
        _device_id: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let io = FakeDeviceIo::new(&self.device);
        let inner = match tap.clone() {
            Some(tap) => io.with_tap(tap),
            None => io,
        };
        Ok(Box::new(AirIo {
            inner,
            air: Rc::clone(&self.air),
            tap,
        }))
    }
}

/// The board's conversation, through whatever [`Air`] is between.
struct AirIo {
    inner: FakeDeviceIo,
    air: Rc<Cell<Air>>,
    tap: Option<LensLineTap>,
}

/// The Bluetooth io's own words for a request nobody answered
/// (`ble_client_io.rs`).
const BLE_TIMEOUT: &str = "the device did not respond over Bluetooth within 5.0s";

#[async_trait::async_trait(?Send)]
impl lpa_client::ClientIo for AirIo {
    async fn send(&mut self, msg: lpc_wire::ClientMessage) -> Result<(), lpc_wire::TransportError> {
        match self.air.get() {
            Air::Clear => self.inner.send(msg).await,
            // Queued on the page's link, written into the air.
            Air::OutOfRange | Air::Unanswered => Ok(()),
        }
    }

    async fn receive(&mut self) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
        match self.air.get() {
            Air::Clear => self.inner.receive().await,
            Air::OutOfRange => {
                if let Some(tap) = &self.tap {
                    tap(LensTapEvent::Note(
                        lpa_link::device_link::link_note::LINK_STALLED_NOTE.to_string(),
                    ));
                }
                Err(lpc_wire::TransportError::Other(BLE_TIMEOUT.to_string()))
            }
            Air::Unanswered => Err(lpc_wire::TransportError::Other(BLE_TIMEOUT.to_string())),
        }
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        self.inner.close().await
    }
}

/// The 2026-10-05 desk check, finding 2 of the walk (`drop-back`) and the
/// shape of finding 1: the editor is open on a board reached over
/// Bluetooth, the board restarts and its radio link drops, and the board
/// comes back on a new link. The editor holds, then resumes on the new link
/// with the same session — never the Devices page.
#[test]
fn a_bluetooth_board_that_restarts_under_the_editor_resumes_it() {
    let (mut bench, tasks, device, present) = lens_over_bluetooth("dev000000bledrop01");
    let session = bench.lens_session_id();
    let old_link = lens_link(&bench);

    restart_off_the_air(&mut bench, &tasks, &device, &present);
    back_on_the_air(&mut bench, &device, &present);
    wait_for_the_resume(&mut bench, &tasks);

    assert_eq!(bench.lens_session_id(), session, "the same session resumed");
    assert_ne!(lens_link(&bench), old_link, "on the new link");
    assert!(bench.controller.view().home.is_none(), "never the gallery");
}

/// The desk check's finding 1, Studio's half: a LOCKED board over
/// Bluetooth (an untrusted link, a password in its device store, Studio
/// remembering it) restarts under the editor. Its new link holds nothing —
/// a fresh server, a fresh session — so the editor resumes only once
/// Studio has logged in on the NEW link; the pull after the resume is
/// answered, which a link that never logged in is not.
#[test]
fn a_locked_bluetooth_board_that_restarts_under_the_editor_logs_in_again_and_resumes() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000bledrop02")
            // Running at boot, so the restart brings it back running too.
            .with_project_files(bundled_example_files())
            .with_loaded_project()
            .with_untrusted_link()
            .with_root_files(vec![locked_store_file()]),
    )));
    let (mut bench, tasks, present) = bench_over_bluetooth(&device, |bench| {
        let mut remembered =
            crate::app::access::remembered_passwords::RememberedPasswords::default();
        remembered.remember(BENCH_PASSWORD, 1.0);
        bench
            .controller
            .apply_access_command(crate::AccessCommand::MemoryLoaded {
                passwords_json: Some(remembered.to_json()),
                devices_json: None,
                browser_json: None,
                account_json: None,
            });
    });
    open_running_lens(&mut bench, &tasks, "Unlocked by bench password", false);
    let session = bench.lens_session_id();
    let old_link = lens_link(&bench);

    restart_off_the_air(&mut bench, &tasks, &device, &present);
    back_on_the_air(&mut bench, &device, &present);
    wait_for_the_resume(&mut bench, &tasks);

    assert_eq!(bench.lens_session_id(), session, "the same session resumed");
    assert_ne!(lens_link(&bench), old_link, "on the new link");
    let card = bench.view().devices[0].id;
    assert_eq!(
        bench
            .controller
            .device_roster_view()
            .access
            .get(&card)
            .and_then(|access| access.line.clone())
            .as_deref(),
        Some("Unlocked by bench password"),
        "logged in on the new link"
    );
    match bench.tick() {
        Some(crate::ProjectRefreshOutcome::Synced(sync)) => {
            assert!(
                sync.synced,
                "the restarted board answers the editor's pull: {:?}",
                sync.logs
                    .iter()
                    .map(|log| log.message.clone())
                    .collect::<Vec<_>>()
            );
        }
        Some(_) => panic!("the resumed lens should pull and be answered"),
        None => panic!("the resumed lens had nothing to pull"),
    }
}

/// The silicon re-check's walk-2 (2026-10-06): the FIRST unlock of a
/// locked board is a typed password (remembered), because nothing this
/// browser held was on the board — its automatic try came up empty and was
/// spent. The board then restarts under the editor. The password Studio
/// just remembered must unlock the new link by itself: no sheet, and no
/// link the board drops at its 10 s deadline.
#[test]
fn a_password_typed_once_unlocks_the_restarted_board_by_itself() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000bledrop03")
            .with_project_files(bundled_example_files())
            .with_loaded_project()
            .with_untrusted_link()
            .with_root_files(vec![locked_store_file()]),
    )));
    let (mut bench, tasks, present) = bench_over_bluetooth(&device, |_| {});
    bench.run_until(&tasks, "the unlock sheet", |bench| {
        bench.controller.view().login_prompt.is_some()
    });
    let card = bench.view().devices[0].id;
    bench
        .controller
        .apply_access_command(crate::AccessCommand::SubmitPassword {
            device: card,
            password: BENCH_PASSWORD.to_string(),
            remember: true,
        });
    open_running_lens(&mut bench, &tasks, "Unlocked by bench password", false);
    let session = bench.lens_session_id();

    restart_off_the_air(&mut bench, &tasks, &device, &present);
    back_on_the_air(&mut bench, &device, &present);
    wait_for_the_resume(&mut bench, &tasks);

    assert_eq!(bench.lens_session_id(), session, "the same session resumed");
    assert!(
        bench.controller.view().login_prompt.is_none(),
        "no sheet: the remembered password unlocked the new link"
    );
    match bench.tick() {
        Some(crate::ProjectRefreshOutcome::Synced(sync)) => {
            assert!(sync.synced, "the restarted board answers the editor's pull");
        }
        Some(_) => panic!("the resumed lens should pull and be answered"),
        None => panic!("the resumed lens had nothing to pull"),
    }
}

/// The 2026-10-06 desk walk of #880: walked out of range under Play, the
/// board's radio link went quiet without dropping, and the first pull to
/// time out flashed a red "Transport error … did not respond within 5.0s"
/// before the "Reconnecting…" curtain took over. Through the actor, in the
/// order it happens: a pull is in flight, the link goes quiet (its stall
/// note reaches the lens's tap), the pull times out — and the one view the
/// batch emits shows the curtain, with no error on the project.
#[test]
fn a_bluetooth_board_out_of_range_shows_reconnecting_not_a_transport_error() {
    let (mut bench, air, uid) = board_over_bluetooth_air("dev000000blequiet1");
    let (mut actor, mut handle) = actor_with_lens_on(&mut bench, &uid);

    air.set(Air::OutOfRange);
    let view = pull_through_the_actor(&mut actor, &mut handle, &bench.clock);

    let curtain = view
        .lens_reconnecting
        .clone()
        .expect("the first timeout on a quiet link raises the curtain");
    assert_eq!(curtain.headline, "Reconnecting…");
    assert!(
        curtain.detail.contains("stopped responding"),
        "{}",
        curtain.detail
    );
    assert_eq!(sync_issue(&view), None, "no transport error on the project");

    // The next pull on the still-quiet link is no error either.
    let view = pull_through_the_actor(&mut actor, &mut handle, &bench.clock);
    assert!(view.lens_reconnecting.is_some());
    assert_eq!(sync_issue(&view), None);
}

/// The guard on the fix above: a request the board's server never answers,
/// on a link that is still answering underneath, is a real error, and it
/// still shows — no curtain papers over it.
#[test]
fn a_request_unanswered_on_a_healthy_bluetooth_link_still_shows_its_error() {
    let (mut bench, air, uid) = board_over_bluetooth_air("dev000000blequiet2");
    let (mut actor, mut handle) = actor_with_lens_on(&mut bench, &uid);

    air.set(Air::Unanswered);
    let view = pull_through_the_actor(&mut actor, &mut handle, &bench.clock);

    assert!(
        view.lens_reconnecting.is_none(),
        "a healthy link: no curtain"
    );
    let issue = sync_issue(&view).expect("the error shows");
    assert!(issue.contains(BLE_TIMEOUT), "{issue}");
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// A board running the bundled example over Bluetooth, ready to open, and
/// the [`Air`] between them in the test's hand. Returns the board's uid.
fn board_over_bluetooth_air(uid: &str) -> (DeviceBench, Rc<Cell<Air>>, String) {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board(uid),
    )));
    let air = Rc::new(Cell::new(Air::Clear));
    let (mut bench, tasks, _present) = bench_over_bluetooth_in(&device, |_| {}, Rc::clone(&air));
    let uid = board_ready_to_open(&mut bench, &tasks, "Unlocked", true);
    (bench, air, uid)
}

/// Hand the bench's controller to a real [`crate::StudioActor`], the way
/// the web shell runs it, and open the editor on `uid` through it — so the
/// lens's tap feeds the actor's command queue, landing behind whatever
/// batch is running, exactly as in the page.
fn actor_with_lens_on(
    bench: &mut DeviceBench,
    uid: &str,
) -> (
    crate::StudioActor<impl FnMut(Duration) -> Sleep + Clone + 'static>,
    crate::StudioHandle,
) {
    let clock = Rc::clone(&bench.clock);
    let controller = std::mem::replace(&mut bench.controller, StudioController::new(|| 0.0));
    let (mut actor, handle) = crate::StudioActor::new_with_options(
        controller,
        move |delay: Duration| Sleep {
            clock: Rc::clone(&clock),
            due: clock.get() + delay.as_secs_f64(),
        },
        crate::StudioActorOptions { drain_logs: false },
    );
    handle
        .tx
        .send(crate::StudioCommand::Action(UiAction::from_op(
            crate::RuntimeOp::NODE_ID,
            crate::RuntimeOp::OpenDeviceLens {
                uid: uid.to_string(),
            },
        )));
    drive(actor.run_one_batch_for_test());
    assert!(
        actor
            .controller_mut_for_test()
            .runtime_pool_for_test()
            .lens_session()
            .is_some(),
        "the editor opens over Bluetooth"
    );
    (actor, handle)
}

/// Let the lens's next pull come due, send one tick, run the actor's batch
/// and return the view it emitted.
fn pull_through_the_actor<M, T>(
    actor: &mut crate::StudioActor<M>,
    handle: &mut crate::StudioHandle,
    clock: &Rc<Cell<f64>>,
) -> crate::UiStudioView
where
    M: FnMut(Duration) -> T + Clone + 'static,
    T: Future<Output = ()> + 'static,
{
    let gap = actor
        .controller_mut_for_test()
        .lens_refresh_gap_for_test()
        .expect("the editor is open on the board");
    clock.set(clock.get() + gap.as_secs_f64() + 1.0);
    handle.tx.send(crate::StudioCommand::RefreshTick);
    drive(actor.run_one_batch_for_test());
    handle.view.try_recv().expect("the batch emits a view")
}

/// The project's sync issue, as the page shows it.
fn sync_issue(view: &crate::UiStudioView) -> Option<String> {
    view.panes.iter().find_map(|pane| match &pane.body {
        crate::UiViewContent::ProjectEditor(editor) => editor
            .sync
            .issue
            .as_ref()
            .map(|issue| issue.message.clone()),
        _ => None,
    })
}

/// A password the bench's locked board holds and Studio remembers.
const BENCH_PASSWORD: &str = "bench-password-1";

/// A device store holding one edit password, [`BENCH_PASSWORD`], with
/// nobody let in without it: a locked board.
fn locked_store_file() -> (String, Vec<u8>) {
    let store = lpc_access::DeviceAccessFile {
        version: lpc_access::DeviceAccessFile::VERSION,
        secrets: vec![lpc_access::SecretEntry::from_password(
            "bench password",
            lpc_access::Tier::Edit,
            BENCH_PASSWORD.as_bytes(),
            [7; lpc_access::SALT_BYTES],
            16,
        )],
        ble_enabled: true,
        open: lpc_access::OpenTo::Nobody,
    };
    (
        lpc_access::DeviceAccessFile::PATH.to_string(),
        store.to_json().expect("the store serializes").into_bytes(),
    )
}

/// The bundled example's files, as a board holds them.
fn bundled_example_files() -> Vec<(String, Vec<u8>)> {
    crate::app::home::embedded_example::embedded_example(
        crate::first_bundled_example_id().expect("this build bundles examples"),
    )
    .expect("the bundled example resolves")
    .files()
}

/// A LightPlayer script for the board reached over Bluetooth.
fn bluetooth_board(uid: &str) -> FakeLightPlayerState {
    FakeLightPlayerState::new()
        .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
        .with_base_mac("a0:f2:62:87:b4:8c")
        .with_heartbeat_interval(Duration::from_millis(20))
}

/// A board running the bundled example, reached over Bluetooth only, with
/// the editor open on it.
fn lens_over_bluetooth(uid: &str) -> (DeviceBench, TaskPool, FakeEsp32Device, Rc<Cell<bool>>) {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board(uid),
    )));
    let (mut bench, tasks, present) = bench_over_bluetooth(&device, |_| {});
    open_running_lens(&mut bench, &tasks, "Unlocked", true);
    (bench, tasks, device, present)
}

/// A bench reaching `device` over Bluetooth only. `memory` runs before the
/// Bluetooth half is installed: what the browser remembers.
fn bench_over_bluetooth(
    device: &FakeEsp32Device,
    memory: impl FnOnce(&mut DeviceBench),
) -> (DeviceBench, TaskPool, Rc<Cell<bool>>) {
    bench_over_bluetooth_in(device, memory, Rc::new(Cell::new(Air::Clear)))
}

/// [`bench_over_bluetooth`], with the test holding the [`Air`].
fn bench_over_bluetooth_in(
    device: &FakeEsp32Device,
    memory: impl FnOnce(&mut DeviceBench),
    air: Rc<Cell<Air>>,
) -> (DeviceBench, TaskPool, Rc<Cell<bool>>) {
    // No USB grant: the serial half is installed, and never sees the board.
    let (mut bench, tasks) = DeviceBench::build(device, "usb-unused", false, false);
    memory(&mut bench);
    let present = Rc::new(Cell::new(true));
    // The shipped build's shape: the Bluetooth half joins the composite
    // beside a sim half (no sim is ever created here).
    bench
        .controller
        .set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
            ScriptedSimSource {
                device: sim_light_player(),
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            },
        ))));
    bench
        .controller
        .set_ble_transport(Rc::new(crate::BleDeviceTransport::new(Rc::new(
            DroppableBleBoard {
                device: device.clone(),
                present: Rc::clone(&present),
                air,
            },
        ))));
    (bench, tasks, present)
}

/// Identify the board, wait for its link to unlock (`access_line`), push
/// the bundled example over Bluetooth when `push` (else the board already
/// runs it) and open it in the editor.
fn open_running_lens(bench: &mut DeviceBench, tasks: &TaskPool, access_line: &str, push: bool) {
    let uid = board_ready_to_open(bench, tasks, access_line, push);
    bench.open_lens(&uid).expect("opens over Bluetooth");
}

/// [`open_running_lens`] up to the open: returns the board's registry uid.
fn board_ready_to_open(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    access_line: &str,
    push: bool,
) -> String {
    bench.run_until(tasks, "the board to identify over Bluetooth", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    let card = bench.view().devices[0].clone();
    wait_for_access_line(bench, tasks, card.id, access_line);
    if push {
        bench.wait_for_verb(tasks, card.id, "push");
        bench.push_gesture(card.id, bundled_example());
        bench.run_until(tasks, "the push to finish", |bench| {
            bench
                .view()
                .devices
                .first()
                .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
        });
    }
    bench.registry()[0].uid.clone()
}

/// The board restarts and its radio link goes with it: the old link's
/// stream ends, the platform reports nothing connected, and the presence
/// edge says so.
fn restart_off_the_air(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    device: &FakeEsp32Device,
    present: &Rc<Cell<bool>>,
) {
    device.set_failure_plan(
        lpa_link::providers::fake_device::FakeFailurePlan::none()
            .with_disconnect_after_bytes(device.served_bytes()),
    );
    present.set(false);
    device.reset_runtime();
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(tasks, "the lens to be held on the drop", |bench| {
        bench.controller.lens_is_held()
    });
    assert!(bench.controller.view().home.is_none(), "never the gallery");
}

/// The board is back on the air: Web Bluetooth's reconnect lands a new
/// link.
fn back_on_the_air(bench: &mut DeviceBench, device: &FakeEsp32Device, present: &Rc<Cell<bool>>) {
    device.set_failure_plan(lpa_link::providers::fake_device::FakeFailurePlan::none());
    present.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
}

/// Tick (the actor's held-lens look) until the editor is back on a wire,
/// failing if the hold's grace runs out first.
fn wait_for_the_resume(bench: &mut DeviceBench, tasks: &TaskPool) {
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    while bench.controller.lens_is_held() {
        let _ = bench.tick();
        bench.step(tasks);
        assert!(
            bench.lens_device_uid().is_some(),
            "the editor closed instead of resuming; card: {:?}, access: {:?}",
            bench.view().devices.first(),
            bench.controller.device_roster_view().access,
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the held lens never resumed; card: {:?}, access: {:?}",
            bench.view().devices.first(),
            bench.controller.device_roster_view().access,
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(bench.controller.view().lens_reconnecting.is_none());
}

/// Step until the device's access line reads `line`.
fn wait_for_access_line(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    device: crate::DeviceId,
    line: &str,
) {
    bench.run_until(
        tasks,
        &format!("the access line to read {line:?}"),
        |bench| {
            bench
                .controller
                .device_roster_view()
                .access
                .get(&device)
                .and_then(|access| access.line.as_deref())
                == Some(line)
        },
    );
}
