use js_sys::{Array, Promise, Reflect};
use lp_studio_core::ProgressState;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::StudioRuntimeError;

pub const DEFAULT_ESP32C6_FIRMWARE_MANIFEST_URL: &str = "./firmware/esp32c6/manifest.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32FirmwareManifest {
    pub firmware_id: String,
    pub display_name: String,
    pub target_chip: String,
    pub image_count: u32,
    pub total_bytes: u32,
    pub manifest_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserEsp32FlashResult {
    pub manifest: BrowserEsp32FirmwareManifest,
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<ProgressState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32ProbeResult {
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = globalThis, js_name = lpBrowserEsp32FlashIsSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_namespace = globalThis, js_name = lpBrowserEsp32FlashLoadManifest)]
    fn js_load_manifest(manifest_url: &str) -> Promise;

    #[wasm_bindgen(js_namespace = globalThis, js_name = lpBrowserEsp32FlashProbeTarget)]
    fn js_probe_target(port_id: u32) -> Promise;

    #[wasm_bindgen(js_namespace = globalThis, js_name = lpBrowserEsp32FlashFirmware)]
    fn js_flash_firmware(port_id: u32, manifest_url: &str) -> Promise;
}

pub fn is_supported() -> bool {
    js_is_supported()
}

pub async fn load_manifest(
    manifest_url: &str,
) -> Result<BrowserEsp32FirmwareManifest, StudioRuntimeError> {
    let value = JsFuture::from(js_load_manifest(manifest_url))
        .await
        .map_err(js_error)?;
    parse_manifest(&value)
}

pub async fn flash_firmware(
    port_id: u32,
    manifest_url: &str,
) -> Result<BrowserEsp32FlashResult, StudioRuntimeError> {
    let value = JsFuture::from(js_flash_firmware(port_id, manifest_url))
        .await
        .map_err(js_error)?;
    let manifest_value = reflect_value(&value, "manifest")?;
    Ok(BrowserEsp32FlashResult {
        manifest: parse_manifest(&manifest_value)?,
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

pub async fn probe_target(port_id: u32) -> Result<BrowserEsp32ProbeResult, StudioRuntimeError> {
    let value = JsFuture::from(js_probe_target(port_id))
        .await
        .map_err(js_error)?;
    Ok(BrowserEsp32ProbeResult {
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
    })
}

fn parse_manifest(value: &JsValue) -> Result<BrowserEsp32FirmwareManifest, StudioRuntimeError> {
    Ok(BrowserEsp32FirmwareManifest {
        firmware_id: reflect_string(value, "firmwareId")?,
        display_name: reflect_string(value, "displayName")?,
        target_chip: reflect_string(value, "targetChip")?,
        image_count: reflect_u32(value, "imageCount")?,
        total_bytes: reflect_u32(value, "totalBytes")?,
        manifest_url: reflect_optional_string(value, "manifestUrl")?,
    })
}

fn reflect_progress_array(
    value: &JsValue,
    key: &str,
) -> Result<Vec<ProgressState>, StudioRuntimeError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    let array = Array::from(&value);
    let mut progress = Vec::with_capacity(array.length() as usize);
    for entry in array.iter() {
        let mut state = ProgressState::new(reflect_string(&entry, "label")?);
        let completed_steps = reflect_optional_u32(&entry, "completedSteps")?.unwrap_or(0);
        if let Some(total_steps) = reflect_optional_u32(&entry, "totalSteps")? {
            state = state.with_steps(completed_steps, total_steps);
        } else {
            state.completed_steps = completed_steps;
        }
        if let Some(percent) = reflect_optional_u32(&entry, "percent")? {
            state = state.with_percent(percent as u8);
        }
        progress.push(state);
    }
    Ok(progress)
}

fn reflect_string_array(value: &JsValue, key: &str) -> Result<Vec<String>, StudioRuntimeError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    Ok(Array::from(&value)
        .iter()
        .filter_map(|value| value.as_string())
        .collect())
}

fn reflect_value(value: &JsValue, key: &str) -> Result<JsValue, StudioRuntimeError> {
    Reflect::get(value, &JsValue::from_str(key)).map_err(js_error)
}

fn reflect_u32(value: &JsValue, key: &str) -> Result<u32, StudioRuntimeError> {
    reflect_optional_u32(value, key)?.ok_or_else(|| {
        StudioRuntimeError::Browser(format!(
            "browser ESP32 flash response missing numeric `{key}`"
        ))
    })
}

fn reflect_optional_u32(value: &JsValue, key: &str) -> Result<Option<u32>, StudioRuntimeError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let Some(value) = value.as_f64() else {
        return Err(StudioRuntimeError::Browser(format!(
            "browser ESP32 flash response `{key}` is not numeric"
        )));
    };
    Ok(Some(value as u32))
}

fn reflect_string(value: &JsValue, key: &str) -> Result<String, StudioRuntimeError> {
    reflect_optional_string(value, key)?.ok_or_else(|| {
        StudioRuntimeError::Browser(format!(
            "browser ESP32 flash response missing string `{key}`"
        ))
    })
}

fn reflect_optional_string(
    value: &JsValue,
    key: &str,
) -> Result<Option<String>, StudioRuntimeError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value.as_string().map(Some).ok_or_else(|| {
        StudioRuntimeError::Browser(format!(
            "browser ESP32 flash response `{key}` is not a string"
        ))
    })
}

fn js_error(value: JsValue) -> StudioRuntimeError {
    if let Some(error) = value.dyn_ref::<js_sys::Error>() {
        StudioRuntimeError::Browser(error.message().into())
    } else if let Some(message) = value.as_string() {
        StudioRuntimeError::Browser(message)
    } else {
        StudioRuntimeError::Browser(format!("{value:?}"))
    }
}
