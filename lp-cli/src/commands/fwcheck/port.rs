use anyhow::{Context, Result, bail};
use lpa_link::providers::host_serial_esp32::{is_likely_esp32_serial_port, prefer_cu_ports};

use crate::client::serial_port::list_host_serial_esp32_ports;

/// Resolve the ESP32 serial port for fwcheck.
///
/// Precedence: explicit `--port` override, then the `ESPFLASH_PORT`
/// environment variable, then lpa-link port discovery filtered by the shared
/// ESP32 heuristic (with the macOS `/dev/cu.*` preference applied). fwcheck
/// is non-interactive by design, so multiple candidates bail with the list
/// instead of prompting.
pub fn resolve_esp32_port(override_port: Option<&str>) -> Result<String> {
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
        ports => bail!(
            "multiple ESP32 serial ports found; pass --port:\n{}",
            ports
                .iter()
                .map(|port| format!("  - {port}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}
