//! The LAN endpoint's contract, one named test per fact a fielded host
//! depends on (OTA-over-Wi-Fi plan, W1 and P2). Once a core that updates over
//! Wi-Fi is on a board in a house, that board is reachable only here, so
//! these facts are frozen with the secure bytes
//! (`lp-base/lp-link/tests/secure_ws_bytes_golden.rs`):
//!
//! - the path is `/link` (and only that);
//! - the port is 80, the board's `LINK_PORT`;
//! - one binary WebSocket message is one lp-link frame, each way, and
//!   nothing else is a message (a text message is refused);
//! - a board with every LAN slot taken upgrades the extra connection and
//!   closes it **1013** ("try again later").
//!
//! A red test here names a fact that fielded hosts (lp-cli's `lan:`,
//! Studio's Wi-Fi link, the cloud relay's board end) depend on: change the
//! fact only with a host that still reaches the old board, never by editing
//! the assertion.
//!
//! These run the host LAN harness (`net::host_lan_harness`), which carries
//! the board's own WebSocket server, link mux and radio-link port over a
//! std socket. The port is the one fact the harness cannot show (it binds
//! `127.0.0.1:0`), so that test reads the firmware's source; see it.
//!
//! Run: `cargo test -p fw-esp32-common --features wifi,host-lan-harness`.

#![cfg(feature = "host-lan-harness")]

use std::net::TcpStream;
use std::time::{Duration, Instant};

use fw_esp32_common::net::host_lan_harness::harness_entropy::harness_entropy;
use fw_esp32_common::net::host_lan_harness::{HarnessAccess, LanHarness, LanHarnessOptions};
use fw_esp32_common::net::ws::LINK_PATH;
use fw_esp32_common::radio_link::LAN_LINK_SLOTS;
use fw_esp32_common::radio_link::lan_link_config::lan_link_config;
use lp_link::secure_channel::{KeyId, Psk, SecureRole};
use lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, LinkState, SelectiveRepeat};
use lpc_access::OpenTo;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

#[test]
fn the_link_path_is_slash_link() {
    assert_eq!(LINK_PATH, "/link");
    let harness = start();
    // `/link` upgrades.
    let (_ws, response) =
        tungstenite::connect(format!("ws://{}/link", harness.addr())).expect("/link upgrades");
    assert_eq!(response.status(), 101);
    // Nothing else does: another path, a prefix, a trailing slash, a query.
    for path in ["/", "/links", "/link/", "/link?x=1", "/ws", "/LINK"] {
        match tungstenite::connect(format!("ws://{}{path}", harness.addr())) {
            Err(tungstenite::Error::Http(response)) => {
                assert_eq!(response.status(), 404, "{path}");
            }
            other => panic!("{path} must be refused 404: {:?}", other.map(|_| ())),
        }
    }
    harness.stop();
}

/// The board listens on 80 and advertises 80. Both constants live in the
/// bare-metal `fw-esp32c6` crate, which this host crate cannot depend on,
/// and the harness binds an ephemeral port, so this pins the two source
/// declarations (the listener's and the mDNS record's) instead of a running
/// socket. A change that moves the port edits one of these lines and fails
/// here, naming the fact.
#[test]
fn the_link_port_is_80() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fw-esp32c6/src/net");
    for file in ["lan_endpoint_task.rs", "mdns_task.rs"] {
        let path = root.join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            source
                .lines()
                .any(|line| line.trim() == "pub const LINK_PORT: u16 = 80;"
                    || line.trim() == "const LINK_PORT: u16 = 80;"),
            "{file} must declare the link port as 80: fielded hosts dial `lan:<board>` on it"
        );
    }
    // The endpoint listens on that constant, not on a literal.
    let endpoint = std::fs::read_to_string(root.join("lan_endpoint_task.rs")).unwrap();
    assert!(
        endpoint.contains("socket.accept(LINK_PORT)"),
        "the endpoint must accept on LINK_PORT"
    );
    // And the record names the same constant.
    let mdns = std::fs::read_to_string(root.join("mdns_task.rs")).unwrap();
    assert!(
        mdns.contains("port: LINK_PORT"),
        "the mDNS record must advertise LINK_PORT"
    );
}

/// The secure LAN golden (`lp-base/lp-link/tests/secure_ws_bytes_golden.rs`)
/// cannot depend on this crate, so it builds the board's config again: the
/// preset with windows of 2 and an ACK every second frame. This is the
/// other half of that pin: the config the board really builds is that one,
/// in every field that reaches the wire (only the local buffer budgets
/// differ, and those put no byte on the wire).
#[test]
fn the_boards_link_config_is_the_one_the_golden_pins() {
    let board = lan_link_config();
    let mut pinned = LinkConfig::ws();
    pinned.tx_window = 2;
    pinned.rx_window = 2;
    pinned.ack_every = 2;
    // The budgets and queues are local memory, not wire.
    pinned.max_message = board.max_message;
    pinned.rx_budget = board.rx_budget;
    pinned.send_budget = board.send_budget;
    pinned.send_queue = board.send_queue;
    pinned.keep_reassembly = board.keep_reassembly;
    pinned.datagram_queue = board.datagram_queue;
    assert_eq!(
        format!("{board:?}"),
        format!("{pinned:?}"),
        "the board's LAN link config moved off what the secure LAN golden pins"
    );
}

#[test]
fn one_binary_message_is_one_frame_each_way() {
    let harness = start();
    let mut ws = connect(&harness);
    let mut link = host_link();
    set_read_timeout(&mut ws, Duration::from_millis(5));

    let started = Instant::now();
    let now = || started.elapsed().as_micros() as u64;
    let (mut sent, mut received, mut hello) = (0u32, 0u32, false);
    while started.elapsed() < Duration::from_secs(10) && !hello {
        // Host to board: every frame the link has, one binary message each.
        while let Some(frame) = link.poll_transmit(now()) {
            ws.send(Message::binary(frame.to_vec())).unwrap();
            sent += 1;
        }
        // Board to host: only binary messages, each one whole frame.
        match ws.read() {
            Ok(Message::Binary(frame)) => {
                received += 1;
                link.on_datagram(now(), &frame);
            }
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(other) => panic!("the board sent something other than a binary frame: {other:?}"),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("the link's socket failed: {error}"),
        }
        while let Some(event) = link.recv() {
            if matches!(event, LinkEvent::Message { channel, .. } if channel == CH_PROTO) {
                hello = true;
            }
        }
    }
    assert!(hello, "the board's hello arrives over the link");
    assert_eq!(link.state(), LinkState::Established);
    // The link took each message as exactly one frame, and none as damage.
    let counters = link.counters();
    assert_eq!(
        counters.frames_rx, received,
        "each binary message is one frame"
    );
    assert_eq!(counters.bad_frames, 0, "no message was a partial frame");
    assert_eq!(counters.protocol_errors, 0);
    // The board took each of ours as one frame too: it answered, so it is up.
    assert!(sent >= 2, "SYN and confirmation went as {sent} messages");
    drop(ws);
    harness.stop();
}

#[test]
fn a_text_message_is_refused() {
    let harness = start();
    let mut ws = connect(&harness);
    ws.send(Message::text("{\"not\":\"a frame\"}")).unwrap();
    set_read_timeout(&mut ws, Duration::from_secs(5));
    let code = loop {
        match ws.read().expect("the board answers a text message") {
            Message::Close(frame) => break frame.map(|f| u16::from(f.code)),
            _ => {}
        }
    };
    assert_eq!(code, Some(1003), "unsupported data");
    harness.stop();
}

#[test]
fn a_busy_board_answers_close_1013() {
    let harness = start();
    // Every LAN slot taken (each upgraded connection holds one).
    let _held: Vec<Socket> = (0..LAN_LINK_SLOTS).map(|_| connect(&harness)).collect();
    // The extra one upgrades, then is told to try again later.
    let mut extra = connect(&harness);
    set_read_timeout(&mut extra, Duration::from_secs(5));
    let code = loop {
        match extra
            .read()
            .expect("the board answers the extra connection")
        {
            Message::Close(frame) => break frame.map(|f| u16::from(f.code)),
            _ => {}
        }
    };
    assert_eq!(code, Some(1013), "try again later");
    assert_eq!(harness.stats().refused, 1);
    harness.stop();
}

fn start() -> LanHarness {
    LanHarness::start(LanHarnessOptions {
        access: HarnessAccess::open(OpenTo::Edit),
        graphics: None,
    })
    .expect("the harness starts")
}

fn connect(harness: &LanHarness) -> Socket {
    tungstenite::connect(format!("ws://{}{LINK_PATH}", harness.addr()))
        .expect("the upgrade")
        .0
}

fn set_read_timeout(ws: &mut Socket, wait: Duration) {
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        stream.set_read_timeout(Some(wait)).unwrap();
    }
}

/// A host's end as lp-cli's `lan:` builds it: `ws()` unchanged, secure, the
/// anonymous key (the harness board is open).
fn host_link() -> Link<SelectiveRepeat> {
    Link::new_secure(
        LinkConfig::ws(),
        0x00c0_47ac,
        SecureRole::Initiator {
            key_id: KeyId::ANONYMOUS,
            psk: Psk::ANONYMOUS,
        },
        harness_entropy,
    )
}
