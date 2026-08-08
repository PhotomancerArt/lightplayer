//! Classic-ESP32 half of the switched-power-rail mechanism.
//!
//! The state machine — assert, settle, transmit; debounce, drain, deassert —
//! is chip-agnostic and lives in
//! [`fw_esp32_common::output::power_gate`]. Everything here is the two things
//! that cannot be: a real GPIO to drive, and a real monotonic clock.
//!
//! # The data line needs no parking here
//!
//! The deassert sequence requires the strand's data pin to be low before the
//! supply drops. On this chip it already is, from two directions: every wire's
//! pad is parked plain-GPIO solid low at open ([`super::rmt::v3_rmt::park_gpio`]),
//! and every RMT TX channel is configured `idle_output_level = Low`, so a pad
//! carrying the RMT signal rests low between frames. There is nothing for the
//! gate to do about it — but it is a hardware-safety invariant, not a detail,
//! so it is written down where the deassert is wired rather than left implied.

use alloc::boxed::Box;
use alloc::vec::Vec;

use esp_hal::gpio::{AnyPin, DriveMode, Level, Output, OutputConfig};
use fw_esp32_common::output::power_gate::{PowerGateController, PowerGatePin};
use lpc_hardware::{HwGateLevel, HwPowerGate};

/// One gate pin held as a plain esp-hal output.
struct Esp32V3PowerGatePin {
    output: Output<'static>,
}

impl PowerGatePin for Esp32V3PowerGatePin {
    fn set_level(&mut self, high: bool) {
        self.output
            .set_level(if high { Level::High } else { Level::Low });
    }
}

/// Build the controller for a board's declared gates, or `None` when it
/// declares none (every board but the dig2go today) or none could be driven.
///
/// The pins come up **inactive** and stay there until the provider sees a lit
/// frame: `Output::new` takes the inactive level as its initial level, and
/// `PowerGateController::new` drives it again, so the pad is never briefly
/// active between the two. That matters beyond the LEDs — the dig2go's gate is
/// GPIO12 = MTDI, the flash-voltage strap, where high at boot selects 1.8 V
/// VDD_SDIO and the board does not come up at all.
pub fn controller_for(gates: &[HwPowerGate]) -> Option<PowerGateController> {
    let mut built: Vec<(HwPowerGate, Box<dyn PowerGatePin>)> = Vec::new();
    for descriptor in gates {
        let Some(gpio) = gpio_number(descriptor) else {
            continue;
        };
        let inactive = match descriptor.active_level() {
            HwGateLevel::High => Level::Low,
            HwGateLevel::Low => Level::High,
        };
        let config = OutputConfig::default().with_drive_mode(if descriptor.open_drain() {
            DriveMode::OpenDrain
        } else {
            DriveMode::PushPull
        });
        // SAFETY: `init_board` hands out no GPIO tokens, so pads are recreated
        // by number — the same pattern, and the same justification, as the RMT
        // driver's pad handling in `v3_rmt`. Exclusivity here comes from the
        // manifest rather than a lease: a gate GPIO is board metadata, and the
        // profile reserves that resource so no driver can claim it as a wire.
        let pin = unsafe { AnyPin::steal(gpio) };
        built.push((
            descriptor.clone(),
            Box::new(Esp32V3PowerGatePin {
                output: Output::new(pin, inactive, config),
            }),
        ));
        esp_println::println!(
            "[INIT] power gate on GPIO{gpio}: {:?}-active, settle {} ms, off after {} ms black",
            descriptor.active_level(),
            descriptor.settle_ms(),
            descriptor.off_debounce_ms(),
        );
    }

    if built.is_empty() {
        return None;
    }
    Some(PowerGateController::new(Box::new(now_us), built))
}

/// Monotonic µs since boot — the same `Instant` the wire pusher measures its
/// queue waits with, so the settle window and the transmission telemetry read
/// one clock.
fn now_us() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_micros()
}

/// `/gpio/N` → N, refusing anything else loudly: a gate is useless if its
/// address is wrong, and silently skipping it presents on the bench as a dead
/// board.
fn gpio_number(descriptor: &HwPowerGate) -> Option<u8> {
    let address = descriptor.gpio();
    let parsed = address
        .as_str()
        .strip_prefix("/gpio/")
        .and_then(|raw| raw.parse::<u8>().ok());
    if parsed.is_none() {
        esp_println::println!("[ERROR] power gate address is not a GPIO: {address}; rail ungated");
    }
    parsed
}
