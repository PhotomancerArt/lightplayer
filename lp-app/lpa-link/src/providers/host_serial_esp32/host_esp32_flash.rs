//! Native ESP32 flashing for the `host-serial-esp32` provider, built on
//! espflash-as-a-library (no `cli` feature).
//!
//! This is the host analogue of the browser provider's JS `esptool-js` bridge
//! ([`super::super::browser_serial_esp32::browser_esp32_flash`]): it drives
//! flash / erase / reset over a serial port and emits the same
//! [`LinkManagementEvent`] progress the browser provider does, so
//! `DeviceSession` folds both into identical `DeviceEvent`s.
//!
//! Injection model (see the M5 espflash-lib spike verdict): espflash owns a
//! *concrete* [`serialport::TTYPort`], so we do NOT reuse the session's
//! `DeviceByteStream`. `DeviceSession` releases the OS serial port before
//! calling `manage()`, we open a fresh port here by name, run the operation,
//! drop it, and the session rebuilds its wire transport afterwards.

use std::path::{Path, PathBuf};
use std::time::Duration;

use espflash::connection::reset::{ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::{Flasher, ProgressCallbacks};
use espflash::targets::Chip;
use serde::Deserialize;
use serialport::{SerialPort, SerialPortType, UsbPortInfo};

use crate::{
    LinkEraseDeviceResult, LinkError, LinkFirmwareFlashResult, LinkFirmwareManifest,
    LinkManagementEvent, LinkManagementEventSink, LinkManagementProgress,
};

/// Chip this provider flashes. The firmware package targets the ESP32-C6
/// exclusively (`just studio-firmware-package-esp32c6`); a mismatch surfaces
/// as an espflash `ChipMismatch` at connect time.
const TARGET_CHIP: Chip = Chip::Esp32c6;

/// Baud rate for the espflash connection. 115200 is the ROM/stub default and
/// matches the browser provider; espflash negotiates faster stub baud itself.
const CONNECT_BAUD: u32 = 115_200;

/// Flash firmware from the merged-image manifest at `manifest_path` over the
/// serial port `port_name`. Emits live progress into `events` and returns the
/// accumulated result (same shape as the browser provider's).
pub(super) fn flash_firmware(
    port_name: &str,
    manifest_path: &str,
    events: &LinkManagementEventSink,
) -> Result<LinkFirmwareFlashResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    let (manifest, images) = load_manifest(manifest_path)?;
    recorder.log(format!(
        "Flashing {} ({} image(s), {} bytes) from {manifest_path}",
        manifest.firmware_id, manifest.image_count, manifest.total_bytes
    ));

    let mut flasher = connect(port_name, &mut recorder)?;
    let chip_name = chip_name(&mut flasher);

    for image in &images {
        let data = std::fs::read(&image.absolute_path).map_err(|error| {
            LinkError::other(format!(
                "failed to read firmware image {}: {error}",
                image.absolute_path.display()
            ))
        })?;
        recorder.log(format!(
            "Writing {} bytes at 0x{:x}",
            data.len(),
            image.address
        ));
        let mut bridge =
            ProgressBridge::new(&mut recorder, format!("Flashing 0x{:x}", image.address));
        flasher
            .write_bin_to_flash(image.address, &data, Some(&mut bridge))
            .map_err(|error| LinkError::other(format!("flash write failed: {error}")))?;
    }

    // No explicit reset: `write_bin_to_flash`'s flash-target `finish`
    // already applies the connection's after-operation (HardReset), exactly
    // like the espflash CLI's flash command. A second `reset_after` would
    // talk to a stub that is already gone (found on hardware, M5 smoke).
    recorder.log("Flash complete");

    Ok(LinkFirmwareFlashResult {
        manifest,
        chip_name,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

/// Full-chip erase, leaving the device blank (the `BlankFlash` readiness
/// state). Emits live progress into `events`.
pub(super) fn erase_device_flash(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<LinkEraseDeviceResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    recorder.log("Erasing device flash");

    let mut flasher = connect(port_name, &mut recorder)?;
    let chip_name = chip_name(&mut flasher);

    recorder.progress(LinkManagementProgress::new("Erasing flash"));
    flasher
        .erase_flash()
        .map_err(|error| LinkError::other(format!("flash erase failed: {error}")))?;
    recorder.progress(LinkManagementProgress::new("Erasing flash").with_percent(100));

    reset_into_app(&mut flasher, &mut recorder);
    recorder.log("Erase complete");

    Ok(LinkEraseDeviceResult {
        chip_name,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

/// Reboot the device into its application firmware via a hard-reset signal
/// pulse — no bootloader entry. Returns the emitted log lines.
pub(super) fn reset_runtime(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<Vec<String>, LinkError> {
    let mut recorder = EventRecorder::new(events);
    recorder.log(format!("Resetting device on {port_name}"));
    let mut port = serialport::new(port_name, CONNECT_BAUD)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|error| LinkError::other(format!("failed to open {port_name}: {error}")))?;
    hard_reset_pulse(port.as_mut())
        .map_err(|error| LinkError::other(format!("reset failed: {error}")))?;
    recorder.log("Reset complete");
    Ok(recorder.logs.clone())
}

/// Open the port and establish an espflash connection (reset into bootloader,
/// sync, chip-detect, upload stub). `before = DefaultReset` performs the
/// USB-JTAG download-mode entry; `after = HardReset` is applied by
/// [`reset_into_app`] once the operation finishes.
fn connect(port_name: &str, recorder: &mut EventRecorder) -> Result<Flasher, LinkError> {
    recorder.log(format!("Connecting to {port_name}"));
    let serial = serialport::new(port_name, CONNECT_BAUD)
        .flow_control(serialport::FlowControl::None)
        .open_native()
        .map_err(|error| LinkError::other(format!("failed to open {port_name}: {error}")))?;
    Flasher::connect(
        serial,
        port_info_for(port_name),
        Some(CONNECT_BAUD),
        /* use_stub  */ true,
        /* verify    */ false,
        /* skip      */ false,
        Some(TARGET_CHIP),
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
    )
    .map_err(|error| LinkError::other(format!("espflash connect failed: {error}")))
}

/// Apply the connection's `after` operation (HardReset) so the chip leaves
/// download mode and boots the (now blank) flash. Needed on the ERASE path
/// only: erase commands have no flash-target `finish`, so nothing else
/// applies the after-operation (the espflash CLI's erase commands do the
/// same). Best-effort: a reset failure is logged but not fatal —
/// `DeviceSession` re-runs readiness on rebuild regardless.
fn reset_into_app(flasher: &mut Flasher, recorder: &mut EventRecorder) {
    // `is_stub = true` matches the `use_stub = true` passed to `connect`.
    if let Err(error) = flasher.connection().reset_after(true) {
        recorder.log(format!("warning: post-operation reset failed: {error}"));
    }
}

fn chip_name(flasher: &mut Flasher) -> Option<String> {
    // Chip identity was already detected during `Flasher::connect`; no extra
    // ROM round-trip needed.
    Some(flasher.chip().to_string())
}

/// Resolve `UsbPortInfo` for `port_name` from the OS port list. espflash's
/// reset strategy branches on the USB PID (USB-Serial-JTAG vs classic), so a
/// correct pid matters; fall back to zeros if the port isn't enumerable.
fn port_info_for(port_name: &str) -> UsbPortInfo {
    serialport::available_ports()
        .ok()
        .into_iter()
        .flatten()
        .find(|port| port.port_name == port_name)
        .and_then(|port| match port.port_type {
            SerialPortType::UsbPort(info) => Some(info),
            _ => None,
        })
        .unwrap_or(UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        })
}

/// The USB-JTAG-serial hard-reset pulse (RTS = EN line), identical to the
/// post-flash reset the hardware serial transport performs on open. Boots the
/// application firmware without entering download mode.
fn hard_reset_pulse(port: &mut dyn SerialPort) -> serialport::Result<()> {
    port.write_data_terminal_ready(false)?;
    std::thread::sleep(Duration::from_millis(100));
    port.write_request_to_send(true)?;
    port.write_data_terminal_ready(false)?;
    port.write_request_to_send(true)?;
    std::thread::sleep(Duration::from_millis(100));
    port.write_request_to_send(false)?;
    Ok(())
}

/// Records management events into `logs`/`progress` for the returned result
/// while forwarding each one live to the sink.
struct EventRecorder<'a> {
    sink: &'a LinkManagementEventSink,
    logs: Vec<String>,
    progress: Vec<LinkManagementProgress>,
}

impl<'a> EventRecorder<'a> {
    fn new(sink: &'a LinkManagementEventSink) -> Self {
        Self {
            sink,
            logs: Vec::new(),
            progress: Vec::new(),
        }
    }

    fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.sink.emit(LinkManagementEvent::log(message.clone()));
        self.logs.push(message);
    }

    fn progress(&mut self, progress: LinkManagementProgress) {
        self.sink
            .emit(LinkManagementEvent::progress(progress.clone()));
        self.progress.push(progress);
    }
}

/// Bridges espflash's byte-count [`ProgressCallbacks`] onto our step/percent
/// [`LinkManagementProgress`] events. One bridge per flashed image.
struct ProgressBridge<'a, 'b> {
    recorder: &'a mut EventRecorder<'b>,
    label: String,
    total: u32,
}

impl<'a, 'b> ProgressBridge<'a, 'b> {
    fn new(recorder: &'a mut EventRecorder<'b>, label: String) -> Self {
        Self {
            recorder,
            label,
            total: 0,
        }
    }
}

impl ProgressCallbacks for ProgressBridge<'_, '_> {
    fn init(&mut self, _addr: u32, total: usize) {
        self.total = total as u32;
        self.recorder.progress(
            LinkManagementProgress::new(self.label.clone())
                .with_steps(0, self.total)
                .with_percent(0),
        );
    }

    fn update(&mut self, current: usize) {
        let current = current as u32;
        let percent = if self.total > 0 {
            ((current as u64 * 100) / self.total as u64) as u32
        } else {
            0
        };
        self.recorder.progress(
            LinkManagementProgress::new(self.label.clone())
                .with_steps(current, self.total)
                .with_percent(percent),
        );
    }

    fn finish(&mut self) {
        self.recorder.progress(
            LinkManagementProgress::new(self.label.clone())
                .with_steps(self.total, self.total)
                .with_percent(100),
        );
    }
}

/// A firmware image resolved against the manifest directory.
#[derive(Debug)]
struct ResolvedImage {
    absolute_path: PathBuf,
    address: u32,
}

/// Load and validate the firmware manifest, returning a provider-neutral
/// [`LinkFirmwareManifest`] plus the images resolved to absolute paths.
fn load_manifest(
    manifest_path: &str,
) -> Result<(LinkFirmwareManifest, Vec<ResolvedImage>), LinkError> {
    let manifest_path = Path::new(manifest_path);
    let bytes = std::fs::read(manifest_path).map_err(|error| {
        LinkError::other(format!(
            "failed to read firmware manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let raw: RawManifest = serde_json::from_slice(&bytes).map_err(|error| {
        LinkError::other(format!(
            "failed to parse firmware manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    if raw.images.is_empty() {
        return Err(LinkError::other(format!(
            "firmware manifest {} lists no images",
            manifest_path.display()
        )));
    }

    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut images = Vec::with_capacity(raw.images.len());
    let mut total_bytes: u32 = 0;
    for image in &raw.images {
        let address = parse_hex_u32(&image.address).ok_or_else(|| {
            LinkError::other(format!(
                "firmware manifest image address `{}` is not a hex offset",
                image.address
            ))
        })?;
        total_bytes = total_bytes.saturating_add(image.size_bytes);
        images.push(ResolvedImage {
            absolute_path: manifest_dir.join(&image.path),
            address,
        });
    }

    let manifest = LinkFirmwareManifest {
        firmware_id: raw.firmware_id,
        display_name: raw.display_name,
        target_chip: raw.target.chip,
        image_count: raw.images.len() as u32,
        total_bytes,
        manifest_path: Some(manifest_path.display().to_string()),
    };
    Ok((manifest, images))
}

fn parse_hex_u32(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(digits, 16).ok()
}

/// The subset of `manifest.json` (produced by `just
/// studio-firmware-package-esp32c6`) this provider consumes.
#[derive(Deserialize)]
struct RawManifest {
    #[serde(rename = "firmwareId")]
    firmware_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    target: RawTarget,
    images: Vec<RawImage>,
}

#[derive(Deserialize)]
struct RawTarget {
    chip: String,
}

#[derive(Deserialize)]
struct RawImage {
    path: String,
    address: String,
    #[serde(rename = "sizeBytes")]
    size_bytes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_JSON: &str = r#"{
        "schemaVersion": 1,
        "firmwareId": "lightplayer-esp32c6-server",
        "displayName": "LightPlayer ESP32-C6 server firmware",
        "target": { "family": "esp32", "chip": "esp32c6" },
        "build": { "package": "fw-esp32" },
        "flash": { "format": "espflash-merged-image", "address": "0x0" },
        "images": [
            {
                "path": "fw-esp32c6-server-merged.bin",
                "address": "0x0",
                "sizeBytes": 3022960,
                "sha256": "abc"
            }
        ]
    }"#;

    #[test]
    fn parses_studio_firmware_manifest() {
        let dir = std::env::temp_dir().join("lpa-link-manifest-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        std::fs::write(&path, MANIFEST_JSON).unwrap();

        let (manifest, images) = load_manifest(path.to_str().unwrap()).unwrap();
        assert_eq!(manifest.firmware_id, "lightplayer-esp32c6-server");
        assert_eq!(manifest.target_chip, "esp32c6");
        assert_eq!(manifest.image_count, 1);
        assert_eq!(manifest.total_bytes, 3_022_960);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].address, 0x0);
        assert_eq!(
            images[0].absolute_path,
            dir.join("fw-esp32c6-server-merged.bin")
        );
    }

    #[test]
    fn rejects_manifest_without_images() {
        let dir = std::env::temp_dir().join("lpa-link-manifest-empty-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        std::fs::write(
            &path,
            r#"{
                "firmwareId": "x",
                "displayName": "x",
                "target": { "chip": "esp32c6" },
                "images": []
            }"#,
        )
        .unwrap();

        let error = load_manifest(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("no images"), "{error}");
    }

    #[test]
    fn parses_hex_addresses() {
        assert_eq!(parse_hex_u32("0x0"), Some(0));
        assert_eq!(parse_hex_u32("0x310000"), Some(0x310000));
        assert_eq!(parse_hex_u32("0X10"), Some(0x10));
        assert_eq!(parse_hex_u32("10000"), Some(0x10000));
        assert_eq!(parse_hex_u32("zz"), None);
    }

    #[test]
    fn progress_bridge_reports_percent_steps() {
        let sink = LinkManagementEventSink::noop();
        let mut recorder = EventRecorder::new(&sink);
        let mut bridge = ProgressBridge::new(&mut recorder, "Flashing 0x0".to_string());
        bridge.init(0x0, 200);
        bridge.update(50);
        bridge.finish();
        assert_eq!(recorder.progress.len(), 3);
        assert_eq!(recorder.progress[0].percent, Some(0));
        assert_eq!(recorder.progress[1].percent, Some(25));
        assert_eq!(recorder.progress[1].completed_steps, 50);
        assert_eq!(recorder.progress[2].percent, Some(100));
        assert_eq!(recorder.progress[2].completed_steps, 200);
    }
}
