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

use espflash::command::{Command, CommandType};
use espflash::connection::reset::{ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::{Flasher, ProgressCallbacks};
use espflash::targets::Chip;
use md5::Digest;
use serde::Deserialize;
use serialport::{SerialPort, SerialPortType, UsbPortInfo};

use lp_bootctl::{BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE, BootFlags, encode_record};

use super::lp_analog_i2c::{
    LP_ANA_I2C_BIT, LP_I2C_ANA_MST_BUSY, LP_I2C_ANA_MST_I2C0_CTRL, LPPERI_CLK_EN, LPPERI_RESET_EN,
    describe, lp_analog_i2c_plan,
};

use crate::{
    LinkBootControlResult, LinkEraseDeviceResult, LinkError, LinkFirmwareFlashResult,
    LinkFirmwareManifest, LinkFlashRegion, LinkManagementEvent, LinkManagementEventSink,
    LinkManagementProgress, LinkRawFilesystemReadResult,
};

/// The espflash chip a manifest's `core.platform.chip` names, or `None` for a
/// chip this build has no `Chip` variant for.
///
/// The image decides which chip we are willing to talk to — not a constant.
/// This used to be `const TARGET_CHIP: Chip = Chip::Esp32c6`, which was true
/// only while exactly one image existed; the day a second one shipped it
/// would have declared "C6" while writing an S3 image.
pub(super) fn manifest_chip(target_chip: &str) -> Option<Chip> {
    match crate::chip_id_from_reported(target_chip)? {
        "esp32c6" => Some(Chip::Esp32c6),
        "esp32s3" => Some(Chip::Esp32s3),
        "esp32" => Some(Chip::Esp32),
        _ => None,
    }
}

/// Baud rate for the espflash connection. 115200 is the ROM/stub default and
/// matches the browser provider; espflash negotiates faster stub baud itself.
const CONNECT_BAUD: u32 = 115_200;

/// `ESP_READ_FLASH` packet size and in-flight window. The stub is already
/// running by the time we read (`Flasher::connect` uploads it).
///
/// ONE packet in flight, not the M1 spike's 1024: espflash 3.3's
/// `Connection::write_raw` — the per-packet ack — clears the port's INPUT
/// buffer before it writes, so any part of the next packet that has already
/// arrived is thrown away and the read fails "truncated". With 1024 in
/// flight the stub streams ahead of the acks, and whether bytes are waiting
/// at the clear is a race the host's own speed decides: the emulated C6
/// lost 7–45 bytes per run in the second packet (C6 repartition P08), and a
/// busy laptop can lose them against silicon too. One in flight means the
/// stub sends nothing until it has the ack, so the clear finds nothing to
/// lose — at the cost of one round trip per 4 KB block (~240 for a C6
/// filesystem).
const READ_BLOCK_SIZE: u32 = 4096;
const READ_MAX_IN_FLIGHT: u32 = 1;

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

    // Declaring the manifest's chip makes espflash refuse the handshake on a
    // mismatch; the explicit check below turns that into an error naming
    // both chips, and covers the unknown-chip manifest espflash cannot be
    // told about at all.
    let expected_chip = manifest_chip(&manifest.target_chip).ok_or_else(|| {
        LinkError::other(format!(
            "firmware manifest {} targets chip `{}`, which this build cannot flash",
            manifest_path, manifest.target_chip
        ))
    })?;
    // `NoResetNoStub`: the flash-target `finish` must NOT reset the board —
    // the LP-clock restore below has to run between the write and the reset,
    // over the stub that is still up. The reset is ours, after it. (The
    // ROM-only pass in `connect` normally leaves nothing for this one to do;
    // it stays as the belt to that brace.)
    let mut flasher = connect(
        port_name,
        Some(expected_chip),
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    let chip_name = chip_name(&mut flasher);
    assert_chip_matches_manifest(flasher.chip(), &manifest)?;

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
        let md5 = write_verified(&mut flasher, image.address, &data, Some(&mut bridge))
            .map_err(LinkError::other)?;
        recorder.log(format!(
            "Verified {} bytes at 0x{:x} (MD5 {md5})",
            data.len(),
            image.address
        ));
    }

    // `write_bin_to_flash`'s flash-target `finish` applied the connection's
    // after-operation — `NoResetNoStub`, so the stub is still up and the
    // board has not been reset. (With `HardReset`, as this path once used, a
    // second `reset_after` would talk to a stub that is already gone — found
    // on hardware, M5 smoke.) Restore what the previous firmware may have
    // left gated, THEN reset: the bootloader we just wrote cannot boot
    // otherwise.
    recorder.log("Flash complete");
    restore_lp_analog_i2c_clock(&mut flasher, &mut recorder);
    recorder.log("Resetting flashed device");
    reset_chip(&mut flasher, &mut recorder, "post-flash reset")?;

    Ok(LinkFirmwareFlashResult {
        manifest,
        chip_name,
        // No MAC read on this path yet. A2 evidence is what the BROWSER
        // flash preflight collects, because that is the flow a user's board
        // is identified in; the host flasher is the bench/CLI route and
        // reading it here would be an extra ROM round-trip nothing consumes.
        probed_mac: None,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

/// Write `data` at `address` and prove it landed: the stub's MD5 of the
/// written range must equal the bytes' own. Returns that MD5, hex.
///
/// espflash 3.3.0's `write_bin_to_flash` checks nothing (`verify` is off
/// on every connection here, and its own check would run before
/// `FLASH_DEFL_END`), and its stub silently dropped the last 254 bytes of a
/// packaged image whose length was not a multiple of 4: the engine's last
/// functions read back `0xFF` and it faulted on every boot
/// (`docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md`).
/// The check runs after the write is finished, over the whole range, so any
/// short or wrong write fails here, loudly, instead of on the next boot.
///
/// Needs the stub still up afterwards: a connection whose after-operation
/// resets the chip (`HardReset`) cannot be checked.
pub(super) fn write_verified(
    flasher: &mut Flasher,
    address: u32,
    data: &[u8],
    progress: Option<&mut dyn ProgressCallbacks>,
) -> Result<String, String> {
    flasher
        .write_bin_to_flash(address, data, progress)
        .map_err(|error| format!("flash write failed: {error}"))?;
    let local: [u8; 16] = md5::Md5::digest(data).into();
    let length = data.len() as u32;
    let device = flasher
        .connection()
        .with_timeout(md5_timeout(length), |connection| {
            connection
                .command(Command::FlashMd5 {
                    offset: address,
                    size: length,
                })?
                .try_into()
        })
        .map_err(|error: espflash::error::Error| {
            format!(
                "could not check the write at {address:#x} (+{length} B): the flash MD5 \
                 failed: {error}"
            )
        })
        .map(|value: u128| value.to_be_bytes())?;
    check_written(address, length, &local, &device)
}

/// The written range's MD5 against the bytes'. `Ok` carries it, hex.
fn check_written(
    address: u32,
    length: u32,
    local: &[u8; 16],
    device: &[u8; 16],
) -> Result<String, String> {
    let hex = |d: &[u8; 16]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    if local == device {
        Ok(hex(local))
    } else {
        Err(format!(
            "the flash does not hold what was written at {address:#x} (+{length} B): its MD5 \
             is {}, the image's is {} — the write is incomplete; flash again before resetting \
             the board",
            hex(device),
            hex(local)
        ))
    }
}

/// How long the stub may take to hash `length` bytes of flash: esptool's
/// 8 s per MB, never less than espflash's 8 s.
fn md5_timeout(length: u32) -> Duration {
    const PER_MB_MS: u64 = 8_000;
    Duration::from_millis((u64::from(length) * PER_MB_MS / 1_000_000).max(PER_MB_MS))
}

/// Full-chip erase, leaving the device blank (the `BlankFlash` readiness
/// state). Emits live progress into `events`.
pub(super) fn erase_device_flash(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<LinkEraseDeviceResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    recorder.log("Erasing device flash");

    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::HardReset,
        &mut recorder,
    )?;
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

/// Write the boot-control sector, instructing the device's next boot.
///
/// **One write, not several.** `write_bin_to_flash` issues `FLASH_BEGIN`,
/// which erases the sectors it is about to write — so splitting the 16-byte
/// record across two writes would have the second erase the first, leaving a
/// record that always fails its CRC and a feature that silently never works.
/// The explicit `erase_region` below is belt-and-braces for the rest of the
/// sector; integrity of the record itself comes from its magic and CRC.
pub(super) fn write_boot_control(
    port_name: &str,
    flags: BootFlags,
    events: &LinkManagementEventSink,
) -> Result<LinkBootControlResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    recorder.log(format!(
        "Writing boot-control record (flags {:#010x})",
        flags.bits()
    ));

    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::HardReset,
        &mut recorder,
    )?;
    let chip_name = chip_name(&mut flasher);

    recorder.progress(LinkManagementProgress::new("Erasing boot-control sector"));
    flasher
        .erase_region(BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE)
        .map_err(|error| LinkError::other(format!("boot-control erase failed: {error}")))?;

    recorder.progress(LinkManagementProgress::new("Writing boot-control record"));
    flasher
        .write_bin_to_flash(BOOTCTL_PARTITION_OFFSET, &encode_record(flags), None)
        .map_err(|error| LinkError::other(format!("boot-control write failed: {error}")))?;
    recorder.progress(LinkManagementProgress::new("Writing boot-control record").with_percent(100));

    reset_into_app(&mut flasher, &mut recorder);
    recorder.log("Boot-control record written; it applies on the next restart");

    Ok(LinkBootControlResult {
        flags: flags.bits(),
        chip_name,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

/// Read the device's `lpfs` partition back to the host, verbatim.
///
/// The region is read from the device's own partition table at `0x8000`, in
/// the same session, never hardcoded: boards put `lpfs` in different places
/// (and a C6 moved it in 2026-10), and a backup of the wrong 960 KB looks
/// exactly like a backup of the right one. A table with no `lpfs` row is
/// refused.
///
/// **Default baud, deliberately.** These parts speak USB-Serial-JTAG, where
/// the baud parameter is meaningless and negotiating a higher one costs real
/// time — measured on the bench (M1): 3.2 s at the default versus 4.2 s at
/// 921600 for the same 960 KB. Do not "optimize" this by raising it.
///
/// The read is acked per packet, so progress is genuinely per-block rather
/// than a spinner: 240 packets for a C6's partition.
pub fn read_raw_filesystem(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<LinkRawFilesystemReadResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::HardReset,
        &mut recorder,
    )?;
    let chip_name = chip_name(&mut flasher);
    let partition_table = read_flash_region(
        &mut flasher,
        LinkFlashRegion {
            offset: crate::PARTITION_TABLE_OFFSET,
            length: crate::PARTITION_TABLE_LEN as u32,
        },
        "Reading partition table",
        &mut recorder,
    )?;
    let region = crate::PartitionTable::parse(&partition_table)
        .ok()
        .as_ref()
        .and_then(LinkFlashRegion::lpfs_in)
        .ok_or_else(|| {
            LinkError::other(format!(
                "the {} holds no LightPlayer filesystem partition (its partition table has \
                 no `lpfs` row)",
                chip_name.as_deref().unwrap_or("device")
            ))
        })?;
    recorder.log(format!(
        "Reading {} bytes of filesystem at {:#x}",
        region.length, region.offset
    ));

    let image = read_flash_region(&mut flasher, region, "Reading filesystem", &mut recorder)?;
    reset_into_app(&mut flasher, &mut recorder);
    recorder.log("Filesystem read complete");

    Ok(LinkRawFilesystemReadResult {
        image,
        region,
        partition_table,
        chip_name,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

/// Drive `ESP_READ_FLASH` over an established connection, acking each packet
/// and reporting progress, and verify the trailing MD5 the device sends.
///
/// espflash's own `Flasher::read_flash` writes straight to a file and reports
/// nothing, neither of which suits a browser-shaped operation whose whole UX
/// problem is looking hung; this keeps the bytes in memory and narrates.
pub(super) fn read_flash_region(
    flasher: &mut Flasher,
    region: LinkFlashRegion,
    label: &str,
    recorder: &mut EventRecorder,
) -> Result<Vec<u8>, LinkError> {
    recorder.progress(
        LinkManagementProgress::new(label)
            .with_steps(0, region.length)
            .with_percent(0),
    );

    let connection = flasher.connection();
    connection
        .with_timeout(CommandType::ReadFlash.timeout(), |connection| {
            connection.command(Command::ReadFlash {
                offset: region.offset,
                size: region.length,
                block_size: READ_BLOCK_SIZE,
                max_in_flight: READ_MAX_IN_FLIGHT,
            })
        })
        .map_err(|error| LinkError::other(format!("filesystem read failed to start: {error}")))?;

    let total = region.length as usize;
    let mut image: Vec<u8> = Vec::with_capacity(total);
    while image.len() < total {
        let chunk = read_vector_response(connection, "filesystem data")?;
        // A short packet before the end means the device stopped mid-stream;
        // a silently truncated backup is the worst possible outcome here.
        if image.len() + chunk.len() < total && chunk.len() < READ_BLOCK_SIZE as usize {
            return Err(LinkError::other(format!(
                "filesystem read truncated at {} of {total} bytes",
                image.len() + chunk.len()
            )));
        }
        image.extend_from_slice(&chunk);
        // The device waits for the running total before sending more.
        connection
            .write_raw(image.len() as u32)
            .map_err(|error| LinkError::other(format!("filesystem read ack failed: {error}")))?;
        recorder.progress(
            LinkManagementProgress::new(label)
                .with_steps(image.len().min(total) as u32, region.length)
                .with_percent(((image.len().min(total) as u64 * 100) / total as u64) as u32),
        );
    }
    if image.len() > total {
        return Err(LinkError::other(format!(
            "filesystem read returned {} bytes, expected {total}",
            image.len()
        )));
    }

    let digest = read_vector_response(connection, "filesystem digest")?;
    let mut hasher = md5::Md5::new();
    hasher.update(&image);
    if digest != hasher.finalize().as_slice() {
        return Err(LinkError::other(
            "filesystem read failed its checksum — the image is not trustworthy",
        ));
    }
    recorder.progress(
        LinkManagementProgress::new(label)
            .with_steps(region.length, region.length)
            .with_percent(100),
    );
    Ok(image)
}

/// One `Vector` response from the flash-read stream.
fn read_vector_response(
    connection: &mut espflash::connection::Connection,
    what: &str,
) -> Result<Vec<u8>, LinkError> {
    let response = connection
        .read_response()
        .map_err(|error| LinkError::other(format!("{what} read failed: {error}")))?
        .ok_or_else(|| LinkError::other(format!("{what}: the device stopped responding")))?;
    match response.value {
        espflash::connection::CommandResponseValue::Vector(bytes) => Ok(bytes),
        other => Err(LinkError::other(format!(
            "{what}: unexpected response {other:?}"
        ))),
    }
}

/// Ask the device whether a ROM/stub bootloader is listening, and which chip
/// it is.
///
/// This is the **authoritative** bootloader-mode test: `connect` performs the
/// esptool SYNC handshake, which only a bootloader answers. Enumeration data
/// cannot substitute — USB-Serial-JTAG parts present the same VID/PID in app
/// mode and download mode.
///
/// **It reboots the device.** `connect` drives DTR/RTS to enter download
/// mode, and on USB-Serial-JTAG that reset drops USB enumeration. Callers
/// must own the wire exclusively and rebuild the link afterwards; never run
/// this speculatively against a healthy board.
///
/// `Ok(None)` means "answered, but would not name itself" — still a
/// bootloader. `Err` means nothing answered, which is *not* proof the device
/// is absent; it may be running the app.
pub(super) fn probe_target(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<Option<String>, LinkError> {
    let mut recorder = EventRecorder::new(events);
    recorder.log(format!("Probing {port_name} for a bootloader"));
    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::HardReset,
        &mut recorder,
    )?;
    let chip_name = chip_name(&mut flasher);
    reset_into_app(&mut flasher, &mut recorder);
    recorder.log(match &chip_name {
        Some(name) => format!("Bootloader answered: {name}"),
        None => "Bootloader answered (chip did not identify itself)".to_string(),
    });
    Ok(chip_name)
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
/// USB-JTAG download-mode entry; `after` is what the operation's `finish`
/// (or [`reset_into_app`]) applies once it is done — `HardReset` everywhere
/// but the flash path, which resets by hand after
/// [`restore_lp_analog_i2c_clock`].
/// Open `port_name` and run the espflash handshake.
///
/// `expect_chip` is `Some` only on the flash path, where an image is about to
/// be written and the chip it was built for is the one we are willing to
/// find. Every other operation here (erase, raw read, boot-control) is
/// chip-agnostic by design — the raw read in particular exists for a board
/// nobody can identify any other way, so declaring a chip would refuse the
/// one case it is for.
pub(super) fn connect(
    port_name: &str,
    expect_chip: Option<Chip>,
    after: ResetAfterOperation,
    recorder: &mut EventRecorder,
) -> Result<Flasher, LinkError> {
    recorder.log(format!("Connecting to {port_name}"));
    prepare_lp_domain(port_name, recorder);
    let mut serial = open_port(port_name)?;
    let before = reset_before(&mut serial, recorder);
    Flasher::connect(
        serial,
        port_info_for(port_name),
        Some(CONNECT_BAUD),
        /* use_stub  */ true,
        /* verify    */ false,
        /* skip      */ false,
        expect_chip,
        after,
        before,
    )
    .map_err(|error| LinkError::other(format!("espflash connect failed: {error}")))
}

/// How to reach the bootloader on this port: the DTR/RTS dance, unless the
/// port has no modem lines at all — a pseudo-terminal (the emulated board's
/// pty bridge), where every line ioctl fails ENOTTY and the board is put in
/// its download console by whoever holds the other end. Probed with a
/// read-only line query, so a real port is never touched by the probe.
fn reset_before(
    serial: &mut serialport::TTYPort,
    recorder: &mut EventRecorder,
) -> ResetBeforeOperation {
    use serialport::SerialPort;
    match serial.read_clear_to_send() {
        Err(error) if is_enotty(&error) => {
            recorder.log(
                "no modem lines on this port (a pseudo-terminal): connecting without the reset \
                 dance",
            );
            recorder.no_modem_lines = true;
            ResetBeforeOperation::NoReset
        }
        _ => ResetBeforeOperation::DefaultReset,
    }
}

/// Reset the chip out of its bootloader into its firmware. On a port with
/// no modem lines (a pseudo-terminal) there is nothing to toggle: the reset
/// is the harness's (the emulator's control channel), so it is logged, not
/// fatal.
pub(super) fn reset_chip(
    flasher: &mut Flasher,
    recorder: &mut EventRecorder,
    what: &str,
) -> Result<(), LinkError> {
    if recorder.no_modem_lines {
        recorder.log(format!(
            "{what}: no reset lines on this port (a pseudo-terminal) — reset the board from \
                 the other end"
        ));
        return Ok(());
    }
    flasher
        .connection()
        .reset()
        .map_err(|error| LinkError::other(format!("{what} failed: {error}")))
}

/// Open the port, tolerating the brief window after a link release where the
/// OS has not yet freed the USB serial FD.
///
/// `DeviceSession::manage` releases the wire (closing the byte-stream
/// framing thread) and then immediately runs an operation whose first act is
/// to reopen the port here; on macOS the just-closed `/dev/cu.*` reports
/// "Resource busy" for a short beat after the previous holder dropped it
/// (worsened by our own ROM pre-pass, which opens once more right before the
/// stub connect). A bounded retry turns that transient into a non-event;
/// anything else (a real second holder, a vanished port) surfaces after the
/// budget, unchanged.
/// "Inappropriate ioctl for device": the port is not a real serial device.
fn is_enotty(error: &serialport::Error) -> bool {
    // serialport folds the errno into a description; nix renders ENOTTY as
    // its libc text, which differs by platform.
    let text = error.to_string();
    text.contains("ENOTTY")
        || text.contains("Not a typewriter")
        || text.contains("Inappropriate ioctl")
}

fn open_port(port_name: &str) -> Result<serialport::TTYPort, LinkError> {
    const OPEN_ATTEMPTS: u32 = 20;
    const OPEN_RETRY: Duration = Duration::from_millis(100);
    let mut last: Option<serialport::Error> = None;
    for attempt in 0..OPEN_ATTEMPTS {
        match serialport::new(port_name, CONNECT_BAUD)
            .flow_control(serialport::FlowControl::None)
            .open_native()
        {
            Ok(port) => return Ok(port),
            // A pseudo-terminal (the emulated board's bridge, a pty test
            // harness): macOS has no baud ioctl for one (`IOSSIOSPEED` fails
            // with ENOTTY) and a baud means nothing there, so open it without
            // one. A real USB serial port never answers ENOTTY.
            Err(error) if is_enotty(&error) => {
                return serialport::new(port_name, 0)
                    .flow_control(serialport::FlowControl::None)
                    .open_native()
                    .map_err(|error| {
                        LinkError::other(format!("failed to open {port_name}: {error}"))
                    });
            }
            Err(error)
                if error.kind() == serialport::ErrorKind::Io(std::io::ErrorKind::ResourceBusy) =>
            {
                last = Some(error);
                if attempt + 1 < OPEN_ATTEMPTS {
                    std::thread::sleep(OPEN_RETRY);
                }
            }
            Err(error) => {
                return Err(LinkError::other(format!(
                    "failed to open {port_name}: {error}"
                )));
            }
        }
    }
    Err(LinkError::other(format!(
        "failed to open {port_name} after {OPEN_ATTEMPTS} attempts: {}",
        last.map(|error| error.to_string()).unwrap_or_default()
    )))
}

/// A ROM-only pass over the board BEFORE the stub is loaded: on a C6 whose
/// previous firmware gated the LP analog I2C clock, espflash's RAM stub
/// never answers its first command (bench, 2026-09-06: `Timeout while
/// running command` 3.7 s in, on every stub operation; the ROM alone
/// connects and answers `board-info` fine, and after the fix below the stub
/// comes up again). So the same restore the flash path runs before its
/// closing reset also has to run here, over the ROM, or a factory-fresh
/// board cannot be flashed at all. One extra ROM handshake per operation;
/// silent on a clean board.
///
/// Best-effort: any failure is logged and the real connect below produces
/// the error that matters (chip mismatch, no bootloader, port gone).
fn prepare_lp_domain(port_name: &str, recorder: &mut EventRecorder) {
    let serial = match open_port(port_name) {
        Ok(serial) => serial,
        Err(error) => {
            recorder.log(format!("warning: LP-domain pre-check skipped: {error}"));
            return;
        }
    };
    let flasher = Flasher::connect(
        serial,
        port_info_for(port_name),
        Some(CONNECT_BAUD),
        /* use_stub  */ false,
        /* verify    */ false,
        /* skip      */ false,
        None,
        ResetAfterOperation::NoResetNoStub,
        ResetBeforeOperation::DefaultReset,
    );
    match flasher {
        Ok(mut flasher) => restore_lp_analog_i2c_clock(&mut flasher, recorder),
        Err(error) => recorder.log(format!("warning: LP-domain pre-check skipped: {error}")),
    }
    // Dropping the ROM session closes the port; the stub connect that
    // follows resets into the downloader again (HP-only — the restore
    // survives it).
}

/// Refuse to write `manifest` onto `detected`.
///
/// Belt to the declared chip's braces: `Flasher::connect` already rejects a
/// mismatch, but it does so as espflash's `ChipMismatch`, which names neither
/// the image nor what the user should do about it. This runs before the first
/// `write_bin_to_flash` and says both.
pub(super) fn assert_chip_matches_manifest(
    detected: Chip,
    manifest: &LinkFirmwareManifest,
) -> Result<(), LinkError> {
    let detected_name = detected.to_string();
    if crate::chip_ids_match(&detected_name, &manifest.target_chip) {
        return Ok(());
    }
    Err(LinkError::other(format!(
        "refusing to flash: this device is {detected_name}, but the firmware image {} \
         is built for {}",
        manifest.firmware_id, manifest.target_chip
    )))
}

/// Apply the connection's `after` operation (HardReset) so the chip leaves
/// download mode and boots the (now blank) flash. Needed on the ERASE path
/// only: erase commands have no flash-target `finish`, so nothing else
/// applies the after-operation (the espflash CLI's erase commands do the
/// same). Best-effort: a reset failure is logged but not fatal —
/// `DeviceSession` re-runs readiness on rebuild regardless.
pub(super) fn reset_into_app(flasher: &mut Flasher, recorder: &mut EventRecorder) {
    if recorder.no_modem_lines {
        recorder.log(
            "no reset lines on this port (a pseudo-terminal) — reset the board from the other end",
        );
        return;
    }
    // `is_stub = true` matches the `use_stub = true` passed to `connect`.
    if let Err(error) = flasher.connection().reset_after(true) {
        recorder.log(format!("warning: post-operation reset failed: {error}"));
    }
}

/// Undo the LP-domain state a fresh board's factory firmware leaves behind,
/// so the bootloader we just wrote can boot on the reset that follows. C6
/// only; a no-op (and silent) on a board whose clock is on and whose master
/// is idle. See [`super::lp_analog_i2c`] for the mechanism and the bench proof.
///
/// Best-effort on purpose: a register read that fails is logged, and the
/// flash still ends in a reset — the board may then need a replug, which is
/// exactly where it stood before this existed.
pub(super) fn restore_lp_analog_i2c_clock(flasher: &mut Flasher, recorder: &mut EventRecorder) {
    if flasher.chip() != Chip::Esp32c6 {
        return;
    }
    let connection = flasher.connection();
    let read = |connection: &mut espflash::connection::Connection, addr: u32| {
        connection
            .read_reg(addr)
            .map_err(|error| LinkError::other(format!("read 0x{addr:08x}: {error}")))
    };
    let result: Result<Option<String>, LinkError> = (|| {
        let clk_before = read(connection, LPPERI_CLK_EN)?;
        let ctrl = read(connection, LP_I2C_ANA_MST_I2C0_CTRL)?;
        let Some(fix) = lp_analog_i2c_plan(clk_before, ctrl) else {
            return Ok(None);
        };
        let write = |connection: &mut espflash::connection::Connection, addr: u32, value: u32| {
            connection
                .write_reg(addr, value, None)
                .map_err(|error| LinkError::other(format!("write 0x{addr:08x}: {error}")))
        };
        write(connection, LPPERI_CLK_EN, clk_before | LP_ANA_I2C_BIT)?;
        // The clock alone does not clear a latched busy: pulse the master's
        // reset line (bench, 2026-09-06).
        let reset_before = read(connection, LPPERI_RESET_EN)?;
        write(connection, LPPERI_RESET_EN, reset_before | LP_ANA_I2C_BIT)?;
        write(connection, LPPERI_RESET_EN, reset_before & !LP_ANA_I2C_BIT)?;
        let clk_after = read(connection, LPPERI_CLK_EN)?;
        let busy_after = read(connection, LP_I2C_ANA_MST_I2C0_CTRL)? & LP_I2C_ANA_MST_BUSY != 0;
        Ok(Some(describe(fix, clk_before, clk_after, busy_after)))
    })();
    match result {
        Ok(Some(line)) => recorder.log(line),
        Ok(None) => {}
        Err(error) => recorder.log(format!(
            "warning: LP analog I2C clock check failed: {error}"
        )),
    }
}

pub(super) fn chip_name(flasher: &mut Flasher) -> Option<String> {
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
pub(super) struct EventRecorder<'a> {
    sink: &'a LinkManagementEventSink,
    pub(super) logs: Vec<String>,
    pub(super) progress: Vec<LinkManagementProgress>,
    /// The port has no modem lines (a pseudo-terminal), as `connect`
    /// found: nothing can reset the chip from this end.
    pub(super) no_modem_lines: bool,
}

impl<'a> EventRecorder<'a> {
    pub(super) fn new(sink: &'a LinkManagementEventSink) -> Self {
        Self {
            sink,
            logs: Vec::new(),
            progress: Vec::new(),
            no_modem_lines: false,
        }
    }

    pub(super) fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.sink.emit(LinkManagementEvent::log(message.clone()));
        self.logs.push(message);
    }

    pub(super) fn progress(&mut self, progress: LinkManagementProgress) {
        self.sink
            .emit(LinkManagementEvent::progress(progress.clone()));
        self.progress.push(progress);
    }
}

/// Bridges espflash's byte-count [`ProgressCallbacks`] onto our step/percent
/// [`LinkManagementProgress`] events. One bridge per flashed image.
pub(super) struct ProgressBridge<'a, 'b> {
    recorder: &'a mut EventRecorder<'b>,
    label: String,
    total: u32,
}

impl<'a, 'b> ProgressBridge<'a, 'b> {
    pub(super) fn new(recorder: &'a mut EventRecorder<'b>, label: String) -> Self {
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

/// The only `manifest.json` schemaVersion this build reads.
const FIRMWARE_MANIFEST_SCHEMA_VERSION: u32 = 2;

/// A firmware image resolved against the manifest directory.
#[derive(Debug)]
pub(super) struct ResolvedImage {
    pub(super) absolute_path: PathBuf,
    pub(super) address: u32,
}

/// Load and validate the firmware manifest, returning a provider-neutral
/// [`LinkFirmwareManifest`] plus the images resolved to absolute paths.
pub(super) fn load_manifest(
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
    if raw.schema_version != FIRMWARE_MANIFEST_SCHEMA_VERSION {
        return Err(LinkError::other(format!(
            "firmware manifest {} has schemaVersion {} — this build understands \
             only {}; repackage with `lp-cli firmware package`",
            manifest_path.display(),
            raw.schema_version,
            FIRMWARE_MANIFEST_SCHEMA_VERSION
        )));
    }
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
        target_chip: raw.core.platform.chip,
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

/// The subset of `manifest.json` (produced by `lp-cli firmware package`) this
/// provider consumes. schemaVersion 2 only — alpha posture is version +
/// refuse, so a v1 manifest is an error, never a fallback decode.
#[derive(Deserialize)]
struct RawManifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "firmwareId")]
    firmware_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    /// The manifest core extracted from the image; the chip identity lives
    /// here because the *build* is what knows it.
    core: RawCore,
    images: Vec<RawImage>,
}

#[derive(Deserialize)]
struct RawCore {
    platform: RawPlatform,
}

#[derive(Deserialize)]
struct RawPlatform {
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
        "schemaVersion": 2,
        "firmwareId": "esp32c6-4mb",
        "displayName": "LightPlayer ESP32-C6 server firmware",
        "generatedAt": "2026-08-01T12:00:00Z",
        "core": {
            "lpManifestCore": 3,
            "package": "fw-esp32c6",
            "target": "esp32c6-4mb",
            "profile": "release-esp32",
            "commit": "abc123456789",
            "dirty": false,
            "platform": {
                "family": "esp32",
                "chip": "esp32c6",
                "cargoTarget": "riscv32imac-unknown-none-elf"
            },
            "features": ["node.shader", "gfx.lpvm"],
            "limits": { "flashAppBytes": 3145728 },
            "wireProto": 4
        },
        "flash": { "format": "espflash-merged-image", "address": "0x0" },
        "images": [
            {
                "path": "fw-esp32c6-merged.bin",
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
        assert_eq!(manifest.firmware_id, "esp32c6-4mb");
        assert_eq!(manifest.target_chip, "esp32c6");
        assert_eq!(manifest.image_count, 1);
        assert_eq!(manifest.total_bytes, 3_022_960);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].address, 0x0);
        assert_eq!(images[0].absolute_path, dir.join("fw-esp32c6-merged.bin"));
    }

    /// Version + refuse: a v1 manifest is rejected with its version named,
    /// not decoded on a best-effort basis.
    #[test]
    fn refuses_a_v1_manifest() {
        let dir = std::env::temp_dir().join("lpa-link-manifest-v1-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        std::fs::write(
            &path,
            MANIFEST_JSON.replace("\"schemaVersion\": 2", "\"schemaVersion\": 1"),
        )
        .unwrap();

        let error = load_manifest(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("schemaVersion 1"), "{error}");
    }

    #[test]
    fn rejects_manifest_without_images() {
        let dir = std::env::temp_dir().join("lpa-link-manifest-empty-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        std::fs::write(
            &path,
            r#"{
                "schemaVersion": 2,
                "firmwareId": "x",
                "displayName": "x",
                "core": { "platform": { "chip": "esp32c6" } },
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
    fn every_served_build_target_maps_to_an_espflash_chip() {
        // The three chips the site serves images for. A build def whose
        // `chip.name` this cannot map is a firmware variant the host
        // provider silently cannot flash.
        assert_eq!(manifest_chip("esp32c6"), Some(Chip::Esp32c6));
        assert_eq!(manifest_chip("esp32s3"), Some(Chip::Esp32s3));
        assert_eq!(manifest_chip("esp32"), Some(Chip::Esp32));
        // esptool-js's chatty spelling reaches this through the manifest's
        // `core.platform.chip` only in principle, but normalizing costs
        // nothing and keeps the two providers' rules identical.
        assert_eq!(manifest_chip("ESP32-C6"), Some(Chip::Esp32c6));
        assert_eq!(manifest_chip("esp32c3"), None);
    }

    #[test]
    fn a_mismatched_image_is_refused_by_name() {
        let manifest = LinkFirmwareManifest {
            firmware_id: "esp32c6-4mb".into(),
            display_name: "LightPlayer ESP32-C6 server firmware".into(),
            target_chip: "esp32c6".into(),
            image_count: 1,
            total_bytes: 1,
            manifest_path: None,
        };
        assert!(assert_chip_matches_manifest(Chip::Esp32c6, &manifest).is_ok());

        let error = assert_chip_matches_manifest(Chip::Esp32s3, &manifest)
            .expect_err("an S3 must not take a C6 image");
        let text = error.to_string();
        // Both halves of the mismatch, so the message is actionable without
        // reading the log above it.
        assert!(text.contains("esp32s3"), "{text}");
        assert!(text.contains("esp32c6"), "{text}");
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

    /// The bench C6's case: the flash held the image up to `0x2F54FF` and
    /// `0xFF` after it, so its MD5 was not the image's. That is a refusal
    /// naming the range, never a reset into a truncated engine.
    #[test]
    fn a_short_write_is_refused_by_its_md5() {
        let image = vec![0x37u8; 0x2F_55FE];
        let mut flash = image.clone();
        flash[0x2F_5500..].fill(0xff);
        let local: [u8; 16] = md5::Md5::digest(&image).into();
        let device: [u8; 16] = md5::Md5::digest(&flash).into();
        let error = check_written(0, image.len() as u32, &local, &device).unwrap_err();
        assert!(
            error.contains("does not hold what was written at 0x0"),
            "{error}"
        );
        assert!(error.contains("+3102206 B"), "{error}");
        assert_eq!(check_written(0, 4, &local, &local).unwrap().len(), 32);
    }

    #[test]
    fn the_md5_timeout_grows_with_the_range() {
        assert_eq!(md5_timeout(16), Duration::from_secs(8));
        assert_eq!(md5_timeout(3_102_208), Duration::from_millis(24_817));
    }
}
