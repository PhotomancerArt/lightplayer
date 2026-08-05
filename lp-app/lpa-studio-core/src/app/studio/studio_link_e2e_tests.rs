//! End-to-end StudioController tests through the REAL link path.
//!
//! Unlike `studio_edit_e2e_tests` (which bypasses the link via a stubbed
//! device attachment + an in-process `ClientIo`), these tests
//! go `open_provider → discover → connect_endpoint → DeviceSession →
//! readiness → attach → pull` through the real async seams, against the
//! scripted byte-level `FakeEsp32Device`: a REAL host `LpServer` behind the
//! REAL `M!` serial framing, reached through the fake provider in the
//! registry.
//!
//! This is the seam where both M5 hardware bugs lived
//! (pull-before-readiness ordering; fresh device classified unreadable), so
//! rows 2 and 3 of the matrix are wire-level regressions for them. Rows
//! 6–10 cover the M4 DeviceSession states end to end: Incompatible (hello
//! suppressed / proto mismatch) with the reflash affordance, Unresponsive
//! with reconnect recovery, reconnect-after-Gone, and erase landing
//! BlankFlash as success through the card's Danger tab.

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use lpa_link::providers::LinkProviderRegistry;
use lpa_link::providers::fake::FakeProvider;
use lpa_link::providers::fake_device::{
    FakeBootState, FakeDeviceIdentity, FakeDeviceScript, FakeEsp32Device, FakeFailurePlan,
    FakeLightPlayerState,
};
use lpa_link::{
    DeviceDeadlines, DeviceState, IncompatibleReason, LinkEndpointId, LinkProviderKind,
};
use lpfs::LpFsMemory;

use crate::app::device::{DEPLOY_NODE_ID, DeployOp};
use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
use crate::app::places::DeviceContent;
use crate::{
    ControllerId, DeviceController, DeviceOp, ServerFailureKind, ServerState, StudioController,
    UiAction, UiError, UiNotices, UxUpdate, UxUpdateSink,
};

/// Row 1 (happy path, part 1): a LightPlayer device holding a stamped
/// identity and a project the library knows at head → connect through the
/// real link → readiness settles → the connect-time pull classifies AtHead.
#[test]
fn known_device_connects_and_classifies_at_head_through_the_link() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let library_files = store.open(summary.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_boot_delay(Duration::from_millis(20))
            .with_project_files(library_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    assert!(
        matches!(
            studio.snapshot().server.state,
            ServerState::Connected { .. }
        ),
        "protocol attached"
    );
    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    assert_eq!(
        sync.identity
            .as_ref()
            .map(|identity| identity.name.as_str()),
        Some("Bench board")
    );
    let DeviceContent::Known { relation, slug, .. } = &sync.content else {
        panic!("library-known project classifies, got {:?}", sync.content);
    };
    assert_eq!(*relation, lpc_history::SyncRelation::AtHead);
    assert_eq!(slug, &summary.slug);
    assert_eq!(
        device.premature_input_bytes(),
        0,
        "nothing was written to the device before readiness"
    );
}

/// Row 1b (roster model regression): a device that boots with its project
/// LOADED — the real-hardware shape since standalone startup-resume — must
/// attach as pure observation: the gallery keeps the view (no open
/// project, no editor entry), while connect-as-pull classifies the running
/// copy for the device card. Editor entry is the explicit D29 click (M5).
#[test]
fn attaching_a_device_with_a_loaded_project_never_opens_the_editor() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let library_files = store.open(summary.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(library_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let snapshot = studio.snapshot();
    assert!(
        matches!(snapshot.project.state, crate::ProjectState::NotLoaded),
        "hardware attach observes only — the editor must not open, got {:?}",
        snapshot.project.state
    );
    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    let DeviceContent::Known { relation, .. } = &sync.content else {
        panic!(
            "running copy classifies for the card, got {:?}",
            sync.content
        );
    };
    assert_eq!(*relation, lpc_history::SyncRelation::AtHead);
}

/// Row 1c (storage-discovery regression): a device provisioned OUTSIDE
/// Studio — its project lives in `/projects/bench`, not the sim's default
/// slot — and running it. The connect-time pull must discover the loaded
/// project's storage dir and classify the copy (a fixed-"studio" pull
/// reported this device as Empty).
#[test]
fn device_running_from_a_non_default_storage_dir_classifies_not_empty() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let library_files = store.open(summary.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(library_files)
            .with_project_dir("bench")
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    let DeviceContent::Known { relation, slug, .. } = &sync.content else {
        panic!(
            "the running copy must classify from its real dir, got {:?}",
            sync.content
        );
    };
    assert_eq!(*relation, lpc_history::SyncRelation::AtHead);
    assert_eq!(slug, &summary.slug);
}

/// Save-as-pull regression (2026-07-26 walk): attaching the editor to a
/// device running from a NON-default storage dir must point library sync
/// at that dir. A fixed-"studio" pull returned empty and silently
/// skipped the library half of every save — the device kept the edits,
/// the library never saw them, and the next connect classified Diverged
/// ("my edits didn't save locally").
#[test]
fn lens_attach_targets_the_devices_real_storage_dir() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let library_files = store.open(summary.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(library_files)
            .with_project_dir("bench")
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        crate::ProjectOp::OpenDeviceProject { uid: None },
    )))
    .expect("the D29 open attaches the device lens");

    assert_eq!(
        studio.project_runtime_storage_id_for_test(),
        "bench",
        "library sync must target the dir the device actually serves"
    );
}

/// Row 1 (happy path, part 2): the card-native stamp→push on an empty
/// device (M8′ — the dialog's wizard, re-homed onto the card): the name
/// sheet's op stamps over the real serial framing, the Project-tab
/// picker's push replaces the device copy, and connect-as-pull
/// re-classifies to at-head.
#[test]
fn card_native_stamp_and_push_through_the_link() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    let sync = studio.device_sync_for_test().expect("pull landed");
    assert_eq!(sync.content, DeviceContent::Empty, "fresh device is empty");
    // the gallery narrates the unstamped board as Needs-a-name
    let home = studio.view().home.expect("no project open — gallery shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsAName),
        "unstamped empty device asks for a name on its card: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );

    // Stamp (the name sheet's op): writes `/.lp/device.json` at the REAL
    // server's fs root over the wire.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::NameDevice {
            target: studio.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .unwrap();
    let sync = studio
        .device_sync_for_test()
        .expect("re-pulled after stamp");
    assert_eq!(
        sync.identity
            .as_ref()
            .map(|identity| identity.name.as_str()),
        Some("Luna's porch sign")
    );

    // Push (the Project-tab picker's op): hash-verified replace-and-load
    // + re-pull (no re-stamp — the root identity is outside the replaced
    // storage dir).
    drive(studio.dispatch(deploy_action(DeployOp::PushProject {
        target: studio.device_target_for_test(),
        key: summary.uid.to_string(),
    })))
    .unwrap();
    let sync = studio.device_sync_for_test().expect("re-pulled after push");
    assert_eq!(
        sync.identity
            .as_ref()
            .map(|identity| identity.name.as_str()),
        Some("Luna's porch sign"),
        "the root-stamped identity survives the push"
    );
    assert!(
        matches!(
            &sync.content,
            DeviceContent::Known {
                relation: lpc_history::SyncRelation::AtHead,
                ..
            }
        ),
        "device is at head after the push, got {:?}",
        sync.content
    );
}

/// Row 2 (pull-before-readiness regression): with a boot delay long enough
/// that a premature pull would race the server start, the pull must only
/// happen after the server-started marker + first `M!` frame. The fake
/// DISCARDS (and counts) bytes written before its server loop runs — real
/// ESP32 behavior, and the exact M5 hardware bug: a pull sent early was
/// silently lost and the connect hung.
#[test]
fn pull_waits_for_server_started_marker_and_first_frame() {
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_boot_delay(Duration::from_millis(400)),
    ));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    assert_eq!(
        device.premature_input_bytes(),
        0,
        "no request bytes reached the wire before the server-started marker \
         and the first M! frame"
    );
    assert_eq!(
        studio.device_sync_for_test().map(|sync| &sync.content),
        Some(&DeviceContent::Empty),
        "the pull still ran — after readiness"
    );
}

/// Row 3 (fresh device): an empty LpFsMemory behind the real wire pulls as
/// `DeviceContent::Empty`, NOT `Unreadable` — the second M5 hardware bug
/// (a never-pushed storage dir misclassified as an unreadable device).
#[test]
fn fresh_device_pulls_as_empty_not_unreadable() {
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    assert_eq!(sync.identity, None);
    assert_eq!(
        sync.content,
        DeviceContent::Empty,
        "a fresh device is EMPTY, not unreadable"
    );
}

/// A flash must NARRATE while it runs, not only when it lands
/// (2026-07-28-flash-progress-never-reached-the-ui): the whole point of
/// the card-owned op flow is that the user watches a minute-long
/// operation happen. Two things have to leave the controller mid-op —
/// a view carrying the op (so the overlay mounts at all) and the
/// progress deltas (so its bar moves) — because the op holds
/// `&mut controller` throughout and no other snapshot can escape.
#[test]
fn a_flash_narrates_its_progress_while_it_runs() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::BlankFlash);
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("no-firmware connect resolves");

    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = UxUpdateSink::new({
        let seen = Rc::clone(&seen);
        move |update| seen.borrow_mut().push(update)
    });
    drive(studio.dispatch_with_updates(
        device_action(DeviceOp::ProvisionFirmware {
            target: studio.device_target_for_test(),
            setup_name: None,
            board_id: None,
        }),
        sink,
    ))
    .unwrap();

    let seen = seen.borrow();
    let carries_op = |update: &UxUpdate| {
        matches!(
            update,
            UxUpdate::View(view)
                if view
                    .home
                    .as_ref()
                    .is_some_and(|home| home.devices.iter().any(|card| card.ui.op.is_some()))
        )
    };
    let mounted_at = seen.iter().position(carries_op);
    let first_progress = seen
        .iter()
        .position(|update| matches!(update, UxUpdate::CardOp { .. }));
    // The overlay must mount BEFORE the work starts reporting. A view
    // carrying the op does eventually escape — during the post-flash
    // reattach — which is exactly the bug: the user watched a minute of
    // nothing and then saw the result. The dispatch-time seed cannot
    // serve here either; it is built before the op slot is installed.
    assert!(
        mounted_at.is_some(),
        "a flash must publish a view whose card wears the op, or the \
         overlay never mounts"
    );
    assert!(
        mounted_at < first_progress,
        "the op overlay must mount before the first progress tick \
         (mounted at {mounted_at:?}, first progress at {first_progress:?})"
    );
    // The fake provider scripts 50% then 100%; both must reach the card.
    let percents: Vec<_> = seen
        .iter()
        .filter_map(|update| match update {
            UxUpdate::CardOp { op, .. } => Some(op.percent),
            _ => None,
        })
        .collect();
    assert!(
        percents.contains(&Some(50)) && percents.contains(&Some(100)),
        "progress must reach the card as it ticks, got {percents:?}"
    );
    // …and the work's own output streams too (the overlay's log tail).
    assert!(
        seen.iter().any(|update| matches!(update, UxUpdate::Log(_))),
        "the flash's log lines are mirrored while it runs"
    );
}

/// Row 4 (blank flash): boot output classifies as no-firmware
/// (BlankOrErasedFlash) → the CARD derives Ready-to-set-up (M8′ — the
/// dialog is gone); the card's Set-up (`ProvisionFirmware`) flashes
/// through the real `manage()` path, and the flashed, unstamped device
/// lands on the Needs-a-name card (the name sheet is next).
#[test]
fn blank_flash_classifies_flashes_and_reaches_needs_a_name() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::BlankFlash);
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    // Readiness classifies the ROM's invalid-header boot output as
    // no-firmware; the connect completes Ok (flash must stay reachable).
    connect_through_link(&mut studio, &endpoint_id)
        .expect("no-firmware connect resolves without error");
    assert!(
        matches!(
            &studio.snapshot().server.state,
            ServerState::Failed {
                kind: ServerFailureKind::NoFirmware,
                ..
            }
        ),
        "blank flash classifies as no-firmware, got {:?}",
        studio.snapshot().server.state
    );

    let home = studio.view().home.expect("no project open — gallery shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::ReadyToSetUp),
        "blank flash derives the Ready-to-set-up card: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );

    // Scripted flash via the real manage() path (the card's Set-up
    // affordance): the device reboots as LightPlayer, the controller
    // reconnects, and the empty unstamped device lands on Needs-a-name.
    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: studio.device_target_for_test(),
        setup_name: None,
        board_id: None,
    })))
    .unwrap();
    let home = studio.view().home.expect("gallery still shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsAName),
        "flashed empty device asks for a name on its card: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Connected { .. }
    ));
}

/// Row 5a (failure injection: disconnect mid-pull): the device vanishing
/// during a pull surfaces as a non-fatal `Unreadable` state — no panic, and
/// management operations (erase) remain reachable.
#[test]
fn disconnect_mid_pull_is_nonfatal_and_erase_stays_reachable() {
    let (store, host) = library();
    store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_project_files(project_files("v-device")),
    ));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id).expect("initial connect succeeds");

    // Cut the wire a little into the NEXT pull: some bytes flow, then the
    // stream reports the device gone mid-transfer.
    device.set_failure_plan(
        FakeFailurePlan::none().with_disconnect_after_bytes(device.served_bytes() + 64),
    );
    drive(studio.refresh_device_sync_for_test());

    let sync = studio
        .device_sync_for_test()
        .expect("failed pull leaves a state");
    assert!(
        matches!(sync.content, DeviceContent::Unreadable { .. }),
        "mid-pull disconnect surfaces as unreadable, got {:?}",
        sync.content
    );

    // Erase is still reachable: the scripted transition runs and the
    // controller degrades gracefully when the (dead) wire cannot reattach.
    let outcome = drive(studio.dispatch(device_action(DeviceOp::ResetToBlank {
        target: studio.device_target_for_test(),
    })));
    assert!(
        outcome.is_ok(),
        "erase after a disconnect must not fail fatally: {outcome:?}"
    );
    // The device really was erased: its next boot output is blank-flash ROM
    // chatter. (Lift the wire failure first — the erased DEVICE is what we
    // are asserting, not the dead stream.)
    device.set_failure_plan(FakeFailurePlan::none());
    let erased_lines = read_device_lines(&device, Duration::from_millis(500));
    assert!(
        erased_lines
            .iter()
            .any(|line| line.contains("invalid header: 0xffffffff")),
        "the erase transition landed on the device: {erased_lines:?}"
    );
}

/// Row 5b (failure injection: stall during connect): a device that never
/// produces output times out through the readiness classifier with the
/// no-serial-output message.
///
/// NOTE: the bounded wait is `DeviceSession`'s readiness deadline
/// (`DeviceTimers`); after readiness, mid-request stalls are bounded by the
/// session channel's request-idle budget. This row pins the connect-time
/// half: a fully silent device fails the attach with the no-serial-output
/// diagnosis instead of hanging (row 8 covers the Unresponsive state +
/// reconnect recovery behind the same silence).
#[test]
fn stall_during_connect_times_out_with_no_serial_output() {
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    device.set_failure_plan(FakeFailurePlan::none().with_stall_after_bytes(0));

    let outcome = connect_through_link(&mut studio, &endpoint_id);

    let error = outcome.expect_err("a fully stalled device cannot attach");
    let message = match &error {
        UiError::Transport(message) => message.clone(),
        other => other.to_string(),
    };
    assert!(
        message.contains("no serial output"),
        "stalled connect classifies as no-serial-output: {message}"
    );
    assert!(
        matches!(studio.snapshot().server.state, ServerState::Failed { .. }),
        "server state reflects the failed attach"
    );
}

/// Row 6 (Incompatible: hello suppressed): an `M!`-speaking device whose
/// firmware predates the wire hello classifies `Incompatible` through the
/// real path; the card surfaces reflash as the affordance; a flash
/// reboots the device to a compatible build and the session lands `Ready`.
#[test]
fn incompatible_no_hello_reflashes_through_the_card() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_suppressed_hello(),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    shorten_ready_deadline(&mut studio, Duration::from_millis(700));
    studio.attach_library(host);

    // The connect resolves Ok with the incompatibility notice (no dead-end).
    let outcome = connect_through_link(&mut studio, &endpoint_id)
        .expect("incompatible connect resolves without error");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("incompatible")),
        "the connect surfaces the incompatibility notice, got {:?}",
        outcome.notices
    );
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::Incompatible {
                reason: IncompatibleReason::NoHello
            })
        ),
        "hello suppression classifies Incompatible(NoHello), got {:?}",
        studio.device_state_for_test()
    );

    // Reflash is the ONE affordance: the card derives Needs-firmware-
    // update, whose Update runs the same ProvisionFirmware (M8′).
    let home = studio.view().home.expect("gallery shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsFirmwareUpdate),
        "incompatible firmware derives the update card: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );

    // Flash → reboot → Ready (the flashed build speaks the current proto).
    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: studio.device_target_for_test(),
        setup_name: None,
        board_id: None,
    })))
    .unwrap();
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::Ready { .. })
        ),
        "the reflashed device lands Ready, got {:?}",
        studio.device_state_for_test()
    );
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Connected { .. }
    ));
    let home = studio.view().home.expect("gallery still shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsAName),
        "the flow proceeds to the name sheet after the reflash: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
}

/// Row 7 (Incompatible: proto mismatch): a hello carrying a foreign wire
/// proto classifies `Incompatible` immediately (no deadline burn); same
/// reflash affordance and recovery as the no-hello row.
#[test]
fn incompatible_proto_mismatch_reflashes_through_the_card() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_proto_override(lpc_wire::WIRE_PROTO_VERSION + 1),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id)
        .expect("incompatible connect resolves without error");
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::Incompatible {
                reason: IncompatibleReason::ProtoMismatch { .. }
            })
        ),
        "a foreign proto hello classifies Incompatible(ProtoMismatch), got {:?}",
        studio.device_state_for_test()
    );

    let home = studio.view().home.expect("gallery shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsFirmwareUpdate)
    );

    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: studio.device_target_for_test(),
        setup_name: None,
        board_id: None,
    })))
    .unwrap();
    assert!(matches!(
        studio.device_state_for_test(),
        Some(DeviceState::Ready { .. })
    ));
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Connected { .. }
    ));
}

/// Row 8 (Unresponsive → reconnect): a fully stalled wire surfaces
/// `Unresponsive` at the readiness deadline; once the device answers again,
/// `ConnectLightPlayer` reconnects (rebuild) and the session lands `Ready`.
#[test]
fn unresponsive_device_reconnects_to_ready_after_unstall() {
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    shorten_ready_deadline(&mut studio, Duration::from_millis(700));
    device.set_failure_plan(FakeFailurePlan::none().with_stall_after_bytes(0));

    let error = connect_through_link(&mut studio, &endpoint_id)
        .expect_err("a fully stalled device cannot attach");
    assert!(
        error.to_string().contains("no serial output"),
        "the diagnosis names the silence: {error}"
    );
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::Unresponsive { .. })
        ),
        "the readiness deadline surfaces Unresponsive, got {:?}",
        studio.device_state_for_test()
    );
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Failed { .. }
    ));

    // The wire recovers (un-stall) → explicit reconnect rebuilds the link.
    device.set_failure_plan(FakeFailurePlan::none());
    drive(studio.dispatch(device_action(DeviceOp::ConnectLightPlayer {
        target: studio.device_target_for_test(),
    })))
    .expect("reconnect after un-stall succeeds");

    assert!(matches!(
        studio.device_state_for_test(),
        Some(DeviceState::Ready { .. })
    ));
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Connected { .. }
    ));
}

/// Row 9 (reconnect after Gone): the device vanishing mid-session marks the
/// session `Gone`; `ConnectLightPlayer` reconnects — a rebuilt stream +
/// transport on the same endpoint — and readiness lands `Ready` again
/// (finding 8: reopen used to reuse the dead serial thread).
#[test]
fn reconnect_after_gone_rebuilds_the_link_to_ready() {
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);

    connect_through_link(&mut studio, &endpoint_id).expect("initial connect succeeds");
    assert!(matches!(
        studio.device_state_for_test(),
        Some(DeviceState::Ready { .. })
    ));

    // Unplug: the stream reports EOF on the next pull and the session goes
    // Gone (observed via the channel's ConnectionLost).
    device.set_failure_plan(
        FakeFailurePlan::none().with_disconnect_after_bytes(device.served_bytes()),
    );
    drive(studio.refresh_device_sync_for_test());
    assert!(
        matches!(studio.device_state_for_test(), Some(DeviceState::Gone)),
        "a dead stream marks the session Gone, got {:?}",
        studio.device_state_for_test()
    );

    // Replug: reconnect rebuilds stream + transport and re-runs readiness.
    device.set_failure_plan(FakeFailurePlan::none());
    drive(studio.dispatch(device_action(DeviceOp::ConnectLightPlayer {
        target: studio.device_target_for_test(),
    })))
    .expect("reconnect after Gone succeeds");

    assert!(matches!(
        studio.device_state_for_test(),
        Some(DeviceState::Ready { .. })
    ));
    assert!(matches!(
        studio.snapshot().server.state,
        ServerState::Connected { .. }
    ));
}

/// Row 10 (erase lands BlankFlash as success): erasing a healthy device
/// through the card's Danger tab succeeds — the rebuilt link classifies
/// `BlankFlash` and the card derives Ready-to-set-up (flash stays the next
/// step), all without an error. Device-lifecycle P3: erasing the open
/// device DETACHES the editor lens, so the top-level server releases to
/// `Disconnected` (a clean return to the gallery) rather than leaving a
/// no-firmware server bound to the wiped device.
#[test]
fn erase_lands_blank_flash_as_success_through_the_card() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let outcome = drive(studio.dispatch(deploy_action(DeployOp::EraseDevice {
        target: studio.device_target_for_test(),
    })))
    .expect("erase from the card is a success");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("wiped")),
        "the erase reports its result, got {:?}",
        outcome.notices
    );
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::BlankFlash)
        ),
        "post-erase readiness lands BlankFlash — success for an erase, got {:?}",
        studio.device_state_for_test()
    );
    assert!(
        matches!(studio.snapshot().server.state, ServerState::Disconnected),
        "erasing the open device detaches the lens — the server releases \
         to the gallery, got {:?}",
        studio.snapshot().server.state
    );
    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        None,
        "the editor lens is detached after the wipe"
    );
    let home = studio.view().home.expect("gallery shows");
    assert!(
        home.devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::ReadyToSetUp),
        "the card derives Ready-to-set-up after the erase: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
}

/// Row 11 (D34 rename, both halves, through the real link): a device
/// renamed while OFFLINE reconciles at the next connect — the registry
/// name wins over the device-reported name (and the connect path writes it
/// back to `/.lp/device.json`); a rename dispatched while LIVE updates the
/// registry and the cached sync identity in one action.
#[test]
fn device_rename_reconciles_registry_name_over_the_link() {
    use crate::app::places::{DeviceRegistry, RegisteredDevice};

    let (store, host) = library();
    // remembered under its stamped name, then renamed while offline
    let registry = DeviceRegistry::new(store.fs_handle());
    registry
        .upsert(RegisteredDevice {
            uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
            name: "Bench board".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: None,
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        })
        .unwrap();
    registry
        .rename("dev_aaaaaaaaaaaaaaaa", "Luna's sign")
        .unwrap();

    // the device still reports the STALE stamped name
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_identity(FakeDeviceIdentity::new(
            "dev_aaaaaaaaaaaaaaaa",
            "Bench board",
        )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    assert_eq!(
        sync.identity
            .as_ref()
            .map(|identity| identity.name.as_str()),
        Some("Luna's sign"),
        "the registry name wins over the device-reported name at connect"
    );

    // live rename: registry + cached identity move together
    let outcome = drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::app::home::HOME_NODE_ID),
        crate::HomeOp::RenameDevice {
            uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
            name: "Porch sign".to_string(),
        },
    )))
    .expect("live rename succeeds");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("Porch sign")),
        "the rename reports its result, got {:?}",
        outcome.notices
    );
    assert_eq!(
        studio
            .device_sync_for_test()
            .and_then(|sync| sync.identity.as_ref())
            .map(|identity| identity.name.as_str()),
        Some("Porch sign"),
        "the cached sync identity carries the new name"
    );
    assert_eq!(
        registry.list().unwrap()[0].name,
        "Porch sign",
        "the registry carries the new name"
    );
}

/// Row 11b (G2 walk, 2026-08-05): forgetting the board in front of you
/// revokes the transport's persistent access to it, not just its registry
/// row. Deleting the row alone was silently undone on the next page load —
/// the Web Serial grant outlives the page, so the app re-enumerated the
/// port, re-derived the same uid from the board's silicon, and the sighting
/// write recreated the row. Here the revocation must REACH the provider
/// (the seam a defaulted trait method would swallow), the session must be
/// gone, and an OFFLINE device must stay registry-only — nothing to revoke,
/// nothing revoked.
#[test]
fn forgetting_a_live_device_revokes_its_port_grant_and_its_registry_row() {
    use crate::app::home::HOME_NODE_ID;
    use crate::app::places::RegisteredDevice;

    let (store, host) = library();
    seed_registry(
        &store,
        RegisteredDevice {
            uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
            name: "Shed board".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: None,
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        },
    );

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(STAMPED_UID, "Bench board")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    assert!(
        registry(&store)
            .iter()
            .any(|device| device.uid == STAMPED_UID),
        "the connect sighting registered the live board"
    );

    // The connector outlives the session the forget tears down, so grab it
    // while the pool still holds one.
    let connector = live_device_connector(&studio);

    let outcome = drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        crate::HomeOp::ForgetDevice {
            uid: STAMPED_UID.to_string(),
        },
    )))
    .expect("forget succeeds");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("forgotten")),
        "the forget reports its result, got {:?}",
        outcome.notices
    );
    assert_eq!(
        fake_forgotten_endpoints(&connector),
        vec![endpoint_id.clone()],
        "the revocation reached the provider, naming the live board's endpoint"
    );
    assert_eq!(
        studio.runtime_pool_for_test().device_sessions().count(),
        0,
        "the session was disconnected before its grant was revoked"
    );
    assert!(
        !registry(&store)
            .iter()
            .any(|device| device.uid == STAMPED_UID),
        "the registry row went with it"
    );

    // The offline board: its row goes, but no grant can be named for it —
    // a specific offline board cannot be matched to a granted port.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        crate::HomeOp::ForgetDevice {
            uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
        },
    )))
    .expect("forgetting an offline device succeeds");
    assert_eq!(
        fake_forgotten_endpoints(&connector),
        vec![endpoint_id],
        "an offline device revokes nothing"
    );
    assert!(
        registry(&store).is_empty(),
        "the offline row went too, got {:?}",
        registry(&store)
    );
}

/// The connector behind the one live device session (the forget path drops
/// the session, so tests capture this first).
fn live_device_connector(studio: &StudioController) -> Rc<lpa_link::LinkConnector> {
    studio
        .runtime_pool_for_test()
        .device_sessions()
        .find_map(crate::RuntimeSession::hardware_session)
        .expect("a live device session")
        .connector()
}

fn fake_forgotten_endpoints(connector: &lpa_link::LinkConnector) -> Vec<LinkEndpointId> {
    #[allow(
        unreachable_patterns,
        reason = "providers beyond Fake are feature/target-gated, so the \
                  wildcard arm is unreachable in some test configurations"
    )]
    match connector {
        lpa_link::LinkConnector::Fake(provider) => provider.forgotten_endpoints(),
        _ => panic!("the scripted board runs on the fake connector"),
    }
}

/// Row 12 (P2 coexistence): a fake device connected through the real link
/// AND a project opened on the sim — both sessions live in the pool at
/// once. The old `open_from_home` hardware refusal is gone: the open
/// succeeds, the editor mirror lands on the SIM session (lens), the device
/// session keeps its connect-time classification (`device_sync` intact), a
/// slot-edit round-trips over the sim's wire, and the device's slow status
/// heartbeat drains a buffered console line into the ring.
///
/// Host builds have no browser-worker provider, so the sim session is
/// installed through the stub seam with an in-process server client; the
/// open itself still runs the REAL `open_from_home` reuse path.
#[test]
fn sim_and_device_sessions_coexist_and_the_open_guard_is_gone() {
    use super::studio_edit_e2e_tests::{
        InProcessServerIo, edit_e2e_files, edit_e2e_server, find_slot, slot_value_display,
    };
    use crate::app::home::HOME_NODE_ID;
    use crate::{HomeOp, SlotEditOp, StudioServerClient, UiLogDraft, UiLogLevel, UiLogOrigin};
    use lpc_model::LpValue;
    use std::collections::VecDeque;

    let (store, host) = library();
    // "Porch" runs on the DEVICE; "Sign" (the edit-e2e node graph, so a
    // slot exists to edit) opens on the SIM.
    let porch = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let porch_files = store.open(porch.uid).unwrap().read_all_files().unwrap();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(porch_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("device connect succeeds");
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known { .. })
        ),
        "the device classifies before the open"
    );

    // The sim session, alongside the device (an in-process server client
    // stands in for the browser worker on host).
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let io = InProcessServerIo {
        server: Rc::clone(&server),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let sim_id = studio.install_stub_sim_with_client_for_test(
        StudioServerClient::from_io_for_test("in-process", Box::new(io)),
    );

    // THE forcing case: opening a project with a device attached used to
    // refuse ("disconnect the device to open this project"). Now it opens
    // on the sim while the device stays attached.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: sign.uid.to_string(),
        },
    )))
    .expect("opening a project with a device attached no longer refuses");

    // Both sessions in the pool; the lens (editor mirror) is on the sim.
    let pool = studio.runtime_pool_for_test();
    assert!(
        pool.oldest_device_session().is_some(),
        "device session survives"
    );
    assert!(pool.sim_session().is_some(), "sim session exists");
    assert_eq!(pool.lens(), Some(sim_id), "the editor is a lens on the sim");
    // The device session is still classified: device_sync intact.
    let sync = studio
        .device_sync_for_test()
        .expect("device_sync survives the open");
    let DeviceContent::Known { slug, relation, .. } = &sync.content else {
        panic!("device stays classified, got {:?}", sync.content);
    };
    assert_eq!(slug, &porch.slug);
    assert_eq!(*relation, lpc_history::SyncRelation::AtHead);

    // The editor mirror is live on the sim: a slot-edit round-trips.
    let view = studio.view();
    assert!(view.home.is_none(), "the open left the gallery");
    let rate = find_slot(&view, "controls.rate");
    let address = rate.address.clone().expect("rate slot carries an address");
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        SlotEditOp::SetValue {
            address,
            value: LpValue::F32(2.0),
        },
    )))
    .expect("slot edit lands on the sim session");
    let view = studio.view();
    assert_eq!(slot_value_display(find_slot(&view, "controls.rate")), "2");

    // The device heartbeat drains a buffered console line into the
    // SESSION's console tail (D42: the per-device console — session
    // streams no longer land in the global ring). Trace/debug
    // diagnostics stay OFF the tail (the retired console's Info+
    // display floor; the sim worker's per-tick spam must not drown
    // the 40-line ring)…
    studio.push_device_console_log_for_test(UiLogDraft::new(
        UiLogLevel::Trace,
        UiLogOrigin::Device,
        "tick delta=32ms incoming=1 responses=1",
    ));
    studio.push_device_console_log_for_test(UiLogDraft::new(
        UiLogLevel::Info,
        UiLogOrigin::Device,
        "standalone frame tick",
    ));
    studio.run_due_heartbeats();
    let device_tail_has = |studio: &StudioController, message: &str| {
        studio
            .runtime_pool_for_test()
            .oldest_device_session()
            .expect("a device session is attached")
            .console_tail()
            .iter()
            .any(|entry| entry.message == message)
    };
    assert!(
        device_tail_has(&studio, "standalone frame tick"),
        "the first heartbeat drains the device session's console buffer into its tail"
    );
    assert!(
        !device_tail_has(&studio, "tick delta=32ms incoming=1 responses=1"),
        "trace diagnostics stay below the tail's Info+ floor"
    );
    assert!(
        !studio
            .logs()
            .iter()
            .any(|entry| entry.message == "standalone frame tick"),
        "session console streams stay off the global ring (D42)"
    );
    // …and stays SLOW: a line buffered right after is not drained until
    // the heartbeat interval elapses (the fixed test clock never advances).
    studio.push_device_console_log_for_test(UiLogDraft::new(
        UiLogLevel::Info,
        UiLogOrigin::Device,
        "buffered until the next heartbeat",
    ));
    studio.run_due_heartbeats();
    assert!(
        !device_tail_has(&studio, "buffered until the next heartbeat"),
        "a heartbeat inside the interval drains nothing"
    );
}

/// Row 13 (re-homed from the dialog's pre-target row, M8′): a device
/// already running a known project can still be pushed a DIFFERENT
/// project — the project card's "Push to <device>" / picker lane
/// (`PushProject` with another key) replaces the running copy; nothing
/// about the direct-push lane locks the device to its current project.
#[test]
fn push_replaces_the_running_project_with_a_different_one() {
    let (store, host) = library();
    let porch = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let porch_files = store.open(porch.uid).unwrap().read_all_files().unwrap();
    let other = store
        .install_package(
            "Other",
            &project_files("v-other"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(porch_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known { slug, .. }) if slug == &porch.slug
        ),
        "the device runs the known project"
    );

    drive(studio.dispatch(deploy_action(DeployOp::PushProject {
        target: studio.device_target_for_test(),
        key: other.uid.to_string(),
    })))
    .expect("pushing a different project succeeds");
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known { slug, relation: lpc_history::SyncRelation::AtHead, .. })
                if slug == &other.slug
        ),
        "the device now runs the other project at its head, got {:?}",
        studio
            .device_sync_for_test()
            .map(|sync| sync.content.clone())
    );
}

/// Row P3-a + Q3 (gallery return keeps sessions): a project open on the
/// sim with hardware attached, an acked edit applied → detach the lens
/// (the gallery-return dispatch) → BOTH sessions survive — sim wire
/// client attached, device reconcile state intact — and re-attaching the
/// lens rebuilds the mirror over the server-side overlay: the acked edit
/// is still visible. The re-attach answering `list_loaded_projects` on
/// the detached session's own client is the worker-alive proxy.
#[test]
fn detach_lens_keeps_sessions_and_reattach_rebuilds_the_mirror() {
    use super::studio_edit_e2e_tests::{find_slot, slot_value_display};
    use crate::{ProjectOp, SlotEditOp, UxUpdateSink};
    use lpc_model::LpValue;

    let (mut studio, _device, sim_id) = coexisting_sim_and_device();

    // An acked edit on the sim mirror.
    let view = studio.view();
    assert!(view.home.is_none(), "the open left the gallery");
    let address = find_slot(&view, "controls.rate")
        .address
        .clone()
        .expect("rate slot carries an address");
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        SlotEditOp::SetValue {
            address,
            value: LpValue::F32(2.0),
        },
    )))
    .expect("slot edit lands on the sim session");

    // Detach the lens — the gallery-return route policy's dispatch.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::DetachLens,
    )))
    .expect("lens detach succeeds");

    let view = studio.view();
    assert!(view.home.is_some(), "a detached editor shows the gallery");
    {
        let pool = studio.runtime_pool_for_test();
        assert_eq!(pool.lens(), None, "the lens is released");
        let sim = pool.sim_session().expect("sim session survives");
        assert!(sim.is_connected(), "sim wire client stays attached");
        assert!(
            pool.oldest_device_session().is_some(),
            "device session survives"
        );
    }
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known { .. })
        ),
        "device reconcile state is intact across the detach"
    );

    // Re-attach: the connect sequence rebuilds the mirror on the SAME
    // session — the client answers, and the acked edit is visible.
    drive(studio.attach_lens(sim_id, UxUpdateSink::noop())).expect("re-attach connects");
    assert_eq!(studio.runtime_pool_for_test().lens(), Some(sim_id));
    let view = studio.view();
    assert!(view.home.is_none(), "the editor is back");
    assert_eq!(
        slot_value_display(find_slot(&view, "controls.rate")),
        "2",
        "the acked edit survived detach → re-attach"
    );
}

/// Row P3-c (stop-sim): the destroy-session op removes THE sim session
/// from the pool — quiesce first when the lens is on it — while the
/// device session stays; stopping again reports the truth.
#[test]
fn stop_sim_destroys_the_sim_session_and_keeps_the_device() {
    let (mut studio, _device, _sim_id) = coexisting_sim_and_device();
    assert!(studio.view().home.is_none(), "a project is open on the sim");

    let outcome =
        drive(studio.dispatch(device_action(DeviceOp::StopSimulator))).expect("stop-sim succeeds");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("Simulator stopped")),
        "stop-sim reports itself"
    );

    let view = studio.view();
    assert!(
        view.home.is_some(),
        "stopping the sim returns to the gallery"
    );
    {
        let pool = studio.runtime_pool_for_test();
        assert!(pool.sim_session().is_none(), "the sim session is gone");
        assert!(
            pool.oldest_device_session().is_some(),
            "the device session stays"
        );
        assert_eq!(pool.lens(), None);
    }
    assert!(
        studio.device_sync_for_test().is_some(),
        "device reconcile state survives stop-sim"
    );

    drive(studio.dispatch(device_action(DeviceOp::StopSimulator)))
        .expect_err("stopping a stopped simulator reports it is not running");
}

/// Row P4 (pool-fed roster): both sessions live, editor detached → the
/// home view carries the live SIM card (Running + the loaded project's
/// chip, pinned first among live) AND the live device card, and the sim's
/// project card wears the "Running in simulator" stamp (the D28 sim arm);
/// stop-sim removes the sim card and the stamp while the device card
/// stays.
#[test]
fn home_view_carries_both_pool_cards_and_stop_sim_removes_the_sim_card() {
    use crate::ProjectOp;

    let (mut studio, _device, _sim_id) = coexisting_sim_and_device();
    drive(studio.settle_library());

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::DetachLens,
    )))
    .expect("lens detach succeeds");

    let view = studio.view();
    let home = view.home.expect("a detached editor shows the gallery");
    let sim_card = &home.devices[0];
    assert!(sim_card.sim, "the sim card pins first among live");
    assert_eq!(sim_card.state, crate::RosterCardState::RunningUpToDate);
    let chip = sim_card
        .project
        .as_ref()
        .expect("the sim card wears its loaded project chip");
    assert_eq!(chip.name, "2026-07-14-0900-sign");
    let device_card = home
        .devices
        .iter()
        .find(|card| !card.sim && card.name == "Bench board")
        .expect("the live device keeps its card");
    assert!(
        !matches!(device_card.state, crate::RosterCardState::Offline { .. }),
        "the device card is live, got {:?}",
        device_card.state
    );
    let sign_project = home
        .projects
        .iter()
        .find(|card| card.slug == "2026-07-14-0900-sign")
        .expect("the sim's project is in the library section");
    assert!(
        sign_project.running_in_sim,
        "the D28 sim arm stamps the project card"
    );

    // Stop-sim: the sim card and the stamp die with the session.
    drive(studio.dispatch(device_action(DeviceOp::StopSimulator))).expect("stop-sim succeeds");
    let view = studio.view();
    let home = view.home.expect("still on the gallery");
    assert!(
        home.devices.iter().all(|card| !card.sim),
        "the sim card is gone with the session"
    );
    assert!(
        home.devices
            .iter()
            .any(|card| !card.sim && card.name == "Bench board"),
        "the device card stays"
    );
    assert!(
        home.projects.iter().all(|card| !card.running_in_sim),
        "no session, no 'Running in simulator' stamp"
    );
}

/// Row P4-b (the sim-card click): `ProjectOp::OpenSimProject` re-attaches
/// the editor lens to THE sim session — the pool's attach path, mirror
/// rebuilt over the session's server-side state.
#[test]
fn open_sim_project_reattaches_the_lens_to_the_sim() {
    use crate::ProjectOp;

    let (mut studio, _device, sim_id) = coexisting_sim_and_device();
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::DetachLens,
    )))
    .expect("lens detach succeeds");
    assert!(studio.view().home.is_some(), "detached editor = gallery");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenSimProject,
    )))
    .expect("the sim-card click reopens the editor on the sim session");

    assert_eq!(studio.runtime_pool_for_test().lens(), Some(sim_id));
    assert!(studio.view().home.is_none(), "the editor is back");
}

/// Poisoned-instance recovery, part 1 (worker crash): the link layer
/// reports a sticky instance-fatal for the sim session; the tick-cadence
/// recovery edge-detects it, surfaces the primary panic on the console,
/// tears the dead session down, and attempts the auto-reboot with the
/// last-known project. On this host harness the reboot's fresh install
/// cannot complete (the browser-worker provider is wasm-only), so the
/// attempt is asserted through its logged failure; the wasm-side reboot
/// is covered by the live check.
#[test]
fn sim_crash_is_detected_torn_down_and_auto_reboot_attempted() {
    let (mut studio, _device, _sim_id) = coexisting_sim_and_device();
    arm_sim_fatal(&studio, "browser worker instance fatal: panicked at 'boom'");

    drive(studio.run_due_sim_crash_recovery());

    let logs = studio.logs();
    assert!(
        logs.iter().any(|entry| {
            entry.message.contains("Simulator crashed:")
                && entry.message.contains("panicked at 'boom'")
        }),
        "the primary panic reaches the console: {:?}",
        logs.iter().map(|entry| &entry.message).collect::<Vec<_>>()
    );
    assert!(
        logs.iter().any(|entry| entry.message.contains("restarted")),
        "the auto-reboot announces itself"
    );
    assert!(
        logs.iter()
            .any(|entry| entry.message.contains("simulator restart failed")),
        "the reboot was attempted (and failed on the host harness): {:?}",
        logs.iter().map(|entry| &entry.message).collect::<Vec<_>>()
    );
    assert!(
        studio.runtime_pool_for_test().sim_session().is_none(),
        "the dead session was torn down"
    );
}

/// Poisoned-instance recovery, part 2 (flap guard): a second crash within
/// the guard window (the fixed test clock never advances) is marked
/// `SimCrashed` but NOT auto-rebooted — the session stays Failed so the
/// card offers manual restart.
#[test]
fn sim_crash_within_the_flap_guard_stays_failed_for_manual_restart() {
    use super::studio_edit_e2e_tests::{InProcessServerIo, edit_e2e_server};
    use crate::StudioServerClient;
    use std::collections::VecDeque;

    let (mut studio, _device, _sim_id) = coexisting_sim_and_device();
    arm_sim_fatal(&studio, "browser worker instance fatal: first crash");
    drive(studio.run_due_sim_crash_recovery());
    assert!(
        studio.runtime_pool_for_test().sim_session().is_none(),
        "the first crash consumed the auto-reboot"
    );

    // A fresh sim (as if the reboot had succeeded on wasm), crashing
    // again inside the guard window.
    let sim_io = InProcessServerIo {
        server: Rc::new(RefCell::new(edit_e2e_server())),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    studio.install_stub_sim_with_client_for_test(StudioServerClient::from_io_for_test(
        "in-process",
        Box::new(sim_io),
    ));
    arm_sim_fatal(&studio, "browser worker instance fatal: second crash");
    drive(studio.run_due_sim_crash_recovery());

    let pool = studio.runtime_pool_for_test();
    let sim = pool
        .sim_session()
        .expect("the crashed session stays for manual restart");
    assert!(
        matches!(
            sim.server_state(),
            ServerState::Failed {
                kind: ServerFailureKind::SimCrashed,
                ..
            }
        ),
        "the session is marked SimCrashed, got {:?}",
        sim.server_state()
    );
    assert!(
        studio
            .logs()
            .iter()
            .any(|entry| entry.message.contains("keeps crashing")),
        "the guard explains why no reboot ran"
    );
}

/// Arm the scripted instance-fatal on the sim session's fake connector —
/// the host stand-in for the browser worker's sticky fatal report.
fn arm_sim_fatal(studio: &StudioController, message: &str) {
    let pool = studio.runtime_pool_for_test();
    let session = pool.sim_session().expect("a sim session");
    let crate::RuntimePayload::Sim(sim) = session.payload() else {
        panic!("sim session holds a sim payload");
    };
    #[allow(
        unreachable_patterns,
        reason = "providers beyond Fake are feature/target-gated, so the \
                  wildcard arm is unreachable in some test configurations"
    )]
    match &*sim.connector {
        lpa_link::LinkConnector::Fake(provider) => {
            provider.set_session_fatal(Some(message.to_string()));
        }
        _ => panic!("stub sim uses the fake connector"),
    }
}

/// Row P3-d (minimal D29): a device attached with its project LOADED and
/// library-known → `ProjectOp::OpenDeviceProject` attaches the lens to
/// the DEVICE session and opens its running project in the editor over
/// the device's own wire client; a slot edit round-trips over the fake
/// wire. (The web device card routes Running-family clicks to this op;
/// no URL work — D37 stays M5.)
#[test]
fn d29_click_opens_the_devices_running_project_in_the_editor() {
    use super::studio_edit_e2e_tests::{edit_e2e_files, find_slot, slot_value_display};
    use crate::{ProjectOp, SlotEditOp};
    use lpc_model::LpValue;

    let (store, host) = library();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let sign_files = store.open(sign.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(sign_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    // Attach observed only (roster model): no editor yet.
    assert!(matches!(
        studio.snapshot().project.state,
        crate::ProjectState::NotLoaded
    ));

    // The D29 click.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject { uid: None },
    )))
    .expect("the D29 op connects the device's running project");

    let device_id = {
        let pool = studio.runtime_pool_for_test();
        let device_id = pool.oldest_device_session().expect("device session").id();
        assert_eq!(
            pool.lens(),
            Some(device_id),
            "the lens is on the DEVICE session"
        );
        device_id
    };
    let view = studio.view();
    assert!(view.home.is_none(), "the editor shows the device's project");
    let address = find_slot(&view, "controls.rate")
        .address
        .clone()
        .expect("rate slot carries an address");
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        SlotEditOp::SetValue {
            address,
            value: LpValue::F32(2.0),
        },
    )))
    .expect("slot edit round-trips over the device's wire");
    assert_eq!(
        slot_value_display(find_slot(&studio.view(), "controls.rate")),
        "2"
    );
    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(device_id),
        "the lens stays on the device across edits"
    );
}

/// Device-lifecycle P3 (editor sever + post-reset nav): erasing a device
/// whose project is OPEN in the editor severs the lens — the app returns
/// to the gallery — and says why. A runtime reset (which keeps the
/// project) leaves the editor open. This is the fix for the hardware-walk
/// "factory reset weird state / can't reflash without refresh": the old
/// path reset the project content but left the lens bound to the wiped
/// device.
#[test]
fn erase_from_the_editor_severs_the_lens_and_returns_to_the_gallery() {
    use super::studio_edit_e2e_tests::edit_e2e_files;
    use crate::ProjectOp;

    let (store, host) = library();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let sign_files = store.open(sign.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(sign_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    // Open the device's project in the editor (lens on the device).
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject { uid: None },
    )))
    .expect("the D29 op opens the device project");
    let device_id = studio
        .runtime_pool_for_test()
        .oldest_device_session()
        .expect("device session")
        .id();
    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(device_id),
        "the editor is a lens on the device"
    );
    assert!(studio.view().home.is_none(), "the editor is showing");

    // Erase the device from under the open editor.
    let outcome = drive(studio.dispatch(device_action(DeviceOp::ResetToBlank {
        target: studio.device_target_for_test(),
    })))
    .expect("erase succeeds even from the editor");

    // The lens is severed → the app is back at the gallery.
    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        None,
        "erasing the open device detaches the lens"
    );
    assert!(
        studio.view().home.is_some(),
        "erasing the open project returns to the gallery"
    );
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("no longer on this device")),
        "the sever is explained: {:?}",
        outcome
            .notices
            .iter()
            .map(|n| &n.message)
            .collect::<Vec<_>>()
    );
}

/// The state-audit's promise, end-to-end: a card's tab and sheet are
/// CORE view-state, drivable past the dispatch boundary — no widget
/// signals involved. The e2e opens the Danger tab and the erase confirm
/// purely through `HomeOp::CardUi` and reads them back off the view.
#[test]
fn card_tab_and_sheet_drive_through_core_ops() {
    use crate::app::home::HOME_NODE_ID;
    use crate::{CardSheet, CardUiOp, CardVerb, DeviceCardTab, HomeOp};

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_identity(FakeDeviceIdentity::new(
            "dev_aaaaaaaaaaaaaaaa",
            "Bench board",
        )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let card_key = {
        let view = studio.view();
        let home = view.home.as_ref().expect("gallery showing");
        let card = home
            .devices
            .iter()
            .find(|card| !card.sim)
            .expect("the connected device card");
        assert_eq!(card.ui.tab, DeviceCardTab::Status, "fresh card = Status");
        card.identity_key().to_string()
    };

    let card_ui =
        |op: CardUiOp| UiAction::from_op(ControllerId::new(HOME_NODE_ID), HomeOp::CardUi(op));
    drive(studio.dispatch(card_ui(CardUiOp::SelectTab {
        card: card_key.clone(),
        tab: DeviceCardTab::Danger,
    })))
    .expect("tab select dispatches");
    drive(studio.dispatch(card_ui(CardUiOp::OpenSheet {
        card: card_key.clone(),
        sheet: CardSheet::Confirm(CardVerb::Erase),
    })))
    .expect("sheet open dispatches");

    let view = studio.view();
    let card = view
        .home
        .as_ref()
        .expect("gallery showing")
        .devices
        .iter()
        .find(|card| card.identity_key() == card_key)
        .expect("the same card by identity");
    assert_eq!(card.ui.tab, DeviceCardTab::Danger);
    assert_eq!(card.ui.sheet, Some(CardSheet::Confirm(CardVerb::Erase)));

    drive(studio.dispatch(card_ui(CardUiOp::CloseSheet {
        card: card_key.clone(),
    })))
    .expect("sheet close dispatches");
    let view = studio.view();
    let card = view
        .home
        .as_ref()
        .expect("gallery showing")
        .devices
        .iter()
        .find(|card| card.identity_key() == card_key)
        .expect("the same card");
    assert_eq!(card.ui.sheet, None, "the sheet closed through core");
    assert_eq!(
        card.ui.tab,
        DeviceCardTab::Danger,
        "the tab survives the sheet round-trip"
    );
}

/// Journey B (state-flow model §1-B, contrast): erasing a DIFFERENT
/// device leaves the editor alone — the sever is specific to the lens's
/// own device. A sim-lens editor stays open and bound through a hardware
/// erase running in the background.
#[test]
fn erasing_the_device_leaves_a_sim_lens_editor_alone() {
    use super::studio_edit_e2e_tests::{InProcessServerIo, edit_e2e_files, edit_e2e_server};
    use crate::app::home::HOME_NODE_ID;
    use crate::{HomeOp, StudioServerClient};
    use std::collections::VecDeque;

    let (store, host) = library();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_identity(FakeDeviceIdentity::new(
            "dev_aaaaaaaaaaaaaaaa",
            "Bench board",
        )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    // The editor opens on the SIM…
    let sim_io = InProcessServerIo {
        server: Rc::new(RefCell::new(edit_e2e_server())),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let sim_id = studio.install_stub_sim_with_client_for_test(
        StudioServerClient::from_io_for_test("in-process", Box::new(sim_io)),
    );
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: sign.uid.to_string(),
        },
    )))
    .expect("open on the sim succeeds");
    assert_eq!(studio.runtime_pool_for_test().lens(), Some(sim_id));

    // …the hardware connects alongside, and gets erased from its card.
    connect_through_link(&mut studio, &endpoint_id).expect("device connect succeeds");
    drive(studio.dispatch(device_action(DeviceOp::ResetToBlank {
        target: studio.device_target_for_test(),
    })))
    .expect("erase succeeds with a sim lens open");

    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(sim_id),
        "erasing a different device never touches the editor (model §1-B)"
    );
    assert!(
        studio.view().home.is_none(),
        "the sim editor stays showing — no gallery bounce"
    );
}

/// Device-lifecycle P3 (contrast): the lens DETACH is specific to the
/// destructive erase. A runtime reset keeps the project on the device, so
/// it does not detach the lens (its live edit-state resets and reloads on
/// reattach — that reload path is pre-existing and unchanged here). This
/// pins that only a wipe sends you back to the gallery.
#[test]
fn runtime_reset_from_the_editor_keeps_the_lens_bound() {
    use super::studio_edit_e2e_tests::edit_e2e_files;
    use crate::ProjectOp;

    let (store, host) = library();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let sign_files = store.open(sign.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(sign_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_bbbbbbbbbbbbbbbb",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject { uid: None },
    )))
    .expect("the D29 op opens the device project");
    let device_id = studio
        .runtime_pool_for_test()
        .oldest_device_session()
        .expect("device session")
        .id();

    drive(studio.dispatch(device_action(DeviceOp::ResetDevice {
        target: studio.device_target_for_test(),
    })))
    .expect("runtime reset succeeds");

    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(device_id),
        "a runtime reset keeps the editor's lens on the device — only a \
         destructive wipe detaches it"
    );
}

/// Row P3-d (sim-open variant): the D29 click while a project is open on
/// the sim quiesces the sim mirror first and moves the lens; the sim
/// session STAYS in the pool with its wire client attached.
#[test]
fn d29_click_with_a_sim_project_open_moves_the_lens_and_keeps_the_sim() {
    use crate::ProjectOp;

    let (mut studio, _device, sim_id) = coexisting_sim_and_device_running();
    assert_eq!(studio.runtime_pool_for_test().lens(), Some(sim_id));

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject { uid: None },
    )))
    .expect("the D29 op connects the device's running project");

    let pool = studio.runtime_pool_for_test();
    let device_id = pool.oldest_device_session().expect("device session").id();
    assert_eq!(pool.lens(), Some(device_id), "the lens moved to the device");
    let sim = pool
        .sim_session()
        .expect("the sim session STAYS in the pool");
    assert!(sim.is_connected(), "the sim keeps its wire client");
    assert!(
        studio.view().home.is_none(),
        "the editor shows the device's project"
    );
}

/// Row P3-e (the P2 interim is gone): connecting hardware while a project
/// is open on the sim leaves the lens on the sim — attaching observes —
/// while the device reconciles in the background on its own client.
#[test]
fn device_connect_while_a_sim_project_is_open_leaves_the_lens_on_the_sim() {
    use super::studio_edit_e2e_tests::{
        InProcessServerIo, edit_e2e_files, edit_e2e_server, find_slot, slot_value_display,
    };
    use crate::app::home::HOME_NODE_ID;
    use crate::{HomeOp, StudioServerClient};
    use std::collections::VecDeque;

    let (store, host) = library();
    let porch = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let porch_files = store.open(porch.uid).unwrap().read_all_files().unwrap();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(porch_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);

    // The sim project opens FIRST.
    let sim_io = InProcessServerIo {
        server: Rc::new(RefCell::new(edit_e2e_server())),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let sim_id = studio.install_stub_sim_with_client_for_test(
        StudioServerClient::from_io_for_test("in-process", Box::new(sim_io)),
    );
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: sign.uid.to_string(),
        },
    )))
    .expect("open on the sim succeeds");
    assert_eq!(studio.runtime_pool_for_test().lens(), Some(sim_id));

    // NOW the hardware connects.
    connect_through_link(&mut studio, &endpoint_id).expect("device connect succeeds");

    let pool = studio.runtime_pool_for_test();
    assert_eq!(
        pool.lens(),
        Some(sim_id),
        "attaching a device does NOT steal the lens from the sim"
    );
    assert!(
        pool.oldest_device_session().is_some(),
        "device session installed"
    );
    // The sim mirror is untouched…
    let view = studio.view();
    assert!(view.home.is_none(), "the editor stayed open");
    assert_eq!(slot_value_display(find_slot(&view, "controls.rate")), "1");
    // …and the device reconciled in the background on its own client.
    let sync = studio
        .device_sync_for_test()
        .expect("connect-as-pull landed");
    assert!(
        matches!(&sync.content, DeviceContent::Known { .. }),
        "device classified while the lens stayed on the sim, got {:?}",
        sync.content
    );
}

/// M5/D37 (`#/device/<uid>` re-derivation): with the editor detached and
/// the device session already in the pool, the route's op attaches the
/// lens by uid — no reconnect — and the emitted view binds the device
/// lens (the URL's evidence). A uid the pool does NOT hold refuses
/// honestly instead of tearing the live session down.
#[test]
fn device_route_attaches_the_existing_session_by_uid() {
    use crate::{ProjectOp, UiLensRuntime};

    let (mut studio, _device, _sim_id) = coexisting_sim_and_device_running();
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::DetachLens,
    )))
    .expect("lens detach succeeds");
    assert!(studio.view().home.is_some(), "detached editor = gallery");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject {
            uid: Some("dev_aaaaaaaaaaaaaaaa".to_string()),
        },
    )))
    .expect("the route op attaches the existing session");

    let device_id = {
        let pool = studio.runtime_pool_for_test();
        let device_id = pool.oldest_device_session().expect("device session").id();
        assert_eq!(pool.lens(), Some(device_id), "the lens is on the device");
        device_id
    };
    let view = studio.view();
    assert!(view.home.is_none(), "the editor shows the device's project");
    assert_eq!(
        view.lens,
        Some(UiLensRuntime::Device {
            uid: Some("dev_aaaaaaaaaaaaaaaa".to_string()),
        }),
        "the view binds the device lens for the URL"
    );

    // A different uid: the live session is never sacrificed to the route.
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject {
            uid: Some("dev_bbbbbbbbbbbbbbbb".to_string()),
        },
    )))
    .expect_err("a mismatched uid refuses");
    let pool = studio.runtime_pool_for_test();
    assert_eq!(
        pool.oldest_device_session().map(crate::RuntimeSession::id),
        Some(device_id),
        "the attached session survives the refusal"
    );
}

/// M5/D37 (`#/sim/<key>` reuse-vs-open): re-opening the project the sim
/// ALREADY runs re-attaches the lens to the running session — the acked
/// overlay edit survives — instead of pushing the head again (which would
/// reset it). D19's head push stays for everything else; the emitted view
/// binds the sim lens with the loaded project's key (the URL's evidence).
#[test]
fn open_package_reattaches_when_the_sim_already_runs_it() {
    use super::studio_edit_e2e_tests::{find_slot, slot_value_display};
    use crate::app::home::HOME_NODE_ID;
    use crate::{HomeOp, ProjectOp, SlotEditOp, UiLensRuntime};
    use lpc_model::LpValue;

    let (mut studio, _device, sim_id) = coexisting_sim_and_device();
    // an acked edit on the open sim project ("Sign")
    let view = studio.view();
    let address = find_slot(&view, "controls.rate")
        .address
        .clone()
        .expect("rate slot carries an address");
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        SlotEditOp::SetValue {
            address,
            value: LpValue::F32(2.0),
        },
    )))
    .expect("slot edit lands on the sim session");
    // gallery return (the route policy's detach)
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::DetachLens,
    )))
    .expect("lens detach succeeds");

    let (sign_uid, sign_slug) = {
        let pool = studio.runtime_pool_for_test();
        let loaded = pool
            .sim_session()
            .expect("sim session survives detach")
            .sim_loaded_project()
            .expect("the sim remembers its loaded project");
        (loaded.uid.clone(), loaded.name.clone())
    };

    // the `#/sim/<key>` navigation (and the project-card click that rides
    // it): the same key the sim already runs
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: sign_uid.clone(),
        },
    )))
    .expect("the open succeeds");

    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(sim_id),
        "the lens re-attached to the running sim session"
    );
    let view = studio.view();
    assert!(view.home.is_none(), "the editor is back");
    assert_eq!(
        slot_value_display(find_slot(&view, "controls.rate")),
        "2",
        "the applied edit survived — re-attach, not a head re-push"
    );
    assert_eq!(
        view.lens,
        Some(UiLensRuntime::Sim {
            project_key: Some(sign_slug),
        }),
        "the view binds the sim lens with the loaded project's key"
    );
}

/// M5 (in-card push): the Running-behind card's Push button dispatches
/// `DeployOp::PushProject` directly — the button IS the D11 consent, no
/// dialog. While the push runs, the device card narrates through the
/// Operation-in-flight lane (the same session flag that blocks pool
/// replaces); it settles back to Running-up-to-date with the device at
/// head.
#[test]
fn push_from_card_narrates_operation_in_flight_and_settles() {
    use crate::app::roster::RosterCardState;
    use crate::{UxUpdate, UxUpdateSink};

    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let v1_files = store.open(summary.uid).unwrap().read_all_files().unwrap();
    // the library moves on: v2 becomes the head, so the device (holding
    // v1) classifies Behind at connect
    {
        use lpc_model::AsLpPath;
        let mut handle = store.open(summary.uid).unwrap();
        handle
            .apply_update("/shader.glsl".as_path(), Some(b"v2"))
            .unwrap();
        handle.record_save(2.0).unwrap().expect("head advanced");
    }

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(v1_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::Behind,
                ..
            })
        ),
        "device classifies Behind, got {:?}",
        studio.device_sync_for_test().map(|sync| &sync.content)
    );

    // Capture the progressive views the dispatch emits: the card must
    // pass through Operation-in-flight while the push runs.
    let seen_labels: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink_labels = Rc::clone(&seen_labels);
    let sink = UxUpdateSink::new(move |update| {
        if let UxUpdate::View(view) = update
            && let Some(home) = &view.home
        {
            for card in &home.devices {
                if let RosterCardState::OperationInFlight { label, .. } = &card.state {
                    sink_labels.borrow_mut().push(label.clone());
                }
            }
        }
    });
    let outcome = drive(studio.dispatch_with_updates(
        deploy_action(DeployOp::PushProject {
            target: studio.device_target_for_test(),
            key: summary.uid.to_string(),
        }),
        sink,
    ))
    .expect("the in-card push succeeds");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("Pushed")),
        "the push reports itself: {:?}",
        outcome.notices
    );
    assert!(
        seen_labels
            .borrow()
            .iter()
            .any(|label| label.starts_with("Pushing")),
        "the card narrated the push in flight: {:?}",
        seen_labels.borrow()
    );

    // Settled: at head, the operation cleared, the card derives
    // Running-up-to-date again.
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::AtHead,
                ..
            })
        ),
        "device is at head after the push, got {:?}",
        studio.device_sync_for_test().map(|sync| &sync.content)
    );
    let pool = studio.runtime_pool_for_test();
    assert!(
        !pool
            .oldest_device_session()
            .expect("device session")
            .op_in_flight(),
        "the operation cleared"
    );
    let view = studio.view();
    let home = view.home.expect("gallery view");
    assert!(
        home.devices
            .iter()
            .any(|card| matches!(card.state, RosterCardState::RunningUpToDate)),
        "the card settled to Running-up-to-date: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
}

/// P5 (project-format upgrades): a board holding an OLD-FORMAT project
/// must say so, and the one verb on its card must fix it end to end.
///
/// Before this, the connect-time pull read the manifest's `uid` and never
/// its `format`, so a format-4 board classified as Known/at-head and the
/// card claimed it was running a project the firmware had refused to load.
/// The upgrade runs pull → migrate IN THE LIBRARY → push (the device is
/// never rewritten in place, D14), so the board comes back at head with
/// current-format bytes.
#[test]
fn an_old_format_board_classifies_honestly_and_upgrades_in_one_verb() {
    use crate::app::roster::{DeviceFormatStanding, RosterCardState};

    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files_at_format(4, "v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let stale_files = store.open(summary.uid).unwrap().read_all_files().unwrap();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(stale_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    // (a) classification: the format, not the hash relation, owns the card
    let sync = studio.device_sync_for_test().expect("the pull landed");
    let DeviceContent::OldFormat {
        project_uid, class, ..
    } = &sync.content
    else {
        panic!(
            "a format-4 board is not Known/Running, got {:?}",
            sync.content
        );
    };
    assert_eq!(*class, lpa_upgrade::FormatClass::Upgradable { found: 4 });
    assert_eq!(
        project_uid.as_deref(),
        Some(summary.uid.to_string().as_str()),
        "the card knows which library project this board's copy belongs to"
    );
    let home = studio.view().home.expect("gallery view");
    assert!(
        home.devices.iter().any(|card| card.state
            == RosterCardState::HoldsOldFormatProject {
                standing: DeviceFormatStanding::Upgradable { found: 4 },
                expected: lpc_model::PROJECT_FORMAT_VERSION,
            }),
        "the card names the format it found: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );

    // (b) the verb: one dispatch, no confirm sheet
    let outcome = drive(
        studio.dispatch(deploy_action(DeployOp::UpgradeDeviceProject {
            target: studio.device_target_for_test(),
        })),
    )
    .expect("the upgrade succeeds");
    assert!(
        outcome.notices.iter().any(
            |notice| notice.message.contains("Upgraded") && notice.message.contains("format 4")
        ),
        "the upgrade says what it did: {:?}",
        outcome.notices
    );

    // The migrated bytes were born in the LIBRARY…
    let handle = store.open(summary.uid).unwrap();
    let manifest = handle
        .read_all_files()
        .unwrap()
        .into_iter()
        .find(|(path, _)| path == "project.json")
        .map(|(_, bytes)| String::from_utf8_lossy(&bytes).to_string())
        .expect("a manifest");
    assert!(
        manifest.contains(&format!(
            "\"format\": {}",
            lpc_model::PROJECT_FORMAT_VERSION
        )),
        "the library copy is current now: {manifest}"
    );
    // …and the pre-upgrade version is still there. That is what makes the
    // verb safe to dispatch without a confirm gate.
    assert!(
        handle.history.events().len() > 1,
        "the upgrade left history to fall back on"
    );

    // …and travelled back over the ordinary hash-checked push: the board
    // now holds exactly the library head.
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::AtHead,
                ..
            })
        ),
        "the board settles at head, got {:?}",
        studio.device_sync_for_test().map(|sync| &sync.content)
    );
    let home = studio.view().home.expect("gallery view");
    assert!(
        home.devices
            .iter()
            .any(|card| matches!(card.state, RosterCardState::RunningUpToDate)),
        "the card settled to Running-up-to-date: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
}

/// The board is stale but the LIBRARY is not — the user already opened
/// the project in the editor, which migrated it (P3). There is nothing to
/// migrate, so the verb is just the push, and it says so rather than
/// claiming an upgrade it did not perform.
///
/// Also the guard on a real trap: sending the migrate transaction anyway
/// would take the project lock and refuse for a project open in this very
/// tab — an upgrade failing because the project is open is exactly the
/// kind of nonsense the format work exists to end.
#[test]
fn a_stale_board_whose_library_copy_is_current_is_simply_pushed() {
    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let head_files = store.open(summary.uid).unwrap().read_all_files().unwrap();
    // the board still holds what it was pushed BEFORE the format bump:
    // same project uid, older manifest
    let stale_files: Vec<(String, Vec<u8>)> = head_files
        .iter()
        .map(|(path, bytes)| {
            if path != "project.json" {
                return (path.clone(), bytes.clone());
            }
            let mut manifest: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            manifest["format"] = serde_json::json!(4);
            (path.clone(), serde_json::to_vec_pretty(&manifest).unwrap())
        })
        .collect();

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(stale_files)
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::OldFormat { .. })
        ),
        "the board's format owns its card, got {:?}",
        studio.device_sync_for_test().map(|sync| &sync.content)
    );

    let head_before = store.open(summary.uid).unwrap().history.head();
    let outcome = drive(
        studio.dispatch(deploy_action(DeployOp::UpgradeDeviceProject {
            target: studio.device_target_for_test(),
        })),
    )
    .expect("the verb succeeds");
    assert!(
        outcome
            .notices
            .iter()
            .any(|notice| notice.message.contains("already upgraded")),
        "no upgrade was needed — say what actually happened: {:?}",
        outcome.notices
    );
    assert_eq!(
        store.open(summary.uid).unwrap().history.head(),
        head_before,
        "the library's line did not move: the board's old copy must never \
         become the head behind the user's back"
    );
    assert!(
        matches!(
            studio.device_sync_for_test().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::AtHead,
                ..
            })
        ),
        "the board runs the library head now, got {:?}",
        studio.device_sync_for_test().map(|sync| &sync.content)
    );
}

/// The other half of P5's honesty: a board below the upgrade floor gets
/// the same clear card and NO upgrade button — the migration chain does
/// not reach it, and a verb that refuses the moment it is pressed is worse
/// than the honest way out. A stray dispatch says why.
#[test]
fn a_board_below_the_upgrade_floor_is_named_but_not_offered_an_upgrade() {
    use crate::app::roster::{DeviceFormatStanding, RosterAffordance, RosterCardState};

    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(project_files_at_format(2, "ancient"))
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("the pull landed");
    assert!(
        matches!(
            &sync.content,
            DeviceContent::OldFormat {
                class: lpa_upgrade::FormatClass::BelowFloor { found: Some(2) },
                ..
            }
        ),
        "got {:?}",
        sync.content
    );
    let home = studio.view().home.expect("gallery view");
    let card = home
        .devices
        .iter()
        .find(|card| {
            matches!(
                card.state,
                RosterCardState::HoldsOldFormatProject {
                    standing: DeviceFormatStanding::TooOld { found: Some(2) },
                    ..
                }
            )
        })
        .expect("the below-floor card");
    assert_eq!(
        card.state.affordance(),
        Some(RosterAffordance::WipeProject),
        "no upgrade path — the way out is the offer"
    );

    let error = drive(
        studio.dispatch(deploy_action(DeployOp::UpgradeDeviceProject {
            target: studio.device_target_for_test(),
        })),
    )
    .expect_err("a stray dispatch refuses");
    let message = error.to_string();
    assert!(
        message.contains('2') && message.contains("too old"),
        "the refusal names what was found: {message}"
    );
}

/// Regression (2026-07-26 hardware walk): a push to the LIVE device
/// smeared its "Pushing…" overlay onto a REMEMBERED (unplugged) device's
/// card too — the overlay's session-op fallback matched any card with a
/// uid, and an offline card carries one. The session op must narrate only
/// on the session's own card (stamped identity == card uid).
#[test]
fn push_progress_stays_on_the_live_card() {
    use crate::app::places::{DeviceRegistry, RegisteredDevice};
    use crate::{UxUpdate, UxUpdateSink};

    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let v1_files = store.open(summary.uid).unwrap().read_all_files().unwrap();
    {
        use lpc_model::AsLpPath;
        let mut handle = store.open(summary.uid).unwrap();
        handle
            .apply_update("/shader.glsl".as_path(), Some(b"v2"))
            .unwrap();
        handle.record_save(2.0).unwrap().expect("head advanced");
    }
    // The FIRST board: connected earlier, unplugged, remembered — its
    // offline card still carries the stamped uid.
    let registry = DeviceRegistry::new(store.fs_handle());
    registry
        .upsert(RegisteredDevice {
            uid: "dev_bbbbbbbbbbbbbbbb".to_string(),
            name: "First board".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: None,
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        })
        .unwrap();

    // The SECOND board: live, holding v1 → classifies Behind → push.
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(v1_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    // Mid-push, record which cards wear the in-place op overlay.
    let seen: Rc<RefCell<Vec<(Option<String>, bool)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink_seen = Rc::clone(&seen);
    let sink = UxUpdateSink::new(move |update| {
        if let UxUpdate::View(view) = update
            && let Some(home) = &view.home
        {
            for card in &home.devices {
                sink_seen
                    .borrow_mut()
                    .push((card.uid.clone(), card.ui.op.is_some()));
            }
        }
    });
    drive(studio.dispatch_with_updates(
        deploy_action(DeployOp::PushProject {
            target: studio.device_target_for_test(),
            key: summary.uid.to_string(),
        }),
        sink,
    ))
    .expect("the push succeeds");

    let seen = seen.borrow();
    assert!(
        seen.iter()
            .any(|(uid, has_op)| uid.as_deref() == Some("dev_aaaaaaaaaaaaaaaa") && *has_op),
        "the live card narrates its own push: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|(uid, has_op)| uid.as_deref() == Some("dev_bbbbbbbbbbbbbbbb") && *has_op),
        "the remembered offline card must never wear the live push op: {seen:?}"
    );
}

/// The auto-fast-forward fixture (§3c-1): "Porch" installed, a device
/// remembered with a push association at the given version, and the fake
/// board holding an EDITED copy of those files. Returns the studio (with
/// library attached, not yet connected), the endpoint, and the project uid.
fn diverged_board_fixture(
    local_moves_after_push: bool,
) -> (StudioController, FakeEsp32Device, LinkEndpointId, String) {
    use crate::app::places::{DeviceRegistry, RegisteredDevice};

    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let pushed_head = store.open(summary.uid).unwrap().history.head().unwrap();
    let board_files: Vec<(String, Vec<u8>)> = store
        .open(summary.uid)
        .unwrap()
        .read_all_files()
        .unwrap()
        .into_iter()
        .map(|(path, bytes)| {
            if path.contains("shader.glsl") {
                (path, b"edited on the board".to_vec())
            } else {
                (path, bytes)
            }
        })
        .collect();
    // the push marker: what WE last pushed to this board is v1
    DeviceRegistry::new(store.fs_handle())
        .upsert(RegisteredDevice {
            uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
            name: "Bench board".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: Some(lpc_history::DeviceAssociation {
                device: "dev_aaaaaaaaaaaaaaaa".parse().unwrap(),
                project: summary.uid,
                version: pushed_head,
                at: 1.0,
            }),
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        })
        .unwrap();
    if local_moves_after_push {
        use lpc_model::AsLpPath;
        let mut handle = store.open(summary.uid).unwrap();
        handle
            .apply_update("/shader.glsl".as_path(), Some(b"local v2"))
            .unwrap();
        handle.record_save(2.0).unwrap().expect("head advanced");
    }

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(board_files)
            .with_loaded_project()
            .with_identity(FakeDeviceIdentity::new(
                "dev_aaaaaaaaaaaaaaaa",
                "Bench board",
            )),
    ));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    (studio, device, endpoint_id, summary.uid.to_string())
}

/// §3c-1 happy path: the board's copy diverges but the push marker still
/// equals the local head (nothing local happened since the push) — the
/// connect adopts the board's edits automatically; no Edited-on-device
/// decision, the card lands Running-up-to-date.
#[test]
fn connect_auto_fast_forwards_a_pure_board_extension() {
    let (mut studio, _device, endpoint_id, _uid) = diverged_board_fixture(false);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    let sync = studio.device_sync_for_test().expect("device state cached");
    let DeviceContent::Known { relation, .. } = &sync.content else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(
        *relation,
        lpc_history::SyncRelation::AtHead,
        "the pure extension fast-forwarded without asking"
    );
    let view = studio.view();
    let home = view.home.expect("gallery view");
    assert!(
        home.devices
            .iter()
            .any(|card| matches!(card.state, crate::RosterCardState::RunningUpToDate)),
        "the card landed Running-up-to-date: {:?}",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
}

/// §3c-1 counter-case: local ALSO moved since the push — a genuine fork.
/// No auto-adopt; the card still asks (Edited-on-device).
#[test]
fn connect_keeps_a_true_fork_for_the_user() {
    let (mut studio, _device, endpoint_id, _uid) = diverged_board_fixture(true);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    let sync = studio.device_sync_for_test().expect("device state cached");
    let DeviceContent::Known { relation, .. } = &sync.content else {
        panic!("known project classifies, got {:?}", sync.content);
    };
    assert_eq!(
        *relation,
        lpc_history::SyncRelation::Diverged,
        "a genuine fork must never auto-resolve"
    );
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The P3 coexistence fixture: "Porch" installed on a fake DEVICE
/// (connected through the real link, project idle on flash) and "Sign"
/// (the edit-e2e node graph, so a slot exists to edit) OPEN on a stub SIM
/// session speaking to an in-process server. Returns the sim session's
/// id; the lens is on the sim.
fn coexisting_sim_and_device() -> (StudioController, FakeEsp32Device, crate::RuntimeId) {
    coexisting_fixture(false)
}

/// [`coexisting_sim_and_device`], with the device BOOTED INTO its project
/// (loaded and running) so the D29 connect has a running project to
/// attach to.
fn coexisting_sim_and_device_running() -> (StudioController, FakeEsp32Device, crate::RuntimeId) {
    coexisting_fixture(true)
}

fn coexisting_fixture(
    device_project_loaded: bool,
) -> (StudioController, FakeEsp32Device, crate::RuntimeId) {
    use super::studio_edit_e2e_tests::{InProcessServerIo, edit_e2e_files, edit_e2e_server};
    use crate::app::home::HOME_NODE_ID;
    use crate::{HomeOp, StudioServerClient};
    use std::collections::VecDeque;

    let (store, host) = library();
    let porch = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let porch_files = store.open(porch.uid).unwrap().read_all_files().unwrap();
    let sign = store
        .install_package(
            "Sign",
            &edit_e2e_files()
                .iter()
                .map(|(name, body)| (name.to_string(), body.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();

    let mut device_state = FakeLightPlayerState::new()
        .with_project_files(porch_files)
        .with_identity(FakeDeviceIdentity::new(
            "dev_aaaaaaaaaaaaaaaa",
            "Bench board",
        ));
    if device_project_loaded {
        device_state = device_state.with_loaded_project();
    }
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(device_state));
    let (mut studio, device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("device connect succeeds");

    let sim_io = InProcessServerIo {
        server: Rc::new(RefCell::new(edit_e2e_server())),
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::new(RefCell::new(Vec::new())),
    };
    let sim_id = studio.install_stub_sim_with_client_for_test(
        StudioServerClient::from_io_for_test("in-process", Box::new(sim_io)),
    );
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: sign.uid.to_string(),
        },
    )))
    .expect("open on the sim succeeds");
    (studio, device, sim_id)
}

/// A studio whose link registry holds one fake provider with one scripted
/// device endpoint. Returns the device handle for injection/assertions.
/// M6, the whole point: a device's storage comes off as a ZIP through the
/// REAL provider path — read raw, mounted in-process, archived — and the
/// archive holds the files the board actually has.
///
/// The fake answers the raw read with a genuine littlefs image, so this
/// exercises every step the hardware path takes except the serial bytes:
/// dispatch, capability gate, mount, walk, manifest, zip, and the seq-gated
/// hand-off to the shell.
#[test]
fn backing_up_a_device_publishes_a_zip_of_its_files() {
    use std::io::Read;

    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(vec![
                // Post-mitosis shape: the container manifest carries the
                // format/name, the root MODULE carries the node map. A
                // pre-mitosis single `project.json` is refused outright
                // (D-A), which would leave the device unidentified here.
                (
                    "project.json".to_string(),
                    br#"{"format":5,"name":"sign"}"#.to_vec(),
                ),
                (
                    "module.json".to_string(),
                    br#"{"kind":"Module","nodes":{}}"#.to_vec(),
                ),
                ("shader.glsl".to_string(), b"void main() {}".to_vec()),
            ])
            .with_identity(FakeDeviceIdentity::new(
                "dev_bbbbbbbbbbbbbbbb",
                "Bench board",
            )),
    ));
    let (_store, host) = library();
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");
    drive(studio.dispatch(device_action(DeviceOp::BackUpFilesystem {
        target: studio.device_target_for_test(),
    })))
    .expect("the backup succeeds");

    let backup = studio
        .view()
        .home
        .expect("the gallery is showing")
        .backup
        .expect("a finished backup rides the view");
    assert_eq!(backup.seq, 1, "the first backup of the session");
    assert!(
        backup
            .file_name
            .starts_with("lightplayer-backup-bench-board-"),
        "the file names itself after the device: {}",
        backup.file_name
    );

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(backup.bytes.as_ref())).expect("a zip archive");
    let names: Vec<String> = (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect();
    assert!(
        names.contains(&"manifest.json".to_string()),
        "the manifest is at the archive root: {names:?}"
    );
    assert!(
        names.contains(&"files/projects/studio/shader.glsl".to_string()),
        "device paths mirror verbatim under files/: {names:?}"
    );
    let mut shader = String::new();
    archive
        .by_name("files/projects/studio/shader.glsl")
        .expect("the shader entry")
        .read_to_string(&mut shader)
        .expect("shader bytes");
    assert_eq!(shader, "void main() {}");

    // The identity hazard M7 has to detect: the captured uid is recorded, so
    // a restore can tell it is about to clone a device.
    let mut manifest_json = String::new();
    archive
        .by_name("manifest.json")
        .expect("the manifest entry")
        .read_to_string(&mut manifest_json)
        .expect("manifest bytes");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).expect("the manifest parses");
    assert_eq!(manifest["deviceUid"], "dev_bbbbbbbbbbbbbbbb");
    assert_eq!(manifest["formatVersion"], 1);
    assert_eq!(manifest["partitionOffset"], 0x0031_0000);
}

/// Two boards attached at once (multi-device M3): the roster renders one
/// card per live session, keyed distinctly even while BOTH are anonymous —
/// the exact shape that used to erase the second board (its name was its
/// render key and both were "Connected device"; 2026-08-02 walk).
///
/// This helper + test pair is the roadmap's agentic substitute for
/// physically plugging in two boards. M4's op-targeting regressions build
/// on it; M5's connect-flow ones will.
#[test]
fn two_fake_boards_render_two_cards_with_distinct_keys() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new())),
        FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new())),
    );
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &first_id).expect("first board connects");
    connect_through_link(&mut studio, &second_id).expect("second board connects");

    let home = studio.view().home.expect("no project open — gallery shows");
    let boards: Vec<_> = home.devices.iter().filter(|card| !card.sim).collect();
    assert_eq!(
        boards.len(),
        2,
        "both live boards render (states: {:?})",
        home.devices
            .iter()
            .map(|card| card.state.clone())
            .collect::<Vec<_>>()
    );
    assert_ne!(
        boards[0].render_key(),
        boards[1].render_key(),
        "anonymous boards key by session, never by name"
    );
    // Both unstamped empty boards reached the post-pull state — the second
    // session is fully live, not a stub of the first.
    for card in &boards {
        assert_eq!(
            card.state,
            crate::RosterCardState::NeedsAName,
            "an unstamped empty board asks for a name"
        );
    }
}

/// The Danger-zone disconnect targets ONE board (gate-1 sitting feedback,
/// 2026-08-03: "I can't disconnect a device from the UI"): closing card B's
/// session leaves board A attached — the pre-M3 op took every session down,
/// sim included.
#[test]
fn disconnecting_one_board_leaves_the_other_attached() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new())),
        FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new())),
    );
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &first_id).expect("first board connects");
    connect_through_link(&mut studio, &second_id).expect("second board connects");

    let home = studio.view().home.expect("gallery shows");
    let keys: Vec<String> = home
        .devices
        .iter()
        .filter(|card| !card.sim)
        .filter_map(|card| card.session_key.clone())
        .collect();
    assert_eq!(keys.len(), 2, "two live boards to start");

    drive(studio.dispatch(device_action(DeviceOp::DisconnectDevice {
        target: crate::DeviceTarget::card(keys[1].clone()),
    })))
    .expect("disconnect dispatches");

    let home = studio.view().home.expect("gallery still shows");
    let remaining: Vec<String> = home
        .devices
        .iter()
        .filter(|card| !card.sim)
        .filter_map(|card| card.session_key.clone())
        .collect();
    assert_eq!(
        remaining,
        vec![keys[0].clone()],
        "board A is untouched; only board B's card left"
    );
}

/// A card-owned op flow belongs to the SESSION it runs on (M4 P2).
///
/// Before this, `takes_card_op` matched "any uid-less live card" and
/// `op_in_flight` was stamped on the OLDEST board's evidence — so one
/// flash narrated on BOTH blank boards, and the card that lost its
/// Danger affordance to `OperationInFlight` was not necessarily the one
/// being flashed ("the duplicate card missing its Danger tab").
#[test]
fn a_flash_narrates_on_its_own_board_and_leaves_the_other_alone() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::BlankFlash),
        FakeDeviceScript::new(FakeBootState::BlankFlash),
    );
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &first_id).expect("first blank board connects");
    connect_through_link(&mut studio, &second_id).expect("second blank board connects");

    let home = studio.view().home.expect("gallery shows");
    let keys: Vec<String> = home
        .devices
        .iter()
        .filter(|card| !card.sim)
        .filter_map(|card| card.session_key.clone())
        .collect();
    assert_eq!(keys.len(), 2, "two blank boards to start");

    // Aimed at the board `device_target_for_test` resolves; what this
    // test pins is where the op NARRATES, not where it runs (that is
    // `a_flash_aimed_at_the_second_board_leaves_the_first_untouched`).
    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: studio.device_target_for_test(),
        setup_name: None,
        board_id: None,
    })))
    .expect("flash dispatches");

    let home = studio.view().home.expect("gallery still shows");
    let narrating: Vec<String> = home
        .devices
        .iter()
        .filter(|card| !card.sim && card.ui.op.is_some())
        .filter_map(|card| card.session_key.clone())
        .collect();
    assert!(
        narrating.len() <= 1,
        "one flash must narrate on at most one card, got {narrating:?}"
    );
    if let Some(narrating) = narrating.first() {
        assert_eq!(
            *narrating, keys[0],
            "the op narrates on the board it ran on"
        );
    }

    // And the OTHER board keeps a card that is not mid-operation — the
    // state whose empty Danger section costs a card its Danger tab.
    let other = home
        .devices
        .iter()
        .find(|card| card.session_key.as_deref() == Some(keys[1].as_str()))
        .expect("the second board still has a card");
    assert!(
        !matches!(
            other.state,
            crate::RosterCardState::OperationInFlight { .. }
        ),
        "the board nobody flashed is not mid-operation: {:?}",
        other.state
    );
}

/// THE test this milestone exists for: an operation runs on the board
/// the user clicked, not on whichever one attached first (M4 P3).
///
/// Before this, every device op resolved "the" device — the OLDEST
/// device session — so with two boards attached, clicking flash on the
/// second card flashed the first.
#[test]
fn a_flash_aimed_at_the_second_board_leaves_the_first_untouched() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::BlankFlash),
        FakeDeviceScript::new(FakeBootState::BlankFlash),
    );
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &first_id).expect("first board connects");
    connect_through_link(&mut studio, &second_id).expect("second board connects");

    let state_of = |studio: &StudioController, key: &str| {
        studio
            .view()
            .home
            .expect("gallery shows")
            .devices
            .iter()
            .find(|card| card.session_key.as_deref() == Some(key))
            .map(|card| card.state.clone())
    };
    let keys: Vec<String> = studio
        .view()
        .home
        .expect("gallery shows")
        .devices
        .iter()
        .filter(|card| !card.sim)
        .filter_map(|card| card.session_key.clone())
        .collect();
    assert_eq!(keys.len(), 2, "two boards to start");
    let first_before = state_of(&studio, &keys[0]);
    let second_before = state_of(&studio, &keys[1]);

    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: crate::DeviceTarget::card(keys[1].clone()),
        setup_name: None,
        board_id: None,
    })))
    .expect("flashing the SECOND board dispatches");

    assert_eq!(
        state_of(&studio, &keys[0]),
        first_before,
        "board A is exactly as it was — it is not the board the user clicked"
    );
    assert_ne!(
        state_of(&studio, &keys[1]),
        second_before,
        "board B took the flash and left the unflashed state"
    );
}

/// An operation whose target names no live session REFUSES. It must not
/// quietly land on some other board — that fallback is the whole defect.
#[test]
fn an_op_aimed_at_no_live_board_refuses_instead_of_picking_one() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, _second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::BlankFlash),
        FakeDeviceScript::new(FakeBootState::BlankFlash),
    );
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &first_id).expect("first board connects");

    let card_state = |studio: &StudioController| {
        studio
            .view()
            .home
            .expect("gallery shows")
            .devices
            .iter()
            .find(|card| !card.sim)
            .map(|card| card.state.clone())
    };
    let before = card_state(&studio);

    let outcome = drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: crate::DeviceTarget::card("dev_never_attached"),
        setup_name: None,
        board_id: None,
    })));

    assert!(
        matches!(outcome, Err(crate::UiError::MissingSession(ref message))
            if message.contains("dev_never_attached")),
        "the refusal names the card that could not be resolved: {outcome:?}"
    );
    assert_eq!(
        card_state(&studio),
        before,
        "the attached board must NOT absorb an op aimed elsewhere"
    );
}

// ---------------------------------------------------------------------
// Device identity anchored in silicon (design §3/§4): the connect path
// resolves A1–A4, migrates legacy rows at first sight, and refuses to
// let two boards share one identity.
// ---------------------------------------------------------------------

/// A1, end to end: a board that reports its efuse MAC is identified by
/// SILICON — no stamp, no file — and its uid is the deterministic
/// derivation of that MAC.
///
/// And the rule that keeps the registry honest (design §4 step 4): being
/// seen is not being remembered. A board nobody has adopted, provisioned,
/// or named registers NOTHING; the card carries the whole story.
#[test]
fn a_mac_only_board_derives_its_uid_and_registers_nothing() {
    let (store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_base_mac(BENCH_MAC),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("pull landed");
    let identity = sync.identity.as_ref().expect("silicon is an identity");
    assert_eq!(identity.uid, derived_uid(BENCH_MAC));
    assert_eq!(identity.name, "", "a MAC names nothing — the user does");
    assert!(
        registry(&store).is_empty(),
        "a sighting alone never registers a board: {:?}",
        registry(&store)
    );
    // and the naming flow is still insisted on: identity ≠ named
    assert!(
        studio
            .view()
            .home
            .expect("gallery shows")
            .devices
            .iter()
            .any(|card| card.state == crate::RosterCardState::NeedsAName),
        "an unnamed MAC board still asks for a name"
    );
}

/// The migration (design §4 steps 1–2): a board remembered under the uid
/// a stamp gave it shows up carrying BOTH that stamp and its MAC. The row
/// moves to the derived uid at first sight, keeping its name, its board,
/// and its association — and recording where it came from so old history
/// events still resolve.
#[test]
fn a_stamped_board_rekeys_its_legacy_registry_row_at_first_sight() {
    let (store, host) = library();
    seed_registry(
        &store,
        crate::app::places::RegisteredDevice {
            uid: STAMPED_UID.to_string(),
            name: "Luna's porch sign".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 10.0,
            association: None,
            board_id: Some("esp32-c6-devkit".to_string()),
            hardware_id: None,
            previous_uids: Vec::new(),
        },
    );
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_MAC)
            .with_identity(FakeDeviceIdentity::new(STAMPED_UID, "Bench board")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("pull landed");
    let identity = sync.identity.as_ref().expect("identified");
    assert_eq!(
        identity.uid,
        derived_uid(BENCH_MAC),
        "silicon outranks the stamp"
    );
    assert_eq!(
        identity.name, "Luna's porch sign",
        "D34: the registry names the device, not the device's file"
    );

    let rows = registry(&store);
    assert_eq!(rows.len(), 1, "moved, not duplicated: {rows:?}");
    assert_eq!(rows[0].uid, derived_uid(BENCH_MAC));
    assert_eq!(rows[0].name, "Luna's porch sign");
    assert_eq!(rows[0].board_id.as_deref(), Some("esp32-c6-devkit"));
    assert_eq!(
        rows[0].previous_uids,
        vec![STAMPED_UID.to_string()],
        "the old uid is kept so old history events still resolve"
    );
    assert_eq!(
        rows[0].hardware_id.as_deref(),
        Some(format!("efuse:{BENCH_MAC}").as_str())
    );
}

/// Design §4 step 3: the board was sighted under BOTH schemes (a stamped
/// row from before, a derived row from a studio that already saw its
/// MAC). The rows merge into the derived key rather than one shadowing
/// the other.
#[test]
fn rows_under_both_uids_merge_into_the_derived_one() {
    let (store, host) = library();
    seed_registry(
        &store,
        crate::app::places::RegisteredDevice {
            uid: STAMPED_UID.to_string(),
            name: "Luna's porch sign".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 10.0,
            association: None,
            board_id: Some("esp32-c6-devkit".to_string()),
            hardware_id: None,
            previous_uids: Vec::new(),
        },
    );
    seed_registry(
        &store,
        crate::app::places::RegisteredDevice {
            uid: derived_uid(BENCH_MAC),
            name: String::new(),
            transport: String::new(),
            last_seen_at: 20.0,
            association: None,
            board_id: None,
            hardware_id: Some(format!("efuse:{BENCH_MAC}")),
            previous_uids: Vec::new(),
        },
    );
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_MAC)
            .with_identity(FakeDeviceIdentity::new(STAMPED_UID, "Bench board")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let rows = registry(&store);
    assert_eq!(rows.len(), 1, "one board, one row: {rows:?}");
    assert_eq!(rows[0].uid, derived_uid(BENCH_MAC));
    assert_eq!(
        rows[0].name, "Luna's porch sign",
        "the named row's name survives the merge"
    );
    assert_eq!(rows[0].board_id.as_deref(), Some("esp32-c6-devkit"));
    assert_eq!(rows[0].previous_uids, vec![STAMPED_UID.to_string()]);
}

/// A3/D6, unchanged: pre-hello-MAC firmware (and host-class embedders)
/// keep the stamped uid as their identity. Nothing is re-keyed, and the
/// row records that its identity was minted rather than read.
#[test]
fn a_board_with_no_mac_keeps_its_stamped_identity() {
    let (store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(STAMPED_UID, "Bench board")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("pull landed");
    let identity = sync.identity.as_ref().expect("the stamp is the identity");
    assert_eq!(identity.uid, STAMPED_UID);
    assert_eq!(identity.name, "Bench board");

    let rows = registry(&store);
    assert_eq!(rows.len(), 1, "a stamped board still registers on sight");
    assert_eq!(rows[0].uid, STAMPED_UID);
    assert_eq!(rows[0].hardware_id.as_deref(), Some("minted"));
    assert!(rows[0].previous_uids.is_empty(), "nothing was re-keyed");
}

/// A failed efuse read reports all-zeroes. Trusting it would hand every
/// board whose read failed the SAME identity, so it is treated as no MAC
/// at all and the board falls back to its stamp (A3).
#[test]
fn an_all_zero_mac_is_treated_as_no_mac() {
    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac("00:00:00:00:00:00")
            .with_identity(FakeDeviceIdentity::new(STAMPED_UID, "Bench board")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("pull landed");
    assert_eq!(
        sync.identity.as_ref().map(|identity| identity.uid.as_str()),
        Some(STAMPED_UID),
        "a failed read is evidence of nothing"
    );
}

/// Unknown content on a MAC-identified board ADOPTS at connect (design
/// §5): the uid exists from the first hello, so there is nothing left for
/// `PendingIdentity` to wait for. Adoption is also the event that earns
/// the board its registry row.
#[test]
fn unknown_content_on_a_mac_board_adopts_instead_of_pending_identity() {
    let (store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_MAC)
            .with_project_files(project_files("wild")),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let sync = studio.device_sync_for_test().expect("pull landed");
    assert!(
        matches!(&sync.content, DeviceContent::Adopted { .. }),
        "a MAC board's unknown project adopts, got {:?}",
        sync.content
    );
    let rows = registry(&store);
    assert_eq!(rows.len(), 1, "adoption remembers the board: {rows:?}");
    assert_eq!(rows[0].uid, derived_uid(BENCH_MAC));
    assert_eq!(
        rows[0].hardware_id.as_deref(),
        Some(format!("efuse:{BENCH_MAC}").as_str())
    );
}

/// The product rule the uid must not quietly repeal: a MAC board HAS an
/// identity from its first hello, but it has no NAME, so the flow still
/// gently insists on one — the card asks, a push refuses until it is
/// answered, and the answer lands on the derived uid's row.
#[test]
fn a_mac_board_is_still_pushed_through_the_naming_flow() {
    use crate::HomeOp;
    use crate::app::home::HOME_NODE_ID;

    let (store, host) = library();
    let summary = store
        .install_package(
            "Porch",
            &project_files("v1"),
            PackageProvenance::Created,
            1.0,
        )
        .unwrap();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_base_mac(BENCH_MAC),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let refused = drive(studio.dispatch(deploy_action(DeployOp::PushProject {
        target: studio.device_target_for_test(),
        key: summary.uid.to_string(),
    })));
    assert!(
        matches!(refused, Err(crate::UiError::MissingSession(ref message))
            if message.contains("no named device")),
        "an unnamed board is not pushable, uid or no uid: {refused:?}"
    );

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::NameDevice {
            target: studio.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .expect("naming dispatches");

    let sync = studio
        .device_sync_for_test()
        .expect("re-pulled after naming");
    let identity = sync.identity.as_ref().expect("identified");
    assert_eq!(
        identity.uid,
        derived_uid(BENCH_MAC),
        "the name lands on the SILICON's uid, not the one the stamp minted"
    );
    assert_eq!(identity.name, "Luna's porch sign");
    let rows = registry(&store);
    assert_eq!(rows.len(), 1, "one board, one row: {rows:?}");
    assert_eq!(rows[0].uid, derived_uid(BENCH_MAC));
    assert_eq!(rows[0].name, "Luna's porch sign");

    drive(studio.dispatch(deploy_action(DeployOp::PushProject {
        target: studio.device_target_for_test(),
        key: summary.uid.to_string(),
    })))
    .expect("a named board pushes");
}

/// Erase amnesia, cured (design §6): erasing a named board wipes its
/// projects and leaves the BOARD remembered. The registry row is not
/// forgotten, and when the board comes back from a re-flash — same
/// silicon, empty filesystem, nothing stamped anywhere — it lands on that
/// same row, under its own name, without asking to be named again.
///
/// The old scheme could not do this: the erase took `/.lp/device.json`
/// with it and the board reconnected as a stranger.
#[test]
fn erasing_a_board_keeps_it_remembered_and_it_comes_back_as_itself() {
    use crate::HomeOp;
    use crate::app::home::HOME_NODE_ID;

    let (store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_base_mac(BENCH_MAC),
    ));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::NameDevice {
            target: studio.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .expect("naming dispatches");

    drive(studio.dispatch(deploy_action(DeployOp::EraseDevice {
        target: studio.device_target_for_test(),
    })))
    .expect("erase from the card is a success");
    assert!(
        matches!(
            studio.device_state_for_test(),
            Some(DeviceState::BlankFlash)
        ),
        "the erase landed: {:?}",
        studio.device_state_for_test()
    );
    let rows = registry(&store);
    assert_eq!(
        rows.len(),
        1,
        "an erase wipes the board's projects, not our memory of it: {rows:?}"
    );
    assert_eq!(rows[0].name, "Luna's porch sign");

    // Back from a re-flash: fresh firmware, empty filesystem — and the
    // same efuse MAC, because nothing a flash tool does can change it.
    drive(studio.dispatch(device_action(DeviceOp::ProvisionFirmware {
        target: studio.device_target_for_test(),
        setup_name: None,
        board_id: None,
    })))
    .expect("re-flash succeeds");

    let sync = studio
        .device_sync_for_test()
        .expect("re-pulled after flash");
    let identity = sync.identity.as_ref().expect("identified again");
    assert_eq!(
        identity.uid,
        derived_uid(BENCH_MAC),
        "the same board derives the same uid after an erase"
    );
    assert_eq!(
        identity.name, "Luna's porch sign",
        "it comes back under the name it was given"
    );
    assert_eq!(registry(&store).len(), 1, "no stranger row was created");
    let states: Vec<_> = studio
        .view()
        .home
        .expect("gallery shows")
        .devices
        .iter()
        .filter(|card| !card.sim)
        .map(|card| card.state.clone())
        .collect();
    assert!(
        !states.contains(&crate::RosterCardState::NeedsAName),
        "a remembered board is never asked for its name again: {states:?}"
    );
}

/// D5 (clones): two live boards reporting one MAC. The newcomer stays
/// ANONYMOUS — two cards sharing an `identity_key()` is a duplicate key
/// in a Dioxus keyed list, which panics (the 2026-07-15 crash class) —
/// and the console says plainly what happened.
#[test]
fn a_second_board_with_the_same_mac_stays_anonymous_and_warns() {
    let (_store, host) = library();
    let (mut studio, _devices, first_id, second_id) = studio_with_two_fake_devices(
        FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new().with_base_mac(BENCH_MAC),
        )),
        FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new().with_base_mac(BENCH_MAC),
        )),
    );
    studio.attach_library(host);
    drive(studio.settle_library());

    connect_through_link(&mut studio, &first_id).expect("first board connects");
    connect_through_link(&mut studio, &second_id).expect("second board connects");

    let home = studio.view().home.expect("gallery shows");
    let boards: Vec<_> = home.devices.iter().filter(|card| !card.sim).collect();
    assert_eq!(boards.len(), 2, "both boards still render");
    let uids: Vec<Option<&str>> = boards.iter().map(|card| card.uid.as_deref()).collect();
    assert!(
        uids.contains(&Some(derived_uid(BENCH_MAC).as_str())),
        "the first board keeps the identity: {uids:?}"
    );
    assert!(
        uids.contains(&None),
        "the clone stays anonymous rather than sharing a key: {uids:?}"
    );
    assert_ne!(
        boards[0].render_key(),
        boards[1].render_key(),
        "two live cards must never share a render key"
    );
    let logs = studio.logs();
    assert!(
        logs.iter().any(|entry| {
            entry.level == crate::UiLogLevel::Warn && entry.message.contains("same hardware id")
        }),
        "the duplicate is reported, not swallowed: {:?}",
        logs.iter().map(|entry| &entry.message).collect::<Vec<_>>()
    );
}

/// Design §6: card UI state survives the anonymous → identified key flip.
///
/// A card keys by its session while the board is anonymous and by its
/// `dev_…` uid the moment identity resolves, so state built on the
/// anonymous card orphans at the flip — the same 2026-08-02 wart
/// `migrate_card_op` carries op flows across. Naming an anonymous board
/// is the flip that is easiest to drive; a blank board's first hello
/// after a flash takes exactly the same path.
#[test]
fn card_ui_state_survives_the_identity_key_flip() {
    use crate::app::home::HOME_NODE_ID;
    use crate::{CardUiOp, DeviceCardTab, HomeOp};

    let (_store, host) = library();
    let script = FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()));
    let (mut studio, _device, endpoint_id) = studio_with_fake_device(script);
    studio.attach_library(host);
    drive(studio.settle_library());
    connect_through_link(&mut studio, &endpoint_id).expect("connect succeeds");

    let card_key = device_card_key(&studio);
    assert!(
        card_key.starts_with("runtime-"),
        "the board is anonymous to start: {card_key}"
    );
    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::CardUi(CardUiOp::SelectTab {
            card: card_key.clone(),
            tab: DeviceCardTab::Danger,
        }),
    )))
    .expect("tab select dispatches");

    drive(studio.dispatch(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::NameDevice {
            target: studio.device_target_for_test(),
            name: "Luna's porch sign".to_string(),
        },
    )))
    .expect("naming dispatches");

    let flipped = device_card_key(&studio);
    assert!(
        flipped.starts_with("dev_"),
        "identity arrived and the key flipped: {flipped}"
    );
    let view = studio.view();
    let card = view
        .home
        .as_ref()
        .expect("gallery shows")
        .devices
        .iter()
        .find(|card| !card.sim)
        .expect("the board's card");
    assert_eq!(
        card.ui.tab,
        DeviceCardTab::Danger,
        "the open tab followed the card across the flip"
    );
}

/// The base MAC the identity fakes report — a plausible Espressif OUI, in
/// the canonical lowercase spelling the hello uses.
const BENCH_MAC: &str = "60:55:f9:0a:0b:0c";

/// The `dev_` uid a legacy stamp gave the same board.
const STAMPED_UID: &str = "dev_aaaaaaaaaaaaaaaa";

/// The uid `mac` derives to — computed through the production derivation
/// so the tests assert the RELATIONSHIP, never a hand-copied string (the
/// derivation itself is pinned by a golden in `hardware_id.rs`).
fn derived_uid(mac: &str) -> String {
    crate::app::places::HardwareId::from_base_mac(mac)
        .expect("a well-formed MAC")
        .device_uid()
        .to_string()
}

fn registry(store: &LibraryStore) -> Vec<crate::app::places::RegisteredDevice> {
    crate::app::places::DeviceRegistry::new(store.fs_handle())
        .list()
        .expect("the registry reads")
}

fn seed_registry(store: &LibraryStore, entry: crate::app::places::RegisteredDevice) {
    crate::app::places::DeviceRegistry::new(store.fs_handle())
        .upsert(entry)
        .expect("the registry writes");
}

/// The one live board's card key (`identity_key()`).
fn device_card_key(studio: &StudioController) -> String {
    studio
        .view()
        .home
        .expect("gallery shows")
        .devices
        .iter()
        .find(|card| !card.sim)
        .expect("the board's card")
        .identity_key()
        .to_string()
}

fn studio_with_two_fake_devices(
    first: FakeDeviceScript,
    second: FakeDeviceScript,
) -> (
    StudioController,
    (FakeEsp32Device, FakeEsp32Device),
    LinkEndpointId,
    LinkEndpointId,
) {
    let first_id = LinkEndpointId::new("fake-device-0");
    let second_id = LinkEndpointId::new("fake-device-1");
    let provider = FakeProvider::new()
        .with_device_endpoint(first_id.clone(), "Fake ESP32 A (scripted)", first)
        .with_device_endpoint(second_id.clone(), "Fake ESP32 B (scripted)", second);
    let first_device = provider.device(&first_id).expect("first device registered");
    let second_device = provider
        .device(&second_id)
        .expect("second device registered");
    let mut registry = LinkProviderRegistry::new();
    registry.insert(provider);
    let studio = StudioController::with_link_registry_for_test(|| 1.0, registry);
    (studio, (first_device, second_device), first_id, second_id)
}

fn studio_with_fake_device(
    script: FakeDeviceScript,
) -> (StudioController, FakeEsp32Device, LinkEndpointId) {
    let endpoint_id = LinkEndpointId::new("fake-device-0");
    let provider = FakeProvider::new().with_device_endpoint(
        endpoint_id.clone(),
        "Fake ESP32 (scripted)",
        script,
    );
    let device = provider.device(&endpoint_id).expect("device registered");
    let mut registry = LinkProviderRegistry::new();
    registry.insert(provider);
    let studio = StudioController::with_link_registry_for_test(|| 1.0, registry);
    (studio, device, endpoint_id)
}

/// Drive the REAL connect path: `open_provider` (discover) then
/// `connect_endpoint` (connect → attach → readiness → pull), both through
/// the controller's dispatch surface. Returns the connect dispatch's
/// notices (Incompatible/NoFirmware connects resolve Ok WITH a notice).
fn connect_through_link(
    studio: &mut StudioController,
    endpoint_id: &LinkEndpointId,
) -> Result<UiNotices, UiError> {
    drive(studio.dispatch(device_action(DeviceOp::OpenProvider {
        provider_id: LinkProviderKind::Fake,
    })))?;
    drive(studio.dispatch(device_action(DeviceOp::ConnectEndpoint {
        provider_id: LinkProviderKind::Fake,
        endpoint_id: endpoint_id.clone(),
    })))
}

/// Install poll timers with a shortened readiness deadline, so
/// deadline-expiry rows (no hello / stalled wire) do not burn the default
/// five-second budget per test.
fn shorten_ready_deadline(studio: &mut StudioController, ready: Duration) {
    studio.set_device_timers(DeviceController::test_poll_timers().with_deadlines(
        DeviceDeadlines {
            ready,
            ..DeviceDeadlines::default()
        },
    ));
}

fn device_action(op: DeviceOp) -> UiAction {
    UiAction::from_op(ControllerId::new(DeviceController::NODE_ID), op)
}

fn deploy_action(op: DeployOp) -> UiAction {
    UiAction::from_op(ControllerId::new(DEPLOY_NODE_ID), op)
}

fn library() -> (LibraryStore, Rc<MemoryLibraryHost>) {
    // Counter-based uid bytes: rows installing MORE than one package need
    // distinct `prj_` uids (a fixed byte pattern would collide them).
    let counter = Rc::new(RefCell::new(6u8));
    let store = LibraryStore::new(
        Rc::new(RefCell::new(LpFsMemory::new())),
        Rc::new(move || {
            *counter.borrow_mut() += 1;
            [*counter.borrow(); 16]
        }),
        Rc::new(|| "2026-07-14-0900".to_string()),
    );
    let host = Rc::new(MemoryLibraryHost::new(store.clone(), Rc::new(|| 1.0)));
    (store, host)
}

fn project_files(marker: &str) -> Vec<(String, Vec<u8>)> {
    project_files_at_format(lpc_model::PROJECT_FORMAT_VERSION, marker)
}

/// The same minimal project, authored at an arbitrary format — the
/// fixture the format-upgrade rows (P5) are built on.
fn project_files_at_format(format: u32, marker: &str) -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "project.json".to_string(),
            format!(r#"{{"format":{format},"name":"Porch {marker}"}}"#).into_bytes(),
        ),
        (
            "module.json".to_string(),
            br#"{"kind":"Module","nodes":{}}"#.to_vec(),
        ),
        ("shader.glsl".to_string(), marker.as_bytes().to_vec()),
    ]
}

/// Read the device's boot output directly (a fresh stream on the same
/// device), for asserting scripted transitions when the studio's wire is
/// already dead.
fn read_device_lines(device: &FakeEsp32Device, timeout: Duration) -> Vec<String> {
    use lpa_link::providers::fake_device::FakeDeviceByteStream;
    use lpa_link::stream::DeviceByteStream;

    let mut stream = FakeDeviceByteStream::new(device.clone());
    let deadline = std::time::Instant::now() + timeout;
    let mut bytes = Vec::new();
    while std::time::Instant::now() < deadline {
        let mut buf = [0u8; 256];
        match stream.read_available(&mut buf) {
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if String::from_utf8_lossy(&bytes).contains("invalid header") {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Drive a future to completion against the fake device's real threads:
/// poll with a no-op waker, sleeping briefly between polls (channel state
/// advances on the device/serial threads), bounded so a hang fails the
/// test instead of the suite.
fn drive<F: Future>(future: F) -> F::Output {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    for _ in 0..60_000 {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::sleep(Duration::from_micros(500)),
        }
    }
    panic!("link e2e future did not complete within the poll budget");
}
