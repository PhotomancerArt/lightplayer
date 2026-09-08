//! The accept-and-remember blocks: a [`RegFile`] each, with the exceptions
//! the boot path needs written down as overrides. Every override cites the
//! esp-hal line that reads or spins on it.
//!
//! The rule (`lp_emu_esp_common::regfile`): a `RegFile` never invents
//! behaviour, it only remembers, and everything it pretends about is one
//! line here. Values marked *modeled* are plausible, not measured.
//!
//! **Reset values are not among the exceptions.** `with_names` seeds every
//! block from the PAC's own `Resettable` impls, generated into `crate::regs`
//! beside the names, so a block reads what the part reads before anyone
//! writes it. Until 2026-09-08 each block carried only the resets a boot had
//! been *observed* to need and read 0 everywhere else — which is how SPI1's
//! `user` came to read 0 with `usr_command` clear and every flash read moved
//! no bytes while looking exactly like a blank chip
//! (`docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`).
//! A `with_reset` in this file is now a **deviation from the PAC**, and
//! `the_only_deviations_from_the_pacs_resets_are_the_listed_ones` below is
//! the list.

use lp_emu_esp_common::RegFile;

use crate::regs;

// ---- APM: `pre_init` writes 0 to each `func_ctrl` (soc/esp32c6/mod.rs:31-49)

pub fn lp_apm() -> RegFile {
    RegFile::new("LP_APM", 0x100).with_names(regs::LP_APM)
}

pub fn lp_apm0() -> RegFile {
    RegFile::new("LP_APM0", 0x800).with_names(regs::LP_APM0)
}

pub fn hp_apm() -> RegFile {
    RegFile::new("HP_APM", 0x800).with_names(regs::HP_APM)
}

// ---- LP_AON: `store0..7` plain RW; `store1` carries the calibration value
// the firmware writes and reads back (clock/mod.rs:518-520, :539-548).

pub fn lp_aon() -> RegFile {
    RegFile::new("LP_AON", 0x400).with_names(regs::LP_AON)
}

// ---- PMU: written by `rtc::init`, never read back (discovery §4).

pub fn pmu() -> RegFile {
    RegFile::new("PMU", 0x400).with_names(regs::PMU)
}

/// `LP_CLKRST`. `reset_cause` at `+0x10` is the very first MMIO access of
/// every boot, from inside the mask ROM's `rtc_get_reset_reason`
/// (`lw a0, 0x410(a5); andi a0, a0, 31`), and `__pre_init` zeroes
/// `.rtc_fast.persistent` iff it returns 1 (`POWERON`). Seeded to 1: the
/// direct-load machine asserts a power-on (`loader.rs`, item 7). See
/// `tests/rom_reset_reason.rs` for both halves pinned.
///
/// `lp_clk_conf` resets to `0x04` (`slow_clk_sel = 0`, RC_SLOW), which
/// `RtcSlowClockSource::current()` reads (`rtc/esp32c6.rs:126`) — that one
/// comes from the PAC now, like the rest of the block.
///
/// `reset_cause` is the deviation. The PAC's reset is `0x20`; this machine
/// asserts a cause instead, because the cause is an input to the run and
/// not a property of the part. `rtc_get_reset_reason` masks to five bits
/// (`andi a0, a0, 31`), so the `0x20` the PAC states never reaches the
/// comparison either way.
pub fn lp_clkrst(cause: crate::loader::ResetCause) -> RegFile {
    RegFile::new("LP_CLKRST", 0x400)
        .with_names(regs::LP_CLKRST)
        .with_reset(0x010, cause.rom_code())
}

/// `SLC`, the SDIO slave DMA controller. Nothing in any image this machine
/// runs is an SDIO slave — but the **mask ROM** reads one word of it while
/// deciding whether the SPI download path is available:
///
/// ```text
/// 40018772 <ets_spi_download_disabled>:
/// 40018772:  lui  a5, 0x60017
/// 40018776:  lw   a5, 504(a5)     ; SLC + 0x1f8
/// 4001877a:  bnez a5, +0x18       ; non-zero -> consult eFuse
/// ```
///
/// The PAC's `slc` block ends at `+0x17c`, so `+0x1f8` is unnamed; it reads
/// **0** here, which is the RegFile's own reset and takes the ROM down the
/// short arm. That the boot then reaches the flash bootloader and prints
/// silicon's log line for line is the evidence for the value.
pub fn slc() -> RegFile {
    RegFile::new("SLC", 0x1000).with_names(regs::SLC)
}

/// `HINF`, the SDIO slave host interface. Two read-modify-writes from
/// `sdio_slv_hal_init_device_id` (`0x40020fd4`), which the ROM's startup
/// calls unconditionally: it stamps `0x100d` into the low half of `+0x00`
/// and `0x200d` into the low half of `+0x40` — a device id for a slave
/// nothing will ever enumerate. Accept-and-remember; the ROM reads back only
/// what it wrote.
/// `LP_ANA` — the LP analog block: brownout detector and the LP power
/// switches. The **second-stage bootloader** reaches it before it loads a
/// segment (`bootloader_init` → the BOD configuration at `0x4086ff60`),
/// read-modify-writing `+0x04` and `+0x0c`. Nothing reads a status bit out
/// of it, so accept-and-remember is the whole model; the brownout it
/// configures cannot happen on a machine with no analog supply.
pub fn lp_ana() -> RegFile {
    RegFile::new("LP_ANA", 0x400).with_names(regs::LP_ANA)
}

pub fn hinf() -> RegFile {
    RegFile::new("HINF", 0x1000).with_names(regs::HINF)
}

/// `PLIC_UX`, the user-mode half of the interrupt controller. The C6's
/// `tee_enabled()` is a `const false`, so nothing this machine runs uses
/// user mode — but the **mask ROM** writes the last word of this aperture
/// (`0x2000_17fc`) in `_init`, three instructions after it writes the
/// machine-mode one, and a strict boot stops there. Accept-and-remember with
/// the PAC's names; every register in it is *modeled*, and no image has ever
/// read one back.
pub fn plic_ux() -> RegFile {
    RegFile::new("PLIC_UX", 0x400).with_names(regs::PLIC_UX)
}

// ---- MODEM_*: written by `modem_clock_*`, never read back.

pub fn modem_syscon() -> RegFile {
    RegFile::new("MODEM_SYSCON", 0x100).with_names(regs::MODEM_SYSCON)
}

pub fn modem_lpcon() -> RegFile {
    RegFile::new("MODEM_LPCON", 0x100).with_names(regs::MODEM_LPCON)
}

// `I2C_ANA_MST` was an accept block here from P5 until 2026-09-08, and it
// is the reason the defect it closed has a class of its own. The block is a
// transaction port onto two hundred analog registers; a `RegFile` gave it
// **one** `data` byte, so `regi2c_read(block, reg)` answered with whatever
// was last written to any other pair. `super::i2c_ana_mst` gives it the
// `{block, register}` store the drivers and the mask ROM both address, and
// carries the two read overrides (`busy` 0, `cal_done` 1) unchanged.
//
// `docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`

/// `LP_I2C_ANA_MST` — the fourth spin site, found the hard way on the bench
/// (director note 6, `docs/defects/2026-09-06-c6-analog-master-wedges-the-
/// bootloader.md`): the bootloader's BBPLL path spins on `I2C0_CTRL` bit 25
/// (`I2C0_BUSY`) in `regi2c_ctrl_write_reg_mask`, and a chip whose LP
/// domain holds it high wedges exactly like silicon did on 2026-09-06. It
/// reads **0** here so the ROM-up boot (M7) does not.
pub fn lp_i2c_ana_mst() -> RegFile {
    RegFile::new("LP_I2C_ANA_MST", 0x400)
        .with_names(regs::LP_I2C_ANA_MST)
        .with_read_override(0x000, 1 << 25, 0)
}

/// `PCR` — the clock and reset controller. The `disable_peripherals` sweep
/// and the clock-tree apply write it; three registers are read:
///
/// - `sysclk_conf.clk_xtal_freq` (`+0x110` bits 24:30) must read **40**:
///   `xtal_clk_frequency` derives every clock from it, and
///   `SystemTimer::ticks_per_second` is `xtal * 10 / 25`
///   (`timer/systimer.rs:186-208`). The PAC reset value `0x2800_0200`
///   already says 40; the override keeps it 40 whatever is written.
/// - `cpu_waiti_conf.cpu_wait_mode_force_on` (`+0x114` bit 3) reads **0**
///   (the brief's override; `interrupt/riscv.rs:409-412`, read by
///   `wait_for_interrupt` only when a debugger is connected, which
///   `ASSIST_DEBUG` says it is not). The PAC reset value is `0x0d`.
/// - `timergroup(n).timer_clk_conf` resets to `0x0040_0000` (`timer_clk_en`
///   set, `timer_clk_sel = 0` = XTAL), the source [`super::timg`] assumes.
pub fn pcr() -> RegFile {
    RegFile::new("PCR", 0x1000)
        .with_names(regs::PCR)
        .with_read_override(0x110, 0x7f << 24, 40 << 24)
        .with_read_override(0x114, 1 << 3, 0)
}

/// `LP_TIMER` at `0x600B_0C00`: the block *after* EFUSE. The vendor
/// emulator's trace attributed `0x410/0x414/0x418/0x440/0x444` to EFUSE;
/// EFUSE's window is `0x400`, so those are `LP_TIMER + 0x10/0x14/0x18/0x40/
/// 0x44` (`update`, `main_buf0_low/high`, and two more), accepted here
/// under their own names rather than as "undocumented eFuse offsets".
pub fn lp_timer() -> RegFile {
    RegFile::new("LP_TIMER", 0x400).with_names(regs::LP_TIMER)
}

pub fn apb_saradc() -> RegFile {
    RegFile::new("APB_SARADC", 0x400).with_names(regs::APB_SARADC)
}

/// `ASSIST_DEBUG`. `cpu(0).debug_mode` at `+0x74`: bit 1
/// (`debug_module_active`) must read **0** — `debugger_connected()`
/// (`debugger.rs:8-14`) reads it, and a 1 makes `set_stack_watchpoint`
/// silently no-op (`:32-34`) and `wait_for_interrupt` skip `wfi`
/// (`interrupt/riscv.rs:433-438`). `rcd_*` and the monitors are accepted;
/// nothing in esp-hal 1.1.1 or esp-rtos 0.3.0 drives them (discovery §4e).
pub fn assist_debug() -> RegFile {
    RegFile::new("ASSIST_DEBUG", 0x400)
        .with_names(regs::ASSIST_DEBUG)
        .with_read_override(0x074, 0b11, 0)
}

pub fn hp_sys() -> RegFile {
    RegFile::new("HP_SYS", 0x400).with_names(regs::HP_SYS)
}

pub fn tee() -> RegFile {
    RegFile::new("TEE", 0x1000).with_names(regs::TEE)
}

pub fn lp_tee() -> RegFile {
    RegFile::new("LP_TEE", 0x100).with_names(regs::LP_TEE)
}

pub fn lp_io() -> RegFile {
    RegFile::new("LP_IO", 0x400).with_names(regs::LP_IO)
}

/// `EXTMEM` — the cache controller.
///
/// The application never waits for this block. The **mask ROM** does, four
/// times, and each one is a `set the enable, then spin until done` pair:
///
/// ```text
/// Cache_Invalidate_ICache_Items  +0x098 bit 4  cache_sync_done
/// Cache_Lock/Unlock_ICache_Items +0x088 bit 2  cache_lock_done
/// Cache_Suspend_ICache           +0x0d8 bit 1  l1_cache_preload_done
/// Cache_Suspend_ICache_Autoload  +0x134 bit 1  l1_cache_autoload_done
/// ```
///
/// e.g. `lw a5,308(a4); andi a5,a5,2; beqz a5,-4` at `0x40027780`. An accept
/// block that only remembered writes holds every one of those at zero, and
/// the ROM-up boot hangs in the first of them the instant after its banner.
///
/// So the four `done` bits read **1**, and the enables that the PAC says
/// "will be cleared by hardware after the operation is done"
/// (`cache_invalidate/clean/writeback/writeback_invalidate_ena`,
/// `cache_lock_ena`, `cache_unlock_ena`, `l1_cache_preload_ena`) read
/// **0**. Together those two are one statement: *the operation the guest
/// just asked for has already finished* — which is the truthful reading of
/// a machine with no cache timing model, and the only reading that is
/// self-consistent (a `done` that is always 1 beside an `ena` that is
/// always 1 describes a cache that is permanently mid-operation).
///
/// `l1_cache_autoload_ena` is **not** in that list: the PAC calls it a
/// plain enable/disable and the ROM reads it back to decide whether to
/// restore autoload later, so it is stored like any other bit.
pub fn extmem() -> RegFile {
    RegFile::new("EXTMEM", 0x400)
        .with_names(regs::EXTMEM)
        // cache_lock_ctrl: lock_ena, unlock_ena clear; lock_done sets.
        .with_read_override(0x088, 0b111, 0b100)
        // cache_sync_ctrl: the four sync enables clear; sync_done sets.
        .with_read_override(0x098, 0b1_1111, 0b1_0000)
        // l1_cache_preload_ctrl: preload_ena clears; preload_done sets.
        .with_read_override(0x0d8, 0b11, 0b10)
        // l1_cache_autoload_ctrl: autoload_done sets; ena is stored.
        .with_read_override(0x134, 0b10, 0b10)
        // l1_cache_freeze_ctrl: freeze_done (bit 18) MIRRORS freeze_ena
        // (bit 16). `Cache_Freeze_ICache_Enable` spins until it is 1 and
        // `Cache_Freeze_ICache_Disable` spins until it is 0, so no constant
        // works; see `RegFile::with_read_mirror`.
        .with_read_mirror(0x02c, 1 << 16, 1 << 18)
}

// `uart(name)` was an accept block here from P5 — so the ROM's
// `uart_tx_one_char` would not fault and the no-radio image would boot —
// until P6 gave both instances a model with FIFOs, a shifter at baud and
// `RXFIFO_TOUT` (`super::uart`). The machine has mapped `uart::Uart` at
// UART0 and UART1 ever since; the accept block was left behind, unmapped,
// and is retired here rather than swept.
//
// `USB_DEVICE` was one too, reading 0 everywhere — the *host absent, FIFO
// full* reading: `ep1_conf.serial_in_ep_data_free` 0, `int_raw.sof` 0. Two
// of the three claims in that sentence are things the PAC contradicts
// (`ep1_conf` resets to `0x02`, `int_raw` to `0x08`), which the reset sweep
// makes visible and `super::usb_sj` makes moot: it computes `ep1_conf`,
// `int_raw` and both `ep_st` registers from the link's own state, so the
// stored word is never what a guest reads. M6 owns that block.

/// `IO_MUX`: `pin_ctrl` plus all **31** pads at `+0x04 + 4n` (the PAC's
/// `gpio: [GPIO; 31]`; the vendor emulator stopped at 15). Each pad resets
/// to `0x0800` and `pin_ctrl` to `0x1def` — from the generated table, which
/// expands the array the same way the name table does.
pub fn io_mux() -> RegFile {
    RegFile::new("IO_MUX", 0x100).with_names(regs::IO_MUX)
}

// `GPIO` was an accept block here from P5 (`in_` and `pcpu_int` reading 0,
// `out`/`enable` remembered) until M5 P2 made it a routing view over the
// bus's signal fabric: `super::gpio`. Both read-zero overrides moved with
// it, unchanged.

// `RMT` was an accept block here from P5 (its `sys_conf` write from
// `esp_hal::rmt::Rmt::new` was P5's first strict stop) until M5 P1 gave it a
// model: `super::rmt`. M4's upload walk had already found the two edges of
// what an accept block could not do for it — the channel RAM at `+0x400`
// past the end of the mapped window, and, once that was widened,
// `Ws281xOutput::write` spinning to its 50 ms deadline because completion
// arrives as the RMT **interrupt** and a register file raises none.
//
// `SPI0`/`SPI1` were accept blocks here until M4 gave them models
// (`super::spi0`, `super::spi1`): a flash access used to spin on `SPI1.cmd`,
// which is what every flash-backed image stopped on at 11 ms.

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    #[test]
    fn the_spin_bits_read_the_way_the_discovery_says_whatever_was_written() {
        let mut sb = Sandbox::new();
        // `I2C_ANA_MST`'s own spin bits moved to `super::i2c_ana_mst` with
        // the block; `LP_I2C_ANA_MST` is still an accept block.
        let mut lp = lp_i2c_ana_mst();
        sb.write(&mut lp, 0x000, 0xffff_ffff);
        assert_eq!(sb.read(&mut lp, 0x000) & (1 << 25), 0, "I2C0_BUSY");

        let mut p = pcr();
        assert_eq!((sb.read(&mut p, 0x110) >> 24) & 0x7f, 40);
        sb.write(&mut p, 0x110, 0);
        assert_eq!((sb.read(&mut p, 0x110) >> 24) & 0x7f, 40, "pinned");
        assert_eq!(
            sb.read(&mut p, 0x114) & (1 << 3),
            0,
            "cpu_wait_mode_force_on"
        );
        assert_eq!(sb.read(&mut p, 0x040), 0x0040_0000);

        let mut a = assist_debug();
        sb.write(&mut a, 0x074, 0b11);
        assert_eq!(sb.read(&mut a, 0x074), 0, "no debugger");
        assert_eq!(a.reg_name(0x074), Some("cpu0.debug_mode"));

        let mut c = lp_clkrst(crate::loader::ResetCause::PowerOn);
        assert_eq!(sb.read(&mut c, 0x010) & 0x1f, 1, "POWERON");
        assert_eq!(sb.read(&mut c, 0x000) & 0b11, 0, "RC_SLOW");
        // The other cause the ROM's banner can print, from the same seed.
        let mut c = lp_clkrst(crate::loader::ResetCause::UsbUartHpSys);
        assert_eq!(sb.read(&mut c, 0x010) & 0x1f, 0x15, "USB_UART_HPSYS");

        // EXTMEM's four `done` bits, and the freeze mirror the ROM waits
        // for in both directions.
        let mut e = extmem();
        sb.write(&mut e, 0x098, 0b1);
        assert_eq!(sb.read(&mut e, 0x098), 0b1_0000, "sync done, ena cleared");
        sb.write(&mut e, 0x088, 0b10);
        assert_eq!(sb.read(&mut e, 0x088), 0b100, "lock done, ena cleared");
        sb.write(&mut e, 0x0d8, 0b1);
        assert_eq!(sb.read(&mut e, 0x0d8), 0b10, "preload done, ena cleared");
        sb.write(&mut e, 0x134, 0b1);
        assert_eq!(sb.read(&mut e, 0x134), 0b11, "autoload done, ena stored");
        assert_eq!(sb.read(&mut e, 0x02c), 0, "not frozen, not freeze-done");
        sb.write(&mut e, 0x02c, 1 << 16);
        assert_eq!(sb.read(&mut e, 0x02c), (1 << 16) | (1 << 18), "freeze done");
        sb.write(&mut e, 0x02c, 0);
        assert_eq!(sb.read(&mut e, 0x02c), 0, "unfreeze done");
    }

    #[test]
    fn io_mux_has_thirty_one_pads() {
        let mut sb = Sandbox::new();
        let mut m = io_mux();
        for pad in 0..31u32 {
            assert_eq!(sb.read(&mut m, 0x004 + 4 * pad), 0x0800);
        }
        assert_eq!(sb.read(&mut m, 0x000), 0x1def, "pin_ctrl");
        assert_eq!(m.reg_name(0x07c), Some("gpio30"));
        assert_eq!(m.reg_name(0x080), None, "there is no pad 31");
        // GPIO's own reads are `super::gpio`'s tests now.
    }

    /// The accept blocks, each with the generated table it is built from.
    fn every_block() -> Vec<(RegFile, lp_emu_esp_common::regnames::RegNames)> {
        vec![
            (lp_apm(), regs::LP_APM),
            (lp_apm0(), regs::LP_APM0),
            (hp_apm(), regs::HP_APM),
            (lp_aon(), regs::LP_AON),
            (pmu(), regs::PMU),
            (
                lp_clkrst(crate::loader::ResetCause::PowerOn),
                regs::LP_CLKRST,
            ),
            (slc(), regs::SLC),
            (lp_ana(), regs::LP_ANA),
            (hinf(), regs::HINF),
            (plic_ux(), regs::PLIC_UX),
            (modem_syscon(), regs::MODEM_SYSCON),
            (modem_lpcon(), regs::MODEM_LPCON),
            (lp_i2c_ana_mst(), regs::LP_I2C_ANA_MST),
            (pcr(), regs::PCR),
            (lp_timer(), regs::LP_TIMER),
            (apb_saradc(), regs::APB_SARADC),
            (assist_debug(), regs::ASSIST_DEBUG),
            (hp_sys(), regs::HP_SYS),
            (tee(), regs::TEE),
            (lp_tee(), regs::LP_TEE),
            (lp_io(), regs::LP_IO),
            (extmem(), regs::EXTMEM),
            (io_mux(), regs::IO_MUX),
        ]
    }

    /// Where an accept block's power-on state differs from what the PAC
    /// states, and why. Anything not on this list is a bug in the seeding
    /// or an undocumented hand exception — the whole point of the sweep
    /// (`docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`).
    ///
    /// `(block, offset, what this machine reads instead, why)`.
    const DEVIATIONS: &[(&str, u32, u32, &str)] = &[(
        "LP_CLKRST",
        0x010,
        1,
        "reset_cause is an input to the run, not a property of the part: the machine          asserts a power-on and the ROM masks to five bits",
    )];

    #[test]
    fn the_only_deviations_from_the_pacs_resets_are_the_listed_ones() {
        let mut unlisted = Vec::new();
        for (block, names) in every_block() {
            let name = Peripheral::name(&block);
            for (off, _) in names.entries {
                if *off >= block.len_bytes() {
                    // Not mapped by this machine: reads 0 for the guest
                    // whatever the part does.
                    continue;
                }
                let want = names.reset(*off).unwrap_or(0);
                let got = block.stored(*off);
                if got == want {
                    continue;
                }
                match DEVIATIONS
                    .iter()
                    .find(|(b, o, _, _)| *b == name && o == off)
                {
                    Some((_, _, expected, _)) => assert_eq!(
                        got, *expected,
                        "{name}+{off:#05x} is a listed deviation, but it reads {got:#010x}                          rather than the {expected:#010x} the list says"
                    ),
                    None => unlisted.push(format!(
                        "  {name}+{off:#05x} {}: reads {got:#010x}, the PAC says {want:#010x}",
                        names.name(*off).unwrap_or("?")
                    )),
                }
            }
        }
        assert!(
            unlisted.is_empty(),
            "these accept-block registers do not read what the PAC says, and are not on \
             DEVIATIONS:\n{}",
            unlisted.join("\n")
        );
    }

    /// The other half: every listed deviation is real. A stale entry would
    /// otherwise sit here excusing something that no longer happens.
    #[test]
    fn every_listed_deviation_is_one() {
        for (name, off, _, why) in DEVIATIONS {
            let (block, names) = every_block()
                .into_iter()
                .find(|(b, _)| Peripheral::name(b) == *name)
                .unwrap_or_else(|| panic!("DEVIATIONS names `{name}`, which is not a block"));
            assert!(!why.is_empty(), "{name}+{off:#05x} has no reason");
            assert_ne!(
                block.stored(*off),
                names.reset(*off).unwrap_or(0),
                "{name}+{off:#05x} agrees with the PAC now; take it off the list"
            );
        }
    }

    #[test]
    fn the_blocks_carry_their_names() {
        assert_eq!(lp_apm().reg_name(0x0c4), Some("func_ctrl"));
        assert_eq!(
            lp_aon().reg_name(0x0c4).is_some(),
            regs::LP_AON.name(0x0c4).is_some()
        );
        assert_eq!(pmu().reg_name(0x000).is_some(), true);
        assert_eq!(lp_timer().reg_name(0x010).is_some(), true);
        assert_eq!(extmem().reg_name(0x000).is_some(), true);
    }
}
