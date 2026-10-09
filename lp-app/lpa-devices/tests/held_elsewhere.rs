//! "Another tab holds this board": the board's fact (`Event::BoardHeld`) and
//! the link's (`Event::LinkHeld`), scripted through the crate's public
//! surface like every other scenario.
//!
//! The claims these pin:
//!
//! - the fact rides the BOARD, addressed by MAC, and the order of the claim,
//!   the record load, a promotion and a hello does not matter;
//! - a held port settles its identify at once, never retries, and is never
//!   opened by the model;
//! - a held link the claims name merges onto the board's own card, which is
//!   then the ordinary `Attached` card that Connect opens;
//! - the board gets the last word: a hello that contradicts the presumed MAC
//!   re-routes the link instead of wedging the card.

use lpa_devices::activity::identify::HELD_BY_ANOTHER_TAB;
use lpa_devices::replay::{Fixture, Replay, Step};
use lpa_devices::{
    Command, DeviceId, DeviceRecord, DeviceStatus, DeviceUid, Escape, Event, HeldElsewhere,
    HoldLevel, HoldVia, IdentityChain, Input, LinkCommand, LinkId, MacAddress, Millis, Roster,
    RosterConfig,
};

const BOARD_MAC: &str = "a0:f2:62:87:b4:8c";
const OTHER_MAC: &str = "a0:f2:62:87:b4:9d";

#[test]
fn a_record_backed_offline_board_wears_the_fact_until_it_is_cleared() {
    let mut replay =
        Replay::with_roster(roster_with_records(&[board_record(4, BOARD_MAC, "dev_a")]));

    replay.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Watching))),
    );

    let card = &replay.view().devices[0];
    assert_eq!(card.status, DeviceStatus::Offline, "no link in this tab");
    assert_eq!(card.held_elsewhere, Some(held(HoldLevel::Watching)));

    // A level change is the same fact, re-said.
    replay.step(
        Millis(10),
        Step::board_held(
            BOARD_MAC,
            Some(held(HoldLevel::Busy("Updating · 42%".into()))),
        ),
    );
    assert_eq!(
        replay.view().devices[0].held_elsewhere,
        Some(held(HoldLevel::Busy("Updating · 42%".into())))
    );

    replay.step(Millis(20), Step::board_held(BOARD_MAC, None));
    assert_eq!(replay.view().devices[0].held_elsewhere, None);
}

#[test]
fn the_fact_lands_whether_the_claim_or_the_record_arrives_first() {
    // The record first, then the claim.
    let mut record_first =
        Replay::with_roster(roster_with_records(&[board_record(4, BOARD_MAC, "dev_a")]));
    record_first.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Open))),
    );

    // The claim first (spelled another way), then the record loads.
    let mut claim_first = Replay::new(RosterConfig::default());
    claim_first.step(
        Millis(0),
        Step::board_held("A0-F2-62-87-B4-8C", Some(held(HoldLevel::Open))),
    );
    claim_first
        .roster_mut()
        .load_records(vec![board_record(4, BOARD_MAC, "dev_a")]);

    for (order, replay) in [
        ("record first", &record_first),
        ("claim first", &claim_first),
    ] {
        let view = replay.view();
        assert_eq!(view.devices.len(), 1, "{order}");
        assert_eq!(
            view.devices[0].held_elsewhere,
            Some(held(HoldLevel::Open)),
            "{order}"
        );
    }
}

#[test]
fn a_board_that_names_its_mac_after_the_claim_wears_the_fact() {
    // No record: the board is new to this tab, and says who it is only in
    // its hello, long after the claim arrived.
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Watching))),
    );
    replay.step(Millis(10), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(30), Step::hello(1).uid("dev_a").mac(BOARD_MAC));

    let view = replay.view();
    assert!(view.pending.is_empty());
    assert_eq!(view.devices.len(), 1);
    assert_eq!(
        view.devices[0].held_elsewhere,
        Some(held(HoldLevel::Watching))
    );
}

#[test]
fn link_attach_detach_and_loss_never_clear_the_boards_fact() {
    let mut replay =
        Replay::with_roster(roster_with_records(&[board_record(4, BOARD_MAC, "dev_a")]));
    replay.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Watching))),
    );
    replay.step(Millis(10), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::link_held(1, Some(BOARD_MAC)));
    replay.step(Millis(30), Step::detach(1));

    let card = &replay.view().devices[0];
    assert_eq!(card.status, DeviceStatus::Offline);
    assert_eq!(card.held_elsewhere, Some(held(HoldLevel::Watching)));
}

/// The merge-by-claim path: a port the other tab's claims account for is
/// attached (never opened), and because the claims name its board it lands
/// on that board's own record-backed card — `Attached`, Connect offered —
/// with no "new device found" beside it.
#[test]
fn a_held_link_the_claims_name_merges_onto_the_boards_own_card() {
    let config = RosterConfig::default();
    let mut replay =
        Replay::with_roster(roster_with_records(&[board_record(4, BOARD_MAC, "dev_a")]));
    replay.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Watching))),
    );
    replay.step(Millis(10), Step::attach(1, "usb-1"));
    assert_eq!(replay.view().pending.len(), 1, "an unknown port identifies");

    let commands = replay.step(Millis(20), Step::link_held(1, Some(BOARD_MAC)));

    let view = replay.view();
    assert!(view.pending.is_empty(), "no new-device card: {view:#?}");
    assert_eq!(view.devices.len(), 1, "one board, one card");
    let card = &view.devices[0];
    assert_eq!(card.id, DeviceId(4), "the board's own record-backed card");
    assert_eq!(card.status, DeviceStatus::Attached, "port there, closed");
    assert!(card.activity.is_none(), "identify settled at once");
    assert!(
        card.last_outcome
            .as_ref()
            .is_some_and(|outcome| !outcome.ok && outcome.summary == HELD_BY_ANOTHER_TAB),
        "{card:?}"
    );
    assert_eq!(card.held_elsewhere, Some(held(HoldLevel::Watching)));
    assert!(
        card.escapes.contains(&Escape::Disconnect),
        "{:?}",
        card.escapes
    );
    assert!(
        !opens(&commands, LinkId(1)),
        "nothing opens a held port: {commands:?}"
    );

    // No retry and no deadline wait: well past every identify window, the
    // model has asked for nothing more on this link.
    let before = replay.commands().len();
    replay.advance_to(Millis(
        20 + (config.identify_deadline_ms + 1_000) * u64::from(config.identify_auto_retries + 2),
    ));
    let later: Vec<_> = replay.commands()[before..].to_vec();
    assert!(
        !later.iter().any(
            |(_, command)| matches!(command, Command::Link { link, .. } if *link == LinkId(1))
        ),
        "no retry: {later:?}"
    );
    assert_eq!(replay.view().devices[0].status, DeviceStatus::Attached);

    // The holder lets go: the fact clears and NOTHING opens the port. The
    // card is the ordinary Attached card; Connect is how it opens.
    let before = replay.commands().len();
    replay.step(Millis(60_000), Step::board_held(BOARD_MAC, None));
    assert!(
        !replay.commands()[before..]
            .iter()
            .any(|(_, command)| opens(std::slice::from_ref(command), LinkId(1))),
        "a freed board is not opened by the model"
    );
    let card = &replay.view().devices[0];
    assert_eq!(card.held_elsewhere, None);
    assert_eq!(card.status, DeviceStatus::Attached);

    // Connect opens the link, and the board's hello merges as usual.
    let commands = replay.step(Millis(60_010), Step::Connect { device: 4 });
    assert!(opens(&commands, LinkId(1)), "{commands:?}");
    replay.step(Millis(60_020), Step::opened(1));
    replay.step(Millis(60_030), Step::hello(1).uid("dev_a").mac(BOARD_MAC));

    let view = replay.view();
    assert!(view.pending.is_empty());
    assert_eq!(view.devices.len(), 1);
    assert_eq!(view.devices[0].id, DeviceId(4));
    assert_eq!(view.devices[0].status, DeviceStatus::Ready);
}

#[test]
fn a_held_link_the_claims_cannot_name_stays_pending_and_says_so() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));

    let commands = replay.step(Millis(10), Step::link_held(1, None));

    let view = replay.view();
    assert_eq!(view.pending.len(), 1);
    assert!(view.devices.is_empty());
    let pending = &view.pending[0];
    assert!(pending.held_by_tab);
    assert_eq!(
        pending.state_label,
        "New device found — open in another Studio tab"
    );
    assert_eq!(pending.escapes, vec![Escape::Forget]);
    assert!(
        !replay.roster().pending()[0].is_identifying(),
        "settled at once, not at the deadline"
    );
    assert!(!opens(&commands, LinkId(1)), "{commands:?}");

    let before = replay.commands().len();
    replay.advance_to(Millis(
        10 + (config.identify_deadline_ms + 1_000) * u64::from(config.identify_auto_retries + 2),
    ));
    assert!(
        !replay.commands()[before..].iter().any(
            |(_, command)| matches!(command, Command::Link { link, .. } if *link == LinkId(1))
        ),
        "no retry"
    );
    assert_eq!(
        replay.view().pending[0].state_label,
        "New device found — open in another Studio tab"
    );
}

/// The hold that kept a pending port shut ends (`LinkFreed`): the mark and
/// its words go, and nothing else moves — the port is not opened, its
/// identify does not run again, and it reads as the shut port it is.
#[test]
fn a_freed_port_loses_its_mark_and_stays_shut() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(10), Step::link_held(1, None));
    assert!(replay.view().pending[0].held_by_tab);

    let before = replay.commands().len();
    let commands = replay.step(Millis(20), Step::link_freed(1));

    let view = replay.view();
    let pending = &view.pending[0];
    assert!(!pending.held_by_tab);
    assert_ne!(
        pending.state_label,
        "New device found — open in another Studio tab"
    );
    assert!(!opens(&commands, LinkId(1)), "{commands:?}");
    assert!(!replay.roster().pending()[0].is_identifying());
    replay.advance_to(Millis(
        20 + (config.identify_deadline_ms + 1_000) * u64::from(config.identify_auto_retries + 2),
    ));
    assert!(
        !replay.commands()[before..].iter().any(
            |(_, command)| matches!(command, Command::Link { link, .. } if *link == LinkId(1))
        ),
        "nothing opens it"
    );
}

/// A refused open the claims explain later (the OS refused the port before
/// any other tab said it held it): the existing words stand until the
/// `LinkHeld` arrives, then the other tab's own word replaces them.
#[test]
fn a_refused_open_read_against_a_later_claim_says_another_tab() {
    let config = RosterConfig::default();
    let mut replay = Replay::new(config);
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(
        Millis(10),
        Step::Error {
            link: 1,
            message: "NetworkError: Failed to open serial port".to_string(),
        },
    );
    replay.advance_to(Millis(
        (config.identify_deadline_ms + 1_000) * u64::from(config.identify_auto_retries + 2),
    ));
    assert_eq!(
        replay.view().pending[0].state_label,
        "New device found — in use by another app or another Studio tab"
    );
    assert!(!replay.view().pending[0].held_by_tab);

    replay.step(Millis(60_000), Step::link_held(1, None));

    assert_eq!(
        replay.view().pending[0].state_label,
        "New device found — open in another Studio tab"
    );
}

/// The presumption was wrong (two boards of one kind, say): the port that
/// opens says it is ANOTHER board. The link leaves the card the claim put it
/// on, and the board's own word puts it where it belongs. Nothing wedges.
#[test]
fn a_presumed_mac_the_board_contradicts_reroutes_the_link() {
    let mut replay = Replay::with_roster(roster_with_records(&[
        board_record(4, BOARD_MAC, "dev_a"),
        board_record(5, OTHER_MAC, "dev_b"),
    ]));
    replay.step(
        Millis(0),
        Step::board_held(BOARD_MAC, Some(held(HoldLevel::Watching))),
    );
    replay.step(Millis(10), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::link_held(1, Some(BOARD_MAC)));
    let card_a = card(&replay, 4);
    assert_eq!(card_a.status, DeviceStatus::Attached, "the claim's guess");

    replay.step(Millis(60_000), Step::board_held(BOARD_MAC, None));
    let commands = replay.step(Millis(60_010), Step::Connect { device: 4 });
    assert!(opens(&commands, LinkId(1)));
    replay.step(Millis(60_020), Step::opened(1));
    replay.step(Millis(60_030), Step::hello(1).uid("dev_b").mac(OTHER_MAC));

    let view = replay.view();
    assert!(view.pending.is_empty(), "{view:#?}");
    assert_eq!(view.devices.len(), 2, "still two boards, never merged");
    let card_a = card(&replay, 4);
    let card_b = card(&replay, 5);
    assert_eq!(card_a.status, DeviceStatus::Offline, "the guess let go");
    assert_eq!(card_b.status, DeviceStatus::Ready, "the board's own word");
    assert_eq!(
        replay
            .roster()
            .device(DeviceId(5))
            .and_then(|device| device.link()),
        Some(LinkId(1))
    );
    assert_eq!(
        replay
            .roster()
            .device(DeviceId(4))
            .and_then(|device| device.identity.mac.clone()),
        Some(MacAddress(BOARD_MAC.to_string())),
        "board A keeps its own identity"
    );
    assert!(
        replay
            .journal_notes()
            .iter()
            .any(|note| note.contains("LinkRerouted")),
        "the correction is journaled"
    );

    // Not wedged: the right board's card answers verbs, and board A's card
    // is an ordinary offline card that can be reached again.
    assert!(card_b.escapes.contains(&Escape::Disconnect));
    assert!(card_a.escapes.contains(&Escape::Reconnect));
}

#[test]
fn a_presumed_mac_the_board_confirms_stays_where_it_is() {
    let mut replay =
        Replay::with_roster(roster_with_records(&[board_record(4, BOARD_MAC, "dev_a")]));
    replay.step(Millis(10), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::link_held(1, Some(BOARD_MAC)));
    replay.step(Millis(60_010), Step::Connect { device: 4 });
    replay.step(Millis(60_020), Step::opened(1));
    replay.step(Millis(60_030), Step::hello(1).uid("dev_a").mac(BOARD_MAC));
    // A later heartbeat naming the same board changes nothing either.
    replay.step(Millis(60_040), Step::heartbeat(1).mac(BOARD_MAC));

    let view = replay.view();
    assert_eq!(view.devices.len(), 1);
    assert!(view.pending.is_empty());
    assert_eq!(view.devices[0].status, DeviceStatus::Ready);
    assert!(
        !replay
            .journal_notes()
            .iter()
            .any(|note| note.contains("LinkRerouted"))
    );
}

/// The journal records every input verbatim and fixtures are JSON, so the
/// hold events round-trip like every other input; a `LinkHeld` written
/// without its `mac` reads as naming no board.
#[test]
fn the_hold_events_round_trip_through_json() {
    let inputs = [
        Input::Event(Event::BoardHeld {
            mac: MacAddress(BOARD_MAC.to_string()),
            held: Some(HeldElsewhere {
                via: HoldVia::Network,
                level: HoldLevel::Busy("Flashing · 10%".to_string()),
                taken_from_here: true,
            }),
        }),
        Input::Event(Event::BoardHeld {
            mac: MacAddress(BOARD_MAC.to_string()),
            held: None,
        }),
        Input::Event(Event::LinkHeld {
            link: LinkId(1),
            mac: Some(MacAddress(BOARD_MAC.to_string())),
        }),
        Input::Event(Event::LinkFreed { link: LinkId(1) }),
    ];
    for input in inputs {
        let json = serde_json::to_string(&input).expect("serialize");
        let back: Input = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, input);
    }

    let bare: Input = serde_json::from_str(r#"{"Event":{"LinkHeld":{"link":1}}}"#)
        .expect("a LinkHeld with no mac");
    assert_eq!(
        bare,
        Input::Event(Event::LinkHeld {
            link: LinkId(1),
            mac: None
        })
    );

    // And a fixture step that scripts them parses back the same.
    for step in [Step::link_held(1, Some(BOARD_MAC)), Step::link_freed(1)] {
        let json = serde_json::to_string(&step).expect("step serializes");
        let back: Step = serde_json::from_str(&json).expect("step parses");
        assert_eq!(serde_json::to_string(&back).expect("re-serializes"), json);
    }
}

/// The same story as JSON, the shape a triaged bug's fixture takes: the
/// hold steps have a written form, and it plays.
#[test]
fn the_held_by_another_tab_fixture_plays() {
    let fixture = Fixture::from_json(include_str!("../fixtures/held-by-another-tab.json"))
        .expect("fixture parses");
    let mut replay = Replay::new(RosterConfig::default());
    if let Err(failure) = replay.run(&fixture) {
        panic!("{failure}\nview: {:#?}", replay.view());
    }
}

fn held(level: HoldLevel) -> HeldElsewhere {
    HeldElsewhere {
        via: HoldVia::Usb,
        level,
        taken_from_here: false,
    }
}

fn board_record(id: u64, mac: &str, uid: &str) -> DeviceRecord {
    DeviceRecord::new(
        DeviceId(id),
        IdentityChain {
            mac: Some(MacAddress(mac.to_string())),
            uid: Some(DeviceUid(uid.to_string())),
            ..Default::default()
        },
    )
}

fn roster_with_records(records: &[DeviceRecord]) -> Roster {
    let mut roster = Roster::new(RosterConfig::default());
    roster.load_records(records.to_vec());
    roster
}

fn card(replay: &Replay, id: u64) -> lpa_devices::DeviceView {
    replay
        .view()
        .devices
        .into_iter()
        .find(|card| card.id == DeviceId(id))
        .unwrap_or_else(|| panic!("no card {id}"))
}

fn opens(commands: &[Command], link: LinkId) -> bool {
    commands.iter().any(|command| {
        matches!(
            command,
            Command::Link {
                link: addressed,
                command: LinkCommand::Open { .. },
            } if *addressed == link
        )
    })
}
