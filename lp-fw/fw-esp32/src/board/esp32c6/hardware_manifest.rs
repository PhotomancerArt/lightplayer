extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpc_shared::hardware::{
    HardwareAddress, HardwareCapability, HardwareManifest, HardwareResource,
};

pub fn esp32c6_devkit_hardware_manifest() -> HardwareManifest {
    HardwareManifest::new(
        "esp32c6-devkit-provisional",
        "ESP32-C6 DevKit Provisional",
        esp32c6_devkit_resources(),
    )
    .with_description(
        "Provisional ESP32-C6 dev board profile with HAL GPIO identities and hand-entered labels.",
    )
    .with_url("https://www.espressif.com/en/products/devkits")
}

fn esp32c6_devkit_resources() -> Vec<HardwareResource> {
    let mut resources = Vec::new();
    for pin in 0..=21 {
        resources.push(gpio_resource(pin));
    }
    resources.push(HardwareResource::new(
        HardwareAddress::rmt_ws281x(0),
        [HardwareCapability::Rmt, HardwareCapability::Ws281xOutput],
        "RMT WS281x 0",
    ));
    resources
}

fn gpio_resource(pin: u32) -> HardwareResource {
    let mut resource = HardwareResource::new(
        HardwareAddress::gpio(pin),
        [
            HardwareCapability::GpioOutput,
            HardwareCapability::GpioInput,
        ],
        format!("GPIO{pin}"),
    )
    .with_aliases(gpio_aliases(pin));

    resource = match pin {
        12 => resource.reserved("crashed during GPIO scan test"),
        18 => resource.with_location("known WS281x output header"),
        _ => resource,
    };
    resource
}

fn gpio_aliases(pin: u32) -> Vec<String> {
    vec![format!("IO{pin}"), format!("GPIO{pin}"), pin.to_string()]
}
