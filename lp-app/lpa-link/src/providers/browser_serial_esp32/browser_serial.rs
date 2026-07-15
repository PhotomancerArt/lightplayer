use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialPortHandle {
    pub id: u32,
    pub label: String,
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

    #[wasm_bindgen(js_name = requestPort)]
    fn js_request_port() -> Promise;

    #[wasm_bindgen(js_name = grantedPortsCount)]
    fn js_granted_ports_count() -> Promise;

    #[wasm_bindgen(js_name = openPort)]
    fn js_open(id: u32, baud_rate: u32) -> Promise;

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
}

pub fn is_supported() -> bool {
    js_is_supported()
}

/// Number of serial ports the user has ALREADY granted this origin
/// (`navigator.serial.getPorts()` length) — no permission prompt is shown.
/// `0` when Web Serial is unsupported or the probe fails.
pub async fn granted_ports_count() -> usize {
    match JsFuture::from(js_granted_ports_count()).await {
        Ok(value) => value.as_f64().unwrap_or(0.0) as usize,
        Err(_) => 0,
    }
}

pub async fn request_port() -> Result<BrowserSerialPortHandle, LinkError> {
    let value = JsFuture::from(js_request_port())
        .await
        .map_err(js_request_port_error)?;
    let id = reflect_u32(&value, "id")?;
    let label = reflect_string(&value, "label")?;
    Ok(BrowserSerialPortHandle { id, label })
}

pub async fn open(id: u32, baud_rate: u32) -> Result<BrowserSerialProtocolOpenResult, LinkError> {
    let value = JsFuture::from(js_open(id, baud_rate))
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
