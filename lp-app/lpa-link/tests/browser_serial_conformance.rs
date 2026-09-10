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
//! | read-pump error paths | [`the_read_pump_reports_a_lost_device_and_the_port_reopens`] | `browser_esp32_device_controller.js:380-410` (`readPump`) |
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

    #[wasm_bindgen(js_name = takeLines)]
    fn js_take_lines(id: u32) -> Promise;

    #[wasm_bindgen(js_name = takeErrors)]
    fn js_take_errors(id: u32) -> Promise;

    #[wasm_bindgen(js_name = writeLine)]
    fn js_write_line(id: u32, line: &str) -> Promise;

    #[wasm_bindgen(js_name = installScripted)]
    fn js_install_scripted(board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = installLive)]
    fn js_install_live(base_url: &str, board_ids: &Array) -> Promise;

    #[wasm_bindgen(js_name = uninstallShim)]
    fn js_uninstall_shim() -> Promise;

    #[wasm_bindgen(js_name = busBoardIds)]
    fn js_bus_board_ids() -> Promise;

    #[wasm_bindgen(js_name = controlLog)]
    fn js_control_log(board_id: &str) -> String;

    #[wasm_bindgen(js_name = receivedBytes)]
    fn js_received_bytes(board_id: &str) -> String;

    #[wasm_bindgen(js_name = rebootCount)]
    fn js_reboot_count(board_id: &str) -> i32;

    #[wasm_bindgen(js_name = deliverBytes)]
    fn js_deliver_bytes(board_id: &str, text: &str);

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

/// Install the shim over a scripted door holding `boards`. The live half of
/// the suite (a `just` recipe, never CI) points the same assertions at a real
/// `lp-cli emu serve` through `LP_EMU_SERVE_URL`.
async fn shim_over(boards: &[&str]) {
    drain_sessions().await;
    let _ = JsFuture::from(js_uninstall_shim()).await;
    match option_env!("LP_EMU_SERVE_URL") {
        Some(url) if !url.is_empty() => {
            let ids = Array::new();
            for board in boards {
                ids.push(&JsValue::from_str(board));
            }
            JsFuture::from(js_install_live(url, &ids))
                .await
                .expect("install the shim over a live `lp-cli emu serve`");
        }
        _ => {
            let ids = Array::new();
            for board in boards {
                ids.push(&JsValue::from_str(board));
            }
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
/// scripted door. Set by `scripts/emu/browser-conformance-live.sh`; a few
/// claims can only be made against the scripted door (they need to reach
/// inside it) and say so in the log rather than pretending to have run.
fn live_backing() -> bool {
    option_env!("LP_EMU_SERVE_URL").is_some_and(|url| !url.is_empty())
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
    if live_backing() {
        log("SKIPPED against the live door (this claim is pinned by the scripted half)");
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
        boolean(js_port_readable_is_null(&after)).await,
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
    if live_backing() {
        log("SKIPPED against the live door (this claim is pinned by the scripted half)");
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
    let lines = strings(js_take_lines(id)).await;
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
    let lines_after = strings(js_take_lines(id)).await;
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
    if live_backing() {
        log("SKIPPED against the live door (this claim is pinned by the scripted half)");
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

/// The bytes go both ways through the real controller: `writeLine` reaches
/// the board's byte channel unchanged, and what the board says comes back as
/// lines.
#[wasm_bindgen_test]
async fn bytes_travel_both_ways_through_the_controller() {
    if live_backing() {
        log("SKIPPED against the live door (this claim is pinned by the scripted half)");
        return;
    }
    shim_over(&["c6-a"]).await;
    let board = board_ids().await.first().cloned().expect("a board");
    let id = granted_sessions().await[0].id;

    open_port(id, false).await.expect("openPort");
    yield_to_event_loop().await;
    let greeting = strings(js_take_lines(id)).await;
    assert!(
        greeting.iter().any(|line| line.contains("hello")),
        "the board's greeting never reached the read pump: {greeting:?}"
    );

    JsFuture::from(js_write_line(id, "M! {\"ping\":1}\n"))
        .await
        .expect("writeLine");
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
