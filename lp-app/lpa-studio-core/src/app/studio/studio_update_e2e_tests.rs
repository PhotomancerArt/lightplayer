//! End-to-end update host tests: Studio's own controller, its real effects
//! layer and update host, driving `lpc-update`'s model board.
//!
//! The transport double ([`RigLink`]) stands where the channel-3 adapter
//! (`lpa-link`'s `device_link`, M7 P7) stands on a real port:
//! `LinkCommand::SendUpdate` goes into a [`BoardRig`]; every message the
//! rig sends comes back as `LinkEvent::Update`, and each `M` is also decoded
//! into `LinkEvent::UpdateFacts` by the adapter's own mirror
//! ([`update_facts_from_manifest_json`]). A board
//! running its engine says hello on channel 1 (its MAC, so it gets a card)
//! and sends heartbeats; a core-only board says nothing there. The rig's
//! resets and power cuts close the link, and Studio's own reopen rung opens
//! it again — the board resets mid-update the way a C6 does.
//!
//! Everything a test asserts it reaches through the controller: the card's
//! standing, the evidence's outcome, the terminal's lines, the engine
//! cache — and the board's end build, read off the rig.

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::rc::Rc;

use lpa_devices::identity::{EndpointKey, MacAddress, PeerIdentity};
use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, UsbIds};
use lpa_devices::wire::{BoardFs, ClientFrameBody, HelloFacts, ServerFrame};
use lpa_devices::{TerminalKind, UpdateOutcomeFacts};
use lpa_firmware_store::{
    EngineCache, EngineCacheEntry, EngineSource, FetchError, FirmwareFetch, FirmwareStore,
    LocalBoxFuture, MemoryEngineCache,
};
use lpa_link::device_link::update_facts_mirror::update_facts_from_manifest_json;
use lpa_update::decide::{SourceEffect, SourceResult};
use lpa_update::{DriverConfig, DriverEffect, HostBuild, HostIdentity, UpdateDriver, UpdateIntent};
use lpc_access::{OpenTo, SecretEntry, Tier};
use lpc_firmware_release::{
    OtaManifest, PackageRef, PieceFile, ReleaseSelector, Requires, TargetName, sha256_hex,
};
use lpc_update::board::{AccessFacts, LinkId as RigLinkId, LinkTrust, SessionConfig, SessionMode};
use lpc_update::code_table::CHUNK;
use lpc_update::testing::{BoardRig, BootFault, FakeBoard, ModelBuild};
use lpc_update::{BoardMessage, PieceKind};

use crate::app::studio::offer_press_test_api::OfferPressTestApi;
use crate::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceId, DeviceInput,
    DeviceTaskFuture, DeviceTransport, DeviceTransportFuture, GrantedLink, LensLineTap,
    MemoryOwnBuildSource, OfferArgs, StudioController, UpdateStanding,
};

/// The model board's region (the sim's own size).
const REGION: u32 = 40 * 4096;

/// The fake clock's step (seconds).
const STEP_SECS: f64 = 0.005;

/// A bound on any one wait, in steps (fake minutes, never wall time).
const MAX_STEPS: usize = 120_000;

/// X: what the board runs. Y: this Studio's own build.
fn x() -> ModelBuild {
    ModelBuild::synthetic("2026.10.05-1", 1, 5 * 4096 + 300, 8 * 4096 + 77)
}

fn y() -> ModelBuild {
    ModelBuild::synthetic("2026.10.06-1", 2, 6 * 4096 + 11, 9 * 4096 + 1000)
}

// ---------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------

/// Update X → Y with X's engine already in the cache: no read-back, the
/// backup pinned while the board is mid-update and let go on Y.
#[test]
fn update_x_to_y_with_the_backup_cached_reads_nothing_back_and_pins_it_until_y() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until("the backup pinned while the board is mid-update", |bench| {
        bench.board().rig.mode() == Some(SessionMode::CoreOnly) && bench.held(&x())
    });

    bench.run_until_update_ends(device);
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    bench.run_until("the backup let go on the new version", |bench| {
        !bench.held(&x())
    });
    assert!(bench.said("update 2026.10.05-1 → 2026.10.06-1 over USB"));
    assert!(bench.said("backup of 2026.10.05-1 already here · no read-back"));
    assert!(!bench.said("backing up current firmware from the board"));
    assert!(bench.said_starting("board reset · reconnected in "));
    assert!(bench.said_starting("core 25 KB in "));
    assert!(matches!(
        bench.standing(device),
        UpdateStanding::UpToDate { .. }
    ));
}

/// The board's resets keep the port open (the link's session resets, as
/// over the emulator's door): the update host drops what was in flight on
/// the old session, comes back up when the board speaks on the new one —
/// its `M` in core-only, its hello when its engine runs — and the update
/// ends on Y with no click.
#[test]
fn an_update_whose_resets_keep_the_port_open_ends_on_y() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    bench.board_mut().resets_keep_port = true;
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until_update_ends(device);
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    assert!(bench.said("backing up current firmware from the board"));
    assert!(bench.said_starting("board reset · reconnected in "));
    assert!(matches!(
        bench.standing(device),
        UpdateStanding::UpToDate { .. }
    ));
}

/// Update X → Y with an empty cache and no store: the backup is read back
/// from the board, kept and pinned, and let go on Y.
#[test]
fn update_x_to_y_with_no_cached_backup_reads_it_back_keeps_and_pins_it() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until("the read-back kept and pinned mid-update", |bench| {
        bench.board().rig.mode() == Some(SessionMode::CoreOnly) && bench.held(&x())
    });

    bench.run_until_update_ends(device);
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    bench.run_until("the backup let go on the new version", |bench| {
        !bench.held(&x())
    });
    assert!(bench.cached(&x()), "the backup stays in the cache");
    assert!(bench.said("backing up current firmware from the board"));
    assert!(bench.said_starting("backup 33 KB in "));
}

/// The link drops mid-core and stays away past the update's gap: the
/// activity ends honestly. When the board comes back it holds this Studio's
/// transfer, so the card says "Finishing" and the update completes itself
/// — no click.
#[test]
fn a_link_drop_mid_core_finishes_on_reconnect_with_no_click() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.board_mut().unplug_after_core_requests = Some(3);
    bench.press_update(device);

    bench.run_until("the update to end without the board", |bench| {
        bench.outcome(device) == Some(UpdateOutcomeFacts::BoardDidNotComeBack)
    });
    assert!(bench.said_starting("link dropped during update at "));
    let presses = bench.presses;

    let starts = bench.update_starts;
    bench.board_mut().replug();
    let mut finishing = false;
    for _ in 0..MAX_STEPS {
        bench.step();
        finishing |= matches!(bench.standing(device), UpdateStanding::Finishing { .. });
        if bench.update_starts > starts && bench.outcome(device).is_some() {
            break;
        }
    }
    assert!(finishing, "the card said Finishing");
    assert_eq!(bench.update_starts, starts + 1, "started by itself, once");
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    assert_eq!(bench.presses, presses, "no click");
    assert!(bench.said("board is half-way to 2026.10.06-1 · finishing it"));
}

/// A board that lost its engine, plugged in (a pending link: a core-only
/// board says no hello), with its engine cached and no build of this
/// Studio's own: restored on connect over an untrusted, locked link — no
/// click, no login.
#[test]
fn an_engineless_board_is_restored_on_connect_from_the_cache_with_no_click_or_login() {
    let mut board = Board::engineless_x();
    board.trust = LinkTrust::Untrusted;
    board.rig.access = locked_access(Vec::new());
    board.rig.reboot().expect("boots");
    let mut bench = Bench::new(board, None);
    bench.cache_engine(&x());
    bench.grant();

    // While it runs, the board's pending card says what it is doing (the
    // spike's E1), not "identifying" (found in the emulator walk).
    bench.run_until("the pending card to say it is restoring", |bench| {
        bench
            .controller
            .device_roster_view()
            .roster
            .pending
            .iter()
            .any(|pending| pending.state_label.starts_with("Restoring firmware…"))
    });
    bench.run_until("the board to run X again", |bench| {
        bench.board().rig.mode() == Some(SessionMode::EngineRunning)
    });
    bench.run_until("the restore to end", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&x());
    assert_eq!(bench.presses, 0, "no click");
    assert!(!bench.board().saw(b'L'), "no login");
    assert!(bench.said("board is missing its 2026.10.05-1 engine · restoring it"));
}

/// The same board with its engine nowhere (no cache, no store): E13's row,
/// once — the no-click start does not fire again on the same link. A
/// reconnect tries again (and misses again), and still no loop.
#[test]
fn an_engineless_board_whose_engine_is_nowhere_ends_on_e13_and_does_not_loop() {
    let mut bench = Bench::new(Board::engineless_x(), None);
    bench.grant();
    bench.run_until("the restore to miss", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::MissingEngine { offline: true })
    });
    assert!(bench.said("no copy of 2026.10.05-1 here, and the release store is unreachable"));
    bench.steps(4_000);

    assert_eq!(bench.update_starts, 1, "one restore, no loop");

    bench.board_mut().reset_in_place();
    bench.steps(4_000);
    assert_eq!(bench.update_starts, 2, "a reconnect tries again");
    bench.steps(4_000);
    assert_eq!(bench.update_starts, 2, "…once");
    assert_eq!(
        bench.any_outcome(),
        Some(UpdateOutcomeFacts::MissingEngine { offline: true })
    );
}

/// E13 with a build of this Studio's own (the walk's `cant-get`): the
/// engine-less board's restore misses, once, on its pending link (a
/// core-only board says no hello). Kept ("Set up this device"), its card
/// reads the row — "Needs X, which Studio can't get" — and offers this
/// Studio's build, which ends on Y. The identify that keeping it runs must
/// not wipe how the restore ended: the card would read "Restoring…" with
/// nothing to press (found in the emulator walk).
#[test]
fn an_engineless_board_whose_engine_is_nowhere_offers_this_studios_build_once_kept() {
    for resets_keep_port in [false, true] {
        engineless_board_kept_then_installed(resets_keep_port);
    }
}

fn engineless_board_kept_then_installed(resets_keep_port: bool) {
    let mut bench = Bench::new(Board::engineless_x(), Some(y()));
    bench.board_mut().resets_keep_port = resets_keep_port;
    bench.grant();
    bench.run_until("the restore to miss", |bench| {
        matches!(
            bench.any_outcome(),
            Some(UpdateOutcomeFacts::MissingEngine { .. })
        )
    });
    bench.steps(4_000);
    assert_eq!(bench.update_starts, 1, "one restore, no loop");

    let link = bench.controller.devices_for_test().roster().pending()[0].link;
    bench
        .controller
        .fold_device_input(DeviceInput::Action(lpa_devices::Action::AdoptLink { link }));
    let device = bench.controller.devices_for_test().roster().devices()[0].id;
    bench.run_until("the kept card to read E13, its identify done", |bench| {
        matches!(
            bench.standing(device),
            UpdateStanding::CantGetVersion { .. }
        ) && bench
            .controller
            .devices_for_test()
            .roster()
            .device(device)
            .is_some_and(|d| d.activity_kind().is_none())
    });
    assert_eq!(bench.update_starts, 1, "still no loop");
    bench.press(device, "install-firmware", OfferArgs::new());
    bench.run_until("the install to end on Y", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&y());
}

/// E13 on a board Studio already knows (found in the emulator walk): it
/// lost its engine and comes back through a port Studio has never seen, so
/// it is a pending link (a core-only board says no hello). Kept anonymously
/// and given this Studio's build, its hello at the end names the remembered
/// board, and the roster merges the two cards — moving the running update to
/// the remembered one. The update follows it and ends there, on Y.
#[test]
fn an_install_on_a_kept_card_that_merges_into_a_remembered_board_ends_on_y() {
    let mut bench = Bench::new(Board::engineless_x(), Some(y()));
    bench.board_mut().resets_keep_port = true;
    bench
        .controller
        .devices_mut_for_test()
        .load_records(&[crate::app::places::RegisteredDevice {
            uid: "dev0000000000000042".to_string(),
            name: "Porch sign".to_string(),
            hardware_id: Some("efuse:60:55:f9:0a:0b:01".to_string()),
            ..crate::app::places::RegisteredDevice::default()
        }]);
    let known = bench.controller.devices_for_test().roster().devices()[0].id;
    bench.grant();
    bench.run_until("the restore to miss on the pending link", |bench| {
        matches!(
            bench.any_outcome(),
            Some(UpdateOutcomeFacts::MissingEngine { .. })
        )
    });
    let link = bench.controller.devices_for_test().roster().pending()[0].link;
    bench
        .controller
        .fold_device_input(DeviceInput::Action(lpa_devices::Action::AdoptLink { link }));
    let kept = bench
        .controller
        .devices_for_test()
        .roster()
        .devices()
        .iter()
        .find(|d| d.id != known)
        .map(|d| d.id)
        .expect("the kept card");
    bench.run_until("the kept card to read E13, its identify done", |bench| {
        matches!(bench.standing(kept), UpdateStanding::CantGetVersion { .. })
            && bench
                .controller
                .devices_for_test()
                .roster()
                .device(kept)
                .is_some_and(|d| d.activity_kind().is_none())
    });
    bench.press(kept, "install-firmware", OfferArgs::new());
    bench.run_until("the install to end on Y", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&y());
    let roster = bench.controller.devices_for_test().roster();
    assert!(roster.device(kept).is_none(), "the kept card merged away");
    assert!(
        roster
            .device(known)
            .is_some_and(|d| d.activity_kind().is_none()),
        "the remembered card is done updating"
    );
}

/// The firmware store answers for a released build: a board missing X's
/// engine is restored from the store (the engine verified and kept in the
/// cache), and the store's `latest` reaches the build facts.
#[test]
fn the_store_restores_a_released_build_and_names_its_latest() {
    let mut bench = Bench::new(Board::engineless_x(), None);
    let fetch = Rc::new(StoreFetch::default());
    fetch.publish(&x(), "2026.10.05-1");
    fetch.publish_latest(&y());
    bench
        .controller
        .set_firmware_store(Rc::new(FirmwareStore::new(
            STORE_ORIGIN,
            Rc::clone(&fetch) as Rc<dyn FirmwareFetch>,
        )));
    bench.grant();

    bench.run_until("the restore to end", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&x());
    assert!(bench.cached(&x()), "the store's engine is kept");
    assert!(
        fetch.asked("/2026.10.05-1/engine.bin"),
        "{:?}",
        fetch.urls.borrow()
    );
    bench.run_until("the store's latest to be known", |bench| {
        bench
            .controller
            .update_build_facts()
            .store_latest()
            .is_some()
    });
    assert_eq!(
        bench
            .controller
            .update_build_facts()
            .store_latest()
            .map(|latest| latest.version().to_string()),
        Some("2026.10.06-1".to_string())
    );
}

/// Reinstall on a crashing board: the board's own engine (from the cache)
/// is written once; the model's crash is the build's, so the board comes
/// back crashing and the update says so rather than writing it again.
#[test]
fn reinstall_on_a_crashing_board_writes_its_engine_once() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.board_mut().rig.board.catalog[0].crashes = true;
    bench.board_mut().reset_in_place();
    bench.run_until("the card to offer Reinstall", |bench| {
        matches!(bench.standing(device), UpdateStanding::KeepsCrashing { .. })
    });
    let boots = bench.board().rig.boots;

    bench.press(device, "reinstall-firmware", OfferArgs::new());
    bench.run_until_update_ends(device);
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::Crashing));
    assert_eq!(
        bench.board().rig.boots,
        boots + 1,
        "written once, reset once"
    );
    assert!(bench.said("writing 2026.10.05-1 again over USB"));
}

/// E13 on a known board, then "Install Y": the board waits for an engine
/// nobody has; this Studio's own build goes on instead.
#[test]
fn install_y_on_an_e13_board_puts_y_on_it() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    let device = bench.connect_device();
    bench.board_mut().erase_engine();
    bench.board_mut().reset_in_place();
    bench.run_until("the restore to miss", |bench| {
        bench.outcome(device) == Some(UpdateOutcomeFacts::MissingEngine { offline: true })
    });
    bench.run_until("the card to offer Install Y", |bench| {
        matches!(
            bench.standing(device),
            UpdateStanding::CantGetVersion { .. }
        )
    });

    // Y is the one version this Studio can get: "Install Y" is one press.
    bench.press(device, "install-firmware", OfferArgs::new());
    bench.run_until_update_ends(device);
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
}

/// Another device owns the board's transfer (Y's core, part-way): the card
/// waits and Studio asks `Q` every 3 s; when that device's link drops,
/// Studio takes over and finishes Y with no click.
#[test]
fn another_devices_transfer_is_watched_then_taken_over_when_its_link_drops() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    let now = bench.now_ms();
    let other = other_host_starts_y(&mut bench.board_mut().rig, now, 3);
    bench.grant();
    bench.run_until("the board to be seen", |bench| bench.any_facts());
    bench.run_until("two watch questions", |bench| {
        bench.board().count(b'Q') >= 3
    });
    assert_eq!(bench.board().rig.mode(), Some(SessionMode::CoreOnly));
    assert!(
        bench.any_outcome().is_none(),
        "nothing started: another device owns it"
    );

    let now = bench.now_ms();
    bench.board_mut().rig.link_down(now, other);
    bench.run_until("Studio to take over and finish", |bench| {
        bench.board().rig.mode() == Some(SessionMode::EngineRunning)
            && bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&y());
    assert_eq!(bench.presses, 0, "no click");
}

/// An install that meets another device's transfer waits inside the
/// activity (the card's `Waiting`), asking `Q` every 3 s, and starts over
/// on the board's word when that device's link drops — ending on Y.
#[test]
fn an_install_that_meets_another_devices_transfer_waits_then_takes_over() {
    let mut bench = Bench::new(Board::running_x(), Some(y()));
    let now = bench.now_ms();
    let other = other_host_starts_y(&mut bench.board_mut().rig, now, 3);
    bench.grant();
    bench.run_until("the board to be seen", |bench| bench.any_facts());
    let device = bench.pending_device().expect("a pending link");
    bench.install_on_pending(device, "2026.10.06-1");
    bench.run_until("the install to wait", |bench| {
        bench.said_starting("another device is updating it")
    });
    let asked = bench.board().count(b'Q');
    bench.steps(1_300);
    assert!(
        bench.board().count(b'Q') >= asked + 2,
        "asked again every 3 s while waiting"
    );
    assert!(bench.any_outcome().is_none(), "still waiting, not ended");

    let now = bench.now_ms();
    bench.board_mut().rig.link_down(now, other);
    bench.run_until("the install to take over and finish", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::UpToDate)
    });
    bench.assert_runs(&y());
}

/// DS9: a board that never offered channel 3 (no `M`, no manifest in its
/// hello) gets nothing on it; an update leg there ends at once, "needs one
/// update over USB".
#[test]
fn a_leg_on_a_board_that_never_offered_channel_3_ends_needs_usb_and_sends_nothing() {
    let mut board = Board::running_x();
    board.announces = false;
    let mut bench = Bench::new(board, Some(y()));
    bench.grant();
    bench.run_until("the board to get a card", |bench| {
        bench
            .controller
            .devices_for_test()
            .roster()
            .devices()
            .iter()
            .any(|d| d.evidence.has_hello())
    });
    let device = bench.controller.devices_for_test().roster().devices()[0].id;
    // No card offers an update here (nothing announced it); the gesture
    // the model would get, folded as a press would fold it.
    bench
        .controller
        .fold_device_input(DeviceInput::Action(lpa_devices::Action::Update {
            device,
            intent: lpa_devices::UpdateIntentFacts::Auto,
        }));
    bench.run_until("the leg to end", |bench| bench.outcome(device).is_some());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::NeedsUsb));
    assert!(
        bench.board().received.is_empty(),
        "nothing sent on channel 3"
    );
    assert!(bench.said("this board did not offer the update channel · USB once"));
}

/// A core install over an untrusted link to a locked board holding this
/// browser's key: the board asks for a login, the held key answers it, the
/// install goes on.
#[test]
fn a_core_install_over_an_untrusted_link_logs_in_with_the_held_key() {
    let mut board = Board::engineless_x();
    board.trust = LinkTrust::Untrusted;
    let mut bench = Bench::new(board, Some(y()));
    let key = bench.browser_key_entry();
    bench.board_mut().rig.access = locked_access(vec![key]);
    bench.board_mut().rig.reboot().expect("boots");
    bench.grant();
    bench.run_until("the restore to miss", |bench| bench.any_outcome().is_some());
    let device = bench.pending_device().expect("a pending link");

    bench.install_on_pending(device, "2026.10.06-1");
    bench.run_until("Y to run", |bench| {
        bench.board().rig.mode() == Some(SessionMode::EngineRunning)
    });
    bench.assert_runs(&y());
    assert!(bench.board().saw(b'L'), "logged in");
}

/// The same install when the board does not know this browser's key: the
/// login is refused and the update ends saying so; nothing is written.
#[test]
fn a_core_install_over_an_untrusted_link_without_a_known_key_is_refused() {
    let mut board = Board::engineless_x();
    board.trust = LinkTrust::Untrusted;
    board.rig.access = locked_access(vec![SecretEntry::from_password(
        "someone else",
        Tier::Edit,
        b"hunter2",
        [4; 16],
        16,
    )]);
    board.rig.reboot().expect("boots");
    let mut bench = Bench::new(board, Some(y()));
    bench.grant();
    bench.run_until("the restore to miss", |bench| bench.any_outcome().is_some());
    let device = bench.pending_device().expect("a pending link");
    let ops = bench.board().rig.board.flash.ops();

    bench.install_on_pending(device, "2026.10.06-1");
    bench.run_until("the install to end", |bench| {
        bench.any_outcome() == Some(UpdateOutcomeFacts::LoginRefused)
    });
    assert_eq!(bench.board().rig.board.flash.ops(), ops, "nothing written");
    assert!(bench.board().saw(b'L'), "a login was tried");
}

/// A power cut after flash operation k of the update, for every k of a
/// clean run (stepped, to keep the suite quick): Studio brings the board
/// to Y every time, with no click after the one Update.
#[test]
fn a_power_cut_after_each_flash_operation_converges_through_studio() {
    let clean = {
        let mut bench = Bench::new(Board::running_x(), Some(y()));
        bench.cache_engine(&x());
        let device = bench.connect_device();
        let before = bench.board().rig.board.flash.ops();
        bench.press_update(device);
        bench.run_until_update_ends(device);
        bench.assert_runs(&y());
        bench.board().rig.board.flash.ops() - before
    };
    let mut cases = 0;
    for tear in [false, true] {
        for k in 0..clean {
            let mut bench = Bench::new(Board::running_x(), Some(y()));
            bench.cache_engine(&x());
            let device = bench.connect_device();
            bench.board_mut().rig.board.flash.tear(tear);
            bench.board_mut().rig.board.flash.cut_after(k);
            bench.press_update(device);
            bench.run_until(
                &format!("Y after a cut at op {k} (torn: {tear})"),
                |bench| {
                    bench.board().rig.mode() == Some(SessionMode::EngineRunning)
                        && bench.board().rig.board.running_build() == Some(&y())
                        && bench.outcome(device).is_some()
                },
            );
            bench.assert_runs(&y());
            assert_eq!(bench.board().cuts, 1, "k={k} torn={tear}");
            assert_eq!(bench.presses, 1, "k={k} torn={tear}: one click");
            cases += 1;
        }
    }
    println!(
        "power cut after flash op k of {clean} (clean and torn): {cases} cases converged through Studio"
    );
}

// ---------------------------------------------------------------------
// The board, as a link
// ---------------------------------------------------------------------

/// A model board on a link Studio holds.
struct Board {
    rig: BoardRig,
    trust: LinkTrust,
    /// The rig's id for the link while the port is open.
    open: Option<RigLinkId>,
    next_rig_link: u32,
    events: VecDeque<LinkEvent>,
    clock: Rc<Cell<f64>>,
    last_heartbeat: f64,
    /// Every host message's type byte, in order.
    received: Vec<u8>,
    cuts: u32,
    /// Drop the link (and keep it down) after this many core requests.
    unplug_after_core_requests: Option<u32>,
    core_requests: u32,
    unplugged: bool,
    /// The board speaks channel 3 (`false`: pre-update firmware — no `M`,
    /// no manifest in its hello).
    announces: bool,
    /// The board's resets keep the port open, as an lp-link transport that
    /// does not re-enumerate sees them (the emulator's door, a classic's
    /// UART): the link's SESSION resets — a link-reset note, then the new
    /// session's words — and nothing closes.
    resets_keep_port: bool,
    /// The port's endpoint: a replug through another port (or another
    /// Web Serial grant) is another endpoint, and the roster cannot route
    /// it to a card by endpoint alone.
    endpoint: String,
}

impl Board {
    fn new(board: FakeBoard) -> Self {
        let session = SessionConfig {
            entropy: Some(|b: &mut [u8]| b.fill(0x3c)),
            ..SessionConfig::default()
        };
        Self {
            rig: BoardRig::new(board, AccessFacts::from_store(None), session)
                .expect("the board boots"),
            trust: LinkTrust::Trusted,
            open: None,
            next_rig_link: 10,
            events: VecDeque::new(),
            clock: Rc::new(Cell::new(1_000.0)),
            last_heartbeat: 0.0,
            received: Vec::new(),
            cuts: 0,
            unplug_after_core_requests: None,
            core_requests: 0,
            unplugged: false,
            resets_keep_port: false,
            endpoint: "serial:model-board".to_string(),
            announces: true,
        }
    }

    fn running_x() -> Self {
        Self::new(FakeBoard::flashed_with(vec![x(), y()], 0, REGION))
    }

    fn engineless_x() -> Self {
        let mut board = Self::running_x();
        board.erase_engine();
        board.rig.reboot().expect("boots");
        board
    }

    fn info(&self) -> LinkInfo {
        LinkInfo {
            label: "model board".to_string(),
            endpoint: EndpointKey(self.endpoint.clone()),
            usb: Some(UsbIds {
                vendor: 0x303a,
                product: 0x1001,
            }),
            serial_number: None,
            carries_update_channel: true,
        }
    }

    fn now_ms(&self) -> u64 {
        (self.clock.get() * 1_000.0) as u64
    }

    fn mac() -> PeerIdentity {
        PeerIdentity {
            mac: Some(MacAddress("60:55:f9:0a:0b:01".to_string())),
            ..PeerIdentity::default()
        }
    }

    /// Erase the running core's engine header (the board lost its engine).
    fn erase_engine(&mut self) {
        let x = x();
        let at = (0x8000 + x.core.len() as u32).div_ceil(CHUNK) * CHUNK;
        self.rig
            .board
            .flash
            .flash_image(at, &vec![0xFF; CHUNK as usize]);
    }

    fn saw(&self, ty: u8) -> bool {
        self.received.contains(&ty)
    }

    fn count(&self, ty: u8) -> usize {
        self.received.iter().filter(|&&t| t == ty).count()
    }

    fn engine_running(&self) -> bool {
        self.rig.mode() == Some(SessionMode::EngineRunning)
    }

    fn open(&mut self) {
        if self.open.is_some() || self.unplugged {
            return;
        }
        let link = RigLinkId(self.next_rig_link);
        self.next_rig_link += 1;
        self.open = Some(link);
        self.events
            .push_back(LinkEvent::Opened { info: self.info() });
        let now = self.now_ms();
        let outs = self.rig.link_up(now, link, self.trust);
        self.push_outgoing(outs.into_iter().map(|o| o.bytes));
        if self.engine_running() {
            self.hello(0);
        }
    }

    fn close(&mut self, reason: &str) {
        if let Some(link) = self.open.take() {
            let now = self.now_ms();
            self.rig.link_down(now, link);
            self.events.push_back(LinkEvent::Closed {
                reason: reason.to_string(),
            });
        }
    }

    /// A reset while the port stays held: closed, rebooted, open again.
    fn reset_in_place(&mut self) {
        self.close("board reset");
        self.rig.reboot().expect("boots");
        self.open();
    }

    /// The cable back in: the port opens by itself.
    fn replug(&mut self) {
        self.unplugged = false;
        self.unplug_after_core_requests = None;
        self.open();
    }

    /// A running board's hello, carrying its manifest (Part B's `firmware`
    /// field): what tells Studio a running board speaks channel 3, since it
    /// sends `M` only when asked.
    fn hello(&mut self, request_id: u32) {
        let version = self
            .rig
            .board
            .running_build()
            .map(|b| b.version.clone())
            .unwrap_or_default();
        let update = self.open.filter(|_| self.announces).and_then(|link| {
            let now = self.now_ms();
            self.rig
                .deliver(
                    now,
                    link,
                    None,
                    &lpc_update::encode_query(lpc_update::PROTO_V1),
                )
                .into_iter()
                .find_map(|o| match BoardMessage::decode(&o.bytes) {
                    Ok(BoardMessage::Manifest(json)) => update_facts_from_manifest_json(json),
                    _ => None,
                })
        });
        self.events.push_back(LinkEvent::Frame(ServerFrame::hello(
            request_id,
            HelloFacts {
                proto: lpc_wire::WIRE_PROTO_VERSION,
                identity: Self::mac(),
                firmware: Some(format!("fw-esp32c6 {version}")),
                version: Some(version),
                board_id: None,
                fs: BoardFs::Mounted,
                update,
            },
        )));
    }

    fn push_outgoing(&mut self, outs: impl IntoIterator<Item = Vec<u8>>) {
        if !self.announces {
            return;
        }
        for bytes in outs {
            if let Ok(BoardMessage::Manifest(json)) = BoardMessage::decode(&bytes)
                && let Some(facts) = update_facts_from_manifest_json(json)
            {
                self.events.push_back(LinkEvent::UpdateFacts(facts));
            }
            if let Ok(BoardMessage::Request(r)) = BoardMessage::decode(&bytes)
                && r.kind == PieceKind::Core
            {
                self.core_requests += 1;
            }
            self.events.push_back(LinkEvent::Update(bytes));
        }
    }

    fn deliver(&mut self, bytes: &[u8]) {
        let Some(link) = self.open else {
            return;
        };
        self.received.extend(bytes.first());
        let now = self.now_ms();
        let outs = self.rig.deliver(now, link, None, bytes);
        if self.rig.board.flash.is_frozen() {
            return self.power_cut();
        }
        self.push_outgoing(outs.into_iter().map(|o| o.bytes));
        if self
            .unplug_after_core_requests
            .is_some_and(|n| self.core_requests >= n)
        {
            self.unplugged = true;
            self.close("the cable came out");
            return;
        }
        if self.rig.reset_pending {
            if self.resets_keep_port {
                return self.session_reset();
            }
            self.close("board reset");
            match self.rig.reboot() {
                Ok(()) => {}
                Err(BootFault::PowerCut) => self.power_cut(),
                Err(e) => panic!("boots after a reset: {e:?}"),
            }
        }
    }

    /// A reset on a port that stays open: the old session's words are
    /// lost, the link says it reset, and the rebooted board speaks on a new
    /// session (its `M` in core-only, its hello when its engine runs).
    fn session_reset(&mut self) {
        let Some(old) = self.open.take() else {
            return;
        };
        let now = self.now_ms();
        self.rig.link_down(now, old);
        match self.rig.reboot() {
            Ok(()) => {}
            Err(e) => panic!("boots after a reset: {e:?}"),
        }
        self.events.push_back(LinkEvent::WireNote(
            lpa_link::device_link::port_read_map::link_reset_note(
                lpc_wire::lp_link::ResetReason::PeerRestarted,
            ),
        ));
        let link = RigLinkId(self.next_rig_link);
        self.next_rig_link += 1;
        self.open = Some(link);
        let now = self.now_ms();
        let outs = self.rig.link_up(now, link, self.trust);
        self.push_outgoing(outs.into_iter().map(|o| o.bytes));
        if self.engine_running() {
            self.hello(0);
        }
    }

    /// The power is gone: the flash must still hold a bootable board.
    fn power_cut(&mut self) {
        self.cuts += 1;
        self.rig
            .board
            .check_invariant()
            .unwrap_or_else(|e| panic!("after a cut: {e:?}"));
        self.close("power cut");
        self.rig.power_cycle().expect("boots after a cut");
    }
}

/// The link Studio holds over the board.
struct RigLink {
    board: Rc<RefCell<Board>>,
    info: LinkInfo,
}

impl Link for RigLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        let mut board = self.board.borrow_mut();
        match command {
            LinkCommand::Open { .. } => board.open(),
            LinkCommand::Close => board.close("closed"),
            LinkCommand::SendFrame(frame) => {
                if board.open.is_some()
                    && board.engine_running()
                    && matches!(frame.body, ClientFrameBody::Hello)
                {
                    board.hello(frame.request_id);
                }
            }
            LinkCommand::SendUpdate(bytes) => board.deliver(&bytes),
            LinkCommand::RunReset(_) | LinkCommand::SendLine(_) => {}
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        let mut board = self.board.borrow_mut();
        let now = board.clock.get();
        if board.open.is_some() && board.engine_running() && now - board.last_heartbeat >= 1.0 {
            board.last_heartbeat = now;
            board
                .events
                .push_back(LinkEvent::Frame(ServerFrame::heartbeat(Some(Board::mac()))));
        }
        board.events.pop_front()
    }
}

/// The one board, granted when the test says.
struct RigTransport {
    board: Rc<RefCell<Board>>,
    granted: Rc<Cell<bool>>,
}

impl RigTransport {
    fn link(&self) -> GrantedLink {
        let info = self.board.borrow().info();
        GrantedLink {
            info: info.clone(),
            link: Box::new(RigLink {
                board: Rc::clone(&self.board),
                info,
            }),
        }
    }
}

impl DeviceTransport for RigTransport {
    fn label(&self) -> &'static str {
        "model board"
    }

    fn run_effect(
        &self,
        _info: LinkInfo,
        _call: DeviceEffectCall,
        _progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        Box::pin(core::future::ready(Err(
            "the model board takes no borrowed-wire effects".to_string(),
        )))
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let granted = match self.granted.get() {
            true => vec![self.link()],
            false => Vec::new(),
        };
        Box::pin(core::future::ready(Ok(granted)))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        self.granted.set(true);
        Box::pin(core::future::ready(Ok(Some(self.link()))))
    }

    fn revoke_grant(&self, _info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        Box::pin(core::future::ready(Ok(())))
    }

    fn lens_client_io(
        &self,
        _info: LinkInfo,
        _tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        Err("no lens on the model board".to_string())
    }
}

// ---------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------

const STORE_ORIGIN: &str = "https://store.test";

/// A firmware store holding what a test publishes.
#[derive(Default)]
struct StoreFetch {
    files: RefCell<HashMap<String, Vec<u8>>>,
    urls: RefCell<Vec<String>>,
}

impl StoreFetch {
    /// Publish `build` as release `version`: its manifest (by build id and
    /// by version) and its pieces.
    fn publish(&self, build: &ModelBuild, version: &str) {
        let manifest = ota_manifest(build);
        let store = FirmwareStore::new(STORE_ORIGIN, NoFetch);
        let target = TargetName::parse("esp32c6-4mb").expect("a target");
        let by_id = ReleaseSelector::parse(&build.build_id).expect("a build id");
        let by_version = ReleaseSelector::parse(version).expect("a version");
        let mut files = self.files.borrow_mut();
        for selector in [&by_id, &by_version] {
            files.insert(
                store.url(&target, selector, "ota-manifest.json"),
                manifest.to_json_bytes(),
            );
        }
        files.insert(
            store.url(&target, &by_version, "core.bin"),
            build.core.clone(),
        );
        files.insert(
            store.url(&target, &by_version, "engine.bin"),
            build.engine.clone(),
        );
    }

    /// Name `build` the store's latest.
    fn publish_latest(&self, build: &ModelBuild) {
        let store = FirmwareStore::new(STORE_ORIGIN, NoFetch);
        let target = TargetName::parse("esp32c6-4mb").expect("a target");
        self.files.borrow_mut().insert(
            store.url(&target, &ReleaseSelector::Latest, "ota-manifest.json"),
            ota_manifest(build).to_json_bytes(),
        );
    }

    fn asked(&self, tail: &str) -> bool {
        self.urls.borrow().iter().any(|url| url.ends_with(tail))
    }
}

impl FirmwareFetch for StoreFetch {
    fn get(&self, url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
        self.urls.borrow_mut().push(url.to_string());
        let found = self.files.borrow().get(url).cloned();
        Box::pin(core::future::ready(Ok(found)))
    }
}

/// A fetch for URL-building only.
struct NoFetch;

impl FirmwareFetch for NoFetch {
    fn get(&self, _url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
        Box::pin(core::future::ready(Ok(None)))
    }
}

/// `build`'s `ota-manifest.json`, as a release would publish it.
fn ota_manifest(build: &ModelBuild) -> OtaManifest {
    let commit12 = build
        .build_id
        .split_once('+')
        .map(|(_, c)| c.to_string())
        .expect("a build id");
    let piece = |file: &str, bytes: &[u8]| PieceFile {
        file: file.to_string(),
        length: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    };
    let manifest = OtaManifest {
        format: 1,
        target: "esp32c6-4mb".to_string(),
        chip: "esp32c6".to_string(),
        version: build.version.clone(),
        commit: format!("{commit12}{}", "0".repeat(28)),
        wire_proto: 36,
        requires: Requires {
            layout: 1,
            loader: 1,
        },
        core: piece("core.bin", &build.core),
        engine: piece("engine.bin", &build.engine),
        encodings: Vec::new(),
        package: PackageRef {
            file: "package.json".to_string(),
            length: 2,
            sha256: sha256_hex(b"{}"),
            image: piece("image.bin", b"image"),
        },
    };
    manifest.validate().expect("a valid manifest");
    manifest
}

// ---------------------------------------------------------------------
// Another host (DS8's other device)
// ---------------------------------------------------------------------

/// Another host puts Y on the board over its own trusted link and stops
/// after `requests` core requests — its link still up, owning the
/// transfer. Returns that link.
fn other_host_starts_y(rig: &mut BoardRig, now: u64, requests: u32) -> RigLinkId {
    let config = DriverConfig {
        intent: UpdateIntent::Install {
            allow_downgrade: false,
        },
        ..DriverConfig::default()
    };
    let mut driver = UpdateDriver::new(host_build(&y()), config);
    let mut link = RigLinkId(900);
    let mut to_board: VecDeque<Vec<u8>> = VecDeque::new();
    let mut to_host: VecDeque<Vec<u8>> = rig
        .link_up(now, link, LinkTrust::Trusted)
        .into_iter()
        .map(|o| o.bytes)
        .collect();
    driver.link_up(now);
    let mut served = 0;
    for _ in 0..100_000 {
        loop {
            let effects = driver.take_effects();
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                match effect {
                    DriverEffect::Send(bytes) => to_board.push_back(bytes),
                    DriverEffect::Source(SourceEffect::LookUpCache { .. }) => {
                        driver.source_result(SourceResult::Cache(Some(x().engine)));
                    }
                    DriverEffect::Done(finish) => panic!("the other host finished: {finish:?}"),
                    _ => {}
                }
            }
        }
        if let Some(bytes) = to_board.pop_front() {
            let outs = rig.deliver(now, link, None, &bytes);
            if rig.reset_pending {
                driver.link_down(now);
                rig.link_down(now, link);
                rig.reboot().expect("boots");
                link = RigLinkId(link.0 + 1);
                to_board.clear();
                to_host = rig
                    .link_up(now, link, LinkTrust::Trusted)
                    .into_iter()
                    .map(|o| o.bytes)
                    .collect();
                driver.link_up(now);
                continue;
            }
            to_host.extend(outs.into_iter().map(|o| o.bytes));
        } else if let Some(bytes) = to_host.pop_front() {
            if let Ok(BoardMessage::Request(r)) = BoardMessage::decode(&bytes)
                && r.kind == PieceKind::Core
            {
                served += 1;
                if served > requests {
                    return link;
                }
            }
            driver.on_board(now, &bytes, &[]);
        } else {
            panic!("the other host stalled");
        }
    }
    panic!("the other host never reached the core");
}

/// The `HostBuild` of a model build (raw pieces).
fn host_build(b: &ModelBuild) -> HostBuild {
    HostBuild::from_parts(
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: b.version.clone(),
            build_id: b.build_id.clone(),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        },
        b.core.clone(),
        b.engine.clone(),
        None,
        None,
    )
    .expect("a model build is a host build")
}

/// A board open to nobody, with these secrets.
fn locked_access(secrets: Vec<SecretEntry>) -> AccessFacts {
    AccessFacts {
        secrets,
        open: OpenTo::Nobody,
        core_install_follows_open_to: true,
    }
}

// ---------------------------------------------------------------------
// The bench
// ---------------------------------------------------------------------

type TaskPool = Rc<RefCell<Vec<DeviceTaskFuture>>>;

/// Studio's controller over one model board.
struct Bench {
    controller: StudioController,
    board: Rc<RefCell<Board>>,
    granted: Rc<Cell<bool>>,
    clock: Rc<Cell<f64>>,
    inbox: Rc<RefCell<VecDeque<DeviceInput>>>,
    tasks: TaskPool,
    cache: MemoryEngineCache,
    access_rx: crate::app::studio::studio_view_channel::CommandReceiver,
    /// Every Studio terminal line seen, on any card or pending link.
    lines: BTreeSet<String>,
    /// Update presses this test made.
    presses: usize,
    /// Update activities started, read off the roster's journal.
    update_starts: usize,
    journal_seen: Option<u64>,
}

impl Bench {
    /// A Studio holding `own` as its own build (or none), with `board` not
    /// yet granted.
    fn new(board: Board, own: Option<ModelBuild>) -> Self {
        let clock = Rc::clone(&board.clock);
        let board = Rc::new(RefCell::new(board));
        let granted = Rc::new(Cell::new(false));
        let tasks: TaskPool = Rc::new(RefCell::new(Vec::new()));
        let inbox: Rc<RefCell<VecDeque<DeviceInput>>> = Rc::new(RefCell::new(VecDeque::new()));
        let mut controller = StudioController::new({
            let clock = Rc::clone(&clock);
            move || clock.get()
        });
        controller.set_device_roster_config_for_test(crate::DeviceRosterConfig {
            expected_proto: lpc_wire::WIRE_PROTO_VERSION,
            flash_rung_ms: 400,
            flash_reopen_retry_ms: 100,
            ..Default::default()
        });
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
        let (access_tx, access_rx) = crate::app::studio::studio_view_channel::command_channel();
        controller.set_access_command_sender(access_tx);
        let cache = MemoryEngineCache::new();
        controller.set_engine_cache(Rc::new(cache.clone()));
        controller.set_device_transport(Rc::new(RigTransport {
            board: Rc::clone(&board),
            granted: Rc::clone(&granted),
        }));
        if let Some(own) = own {
            controller.set_own_build_source(Rc::new(MemoryOwnBuildSource::new(host_build(&own))));
        }
        Self {
            controller,
            board,
            granted,
            clock,
            inbox,
            tasks,
            cache,
            access_rx,
            lines: BTreeSet::new(),
            presses: 0,
            update_starts: 0,
            journal_seen: None,
        }
    }

    fn board(&self) -> std::cell::Ref<'_, Board> {
        self.board.borrow()
    }

    fn board_mut(&self) -> std::cell::RefMut<'_, Board> {
        self.board.borrow_mut()
    }

    fn now_ms(&self) -> u64 {
        (self.clock.get() * 1_000.0) as u64
    }

    /// Grant the port and sweep it in (a reload with the grant held).
    fn grant(&mut self) {
        self.granted.set(true);
        self.controller
            .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    }

    /// Grant, and wait for the board's hello to make it a card.
    fn connect_device(&mut self) -> DeviceId {
        self.grant();
        self.run_until("the board to get a card", |bench| {
            bench
                .controller
                .devices_for_test()
                .roster()
                .devices()
                .iter()
                .any(|d| d.evidence.update_facts().is_some() && d.evidence.has_hello())
        });
        self.controller.devices_for_test().roster().devices()[0].id
    }

    fn cache_engine(&self, build: &ModelBuild) {
        let entry = EngineCacheEntry::new(
            sha256_hex(&build.engine),
            build.engine.len() as u64,
            EngineSource::Installed,
            1.0,
        );
        block_on(self.cache.put(entry, build.engine.clone())).expect("cached");
    }

    fn cached(&self, build: &ModelBuild) -> bool {
        block_on(self.cache.has(&sha256_hex(&build.engine)))
    }

    fn held(&self, build: &ModelBuild) -> bool {
        block_on(self.cache.index())
            .get(&sha256_hex(&build.engine))
            .is_some_and(|entry| entry.held)
    }

    /// This browser's key, as an entry a board can hold.
    fn browser_key_entry(&mut self) -> SecretEntry {
        self.controller.drive_device_access();
        let key = self
            .controller
            .access_for_test()
            .browser_key()
            .expect("a browser key")
            .installable();
        key.entry(0)
    }

    fn press(&mut self, device: DeviceId, verb: &str, args: OfferArgs) {
        let path = self.controller.device_verb(device, verb);
        self.controller
            .press(path, args)
            .expect("a press never fails loudly");
        self.presses += 1;
    }

    fn press_update(&mut self, device: DeviceId) {
        self.run_until("the card to offer Update", |bench| {
            matches!(bench.standing(device), UpdateStanding::Available { .. })
        });
        self.press(device, "update-firmware", OfferArgs::new());
    }

    /// "Install Y" on a link still pending adoption: a core-only board says
    /// no hello, so it has no card to press on. The gesture the card would
    /// bind, folded as the press would fold it.
    fn install_on_pending(&mut self, device: DeviceId, version: &str) {
        self.controller
            .fold_device_input(DeviceInput::Action(lpa_devices::Action::Update {
                device,
                intent: lpa_devices::UpdateIntentFacts::Install {
                    version: version.to_string(),
                    allow_downgrade: false,
                },
            }));
        self.presses += 1;
    }

    fn pending_device(&self) -> Option<DeviceId> {
        self.controller
            .devices_for_test()
            .roster()
            .pending()
            .first()
            .map(lpa_devices::PendingLink::device_id)
    }

    fn standing(&self, device: DeviceId) -> UpdateStanding {
        let view = self.controller.device_roster_view();
        view.roster
            .devices
            .iter()
            .find(|card| card.id == device)
            .map(|card| self.controller.device_update_facts(card).standing)
            .unwrap_or_default()
    }

    fn outcome(&self, device: DeviceId) -> Option<UpdateOutcomeFacts> {
        self.controller
            .devices_for_test()
            .roster()
            .device(device)
            .and_then(|d| d.evidence.last_update_outcome)
    }

    /// The latest update outcome on the board, card or pending link.
    fn any_outcome(&self) -> Option<UpdateOutcomeFacts> {
        let roster = self.controller.devices_for_test().roster();
        roster
            .devices()
            .iter()
            .map(|d| &d.evidence)
            .chain(
                roster
                    .pending()
                    .iter()
                    .map(lpa_devices::PendingLink::evidence),
            )
            .find_map(|e| e.last_update_outcome)
    }

    fn any_facts(&self) -> bool {
        let roster = self.controller.devices_for_test().roster();
        roster
            .devices()
            .iter()
            .map(|d| &d.evidence)
            .chain(
                roster
                    .pending()
                    .iter()
                    .map(lpa_devices::PendingLink::evidence),
            )
            .any(|e| e.update_facts().is_some())
    }

    fn said(&self, line: &str) -> bool {
        self.lines.contains(line)
    }

    fn said_starting(&self, prefix: &str) -> bool {
        self.lines.iter().any(|line| line.starts_with(prefix))
    }

    fn assert_runs(&self, build: &ModelBuild) {
        let board = self.board();
        assert_eq!(
            board.rig.board.running_build().map(|b| b.version.clone()),
            Some(build.version.clone()),
            "lines: {:#?}",
            self.lines
        );
        assert!(board.rig.board.engine_valid());
        assert_eq!(board.rig.mode(), Some(SessionMode::EngineRunning));
    }

    /// One turn: time passes, tasks run, everything they queued folds.
    fn step(&mut self) {
        self.clock.set(self.clock.get() + STEP_SECS);
        pump(&self.tasks);
        let queued: Vec<DeviceInput> = self.inbox.borrow_mut().drain(..).collect();
        for input in queued {
            self.controller.fold_device_input(input);
        }
        while self.access_rx.peek_any(|_| true) {
            for command in block_on(self.access_rx.recv_coalesced()).unwrap_or_default() {
                if let crate::StudioCommand::Access(command) = command {
                    self.controller.apply_access_command(command);
                }
            }
        }
        block_on(self.controller.settle_device_records());
        self.collect_lines();
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }

    fn run_until(&mut self, what: &str, ready: impl Fn(&Self) -> bool) {
        for _ in 0..MAX_STEPS {
            self.step();
            if ready(self) {
                return;
            }
        }
        panic!(
            "timed out waiting for {what}; lines: {:#?}; roster: {:?}",
            self.lines,
            self.controller.device_roster_view().roster
        );
    }

    fn run_until_update_ends(&mut self, device: DeviceId) {
        self.run_until("the update activity to end", |bench| {
            bench.outcome(device).is_some()
                && bench
                    .controller
                    .devices_for_test()
                    .roster()
                    .device(device)
                    .is_some_and(|d| d.activity.is_none())
        });
    }

    fn collect_lines(&mut self) {
        let roster = self.controller.devices_for_test().roster();
        for (entry, note) in roster.journal().notes() {
            if self.journal_seen.is_some_and(|seen| entry.seq <= seen) {
                continue;
            }
            self.journal_seen = Some(entry.seq);
            if matches!(
                note,
                lpa_devices::JournalNote::ActivityStarted {
                    kind: lpa_devices::ActivityKind::Update
                }
            ) {
                self.update_starts += 1;
            }
        }
        let evidence = roster.devices().iter().map(|d| &d.evidence).chain(
            roster
                .pending()
                .iter()
                .map(lpa_devices::PendingLink::evidence),
        );
        for e in evidence {
            for line in e.recent_output() {
                if line.kind == TerminalKind::Studio {
                    self.lines.insert(line.text.clone());
                }
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
            false => Poll::Pending,
        }
    }
}

/// Poll every spawned task once (FIFO), dropping the finished ones.
fn pump(tasks: &TaskPool) {
    let mut taken: Vec<DeviceTaskFuture> = tasks.borrow_mut().drain(..).collect();
    let mut cx = Context::from_waker(core::task::Waker::noop());
    taken.retain_mut(|task| task.as_mut().poll(&mut cx).is_pending());
    let mut live = tasks.borrow_mut();
    taken.append(&mut live);
    *live = taken;
}

/// Drive an immediately-ready future (memory caches, the test fetch).
fn block_on<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(core::task::Waker::noop());
    let mut future = core::pin::pin!(future);
    for _ in 0..1_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
    panic!("a bench future did not complete");
}
