//! Generated register-name tables for the classic ESP32's blocks.
//!
//! A bus log that says `DPORT+0x040` has to be decoded by hand against a PAC;
//! one that says `DPORT+0x040 pro_cache_ctrl` can be read. The tables are
//! **generated** from the `esp32` PAC's svd2rust offset comments by
//! `scripts/emu/pac-regnames.py --pac esp32`, carry the provenance header
//! `docs/adr/2026-07-29-license-provenance-discipline.md` requires, and are
//! checked by `just lint-emu-regnames` — which checks **both** chips, so a
//! hand edit here is caught the same way one in the C6's tables is.
//!
//! They live in this crate, not in `lp-emu-esp-common`, because a register
//! layout is chip-family data and that crate holds no chip numbers.
//!
//! This module itself — the `mod`/`pub use` list and the tests below — is
//! hand-written, like the C6's. The generator writes one file per block; it
//! does not write the module that gathers them, because that module carries
//! prose and assertions no generator could produce.
//!
//! # Three tables serve more than one peripheral
//!
//! The PAC gives several peripherals one `RegisterBlock` type each, so one
//! generated table is the layout at several bases:
//!
//! - [`SPI0`] is SPI0 (`0x3FF4_3000`), SPI1 (`0x3FF4_2000`), SPI2 and SPI3;
//! - [`TIMG0`] is TIMG0 (`0x3FF5_F000`) and TIMG1 (`0x3FF6_0000`);
//! - [`UART0`] is UART0 (`0x3FF4_0000`), UART1 and UART2.
//!
//! # `RNG` is on the AHB bus, and P1 was wrong to exclude it
//!
//! `esp32-0.40.2/src/lib.rs:647` gives `RNG` the base `0x6003_5000`. P1 read
//! that as an SVD leak from another family and put it in the generator's
//! `SKIP` list; P3's fifth strict stop found the classic's **second
//! peripheral window** — the AHB bus at `0x6000_0000`, a mirror of the DPORT
//! blocks from `0x3FF4_0000` up (`crate::memmap::MMIO_AHB_BASE` has the
//! evidence) — and `0x6003_5000` is the AHB address of the WiFi window's
//! WDEV block: [`RNG`]'s `data` at `+0x144` is `0x6003_5144`, the classic's
//! `WDEV_RND_REG`. The table is generated like every other, and the `SKIP`
//! entry is gone.
//!
//! # One thing the generator cannot produce, and does not pretend to
//!
//! **The flash MMU page tables are not in [`DPORT`].** They are raw 256-entry
//! `u32` arrays at `0x3FF1_0000` (PRO) and `0x3FF1_2000` (APP)
//! ([`crate::memmap::FLASH_MMU_PRO`]) — inside the DPORT window but past the
//! end of the register block svd2rust generates. [`DPORT`]'s `immu_table0` /
//! `dmmu_table0` at `+0x504` / `+0x544` are a **different** thing, the 16-entry
//! internal-SRAM MMU. P4 declares the flash tables by hand from the ROM's own
//! `Cache_Flash_MMU_Set`, never from a datasheet.

mod apb_ctrl;
mod dport;
mod efuse;
mod frc_timer;
mod gpio;
mod i2s0;
mod io_mux;
mod rmt;
mod rng;
mod rtc_cntl;
mod rtc_i2c;
mod rtc_io;
mod sens;
mod sha;
mod spi0;
mod timg0;
mod uart0;

pub use apb_ctrl::APB_CTRL;
pub use dport::DPORT;
pub use efuse::EFUSE;
pub use frc_timer::FRC_TIMER;
pub use gpio::GPIO;
pub use i2s0::I2S0;
pub use io_mux::IO_MUX;
pub use rmt::RMT;
pub use rng::RNG;
pub use rtc_cntl::RTC_CNTL;
pub use rtc_i2c::RTC_I2C;
pub use rtc_io::RTC_IO;
pub use sens::SENS;
pub use sha::SHA;
pub use spi0::SPI0;
pub use timg0::TIMG0;
pub use uart0::UART0;

/// Every table, for the tests that sweep them.
pub const ALL: &[&lp_emu_esp_common::RegNames] = &[
    &APB_CTRL, &DPORT, &EFUSE, &FRC_TIMER, &GPIO, &IO_MUX, &RMT, &RNG, &RTC_CNTL, &RTC_I2C,
    &RTC_IO, &SENS, &SHA, &SPI0, &TIMG0, &UART0,
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

    /// The DPORT offsets M3's discovery read out of `esp32-0.40.2/src/dport.rs`
    /// by hand (`m3/notes.md` §4). The generator read the same file from the
    /// other end; this is the two agreeing.
    #[test]
    fn dport_names_the_registers_the_boot_uses() {
        assert_eq!(DPORT.name(0x000), Some("pro_boot_remap_ctrl"));
        assert_eq!(DPORT.name(0x004), Some("app_boot_remap_ctrl"));
        // Core 1: reset, clock gate, and the runstall M3 keeps asserted.
        assert_eq!(DPORT.name(0x02c), Some("appcpu_ctrl_a"));
        assert_eq!(DPORT.name(0x030), Some("appcpu_ctrl_b"));
        assert_eq!(DPORT.name(0x034), Some("appcpu_ctrl_c"));
        assert_eq!(DPORT.name(0x038), Some("appcpu_ctrl_d"));
        assert_eq!(DPORT.name(0x03c), Some("cpu_per_conf"));
        // D4's register: `pro_cache_enable` is bit 3 of this word.
        assert_eq!(DPORT.name(0x040), Some("pro_cache_ctrl"));
        assert_eq!(DPORT.name(0x044), Some("pro_cache_ctrl1"));
        assert_eq!(DPORT.name(0x058), Some("app_cache_ctrl"));
        assert_eq!(DPORT.name(0x05c), Some("app_cache_ctrl1"));
        assert_eq!(DPORT.name(0x07c), Some("cache_mux_mode"));
        assert_eq!(DPORT.name(0x0c0), Some("perip_clk_en"));
        assert_eq!(DPORT.name(0x0c4), Some("perip_rst_en"));
        // The internal-SRAM MMU, NOT the flash MMU — see the module docs.
        assert_eq!(DPORT.name(0x504), Some("immu_table0"));
        assert_eq!(DPORT.name(0x544), Some("dmmu_table0"));
    }

    /// The per-core interrupt maps and the four software interrupts. A map
    /// entry the generator failed to expand would be silent — the trace would
    /// just stop naming a register — so the count is asserted, not eyeballed.
    #[test]
    fn dport_expands_both_cores_interrupt_maps() {
        let count = |prefix: &str| {
            DPORT
                .entries
                .iter()
                .filter(|(_, name)| name.starts_with(prefix))
                .count()
        };
        assert_eq!(count("core_0_intr_map"), 69);
        assert_eq!(count("core_1_intr_map"), 69);
        assert_eq!(DPORT.name(0x104), Some("core_0_intr_map0"));
        assert_eq!(DPORT.name(0x218), Some("core_1_intr_map0"));
        // swi1 is the wire-pusher doorbell and swi2 the io_task executor;
        // both are `cpu_intr_from_cpu` registers (`m3/notes.md` §4).
        assert_eq!(count("cpu_intr_from_cpu"), 4);
    }

    /// UART0's first six registers are at the same offsets as the C6's, and
    /// everything from `+0x18` diverges. The classic has no `CLK_CONF`, no
    /// `REG_UPDATE` and no `TOUT_CONF`; it selects its clock through
    /// `conf0.tick_ref_always_on` and `clkdiv` (`m3/notes.md` §5).
    #[test]
    fn uart0_is_the_classics_layout_not_the_c6s() {
        assert_eq!(UART0.name(0x000), Some("fifo"));
        assert_eq!(UART0.name(0x004), Some("int_raw"));
        assert_eq!(UART0.name(0x010), Some("int_clr"));
        assert_eq!(UART0.name(0x014), Some("clkdiv"));
        assert_eq!(UART0.name(0x018), Some("autobaud"));
        assert_eq!(UART0.name(0x01c), Some("status"));
        assert_eq!(UART0.name(0x020), Some("conf0"));
        assert_eq!(UART0.name(0x024), Some("conf1"));
        for absent in ["clk_conf", "reg_update", "tout_conf"] {
            assert!(
                !UART0.entries.iter().any(|(_, n)| *n == absent),
                "`{absent}` is a C6 register and must not appear here"
            );
        }
    }

    /// LACT is the classic's own clock source (P5) and it is a whole register
    /// family TIMG0 carries and the C6's TIMG does not.
    #[test]
    fn timg0_carries_lact_and_two_timers() {
        let count = |prefix: &str| {
            TIMG0
                .entries
                .iter()
                .filter(|(_, name)| name.starts_with(prefix))
                .count()
        };
        assert!(count("lact") >= 10, "the LACT register family");
        assert!(count("t0") > 0 && count("t1") > 0, "the classic has two");
    }

    /// The 40-pad fabric (P8). The C6 has 31; a table that quietly stopped at
    /// 31 here would lose the pads the strip and the buttons are on.
    #[test]
    fn the_gpio_matrix_has_forty_pads() {
        let outs = GPIO
            .entries
            .iter()
            .filter(|(_, n)| n.starts_with("func") && n.ends_with("_out_sel_cfg"))
            .count();
        assert_eq!(outs, 40);
        assert!(GPIO.entries.iter().any(|(_, n)| *n == "func39_out_sel_cfg"));
    }

    /// The classic SHA block: one `text` window that is both the message
    /// block and the digest read-back, and a register quad per mode rather
    /// than one `MODE` register (`m3/notes.md` §7).
    #[test]
    fn sha_has_one_text_window_and_a_quad_per_mode() {
        assert_eq!(SHA.name(0x000), Some("text0"));
        assert_eq!(SHA.name(0x07c), Some("text31"));
        for reg in [
            "sha1_start",
            "sha1_continue",
            "sha1_load",
            "sha1_busy",
            "sha256_start",
            "sha256_load",
            "sha512_busy",
        ] {
            assert!(
                SHA.entries.iter().any(|(_, n)| *n == reg),
                "`{reg}` is missing from the classic's SHA table"
            );
        }
        // The C6's separate message and digest memories are NOT here.
        for absent in ["mode", "m_mem", "h_mem"] {
            assert!(!SHA.entries.iter().any(|(_, n)| *n == absent));
        }
    }

    /// RTC_CNTL holds both halves of the CPU stall key and the reset causes
    /// the ROM's `rtc_get_reset_reason` reads (`m3/notes.md` §4).
    #[test]
    fn rtc_cntl_names_the_stall_key_and_the_watchdog() {
        assert_eq!(RTC_CNTL.name(0x000), Some("options0"));
        for reg in [
            "sw_cpu_stall",
            "wdtconfig0",
            "wdtfeed",
            "wdtwprotect",
            "store0",
        ] {
            assert!(
                RTC_CNTL.entries.iter().any(|(_, n)| *n == reg),
                "`{reg}` is missing from RTC_CNTL"
            );
        }
    }

    /// Every table names the block the emulator will register it under, so a
    /// trace line can be attributed without a second lookup.
    #[test]
    fn the_block_names_are_the_pac_module_names() {
        let blocks: Vec<&str> = ALL.iter().map(|t| t.block).collect();
        for expected in [
            "apb_ctrl",
            "dport",
            "efuse",
            "frc_timer",
            "gpio",
            "io_mux",
            "rmt",
            "rng",
            "rtc_cntl",
            "rtc_i2c",
            "rtc_io",
            "sens",
            "sha",
            "spi0",
            "timg0",
            "uart0",
        ] {
            assert!(blocks.contains(&expected), "`{expected}` has no table");
        }
        assert_eq!(blocks.len(), 16);
    }

    /// `RNG` is one register on the AHB bus: `data` at `+0x144`, which at
    /// the PAC's base `0x6003_5000` is `0x6003_5144` — `WDEV_RND_REG`. P1
    /// excluded the block as an SVD leak; P3 found the bus (module docs).
    #[test]
    fn rng_is_wdev_rnd_reg_on_the_ahb_bus() {
        assert_eq!(RNG.name(0x144), Some("data"));
        assert_eq!(crate::memmap::periph::RNG + 0x144, 0x6003_5144);
    }
}
