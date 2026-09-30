//! A count of boots in RTC fast RAM, for the link nonce's salt (ruling DD28
//! of the classic-UART plan; `fw_esp32_common::uart_link::session_nonce`).
//!
//! RTC fast RAM keeps its contents across a software reset (a Reboot request)
//! and a watchdog reset, and loses them on a power-on or EN-pin reset — the
//! same boundary the recovery ledger lives by
//! (`crate::recovery::esp32v3_recovery_backend`'s module docs). So every boot
//! after a software reset reads the previous boot's count and writes the next
//! one, and its nonce differs from the previous boot's even if the RNG gave
//! the same word. At power-on the count is whatever the RAM powered up
//! holding, which is as good a starting point as any: only the step matters.
//!
//! Four bytes of the 8 KiB segment, beside the recovery region; none of it
//! comes out of the heap or `.stack`.

use esp_hal::ram;

/// The count. `persistent`: not initialized at load, so a software reset
/// leaves the last boot's value here.
#[ram(unstable(rtc_fast, persistent))]
static mut BOOT_COUNT: u32 = 0;

/// Bump the count and return this boot's value. Called once per boot, on the
/// PRO core (RTC fast RAM is reachable from PRO_CPU only), before the link
/// is made.
pub fn next_boot_count() -> u32 {
    // SAFETY: called once, from `main` on the PRO core before any task is
    // spawned, so nothing else can be reading or writing the static.
    unsafe {
        let count = (*&raw const BOOT_COUNT).wrapping_add(1);
        *&raw mut BOOT_COUNT = count;
        count
    }
}
