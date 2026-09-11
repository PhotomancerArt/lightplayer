//! Generated register-name tables for the ESP32-S3's blocks.
//!
//! A bus log that says `EXTMEM+0x060` has to be decoded by hand against a
//! PAC; one that says `EXTMEM+0x060 icache_ctrl` can be read. The tables are
//! **generated** from the `esp32s3` PAC's svd2rust offset comments by
//! `scripts/emu/pac-regnames.py --pac esp32s3`, carry the provenance header
//! `docs/adr/2026-07-29-license-provenance-discipline.md` requires, and are
//! checked by `just lint-emu-regnames` — which checks **all three** chips, so
//! a hand edit here is caught the same way one in the C6's or the classic's
//! tables is.
//!
//! They live in this crate, not in `lp-emu-esp-common`, because a register
//! layout is chip-family data and that crate holds no chip numbers.
//!
//! This module itself — the `mod`/`pub use` list and the tests below — is
//! hand-written, like both siblings'. The generator writes one file per
//! block; it does not write the module that gathers them, because that module
//! carries prose and assertions no generator could produce.
//!
//! # Which blocks are here, and why
//!
//! Every block M6 P01's MMIO census found the shipped `fw-esp32s3` image
//! touching (`docs/reports/2026-09-11-esp32s3-firmware-inventory.md` §6),
//! plus the two the ROM-up path reaches that the application never does —
//! [`UART0`] for the mask ROM's own console and [`SHA`] for the IDF
//! second-stage bootloader's image hash. A table nothing reads is cheap; a
//! missing one is a phase blocked on a regenerate.
//!
//! # Two tables serve more than one peripheral
//!
//! - [`TIMG0`] is TIMG0 (`0x6001_f000`) and TIMG1 (`0x6002_0000`);
//! - [`UART0`] is UART0 (`0x6000_0000`), UART1 and UART2.
//!
//! [`SPI0`] and [`SPI1`] are **separate** PAC modules on this chip, unlike
//! the classic's single `spi0` type at four bases.
//!
//! # `interrupt_core0` and `interrupt_core1` are one 4 KB window
//!
//! `esp32s3-0.35.2/src/lib.rs` gives both PAC types the base `0x600c_2000`,
//! and that is not an SVD leak: core 0's registers are at `+0x000` and core
//! 1's at `+0x800` **inside the same block**, which is why
//! [`INTERRUPT_CORE1`]'s own first entry is `core_1_intr_map0` at `+0x800`
//! rather than at `+0x000`. Both tables are generated because the register
//! names differ per core, and a trace that read one window against the
//! other's table would name every register wrongly. Whether P03 registers
//! them as one peripheral or two is P03's call; the offsets are already
//! disjoint either way.
//!
//! # One thing the generator cannot produce, and does not pretend to
//!
//! **The flash MMU page table is not in [`EXTMEM`].** The block names only
//! `cache_mmu_fault_content` / `cache_mmu_fault_vaddr` (`+0x120`, `+0x124`),
//! `cache_mmu_power_ctrl` (`+0x12c`) and `cache_mmu_owner` (`+0x148`) — the
//! fault, power and ownership registers, never the table. The table is a
//! directly-addressed window elsewhere in the peripheral space; its address,
//! its entry count and its entry format came out of the vendored mask ROM's
//! own `Cache_*` disassembly (the report's §8) and must never be taken from
//! the classic's numbers or from a datasheet paragraph.
//!
//! # ⚠️ The cache-enable polarity is INVERTED relative to the C6's
//!
//! The S3's [`EXTMEM`] `icache_ctrl` (`+0x060`) / `dcache_ctrl` (`+0x000`)
//! carry an *enable* bit — "0 disable, 1 enable" — where the C6's
//! `l1_icache_ctrl.l1_icache_shut_ibus0` is a *shut* bit, "0 enable, 1
//! disable". A cache-off watch (D4) copied from the C6 arms backwards. The
//! table cannot say this — svd2rust's offset comments carry names, not field
//! semantics — so it is said here, where a reader of the table will be.

mod apb_ctrl;
mod bb;
mod efuse;
mod extmem;
mod fe;
mod fe2;
mod gpio;
mod i2c_ana_mst;
mod interrupt_core0;
mod interrupt_core1;
mod io_mux;
mod nrx;
mod rmt;
mod rtc_cntl;
mod sha;
mod spi0;
mod spi1;
mod system;
mod systimer;
mod timg0;
mod uart0;
mod usb_device;

pub use apb_ctrl::APB_CTRL;
pub use bb::BB;
pub use efuse::EFUSE;
pub use extmem::EXTMEM;
pub use fe::FE;
pub use fe2::FE2;
pub use gpio::GPIO;
pub use i2c_ana_mst::I2C_ANA_MST;
pub use interrupt_core0::INTERRUPT_CORE0;
pub use interrupt_core1::INTERRUPT_CORE1;
pub use io_mux::IO_MUX;
pub use nrx::NRX;
pub use rmt::RMT;
pub use rtc_cntl::RTC_CNTL;
pub use sha::SHA;
pub use spi0::SPI0;
pub use spi1::SPI1;
pub use system::SYSTEM;
pub use systimer::SYSTIMER;
pub use timg0::TIMG0;
pub use uart0::UART0;
pub use usb_device::USB_DEVICE;

/// Every table, for the tests that sweep them.
pub const ALL: &[&lp_emu_esp_common::RegNames] = &[
    &APB_CTRL,
    &BB,
    &EFUSE,
    &EXTMEM,
    &FE,
    &FE2,
    &GPIO,
    &I2C_ANA_MST,
    &INTERRUPT_CORE0,
    &INTERRUPT_CORE1,
    &IO_MUX,
    &NRX,
    &RMT,
    &RTC_CNTL,
    &SHA,
    &SPI0,
    &SPI1,
    &SYSTEM,
    &SYSTIMER,
    &TIMG0,
    &UART0,
    &USB_DEVICE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generated_table_is_sorted_and_non_empty() {
        for table in ALL {
            table.assert_sorted();
            assert!(!table.is_empty(), "`{}` is empty", table.block);
        }
    }

    /// Every table names the block the emulator will register it under, so a
    /// trace line can be attributed without a second lookup.
    #[test]
    fn the_block_names_are_the_pac_module_names() {
        let blocks: Vec<&str> = ALL.iter().map(|t| t.block).collect();
        for expected in [
            "apb_ctrl",
            "bb",
            "efuse",
            "extmem",
            "fe",
            "fe2",
            "gpio",
            "i2c_ana_mst",
            "interrupt_core0",
            "interrupt_core1",
            "io_mux",
            "nrx",
            "rmt",
            "rtc_cntl",
            "sha",
            "spi0",
            "spi1",
            "system",
            "systimer",
            "timg0",
            "uart0",
            "usb_device",
        ] {
            assert!(blocks.contains(&expected), "`{expected}` has no table");
        }
        assert_eq!(blocks.len(), 22);
    }

    /// The four RF-adjacent blocks `esp_hal::init` touches inline are one
    /// register each, and the register is the one the census saw
    /// (`BB+0x54`, `NRX+0xd4`, `FE+0x90`, `FE2+0xf0`). None of them is named
    /// in the milestone brief's peripheral list; all four arrive in the first
    /// few thousand cycles of a strict bring-up, so P04 accepts them from its
    /// first commit rather than meeting them one stop at a time.
    #[test]
    fn the_four_rf_blocks_are_one_register_each_at_the_offset_the_census_saw() {
        assert_eq!(BB.name(0x054), Some("bbpd_ctrl"));
        assert_eq!(NRX.name(0x0d4), Some("nrxpd_ctrl"));
        assert_eq!(FE.name(0x090), Some("gen_ctrl"));
        assert_eq!(FE2.name(0x0f0), Some("tx_interp_ctrl"));
        for table in [&BB, &NRX, &FE, &FE2] {
            assert_eq!(table.entries.len(), 1, "`{}`", table.block);
        }
    }

    /// The four software interrupts live in `SYSTEM`, not in an INTPRI as on
    /// the C6 and not in DPORT as on the classic. `swi1` is the wire-pusher
    /// doorbell and `swi2` the io_task executor, on every chip in this plan.
    #[test]
    fn the_software_interrupts_are_systems_on_this_chip() {
        assert_eq!(SYSTEM.name(0x030), Some("cpu_intr_from_cpu0"));
        assert_eq!(SYSTEM.name(0x034), Some("cpu_intr_from_cpu1"));
        assert_eq!(SYSTEM.name(0x038), Some("cpu_intr_from_cpu2"));
        assert_eq!(SYSTEM.name(0x03c), Some("cpu_intr_from_cpu3"));
    }

    /// The two interrupt-matrix windows are one block: core 0 at `+0x000`,
    /// core 1 at `+0x800`, 99 map entries each (the PAC's `Interrupt` enum
    /// numbers sources 0..=98 with gaps, so 99 is the number RANGE and not
    /// the variant count).
    #[test]
    fn the_interrupt_matrix_is_one_window_with_two_halves() {
        assert_eq!(INTERRUPT_CORE0.name(0x000), Some("core_0_intr_map0"));
        assert_eq!(INTERRUPT_CORE1.name(0x800), Some("core_1_intr_map0"));
        let count = |t: &lp_emu_esp_common::RegNames, prefix: &str| {
            t.entries
                .iter()
                .filter(|(_, name)| name.starts_with(prefix))
                .count()
        };
        assert_eq!(count(&INTERRUPT_CORE0, "core_0_intr_map"), 99);
        assert_eq!(count(&INTERRUPT_CORE1, "core_1_intr_map"), 99);
        // The two halves do not overlap, whichever way P03 registers them.
        assert!(INTERRUPT_CORE0.entries.iter().all(|(off, _)| *off < 0x800));
        assert!(INTERRUPT_CORE1.entries.iter().all(|(off, _)| *off >= 0x800));
    }

    /// 49 pads (GPIO0..=GPIO48) in IO_MUX — and **54** output-mux slots in
    /// the GPIO matrix, which is not the same number. A model that sized the
    /// matrix from the pad count would lose five slots, and one that sized
    /// the pads from the matrix would invent five.
    #[test]
    fn forty_nine_pads_but_fifty_four_output_mux_slots() {
        let pads = IO_MUX
            .entries
            .iter()
            .filter(|(_, n)| n.starts_with("gpio"))
            .count();
        assert_eq!(pads, 49);
        assert_eq!(IO_MUX.name(0x004), Some("gpio0"));
        assert_eq!(IO_MUX.name(0x0c4), Some("gpio48"));

        let slots = GPIO
            .entries
            .iter()
            .filter(|(_, n)| n.starts_with("func") && n.ends_with("_out_sel_cfg"))
            .count();
        assert_eq!(slots, 54);
    }

    /// `EXTMEM` carries the cache's control and the MMU's fault, power and
    /// ownership registers — and **no table**. The table's address is the
    /// ROM's to say (module docs, report §8); an `EXTMEM` offset claimed for
    /// it here would be a model that boots and lies.
    #[test]
    fn extmem_has_a_cache_and_an_mmu_but_no_mmu_table() {
        assert_eq!(EXTMEM.name(0x000), Some("dcache_ctrl"));
        assert_eq!(EXTMEM.name(0x060), Some("icache_ctrl"));
        assert_eq!(EXTMEM.name(0x120), Some("cache_mmu_fault_content"));
        assert_eq!(EXTMEM.name(0x124), Some("cache_mmu_fault_vaddr"));
        assert_eq!(EXTMEM.name(0x12c), Some("cache_mmu_power_ctrl"));
        assert_eq!(EXTMEM.name(0x148), Some("cache_mmu_owner"));
        for absent in ["mmu_item_index", "mmu_item_content", "immu_table0"] {
            assert!(
                !EXTMEM.entries.iter().any(|(_, n)| *n == absent),
                "`{absent}` is another chip's MMU path and must not appear here"
            );
        }
    }

    /// The S3's RMT register block stops at `+0x0cc`. The channel RAM the
    /// firmware's own driver writes is at `RMT_BASE + 0x800` — outside the
    /// block, so a view that registered only the register window would drop
    /// every symbol the strip is made of.
    #[test]
    fn the_rmt_register_block_stops_before_the_channel_ram() {
        assert_eq!(RMT.name(0x0cc), Some("date"));
        assert!(RMT.entries.iter().all(|(off, _)| *off <= 0x0cc));
        // Four TX channels, and the RX conf registers interleaved above them.
        for ch in 0..4 {
            let name = format!("ch{ch}_tx_conf0");
            assert!(RMT.entries.iter().any(|(_, n)| *n == name), "{name}");
        }
    }

    /// The link. The first twenty registers are byte-for-byte the C6's
    /// layout — which is what D1 rests on — and the census says the image
    /// touches only `+0x00..=+0x18` of them. The C6 has eight MORE registers
    /// at `+0x04c..=+0x068` (`chip_rst`, the CDC line-coding quad,
    /// `config_update`, `ser_afifo_config`, `bus_reset_st`) that the S3 does
    /// not: a view shared between the two chips must not answer those here.
    #[test]
    fn usb_device_is_the_c6s_layout_for_every_register_the_image_touches() {
        assert_eq!(USB_DEVICE.name(0x000), Some("ep1"));
        assert_eq!(USB_DEVICE.name(0x004), Some("ep1_conf"));
        assert_eq!(USB_DEVICE.name(0x008), Some("int_raw"));
        assert_eq!(USB_DEVICE.name(0x00c), Some("int_st"));
        assert_eq!(USB_DEVICE.name(0x010), Some("int_ena"));
        assert_eq!(USB_DEVICE.name(0x014), Some("int_clr"));
        assert_eq!(USB_DEVICE.name(0x018), Some("conf0"));
        for absent in [
            "chip_rst",
            "set_line_code_w0",
            "config_update",
            "bus_reset_st",
        ] {
            assert!(
                !USB_DEVICE.entries.iter().any(|(_, n)| *n == absent),
                "`{absent}` is a C6 register and the S3's PAC does not have it"
            );
        }
    }

    /// RTC_CNTL holds the reset cause and the RWDT the shipped image **arms
    /// and feeds on every boot** — the first watchdog in this plan that
    /// actually runs.
    #[test]
    fn rtc_cntl_names_the_watchdog_the_firmware_arms() {
        assert_eq!(RTC_CNTL.name(0x000), Some("options0"));
        for reg in [
            "wdtconfig0",
            "wdtfeed",
            "wdtwprotect",
            "store0",
            "reset_state",
        ] {
            assert!(
                RTC_CNTL.entries.iter().any(|(_, n)| *n == reg),
                "`{reg}` is missing from RTC_CNTL"
            );
        }
    }
}
