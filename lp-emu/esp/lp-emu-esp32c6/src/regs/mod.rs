//! Generated register-name tables for the C6's blocks, and the interrupt
//! source table.
//!
//! A bus log that says `LP_CLKRST+0x010` has to be decoded by hand against a
//! PAC; one that says `LP_CLKRST+0x010 reset_cause` can be read. The tables
//! are **generated** from the `esp32c6` PAC's svd2rust offset comments by
//! `scripts/emu/pac-regnames.py`, carry the provenance header
//! `docs/adr/2026-07-29-license-provenance-discipline.md` requires, and are
//! checked by `just lint-emu-regnames` — a hand edit is reverted by the next
//! regeneration and takes its provenance with it.
//!
//! They live in this crate, not in `lp-emu-esp-common`, because a register
//! layout is chip-family data and that crate holds no chip numbers.
//!
//! One table per block the boot path touches (P5), modelled or accepted —
//! every block in [`crate::periph::boot_set`] has its row here — plus
//! [`INTERRUPT_SOURCES`], the PAC's `Interrupt` enum: the numbers are the
//! indices of `core_0_intr_map`, so they are SVD-derived data too.
//!
//! One table here is **hand-written**, and says so in its own header:
//! [`output_signals`], the GPIO matrix's output-signal numbers (M5 P2). They
//! are not a register block and not in the PAC — they live in esp-hal's
//! generated metadata — so the generator cannot produce them; the file
//! carries the same provenance citation instead.

mod apb_saradc;
mod assist_debug;
mod efuse;
mod extmem;
mod gpio;
mod hinf;
mod hp_apm;
mod hp_sys;
mod i2c_ana_mst;
mod interrupt_core0;
mod interrupt_sources;
mod intpri;
mod io_mux;
mod lp_ana;
mod lp_aon;
mod lp_apm;
mod lp_apm0;
mod lp_clkrst;
mod lp_i2c_ana_mst;
mod lp_io;
mod lp_peri;
mod lp_tee;
mod lp_timer;
mod lp_wdt;
mod modem_lpcon;
mod modem_syscon;
pub mod output_signals;
mod pcr;
mod plic_mx;
mod plic_ux;
mod pmu;
mod rmt;
mod sha;
mod slc;
mod spi0;
mod spi1;
mod systimer;
mod tee;
mod timg0;
mod uart0;
mod usb_device;

pub use apb_saradc::APB_SARADC;
pub use assist_debug::ASSIST_DEBUG;
pub use efuse::EFUSE;
pub use extmem::EXTMEM;
pub use gpio::GPIO;
pub use hinf::HINF;
pub use hp_apm::HP_APM;
pub use hp_sys::HP_SYS;
pub use i2c_ana_mst::I2C_ANA_MST;
pub use interrupt_core0::INTERRUPT_CORE0;
pub use interrupt_sources::{INTERRUPT_SOURCES, source};
pub use intpri::INTPRI;
pub use io_mux::IO_MUX;
pub use lp_ana::LP_ANA;
pub use lp_aon::LP_AON;
pub use lp_apm::LP_APM;
pub use lp_apm0::LP_APM0;
pub use lp_clkrst::LP_CLKRST;
pub use lp_i2c_ana_mst::LP_I2C_ANA_MST;
pub use lp_io::LP_IO;
pub use lp_peri::LP_PERI;
pub use lp_tee::LP_TEE;
pub use lp_timer::LP_TIMER;
pub use lp_wdt::LP_WDT;
pub use modem_lpcon::MODEM_LPCON;
pub use modem_syscon::MODEM_SYSCON;
pub use pcr::PCR;
pub use plic_mx::PLIC_MX;
pub use plic_ux::PLIC_UX;
pub use pmu::PMU;
pub use rmt::RMT;
pub use sha::SHA;
pub use slc::SLC;
pub use spi0::SPI0;
pub use spi1::SPI1;
pub use systimer::SYSTIMER;
pub use tee::TEE;
pub use timg0::TIMG0;
pub use uart0::UART0;
pub use usb_device::USB_DEVICE;

/// Every table, for the tests that sweep them.
pub const ALL: &[&lp_emu_esp_common::RegNames] = &[
    &APB_SARADC,
    &ASSIST_DEBUG,
    &EFUSE,
    &EXTMEM,
    &GPIO,
    &HP_APM,
    &HP_SYS,
    &I2C_ANA_MST,
    &INTERRUPT_CORE0,
    &INTPRI,
    &IO_MUX,
    &LP_AON,
    &LP_APM,
    &LP_APM0,
    &LP_CLKRST,
    &LP_I2C_ANA_MST,
    &LP_IO,
    &LP_PERI,
    &LP_TEE,
    &LP_TIMER,
    &LP_WDT,
    &MODEM_LPCON,
    &MODEM_SYSCON,
    &PCR,
    &HINF,
    &LP_ANA,
    &PLIC_MX,
    &PLIC_UX,
    &SHA,
    &SLC,
    &PMU,
    &RMT,
    &SPI0,
    &SPI1,
    &SYSTIMER,
    &TEE,
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

    #[test]
    fn interrupt_core0_expands_all_77_map_entries() {
        // svd2rust leaves some register arrays unexpanded, and a missing
        // element is silent — the trace just stops naming a register. The
        // count comes from `esp32c6-0.23.2/src/interrupt.rs` (sources 0..=76,
        // mirrored by `core_0_intr_map[77]`), M3 discovery §6c.
        let maps = INTERRUPT_CORE0
            .entries
            .iter()
            .filter(|(_, name)| name.starts_with("core_0_intr_map"))
            .count();
        assert_eq!(maps, 77);
        assert_eq!(INTERRUPT_CORE0.name(0x000), Some("core_0_intr_map0"));
        assert_eq!(INTERRUPT_CORE0.name(0x130), Some("core_0_intr_map76"));
        assert_eq!(INTERRUPT_CORE0.name(4 * 43), Some("core_0_intr_map43"));
        assert_eq!(INTERRUPT_CORE0.name(4 * 57), Some("core_0_intr_map57"));
        assert_eq!(INTERRUPT_CORE0.name(4 * 30), Some("core_0_intr_map30"));
    }

    /// The element counts the discovery gives, eyeballed against what
    /// svd2rust expanded (director note 5).
    #[test]
    fn the_arrays_the_discovery_counts_are_fully_expanded() {
        let count = |t: &lp_emu_esp_common::RegNames, prefix: &str| {
            t.entries
                .iter()
                .filter(|(_, n)| n.starts_with(prefix))
                .count()
        };
        assert_eq!(count(&PLIC_MX, "mxint") - 5, 32, "32 mxintN_pri"); // + enable/type/clear/thresh/claim
        assert_eq!(count(&INTPRI, "cpu_int_pri"), 32);
        assert_eq!(count(&INTPRI, "cpu_intr_from_cpu"), 4);
        assert_eq!(count(&IO_MUX, "gpio"), 31, "all 31 pads");
        assert_eq!(count(&SYSTIMER, "trgt"), 6, "3 x hi/lo");
        assert_eq!(count(&SYSTIMER, "real_target"), 6);
        assert_eq!(count(&SYSTIMER, "comp"), 3);
        assert_eq!(count(&TIMG0, "t0."), 9, "the C6 TIMG has one timer");
        assert_eq!(count(&TIMG0, "t1."), 0);
        assert_eq!(count(&LP_WDT, "wdtconfig"), 5);
        assert_eq!(count(&ASSIST_DEBUG, "cpu0."), 28, "the cpu(0) cluster");
        assert_eq!(count(&EFUSE, "rd_mac_spi_sys_"), 6);
        assert_eq!(GPIO.name(0x03c), Some("in_"));
        assert_eq!(PCR.name(0x110), Some("sysclk_conf"));
        assert_eq!(PCR.name(0x114), Some("cpu_waiti_conf"));
        assert_eq!(LP_PERI.name(0x008), Some("rng_data"));
        assert_eq!(I2C_ANA_MST.name(0x018), Some("ana_conf0"));
        assert_eq!(LP_I2C_ANA_MST.name(0x000), Some("i2c0_ctrl"));
        assert_eq!(ASSIST_DEBUG.name(0x074), Some("cpu0.debug_mode"));
    }

    #[test]
    fn lp_clkrst_names_the_register_the_mask_rom_reads() {
        assert_eq!(LP_CLKRST.name(0x010), Some("reset_cause"));
        assert_eq!(LP_CLKRST.name(0x012), Some("reset_cause"));
        assert_eq!(LP_CLKRST.qualified_block(), "lp_clkrst");
    }

    #[test]
    fn the_interrupt_sources_are_the_77_the_pac_declares() {
        assert_eq!(INTERRUPT_SOURCES.len(), 77);
        for (i, (n, _)) in INTERRUPT_SOURCES.iter().enumerate() {
            assert_eq!(usize::from(*n), i, "dense, sorted");
        }
        assert_eq!(source::FROM_CPU_INTR0, 22);
        assert_eq!(source::TG0_T0_LEVEL, 51);
        assert_eq!(source::TG0_WDT_LEVEL, 53);
        assert_eq!(source::LP_WDT, 18);
        assert_eq!(source::GPIO, 30);
        assert_eq!(source::UART0, 43);
        assert_eq!(source::USB_DEVICE, 48);
        assert_eq!(source::RMT, 49);
        assert_eq!(source::ASSIST_DEBUG, 26);
        assert_eq!(source::SYSTIMER_TARGET2, 59);
    }
}
