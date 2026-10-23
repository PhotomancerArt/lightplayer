//! `test_lcd_backend`: five uneven WS281x lanes through the **product**
//! LCD_CAM backend (`output::lcd::Esp32S3LcdWs281xDriver`, feature
//! `lcd-ws281x`) — the registry, the board profile's endpoints, `open`, and
//! `Ws281xOutput::start`/`wait_complete` — not a harness of its own. Never
//! shipped.
//!
//! `lp-emu/esp/lp-emu-esp32s3/tests/lcd_backend_lanes.rs` boots it and checks
//! every lane decodes byte-exact off its pad (`just test-emu-esp32s3-lcd`).
//! On silicon it drives the same pads and prints the same lines.
//!
//! # The frame
//!
//! The XIAO S3 Plus's D0–D4 (GPIO1–5) carry 12, 9, 7, 5 and 3 LEDs. Each
//! lane's **wire** byte `k` is [`lane_byte`] (the emulator test's own
//! arithmetic); the driver takes RGB and sends GRB, so the RGB handed to
//! `start` is the wire order with each pixel's first two bytes swapped back.
//! Two frames: all five lanes `start`, then all five `wait_complete`, which is
//! the provider's order.
//!
//! # Lines
//!
//! `[LCD-BACKEND] open D<n> lane bytes=B`, per frame `[LCD-BACKEND] frame N
//! ok|err <why>`, then `[LCD-BACKEND] done`. The harness then idles.

extern crate alloc;

use alloc::rc::Rc;
use alloc::vec::Vec;

use lpc_hardware::{HwRegistry, Ws281xConfig, Ws281xDriver, Ws281xOutput};

/// LEDs per lane, on the board labels D0..D4.
pub const LANES: [usize; 5] = [12, 9, 7, 5, 3];
const LABELS: [&str; 5] = ["D0", "D1", "D2", "D3", "D4"];
const FRAMES: u32 = 2;

/// Lane `lane`'s wire byte `k`. The emulator's test computes the same.
pub const fn lane_byte(lane: usize, k: usize) -> u8 {
    ((lane as u8 + 1).wrapping_mul(29)) ^ (k as u8).wrapping_mul(13) ^ 0xa5
}

pub fn run(p: esp_hal::peripherals::Peripherals) -> ! {
    esp_println::println!("[LCD-BACKEND] start");
    let registry = Rc::new(HwRegistry::new(
        lpc_hardware::default_esp32s3_hardware_manifest(),
    ));
    let driver = match crate::output::lcd::new_driver(Rc::clone(&registry), p.LCD_CAM, p.DMA_CH0) {
        Ok(driver) => driver,
        Err(why) => fail(format_args!("driver: {why}")),
    };

    let endpoints = driver.endpoints();
    let mut outputs: Vec<alloc::boxed::Box<dyn Ws281xOutput>> = Vec::new();
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for (lane, label) in LABELS.iter().enumerate() {
        let spec = alloc::format!("ws281x:local:{label}");
        let Some(endpoint) = endpoints.iter().find(|e| e.spec().as_str() == spec) else {
            fail(format_args!("no endpoint {spec}"))
        };
        let bytes = LANES[lane] * 3;
        match driver.open(endpoint.id(), Ws281xConfig::new(bytes as u32)) {
            Ok(output) => outputs.push(output),
            Err(why) => fail(format_args!("open {label}: {why}")),
        }
        esp_println::println!("[LCD-BACKEND] open {label} bytes={bytes}");
        // RGB in, GRB on the wire: swap each pixel's first two wire bytes back.
        let mut rgb: Vec<u8> = (0..bytes).map(|k| lane_byte(lane, k)).collect();
        for pixel in rgb.chunks_exact_mut(3) {
            pixel.swap(0, 1);
        }
        frames.push(rgb);
    }

    for n in 0..FRAMES {
        let mut result = Ok(());
        for (output, frame) in outputs.iter_mut().zip(frames.iter()) {
            // SAFETY: `frames` outlives the `wait_complete` loop below.
            if let Err(why) = unsafe { output.start(frame) } {
                result = Err(why);
            }
        }
        for output in outputs.iter_mut() {
            if let Err(why) = output.wait_complete() {
                result = Err(why);
            }
        }
        match result {
            Ok(()) => esp_println::println!("[LCD-BACKEND] frame {n} ok"),
            Err(why) => esp_println::println!("[LCD-BACKEND] frame {n} err {why}"),
        }
    }
    esp_println::println!("[LCD-BACKEND] done");
    idle()
}

fn fail(why: core::fmt::Arguments<'_>) -> ! {
    esp_println::println!("[LCD-BACKEND] FAIL {why}");
    idle()
}

fn idle() -> ! {
    let delay = esp_hal::delay::Delay::new();
    loop {
        delay.delay_millis(1000);
    }
}
