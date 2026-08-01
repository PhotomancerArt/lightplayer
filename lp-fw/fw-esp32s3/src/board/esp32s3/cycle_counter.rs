//! ESP32-S3 (Xtensa LX7) cycle counter.
//!
//! Xtensa exposes a free-running cycle count in the `CCOUNT` special register,
//! read with `rsr.ccount`. Simpler than the C6's counterpart, which needs three
//! Espressif PMU CSRs configured first because the standard RISC-V Zicntr CSRs
//! raise "Illegal instruction" on that part.
//!
//! The counter is 32-bit and wraps every ~17.9 s at 240 MHz, so `wrapping_sub`
//! is the intended delta operation. Individual measurements here are far
//! shorter than that.

/// Nothing to configure — `CCOUNT` free-runs after reset. Present so callers
/// read the same on both chips; the C6's counterpart really does need setup.
pub fn setup() {}

#[inline(always)]
pub fn read() -> u32 {
    let cycles: u32;
    // SAFETY: `rsr.ccount` reads a special register and has no side effects.
    unsafe {
        core::arch::asm!("rsr.ccount {}", out(reg) cycles, options(nomem, nostack));
    }
    cycles
}

/// Convert a cycle delta to microseconds.
#[inline]
pub fn cycles_to_us(cycles: u64) -> u64 {
    (cycles * 1_000_000) / super::constants::CPU_HZ
}
