//! One tab holds a board, end to end: two (or three) Studio tabs, each a
//! whole controller with its own effects layer, on one in-memory hold bus
//! ([`crate::MemoryBoardHoldBus`]) and one desk of boards
//! ([`SharedUsbBoard`]) whose ports open for one tab at a time — the OS's
//! own exclusion — counting every open each tab attempts.
//!
//! The rows: the inbound door (a note reaches the other tab's book through
//! the actor's queue), the holder's side (T1–T8: claim, gate, refused open,
//! levels, crash, the answer, no edge) and the asker's side (Z1–Z10: the
//! `take-over` offer pressed by path, refusals, no answer, levels, taking it
//! back, two of a kind, two askers, pictures, the Online rule, a crash;
//! Z9's unit row is `device_roster.rs`'s).

use std::collections::BTreeMap;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo};

use super::*;
use crate::app::studio::studio_actor::StudioActor;
use crate::app::studio::studio_command::StudioCommand;
use crate::{
    BoardHoldEdge, BookChange, DeviceStatus, HoldKey, HoldLevel, HoldNote, MemoryBoardHold,
    MemoryBoardHoldBus, TabId, UsbPair,
};

// ---------------------------------------------------------------------
// The inbound door
// ---------------------------------------------------------------------

#[test]
fn a_note_another_tab_posts_reaches_this_tabs_book_through_the_queue() {
    let bus = MemoryBoardHoldBus::new();
    let (tab_a, tab_b) = (bus.tab(), bus.tab());
    let mut controller = StudioController::new(|| 0.0);
    controller.set_board_hold_edge(Rc::new(tab_b.clone()));
    let (mut actor, handle) =
        StudioActor::new(controller, |_budget: Duration| core::future::ready(()));

    tab_a.post(&HoldNote::Holds {
        key: board(),
        level: HoldLevel::Open,
    });
    for (from, note) in tab_b.take_inbox() {
        handle.tx.send(StudioCommand::BoardHold { from, note });
    }
    drive(actor.run_one_batch_for_test());

    let book = actor
        .controller_mut_for_test()
        .board_hold_book()
        .expect("an edge brings a book");
    assert_eq!(book.tab(), &tab_b.tab_id());
    let hold = book.held_elsewhere(&board()).expect("tab a's hold");
    assert_eq!(hold.tab, Some(tab_a.tab_id()));
    assert_eq!(book.level_of(&board()), Some(&HoldLevel::Open));
    assert_eq!(book.claims_for_usb(0x303a, 0x1001), 1);
}

#[test]
fn the_door_reports_what_a_note_changed_and_ignores_the_tabs_own_name() {
    let bus = MemoryBoardHoldBus::new();
    let tab = bus.tab();
    let mut controller = StudioController::new(|| 0.0);
    controller.set_board_hold_edge(Rc::new(tab.clone()));
    let note = HoldNote::Holds {
        key: board(),
        level: HoldLevel::Watching,
    };

    assert!(
        controller
            .on_hold_note(tab.tab_id(), note.clone())
            .is_empty(),
        "a note in this tab's own name is its echo"
    );
    assert_eq!(
        controller.on_hold_note(TabId::new("other"), note),
        vec![BookChange::HeldElsewhere {
            key: board(),
            holder: Some(TabId::new("other")),
            level: Some(HoldLevel::Watching),
        }]
    );
}

/// With no edge installed (a browser without Web Locks or a
/// `BroadcastChannel`, or any rig that installs none) there is no book, and
/// a note changes nothing.
#[test]
fn without_an_edge_a_note_changes_nothing() {
    let mut controller = StudioController::new(|| 0.0);

    assert!(controller.board_hold_book().is_none());
    assert!(
        controller
            .on_hold_note(TabId::new("other"), HoldNote::Who)
            .is_empty()
    );
    assert!(controller.board_hold_book().is_none());
}

// ---------------------------------------------------------------------
// The holder's side (T1–T8)
// ---------------------------------------------------------------------

/// T1: a tab that loads while another holds the board never opens its
/// port. It learns the hold before its first sweep, the port is attached
/// and gated, and — one claim, one port — merges onto the board's own
/// record-backed card, which wears the fact. No "new device found".
#[test]
fn t1_a_tab_that_loads_beside_a_holder_never_opens_the_held_port() {
    let desk = Desk::new(&[("dev000000holdt1aa", MAC_A)]);
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold the board", |tabs| tabs[0].holds(MAC_A));

    let mut b = desk.tab("B", &[0]);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to see the board held", |tabs| {
        tabs[1].card(MAC_A).is_some_and(|card| card.held_elsewhere.is_some())
    });
    for _ in 0..40 {
        step(&mut [&mut a, &mut b]);
    }

    assert_eq!(desk.attempts(0, "B"), 0, "B never opened the held port");
    let card = b.card(MAC_A).expect("B's card for the board");
    assert_eq!(
        card.held_elsewhere,
        Some(held(HoldLevel::Watching, false)),
        "{card:?}"
    );
    assert_eq!(card.status, DeviceStatus::Attached, "merged onto its card");
    assert!(
        b.bench.view().pending.is_empty(),
        "no new device found: {:?}",
        b.bench.view().pending
    );
    assert!(a.holds(MAC_A), "A still holds it");
    assert_eq!(
        desk.bus.holder_of(&usb_key(MAC_A)),
        Some(a.tab_id()),
        "A's lock"
    );
}

/// T2: two tabs load before anyone claims. The OS lets A open the port and
/// refuses B; A's claim then arrives, and B reads its refused open against
/// it — one claim, one port — so the port merges onto the board's card,
/// the same end state as T1.
#[test]
fn t2_a_refused_open_read_against_a_later_claim_is_the_other_tabs() {
    let desk = Desk::new(&[("dev000000holdt2aa", MAC_A)]);
    let mut a = desk.tab("A", &[0]);
    let mut b = desk.tab("B", &[0]);
    // Both prime with nothing held, and both sweep: A's open lands first.
    run_until(&mut [&mut a, &mut b], "both tabs to try the port", |_| {
        desk.attempts(0, "A") > 0 && desk.attempts(0, "B") > 0
    });
    run_until(&mut [&mut a, &mut b], "A to hold the board", |tabs| {
        tabs[0].holds(MAC_A)
    });
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to read the refusal as held", |tabs| {
        tabs[1].card(MAC_A).is_some_and(|card| card.held_elsewhere.is_some())
            && tabs[1].bench.view().pending.is_empty()
    });

    assert!(
        !desk.log_of(&["refused:B"]).is_empty(),
        "the OS refused B's open"
    );
    let card = b.card(MAC_A).expect("B's card");
    assert_eq!(card.status, DeviceStatus::Attached, "{card:?}");
    assert_eq!(card.held_elsewhere, Some(held(HoldLevel::Watching, false)));
    assert!(b.bench.view().pending.is_empty());
    assert!(a.holds(MAC_A));
}

/// T3: two boards of one kind, both held by A. B's two ports are no more
/// than A's two claims, so both are gated — never opened — and with two of
/// a kind no port can be named: both records carry the fact, and the ports
/// the claims account for are no "new device found".
#[test]
fn t3_two_boards_of_a_kind_both_held_are_both_gated() {
    let desk = Desk::new(&[("dev000000holdt3aa", MAC_A), ("dev000000holdt3bb", MAC_B)]);
    let mut a = desk.tab("A", &[0, 1]);
    run_until(&mut [&mut a], "A to hold both boards", |tabs| {
        tabs[0].holds(MAC_A) && tabs[0].holds(MAC_B)
    });

    let mut b = desk.tab("B", &[0, 1]);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to see both boards held", |tabs| {
        [MAC_A, MAC_B].iter().all(|mac| tabs[1].fact(mac).is_some())
            && tabs[1].bench.view().pending.is_empty()
            && tabs[1]
                .bench
                .controller
                .devices_for_test()
                .roster()
                .pending()
                .len()
                == 2
    });
    for _ in 0..40 {
        step(&mut [&mut a, &mut b]);
    }

    assert_eq!(desk.attempts(0, "B"), 0);
    assert_eq!(desk.attempts(1, "B"), 0);
    // Two of a kind: neither port is named, so the boards keep no link
    // here; the ports wait, held, out of sight.
    let roster = b.bench.controller.devices_for_test().roster();
    assert_eq!(roster.pending().len(), 2, "both ports are attached, held");
    assert!(
        roster
            .pending()
            .iter()
            .all(|pending| pending.evidence().link_held_by_tab())
    );
    assert!(b.bench.view().pending.is_empty(), "no new device found");
    for mac in [MAC_A, MAC_B] {
        assert_eq!(b.fact(mac), Some(held(HoldLevel::Watching, false)));
    }
}

/// T4: two boards of one kind, only one held by A. B's two ports outnumber
/// the one claim, so B opens both: the OS refuses the held one, which is
/// then read as A's (one claim, one refused port: it is that board), and
/// the other is B's own board, which B claims.
#[test]
fn t4_two_of_a_kind_one_held_opens_both_and_reads_the_refusal() {
    let desk = Desk::new(&[("dev000000holdt4aa", MAC_A), ("dev000000holdt4bb", MAC_B)]);
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold board A", |tabs| tabs[0].holds(MAC_A));

    let mut b = desk.tab("B", &[0, 1]);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to hold board B", |tabs| {
        tabs[1].holds(MAC_B)
    });
    b.library_changed();
    run_until(
        &mut [&mut a, &mut b],
        "B to read board A's port as held",
        |tabs| {
            tabs[1]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Attached)
                && tabs[1].bench.view().pending.is_empty()
        },
    );

    assert!(desk.attempts(0, "B") >= 1, "B tried board A's port");
    assert!(
        !desk.log_of(&["refused:B"]).is_empty(),
        "and the OS refused it"
    );
    assert_eq!(b.fact(MAC_A), Some(held(HoldLevel::Watching, false)));
    let own = b.card(MAC_B).expect("board B's card in B");
    assert_eq!(own.status, DeviceStatus::Ready);
    assert_eq!(own.held_elsewhere, None);
    assert_eq!(desk.bus.holder_of(&usb_key(MAC_B)), Some(b.tab_id()));
    // A hears B's claim on board B.
    run_until(&mut [&mut a, &mut b], "A to hear B's hold", |tabs| {
        tabs[0]
            .bench
            .controller
            .board_hold_book()
            .is_some_and(|book| book.held_elsewhere(&usb_key(MAC_B)).is_some())
    });
}

/// T5: the holder announces what taking the board would cost, and the
/// other tab's fact follows: `Open` while its editor is on the board,
/// busy (with the activity's own label) while it works on the board,
/// `Watching` otherwise.
#[test]
fn t5_the_holders_level_follows_what_it_is_doing() {
    let desk = Desk::new(&[("dev000000holdt5aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    let board = a.device_id(MAC_A);
    assert_eq!(b.fact(MAC_A), Some(held(HoldLevel::Watching, false)));

    // Open: the editor on the board.
    a.run_a_project(MAC_A, &mut b);
    let uid = a.registry_uid(MAC_A);
    a.bench.open_lens(&uid).expect("the editor opens on A's board");
    run_until(&mut [&mut a, &mut b], "B to see A's editor open", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Open)
    });
    a.bench.detach_lens();
    run_until(&mut [&mut a, &mut b], "B to see A watching", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Watching)
    });

    // Busy: a push that hangs until it is cancelled (over what the board
    // runs, so the user's second click).
    a.push_plan.set(PushPlan::Hang);
    a.bench
        .press_device_lasting(board, "push", push_args(bundled_example()));
    run_until(&mut [&mut a, &mut b], "B to see A busy", |tabs| {
        matches!(
            tabs[1].fact(MAC_A).map(|fact| fact.level),
            Some(HoldLevel::Busy(_))
        )
    });
    let label = a
        .card(MAC_A)
        .and_then(|card| card.activity)
        .map(|activity| activity.label)
        .expect("A's push");
    assert_eq!(
        b.fact(MAC_A).map(|fact| fact.level),
        Some(HoldLevel::Busy(label)),
        "the busy level carries the activity's own label"
    );
    a.bench.press_device(board, "cancel", OfferArgs::new());
    run_until(&mut [&mut a, &mut b], "B to see A watching again", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Watching)
    });
}

/// T6: the holder crashes (its tab closes with no word): its lock vanishes,
/// B's sentinel fires, and the fact clears — and B opens nothing. The board
/// is the ordinary `Attached` card, and Connect is how it opens.
#[test]
fn t6_a_holder_that_dies_clears_the_fact_and_nothing_opens() {
    let desk = Desk::new(&[("dev000000holdt6aa", MAC_A)]);
    let (a, mut b) = holder_and_watcher(&desk);

    desk.crash(a);
    run_until(&mut [&mut b], "the fact to clear", |tabs| {
        tabs[0]
            .card(MAC_A)
            .is_some_and(|card| card.held_elsewhere.is_none())
    });
    for _ in 0..200 {
        b.step();
    }

    assert_eq!(desk.attempts(0, "B"), 0, "nothing opened the port");
    let card = b.card(MAC_A).expect("the board's card");
    assert_eq!(card.status, DeviceStatus::Attached, "{card:?}");
    let connect = b.bench.device_verb(card.id, "connect");
    b.bench.offered(connect);
    let take_over = b.bench.device_verb(card.id, "take-over");
    b.bench.not_offered(take_over);
}

/// T7, busy: a holder working on the board refuses an ask with the
/// activity's own label, and nothing closes or lets go.
#[test]
fn t7_a_busy_holder_refuses_and_nothing_closes() {
    let desk = Desk::new(&[("dev000000holdt7aa", MAC_A)]);
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold the board", |tabs| tabs[0].holds(MAC_A));
    let board = a.device_id(MAC_A);
    let asker = desk.bus.tab();

    a.push_plan.set(PushPlan::Hang);
    a.wait_until_empty(MAC_A);
    a.bench.push_gesture(board, bundled_example());
    run_until(&mut [&mut a], "A to be busy", |tabs| {
        tabs[0].card(MAC_A).is_some_and(|card| card.activity.is_some())
    });
    let label = a
        .card(MAC_A)
        .and_then(|card| card.activity)
        .map(|activity| activity.label)
        .expect("A's push");
    asker.take_inbox();
    asker.post(&HoldNote::Ask {
        request: 1,
        key: usb_key(MAC_A),
        holder: None,
    });
    for _ in 0..40 {
        a.step();
    }

    let answers: Vec<HoldNote> = asker
        .take_inbox()
        .into_iter()
        .map(|(_, note)| note)
        .filter(|note| matches!(note, HoldNote::Answer { .. }))
        .collect();
    assert_eq!(
        answers,
        vec![HoldNote::Answer {
            request: 1,
            asker: asker.tab_id(),
            outcome: crate::AskOutcome::Refused(crate::AskRefusal::Busy(label)),
        }]
    );
    assert!(desk.log_of(&["close:A"]).is_empty(), "nothing closed");
    assert!(a.holds(MAC_A), "the hold stands");
    assert_eq!(desk.bus.holder_of(&usb_key(MAC_A)), Some(a.tab_id()));
    assert!(
        a.card(MAC_A).is_some_and(|card| card.activity.is_some()),
        "the push runs on"
    );
}

/// T7, idle: the holder lets go in order — its editor closes, the board's
/// last picture is written, the link disconnects, the lock goes only after
/// the port closed, then `Released` — and its own card wears "taken by
/// another tab". Nothing in that tab reopens the port afterwards.
#[test]
fn t7_an_idle_holder_lets_go_in_order() {
    let desk = Desk::new(&[("dev000000holdt7bb", MAC_A)]);
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold the board", |tabs| tabs[0].holds(MAC_A));
    let asker = desk.bus.tab();

    a.run_a_project_alone(MAC_A);
    a.feed_a_frame(MAC_A);
    let uid = a.registry_uid(MAC_A);
    let last_frame = a.frame_captured_at(MAC_A).expect("A's picture");
    a.bench.open_lens(&uid).expect("the editor opens");
    asker.take_inbox();

    asker.post(&HoldNote::Ask {
        request: 2,
        key: usb_key(MAC_A),
        holder: None,
    });
    let mut order: Vec<&str> = Vec::new();
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    while !order.contains(&"released") {
        a.step();
        if a.bench.lens_session_id().is_none() && !order.contains(&"lens closed") {
            order.push("lens closed");
        }
        if !desk.log_of(&["close:A"]).is_empty() && !order.contains(&"port closed") {
            let written = a.sidecar_captured_at(&uid).expect("a sidecar");
            assert!(
                (written - last_frame).abs() < 1e-6,
                "the last picture was written before the port closed: {written} vs {last_frame}"
            );
            order.push("port closed");
        }
        if desk.bus.holder_of(&usb_key(MAC_A)).is_none() && !order.contains(&"lock released") {
            order.push("lock released");
        }
        for (_, note) in asker.take_inbox() {
            if note
                == (HoldNote::Answer {
                    request: 2,
                    asker: asker.tab_id(),
                    outcome: crate::AskOutcome::Released,
                })
            {
                order.push("released");
            }
        }
        assert!(std::time::Instant::now() < deadline, "no answer: {order:?}");
    }
    assert_eq!(
        order,
        ["lens closed", "port closed", "lock released", "released"],
        "the holder's order"
    );
    let card = a.card(MAC_A).expect("A's card");
    assert_eq!(card.status, DeviceStatus::Attached, "port closed, card kept");
    assert_eq!(
        card.held_elsewhere.map(|fact| fact.taken_from_here),
        Some(true),
        "A's own board says it was taken"
    );
    assert!(!a.holds(MAC_A));

    // Nothing in A reopens the port: not a sweep, not a hotplug.
    let opens = desk.attempts(0, "A");
    a.bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    for _ in 0..400 {
        a.step();
    }
    assert_eq!(desk.attempts(0, "A"), opens, "nothing reopened the port");
}

/// T8: without an edge every flow is as before holds existed: the sweep
/// runs at once and opens the port, and nothing is gated or claimed.
#[test]
fn t8_without_an_edge_the_sweep_opens_as_before() {
    let desk = Desk::new(&[("dev000000holdt8aa", MAC_A)]);
    let mut a = desk.tab_without_edge("A", &[0]);
    run_until(&mut [&mut a], "the board to identify", |tabs| {
        tabs[0]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });

    assert_eq!(desk.attempts(0, "A"), 1);
    assert!(a.bench.controller.board_hold_book().is_none());
    assert!(desk.bus.holder_of(&usb_key(MAC_A)).is_none(), "no lock");
}

// ---------------------------------------------------------------------
// The asker's side (Z1–Z10)
// ---------------------------------------------------------------------

/// Z1 and Z5: B presses `devices/<board>/take-over` by path. A lets go in
/// order — its editor closes, its port closes, its lock goes — and only
/// then does B's port open; B ends `Ready` on the board's MAC, and A's
/// card says the board was taken. Then A presses its own `take-over`, and
/// the same happens the other way round.
#[test]
fn z1_z5_take_over_by_path_and_take_it_back() {
    let desk = Desk::new(&[("dev000000holdz1aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);
    let uid = a.registry_uid(MAC_A);
    a.bench.open_lens(&uid).expect("A's editor opens");
    run_until(&mut [&mut a, &mut b], "B to see A's editor", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Open)
    });

    // Z1: B takes it.
    let path = b.verb(MAC_A, "take-over");
    assert!(
        !b.bench.offered(path.clone()).consequence().is_routine(),
        "it closes A's editor: the tint says so"
    );
    b.bench.press(path, OfferArgs::new()).expect("the press runs");
    assert_eq!(
        b.take_over_words(MAC_A).as_deref(),
        Some("Asking the other tab\u{2026}")
    );
    let mut order: Vec<&str> = Vec::new();
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT * 4;
    while !b
        .card(MAC_A)
        .is_some_and(|card| card.status == DeviceStatus::Ready)
    {
        step(&mut [&mut a, &mut b]);
        if a.bench.lens_session_id().is_none() && !order.contains(&"A's editor closed") {
            order.push("A's editor closed");
        }
        if !desk.log_of(&["close:A"]).is_empty() && !order.contains(&"A's port closed") {
            order.push("A's port closed");
        }
        if desk.bus.holder_of(&usb_key(MAC_A)) != Some(a.tab_id())
            && !order.contains(&"A's lock released")
        {
            order.push("A's lock released");
        }
        if !desk.log_of(&["open:B"]).is_empty() && !order.contains(&"B's port opened") {
            order.push("B's port opened");
        }
        assert!(std::time::Instant::now() < deadline, "no take-over: {order:?}");
    }
    assert_eq!(
        order,
        [
            "A's editor closed",
            "A's port closed",
            "A's lock released",
            "B's port opened"
        ]
    );
    assert!(desk.log_of(&["refused:B"]).is_empty(), "B never fought A");
    let card = b.card(MAC_A).expect("B's card");
    assert_eq!(card.held_elsewhere, None);
    run_until(&mut [&mut a, &mut b], "B to hold it and A to hear", |tabs| {
        tabs[1].holds(MAC_A) && tabs[0].fact(MAC_A).is_some_and(|fact| fact.level == HoldLevel::Watching)
    });
    assert_eq!(b.take_over_words(MAC_A), None, "done: nothing to say");
    assert_eq!(a.fact(MAC_A), Some(held(HoldLevel::Watching, true)));
    a.bench.not_offered(a.verb(MAC_A, "connect"));

    // Z5: A takes it back, the same way.
    let path = a.verb(MAC_A, "take-over");
    assert!(a.bench.offered(path.clone()).consequence().is_routine());
    a.bench.press(path, OfferArgs::new()).expect("the press runs");
    run_until(&mut [&mut a, &mut b], "A to have it back", |tabs| {
        tabs[0].holds(MAC_A)
            && tabs[0]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[1].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
    });
    assert_eq!(a.fact(MAC_A), None, "A's own again");
    assert_eq!(
        desk.log_of(&["open:", "close:", "refused:"]),
        ["open:A", "close:A", "open:B", "close:B", "open:A"],
        "one holder at a time, never a refusal"
    );
}

/// Z2: the holder is busy when the ask lands. B fails with the holder's
/// words, A changes nothing, and B's offer is disabled with that reason —
/// until A's activity ends.
#[test]
fn z2_a_busy_holder_refuses_and_the_offer_waits() {
    let desk = Desk::new(&[("dev000000holdz2aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    let board = a.device_id(MAC_A);

    // A starts a push B has not heard about yet.
    a.push_plan.set(PushPlan::Hang);
    a.wait_until_empty(MAC_A);
    a.bench.push_gesture(board, bundled_example());
    run_until(&mut [&mut a], "A to be busy", |tabs| {
        tabs[0].card(MAC_A).is_some_and(|card| card.activity.is_some())
    });
    let path = b.verb(MAC_A, "take-over");
    b.bench.press(path.clone(), OfferArgs::new()).expect("press");
    run_until(&mut [&mut a, &mut b], "B to hear the refusal", |tabs| {
        tabs[1]
            .take_over_words(MAC_A)
            .is_some_and(|words| words.starts_with("Busy in the other tab: "))
    });

    let words = b.take_over_words(MAC_A).expect("B's words");
    assert_eq!(b.take_over_failed(MAC_A), Some(true));
    assert_eq!(b.bench.offer_reason(path.clone()), words, "the same reason");
    assert!(a.holds(MAC_A), "A changed nothing");
    assert!(desk.log_of(&["close:A"]).is_empty());
    assert_eq!(desk.attempts(0, "B"), 0);

    a.bench.press_device(board, "cancel", OfferArgs::new());
    run_until(&mut [&mut a, &mut b], "B's offer to come back", |tabs| {
        tabs[1]
            .bench
            .controller
            .view()
            .offers
            .get(&path)
            .is_some_and(crate::UiOffer::is_enabled)
    });
}

/// Z3: the holder never answers (a tab of an older build ignores the
/// note): after five seconds B says so. Pressing again once the holder
/// answers works.
#[test]
fn z3_no_answer_fails_after_five_seconds_and_a_retry_works() {
    let desk = Desk::new(&[("dev000000holdz3aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    let path = b.verb(MAC_A, "take-over");
    b.bench.press(path.clone(), OfferArgs::new()).expect("press");
    assert_eq!(
        b.bench.offer_reason(path.clone()),
        "Asking the other tab\u{2026}"
    );

    // A runs, but never hears the ask.
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT * 4;
    while b.take_over_failed(MAC_A) != Some(true) {
        a.hold.as_ref().expect("an edge").take_inbox();
        a.bench.step(&a.tasks);
        b.step();
        assert!(std::time::Instant::now() < deadline, "no timeout");
    }
    assert_eq!(
        b.take_over_words(MAC_A).as_deref(),
        Some("That tab didn't answer")
    );
    assert!(a.holds(MAC_A));
    assert_eq!(desk.attempts(0, "B"), 0);

    // The holder answers now: pressing again works.
    b.bench.press(path, OfferArgs::new()).expect("press again");
    run_until(&mut [&mut a, &mut b], "B to have the board", |tabs| {
        tabs[1]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
    assert_eq!(b.take_over_words(MAC_A), None);
}

/// Z4: what the take-over costs is what it closes in the other tab, by its
/// last word: a level not said yet and `Open` are `Undoable`, `Watching`
/// is `Routine`, `Busy` is disabled. The plain Connect, Retry and Identify
/// are never offered while the fact stands.
#[test]
fn z4_the_take_over_costs_what_it_closes_over_there() {
    let desk = Desk::new(&[("dev000000holdz4aa", MAC_A)]);
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold the board", |tabs| tabs[0].holds(MAC_A));
    let mut b = desk.tab("B", &[0]);
    b.library_changed();
    // B alone: A has not answered B's `Who` yet.
    run_until(&mut [&mut b], "B to see the primed hold", |tabs| {
        tabs[0].fact(MAC_A).is_some()
    });
    assert_eq!(
        b.fact(MAC_A).map(|fact| fact.level),
        Some(HoldLevel::Open),
        "an unsaid level reads as Open"
    );
    let take_over = |tab: &mut Tab| tab.bench.offered(tab.verb(MAC_A, "take-over"));
    let undoable = |offer: &crate::UiOffer| {
        offer.is_enabled() && !offer.consequence().is_routine() && !offer.consequence().arms()
    };
    assert!(undoable(&take_over(&mut b)));
    for verb in ["connect", "retry", "identify"] {
        b.bench.not_offered(b.verb(MAC_A, verb));
    }

    run_until(&mut [&mut a, &mut b], "A's answer to Who", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Watching)
    });
    let offer = take_over(&mut b);
    assert!(offer.is_enabled() && offer.consequence().is_routine());

    a.run_a_project(MAC_A, &mut b);
    let uid = a.registry_uid(MAC_A);
    a.bench.open_lens(&uid).expect("A's editor opens");
    run_until(&mut [&mut a, &mut b], "B to see A's editor", |tabs| {
        tabs[1].fact(MAC_A).map(|fact| fact.level) == Some(HoldLevel::Open)
    });
    assert!(undoable(&take_over(&mut b)));
    a.bench.detach_lens();

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
    assert_eq!(
        b.bench.offer_reason(b.verb(MAC_A, "take-over")),
        format!("Busy in the other tab: {label}")
    );
    for verb in ["connect", "retry", "identify"] {
        b.bench.not_offered(b.verb(MAC_A, verb));
    }
}

/// Z6: two boards of one kind, both held by A. B asks for X: only X is let
/// go. B opens every port of the kind the hold kept shut; X's opens and
/// says it is X, Y's is refused again, and Y stays held with its card as it
/// was.
#[test]
fn z6_two_of_a_kind_asking_for_one_frees_only_that_one() {
    let desk = Desk::new(&[("dev000000holdz6aa", MAC_A), ("dev000000holdz6bb", MAC_B)]);
    let mut a = desk.tab("A", &[0, 1]);
    run_until(&mut [&mut a], "A to hold both", |tabs| {
        tabs[0].holds(MAC_A) && tabs[0].holds(MAC_B)
    });
    let mut b = desk.tab("B", &[0, 1]);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to see both held", |tabs| {
        [MAC_A, MAC_B]
            .iter()
            .all(|mac| tabs[1].fact(mac) == Some(held(HoldLevel::Watching, false)))
    });
    let y_before = b.card(MAC_B).expect("Y's card");
    // Z9: a held board with no port here is still Online.
    let split = crate::split_roster(&b.bench.controller.device_roster_view());
    assert!(
        [MAC_A, MAC_B].iter().all(|mac| {
            let id = b.device_id(mac);
            split.connected.iter().any(|card| card.id == id)
        }),
        "held boards are Online: {split:?}"
    );

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "B to have X", |tabs| {
        tabs[1]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
    run_until(&mut [&mut a, &mut b], "Y's port to be refused again", |_| {
        desk.attempts(1, "B") >= 1
    });
    for _ in 0..40 {
        step(&mut [&mut a, &mut b]);
    }

    assert!(a.holds(MAC_B), "Y stays A's");
    assert!(!a.holds(MAC_A), "X went");
    assert!(!desk.log_of(&["refused:B"]).is_empty(), "Y refused B");
    assert_eq!(desk.boards[1].state.borrow().holder, Some("A"));
    let y_after = b.card(MAC_B).expect("Y's card");
    assert_eq!(y_after.held_elsewhere, y_before.held_elsewhere);
    assert_eq!(y_after.status, y_before.status, "Y's card did not change");
    assert!(b.bench.view().pending.is_empty(), "no new device found");
}

/// Z7: two tabs ask one holder for one board. The first ask wins; the
/// second is told the board is not held there any more and fails with
/// "Another tab has it now". Neither tab opens the port twice.
#[test]
fn z7_two_askers_one_wins_and_the_other_is_told_another_tab_has_it() {
    let desk = Desk::new(&[("dev000000holdz7aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    let mut c = desk.tab("C", &[0]);
    c.library_changed();
    run_until(&mut [&mut a, &mut b, &mut c], "C to see the board held", |tabs| {
        tabs[2].fact(MAC_A) == Some(held(HoldLevel::Watching, false))
    });

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("B presses");
    c.bench
        .press(c.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("C presses");
    run_until(&mut [&mut a, &mut b, &mut c], "B to win and C to fail", |tabs| {
        tabs[1]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[2].take_over_failed(MAC_A) == Some(true)
    });

    assert_eq!(
        c.take_over_words(MAC_A).as_deref(),
        Some("Another tab has it now")
    );
    assert_eq!(desk.attempts(0, "B"), 1, "B opened once");
    assert_eq!(desk.attempts(0, "C"), 0, "C never opened it");
    run_until(&mut [&mut a, &mut b, &mut c], "C to see B holding it", |tabs| {
        tabs[2]
            .bench
            .controller
            .board_hold_book()
            .and_then(|book| book.held_elsewhere(&usb_key(MAC_A)).cloned())
            .is_some_and(|hold| hold.tab == Some(tabs[1].tab_id()))
    });
}

/// Z8: the held card shows the holder's last picture, and a newer one
/// replaces it as the holder writes it (every ten seconds, and at once
/// when it lets the board go).
#[test]
fn z8_a_held_card_follows_the_holders_picture() {
    let desk = Desk::new(&[("dev000000holdz8aa", MAC_A)]);
    let (mut a, mut b) = holder_and_watcher(&desk);
    a.run_a_project(MAC_A, &mut b);
    a.feed_a_frame(MAC_A);
    let uid = a.registry_uid(MAC_A);
    let first = a.sidecar_captured_at(&uid).expect("written at once");

    b.library_changed();
    assert_close(b.frame_captured_at(MAC_A), first, "B shows A's picture");

    // Inside the ten-second window A pulls on and writes nothing; once it
    // passes, the newest is written, and B's picture follows it.
    a.pull_a_newer_frame(MAC_A);
    assert_close(a.sidecar_captured_at(&uid), first, "inside the window");
    desk.clock
        .set(desk.clock.get() + crate::DEVICE_FRAME_SNAPSHOT_INTERVAL_SECS);
    a.pull_a_newer_frame(MAC_A);
    let second = a.sidecar_captured_at(&uid).expect("written");
    assert!(second > first, "{second} vs {first}");
    b.library_changed();
    assert_close(b.frame_captured_at(MAC_A), second, "a newer picture replaces it");

    // A pulls once more inside the window: not written. Letting the board
    // go writes it at once — newer than the last ten-second write — and
    // B picks it up before its own port opens.
    a.pull_a_newer_frame(MAC_A);
    let last = a.frame_captured_at(MAC_A).expect("A's newest");
    assert!(last > second);
    assert_close(a.sidecar_captured_at(&uid), second, "not yet");
    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a], "A's last picture to be written", |tabs| {
        tabs[0]
            .sidecar_captured_at(&uid)
            .is_some_and(|at| (at - last).abs() < 1e-6)
    });
    b.hear_notes();
    b.library_changed();
    assert_close(b.frame_captured_at(MAC_A), last, "the holder's last picture");
}

/// Z10: the holder crashes. B's take-over goes and the plain Connect comes
/// back; pressing it opens the board, which B had never opened.
#[test]
fn z10_after_a_crash_connect_is_back_and_opens_the_board() {
    let desk = Desk::new(&[("dev000000holdz10a", MAC_A)]);
    let (a, mut b) = holder_and_watcher(&desk);
    b.bench.offered(b.verb(MAC_A, "take-over"));
    b.bench.not_offered(b.verb(MAC_A, "connect"));

    desk.crash(a);
    run_until(&mut [&mut b], "the fact to clear", |tabs| {
        tabs[0].fact(MAC_A).is_none()
    });
    b.bench.not_offered(b.verb(MAC_A, "take-over"));
    assert_eq!(desk.attempts(0, "B"), 0, "nothing opened on its own");

    b.bench
        .press(b.verb(MAC_A, "connect"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut b], "B to have the board", |tabs| {
        tabs[0]
            .card(MAC_A)
            .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
    assert_eq!(desk.attempts(0, "B"), 1, "opened once, on the press");
}

// ---------------------------------------------------------------------
// The desk: boards shared by tabs
// ---------------------------------------------------------------------

/// One physical board on the desk, shared by every tab's transport. Its
/// port opens for one tab at a time — the OS refuses a second `open()`
/// until the first closes — and every open attempt is counted per tab.
#[derive(Clone)]
struct SharedUsbBoard {
    device: FakeEsp32Device,
    state: Rc<RefCell<SharedBoardState>>,
}

#[derive(Default)]
struct SharedBoardState {
    /// The tab whose open is live.
    holder: Option<&'static str>,
    /// Every open each tab attempted, refused ones included.
    attempts: BTreeMap<&'static str, usize>,
}

/// One tab's port onto a [`SharedUsbBoard`]: the fake's link exists only
/// while this tab has the port open (so the board's bytes always go to the
/// tab that holds it), and a refused open is the OS's `NetworkError`.
struct SharedPortLink {
    info: LinkInfo,
    board: SharedUsbBoard,
    tab: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    inner: Option<lpa_link::device_link::fake::FakeDeviceLink>,
    queued: VecDeque<LinkEvent>,
}

impl Link for SharedPortLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        match command {
            LinkCommand::Open { baud } => {
                let refused = {
                    let mut state = self.board.state.borrow_mut();
                    *state.attempts.entry(self.tab).or_default() += 1;
                    match state.holder {
                        Some(holder) if holder != self.tab => true,
                        _ => {
                            state.holder = Some(self.tab);
                            false
                        }
                    }
                };
                if refused {
                    self.log.borrow_mut().push(format!("refused:{}", self.tab));
                    self.queued.push_back(LinkEvent::Error(
                        "NetworkError: Failed to open serial port.".to_string(),
                    ));
                    return;
                }
                self.log.borrow_mut().push(format!("open:{}", self.tab));
                let mut link = bench_link(self.info.clone(), &self.board.device);
                link.submit(LinkCommand::Open { baud });
                self.inner = Some(link);
            }
            LinkCommand::Close => {
                if let Some(inner) = self.inner.as_mut() {
                    inner.submit(LinkCommand::Close);
                }
                let mut state = self.board.state.borrow_mut();
                if state.holder == Some(self.tab) {
                    state.holder = None;
                    self.log.borrow_mut().push(format!("close:{}", self.tab));
                }
            }
            other => {
                if let Some(inner) = self.inner.as_mut() {
                    inner.submit(other);
                }
            }
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        self.queued
            .pop_front()
            .or_else(|| self.inner.as_mut()?.poll_event())
    }
}

/// One tab's transport over the desk: the ports its browser has granted.
struct DeskTransport {
    tab: &'static str,
    ports: Vec<(String, SharedUsbBoard)>,
    log: Rc<RefCell<Vec<String>>>,
    push_plan: Rc<Cell<PushPlan>>,
}

impl DeskTransport {
    fn board_at(&self, info: &LinkInfo) -> &SharedUsbBoard {
        &self
            .ports
            .iter()
            .find(|(endpoint, _)| *endpoint == info.endpoint.0)
            .expect("a port of this tab")
            .1
    }

    /// The bench's scripted effects, over the board at `info`.
    fn scripted(&self, info: &LinkInfo) -> ScriptedTransport {
        ScriptedTransport {
            device: self.board_at(info).device.clone(),
            endpoint: info.endpoint.0.clone(),
            granted: Rc::new(Cell::new(true)),
            chooser_grants: Rc::new(Cell::new(false)),
            revoked: Rc::new(RefCell::new(Vec::new())),
            flash_plan: Rc::new(Cell::new(FlashPlan::default())),
            manifest_writes: Rc::new(RefCell::new(Vec::new())),
            push_plan: Rc::clone(&self.push_plan),
            remove_plan: Rc::new(Cell::new(RemovePlan::default())),
            pushed: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl DeviceTransport for DeskTransport {
    fn label(&self) -> &'static str {
        "the desk"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let granted = self
            .ports
            .iter()
            .map(|(endpoint, board)| {
                let info = fake_link_info(endpoint);
                GrantedLink {
                    link: Box::new(SharedPortLink {
                        info: info.clone(),
                        board: board.clone(),
                        tab: self.tab,
                        log: Rc::clone(&self.log),
                        inner: None,
                        queued: VecDeque::new(),
                    }),
                    info,
                }
            })
            .collect();
        Box::pin(core::future::ready(Ok(granted)))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        Box::pin(core::future::ready(Ok(None)))
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        self.log
            .borrow_mut()
            .push(format!("revoked:{}:{}", self.tab, info.endpoint.0));
        Box::pin(core::future::ready(Ok(())))
    }

    fn lens_client_io(
        &self,
        info: LinkInfo,
        tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        Ok(Box::new(
            FakeDeviceIo::new(&self.board_at(&info).device).with_tap(tap),
        ))
    }

    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        self.scripted(&info).run_effect(info, call, progress)
    }
}

/// The desk: the boards, the hold bus every tab's edge is on, one store
/// (one browser's library), one clock and one wire.
struct Desk {
    bus: MemoryBoardHoldBus,
    boards: Vec<SharedUsbBoard>,
    clock: Rc<Cell<f64>>,
    store: LibraryStore,
    wire: BenchWire,
    /// Every open, refusal, close and revoke on the desk, in order.
    log: Rc<RefCell<Vec<String>>>,
}

impl Desk {
    /// A desk of LightPlayer boards, each `(uid, mac)`.
    fn new(boards: &[(&str, &str)]) -> Self {
        let clock = Rc::new(Cell::new(1_000.0));
        Self {
            bus: MemoryBoardHoldBus::new(),
            boards: boards
                .iter()
                .map(|(uid, mac)| SharedUsbBoard {
                    device: desk_board(uid, mac),
                    state: Rc::default(),
                })
                .collect(),
            store: memory_store(Rc::clone(&clock)),
            clock,
            wire: BenchWire::new(),
            log: Rc::default(),
        }
    }

    /// A Studio tab whose browser has granted the ports of `boards`, with a
    /// hold edge on the desk's bus.
    fn tab(&self, name: &'static str, boards: &[usize]) -> Tab {
        let hold = self.bus.tab();
        let mut tab = self.tab_without_edge(name, boards);
        tab.bench
            .controller
            .set_board_hold_edge(Rc::new(hold.clone()));
        tab.hold = Some(hold);
        tab
    }

    /// A tab with no hold edge (a browser without Web Locks).
    fn tab_without_edge(&self, name: &'static str, boards: &[usize]) -> Tab {
        let (mut bench, tasks) = DeviceBench::build_on(
            &self.boards[0].device,
            "unused-desk-port",
            false,
            false,
            Rc::clone(&self.clock),
            memory_store_sharing(&self.store),
            self.wire.clone(),
        );
        let push_plan = Rc::new(Cell::new(PushPlan::default()));
        bench
            .controller
            .set_device_transport(Rc::new(DeskTransport {
                tab: name,
                ports: boards
                    .iter()
                    .map(|index| {
                        (
                            format!("{name}-usb-{index}").to_ascii_lowercase(),
                            self.boards[*index].clone(),
                        )
                    })
                    .collect(),
                log: Rc::clone(&self.log),
                push_plan: Rc::clone(&push_plan),
            }));
        Tab {
            name,
            bench,
            tasks,
            hold: None,
            push_plan,
        }
    }

    /// `tab` crashes: its locks vanish with no word (the bus), and the
    /// browser closes the ports it had open (the desk).
    fn crash(&self, tab: Tab) {
        self.bus.kill(&tab.tab_id());
        for board in &self.boards {
            let mut state = board.state.borrow_mut();
            if state.holder == Some(tab.name) {
                state.holder = None;
                self.log.borrow_mut().push(format!("close:{}", tab.name));
            }
        }
        drop(tab);
    }

    /// How many opens `tab` attempted on board `index`.
    fn attempts(&self, index: usize, tab: &str) -> usize {
        self.boards[index]
            .state
            .borrow()
            .attempts
            .get(tab)
            .copied()
            .unwrap_or(0)
    }

    /// The desk's log entries matching `prefixes`, in order.
    fn log_of(&self, prefixes: &[&str]) -> Vec<String> {
        self.log
            .borrow()
            .iter()
            .filter(|line| prefixes.iter().any(|prefix| line.starts_with(prefix)))
            .cloned()
            .collect()
    }
}

/// One Studio tab on the desk.
struct Tab {
    name: &'static str,
    bench: DeviceBench,
    tasks: TaskPool,
    hold: Option<MemoryBoardHold>,
    push_plan: Rc<Cell<PushPlan>>,
}

impl Tab {
    /// One turn: the notes other tabs said reach this tab's queue first
    /// (as the actor applies them before its device folds), then the bench
    /// runs its turn.
    fn step(&mut self) {
        self.hear_notes();
        self.bench.step(&self.tasks);
    }

    /// Hand this tab every note other tabs said since its last turn.
    fn hear_notes(&mut self) {
        let Some(hold) = self.hold.as_ref() else {
            return;
        };
        for (from, note) in hold.take_inbox() {
            self.bench.controller.on_hold_note(from, note);
        }
    }

    /// Another tab wrote to the library (the `lp-library` broadcast): the
    /// gallery and the remembered boards re-hydrate, pictures included.
    fn library_changed(&mut self) {
        self.bench.controller.request_library_refresh();
        self.bench.settle_library();
    }

    fn tab_id(&self) -> TabId {
        self.hold.as_ref().expect("a tab with an edge").tab_id()
    }

    /// Whether this tab holds the board with `mac` over USB.
    fn holds(&self, mac: &str) -> bool {
        self.bench
            .controller
            .board_hold_book()
            .and_then(|book| book.holds(&usb_key(mac)))
            .is_some()
    }

    /// This tab's card for the board with `mac`.
    fn card(&self, mac: &str) -> Option<lpa_devices::view::DeviceView> {
        card_for_mac(&self.bench, mac)
    }
}

/// A holder (A) and a tab that loads beside it (B), B already wearing the
/// fact on A's board.
fn holder_and_watcher(desk: &Desk) -> (Tab, Tab) {
    let mut a = desk.tab("A", &[0]);
    run_until(&mut [&mut a], "A to hold the board", |tabs| tabs[0].holds(MAC_A));
    let mut b = desk.tab("B", &[0]);
    b.library_changed();
    // Primed at no level (read as Open), then A's answer to B's `Who`.
    run_until(&mut [&mut a, &mut b], "B to see the board held", |tabs| {
        tabs[1].fact(MAC_A) == Some(held(HoldLevel::Watching, false))
    });
    (a, b)
}

impl Tab {
    /// The fact on this tab's card for the board with `mac`.
    fn fact(&self, mac: &str) -> Option<crate::HeldElsewhere> {
        self.card(mac)?.held_elsewhere
    }

    /// The roster id of the board with `mac` here.
    fn device_id(&self, mac: &str) -> crate::DeviceId {
        self.card(mac).expect("a card for the board").id
    }

    /// The registry row this browser keeps for the board with `mac`.
    fn registry_uid(&self, mac: &str) -> String {
        let id = self.device_id(mac);
        self.bench
            .controller
            .devices_for_test()
            .key_for(id)
            .expect("a registry row")
            .to_string()
    }

    /// Step this tab alone until its board reports nothing loaded.
    fn wait_until_empty(&mut self, mac: &str) {
        run_until(&mut [self], "the board to report nothing loaded", |tabs| {
            tabs[0].card(mac).is_some_and(|card| {
                card.loaded_project == lpa_devices::view::LoadedProject::Empty
            })
        });
    }

    /// Push the bundled example and wait for the board to run it, stepping
    /// `other` alongside.
    fn run_a_project(&mut self, mac: &str, other: &mut Tab) {
        self.wait_until_empty(mac);
        let board = self.device_id(mac);
        self.bench.push_gesture(board, bundled_example());
        run_until(&mut [self, other], "the board to run the example", |tabs| {
            runs_a_project(&tabs[0], mac)
        });
    }

    /// [`Self::run_a_project`] with no other tab running.
    fn run_a_project_alone(&mut self, mac: &str) {
        self.wait_until_empty(mac);
        let board = self.device_id(mac);
        self.bench.push_gesture(board, bundled_example());
        run_until(&mut [self], "the board to run the example", |tabs| {
            runs_a_project(&tabs[0], mac)
        });
    }

    /// Want the board's picture and pull until one lands (written to its
    /// sidecar at once, the first time).
    fn feed_a_frame(&mut self, mac: &str) {
        let board = self.device_id(mac);
        self.bench.controller.set_device_feed_wanted(board, true);
        let mut pulls = 0;
        while self.frame_captured_at(mac).is_none() {
            self.hear_notes();
            feed_tick(&mut self.bench, &self.tasks, 5.0);
            pulls += 1;
            assert!(pulls <= 3, "no frame after three pulls");
            for _ in 0..40 {
                self.step();
            }
        }
    }

    /// When the newest picture of the board with `mac` was captured.
    fn frame_captured_at(&self, mac: &str) -> Option<f64> {
        let board = self.card(mac)?.id;
        let now = self.bench.clock.get();
        let feed = self.bench.controller.device_feeds().get(board)?;
        feed.frame()?;
        feed.frame_age_secs(now).map(|age| now - age)
    }

    /// The capture stamp of the picture in the board's sidecar.
    fn sidecar_captured_at(&self, uid: &str) -> Option<f64> {
        let path = crate::app::devices::device_frame_snapshot::snapshot_path(uid);
        let bytes = self
            .bench
            .store
            .fs_handle()
            .borrow()
            .read_file(path.as_path())
            .ok()?;
        crate::app::devices::device_frame_snapshot::decode(&bytes).map(|(_, at)| at)
    }
}

/// Whether the tab's board with `mac` is idle and running a project.
fn runs_a_project(tab: &Tab, mac: &str) -> bool {
    tab.card(mac).is_some_and(|card| {
        card.activity.is_none()
            && matches!(
                card.loaded_project,
                lpa_devices::view::LoadedProject::Running { .. }
            )
    })
}

impl Tab {
    /// Where this tab offers `verb` on the board with `mac`
    /// (`devices/<board ref>/<verb>`): the board ref is core's to work out.
    fn verb(&self, mac: &str, verb: &str) -> crate::OfferPath {
        let id = self.device_id(mac);
        self.bench
            .controller
            .view()
            .offers
            .device_prefix(id)
            .unwrap_or_else(|| panic!("{id:?} has no place in the offer tree"))
            .clone()
            .child(verb)
    }

    /// What this tab's card says about its take-over of the board.
    fn take_over_words(&self, mac: &str) -> Option<String> {
        let id = self.device_id(mac);
        self.bench
            .controller
            .device_roster_view()
            .take_overs
            .get(&id)
            .map(|take_over| take_over.words.clone())
    }

    /// Whether this tab's take-over of the board ended without it.
    fn take_over_failed(&self, mac: &str) -> Option<bool> {
        let id = self.device_id(mac);
        self.bench
            .controller
            .device_roster_view()
            .take_overs
            .get(&id)
            .map(|take_over| take_over.failed)
    }

    /// Pull until the board's picture is newer than it is now.
    fn pull_a_newer_frame(&mut self, mac: &str) {
        let before = self.frame_captured_at(mac).expect("a picture to beat");
        let mut pulls = 0;
        while self.frame_captured_at(mac).is_none_or(|at| at <= before) {
            for _ in 0..40 {
                self.step();
            }
            self.hear_notes();
            feed_tick(&mut self.bench, &self.tasks, 5.0);
            pulls += 1;
            assert!(pulls <= 10, "the picture never moved past {before}");
        }
    }
}

/// `actual` is `expected`, as capture stamps compare (their f64 sums can
/// differ in the last bit).
#[track_caller]
fn assert_close(actual: Option<f64>, expected: f64, what: &str) {
    let actual = actual.unwrap_or_else(|| panic!("{what}: nothing"));
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} vs {expected}"
    );
}

/// One turn for every tab, in order.
fn step(tabs: &mut [&mut Tab]) {
    for tab in tabs.iter_mut() {
        tab.step();
    }
}

/// Step every tab until `ready`, with a wall-clock hang guard.
fn run_until(tabs: &mut [&mut Tab], what: &str, ready: impl Fn(&[&mut Tab]) -> bool) {
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT * 4;
    loop {
        step(tabs);
        if ready(tabs) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}; {}",
            tabs.iter()
                .map(|tab| format!("{}: {:?}", tab.name, tab.bench.view()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// A LightPlayer on the desk, stamped and heartbeating (so its loaded
/// project is reported), with its own MAC.
fn desk_board(uid: &str, mac: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Desk board"))
            .with_base_mac(mac)
            .with_heartbeat_interval(Duration::from_millis(20)),
    )))
}

const MAC_A: &str = "60:55:f9:0a:0b:0c";
const MAC_B: &str = "60:55:f9:0a:0b:0d";

const C6: UsbPair = UsbPair {
    vendor: 0x303a,
    product: 0x1001,
};

fn usb_key(mac: &str) -> HoldKey {
    HoldKey::usb(crate::BoardKey::parse(mac).expect("a mac"), C6)
}

fn held(level: HoldLevel, taken_from_here: bool) -> crate::HeldElsewhere {
    crate::HeldElsewhere {
        via: crate::HoldVia::Usb,
        level,
        taken_from_here,
    }
}

fn board() -> HoldKey {
    HoldKey::usb(
        crate::BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac"),
        UsbPair {
            vendor: 0x303a,
            product: 0x1001,
        },
    )
}

