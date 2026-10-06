//! The Rust side of `browser_serial.js`: Web Serial ports by id, and each
//! open port's lp-link end.
//!
//! # One link per port, serviced by its own loop
//!
//! Since `WIRE_PROTO_VERSION` 30 the board's USB serial link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, P4). Every open port
//! keeps ONE [`LinkPortService`] ([`PORTS`]) for as long as the controller's
//! buffer generation lasts; a reopen is a new generation, so a new link with
//! a new nonce, and the board starts a new session with it. A loop per port
//! (`link_port_edge::spawn_service_loop`, at most every
//! [`SERVICE_TICK_CAP`]) pulls the controller's bytes, feeds the link, and
//! writes its frames with a BYTES write (`writeBytes`) — resends and
//! acknowledgements happen whether or not anyone is reading. Every
//! [`take_reads`] services the port once more on the way, and so does every
//! chunk the read pump buffers (`onBytes`): a hidden tab throttles the loop's
//! timer to a second or worse, but not a stream read, so the board's frames
//! are still acknowledged promptly and it does not give up on the session.
//!
//! # Which preset a port's link runs
//!
//! A port's link is tuned for what is on the other end of the cable, and the
//! only thing the page knows about that before a hello is the USB vendor id
//! the port enumerated with — the same id the chooser filter and the
//! granted-ports sweep already read. [`port_handle`] records it for every
//! port it hands out, and a new link takes
//! [`link_config_for_usb_vendor`]'s answer: `uart()` behind a bridge (the
//! classic ESP32's CH340), `usb()` on Espressif's native USB (plan
//! `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, P4 — the native host
//! makes the same call from the same id).
//!
//! Above the link nothing changed shape (D1): [`take_reads`] hands out the
//! same [`WireRead`]s the `M!` reader did, to whichever drainer holds the
//! port (the model's pump, or a borrowed conversation — D2), and a request
//! goes out as one link message ([`send_client_json`]) instead of an `M!`
//! line. The capture tee (`?wire-capture=1`) and the recorder tap still see
//! every raw byte, both ways.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, Promise, Reflect, Uint8Array};
use lpa_devices::link::ResetKind;
use lpc_wire::lp_link::{LinkConfig, Micros};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::LinkError;
use crate::device_link::link_port_edge::{
    SERVICE_TICK_CAP, now_micros, random_nonce, spawn_service_loop,
};
use crate::device_link::link_port_service::LinkPortService;
use crate::device_link::wire_capture::capture_wire_bytes;
use crate::device_link::wire_reader::{WireRead, device_log_level, packed_replies_wanted};
use crate::device_link::wire_tap::{WireTapDir, tap_wire};
use crate::provider::usb_vendors::link_config_for_usb_vendor;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialPortHandle {
    pub id: u32,
    pub label: String,
    /// USB vendor:product ids from `SerialPort.getInfo()`, absent on
    /// non-USB ports (and on browsers that expose nothing). Carried as
    /// fields — not only inside `label`'s prose — so a grant can be
    /// matched against a board's declared `usb_bridge` (D7).
    pub usb_vendor_id: Option<u16>,
    pub usb_product_id: Option<u16>,
}

impl BrowserSerialPortHandle {
    /// The `(vid, pid)` pair when the browser exposed both.
    pub fn usb_vid_pid(&self) -> Option<(u16, u16)> {
        Some((self.usb_vendor_id?, self.usb_product_id?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialProtocolOpenResult {
    pub logs: Vec<String>,
    pub progress: Vec<BrowserSerialProtocolProgress>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialProtocolProgress {
    pub label: String,
    pub completed_steps: u32,
    pub total_steps: Option<u32>,
    pub percent: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialResetResult {
    pub logs: Vec<String>,
}

#[wasm_bindgen(module = "/src/providers/browser_serial_esp32/browser_serial.js")]
extern "C" {
    #[wasm_bindgen(js_name = isSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_name = installSerialEvents)]
    fn js_install_serial_events(
        on_connect: &js_sys::Function,
        on_disconnect: &js_sys::Function,
    ) -> bool;

    #[wasm_bindgen(js_name = requestPort)]
    fn js_request_port() -> Promise;

    #[wasm_bindgen(js_name = getGrantedPorts)]
    fn js_get_granted_ports() -> Promise;

    #[wasm_bindgen(js_name = openPort)]
    fn js_open(id: u32, baud_rate: u32, reset: bool, reset_kind: &str) -> Promise;

    #[wasm_bindgen(js_name = writeBytes, catch)]
    fn js_write_bytes(id: u32, bytes: &[u8]) -> Result<Promise, JsValue>;

    #[wasm_bindgen(js_name = takeBytes, catch)]
    fn js_take_bytes(id: u32) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = onBytes, catch)]
    fn js_on_bytes(id: u32, callback: &Closure<dyn FnMut()>) -> Result<js_sys::Function, JsValue>;

    #[wasm_bindgen(js_name = takeErrors)]
    fn js_take_errors(id: u32) -> Array;

    #[wasm_bindgen(js_name = releasePort)]
    fn js_release(id: u32) -> Promise;

    #[wasm_bindgen(js_name = resetAndRead)]
    fn js_reset_and_read(id: u32, baud_rate: u32, read_window_ms: u32, reset_kind: &str)
    -> Promise;

    #[wasm_bindgen(js_name = closePort)]
    fn js_close(id: u32) -> Promise;

    #[wasm_bindgen(js_name = forgetPort)]
    fn js_forget_port(id: u32) -> Promise;
}

pub fn is_supported() -> bool {
    js_is_supported()
}

/// M6 (D32): install the `navigator.serial` hotplug listeners —
/// `connect` fires when a granted port (re)appears (the auto-connect
/// sweep's trigger), `disconnect` when one leaves (Gone handling
/// hastens). At most once per page; returns whether listeners are live
/// (`false` when Web Serial is unsupported).
pub fn install_serial_events(
    on_connect: &js_sys::Function,
    on_disconnect: &js_sys::Function,
) -> bool {
    js_install_serial_events(on_connect, on_disconnect)
}

/// Serial ports the user has ALREADY granted this origin
/// (`navigator.serial.getPorts()`) — no permission prompt is shown. Each
/// granted port is registered as an openable JS session; repeat calls return
/// the same handles (the JS side matches sessions by port identity). Empty
/// when Web Serial is unsupported or the probe fails.
pub async fn granted_ports() -> Result<Vec<BrowserSerialPortHandle>, LinkError> {
    let value = JsFuture::from(js_get_granted_ports())
        .await
        .map_err(js_error)?;
    let array = Array::from(&value);
    let mut ports = Vec::with_capacity(array.length() as usize);
    for entry in array.iter() {
        ports.push(port_handle(&entry)?);
    }
    Ok(ports)
}

pub async fn request_port() -> Result<BrowserSerialPortHandle, LinkError> {
    let value = JsFuture::from(js_request_port())
        .await
        .map_err(js_request_port_error)?;
    port_handle(&value)
}

/// A port descriptor from the JS layer, as a handle — and the port's vendor
/// id remembered for the link it will open (see the module docs). Every port
/// id Rust ever holds came through here.
fn port_handle(value: &JsValue) -> Result<BrowserSerialPortHandle, LinkError> {
    let handle = BrowserSerialPortHandle {
        id: reflect_u32(value, "id")?,
        label: reflect_string(value, "label")?,
        usb_vendor_id: reflect_optional_u32(value, "usbVendorId")?.map(|id| id as u16),
        usb_product_id: reflect_optional_u32(value, "usbProductId")?.map(|id| id as u16),
    };
    PORT_USB_VENDORS.with(|vendors| vendors.borrow_mut().insert(handle.id, handle.usb_vendor_id));
    Ok(handle)
}

/// The JS `runReset` sequence each model reset kind selects.
///
/// The controller (`browser_esp32_device_controller.js`) is the only place
/// these run, and the JS layer has no CI (docs/debt/
/// web-serial-js-untestable.md), so the table lives here too — a rename on
/// either side that is not mirrored is a silently DIFFERENT reset, which is
/// worse than a failed one.
///
/// | [`ResetKind`] | JS name | sequence |
/// |---|---|---|
/// | `Normal` | `"normal"` | `D0 W100 R1 W100 R0` |
/// | `RtsOnly` | `"rts-only"` | `R1 W100 R0` |
/// | `UsbJtagDownload` | `"usb-jtag-download"` | `R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0` |
/// | `BothThenDrop` | `"both-then-drop"` | whole-status: (0,0) (1,1) W100 (0,1) (0,0) |
///
/// `BothThenDrop` is the CH34x sequence and the only one written as
/// whole-status pairs: the WCH macOS driver ignores single-pin writes, and
/// (DTR asserted, RTS released) selects the ROM bootloader rather than
/// rebooting the app — so that crossing never appears in it.
fn reset_kind_js_name(kind: ResetKind) -> &'static str {
    match kind {
        ResetKind::Normal => "normal",
        ResetKind::RtsOnly => "rts-only",
        ResetKind::UsbJtagDownload => "usb-jtag-download",
        ResetKind::BothThenDrop => "both-then-drop",
    }
}

/// Open the protocol port, optionally resetting the board on the way in.
///
/// `reset: None` opens WITHOUT any reset — what identify needs, because a
/// USB-Serial-JTAG chip re-enumerates on a hard reset and kills the port
/// that was just opened.
pub async fn open(
    id: u32,
    baud_rate: u32,
    reset: Option<ResetKind>,
) -> Result<BrowserSerialProtocolOpenResult, LinkError> {
    let kind = reset.unwrap_or(ResetKind::Normal);
    let value = JsFuture::from(js_open(
        id,
        baud_rate,
        reset.is_some(),
        reset_kind_js_name(kind),
    ))
    .await
    .map_err(js_error)?;
    // The controller cleared its buffer (a new generation): this services
    // the port once, which makes its link — a fresh nonce, so the board
    // starts a new session — and starts the port's loop.
    service(id);
    Ok(BrowserSerialProtocolOpenResult {
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

/// Queue one request (its JSON, no `M!`, no newline) on the port's link and
/// write what the link has to send now. Errors when the port is not open or
/// the link will not take the message (its send budget is full, or the
/// message is larger than a link message may be).
pub fn send_client_json(id: u32, json: &str) -> Result<(), LinkError> {
    if matches!(service(id), Serviced::Gone | Serviced::Closed) {
        return Err(LinkError::other("Serial port is not open."));
    }
    PORTS
        .with(|ports| {
            ports
                .borrow_mut()
                .get_mut(&id)
                .map(|port| port.service.send_client_json(json))
        })
        .unwrap_or_else(|| Err("Serial port is not open.".to_string()))
        .map_err(LinkError::other)?;
    service(id);
    Ok(())
}

/// Queue one channel-3 (update) message on the port's link and write what
/// the link has to send now. `Ok(false)`: the board has not announced the
/// update channel this session, so nothing was queued (DS9; the link notes
/// it). Errors like [`send_client_json`]'s.
pub fn send_update(id: u32, message: &[u8]) -> Result<bool, LinkError> {
    if matches!(service(id), Serviced::Gone | Serviced::Closed) {
        return Err(LinkError::other("Serial port is not open."));
    }
    let queued = PORTS
        .with(|ports| {
            ports
                .borrow_mut()
                .get_mut(&id)
                .map(|port| port.service.send_update(message))
        })
        .unwrap_or_else(|| Err("Serial port is not open.".to_string()))
        .map_err(LinkError::other)?;
    service(id);
    Ok(queued)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
}

thread_local! {
    /// One link per open port, shared by every drainer of that port — the
    /// model's link pump and, while it holds the wire, a conversation (the
    /// editor lens, a push). See the module docs.
    static PORTS: RefCell<HashMap<u32, ServedPort>> = RefCell::new(HashMap::new());

    /// The USB vendor id each port handed out enumerated with (`None`: the
    /// browser exposed none), kept from [`port_handle`] so a port's link can
    /// be tuned for the board behind it. Outlives a close: the port, and so
    /// its vendor, is the same the next time it opens.
    static PORT_USB_VENDORS: RefCell<HashMap<u32, Option<u16>>> = RefCell::new(HashMap::new());
}

/// A port's link, the controller buffer generation it is for, and whether
/// its loop is running.
struct ServedPort {
    generation: Option<u32>,
    service: LinkPortService,
    running: Rc<Cell<bool>>,
    /// The read pump's "bytes arrived" subscription, once made.
    wake: Option<WakeOnBytes>,
}

/// A subscription to the controller's read pump that services the port.
struct WakeOnBytes {
    callback: Option<Closure<dyn FnMut()>>,
    stop: js_sys::Function,
}

impl Drop for WakeOnBytes {
    /// Unsubscribe, and LEAK the closure rather than free it: the port can be
    /// dropped from inside its own callback (a service pass that finds the
    /// session gone), and freeing a closure JS is still running is undefined.
    /// One small closure per port session.
    fn drop(&mut self) {
        let _ = self.stop.call0(&JsValue::NULL);
        if let Some(callback) = self.callback.take() {
            callback.forget();
        }
    }
}

/// A link for a newly opened port: the preset for the board behind it, a
/// fresh nonce, and the page's wire flags as they are now.
fn fresh_service(id: u32) -> LinkPortService {
    LinkPortService::new(
        port_link_config(id),
        random_nonce(),
        packed_replies_wanted(),
        device_log_level(),
    )
}

/// The preset a new link on port `id` runs, from the vendor id the port
/// enumerated with (see the module docs). A port this module never described
/// has no vendor on record and takes `usb()`, as every port did before.
fn port_link_config(id: u32) -> LinkConfig {
    let vendor = PORT_USB_VENDORS.with(|vendors| vendors.borrow().get(&id).copied().flatten());
    link_config_for_usb_vendor(vendor)
}

/// The preset port `id`'s current link runs; `None` while the port has no
/// link (never serviced, or released).
pub fn link_config(id: u32) -> Option<LinkConfig> {
    PORTS.with(|ports| {
        ports
            .borrow()
            .get(&id)
            .map(|port| port.service.config().clone())
    })
}

/// What one service pass found.
enum Serviced {
    /// No such session in the page (forgotten): the port's link is dropped.
    Gone,
    /// The session exists but the port is not open for traffic.
    Closed,
    /// Open; come back within this long.
    Open(Micros),
}

/// One pass over a port: pull what the controller read, feed the link, and
/// write the frames it has to send. See the module docs.
fn service(id: u32) -> Serviced {
    let Ok(taken) = js_take_bytes(id) else {
        PORTS.with(|ports| ports.borrow_mut().remove(&id));
        return Serviced::Gone;
    };
    let generation = reflect_value(&taken, "generation")
        .ok()
        .and_then(|value| value.as_f64())
        .map(|value| value as u32);
    let open = reflect_value(&taken, "open")
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let bytes = reflect_value(&taken, "bytes")
        .ok()
        .map(|value| Uint8Array::new(&value).to_vec())
        .unwrap_or_default();
    if !bytes.is_empty() {
        // Dev-only tee (`?wire-capture=1`): every byte the pump read, before
        // the link sees it, in order. A no-op unless the page turned it on.
        if capture_wire_bytes(&bytes) {
            console_warn(&format!(
                "wire capture is full ({} bytes); dropping what the port reads from here on",
                crate::device_link::wire_capture::WIRE_CAPTURE_CAP
            ));
        }
        tap_wire(WireTapDir::Rx, "serial", id, &bytes);
    }
    let now = now_micros();
    let (frames, wake, running) = PORTS.with(|ports| {
        let mut ports = ports.borrow_mut();
        let port = ports.entry(id).or_insert_with(|| ServedPort {
            generation,
            service: fresh_service(id),
            running: Rc::default(),
            wake: None,
        });
        if port.generation != generation {
            // The controller cleared its buffer: a (re)open. The session,
            // what was half read and what the board had agreed belong to the
            // previous port generation.
            port.generation = generation;
            port.service = fresh_service(id);
        }
        port.service.on_bytes(now, &bytes);
        let mut frames = Vec::new();
        if open {
            port.service
                .transmit(now, |frame| frames.push(frame.to_vec()));
        }
        (
            frames,
            port.service.wake_in(now, SERVICE_TICK_CAP),
            Rc::clone(&port.running),
        )
    });
    // No borrow is held past here: the writes and the loop call back into
    // JS, and the loop into `service`.
    for frame in frames {
        write_frame(id, &frame);
    }
    if !open {
        return Serviced::Closed;
    }
    wake_on_bytes(id);
    spawn_service_loop(running, move || match service(id) {
        Serviced::Open(wake) => Some(wake),
        Serviced::Gone | Serviced::Closed => None,
    });
    Serviced::Open(wake)
}

/// Subscribe the port to its read pump's "bytes arrived", once per port
/// session (see the module docs).
fn wake_on_bytes(id: u32) {
    let subscribed = PORTS.with(|ports| {
        ports
            .borrow()
            .get(&id)
            .is_none_or(|port| port.wake.is_some())
    });
    if subscribed {
        return;
    }
    let callback = Closure::<dyn FnMut()>::new(move || {
        service(id);
    });
    let Ok(stop) = js_on_bytes(id, &callback) else {
        return;
    };
    let wake = WakeOnBytes {
        callback: Some(callback),
        stop,
    };
    PORTS.with(|ports| {
        if let Some(port) = ports.borrow_mut().get_mut(&id) {
            port.wake = Some(wake);
        }
    });
}

/// Write one link frame. Fire and forget: the controller's writer queues
/// writes in call order, and a port that cannot take one is dying — the
/// read pump says so (`takeErrors`), and the link resends what was lost.
fn write_frame(id: u32, frame: &[u8]) {
    tap_wire(WireTapDir::Tx, "serial", id, frame);
    if let Ok(promise) = js_write_bytes(id, frame) {
        spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
}

/// Everything the port's link has read since the last drain, in order:
/// console lines, wire messages (packed or not) and link resets. Services
/// the port first, so a drainer never waits a tick for bytes already in the
/// page.
pub fn take_reads(id: u32) -> Vec<WireRead> {
    service(id);
    PORTS.with(|ports| {
        ports
            .borrow_mut()
            .get_mut(&id)
            .map(|port| port.service.take_reads())
            .unwrap_or_default()
    })
}

/// The board's channel-3 (update) messages since the last take, this link
/// session's only. Drained by the model's link pump alone: a conversation
/// borrowing the wire never sees them.
pub fn take_updates(id: u32) -> Vec<Vec<u8>> {
    PORTS.with(|ports| {
        ports
            .borrow_mut()
            .get_mut(&id)
            .map(|port| port.service.take_updates())
            .unwrap_or_default()
    })
}

/// What the port's link has said about itself since the last ask (up, a
/// stall, the packed opt-in's outcome — one note per change). Drained by the
/// model's link pump, or by the editor lens's io while it holds the wire.
pub fn take_wire_notes(id: u32) -> Vec<String> {
    PORTS.with(|ports| {
        ports
            .borrow_mut()
            .get_mut(&id)
            .map(|port| port.service.take_notes())
            .unwrap_or_default()
    })
}

/// [`take_reads`], as whole lines: a message is its `M!{json}` line whichever
/// form it came in, and a reset is dropped (this is the legacy
/// `DeviceSession`'s line tap).
pub fn take_lines(id: u32) -> Vec<String> {
    take_reads(id)
        .into_iter()
        .filter_map(|read| match read {
            WireRead::Line(line) => Some(line),
            WireRead::Frame(frame) => Some(frame.to_line()),
            _ => None,
        })
        .collect()
}

pub fn take_errors(id: u32) -> Vec<String> {
    js_array_to_strings(js_take_errors(id))
}

pub async fn release(id: u32) -> Result<(), LinkError> {
    // The session ends with the port: whatever comes next opens a new one.
    PORTS.with(|ports| ports.borrow_mut().remove(&id));
    JsFuture::from(js_release(id))
        .await
        .map(|_| ())
        .map_err(js_error)
}

pub async fn reset_and_read(
    id: u32,
    baud_rate: u32,
    read_window_ms: u32,
    reset_kind: ResetKind,
) -> Result<BrowserSerialResetResult, LinkError> {
    let value = JsFuture::from(js_reset_and_read(
        id,
        baud_rate,
        read_window_ms,
        reset_kind_js_name(reset_kind),
    ))
    .await
    .map_err(js_error)?;
    Ok(BrowserSerialResetResult {
        logs: reflect_string_array(&value, "logs")?,
    })
}

/// Revoke the persistent Web Serial grant behind a port session
/// (`SerialPort.forget()`, Chrome 103+) and drop the JS session entry.
/// `Ok(false)` = the grant SURVIVES: the id is unknown, or the browser has
/// no `forget()` — callers decide whether that deserves a warning.
pub async fn forget(id: u32) -> Result<bool, LinkError> {
    PORTS.with(|ports| ports.borrow_mut().remove(&id));
    PORT_USB_VENDORS.with(|vendors| vendors.borrow_mut().remove(&id));
    let value = JsFuture::from(js_forget_port(id)).await.map_err(js_error)?;
    Ok(value.as_bool().unwrap_or(false))
}

pub async fn close(id: u32) -> Result<(), LinkError> {
    PORTS.with(|ports| ports.borrow_mut().remove(&id));
    JsFuture::from(js_close(id))
        .await
        .map(|_| ())
        .map_err(js_error)
}

fn js_array_to_strings(array: Array) -> Vec<String> {
    array.iter().filter_map(|value| value.as_string()).collect()
}

fn reflect_progress_array(
    value: &JsValue,
    key: &str,
) -> Result<Vec<BrowserSerialProtocolProgress>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    let array = Array::from(&value);
    let mut progress = Vec::with_capacity(array.length() as usize);
    for entry in array.iter() {
        progress.push(BrowserSerialProtocolProgress {
            label: reflect_string(&entry, "label")?,
            completed_steps: reflect_optional_u32(&entry, "completedSteps")?.unwrap_or(0),
            total_steps: reflect_optional_u32(&entry, "totalSteps")?,
            percent: reflect_optional_u32(&entry, "percent")?,
        });
    }
    Ok(progress)
}

fn reflect_string_array(value: &JsValue, key: &str) -> Result<Vec<String>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    Ok(Array::from(&value)
        .iter()
        .filter_map(|value| value.as_string())
        .collect())
}

fn reflect_value(value: &JsValue, key: &str) -> Result<JsValue, LinkError> {
    Reflect::get(value, &JsValue::from_str(key)).map_err(js_error)
}

fn reflect_u32(value: &JsValue, key: &str) -> Result<u32, LinkError> {
    let value = Reflect::get(value, &JsValue::from_str(key)).map_err(js_error)?;
    let Some(value) = value.as_f64() else {
        return Err(LinkError::other(format!(
            "browser serial response missing numeric `{key}`"
        )));
    };
    Ok(value as u32)
}

fn reflect_string(value: &JsValue, key: &str) -> Result<String, LinkError> {
    reflect_optional_string(value, key)?
        .ok_or_else(|| LinkError::other(format!("browser serial response missing string `{key}`")))
}

fn reflect_optional_u32(value: &JsValue, key: &str) -> Result<Option<u32>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let Some(value) = value.as_f64() else {
        return Err(LinkError::other(format!(
            "browser serial response `{key}` is not numeric"
        )));
    };
    Ok(Some(value as u32))
}

fn reflect_optional_string(value: &JsValue, key: &str) -> Result<Option<String>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .as_string()
        .map(Some)
        .ok_or_else(|| LinkError::other(format!("browser serial response `{key}` is not a string")))
}

fn js_request_port_error(value: JsValue) -> LinkError {
    let message = js_error_message(&value);
    if is_request_port_cancel(js_error_name(&value).as_deref(), &message) {
        LinkError::cancelled("Port selection canceled")
    } else {
        LinkError::other(message)
    }
}

fn js_error(value: JsValue) -> LinkError {
    LinkError::other(js_error_message(&value))
}

fn js_error_message(value: &JsValue) -> String {
    if let Some(error) = value.dyn_ref::<js_sys::Error>() {
        error.message().into()
    } else if let Some(message) = value.as_string() {
        message
    } else {
        format!("{value:?}")
    }
}

fn js_error_name(value: &JsValue) -> Option<String> {
    Reflect::get(value, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
}

fn is_request_port_cancel(name: Option<&str>, message: &str) -> bool {
    matches!(name, Some("NotFoundError")) || message.contains("No port selected by the user")
}
