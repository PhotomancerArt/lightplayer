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
    let sync = studio.device_sync().expect("connect-as-pull landed");
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
    let sync = studio.device_sync().expect("connect-as-pull landed");
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

    let sync = studio.device_sync().expect("connect-as-pull landed");
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
    let sync = studio.device_sync().expect("pull landed");
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
            name: "Luna's porch sign".to_string(),
        },
    )))
    .unwrap();
    let sync = studio.device_sync().expect("re-pulled after stamp");
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
        key: summary.uid.to_string(),
    })))
    .unwrap();
    let sync = studio.device_sync().expect("re-pulled after push");
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
        studio.device_sync().map(|sync| &sync.content),
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

    let sync = studio.device_sync().expect("connect-as-pull landed");
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
        device_action(DeviceOp::ProvisionFirmware { setup_name: None }),
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
        setup_name: None,
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
    drive(studio.refresh_device_sync());

    let sync = studio.device_sync().expect("failed pull leaves a state");
    assert!(
        matches!(sync.content, DeviceContent::Unreadable { .. }),
        "mid-pull disconnect surfaces as unreadable, got {:?}",
        sync.content
    );

    // Erase is still reachable: the scripted transition runs and the
    // controller degrades gracefully when the (dead) wire cannot reattach.
    let outcome = drive(studio.dispatch(device_action(DeviceOp::ResetToBlank)));
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
        setup_name: None,
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
        setup_name: None,
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
    drive(studio.dispatch(device_action(DeviceOp::ConnectLightPlayer)))
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
    drive(studio.refresh_device_sync());
    assert!(
        matches!(studio.device_state_for_test(), Some(DeviceState::Gone)),
        "a dead stream marks the session Gone, got {:?}",
        studio.device_state_for_test()
    );

    // Replug: reconnect rebuilds stream + transport and re-runs readiness.
    device.set_failure_plan(FakeFailurePlan::none());
    drive(studio.dispatch(device_action(DeviceOp::ConnectLightPlayer)))
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

    let outcome = drive(studio.dispatch(deploy_action(DeployOp::EraseDevice)))
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

    let sync = studio.device_sync().expect("connect-as-pull landed");
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
            .device_sync()
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
            studio.device_sync().map(|sync| &sync.content),
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
    assert!(pool.device_session().is_some(), "device session survives");
    assert!(pool.sim_session().is_some(), "sim session exists");
    assert_eq!(pool.lens(), Some(sim_id), "the editor is a lens on the sim");
    // The device session is still classified: device_sync intact.
    let sync = studio.device_sync().expect("device_sync survives the open");
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
            .device_session()
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
            studio.device_sync().map(|sync| &sync.content),
            Some(DeviceContent::Known { slug, .. }) if slug == &porch.slug
        ),
        "the device runs the known project"
    );

    drive(studio.dispatch(deploy_action(DeployOp::PushProject {
        key: other.uid.to_string(),
    })))
    .expect("pushing a different project succeeds");
    assert!(
        matches!(
            studio.device_sync().map(|sync| &sync.content),
            Some(DeviceContent::Known { slug, relation: lpc_history::SyncRelation::AtHead, .. })
                if slug == &other.slug
        ),
        "the device now runs the other project at its head, got {:?}",
        studio.device_sync().map(|sync| sync.content.clone())
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
        assert!(pool.device_session().is_some(), "device session survives");
    }
    assert!(
        matches!(
            studio.device_sync().map(|sync| &sync.content),
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
        assert!(pool.device_session().is_some(), "the device session stays");
        assert_eq!(pool.lens(), None);
    }
    assert!(
        studio.device_sync().is_some(),
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
        let device_id = pool.device_session().expect("device session").id();
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
        .device_session()
        .expect("device session")
        .id();
    assert_eq!(
        studio.runtime_pool_for_test().lens(),
        Some(device_id),
        "the editor is a lens on the device"
    );
    assert!(studio.view().home.is_none(), "the editor is showing");

    // Erase the device from under the open editor.
    let outcome = drive(studio.dispatch(device_action(DeviceOp::ResetToBlank)))
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
    drive(studio.dispatch(device_action(DeviceOp::ResetToBlank)))
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
        .device_session()
        .expect("device session")
        .id();

    drive(studio.dispatch(device_action(DeviceOp::ResetDevice))).expect("runtime reset succeeds");

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
    let device_id = pool.device_session().expect("device session").id();
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
    assert!(pool.device_session().is_some(), "device session installed");
    // The sim mirror is untouched…
    let view = studio.view();
    assert!(view.home.is_none(), "the editor stayed open");
    assert_eq!(slot_value_display(find_slot(&view, "controls.rate")), "1");
    // …and the device reconciled in the background on its own client.
    let sync = studio.device_sync().expect("connect-as-pull landed");
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
        let device_id = pool.device_session().expect("device session").id();
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
        pool.device_session().map(crate::RuntimeSession::id),
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
            studio.device_sync().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::Behind,
                ..
            })
        ),
        "device classifies Behind, got {:?}",
        studio.device_sync().map(|sync| &sync.content)
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
            studio.device_sync().map(|sync| &sync.content),
            Some(DeviceContent::Known {
                relation: lpc_history::SyncRelation::AtHead,
                ..
            })
        ),
        "device is at head after the push, got {:?}",
        studio.device_sync().map(|sync| &sync.content)
    );
    let pool = studio.runtime_pool_for_test();
    assert!(
        !pool
            .device_session()
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
    let sync = studio.device_sync().expect("device state cached");
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
    let sync = studio.device_sync().expect("device state cached");
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
    vec![
        (
            "project.json".to_string(),
            format!(r#"{{"kind":"Project","format":2,"name":"Porch {marker}","nodes":{{}}}}"#)
                .into_bytes(),
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
