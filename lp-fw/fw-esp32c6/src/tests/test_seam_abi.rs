//! `test_seam_abi`: the seam ABI under the real release build. Never ships.
//!
//! The only seam the product image carries takes no arguments and returns
//! nothing, so on its own it cannot show that the generated call shims keep
//! their arguments and their result through `release-esp32`'s LTO +
//! `opt-level = "z"` (the LTO rule, `lp-base/lp-seam`). This image carries
//! the two test seams beside it and calls them:
//!
//! - `test_echo(1, 2, 4)`: silicon answers `7`; the emulator's test
//!   implementation (`test=echo`, feature `test-seams`) answers something
//!   else, so the printed value says whose answer ran;
//! - `test_take`: only when its engaged byte reads 1, drain it until it
//!   returns 0, printing what came back.
//!
//! The emulator test is `lp-emu/esp/lp-emu-esp32c6/tests/seam_abi_harness.rs`.
//! On silicon it prints `echo=7`, `engaged=false`, and idles.

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::seams::{test_echo, test_take};

pub async fn run_seam_abi(_spawner: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, ..) = init_board();
    start_runtime(timg0, sw_int);

    esp_println::println!("[SEAM-ABI] start");
    let echo = test_echo::call(1, 2, 4);
    esp_println::println!("[SEAM-ABI] echo={echo}");

    let engaged = test_take::engaged();
    esp_println::println!("[SEAM-ABI] take engaged={engaged}");
    if engaged {
        let mut buf = [0u8; 16];
        loop {
            let n = test_take::call(0, buf.as_mut_ptr(), buf.len() as u32) as usize;
            if n == 0 {
                break;
            }
            let got = &buf[..n.min(buf.len())];
            esp_println::println!("[SEAM-ABI] take {n} B: {}", Hex(got));
        }
    }
    esp_println::println!("[SEAM-ABI] done");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}

/// Bytes as lowercase hex pairs, space-separated.
struct Hex<'a>(&'a [u8]);

impl core::fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}
