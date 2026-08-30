use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

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
    fn js_open(id: u32, baud_rate: u32, reset: bool) -> Promise;

    #[wasm_bindgen(js_name = writeLine)]
    fn js_write_line(id: u32, line: &str) -> Promise;

    #[wasm_bindgen(js_name = takeLines)]
    fn js_take_lines(id: u32) -> Array;

    #[wasm_bindgen(js_name = takeErrors)]
    fn js_take_errors(id: u32) -> Array;

    #[wasm_bindgen(js_name = releasePort)]
    fn js_release(id: u32) -> Promise;

    #[wasm_bindgen(js_name = resetAndRead)]
    fn js_reset_and_read(id: u32, baud_rate: u32, read_window_ms: u32) -> Promise;

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

fn port_handle(value: &JsValue) -> Result<BrowserSerialPortHandle, LinkError> {
    Ok(BrowserSerialPortHandle {
        id: reflect_u32(value, "id")?,
        label: reflect_string(value, "label")?,
        usb_vendor_id: reflect_optional_u32(value, "usbVendorId")?.map(|id| id as u16),
        usb_product_id: reflect_optional_u32(value, "usbProductId")?.map(|id| id as u16),
    })
}

pub async fn open(
    id: u32,
    baud_rate: u32,
    reset: bool,
) -> Result<BrowserSerialProtocolOpenResult, LinkError> {
    let value = JsFuture::from(js_open(id, baud_rate, reset))
        .await
        .map_err(js_error)?;
    Ok(BrowserSerialProtocolOpenResult {
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

pub async fn write_line(id: u32, line: &str) -> Result<(), LinkError> {
    JsFuture::from(js_write_line(id, line))
        .await
        .map(|_| ())
        .map_err(js_error)
}

pub fn take_lines(id: u32) -> Vec<String> {
    js_array_to_strings(js_take_lines(id))
}

pub fn take_errors(id: u32) -> Vec<String> {
    js_array_to_strings(js_take_errors(id))
}

pub async fn release(id: u32) -> Result<(), LinkError> {
    JsFuture::from(js_release(id))
        .await
        .map(|_| ())
        .map_err(js_error)
}

pub async fn reset_and_read(
    id: u32,
    baud_rate: u32,
    read_window_ms: u32,
) -> Result<BrowserSerialResetResult, LinkError> {
    let value = JsFuture::from(js_reset_and_read(id, baud_rate, read_window_ms))
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
    let value = JsFuture::from(js_forget_port(id)).await.map_err(js_error)?;
    Ok(value.as_bool().unwrap_or(false))
}

pub async fn close(id: u32) -> Result<(), LinkError> {
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
