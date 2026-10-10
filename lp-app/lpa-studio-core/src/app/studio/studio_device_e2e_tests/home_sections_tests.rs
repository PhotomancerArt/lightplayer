//! The home page's sections over the real device bench: a board plugged in
//! is under Online boards, a detached one moves to Offline boards, and every
//! offer the Connect a board section names is published in the tree (so the
//! web has something to press for each square, and the agent something to
//! `act`).
//!
//! The sections are core data (`UiHomeView::sections`); these tests read them
//! the way the web does, from the controller's view, and press offers by path.

use super::*;

/// Plug a board in: it is under Online boards. Detach it: the same board,
/// still known, moves to Offline boards. Plug it back: it returns. The
/// page never shows it in both, and it is not a newcomer's page at any
/// point after the board was seen.
#[test]
fn a_board_plugged_in_is_online_and_a_detached_one_moves_offline() {
    let device = light_player("dev000000daqf6dvhm1");
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-home-sections-1");
    bench.run_until(&tasks, "the board to identify", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let id = bench.view().devices[0].id;

    let sections = bench
        .controller
        .view()
        .home
        .expect("the home view")
        .sections;
    assert_eq!(
        sections
            .online
            .iter()
            .map(|board| (board.id, board.kind))
            .collect::<Vec<_>>(),
        [(id, crate::UiHomeBoardKind::Connected)],
        "{sections:?}"
    );
    assert_eq!(sections.online[0].status, "Ready");
    assert!(sections.offline.is_empty(), "{sections:?}");
    assert!(!sections.newcomer, "a board is here: not a first visit");

    // The user closes the port, then the board leaves the bus.
    bench.press_device(id, "disconnect", OfferArgs::new());
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label.starts_with("Attached"))
    });
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

    let sections = bench
        .controller
        .view()
        .home
        .expect("the home view")
        .sections;
    assert!(sections.online.is_empty(), "{sections:?}");
    assert_eq!(
        sections
            .offline
            .iter()
            .map(|board| (board.id, board.kind))
            .collect::<Vec<_>>(),
        [(id, crate::UiHomeBoardKind::Remembered)],
        "{sections:?}"
    );
    assert_eq!(
        sections.offline[0].row_verbs,
        ["connect-wifi", "reconnect"],
        "a remembered board's row may offer these"
    );
    assert!(!sections.newcomer, "a remembered board is not a newcomer");
    // The verbs the row names are published for the board, so the web can
    // resolve them.
    let reconnect = bench.device_verb(id, "reconnect");
    bench.offered(reconnect);

    // And back in.
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
    let sections = bench
        .controller
        .view()
        .home
        .expect("the home view")
        .sections;
    assert_eq!(sections.online.len(), 1, "{sections:?}");
    assert!(sections.offline.is_empty(), "{sections:?}");
}

/// The join reaches the page: a board that was given a library project is
/// named on that project's card (`on_boards`) and names the project back,
/// the project is in Projects and not in Other projects; once the board is
/// emptied the project is on no board again.
#[test]
fn a_project_a_board_plays_is_on_it_and_leaves_other_projects() {
    let device = empty_light_player("dev000000daqf6dvhm3");
    let (mut bench, tasks) = identified(&device, "usb-home-sections-3");
    let (_device_uid, project) = a_board_running_a_library_project(&mut bench, &tasks);
    let project_uid = project.to_string();
    let device_id = bench.view().devices[0].id;
    let board_title = bench.view().devices[0].title.clone();

    let home = bench.controller.view().home.expect("the home view");
    let card = home
        .projects
        .iter()
        .find(|card| card.uid == project_uid)
        .expect("the pushed project is in the library");
    assert_eq!(card.on_boards, [board_title.clone()]);
    assert_eq!(
        home.sections.online[0].project.as_deref(),
        Some(card.slug.as_str())
    );
    assert_eq!(home.sections.projects, [project_uid.clone()]);
    assert!(
        home.sections.other_projects.is_empty(),
        "a project a board plays is not an \"other\" project: {:?}",
        home.sections
    );
    assert!(!home.sections.newcomer);

    // The board is emptied: its own word beats the stale association, so
    // the project is on no board and is an "other" project again.
    bench.press_device_lasting(device_id, "remove-project", OfferArgs::new());
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && card.loaded_project == lpa_devices::view::LoadedProject::Empty
        })
    });
    let home = bench.controller.view().home.expect("the home view");
    let card = home
        .projects
        .iter()
        .find(|card| card.uid == project_uid)
        .expect("the library copy stays");
    assert!(card.on_boards.is_empty(), "{card:?}");
    assert_eq!(home.sections.other_projects, [project_uid.clone()]);
    assert_eq!(home.sections.projects, [project_uid]);
    assert_eq!(home.sections.online[0].project, None);
}

/// Every path the Connect a board section names is a published offer, and
/// with a transport "start a board here" is one of them.
#[test]
fn every_connect_offer_the_page_names_is_published_with_a_transport() {
    let device = light_player("dev000000daqf6dvhm2");
    let (mut bench, _tasks) = DeviceBench::ungranted(&device, "usb-home-sections-2");
    let sections = bench
        .controller
        .view()
        .home
        .expect("the home view")
        .sections;

    let paths: Vec<String> = sections
        .connect
        .offers
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        paths,
        [
            "devices/connect-usb",
            "devices/connect-ble",
            "devices/connect-wifi-address",
            "devices/new-sim"
        ]
    );
    for path in &sections.connect.offers {
        bench.offered(path);
    }
    assert!(
        sections.newcomer && sections.connect.welcome,
        "no board, no project: the first-visit page, with its hint"
    );
}

/// With no transport there is nothing to start a board on: the section does
/// not name `devices/new-sim`, and the tree does not publish it either. The
/// three ways a board comes in stay (each disabled with its reason).
#[test]
fn without_a_transport_start_a_board_here_is_absent_from_both() {
    let mut controller = crate::StudioController::new(|| 0.0);
    let sections = controller.view().home.expect("the home view").sections;

    let paths: Vec<String> = sections
        .connect
        .offers
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        paths,
        [
            "devices/connect-usb",
            "devices/connect-ble",
            "devices/connect-wifi-address"
        ]
    );
    for path in &sections.connect.offers {
        controller.offered(path);
    }
    controller.not_offered("devices/new-sim");
}
