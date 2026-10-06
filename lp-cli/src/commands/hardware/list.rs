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
//!
//! When the desk's board bench (`board`, see `client::board_bench`) is
//! installed, each board also shows its mark, slug, role and who holds it,
//! a probe skips boards someone else holds, and a probed chip that
//! disagrees with the bench's registry is called out.

use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;
use serialport::SerialPortType;

use super::args::ListArgs;
use crate::client::board_bench::{self, BenchBoard, normalize_mac};
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
    /// The board's MAC, when its USB serial number is one (Espressif native USB).
    #[serde(skip_serializing_if = "Option::is_none")]
    mac: Option<String>,
    /// What the desk's board bench knows about this board.
    #[serde(skip_serializing_if = "Option::is_none")]
    bench: Option<BenchEntry>,
}

#[derive(Debug, Serialize)]
struct BenchEntry {
    mark: Option<String>,
    slug: Option<String>,
    role: Option<String>,
    chip: Option<String>,
    holder: Option<String>,
    purpose: Option<String>,
    expires: Option<String>,
}

pub fn handle_list(args: ListArgs) -> Result<()> {
    let probe = args.probe || args.chip.is_some();
    let mut entries = enumerate_ports(args.all)?;
    let bench = board_bench::list();
    if let Some(boards) = &bench {
        for entry in &mut entries {
            entry.bench = boards
                .iter()
                .find(|board| board.port.as_deref() == Some(entry.port.as_str()))
                .map(bench_entry);
        }
    }

    if probe {
        let timeout = Duration::from_secs(args.probe_timeout_secs);
        for entry in &mut entries {
            // A probe resets the board: never one someone else holds.
            if let Err(held) = board_bench::check(&entry.port, None, None) {
                entry.probe_error = Some(format!("not probed: {held}"));
                continue;
            }
            match probe_esp32_chip(&entry.port, timeout) {
                ProbeOutcome::Chip(chip) => entry.chip = Some(chip),
                outcome => entry.probe_error = Some(outcome.describe()),
            }
        }
        for entry in &entries {
            warn_on_chip_mismatch(entry);
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
    print_table(&entries, probe, bench.is_some());
    Ok(())
}

fn bench_entry(board: &BenchBoard) -> BenchEntry {
    BenchEntry {
        mark: board.mark.clone(),
        slug: board.slug.clone(),
        role: board.role.clone(),
        chip: board.chip.clone(),
        holder: board.lease.as_ref().map(|lease| lease.holder.clone()),
        purpose: board.lease.as_ref().map(|lease| lease.purpose.clone()),
        expires: board.lease.as_ref().map(|lease| lease.expires.clone()),
    }
}

/// The registry says one chip and the board answering says another: the
/// 2026-10-05 "the C6 on port 1 was the S3" mistake, caught at the source.
fn warn_on_chip_mismatch(entry: &PortEntry) {
    let (Some(probed), Some(bench)) = (&entry.chip, &entry.bench) else {
        return;
    };
    let Some(listed) = &bench.chip else {
        return;
    };
    if normalize_chip(listed) != normalize_chip(probed) {
        eprintln!(
            "⚠️ MISMATCH: {} {} on {} is registered as {listed}, but the board answering says {probed}",
            bench.mark.as_deref().unwrap_or("?"),
            bench.slug.as_deref().unwrap_or("?"),
            entry.port
        );
    }
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
                mac: usb
                    .as_ref()
                    .and_then(|info| info.serial_number.as_deref())
                    .and_then(normalize_mac),
                kind: usb
                    .as_ref()
                    .map(|info| describe_usb_device(info.vid).to_string())
                    .unwrap_or_else(|| "non-USB serial".to_string()),
                chip: None,
                probe_error: None,
                bench: None,
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

fn print_table(entries: &[PortEntry], probed: bool, with_bench: bool) {
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            let mut row = vec![
                entry.port.clone(),
                match (entry.vid, entry.pid) {
                    (Some(vid), Some(pid)) => format!("{vid:04x}:{pid:04x}"),
                    _ => "-".to_string(),
                },
                entry.kind.clone(),
                entry.mac.clone().unwrap_or_else(|| "-".to_string()),
            ];
            if with_bench {
                let bench = entry.bench.as_ref();
                let field = |pick: fn(&BenchEntry) -> Option<&String>| {
                    bench
                        .and_then(pick)
                        .cloned()
                        .unwrap_or_else(|| "-".to_string())
                };
                row.push(field(|bench| bench.mark.as_ref()));
                row.push(field(|bench| bench.slug.as_ref()));
                row.push(field(|bench| bench.role.as_ref()));
                row.push(match bench {
                    Some(BenchEntry {
                        holder: Some(holder),
                        purpose,
                        ..
                    }) => match purpose.as_deref() {
                        Some(purpose) if !purpose.is_empty() => format!("{holder}: {purpose}"),
                        _ => holder.clone(),
                    },
                    Some(_) => "free".to_string(),
                    None => "-".to_string(),
                });
            }
            if probed {
                row.push(
                    entry
                        .chip
                        .as_deref()
                        .or(entry.probe_error.as_deref())
                        .unwrap_or("-")
                        .to_string(),
                );
            }
            row
        })
        .collect();

    let mut header = vec!["PORT", "USB", "KIND", "MAC"];
    if with_bench {
        header.extend(["MARK", "SLUG", "ROLE", "HOLDER"]);
    }
    if probed {
        header.push("CHIP");
    }
    let widths: Vec<usize> = header
        .iter()
        .enumerate()
        .map(|(column, title)| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain([title.len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<String>| {
        let last = cells.len() - 1;
        cells
            .into_iter()
            .enumerate()
            .map(|(column, cell)| {
                if column == last {
                    cell
                } else {
                    format!("{cell:width$}", width = widths[column])
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    println!(
        "{}",
        line(header.iter().map(|title| title.to_string()).collect())
    );
    for row in rows {
        println!("{}", line(row));
    }
}
