//! `test_lcd_lanes`: five uneven WS281x lanes out of LCD_CAM's i8080 mode,
//! fed by GDMA from one internal-RAM frame buffer (M9, the parallel LED
//! output). Never shipped; emulator-first.
//!
//! What this image is for: `lp-emu/esp/lp-emu-esp32s3/tests/lcd_cam_lanes.rs`
//! boots it and checks that the per-pad WS281x decoders read each lane's
//! bytes back exactly — the proof that the emulator's LCD_CAM and GDMA models
//! play what esp-hal's i8080 driver asks for. On silicon it drives the same
//! pads (nothing need be attached) and prints the same lines.
//!
//! # The frame
//!
//! Lanes 0–4 on GPIO1–GPIO5 (the XIAO's D0–D4), carrying 12, 9, 7, 5 and 3
//! LEDs (36, 27, 21, 15 and 9 wire bytes), each byte a fixed function of its
//! lane and index ([`lane_byte`]), which the emulator's test computes the same
//! way. Bit `j` of every bus byte is lane `j`'s level for one 400 ns slot
//! (2.5 MHz); three slots make one WS281x bit — high, the data bit, low — so
//! T0H is 400 ns, T1H 800 ns and the period 1.2 µs. A lane that has run out of
//! bits stays low. 300 µs of zero bytes latch the frame.
//!
//! The encoder here is deliberately the plainest one: this image tests the
//! emulator, and the product's encoder (`lp_ws281x::parallel`, lane L's) is
//! tested by its own crate. Two frames go out, so the second proves a channel
//! and a peripheral that already ran once start again.
//!
//! # Lines
//!
//! `[LCD-LANES] start`, then per frame `[LCD-LANES] frame N sent: B bus
//! bytes, result ok|err`, then `[LCD-LANES] done`. The harness then idles.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;

use esp_hal::dma::{DmaDescriptor, DmaTxBuf};
use esp_hal::lcd_cam::LcdCam;
use esp_hal::lcd_cam::lcd::i8080::{Command, Config, I8080};
use esp_hal::time::Rate;

/// LEDs per lane.
pub const LANES: [usize; 5] = [12, 9, 7, 5, 3];
/// The bus clock: 2.5 MHz, a 400 ns slot.
const BUS_HZ: u32 = 2_500_000;
/// The latch: 300 µs of zero slots.
const LATCH_SLOTS: usize = 750;
/// Frames sent.
const FRAMES: u32 = 2;

/// Lane `lane`'s wire byte `k`. The emulator's test computes the same.
pub const fn lane_byte(lane: usize, k: usize) -> u8 {
    ((lane as u8 + 1).wrapping_mul(37)) ^ (k as u8).wrapping_mul(11) ^ 0x5a
}

pub fn run(p: esp_hal::peripherals::Peripherals) -> ! {
    esp_println::println!("[LCD-LANES] start");

    let max_bits = LANES.iter().max().copied().unwrap_or(0) * 24;
    let len = max_bits * 3 + LATCH_SLOTS;
    let buffer: &'static mut [u8] = Box::leak(vec![0u8; len].into_boxed_slice());
    encode(buffer);
    let descriptors: &'static mut [DmaDescriptor] =
        Box::leak(vec![DmaDescriptor::EMPTY; len / 4000 + 2].into_boxed_slice());
    let mut tx = match DmaTxBuf::new(descriptors, buffer) {
        Ok(tx) => tx,
        Err(e) => fail(format_args!("DmaTxBuf: {e:?}")),
    };
    tx.set_length(len);

    let lcd_cam = LcdCam::new(p.LCD_CAM);
    let config = Config::default().with_frequency(Rate::from_hz(BUS_HZ));
    let mut lcd = match I8080::new(lcd_cam.lcd, p.DMA_CH0, config) {
        Ok(lcd) => lcd
            .with_data0(p.GPIO1)
            .with_data1(p.GPIO2)
            .with_data2(p.GPIO3)
            .with_data3(p.GPIO4)
            .with_data4(p.GPIO5),
        Err(e) => fail(format_args!("I8080: {e:?}")),
    };

    for n in 0..FRAMES {
        let transfer = match lcd.send(Command::<u8>::None, 0, tx) {
            Ok(t) => t,
            Err((e, _, _)) => fail(format_args!("send: {e:?}")),
        };
        let (result, back, buf) = transfer.wait();
        lcd = back;
        tx = buf;
        esp_println::println!(
            "[LCD-LANES] frame {n} sent: {len} bus bytes, result {}",
            if result.is_ok() { "ok" } else { "err" }
        );
    }
    esp_println::println!("[LCD-LANES] done");
    idle()
}

/// Three slots per WS281x bit, lanes in bit order, MSB of each byte first.
fn encode(out: &mut [u8]) {
    out.fill(0);
    for (lane, &leds) in LANES.iter().enumerate() {
        let mask = 1u8 << lane;
        for k in 0..leds * 3 {
            let byte = lane_byte(lane, k);
            for b in 0..8 {
                let bit = k * 8 + b;
                out[bit * 3] |= mask;
                if byte & (0x80 >> b) != 0 {
                    out[bit * 3 + 1] |= mask;
                }
            }
        }
    }
}

fn fail(why: core::fmt::Arguments<'_>) -> ! {
    esp_println::println!("[LCD-LANES] FAIL {why}");
    idle()
}

fn idle() -> ! {
    let delay = esp_hal::delay::Delay::new();
    loop {
        delay.delay_millis(1000);
    }
}
