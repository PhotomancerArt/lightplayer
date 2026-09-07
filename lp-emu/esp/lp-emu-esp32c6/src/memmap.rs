//! The ESP32-C6's address space.
//!
//! Provenance: every number here is read from
//! `esp-hal-1.1.1/ld/esp32c6/memory.x` (the verbatim `MEMORY_MAP` comment at
//! `:3-15` and the link regions at `:22-39`) and
//! `esp-metadata-generated-0.4.0/src/_generated_esp32c6.rs:4088-4101`, both
//! surveyed in `m3/discovery-esp-hal-timers-uart-usb-pac.md` §9 and
//! `m3/discovery-esp-hal-csr-irq.md` §6a. Nothing here is inferred from a
//! datasheet PDF and nothing is guessed.
//!
//! # The regions, and why the splits are where they are
//!
//! ```text
//!   0x4000_0000 +0x4_AC00  ROM_MASK     mask ROM code      exec, read-only
//!   0x4004_AC00 +0x0_5400  DROM_MASK    mask ROM data      read-only
//!   0x4080_0000 +0x8_0000  HP SRAM      512 KiB            exec, writable
//!   0x4200_0000 +0x80_0000 flash cache  IROM + RODATA      exec, read-only
//!   0x4280_0000 +0x80_0000 DROM window  (kept readable)    read-only
//!   0x5000_0000 +0x0_4000  LP SRAM      RTC_FAST, 16 KiB   exec, writable
//! ```
//!
//! **HP SRAM is one region.** esp-hal's linker script cuts it into an app
//! part ending at [`APP_RAM_END`], the bootloader's reclaimed
//! [`DRAM2_BASE`]`..`[`DRAM2_END`] (the second heap region), and the ROM's own
//! data and stack above that — but those are *documentation*, not decode. The
//! hardware has one 512 KiB window and modelling three would only let a
//! one-byte overrun read as an unmapped fault where silicon reads a byte.
//!
//! **`MEM_INTERNAL2` (`0x600F_E000`) is deliberately left unmapped.** It sits
//! inside the high MMIO window, so an access to it is reported as "inside a
//! declared MMIO window: an unmodelled block" rather than as a wild pointer —
//! which is exactly the diagnosis a reader wants.

/// Mask ROM code. `rtc_get_reset_reason` at `0x4000_0018` lives here.
pub const ROM_MASK_BASE: u32 = 0x4000_0000;
pub const ROM_MASK_LEN: u32 = 0x0004_AC00;

/// Mask ROM data.
pub const DROM_MASK_BASE: u32 = 0x4004_AC00;
pub const DROM_MASK_LEN: u32 = 0x0000_5400;

/// High-performance SRAM: 512 KiB, one region (see the module docs).
pub const HP_SRAM_BASE: u32 = 0x4080_0000;
pub const HP_SRAM_LEN: u32 = 0x0008_0000;

/// Where esp-hal's `RAM` link region stops, and where the app's stack starts
/// (`_stack_start`; the stack grows down). `memory.x:22` —
/// `LENGTH = 0x6E610`, "2nd stage bootloader iram_loader_seg start address".
pub const APP_RAM_END: u32 = 0x4086_E610;

/// `dram2_seg`: the 64 KiB the ESP-IDF second-stage bootloader occupies and
/// then vacates. esp-alloc's **second heap region** is a
/// `static MaybeUninit` placed here by `#[esp_hal::ram(reclaimed)]`.
pub const DRAM2_BASE: u32 = 0x4086_E610;
pub const DRAM2_END: u32 = 0x4087_E610;

/// Above `dram2_seg`: the mask ROM's own data and stack.
pub const ROM_DATA_BASE: u32 = 0x4087_E610;

/// `rom_spiflash_legacy_data` (`esp32c6.rom.ld:166`) — the pointer through
/// which the C6 reaches the flash-chip struct (there is no `g_rom_flashchip`).
pub const ROM_SPIFLASH_LEGACY_DATA: u32 = 0x4087_FFEC;

/// `syscall_table_ptr` (`esp32c6.rom.ld:27`), written by `esp_hal::init`.
pub const SYSCALL_TABLE_PTR: u32 = 0x4087_FFD4;

/// The flash cache window the app's `.text` and `.rodata` are mapped through.
///
/// M3 models it as flat RAM holding the app's flash-mapped segments; M4
/// replaces it with the MMU page table and SPI1. esp-hal links at
/// `0x4200_0000 + 0x20` — the `+0x20` satisfies the cache MMU's
/// `paddr % 64K == vaddr % 64K`, which is an M4 concern; the region starts at
/// the window base so the first 32 bytes are addressable rather than a hole.
pub const FLASH_CACHE_BASE: u32 = 0x4200_0000;
pub const FLASH_CACHE_LEN: u32 = 0x0080_0000;

/// The separate DROM window. The C6 app maps its read-only data through
/// `0x4200_0000` too (`linkall.x` aliases `RODATA → ROM`), so nothing in the
/// shipped image lands here — it is kept readable for completeness, and so
/// that a pointer that *does* arrive here reads as data rather than as a
/// fault we would then have to explain.
pub const DROM_WINDOW_BASE: u32 = 0x4280_0000;
pub const DROM_WINDOW_LEN: u32 = 0x0080_0000;

/// LP SRAM / `RTC_FAST`, 16 KiB, executable and persistent over deep sleep.
/// The firmware's recovery ledger lives here.
pub const LP_SRAM_BASE: u32 = 0x5000_0000;
pub const LP_SRAM_LEN: u32 = 0x0000_4000;

/// The low MMIO window (the PLIC_MX / interrupt-controller aperture).
pub const MMIO_LOW_BASE: u32 = 0x2000_0000;
pub const MMIO_LOW_LEN: u32 = 0x0001_0000;

/// The main peripheral window: UART0 at `0x6000_0000` through
/// `INTERRUPT_CORE0` at `0x6001_0000` and up.
pub const MMIO_HIGH_BASE: u32 = 0x6000_0000;
pub const MMIO_HIGH_LEN: u32 = 0x0010_0000;

/// `MEM_INTERNAL2`, inside [`MMIO_HIGH_BASE`]'s window and deliberately not
/// mapped. Named so a reader of a fault log can recognise it.
pub const MEM_INTERNAL2_BASE: u32 = 0x600F_E000;

/// Peripheral base addresses, from `esp32c6-0.23.2/src/lib.rs` (every
/// peripheral is `Periph<RegisterBlock, BASE>`; the line numbers are in
/// `m3/discovery-esp-hal-timers-uart-usb-pac.md` §7). The window each block
/// gets is decided where it is registered ([`crate::periph::boot_set`]).
pub mod periph {
    pub const PLIC_MX: u32 = 0x2000_1000;
    pub const UART0: u32 = 0x6000_0000;
    pub const UART1: u32 = 0x6000_1000;
    pub const SPI0: u32 = 0x6000_2000;
    pub const SPI1: u32 = 0x6000_3000;
    pub const RMT: u32 = 0x6000_6000;
    pub const TIMG0: u32 = 0x6000_8000;
    pub const TIMG1: u32 = 0x6000_9000;
    pub const SYSTIMER: u32 = 0x6000_A000;
    pub const APB_SARADC: u32 = 0x6000_E000;
    pub const USB_DEVICE: u32 = 0x6000_F000;
    pub const INTERRUPT_CORE0: u32 = 0x6001_0000;
    pub const IO_MUX: u32 = 0x6009_0000;
    pub const GPIO: u32 = 0x6009_1000;
    pub const HP_SYS: u32 = 0x6009_5000;
    pub const PCR: u32 = 0x6009_6000;
    pub const TEE: u32 = 0x6009_8000;
    pub const HP_APM: u32 = 0x6009_9000;
    pub const LP_APM0: u32 = 0x6009_9800;
    /// The WiFi MAC/BB window; `IEEE802154` is at `0x600A_3000`. Not mapped
    /// on purpose (P5: the radio stub is P6).
    pub const MODEM_WINDOW: u32 = 0x600A_0000;
    pub const MODEM_SYSCON: u32 = 0x600A_9800;
    pub const MODEM_LPCON: u32 = 0x600A_F000;
    pub const I2C_ANA_MST: u32 = 0x600A_F800;
    pub const PMU: u32 = 0x600B_0000;
    pub const LP_CLKRST: u32 = 0x600B_0400;
    pub const EFUSE: u32 = 0x600B_0800;
    pub const LP_TIMER: u32 = 0x600B_0C00;
    pub const LP_AON: u32 = 0x600B_1000;
    pub const LP_WDT: u32 = 0x600B_1C00;
    pub const LP_IO: u32 = 0x600B_2000;
    pub const LP_I2C_ANA_MST: u32 = 0x600B_2400;
    /// `LP_PERI` and `RNG` share this base; `rng_data` is at `+0x08`.
    pub const RNG: u32 = 0x600B_2800;
    pub const LP_TEE: u32 = 0x600B_3400;
    pub const LP_APM: u32 = 0x600B_3800;
    pub const ASSIST_DEBUG: u32 = 0x600C_2000;
    pub const INTPRI: u32 = 0x600C_5000;
    pub const EXTMEM: u32 = 0x600C_8000;
}

/// CPU clock: 160 MHz (`fw-esp32c6/src/board/esp32c6/constants.rs:12`,
/// esp-hal `CpuClock::max()`). Guest microseconds are `cycles / 160`.
pub const CPU_HZ: u64 = 160_000_000;

/// Cycles per emulated microsecond.
pub const CYCLES_PER_US: u64 = CPU_HZ / 1_000_000;

/// One named span of the map, for the machine's `--map` style reporting and
/// for the tests that assert the map does not drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub name: &'static str,
    pub base: u32,
    pub len: u32,
}

impl Span {
    pub const fn end(&self) -> u32 {
        self.base + self.len
    }

    pub const fn contains(&self, address: u32) -> bool {
        address >= self.base && address < self.base + self.len
    }
}

/// The RAM-backed regions, in address order. What
/// [`crate::machine::Esp32C6Builder`] registers on the bus.
pub const RAM_SPANS: &[Span] = &[
    Span {
        name: "rom-mask",
        base: ROM_MASK_BASE,
        len: ROM_MASK_LEN,
    },
    Span {
        name: "drom-mask",
        base: DROM_MASK_BASE,
        len: DROM_MASK_LEN,
    },
    Span {
        name: "hp-sram",
        base: HP_SRAM_BASE,
        len: HP_SRAM_LEN,
    },
    Span {
        name: "flash-cache",
        base: FLASH_CACHE_BASE,
        len: FLASH_CACHE_LEN,
    },
    Span {
        name: "drom-window",
        base: DROM_WINDOW_BASE,
        len: DROM_WINDOW_LEN,
    },
    Span {
        name: "lp-sram",
        base: LP_SRAM_BASE,
        len: LP_SRAM_LEN,
    },
];

/// The declared MMIO windows. An access inside one that no peripheral claims
/// is still unmapped, but the log says "an unmodelled block".
pub const MMIO_WINDOWS: &[Span] = &[
    Span {
        name: "mmio-low",
        base: MMIO_LOW_BASE,
        len: MMIO_LOW_LEN,
    },
    Span {
        name: "mmio-high",
        base: MMIO_HIGH_BASE,
        len: MMIO_HIGH_LEN,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spans_do_not_overlap_and_are_in_address_order() {
        for pair in RAM_SPANS.windows(2) {
            assert!(
                pair[0].end() <= pair[1].base,
                "`{}` (ends 0x{:08x}) overlaps `{}` (starts 0x{:08x})",
                pair[0].name,
                pair[0].end(),
                pair[1].name,
                pair[1].base
            );
        }
        for ram in RAM_SPANS {
            for mmio in MMIO_WINDOWS {
                assert!(
                    ram.end() <= mmio.base || mmio.end() <= ram.base,
                    "RAM `{}` overlaps MMIO window `{}`",
                    ram.name,
                    mmio.name
                );
            }
        }
    }

    #[test]
    fn the_mask_rom_windows_abut_exactly_as_memory_x_says() {
        // memory.x: ROM_MASK 0x40000000..0x4004AC00, DROM_MASK
        // 0x4004AC00..0x40050000. A gap or an overlap here would put the
        // ROM's data segment somewhere the ELF does not say.
        assert_eq!(ROM_MASK_BASE + ROM_MASK_LEN, DROM_MASK_BASE);
        assert_eq!(DROM_MASK_BASE + DROM_MASK_LEN, 0x4005_0000);
    }

    #[test]
    fn hp_sram_covers_the_app_the_reclaimed_heap_and_the_roms_data() {
        // The three documented parts, in order, all inside the one region.
        assert_eq!(DRAM2_BASE, APP_RAM_END, "dram2_seg starts where RAM ends");
        assert_eq!(DRAM2_END, ROM_DATA_BASE);
        assert_eq!(DRAM2_END - DRAM2_BASE, 0x1_0000, "64 KiB, the second heap");
        assert!(ROM_SPIFLASH_LEGACY_DATA >= ROM_DATA_BASE);
        assert!(SYSCALL_TABLE_PTR >= ROM_DATA_BASE);
        assert!(ROM_SPIFLASH_LEGACY_DATA < HP_SRAM_BASE + HP_SRAM_LEN);
    }

    #[test]
    fn mem_internal2_is_inside_the_mmio_window_and_claimed_by_no_ram_region() {
        assert!(MMIO_WINDOWS.iter().any(|w| w.contains(MEM_INTERNAL2_BASE)));
        assert!(!RAM_SPANS.iter().any(|r| r.contains(MEM_INTERNAL2_BASE)));
    }

    #[test]
    fn one_emulated_microsecond_is_160_cycles() {
        assert_eq!(CYCLES_PER_US, 160);
    }
}
