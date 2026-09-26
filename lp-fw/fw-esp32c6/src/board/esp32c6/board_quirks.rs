//! Applies the compiled-in board quirks for the manifest in effect.
//!
//! The table is `lpc_hardware::board_quirks_for`: pure data, host-tested
//! there. This is the half that touches pins. It runs once the hardware
//! manifest is loaded and before any radio init, because the one quirk today
//! is the XIAO ESP32-C6's RF switch
//! (docs/defects/2026-09-23-xiao-c6-rf-switch-never-powered.md).

use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use lpc_hardware::{GpioHold, HwGateLevel, board_quirks_for};

/// Proof that [`apply_board_quirks`] has run. Radio bring-up that must follow
/// it (BLE: `ble::start`) takes one by value, so the ordering is a type
/// error to get wrong, not a comment to forget.
#[must_use = "radio bring-up needs this token"]
pub struct BoardQuirksApplied(());

/// Drive and hold every pin the board `board_id` needs, logging one line per
/// quirk applied. A board with no quirk is left alone.
pub fn apply_board_quirks(board_id: &str) -> BoardQuirksApplied {
    for quirk in board_quirks_for(board_id) {
        for hold in quirk.gpio_holds {
            let level = match hold.level {
                HwGateLevel::Low => Level::Low,
                HwGateLevel::High => Level::High,
            };
            // SAFETY: the quirk table names pins a board reserves for its own
            // circuitry, which no driver in this image opens: the button and
            // WS281x drivers only offer board-labelled header pins, and the
            // XIAO's GPIO3/GPIO14 carry no board label. Stealing by number is
            // the pattern those drivers use for their own pins.
            let pin = unsafe { AnyPin::steal(hold.gpio) };
            // Held for the life of the program, so the driver is never
            // dropped.
            core::mem::forget(Output::new(pin, level, OutputConfig::default()));
        }
        log::info!(
            "[fw-esp32c6] Board quirk applied: {} ({})",
            quirk.name,
            GpioHoldsDisplay(quirk.gpio_holds)
        );
    }
    BoardQuirksApplied(())
}

/// `GPIO3=LOW GPIO14=LOW`, for the boot log.
struct GpioHoldsDisplay(&'static [GpioHold]);

impl core::fmt::Display for GpioHoldsDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, hold) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            let level = match hold.level {
                HwGateLevel::Low => "LOW",
                HwGateLevel::High => "HIGH",
            };
            write!(f, "GPIO{}={}", hold.gpio, level)?;
        }
        Ok(())
    }
}
