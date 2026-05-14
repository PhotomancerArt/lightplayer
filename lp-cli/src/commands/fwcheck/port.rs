use anyhow::{Context, Result, bail};

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

    let mut ports: Vec<String> = serialport::available_ports()
        .context("list serial ports")?
        .into_iter()
        .map(|port| port.port_name)
        .filter(|name| {
            name.contains("usbmodem")
                || name.contains("ttyUSB")
                || name.contains("ttyACM")
                || name.contains("tty.usbserial")
        })
        .collect();
    ports.sort();

    let cu_ports: Vec<String> = ports
        .iter()
        .filter(|name| name.starts_with("/dev/cu."))
        .cloned()
        .collect();
    let candidates = if cu_ports.is_empty() { ports } else { cu_ports };

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
