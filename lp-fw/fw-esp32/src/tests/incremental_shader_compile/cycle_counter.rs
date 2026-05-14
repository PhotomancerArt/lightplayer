//! ESP32-C6 PMU cycle counter helpers for compile-slice timing.

const ESP32C6_HZ: u64 = 160_000_000;

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

#[inline]
pub fn cycles_to_us(cycles: u64) -> u64 {
    (cycles * 1_000_000) / ESP32C6_HZ
}
