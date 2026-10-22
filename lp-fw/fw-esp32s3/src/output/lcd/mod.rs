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

mod esp32s3_lcd_ws281x_driver;
mod lcd_bus;

pub use esp32s3_lcd_ws281x_driver::Esp32S3LcdWs281xDriver;
pub use lcd_bus::BUS_HZ;
