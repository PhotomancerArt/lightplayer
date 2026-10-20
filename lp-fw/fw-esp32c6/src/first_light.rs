//! Time to first light: power-on to the first lit LED frame, and its parts,
//! on one clock (plan `lp2025/2026-10-16-0848-tree-store-adoption`, A1,
//! decision AD9).
//!
//! Each boot stage stamps the system timer once ([`mark`]): SYSTIMER unit 0,
//! 16 MHz, which counts from the chip's reset (esp-hal keeps it enabled and
//! never resets it; `esp_hal::time::Instant` and so `embassy_time` read it).
//! Its raw count, divided here: `Instant`'s tick-to-µs scaler is set inside
//! `esp_hal::init`, and the app's entry is before that (read through
//! `Instant` there, the count came back as raw 16 MHz ticks — 16x).
//! The first lit frame — the first buffer handed to the RMT with any byte
//! that is not zero ([`on_frame`]) — also reads the **LP timer**, the
//! slow-clock counter that starts at power-on, the mask ROM and the
//! bootloader included (what `Rtc::time_since_power_up` reads) — and it read
//! it at the app's entry too. The LP timer's lead over the system timer at
//! the entry (the time before the CPU's reset was released) puts every stamp
//! on the power-up clock, so the parts sum to the total. The stamps
//! themselves stay on the system timer: it runs off the crystal, the LP
//! timer off the RC slow clock, whose calibration is good to a few tenths
//! of a percent (CX1: ~1 ms slow over half a second). One `log` line, once
//! per boot:
//!
//! ```text
//! [perf] first light at 612 ms: boot 85 init 20 fs 136 radio 90 split 2 server 15 load 134 frame1 40 lit 90 (lp-sys 990/-1100 us)
//! ```
//!
//! - `boot`: power-on to the app's entry — the supply's ramp, the mask ROM,
//!   the second-stage bootloader, the split image's loader, the runtime's
//!   own start;
//! - `init`: the board, recovery, the USB link, the partition table and the
//!   boot records, up to the filesystem;
//! - `fs`: the filesystem's mount (or format);
//! - `radio`: the hardware manifest, the status LED, ESP-NOW/Wi-Fi, BLE and
//!   the network thread — the rest of the core;
//! - `split`: choosing and checking the engine (a split image's
//!   `split_boot`);
//! - `server`: the RMT driver, the hardware system and the server;
//! - `load`: the boot project's load (parse, resolve, map; the shader is
//!   compiled a frame later);
//! - `frame1`: to the end of the server loop's first frame;
//! - `lit`: to the first lit frame — the compile window's frame, so the
//!   shader compile is here.
//!
//! A stage that did not run takes no time (its part is 0). A boot that lights
//! nothing in its first [`UNLIT_FRAMES`] frames (no project, safe mode, a
//! black pattern) says `first light none by <ms>` instead, its `lit` 0.
//!
//! **Only on a power-on boot**: the LP timer counts from power-on, and a
//! software, USB or watchdog reset does not reset it, so on those boots its
//! count is not this boot's (the C6 TRM's reset scopes: a CPU or core reset
//! leaves the LP system alone). `lp-sys` is the LP timer less the system
//! timer at the app's entry (the offset used) and again at the line: the two
//! differ by the slow clock's calibration error over the boot. An emulator
//! that models no LP timer reads it as 0, and the figures are then from
//! reset.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

/// A boot stage, in boot order.
#[derive(Clone, Copy)]
pub enum Mark {
    /// The top of `core_boot`.
    App,
    /// The filesystem's mount begins.
    Mount,
    /// The filesystem is mounted (or the boot fell back to memory).
    Mounted,
    /// The end of `core_boot`: the radios are up.
    Radio,
    /// The engine's entry (`lp_engine_entry`).
    Engine,
    /// The boot project's load begins.
    Load,
    /// The boot project's load returned.
    Loaded,
    /// The end of the server loop's first frame.
    Served,
    /// The first lit frame went out.
    Lit,
}

/// Frames to wait for a lit one before saying `first light none by`.
pub const UNLIT_FRAMES: u32 = 64;

const MARKS: usize = Mark::Lit as usize + 1;

/// Each stage's system-timer reading, µs from reset; 0 = not reached.
static AT: [AtomicU32; MARKS] = [const { AtomicU32::new(0) }; MARKS];
/// The LP timer's raw count at the app's entry (its low 32 bits).
static LP_APP: AtomicU32 = AtomicU32::new(0);
/// Frames served so far, to [`UNLIT_FRAMES`].
static FRAMES: AtomicU32 = AtomicU32::new(0);
/// The line went out (or this boot will not print one).
static DONE: AtomicBool = AtomicBool::new(false);

/// Stamp `stage`, once.
#[inline(never)]
pub fn mark(stage: Mark) {
    let slot = &AT[stage as usize];
    if slot.load(Relaxed) == 0 {
        slot.store(sys_us().max(1), Relaxed);
        if matches!(stage, Mark::App) {
            LP_APP.store(lp_ticks() as u32, Relaxed);
        }
    }
}

/// A frame went out on an RMT channel: the first lit one says the line.
#[inline(never)]
pub fn on_frame(data: &[u8]) {
    if !DONE.load(Relaxed) && data.iter().any(|&b| b != 0) {
        mark(Mark::Lit);
        report(true);
    }
}

/// The server loop finished a frame: the first stamps `frame1`, and the
/// [`UNLIT_FRAMES`]th with nothing lit says `first light none by`.
#[inline(never)]
pub fn on_served() {
    if DONE.load(Relaxed) {
        return;
    }
    mark(Mark::Served);
    let n = FRAMES.load(Relaxed) + 1;
    FRAMES.store(n, Relaxed);
    if n >= UNLIT_FRAMES {
        report(false);
    }
}

/// Whether this boot's first light is decided: a frame lit, or
/// [`UNLIT_FRAMES`] went by without one — on every boot, power-on or not.
/// The work held back until after first light waits for this (a split
/// image's core hash, `ota::core_hash_step`; the station's first join,
/// `net::station_task`).
#[cfg(any(lp_split, lp_net))]
pub fn decided() -> bool {
    DONE.load(Relaxed)
}

fn report(lit: bool) {
    DONE.store(true, Relaxed);
    if esp_hal::system::reset_reason() != Some(esp_hal::rtc_cntl::SocResetReason::ChipPowerOn) {
        return;
    }
    let sys_now = i64::from(sys_us());
    let lp_now = lp_us(lp_ticks()) as i64;
    let lp_app = lp_us(u64::from(LP_APP.load(Relaxed)));
    let app = u64::from(AT[Mark::App as usize].load(Relaxed));
    // The power-up clock's lead over the system timer at the app's entry
    // (the offset), and again now (the calibration check).
    let lead = lp_app.saturating_sub(app);
    let lead_now = lp_now - sys_now;
    // Each stage on the power-up clock, ms; a stage that did not run takes
    // the one before's, so its part is 0 and the parts still sum.
    let mut at = [0u32; MARKS];
    let mut prev = 0u32;
    for (slot, stamp) in at.iter_mut().zip(AT.iter()) {
        let us = stamp.load(Relaxed);
        if us != 0 {
            prev = ((u64::from(us) + lead) / 1000) as u32;
        }
        *slot = prev;
    }
    let d = |i: usize| at[i].saturating_sub(at[i - 1]);
    log::info!(
        "[perf] first light {} {} ms: boot {} init {} fs {} radio {} split {} server {} load {} \
         frame1 {} lit {} (lp-sys {}/{} us)",
        if lit { "at" } else { "none by" },
        at[MARKS - 1],
        at[0],
        d(1),
        d(2),
        d(3),
        d(4),
        d(5),
        d(6),
        d(7),
        d(8),
        lead,
        lead_now,
    );
}

/// The system timer, µs from reset.
fn sys_us() -> u32 {
    use esp_hal::timer::systimer::{SystemTimer, Unit};
    // 16 ticks a µs on the C6 (XTAL 40 MHz / 2.5).
    (SystemTimer::unit_value(Unit::Unit0) / 16) as u32
}

/// The LP timer's raw count (slow-clock ticks since power-on).
fn lp_ticks() -> u64 {
    let t = esp_hal::peripherals::LP_TIMER::regs();
    t.update().write(|w| w.main_timer_update().set_bit());
    let high = t.main_buf0_high().read().main_timer_buf0_high().bits();
    let low = t.main_buf0_low().read().main_timer_buf0_low().bits();
    (u64::from(high) << 32) | u64::from(low)
}

/// `ticks` of the LP timer in µs, by the slow clock's calibration (µs per
/// tick with 19 fractional bits, in `LP_AON_STORE1`, where the bootloader
/// and esp-hal keep it) — esp-hal's own conversion, split so it cannot
/// overflow.
fn lp_us(ticks: u64) -> u64 {
    const FRACT: u32 = 19;
    let period = u64::from(esp_hal::peripherals::LP_AON::regs().store1().read().bits());
    (ticks >> FRACT) * period + (((ticks & ((1 << FRACT) - 1)) * period) >> FRACT)
}
