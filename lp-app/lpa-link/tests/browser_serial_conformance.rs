//! The Web Serial JS layer, executed in a real Chrome against a virtual USB
//! bus — the harness `docs/debt/web-serial-js-untestable.md` has been asking
//! for since 2026-07-10.
//!
//! Every test here drives the SHIPPED JS: `browser_serial.js` and, through
//! its own dynamic import, `browser_esp32_device_controller.js`. Nothing in
//! this file re-implements either. Under them sits the polyfill
//! (`/lpa-link/virtual_serial.js` + `/lpa-link/emulator_port.js`), and under
//! *that* sits a scripted door (`tests/js/conformance_support.js`) standing in
//! for `lp-cli emu serve` — so the suite is hermetic: no server, no sockets,
//! no firmware, no timers.
//!
//! The same assertions run against a live `lp-cli emu serve` through
//! `just lpa-link-browser-test-live`, which sets `LP_EMU_SERVE_URL`. That half
//! is deliberately NOT a CI job (PD9).
//!
//! **The four exit criteria and the test that pins each:**
//!
//! | criterion | test | what it pins |
//! |---|---|---|
//! | session-id stability across open/close/re-enumerate | [`session_ids_are_stable_across_open_and_close`], [`session_ids_are_stable_across_a_re_enumeration`] | `browser_serial.js:89-102` (`sessionForPort`), `:58-84` (`adoptReenumeratedPorts`) |
//! | close-vs-release semantics | [`a_closed_port_keeps_its_session_and_a_forgotten_one_does_not`] | `browser_serial.js:140-157` (`closePort` keeps the entry), `:159-181` (`forgetPort` deletes it) |
//! | read-pump error paths | [`the_read_pump_reports_a_lost_device_and_the_port_reopens`] | `browser_esp32_device_controller.js` (`readPump`) |
//!
//! **The link (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, P4).** The
//! pump hands Rust BYTES, and since `WIRE_PROTO_VERSION` 30 those bytes are an
//! lp-link: each open port keeps one lp-link end (`LinkPortService`) and a
//! loop in `browser_serial.rs`, and writes its frames with `writeBytes`. The
//! link tests drive that production registry through
//! `providers::browser_serial_esp32::web_serial_link` against a board-side
//! `lp_link::Link` in Rust, on the scripted door:
//! [`the_link_comes_up_and_the_boards_hello_arrives`],
//! [`a_request_is_one_link_message_and_its_answer_comes_back_packed`],
//! [`a_link_frame_split_across_reads_is_read_whole`],
//! [`console_text_between_link_frames_stays_lines`] and
//! [`a_board_restart_is_a_link_reset_then_a_new_hello`] (plan D9), and
//! [`a_request_in_flight_when_the_board_restarts_fails_the_lens_at_once`]
//! takes D9 up to the editor lens's io.
//!
//! **The classic ESP32 (plan `lp2025/2026-09-28-2015-classic-uart-on-lp-link`,
//! P4).** Each of those six claims runs twice: once against a C6 on native
//! USB, once against a classic ESP32 behind its CH340K — the port enumerating
//! as `1a86:7522` (`presentBehindBridge`, on the port object only), the board
//! double on the classic's own cut of `uart()`. The bench checks the port's
//! link picked the board's preset from the vendor id
//! (`usb_vendors::link_config_for_usb_vendor`) every time:
//! [`a_classic_link_comes_up_and_its_hello_arrives`],
//! [`a_classic_request_is_one_link_message_and_its_answer_comes_back_packed`],
//! [`a_classic_link_frame_split_across_reads_is_read_whole`],
//! [`a_classics_console_text_between_link_frames_stays_lines`],
//! [`a_classic_restart_is_a_link_reset_then_a_new_hello`] and
//! [`a_request_in_flight_when_a_classic_restarts_fails_the_lens_at_once`].
//! | the flash bridge's port acquisition | [`the_flash_bridge_acquires_the_live_generation`] | `browser_serial.js:195-213` (`getPort`'s adoption pass) |
//!
//! **What re-enumerates, since plan two M5:** the CABLE, and nothing else.
//! A chip reset — the `reset` verb, `download-mode`, or the DTR/RTS dance the
//! emulator decodes into one — leaves the `SerialPort` object alone, because
//! on this part the USB-Serial-JTAG controller shares silicon with the CPU it
//! resets. [`a_chip_reset_does_not_re_enumerate`] and
//! [`a_reboot_behind_our_back_is_noticed_and_does_not_re_enumerate`] pin both
//! halves of that; the three tests that used to drive a re-enumeration from a
//! reset now drive it from a replug, and say so.
//!
//! No assertion here is about a duration: an agent-driven hidden tab is
//! throttled to ~1 Hz, so everything waits on an event or an await.

#![cfg(all(target_arch = "wasm32", feature = "browser-serial-esp32"))]

use js_sys::{Array, Function, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lpa_client::ClientIo;
use lpa_link::LinkManagementEventSink;
use lpa_link::device_link::link_port_service::LinkPortService;
use lpa_link::device_link::wire_reader::{ReadFrame, WireRead};
use lpa_link::providers::browser_serial_esp32::{
    BrowserSerialEsp32Provider, LensTapLine, web_serial_link,
};
use lpc_wire::lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
use wasm_bindgen_futures::spawn_local;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// One extern block, on one support module, which loads the SHIPPED JS over
// HTTP (`/provider/browser_serial.js`, `/provider/browser_esp32_flash.js`,
// `/lpa-link/browser_esp32_device_controller.js`) from the roots
// `scripts/wasm-serial-test-runner.sh` serves.
//
// Declaring `#[wasm_bindgen(module = "…/browser_serial.js")]` here instead
// would be two things at once: a duplicate-symbol link error against
// `browser_serial.rs`'s own externs, which the library already brings into
// this binary; and — were it resolved — a SECOND copy of the module-scoped
// session map that is the thing under test. `mod.rs` exports three of those
// twelve functions and may not grow a line of code (the plan's inviolable
// invariant), so the wrappers live in the support module.
// ---------------------------------------------------------------------------
#[wasm_bindgen(module = "/tests/js/conformance_support.js")]
extern "C" {
    #[wasm_bindgen(js_name = serialIsSupported)]
    fn js_is_supported() -> Promise;

    #[wasm_bindgen(js_name = flashBridgeIsSupported)]
    fn js_flash_bridge_is_supported() -> Promise;

    #[wasm_bindgen(js_name = installSerialEvents)]
    fn js_install_serial_events(on_connect: &Function, on_disconnect: &Function) -> Promise;

    #[wasm_bindgen(js_name = getGrantedPorts)]
    fn js_get_granted_ports() -> Promise;

    #[wasm_bindgen(js_name = requestPortSession)]
    fn js_request_port() -> Promise;

    #[wasm_bindgen(js_name = openPort)]
    fn js_open_port(id: u32, baud_rate: u32, reset: bool, reset_kind: &str) -> Promise;

    #[wasm_bindgen(js_name = closePort)]
    fn js_close_port(id: u32) -> Promise;

    #[wasm_bindgen(js_name = forgetPort)]
    fn js_forget_port(id: u32) -> Promise;

    #[wasm_bindgen(js_name = releasePort)]
    fn js_release_port(id: u32) -> Promise;

    #[wasm_bindgen(js_name = getPortObject)]
    fn js_get_port(id: u32) -> Promise;

    #[wasm_bindgen(js_name = takeBytes)]
    fn js_take_bytes(id: u32) -> Promise;

    #[wasm_bindgen(js_name = takeErrors)]
    fn js_take_errors(id: u32) -> Promise;

    #[wasm_bindgen(js_name = writeBytes)]
    fn js_write_bytes(id: u32, bytes: &[u8]) -> Promise;

    #[wasm_bindgen(js_name = installScripted)]
    fn js_install_scripted(board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = installLive)]
    fn js_install_live(base_url: &str, board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = installTab)]
    fn js_install_tab(module_url: &str, board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = uninstallShim)]
    fn js_uninstall_shim() -> Promise;

    #[wasm_bindgen(js_name = busBoardIds)]
    fn js_bus_board_ids() -> Promise;

    #[wasm_bindgen(js_name = controlLog)]
    fn js_control_log(board_id: &str) -> String;

    #[wasm_bindgen(js_name = receivedBytes)]
    fn js_received_bytes(board_id: &str) -> String;

    #[wasm_bindgen(js_name = takeReceivedRaw)]
    fn js_take_received_raw(board_id: &str) -> js_sys::Uint8Array;

    #[wasm_bindgen(js_name = rebootCount)]
    fn js_reboot_count(board_id: &str) -> i32;

    #[wasm_bindgen(js_name = deliverBytes)]
    fn js_deliver_bytes(board_id: &str, text: &str);

    #[wasm_bindgen(js_name = deliverRawBytes)]
    fn js_deliver_raw_bytes(board_id: &str, bytes: &[u8]);

    #[wasm_bindgen(js_name = dropByteChannel)]
    fn js_drop_byte_channel(board_id: &str);

    #[wasm_bindgen(js_name = resetOverControlChannel)]
    fn js_reset_over_control_channel(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = replugOverTheCable)]
    fn js_replug_over_the_cable(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = rebootsNoticed)]
    fn reboots_seen(board_id: &str) -> u32;

    #[wasm_bindgen(js_name = rebootBehindOurBack)]
    fn js_reboot_behind_our_back(board_id: &str);

    #[wasm_bindgen(js_name = pokeControlChannel)]
    fn js_poke_control_channel(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = livePortFor)]
    fn js_live_port_for(board_id: &str) -> Promise;

    #[wasm_bindgen(js_name = presentBehindBridge)]
    fn js_present_behind_bridge(board_id: &str, usb_vendor_id: u16, usb_product_id: u16)
    -> Promise;

    #[wasm_bindgen(js_name = portReadableIsNull)]
    fn js_port_readable_is_null(port: &JsValue) -> bool;

    #[wasm_bindgen(js_name = portWritableIsNull)]
    fn js_port_writable_is_null(port: &JsValue) -> bool;

    #[wasm_bindgen(js_name = portInfoJson)]
    fn js_port_info_json(port: &JsValue) -> String;

    #[wasm_bindgen(js_name = labelForPortOf)]
    fn js_label_for_port_of(port: &JsValue) -> Promise;

    #[wasm_bindgen(js_name = serialShadowingReport)]
    fn js_serial_shadowing_report() -> Promise;

    #[wasm_bindgen(js_name = canShadowNavigatorSerial)]
    fn js_can_shadow_navigator_serial() -> String;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(text: &str);
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// `browser_serial.js`'s session map is module scoped and outlives a test, so
/// every session is revoked between tests: a leftover session whose port is no
/// longer enumerated is a dead generation, and the next test's
/// `getGrantedPorts()` would adopt its ports into it. Sixty-four is a bound,
/// not a budget — ids are minted from 1 and this suite makes a handful.
async fn drain_sessions() {
    for id in 1..=64 {
        let _ = JsFuture::from(js_forget_port(id)).await;
    }
}

/// Install the shim over a scripted door holding `boards`.
///
/// **Three backings, one set of assertions.** The default is the scripted
/// door — hermetic, and what CI runs. `LP_EMU_SERVE_URL` points the same
/// claims at a real `lp-cli emu serve` over a socket
/// (`just lpa-link-browser-test-live`). `LP_EMU_TAB_MODULE` points them at
/// the tab backing: one Worker per board, holding the emulator's own wasm,
/// with no server anywhere (`just lpa-link-browser-test-tab`). Both of the
/// latter are local recipes and never CI (plan two PD9).
async fn shim_over(boards: &[&str]) {
    drain_sessions().await;
    let _ = JsFuture::from(js_uninstall_shim()).await;
    let ids = Array::new();
    for board in boards {
        ids.push(&JsValue::from_str(board));
    }
    match (
        option_env!("LP_EMU_SERVE_URL"),
        option_env!("LP_EMU_TAB_MODULE"),
    ) {
        (Some(url), _) if !url.is_empty() => {
            JsFuture::from(js_install_live(url, &ids))
                .await
                .expect("install the shim over a live `lp-cli emu serve`");
        }
        (_, Some(module)) if !module.is_empty() => {
            JsFuture::from(js_install_tab(module, &ids))
                .await
                .expect("install the shim over boards hosted in this tab");
        }
        _ => {
            JsFuture::from(js_install_scripted(&ids))
                .await
                .expect("install the shim over the scripted door");
        }
    }
}

async fn shim_off() {
    drain_sessions().await;
    let _ = JsFuture::from(js_uninstall_shim()).await;
}

/// The board ids the installed bus actually holds — the scripted names in CI,
/// whatever `emu serve` was given in the live half.
async fn board_ids() -> Vec<String> {
    let value = JsFuture::from(js_bus_board_ids())
        .await
        .expect("bus boards");
    value
        .as_string()
        .unwrap_or_default()
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Session {
    id: u32,
    label: String,
    usb_vendor_id: Option<u32>,
    usb_product_id: Option<u32>,
}

async fn granted_sessions() -> Vec<Session> {
    let value = JsFuture::from(js_get_granted_ports())
        .await
        .expect("getGrantedPorts()");
    Array::from(&value)
        .iter()
        .map(|entry| Session {
            id: number(&entry, "id").expect("session id") as u32,
            label: Reflect::get(&entry, &JsValue::from_str("label"))
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_default(),
            usb_vendor_id: number(&entry, "usbVendorId").map(|value| value as u32),
            usb_product_id: number(&entry, "usbProductId").map(|value| value as u32),
        })
        .collect()
}

fn number(value: &JsValue, key: &str) -> Option<f64> {
    Reflect::get(value, &JsValue::from_str(key)).ok()?.as_f64()
}

fn ids(sessions: &[Session]) -> Vec<u32> {
    sessions.iter().map(|session| session.id).collect()
}

async fn live_port(board_id: &str) -> JsValue {
    JsFuture::from(js_live_port_for(board_id))
        .await
        .expect("the live port for a board")
}

async fn open_port(id: u32, reset: bool) -> Result<JsValue, JsValue> {
    JsFuture::from(js_open_port(id, 115_200, reset, "normal")).await
}

/// Whether this build points at a live `lp-cli emu serve` rather than the
/// scripted door. Set by `scripts/emu/browser-conformance-live.sh`.
fn live_backing() -> bool {
    option_env!("LP_EMU_SERVE_URL").is_some_and(|url| !url.is_empty())
}

/// Whether this build points at boards hosted in this tab. Set by
/// `just lpa-link-browser-test-tab`.
fn tab_backing() -> bool {
    option_env!("LP_EMU_TAB_MODULE").is_some_and(|url| !url.is_empty())
}

/// Whether the board on the other end is a REAL emulator rather than the
/// scripted door.
///
/// A few claims can only be made against the scripted door — they reach
/// inside it, or they pin a mechanism a real machine decides for itself —
/// and they say so in the log rather than pretending to have run. The list
/// is the same for both real backings by construction: a claim the tab
/// skipped and the live door did not would be a divergence worth reporting,
/// which is why this is one predicate and not two.
fn real_backing() -> bool {
    live_backing() || tab_backing()
}

/// What to call the backing in a skip line.
fn backing_name() -> &'static str {
    if live_backing() {
        "the live door"
    } else {
        "the tab backing"
    }
}

/// Say a claim was skipped, and against what. Named so a reader scanning the
/// log can count skips without reading four different sentences, and so
/// adding a fifth skip is visibly adding a skip.
fn log_skip(why: &str) {
    log(&format!("SKIPPED against {} ({why})", backing_name()));
}

async fn boolean(promise: Promise) -> bool {
    JsFuture::from(promise)
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

async fn strings(promise: Promise) -> Vec<String> {
    let value = JsFuture::from(promise).await.expect("a string array");
    Array::from(&value)
        .iter()
        .filter_map(|entry| entry.as_string())
        .collect()
}

fn error_text(value: &JsValue) -> String {
    value
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}

// ---------------------------------------------------------------------------
// DD9 — the install seam M3 rests on
// ---------------------------------------------------------------------------

/// Can an own property on the `navigator` instance shadow Chromium's
/// `Navigator.prototype.serial` getter, and can it be taken back off?
/// Everything about the shim, in M2 and in M3, rests on "yes".
#[wasm_bindgen_test]
async fn navigator_serial_is_shadowed_by_an_own_property_and_is_removable() {
    shim_off().await;

    let standalone = js_can_shadow_navigator_serial();
    log(&format!(
        "DD9 standalone defineProperty probe: {standalone}"
    ));
    assert!(
        standalone.contains("\"shadowed\":true"),
        "Object.defineProperty(navigator, \"serial\", …) did not shadow the \
         prototype getter in this browser: {standalone}"
    );
    assert!(
        standalone.contains("\"removedAgain\":true"),
        "the shadowing own property could not be removed again: {standalone}"
    );

    let before = JsFuture::from(js_serial_shadowing_report())
        .await
        .expect("report")
        .as_string()
        .unwrap_or_default();
    log(&format!("DD9 navigator.serial BEFORE install: {before}"));

    shim_over(&["c6-a"]).await;
    let during = JsFuture::from(js_serial_shadowing_report())
        .await
        .expect("report")
        .as_string()
        .unwrap_or_default();
    log(&format!("DD9 navigator.serial WHILE installed: {during}"));
    assert!(
        during.contains("\"serialIsTheShim\":true"),
        "navigator.serial is not the shim while installed: {during}"
    );
    assert!(
        during.contains("\"hasOwnProperty\":true") && during.contains("\"ownIsConfigurable\":true"),
        "the shim's own property is not configurable, so uninstall could not \
         put the browser's back: {during}"
    );
    assert!(
        boolean(js_is_supported()).await,
        "browser_serial.js sees the shim"
    );
    assert!(
        boolean(js_flash_bridge_is_supported()).await,
        "browser_esp32_flash.js's support probe sees the shim"
    );

    shim_off().await;
    let after = JsFuture::from(js_serial_shadowing_report())
        .await
        .expect("report")
        .as_string()
        .unwrap_or_default();
    log(&format!("DD9 navigator.serial AFTER uninstall: {after}"));
    assert!(
        after.contains("\"serialIsTheShim\":false"),
        "uninstall left the shim in place: {after}"
    );
}

// ---------------------------------------------------------------------------
// Criterion 1 — session-id stability across open/close
// ---------------------------------------------------------------------------

/// `sessionForPort`'s one-session-per-port rule (`browser_serial.js:89-102`):
/// enumerate, open, close, enumerate again — the same ids come back, because
/// the map is keyed on port identity and `closePort` keeps its entry.
#[wasm_bindgen_test]
async fn session_ids_are_stable_across_open_and_close() {
    shim_over(&["c6-a", "c6-b"]).await;

    let first = granted_sessions().await;
    assert!(!first.is_empty(), "the shim enumerated no granted ports");
    let second = granted_sessions().await;
    assert_eq!(
        ids(&first),
        ids(&second),
        "two `getGrantedPorts()` calls minted different sessions for the same ports"
    );

    let id = first[0].id;
    open_port(id, false).await.expect("openPort");
    let while_open = granted_sessions().await;
    assert_eq!(ids(&first), ids(&while_open), "opening moved a session id");

    JsFuture::from(js_close_port(id)).await.expect("closePort");
    let after_close = granted_sessions().await;
    assert_eq!(
        ids(&first),
        ids(&after_close),
        "closing moved a session id — `closePort` must keep the entry"
    );
    log(&format!(
        "session ids across open/close: {:?} → {:?} → {:?}",
        ids(&first),
        ids(&while_open),
        ids(&after_close)
    ));

    shim_off().await;
}

// ---------------------------------------------------------------------------
// Criterion 1 — session-id stability across re-enumerate
// ---------------------------------------------------------------------------

/// A REPLUG makes the emulator enumerate again, and the shim answers it the
/// way Chrome does: a NEW `SerialPort` object. `adoptReenumeratedPorts`
/// (`browser_serial.js:58-84`) pairs the dead generation to its replacement by
/// vid:pid, so the SESSION id survives — the gallery-wallpaper defect (G1,
/// 2026-08-31), pinned.
///
/// **Amended for plan two M5.** It used to drive this from a `reset` over the
/// control channel, on the model that a chip reset looks like a replug. It
/// does not on this part, and the frozen flash flow cannot survive one — see
/// `a_chip_reset_does_not_re_enumerate` below, and `virtual_serial.js`'s
/// header for the measurement. The claim being pinned is unchanged; only the
/// thing that produces a re-enumeration is.
#[wasm_bindgen_test]
async fn session_ids_are_stable_across_a_re_enumeration() {
    shim_over(&["c6-a"]).await;
    let boards = board_ids().await;
    let board = boards.first().cloned().expect("a board");

    let before = granted_sessions().await;
    let old_port = live_port(&board).await;

    let returned = JsFuture::from(js_replug_over_the_cable(&board))
        .await
        .expect("a replug over the cable");
    assert!(
        Object::is(&returned, &old_port),
        "the port that was live before the replug is not the one the replug saw"
    );

    let new_port = live_port(&board).await;
    assert!(
        !Object::is(&new_port, &old_port),
        "a re-enumeration did NOT mint a new SerialPort object — a shim \
         holding one immortal port passes every test and models nothing"
    );
    assert!(
        js_port_info_json(&old_port).contains("\"usbVendorId\":12346"),
        "the dead generation stopped answering getInfo(), which is how \
         adoptReenumeratedPorts pairs it to its replacement"
    );

    let after = granted_sessions().await;
    assert_eq!(
        ids(&before),
        ids(&after),
        "the session id moved across a re-enumeration: {before:?} → {after:?}"
    );
    log(&format!(
        "re-enumeration: session ids {:?} held while the port object moved",
        ids(&after)
    ));

    shim_off().await;
}

/// A reboot the shim did not ask for is NOTICED — from the guest cycle in the
/// next control reply going backwards, never by pattern-matching a DTR/RTS
/// dance, which is the emulator's job to decode — and it does **not**
/// re-enumerate.
///
/// **Amended for plan two M5, and the claim is reversed.** As merged this
/// test asserted that an unrequested reboot mints a new `SerialPort`. It was
/// written before flashing existed, and it is wrong about this part: a C6's
/// USB-Serial-JTAG controller shares silicon with the CPU it resets, so the
/// USB device survives, which is why Studio can flash a real C6 over Web
/// Serial at all. Measured on the real esptool-js/Chrome path — the shim
/// killed the port object esptool-js was holding, mid-`Connecting…`, every
/// run; the quoted trace is in `virtual_serial.js`'s header. `esptool-js` is
/// reached through `browser_esp32_flash.js`, which is frozen and holds one
/// port for the whole call, so tolerating a re-enumeration was never
/// available.
#[wasm_bindgen_test]
async fn a_reboot_behind_our_back_is_noticed_and_does_not_re_enumerate() {
    if real_backing() {
        log_skip("this claim is pinned by the scripted half");
        // The live door decides its own cycle counter; the scripted half is
        // where this mechanism is pinned deterministically.
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");

    let before = granted_sessions().await;
    let old_port = live_port(&board).await;

    // One round trip first: the backing compares each reply's guest cycle
    // against the last one it saw, so the FIRST reply on a control channel is
    // a baseline and can never itself be a reboot. In the real flow the
    // baseline is the `dtr 0` at the head of every reset dance.
    JsFuture::from(js_poke_control_channel(&board))
        .await
        .expect("a baseline control reply");
    js_reboot_behind_our_back(&board);
    JsFuture::from(js_poke_control_channel(&board))
        .await
        .expect("one control round trip");

    let new_port = live_port(&board).await;
    assert!(
        Object::is(&new_port, &old_port),
        "a chip reset re-enumerated the port — the object esptool-js is \
         holding must survive one, as it does on the silicon"
    );
    assert_eq!(
        reboots_seen(&board),
        1,
        "the shim did not NOTICE the reboot; a port that survives because \
         nothing was watching is not the same claim"
    );
    assert_eq!(
        ids(&before),
        ids(&granted_sessions().await),
        "the session id moved across an unrequested reboot"
    );

    shim_off().await;
}

/// The ruling itself, on BOTH halves of the suite: an explicit chip reset —
/// the `reset` verb, which is what the dev banner and an agent at the console
/// press, and what esptool-js's DTR/RTS dance makes the emulator decode —
/// leaves the `SerialPort` object exactly where it was.
///
/// New in plan two M5. The port a flasher is holding must survive the reset
/// that flasher just asked for, or the flash dies between "Connecting…" and
/// the chip guard — measured, quoted in `virtual_serial.js`'s header.
#[wasm_bindgen_test]
async fn a_chip_reset_does_not_re_enumerate() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let before = live_port(&board).await;

    JsFuture::from(js_reset_over_control_channel(&board))
        .await
        .expect("reset over the control channel");

    let after = live_port(&board).await;
    assert!(
        Object::is(&after, &before),
        "a chip reset minted a new SerialPort — on this part the \
         USB-Serial-JTAG controller shares silicon with the CPU it resets, so \
         the USB device does not go away"
    );
    assert!(
        js_port_readable_is_null(&after),
        "the surviving port is enumerated but was never opened here, so its \
         readable must still be null"
    );
    log("chip reset: the port object survived, as it does on the silicon");

    shim_off().await;
}

// ---------------------------------------------------------------------------
// Criterion 2 — close vs release
// ---------------------------------------------------------------------------

/// `closePort` releases the streams and KEEPS the session entry, because the
/// entry is the grant handle and the flash flow closes the link session then
/// flashes through the same id (`docs/defects/2026-07-22-flash-session-map-deleted.md`).
/// `forgetPort` is the contrast: the grant is gone, so the entry goes with it
/// and the port stops being enumerated.
#[wasm_bindgen_test]
async fn a_closed_port_keeps_its_session_and_a_forgotten_one_does_not() {
    shim_over(&["c6-a"]).await;
    let sessions = granted_sessions().await;
    let id = sessions[0].id;

    open_port(id, false).await.expect("openPort");
    JsFuture::from(js_close_port(id)).await.expect("closePort");

    let port_after_close = JsFuture::from(js_get_port(id))
        .await
        .expect("getPort() after closePort() — the entry must have survived");
    assert!(
        !port_after_close.is_undefined() && !port_after_close.is_null(),
        "getPort() answered nothing after closePort()"
    );
    log("close-vs-release: getPort(id) still resolves after closePort(id)");

    let revoked = JsFuture::from(js_forget_port(id))
        .await
        .expect("forgetPort");
    assert_eq!(revoked.as_bool(), Some(true), "forgetPort() did not revoke");

    let after_forget = JsFuture::from(js_get_port(id)).await;
    let message = error_text(&after_forget.expect_err("getPort() after forgetPort() must throw"));
    assert!(
        message.contains(&format!("Unknown browser serial session: {id}")),
        "forgetPort() left the session reachable: {message}"
    );
    log(&format!(
        "close-vs-release: after forgetPort(id) → `{message}`"
    ));

    assert!(
        granted_sessions().await.is_empty(),
        "a forgotten port is still enumerated by getPorts()"
    );

    shim_off().await;
}

// ---------------------------------------------------------------------------
// Criterion 3 — read-pump error paths
// ---------------------------------------------------------------------------

/// The device goes away under an open port: the `readable` stream errors, the
/// read pump catches it and reports through `takeErrors`, and the controller
/// does not wedge — a subsequent `openProtocol` succeeds and reads again.
#[wasm_bindgen_test]
async fn the_read_pump_reports_a_lost_device_and_the_port_reopens() {
    if real_backing() {
        log_skip("this claim is pinned by the scripted half");
        // Killing a live board's byte channel from the browser would need the
        // server to cooperate; the scripted half pins the pump's error path.
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    open_port(id, false).await.expect("openPort");
    js_deliver_bytes(&board, "M! {\"hello\":\"before\"}\n");
    yield_to_event_loop().await;
    let mut reader = PortReader::default();
    let lines = reader.take_lines(id).await;
    assert!(
        !lines.is_empty(),
        "the read pump delivered no lines before the device was lost"
    );

    js_drop_byte_channel(&board);
    yield_to_event_loop().await;

    let errors = strings(js_take_errors(id)).await;
    assert!(
        !errors.is_empty(),
        "the read pump reported nothing when the device was lost"
    );
    log(&format!("read-pump error path: takeErrors → {errors:?}"));

    // Not wedged: the same session opens again and reads again.
    open_port(id, false)
        .await
        .expect("openProtocol after a lost device");
    js_deliver_bytes(&board, "M! {\"hello\":\"after\"}\n");
    yield_to_event_loop().await;
    let lines_after = reader.take_lines(id).await;
    assert!(
        lines_after.iter().any(|line| line.contains("after")),
        "the session did not read again after the read pump errored: {lines_after:?}"
    );
    log(&format!(
        "read-pump error path: reopened and read {lines_after:?}"
    ));

    shim_off().await;
}

// ---------------------------------------------------------------------------
// Criterion 4 — the flash bridge's port acquisition
// ---------------------------------------------------------------------------

/// `getPort` runs the adoption pass at RESOLUTION time, so the object the
/// esptool bridge is handed is the live generation and not the dead one its
/// session was holding — the bench failure its own comment names ("flashing
/// the blank C6 lost the race on the first try", G1 2026-08-31). All five of
/// `browser_esp32_flash.js`'s entry points open with this one call.
///
/// **Amended for plan two M5**: driven by a replug, since a chip reset no
/// longer moves the port object. The claim — `getPort` resolves the LIVE
/// generation, whatever moved it — is unchanged, and it is the one that
/// matters to the flash bridge.
#[wasm_bindgen_test]
async fn the_flash_bridge_acquires_the_live_generation() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    let before = JsFuture::from(js_get_port(id)).await.expect("getPort");
    let dead = live_port(&board).await;
    assert!(
        Object::is(&before, &dead),
        "getPort did not resolve the live port"
    );

    JsFuture::from(js_replug_over_the_cable(&board))
        .await
        .expect("a replug over the cable");
    let live = live_port(&board).await;
    assert!(!Object::is(&live, &dead), "the port object did not move");

    let acquired = JsFuture::from(js_get_port(id))
        .await
        .expect("getPort after a re-enumeration");
    assert!(
        Object::is(&acquired, &live),
        "getPort handed the flash bridge a DEAD generation — its open() would \
         fail instantly with NetworkError"
    );
    assert!(
        !Object::is(&acquired, &dead),
        "getPort handed back the pre-reset object"
    );
    assert!(
        boolean(js_flash_bridge_is_supported()).await,
        "browser_esp32_flash.js reports Web Serial unsupported under the shim"
    );
    log("flash-bridge acquisition: getPort resolved the live generation after a replug");

    shim_off().await;
}

// ---------------------------------------------------------------------------
// The polyfill's own contract
// ---------------------------------------------------------------------------

/// PD7: an emulated C6 is Espressif native USB, honestly indistinguishable,
/// so `labelForPort` gives it the strong label rather than the weak
/// bridge-chip one.
#[wasm_bindgen_test]
async fn get_info_is_303a_1001_and_the_label_is_the_strong_one() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let port = live_port(&board).await;

    let info = js_port_info_json(&port);
    log(&format!("getInfo(): {info}"));
    assert!(
        info.contains("\"vendorHex\":\"0x303a\"") && info.contains("\"productHex\":\"0x1001\""),
        "getInfo() is not 303a:1001: {info}"
    );

    let label = JsFuture::from(js_label_for_port_of(&port))
        .await
        .expect("labelForPort")
        .as_string()
        .unwrap_or_default();
    log(&format!("labelForPort(): {label}"));
    assert_eq!(label, "ESP32 Serial (303a:1001)");

    let session = &granted_sessions().await[0];
    assert_eq!(session.usb_vendor_id, Some(0x303a));
    assert_eq!(session.usb_product_id, Some(0x1001));
    assert_eq!(session.label, "ESP32 Serial (303a:1001)");

    shim_off().await;
}

/// `browser_esp32_device_controller.js:317` tests openness as
/// `Boolean(port?.readable || port?.writable)`. Both are null while closed and
/// non-null while open; this is behaviour, not a convenience.
#[wasm_bindgen_test]
async fn openness_is_readable_or_writable() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;
    let port = live_port(&board).await;

    assert!(
        js_port_readable_is_null(&port) && js_port_writable_is_null(&port),
        "a closed port exposed a stream"
    );
    open_port(id, false).await.expect("openPort");
    assert!(
        !js_port_readable_is_null(&port) && !js_port_writable_is_null(&port),
        "an open port exposed no streams, so isOpen() reads false"
    );
    JsFuture::from(js_close_port(id)).await.expect("closePort");
    assert!(
        js_port_readable_is_null(&port) && js_port_writable_is_null(&port),
        "a closed port kept a stream, so isOpen() reads true forever"
    );
    log("openness: null → non-null → null across open/close");

    shim_off().await;
}

/// The single most important claim in the milestone: the shim translates
/// nothing. `runReset("normal")` is `D0 W100 R1 W100 R0`, and what reaches the
/// control channel is those three lines and NOTHING else — no `reset` verb, no
/// `download-mode`, no dance the shim recognised and shortcut.
#[wasm_bindgen_test]
async fn the_signal_lines_pass_through_undecoded() {
    if real_backing() {
        log_skip("this claim is pinned by the scripted half");
        // The live door keeps no log the browser can read; the scripted half
        // is where the exact lines are pinned.
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    open_port(id, true).await.expect("openPort with a reset");
    let log_text = js_control_log(&board);
    log(&format!(
        "control channel after a normal reset:\n{log_text}"
    ));
    assert_eq!(
        log_text.lines().collect::<Vec<_>>(),
        vec!["dtr 0", "rts 1", "rts 0"],
        "the shim did not pass the reset dance through line for line"
    );
    assert_eq!(
        js_reboot_count(&board),
        1,
        "the emulator did not decode the dance into a reset"
    );

    assert!(
        !log_text.contains("reset") && !log_text.contains("download-mode"),
        "the shim sent a control verb of its own:\n{log_text}"
    );

    shim_off().await;
}

/// Under the shim there is no chooser to show. M2 resolves `requestPort` to
/// the first board and says so here; M3 replaces the resolution behind
/// `resolveRequestedPort` without touching the port double. A filter that
/// excludes everything rejects the way Chrome does — `NotFoundError`, which
/// upstream maps to "cancelled".
#[wasm_bindgen_test]
async fn request_port_resolves_to_the_first_board() {
    shim_over(&["c6-a", "c6-b"]).await;
    let boards = board_ids().await;

    let session = JsFuture::from(js_request_port())
        .await
        .expect("requestPort()");
    let id = number(&session, "id").expect("session id") as u32;
    let first = live_port(&boards[0]).await;
    let resolved = JsFuture::from(js_get_port(id)).await.expect("getPort");
    assert!(
        Object::is(&resolved, &first),
        "requestPort() did not resolve to the first board"
    );

    // Twice over the same port is ONE session — `requestPort` used to mint a
    // second controller over an already-registered port (the multi-board L1
    // defect), and `sessionForPort` is the shared rule that stopped it.
    let again = JsFuture::from(js_request_port())
        .await
        .expect("requestPort()");
    assert_eq!(
        number(&again, "id").expect("session id") as u32,
        id,
        "requestPort() minted a second session for one port"
    );
    log(&format!(
        "requestPort() resolved to board `{}` as session {id}, twice",
        boards[0]
    ));

    shim_off().await;
}

/// `installSerialEvents` wires Studio's hotplug sweep to the bus's `connect`
/// and `disconnect`. It is installed at most once per page, so this is the
/// suite's only caller.
///
/// **Amended for plan two M5**: driven by a replug rather than by a chip
/// reset, for the reason `a_reboot_behind_our_back_is_noticed_and_does_not_re_enumerate`
/// gives. The edges and their order are the claim, and they are unchanged.
#[wasm_bindgen_test]
async fn hotplug_edges_arrive_from_a_re_enumeration() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");

    let connects = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let disconnects = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let on_connect = {
        let connects = connects.clone();
        Closure::<dyn FnMut()>::new(move || connects.set(connects.get() + 1))
    };
    let on_disconnect = {
        let disconnects = disconnects.clone();
        Closure::<dyn FnMut()>::new(move || disconnects.set(disconnects.get() + 1))
    };
    assert!(
        boolean(js_install_serial_events(
            on_connect.as_ref().unchecked_ref(),
            on_disconnect.as_ref().unchecked_ref(),
        ))
        .await,
        "installSerialEvents() found no navigator.serial to listen on"
    );

    JsFuture::from(js_replug_over_the_cable(&board))
        .await
        .expect("a replug over the cable");
    yield_to_event_loop().await;

    assert_eq!(
        (disconnects.get(), connects.get()),
        (1, 1),
        "a re-enumeration did not produce one disconnect and one connect"
    );
    log("hotplug: a re-enumeration produced disconnect then connect");

    drop(on_connect);
    drop(on_disconnect);
    shim_off().await;
}

/// A port that was OPEN when the cable came out opens again on the new
/// generation once it goes back in — and reads.
///
/// The 2026-09-24 walk's stuck card
/// (`docs/defects/2026-09-24-emulated-replug-leaves-the-old-byte-channel-open.md`):
/// `detach` errored the open port's stream but left the board's byte channel
/// open underneath it, so the replugged port's `open()` met the old socket
/// ("already open in this page") and the card sat at "Attached — not
/// listening" forever. On a real board the unplug takes the port with it,
/// and so must the shim's.
#[wasm_bindgen_test]
async fn a_port_open_across_a_replug_reopens_on_the_new_generation() {
    if real_backing() {
        log_skip("this claim is pinned by the scripted half");
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    let mut reader = PortReader::default();
    open_port(id, false)
        .await
        .expect("openPort before the replug");
    yield_to_event_loop().await;
    let _ = reader.take_lines(id).await;

    JsFuture::from(js_replug_over_the_cable(&board))
        .await
        .expect("a replug over the cable");
    yield_to_event_loop().await;
    let _ = strings(js_take_errors(id)).await;

    // The next `getGrantedPorts` is what adopts the new generation into the
    // same session, exactly as Studio's connect-edge sweep does.
    let after = granted_sessions().await;
    assert_eq!(ids(&after), vec![id], "the session survived the replug");
    open_port(id, false)
        .await
        .map_err(|error| error_text(&error))
        .expect("openPort on the replugged generation");
    yield_to_event_loop().await;
    let lines = reader.take_lines(id).await;
    assert!(
        lines.iter().any(|line| line.contains("hello")),
        "the replugged port opened but read nothing: {lines:?}"
    );
    log(&format!("replug while open: reopened and read {lines:?}"));

    JsFuture::from(js_release_port(id))
        .await
        .expect("releasePort");
    shim_off().await;
}

/// An open that RACES the connect-edge sweep still opens the new generation.
///
/// Nothing orders a link's `Open` after the sweep: the model's timers (the
/// flash ladder's reopen knocks, an eviction's reopen) queue opens on the
/// link's own future, while the sweep that adopts a re-enumerated port waits
/// its turn in the studio actor's command queue. So `openPort` adopts first,
/// the way `getPort` does; before it did, this open met the dead object and
/// failed with `NetworkError` — the same words as a port held elsewhere.
#[wasm_bindgen_test]
async fn an_open_that_races_the_connect_sweep_opens_the_new_generation() {
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;
    let dead = live_port(&board).await;

    JsFuture::from(js_replug_over_the_cable(&board))
        .await
        .expect("a replug over the cable");
    let live = live_port(&board).await;
    assert!(!Object::is(&live, &dead), "the port object did not move");

    // No `getGrantedPorts` here: the sweep has not run yet.
    open_port(id, false)
        .await
        .map_err(|error| error_text(&error))
        .expect("openPort before the connect sweep adopted the new generation");
    assert!(
        !js_port_readable_is_null(&live),
        "openPort did not open the live generation"
    );
    let after = granted_sessions().await;
    assert_eq!(ids(&after), vec![id], "the session survived the replug");
    log("an open racing the connect sweep opened the new generation");

    JsFuture::from(js_release_port(id))
        .await
        .expect("releasePort");
    shim_off().await;
}

/// The bytes go both ways through the real controller: `writeBytes` reaches
/// the board's byte channel unchanged, and what the board says comes back as
/// lines.
#[wasm_bindgen_test]
async fn bytes_travel_both_ways_through_the_controller() {
    if real_backing() {
        log_skip("this claim is pinned by the scripted half");
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    open_port(id, false).await.expect("openPort");
    yield_to_event_loop().await;
    let greeting = PortReader::default().take_lines(id).await;
    assert!(
        greeting.iter().any(|line| line.contains("hello")),
        "the board's greeting never reached the read pump: {greeting:?}"
    );

    JsFuture::from(js_write_bytes(id, b"{\"ping\":1}\n"))
        .await
        .expect("writeBytes");
    yield_to_event_loop().await;
    let received = js_received_bytes(&board);
    assert!(
        received.contains("\"ping\":1"),
        "the write never reached the board's byte channel: {received:?}"
    );
    log(&format!("bytes both ways: board received {received:?}"));

    JsFuture::from(js_release_port(id))
        .await
        .expect("releasePort");
    shim_off().await;
}

/// Studio's OWN Rust boundary — the production externs in `browser_serial.rs`,
/// through `providers::browser_serial_esp32::granted_ports()` — enumerates the
/// shim's ports with the vid:pid fields the grant-aware picker matches on
/// (D7). This is the seam Studio actually uses, unchanged.
#[wasm_bindgen_test]
async fn the_production_rust_boundary_enumerates_the_shims_ports() {
    shim_over(&["c6-a"]).await;

    let ports = lpa_link::providers::browser_serial_esp32::granted_ports()
        .await
        .expect("granted_ports() through the production externs");
    assert!(
        !ports.is_empty(),
        "the production Rust boundary saw no ports under the shim"
    );
    for port in &ports {
        assert_eq!(port.usb_vid_pid(), Some((0x303a, 0x1001)));
        assert_eq!(port.label, "ESP32 Serial (303a:1001)");
    }
    log(&format!(
        "production boundary: granted_ports() → {:?}",
        ports
            .iter()
            .map(|port| (port.id, port.label.clone(), port.usb_vid_pid()))
            .collect::<Vec<_>>()
    ));

    shim_off().await;
}

// ---------------------------------------------------------------------------
// The port's lp-link end, through the production surface (plan
// `lp2025/2026-09-27-0215-lp-link-usb-cutover`, P4)
// ---------------------------------------------------------------------------
//
// These drive `browser_serial.rs`'s OWN registry and loop — the production
// `web_serial_link` surface, over the production externs — against a
// board-side `lp_link::Link` running here in Rust, whose frames the scripted
// door delivers and whose input is what the host wrote (`takeReceivedRaw`).
// The board double speaks as the P2 firmware does: hello on every `Up`, JSON
// until the host opts in, a fresh learned table per session.

/// The link comes up on its own and the board's hello arrives as one whole
/// message, with the session noted for the journal.
#[wasm_bindgen_test]
async fn the_link_comes_up_and_the_boards_hello_arrives() {
    the_link_comes_up_and_the_boards_hello_arrives_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_classic_link_comes_up_and_its_hello_arrives() {
    the_link_comes_up_and_the_boards_hello_arrives_on(Board::Classic).await;
}

async fn the_link_comes_up_and_the_boards_hello_arrives_on(board: Board) {
    let Some(mut bench) = LinkBench::open("link-up", board).await else {
        return;
    };
    let reads = bench.exchange_until(|reads| hello_count(reads) >= 1).await;
    assert_eq!(hello_count(&reads), 1, "{reads:?}");
    let notes = web_serial_link::take_wire_notes(bench.id);
    assert!(
        notes.iter().any(|note| note.starts_with("link: up")),
        "{notes:?}"
    );
    log(&format!("link: up, hello whole; notes {notes:?}"));
    bench.close().await;
}

/// A request is one link message on the board's proto channel — not an `M!`
/// line — and its answer comes back as a frame, packed once the port's own
/// opt-in has been answered (D14: packed for every host).
#[wasm_bindgen_test]
async fn a_request_is_one_link_message_and_its_answer_comes_back_packed() {
    a_request_is_one_link_message_and_its_answer_comes_back_packed_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_classic_request_is_one_link_message_and_its_answer_comes_back_packed() {
    a_request_is_one_link_message_and_its_answer_comes_back_packed_on(Board::Classic).await;
}

async fn a_request_is_one_link_message_and_its_answer_comes_back_packed_on(board: Board) {
    let Some(mut bench) = LinkBench::open("link-request", board).await else {
        return;
    };
    bench.exchange_until(|reads| hello_count(reads) >= 1).await;
    // The port asks for packed replies on its own after the hello.
    bench.exchange_until(|_| bench_board_packs()).await;

    let request = lpc_wire::ClientMessage {
        id: 41,
        msg: lpc_wire::ClientRequest::StopAllProjects,
    };
    web_serial_link::send_client_json(
        bench.id,
        &lpc_wire::json::to_string(&request).expect("json"),
    )
    .expect("the link takes the request");
    let reads = bench
        .exchange_until(|reads| answer(reads, 41).is_some())
        .await;

    let frame = answer(&reads, 41).expect("the answer");
    assert!(frame.packed, "the answer came packed: {frame:?}");
    assert!(
        BOARD.with(|double| double.borrow().requests.contains(&41)),
        "the board saw the request as one proto message"
    );
    let raw = js_received_bytes(&bench.board);
    assert!(
        !raw.contains("M!"),
        "nothing the host wrote is an M! line: {raw:?}"
    );
    bench.close().await;
}

/// A link frame the board wrote in two halves, drained in between, comes out
/// once and whole: the port's link holds the first half across the drain.
#[wasm_bindgen_test]
async fn a_link_frame_split_across_reads_is_read_whole() {
    a_link_frame_split_across_reads_is_read_whole_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_classic_link_frame_split_across_reads_is_read_whole() {
    a_link_frame_split_across_reads_is_read_whole_on(Board::Classic).await;
}

async fn a_link_frame_split_across_reads_is_read_whole_on(board: Board) {
    let Some(mut bench) = LinkBench::open("link-split", board).await else {
        return;
    };
    bench.exchange_until(|reads| hello_count(reads) >= 1).await;

    let frames = BOARD.with(|double| {
        let mut double = double.borrow_mut();
        double.send(&unload_project(10));
        double.frames_now()
    });
    let (first, second) = frames.split_at(frames.len() / 2);
    js_deliver_raw_bytes(&bench.board, first);
    yield_to_event_loop().await;
    let half = web_serial_link::take_reads(bench.id);
    assert!(
        half.iter().all(|read| !matches!(read, WireRead::Frame(_))),
        "half a frame came out as something: {half:?}"
    );
    js_deliver_raw_bytes(&bench.board, second);
    let reads = bench
        .exchange_until(|reads| answer(reads, 10).is_some())
        .await;
    assert_eq!(
        answer(&reads, 10).map(|frame| frame.json.clone()),
        Some(json_of(&unload_project(10)))
    );
    log(&format!(
        "link: a {} B frame split {}+{} across two drains came out once, whole",
        frames.len(),
        first.len(),
        second.len()
    ));
    bench.close().await;
}

/// Raw console text before, between and after link frames stays lines, in
/// order; the frames between them stay messages.
#[wasm_bindgen_test]
async fn console_text_between_link_frames_stays_lines() {
    console_text_between_link_frames_stays_lines_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_classics_console_text_between_link_frames_stays_lines() {
    console_text_between_link_frames_stays_lines_on(Board::Classic).await;
}

async fn console_text_between_link_frames_stays_lines_on(board: Board) {
    let Some(mut bench) = LinkBench::open("link-text", board).await else {
        return;
    };
    bench.exchange_until(|reads| hello_count(reads) >= 1).await;

    let (a, b) = (unload_project(3), unload_project(4));
    let [boot, between, after] = board.console_lines();
    let bytes = BOARD.with(|double| {
        let mut double = double.borrow_mut();
        double.send(&a);
        let first = double.frames_now();
        double.send(&b);
        let second = double.frames_now();
        [
            format!("{boot}\r\n").into_bytes(),
            first,
            format!("{between}\n").into_bytes(),
            second,
            format!("{after}\n").into_bytes(),
        ]
        .concat()
    });
    js_deliver_raw_bytes(&bench.board, &bytes);
    let reads = bench
        .exchange_until(|reads| {
            reads
                .iter()
                .any(|read| matches!(read, WireRead::Line(line) if line == after))
        })
        .await;
    let order: Vec<String> = reads
        .iter()
        .filter_map(|read| match read {
            WireRead::Line(line) if line.starts_with('[') => Some(line.clone()),
            WireRead::Frame(frame) if frame.message.as_ref().is_ok_and(|m| m.id != 0) => {
                Some(frame.json.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        [
            boot.to_string(),
            json_of(&a),
            between.to_string(),
            json_of(&b),
            after.to_string(),
        ]
    );
    bench.close().await;
}

/// Plan D9: a board that restarts (a new nonce) ends the session, and a
/// drainer hears it as a link reset at once — before the new session's
/// hello — rather than waiting out a request's budget.
#[wasm_bindgen_test]
async fn a_board_restart_is_a_link_reset_then_a_new_hello() {
    a_board_restart_is_a_link_reset_then_a_new_hello_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_classic_restart_is_a_link_reset_then_a_new_hello() {
    a_board_restart_is_a_link_reset_then_a_new_hello_on(Board::Classic).await;
}

async fn a_board_restart_is_a_link_reset_then_a_new_hello_on(board: Board) {
    let Some(mut bench) = LinkBench::open("link-reset", board).await else {
        return;
    };
    bench.exchange_until(|reads| hello_count(reads) >= 1).await;

    bench.restart_board(0xB0A2_0002);
    let reads = bench.exchange_until(|reads| hello_count(reads) >= 1).await;

    let reset_at = reads
        .iter()
        .position(|read| matches!(read, WireRead::LinkReset(_)))
        .unwrap_or_else(|| panic!("no link reset: {reads:?}"));
    let hello_at = reads
        .iter()
        .position(is_hello)
        .unwrap_or_else(|| panic!("no new hello: {reads:?}"));
    assert!(reset_at < hello_at, "{reads:?}");
    let WireRead::LinkReset(note) = &reads[reset_at] else {
        unreachable!()
    };
    assert!(
        lpa_link::device_link::port_read_map::is_link_reset_note(note),
        "{note}"
    );
    log(&format!("link: a board restart read as {note:?}"));
    bench.close().await;
}

/// Plan D9 where a user meets it: a request in flight when the board
/// restarts FAILS the editor lens's io at once — it reads the link reset in
/// its answer's place, and tees it to the lens tap — instead of waiting out
/// its 5 s budget. Nothing on that path knows which chip is on the cable.
#[wasm_bindgen_test]
async fn a_request_in_flight_when_the_board_restarts_fails_the_lens_at_once() {
    a_request_in_flight_when_the_board_restarts_fails_the_lens_at_once_on(Board::C6).await;
}

/// The same claim for a classic ESP32 behind its CH340 (the port on `uart()`).
#[wasm_bindgen_test]
async fn a_request_in_flight_when_a_classic_restarts_fails_the_lens_at_once() {
    a_request_in_flight_when_the_board_restarts_fails_the_lens_at_once_on(Board::Classic).await;
}

async fn a_request_in_flight_when_the_board_restarts_fails_the_lens_at_once_on(board: Board) {
    let Some(mut bench) = LinkBench::open("lens-reset", board).await else {
        return;
    };
    bench.exchange_until(|reads| hello_count(reads) >= 1).await;

    let provider = BrowserSerialEsp32Provider::new();
    let endpoint = provider.create_granted_endpoint("lens bench", bench.id);
    let taps: Rc<RefCell<Vec<LensTapLine>>> = Rc::default();
    let tap = {
        let taps = Rc::clone(&taps);
        Rc::new(move |line: LensTapLine| taps.borrow_mut().push(line))
    };
    let mut io = provider
        .lens_client_io(&endpoint, tap, LinkManagementEventSink::noop())
        .expect("a lens io over the open port");
    io.send(lpc_wire::ClientMessage {
        id: 51,
        msg: lpc_wire::ClientRequest::StopAllProjects,
    })
    .await
    .expect("the link takes the request");
    // The board goes down before it hears the request, and comes back as a
    // new session. Only the board half is pumped: the lens is the drainer.
    bench.restart_board(0xB0A2_0003);
    let pumping = Rc::new(Cell::new(true));
    spawn_local(pump_board(bench.board.clone(), Rc::clone(&pumping)));
    let answer = io.receive().await;
    pumping.set(false);

    let error = match answer {
        Err(error) => error.to_string(),
        Ok(message) => panic!("the lost request was answered: {message:?}"),
    };
    assert!(error.contains("link: reset"), "{error}");
    assert!(
        BOARD.with(|double| !double.borrow().requests.contains(&51)),
        "the new session carried the old request"
    );
    assert!(
        taps.borrow().iter().any(|line| matches!(
            line,
            LensTapLine::Note(note)
                if lpa_link::device_link::port_read_map::is_link_reset_note(note)
        )),
        "the lens tap heard no reset: {:?}",
        taps.borrow()
    );
    log(&format!(
        "lens ({board:?}): the request in flight failed with {error:?}"
    ));
    bench.close().await;
}

/// Run the board half of the link until `pumping` is cleared: what the host
/// wrote in, what the board has to say out. Leaves the host's reads alone.
async fn pump_board(board: String, pumping: Rc<Cell<bool>>) {
    for _ in 0..2_000 {
        if !pumping.get() {
            break;
        }
        let written = js_take_received_raw(&board).to_vec();
        let out = BOARD.with(|double| {
            let mut double = double.borrow_mut();
            double.on_bytes(&written);
            double.frames_now()
        });
        if !out.is_empty() {
            js_deliver_raw_bytes(&board, &out);
        }
        yield_once().await;
    }
}

/// Which board is at the other end of a link test.
#[derive(Clone, Copy, Debug)]
enum Board {
    /// A C6 on its native USB-Serial-JTAG: `303a:1001`, both ends on
    /// `usb()` — what the polyfill's ports are.
    C6,
    /// A classic ESP32 (v3) on UART0 behind its CH340K, `1a86:7522` (plan
    /// `lp2025/2026-09-28-2015-classic-uart-on-lp-link`): the port on
    /// `uart()`, the board on its own cut of it.
    Classic,
}

impl Board {
    /// The USB ids the port enumerates with.
    fn usb_ids(self) -> (u16, u16) {
        match self {
            Board::C6 => (0x303a, 0x1001),
            Board::Classic => (0x1a86, 0x7522),
        }
    }

    /// The preset the page's end must pick for this board's port.
    fn host_preset(self) -> LinkConfig {
        match self {
            Board::C6 => LinkConfig::usb(),
            Board::Classic => LinkConfig::uart(),
        }
    }

    /// The board's own end: the C6's preset, or the classic's timings on the
    /// `uart()` preset — a 200 ms resend floor and SYNs backed off to 1.6 s,
    /// as `fw_esp32_common::uart_link::uart_board_link_config` sets them
    /// (which this crate cannot depend on; the fake board's classic test uses
    /// the same double).
    fn board_config(self) -> LinkConfig {
        match self {
            Board::C6 => LinkConfig::usb(),
            Board::Classic => LinkConfig {
                min_rto: 200_000,
                syn_backoff: 4,
                ..LinkConfig::uart()
            },
        }
    }

    fn package(self) -> &'static str {
        match self {
            Board::C6 => "fw-esp32c6",
            Board::Classic => "fw-esp32v3",
        }
    }

    /// Raw console text a board writes outside its link frames: before, between
    /// and after. The classic's are its own — its boot banner, and the
    /// WS281x telemetry lines it queues between frames (P2) — and each starts
    /// with `[`, as the order check below expects.
    fn console_lines(self) -> [&'static str; 3] {
        match self {
            Board::C6 => ["[INIT] boot", "[log] between", "[log] after"],
            Board::Classic => [
                "[INIT] fw-esp32 initialized, starting server loop...",
                "[WS281X] telemetry between frames",
                "[WS281X-WIRE] after",
            ],
        }
    }
}

/// One production port, opened through `web_serial_link`, and the board
/// double ([`BOARD`]) at the other end of the scripted door.
struct LinkBench {
    id: u32,
    board: String,
    profile: Board,
    reads: Vec<WireRead>,
}

impl LinkBench {
    /// `None` (and a skip line) on a live or tab backing: those boards speak
    /// their own firmware's link, which is P2's to bring.
    ///
    /// A [`Board::Classic`] port enumerates as the classic's CH340K before the
    /// page describes it; either way the bench checks the port's link took the
    /// board's preset, through the production vendor-id rule.
    async fn open(name: &str, profile: Board) -> Option<Self> {
        if real_backing() {
            log_skip("the board end here is a Rust lp-link double on the scripted door");
            return None;
        }
        shim_over(&["c6-a"]).await;
        let board = board_ids().await.first().cloned().expect("a board");
        if let Board::Classic = profile {
            let (vendor, product) = profile.usb_ids();
            JsFuture::from(js_present_behind_bridge(&board, vendor, product))
                .await
                .expect("the port presents as a bridge");
        }
        let ports = lpa_link::providers::browser_serial_esp32::granted_ports()
            .await
            .expect("granted ports through the production externs");
        let port = ports.first().expect("a port");
        assert_eq!(port.usb_vid_pid(), Some(profile.usb_ids()), "{name}");
        let id = port.id;
        BOARD.with(|double| *double.borrow_mut() = BoardDouble::new(profile, 0xB0A2_0001));
        web_serial_link::open(id, 921_600, None)
            .await
            .unwrap_or_else(|error| panic!("{name}: open: {error}"));
        let preset = web_serial_link::link_config(id).expect("the open port has a link");
        assert_eq!(
            format!("{preset:?}"),
            format!("{:?}", profile.host_preset()),
            "{name}: the port's link runs the {profile:?} board's preset"
        );
        Some(Self {
            id,
            board,
            profile,
            reads: Vec::new(),
        })
    }

    /// The board restarts: a board of the same kind, under a new nonce.
    fn restart_board(&self, nonce: u32) {
        let profile = self.profile;
        BOARD.with(|double| *double.borrow_mut() = BoardDouble::new(profile, nonce));
    }

    /// Run both ends until `done` holds for what the port read (bounded by
    /// rounds, never by a clock), and hand back everything read.
    async fn exchange_until(&mut self, done: impl Fn(&[WireRead]) -> bool) -> Vec<WireRead> {
        for _ in 0..200 {
            let written = js_take_received_raw(&self.board).to_vec();
            let out = BOARD.with(|board| {
                let mut board = board.borrow_mut();
                board.on_bytes(&written);
                board.frames_now()
            });
            if !out.is_empty() {
                js_deliver_raw_bytes(&self.board, &out);
            }
            yield_once().await;
            self.reads.extend(web_serial_link::take_reads(self.id));
            if done(&self.reads) {
                break;
            }
        }
        std::mem::take(&mut self.reads)
    }

    async fn close(self) {
        let _ = web_serial_link::release(self.id).await;
        shim_off().await;
    }
}

thread_local! {
    /// The board end of the current link test.
    static BOARD: std::cell::RefCell<BoardDouble> =
        std::cell::RefCell::new(BoardDouble::new(Board::C6, 0xB0A2_0001));
}

/// Whether the current board double has been asked to pack, and said yes.
fn bench_board_packs() -> bool {
    BOARD.with(|board| board.borrow().packed)
}

/// A board's end of the link, as the firmware runs it (the C6's since the
/// USB cut-over, the classic's since its UART one).
struct BoardDouble {
    profile: Board,
    link: Link<SelectiveRepeat>,
    table: lpc_wire::LearnedTable,
    packed: bool,
    requests: Vec<u64>,
}

impl BoardDouble {
    fn new(profile: Board, nonce: u32) -> Self {
        Self {
            profile,
            link: Link::new(profile.board_config(), nonce),
            table: lpc_wire::LearnedTable::default(),
            packed: false,
            requests: Vec::new(),
        }
    }

    fn now() -> u64 {
        (js_sys::Date::now() * 1_000.0) as u64
    }

    fn on_bytes(&mut self, bytes: &[u8]) {
        self.link.on_bytes(Self::now(), bytes);
        while let Some(event) = self.link.recv() {
            match event {
                LinkEvent::Up { .. } => {
                    self.packed = false;
                    self.table = lpc_wire::LearnedTable::default();
                    self.send(&hello(self.profile));
                }
                LinkEvent::Reset { .. } => {
                    self.packed = false;
                    self.table = lpc_wire::LearnedTable::default();
                }
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => {
                    let request = lpc_wire::decode_client_payload(&data).expect("a request");
                    match request.msg {
                        lpc_wire::ClientRequest::SetEncoding { encoding, .. } => {
                            self.send(&lpc_wire::WireServerMessage::new(
                                request.id,
                                lpc_wire::ServerMsgBody::SetEncoding { encoding },
                            ));
                            self.packed = encoding == lpc_wire::WireEncoding::Packed;
                        }
                        _ => {
                            self.requests.push(request.id);
                            self.send(&unload_project(request.id));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn send(&mut self, message: &lpc_wire::WireServerMessage) {
        let mut payload = Vec::new();
        let table: Option<&mut dyn lpc_wire::LearnStore> = if self.packed {
            Some(&mut self.table)
        } else {
            None
        };
        lpc_wire::encode_server_payload(message, table, &mut payload);
        self.link.send(CH_PROTO, &payload).expect("board send");
    }

    /// Every frame the board's link has to write now, as one chunk.
    fn frames_now(&mut self) -> Vec<u8> {
        let now = Self::now();
        let mut out = Vec::new();
        while let Some(frame) = self.link.poll_transmit(now) {
            out.extend_from_slice(frame);
        }
        out
    }
}

fn hello(board: Board) -> lpc_wire::WireServerMessage {
    use lpc_wire::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    lpc_wire::WireServerMessage::new(
        0,
        lpc_wire::ServerMsgBody::Hello(ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: vec![],
                package: board.package().to_string(),
                version: "unknown".into(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts::default(),
            device_uid: None,
            pack_format: lpc_wire::PACK_FORMAT_VERSION,
            auth: lpc_wire::HelloAuth::TRUSTED,
        }),
    )
}

fn is_hello(read: &WireRead) -> bool {
    matches!(read, WireRead::Frame(frame) if frame.json.contains("\"hello\""))
}

fn hello_count(reads: &[WireRead]) -> usize {
    reads.iter().filter(|read| is_hello(read)).count()
}

fn answer(reads: &[WireRead], id: u64) -> Option<&ReadFrame> {
    reads.iter().find_map(|read| match read {
        WireRead::Frame(frame) if frame.message.as_ref().is_ok_and(|m| m.id == id) => Some(frame),
        _ => None,
    })
}

/// The Rust half of the pump for the tests that drive the SUPPORT module's
/// sessions (a second instance of `browser_serial.js`, so not
/// `browser_serial.rs`'s registry): the shipped controller's bytes through
/// one production [`LinkPortService`], a new one when the controller's
/// buffer generation moves (a reopen). What these tests read is the board's
/// raw console text, which the link hands on as lines.
#[derive(Default)]
struct PortReader {
    reader: Option<(u32, LinkPortService)>,
}

impl PortReader {
    async fn take(&mut self, id: u32) -> Vec<WireRead> {
        let taken = JsFuture::from(js_take_bytes(id)).await.expect("takeBytes");
        let generation = number(&taken, "generation").expect("a generation") as u32;
        let bytes = Reflect::get(&taken, &JsValue::from_str("bytes"))
            .map(|value| js_sys::Uint8Array::new(&value).to_vec())
            .expect("bytes");
        if self.reader.as_ref().map(|(at, _)| *at) != Some(generation) {
            self.reader = Some((
                generation,
                LinkPortService::new(LinkConfig::usb(), 1, false, None),
            ));
        }
        let (_, reader) = self.reader.as_mut().expect("a reader");
        reader.on_bytes((js_sys::Date::now() * 1_000.0) as u64, &bytes);
        reader.take_reads()
    }

    /// [`Self::take`] as lines.
    async fn take_lines(&mut self, id: u32) -> Vec<String> {
        self.take(id)
            .await
            .into_iter()
            .filter_map(|read| match read {
                WireRead::Line(line) => Some(line),
                WireRead::Frame(frame) => Some(frame.to_line()),
                _ => None,
            })
            .collect()
    }
}

fn unload_project(id: u64) -> lpc_wire::WireServerMessage {
    lpc_wire::WireServerMessage::new(id, lpc_wire::ServerMsgBody::UnloadProject)
}

fn json_of(message: &lpc_wire::WireServerMessage) -> String {
    lpc_wire::json::to_string(message).expect("json")
}

/// One turn of the event loop: the scripted socket's delivery, the stream
/// callbacks and the controller's read pump.
async fn yield_once() {
    let promise = Promise::new(&mut |resolve, _reject| {
        let window = web_sys::window().expect("window");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0)
            .expect("set_timeout");
    });
    JsFuture::from(promise).await.expect("event loop turn");
}

/// Let queued microtasks, stream callbacks and the read pump run. Not a delay
/// and not a duration anything is asserted about: a zero-millisecond timeout
/// is one turn of the event loop, and a handful of turns is all a chain of
/// awaits between a socket frame and a drained line needs. An agent-driven
/// hidden tab is throttled to ~1 Hz, which is exactly why nothing here waits
/// on a clock.
async fn yield_to_event_loop() {
    for _ in 0..8 {
        let promise = Promise::new(&mut |resolve, _reject| {
            let window = web_sys::window().expect("window");
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0)
                .expect("set_timeout");
        });
        JsFuture::from(promise).await.expect("event loop turn");
    }
}
