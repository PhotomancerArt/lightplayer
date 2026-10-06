//! The one owner of the radio's bring-up (plan MD7).
//!
//! `esp_radio::wifi::new` may run once per boot, and it hands back the
//! controller and every interface together. Before the station existed the
//! ESP-NOW driver called it, kept the controller alive and dropped the
//! station interface. Now it is called here, once, and the parts go to
//! their owners:
//!
//! - the **ESP-NOW interface** to the ESP-NOW driver
//!   ([`super::espnow_radio_driver::Esp32EspNowRadioDriver`]), unchanged;
//! - the **controller and the station interface** to the station on the
//!   `lp-net` thread (`crate::net`, feature `wifi`), or back to the ESP-NOW
//!   driver to keep alive when the image has no station.
//!
//! Both share #890's lean driver counts ([`espnow_controller_config`]): the
//! Wi-Fi cost experiments joined a station on them with no loss (P08
//! measures RX drops again under the product's traffic).

use alloc::format;

use esp_hal::peripherals::WIFI;
use esp_radio::esp_now::EspNow;
use esp_radio::wifi::{Interface, WifiController};
use lpc_hardware::HardwareEndpointError;

use crate::hardware::espnow_controller_config::espnow_controller_config;

/// What the radio's one bring-up hands out.
pub struct RadioParts {
    /// The Wi-Fi controller: dropping it stops Wi-Fi and ESP-NOW, so its
    /// owner keeps it for the boot.
    pub controller: WifiController<'static>,
    /// The ESP-NOW interface.
    pub esp_now: EspNow<'static>,
    /// The station interface: the frame device under the IP stack.
    #[cfg_attr(
        not(feature = "wifi"),
        allow(dead_code, reason = "only the station (feature `wifi`) takes it")
    )]
    pub station: Interface<'static>,
}

/// Bring the radio up: once per boot.
pub fn bring_up(wifi: WIFI<'static>) -> Result<RadioParts, HardwareEndpointError> {
    let (controller, interfaces) =
        esp_radio::wifi::new(wifi, espnow_controller_config()).map_err(|error| {
            HardwareEndpointError::Other {
                message: format!("ESP-NOW Wi-Fi init failed: {error:?}"),
            }
        })?;
    Ok(RadioParts {
        controller,
        esp_now: interfaces.esp_now,
        station: interfaces.station,
    })
}
