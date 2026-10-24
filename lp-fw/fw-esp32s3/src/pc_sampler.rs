//! Core 1 as a PC sampler for core 0 (feature `pc-sampler`, off by default):
//! the measurement the IRAM hot set is chosen from (`hot_iram/README.md`).
//!
//! Core 1 (the APP CPU) is started once, at boot, into [`sampler_loop`]. It
//! reads core 0's program counter from the debug assistant's pdebug record
//! register (`ASSIST_DEBUG` `CORE_0_RCD_PDEBUGPC`, ESP32-S3 TRM, Debug
//! Assistant chapter) at random gaps, so a sample costs core 0 nothing and is
//! not biased by core 0 masking interrupts (no interrupt is taken on core 0,
//! unlike a timer-interrupt profiler, which can never sample a masked
//! section). The loop runs from IRAM and touches only internal RAM and that
//! register: it fetches nothing through the cache and never uses the MSPI
//! bus, so it neither slows core 0's flash fetches nor is affected by a
//! flash write. The fps spike measured it: 2,048 LEDs at 11.1 fps with the
//! sampler and without (2026-10-23).
//!
//! A round of [`N`] samples fills once the project has run a few heartbeats
//! (`LP_PCS_START`, default 3), and the heartbeat after it dumps the round as
//! `[pcs] <round>.<chunk> <pc>…` lines and re-arms, `LP_PCS_ROUNDS` times
//! (default 4). The log ring drops lines when the link is slower than the
//! dump, by whole lines, so the survivors are an unbiased subsample.
//! `scripts/s3-hot-iram.py fold` reads them with the ELF that printed them.
//!
//! esp-storage refuses a flash write while another core runs, so this image's
//! `FlashStorage` auto-parks core 1 for a write (`main.rs`); the sampler holds
//! no lock, so a park can stop it anywhere. A split image's raw `ll::` flash
//! writes (an update) do not park it, and need not: nothing the sampler runs
//! or reads is in flash.

use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering};

use esp_hal::system::{CpuControl, Stack};

/// Samples a round.
pub const N: usize = 2048;
/// Core 1's stack: the sampler's frames only, never a shader's.
pub const STACK_BYTES: usize = 4096;
/// What this feature takes of `dram_seg`'s `.bss`, given back by the arena.
pub const BSS_BYTES: usize = STACK_BYTES + N * 4;

static mut APP_CORE_STACK: Stack<STACK_BYTES> = Stack::new();
static SAMPLES: [AtomicU32; N] = [const { AtomicU32::new(0) }; N];
/// 0 idle, 1 recording, 2 full (the heartbeat dumps, then re-arms).
static STATE: AtomicU32 = AtomicU32::new(0);
static ROUNDS: AtomicU32 = AtomicU32::new(0);
static BEATS: AtomicU32 = AtomicU32::new(0);
static ALIVE: AtomicU32 = AtomicU32::new(0);
static PROBE: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

const ASSIST_DEBUG: usize = 0x600C_E000;
const RCD_PDEBUGENABLE: usize = ASSIST_DEBUG + 0x48;
const RCD_RECORDING: usize = ASSIST_DEBUG + 0x4C;
const RCD_PDEBUGPC: usize = ASSIST_DEBUG + 0x5C;
const SYSTEM_CPU_PERI_CLK_EN: usize = 0x600C_0008;
const SYSTEM_CPU_PERI_RST_EN: usize = 0x600C_000C;
/// The debug assistant's bit in both SYSTEM registers above.
const ASSIST_DEBUG_BIT: u32 = 1 << 6;

static PERIPHERAL: critical_section::Mutex<
    core::cell::RefCell<Option<esp_hal::peripherals::CPU_CTRL<'static>>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep `CPU_CTRL` until [`start`]. From `init_board`.
pub fn stash(peripheral: esp_hal::peripherals::CPU_CTRL<'static>) {
    critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).replace(peripheral));
}

/// Start core 1 into the sampler. Once, from `core_boot`.
pub fn start() {
    let Some(peripheral) = critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).take())
    else {
        return;
    };
    let mut cpu_control = CpuControl::new(peripheral);
    // SAFETY: the one reference to the stack, taken once.
    let stack = unsafe { &mut *addr_of_mut!(APP_CORE_STACK) };
    match cpu_control.start_app_core(stack, || sampler_loop()) {
        // The guard parks core 1 on drop; the sampler runs for the
        // firmware's life.
        Ok(guard) => core::mem::forget(guard),
        Err(e) => {
            esp_println::println!("[pcs] core 1 did not start: {e:?}");
            return;
        }
    }
    let mut spins = 0u32;
    while ALIVE.load(Ordering::Acquire) == 0 && spins < 10_000_000 {
        spins += 1;
        core::hint::spin_loop();
    }
    if ALIVE.load(Ordering::Acquire) == 0 {
        esp_println::println!("[pcs] core 1 did not reach the sampler");
        return;
    }
    esp_println::println!(
        "[pcs] sampler on core 1: {N} samples a round, mean gap {} cycles",
        mean_gap_cycles()
    );
}

fn rounds_wanted() -> u32 {
    option_env!("LP_PCS_ROUNDS").and_then(|s| s.parse().ok()).unwrap_or(4)
}

fn start_beat() -> u32 {
    option_env!("LP_PCS_START").and_then(|s| s.parse().ok()).unwrap_or(3)
}

/// Mean gap between samples, in core-1 cycles at 240 MHz: ~250 µs, so a round
/// spans about half a second.
fn mean_gap_cycles() -> u32 {
    option_env!("LP_PCS_GAP").and_then(|s| s.parse().ok()).unwrap_or(60_000)
}

#[inline(always)]
fn rd(a: usize) -> u32 {
    // SAFETY: a peripheral register the TRM documents as readable.
    unsafe { (a as *const u32).read_volatile() }
}

#[inline(always)]
fn wr(a: usize, v: u32) {
    // SAFETY: a peripheral register the TRM documents as writable, written
    // only by this core.
    unsafe { (a as *mut u32).write_volatile(v) }
}

#[inline(always)]
fn ccount() -> u32 {
    let c: u32;
    // SAFETY: reads this core's cycle counter, nothing else.
    unsafe { core::arch::asm!("rsr.ccount {0}", out(reg) c) };
    c
}

/// Core 1's whole life. From IRAM; reads one register and writes internal
/// RAM only.
#[esp_hal::ram]
fn sampler_loop() -> ! {
    // The debug assistant's clock on, out of reset, core 0's pdebug interface
    // open and recording.
    wr(SYSTEM_CPU_PERI_CLK_EN, rd(SYSTEM_CPU_PERI_CLK_EN) | ASSIST_DEBUG_BIT);
    wr(SYSTEM_CPU_PERI_RST_EN, rd(SYSTEM_CPU_PERI_RST_EN) & !ASSIST_DEBUG_BIT);
    wr(RCD_PDEBUGENABLE, 1);
    wr(RCD_RECORDING, 1);
    // A liveness probe the first dump prints: eight reads ~1 µs apart.
    for p in PROBE.iter() {
        let t = ccount();
        while ccount().wrapping_sub(t) < 240 {}
        p.store(rd(RCD_PDEBUGPC), Ordering::Relaxed);
    }
    ALIVE.store(1, Ordering::Release);
    let mut x: u32 = 0x9E37_79B9 ^ ccount();
    let mean = mean_gap_cycles();
    loop {
        if STATE.load(Ordering::Acquire) != 1 {
            continue;
        }
        for slot in SAMPLES.iter() {
            // xorshift32: a gap uniform in [mean/2, 3·mean/2), so the samples
            // never lock to a frame-periodic pattern.
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let gap = mean / 2 + x % mean;
            let t = ccount();
            while ccount().wrapping_sub(t) < gap {}
            slot.store(rd(RCD_PDEBUGPC), Ordering::Relaxed);
        }
        STATE.store(2, Ordering::Release);
    }
}

/// Once a heartbeat, on core 0: arm a round once the project is steady, dump
/// a full one.
pub fn on_heartbeat() {
    if ALIVE.load(Ordering::Acquire) == 0 {
        return;
    }
    let beat = BEATS.fetch_add(1, Ordering::Relaxed);
    if beat == 1 {
        let mut line = Line::new();
        for p in PROBE.iter() {
            line.push_hex(p.load(Ordering::Relaxed));
        }
        log::info!("[pcs] probe {}", line.as_str());
    }
    match STATE.load(Ordering::Acquire) {
        0 if beat >= start_beat() && ROUNDS.load(Ordering::Relaxed) < rounds_wanted() => {
            log::info!("[pcs] armed round {}", ROUNDS.load(Ordering::Relaxed));
            STATE.store(1, Ordering::Release);
        }
        2 => {
            let round = ROUNDS.fetch_add(1, Ordering::Relaxed);
            for (i, chunk) in SAMPLES.chunks(16).enumerate() {
                let mut line = Line::new();
                for s in chunk {
                    line.push_hex(s.load(Ordering::Relaxed));
                }
                log::info!("[pcs] {round}.{i} {}", line.as_str());
            }
            log::info!("[pcs] round {round} done");
            STATE.store(0, Ordering::Release);
        }
        _ => {}
    }
}

/// One `[pcs]` line's words, on the stack.
struct Line {
    buf: [u8; 160],
    len: usize,
}

impl Line {
    fn new() -> Self {
        Self {
            buf: [0; 160],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    fn push_hex(&mut self, v: u32) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        if self.len + 9 > self.buf.len() {
            return;
        }
        for k in (0..8).rev() {
            self.buf[self.len] = HEX[((v >> (k * 4)) & 0xF) as usize];
            self.len += 1;
        }
        self.buf[self.len] = b' ';
        self.len += 1;
    }
}
