//! The board's one network session, shared by the LAN and the relay (Wi-Fi
//! relay plan D2, P6), proven on the host harness: the board's own LAN
//! endpoint, relay driver, slot, mux and server, a stand-in hub
//! ([`super::test_hub`]) and real secure lp-link initiators on the other
//! ends.
//!
//! The headline story is the one M8's automatic LAN upgrade rides: a browser
//! on the relay with key A opens the LAN with key A and takes its own
//! session over; anyone else is told busy.

extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use lp_link::secure_channel::{KeyId, Psk, SecureRole};
use lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
use lpc_access::{OpenTo, SecretEntry, SecretKind, Tier, link_psk};
use lpc_relay::{RelayEvent, RelayState, RouteCloseReason};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{HelloAuth, WireServerMessage};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::harness_entropy::harness_entropy;
use super::test_hub::{HubEvent, TestHub};
use super::{HarnessAccess, HarnessRelay, LanHarness, LanHarnessOptions};

const WAIT: Duration = Duration::from_secs(10);

/// The story M8's LAN upgrade rides, both ways round, and the busy answer.
#[test]
fn the_same_key_moves_its_session_between_the_relay_and_the_lan_and_others_are_busy() {
    let alice = account(1);
    let bob = browser_key(2);
    let mut hub = TestHub::start();
    let harness = start(
        HarnessAccess::locked(alloc::vec![alice.clone(), bob.clone()]),
        &hub,
    );
    registered(&mut hub, &harness);

    // Alice through the relay.
    let mut relay = RelayPeer::open(&hub, 1, &alice);
    assert_eq!(relay.hello(&mut hub).granted, Some(Tier::Edit));

    // Alice on the LAN, with the same key: her session moves.
    let mut lan = LanPeer::connect(&harness, &alice);
    assert_eq!(lan.hello().granted, Some(Tier::Edit));
    assert_eq!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Closed { route: 1, .. })),
        Some(HubEvent::Closed {
            route: 1,
            reason: RouteCloseReason::Normal
        }),
        "the relay route is closed when its session moves to the LAN"
    );
    assert_eq!(harness.stats().takeovers, 1);

    // Bob through the relay: busy, and Alice's LAN session is untouched.
    let mut bob_relay = RelayPeer::open(&hub, 2, &bob);
    bob_relay.pump_for(&mut hub, Duration::from_millis(50));
    assert_eq!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Closed { route: 2, .. })),
        Some(HubEvent::Closed {
            route: 2,
            reason: RouteCloseReason::Busy
        })
    );
    assert!(lan.still_up(Duration::from_millis(300)), "Alice stays");

    // Alice back to the relay: the LAN session goes.
    let mut back = RelayPeer::open(&hub, 3, &alice);
    assert_eq!(back.hello(&mut hub).granted, Some(Tier::Edit));
    assert!(lan.closed_by_board(WAIT), "the LAN socket is closed");
    assert_eq!(harness.stats().takeovers, 1, "the LAN took over once");
    let (state, counters) = harness.relay_status();
    assert_eq!(state, RelayState::Connected);
    assert_eq!((counters.takeovers, counters.busy), (1, 1));
    harness.stop();
}

/// A key id is sent in the clear, so knowing one proves nothing: a
/// handshake under the holder's key id but not its key is busy, and the
/// holder's session is never knocked down.
#[test]
fn a_stranger_with_the_holders_key_id_cannot_evict_it() {
    let alice = account(1);
    let mut hub = TestHub::start();
    let harness = start(HarnessAccess::locked(alloc::vec![alice.clone()]), &hub);
    registered(&mut hub, &harness);

    let mut lan = LanPeer::connect(&harness, &alice);
    assert_eq!(lan.hello().granted, Some(Tier::Edit));
    let mut forged = alice.clone();
    forged.k = [0xee; 32];
    let mut stranger = RelayPeer::open(&hub, 7, &forged);
    stranger.pump_for(&mut hub, Duration::from_millis(50));
    assert_eq!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Closed { route: 7, .. })),
        Some(HubEvent::Closed {
            route: 7,
            reason: RouteCloseReason::Busy
        })
    );
    assert!(lan.still_up(Duration::from_millis(500)), "Alice stays");
    assert_eq!(harness.stats().takeovers, 0);
    harness.stop();
}

/// The board's "Anyone" setting never reaches through the relay: on a board
/// open at edit, the anonymous key holds edit on the LAN and nothing on a
/// relay route (`LinkTrust::Relayed`; the cloud relay ADR §5).
#[test]
fn an_anonymous_relay_session_holds_nothing_on_a_board_open_to_anyone_nearby() {
    let alice = account(1);
    let mut hub = TestHub::start();
    let mut access = HarnessAccess::open(OpenTo::Edit);
    access.secrets.push(alice);
    let harness = start(access, &hub);
    registered(&mut hub, &harness);

    let anonymous = anonymous_key();
    let mut relay = RelayPeer::open(&hub, 1, &anonymous);
    assert_eq!(
        relay.hello(&mut hub).granted,
        None,
        "nothing through the relay"
    );
    hub.close(1);
    assert!(
        hub.wait_for(Duration::from_millis(200), |_| false)
            .is_none(),
        "settle"
    );
    let mut lan = LanPeer::connect(&harness, &anonymous);
    assert_eq!(lan.hello().granted, Some(Tier::Edit), "anyone nearby");
    harness.stop();
}

/// Cloud relay off closes the leg and the board never dials again until it
/// is back on.
#[test]
fn cloud_relay_off_closes_the_leg_and_on_brings_it_back() {
    let mut hub = TestHub::start();
    let harness = start(HarnessAccess::locked(alloc::vec![account(1)]), &hub);
    registered(&mut hub, &harness);
    harness.relay_input(RelayEvent::CloudRelay(false));
    assert_eq!(
        hub.wait_for(WAIT, |e| *e == HubEvent::LegClosed),
        Some(HubEvent::LegClosed)
    );
    wait_state(&harness, RelayState::Off);
    assert!(
        hub.wait_for(Duration::from_secs(3), |e| matches!(
            e,
            HubEvent::Registered { .. }
        ))
        .is_none(),
        "no dial while off"
    );
    harness.relay_input(RelayEvent::CloudRelay(true));
    registered(&mut hub, &harness);
    harness.stop();
}

// --- helpers ---

fn start(access: HarnessAccess, hub: &TestHub) -> LanHarness {
    LanHarness::start(LanHarnessOptions {
        access,
        graphics: None,
        relay: Some(HarnessRelay {
            host: String::from("127.0.0.1"),
            port: hub.port,
            board_mac: [0x02, 0, 0, 0, 0, 1],
            label: String::from("harness"),
        }),
    })
    .expect("the harness starts")
}

fn registered(hub: &mut TestHub, harness: &LanHarness) {
    assert!(
        hub.wait_for(WAIT, |e| matches!(e, HubEvent::Registered { .. }))
            .is_some(),
        "the board registers"
    );
    wait_state(harness, RelayState::Connected);
}

fn wait_state(harness: &LanHarness, want: RelayState) {
    let until = Instant::now() + WAIT;
    while harness.relay_status().0 != want {
        assert!(
            Instant::now() < until,
            "relay state {} never became {want}",
            harness.relay_status().0
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// An account key, as Studio installs it.
fn account(seed: u8) -> SecretEntry {
    entry(SecretKind::Account, seed)
}

/// A browser key (no account: the relay never names it).
fn browser_key(seed: u8) -> SecretEntry {
    entry(SecretKind::Browser, seed)
}

/// The anonymous key: salt and K all zero.
fn anonymous_key() -> SecretEntry {
    let mut key = entry(SecretKind::Browser, 0);
    key.salt = [0; 16];
    key
}

fn entry(kind: SecretKind, seed: u8) -> SecretEntry {
    SecretEntry {
        label: String::from("test key"),
        kind,
        tier: Tier::Edit,
        salt: [seed; 16],
        iterations: 1,
        k: [seed.wrapping_add(100); 32],
        added_at: None,
    }
}

/// A client's secure initiator holding `key` (the anonymous key's PSK is
/// zero, not derived).
fn initiator(key: &SecretEntry) -> Link<SelectiveRepeat> {
    let (key_id, psk) = if key.salt == [0; 16] {
        (KeyId::ANONYMOUS, Psk::ANONYMOUS)
    } else {
        (KeyId(key.salt), Psk::new(link_psk(&key.k)))
    };
    Link::new_secure(
        LinkConfig::ws(),
        harness_nonce_for_test(),
        SecureRole::Initiator { key_id, psk },
        harness_entropy,
    )
}

fn harness_nonce_for_test() -> u32 {
    super::harness_entropy::harness_nonce()
}

fn now_us(started: Instant) -> u64 {
    started.elapsed().as_micros() as u64
}

fn decode_hello(data: &[u8]) -> HelloAuth {
    let message: WireServerMessage = lpc_wire::json::from_slice(data).expect("JSON");
    let ServerMsgBody::Hello(hello) = message.msg else {
        panic!("the first message is the hello");
    };
    hello.auth
}

/// A browser through the relay: an initiator on one route of the hub.
struct RelayPeer {
    route: u16,
    link: Link<SelectiveRepeat>,
    started: Instant,
}

impl RelayPeer {
    fn open(hub: &TestHub, route: u16, key: &SecretEntry) -> Self {
        hub.open(route);
        Self {
            route,
            link: initiator(key),
            started: Instant::now(),
        }
    }

    fn pump(&mut self, hub: &mut TestHub) {
        let now = now_us(self.started);
        while let Some(frame) = self.link.poll_transmit(now) {
            hub.frame(self.route, frame);
        }
        for frame in hub.take_frames(self.route) {
            self.link.on_datagram(now_us(self.started), &frame);
        }
    }

    fn pump_for(&mut self, hub: &mut TestHub, wait: Duration) {
        let until = Instant::now() + wait;
        while Instant::now() < until {
            self.pump(hub);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn hello(&mut self, hub: &mut TestHub) -> HelloAuth {
        let until = Instant::now() + WAIT;
        while Instant::now() < until {
            self.pump(hub);
            while let Some(event) = self.link.recv() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == CH_PROTO
                {
                    return decode_hello(&data);
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("no hello through the relay within {WAIT:?}");
    }
}

/// A client on the board's LAN endpoint.
struct LanPeer {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    link: Link<SelectiveRepeat>,
    started: Instant,
    reset: bool,
}

impl LanPeer {
    fn connect(harness: &LanHarness, key: &SecretEntry) -> Self {
        let url = alloc::format!("ws://{}/link", harness.addr());
        let (mut ws, _) = tungstenite::connect(url).expect("the upgrade");
        if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(2)))
                .unwrap();
        }
        Self {
            ws,
            link: initiator(key),
            started: Instant::now(),
            reset: false,
        }
    }

    /// One pass; `false` once the socket is closed.
    fn pump(&mut self, protos: &mut Vec<Vec<u8>>) -> bool {
        let now = now_us(self.started);
        while let Some(frame) = self.link.poll_transmit(now) {
            if self.ws.send(Message::binary(frame.to_vec())).is_err() {
                return false;
            }
        }
        let open = match self.ws.read() {
            Ok(Message::Binary(frame)) => {
                self.link.on_datagram(now_us(self.started), &frame);
                true
            }
            Ok(Message::Close(_)) => false,
            Ok(_) => true,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                true
            }
            Err(_) => false,
        };
        while let Some(event) = self.link.recv() {
            match event {
                LinkEvent::Message { channel, data } if channel == CH_PROTO => protos.push(data),
                LinkEvent::Reset { .. } => self.reset = true,
                _ => {}
            }
        }
        open
    }

    fn hello(&mut self) -> HelloAuth {
        let until = Instant::now() + WAIT;
        let mut protos = Vec::new();
        while Instant::now() < until {
            assert!(
                self.pump(&mut protos),
                "the LAN socket closed before the hello"
            );
            if let Some(first) = protos.first() {
                return decode_hello(first);
            }
        }
        panic!("no hello on the LAN within {WAIT:?}");
    }

    /// The session stays up (no reset, socket open) for `wait`.
    fn still_up(&mut self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        let mut protos = Vec::new();
        while Instant::now() < until {
            if !self.pump(&mut protos) {
                return false;
            }
        }
        !self.reset
    }

    /// The board closes the socket within `wait`.
    fn closed_by_board(&mut self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        let mut protos = Vec::new();
        while Instant::now() < until {
            if !self.pump(&mut protos) {
                return true;
            }
        }
        false
    }
}
