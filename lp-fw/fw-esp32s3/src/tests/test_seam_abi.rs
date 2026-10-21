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
//!
//! Then, when the take is engaged, **the wake**: the harness binds the wake
//! handler ([`crate::seams::seam_wake_handler`]) and sleeps in `waiti`; each
//! time the emulator raises `FROM_CPU_INTR3` for bytes it queued, the handler
//! swaps the pending word and wakes a waker the harness registered, and the
//! harness drains the take and prints `woke N: take …`.
//!
//! On silicon it prints `echo=7`, `engaged=false`, `net engaged=false`, and
//! idles.

use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{RawWaker, RawWakerVTable, Waker};

use fw_esp32_common::seams::net::{net_link, net_mac};
use fw_esp32_common::seams::seam_wake;

use crate::seams::{seam_wake_handler, test_echo, test_take};

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
        drain(None);
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

    if engaged {
        wait_for_wakes();
    }
    let delay = esp_hal::delay::Delay::new();
    loop {
        delay.delay_millis(1000);
    }
}

/// Drain `test_take` until it returns 0, printing each take — after which
/// wake, when a wake asked for it.
fn drain(wake: Option<u32>) {
    let mut buf = [0u8; 16];
    loop {
        let n = test_take::call(0, buf.as_mut_ptr(), buf.len() as u32) as usize;
        if n == 0 {
            break;
        }
        let got = &buf[..n.min(buf.len())];
        match wake {
            None => esp_println::println!("[SEAM-ABI] take {n} B: {}", Hex(got)),
            Some(w) => esp_println::println!("[SEAM-ABI] woke {w}: take {n} B: {}", Hex(got)),
        }
    }
}

/// Sleep in `waiti` until the wake handler wakes [`WOKEN`]'s waker, then
/// drain the take; forever. The waker is registered before each drain, so a
/// raise that lands after the drain is the next loop's wake, never lost.
fn wait_for_wakes() -> ! {
    // esp-hal's interrupt entry saves the FP registers (`float-save-restore`),
    // so a core that takes an interrupt needs coprocessor 0 armed — the app
    // path's `init` does the same before its first interrupt.
    crate::board::esp32s3::fpu::arm();
    seam_wake_handler::bind();
    esp_println::println!("[SEAM-ABI] waiting for wakes");
    let mut wakes = 0u32;
    loop {
        WOKEN.store(false, Ordering::SeqCst);
        seam_wake::NET_EVENTS.register(&flag_waker());
        while !WOKEN.load(Ordering::SeqCst) {
            // SAFETY: `waiti 0` lowers the interrupt level to 0 and idles
            // until an interrupt; the handler bound above is that interrupt.
            unsafe { core::arch::asm!("waiti 0") };
        }
        wakes += 1;
        drain(Some(wakes));
    }
}

/// Set by the waker the wake handler wakes.
static WOKEN: AtomicBool = AtomicBool::new(false);

fn flag_waker() -> Waker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(core::ptr::null(), &VTABLE),
        |_| WOKEN.store(true, Ordering::SeqCst),
        |_| WOKEN.store(true, Ordering::SeqCst),
        |_| {},
    );
    // SAFETY: every vtable entry ignores its (null) data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
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
