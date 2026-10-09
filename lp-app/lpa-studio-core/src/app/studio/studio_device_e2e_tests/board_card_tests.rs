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
    let device = empty_light_player("dev000000card00001");
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
    let crate::UiLensCard::Board { card, .. } =
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
    let device = empty_light_player("dev000000card00002");
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
    let device = light_player("dev000000card00003");
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
    let device = joined_light_player("dev000000card00004");
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
