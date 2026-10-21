//! `test_seam_abi`: the Xtensa seam ABI under the real release build. Never
//! ships (S3 radio M5; the C6's harness of the same name, one chip over).
//!
//! The seams here are reached the way `lp_seam::seam_fn!` reaches every
//! Xtensa seam: through a function pointer LLVM cannot see through, as an
//! ordinary windowed call. What this image proves, against
//! `lp-emu/esp/lp-emu-esp32s3/tests/seam_abi_harness.rs`, is that the
//! arguments and the result survive `release-esp32s3`'s LTO +
//! `opt-level = "z"` and that an emulator answering before `entry` returns
//! to a caller whose registers and window are intact:
//!
//! - `test_echo(1, 2, 4)`: silicon answers `7`; the emulator's `test=echo`
//!   answers something else, so the printed value says whose answer ran;
//! - `test_take`: only when its engaged byte reads 1, drained until it
//!   returns 0;
//! - `net_mac` (an out-pointer, 6 bytes back) and `net_link`, only when the
//!   network seam's engaged byte reads 1 — the network seam's own calls on
//!   this chip, before any S3 image has a network stack to make them.
//!
//! Each call is followed by a check that a local kept across it still holds
//! its value: the window the call never rotated must be the caller's own.
//! On silicon it prints `echo=7`, `engaged=false`, `net engaged=false`, and
//! idles.

use fw_esp32_common::seams::net::{net_link, net_mac};

use crate::seams::{test_echo, test_take};

pub fn run() -> ! {
    esp_println::println!("[SEAM-ABI] start");
    // A value live across every call below: if an answer left the caller in
    // the wrong window, this is what would show it.
    let canary = core::hint::black_box(0x5eed_c0de_u32);
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

    let net = net_mac::engaged();
    esp_println::println!("[SEAM-ABI] net engaged={net}");
    if net {
        let mut mac = [0u8; 6];
        let n = net_mac::call(mac.as_mut_ptr());
        esp_println::println!("[SEAM-ABI] net mac {n}: {}", Hex(&mac));
        let link = net_link::call();
        esp_println::println!("[SEAM-ABI] net link={link}");
    }
    let canary = core::hint::black_box(canary);
    esp_println::println!("[SEAM-ABI] canary={canary:#010x}");
    esp_println::println!("[SEAM-ABI] done");

    let delay = esp_hal::delay::Delay::new();
    loop {
        delay.delay_millis(1000);
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
