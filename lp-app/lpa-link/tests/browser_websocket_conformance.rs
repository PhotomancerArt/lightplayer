//! Studio's LAN link (the `browser-websocket` provider), executed in a real
//! browser against a secure lp-link board — the Wi-Fi twin of
//! `browser_ble_conformance.rs`.
//!
//! Every test drives the SHIPPED stack through the library's public API:
//! `browser_websocket.js` (the one instance the library embeds), `WsWire`,
//! `BrowserWebsocketLink`, `WsClientIo` and the page's key source. The board
//! at the other end is a real secure lp-link RESPONDER, built here in Rust
//! the way the firmware builds its LAN slot (`Link::new_secure` with
//! `SecureRole::Responder`, `LinkConfig::ws()`, selective repeat), with a
//! small key store that answers the handshake's lookups and a hello that
//! reports the tier the session's key grants.
//!
//! **What is faked, and why.** `wasm-bindgen-test-runner` serves static files
//! over HTTP and cannot serve a WebSocket peer, so the boundary is faked at
//! the one place the provider touches the browser: `globalThis.WebSocket`
//! (`tests/js/websocket_support.js`). The double moves whole binary messages
//! between the page and the board here; every byte in them — the Noise SYNs,
//! the sealed frames — is the two real link ends'.
//!
//! | rule | test |
//! |---|---|
//! | an open board comes up on the anonymous key; one frame per message | [`an_open_board_comes_up_anonymous_and_says_hello`] |
//! | the held keys are presented in order; an unknown one costs nothing | [`a_held_key_is_presented_and_the_board_grants_its_tier`] |
//! | a request round-trips through the secure link (the conversation io) | [`a_request_round_trips_through_the_secure_link`] |
//! | a key that arrives while up rekeys the link in place, on one socket | [`a_key_that_arrives_while_up_rekeys_the_link_in_place`] |
//! | a wrong key is reported back and never presented there again | [`a_wrong_key_is_reported_and_the_walk_moves_on`] |
//! | a plain-link board is named, never downgraded to | [`a_board_that_runs_a_plain_link_is_named`] |
//! | a drop is a departure, then a reconnect with no gesture | [`a_drop_is_a_departure_and_the_session_reconnects_by_itself`] |
//! | a connect someone asked for waits for the board's answer (a frame, a busy 1013, a socket that never opens) | [`a_connect_someone_asked_for_settles_on_the_boards_answer`] |
//! | a busy board (1013) is said once, never announced again until it answers, and redialled slowly | [`a_busy_board_is_said_once_and_not_announced_until_it_answers`] |
//! | the update channel flows both ways once a core-only board announces it with its `M` | [`the_update_channel_flows_both_ways_once_the_board_announces_it`] |
//! | no reset over Wi-Fi | [`a_reset_over_wifi_fails_by_name`] |
//!
//! Each test uses its own URL: the provider's sessions are page-wide, one per
//! URL, exactly as Studio's are. No assertion is about a duration: waits poll
//! for an event, bounded by a count.

#![cfg(all(target_arch = "wasm32", feature = "browser-websocket"))]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Promise, Uint8Array};
use lpa_client::ClientIo;
use lpa_devices::link::{Link, LinkCommand, LinkEvent, ResetKind};
use lpa_link::device_link::browser_websocket::{BrowserWebsocketLink, lan_link_info};
use lpa_link::device_link::wire_reader::WireRead;
use lpa_link::providers::browser_websocket::{
    self as lan, LanSession, PLAIN_LINK_NOTE, WsClientIo, WsWire,
};
use lpa_link::providers::network_link::{KEY_ID_BYTES, LinkKey, LinkKeys, PSK_BYTES};
use lpc_access::Tier;
use lpc_update::BoardManifest;
use lpc_wire::lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
use lpc_wire::lp_link::{
    CH_PROTO, CH_UPDATE, Link as BoardLink, LinkConfig, LinkEvent as BoardEvent, SelectiveRepeat,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(module = "/tests/js/websocket_support.js")]
extern "C" {
    #[wasm_bindgen(js_name = installFakeWebSocket)]
    fn js_install_fake_websocket();

    #[wasm_bindgen(js_name = takeSent, catch)]
    fn js_take_sent(url: &str) -> Result<Array, JsValue>;

    #[wasm_bindgen(js_name = deliver)]
    fn js_deliver(url: &str, frame: &[u8]) -> bool;

    #[wasm_bindgen(js_name = dropSocket)]
    fn js_drop_socket(url: &str, code: u32, reason: &str) -> bool;

    #[wasm_bindgen(js_name = acceptConnects)]
    fn js_accept_connects(url: &str, accept: bool);

    #[wasm_bindgen(js_name = socketsOpened)]
    fn js_sockets_opened(url: &str) -> u32;

    #[wasm_bindgen(js_name = tick)]
    fn js_tick(ms: u32) -> Promise;
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn an_open_board_comes_up_anonymous_and_says_hello() {
    let url = "ws://10.0.0.1/link";
    let keys = TestKeys::install(Vec::new());
    let mut bench = Bench::new(url, BoardDouble::secure(Opens::Play, Vec::new()));
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);

    let reads = bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    assert_eq!(hello_count(&reads), 1, "{reads:?}");
    assert!(wire.is_link_up(), "the secure handshake finished");
    assert_eq!(
        bench.board.borrow().keys_seen,
        vec![[0; KEY_ID_BYTES]],
        "with no keys held the anonymous key is presented"
    );
    assert_eq!(
        bench.board.borrow().last_hello_granted,
        Some(Some(Tier::Play))
    );
    assert!(keys.wrong.borrow().is_empty());
    let info = lan_link_info(&session);
    assert_eq!(info.endpoint.0, format!("lan:{url}"));
    assert_eq!(info.label, "10.0.0.1");
    assert!(info.usb.is_none() && info.serial_number.is_none());
    assert!(
        bench.board.borrow().largest_frame <= MAX_FRAME_BYTES,
        "every message is one frame"
    );
}

/// The keys the page holds are presented best first: one the board does not
/// know is refused `UnknownKey` (free), and the next is the board's edit key,
/// whose tier the hello reports.
#[wasm_bindgen_test]
async fn a_held_key_is_presented_and_the_board_grants_its_tier() {
    let url = "ws://10.0.0.2/link";
    let keys = TestKeys::install(vec![key(0x51), key(0xED)]);
    let mut bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Nobody, vec![(key(0xED), Tier::Edit)]),
    );
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);

    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    assert_eq!(
        bench.board.borrow().keys_seen,
        vec![[0x51; KEY_ID_BYTES], [0xED; KEY_ID_BYTES]]
    );
    assert_eq!(
        bench.board.borrow().last_hello_granted,
        Some(Some(Tier::Edit))
    );
    assert!(
        keys.wrong.borrow().is_empty(),
        "an unknown key is not wrong"
    );
    let notes = wire.take_notes();
    assert!(
        notes.iter().any(|note| note.contains("refused this key")),
        "the refusal reached the journal: {notes:?}"
    );
}

/// A request goes out as one link message over the secure link and its
/// reply comes back whole — through the borrowing conversation's io, the
/// road a push and the editor lens take.
#[wasm_bindgen_test]
async fn a_request_round_trips_through_the_secure_link() {
    let url = "ws://10.0.0.3/link";
    TestKeys::install(Vec::new());
    let mut bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));
    let session = bench.connect().await;
    let wire = Rc::new(WsWire::new(session.session));
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    let tapped = Rc::new(RefCell::new(Vec::new()));
    let tap = {
        let tapped = Rc::clone(&tapped);
        Rc::new(move |line| tapped.borrow_mut().push(line)) as Rc<dyn Fn(lan::WsTapLine)>
    };
    let mut io = WsClientIo::new(Rc::clone(&wire), Some(tap));
    io.send(lpc_wire::ClientMessage {
        id: 41,
        msg: lpc_wire::ClientRequest::ListLoadedProjects,
    })
    .await
    .expect("the link takes the request");
    let board = Rc::clone(&bench.board);
    let pump = bench.spawn_board_loop();
    let reply = io.receive().await.expect("a reply");
    pump.set(false);

    assert_eq!(reply.id, 41);
    assert_eq!(board.borrow().requests, vec![41]);
    assert!(
        tapped
            .borrow()
            .iter()
            .any(|line| matches!(line, lan::WsTapLine::Line(line) if line.starts_with("M!"))),
        "the fold keeps hearing the board through the tap"
    );
}

/// A locked board brings the anonymous link up holding nothing. A key that
/// arrives while it is up (what a typed password becomes) moves the link
/// onto it IN PLACE: the session ends — read as a link reset — and a new one
/// on the same socket presents the key and is granted its tier.
#[wasm_bindgen_test]
async fn a_key_that_arrives_while_up_rekeys_the_link_in_place() {
    let url = "ws://10.0.0.4/link";
    let keys = TestKeys::install(Vec::new());
    let mut bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Nobody, vec![(key(0x7A), Tier::Edit)]),
    );
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    assert_eq!(bench.board.borrow().last_hello_granted, Some(None));

    keys.set(vec![key(0x7A)]);
    let reads = bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    let reset_at = reads
        .iter()
        .position(|read| matches!(read, WireRead::LinkReset(_)))
        .unwrap_or_else(|| panic!("no link reset read: {reads:?}"));
    let hello_at = reads
        .iter()
        .position(|read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")))
        .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
    assert!(reset_at < hello_at, "{reads:?}");
    assert_eq!(
        bench.board.borrow().last_hello_granted,
        Some(Some(Tier::Edit))
    );
    assert_eq!(js_sockets_opened(url), 1, "one socket, two sessions");
}

/// The board knows the key id but the PSK does not match: it refuses
/// `WrongKey` (charged to its backoff), the page tells the key source so the
/// key is never presented there again, and the walk moves on.
#[wasm_bindgen_test]
async fn a_wrong_key_is_reported_and_the_walk_moves_on() {
    let url = "ws://10.0.0.5/link";
    let mut stale = key(0x33);
    stale.psk = [0xEE; PSK_BYTES];
    let keys = TestKeys::install(vec![stale]);
    let mut bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Play, vec![(key(0x33), Tier::Edit)]),
    );
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);

    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    assert_eq!(
        keys.wrong.borrow().as_slice(),
        [(url.to_string(), [0x33; KEY_ID_BYTES])]
    );
    assert_eq!(
        bench.board.borrow().last_hello_granted,
        Some(Some(Tier::Play)),
        "the anonymous key reached what the board is open to"
    );
}

#[wasm_bindgen_test]
async fn a_board_that_runs_a_plain_link_is_named() {
    let url = "ws://10.0.0.6/link";
    TestKeys::install(Vec::new());
    let mut bench = Bench::new(url, BoardDouble::plain());
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);

    let mut notes = Vec::new();
    for _ in 0..100 {
        bench.round();
        tick(10).await;
        wire.take_reads();
        notes.extend(wire.take_notes());
        if notes.iter().any(|note| note == PLAIN_LINK_NOTE) {
            break;
        }
    }
    assert!(
        notes.iter().any(|note| note == PLAIN_LINK_NOTE),
        "{notes:?}"
    );
    assert!(!wire.is_link_up(), "no downgrade to a plain link");
}

#[wasm_bindgen_test]
async fn a_drop_is_a_departure_and_the_session_reconnects_by_itself() {
    let url = "ws://10.0.0.7/link";
    TestKeys::install(Vec::new());
    let edges = edges();
    let mut bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));
    let session = bench.connect().await;
    let mut link = open_link(&session).await;
    let board = Rc::clone(&bench.board);
    let pump = bench.spawn_board_loop();
    let hello = wait_for(&mut link, |event| matches!(event, LinkEvent::Frame(_))).await;
    assert!(hello.is_some(), "the hello reached the model's link");
    let (connects, disconnects) = (edges.0.get(), edges.1.get());

    js_accept_connects(url, false);
    assert!(js_drop_socket(url, 1001, "rebooting"));

    let lost = wait_for(
        &mut link,
        |event| matches!(event, LinkEvent::Error(error) if error.starts_with("wi-fi link lost")),
    )
    .await;
    assert!(lost.is_some(), "the drop is said in the departure's words");
    let closed = wait_for(&mut link, |event| matches!(event, LinkEvent::Closed { .. })).await;
    assert!(closed.is_some(), "and the link is closed");
    assert_eq!(edges.1.get(), disconnects + 1, "one disconnect edge");
    assert!(
        !lan::present_sessions()
            .iter()
            .any(|present| present.url == url),
        "a dropped session is not present"
    );

    // The board is back: the session reconnects with no call from us, says
    // so with a connect edge, and the new socket is a new link.
    board.borrow_mut().reboot();
    js_accept_connects(url, true);
    for _ in 0..400 {
        if edges.0.get() > connects {
            break;
        }
        tick(20).await;
    }
    pump.set(false);
    assert_eq!(
        edges.0.get(),
        connects + 1,
        "one connect edge on the reconnect"
    );
    assert!(
        lan::present_sessions()
            .iter()
            .any(|present| present.url == url)
    );
    assert_eq!(js_sockets_opened(url), 2);
}

/// A connect a person asked for ("Connect over Wi‑Fi", an address typed into
/// the add slot) waits for the board's answer, not just the upgrade: a busy
/// board takes the upgrade and closes with 1013 at once, and that is the
/// answer the caller hears, in the socket's words; a board that sends its
/// first frame has answered; a socket that never opens says so.
#[wasm_bindgen_test]
async fn a_connect_someone_asked_for_settles_on_the_boards_answer() {
    let url = "ws://10.0.0.9/link";
    TestKeys::install(Vec::new());
    let _bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));

    // Busy: the board takes the upgrade, then turns the connection away.
    let session = lan::open_session(url).expect("a session");
    let busy = spawn_settle(session.session);
    for _ in 0..200 {
        if js_sockets_opened(url) >= 1 {
            break;
        }
        tick(5).await;
    }
    assert!(js_drop_socket(url, 1013, "try again later"));
    let refused = settled(&busy)
        .await
        .expect_err("a busy board turned it away");
    assert!(refused.contains("code 1013"), "{refused}");

    // Free again: the board answers with a frame, and the connect is done.
    let answered = spawn_settle(session.session);
    let opened = js_sockets_opened(url);
    for _ in 0..400 {
        if js_sockets_opened(url) > opened || lan::present_sessions().iter().any(|s| s.url == url) {
            break;
        }
        tick(5).await;
    }
    for _ in 0..400 {
        if js_deliver(url, &[0x5a; 12]) {
            break;
        }
        tick(5).await;
    }
    settled(&answered).await.expect("the board answered");
    assert!(lan::forget(session.session).await);

    // Nothing there: the socket never opens, in the connect's own words.
    let nowhere = "ws://10.0.0.10/link";
    js_accept_connects(nowhere, false);
    let session = lan::open_session(nowhere).expect("a session");
    let failed = settled(&spawn_settle(session.session))
        .await
        .expect_err("nothing answered");
    assert!(failed.starts_with("wi-fi connect to"), "{failed}");
    assert!(lan::forget(session.session).await);
}

/// A busy board (its one LAN slot taken: the upgrade, then close 1013) is
/// said once, in words; the redial that follows is not announced as a
/// board until the board answers, so a busy board does not flap in and out
/// of Studio's roster, and a second refusal is quiet.
#[wasm_bindgen_test]
async fn a_busy_board_is_said_once_and_not_announced_until_it_answers() {
    let url = "ws://10.0.0.11/link";
    TestKeys::install(Vec::new());
    let edges = edges();
    let mut bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));
    let session = bench.connect().await;
    let wire = WsWire::new(session.session);
    let connects = edges.0.get();

    assert!(js_drop_socket(url, 1013, "try again later"));
    let mut errors = Vec::new();
    for _ in 0..200 {
        errors.extend(wire.take_errors().expect("the session's errors"));
        if !errors.is_empty() {
            break;
        }
        tick(5).await;
    }
    for _ in 0..20 {
        errors.extend(wire.take_errors().expect("the session's errors"));
        tick(5).await;
    }
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("wi-fi link lost: busy with another Wi"),
        "{errors:?}"
    );
    assert!(errors[0].contains("code 1013"), "{errors:?}");

    // The redial opens, and is turned away again: nothing announced, nothing
    // said.
    for _ in 0..1_000 {
        if js_sockets_opened(url) >= 2 {
            break;
        }
        tick(10).await;
    }
    assert_eq!(js_sockets_opened(url), 2, "the session redialled");
    assert!(
        !lan::present_sessions()
            .iter()
            .any(|present| present.url == url),
        "a redial to a busy board is not a board"
    );
    assert!(js_drop_socket(url, 1013, "try again later"));
    for _ in 0..40 {
        assert!(
            wire.take_errors().expect("the session's errors").is_empty(),
            "the second refusal is quiet"
        );
        tick(5).await;
    }
    assert!(
        !lan::present_sessions()
            .iter()
            .any(|present| present.url == url)
    );
    assert_eq!(edges.0.get(), connects, "no connect edge while busy");

    // Free again: the next redial is answered, and the board is back.
    let pump = bench.spawn_board_loop();
    for _ in 0..3_000 {
        if lan::present_sessions()
            .iter()
            .any(|present| present.url == url)
        {
            break;
        }
        tick(10).await;
    }
    pump.set(false);
    assert!(
        lan::present_sessions()
            .iter()
            .any(|present| present.url == url),
        "the board answered, and is present"
    );
    assert_eq!(
        edges.0.get(),
        connects + 1,
        "one connect edge when it answers"
    );
    assert!(lan::forget(session.session).await);
}

/// OTA M8: a core-only board (no server, so no hello) announces the update
/// channel with its `M` on link-up. The LAN link hears it — the manifest as
/// `UpdateFacts`, then the bytes as `Update` — and what Studio sends on
/// channel 3 reaches the board's channel 3.
#[wasm_bindgen_test]
async fn the_update_channel_flows_both_ways_once_the_board_announces_it() {
    let url = "ws://10.0.0.12/link";
    TestKeys::install(Vec::new());
    let mut bench = Bench::new(url, BoardDouble::core_only());
    let session = bench.connect().await;
    let mut link = open_link(&session).await;
    let pump = bench.spawn_board_loop();

    let facts = wait_for(&mut link, |event| {
        matches!(event, LinkEvent::UpdateFacts(_))
    })
    .await;
    assert!(
        matches!(&facts, Some(LinkEvent::UpdateFacts(f)) if f.version.as_deref() == Some("2026.10.06-1")),
        "the board's manifest, mirrored: {facts:?}"
    );
    let update = wait_for(&mut link, |event| matches!(event, LinkEvent::Update(_))).await;
    assert!(
        matches!(&update, Some(LinkEvent::Update(bytes)) if bytes.first() == Some(&b'M')),
        "the manifest's bytes, for the update host: {update:?}"
    );

    link.submit(LinkCommand::SendUpdate(b"Q\x01".to_vec()));
    for _ in 0..300 {
        if !bench.board.borrow().updates.is_empty() {
            break;
        }
        tick(10).await;
    }
    pump.set(false);
    assert_eq!(
        bench.board.borrow().updates,
        vec![b"Q\x01".to_vec()],
        "Studio's message reached the board's channel 3"
    );
    assert!(lan::forget(session.session).await);
}

/// Through the relay (its browser leg, `…/relay/board/<mac>`) the page
/// presents the keys it holds and NEVER the anonymous key — even to a board
/// open to anyone — and a connect someone asked for waits for the link to
/// come up, not for the relay's first frame.
#[wasm_bindgen_test]
async fn a_relay_session_presents_held_keys_only_and_settles_when_up() {
    let url = "ws://127.0.0.1:2812/relay/board/a0f26287b401";
    TestKeys::install(vec![key(0x51), key(0xED)]);
    let bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Edit, vec![(key(0xED), Tier::Play)]),
    );
    let session = lan::open_relay_session(url, &[4404, 4429], &[]).expect("a session");
    assert!(session.is_relay());
    // The model's link services the session from its first connection.
    let wire = WsWire::new(session.session);
    let pump = bench.spawn_board_loop();
    let up = spawn_until_up(session.session);
    settled(&up).await.expect("the board accepted a held key");
    assert!(wire.is_link_up());
    // The page's end is up first; the board's says hello on its next turn.
    for _ in 0..200 {
        if bench.board.borrow().last_hello_granted.is_some() {
            break;
        }
        tick(10).await;
    }
    pump.set(false);

    assert_eq!(
        bench.board.borrow().keys_seen,
        vec![[0x51; KEY_ID_BYTES], [0xED; KEY_ID_BYTES]],
        "the held keys, in order, and no anonymous key"
    );
    assert_eq!(
        bench.board.borrow().last_hello_granted,
        Some(Some(Tier::Play)),
        "the key's tier, not the board's open one"
    );
    assert!(lan::forget(session.session).await);
}

/// When no held key opens the board, the relay session is given up in its
/// own words (`relay link lost: …`) and not redialled — and the anonymous
/// key the board would take is never offered.
#[wasm_bindgen_test]
async fn a_relay_session_no_held_key_opens_is_given_up_in_words() {
    let url = "ws://127.0.0.1:2812/relay/board/a0f26287b402";
    TestKeys::install(vec![key(0x51)]);
    let bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));
    let session = lan::open_relay_session(url, &[4404, 4429], &[]).expect("a session");
    let _wire = WsWire::new(session.session);
    let pump = bench.spawn_board_loop();
    let refused = settled(&spawn_until_up(session.session))
        .await
        .expect_err("nothing this page holds opens the board");
    pump.set(false);
    assert_eq!(
        refused,
        format!(
            "relay link lost: {}",
            lpa_link::providers::browser_websocket::RELAY_NO_HELD_KEY
        )
    );
    assert_eq!(bench.board.borrow().keys_seen, vec![[0x51; KEY_ID_BYTES]]);
    for _ in 0..20 {
        tick(20).await;
    }
    assert_eq!(
        js_sockets_opened(url),
        1,
        "a given-up session is not redialled"
    );
    assert!(
        !lan::present_sessions()
            .iter()
            .any(|present| present.url == url)
    );
    assert!(lan::forget(session.session).await);
}

/// A relay link that ran out of held keys waits a few seconds for the page's
/// keys to change (the account's key still loading, a sign-in) and, when one
/// arrives, presents it and comes up — on the same socket.
#[wasm_bindgen_test]
async fn a_relay_session_out_of_keys_comes_up_when_a_key_arrives() {
    let url = "ws://127.0.0.1:2812/relay/board/a0f26287b404";
    let keys = TestKeys::install(vec![key(0x51)]);
    let bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Edit, vec![(key(0xED), Tier::Edit)]),
    );
    let session = lan::open_relay_session(url, &[4404, 4429], &[]).expect("a session");
    let wire = WsWire::new(session.session);
    let pump = bench.spawn_board_loop();
    let up = spawn_until_up(session.session);
    for _ in 0..200 {
        if bench.board.borrow().keys_seen.len() >= 1 {
            break;
        }
        tick(10).await;
    }
    // The account's key arrives after the browser's was refused.
    keys.set(vec![key(0x51), key(0xED)]);
    settled(&up)
        .await
        .expect("the key that arrived opened the board");
    pump.set(false);
    assert!(wire.is_link_up());
    let seen = bench.board.borrow().keys_seen.clone();
    assert_eq!(seen.first(), Some(&[0x51; KEY_ID_BYTES]), "{seen:?}");
    assert_eq!(seen.last(), Some(&[0xED; KEY_ID_BYTES]), "{seen:?}");
    assert!(
        seen.iter()
            .all(|id| *id == [0x51; KEY_ID_BYTES] || *id == [0xED; KEY_ID_BYTES]),
        "only held keys, never the anonymous one: {seen:?}"
    );
    assert_eq!(js_sockets_opened(url), 1, "on the same socket");
    assert!(lan::forget(session.session).await);
}

/// A relay refusal (its close code) ends the session in the relay's words
/// and is not redialled — each redial would spend the page's tries at the
/// relay; any other drop redials, as on the LAN.
#[wasm_bindgen_test]
async fn a_relay_refusal_ends_the_session_and_another_drop_redials() {
    let url = "ws://127.0.0.1:2812/relay/board/a0f26287b403";
    TestKeys::install(vec![key(0xED)]);
    let mut bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Nobody, vec![(key(0xED), Tier::Edit)]),
    );
    let session = lan::open_relay_session(url, &[4404, 4429], &[]).expect("a session");
    for _ in 0..200 {
        if lan::present_sessions()
            .iter()
            .any(|present| present.url == url && present.connected)
        {
            break;
        }
        tick(10).await;
    }
    let wire = WsWire::new(session.session);
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    // The board left the relay mid-session (4410): the page redials.
    js_accept_connects(url, true);
    assert!(js_drop_socket(url, 4410, "board-gone"));
    let mut errors = Vec::new();
    for _ in 0..400 {
        errors.extend(wire.take_errors().unwrap_or_default());
        if js_sockets_opened(url) >= 2 {
            break;
        }
        tick(10).await;
    }
    assert!(
        errors
            .iter()
            .any(|error| error.starts_with("relay link lost") && error.contains("code 4410")),
        "{errors:?}"
    );
    assert_eq!(js_sockets_opened(url), 2, "a 4410 is redialled");

    // Now it is offline (4404): the session ends there.
    for _ in 0..200 {
        if js_drop_socket(url, 4404, "board-offline") {
            break;
        }
        tick(10).await;
    }
    for _ in 0..40 {
        tick(20).await;
    }
    assert_eq!(js_sockets_opened(url), 2, "a 4404 is not redialled");
    assert!(
        !lan::present_sessions()
            .iter()
            .any(|present| present.url == url)
    );
    assert!(lan::forget(session.session).await);
}

/// OTA M8 through the relay: while an update holds the session (the board
/// resets three times, and the relay says "board offline" until it is
/// back), a 4404 is a drop redialled after its own delay, not the end; once
/// the hold lapses, a 4404 ends the session again.
#[wasm_bindgen_test]
async fn a_held_relay_session_redials_through_board_offline() {
    let url = "ws://127.0.0.1:2812/relay/board/a0f26287b405";
    TestKeys::install(vec![key(0xED)]);
    let mut bench = Bench::new(
        url,
        BoardDouble::secure(Opens::Nobody, vec![(key(0xED), Tier::Edit)]),
    );
    let session = lan::open_relay_session(url, &[4404, 4429], &[(4404, 40)]).expect("a session");
    for _ in 0..200 {
        if lan::present_sessions()
            .iter()
            .any(|present| present.url == url && present.connected)
        {
            break;
        }
        tick(10).await;
    }
    let wire = WsWire::new(session.session);
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    js_accept_connects(url, true);

    // Held: two "board offline" closes in a row, each redialled.
    wire.hold(400);
    for opened in [2, 3] {
        for _ in 0..200 {
            if js_drop_socket(url, 4404, "board-offline") {
                break;
            }
            tick(10).await;
        }
        for _ in 0..200 {
            if js_sockets_opened(url) >= opened {
                break;
            }
            tick(10).await;
        }
        assert_eq!(js_sockets_opened(url), opened, "a held 4404 is redialled");
    }

    // The hold lapsed: the next 4404 ends the session.
    for _ in 0..30 {
        tick(20).await;
    }
    for _ in 0..200 {
        if js_drop_socket(url, 4404, "board-offline") {
            break;
        }
        tick(10).await;
    }
    for _ in 0..20 {
        tick(20).await;
    }
    assert_eq!(js_sockets_opened(url), 3, "an unheld 4404 is not redialled");
    assert!(lan::forget(session.session).await);
}

#[wasm_bindgen_test]
async fn a_reset_over_wifi_fails_by_name() {
    let url = "ws://10.0.0.8/link";
    TestKeys::install(Vec::new());
    let mut bench = Bench::new(url, BoardDouble::secure(Opens::Edit, Vec::new()));
    let session = bench.connect().await;
    let mut link = open_link(&session).await;

    link.submit(LinkCommand::RunReset(ResetKind::Normal));
    let outcome = wait_for(&mut link, |event| {
        matches!(event, LinkEvent::ResetOutcome { ok: false, .. })
    })
    .await;
    assert!(outcome.is_some(), "a reset is refused, never pretended");
}

// ---------------------------------------------------------------------------
// The bench: the page's link end against a board double, frame by frame
// ---------------------------------------------------------------------------

/// The largest frame either end may put in one message: `ws()`'s 1 KiB
/// payload, the header and CRC, and the seal.
const MAX_FRAME_BYTES: usize = 1024 + 64;

struct Bench {
    url: String,
    board: Rc<RefCell<BoardDouble>>,
    reads: Vec<WireRead>,
}

impl Bench {
    fn new(url: &str, board: BoardDouble) -> Self {
        js_install_fake_websocket();
        js_accept_connects(url, true);
        Self {
            url: url.to_string(),
            board: Rc::new(RefCell::new(board)),
            reads: Vec::new(),
        }
    }

    /// Open the page's session for this board and wait for its socket.
    async fn connect(&mut self) -> LanSession {
        let session = lan::open_session(&self.url).expect("a session");
        for _ in 0..200 {
            if lan::present_sessions()
                .iter()
                .any(|present| present.url == self.url && present.connected)
            {
                break;
            }
            tick(10).await;
        }
        session
    }

    /// Run the board, draining the page's end, until `done` holds for what
    /// the page read (bounded by rounds, never by a clock).
    async fn exchange_until(
        &mut self,
        wire: &WsWire,
        done: impl Fn(&[WireRead]) -> bool,
    ) -> Vec<WireRead> {
        for _ in 0..300 {
            self.round();
            tick(10).await;
            self.reads.extend(wire.take_reads());
            if done(&self.reads) {
                break;
            }
        }
        std::mem::take(&mut self.reads)
    }

    /// One turn of the board: read what the page sent, answer, and hand its
    /// frames back one message each.
    fn round(&self) {
        board_round(&self.url, &self.board);
    }

    /// Keep the board turning in the background (for a test that waits on
    /// the page's own API); clear the flag to stop it.
    fn spawn_board_loop(&self) -> Rc<Cell<bool>> {
        let running = Rc::new(Cell::new(true));
        let (flag, url, board) = (
            Rc::clone(&running),
            self.url.clone(),
            Rc::clone(&self.board),
        );
        wasm_bindgen_futures::spawn_local(async move {
            while flag.get() {
                board_round(&url, &board);
                tick(10).await;
            }
        });
        running
    }
}

fn board_round(url: &str, board: &Rc<RefCell<BoardDouble>>) {
    let sent = js_take_sent(url).expect("every frame its own buffer");
    let out = {
        let mut board = board.borrow_mut();
        for frame in sent.iter() {
            board.on_datagram(&Uint8Array::new(&frame).to_vec());
        }
        board.frames_now()
    };
    for frame in out {
        js_deliver(url, &frame);
    }
}

/// What the board is open to without a key.
#[derive(Clone, Copy)]
enum Opens {
    Nobody,
    Play,
    Edit,
}

/// A board's LAN slot, as the firmware runs it: a secure lp-link responder
/// on `ws()` with selective repeat, a key store answering the lookups, the
/// hello on every `Up` reporting the session's tier, JSON replies.
struct BoardDouble {
    link: BoardLink<SelectiveRepeat>,
    secure: bool,
    open: Opens,
    entries: Vec<(LinkKey, Tier)>,
    /// Every key id a handshake named, in order.
    keys_seen: Vec<[u8; KEY_ID_BYTES]>,
    /// The `granted` of the last hello sent (`None` = none sent yet).
    last_hello_granted: Option<Option<Tier>>,
    requests: Vec<u64>,
    largest_frame: usize,
    nonce: u32,
    /// A core-only board's manifest, sent as `M` on channel 3 on every `Up`
    /// instead of a hello (it has no server).
    manifest: Option<BoardManifest>,
    /// Every channel-3 message read.
    updates: Vec<Vec<u8>>,
}

impl BoardDouble {
    fn secure(open: Opens, entries: Vec<(LinkKey, Tier)>) -> Self {
        Self::build(true, open, entries, 0xB0A2_0001)
    }

    fn plain() -> Self {
        Self::build(false, Opens::Edit, Vec::new(), 0xB0A2_0001)
    }

    /// A split image waiting for its engine, open to edit: no hello, its
    /// `M` instead.
    fn core_only() -> Self {
        Self {
            manifest: Some(board_manifest()),
            ..Self::secure(Opens::Edit, Vec::new())
        }
    }

    fn build(secure: bool, open: Opens, entries: Vec<(LinkKey, Tier)>, nonce: u32) -> Self {
        let link = if secure {
            BoardLink::new_secure(
                LinkConfig::ws(),
                nonce,
                SecureRole::Responder,
                board_entropy,
            )
        } else {
            BoardLink::new(LinkConfig::ws(), nonce)
        };
        Self {
            link,
            secure,
            open,
            entries,
            keys_seen: Vec::new(),
            last_hello_granted: None,
            requests: Vec::new(),
            largest_frame: 0,
            nonce,
            manifest: None,
            updates: Vec::new(),
        }
    }

    /// The board restarts: a new link, a new nonce, the same store.
    fn reboot(&mut self) {
        let entries = std::mem::take(&mut self.entries);
        let manifest = self.manifest.take();
        *self = Self::build(self.secure, self.open, entries, self.nonce + 1);
        self.manifest = manifest;
    }

    fn now() -> u64 {
        (js_sys::Date::now() * 1_000.0) as u64
    }

    fn on_datagram(&mut self, frame: &[u8]) {
        self.largest_frame = self.largest_frame.max(frame.len());
        self.link.on_datagram(Self::now(), frame);
        while let Some(event) = self.link.poll_secure_event() {
            if let SecureEvent::KeyLookup { key_id } = event {
                self.keys_seen.push(key_id.0);
                if key_id.is_anonymous() {
                    self.link.provide_keys(key_id, &[Psk::ANONYMOUS]);
                    continue;
                }
                let candidates: Vec<Psk> = self
                    .entries
                    .iter()
                    .filter(|(entry, _)| entry.key_id == key_id.0)
                    .map(|(entry, _)| Psk::new(entry.psk))
                    .collect();
                if candidates.is_empty() {
                    self.link.refuse(key_id, RefusalReason::UnknownKey, 0);
                } else {
                    self.link.provide_keys(key_id, &candidates);
                }
            }
        }
        while let Some(event) = self.link.recv() {
            match event {
                BoardEvent::Up { .. } => match &self.manifest {
                    Some(manifest) => {
                        let mut m = vec![b'M'];
                        m.extend_from_slice(&manifest.to_json());
                        self.link.send(CH_UPDATE, &m).expect("board send");
                    }
                    None => {
                        let granted = self.granted();
                        self.last_hello_granted = Some(granted);
                        self.send(&hello(granted));
                    }
                },
                BoardEvent::Message {
                    channel: CH_UPDATE,
                    data,
                } => self.updates.push(data),
                BoardEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => {
                    let request = lpc_wire::decode_client_payload(&data).expect("a request");
                    self.requests.push(request.id);
                    self.send(&lpc_wire::WireServerMessage::new(
                        request.id,
                        lpc_wire::ServerMsgBody::ListLoadedProjects {
                            projects: Vec::new(),
                        },
                    ));
                }
                _ => {}
            }
        }
    }

    /// The tier this session's key grants: the entry's, or what the board is
    /// open to for the anonymous key.
    fn granted(&self) -> Option<Tier> {
        if !self.secure {
            return Some(Tier::Edit);
        }
        let auth = self.link.session_auth()?;
        if auth.key_id == KeyId::ANONYMOUS {
            return match self.open {
                Opens::Nobody => None,
                Opens::Play => Some(Tier::Play),
                Opens::Edit => Some(Tier::Edit),
            };
        }
        self.entries
            .iter()
            .find(|(entry, _)| entry.key_id == auth.key_id.0)
            .map(|(_, tier)| *tier)
    }

    fn send(&mut self, message: &lpc_wire::WireServerMessage) {
        let json = lpc_wire::json::to_string(message).expect("json");
        self.link
            .send(CH_PROTO, json.as_bytes())
            .expect("board send");
    }

    /// Every frame the board's link has to send now, one message each.
    fn frames_now(&mut self) -> Vec<Vec<u8>> {
        let now = Self::now();
        let mut out = Vec::new();
        while let Some(frame) = self.link.poll_transmit(now) {
            self.largest_frame = self.largest_frame.max(frame.len());
            out.push(frame.to_vec());
        }
        out
    }
}

/// The board double's entropy: distinct bytes per handshake, which is all a
/// test needs (the page's end draws from `crypto.getRandomValues`).
fn board_entropy(buf: &mut [u8]) {
    thread_local! {
        static NEXT: Cell<u8> = const { Cell::new(1) };
    }
    NEXT.with(|next| {
        for byte in buf.iter_mut() {
            *byte = next.get();
            next.set(next.get().wrapping_mul(31).wrapping_add(7));
        }
    });
}

fn hello(granted: Option<Tier>) -> lpc_wire::WireServerMessage {
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    lpc_wire::WireServerMessage::new(
        0,
        lpc_wire::ServerMsgBody::Hello(ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![],
                package: "fw-esp32c6".to_string(),
                version: "unknown".into(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts::default(),
            device_uid: None,
            // No pack format: the page's link stays JSON.
            pack_format: 0,
            auth: lpc_wire::HelloAuth {
                required: true,
                granted,
            },
            firmware: None,
        }),
    )
}

// ---------------------------------------------------------------------------
// The page's keys
// ---------------------------------------------------------------------------

/// A key source the suite controls, installed as the page's.
struct TestKeys {
    keys: RefCell<Vec<LinkKey>>,
    generation: Cell<u64>,
    wrong: RefCell<Vec<(String, [u8; KEY_ID_BYTES])>>,
}

impl TestKeys {
    fn install(keys: Vec<LinkKey>) -> Rc<Self> {
        let source = Rc::new(Self {
            keys: RefCell::new(keys),
            generation: Cell::new(1),
            wrong: RefCell::new(Vec::new()),
        });
        lan::set_link_keys(Rc::clone(&source) as Rc<dyn LinkKeys>);
        source
    }

    fn set(&self, keys: Vec<LinkKey>) {
        *self.keys.borrow_mut() = keys;
        self.generation.set(self.generation.get() + 1);
    }
}

impl LinkKeys for TestKeys {
    fn keys_for(&self, _address: &str) -> Vec<LinkKey> {
        let wrong = self.wrong.borrow();
        self.keys
            .borrow()
            .iter()
            .filter(|key| !wrong.iter().any(|(_, id)| *id == key.key_id))
            .cloned()
            .collect()
    }

    fn generation(&self, _address: &str) -> u64 {
        self.generation.get()
    }

    fn refused_wrong(&self, address: &str, key: &LinkKey) {
        self.wrong
            .borrow_mut()
            .push((address.to_string(), key.key_id));
        self.generation.set(self.generation.get() + 1);
    }
}

fn key(id: u8) -> LinkKey {
    LinkKey {
        key_id: [id; KEY_ID_BYTES],
        psk: [id ^ 0xA5; PSK_BYTES],
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn open_link(session: &LanSession) -> BrowserWebsocketLink {
    let mut link = BrowserWebsocketLink::new(
        Rc::new(WsWire::new(session.session)),
        lan_link_info(session),
    );
    link.submit(LinkCommand::Open { baud: 0 });
    let opened = wait_for(&mut link, |event| matches!(event, LinkEvent::Opened { .. })).await;
    assert!(opened.is_some(), "the link opened");
    link
}

/// Poll the link until an event matches, draining everything before it.
async fn wait_for(
    link: &mut BrowserWebsocketLink,
    matches: impl Fn(&LinkEvent) -> bool,
) -> Option<LinkEvent> {
    for _ in 0..250 {
        while let Some(event) = link.poll_event() {
            if matches(&event) {
                return Some(event);
            }
        }
        tick(20).await;
    }
    None
}

/// A core-only board's manifest (what its `M` carries).
fn board_manifest() -> BoardManifest {
    BoardManifest {
        proto: 1,
        target: "esp32c6-4mb".to_string(),
        chip: "esp32c6".to_string(),
        version: "2026.10.06-1".to_string(),
        build_id: "2026.10.06-1+abc123456789".to_string(),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        core_sha256: "11".repeat(32),
        core_len: 4096,
        engine_sha256: "22".repeat(32),
        engine_len: Some(8192),
        layout: 1,
        loader: 1,
        region_len: 65_536,
        state: lpc_update::BoardState::NeedsEngine,
        refused_build: None,
        transfer: None,
    }
}

fn hello_count(reads: &[WireRead]) -> usize {
    reads
        .iter()
        .filter(|read| matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\"")))
        .count()
}

/// The presence edges, installed ONCE per page (the provider's rule) and
/// counted for every test after.
fn edges() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    thread_local! {
        static EDGES: (Rc<Cell<u32>>, Rc<Cell<u32>>) = {
            let connects = Rc::new(Cell::new(0));
            let disconnects = Rc::new(Cell::new(0));
            let (c, d) = (Rc::clone(&connects), Rc::clone(&disconnects));
            let on_connect = Closure::wrap(Box::new(move || c.set(c.get() + 1)) as Box<dyn FnMut()>);
            let on_disconnect =
                Closure::wrap(Box::new(move || d.set(d.get() + 1)) as Box<dyn FnMut()>);
            lan::install_websocket_events(
                on_connect.as_ref().unchecked_ref(),
                on_disconnect.as_ref().unchecked_ref(),
            );
            on_connect.forget();
            on_disconnect.forget();
            (connects, disconnects)
        };
    }
    EDGES.with(|(connects, disconnects)| (Rc::clone(connects), Rc::clone(disconnects)))
}

async fn tick(ms: u32) {
    let _ = JsFuture::from(js_tick(ms)).await;
}

/// Start `connect_and_settle` on `session` (a generous settle window: the
/// tests end it with the board's own answer, never by waiting it out).
fn spawn_settle(session: u32) -> Rc<RefCell<Option<Result<(), String>>>> {
    let out = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&out);
    wasm_bindgen_futures::spawn_local(async move {
        let result = lan::connect_and_settle(session, 30_000).await;
        *slot.borrow_mut() = Some(result);
    });
    out
}

/// Start `connect_until_up` on `session` (the relay's connect; the same
/// generous window).
fn spawn_until_up(session: u32) -> Rc<RefCell<Option<Result<(), String>>>> {
    let out = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&out);
    wasm_bindgen_futures::spawn_local(async move {
        let result = lan::connect_until_up(session, 30_000).await;
        *slot.borrow_mut() = Some(result);
    });
    out
}

/// What a spawned connect came to (bounded by rounds, never by a clock).
async fn settled(out: &Rc<RefCell<Option<Result<(), String>>>>) -> Result<(), String> {
    for _ in 0..1_000 {
        if let Some(result) = out.borrow_mut().take() {
            return result;
        }
        tick(10).await;
    }
    panic!("the connect never settled");
}
