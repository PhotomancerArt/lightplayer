//! The board card, built in core, on real boards over the bench's wire: the
//! card the home view publishes for each board, its primary pressed by path
//! the way the web's button and the app agent press it, and every action
//! anywhere on it an offer the view's tree publishes.

use super::wifi_connect_tests::{KEY, joined_light_player, with_lan};
use super::*;
use crate::{UiBoardCard, UiCardAction, UiPrimary};

/// A board with nothing on it cannot be edited yet; once a project runs,
/// the card's primary is Edit, and pressing it by path opens the editor on
/// the board once — and the docked lens card has no primary while the
/// editor holds it.
#[test]
fn a_running_boards_card_presses_edit_by_path_and_opens_the_lens_once() {
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
    assert_eq!(
        card_of(&bench, id).name_bar.primary,
        Some(UiPrimary::Unavailable {
            word: "Edit".to_string(),
            icon: "edit".to_string(),
            reason: crate::NOTHING_TO_EDIT.to_string(),
        })
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
    let edit = primary_offer(&card_of(&bench, id));
    assert_eq!(edit.offer, bench.device_verb(id, "edit"));
    assert_eq!(edit.word, "Edit");
    assert_eq!(edit.icon.as_deref(), Some("edit"));
    every_card_action_is_offered(&mut bench);

    bench
        .press(&edit.offer, edit.args.clone())
        .expect("Edit opens the editor");
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
    let view = bench.controller.view();
    let crate::UiLensCard::Board(card) =
        *view.lens_card.expect("the editor docks the board's card");
    assert_eq!(card.device, id);
    assert_eq!(
        card.name_bar.primary, None,
        "no primary while the editor holds it"
    );
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

/// Unplugged, a board is an offline card: its primary reconnects by its
/// cable, or over Wi‑Fi first when this browser remembers where it is.
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
    assert_eq!(reconnect.offer, bench.device_verb(id, "reconnect"));
    assert_eq!(reconnect.word, "Connect");
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
    assert_eq!(wifi.offer, bench.device_verb(id, "connect-wifi"));
    assert_eq!(wifi.word, "Connect");
    assert_eq!(wifi.icon.as_deref(), Some("wifi"));
    every_card_action_is_offered(&mut bench);
}

/// An empty board's project bar offers "Add a project": the `push` offer,
/// drawn as the picker; pressed by path with a source, the push runs.
#[test]
fn an_empty_boards_project_action_adds_a_project_by_path() {
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
    let bar = card_of(&bench, id).bar(crate::BarLayer::Project).clone();
    assert_eq!(bar.summary, "Nothing on it yet");
    let add = bar.action.expect("Add a project");
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

    let bar = card_of(&bench, id).bar(crate::BarLayer::Project).clone();
    assert_eq!(bar.summary, "Out of date");
    assert_eq!(bar.aside.as_deref(), Some(slug.as_str()));
    assert_eq!(bar.tone, crate::UiStatusKind::Attention);
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
        crate::CornerMark::Notice(crate::UiStatusKind::Attention),
        "the corner says it too"
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
    let bar = card_of(&bench, id).bar(crate::BarLayer::Connection).clone();
    assert_eq!(bar.summary, "USB · not responding");
    assert_eq!(bar.tone, crate::UiStatusKind::Warning);
    let retry = bar.action.expect("Retry");
    assert_eq!(retry.word, "Retry");
    assert_eq!(retry.offer, bench.device_verb(id, "retry"));
    assert_eq!(
        card_of(&bench, id).status.mark,
        crate::CornerMark::Notice(crate::UiStatusKind::Warning)
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

/// The hardware bar's details: Reset says why it waits while work runs
/// (the offer's own reason), and Forget is Lasting — the second click
/// forgets the board.
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

    let forget = card_of(&bench, id)
        .bar(crate::BarLayer::Hardware)
        .details
        .sections
        .last()
        .expect("the danger section")
        .affordances
        .first()
        .cloned()
        .expect("Forget");
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

    let bar = card_of(&bench, target)
        .bar(crate::BarLayer::Firmware)
        .clone();
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
        bar.work.as_ref().map(|work| work.words.as_str()),
        Some("Waiting for your answer…"),
        "the running flash names its step"
    );
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

    let bar = card_of(&bench, target)
        .bar(crate::BarLayer::Firmware)
        .clone();
    assert_eq!(bar.summary, "Files waiting");
    assert_eq!(bar.tone, crate::UiStatusKind::Attention);
    let finish = bar.action.clone().expect("Finish update");
    assert_eq!(finish.word, "Finish update");
    assert_eq!(finish.offer, bench.device_verb(target, "finish-update"));
    assert!(
        bar.details
            .notice()
            .and_then(|notice| notice.sentence.as_deref())
            .is_some_and(|line| line.contains("waiting")),
        "the waiting-files line is the notice"
    );
    every_card_action_is_offered(&mut bench);
}

/// The cable pulled mid filesystem write: the board boots formatted, and
/// the firmware bar's action puts the stored backup back, at
/// `…/restore-files`.
#[test]
fn a_formatted_boards_firmware_action_restores_its_files() {
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

    let bar = card_of(&bench, target)
        .bar(crate::BarLayer::Firmware)
        .clone();
    assert_eq!(bar.summary, "Its files need restoring");
    let restore = bar.action.clone().expect("Restore files");
    assert_eq!(restore.word, "Restore files");
    assert_eq!(restore.offer, bench.device_verb(target, RESTORE_FILES));
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
