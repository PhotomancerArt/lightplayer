//! The Rust face of `browser_ble.js`: one binding per export, and the
//! session descriptor it hands back.
//!
//! The JS owns the `BluetoothDevice`, the GATT connection, the write queue
//! and the reconnect loop (see its header for the four rules and the G1
//! measurements behind them). What crosses into Rust is a session id — a
//! `u32`, because a `JsValue` cannot live in anything `Send` — plus plain
//! strings and bytes.

use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::device_link::wire_tap::{WireTapDir, tap_wire};

#[wasm_bindgen(module = "/src/providers/browser_ble/browser_ble.js")]
extern "C" {
    #[wasm_bindgen(js_name = isSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_name = availability)]
    fn js_availability() -> Promise;

    #[wasm_bindgen(js_name = installBleEvents)]
    fn js_install_ble_events(on_connect: &Function, on_disconnect: &Function) -> bool;

    #[wasm_bindgen(js_name = recheckAll)]
    fn js_recheck_all(why: &str);

    #[wasm_bindgen(js_name = requestDevice)]
    fn js_request_device() -> Promise;

    #[wasm_bindgen(js_name = restoreGrantedDevices)]
    fn js_restore_granted_devices() -> Promise;

    #[wasm_bindgen(js_name = presentDevices)]
    fn js_present_devices() -> Array;

    #[wasm_bindgen(js_name = isConnected)]
    fn js_is_connected(id: u32) -> bool;

    #[wasm_bindgen(js_name = connect)]
    fn js_connect(id: u32) -> Promise;

    #[wasm_bindgen(js_name = disconnect)]
    fn js_disconnect(id: u32) -> Promise;

    #[wasm_bindgen(js_name = forget)]
    fn js_forget(id: u32) -> Promise;

    #[wasm_bindgen(js_name = write, catch)]
    fn js_write(id: u32, bytes: &[u8]) -> Result<bool, JsValue>;

    #[wasm_bindgen(js_name = takeBytes, catch)]
    fn js_take_bytes(id: u32) -> Result<Vec<u8>, JsValue>;

    #[wasm_bindgen(js_name = takeErrors, catch)]
    fn js_take_errors(id: u32) -> Result<Array, JsValue>;
}

/// One Bluetooth device this page holds a session for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BleDevice {
    /// The JS session id: what every other call here takes.
    pub session: u32,
    /// Web Bluetooth's `BluetoothDevice.id` — opaque, per origin, stable
    /// while the permission lasts. The `ble:` endpoint is built from it.
    pub device_id: String,
    /// The advertised name, or "Bluetooth device" when there is none.
    pub name: String,
    pub connected: bool,
}

/// Which browser the page is in, for the one case the Add verb has specific
/// words for (M5 S4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleBrowser {
    /// Brave: Web Bluetooth exists behind `brave://flags/#brave-web-bluetooth-api`.
    Brave,
    /// Any browser on iOS/iPadOS: WebKit, no Web Bluetooth — Bluefy brings its own.
    Ios,
    /// Firefox: no Web Bluetooth.
    Firefox,
    /// Desktop Safari: no Web Bluetooth.
    Safari,
    Other,
}

/// What `availability()` found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleAvailability {
    /// `navigator.bluetooth` exists.
    pub supported: bool,
    /// `getAvailability()`'s answer; `None` when it could not be asked.
    pub available: Option<bool>,
    pub browser: BleBrowser,
}

/// Whether this page has Web Bluetooth at all (a synchronous check).
pub fn is_supported() -> bool {
    js_is_supported()
}

/// Ask what the Add verb should say about Bluetooth here.
pub async fn availability() -> BleAvailability {
    let value = JsFuture::from(js_availability())
        .await
        .unwrap_or(JsValue::NULL);
    let supported = bool_field(&value, "supported").unwrap_or(false);
    let available = bool_field(&value, "available");
    let browser = match string_field(&value, "browser").as_deref() {
        Some("brave") => BleBrowser::Brave,
        Some("ios") => BleBrowser::Ios,
        Some("firefox") => BleBrowser::Firefox,
        Some("safari") => BleBrowser::Safari,
        _ => BleBrowser::Other,
    };
    BleAvailability {
        supported,
        available,
        browser,
    }
}

/// Install the presence edges (see `browser_ble.js`): `on_connect` when a
/// session becomes present, `on_disconnect` when one drops. Once per page;
/// returns whether this call installed them.
pub fn install_ble_events(on_connect: &Function, on_disconnect: &Function) -> bool {
    js_install_ble_events(on_connect, on_disconnect)
}

/// Re-read every session's link now, as becoming visible does.
pub fn recheck_all(why: &str) {
    js_recheck_all(why);
}

/// The Bluetooth chooser, then a bounded connect. `Ok(None)` = the user
/// closed the chooser.
pub async fn request_device() -> Result<Option<BleDevice>, String> {
    match JsFuture::from(js_request_device()).await {
        Ok(value) => device_from(&value).map(Some),
        Err(error) if error_name(&error).as_deref() == Some("NotFoundError") => Ok(None),
        Err(error) => Err(error_message(&error)),
    }
}

/// Kick the quiet one-shot re-connect of the devices this origin was already
/// granted. Idempotent per page; successes arrive as presence edges.
pub async fn restore_granted_devices() {
    let _ = JsFuture::from(js_restore_granted_devices()).await;
}

/// The sessions Studio should hold a link for right now.
pub fn present_devices() -> Vec<BleDevice> {
    js_present_devices()
        .iter()
        .filter_map(|value| device_from(&value).ok())
        .collect()
}

/// Revoke the browser's permission for a session's device. `true` when it
/// was actually revoked.
pub async fn forget(session: u32) -> bool {
    JsFuture::from(js_forget(session))
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn is_connected(session: u32) -> bool {
    js_is_connected(session)
}

pub(crate) async fn connect(session: u32) -> Result<(), String> {
    JsFuture::from(js_connect(session))
        .await
        .map(|_| ())
        .map_err(|error| error_message(&error))
}

pub(crate) async fn disconnect(session: u32) {
    let _ = JsFuture::from(js_disconnect(session)).await;
}

pub(crate) fn write(session: u32, bytes: &[u8]) -> Result<bool, String> {
    let queued = js_write(session, bytes).map_err(|error| error_message(&error))?;
    // Only what the link took: a link that is down answers `false` and the
    // bytes never left.
    if queued {
        tap_wire(WireTapDir::Tx, "ble", session, bytes);
    }
    Ok(queued)
}

pub(crate) fn take_bytes(session: u32) -> Result<Vec<u8>, String> {
    let bytes = js_take_bytes(session).map_err(|error| error_message(&error))?;
    tap_wire(WireTapDir::Rx, "ble", session, &bytes);
    Ok(bytes)
}

pub(crate) fn take_errors(session: u32) -> Result<Vec<String>, String> {
    js_take_errors(session)
        .map(|array| array.iter().filter_map(|value| value.as_string()).collect())
        .map_err(|error| error_message(&error))
}

fn device_from(value: &JsValue) -> Result<BleDevice, String> {
    let session = Reflect::get(value, &JsValue::from_str("id"))
        .ok()
        .and_then(|id| id.as_f64())
        .ok_or_else(|| "bluetooth session descriptor has no id".to_string())?;
    Ok(BleDevice {
        session: session as u32,
        device_id: string_field(value, "deviceId").unwrap_or_default(),
        name: string_field(value, "name").unwrap_or_else(|| "Bluetooth device".to_string()),
        connected: bool_field(value, "connected").unwrap_or(false),
    })
}

fn string_field(value: &JsValue, name: &str) -> Option<String> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()
        .and_then(|field| field.as_string())
}

fn bool_field(value: &JsValue, name: &str) -> Option<bool> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()
        .and_then(|field| field.as_bool())
}

fn error_name(error: &JsValue) -> Option<String> {
    string_field(error, "name")
}

fn error_message(error: &JsValue) -> String {
    string_field(error, "message")
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}
