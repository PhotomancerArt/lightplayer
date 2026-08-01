//! Chip identification probe for attached ESP32 boards.
//!
//! Wraps espflash's connect handshake (reset into bootloader, sync, read the
//! chip magic) behind a hard timeout, so a wedged or hijacked port can never
//! hang the caller. The shell loops this replaces ran bare `espflash
//! board-info` with no such guard — and macOS ships no `timeout(1)` to add
//! one from the outside.
//!
//! A probe is ACTIVE: it resets an idle board into the ROM bootloader and
//! hard-resets it back into the application afterwards. Ports held open by
//! another process (a monitor, another agent's flash) fail the open and are
//! reported as errors rather than reset out from under their owner.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use espflash::connection::reset::{ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use serialport::{SerialPortType, UsbPortInfo};

/// ROM/stub default baud, same as every other espflash entry point here.
const CONNECT_BAUD: u32 = 115_200;

#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// espflash identified the chip (`esp32c6`, `esp32s3`, ...).
    Chip(String),
    /// No handshake result within the deadline. The abandoned probe thread
    /// may keep the OS port open until this process exits.
    Timeout,
    /// The handshake failed: port busy, no ESP32 behind it, etc.
    Error(String),
}

impl ProbeOutcome {
    /// Short human-readable form for tables and error listings.
    pub fn describe(&self) -> String {
        match self {
            ProbeOutcome::Chip(chip) => chip.clone(),
            ProbeOutcome::Timeout => "timeout (port busy or not responding)".to_string(),
            ProbeOutcome::Error(error) => format!("error: {error}"),
        }
    }

    /// Whether the probed chip matches `chip` (compared with
    /// [`normalize_chip`], so `esp32-s3`, `ESP32S3`, and `esp32s3` all agree).
    pub fn matches_chip(&self, chip: &str) -> bool {
        match self {
            ProbeOutcome::Chip(probed) => normalize_chip(probed) == normalize_chip(chip),
            _ => false,
        }
    }
}

/// Lowercase and strip non-alphanumerics so chip names compare across the
/// spellings in circulation (`esp32s3` / `esp32-s3` / `ESP32S3`).
pub fn normalize_chip(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Identify the chip behind `port_name`, giving up after `timeout`.
///
/// The espflash handshake is synchronous serial I/O with its own internal
/// retries; it runs on a spawned thread and the deadline is enforced on the
/// channel, which is the only way to bound it without cooperation.
pub fn probe_esp32_chip(port_name: &str, timeout: Duration) -> ProbeOutcome {
    let (sender, receiver) = mpsc::channel();
    let port = port_name.to_owned();
    thread::spawn(move || {
        // A send failure means the receiver timed out and left; the outcome
        // has nowhere to go.
        let _ = sender.send(probe_blocking(&port));
    });
    match receiver.recv_timeout(timeout) {
        Ok(outcome) => outcome,
        Err(_) => ProbeOutcome::Timeout,
    }
}

fn probe_blocking(port_name: &str) -> ProbeOutcome {
    let serial = match serialport::new(port_name, CONNECT_BAUD)
        .flow_control(serialport::FlowControl::None)
        .open_native()
    {
        Ok(serial) => serial,
        Err(error) => return ProbeOutcome::Error(format!("open failed: {error}")),
    };
    // `chip: None` detects the chip from its magic value; `use_stub: false`
    // keeps the handshake to the ROM loader (identification needs no stub).
    let mut flasher = match Flasher::connect(
        serial,
        usb_port_info_for(port_name),
        Some(CONNECT_BAUD),
        /* use_stub */ false,
        /* verify   */ false,
        /* skip     */ false,
        None,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
    ) {
        Ok(flasher) => flasher,
        Err(error) => return ProbeOutcome::Error(format!("espflash connect failed: {error}")),
    };
    let chip = flasher.chip().to_string();
    // Boot back into the application firmware — the connect reset left the
    // chip in the bootloader. Best-effort: identification already succeeded.
    let _ = flasher.connection().reset_after(false);
    ProbeOutcome::Chip(chip)
}

/// Resolve `UsbPortInfo` for `port_name` from the OS port list. espflash
/// picks its reset strategy by USB PID (USB-Serial-JTAG vs classic DTR/RTS),
/// so a correct pid matters; fall back to zeros if the port isn't enumerable.
pub fn usb_port_info_for(port_name: &str) -> UsbPortInfo {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_chip_spellings() {
        assert_eq!(normalize_chip("esp32-s3"), "esp32s3");
        assert_eq!(normalize_chip("ESP32S3"), "esp32s3");
        assert!(ProbeOutcome::Chip("esp32c6".into()).matches_chip("ESP32-C6"));
        assert!(!ProbeOutcome::Timeout.matches_chip("esp32c6"));
    }
}
