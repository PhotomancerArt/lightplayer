use std::time::Duration;

use anyhow::{Context, Result, bail};
use lpa_link::providers::host_serial_esp32::{is_likely_esp32_serial_port, prefer_cu_ports};

use crate::client::esp32_probe::{ProbeOutcome, probe_esp32_chip};
use crate::client::serial_port::list_host_serial_esp32_ports;

/// Per-port deadline for the disambiguation probe. Enforced in-process; a
/// wedged port costs one timeout instead of hanging the resolution.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve the ESP32 serial port for fwcheck and the just recipes.
///
/// Precedence: explicit `--port` override, then the `ESPFLASH_PORT`
/// environment variable, then lpa-link port discovery filtered by the shared
/// ESP32 heuristic (with the macOS `/dev/cu.*` preference applied). fwcheck
/// is non-interactive by design, so ambiguity bails with the list instead of
/// prompting — first-match auto-picking has flashed the wrong board before.
///
/// `chip` (from `--chip` or the `LP_CHIP` environment variable) breaks the
/// multi-board tie by actively probing each candidate and keeping the ones
/// that identify as that chip. A single candidate is returned WITHOUT
/// probing: the probe reboots the board and costs seconds, and every flash
/// path here re-verifies chip identity at connect time anyway (espflash
/// `ChipMismatch`).
pub fn resolve_esp32_port(override_port: Option<&str>, chip: Option<&str>) -> Result<String> {
    if let Some(port) = override_port {
        if port != "auto" && !port.is_empty() {
            return Ok(port.to_owned());
        }
    }
    if let Ok(port) = std::env::var("ESPFLASH_PORT") {
        if !port.is_empty() {
            return Ok(port);
        }
    }
    let chip_env = std::env::var("LP_CHIP").ok().filter(|env| !env.is_empty());
    let chip = chip.or(chip_env.as_deref());

    let ports = list_host_serial_esp32_ports().context("list serial ports")?;
    let candidates = prefer_cu_ports(
        ports
            .into_iter()
            .filter(|name| is_likely_esp32_serial_port(name))
            .collect(),
    );

    match candidates.as_slice() {
        [] => bail!("no ESP32 serial port found; set --port or ESPFLASH_PORT"),
        [port] => Ok(port.clone()),
        ports => match chip {
            Some(chip) => resolve_by_probe(ports, chip),
            None => bail!(
                "multiple ESP32 serial ports found; pass --port, or --chip <chip> \
                 (or LP_CHIP=<chip>) to select by probing:\n{}",
                format_port_list(ports)
            ),
        },
    }
}

/// Probe every candidate and keep the ones that identify as `chip`.
fn resolve_by_probe(ports: &[String], chip: &str) -> Result<String> {
    let outcomes: Vec<(String, ProbeOutcome)> = ports
        .iter()
        .map(|port| {
            eprintln!("probing {port} ...");
            (port.clone(), probe_esp32_chip(port, PROBE_TIMEOUT))
        })
        .collect();

    let matches: Vec<&String> = outcomes
        .iter()
        .filter(|(_, outcome)| outcome.matches_chip(chip))
        .map(|(port, _)| port)
        .collect();

    match matches.as_slice() {
        [port] => Ok((*port).clone()),
        [] => bail!(
            "no attached board probed as `{chip}`:\n{}",
            format_probe_outcomes(&outcomes)
        ),
        _ => bail!(
            "several boards probed as `{chip}`; pass --port:\n{}",
            format_probe_outcomes(&outcomes)
        ),
    }
}

fn format_port_list(ports: &[String]) -> String {
    ports
        .iter()
        .map(|port| format!("  - {port}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_probe_outcomes(outcomes: &[(String, ProbeOutcome)]) -> String {
    outcomes
        .iter()
        .map(|(port, outcome)| format!("  - {port}: {}", outcome.describe()))
        .collect::<Vec<_>>()
        .join("\n")
}
