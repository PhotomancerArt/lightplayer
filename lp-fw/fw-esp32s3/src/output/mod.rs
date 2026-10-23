//! Output for the ESP32-S3 app layer.
//!
//! The provider itself is chip-agnostic and comes from `fw-esp32-common`
//! unchanged; only the *driver* below it is chip-side. That driver is
//! [`rmt::Esp32S3RmtWs281xDriver`]: real RMT output on up to four channels at
//! once, backed by the portable `lp-ws281x` transmitter.
//!
//! [`rmt`] is compiled into harness builds too — the `test_loopback` harness
//! drives the same backend and the same shared driver, which is what makes its
//! verdict mean anything about the app path.
//!
//! Under the `frame-dump` feature that driver's write path also prints each
//! transmitted frame to serial ([`rmt::frame_dump`]). An LED cannot be diffed;
//! the transcript can, which is how the M4 hardware walk states its result as
//! "192 of 192 bytes" instead of "it lit up".

//!
//! With feature `lcd-ws281x` (not the default) the driver is
//! [`lcd::Esp32S3LcdWs281xDriver`] instead: up to eight lanes from LCD_CAM and
//! GDMA, with no refill deadline. Which one an image registers is decided
//! here, in [`register_ws281x_driver`], and nowhere else.

#[cfg(any(
    all(not(fw_harness), not(feature = "lcd-ws281x")),
    feature = "test_loopback"
))]
pub mod rmt;

#[cfg(any(
    all(not(fw_harness), feature = "lcd-ws281x"),
    feature = "test_lcd_backend"
))]
pub mod lcd;

#[cfg(any(not(fw_harness), feature = "test_lcd_backend"))]
pub mod ws281x_endpoints;

/// The LED-channels spike's LCD_CAM rig (never shipped).
#[cfg(any(feature = "test_lcd_ws281x", feature = "lcd_ws281x_probe"))]
pub mod lcd_spike;

#[cfg(not(fw_harness))]
pub use fw_esp32_common::output::provider::Esp32OutputProvider;

/// The peripherals the WS281x driver this image registers needs, taken from
/// the singleton in `init_board`.
#[cfg(not(fw_harness))]
pub struct OutputPeripherals {
    /// The RMT driver's under the default; under `lcd-ws281x` only a
    /// core-only split image's status light (`split_core::StatusLight`) uses it.
    pub rmt: esp_hal::peripherals::RMT<'static>,
    #[cfg(feature = "lcd-ws281x")]
    pub lcd_cam: esp_hal::peripherals::LCD_CAM<'static>,
    /// The GDMA channel the LCD bus reads frames through. Nothing else in
    /// this image uses GDMA.
    #[cfg(feature = "lcd-ws281x")]
    pub dma: esp_hal::peripherals::DMA_CH0<'static>,
}

/// Register this image's WS281x driver: the RMT's (default) or LCD_CAM's
/// (`lcd-ws281x`). A failure costs the board its output, not its boot.
#[cfg(not(fw_harness))]
pub fn register_ws281x_driver(
    system: &mut lpc_hardware::HardwareSystem,
    registry: &alloc::rc::Rc<lpc_hardware::HwRegistry>,
    peripherals: OutputPeripherals,
) {
    use alloc::boxed::Box;
    use alloc::rc::Rc;

    #[cfg(not(feature = "lcd-ws281x"))]
    {
        // 80 MHz with divider 1 gives the 12.5 ns tick `lp_ws281x::PulseCodes`
        // assumes; a failure here is a clock-tree problem.
        match esp_hal::rmt::Rmt::new(peripherals.rmt, rmt::shared_driver::RMT_CLOCK) {
            Ok(rmt) => system.add_ws281x_driver(Box::new(rmt::Esp32S3RmtWs281xDriver::new(
                Rc::clone(registry),
                rmt,
            ))),
            Err(error) => {
                esp_println::println!(
                    "[ERROR] RMT init failed ({error:?}); no LED output this boot"
                );
            }
        }
    }
    #[cfg(feature = "lcd-ws281x")]
    {
        // The RMT stays unclaimed: the engine has no use for it here.
        let _ = peripherals.rmt;
        match lcd::new_driver(Rc::clone(registry), peripherals.lcd_cam, peripherals.dma) {
            Ok(driver) => system.add_ws281x_driver(Box::new(driver)),
            Err(why) => {
                esp_println::println!("[ERROR] {why}; no LED output this boot");
            }
        }
    }
}
