//! Main-task stack high-water probe.
//!
//! Ported from `fw-esp32c6/src/stack_probe.rs`; only `current_sp` differs
//! (Xtensa reads `a1`). On this chip the measurement matters more than it does
//! anywhere else in the family: `dram_seg` is 192 KB and `.stack` is the
//! *residual* after `.data` and `.bss` — esp-hal's `stack.x` hands it whatever
//! the arena leaves — so every byte of unused stack is a byte the allocator
//! could have had, and nothing has ever measured how much of it a workload
//! actually uses.
//!
//! This paints the unused stack once at boot and later scans for the lowest
//! painted word still intact — the classic watermark — so the heartbeat can log
//! "high-water N B of M B" and `HEAP_SIZE` can be set from a number instead of
//! from caution.
//!
//! Xtensa-specific (reads `a1` with inline asm, which `main.rs`'s
//! `asm_experimental_arch` allows); the linker symbols are esp-hal's
//! `_stack_start`/`_stack_end` (top/bottom of the main stack).

use core::sync::atomic::{AtomicUsize, Ordering};

unsafe extern "C" {
    static _stack_start: u32;
    static _stack_end: u32;
}

const PATTERN: u32 = 0xA5A5_5A5A;
/// Bytes below the current `sp` left unpainted so the painter's own frame and
/// any interrupt frame that lands during the paint stay intact. Generous for
/// the Xtensa windowed ABI, whose spill area sits just below a frame.
const PAINT_MARGIN: usize = 1024;
/// Bytes above the stack bottom left alone: esp-hal keeps the main stack's
/// guard word there (`ESP_HAL_CONFIG_STACK_GUARD_OFFSET` from `_stack_end`,
/// default 60) and may watch it, so painting over it is itself reported as an
/// overflow — that is how the C6 port first failed. The high-water scan starts
/// above it too; the 256 B it costs the measurement are noise against a 40 KB
/// stack.
const GUARD_SKIP: usize = 256;

static HIGH_WATER_REPORTED: AtomicUsize = AtomicUsize::new(0);

/// Lowest address the probe touches: the stack bottom plus the guard zone.
fn stack_bottom() -> usize {
    (&raw const _stack_end) as usize + GUARD_SKIP
}

fn stack_top() -> usize {
    (&raw const _stack_start) as usize
}

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reads `a1`, the Xtensa stack pointer; no memory is touched.
    unsafe { core::arch::asm!("mov {0}, a1", out(reg) sp) };
    sp
}

/// Total main-stack size in bytes (guard zone included).
pub fn total_bytes() -> usize {
    stack_top() - (&raw const _stack_end) as usize
}

/// Paint everything between the stack bottom and (a margin below) the current
/// `sp`. Call once, early in boot, from the main task.
pub fn paint() {
    critical_section::with(|_| {
        let lo = stack_bottom();
        let hi = current_sp().saturating_sub(PAINT_MARGIN);
        let mut addr = lo;
        while addr + 4 <= hi {
            // SAFETY: `lo..hi` is main-stack memory below the live frames (by
            // `PAINT_MARGIN`), unused at this point in boot, and interrupts are
            // masked so nothing else can push into it.
            unsafe { (addr as *mut u32).write_volatile(PATTERN) };
            addr += 4;
        }
    });
}

/// Bytes of the main stack ever used since `paint`: the distance from the top
/// to the lowest word whose paint is gone. Saturates at the total if the stack
/// overflowed past its bottom (every word touched).
pub fn high_water_bytes() -> usize {
    let lo = stack_bottom();
    let top = stack_top();
    let mut addr = lo;
    while addr + 4 <= top {
        // SAFETY: reading main-stack memory; a torn read of a live frame just
        // fails the pattern test, which is the conservative answer.
        if unsafe { (addr as *const u32).read_volatile() } != PATTERN {
            return top - addr;
        }
        addr += 4;
    }
    0
}

/// Log the high-water mark when it has grown since the last report (so a
/// once-per-heartbeat call stays quiet in the steady state).
pub fn log_if_grown(label: &str) {
    let used = high_water_bytes();
    let last = HIGH_WATER_REPORTED.load(Ordering::Relaxed);
    if used > last {
        HIGH_WATER_REPORTED.store(used, Ordering::Relaxed);
        let total = total_bytes();
        esp_println::println!(
            "[stack] {label}: high-water {used} B of {total} B ({} B headroom)",
            total.saturating_sub(used)
        );
    }
}
