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
//! | lp-link comes up over the GATT subset; one frame per notification | [`a_link_comes_up_and_the_hello_arrives_one_frame_per_notification`] |
//! | one frame per write, never a long write; a request arrives whole | [`a_request_goes_out_one_frame_per_write`] |
//! | each frame is its own buffer (Bluefy writes a view's whole buffer) | [`a_large_request_survives_a_browser_that_writes_a_views_whole_buffer`] |
//! | a drop is a departure, then a reconnect with no gesture | [`a_drop_is_a_departure_and_the_session_reconnects_by_itself`] |
//! | a drop the page never heard is found by the visibility re-check | [`a_drop_the_page_never_heard_is_found_on_the_recheck`] |
//! | a drop tears the radio link down, so the reconnect is a fresh link | [`a_phantom_drop_is_torn_down_and_the_reconnect_is_a_fresh_link`] |
//! | every connect is bounded | [`a_connect_that_never_settles_fails_by_name`] |
//! | close is ours: the session stays, reopening needs no chooser | [`a_closed_link_stays_present_and_reopens_without_the_chooser`] |
//! | no reset over GATT | [`a_reset_over_bluetooth_fails_by_name`] |
//! | M4: an untrusted link that never logs in is closed in 10 s | [`an_untrusted_link_that_never_logs_in_is_dropped`] |
//! | a borrowing conversation's io | [`the_conversation_io_round_trips_a_request`] |
//! | availability, for the add slot's copy | [`availability_reads_the_browser_not_a_guess`] |
//! | channel 3 (the update) both ways, once the board announced it (M7 P12) | [`the_update_channel_flows_both_ways_once_the_board_announces_it`] |
//! | nothing on channel 3 to a board that never announced it (DS9) | [`nothing_goes_out_on_channel_3_to_a_board_that_never_announced_it`] |
//! | `?ble=emu`: a board's reset is a GATT drop, then a reconnect | [`a_board_reset_is_a_gatt_drop_and_the_session_reconnects_by_itself`] |
//! | `?ble=emu`: a link restart with no reset stays one connection | [`a_link_restart_without_a_reset_stays_one_connection`] |
//!
//! The link tests (the two above and the conversation io) run a board-side
//! `lp_link::Link` here in Rust, on `LinkConfig::usb()`'s STREAM framing —
//! what the scripted door stands in for is the emulated board's USB link,
//! and the polyfill translates Studio's datagrams to and from it (its header
//! says how). So they prove Studio's datagram link end and the polyfill's
//! translation, never a board's radio-link code.
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
use lpa_link::device_link::link_note::UPDATE_NOT_ANNOUNCED_NOTE;
use lpa_link::device_link::wire_reader::WireRead;
use lpa_link::providers::browser_ble::{self as ble, BleClientIo, BleDevice, BleTapLine, BleWire};
use lpc_update::BoardManifest;
use lpc_wire::lp_link::{
    CH_PROTO, CH_UPDATE, Link as BoardLink, LinkConfig, LinkEvent as BoardEvent, SelectiveRepeat,
};
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

    #[wasm_bindgen(js_name = bleWholeBufferWrites)]
    fn js_ble_whole_buffer_writes(on: bool) -> Promise;

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

    #[wasm_bindgen(js_name = takeReceivedRaw)]
    fn js_take_received_raw(board_id: &str) -> js_sys::Uint8Array;

    #[wasm_bindgen(js_name = deliverRawBytes)]
    fn js_deliver_raw_bytes(board_id: &str, bytes: &[u8]);

    #[wasm_bindgen(js_name = rebootBehindOurBack)]
    fn js_reboot_behind_our_back(board_id: &str);

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
    assert!(
        info.carries_update_channel,
        "a Bluetooth link carries lp-link's update channel (M7 P12)"
    );

    polyfill_off().await;
}

/// lp-link comes up over the GATT subset — the board's SYNs as
/// notifications, Studio's as writes — and the board's hello arrives as one
/// whole message. Every notification the page heard was one frame, no more
/// than one payload plus header and CRC.
#[wasm_bindgen_test]
async fn a_link_comes_up_and_the_hello_arrives_one_frame_per_notification() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let mut bench = LinkBench::new("c6-a");

    let reads = bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;

    assert_eq!(hello_count(&reads), 1, "{reads:?}");
    assert!(wire.is_link_up(), "the handshake finished");
    let seen = stats("c6-a").await;
    assert!(seen.notifications >= 2, "SYN and hello at least: {seen:?}");
    assert!(
        seen.largest_notification <= MAX_FRAME_BYTES,
        "a notification of {} B is more than one frame: {seen:?}",
        seen.largest_notification
    );

    polyfill_off().await;
}

/// A request larger than many frames goes out as one GATT write per frame —
/// each at most one payload plus header and CRC, never an ATT long write —
/// and the board's link reassembles it whole.
#[wasm_bindgen_test]
async fn a_request_goes_out_one_frame_per_write() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let mut bench = LinkBench::new("c6-a");
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    let before = stats("c6-a").await;

    let json = big_request(41, 2_000);
    wire.send_client_json(&json).expect("the link takes it");
    bench.exchange_until(&wire, |_| bench_saw_request(41)).await;

    let seen = BOARD.with(|board| board.borrow().requests.clone());
    assert_eq!(seen, vec![(41, json.clone())], "the board read it whole");
    let after = stats("c6-a").await;
    let writes = after.writes - before.writes;
    assert!(
        (after.written - before.written) as usize > json.len(),
        "the request and its framing went out: {before:?} → {after:?}"
    );
    assert!(
        writes as usize >= json.len() / 180,
        "{} B in only {writes} writes: {before:?} → {after:?}",
        json.len()
    );
    assert!(
        after.largest_write <= MAX_FRAME_BYTES,
        "a write of {} B is more than one frame: {after:?}",
        after.largest_write
    );

    polyfill_off().await;
}

/// Bluefy, 2026-10-02: pinning a palette (a > 512 B request) dropped the
/// link every time. Bluefy writes a typed-array view's whole underlying
/// buffer, so a write cut as a view carried far more than its own bytes; the
/// board refused it as a long write and the page tore the link down. A frame
/// from Rust is a view onto wasm memory, so every write must be a buffer of
/// its own: under the same quirk, a request of many frames arrives whole,
/// every write is one frame, and the link stays up.
#[wasm_bindgen_test]
async fn a_large_request_survives_a_browser_that_writes_a_views_whole_buffer() {
    polyfill_over(&["c6-a"]).await;
    JsFuture::from(js_ble_whole_buffer_writes(true))
        .await
        .unwrap();
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let mut bench = LinkBench::new("c6-a");
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    let before = stats("c6-a").await;

    let json = big_request(42, 2_000);
    wire.send_client_json(&json).expect("the link takes it");
    bench.exchange_until(&wire, |_| bench_saw_request(42)).await;

    let seen = BOARD.with(|board| board.borrow().requests.clone());
    assert_eq!(seen, vec![(42, json.clone())], "the board read it whole");
    let after = stats("c6-a").await;
    assert!(
        after.largest_write <= MAX_FRAME_BYTES,
        "a write of {} B is more than one frame: {after:?}",
        after.largest_write
    );
    assert_eq!(after.link_closes, before.link_closes, "the link stayed up");
    assert!(wire.is_link_up());

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

/// The 2026-10-05 desk check (a board restarted under the editor) and
/// `walk-ble-emu`'s `drop-back`: the departure sweep detached the dropped
/// link before its pump read the drop, so `bluetooth link lost` waited in
/// the session — and the link the reconnect attached read it on open and
/// closed at once. With the session present again no edge came, nothing
/// re-attached the board, and the editor's hold ran out over a board that
/// was connected and saying hello. A link opened on the reconnect hears its
/// own connection, not the last one's end.
#[wasm_bindgen_test]
async fn a_link_opened_on_the_reconnect_does_not_read_the_old_links_loss() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let link = open_link(&device).await;
    let connects = edges.0.get();

    // The drop, and the departure sweep detaching the link before its pump
    // ever ran again: the loss is never drained by the link it ended.
    JsFuture::from(js_ble_out_of_range("c6-a")).await.unwrap();
    drop(link);
    JsFuture::from(js_ble_back_in_range("c6-a")).await.unwrap();
    for _ in 0..300 {
        if edges.0.get() > connects {
            break;
        }
        tick(20).await;
    }
    assert_eq!(edges.0.get(), connects + 1, "reconnected by itself");

    // The connect edge's sweep attaches a NEW link on the same session.
    let mut link = open_link(&device).await;
    let lost = wait_for_up_to(&mut link, 50, |event| {
        matches!(event, LinkEvent::Error(error) if error.starts_with("bluetooth link lost"))
            || matches!(event, LinkEvent::Closed { .. })
    })
    .await;
    assert_eq!(lost, None, "the new link read the old link's loss");
    assert!(link.is_open(), "the new link stays open");

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
    assert_eq!(
        before.link_opens, 1,
        "the pick opened the board's link: {before:?}"
    );

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
    let mut bench = LinkBench::new("c6-a");
    // The hello is the model's; the io is built after it, as a borrow is.
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
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

    io.send(ClientMessage {
        id: 0x4000_0007,
        msg: ClientRequest::ListLoadedProjects,
    })
    .await
    .expect("sent");
    // Until the board has read the request and the page's link has
    // acknowledged the whole answer — which leaves it waiting, read, for the
    // io, and nothing in this loop drains it.
    bench
        .run_until(|board| {
            board.requests.iter().any(|(id, _)| *id == 0x4000_0007) && board.link.is_idle()
        })
        .await;
    let answer: WireServerMessage = io.receive().await.expect("answered");

    assert_eq!(answer.id, 0x4000_0007);
    assert!(tapped.get() >= 1, "the tap carried the message to the fold");

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
/// M7 P12: a board that announces the update channel (a core-only board's
/// `M` on link-up) is heard on it — its manifest as `UpdateFacts`, then the
/// bytes as `Update` — and what Studio sends on it reaches the board's
/// channel 3, through the polyfill's framing like every other frame.
#[wasm_bindgen_test]
async fn the_update_channel_flows_both_ways_once_the_board_announces_it() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;
    let mut bench = LinkBench::with_board("c6-a", BoardDouble::core_only(0xB0A2_0003));

    let facts = bench
        .drive_link_until(&mut link, |event| {
            matches!(event, LinkEvent::UpdateFacts(_))
        })
        .await;
    assert!(
        matches!(&facts, Some(LinkEvent::UpdateFacts(f)) if f.version.as_deref() == Some("2026.10.06-1")),
        "the board's manifest, mirrored: {facts:?}"
    );
    let update = bench
        .drive_link_until(&mut link, |event| matches!(event, LinkEvent::Update(_)))
        .await;
    assert!(
        matches!(&update, Some(LinkEvent::Update(bytes)) if bytes.first() == Some(&b'M')),
        "the manifest's bytes, for the update host: {update:?}"
    );

    link.submit(LinkCommand::SendUpdate(b"Q\x01".to_vec()));
    bench.run_until(|board| !board.updates.is_empty()).await;
    assert_eq!(
        BOARD.with(|board| board.borrow().updates.clone()),
        vec![b"Q\x01".to_vec()],
        "Studio's message reached the board's channel 3"
    );

    polyfill_off().await;
}

/// DS9 over Bluetooth: a board whose hello carries no `firmware` and that
/// never sent an update message gets nothing on channel 3 — the link says
/// why — so a pre-update board's link is never stalled on a frame it would
/// never acknowledge.
#[wasm_bindgen_test]
async fn nothing_goes_out_on_channel_3_to_a_board_that_never_announced_it() {
    polyfill_over(&["c6-a"]).await;
    let device = pick().await;
    let mut link = open_link(&device).await;
    let bench = LinkBench::new("c6-a");
    let hello = bench
        .drive_link_until(&mut link, |event| matches!(event, LinkEvent::Frame(_)))
        .await;
    assert!(hello.is_some(), "the board's hello");

    link.submit(LinkCommand::SendUpdate(b"Q\x01".to_vec()));
    let note = bench
        .drive_link_until(
            &mut link,
            |event| matches!(event, LinkEvent::WireNote(note) if note == UPDATE_NOT_ANNOUNCED_NOTE),
        )
        .await;
    assert!(note.is_some(), "the link says why nothing went out");
    for _ in 0..20 {
        bench.round();
        tick(10).await;
    }
    assert!(
        BOARD.with(|board| board.borrow().updates.is_empty()),
        "nothing reached the board's channel 3"
    );

    polyfill_off().await;
}

/// `?ble=emu` models a board's reset the way a real board's radio has it
/// (M7 P12): the emulated board's USB link survives a reset, but a reset
/// board drops its GATT connection. When the chip reboots under a live
/// link (the scripted door's cycle count goes back to zero, and the board's
/// fresh link sends its SYN), the polyfill drops the connection — a
/// departure in the provider's words — and the held device reconnects with
/// no gesture.
#[wasm_bindgen_test]
async fn a_board_reset_is_a_gatt_drop_and_the_session_reconnects_by_itself() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let mut bench = LinkBench::new("c6-a");
    let reads = bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    assert_eq!(hello_count(&reads), 1, "{reads:?}");
    let (connects, disconnects) = (edges.0.get(), edges.1.get());
    let before = stats("c6-a").await;

    // The chip reboots; its new link starts over with a new nonce.
    js_reboot_behind_our_back("c6-a");
    BOARD.with(|board| *board.borrow_mut() = BoardDouble::new(0xB0A2_0002));
    for _ in 0..300 {
        if edges.1.get() > disconnects && edges.0.get() > connects {
            break;
        }
        // The board's side of the link closes with the drop: nothing more
        // to hand it until the reconnect opens a new one.
        if edges.1.get() == disconnects {
            bench.round();
        }
        tick(10).await;
    }

    assert_eq!(
        edges.1.get(),
        disconnects + 1,
        "one disconnect edge: the reset"
    );
    assert_eq!(
        edges.0.get(),
        connects + 1,
        "one connect edge: the reconnect"
    );
    let seen = stats("c6-a").await;
    assert_eq!(seen.reset_drops, 1, "{seen:?}");
    assert!(
        seen.connects > before.connects,
        "the reconnect is a new connection: {seen:?}"
    );
    assert_eq!(
        seen.link_closes, before.link_closes,
        "the board's side stays open across its reset (it already started a new session): {seen:?}"
    );
    let errors = wire.take_errors().unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|error| error.starts_with("bluetooth link lost")),
        "the drop is said in the departure's words: {errors:?}"
    );

    polyfill_off().await;
}

/// …and only a reset is: the board's link restarting by itself (the chip
/// still running) is a reset INSIDE one connection, which the page's link
/// reads as one — no drop, the next hello on the same connection.
#[wasm_bindgen_test]
async fn a_link_restart_without_a_reset_stays_one_connection() {
    polyfill_over(&["c6-a"]).await;
    let edges = edges();
    let device = pick().await;
    let wire = BleWire::new(device.session);
    let mut bench = LinkBench::new("c6-a");
    bench
        .exchange_until(&wire, |reads| hello_count(reads) >= 1)
        .await;
    let disconnects = edges.1.get();

    BOARD.with(|board| *board.borrow_mut() = BoardDouble::new(0xB0A2_0004));
    let reads = bench
        .exchange_until(&wire, |reads| {
            reads
                .iter()
                .any(|read| matches!(read, WireRead::LinkReset(_)))
                && hello_count(reads) >= 1
        })
        .await;

    assert!(
        reads
            .iter()
            .any(|read| matches!(read, WireRead::LinkReset(_))),
        "a reset inside the connection: {reads:?}"
    );
    assert_eq!(hello_count(&reads), 1, "the new session's hello: {reads:?}");
    assert_eq!(edges.1.get(), disconnects, "no drop");
    assert_eq!(stats("c6-a").await.reset_drops, 0);
    assert!(wire.is_connected());

    polyfill_off().await;
}

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

/// The most one frame may be on the air: a 180 B payload (the smaller of
/// the two ends' SYNs here, Studio's `ble()`) plus a 4-byte header and a
/// 4-byte CRC.
const MAX_FRAME_BYTES: u32 = 180 + 8;

/// The page's link end (the production `ble_link_port` loop, reached
/// through a [`BleWire`]) against [`BOARD`] at the other end of the
/// polyfill and the scripted door.
struct LinkBench {
    board: String,
    reads: Vec<WireRead>,
}

impl LinkBench {
    fn new(board: &str) -> Self {
        Self::with_board(board, BoardDouble::new(0xB0A2_0001))
    }

    /// A bench whose board end is `double`.
    fn with_board(board: &str, double: BoardDouble) -> Self {
        BOARD.with(|current| *current.borrow_mut() = double);
        Self {
            board: board.to_string(),
            reads: Vec::new(),
        }
    }

    /// Run the board's end while the model's LINK drains the page's (its
    /// pump: reads, channel 3, notes), until an event matches; bounded by
    /// rounds, never by a clock.
    async fn drive_link_until(
        &self,
        link: &mut BrowserBleLink,
        matches: impl Fn(&LinkEvent) -> bool,
    ) -> Option<LinkEvent> {
        for _ in 0..300 {
            self.round();
            tick(10).await;
            while let Some(event) = link.poll_event() {
                if matches(&event) {
                    return Some(event);
                }
            }
        }
        None
    }

    /// Run the board's end, draining the page's, until `done` holds for what
    /// the page read (bounded by rounds, never by a clock), and hand back
    /// everything read.
    async fn exchange_until(
        &mut self,
        wire: &BleWire,
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

    /// Run the board's end — and NOT drain the page's — until `done` holds
    /// for the board.
    async fn run_until(&mut self, done: impl Fn(&BoardDouble) -> bool) {
        for _ in 0..300 {
            self.round();
            tick(10).await;
            if BOARD.with(|board| done(&board.borrow())) {
                break;
            }
        }
    }

    /// One turn of the board: read what the page wrote (stream-wrapped by the
    /// polyfill), answer, and hand its frames to the door.
    fn round(&self) {
        let written = js_take_received_raw(&self.board).to_vec();
        let out = BOARD.with(|board| {
            let mut board = board.borrow_mut();
            board.on_bytes(&written);
            board.frames_now()
        });
        if !out.is_empty() {
            js_deliver_raw_bytes(&self.board, &out);
        }
    }
}

thread_local! {
    /// The board end of the current link test.
    static BOARD: std::cell::RefCell<BoardDouble> =
        std::cell::RefCell::new(BoardDouble::new(0xB0A2_0001));
}

/// Whether the board has read request `id`.
fn bench_saw_request(id: u64) -> bool {
    BOARD.with(|board| board.borrow().requests.iter().any(|(seen, _)| *seen == id))
}

/// The emulated board's USB end of the link, as the shipped firmware runs
/// it: stream framing, the hello on every `Up`, JSON replies.
struct BoardDouble {
    link: BoardLink<SelectiveRepeat>,
    /// Every request read, by id, as the JSON it arrived as.
    requests: Vec<(u64, String)>,
    /// A core-only board's manifest, sent as `M` on channel 3 on every
    /// `Up` (its announcement of the update channel; it says no hello).
    manifest: Option<BoardManifest>,
    /// Every channel-3 message read.
    updates: Vec<Vec<u8>>,
}

impl BoardDouble {
    fn new(nonce: u32) -> Self {
        Self {
            link: BoardLink::new(LinkConfig::usb(), nonce),
            requests: Vec::new(),
            manifest: None,
            updates: Vec::new(),
        }
    }

    /// A split image waiting for its engine: no hello, its `M` instead.
    fn core_only(nonce: u32) -> Self {
        Self {
            manifest: Some(board_manifest()),
            ..Self::new(nonce)
        }
    }

    fn now() -> u64 {
        (js_sys::Date::now() * 1_000.0) as u64
    }

    fn on_bytes(&mut self, bytes: &[u8]) {
        self.link.on_bytes(Self::now(), bytes);
        while let Some(event) = self.link.recv() {
            match event {
                BoardEvent::Up { .. } => match &self.manifest {
                    Some(manifest) => {
                        let mut m = vec![b'M'];
                        m.extend_from_slice(&manifest.to_json());
                        self.link.send(CH_UPDATE, &m).expect("board send");
                    }
                    None => self.send(&hello()),
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
                    let json = String::from_utf8(data.clone()).expect("a JSON request");
                    self.requests.push((request.id, json));
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

    /// A reply as a JSON payload (a `{`-tagged proto message).
    fn send(&mut self, message: &lpc_wire::WireServerMessage) {
        let json = lpc_wire::json::to_string(message).expect("json");
        self.link
            .send(CH_PROTO, json.as_bytes())
            .expect("board send");
    }

    /// Every frame the board's link has to write now, as one chunk of stream.
    fn frames_now(&mut self) -> Vec<u8> {
        let now = Self::now();
        let mut out = Vec::new();
        while let Some(frame) = self.link.poll_transmit(now) {
            out.extend_from_slice(frame);
        }
        out
    }
}

fn hello() -> lpc_wire::WireServerMessage {
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
            // No pack format: the page's link stays JSON, so every read here
            // is the board's plain JSON.
            pack_format: 0,
            auth: lpc_wire::HelloAuth::TRUSTED,
            firmware: None,
        }),
    )
}

/// A core-only board's manifest (the shape `link_port_service`'s own tests
/// use).
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

/// A request carrying `bytes` of file data: many frames' worth.
fn big_request(id: u64, bytes: usize) -> String {
    lpc_wire::json::to_string(&lpc_wire::ClientMessage {
        id,
        msg: lpc_wire::ClientRequest::Filesystem(lpc_wire::server::FsRequest::Write {
            path: "/big.txt".into(),
            data: vec![b'z'; bytes],
        }),
    })
    .expect("json")
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
    largest_write: u32,
    notifications: u32,
    largest_notification: u32,
    connects: u32,
    /// The board's side of the link opening and closing.
    link_opens: u32,
    link_closes: u32,
    /// Board resets the polyfill turned into GATT drops.
    reset_drops: u32,
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
        largest_write: field("largestWrite"),
        notifications: field("notifications"),
        largest_notification: field("largestNotification"),
        connects: field("connects"),
        link_opens: field("linkOpens"),
        link_closes: field("linkCloses"),
        reset_drops: field("resetDrops"),
    }
}
