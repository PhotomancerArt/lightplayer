//! One tab holds a board, end to end: two Studio tabs on one in-memory hold
//! bus ([`crate::MemoryBoardHoldBus`]).
//!
//! What stands so far is the inbound door: a note one tab posts reaches the
//! other tab's controller as `StudioCommand::BoardHold`, on the actor's
//! ordered queue, and lands in that tab's hold book. The flows built on it
//! (the claim, the gate, the answer, the take-over) add their rows here.

use super::*;
use crate::app::studio::studio_actor::StudioActor;
use crate::app::studio::studio_command::StudioCommand;
use crate::{
    BoardHoldEdge, BookChange, HoldKey, HoldLevel, HoldNote, MemoryBoardHoldBus, TabId, UsbPair,
};

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

/// A port the other tab's claims account for, naming its board, lands on
/// the board's own record-backed card, which is the ordinary `Attached`
/// card: Connect is offered there, and that is how a freed board is opened.
#[test]
fn a_held_port_merged_onto_its_board_is_offered_connect() {
    use lpa_devices::replay::{Replay, Step};

    let mut roster = lpa_devices::Roster::new(lpa_devices::RosterConfig::default());
    roster.load_records(vec![lpa_devices::DeviceRecord::new(
        lpa_devices::DeviceId(4),
        lpa_devices::IdentityChain {
            mac: Some(crate::MacAddress("a0:f2:62:87:b4:8c".to_string())),
            ..Default::default()
        },
    )]);
    let mut replay = Replay::with_roster(roster);
    replay.step(
        lpa_devices::Millis(0),
        Step::board_held(
            "a0:f2:62:87:b4:8c",
            Some(crate::HeldElsewhere {
                via: crate::HoldVia::Usb,
                level: HoldLevel::Watching,
                taken_from_here: false,
            }),
        ),
    );
    replay.step(lpa_devices::Millis(10), Step::attach(1, "usb-1"));
    replay.step(
        lpa_devices::Millis(20),
        Step::link_held(1, Some("a0:f2:62:87:b4:8c")),
    );

    let view = replay.view();
    assert!(view.pending.is_empty());
    let card = &view.devices[0];
    assert_eq!(card.status, crate::DeviceStatus::Attached);
    assert!(card.held_elsewhere.is_some());

    let offers = crate::device_offers(
        card,
        &crate::DeviceOfferFacts {
            prefix: crate::OfferPath::board(&crate::BoardRef::Mac(
                crate::BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac"),
            )),
            face: crate::DeviceFace::Wire,
            autoconnect: false,
            locked: false,
            reset: crate::ResetReach::Lines,
            banked: false,
            projects: &[],
            examples: &[],
            update: crate::UpdateOfferFacts::default(),
        },
    );
    let connect = offers
        .iter()
        .find(|offer| offer.path.last() == Some("connect"))
        .expect("connect is offered on the held board's card");
    assert!(connect.is_enabled());
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
