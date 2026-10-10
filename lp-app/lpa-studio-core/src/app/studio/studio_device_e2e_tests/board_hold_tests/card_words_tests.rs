//! The card's words for a board another tab holds (P6), end to end: two
//! tabs on one hold bus, the card core publishes in the asking tab, and its
//! primary pressed by path the way the web's button and the app agent press
//! it.
//!
//! The rows: what a watcher's card says and carries (C1), the holder's level
//! in its aside and its tint (C2), the take-over in the bar and the primary
//! while it runs (C3), the tab that let go (C4), a take-over that failed
//! (C5), and the board under Online boards on the page (C6).

use super::*;
use crate::{BarLayer, BarWorkState, CornerMark, PictureSource, UiPrimary, UiStatusKind};

/// C1: B loads beside a holder that has a picture saved. B's card for the
/// board is the board's own card, Online, with the picture A saved (dimmed),
/// "Open in another tab" in the connection bar in orange, and Connect as
/// its primary — which IS the offer `devices/<board>/take-over`.
#[test]
fn c1_a_watchers_card_is_online_saying_another_tab_has_it_and_connects_by_taking_over() {
    let desk = Desk::new(&[("dev000000holdc1aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);
    a.feed_a_frame(MAC_A);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to show A's picture", |tabs| {
        tabs[1].frame_captured_at(MAC_A).is_some()
    });

    let card = b.ui_card(MAC_A);
    assert_eq!(card.presence, crate::UiBoardPresence::Online);
    assert_eq!(card.picture.source, PictureSource::Saved);
    assert!(card.picture.dim, "A's picture, dimmed");
    assert!(card.picture.frame.is_some());
    assert!(
        card.status
            .reading
            .as_deref()
            .is_some_and(|age| age.ends_with(" ago")),
        "the corner reads the picture's age: {:?}",
        card.status.reading
    );
    let connection = card.bar(BarLayer::Connection);
    assert_eq!(connection.summary, "Open in another tab");
    assert_eq!(connection.aside, None, "A only watches");
    assert_eq!(connection.tone, UiStatusKind::Attention);
    assert_eq!(connection.action, None);
    assert_eq!(
        card.status.mark,
        CornerMark::Notice(UiStatusKind::Attention)
    );

    let primary = primary_of(&card);
    assert_eq!(primary.word, "Connect");
    assert_eq!(primary.offer, b.verb(MAC_A, "take-over"));
    assert_eq!(primary.icon.as_deref(), Some("usb"));
    assert!(
        b.bench
            .offered(primary.offer.clone())
            .consequence()
            .is_routine(),
        "A is only watching"
    );
    every_card_action_is_offered(&b);
    assert_eq!(desk.attempts(0, "B"), 0, "B never tried the port");
}

/// C2: the holder's level is in the aside and in the cost. With its editor
/// open, B's bar says "editor open" and its Connect is Undoable (the error
/// tint, one click); busy with a push, B's Connect is disabled with the
/// holder's reason, which the offer reads out too.
#[test]
fn c2_the_holders_level_is_the_aside_and_the_cost() {
    let desk = Desk::new(&[("dev000000holdc2aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);
    let uid = a.registry_uid(MAC_A);
    a.bench.open_lens(&uid).expect("A's editor opens");
    run_until(&mut [&mut a, &mut b], "B to see A's editor", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Open)
    });
    let card = b.ui_card(MAC_A);
    let connection = card.bar(BarLayer::Connection);
    assert_eq!(connection.summary, "Open in another tab");
    assert_eq!(connection.aside.as_deref(), Some("editor open"));
    assert!(
        connection
            .details
            .notice()
            .and_then(|notice| notice.sentence.as_deref())
            .is_some_and(|sentence| sentence.contains("Taking it over closes it there")),
        "the details name what Connect costs"
    );
    let primary = primary_of(&card);
    assert_eq!(primary.offer, b.verb(MAC_A, "take-over"));
    let cost = b.bench.offered(primary.offer.clone());
    assert!(!cost.consequence().is_routine() && !cost.consequence().arms());
    a.bench.detach_lens();
    run_until(&mut [&mut a, &mut b], "B to see A watching", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Watching)
    });

    // Busy: a push that hangs.
    let board = a.device_id(MAC_A);
    a.push_plan.set(PushPlan::Hang);
    a.bench
        .press_device_lasting(board, "push", push_args(bundled_example()));
    run_until(&mut [&mut a, &mut b], "B to see A busy", |tabs| {
        matches!(
            tabs[1].fact(MAC_A).map(|fact| fact.level),
            Some(HoldLevel::Busy(_))
        )
    });
    let Some(HoldLevel::Busy(label)) = b.fact(MAC_A).map(|fact| fact.level) else {
        unreachable!("busy, just checked")
    };
    let card = b.ui_card(MAC_A);
    let reason = format!("Busy in the other tab: {label}");
    assert_eq!(
        card.bar(BarLayer::Connection).aside.as_deref(),
        Some(label.as_str())
    );
    assert_eq!(
        card.name_bar.primary,
        Some(UiPrimary::Unavailable {
            word: "Connect".to_string(),
            icon: "usb".to_string(),
            reason: reason.clone(),
        })
    );
    assert_eq!(
        b.bench.offer_reason(b.verb(MAC_A, "take-over")),
        reason,
        "the offer reads the same reason"
    );
    every_card_action_is_offered(&b);
}

/// C3: pressing the primary by path IS pressing `take-over`. While A is
/// asked the bar is the work "Asking the other tab…" and Connect is
/// disabled with those words; then "Opening…"; then the card is the
/// board's ordinary live card, whose primary is Edit.
#[test]
fn c3_the_primary_is_the_take_over_and_the_bar_works_while_it_runs() {
    let desk = Desk::new(&[("dev000000holdc3aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);

    let primary = primary_of(&b.ui_card(MAC_A));
    b.bench
        .press(primary.offer.clone(), primary.args.clone())
        .expect("the primary's press runs");

    let card = b.ui_card(MAC_A);
    let work = card
        .bar(BarLayer::Connection)
        .work
        .clone()
        .expect("asking is the bar's work");
    assert_eq!(work.words, "Asking the other tab\u{2026}");
    assert_eq!(work.state, BarWorkState::Running);
    assert_eq!(
        card.bar(BarLayer::Connection).summary,
        "Open in another tab",
        "the summary is unchanged"
    );
    assert_eq!(
        card.name_bar.primary,
        Some(UiPrimary::Unavailable {
            word: "Connect".to_string(),
            icon: "usb".to_string(),
            reason: "Asking the other tab\u{2026}".to_string(),
        })
    );
    assert_eq!(
        b.bench.offer_reason(primary.offer.clone()),
        "Asking the other tab\u{2026}"
    );

    run_until(&mut [&mut a, &mut b], "B to have the board", |tabs| {
        tabs[1]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
    let card = b.ui_card(MAC_A);
    assert!(
        card.bar(BarLayer::Connection)
            .work
            .as_ref()
            .is_none_or(|work| work.state != BarWorkState::Running),
        "the take-over is done: {:?}",
        card.bar(BarLayer::Connection).work
    );
    assert_eq!(
        card.bar(BarLayer::Connection).summary,
        "USB \u{b7} connected"
    );
    // An ordinary live card: its Connect opens its panel here (`connect`),
    // with nothing left to take over.
    let Some(UiPrimary::Offer(connect)) = card.name_bar.primary.as_ref() else {
        panic!("an ordinary live card: {:?}", card.name_bar.primary);
    };
    assert_eq!(connect.word, "Connect");
    assert_eq!(
        connect.offer,
        b.verb(MAC_A, "connect"),
        "nothing left to take over"
    );
}

/// C4: the tab that let go on request keeps its card: "Taken by another
/// tab", orange, Connect (the same `take-over`, which takes the board back),
/// its picture the one it saved on the way out.
#[test]
fn c4_the_tab_that_let_go_says_taken_by_another_tab_and_connects_to_take_it_back() {
    let desk = Desk::new(&[("dev000000holdc4aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);
    a.feed_a_frame(MAC_A);

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "A to wear \"taken\"", |tabs| {
        tabs[1].holds(MAC_A)
            && tabs[0].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
            && hears_a_holder(&tabs[0], MAC_A)
    });

    let card = a.ui_card(MAC_A);
    assert_eq!(card.presence, crate::UiBoardPresence::Online);
    let connection = card.bar(BarLayer::Connection);
    assert_eq!(connection.summary, "Taken by another tab");
    assert_eq!(connection.tone, UiStatusKind::Attention);
    assert_eq!(connection.aside, None);
    assert_eq!(card.picture.source, PictureSource::Saved);
    assert!(card.picture.dim && card.picture.frame.is_some());
    let primary = primary_of(&card);
    assert_eq!(primary.word, "Connect");
    assert_eq!(primary.offer, a.verb(MAC_A, "take-over"));
    every_card_action_is_offered(&a);

    // Pressing it takes the board back; B then wears "Taken".
    a.bench
        .press(primary.offer.clone(), primary.args.clone())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "A to have it back", |tabs| {
        tabs[0].holds(MAC_A)
            && tabs[0]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[1].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
    });
    assert_eq!(
        b.ui_card(MAC_A).bar(BarLayer::Connection).summary,
        "Taken by another tab"
    );
}

/// C5: a holder that never answers: after five seconds the bar is striped
/// with "That tab didn't answer", its Retry is the same `take-over`, and
/// Connect is a press again.
#[test]
fn c5_a_take_over_that_failed_is_striped_with_retry_on_the_take_over() {
    let desk = Desk::new(&[("dev000000holdc5aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    let take_over = b.verb(MAC_A, "take-over");
    b.bench
        .press(take_over.clone(), OfferArgs::new())
        .expect("press");

    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT * 4;
    while b.take_over_failed(MAC_A) != Some(true) {
        a.hold.as_ref().expect("an edge").take_inbox();
        a.bench.step(&a.tasks);
        b.step();
        assert!(std::time::Instant::now() < deadline, "no timeout");
    }

    let card = b.ui_card(MAC_A);
    let connection = card.bar(BarLayer::Connection);
    assert_eq!(connection.summary, "Open in another tab");
    let work = connection.work.as_ref().expect("the failure");
    assert_eq!(work.words, "That tab didn't answer");
    let BarWorkState::Failed { retry: Some(retry) } = &work.state else {
        panic!("Retry: {work:?}");
    };
    assert_eq!(retry.word, "Retry");
    assert_eq!(retry.offer, take_over);
    assert_eq!(primary_of(&card).offer, take_over, "Connect presses again");
    every_card_action_is_offered(&b);

    // Retry works once the holder answers.
    b.bench
        .press(retry.offer.clone(), retry.args.clone())
        .expect("retry");
    run_until(&mut [&mut a, &mut b], "B to have the board", |tabs| {
        tabs[1]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
}

/// C6 (A3): on the page, a record-backed board another tab holds is under
/// Online boards, not Offline boards — with its card there.
#[test]
fn c6_a_held_board_is_under_online_boards_not_offline() {
    let desk = Desk::new(&[("dev000000holdc6aa", MAC_A)]);
    let (_a, b) = holder_and_watcher(&desk);
    let id = b.device_id(MAC_A);
    let home = b.bench.controller.view().home.expect("the home view");

    assert!(
        home.sections.online.iter().any(|board| board.id == id),
        "Online boards: {:?}",
        home.sections.online
    );
    assert!(
        !home.sections.offline.iter().any(|board| board.id == id),
        "not Offline boards: {:?}",
        home.sections.offline
    );
    assert!(home.devices.cards.iter().any(|card| card.device == id));
}

/// Whether the tab's book lists another tab's hold on the board with `mac`
/// (it heard the holder's announcement, not only its release).
fn hears_a_holder(tab: &Tab, mac: &str) -> bool {
    let mac = crate::BoardKey::parse(mac).expect("a mac");
    tab.bench
        .controller
        .board_hold_book()
        .is_some_and(|book| book.others_for_mac(mac).next().is_some())
}

/// The card's primary, which must be an offer.
#[track_caller]
fn primary_of(card: &crate::UiBoardCard) -> crate::UiCardAction {
    match &card.name_bar.primary {
        Some(UiPrimary::Offer(action)) => action.clone(),
        other => panic!("the primary is an offer, not {other:?}"),
    }
}

/// Every action on every card the tab publishes names an offer its tree
/// publishes.
#[track_caller]
fn every_card_action_is_offered(tab: &Tab) {
    let view = tab.bench.controller.view();
    let home = view.home.expect("the home view");
    for card in &home.devices.cards {
        for path in card.offer_paths() {
            assert!(
                view.offers.get(path).is_some(),
                "{path} is on the card and not offered"
            );
        }
    }
}
