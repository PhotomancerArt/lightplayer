//! Connected: the home page holds the open session, end to end on the
//! bench's real wire.
//!
//! Connect (`devices/<board>/connect`) opens the editor's session on a
//! board with the home page still up and the session on the board's card;
//! Edit (`devices/<board>/edit`) shows the editor on that same session with
//! nothing reopened, or connects first; Done (`devices/<board>/done`) closes
//! it. Where the user is decides which surface the open session shows on,
//! and nothing else. Connect reaches a board Studio is not talking to yet
//! first — a closed port, a board offline on its cable or on Wi‑Fi — then
//! opens its session, holding the intent for a minute at most. Connected,
//! the card carries the board's panel, Done, the session's own moving
//! picture, and the session's state in its connection bar. Every verb is
//! pressed by its offer path, as the card and the app agent press it.

use super::ble_drop_tests::{
    bench_over_bluetooth, bluetooth_board, bundled_example_files, locked_store_file,
    wait_for_access_line,
};
use super::unlock_tests::{PLAY_PASSWORD, two_password_store_file};
use super::wifi_connect_tests::{KEY, joined_light_player, with_lan};
use super::*;
use crate::app::studio::studio_edit_e2e_tests::project_editor;
use crate::{BoardPlays, ConnectPhase, UiPage, UiPlace, UiProjectView};

/// Connect on a running board: the session opens, and the home page stays —
/// no editor panes, the lens bound, the session control saying the home
/// page holds it, the open project's address known, and the board's card
/// saying the session is on it.
#[test]
fn connect_opens_the_session_on_the_card() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000001", "usb-conn-1");
    let connect = bench.device_verb(id, "connect");
    bench
        .press(&connect, OfferArgs::new())
        .expect("Connect opens the session");

    let view = bench.controller.view();
    let home = view.home.as_ref().expect("the home page stays");
    assert!(view.panes.is_empty(), "no editor: {:?}", view.panes.len());
    assert!(view.lens.is_some(), "the session is open");
    assert!(
        view.session
            .as_ref()
            .is_some_and(|session| session.connected),
        "the home page holds it"
    );
    let project_uid = view
        .open_project_uid
        .clone()
        .expect("the open session's address is known on the home page");
    assert_eq!(
        home.devices.board_projects.plays(id),
        &BoardPlays::Open { project_uid },
        "the card's board plays what the session holds"
    );
    let connected = bench.controller.connected().expect("connected").clone();
    assert_eq!(connected.device, id);
    assert_eq!(connected.phase, ConnectPhase::Open);
    assert!(!connected.editor_waiting);
}

/// Done closes the session: the lens goes, the wire is the roster's again,
/// and the home page is the one it was before Connect, apart from the
/// device side.
#[test]
fn done_closes_it_and_the_home_page_is_whole() {
    let (mut bench, tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000002", "usb-conn-2");
    let before = *bench.controller.view().home.expect("home before Connect");
    bench.press_device(id, "connect", OfferArgs::new());
    let link = lens_link(&bench);

    bench.press_device(id, "done", OfferArgs::new());
    let view = bench.controller.view();
    assert!(view.lens.is_none(), "the session is closed");
    assert!(view.session.is_none());
    assert!(bench.controller.connected().is_none());
    assert!(
        !bench
            .controller
            .devices_for_test()
            .effects()
            .wire_borrowed(link),
        "the wire is back with the roster"
    );
    let after = *view.home.expect("the home page");
    assert_eq!(after.projects, before.projects);
    assert_eq!(after.examples, before.examples);
    assert_eq!(after.library_available, before.library_available);
    assert_eq!(after.opening, before.opening);
    assert_eq!(after.issue, before.issue);
    assert_eq!(
        after.sections.other_projects,
        before.sections.other_projects
    );
    assert_eq!(after.sections.projects, before.sections.projects);
    assert_eq!(after.sections.patterns, before.sections.patterns);
    assert_eq!(after.sections.newcomer, before.sections.newcomer);
    let boards = |home: &crate::UiHomeView| -> Vec<crate::DeviceId> {
        home.sections.online.iter().map(|board| board.id).collect()
    };
    assert_eq!(boards(&after), boards(&before), "the same boards, online");

    bench.run_until(&tasks, "the pump to hear the board again", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });
    let connect = bench.device_verb(id, "connect");
    bench.offered(connect);
}

/// Edit on the connected board shows the editor on the session already
/// open: no new pool session, the same runtime handle, the mirror not read
/// again from scratch.
#[test]
fn edit_on_the_connected_board_reopens_nothing() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000003", "usb-conn-3");
    bench.press_device(id, "connect", OfferArgs::new());
    let installs = pool_installs(&bench);
    let session = bench.lens_session_id();
    let handle = bench.ready_handle();
    let revision = mirror_revision(&bench);
    assert!(revision.is_some(), "the session read its project");

    bench.press_device(id, "edit", OfferArgs::new());
    let view = bench.controller.view();
    assert!(!view.panes.is_empty(), "the editor shows");
    assert!(view.home.is_none());
    assert!(view.lens_card.is_some(), "with the board's card docked");
    assert_eq!(pool_installs(&bench), installs, "no Pool install");
    assert_eq!(bench.lens_session_id(), session, "the same session");
    assert_eq!(bench.ready_handle(), handle, "the same runtime handle");
    assert_eq!(
        mirror_revision(&bench),
        revision,
        "the mirror was not reset"
    );
    assert!(
        view.session
            .as_ref()
            .is_some_and(|session| session.connected),
        "still the home page's session"
    );
}

/// Edit on a board that is not connected connects it first, then shows the
/// editor.
#[test]
fn edit_on_a_board_not_connected_connects_first() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000004", "usb-conn-4");
    assert!(bench.controller.connected().is_none());
    bench.press_device(id, "edit", OfferArgs::new());
    let connected = bench.controller.connected().expect("connected").clone();
    assert_eq!(connected.device, id);
    assert!(connected.editor_waiting, "the editor is wanted");
    let view = bench.controller.view();
    assert!(!view.panes.is_empty(), "the editor shows");
    assert!(view.home.is_none());
    assert!(
        view.session
            .as_ref()
            .is_some_and(|session| session.connected)
    );
}

/// Where the user is decides the surface, and nothing else. A waiting Edit
/// ends at the first place report that moves the user (the director's
/// ruling on P2–P3): Edit, then Home straight away, shows home, with the
/// session still open. On the session's own page the editor shows; back
/// home the card does; on the session's page again the editor shows with no
/// Edit pressed.
#[test]
fn going_home_shows_the_card_again() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000005", "usb-conn-5");
    bench.press_device(id, "edit", OfferArgs::new());
    assert!(
        !bench.controller.view().panes.is_empty(),
        "the editor shows for the waiting Edit"
    );
    let project_uid = bench
        .controller
        .view()
        .open_project_uid
        .expect("a bound project");
    let session = bench.lens_session_id();
    let its_page = UiPlace::new(UiPage::Project {
        uid: project_uid,
        view: UiProjectView::Nodes,
    });

    // Home, straight after Edit: the wait is over and home shows.
    bench.controller.set_place(UiPlace::new(UiPage::Home));
    assert!(
        !bench.controller.connected().unwrap().editor_waiting,
        "the first move ends the wait"
    );
    let view = bench.controller.view();
    assert!(view.home.is_some(), "the home page");
    assert!(view.panes.is_empty());
    assert!(view.lens.is_some(), "the session is still open");
    assert_eq!(bench.lens_session_id(), session);

    // The session's page: the editor.
    bench.controller.set_place(its_page.clone());
    assert!(!bench.controller.view().panes.is_empty());

    // Home again: the card, the session open.
    bench.controller.set_place(UiPlace::new(UiPage::Home));
    let view = bench.controller.view();
    assert!(view.home.is_some(), "the home page is back");
    assert!(view.panes.is_empty());
    assert_eq!(bench.lens_session_id(), session);
    assert!(bench.controller.connected().is_some());

    // Back on the session's page: the editor, with no Edit pressed.
    bench.controller.set_place(its_page);
    let view = bench.controller.view();
    assert!(!view.panes.is_empty());
    assert!(view.home.is_none());
    assert_eq!(bench.lens_session_id(), session, "nothing reopened");
}

/// One board connected at a time: Connect on a second board closes the
/// first session (its wire back with the roster), then opens the second.
#[test]
fn connect_on_a_second_board_hands_over() {
    let (mut bench, tasks, _device, first, second) =
        a_sim_and_a_usb_board("dev000000cnct000006", "usb-conn-6");

    bench.press_device(first, "connect", OfferArgs::new());
    let first_link = lens_link(&bench);
    bench.press_device(second, "connect", OfferArgs::new());

    let connected = bench.controller.connected().expect("connected").clone();
    assert_eq!(connected.device, second, "the second board is connected");
    assert!(
        !bench
            .controller
            .devices_for_test()
            .effects()
            .wire_borrowed(first_link),
        "the first board's wire is back with the roster"
    );
    let view = bench.controller.view();
    assert!(view.home.is_some(), "still the home page");
    let first_done = bench.device_verb(first, "done");
    bench.not_offered(first_done);
    let second_done = bench.device_verb(second, "done");
    bench.offered(second_done);
    bench.run_until(&tasks, "the first board to be watched again", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == first && card.state_label == "Ready")
    });
    let first_connect = bench.device_verb(first, "connect");
    assert!(
        bench.offered(first_connect).is_enabled(),
        "the first card can be connected again"
    );
}

/// A session an address opens (the route's `/device/<uid>`) is not the home
/// page's: the editor shows and the home page does not, as today.
#[test]
fn an_address_open_is_not_connected() {
    let (mut bench, _tasks, _device, _id, uid) =
        running_library_board("dev000000cnct000007", "usb-conn-7");
    bench.open_lens(&uid).expect("the address opens the board");
    assert!(bench.controller.connected().is_none());
    let view = bench.controller.view();
    assert!(
        view.session
            .as_ref()
            .is_some_and(|session| !session.connected),
        "an address holds it"
    );
    assert!(!view.panes.is_empty(), "the editor");
    assert!(view.home.is_none(), "no home page");
}

/// The verbs follow the session: Connect on a running board and not on the
/// board the session is on; Done only there; Edit not while the editor
/// shows the board; and a board running nothing says why it cannot connect.
#[test]
fn the_offers_follow_the_session() {
    let (mut bench, _tasks, _device, id, uid) =
        running_library_board("dev000000cnct000008", "usb-conn-8");
    let connect = bench.device_verb(id, "connect");
    let edit = bench.device_verb(id, "edit");
    let done = bench.device_verb(id, "done");
    let offer = bench.offered(&connect);
    assert!(offer.is_enabled());
    assert!(offer.consequence().is_routine());
    assert_eq!(offer.icon, "usb", "the link's icon");
    bench.offered(&edit);
    bench.not_offered(&done);

    // Connected, on its card.
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    bench.not_offered(&connect);
    bench.offered(&done);
    bench.offered(&edit);

    // The editor shows it: nowhere further for Edit to go.
    bench.press(&edit, OfferArgs::new()).expect("Edit");
    bench.not_offered(&edit);
    bench.not_offered(&connect);
    bench.offered(&done);

    // An address's session: Done closes it too; Connect and Edit wait.
    bench.press(&done, OfferArgs::new()).expect("Done");
    bench.open_lens(&uid).expect("the address opens the board");
    bench.not_offered(&connect);
    bench.not_offered(&edit);
    bench.offered(&done);

    // A board running nothing has no panel to show.
    let device = empty_light_player("dev000000cnct000009");
    let (mut empty, tasks) = identified(&device, "usb-conn-9");
    empty.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let board = empty.view().devices[0].id;
    let empty_connect = empty.device_verb(board, "connect");
    assert_eq!(empty.offer_reason(empty_connect), crate::NOTHING_ON_IT_YET);
}

/// A closed port: Connect opens it, the board identifies, and the session
/// opens on the card — one press.
#[test]
fn connect_on_an_attached_board_opens_the_port_then_the_panel() {
    let (mut bench, tasks, _device, id, uid) =
        running_library_board("dev000000cnct00000a", "usb-conn-10");
    bench.press_device(id, "disconnect", OfferArgs::new());
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.status == crate::DeviceStatus::Attached)
    });

    bench.press_device(id, "connect", OfferArgs::new());
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.is_connect_for(&uid)),
        "the Connect holds while the port opens"
    );
    let connect = bench.device_verb(id, "connect");
    bench.not_offered(&connect);

    tick_until(&mut bench, &tasks, "the session to open", |bench| {
        bench.controller.connected().is_some()
    });
    assert_eq!(bench.controller.connected().unwrap().device, id);
    assert!(bench.controller.pending_lens().is_none());
    on_its_card(&bench);
}

/// An offline board Studio reaches by its cable: Connect asks for its port
/// back (the browser's chooser, which needs the user's click), then opens
/// the session when it is back.
#[test]
fn connect_on_an_offline_usb_board_reaches_then_connects() {
    let (mut bench, tasks, _device, id, _uid) =
        running_library_board("dev000000cnct00000b", "usb-conn-11");
    unplug(&mut bench, &tasks, id);

    let connect = bench.device_verb(id, "connect");
    let offer = bench.offered(&connect);
    assert_eq!(
        offer.action.op_as::<crate::RuntimeOp>(),
        Some(&crate::RuntimeOp::ConnectDevice {
            device: id,
            reach: Some(crate::ConnectReach::Usb)
        })
    );
    assert!(
        offer.action.meta().needs_user_activation,
        "the chooser is the user's"
    );

    bench.press(&connect, OfferArgs::new()).expect("Connect");
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.connect.is_some()),
        "the Connect holds while the board comes back"
    );
    tick_until(
        &mut bench,
        &tasks,
        "the board back and connected",
        |bench| bench.controller.connected().is_some(),
    );
    assert_eq!(bench.controller.connected().unwrap().device, id);
    on_its_card(&bench);
}

/// An offline board this browser remembers on Wi‑Fi: Connect reaches it
/// there (Wi‑Fi first, as the card's primary orders it), then opens the
/// session.
#[test]
fn connect_over_wifi_reaches_then_connects() {
    let device = joined_light_player("dev000000cnct00000c");
    let (mut bench, tasks) = identified(&device, "usb-conn-12");
    let _rig = with_lan(&mut bench, &device);
    let id = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    bench.settle_library();
    unplug(&mut bench, &tasks, id);

    let connect = bench.device_verb(id, "connect");
    let offer = bench.offered(&connect);
    assert_eq!(offer.icon, "wifi");
    assert_eq!(
        offer.action.op_as::<crate::RuntimeOp>(),
        Some(&crate::RuntimeOp::ConnectDevice {
            device: id,
            reach: Some(crate::ConnectReach::Wifi)
        })
    );
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.connect.is_some()),
        "the Connect holds while Wi‑Fi reaches the board"
    );
    tick_until(
        &mut bench,
        &tasks,
        "the board connected over Wi‑Fi",
        |bench| bench.controller.connected().is_some(),
    );
    assert_eq!(bench.controller.connected().unwrap().device, id);
    assert!(
        bench
            .controller
            .device_roster_view()
            .lan_links
            .contains_key(&id),
        "reached on Wi‑Fi"
    );
    assert!(bench.controller.view().home.is_some());
}

/// A board that never comes back: after the grace the hold is gone, the
/// card's words say why, and Connect is offered again.
#[test]
fn a_connect_that_never_lands_gives_up_after_the_grace() {
    let (mut bench, tasks, _device, id, uid) =
        running_library_board("dev000000cnct00000d", "usb-conn-13");
    unplug(&mut bench, &tasks, id);
    // The chooser comes back empty: the board is nowhere to be found.
    bench.chooser_grants.set(false);
    let connect = bench.device_verb(id, "connect");
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    for _ in 0..20 {
        bench.step(&tasks);
        drive(bench.controller.try_pending_device_lens());
    }
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.is_connect_for(&uid)),
        "still waiting inside the grace"
    );
    bench.not_offered(&connect);

    bench
        .clock
        .set(bench.clock.get() + crate::CONNECT_INTENT_GRACE.as_secs_f64() + 1.0);
    drive(bench.controller.try_pending_device_lens());
    assert!(bench.controller.pending_lens().is_none(), "given up");
    let failure = bench
        .controller
        .connect_failure()
        .expect("the card says why");
    assert_eq!(failure.device, id);
    assert_eq!(failure.reason, crate::CONNECT_GAVE_UP);
    assert!(bench.offered(&connect).is_enabled(), "Retry is Connect");
    assert!(bench.controller.connected().is_none());
}

/// A board reached over Bluetooth that comes back locked: the hold lets go
/// at once, and Unlock is the way in.
#[test]
fn a_locked_board_drops_the_hold_and_offers_unlock() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000cnct00000e")
            .with_untrusted_link()
            .with_root_files(vec![locked_store_file()]),
    )));
    let (mut bench, tasks, present) = bench_over_bluetooth(&device, |_| {});
    bench.run_until(&tasks, "the board to ask for its password", |bench| {
        bench.controller.view().login_prompt.is_some()
    });
    let id = bench.view().devices[0].id;
    bench.settle_library();

    // Out of range: the board is remembered, offline.
    present.set(false);
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
    let connect = bench.device_verb(id, "connect");
    bench.press(&connect, OfferArgs::new()).expect("Connect");
    assert!(
        bench.controller.pending_lens().is_some(),
        "the Connect holds"
    );

    // Back in range, and still locked.
    present.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    tick_until(&mut bench, &tasks, "the hold to let go", |bench| {
        bench.controller.pending_lens().is_none()
    });
    assert!(bench.controller.connected().is_none());
    assert_eq!(
        bench.controller.connect_failure(),
        None,
        "not a failure: Unlock takes over"
    );
    let said = bench
        .console_line_containing("the connect is let go")
        .expect("the console says why the hold went");
    assert!(said.contains("is locked"), "{said}");
    bench.wait_for_verb(&tasks, id, crate::UNLOCK_VERB);
    let unlock = bench.device_verb(id, crate::UNLOCK_VERB);
    bench.offered(unlock);
}

/// The opening frame's exit presses a closed port's Connect while an
/// editor open is held for the board: the port opens, and the held open goes
/// on to the EDITOR, not the card.
#[test]
fn the_opening_frames_connect_keeps_the_editor_open() {
    let (mut bench, tasks, _device, id, uid) =
        running_library_board("dev000000cnct00000f", "usb-conn-15");
    bench.press_device(id, "disconnect", OfferArgs::new());
    bench.run_until(&tasks, "the port to close", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.status == crate::DeviceStatus::Attached)
    });
    // The address asks for the board: held, the port being closed.
    bench.open_lens(&uid).expect("the address is held");
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.is_address_for(&uid))
    );

    bench.press_device(id, "connect", OfferArgs::new());
    assert!(
        bench
            .controller
            .pending_lens()
            .is_some_and(|pending| pending.is_address_for(&uid)),
        "the held open is still the address's"
    );
    tick_until(&mut bench, &tasks, "the held open to land", |bench| {
        bench.lens_device_uid().is_some()
    });
    assert_eq!(bench.lens_device_uid().as_deref(), Some(uid.as_str()));
    assert!(
        bench.controller.connected().is_none(),
        "an address holds it"
    );
    let view = bench.controller.view();
    assert!(view.home.is_none(), "the editor");
    assert!(view.session.is_some_and(|session| !session.connected));
}

// ---------------------------------------------------------------------
// The card's connected face
// ---------------------------------------------------------------------

/// CD6, CD7: Connect on a running board whose project has a root panel.
/// The card carries the panel in the bars' place: the root panel's own
/// picks — the master brightness fader, then the shader's knob — with the
/// clock's transport, the pattern picker and the palette left to All
/// controls. Done is the primary, and auto-save rides along on USB (the
/// edit tier). The editor's view is built to pick them, and none of its
/// verbs are published on the home page.
#[test]
fn the_connected_card_carries_the_panel() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000010", "usb-conn-16");
    bench.press_device(id, "connect", OfferArgs::new());

    let card = the_card(&bench, id);
    let panel = card.panel.clone().expect("the panel in the bars' place");
    assert_eq!(channels(&panel), [crate::MASTER_CHANNEL, "glow"]);
    assert!(
        matches!(
            panel.controls[0].view.control.widget,
            crate::UiPanelWidget::Fader { .. }
        ),
        "the master leads"
    );
    assert_eq!(
        panel.more, 3,
        "the transport, the pattern picker and the palette"
    );
    assert!(panel.target.is_some(), "the root panel's scope");
    assert!(panel.auto_save.is_some(), "USB holds the edit tier");
    let edit = panel.edit.clone().expect("Edit on the All controls row");
    assert_eq!(edit.offer, bench.device_verb(id, "edit"));
    assert_eq!(edit.icon.as_deref(), Some("edit"));
    assert_eq!(
        card.name_bar.primary.as_ref().map(|primary| primary.word()),
        Some("Done")
    );
    assert_eq!(card.bars.len(), 5, "the bars are still built");
    assert_card_offers(&bench, id);
    bench.not_offered("project/add-node");

    // They are the editor's own root panel, picked.
    bench.press_device(id, "edit", OfferArgs::new());
    bench.offered("project/add-node");
    let view = bench.controller.view();
    let face = project_editor(&view)
        .root_module_face()
        .expect("the root wears the module face");
    let picks = crate::board_panel_picks(&face.panel, face.auto_save);
    assert_eq!(panel.controls, picks.controls);
    assert_eq!(panel.more, picks.more);
    assert_eq!(panel.target, picks.target);
    assert_eq!(panel.auto_save, picks.auto_save);
}

/// CD8 (#571's test, redone on Connect): while the session holds the wire
/// the card is not a dimmed last frame. It draws the session's own
/// composed picture, current and named as the session's, and that
/// picture's revision advances with the session's reads — while the card
/// feed's own pull never runs under the borrow. After Done the feed pulls
/// again.
#[test]
fn the_card_shows_the_lens_sessions_frames() {
    let (mut bench, tasks) = running_board_wanting_a_picture("dev000000cnct000011", "usb-conn-17");
    let device = bench.view().devices[0].id;
    feed_tick(&mut bench, &tasks, 5.0);
    for _ in 0..40 {
        bench.step(&tasks);
    }
    feed_tick(&mut bench, &tasks, 5.0);
    let own_revision = feed_frame_revision(&bench, device).expect("the feed pulled once");
    let stamp_before = feed_stamp(&bench, device).expect("the feed has pulled");

    bench.press_device(device, "connect", OfferArgs::new());
    let link = lens_link(&bench);
    assert!(
        bench
            .controller
            .devices_for_test()
            .effects()
            .lens_holds_wire(link)
    );

    // The session reads at its own cadence; each read carries the
    // published frame. Drive it until the card's picture is the session's
    // and has moved past the feed's last one, then until it moves again.
    let mut lens_revisions: Vec<i64> = Vec::new();
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    while lens_revisions.len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the card's picture never advanced under the session: {lens_revisions:?}"
        );
        bench.clock.set(bench.clock.get() + 0.2);
        let _ = bench.tick();
        for _ in 0..40 {
            bench.step(&tasks);
        }
        feed_tick(&mut bench, &tasks, 5.0);
        let feeds = bench.controller.device_roster_view().feeds;
        let Some(feed) = feeds.get(&device) else {
            continue;
        };
        if !feed.from_lens {
            assert_eq!(
                feed.liveness,
                crate::FeedLiveness::Lens,
                "before the session has a frame the card keeps the dimmed last one: {feed:?}"
            );
            continue;
        }
        assert_eq!(feed.liveness, crate::FeedLiveness::Live, "{feed:?}");
        let revision = feed
            .frame
            .as_ref()
            .map(|frame| frame.revision)
            .expect("the session's picture");
        if revision > own_revision && lens_revisions.last() != Some(&revision) {
            lens_revisions.push(revision);
        }
    }
    assert!(
        lens_revisions[1] > lens_revisions[0],
        "the card's frame revision advances while connected: {lens_revisions:?}"
    );
    let card = the_card(&bench, device);
    assert_eq!(card.picture.source, crate::PictureSource::Lens);
    assert!(!card.picture.dim, "current, not last known");
    let picture = card
        .status
        .details
        .sections
        .iter()
        .flat_map(|section| section.lines.iter())
        .find(|line| line.label == "Picture")
        .map(|line| line.value.clone())
        .expect("the picture line");
    assert!(
        picture.ends_with(crate::LENS_SOURCE_WORDS),
        "the details name where it comes from: {picture}"
    );

    // The feed itself never asked: the frames came from the session's reads.
    assert_eq!(
        feed_stamp(&bench, device),
        Some(stamp_before),
        "no second pull"
    );
    assert_eq!(
        feed_frame_revision(&bench, device),
        Some(own_revision),
        "the feed's own last frame is untouched by the session's"
    );

    // Done: the wire comes back, and the feed is the card's again.
    bench.press_device(device, "done", OfferArgs::new());
    bench.run_until(&tasks, "the wire to come back", |bench| {
        !bench
            .controller
            .devices_for_test()
            .effects()
            .wire_borrowed(link)
    });
    for _ in 0..40 {
        bench.step(&tasks);
    }
    feed_tick(&mut bench, &tasks, 5.0);
    assert!(
        feed_stamp(&bench, device) > Some(stamp_before),
        "the feed resumed once the session let go"
    );
}

/// AC7: a link that holds play only connects — the play password is
/// enough — and its panel plays: a panel write (the playlist's `cycle`,
/// pressed by path where the editor shows the session) is accepted at the
/// play tier. Auto-save is hidden below the edit tier, and Edit wears the
/// lock: the All controls row presses `unlock`, and after Done so does the
/// project bar.
#[test]
fn a_play_only_link_connects_and_plays() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000cnct000012")
            .with_project_files(bundled_example_files())
            .with_loaded_project()
            .with_untrusted_link()
            .with_root_files(vec![two_password_store_file()]),
    )));
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |bench| {
        let mut remembered =
            crate::app::access::remembered_passwords::RememberedPasswords::default();
        remembered.remember(PLAY_PASSWORD, 1.0);
        bench
            .controller
            .apply_access_command(crate::AccessCommand::MemoryLoaded {
                passwords_json: Some(remembered.to_json()),
                devices_json: None,
                browser_json: None,
                account_json: None,
            });
    });
    bench.run_until(&tasks, "the board to identify over Bluetooth", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    let id = bench.view().devices[0].id;
    bench.run_until(&tasks, "the link to hold play only", |bench| {
        bench
            .controller
            .device_roster_view()
            .access
            .get(&id)
            .and_then(|access| access.unlock)
            == Some(crate::UiUnlockOffer::PlayOnly)
    });
    // The fake's heartbeat names only the projects its server has been
    // asked about (real firmware's names the one its boot resumed), so one
    // look by the board's address teaches it; Done lets that session go.
    let uid = bench.registry()[0].uid.clone();
    bench
        .open_lens(&uid)
        .expect("the address opens at the play tier");
    bench.press_device(id, crate::DONE_VERB, OfferArgs::new());
    wait_running(&mut bench, &tasks, id);
    bench.wait_for_verb(&tasks, id, crate::CONNECT_VERB);
    let connect = bench.device_verb(id, crate::CONNECT_VERB);
    assert!(
        bench.offered(&connect).is_enabled(),
        "the play password is enough to connect"
    );
    bench
        .press(&connect, OfferArgs::new())
        .expect("Connect opens the session at the play tier");
    assert!(
        bench.controller.connected().is_some_and(
            |connected| connected.device == id && connected.phase == ConnectPhase::Open
        )
    );
    let panel = the_card(&bench, id).panel.expect("the panel");
    assert_eq!(panel.auto_save, None, "auto-save is the edit tier's");
    let edit = panel.edit.expect("Edit, locked");
    assert_eq!(edit.offer, bench.device_verb(id, crate::UNLOCK_VERB));
    assert_eq!(edit.icon.as_deref(), Some("lock"));
    assert_eq!(edit.draw, crate::UiActionDraw::Sheet);

    // A panel write at the play tier.
    let uid = bench.controller.connected().unwrap().uid.clone();
    bench.controller.set_place(UiPlace::new(UiPage::Device {
        uid,
        view: UiProjectView::Play,
    }));
    let cycle = crate::OfferPath::project_node(
        &crate::ProjectNodeAddress::parse("/studio.show/playlist.playlist")
            .expect("the bundled example's playlist"),
    )
    .child(crate::PLAYLIST_CYCLE_VERB);
    let notices = bench
        .press(
            &cycle,
            OfferArgs::new().with(crate::PLAYLIST_CYCLING_PARAM, "true"),
        )
        .expect("the board takes the write at the play tier");
    assert!(
        notices.notices.is_empty(),
        "accepted, not refused: {notices:?}"
    );
    bench.controller.set_place(UiPlace::new(UiPage::Home));

    // Done: the project bar's Edit wears the lock too.
    bench.press_device(id, "done", OfferArgs::new());
    let edit = the_card(&bench, id)
        .bar(crate::BarLayer::Project)
        .action
        .clone()
        .expect("Edit, locked");
    assert_eq!(edit.offer, bench.device_verb(id, crate::UNLOCK_VERB));
    assert_eq!(edit.icon.as_deref(), Some("lock"));
}

/// CD15 (Q19): while connected, Edit sits at the end of the All controls
/// row. `panel.edit` presses `devices/<board>/edit` by path, and the editor
/// shows on the open session, with no new session installed.
#[test]
fn edit_while_connected_is_on_the_all_controls_row() {
    let (mut bench, _tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000013", "usb-conn-19");
    bench.press_device(id, "connect", OfferArgs::new());
    let installs = pool_installs(&bench);
    let session = bench.lens_session_id();
    let edit = the_card(&bench, id)
        .panel
        .expect("the panel")
        .edit
        .expect("Edit on the All controls row");
    assert_eq!(edit.offer, bench.device_verb(id, crate::EDIT_VERB));
    assert_eq!(edit.word, "Edit");

    bench
        .press(&edit.offer, edit.args.clone())
        .expect("Edit shows the editor");
    let view = bench.controller.view();
    assert!(!view.panes.is_empty(), "the editor shows");
    assert!(view.home.is_none());
    assert_eq!(pool_installs(&bench), installs, "no Pool install");
    assert_eq!(bench.lens_session_id(), session, "the same session");
}

/// Q4, as ruled: connecting never makes a card's picture slower than
/// watching it. Over Bluetooth the connected card's picture is the
/// session's reads, and a session on its card keeps its own cadence — at
/// least as often as a watched card's feed reads — not the Play page's
/// once-a-minute budget.
#[test]
fn a_bluetooth_card_reads_at_least_as_often_as_a_watched_one() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000cnct000014"),
    )));
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |_| {});
    bench.run_until(&tasks, "the board to identify over Bluetooth", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    let id = bench.view().devices[0].id;
    wait_for_access_line(&mut bench, &tasks, id, "Unlocked");
    bench.wait_for_verb(&tasks, id, "push");
    bench.push_gesture(id, bundled_example());
    wait_running(&mut bench, &tasks, id);
    bench.wait_for_verb(&tasks, id, crate::CONNECT_VERB);
    bench.press_device(id, crate::CONNECT_VERB, OfferArgs::new());
    on_its_card(&bench);

    let gap = bench
        .controller
        .lens_refresh_gap_for_test()
        .expect("the session reads");
    assert!(
        gap <= crate::app::studio::DEVICE_CARD_FEED_BLE_INTERVAL,
        "at least as often as a watched card: {gap:?}"
    );
    assert!(
        gap < crate::app::studio::BLE_PLAY_IDLE_REFRESH_INTERVAL,
        "the Play budget is the play page's: {gap:?}"
    );
}

/// CD7: the connected session's link drops (the cable out) and the lens
/// holds on for the board. The home page stays; the card says
/// "Reconnecting…" as its connection bar's work, with Done its primary.
/// When the hold runs out the session ends and the card is back to its
/// facts.
#[test]
fn a_dropped_link_reads_reconnecting_on_the_card() {
    let (mut bench, tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000015", "usb-conn-20");
    bench.press_device(id, "connect", OfferArgs::new());

    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the lens to be held on the departure", |bench| {
        bench.controller.lens_is_held()
    });
    assert!(
        bench.controller.view().home.is_some(),
        "the card, not the editor"
    );
    let card = the_card(&bench, id);
    let work = card
        .bar(crate::BarLayer::Connection)
        .work
        .clone()
        .expect("the session's work");
    assert_eq!(work.words, crate::RECONNECTING);
    assert_eq!(work.state, crate::BarWorkState::Running);
    assert_eq!(
        card.name_bar.primary.as_ref().map(|primary| primary.word()),
        Some("Done")
    );
    assert_eq!(card.panel, None, "the panel waits for the board");

    // The board stays away: awake time runs the hold's grace out.
    let grace = crate::app::studio::lens_hold::LENS_HOLD_GRACE.as_secs_f64();
    let start = bench.clock.get();
    while bench.clock.get() < start + grace + 1.0 {
        bench.clock.set(bench.clock.get() + 1.0);
        let _ = bench.tick();
        if bench.lens_device_uid().is_none() {
            break;
        }
    }
    assert!(bench.controller.connected().is_none(), "the session ended");
    bench.step(&tasks);
    let card = the_card(&bench, id);
    assert_eq!(
        card.bar(crate::BarLayer::Connection).work,
        None,
        "back to its facts"
    );
    assert_eq!(
        card.name_bar.primary.as_ref().map(|primary| primary.word()),
        Some("Connect")
    );
}

/// The director's ruling on P2–P3: Done with unsaved edits stays Routine.
/// The edits live on the board as its overlay, and the next Connect
/// rebuilds them as unsaved (the runtime-pool ADR, "Lens moves quiesce,
/// then rebuild").
#[test]
fn done_with_unsaved_edits_is_routine_and_connect_reads_them_unsaved() {
    let (mut bench, tasks, _device, id, _uid) =
        running_library_board("dev000000cnct000016", "usb-conn-21");
    bench.press_device(id, crate::EDIT_VERB, OfferArgs::new());
    // An edit in the editor: the shader's `glow` default, the slot its
    // panel knob falls back to (a def slot, saved with the project).
    let view = bench.controller.view();
    let glow = project_editor(&view)
        .root_module_face()
        .expect("the root wears the module face")
        .panel
        .groups
        .iter()
        .flat_map(|group| group.groups.iter().chain(core::iter::once(group)))
        .flat_map(|group| group.controls.iter())
        .find(|control| control.channel == "glow")
        .and_then(|control| control.control.address.clone())
        .expect("the glow knob's slot");
    set_slot_value(&mut bench, glow, lpc_model::LpValue::F32(0.25)).expect("the edit lands");
    assert!(unsaved(&bench), "the edit is unsaved work");

    // Done, from the editor's docked card.
    let done = bench.device_verb(id, crate::DONE_VERB);
    assert!(
        bench.offered(&done).consequence().is_routine(),
        "Done stays Routine with unsaved edits"
    );
    bench.press(&done, OfferArgs::new()).expect("Done");
    assert!(bench.controller.connected().is_none());
    bench.run_until(&tasks, "the pump to hear the board again", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready")
    });

    bench.press_device(id, crate::CONNECT_VERB, OfferArgs::new());
    on_its_card(&bench);
    assert!(
        unsaved(&bench),
        "the next Connect reads the edit back from the board, unsaved"
    );
}

/// A Connect that cannot open says so: the card's connection bar says
/// "Couldn't connect" with the reason in its details, and the console line
/// is a Connect's, in plain words — not "could not open the board in the
/// editor".
#[test]
fn a_failed_connect_says_so_on_the_card_in_plain_words() {
    let (mut bench, _tasks, device, id, _uid) =
        running_library_board("dev000000cnct000017", "usb-conn-22");
    // The wire dies at the next byte: the session's own hello is the first
    // thing to meet it.
    device.set_failure_plan(
        lpa_link::providers::fake_device::FakeFailurePlan::none()
            .with_disconnect_after_bytes(device.served_bytes()),
    );
    let connect = bench.device_verb(id, crate::CONNECT_VERB);
    let error = bench
        .press(&connect, OfferArgs::new())
        .expect_err("the session's hello dies on the cut wire");
    let failure = bench
        .controller
        .connect_failure()
        .expect("the card's reason")
        .clone();
    assert_eq!(failure.device, id);
    assert_eq!(failure.reason, error.to_string());
    let logged = bench
        .console_line_containing("could not connect to the board")
        .expect("the console says the Connect failed");
    assert!(logged.contains(&failure.reason), "{logged}");
    assert!(
        bench
            .console_line_containing("could not open the board in the editor")
            .is_none(),
        "a Connect is not an editor open"
    );

    let bar = the_card(&bench, id)
        .bar(crate::BarLayer::Connection)
        .clone();
    let work = bar.work.expect("the failure");
    assert_eq!(work.words, crate::COULDNT_CONNECT);
    assert!(matches!(work.state, crate::BarWorkState::Failed { .. }));
    let reason = bar
        .details
        .sections
        .iter()
        .flat_map(|section| section.lines.iter())
        .find(|line| line.label == crate::COULDNT_CONNECT)
        .map(|line| line.value.clone());
    assert_eq!(reason.as_deref(), Some(failure.reason.as_str()));
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Step the bench and run the refresh tick's held-lens look, until `ready`.
fn tick_until(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
    what: &str,
    ready: impl Fn(&DeviceBench) -> bool,
) {
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    loop {
        bench.step(tasks);
        drive(bench.controller.try_pending_device_lens());
        if ready(bench) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}; roster now: {:?}",
            bench.view()
        );
    }
}

/// Unplug the bench's USB board and wait for its card to go offline.
fn unplug(bench: &mut DeviceBench, tasks: &TaskPool, id: crate::DeviceId) {
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(tasks, "the board to go offline", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == id && card.status == crate::DeviceStatus::Offline)
    });
}

/// The connected session shows on its card: the home page is up, no
/// editor panes, and the session control says the home page holds it.
fn on_its_card(bench: &DeviceBench) {
    let view = bench.controller.view();
    assert!(view.home.is_some(), "the home page");
    assert!(view.panes.is_empty(), "no editor");
    assert!(view.session.is_some_and(|session| session.connected));
}

/// A board running a library project at its head, its card Ready and
/// running, watched (no session).
fn running_library_board(
    uid: &str,
    endpoint: &str,
) -> (
    DeviceBench,
    TaskPool,
    FakeEsp32Device,
    crate::DeviceId,
    String,
) {
    let device = empty_light_player(uid);
    let (mut bench, tasks) = identified(&device, endpoint);
    let (device_uid, _project) = a_board_running_a_library_project(&mut bench, &tasks);
    let id = bench.view().devices[0].id;
    wait_running(&mut bench, &tasks, id);
    (bench, tasks, device, id, device_uid)
}

/// Step until `device`'s card is idle and running.
fn wait_running(bench: &mut DeviceBench, tasks: &TaskPool, device: crate::DeviceId) {
    bench.run_until(tasks, "the board to run its project", |bench| {
        bench.view().devices.iter().any(|card| {
            card.id == device
                && card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });
}

/// Two boards running the bundled example: a sim, then a board on USB.
///
/// The sim comes first, while the USB port is not granted yet: every sweep
/// mints the scripted USB transport a fresh fake link, and a sweep after
/// the board's link is attached would leave the editor's io on one the
/// effects layer let go of. Here the only sweep after the board's link is
/// the one that brings it.
fn a_sim_and_a_usb_board(
    uid: &str,
    endpoint: &str,
) -> (
    DeviceBench,
    TaskPool,
    FakeEsp32Device,
    crate::DeviceId,
    crate::DeviceId,
) {
    let device = empty_light_player(uid);
    let (mut bench, tasks) = DeviceBench::build(&device, endpoint, false, true);
    bench
        .controller
        .set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
            ScriptedSimSource {
                device: sim_light_player(),
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            },
        ))));
    drive(bench.controller.create_runtime_record(
        SIM_TARGET,
        None,
        &SIM_RANDOM,
        crate::RuntimeKind::Sim,
    ))
    .expect("a sim is created");
    bench.settle_library();
    let sim = bench.view().devices[0].id;
    // A powered-off sim's Power on sits in the card's Reconnect slot.
    bench.press_device(sim, "reconnect", OfferArgs::new());
    bench.run_until(&tasks, "the sim to say it runs nothing", |bench| {
        bench.view().devices.iter().any(|card| {
            card.id == sim
                && card.activity.is_none()
                && card.loaded_project == lpa_devices::view::LoadedProject::Empty
        })
    });
    bench.push_gesture(sim, bundled_example());
    wait_running(&mut bench, &tasks, sim);

    // The board is plugged in.
    bench.granted.set(true);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    bench.run_until(&tasks, "the board to say it runs nothing", |bench| {
        bench.view().devices.iter().any(|card| {
            card.id != sim
                && card.activity.is_none()
                && card.loaded_project == lpa_devices::view::LoadedProject::Empty
        })
    });
    let board = bench
        .view()
        .devices
        .iter()
        .find(|card| card.id != sim)
        .expect("the board's card")
        .id;
    bench.push_gesture(board, bundled_example());
    wait_running(&mut bench, &tasks, board);
    bench.settle_library();
    (bench, tasks, device, board, sim)
}

/// How many sessions the pool has installed, by its own records.
fn pool_installs(bench: &DeviceBench) -> usize {
    bench
        .controller
        .device_events()
        .iter()
        .filter(|record| {
            matches!(
                &record.kind,
                crate::DeviceEventKind::Pool { action, .. } if action == "install"
            )
        })
        .count()
}

/// The revision of the session's mirror, as last read.
fn mirror_revision(bench: &DeviceBench) -> Option<i64> {
    bench
        .controller
        .project_for_test()
        .snapshot()
        .sync
        .map(|sync| sync.revision)
}

/// The card the home view publishes for `device`.
fn the_card(bench: &DeviceBench, device: crate::DeviceId) -> crate::UiBoardCard {
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

/// Every action on `device`'s card names an offer the view's tree
/// publishes, the panel's Edit among them.
#[track_caller]
fn assert_card_offers(bench: &DeviceBench, device: crate::DeviceId) {
    let view = bench.controller.view();
    let card = the_card(bench, device);
    for path in card.offer_paths() {
        assert!(
            view.offers.get(path).is_some(),
            "{path} is on the card and not offered"
        );
    }
}

/// The channels the card's panel draws, in order.
fn channels(panel: &crate::UiBoardPanel) -> Vec<&str> {
    panel
        .controls
        .iter()
        .map(|control| control.view.channel.as_str())
        .collect()
}

/// When `device`'s card feed last finished a pull of its own.
fn feed_stamp(bench: &DeviceBench, device: crate::DeviceId) -> Option<f64> {
    bench
        .controller
        .device_feeds()
        .get(device)
        .and_then(|feed| feed.last_pull_completed_at())
}

/// The session's project holds unsaved work.
fn unsaved(bench: &DeviceBench) -> bool {
    crate::has_unsaved_work(&bench.controller.project_for_test().dirty_summary())
}
