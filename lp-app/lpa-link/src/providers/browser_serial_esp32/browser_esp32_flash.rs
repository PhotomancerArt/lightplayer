use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32FirmwareManifest {
    pub firmware_id: String,
    pub display_name: String,
    pub target_chip: String,
    pub image_count: u32,
    pub total_bytes: u32,
    pub manifest_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserEsp32FlashResult {
    pub manifest: BrowserEsp32FirmwareManifest,
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<BrowserEsp32FlashProgress>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserEsp32EraseResult {
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<BrowserEsp32FlashProgress>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32FlashProgress {
    pub label: String,
    pub completed_steps: u32,
    pub total_steps: Option<u32>,
    pub percent: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32ProbeResult {
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
}

#[wasm_bindgen(module = "/src/providers/browser_serial_esp32/browser_esp32_flash.js")]
extern "C" {
    #[wasm_bindgen(js_name = isSupported)]
    fn js_is_supported() -> bool;

    #[wasm_bindgen(js_name = loadManifest)]
    fn js_load_manifest(manifest_path: &str) -> Promise;

    #[wasm_bindgen(js_name = probeTarget)]
    fn js_probe_target(port_id: u32, esptool_module_path: &str) -> Promise;

    #[wasm_bindgen(js_name = flashFirmware)]
    fn js_flash_firmware(port_id: u32, manifest_path: &str, esptool_module_path: &str) -> Promise;

    #[wasm_bindgen(js_name = eraseDeviceFlash)]
    fn js_erase_device_flash(port_id: u32, esptool_module_path: &str) -> Promise;
}

pub fn is_supported() -> bool {
    js_is_supported()
}

pub async fn load_manifest(manifest_path: &str) -> Result<BrowserEsp32FirmwareManifest, LinkError> {
    let value = JsFuture::from(js_load_manifest(manifest_path))
        .await
        .map_err(js_error)?;
    parse_manifest(&value)
}

pub async fn flash_firmware(
    port_id: u32,
    manifest_path: &str,
    esptool_module_path: &str,
) -> Result<BrowserEsp32FlashResult, LinkError> {
    let value = JsFuture::from(js_flash_firmware(
        port_id,
        manifest_path,
        esptool_module_path,
    ))
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

pub async fn erase_device_flash(
    port_id: u32,
    esptool_module_path: &str,
) -> Result<BrowserEsp32EraseResult, LinkError> {
    let value = JsFuture::from(js_erase_device_flash(port_id, esptool_module_path))
        .await
        .map_err(js_error)?;
    Ok(BrowserEsp32EraseResult {
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

pub async fn probe_target(
    port_id: u32,
    esptool_module_path: &str,
) -> Result<BrowserEsp32ProbeResult, LinkError> {
    let value = JsFuture::from(js_probe_target(port_id, esptool_module_path))
        .await
        .map_err(js_error)?;
    Ok(BrowserEsp32ProbeResult {
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
    })
}

fn parse_manifest(value: &JsValue) -> Result<BrowserEsp32FirmwareManifest, LinkError> {
    Ok(BrowserEsp32FirmwareManifest {
        firmware_id: reflect_string(value, "firmwareId")?,
        display_name: reflect_string(value, "displayName")?,
        target_chip: reflect_string(value, "targetChip")?,
        image_count: reflect_u32(value, "imageCount")?,
        total_bytes: reflect_u32(value, "totalBytes")?,
        manifest_path: reflect_optional_string(value, "manifestPath")?,
    })
}

fn reflect_progress_array(
    value: &JsValue,
    key: &str,
) -> Result<Vec<BrowserEsp32FlashProgress>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    let array = Array::from(&value);
    let mut progress = Vec::with_capacity(array.length() as usize);
    for entry in array.iter() {
        progress.push(BrowserEsp32FlashProgress {
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
    reflect_optional_u32(value, key)?
        .ok_or_else(|| LinkError::other(format!("browser ESP32 response missing numeric `{key}`")))
}

fn reflect_optional_u32(value: &JsValue, key: &str) -> Result<Option<u32>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let Some(value) = value.as_f64() else {
        return Err(LinkError::other(format!(
            "browser ESP32 response `{key}` is not numeric"
        )));
    };
    Ok(Some(value as u32))
}

fn reflect_string(value: &JsValue, key: &str) -> Result<String, LinkError> {
    reflect_optional_string(value, key)?
        .ok_or_else(|| LinkError::other(format!("browser ESP32 response missing string `{key}`")))
}

fn reflect_optional_string(value: &JsValue, key: &str) -> Result<Option<String>, LinkError> {
    let value = reflect_value(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .as_string()
        .map(Some)
        .ok_or_else(|| LinkError::other(format!("browser ESP32 response `{key}` is not a string")))
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
