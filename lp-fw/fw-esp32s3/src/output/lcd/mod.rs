//! WS281x output over the ESP32-S3's LCD_CAM in i8080 mode: up to eight
//! lanes in lockstep from one GDMA stream (feature `lcd-ws281x`, not the
//! default; the RMT driver is).
//!
//! * [`lcd_bus`] — the frame buffers, the encode and the transfer in flight.
//! * [`esp32s3_lcd_ws281x_driver`] — the `lpc-hardware` seam: endpoints,
//!   leases, open-time lane binding.
//!
//! The encoding itself is `lp_ws281x::parallel`, host-tested; the evidence
//! that a frame survives radio interrupts and flash writes is the
//! LED-channels spike's on FS3 (`docs/adr/2026-10-22-s3-parallel-ws281x-output.md`).

extern crate alloc;

mod esp32s3_lcd_ws281x_driver;
#[cfg(feature = "ws281x_frame_check")]
mod frame_check;
mod lcd_bus;
#[cfg(feature = "ws281x_pad_probe")]
mod pad_probe;

pub use esp32s3_lcd_ws281x_driver::Esp32S3LcdWs281xDriver;
pub use lcd_bus::BUS_HZ;

/// Build the driver over LCD_CAM's i8080 half and one GDMA channel. The
/// product registers it (`output::register_ws281x_driver`); the
/// `test_lcd_backend` harness drives it directly.
pub fn new_driver(
    registry: alloc::rc::Rc<lpc_hardware::HwRegistry>,
    lcd_cam: esp_hal::peripherals::LCD_CAM<'static>,
    dma: esp_hal::peripherals::DMA_CH0<'static>,
) -> Result<Esp32S3LcdWs281xDriver, alloc::string::String> {
    use esp_hal::lcd_cam::LcdCam;
    use esp_hal::lcd_cam::lcd::i8080::{Config, I8080};
    use esp_hal::time::Rate;

    let lcd_cam = LcdCam::new(lcd_cam);
    let config = Config::default().with_frequency(Rate::from_hz(BUS_HZ));
    let driver = I8080::new(lcd_cam.lcd, dma, config)
        .map(|i8080| Esp32S3LcdWs281xDriver::new(registry, i8080))
        .map_err(|error| alloc::format!("LCD_CAM init failed ({error:?})"))?;
    // The frame check takes the CAM half, GDMA channel 1 and two back pads
    // (GPIO11, the sampler's clock loop; GPIO12, its VSYNC), none of which
    // this image otherwise uses.
    #[cfg(feature = "ws281x_frame_check")]
    {
        // SAFETY: nothing else in a `ws281x_frame_check` image claims them.
        let (dma, clk, vsync) = unsafe {
            use esp_hal::peripherals::{DMA_CH1, GPIO11, GPIO12};
            (
                DMA_CH1::steal(),
                GPIO11::steal().into(),
                GPIO12::steal().into(),
            )
        };
        if let Some(check) = frame_check::FrameCheck::new(lcd_cam.cam, dma, clk, vsync) {
            driver.attach_check(check);
        }
    }
    #[cfg(not(feature = "ws281x_frame_check"))]
    let _ = lcd_cam.cam;
    Ok(driver)
}
