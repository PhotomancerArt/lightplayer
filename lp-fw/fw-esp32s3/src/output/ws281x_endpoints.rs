//! The endpoint vocabulary every S3 WS281x driver shares: which GPIOs a
//! project is offered, how a GPIO address becomes a pin number, and the
//! frame-size rules. The RMT driver and the LCD_CAM driver offer the same
//! endpoints (`ws281x:local:<board label>`) over different hardware, so a
//! project authored against one opens on the other.

extern crate alloc;

use alloc::format;
use alloc::string::ToString;

use lpc_hardware::{HardwareEndpointError, HwAddress, HwEndpointSpec, OutputError};

pub fn validate_byte_count(byte_count: u32) -> Result<(), HardwareEndpointError> {
    if byte_count < 3 {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: "WS281x byte_count must be at least 3".into(),
        });
    }
    Ok(())
}

pub fn byte_len_for_byte_count(byte_count: u32) -> usize {
    ((byte_count / 3) as usize) * 3
}

pub fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

pub fn gpio_number(address: &HwAddress) -> Result<u8, HardwareEndpointError> {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("WS281x endpoint address is not a GPIO: {address}"),
        });
    };
    let gpio = raw
        .parse::<u8>()
        .map_err(|_| HardwareEndpointError::UnsupportedConfig {
            reason: format!("invalid ESP32-S3 GPIO address: {address}"),
        })?;
    if !gpio_exists(gpio) {
        return Err(HardwareEndpointError::UnsupportedConfig {
            reason: format!("ESP32-S3 has no GPIO {gpio}"),
        });
    }
    Ok(gpio)
}

/// Does this chip have GPIO `pin` at all?
///
/// The ESP32-S3 numbers **GPIO0-21 and GPIO26-48** — there is no GPIO22-25 and
/// no GPIO49+. This is not a policy filter (reserving pins is the manifest's
/// job); it is the physical pin list, and it is checked because
/// [`AnyPin::steal`] *panics* on a number the chip does not have. A
/// hand-written `/hardware.json` naming `/gpio/22` must fail the open, not the
/// boot.
pub fn gpio_exists(pin: u8) -> bool {
    pin <= 21 || (26..=48).contains(&pin)
}

/// A resource whose display label is still `GPIO<n>` has no board label, and a
/// pin the board does not name is not one a project should be offered.
pub fn has_board_assigned_label(address: &HwAddress, display_label: &str) -> bool {
    let Some(raw) = address.as_str().strip_prefix("/gpio/") else {
        return false;
    };
    !display_label.eq_ignore_ascii_case(&format!("GPIO{raw}"))
}

pub fn ws281x_local_spec(config: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(format!("ws281x:local:{config}"))
        .expect("manifest display label should form a valid endpoint spec")
}
