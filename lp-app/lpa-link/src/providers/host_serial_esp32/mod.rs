mod provider;

pub use provider::{
    HostSerialEsp32Options, HostSerialEsp32Provider, descriptor, is_likely_esp32_serial_port,
    label_for_port, prefer_cu_ports,
};

#[cfg(test)]
mod tests;
