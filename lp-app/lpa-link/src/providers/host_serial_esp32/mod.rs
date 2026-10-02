mod host_esp32_flash;
mod host_esp32_layout;
mod lp_analog_i2c;
mod provider;

pub use host_esp32_flash::read_raw_filesystem;
pub use host_esp32_layout::{LayoutSessionOutcome, layout_session};
pub use provider::{
    HostSerialEsp32Options, HostSerialEsp32Provider, descriptor, is_likely_esp32_serial_port,
    label_for_port, prefer_cu_ports,
};

#[cfg(test)]
mod tests;
