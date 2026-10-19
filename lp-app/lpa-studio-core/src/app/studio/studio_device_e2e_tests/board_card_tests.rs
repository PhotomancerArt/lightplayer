//! The board card, built in core, on real boards over the bench's wire: the
//! card the home view publishes for each board, its primary pressed by path
//! the way the web's button and the app agent press it, and every action
//! anywhere on it an offer the view's tree publishes.

use super::wifi_connect_tests::{KEY, joined_light_player, with_lan};
use super::*;
use crate::{UiBoardCard, UiCardAction, UiPrimary};

/// A board running no project has no panel to connect to: its primary is
/// the next step, Add a project (`push`, the project pick), never a greyed
/// Connect, and `connect` is refused with "No project". Once a project
/// runs, the card's primary is Connect, and pressing it by path opens the
/// session on the card once — its primary Done. Edit, the project bar's action, shows the editor on
/// that same session, and the docked card's primary is Done too.
#[test]
fn a_running_boards_card_presses_connect_by_path_and_opens_the_session_once() {
    let device = empty_light_player("dev000000card000001");
    let (mut bench, tasks) = identified(&device, "usb-card-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let id = bench.view().devices[0].id;
    let add = primary_offer(&card_of(&bench, id));
    assert_eq!(add.offer, bench.device_verb(id, "push"));
    assert_eq!(add.word, "Add a project");
    assert_eq!(add.refused, None, "never a greyed primary");
    assert!(matches!(add.draw, crate::UiActionDraw::ProjectPick { .. }));
    let connect = bench.device_verb(id, "connect");
    assert_eq!(
        bench.offer_reason(connect),
        crate::NO_PROJECT,
        "the agent reads why Connect waits"
    );
    every_card_action_is_offered(&mut bench);

    bench.push_gesture(id, bundled_example());
    bench.run_until(&tasks, "the board to run what was pushed", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });
    let connect = primary_offer(&card_of(&bench, id));
    assert_eq!(connect.offer, bench.device_verb(id, "connect"));
    assert_eq!(connect.word, "Connect");
    assert_eq!(connect.icon.as_deref(), Some("usb"));
    assert_eq!(connect.refused, None);
    every_card_action_is_offered(&mut bench);

    bench
        .press(&connect.offer, connect.args.clone())
        .expect("Connect opens the session");
    let uid = bench.registry()[0].uid.clone();
    assert_eq!(bench.lens_device_uid().as_deref(), Some(uid.as_str()));
    let session = bench.lens_session_id();
    for _ in 0..40 {
        bench.step(&tasks);
    }
    assert_eq!(
        bench.lens_session_id(),
        session,
        "one open: the same session, never a second"
    );
    let card = card_of(&bench, id);
    let done = primary_offer(&card);
    assert_eq!(done.offer, bench.device_verb(id, "done"));
    assert_eq!(done.word, "Done");
    assert_eq!(done.icon.as_deref(), Some("check"));
    every_card_action_is_offered(&mut bench);

    // Edit: the project bar's action (the panel foot's too).
    let edit = card
        .bar(crate::BarLayer::Project)
        .action
        .clone()
        .expect("Edit on the project bar");
    assert_eq!(edit.offer, bench.device_verb(id, "edit"));
    bench
        .press(&edit.offer, edit.args.clone())
        .expect("Edit shows the editor");
    assert_eq!(bench.lens_session_id(), session, "on the same session");
    let view = bench.controller.view();
    let crate::UiLensCard::Board(docked) =
        *view.lens_card.expect("the editor docks the board's card");
    assert_eq!(docked.device, id);
    assert_eq!(
        docked
            .name_bar
            .primary
            .as_ref()
            .and_then(|primary| primary.word()),
        Some("Done"),
        "Done on the docked card"
    );
    assert_eq!(
        docked.bar(crate::BarLayer::Project).action,
        None,
        "no Edit on the docked card: the editor shows the board"
    );
    assert_eq!(docked.panel, None, "the editor is the session's surface");
}

/// A push that ends well leaves its bar green for a few seconds, read off
/// the activity's end (`ActivityEnds`), then the bar says what it said.
#[test]
fn a_push_that_ends_well_is_done_for_three_seconds() {
    let device = empty_light_player("dev000000card000002");
    let (mut bench, tasks) = identified(&device, "usb-card-2");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let id = bench.view().devices[0].id;
    bench.push_gesture(id, bundled_example());
    let running = card_of(&bench, id)
        .bar(crate::BarLayer::Project)
        .work
        .clone()
        .expect("the push is the project bar's work");
    assert_eq!(running.state, crate::BarWorkState::Running);
    bench.run_until(&tasks, "the push to end", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let done = card_of(&bench, id)
        .bar(crate::BarLayer::Project)
        .work
        .clone()
        .expect("green for a moment");
    assert_eq!(done.state, crate::BarWorkState::Done);
    assert!(
        bench
            .controller
            .device_roster_view()
            .ends
            .get(&id)
            .is_some_and(|end| end.ok && end.kind == lpa_devices::ActivityKind::Push)
    );

    bench.clock.set(bench.clock.get() + crate::DONE_SHOWS_SECS);
    assert_eq!(
        card_of(&bench, id).bar(crate::BarLayer::Project).work,
        None,
        "the green goes"
    );
}

/// A board whose port is there but closed: the card's primary is Connect,
/// pressed by path, and the board comes back.
#[test]
fn an_attached_boards_card_connects_by_path() {
    let device = light_player("dev000000card000003");
    let (mut bench, tasks) = identified(&device, "usb-card-3");
    let id = bench.view().devices[0].id;
    bench.press_device(id, "disconnect", OfferArgs::new());
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.status == crate::DeviceStatus::Attached)
    });
    let card = card_of(&bench, id);
    let connect = primary_offer(&card);
    assert_eq!(connect.offer, bench.device_verb(id, "connect"));
    assert_eq!(connect.word, "Connect");
    assert_eq!(connect.icon.as_deref(), Some("usb"));
    every_card_action_is_offered(&mut bench);

    bench
        .press(&connect.offer, connect.args.clone())
        .expect("Connect opens the port");
    bench.run_until(&tasks, "the board to be ready again", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
}

/// Unplugged, a board is an offline card: its primary is Connect, which
/// reaches it by its cable, or over Wi‑Fi first when this browser remembers
/// where it is, then opens its panel (one press).
#[test]
fn an_offline_boards_card_reconnects_wifi_first() {
    let device = joined_light_player("dev000000card000004");
    let (mut bench, tasks) = identified(&device, "usb-card-4");
    let id = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();

    // Without a LAN to reach it on, unplugged, it reconnects by its cable.
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the board to go offline", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == id && card.status == crate::DeviceStatus::Offline)
    });
    let card = card_of(&bench, id);
    assert_eq!(card.presence, crate::UiBoardPresence::Offline);
    assert_eq!(card.status.mark, crate::CornerMark::Quiet);
    let reconnect = primary_offer(&card);
    assert_eq!(reconnect.offer, bench.device_verb(id, "connect"));
    assert_eq!(reconnect.word, "Connect");
    assert_eq!(reconnect.icon.as_deref(), Some("usb"), "by its cable");
    every_card_action_is_offered(&mut bench);

    // Back on its cable, the board says where it is on Wi‑Fi; unplugged
    // again, Wi‑Fi comes first.
    let _rig = with_lan(&mut bench, &device);
    bench.granted.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the board to go offline again", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == id && card.status == crate::DeviceStatus::Offline)
    });
    let wifi = primary_offer(&card_of(&bench, id));
    assert_eq!(wifi.offer, bench.device_verb(id, "connect"));
    assert_eq!(wifi.word, "Connect");
    assert_eq!(wifi.icon.as_deref(), Some("wifi"), "over Wi‑Fi first");
    every_card_action_is_offered(&mut bench);
}

/// An empty board's Project row says "none", and its primary is "Add a project":
/// the `push` offer, drawn as the picker, which the project bar does not
/// repeat; pressed by path with a source, the push runs.
#[test]
fn an_empty_boards_primary_adds_a_project_by_path() {
    let device = empty_light_player("dev000000card000005");
    let (mut bench, tasks) = identified(&device, "usb-card-5");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let id = bench.view().devices[0].id;
    let card = card_of(&bench, id);
    let bar = card.bar(crate::BarLayer::Project).clone();
    assert_eq!(bar.summary, "none");
    assert_eq!(bar.action, None, "the primary carries it, once");
    let add = primary_offer(&card);
    assert_eq!(add.word, "Add a project");
    assert_eq!(add.offer, bench.device_verb(id, "push"));
    assert!(matches!(add.draw, crate::UiActionDraw::ProjectPick { .. }));

    // The picker hands the press a source, as the web's pick does.
    bench
        .press(&add.offer, push_args(bundled_example()))
        .expect("the push starts");
    bench.run_until(&tasks, "the board to run what was added", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });
    every_card_action_is_offered(&mut bench);
}

/// A board given an older save than the library's newest says "Out of
/// date", and "Send latest" presses `push` by path with the project
/// preset; after it, the bar names the project again.
#[test]
fn an_out_of_date_boards_send_latest_presses_push_with_the_project() {
    let device = empty_light_player("dev000000card000006");
    let (mut bench, tasks) = identified(&device, "usb-card-6");
    let (_uid, project) = a_board_running_a_library_project(&mut bench, &tasks);
    let id = bench.view().devices[0].id;
    let slug = bench.library()[0].slug.clone();
    assert_eq!(
        card_of(&bench, id).bar(crate::BarLayer::Project).summary,
        slug,
        "at head, the bar names the project"
    );

    // A save lands in the library copy after the push.
    let mut handle = bench.store.open(project).expect("the library copy opens");
    handle
        .apply_update(
            lpc_model::LpPath::new("/notes.txt"),
            Some(b"edited here since the push"),
        )
        .expect("the edit writes");
    handle.record_save(2_000.0).expect("the edit is saved");
    drop(handle);
    bench.controller.request_library_refresh();
    drive(bench.controller.settle_library());
    // Past the push's few green seconds: the row's own tone.
    bench.clock.set(bench.clock.get() + crate::DONE_SHOWS_SECS);

    let bar = card_of(&bench, id).bar(crate::BarLayer::Project).clone();
    assert_eq!(bar.summary, slug, "the name stays in place");
    assert_eq!(bar.aside.as_deref(), Some("out of date"));
    assert_eq!(
        bar.tone,
        crate::UiStatusKind::Neutral,
        "the button carries it"
    );
    let send = bar.action.expect("Send latest");
    assert_eq!(send.word, "Send latest");
    assert_eq!(send.offer, bench.device_verb(id, "push"));
    assert_eq!(
        send.args.get(crate::PUSH_SOURCE_PARAM),
        Some(format!("library:{project}").as_str())
    );
    let card = card_of(&bench, id);
    assert_eq!(
        card.status.mark,
        crate::CornerMark::Fine,
        "an older copy is no alarm: the row says it, once"
    );
    every_card_action_is_offered(&mut bench);

    let banked = bench.registry()[0].association.clone();
    bench
        .press(&send.offer, send.args.clone())
        .expect("the latest goes out");
    bench.run_until(&tasks, "the latest to be banked", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
            && bench.registry()[0].association != banked
    });
    drive(bench.controller.settle_library());
    assert_eq!(
        bench
            .controller
            .device_roster_view()
            .board_projects
            .plays(id),
        &crate::BoardPlays::Given {
            project_uid: project.to_string(),
            at_head: true,
        }
    );
    // Past the second push's green seconds.
    bench.clock.set(bench.clock.get() + crate::DONE_SHOWS_SECS);
    let bar = card_of(&bench, id).bar(crate::BarLayer::Project).clone();
    assert_eq!(bar.summary, slug);
    assert_eq!(bar.tone, crate::UiStatusKind::Neutral);
}

/// A board on a live wire that never answers: the connection bar says it is
/// not responding, in Warning, and Retry presses `retry` by path.
#[test]
fn a_not_responding_boards_retry_presses_retry_by_path() {
    // A board Studio knows goes deaf: closed, then opened again, it answers
    // nothing at all (no heartbeat either), so the wire is open and silent.
    let device = light_player("dev000000card000007");
    let (mut bench, tasks) = identified(&device, "usb-card-7");
    let id = bench.view().devices[0].id;
    bench.press_device(id, "disconnect", OfferArgs::new());
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.status == crate::DeviceStatus::Attached)
    });
    device.set_drop_responses(true);
    bench.press_device(id, "connect", OfferArgs::new());
    bench.run_until(&tasks, "the board to stop answering", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.status == crate::DeviceStatus::NotResponding)
    });
    let id = bench.view().devices[0].id;
    let card = card_of(&bench, id);
    let bar = card.bar(crate::BarLayer::Connection).clone();
    assert_eq!(bar.summary, "USB · not responding");
    assert_eq!(
        bar.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    let retry = bar.action.expect("Retry");
    assert_eq!(retry.word, "Retry");
    assert_eq!(retry.offer, bench.device_verb(id, "retry"));
    assert_eq!(card.surface.state, crate::SurfaceState::NotAnswering);
    assert_eq!(
        card.surface
            .notice
            .as_ref()
            .map(|notice| notice.first.as_str()),
        Some("This board stopped responding over USB.")
    );
    let primary = primary_offer(&card);
    assert_eq!(primary.word, "Retry");
    assert_eq!(primary.offer, retry.offer, "the row's Retry, repeated");
    assert_eq!(
        card_of(&bench, id).status.mark,
        crate::CornerMark::Fine,
        "the row says it; the corner stays quiet"
    );
    every_card_action_is_offered(&mut bench);
    bench
        .press(&retry.offer, retry.args.clone())
        .expect("Retry asks again");
    let identifying = card_of(&bench, id).bar(crate::BarLayer::Connection).clone();
    assert!(
        identifying.work.is_some(),
        "asking again is the bar's work: {identifying:?}"
    );
}

/// The dead end of 2026-10-18 (a first-time user on lightplayer.app): a new
/// board whose firmware prints nothing at all, set up before it answered,
/// read "This board stopped responding over USB." with Retry — "duh, I'm
/// trying to program it". A board that never said hello and says nothing
/// is a board to install: the surface says it doesn't respond, the primary
/// is Install firmware (`flash`), pressed by path, and the Link row says
/// nothing loud.
#[test]
fn a_silent_board_that_never_said_hello_offers_install_not_retry() {
    let device = FakeEsp32Device::new(silent_script());
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-card-silent-1");
    bench.run_until(&tasks, "the new board to be identifying", |bench| {
        !bench.view().pending.is_empty()
    });
    // "Set up this device" while it identifies: the board becomes a card.
    bench
        .press("devices/new-1/adopt", OfferArgs::new())
        .expect("Set up this device");
    bench.run_until(&tasks, "its identify to settle silent", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });
    let id = bench.view().devices[0].id;
    assert_eq!(
        bench.view().devices[0].status,
        crate::DeviceStatus::NeedsAttention,
        "it never said hello: it needs firmware, it did not stop responding"
    );
    let card = card_of(&bench, id);
    assert_eq!(card.surface.state, crate::SurfaceState::Silent);
    let notice = card.surface.notice.clone().expect("the surface says it");
    assert_eq!(notice.first, "This board doesn't respond.");
    assert_eq!(
        notice.second.as_deref(),
        Some("Install the firmware to use it.")
    );
    let install = primary_offer(&card);
    assert_eq!(install.word, "Install firmware");
    assert_eq!(install.offer, bench.device_verb(id, "flash"));
    let link = card.bar(crate::BarLayer::Connection).clone();
    assert_eq!(link.summary, "USB · connected");
    assert_eq!(link.tone, crate::UiStatusKind::Neutral);
    assert_eq!(link.action, None, "no Retry on the Link row");
    every_card_action_is_offered(&mut bench);

    // Over firmware that will not say what it is, the flash is Lasting:
    // the user's second click.
    bench
        .press_lasting(
            &install.offer,
            OfferArgs::new().with(crate::FLASH_BOARD_PARAM, c6_board_choice().board_id),
        )
        .expect("Install firmware starts the flash");
    bench.run_until(&tasks, "the flash to run", |bench| {
        bench.view().devices.iter().any(|card| {
            card.activity
                .as_ref()
                .is_some_and(|work| work.kind == crate::DeviceActivityKind::Flash)
        })
    });
}

/// The same silent board left on its new-board card: once it settles, the
/// card says it doesn't respond, and Install firmware is the primary.
#[test]
fn a_silent_new_boards_card_offers_install() {
    let device = FakeEsp32Device::new(silent_script());
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-card-silent-2");
    bench.run_until(&tasks, "the silent verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware())
    });
    let id = bench.view().pending[0].device;
    let card = card_of(&bench, id);
    assert_eq!(card.surface.state, crate::SurfaceState::Silent);
    let notice = card.surface.notice.clone().expect("the surface says it");
    assert_eq!(notice.first, "This board doesn't respond.");
    assert_eq!(
        notice.second.as_deref(),
        Some("Install the firmware to use it.")
    );
    let install = primary_offer(&card);
    assert_eq!(install.word, "Install firmware");
    assert_eq!(install.offer.to_string(), "devices/new-1/flash");
    assert_eq!(
        card.bar(crate::BarLayer::Connection).summary,
        "USB · connected"
    );
    every_card_action_is_offered(&mut bench);
}

/// The hardware bar's details: Reset says why it waits while work runs
/// (the offer's own reason), Factory reset and Forget are its danger
/// section, and Forget is Lasting — the second click forgets the board.
#[test]
fn the_hardware_details_reset_says_why_it_waits_and_forget_is_lasting() {
    let device = empty_light_player("dev000000card000008");
    let (mut bench, tasks) = identified(&device, "usb-card-8");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let id = bench.view().devices[0].id;
    bench.push_gesture(id, bundled_example());
    let reset_path = bench.device_verb(id, "reset-board");
    let reason = bench.offer_reason(&reset_path);
    let card = card_of(&bench, id);
    let reset = card
        .bar(crate::BarLayer::Hardware)
        .details
        .sections
        .iter()
        .flat_map(|section| section.affordances.iter())
        .find(|action| action.offer == reset_path)
        .expect("Reset in the hardware details");
    assert_eq!(reset.refused.as_deref(), Some(reason.as_str()));
    every_card_action_is_offered(&mut bench);
    bench.run_until(&tasks, "the push to end", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none())
    });

    let danger = card_of(&bench, id)
        .bar(crate::BarLayer::Hardware)
        .details
        .sections
        .last()
        .cloned()
        .expect("the danger section");
    let words: Vec<&str> = danger.affordances.iter().map(|a| a.word.as_str()).collect();
    assert_eq!(words, ["Factory reset", "Forget"]);
    assert_eq!(danger.affordances[0].offer, bench.device_verb(id, "erase"));
    let forget = danger.affordances[1].clone();
    assert_eq!(forget.word, "Forget");
    assert_eq!(forget.offer, bench.device_verb(id, "forget"));
    bench
        .press_lasting(&forget.offer, forget.args.clone())
        .expect("Forget, after the arm");
    bench.step(&tasks);
    assert!(
        bench.view().devices.iter().all(|card| card.id != id),
        "forgotten"
    );
}

/// Yona's C6 over USB, 2026-10-19: Factory reset was under Firmware, and
/// after it the card read "This board stopped responding over USB." with a
/// Retry that timed out; only a replug reached the blank board. Factory
/// reset is the Hardware row's now, and after it the card is the blank
/// board — even when the erased board says nothing (the flasher's stub, or
/// a boot loop lost to re-enumerating): "This board doesn't have LightPlayer
/// firmware.", Install firmware as the primary, no Retry. Install firmware
/// works in one press from there.
#[test]
fn a_factory_reset_from_hardware_lands_on_install_firmware_with_no_retry() {
    let device = light_player("dev_wipesilent");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-card-reset");
    bench.run_until(&tasks, "the hello to settle the link", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let id = bench.view().devices[0].id;
    let erase = bench.device_verb(id, "erase");
    let card = card_of(&bench, id);
    let in_bar = |card: &UiBoardCard, layer: crate::BarLayer| {
        card.bar(layer)
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .any(|action| action.offer == erase)
    };
    assert!(in_bar(&card, crate::BarLayer::Hardware), "under Hardware");
    assert!(
        !in_bar(&card, crate::BarLayer::Firmware),
        "not under Firmware"
    );

    bench.erase_plan.set(ErasePlan::Silent);
    bench
        .press_lasting(&erase, OfferArgs::new())
        .expect("Factory reset, after the arm");
    bench.run_until(&tasks, "the erase to end", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });

    let card = card_of(&bench, id);
    let notice = card.surface.notice.as_ref().expect("the surface says it");
    assert_eq!(
        (notice.first.as_str(), notice.second.as_deref()),
        (
            "This board doesn't have LightPlayer firmware.",
            Some("Install the firmware to use it.")
        ),
        "{:?}",
        card.surface
    );
    let install = primary_offer(&card);
    assert_eq!(install.word, "Install firmware");
    assert_eq!(install.offer, bench.device_verb(id, "flash"));
    let retry = bench.device_verb(id, "retry");
    bench.not_offered(retry);
    assert_eq!(card.bar(crate::BarLayer::Firmware).summary, "none");
    assert_eq!(
        card.bar(crate::BarLayer::Connection).action,
        None,
        "the Link row offers no Retry either"
    );
    every_card_action_is_offered(&mut bench);

    // Install firmware, one press: the flash parks the chip and writes it.
    let choice = c6_board_choice();
    bench
        .press(
            &install.offer,
            OfferArgs::new().with("board", &choice.board_id),
        )
        .expect("Install firmware starts the flash");
    bench.run_until(&tasks, "the installed board to land Ready", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready" && card.activity.is_none())
    });
}

/// Yona's C6, 2026-10-19: "the card periodically 'refreshes' which is
/// jarring and resets the ui." An erased native-USB chip re-enumerates on
/// every watchdog reset of its boot loop; each one took the board offline
/// and back, and its card moved from Online boards to Offline boards and
/// back, remounted (open details closed, an armed button disarmed). A port
/// that comes back within the grace is the same board that never left: the
/// blank card holds — Online, the blank board, Install firmware — through
/// every turn of the loop. A board that stays gone is offline after the
/// grace.
#[test]
fn an_erased_board_re_enumerating_holds_its_card_until_it_is_really_gone() {
    let device = light_player("dev_bootloop");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-card-bootloop");
    bench.run_until(&tasks, "the hello to settle the link", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let id = bench.view().devices[0].id;
    bench.erase_plan.set(ErasePlan::Silent);
    let erase = bench.device_verb(id, "erase");
    bench
        .press_lasting(&erase, OfferArgs::new())
        .expect("Factory reset, after the arm");
    bench.run_until(&tasks, "the erase to end", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let blank = |bench: &DeviceBench| {
        let card = card_of(bench, id);
        assert_eq!(
            card.presence,
            crate::UiBoardPresence::Online,
            "{:?}",
            card.surface
        );
        assert_eq!(
            card.surface.state,
            crate::SurfaceState::NoFirmware,
            "{:?}",
            card.surface
        );
        assert_eq!(primary_offer(&card).word, "Install firmware");
    };
    blank(&bench);

    // Three turns of the boot loop: the port leaves, and is back (under the
    // same grant) a moment later. The card never moves or changes.
    let steps = |seconds: f64| (seconds / STEP_MS) as usize;
    for _ in 0..3 {
        bench.granted.set(false);
        bench
            .controller
            .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
        for _ in 0..steps(1.0) {
            bench.step(&tasks);
            blank(&bench);
        }
        bench.granted.set(true);
        bench
            .controller
            .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
        for _ in 0..steps(4.0) {
            bench.step(&tasks);
            blank(&bench);
        }
    }

    // Unplugged for good: offline once the grace has passed.
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the board to be offline", |bench| {
        card_of(bench, id).presence == crate::UiBoardPresence::Offline
    });
}

/// The layout question rides the firmware bar: while it is open the bar's
/// details are raised (the web opens them) and hold the Layout panel, its
/// work is the flash's own step, and Continue — Lasting — is pressed by
/// path from the panel and finishes the update.
#[test]
fn the_layout_question_raises_the_firmware_details_and_continue_runs_by_path() {
    let device = legacy_light_player(Vec::new());
    let (mut bench, tasks) = identified(&device, "usb-card-layout-1");
    let target = bench.view().devices[0].id;
    update(&mut bench, target);
    let panel = layout_panel(&mut bench, &tasks, target);

    let card = card_of(&bench, target);
    assert_eq!(
        card.surface.state,
        crate::SurfaceState::ConfirmationRequired,
        "the flash waits for the answer: the surface asks, not \"installing\""
    );
    let bar = card.bar(crate::BarLayer::Firmware).clone();
    assert!(bar.details.raised, "the question rises on its own");
    assert!(
        bar.details
            .panels
            .iter()
            .any(|shown| matches!(shown, crate::UiDetailPanel::Layout(shown) if *shown == panel)),
        "{:?}",
        bar.details.panels
    );
    assert_eq!(
        bar.work, None,
        "a flash waiting for its answer is not working"
    );
    assert_eq!(bar.aside.as_deref(), Some(crate::CONFIRMATION_REQUIRED));
    assert_eq!(
        bar.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    assert_eq!(bar.opens_details, Some("Review"), "no dead end: Review");
    assert_eq!(
        bar.details
            .notice()
            .and_then(|notice| notice.sentence.as_deref()),
        Some(panel.title.as_str())
    );
    every_card_action_is_offered(&mut bench);

    let continue_path = panel.continue_action.expect("a migration can continue");
    assert_eq!(continue_path, bench.device_verb(target, "continue-update"));
    bench
        .press_lasting(&continue_path, OfferArgs::new())
        .expect("Continue, after the arm");
    settle(&mut bench, &tasks);
    assert!(bench.view().devices[0].last_outcome.clone().unwrap().ok);
    assert!(
        !card_of(&bench, target)
            .bar(crate::BarLayer::Firmware)
            .details
            .raised,
        "nothing left to answer"
    );
}

/// While the layout question is open, Download backup is the question's
/// own button: the firmware details' verbs do not offer it a second time.
#[test]
fn the_layout_question_alone_offers_download_backup() {
    let device = legacy_light_player(Vec::new());
    let (mut bench, tasks) = identified(&device, "usb-card-layout-3");
    let target = bench.view().devices[0].id;
    update(&mut bench, target);
    let panel = layout_panel(&mut bench, &tasks, target);
    let download = bench.device_verb(target, "download-backup");
    assert_eq!(panel.download, download, "the question presses the verb");

    let details_verbs = |bench: &DeviceBench| -> Vec<crate::OfferPath> {
        card_of(bench, target)
            .bar(crate::BarLayer::Firmware)
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .map(|action| action.offer.clone())
            .collect()
    };
    assert!(
        !details_verbs(&bench).contains(&download),
        "one Download backup, the question's: {:?}",
        details_verbs(&bench)
    );
    // The offer itself stands: the question's button presses it.
    bench.offered(&download);
}

/// A board holding its files for a layout change: the firmware bar says so
/// and its action is Finish update, at `…/finish-update`.
#[test]
fn a_board_holding_its_files_offers_finish_update_as_the_firmware_action() {
    let device = legacy_light_player(Vec::new());
    let (mut bench, tasks) = identified(&device, "usb-card-layout-2");
    let target = bench.view().devices[0].id;
    update(&mut bench, target);
    let panel = layout_panel(&mut bench, &tasks, target);
    device.interrupt_next_plan_after(1);
    press(&mut bench, &panel.continue_action.unwrap());
    settle(&mut bench, &tasks);
    bench.run_until(&tasks, "the held board to say so", |bench| {
        bench
            .controller
            .device_roster_view()
            .layout
            .get(&target)
            .is_some_and(|layout| layout.finish_update.is_some())
    });

    let card = card_of(&bench, target);
    let bar = card.bar(crate::BarLayer::Firmware).clone();
    assert_eq!(bar.aside.as_deref(), Some(crate::UPDATE_INTERRUPTED));
    assert_eq!(
        bar.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    let finish = bar.action.clone().expect("Resume");
    assert_eq!(finish.word, "Resume");
    assert_eq!(finish.offer, bench.device_verb(target, "finish-update"));
    // The surface says it, and its primary is the same Resume.
    assert_eq!(card.surface.state, crate::SurfaceState::UpdateInterrupted);
    let primary = primary_offer(&card);
    assert_eq!(primary.word, "Resume update");
    assert_eq!(
        primary.offer, finish.offer,
        "the row's own button, repeated"
    );
    // The interrupted flash's failure is said first; the waiting files'
    // line is the row's own notice after it.
    assert!(
        bar.details.sections.iter().any(|section| section
            .sentence
            .as_deref()
            .is_some_and(|line| line.contains("waiting"))),
        "the waiting-files line is in the details: {:#?}",
        bar.details.sections
    );
    every_card_action_is_offered(&mut bench);
}

/// The cable pulled mid filesystem write: the board boots formatted, and
/// its Project row says its data was lost (O3), with the stored backup's
/// way back as its action (Restore, `…/restore-files`) and Reset (Start
/// fresh) beside it as the way out. Reset, pressed by path, lets the files
/// go: the state leaves the card, and this browser keeps the choice for the
/// board (the web edge's storage), so a reload does not bring it back.
#[test]
fn a_formatted_boards_project_row_restores_its_data() {
    let device = legacy_light_player(Vec::new());
    let (mut bench, tasks) = identified(&device, "usb-card-layout-3");
    let target = bench.view().devices[0].id;
    update(&mut bench, target);
    let panel = layout_panel(&mut bench, &tasks, target);
    device.interrupt_next_plan_after(4);
    press(&mut bench, &panel.continue_action.unwrap());
    settle(&mut bench, &tasks);
    bench.run_until(&tasks, "the card to offer the backup", |bench| {
        bench
            .controller
            .device_roster_view()
            .layout
            .get(&target)
            .is_some_and(|layout| layout.restore.is_some())
    });

    let card = card_of(&bench, target);
    let bar = card.bar(crate::BarLayer::Project).clone();
    assert_eq!(
        (bar.summary.as_str(), bar.aside.as_deref()),
        (crate::PROJECT_NONE, Some(crate::DATA_LOST))
    );
    assert_eq!(
        bar.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    let restore = bar.action.clone().expect("Restore");
    assert_eq!(restore.word, "Restore");
    assert_eq!(restore.offer, bench.device_verb(target, RESTORE_FILES));
    assert_eq!(card.surface.state, crate::SurfaceState::DataLost);
    assert_eq!(
        primary_offer(&card).offer,
        restore.offer,
        "the primary is the row's Restore"
    );
    let reset = bar.way_out.clone().expect("Reset beside it");
    assert_eq!(reset.word, "Reset");
    assert_eq!(reset.offer, bench.device_verb(target, crate::START_FRESH));
    assert_ne!(
        card_of(&bench, target)
            .bar(crate::BarLayer::Firmware)
            .aside
            .as_deref(),
        Some(crate::DATA_LOST),
        "said once, on the Project row"
    );
    assert!(
        bar.details
            .panels
            .iter()
            .any(|panel| matches!(panel, crate::UiDetailPanel::RestoreFromFile { .. })),
        "Restore from a backup file… beside it"
    );
    let verbs: Vec<String> = bar
        .details
        .sections
        .iter()
        .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
        .collect();
    assert!(verbs.contains(&"Download backup".to_string()), "{verbs:?}");
    every_card_action_is_offered(&mut bench);

    // Start fresh: Routine, the agent's to press too; nothing on the board
    // changes, and the notice goes.
    let kept = Rc::new(RefCell::new(Vec::<String>::new()));
    let sink = Rc::clone(&kept);
    bench
        .controller
        .set_on_fresh_starts(move |json| sink.borrow_mut().push(json.to_string()));
    let start_fresh = bench.device_verb(target, crate::START_FRESH);
    assert!(bench.offered(&start_fresh).consequence().is_routine());
    bench
        .press(&start_fresh, OfferArgs::new())
        .expect("Start fresh");
    let bar = card_of(&bench, target)
        .bar(crate::BarLayer::Project)
        .clone();
    assert_ne!(
        bar.aside.as_deref(),
        Some(crate::DATA_LOST),
        "the state is gone"
    );
    bench.not_offered(&start_fresh);
    assert_eq!(
        kept.borrow().last().map(String::as_str),
        Some(r#"["60:55:f9:0a:0b:0c"]"#),
        "this browser keeps the choice, by the board's MAC"
    );
    every_card_action_is_offered(&mut bench);
}

/// A fresh install's first boot formats its storage, and so does every
/// board Studio installs: a board this browser never saw hold anything has
/// lost nothing. Its card is the ordinary one — no "Its files are gone", no
/// restore — and its primary is the next step, Add a project (the connected
/// feel gate's finding 1).
#[test]
fn a_freshly_installed_board_is_not_told_its_files_are_gone() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_BOARD_MAC)
            .with_fs_boot_state(lpc_wire::FsBootState::Formatted)
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    let (mut bench, tasks) = identified(&device, "usb-card-fresh-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let id = bench.view().devices[0].id;
    let card = card_of(&bench, id);
    let firmware = card.bar(crate::BarLayer::Firmware);
    assert_ne!(
        firmware.tone,
        crate::UiStatusKind::Attention,
        "{firmware:?}"
    );
    assert_ne!(
        card.bar(crate::BarLayer::Project).aside.as_deref(),
        Some(crate::DATA_LOST),
        "a fresh install lost nothing"
    );
    for verb in [crate::START_FRESH, RESTORE_FILES, "restore-from-file"] {
        let path = bench.device_verb(id, verb);
        bench.not_offered(&path);
    }
    assert_eq!(card.bar(crate::BarLayer::Project).summary, "none");
    assert_eq!(primary_offer(&card).word, "Add a project");
    every_card_action_is_offered(&mut bench);
}

/// An `fs-tree` board that refused its file store (`fs: refused`): its
/// files are kept on flash and no click here can safely read or move them.
/// The firmware bar says "Data unreadable" in Attention with no action, the
/// details name the tool that reads the files, and nothing that would
/// restore, start fresh or move its files is offered — nor is the primary
/// one of them.
#[test]
fn a_board_that_refused_its_store_says_its_data_is_unreadable_and_offers_no_way_to_move_it() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_BOARD_MAC)
            .with_fs_boot_state(lpc_wire::FsBootState::Refused)
            .with_fs_tree()
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    let (mut bench, tasks) = identified(&device, "usb-card-refused-1");
    let id = bench.view().devices[0].id;
    bench.run_until(&tasks, "the refused store to reach the card", |bench| {
        bench
            .controller
            .device_roster_view()
            .layout
            .get(&id)
            .is_some_and(|layout| layout.store_refused)
    });
    let card = card_of(&bench, id);
    let firmware = card.bar(crate::BarLayer::Firmware);
    assert_eq!(firmware.aside.as_deref(), Some(crate::DATA_UNREADABLE));
    assert_eq!(
        firmware.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    assert_eq!(card.surface.state, crate::SurfaceState::DataUnreadable);
    assert_eq!(firmware.action, None, "no click here is safe");
    assert_eq!(
        firmware
            .details
            .notice()
            .and_then(|notice| notice.sentence.as_deref()),
        Some(crate::app::devices::device_layout_view::REFUSED_STORE_LINE),
        "the long line is in the details, not on the bar"
    );
    for verb in [
        RESTORE_FILES,
        RESTORE_FROM_FILE,
        DOWNLOAD_BACKUP,
        crate::START_FRESH,
        "finish-update",
    ] {
        let path = bench.device_verb(id, verb);
        bench.not_offered(&path);
    }
    if let Some(UiPrimary::Offer(primary)) = &card.name_bar.primary {
        for verb in [RESTORE_FILES, "finish-update", "flash"] {
            assert_ne!(
                primary.offer,
                bench.device_verb(id, verb),
                "the primary moves no files"
            );
        }
    }
    every_card_action_is_offered(&mut bench);
}

/// A littlefs build that found its board's files in a tree store (`fs:
/// refused` in a hello without `fs.tree`; ADR
/// `2026-10-10-fs-tree-refused-state-and-update-guard`, amended 2026-10-16):
/// the bar says the files need newer firmware, the details say they are kept
/// and what brings them back, and nothing that would write over the store —
/// a restore, Start fresh, a migration's Finish update — is offered.
#[test]
fn a_littlefs_board_on_a_tree_store_says_its_files_need_newer_firmware() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_BOARD_MAC)
            .with_fs_boot_state(lpc_wire::FsBootState::Refused)
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    let (mut bench, tasks) = identified(&device, "usb-card-tree-held-1");
    let id = bench.view().devices[0].id;
    bench.run_until(&tasks, "the held tree store to reach the card", |bench| {
        bench
            .controller
            .device_roster_view()
            .layout
            .get(&id)
            .is_some_and(|layout| layout.store_needs_newer_firmware)
    });
    let card = card_of(&bench, id);
    let firmware = card.bar(crate::BarLayer::Firmware);
    assert_eq!(firmware.aside.as_deref(), Some(crate::UPDATE_REQUIRED));
    assert_eq!(
        firmware.tone,
        crate::UiStatusKind::Neutral,
        "the surface says it; the row goes quiet"
    );
    assert_eq!(card.surface.state, crate::SurfaceState::UpdateRequired);
    assert_eq!(firmware.action, None);
    let line = crate::app::devices::device_layout_view::TREE_STORE_HELD_LINE;
    assert_eq!(
        firmware
            .details
            .notice()
            .and_then(|notice| notice.sentence.as_deref()),
        Some(line)
    );
    assert!(line.contains("kept") && line.contains("update the firmware"));
    for verb in [
        RESTORE_FILES,
        RESTORE_FROM_FILE,
        DOWNLOAD_BACKUP,
        crate::START_FRESH,
        "finish-update",
    ] {
        let path = bench.device_verb(id, verb);
        bench.not_offered(&path);
    }
    every_card_action_is_offered(&mut bench);
}

/// A sim sends no heartbeat: it only says what it runs when asked. As its
/// identify ends it is asked, so a sim running nothing reads "No project"
/// with Add a project, as a board does — never "Project not known" with no
/// way on (the connected feel gate's finding 2).
#[test]
fn a_running_sim_with_no_project_offers_add_a_project() {
    let (uid, base_mac) = sim_identity();
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(&uid, "Sim"))
            .with_base_mac(&base_mac),
    )));
    let (mut bench, tasks, _uid) = DeviceBench::with_sim(&device, SIM_TARGET);
    let sim = bench.view().devices[0].id;
    bench.press_device(sim, "reconnect", OfferArgs::new());
    bench.run_until(&tasks, "the sim to say it runs nothing", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && card.loaded_project == lpa_devices::view::LoadedProject::Empty
        })
    });
    let card = card_of(&bench, sim);
    assert_eq!(card.bar(crate::BarLayer::Project).summary, "none");
    assert_eq!(
        card.bar(crate::BarLayer::Connection).summary,
        "Simulated · running"
    );
    let add = primary_offer(&card);
    assert_eq!(add.word, "Add a project");
    assert_eq!(add.offer, bench.device_verb(sim, "push"));
    every_card_action_is_offered(&mut bench);
}

/// The dead end of 2026-10-17 (the desk's fixture C6 on lightplayer.app): a
/// LightPlayer on an older wire, on USB, seen here for the first time (its
/// hello is unreadable, so it names no MAC and stays a new board). Its
/// card says what it is and what the button does, never a black top and no
/// button; the Project row says "not known", never a false "none"; the
/// primary is Update firmware — Routine, since the update keeps the
/// project and the files — and pressing it by path with a board, as the
/// agent may, starts the flash. (A board Studio remembers, whose board
/// resolves, gets the one-click `update-firmware` instead:
/// `primary_action`'s and `board_surface`'s own tests.)
#[test]
fn an_older_lightplayers_card_updates_its_firmware_by_path() {
    const HELLO_PROTO_32: &str =
        include_str!("../../../../../../lp-core/lpc-wire/testdata/hello-proto32-xiao-c6.json");
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_base_mac(BENCH_BOARD_MAC)
            .with_hello_json(HELLO_PROTO_32),
    )));
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-card-older-1");
    bench.run_until(
        &tasks,
        "the board to read as an older LightPlayer",
        |bench| {
            bench.view().pending.first().is_some_and(|pending| {
                matches!(
                    pending.firmware_face,
                    lpa_devices::view::FirmwareFace::OlderLightPlayer { .. }
                )
            })
        },
    );
    let id = bench.view().pending[0].device;
    let card = card_of(&bench, id);
    assert_eq!(card.surface.state, crate::SurfaceState::OldFirmware);
    let notice = card.surface.notice.clone().expect("the surface says it");
    assert_eq!(notice.first, "This board is running old firmware.");
    assert_eq!(
        notice.second.as_deref(),
        Some("Update the firmware to connect and edit.")
    );
    assert_eq!(
        card.bar(crate::BarLayer::Project).summary,
        crate::PROJECT_UNKNOWN,
        "it plays something this Studio cannot read"
    );
    let update = primary_offer(&card);
    assert_eq!(update.word, "Update firmware");
    assert_eq!(update.offer.to_string(), "devices/new-1/flash");
    assert!(
        bench.offered(&update.offer).consequence().is_routine(),
        "it keeps the project and the files: one click, the agent may press it"
    );
    every_card_action_is_offered(&mut bench);

    let board = crate::flash_offer(Some("esp32c6")).candidates[0]
        .board_id
        .clone();
    bench
        .press(
            &update.offer,
            OfferArgs::new().with(crate::FLASH_BOARD_PARAM, board),
        )
        .expect("Update firmware starts the flash");
    bench.run_until(&tasks, "the flash to run", |bench| {
        bench.view().devices.iter().any(|card| {
            card.activity
                .as_ref()
                .is_some_and(|work| work.kind == crate::DeviceActivityKind::Flash)
        })
    });
}

/// The card the home view publishes for `device`.
fn card_of(bench: &DeviceBench, device: crate::DeviceId) -> UiBoardCard {
    bench
        .controller
        .view()
        .home
        .expect("the home view")
        .devices
        .cards
        .into_iter()
        .find(|card| card.device == device)
        .unwrap_or_else(|| panic!("no card for {device:?}"))
}

/// The card's primary, which must be an offer.
#[track_caller]
fn primary_offer(card: &UiBoardCard) -> UiCardAction {
    match &card.name_bar.primary {
        Some(UiPrimary::Offer(action)) => action.clone(),
        other => panic!("the primary is an offer, not {other:?}"),
    }
}

/// Every action on every card the view publishes names an offer the
/// view's tree publishes.
#[track_caller]
fn every_card_action_is_offered(bench: &mut DeviceBench) {
    let view = bench.controller.view();
    let home = view.home.expect("the home view");
    assert!(!home.devices.cards.is_empty(), "the view publishes cards");
    for card in &home.devices.cards {
        for path in card.offer_paths() {
            assert!(
                view.offers.get(path).is_some(),
                "{path} is on the card and not offered"
            );
        }
    }
}

/// A board whose firmware prints nothing at all, ever: no ROM banner, no
/// boot line, no hello (a factory firmware that keeps quiet on USB).
fn silent_script() -> FakeDeviceScript {
    let mut script = FakeDeviceScript::new(FakeBootState::ForeignFirmware);
    script.rom_banner.clear();
    script.foreign_banner.clear();
    script
}
