//! One tab holds a board, over the network (P5): tabs of one browser on the
//! hold bus, and one board whose single network slot — reached on the LAN
//! or through lightplayer.app's relay — takes one client, the way a C6
//! does (`docs/adr/2026-10-07-c6-wifi-link.md`):
//!
//! - a newcomer that proves the holder's key takes the slot, and the board
//!   closes the holder (`parked_handshake.rs`); every tab of one browser
//!   presents the same keys, so on a KEYED board one tab's connect takes
//!   the slot from another;
//! - anyone else is told busy (close 1013 on the LAN, 4429 through the
//!   relay); on an OPEN board every Studio is anonymous, so a second tab is
//!   always told busy.
//!
//! Each tab's sessions behave as `browser_websocket.js`'s do: a connect
//! opens (or keeps) the session; a session the board closed is a drop, and
//! it REDIALS on its own (rule 2) when the test fires the redial timer
//! ([`NetBoard::fire_redials`]); a close by request (the link's `Close`, or
//! the source's forget) ends that. Every connect, drop, close, forget,
//! busy refusal and redial is logged per tab, which is what the rows
//! assert on.
//!
//! Rows N1–N6 of the plan's P5.

use std::collections::BTreeMap;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo};

use super::*;
use crate::app::access::account_keys::tests::account;

// ---------------------------------------------------------------------
// N1–N6
// ---------------------------------------------------------------------

/// N1: A holds a board over the LAN (an open board: B's own connect would
/// be told busy). B's first look sees the hold, its card wears the fact,
/// and Connect is `take-over` (no "Connect over Wi‑Fi" beside it). B
/// presses it: A closes its session BY REQUEST — one close, and no redial
/// when its timer fires — and B connects over Wi‑Fi at the address it
/// remembers. Then A takes it back the same way, reopening the link it let
/// go.
#[test]
fn n1_take_over_on_the_lan_closes_the_holder_by_request_and_connects() {
    let desk = Desk::new(&[("dev000000holdn1aa", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Open);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Lan);
    b.remember_wifi_address(MAC_A);
    b.step();
    b.bench.not_offered(b.verb(MAC_A, "connect-wifi"));
    let take_over = b.verb(MAC_A, "take-over");
    assert!(
        b.bench
            .offered(take_over.clone())
            .consequence()
            .is_routine(),
        "A is only watching"
    );

    b.bench.press(take_over, OfferArgs::new()).expect("press");
    run_until(&mut [&mut a, &mut b], "B to have the board", |tabs| {
        tabs[1].holds_key(&net_key(MAC_A))
            && tabs[1]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[0].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
    });
    assert!(!net.redial_due("A"));
    assert!(net.fire_redials().is_empty(), "nothing to redial");
    for _ in 0..100 {
        step(&mut [&mut a, &mut b]);
    }

    assert_eq!(
        net.log_of("A"),
        ["connect:A", "close:A"],
        "one close, by request, and no redial"
    );
    assert_eq!(net.log_of("B"), ["connect:B"], "B connected over Wi-Fi");
    assert_eq!(net.holder(), Some(("B", Road::Lan)));
    assert_eq!(desk.bus.holder_of(&net_key(MAC_A)), Some(b.tab_id()));
    assert_eq!(b.take_over_words(MAC_A), None, "done");
    assert_eq!(
        a.fact(MAC_A),
        Some(crate::HeldElsewhere {
            via: crate::HoldVia::Network,
            level: HoldLevel::Watching,
            taken_from_here: true,
        }),
        "A's card says it was taken"
    );

    // A takes it back: its own link, closed by request, reopens.
    let back = a.verb(MAC_A, "take-over");
    a.bench.press(back, OfferArgs::new()).expect("press");
    run_until(&mut [&mut a, &mut b], "A to have it back", |tabs| {
        tabs[0].holds_key(&net_key(MAC_A))
            && tabs[0]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[1].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
    });
    assert_eq!(net.log_of("B"), ["connect:B", "close:B"]);
    assert_eq!(net.log_of("A"), ["connect:A", "close:A", "open:A"]);
    assert_eq!(net.holder(), Some(("A", Road::Lan)));
}

/// N2: the same through lightplayer.app's relay. B remembers no Wi‑Fi
/// address: until someone signs in it has no road to the board, and the
/// take-over says so. Signed in, the board's ordinary connect is
/// `connect-relay` — not offered while A holds the slot; the take-over is,
/// and runs it once A has let go.
#[test]
fn n2_take_over_through_the_relay() {
    let desk = Desk::new(&[("dev000000holdn2aa", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Open);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Relay);
    assert_eq!(
        b.bench.offer_reason(b.verb(MAC_A, "take-over")),
        crate::TAKE_OVER_NO_WAY,
        "no Wi-Fi address and nobody signed in: no road from here"
    );
    b.sign_in();
    b.step();
    b.bench.not_offered(b.verb(MAC_A, "connect-relay"));

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "B to have the board", |tabs| {
        tabs[1].holds_key(&net_key(MAC_A))
            && tabs[1]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
    });
    assert!(net.fire_redials().is_empty(), "nothing to redial");

    assert_eq!(net.log_of("A"), ["connect:A", "close:A"]);
    assert_eq!(net.log_of("B"), ["connect:B"]);
    assert_eq!(net.holder(), Some(("B", Road::Relay)));
    let reached = b.bench.controller.device_roster_view();
    let id = b.device_id(MAC_A);
    assert_eq!(reached.lan_links[&id].kind, crate::UiLinkKind::Relay);
}

/// N1, a connect that fails: A lets go, but the board turns B's connect
/// away (someone else took the slot meanwhile). The take-over ends with
/// the connect's own words, and the card can be pressed again.
#[test]
fn n1_a_take_over_whose_connect_is_turned_away_says_why() {
    let desk = Desk::new(&[("dev000000holdn1bb", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Open);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Lan);
    b.remember_wifi_address(MAC_A);
    net.refuse_next_connect();

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "B's take-over to fail", |tabs| {
        tabs[1].take_over_failed(MAC_A) == Some(true)
    });

    assert_eq!(
        b.take_over_words(MAC_A),
        Some(crate::WifiConnectFailure::Busy.words()),
        "the connect's own words"
    );
    assert_eq!(net.log_of("A"), ["connect:A", "close:A"], "A let go");
    assert_eq!(net.log_of("B"), ["busy:B"]);
    assert!(!b.holds_key(&net_key(MAC_A)));
}

/// N3, the ping-pong pinned: on a keyed board B connects WITHOUT asking (as
/// `?lan=` does at page load) while A holds it. The board gives B the slot
/// and closes A — a drop, which A's session would redial in 250 ms and take
/// the slot back. A hears B's `Holds`, yields: its dropped session is told
/// to stop (one close by request), and when the redial timer fires nothing
/// redials. B keeps the board; A's card says it was taken.
#[test]
fn n3_a_tab_that_hears_another_take_its_network_board_yields_and_never_redials() {
    let desk = Desk::new(&[("dev000000holdn3aa", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Keyed);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Lan);

    net.dial_at_load("B", Road::Lan);
    b.bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    run_until(
        &mut [&mut a, &mut b],
        "B to hold it and A to yield",
        |tabs| {
            tabs[1].holds_key(&net_key(MAC_A))
                && tabs[0].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
        },
    );
    assert!(!net.redial_due("A"), "A's dropped session was stopped");
    for tab in net.fire_redials() {
        assert_ne!(tab, "A", "A redialled");
    }
    for _ in 0..100 {
        step(&mut [&mut a, &mut b]);
    }

    assert_eq!(
        net.log_of("A"),
        ["connect:A", "drop:A", "forget:A"],
        "the board dropped A; A stopped its session by request, once; no redial"
    );
    assert_eq!(net.holder(), Some(("B", Road::Lan)));
    assert!(!a.holds_key(&net_key(MAC_A)));
    assert_eq!(desk.bus.holder_of(&net_key(MAC_A)), Some(b.tab_id()));
    assert_eq!(
        b.card(MAC_A).map(|card| card.status),
        Some(DeviceStatus::Ready)
    );
}

/// N3, the other order: B's `Holds` reaches A before the board's close
/// does (the close is still on its way). A's link is still open, so A
/// closes it by request, lets its lock go and says `Gone`. B — whose claim
/// found A's lock still held — announced its hold all the same and claims
/// again until the lock is its own. The late drop then changes nothing,
/// and nothing redials.
#[test]
fn n3_when_the_holds_note_beats_the_drop_the_holder_closes_its_open_link() {
    let desk = Desk::new(&[("dev000000holdn3bb", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Keyed);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Lan);
    net.hold_drops(true);

    net.dial_at_load("B", Road::Lan);
    b.bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    run_until(&mut [&mut a, &mut b], "A to yield", |tabs| {
        tabs[0].fact(MAC_A).is_some_and(|fact| fact.taken_from_here)
            && !tabs[0].holds_key(&net_key(MAC_A))
    });
    run_until(&mut [&mut a, &mut b], "B's lock to be its own", |_| {
        desk.bus.holder_of(&net_key(MAC_A)).is_some()
    });
    net.deliver_drops();
    for tab in net.fire_redials() {
        assert_ne!(tab, "A", "A redialled");
    }
    for _ in 0..100 {
        step(&mut [&mut a, &mut b]);
    }

    assert_eq!(
        net.log_of("A"),
        ["connect:A", "drop:A", "close:A"],
        "the board dropped A (its close still on the way); A closed its open link by \
         request, once; no redial"
    );
    assert_eq!(desk.bus.holder_of(&net_key(MAC_A)), Some(b.tab_id()));
    assert!(b.holds_key(&net_key(MAC_A)));
    assert_eq!(net.holder(), Some(("B", Road::Lan)));
}

/// N4: A holds the board over the LAN and starts a push B has not heard
/// about yet. B asks: A refuses with the push's own words, and nothing
/// closes.
#[test]
fn n4_a_busy_holder_refuses_and_nothing_closes() {
    let desk = Desk::new(&[("dev000000holdn4aa", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Open);
    let (mut a, mut b) = net_holder_and_watcher(&desk, &net, Road::Lan);
    b.remember_wifi_address(MAC_A);
    let board = a.device_id(MAC_A);

    net.hang_conversations(true);
    a.wait_until_empty(MAC_A);
    a.bench.push_gesture(board, bundled_example());
    run_until(&mut [&mut a], "A to be busy", |tabs| {
        tabs[0]
            .card(MAC_A)
            .is_some_and(|card| card.activity.is_some())
    });
    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut a, &mut b], "B to hear the refusal", |tabs| {
        tabs[1]
            .take_over_words(MAC_A)
            .is_some_and(|words| words.starts_with("Busy in the other tab: "))
    });

    assert_eq!(b.take_over_failed(MAC_A), Some(true));
    assert_eq!(net.log_of("A"), ["connect:A"], "nothing closed");
    assert_eq!(net.log_of("B"), Vec::<String>::new(), "B never dialled");
    assert!(a.holds_key(&net_key(MAC_A)));
    assert!(a.card(MAC_A).is_some_and(|card| card.activity.is_some()));
}

/// N5: one board held two ways by two other tabs — C has its USB port, A
/// its network slot. B's card shows the USB hold first, and Connect asks C
/// for the port. Once B has the board on USB, the card shows the network
/// hold that still stands.
#[test]
fn n5_a_usb_hold_shows_before_a_network_hold_of_the_same_board() {
    let desk = Desk::new(&[("dev000000holdn5aa", MAC_A)]);
    let mut c = desk.tab("C", &[0]);
    run_until(&mut [&mut c], "C to hold the USB port", |tabs| {
        tabs[0].holds(MAC_A)
    });
    // A holds the board's network slot (its link is not the point here).
    let a = desk.bus.tab();
    let net = net_key(MAC_A);
    assert_eq!(
        drive(a.claim(&net)),
        crate::ClaimAnswer::Held,
        "A's network lock"
    );

    let mut b = desk.tab("B", &[0]);
    b.library_changed();
    a.post(&HoldNote::Holds {
        key: net,
        level: HoldLevel::Watching,
        locked: true,
    });
    run_until(&mut [&mut c, &mut b], "B to see both holds", |tabs| {
        tabs[1]
            .bench
            .controller
            .board_hold_book()
            .is_some_and(|book| {
                book.held_elsewhere(&net)
                    .is_some_and(|hold| hold.tab.is_some())
                    && book
                        .held_elsewhere(&usb_key(MAC_A))
                        .is_some_and(|hold| hold.tab.is_some())
            })
    });
    assert_eq!(
        b.fact(MAC_A).map(|fact| fact.via),
        Some(crate::HoldVia::Usb),
        "the USB hold shows first"
    );

    b.bench
        .press(b.verb(MAC_A, "take-over"), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut c, &mut b], "B to have the USB port", |tabs| {
        tabs[1].holds(MAC_A)
            && tabs[1]
                .card(MAC_A)
                .is_some_and(|card| card.status == DeviceStatus::Ready)
            && tabs[1]
                .fact(MAC_A)
                .is_some_and(|fact| fact.via == crate::HoldVia::Network)
    });
    assert!(!c.holds(MAC_A));
    assert_eq!(
        b.fact(MAC_A),
        Some(crate::HeldElsewhere {
            via: crate::HoldVia::Network,
            level: HoldLevel::Watching,
            taken_from_here: false,
        }),
        "then the network hold"
    );
    assert_eq!(desk.bus.holder_of(&net), Some(a.tab_id()), "A's still");
}

/// N6, a stranger's busy: the board's one slot is someone else's — lp-cli,
/// or Studio in another browser, which no tab here hears. B's connect over
/// Wi‑Fi is turned away (1013), and through the relay (4429): no hold fact,
/// no `take-over`, the card says the board is busy, and the connect stays
/// offered, so Retry is a press. (The card plan's `UiWifiConnect.busy` is
/// not on main: the busy is read as `WifiConnectFailure::Busy`'s words.)
#[test]
fn n6_a_strangers_busy_offers_retry_and_no_take_over() {
    let desk = Desk::new(&[("dev000000holdn6aa", MAC_A)]);
    let net = NetBoard::new(&desk, Slot::Open);
    // A tab met the board over the LAN (its record is the library's), then
    // closed.
    let mut a = desk.tab("A", &[]);
    net.join(&mut a);
    net.dial_at_load("A", Road::Lan);
    run_until(&mut [&mut a], "A to meet the board", |tabs| {
        tabs[0].holds_key(&net_key(MAC_A))
    });
    desk.crash(a);
    net.page_closed("A");
    net.stranger_holds();

    let mut b = desk.tab("B", &[]);
    net.join(&mut b);
    b.library_changed();
    b.remember_wifi_address(MAC_A);
    b.sign_in();
    run_until(&mut [&mut b], "B's card to offer Wi-Fi", |tabs| {
        tabs[0]
            .bench
            .controller
            .view()
            .offers
            .get(&tabs[0].verb(MAC_A, "connect-wifi"))
            .is_some()
    });

    let wifi = b.verb(MAC_A, "connect-wifi");
    b.bench
        .press(wifi.clone(), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut b], "the busy to be said", |tabs| {
        tabs[0].wifi_words(MAC_A).is_some()
    });
    assert_eq!(
        b.wifi_words(MAC_A).as_deref(),
        Some(crate::WifiConnectFailure::Busy.words().as_str())
    );
    b.assert_a_strangers_busy(MAC_A, &wifi);

    let relay = b.verb(MAC_A, "connect-relay");
    b.bench
        .press(relay.clone(), OfferArgs::new())
        .expect("press");
    run_until(&mut [&mut b], "the relay's busy to be said", |tabs| {
        tabs[0]
            .bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&tabs[0].device_id(MAC_A))
            .is_some_and(|connect| connect.through_relay && connect.error.is_some())
    });
    assert_eq!(
        b.wifi_words(MAC_A).as_deref(),
        Some(crate::RelayConnectFailure::Busy.words().as_str())
    );
    b.assert_a_strangers_busy(MAC_A, &relay);
    assert_eq!(net.log_of("B"), ["busy:B", "busy:B"]);
    assert_eq!(net.holder(), Some(("stranger", Road::Lan)));
}

// ---------------------------------------------------------------------
// The board's network slot, and each tab's sessions to it
// ---------------------------------------------------------------------

/// The road a session takes to the board's one network slot.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Road {
    Lan,
    Relay,
}

/// Whose key the board takes the slot for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Slot {
    /// The holder's key: a tab of the same browser takes the slot.
    Keyed,
    /// Open (the default): every Studio is anonymous, and a second client
    /// is told busy.
    Open,
}

/// Where a tab's session to the board stands (`browser_websocket.js`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionState {
    Connected,
    /// Closed by request: listed, not redialled.
    ClosedByRequest,
    /// The board closed it: not listed, and it redials (rule 2).
    Dropped,
}

struct NetSession {
    state: SessionState,
    /// Moves with every connect and every drop: a link of an older
    /// generation is gone.
    generation: u64,
    /// The bytes to the board while this session's link is open.
    inner: Option<lpa_link::device_link::fake::FakeDeviceLink>,
    /// The board closed it, and the close has not reached the page yet
    /// ([`NetBoard::hold_drops`]).
    drop_in_flight: bool,
}

#[derive(Default)]
struct NetSlotState {
    keyed: bool,
    /// Whose session has the slot: a tab's, on a road, or a stranger's.
    holder: Option<(&'static str, Road)>,
    sessions: BTreeMap<(&'static str, Road), NetSession>,
    /// Drops reach the page only when the test says.
    hold_drops: bool,
    /// Conversations over the network never answer (a push hangs).
    hang: bool,
    /// The next connect asked for is turned away busy.
    refuse_next: bool,
    log: Vec<String>,
}

/// The board's network slot, shared by every tab's LAN and relay sessions.
#[derive(Clone)]
struct NetBoard {
    device: FakeEsp32Device,
    state: Rc<RefCell<NetSlotState>>,
}

impl NetBoard {
    /// The network side of the desk's first board.
    fn new(desk: &Desk, slot: Slot) -> Self {
        Self {
            device: desk.boards[0].device.clone(),
            state: Rc::new(RefCell::new(NetSlotState {
                keyed: slot == Slot::Keyed,
                ..Default::default()
            })),
        }
    }

    /// Give `tab` the shipped build's network halves over this board: a sim
    /// half (none is created), the LAN half and the relay half.
    fn join(&self, tab: &mut Tab) {
        let controller = &mut tab.bench.controller;
        controller.set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
            ScriptedSimSource {
                device: sim_light_player(),
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            },
        ))));
        controller.set_lan_transport(Rc::new(crate::LanDeviceTransport::new(Rc::new(
            DeskNetSource {
                tab: tab.name,
                road: Road::Lan,
                board: self.clone(),
            },
        ))));
        controller.set_relay_transport(Rc::new(crate::RelayDeviceTransport::new(Rc::new(
            DeskNetSource {
                tab: tab.name,
                road: Road::Relay,
                board: self.clone(),
            },
        ))));
    }

    /// `tab`'s page opened a session at load (`?lan=`, `?relay=`), before
    /// anything asked it to: it dials now.
    fn dial_at_load(&self, tab: &'static str, road: Road) {
        let dialled = self.state.borrow_mut().dial(tab, road, "connect");
        assert!(dialled.is_ok(), "{dialled:?}");
    }

    /// The redial timer of every dropped session fires: each dials again
    /// (taking the slot from whoever has it, on a keyed board). Returns the
    /// tabs that redialled.
    fn fire_redials(&self) -> Vec<&'static str> {
        let mut state = self.state.borrow_mut();
        let dropped: Vec<(&'static str, Road)> = state
            .sessions
            .iter()
            .filter(|(_, session)| session.state == SessionState::Dropped)
            .map(|(at, _)| *at)
            .collect();
        for (tab, road) in &dropped {
            let _ = state.dial(tab, *road, "redial");
        }
        dropped.into_iter().map(|(tab, _)| tab).collect()
    }

    /// Whether `tab` has a dropped session waiting on its redial timer.
    fn redial_due(&self, tab: &str) -> bool {
        self.state
            .borrow()
            .sessions
            .iter()
            .any(|((at, _), session)| *at == tab && session.state == SessionState::Dropped)
    }

    /// Hold the board's closes back from the page until
    /// [`Self::deliver_drops`].
    fn hold_drops(&self, hold: bool) {
        self.state.borrow_mut().hold_drops = hold;
    }

    /// Every close the board made reaches its page now: a session still
    /// connected there drops (and will redial); one the page closed by
    /// request meanwhile stays closed.
    fn deliver_drops(&self) {
        let mut state = self.state.borrow_mut();
        for session in state.sessions.values_mut() {
            if std::mem::take(&mut session.drop_in_flight)
                && session.state == SessionState::Connected
            {
                session.state = SessionState::Dropped;
                session.generation += 1;
            }
        }
        state.hold_drops = false;
    }

    /// Conversations over the network never answer from now on.
    fn hang_conversations(&self, hang: bool) {
        self.state.borrow_mut().hang = hang;
    }

    /// The next connect someone asks for is turned away busy, as if a
    /// stranger had taken the slot a moment before.
    fn refuse_next_connect(&self) {
        self.state.borrow_mut().refuse_next = true;
    }

    /// `tab`'s page is gone: the browser closed its sockets, and the board
    /// frees the slot.
    fn page_closed(&self, tab: &str) {
        let mut state = self.state.borrow_mut();
        state.sessions.retain(|(at, _), _| *at != tab);
        if state.holder.is_some_and(|(at, _)| at == tab) {
            state.holder = None;
        }
    }

    /// Someone no tab here hears — lp-cli, Studio in another browser —
    /// takes the slot.
    fn stranger_holds(&self) {
        let mut state = self.state.borrow_mut();
        assert!(state.holder.is_none(), "the slot is free first");
        state.holder = Some(("stranger", Road::Lan));
    }

    fn holder(&self) -> Option<(&'static str, Road)> {
        self.state.borrow().holder
    }

    /// Everything `tab`'s sessions did, in order: `connect:`, `open:`
    /// (a link reopened), `close:` (by request), `forget:` (by request),
    /// `drop:` (the board closed it), `busy:` (turned away), `redial:`.
    fn log_of(&self, tab: &str) -> Vec<String> {
        let suffix = format!(":{tab}");
        self.state
            .borrow()
            .log
            .iter()
            .filter(|line| line.ends_with(&suffix))
            .cloned()
            .collect()
    }

    /// The links `tab`'s page lists on `road`: connected, or closed by
    /// request.
    fn present(&self, tab: &'static str, road: Road) -> Vec<GrantedLink> {
        let state = self.state.borrow();
        let Some(session) = state.sessions.get(&(tab, road)) else {
            return Vec::new();
        };
        if session.state == SessionState::Dropped {
            return Vec::new();
        }
        let info = road_info(road);
        vec![GrantedLink {
            link: Box::new(NetSessionLink {
                info: info.clone(),
                board: self.clone(),
                at: (tab, road),
                generation: session.generation,
                queued: VecDeque::new(),
                dead: false,
            }),
            info,
        }]
    }

    /// `tab`'s page stops reaching the board on `road`: closed by request,
    /// no longer listed or redialled.
    fn forget(&self, tab: &'static str, road: Road) {
        let mut state = self.state.borrow_mut();
        if let Some(mut session) = state.sessions.remove(&(tab, road))
            && let Some(mut inner) = session.inner.take()
        {
            inner.submit(LinkCommand::Close);
        }
        if state.holder == Some((tab, road)) {
            state.holder = None;
        }
        state.log.push(format!("forget:{tab}"));
    }
}

impl NetSlotState {
    /// `tab`'s session on `road` reaches for the slot (`word` names why, in
    /// the log): it takes it when free or its own; on a keyed board it
    /// takes it from the holder, whom the board closes; on an open board it
    /// is told busy.
    fn dial(&mut self, tab: &'static str, road: Road, word: &str) -> Result<(), String> {
        match self.holder {
            None => {}
            Some(holder) if holder == (tab, road) => {}
            Some(holder) if self.keyed => {
                let hold_drops = self.hold_drops;
                if let Some(session) = self.sessions.get_mut(&holder) {
                    // The board stops talking to it at once; the page hears
                    // the close now, or (held back) later.
                    if let Some(mut inner) = session.inner.take() {
                        inner.submit(LinkCommand::Close);
                    }
                    match hold_drops {
                        true => session.drop_in_flight = true,
                        false => {
                            session.state = SessionState::Dropped;
                            session.generation += 1;
                        }
                    }
                }
                self.log.push(format!("drop:{}", holder.0));
            }
            Some(_) => {
                self.log.push(format!("busy:{tab}"));
                return Err(busy_words(road));
            }
        }
        self.holder = Some((tab, road));
        let session = self.sessions.entry((tab, road)).or_insert(NetSession {
            state: SessionState::Connected,
            generation: 0,
            inner: None,
            drop_in_flight: false,
        });
        session.state = SessionState::Connected;
        session.generation += 1;
        session.drop_in_flight = false;
        self.log.push(format!("{word}:{tab}"));
        Ok(())
    }
}

/// One link of a tab's session, as `BrowserWebsocketLink` is one: it opens
/// the session (dialling again if it was closed by request), closes it BY
/// REQUEST, and is gone the moment the board drops the session.
struct NetSessionLink {
    info: LinkInfo,
    board: NetBoard,
    at: (&'static str, Road),
    generation: u64,
    queued: VecDeque<LinkEvent>,
    dead: bool,
}

impl Link for NetSessionLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        if self.dead {
            return;
        }
        let mut state = self.board.state.borrow_mut();
        match command {
            LinkCommand::Open { baud } => {
                let connected = state.sessions.get(&self.at).is_some_and(|session| {
                    session.state == SessionState::Connected
                        && session.generation == self.generation
                });
                if !connected {
                    if let Err(words) = state.dial(self.at.0, self.at.1, "open") {
                        self.dead = true;
                        self.queued.push_back(LinkEvent::Error(words.clone()));
                        self.queued.push_back(LinkEvent::Closed { reason: words });
                        return;
                    }
                    self.generation = state.sessions[&self.at].generation;
                }
                let mut inner = bench_link(self.info.clone(), &self.board.device);
                inner.submit(LinkCommand::Open { baud });
                let session = state.sessions.get_mut(&self.at).expect("dialled");
                if let Some(mut old) = session.inner.replace(inner) {
                    old.submit(LinkCommand::Close);
                }
            }
            LinkCommand::Close => {
                let (tab, road) = self.at;
                if let Some(session) = state.sessions.get_mut(&self.at)
                    && session.generation == self.generation
                {
                    if let Some(mut inner) = session.inner.take() {
                        inner.submit(LinkCommand::Close);
                    }
                    if session.state == SessionState::Connected {
                        session.state = SessionState::ClosedByRequest;
                        state.log.push(format!("close:{tab}"));
                    }
                    if state.holder == Some((tab, road)) {
                        state.holder = None;
                    }
                }
                self.queued.push_back(LinkEvent::Closed {
                    reason: "closed by request".to_string(),
                });
            }
            other => {
                if let Some(inner) = state
                    .sessions
                    .get_mut(&self.at)
                    .filter(|session| session.generation == self.generation)
                    .and_then(|session| session.inner.as_mut())
                {
                    inner.submit(other);
                }
            }
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if let Some(event) = self.queued.pop_front() {
            return Some(event);
        }
        if self.dead {
            return None;
        }
        let mut state = self.board.state.borrow_mut();
        let session = state.sessions.get_mut(&self.at)?;
        if session.generation != self.generation {
            // The board closed this session: its words, then gone.
            self.dead = true;
            let words = lost_words(self.at.1);
            self.queued.push_back(LinkEvent::Error(words.clone()));
            self.queued.push_back(LinkEvent::Closed { reason: words });
            return self.queued.pop_front();
        }
        session.inner.as_mut()?.poll_event()
    }
}

/// One tab's LAN or relay half over the board (`browser_lan_source.rs`,
/// `browser_relay_source.rs`, in small).
struct DeskNetSource {
    tab: &'static str,
    road: Road,
    board: NetBoard,
}

impl DeskNetSource {
    fn io(&self, tap: Option<LensLineTap>) -> Box<dyn lpa_client::ClientIo> {
        if self.board.state.borrow().hang {
            return Box::new(HangingIo);
        }
        let io = FakeDeviceIo::new(&self.board.device);
        Box::new(match tap {
            Some(tap) => io.with_tap(tap),
            None => io,
        })
    }

    fn reach(&self) -> DeviceTransportFuture<Result<(), String>> {
        let mut state = self.board.state.borrow_mut();
        let dialled = match std::mem::take(&mut state.refuse_next) {
            true => {
                state.log.push(format!("busy:{}", self.tab));
                Err(busy_words(self.road))
            }
            false => state.dial(self.tab, self.road, "connect"),
        };
        Box::pin(core::future::ready(dialled))
    }
}

impl crate::LanLinkSource for DeskNetSource {
    fn present(&self) -> Vec<GrantedLink> {
        self.board.present(self.tab, self.road)
    }

    fn forget(&self, _url: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.board.forget(self.tab, self.road);
        Box::pin(core::future::ready(Ok(())))
    }

    fn client_io(
        &self,
        _url: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        Ok(self.io(tap))
    }

    fn connect(&self, _url: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.reach()
    }
}

impl crate::RelayLinkSource for DeskNetSource {
    fn present(&self) -> Vec<GrantedLink> {
        self.board.present(self.tab, self.road)
    }

    fn forget(&self, _board: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.board.forget(self.tab, self.road);
        Box::pin(core::future::ready(Ok(())))
    }

    fn client_io(
        &self,
        _board: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        Ok(self.io(tap))
    }

    fn connect(&self, _board: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.reach()
    }
}

/// A conversation nobody answers: the push hangs until it is evicted.
struct HangingIo;

#[async_trait::async_trait(?Send)]
impl lpa_client::ClientIo for HangingIo {
    async fn send(
        &mut self,
        _msg: lpc_wire::ClientMessage,
    ) -> Result<(), lpc_wire::TransportError> {
        Ok(())
    }

    async fn receive(&mut self) -> Result<lpc_wire::WireServerMessage, lpc_wire::TransportError> {
        core::future::pending().await
    }

    async fn close(&mut self) -> Result<(), lpc_wire::TransportError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// A holds the board over `road` (its page dialled at load), and B — a tab
/// on the same board's network — loads beside it and wears the fact at
/// A's level.
fn net_holder_and_watcher(desk: &Desk, net: &NetBoard, road: Road) -> (Tab, Tab) {
    let mut a = desk.tab("A", &[]);
    net.join(&mut a);
    net.dial_at_load("A", road);
    run_until(
        &mut [&mut a],
        "A to hold the board's network slot",
        |tabs| {
            tabs[0].holds_key(&net_key(MAC_A))
                && tabs[0]
                    .card(MAC_A)
                    .is_some_and(|card| card.status == DeviceStatus::Ready)
        },
    );
    let mut b = desk.tab("B", &[]);
    net.join(&mut b);
    b.library_changed();
    run_until(&mut [&mut a, &mut b], "B to see the board held", |tabs| {
        tabs[1].fact(MAC_A)
            == Some(crate::HeldElsewhere {
                via: crate::HoldVia::Network,
                level: HoldLevel::Watching,
                taken_from_here: false,
            })
    });
    (a, b)
}

impl Tab {
    /// Whether this tab holds `key`.
    fn holds_key(&self, key: &HoldKey) -> bool {
        self.bench
            .controller
            .board_hold_book()
            .and_then(|book| book.holds(key))
            .is_some()
    }

    /// This browser remembers the board's Wi‑Fi address ([`IP`]), as a
    /// page loads it from `localStorage`.
    fn remember_wifi_address(&mut self, mac: &str) {
        let key = crate::BoardKey::parse(mac).expect("a mac");
        let json = format!(r#"{{"{key}":{{"ip":"{IP}","host":"lp-0b0c.local","seenAt":1000.0}}}}"#);
        self.bench.controller.load_wifi_addresses(&json);
        assert!(
            self.bench.controller.wifi_addresses().get(&key).is_some(),
            "{json}"
        );
    }

    /// Someone is signed in: the account's keys open boards through the
    /// relay.
    fn sign_in(&mut self) {
        self.bench
            .controller
            .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    }

    /// The words the card says under its connect over Wi‑Fi or through the
    /// relay, when the last one failed.
    fn wifi_words(&self, mac: &str) -> Option<String> {
        let id = self.device_id(mac);
        self.bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&id)?
            .error
            .clone()
    }

    /// A stranger's busy: no hold fact, no `take-over`, and the connect
    /// that was turned away is offered again, enabled (Retry is a press).
    #[track_caller]
    fn assert_a_strangers_busy(&mut self, mac: &str, connect: &crate::OfferPath) {
        assert_eq!(self.fact(mac), None, "nobody here holds it");
        self.bench.not_offered(self.verb(mac, "take-over"));
        assert!(
            self.bench.offered(connect.clone()).is_enabled(),
            "Retry is a press"
        );
    }
}

/// The board's network slot hold.
fn net_key(mac: &str) -> HoldKey {
    HoldKey::network(crate::BoardKey::parse(mac).expect("a mac"))
}

/// The board's address on Wi‑Fi (made up), and the socket for it.
const IP: &str = "192.168.4.100";
const URL: &str = "ws://192.168.4.100/link";

/// The link a session on `road` is.
fn road_info(road: Road) -> LinkInfo {
    match road {
        Road::Lan => lpa_link::providers::network_link::lan_link_info(URL),
        Road::Relay => {
            let board = crate::BoardKey::parse(MAC_A).expect("a mac").to_string();
            lpa_link::providers::network_link::relay_link_info(
                &lpa_link::providers::network_link::relay_socket_url(
                    "https://lightplayer.app",
                    &board,
                ),
            )
            .expect("a relay leg")
        }
    }
}

/// What `browser_websocket.js` says when the board turns a session away
/// because its slot is taken.
fn busy_words(road: Road) -> String {
    match road {
        Road::Lan => "wi-fi link lost: busy with another Wi\u{2011}Fi connection (Studio in \
                      another tab, or lp-cli; code 1013)"
            .to_string(),
        Road::Relay => "relay link lost: the board closed the link (code 4429: busy)".to_string(),
    }
}

/// What it says when the board closes a session it gave someone else.
fn lost_words(road: Road) -> String {
    match road {
        Road::Lan => "wi-fi link lost: the board closed the link (code 1000)".to_string(),
        Road::Relay => "relay link lost: the board closed the link (code 1000)".to_string(),
    }
}
