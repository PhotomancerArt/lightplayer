//! The bootloader flavour's prologue: the platform state the ESP-IDF second
//! stage left behind that a core relies on, and nothing else.
//!
//! The core entry contract (C1–C8) is in
//! `docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`. Every
//! value written here was **measured**: the stock IDF bootloader (espflash
//! 3.3.0's bundled ESP-IDF v5.1-beta1 build) run ROM-up on the emulated C6
//! (`lp-emu-esp32c6 --merged … --trace`, the real mask ROM and the real
//! bootloader), its MMIO writes between the ROM's `entry` and the loader's
//! `_start` listed, and each one sorted into *we do it* (here), *esp-hal
//! redoes it* (clocks, the CPU frequency, every watchdog's disable) or *we
//! drop it* (the analog bus, RNG entropy, the console, the app hash). The
//! register meanings are the ESP32-C6 TRM's.
//!
//! No clocks, no PLL, no regi2c, no `LPPERI`, no USB-Serial-JTAG and no
//! GPIO 12/13: the CPU stays on the crystal, as the ROM left it, until
//! `esp_hal::init` sets the clock tree.

mod analog_reset;
mod cache_mmu;
mod debug_assist;
mod flash_setup;
mod lp_interrupts;
mod mmio;
mod rom_flash_chip;
mod watchdogs;

/// Everything the IDF did that the core inherits, in the IDF's order.
pub fn run() {
    // SAFETY: the bootloader runs alone, from RAM, before anything else.
    unsafe {
        flash_setup::mspi_and_spi_clocks();
        analog_reset::configure();
        watchdogs::super_watchdog_auto_feed();
        debug_assist::protection_and_pc_record();
        lp_interrupts::mask_and_clear();
        flash_setup::spi_leftovers();
        rom_flash_chip::four_megabytes();
        watchdogs::rtc_watchdog_9s();
        watchdogs::flash_boot_protection_off();
        cache_mmu::page_mode_32k_and_unmap_all();
    }
}
