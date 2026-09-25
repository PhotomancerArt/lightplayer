//! Studio's Web Bluetooth link, executed in a real browser against the
//! `?ble=emu` polyfill (M5 S5) — the Bluetooth twin of
//! `browser_serial_conformance.rs`.
//!
//! Every test drives the SHIPPED stack through the library's own public API:
//! `browser_ble.js` (the one instance Studio ships — this suite never loads
//! a second copy), `BleWire`, `BrowserBleLink` and `BleClientIo`. Under them
//! sits `/lpa-link/virtual_bluetooth.js`, the polyfill `?ble=emu` installs,
//! and under THAT the same scripted door the serial suite uses — so the
//! suite is hermetic: no server, no firmware, no radio.
//!
//! What it pins, rule by rule (`browser_ble.js`'s header has the why):
//!
//! | rule | test |
//! |---|---|
//! | the GATT subset the provider calls is the polyfill's whole surface | [`a_picked_device_is_connected_present_and_wears_a_ble_endpoint`] |
//! | lines re-join across notifications through the ONE `LineSplitter` | [`a_line_split_across_notifications_arrives_whole`] |
//! | writes are chunked to ≤ 180 B and awaited, in order | [`a_long_line_goes_out_in_awaited_180_byte_writes`] |
//! | a drop is a departure, then a reconnect with no gesture | [`a_drop_is_a_departure_and_the_session_reconnects_by_itself`] |
//! | a drop the page never heard is found by the visibility re-check | [`a_drop_the_page_never_heard_is_found_on_the_recheck`] |
//! | a drop tears the radio link down, so the reconnect is a fresh link | [`a_phantom_drop_is_torn_down_and_the_reconnect_is_a_fresh_link`] |
//! | every connect is bounded | [`a_connect_that_never_settles_fails_by_name`] |
//! | close is ours: the session stays, reopening needs no chooser | [`a_closed_link_stays_present_and_reopens_without_the_chooser`] |
//! | no reset over GATT | [`a_reset_over_bluetooth_fails_by_name`] |
//! | M4: an untrusted link that never logs in is closed in 10 s | [`an_untrusted_link_that_never_logs_in_is_dropped`] |
//! | a borrowing conversation's io | [`the_conversation_io_round_trips_a_request`] |
//! | availability, for the add slot's copy | [`availability_reads_the_browser_not_a_guess`] |
//!
//! ⚠️ TRUST: the polyfill proves the transport, not access enforcement —
//! the emulated firmware sees a trusted USB link (see the polyfill's header).
//!
//! No assertion here is about a duration: waits poll for an event, bounded
//! by a count. The one bounded-connect test takes the real 10 s.

#![cfg(all(target_arch = "wasm32", feature = "browser-ble"))]

use std::cell::Cell;
use std::rc::Rc;

use js_sys::{Array, Promise};
use lpa_devices::link::{Link, LinkCommand, LinkEvent, ResetKind};
use lpa_link::device_link::browser_ble::{BrowserBleLink, ble_link_info};
use lpa_link::providers::browser_ble::{self as ble, BleClientIo, BleDevice, BleTapLine, BleWire};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(module = "/tests/js/conformance_support.js")]
extern "C" {
    #[wasm_bindgen(js_name = installBluetoothScripted)]
    fn js_install_bluetooth_scripted(board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = uninstallBluetooth)]
    fn js_uninstall_bluetooth() -> Promise;

    #[wasm_bindgen(js_name = hideBluetooth)]
    fn js_hide_bluetooth();

    #[wasm_bindgen(js_name = bluetoothUnavailable)]
    fn js_bluetooth_unavailable() -> Promise;

    #[wasm_bindgen(js_name = bleStatsJson)]
    fn js_ble_stats_json(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = bleSilentDrop)]
    fn js_ble_silent_drop(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = blePhantomDrop)]
    fn js_ble_phantom_drop(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = bleHangNextConnect)]
    fn js_ble_hang_next_connect(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = bleUnauthTimeout)]
    fn js_ble_unauth_timeout(ms: u32) -> Promise;

    #[wasm_bindgen(js_name = bleOutOfRange)]
    fn js_ble_out_of_range(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = bleBackInRange)]
    fn js_ble_back_in_range(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = receivedBytes)]
    fn js_received_bytes(board_id: &str) -> String;

    #[wasm_bindgen(js_name = deliverBytes)]
    fn js_deliver_bytes(board_id: &str, text: &str);

    #[wasm_bindgen(js_name = tick)]
    fn js_tick(ms: u32) -> Promise;
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn a_picked_device_is_connected_present_and_wears_a_ble_endpoint() {
    polyfill_over(&["c6-a"]).await;

    let device = pick().await;

    assert!(device.connected, "the chooser flow connects: {device:?}");
    assert!(device.name.starts_with("LP-"), "{device:?}");
    let present = ble::present_devices();
    assert_eq!(present.len(), 1, "{present:?}");
    assert_eq!(present[0].device_id, device.device_id);
    let info = ble_link_info(&device);
    assert_eq!(info.endpoint.0, format!("ble:{}", device.device_id));
    assert!(info.usb.is_none() && info.serial_number.is_none());

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_line_split_across_notifications_arrives_whole() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;

    // 600 characters: three notifications at 244 B each on the way in.
    let long = "x".repeat(600);
    js_deliver_bytes("c6-a", &format!("{long}\n"));

    let line = wait_for(
        &mut link,
        |event| matches!(event, LinkEvent::Line(line) if line.len() == 600),
    )
    .await;
    assert_eq!(line, Some(LinkEvent::Line(long)));

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_long_line_goes_out_in_awaited_180_byte_writes() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;
    let before = stats("c6-a").await;

    let long = format!("M!{}", "y".repeat(598));
    link.submit(LinkCommand::SendLine(long.clone()));
    for _ in 0..200 {
        if js_received_bytes("c6-a").contains(&format!("{long}\n")) {
            break;
        }
        tick(20).await;
    }

    assert!(
        js_received_bytes("c6-a").ends_with(&format!("{long}\n")),
        "the board received the whole line, in order"
    );
    let after = stats("c6-a").await;
    // 601 bytes → 180 + 180 + 180 + 61.
    assert_eq!(after.writes - before.writes, 4, "{before:?} → {after:?}");
    assert_eq!(after.written - before.written, 601);

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_drop_is_a_departure_and_the_session_reconnects_by_itself() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let mut link = open_link(&device).await;
    let (connects, disconnects) = (edges.0.get(), edges.1.get());

    JsFuture::from(js_ble_out_of_range("c6-a")).await.unwrap();

    let lost = wait_for(&mut link, |event| {
        matches!(event, LinkEvent::Error(error) if error.starts_with("bluetooth link lost"))
    })
    .await;
    assert!(lost.is_some(), "the drop is said in the departure's words");
    assert_eq!(edges.1.get(), disconnects + 1, "one disconnect edge");
    assert!(
        ble::present_devices().is_empty(),
        "a dropped device is not present"
    );

    // Back in range: the held device reconnects with no chooser and no call
    // from us, and says so with a connect edge.
    JsFuture::from(js_ble_back_in_range("c6-a")).await.unwrap();
    for _ in 0..300 {
        if edges.0.get() > connects {
            break;
        }
        tick(20).await;
    }
    assert_eq!(
        edges.0.get(),
        connects + 1,
        "one connect edge on the reconnect"
    );
    assert_eq!(ble::present_devices().len(), 1);

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_drop_the_page_never_heard_is_found_on_the_recheck() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let disconnects = edges.1.get();

    // iOS: the link died while the page was hidden, and no event came.
    JsFuture::from(js_ble_silent_drop("c6-a")).await.unwrap();
    assert!(
        wire.take_errors().unwrap().is_empty(),
        "nothing was heard — which is the defect"
    );

    // Becoming visible is "state unknown": the re-check finds it.
    ble::recheck_all("the page was shown again");

    let errors = wire.take_errors().unwrap();
    assert_eq!(
        errors,
        vec!["bluetooth link lost: dropped while the page was hidden".to_string()]
    );
    assert_eq!(edges.1.get(), disconnects + 1);
    // …and the reconnect starts at once.
    for _ in 0..300 {
        if wire.is_connected() {
            break;
        }
        tick(20).await;
    }
    assert!(wire.is_connected(), "reconnected after the re-check");

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_phantom_drop_is_torn_down_and_the_reconnect_is_a_fresh_link() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let before = stats("c6-a").await;
    assert_eq!(before.link_opens, 1, "the pick opened the board's link: {before:?}");

    // Bluefy (G4, 2026-09-25): the page is told its link is gone while iOS
    // keeps the radio link up. The board never sees a drop.
    JsFuture::from(js_ble_phantom_drop("c6-a")).await.unwrap();
    ble::recheck_all("the page was shown again");
    assert!(
        wire.take_errors()
            .unwrap()
            .iter()
            .any(|error| error.starts_with("bluetooth link lost")),
        "the page reads it as a drop"
    );

    // The drop is made real: the board's side closes…
    let torn = stats("c6-a").await;
    assert_eq!(
        torn.link_closes,
        before.link_closes + 1,
        "the page tore the radio link down: {before:?} → {torn:?}"
    );

    // …so the reconnect is a NEW link, the one the board owes a hello.
    for _ in 0..300 {
        if wire.is_connected() {
            break;
        }
        tick(20).await;
    }
    assert!(wire.is_connected(), "reconnected");
    let after = stats("c6-a").await;
    assert_eq!(
        after.link_opens,
        before.link_opens + 1,
        "a fresh link, not the old one: {before:?} → {after:?}"
    );

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_connect_that_never_settles_fails_by_name() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;
    link.submit(LinkCommand::Close);
    wait_for(&mut link, |event| matches!(event, LinkEvent::Closed { .. })).await;

    JsFuture::from(js_ble_hang_next_connect("c6-a"))
        .await
        .unwrap();
    link.submit(LinkCommand::Open { baud: 0 });

    // The real 10 s bound: ~12 s of polling at most.
    let failed = wait_for_up_to(
        &mut link,
        600,
        |event| matches!(event, LinkEvent::Error(error) if error.contains("timed out after 10 s")),
    )
    .await;
    assert!(failed.is_some(), "a hung connect ends with its reason");

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_closed_link_stays_present_and_reopens_without_the_chooser() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let mut link = open_link(&device).await;
    let disconnects = edges.1.get();
    let before = stats("c6-a").await;

    link.submit(LinkCommand::Close);
    assert!(
        wait_for(&mut link, |event| matches!(event, LinkEvent::Closed { .. }))
            .await
            .is_some()
    );
    assert_eq!(
        edges.1.get(),
        disconnects,
        "our own close is not a departure"
    );
    assert_eq!(ble::present_devices().len(), 1, "still ours, still listed");

    link.submit(LinkCommand::Open { baud: 0 });
    assert!(
        wait_for(&mut link, |event| matches!(event, LinkEvent::Opened { .. }))
            .await
            .is_some()
    );
    let after = stats("c6-a").await;
    assert_eq!(
        after.connects,
        before.connects + 1,
        "one GATT connect, no chooser"
    );

    polyfill_off().await;
}

/// M4's rule, as the polyfill models it from the board's own hello: a link
/// whose hello asks for a login and gets none is closed after 10 s — and
/// Studio hears that as the link being lost, not as silence.
#[wasm_bindgen_test]
async fn an_untrusted_link_that_never_logs_in_is_dropped() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;
    // The rule is 10 s; the suite's runner allows ~20 s for everything, so
    // the timeout is shortened here (the rule's SHAPE is what is pinned).
    JsFuture::from(js_ble_unauth_timeout(300)).await.unwrap();

    js_deliver_bytes(
        "c6-a",
        "M!{\"id\":0,\"msg\":{\"hello\":{\"auth\":{\"required\":true,\"granted\":null}}}}\n",
    );

    let lost = wait_for_up_to(&mut link, 250, |event| {
        matches!(event, LinkEvent::Error(error) if error.starts_with("bluetooth link lost"))
    })
    .await;
    assert!(lost.is_some(), "the board closed the unauthenticated link");

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn a_reset_over_bluetooth_fails_by_name() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;

    link.submit(LinkCommand::RunReset(ResetKind::Normal));

    let outcome = wait_for(&mut link, |event| {
        matches!(event, LinkEvent::ResetOutcome { .. })
    })
    .await;
    assert_eq!(
        outcome,
        Some(LinkEvent::ResetOutcome {
            kind: ResetKind::Normal,
            ok: false
        })
    );

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn the_conversation_io_round_trips_a_request() {
    use lpa_client::ClientIo;
    use lpc_wire::{ClientMessage, ClientRequest, WireServerMessage};

    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let wire = Rc::new(BleWire::new(device.session));
    let tapped = Rc::new(Cell::new(0_u32));
    let tap_count = Rc::clone(&tapped);
    let mut io = BleClientIo::new(
        Rc::clone(&wire),
        Some(Rc::new(move |line: BleTapLine| {
            if matches!(line, BleTapLine::Line(_)) {
                tap_count.set(tap_count.get() + 1);
            }
        })),
    );
    // Drop the scripted greeting so the only line is the answer.
    let _ = wire.take_lines();

    io.send(ClientMessage {
        id: 0x4000_0007,
        msg: ClientRequest::ListLoadedProjects,
    })
    .await
    .expect("sent");
    for _ in 0..200 {
        if js_received_bytes("c6-a").contains("listLoadedProjects") {
            break;
        }
        tick(20).await;
    }
    assert!(js_received_bytes("c6-a").contains("\"listLoadedProjects\""));
    let reply = WireServerMessage::new(
        0x4000_0007,
        lpc_wire::server::ServerMsgBody::ListLoadedProjects {
            projects: Vec::new(),
        },
    );
    js_deliver_bytes(
        "c6-a",
        &format!(
            "M!{}\n",
            lpc_wire::json::to_string(&reply).expect("encodes")
        ),
    );
    let answer: WireServerMessage = io.receive().await.expect("answered");

    assert_eq!(answer.id, 0x4000_0007);
    assert!(tapped.get() >= 1, "the tap carried the line to the fold");

    polyfill_off().await;
}

#[wasm_bindgen_test]
async fn availability_reads_the_browser_not_a_guess() {
    polyfill_over(&["c6-a"]).await;

    let ready = ble::availability().await;
    assert!(ready.supported && ble::is_supported());
    assert_eq!(ready.available, Some(true));

    JsFuture::from(js_bluetooth_unavailable()).await.unwrap();
    assert_eq!(ble::availability().await.available, Some(false));

    js_hide_bluetooth();
    assert!(!ble::is_supported());
    assert!(!ble::availability().await.supported);

    polyfill_off().await;
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// `browser_ble.js`'s session map is module scoped and outlives a test, so
/// every session is forgotten between tests. Sixty-four is a bound, not a
/// budget: ids are minted from 1 and this suite makes a handful.
async fn forget_sessions() {
    for id in 1..=64 {
        let _ = ble::forget(id).await;
    }
}

async fn polyfill_over(boards: &[&str]) {
    forget_sessions().await;
    let _ = JsFuture::from(js_uninstall_bluetooth()).await;
    let ids = Array::new();
    for board in boards {
        ids.push(&JsValue::from_str(board));
    }
    JsFuture::from(js_install_bluetooth_scripted(&ids))
        .await
        .expect("install the Bluetooth polyfill over the scripted door");
}

async fn polyfill_off() {
    forget_sessions().await;
    let _ = JsFuture::from(js_uninstall_bluetooth()).await;
}

async fn pick() -> BleDevice {
    ble::request_device()
        .await
        .expect("requestDevice")
        .expect("the polyfill with no picker resolves the first board")
}

async fn open_link(device: &BleDevice) -> BrowserBleLink {
    let mut link =
        BrowserBleLink::new(Rc::new(BleWire::new(device.session)), ble_link_info(device));
    link.submit(LinkCommand::Open { baud: 921_600 });
    let opened = wait_for(&mut link, |event| matches!(event, LinkEvent::Opened { .. })).await;
    assert!(opened.is_some(), "the link opened");
    link
}

/// Poll the link until an event matches, draining everything before it.
async fn wait_for(
    link: &mut BrowserBleLink,
    matches: impl Fn(&LinkEvent) -> bool,
) -> Option<LinkEvent> {
    wait_for_up_to(link, 250, matches).await
}

async fn wait_for_up_to(
    link: &mut BrowserBleLink,
    polls: u32,
    matches: impl Fn(&LinkEvent) -> bool,
) -> Option<LinkEvent> {
    for _ in 0..polls {
        while let Some(event) = link.poll_event() {
            if matches(&event) {
                return Some(event);
            }
        }
        tick(20).await;
    }
    None
}

async fn tick(ms: u32) {
    let _ = JsFuture::from(js_tick(ms)).await;
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
            ble::install_ble_events(
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

#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    written: u32,
    writes: u32,
    connects: u32,
    /// The board's side of the link opening and closing.
    link_opens: u32,
    link_closes: u32,
}

async fn stats(board: &str) -> Stats {
    let json = JsFuture::from(js_ble_stats_json(board))
        .await
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    let field = |name: &str| -> u32 {
        json.split(&format!("\"{name}\":"))
            .nth(1)
            .and_then(|rest| rest.split([',', '}']).next())
            .and_then(|number| number.trim().parse().ok())
            .unwrap_or(0)
    };
    Stats {
        written: field("written"),
        writes: field("writes"),
        connects: field("connects"),
        link_opens: field("linkOpens"),
        link_closes: field("linkCloses"),
    }
}
