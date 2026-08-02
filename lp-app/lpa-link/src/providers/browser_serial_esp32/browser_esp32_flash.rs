use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
use wasm_bindgen_futures::JsFuture;

use crate::{
    LinkError, LinkFlashRegion, LinkManagementEvent, LinkManagementEventSink,
    LinkManagementProgress,
};

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

/// A raw filesystem image read back over Web Serial, plus the region and
/// chip the read resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEsp32FilesystemReadResult {
    pub image: Vec<u8>,
    pub region: LinkFlashRegion,
    pub chip_name: Option<String>,
    pub logs: Vec<String>,
    pub progress: Vec<BrowserEsp32FlashProgress>,
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
    fn js_flash_firmware(
        port_id: u32,
        manifest_path: &str,
        esptool_module_path: &str,
        on_event: &Function,
    ) -> Promise;

    #[wasm_bindgen(js_name = eraseDeviceFlash)]
    fn js_erase_device_flash(
        port_id: u32,
        esptool_module_path: &str,
        on_event: &Function,
    ) -> Promise;

    #[wasm_bindgen(js_name = readRawFilesystem)]
    fn js_read_raw_filesystem(
        port_id: u32,
        esptool_module_path: &str,
        resolve_region: &Function,
        on_event: &Function,
    ) -> Promise;

    #[wasm_bindgen(js_name = writeBootControl)]
    fn js_write_boot_control(
        port_id: u32,
        esptool_module_path: &str,
        address: u32,
        record: &[u8],
        on_event: &Function,
    ) -> Promise;
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

pub async fn flash_firmware_with_events(
    port_id: u32,
    manifest_path: &str,
    esptool_module_path: &str,
    events: LinkManagementEventSink,
) -> Result<BrowserEsp32FlashResult, LinkError> {
    let on_event = management_event_callback(events);
    let value = JsFuture::from(js_flash_firmware(
        port_id,
        manifest_path,
        esptool_module_path,
        on_event.as_ref().unchecked_ref(),
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

pub async fn erase_device_flash_with_events(
    port_id: u32,
    esptool_module_path: &str,
    events: LinkManagementEventSink,
) -> Result<BrowserEsp32EraseResult, LinkError> {
    let on_event = management_event_callback(events);
    let value = JsFuture::from(js_erase_device_flash(
        port_id,
        esptool_module_path,
        on_event.as_ref().unchecked_ref(),
    ))
    .await
    .map_err(js_error)?;
    Ok(BrowserEsp32EraseResult {
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

/// Write the boot-control record, instructing the device's next boot.
///
/// The record is encoded here, in Rust, and handed to JS as bytes — the
/// firmware that reads it cannot renegotiate the format at runtime, so
/// `lp-bootctl` stays the single implementation of it.
pub async fn write_boot_control_with_events(
    port_id: u32,
    esptool_module_path: &str,
    flags: lp_bootctl::BootFlags,
    events: LinkManagementEventSink,
) -> Result<BrowserEsp32EraseResult, LinkError> {
    let on_event = management_event_callback(events);
    let record = lp_bootctl::encode_record(flags);
    let value = JsFuture::from(js_write_boot_control(
        port_id,
        esptool_module_path,
        lp_bootctl::BOOTCTL_PARTITION_OFFSET,
        &record,
        on_event.as_ref().unchecked_ref(),
    ))
    .await
    .map_err(js_error)?;
    Ok(BrowserEsp32EraseResult {
        chip_name: reflect_optional_string(&value, "chipName")?,
        logs: reflect_string_array(&value, "logs")?,
        progress: reflect_progress_array(&value, "progress")?,
    })
}

/// Read the device's `lpfs` partition back into wasm memory.
///
/// The per-board region table stays HERE: the JS side asks for it by chip
/// name once its SYNC handshake has one (see the `resolveRegion` callback in
/// `browser_esp32_flash.js`). Mirroring the offsets into JS would put the
/// same two numbers in two languages, and the wrong one produces a
/// plausible-looking archive of the wrong partition.
pub async fn read_raw_filesystem_with_events(
    port_id: u32,
    esptool_module_path: &str,
    events: LinkManagementEventSink,
) -> Result<BrowserEsp32FilesystemReadResult, LinkError> {
    let on_event = management_event_callback(events);
    let resolve_region = Closure::wrap(Box::new(|chip: JsValue| -> JsValue {
        let Some(region) = chip
            .as_string()
            .as_deref()
            .and_then(LinkFlashRegion::lpfs_for_chip)
        else {
            return JsValue::NULL;
        };
        let out = js_sys::Object::new();
        let _ = Reflect::set(
            &out,
            &"offset".into(),
            &JsValue::from_f64(region.offset.into()),
        );
        let _ = Reflect::set(
            &out,
            &"length".into(),
            &JsValue::from_f64(region.length.into()),
        );
        out.into()
    }) as Box<dyn FnMut(JsValue) -> JsValue>);

    let value = JsFuture::from(js_read_raw_filesystem(
        port_id,
        esptool_module_path,
        resolve_region.as_ref().unchecked_ref(),
        on_event.as_ref().unchecked_ref(),
    ))
    .await
    .map_err(js_error)?;
    Ok(BrowserEsp32FilesystemReadResult {
        image: js_sys::Uint8Array::new(&reflect_value(&value, "image")?).to_vec(),
        region: LinkFlashRegion {
            offset: reflect_u32(&value, "offset")?,
            length: reflect_u32(&value, "length")?,
        },
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

fn management_event_callback(events: LinkManagementEventSink) -> Closure<dyn FnMut(JsValue)> {
    Closure::wrap(Box::new(move |value: JsValue| match parse_event(&value) {
        Ok(event) => events.emit(event),
        Err(error) => events.emit(LinkManagementEvent::log(format!(
            "failed to parse browser ESP32 progress event: {error}"
        ))),
    }) as Box<dyn FnMut(JsValue)>)
}

fn parse_event(value: &JsValue) -> Result<LinkManagementEvent, LinkError> {
    match reflect_string(value, "kind")?.as_str() {
        "log" => Ok(LinkManagementEvent::log(reflect_string(value, "message")?)),
        "progress" => Ok(LinkManagementEvent::progress(LinkManagementProgress {
            label: reflect_string(value, "label")?,
            completed_steps: reflect_optional_u32(value, "completedSteps")?.unwrap_or(0),
            total_steps: reflect_optional_u32(value, "totalSteps")?,
            percent: reflect_optional_u32(value, "percent")?,
        })),
        kind => Err(LinkError::other(format!(
            "unknown browser ESP32 progress event kind `{kind}`"
        ))),
    }
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
