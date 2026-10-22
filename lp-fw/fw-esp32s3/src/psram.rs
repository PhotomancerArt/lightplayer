//! The octal PSRAM (feature `psram`, off by default): brought up once in the
//! core's boot, at a fixed MMU window, and checked before anything may use it.
//!
//! # Where, and why there
//!
//! At [`PSRAM_WINDOW`] (entries `0x80..0x100`; data `0x3C80_0000`, code
//! `0x4280_0000`, 8 MiB) on every S3 image, plain or split, through the
//! esp-hal fork's `mmu_start_entry` (its seventh diff). The window is carved
//! out of the top of the core's entries, so it is clear of the engine,
//! `lpfs` and the core's scratch (`lp_bootctl::s3_mmu`); upstream's own choice
//! does not fit on the split image at all.
//!
//! # When it comes up, and when it does not
//!
//! In the core, right after the heap is installed and before the filesystem
//! maps `lpfs` or a split image maps its engine: the core must reach PSRAM
//! whether or not the engine runs. Only on a part whose eFuse says it carries
//! **8 MB** of PSRAM (`PSRAM_CAP` 1 — ESP-IDF's S3 eFuse table — which is the
//! S3R8's octal part; a 2 MB part is quad and 16 MB is the 1.8 V octal part,
//! neither of which this window or this config is for), in octal mode, at
//! the window's whole 8 MiB. Anything else — a part without PSRAM, a driver
//! that finds none, a self-test that fails — boots exactly as an image
//! without the feature does, and [`State`] says which, in one
//! `[INIT] psram:` line.
//!
//! # The self-test
//!
//! A few milliseconds, never the whole memtest (0.5 s at 80 MHz in the
//! spike): one compare-and-swap (atomics on PSRAM are what an `Arc` in a
//! PSRAM heap relies on, and esp-alloc warns they may not work), then a
//! sampled pattern — one word in every 4 KiB, address-derived so a page
//! mapped twice or at the wrong place reads back another sample's value —
//! written, read back and restored to zero.
//!
//! # Nothing uses it yet
//!
//! This module brings the chip up and says whether it is usable
//! ([`usable`]); the allocator regions, the LED frames and the JIT come in
//! later phases of the same milestone.

use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use esp_hal::psram::{Psram, PsramConfig, PsramMode, PsramSize};
use lp_bootctl::s3_mmu::{PSRAM_DROM, PSRAM_MMU_ENTRY, PSRAM_WINDOW_LEN};

pub use lp_bootctl::s3_mmu::PSRAM_WINDOW;

/// What this boot's bring-up came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not tried yet (before `core_boot` reaches it).
    NotTried,
    /// The eFuse says this part has no 8 MB PSRAM: `PSRAM_CAP` as read.
    NotFused(u8),
    /// The driver found no octal PSRAM where the eFuse said one was.
    DriverFoundNone,
    /// The driver mapped something other than the window.
    WrongWindow,
    /// The compare-and-swap did not behave.
    CasFailed,
    /// A sampled word read back wrong.
    PatternFailed,
    /// Up, tested, and usable: the whole window.
    Up,
}

impl State {
    fn code(self) -> u8 {
        match self {
            State::NotTried => 0,
            State::NotFused(_) => 1,
            State::DriverFoundNone => 2,
            State::WrongWindow => 3,
            State::CasFailed => 4,
            State::PatternFailed => 5,
            State::Up => 6,
        }
    }

    fn of(code: u8) -> Self {
        match code {
            1 => State::NotFused(0),
            2 => State::DriverFoundNone,
            3 => State::WrongWindow,
            4 => State::CasFailed,
            5 => State::PatternFailed,
            6 => State::Up,
            _ => State::NotTried,
        }
    }

    /// The `[INIT] psram:` line's words.
    pub fn describe(self) -> &'static str {
        match self {
            State::NotTried => "not tried",
            State::NotFused(_) => "off: the eFuse says this part has no 8 MB PSRAM",
            State::DriverFoundNone => "off: the driver found no octal PSRAM",
            State::WrongWindow => "off: the driver mapped it outside the window",
            State::CasFailed => "off: the self-test's compare-and-swap failed",
            State::PatternFailed => "off: the self-test's pattern read back wrong",
            State::Up => "on",
        }
    }
}

static STATE: AtomicU8 = AtomicU8::new(0);

/// This boot's state (the heartbeat and the allocator read it).
pub fn state() -> State {
    State::of(STATE.load(Ordering::Acquire))
}

/// Whether PSRAM is up and passed its self-test this boot.
pub fn usable() -> bool {
    state() == State::Up
}

/// The window's data-bus range, when [`usable`].
pub fn window() -> Option<(usize, usize)> {
    usable().then_some((PSRAM_DROM as usize, PSRAM_WINDOW_LEN as usize))
}

/// esp-hal's eFuse field for the PSRAM's capacity, read whole: bits 131..133
/// and its high bit at 179 (`PSRAM_CAP`, `PSRAM_CAP_3`).
fn fused_capacity() -> u8 {
    use esp_hal::efuse::{PSRAM_CAP, PSRAM_CAP_3, read_bit, read_field_le};
    read_field_le::<u8>(PSRAM_CAP) | (u8::from(read_bit(PSRAM_CAP_3)) << 2)
}

/// `PSRAM_CAP`'s code for 8 MB (ESP-IDF's S3 eFuse table: `1: "8M"`).
const FUSED_8MB: u8 = 1;

static PERIPHERAL: critical_section::Mutex<
    core::cell::RefCell<Option<esp_hal::peripherals::PSRAM<'static>>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep the PSRAM peripheral for [`bring_up`]. Called from `init_board`, the
/// one place the peripheral singleton is split (BLE's `stash`'s shape).
pub fn stash(peri: esp_hal::peripherals::PSRAM<'static>) {
    critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).replace(peri));
}

/// Bring PSRAM up: the eFuse gate, the driver at the window, the self-test.
/// Prints the `[INIT] psram:` line. Called once, from `core_boot`.
pub fn bring_up() -> State {
    let state = match critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).take()) {
        Some(peri) => try_bring_up(peri),
        None => State::DriverFoundNone,
    };
    STATE.store(state.code(), Ordering::Release);
    match state {
        State::Up => esp_println::println!(
            "[INIT] psram: on, {PSRAM_WINDOW_LEN} bytes at {PSRAM_DROM:#010x} (MMU entries \
             {:#x}..{:#x}), self-test passed",
            PSRAM_WINDOW.start,
            PSRAM_WINDOW.end
        ),
        State::NotFused(cap) => {
            esp_println::println!("[INIT] psram: {} (PSRAM_CAP={cap})", state.describe())
        }
        other => esp_println::println!("[INIT] psram: {}", other.describe()),
    }
    state
}

fn try_bring_up(peri: esp_hal::peripherals::PSRAM<'static>) -> State {
    let cap = fused_capacity();
    if cap != FUSED_8MB {
        return State::NotFused(cap);
    }
    // `new_octal`: the quad path is never referenced, so none of its `#[ram]`
    // code is linked (the fork's seventh diff).
    let config = PsramConfig {
        mode: PsramMode::OctalSpi,
        size: PsramSize::Size(PSRAM_WINDOW_LEN as usize),
        mmu_start_entry: Some(PSRAM_MMU_ENTRY),
        ..PsramConfig::default()
    };
    let psram = Psram::new_octal(peri, config);
    let (ptr, len) = psram.raw_parts();
    // The mapping outlives the handle (esp-hal's `Psram` has no `Drop`), and
    // nothing else may claim the peripheral again.
    core::mem::forget(psram);
    if len == 0 {
        return State::DriverFoundNone;
    }
    if ptr as usize != PSRAM_DROM as usize || len != PSRAM_WINDOW_LEN as usize {
        return State::WrongWindow;
    }
    self_test(ptr as usize, len)
}

/// One sample per this many bytes.
const SAMPLE_STRIDE: usize = 4096;

/// See the module docs.
fn self_test(base: usize, len: usize) -> State {
    // SAFETY: the window was just mapped, 4-aligned, and nothing else holds
    // a reference into it yet.
    let cas = unsafe { &*(base as *const AtomicU32) };
    cas.store(0x5A5A_0001, Ordering::SeqCst);
    let swapped =
        cas.compare_exchange(0x5A5A_0001, 0xA5A5_0002, Ordering::SeqCst, Ordering::SeqCst);
    let refused =
        cas.compare_exchange(0x5A5A_0001, 0xDEAD_BEEF, Ordering::SeqCst, Ordering::SeqCst);
    if swapped != Ok(0x5A5A_0001)
        || refused != Err(0xA5A5_0002)
        || cas.load(Ordering::SeqCst) != 0xA5A5_0002
    {
        return State::CasFailed;
    }
    cas.store(0, Ordering::SeqCst);

    let word = |i: usize| (base + i * SAMPLE_STRIDE + 4) as *mut u32;
    let pattern = |i: usize| (i as u32).wrapping_mul(0x9E37_79B1) ^ 0x5EED_5EED;
    let samples = len / SAMPLE_STRIDE;
    for i in 0..samples {
        // SAFETY: inside the mapped window, 4-aligned.
        unsafe { core::ptr::write_volatile(word(i), pattern(i)) };
    }
    let mut ok = true;
    for i in 0..samples {
        // SAFETY: as above.
        ok &= unsafe { core::ptr::read_volatile(word(i)) } == pattern(i);
        unsafe { core::ptr::write_volatile(word(i), 0) };
    }
    if ok { State::Up } else { State::PatternFailed }
}
