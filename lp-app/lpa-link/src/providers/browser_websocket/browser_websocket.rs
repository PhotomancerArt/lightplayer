//! The Rust face of `browser_websocket.js`: one binding per export, and the
//! session descriptor it hands back.
//!
//! The JS owns the `WebSocket`, the bounded connect and the reconnect loop
//! (see its header for the three rules). What crosses into Rust is a session
//! id — a `u32`, because a `JsValue` cannot live in anything `Send` — plus
//! plain strings, and lp-link frames: one per binary message on the way in
//! ([`take_frames`]), one per message on the way out ([`write_frame`]).

use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::device_link::wire_tap::{WireTapDir, tap_wire};
use crate::providers::network_link::board_from_relay_socket_url;

#[wasm_bindgen(module = "/src/providers/browser_websocket/browser_websocket.js")]
extern "C" {
    #[wasm_bindgen(js_name = isSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_name = installWebsocketEvents)]
    fn js_install_websocket_events(on_connect: &Function, on_disconnect: &Function) -> bool;

    #[wasm_bindgen(js_name = openSession)]
    fn js_open_session(url: &str, options: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = presentSessions)]
    fn js_present_sessions() -> Array;

    #[wasm_bindgen(js_name = isConnected)]
    fn js_is_connected(id: u32) -> bool;

    #[wasm_bindgen(js_name = connect)]
    fn js_connect(id: u32) -> Promise;

    #[wasm_bindgen(js_name = settle)]
    fn js_settle(id: u32, ms: u32, until_up: bool) -> Promise;

    #[wasm_bindgen(js_name = markUp)]
    fn js_mark_up(id: u32);

    #[wasm_bindgen(js_name = giveUp)]
    fn js_give_up(id: u32, why: &str);

    #[wasm_bindgen(js_name = hold)]
    fn js_hold(id: u32, ms: u32);

    #[wasm_bindgen(js_name = setFallback)]
    fn js_set_fallback(id: u32, url: &str);

    #[wasm_bindgen(js_name = disconnect)]
    fn js_disconnect(id: u32) -> Promise;

    #[wasm_bindgen(js_name = forget)]
    fn js_forget(id: u32) -> Promise;

    #[wasm_bindgen(js_name = write, catch)]
    fn js_write(id: u32, frame: &[u8]) -> Result<bool, JsValue>;

    #[wasm_bindgen(js_name = takeFrames, catch)]
    fn js_take_frames(id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = onActivity, catch)]
    fn js_on_activity(id: u32, callback: &Closure<dyn FnMut()>) -> Result<Function, JsValue>;

    #[wasm_bindgen(js_name = takeErrors, catch)]
    fn js_take_errors(id: u32) -> Result<Array, JsValue>;
}

/// One network board this page holds a session for: on the LAN, or through
/// lightplayer.app's relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanSession {
    /// The JS session id: what every other call here takes.
    pub session: u32,
    /// The socket: the board's own (`ws://<board>/link`, a `lan:` endpoint)
    /// or the relay's browser leg (`wss://<host>/relay/board/<mac>`, a
    /// `relay:` one).
    pub url: String,
    pub connected: bool,
}

impl LanSession {
    /// Whether this session goes through the relay rather than the LAN.
    pub fn is_relay(&self) -> bool {
        board_from_relay_socket_url(&self.url).is_some()
    }
}

/// The wire tap's name for the transport at `url`: `relay` or `lan`.
pub(crate) fn tap_tag(url: &str) -> &'static str {
    if board_from_relay_socket_url(url).is_some() {
        "relay"
    } else {
        "lan"
    }
}

/// Whether this page has a `WebSocket` at all.
pub fn is_supported() -> bool {
    js_is_supported()
}

/// Install the presence edges (see `browser_websocket.js`): `on_connect`
/// when a session becomes present, `on_disconnect` when one drops. Once per
/// page; returns whether this call installed them.
pub fn install_websocket_events(on_connect: &Function, on_disconnect: &Function) -> bool {
    js_install_websocket_events(on_connect, on_disconnect)
}

/// Start (or keep) the session for the board at `url`: it connects now and
/// keeps reconnecting, and is present once connected.
pub fn open_session(url: &str) -> Result<LanSession, String> {
    session_from(&js_open_session(url, &JsValue::UNDEFINED))
}

/// Start (or keep) the session through the relay's browser leg at `url`.
/// Everything it says starts `relay …` (its drop is `relay link lost: …`),
/// and a close with one of `final_codes` (the relay's refusals) ends it
/// rather than redialling — except, while the session is held ([`hold`]),
/// a code in `hold_codes`, which is redialled after its delay (ms).
pub fn open_relay_session(
    url: &str,
    final_codes: &[u16],
    hold_codes: &[(u16, u32)],
) -> Result<LanSession, String> {
    let options = js_sys::Object::new();
    let codes: Array = final_codes
        .iter()
        .map(|code| JsValue::from(*code))
        .collect();
    let held: Array = hold_codes
        .iter()
        .map(|(code, delay_ms)| {
            let pair = Array::new();
            pair.push(&JsValue::from(*code));
            pair.push(&JsValue::from(*delay_ms));
            JsValue::from(pair)
        })
        .collect();
    let _ = Reflect::set(
        &options,
        &JsValue::from_str("kind"),
        &JsValue::from_str("relay"),
    );
    let _ = Reflect::set(&options, &JsValue::from_str("finalCodes"), &codes);
    let _ = Reflect::set(&options, &JsValue::from_str("holdCodes"), &held);
    session_from(&js_open_session(url, &options))
}

/// Hold `session` for `ms` from now: a close with one of its hold codes is
/// redialled meanwhile (see [`open_relay_session`]). A session with none
/// is unchanged.
pub fn hold(session: u32, ms: u32) {
    js_hold(session, ms);
}

/// The sessions Studio should hold a link for right now.
pub fn present_sessions() -> Vec<LanSession> {
    js_present_sessions()
        .iter()
        .filter_map(|value| session_from(&value).ok())
        .collect()
}

/// Forget a session: closed, no longer listed or reconnected.
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

/// Connect a session now because someone asked, and wait up to `settle_ms`
/// for the connection to prove itself (`settle` in the JS): the board sends
/// its first frame, or turns the connection away (a busy board's close 1013
/// right after the upgrade). `Err` carries the socket's own words — a
/// connect that failed or timed out, or the drop. The session is left as it
/// is either way; the caller decides whether to keep it.
pub async fn connect_and_settle(session: u32, settle_ms: u32) -> Result<(), String> {
    connect(session).await?;
    JsFuture::from(js_settle(session, settle_ms, false))
        .await
        .map(|_| ())
        .map_err(|error| error_message(&error))
}

/// [`connect_and_settle`], waiting for the secure link above the socket to
/// come up rather than for the first frame: a relay session, where the first
/// frame proves only that the relay passed the socket on. A session that
/// runs out of keys gives up, and `Err` carries its words.
pub async fn connect_until_up(session: u32, settle_ms: u32) -> Result<(), String> {
    connect(session).await?;
    JsFuture::from(js_settle(session, settle_ms, true))
        .await
        .map(|_| ())
        .map_err(|error| error_message(&error))
}

/// The secure link on the session's current connection is up.
pub(crate) fn mark_up(session: u32) {
    js_mark_up(session);
}

/// End the session for good with `why` (heard as `<kind> link lost: why`).
pub(crate) fn give_up(session: u32, why: &str) {
    js_give_up(session, why);
}

/// Tell the page where else the session's board answers (its `.local`
/// socket): tried beside the session's own URL once that has stopped
/// answering for a while. The session keeps its URL (its identity).
pub(crate) fn set_fallback(session: u32, url: &str) {
    js_set_fallback(session, url);
}

pub(crate) async fn disconnect(session: u32) {
    let _ = JsFuture::from(js_disconnect(session)).await;
}

/// Send one lp-link frame as one binary message. `Ok(false)`: the link was
/// not up, and the frame never left (the session's error says why). `tag`:
/// the wire tap's name for the transport ([`tap_tag`]).
pub(crate) fn write_frame(session: u32, frame: &[u8], tag: &'static str) -> Result<bool, String> {
    let sent = js_write(session, frame).map_err(|error| error_message(&error))?;
    if sent {
        tap_wire(WireTapDir::Tx, tag, session, frame);
    }
    Ok(sent)
}

/// What [`take_frames`] found in a session.
pub(crate) struct TakenFrames {
    /// The board's link socket: the address its keys are looked up by.
    pub url: String,
    /// The JS connection generation the frames belong to: it moves with
    /// every connect, drop and close, and a new one is a new lp-link session.
    pub generation: u32,
    /// Connected: frames may be written now.
    pub connected: bool,
    /// Every message since the last take, one frame each, in order.
    pub frames: Vec<Vec<u8>>,
}

/// Every frame the board sent since the last take. `Err` for a session the
/// page no longer has (forgotten).
pub(crate) fn take_frames(session: u32) -> Result<TakenFrames, String> {
    let value = js_take_frames(session).map_err(|error| error_message(&error))?;
    let frames: Vec<Vec<u8>> = Reflect::get(&value, &JsValue::from_str("frames"))
        .ok()
        .map(|frames| {
            Array::from(&frames)
                .iter()
                .map(|frame| Uint8Array::new(&frame).to_vec())
                .collect()
        })
        .unwrap_or_default();
    let url = string_field(&value, "url").unwrap_or_default();
    let tag = tap_tag(&url);
    for frame in &frames {
        tap_wire(WireTapDir::Rx, tag, session, frame);
    }
    Ok(TakenFrames {
        url,
        generation: number_field(&value, "generation").unwrap_or(0.0) as u32,
        connected: bool_field(&value, "connected").unwrap_or(false),
        frames,
    })
}

/// Subscribe `callback` to the session's activity (a message, the link up or
/// down). Answers the unsubscribe function.
pub(crate) fn on_activity(
    session: u32,
    callback: &Closure<dyn FnMut()>,
) -> Result<Function, String> {
    js_on_activity(session, callback).map_err(|error| error_message(&error))
}

pub(crate) fn take_errors(session: u32) -> Result<Vec<String>, String> {
    js_take_errors(session)
        .map(|array| array.iter().filter_map(|value| value.as_string()).collect())
        .map_err(|error| error_message(&error))
}

fn session_from(value: &JsValue) -> Result<LanSession, String> {
    let session = number_field(value, "id")
        .ok_or_else(|| "wi-fi session descriptor has no id".to_string())?;
    Ok(LanSession {
        session: session as u32,
        url: string_field(value, "url").unwrap_or_default(),
        connected: bool_field(value, "connected").unwrap_or(false),
    })
}

fn string_field(value: &JsValue, name: &str) -> Option<String> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()
        .and_then(|field| field.as_string())
}

fn number_field(value: &JsValue, name: &str) -> Option<f64> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()
        .and_then(|field| field.as_f64())
}

fn bool_field(value: &JsValue, name: &str) -> Option<bool> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()
        .and_then(|field| field.as_bool())
}

fn error_message(error: &JsValue) -> String {
    string_field(error, "message")
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}
