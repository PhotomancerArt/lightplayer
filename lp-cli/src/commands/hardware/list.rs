//! `lp-cli hardware list` — enumerate attached serial hardware.
//!
//! Two tiers, so the default can never hang:
//!
//! - **Passive (default):** OS port enumeration with USB VID/PID/product.
//!   Never opens a port. This distinguishes bridge chips (CH34x, CP210x,
//!   FTDI) from Espressif native USB-Serial/JTAG, but native USB shares one
//!   PID across chip families, so it cannot say S3 vs C6.
//! - **Active (`--probe` / `--chip`):** the espflash handshake identifies the
//!   chip on each candidate port, with a per-port timeout enforced in-process
//!   (macOS has no `timeout(1)`). Probing resets idle boards; busy ports fail
//!   the open and are reported instead of being reset under their owner.

use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;
use serialport::SerialPortType;

use super::args::ListArgs;
use crate::client::esp32_probe::{ProbeOutcome, normalize_chip, probe_esp32_chip};

#[derive(Debug, Serialize)]
struct PortEntry {
    port: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vid: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    serial_number: Option<String>,
    /// Human classification of the USB device (bridge chip or native USB).
    kind: String,
    /// Probed chip name (`esp32s3`, ...) when identification succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    chip: Option<String>,
    /// Probe failure description when a probe ran and did not identify.
    #[serde(skip_serializing_if = "Option::is_none")]
    probe_error: Option<String>,
}

pub fn handle_list(args: ListArgs) -> Result<()> {
    let probe = args.probe || args.chip.is_some();
    let mut entries = enumerate_ports(args.all)?;

    if probe {
        let timeout = Duration::from_secs(args.probe_timeout_secs);
        for entry in &mut entries {
            match probe_esp32_chip(&entry.port, timeout) {
                ProbeOutcome::Chip(chip) => entry.chip = Some(chip),
                outcome => entry.probe_error = Some(outcome.describe()),
            }
        }
    }

    if let Some(chip) = &args.chip {
        let wanted = normalize_chip(chip);
        entries.retain(|entry| {
            entry
                .chip
                .as_deref()
                .is_some_and(|probed| normalize_chip(probed) == wanted)
        });
        if entries.is_empty() {
            bail!("no attached board probed as `{chip}`");
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if entries.is_empty() {
        eprintln!("no USB serial ports found (--all shows non-USB ports)");
        return Ok(());
    }
    print_table(&entries, probe);
    Ok(())
}

/// Enumerate OS serial ports as structured entries.
///
/// macOS exposes each device twice (`/dev/tty.*` dial-in and `/dev/cu.*`
/// call-out); only the `cu.*` twin is kept, since ESP32 boards never assert
/// DCD and the `tty.*` twin blocks on open. Non-USB ports (Bluetooth, debug
/// consoles) are noise for board work and hidden unless `all` is set.
fn enumerate_ports(all: bool) -> Result<Vec<PortEntry>> {
    let ports = serialport::available_ports()?;
    let cu_suffixes: Vec<String> = ports
        .iter()
        .filter_map(|port| port.port_name.strip_prefix("/dev/cu."))
        .map(str::to_owned)
        .collect();

    let mut entries: Vec<PortEntry> = ports
        .into_iter()
        .filter(|port| match port.port_name.strip_prefix("/dev/tty.") {
            Some(suffix) => !cu_suffixes.iter().any(|cu| cu == suffix),
            None => true,
        })
        .filter_map(|port| {
            let usb = match port.port_type {
                SerialPortType::UsbPort(info) => Some(info),
                _ if all => None,
                _ => return None,
            };
            Some(PortEntry {
                port: port.port_name,
                vid: usb.as_ref().map(|info| info.vid),
                pid: usb.as_ref().map(|info| info.pid),
                product: usb.as_ref().and_then(|info| info.product.clone()),
                serial_number: usb.as_ref().and_then(|info| info.serial_number.clone()),
                kind: usb
                    .as_ref()
                    .map(|info| describe_usb_device(info.vid).to_string())
                    .unwrap_or_else(|| "non-USB serial".to_string()),
                chip: None,
                probe_error: None,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.port.cmp(&b.port));
    Ok(entries)
}

/// Classify by USB vendor id. Espressif's native USB-Serial/JTAG shares PID
/// 0x1001 across chip families, so the vendor is the most it can say
/// passively; bridge chips hide the ESP32 entirely.
fn describe_usb_device(vid: u16) -> &'static str {
    match vid {
        0x303A => "Espressif USB-Serial/JTAG",
        0x1A86 => "WCH CH34x bridge",
        0x10C4 => "Silicon Labs CP210x bridge",
        0x0403 => "FTDI bridge",
        _ => "USB serial",
    }
}

fn print_table(entries: &[PortEntry], probed: bool) {
    let port_width = entries
        .iter()
        .map(|entry| entry.port.len())
        .chain(["PORT".len()])
        .max()
        .unwrap_or(0);
    let kind_width = entries
        .iter()
        .map(|entry| entry.kind.len())
        .chain(["KIND".len()])
        .max()
        .unwrap_or(0);

    let chip_header = if probed { "  CHIP" } else { "" };
    println!(
        "{:port_width$}  {:9}  {:kind_width$}{chip_header}",
        "PORT", "USB", "KIND"
    );
    for entry in entries {
        let usb = match (entry.vid, entry.pid) {
            (Some(vid), Some(pid)) => format!("{vid:04x}:{pid:04x}"),
            _ => "-".to_string(),
        };
        let chip = if probed {
            format!(
                "  {}",
                entry
                    .chip
                    .as_deref()
                    .or(entry.probe_error.as_deref())
                    .unwrap_or("-")
            )
        } else {
            String::new()
        };
        println!(
            "{:port_width$}  {usb:9}  {:kind_width$}{chip}",
            entry.port, entry.kind
        );
    }
}
