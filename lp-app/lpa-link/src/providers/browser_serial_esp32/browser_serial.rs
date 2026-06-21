use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialPortHandle {
    pub id: u32,
    pub label: String,
}

#[wasm_bindgen(module = "/src/providers/browser_serial_esp32/browser_serial.js")]
extern "C" {
    #[wasm_bindgen(js_name = isSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_name = requestPort)]
    fn js_request_port() -> Promise;

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

    #[wasm_bindgen(js_name = closePort)]
    fn js_close(id: u32) -> Promise;
}

pub fn is_supported() -> bool {
    js_is_supported()
}

pub async fn request_port() -> Result<BrowserSerialPortHandle, LinkError> {
    let value = JsFuture::from(js_request_port()).await.map_err(js_error)?;
    let id = reflect_u32(&value, "id")?;
    let label = reflect_string(&value, "label")?;
    Ok(BrowserSerialPortHandle { id, label })
}

pub async fn open(id: u32, baud_rate: u32) -> Result<(), LinkError> {
    JsFuture::from(js_open(id, baud_rate))
        .await
        .map(|_| ())
        .map_err(js_error)
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

pub async fn close(id: u32) -> Result<(), LinkError> {
    JsFuture::from(js_close(id))
        .await
        .map(|_| ())
        .map_err(js_error)
}

fn js_array_to_strings(array: Array) -> Vec<String> {
    array.iter().filter_map(|value| value.as_string()).collect()
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
    let value = Reflect::get(value, &JsValue::from_str(key)).map_err(js_error)?;
    value
        .as_string()
        .ok_or_else(|| LinkError::other(format!("browser serial response missing string `{key}`")))
}

fn js_error(value: JsValue) -> LinkError {
    if let Some(error) = value.dyn_ref::<js_sys::Error>() {
        LinkError::other(error.message())
    } else if let Some(message) = value.as_string() {
        LinkError::other(message)
    } else {
        LinkError::other(format!("{value:?}"))
    }
}
