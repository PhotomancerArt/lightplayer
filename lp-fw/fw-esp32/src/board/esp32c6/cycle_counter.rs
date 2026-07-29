//! ESP32-C6 PMU cycle counter helpers.
//!
//! The standard RISC-V Zicntr CSRs (`mcycle` 0xB00 and user-mode mirror
//! `cycle` 0xC00) both raise "Illegal instruction" on this part. The C6
//! exposes Espressif (Andes N22) PMU CSRs instead:
//!
//! - `mpcer` (0x7E0): event select, `1` = cycles
//! - `mpcmr` (0x7E1): mode / enable, `1` = enabled
//! - `mpccr` (0x7E2): 32-bit counter
//!
//! The counter wraps every ~26.8s at 160MHz. Individual benchmark samples are
//! far shorter, so `wrapping_sub` is the intended delta operation.
//!
//! Chip-specific: the ESP32-S3 (Xtensa) equivalent is `rsr.ccount`.

/// Configure the PMU to count CPU cycles into `mpccr`.
pub fn setup() {
    unsafe {
        core::arch::asm!("csrw 0x7E0, {}", in(reg) 1u32);
        core::arch::asm!("csrw 0x7E1, {}", in(reg) 1u32);
    }
}

#[inline(always)]
pub fn read() -> u32 {
    let cycles: u32;
    unsafe {
        core::arch::asm!("csrr {}, 0x7E2", out(reg) cycles);
    }
    cycles
}

#[cfg(feature = "test_shader_compile_incremental")]
#[inline]
pub fn cycles_to_us(cycles: u64) -> u64 {
    (cycles * 1_000_000) / super::constants::CPU_HZ
}
