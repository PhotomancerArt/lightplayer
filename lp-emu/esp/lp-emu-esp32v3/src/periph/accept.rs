//! The accept-and-remember blocks: a [`RegFile`] each, seeded from the PAC's
//! reset values, with the exceptions the boot path needs written down as
//! overrides — **each carrying its evidence beside it.**
//!
//! # The two rules this file exists to hold
//!
//! **A reset value comes from the PAC's `Resettable`.** [`RegFile::with_names`]
//! seeds every register of a block from the generated `regs/<block>.rs`
//! table, so a block reads what the part reads before anyone writes it. A
//! `with_reset` in this file is a **deviation from the PAC**, and the test
//! `the_only_deviations_from_the_pacs_resets_are_the_listed_ones` is the
//! list. The sweep that produced the rule is
//! `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`:
//! an accept block seeded with the value a spin happened to want looks
//! exactly like one seeded from the PAC, right up to the moment a different
//! image spins on a different value.
//!
//! **An exception carries its evidence beside it.** Not "the boot spins
//! here", but *why the value is what it is*: a ROM disassembly line, a PAC
//! field doc, a linker constant, an esp-hal source line. A spin no pin table
//! can justify is an **E-premise stop** — reported in the phase report, not
//! answered with an invented value.
//!
//! # Which phase owns which block
//!
//! Every block here is a probe. The phase that gives it behaviour is named
//! on the block, and the order the strict run needed them in is the ledger
//! in `docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`.

use lp_emu_esp_common::RegFile;

use crate::regs;

/// `DPORT`'s aperture: the generated table runs to `+0xffc` (`date`), and
/// the next block (`AES`) is at `+0x1000`. The flash MMU page tables at
/// `0x3FF1_0000` / `0x3FF1_2000` are **not** inside it — they are raw arrays
/// P4 declares from the ROM's own `cache_flash_mmu_set`
/// (`crate::memmap::FLASH_MMU_PRO`).
pub const DPORT_LEN: u32 = 0x1000;

/// `DPORT` — **P4's block**, accept-and-remember here.
///
/// The first strict stop of the direct load: twenty-nine instructions in,
/// `esp_hal::soc::xtensa::esp32_init` → `interrupt::setup_interrupts` starts
/// clearing the APP core's interrupt map (`core_1_intr_map[0]` at `+0x218`,
/// `Write Word 0x3ff00218` at cycle 29). Everything `esp_hal::init` does to
/// this block on the way to `main` — both cores' interrupt maps, the
/// peripheral clock/reset gates (`perip_clk_en`/`perip_rst_en`, `+0xc0`/
/// `+0xc4`), `cpu_per_conf` for `CpuClock::max()`, the software interrupts
/// (`cpu_intr_from_cpu[0..4]`) — is written and, where read back, read back
/// as written.
///
/// What an accept block cannot do for it, and P4 does: route
/// `core_0_intr_map[src]` writes into `CpuIntMatrix::asserted`, hold core 1
/// through `appcpu_ctrl_c.appcpu_runstall`, and give `pro_cache_ctrl.
/// pro_cache_enable` (bit 3) the cache-off fetch stop D4 defines against it.
/// Every reset value is the PAC's (`esp32-0.40.2/src/dport.rs`, 49 non-zero
/// resets); no exception is needed to reach the next stop.
pub fn dport() -> RegFile {
    RegFile::new("DPORT", DPORT_LEN)
        .with_names(regs::DPORT)
        .with_pac_grades()
}

/// `RTC_CNTL`'s aperture, **tight**: the generated table runs to `+0x13c`
/// (`date`); the PAC's next block (`RTC_IO`) is at `+0x400`. Tight so that
/// an access into the gap is a strict stop naming an undocumented offset,
/// not a silent zero.
pub const RTC_CNTL_LEN: u32 = 0x140;

/// `RTC_CNTL` — **P5's block**, accept-and-remember here.
///
/// The second strict stop of the direct load, 107,539 cycles in:
/// `rtc_get_reset_reason+0xb` (`0x400081df`, the mask ROM) reads
/// `reset_state` at `+0x34` for `esp_hal::rtc_cntl::reset_reason`
/// (`rtc_cntl/mod.rs:679-682`), which the firmware's recovery ledger turns
/// into `[RECOVERY] boot: cause=power-on`.
///
/// # The one deviation: `reset_state`, loader item 7
///
/// The PAC's reset for `+0x34` is `0x0000_3000` — both `reset_cause_*`
/// fields **zero**, because an SVD cannot know why a chip is starting. The
/// ROM masks the field per core:
///
/// ```text
/// 400081df:  l32i.n  a2, a8, 0        ; RTC_CNTL + 0x34
/// 400081e1:  extui   a2, a2, 0, 6     ; PRO: bits 5:0
/// 400081ed:  extui   a2, a2, 6, 6     ; APP: bits 11:6
/// ```
///
/// and `1` in either field is `POWERON_RESET` — the value L0's silicon
/// banner printed as `rst:0x1 (POWERON_RESET)` (`../bench.md`) and
/// `SocResetReason::ChipPowerOn` in esp-hal. The machine asserts the cause
/// ([`crate::loader::ResetCause`]) in **both** fields, because a power-on
/// resets both cores and the ROM's own `rtc_get_reset_reason(1)` would
/// otherwise answer "no reason" for the APP core. The cause is an input to
/// the run, not a property of the part; it is the listed deviation.
///
/// Everything else is the PAC's, including `wdtwprotect` resetting to the
/// write-protect key `0x50D8_3AA1` — so `esp_hal::init`'s
/// `rwdt.disable()` writes it, writes `wdtconfig0`, and reads back what it
/// wrote. P5 makes the RWDT count.
pub fn rtc_cntl(cause: crate::loader::ResetCause) -> RegFile {
    let code = cause.rom_code();
    RegFile::new("RTC_CNTL", RTC_CNTL_LEN)
        .with_names(regs::RTC_CNTL)
        .with_reset(0x034, 0x0000_3000 | code | (code << 6))
        .with_pac_grades()
}

/// `APB_CTRL`'s aperture, tight: the generated table runs to `+0x7c`
/// (`date`).
pub const APB_CTRL_LEN: u32 = 0x80;

/// `APB_CTRL` — the phase file's first-named accept candidate, and the
/// third strict stop of the direct load, 109,663 cycles in:
/// `fw_esp32v3::boot_firmware+0x29a` (the inlined `esp_hal::init` →
/// `Clocks::init`) reads `sysclk_conf` at `+0x00`, whose `pre_div_cnt`
/// field is the APB pre-divider `esp-hal`'s clock tree reads and then
/// re-writes (`soc/esp32/clocks.rs:435-437`, `modify(|_, w|
/// w.pre_div_cnt().bits(…))`); the four `*_tick_conf` registers after it are
/// written outright (`:479-529`). Nothing spins on this block and nothing
/// in the shipped image reads a bit back that hardware would have changed,
/// so accept-and-remember with the PAC's resets (`sysclk_conf` =
/// `0x0000_2000`, `xtal_tick_conf` = `0x27`, …) is the whole model. P5's
/// accept list.
pub fn apb_ctrl() -> RegFile {
    RegFile::new("APB_CTRL", APB_CTRL_LEN)
        .with_names(regs::APB_CTRL)
        .with_pac_grades()
}

/// A timer group's aperture, tight: the generated table runs to `+0xfc`
/// (`timgclk`); the PAC gives TIMG0 and TIMG1 one `RegisterBlock`, so one
/// length and one table serve both.
pub const TIMG_LEN: u32 = 0x100;

/// `TIMG0` / `TIMG1` — **P5's blocks**, accept-and-remember here.
///
/// The fourth strict stop of the direct load, 109,989 cycles in:
/// `esp_hal::clock::Clocks::measure_rtc_clock+0xb` reads `TIMG0.rtccalicfg`
/// (`+0x68`) — the RTC calibration unit, which `detect_xtal_freq`
/// (`soc/esp32/clocks.rs:135-160`) and `calibrate_rtc_slow_clock`
/// (`clock/mod.rs:506`) both drive: set `rtc_cali_max`, set
/// `rtc_cali_start`, `ets_delay_us`, then poll `rtc_cali_rdy` (bit 15) and
/// read `rtccalicfg1.rtc_cali_value`.
///
/// # This is the first thing on the direct path an accept block cannot answer
///
/// `rtc_cali_value` is a **measurement** — XTAL cycles counted over `N`
/// cycles of the calibration clock — and the two calls want two different
/// numbers (`10` cycles of RC_FAST/256 for the XTAL estimate, `1024` cycles
/// of RC_SLOW for the slow-clock period). No constant serves both, and a
/// `RegFile` cannot compute one, so **no override is carried**: the block
/// is the PAC's resets (`rtccalicfg` = `0x0001_3000`, `rtc_cali_rdy` clear)
/// and what the firmware writes. The firmware's own timeout arm
/// (`clock/mod.rs:433-440`, `#[cfg(esp32)]`: `ets_delay_us(1)` per poll,
/// `timeout_us` polls) then answers **0** — `warn!("calibration failed")`,
/// `detect_xtal_freq` picks **26 MHz** (`0.abs_diff(40) < 0.abs_diff(26)`
/// is false), and `cal_val = 0` goes into `RTC_CNTL.store1`.
///
/// That is a recorded divergence from silicon (the desk board is 40 MHz),
/// **not** an E-premise stop — every read is documented and the fallback is
/// the firmware's own — and it is P5's first job: the TIMG view on
/// `engine::timg` computes `rtc_cali_value` from the clock tree. Until then
/// the direct path spends ~320 µs + ~6.8 ms emulated in the two timeout
/// loops and boots with `XtalClkConfig::_26`. The phase report carries it.
///
/// The watchdog half is simpler: `esp_hal::init` disables both groups'
/// MWDTs (`wdtwprotect` = the key, `wdtconfig0.wdt_en` clear) and reads
/// back what it wrote; `wdtwprotect` resets to `0x50D8_3AA1` in the PAC.
/// esp-rtos's tick on `t0` and the io pacer on `t1` are P5's timers.
pub fn timg(name: &'static str) -> RegFile {
    RegFile::new(name, TIMG_LEN)
        .with_names(regs::TIMG0)
        .with_pac_grades()
}

/// The analog I2C master's aperture: eight command/status words, one per
/// `host_id` (`0x6000_E000 + 4·host_id`; the BBPLL is host 4, the highest
/// esp-idf names for this chip is 7). Tight on purpose — a ninth host is a
/// strict stop, not a silent zero — and the ROM's other literals in the
/// block (`+0x50`, `+0x5c`, `+0x80`, the PHY's `ANA_CONFIG` words) are
/// outside it until a boot reaches them.
pub const I2C_ANA_MST_LEN: u32 = 0x20;

/// The analog I2C master's register names. **Hand-written**, because the
/// `esp32` PAC has no block at this address; the layout is the mask ROM's:
///
/// ```text
/// 40004168 <rom_chip_i2c_writeReg>:          (block, host_id, reg, data)
/// 4000416b:  l32r  a9, (0x01000000)          ; bit 24: write
/// 40004177:  l32r  a9, (0x18003800)
/// 40004183:  add.n a9, a3, a9                ; + host_id
/// 4000418b:  slli  a9, a9, 2                 ; ×4 → 0x6000E000 + 4·host_id
/// 40004180:  slli  a4, a4, 8                 ; reg  << 8
/// 40004188:  slli  a8, a5, 16                ; data << 16
/// 40004197:  s32i.n a2, a9, 0                ; write the command word
/// 4000419c:  l32i.n a8, a9, 0
/// 4000419e:  bany  a8, a10(0x02000000), -5   ; spin while bit 25 (busy)
///
/// 40004110 <rom_chip_i2c_readReg>:  same word, no bit 24; after the spin,
/// 40004141:  extui a2, a2, 16, 8             ; data = bits 23:16
/// ```
///
/// So one word per host: `[7:0]` slave address, `[15:8]` register, `[23:16]`
/// data, `[24]` write, `[25]` busy.
pub static I2C_ANA_MST_NAMES: lp_emu_esp_common::regnames::RegNames =
    lp_emu_esp_common::regnames::RegNames {
        block: "i2c_ana_mst",
        entries: &[
            (0x000, "host0"),
            (0x004, "host1"),
            (0x008, "host2"),
            (0x00c, "host3"),
            (0x010, "host4_bbpll"),
            (0x014, "host5"),
            (0x018, "host6"),
            (0x01c, "host7"),
        ],
        resets: &[],
        access: &[],
    };

/// `I2C_ANA_MST` — the analog I2C master on the **AHB bus**
/// ([`crate::memmap::MMIO_AHB_BASE`]): accept-and-remember here, **P5's**
/// `{block, register}` store later.
///
/// The fifth strict stop of the direct load, 143,014 cycles in:
/// `rom_chip_i2c_writeReg+0x2f` (`0x4000_4197`) writes `0x6000_E010` —
/// `esp_hal::soc::esp32::clocks` programming the BBPLL through
/// `rom_i2c_writeReg` (`clocks.rs:222-264`, `I2C_BBPLL_*.write_reg`). It was
/// "outside every region and every declared window" until the AHB window
/// was declared; the memmap carries the evidence.
///
/// # No exception, and why that is honest for this image
///
/// The ROM's spin is on **bit 25** (busy), which the guest never writes —
/// the command word it stores has bits 24 and below only — so a block that
/// remembers what was written answers the spin with 0 on the first read.
/// Nothing is pinned. The shipped image only **writes** through this master
/// (every `regi2c` use in `clocks.rs` is a `write_reg`); a `readReg` would
/// get back bits 23:16 of the last word written to that host, which is the
/// C6's *one data register* defect in waiting
/// (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`)
/// and P5's reason to give it the `{block, register}` store the C6's
/// `i2c_ana_mst` has.
///
/// There are no PAC resets to seed: the PAC does not know this block. Every
/// register here is *modeled*.
pub fn i2c_ana_mst() -> RegFile {
    let rf = RegFile::new("I2C_ANA_MST", I2C_ANA_MST_LEN)
        .with_names(I2C_ANA_MST_NAMES)
        .with_pac_grades();
    // `with_pac_grades` calls a register with no access entry read-write and
    // therefore *documented*; nothing documents these, so every word is
    // demoted by hand to what it is.
    (0..I2C_ANA_MST_LEN / 4).fold(rf, |rf, i| {
        rf.with_grade(4 * i, lp_emu_esp_common::periph::RegGrade::Modeled)
    })
}

/// `GPIO`'s aperture, tight: the generated table runs to `+0x5cc`
/// (`func39_out_sel_cfg`).
pub const GPIO_LEN: u32 = 0x600;

/// `GPIO` — **P8's block** (the 40-pad fabric), accept-and-remember here.
///
/// The seventh strict stop of the direct load, 3,564,113 cycles in:
/// `boot_firmware+0xe02` writes `func14_in_sel_cfg` (`+0x168`) — the GPIO
/// matrix routing `U0RXD_IN` (signal 14) from pad 3, which is
/// `Uart::new(…).with_rx(peripherals.GPIO3)` in `board/esp32v3/init.rs`.
/// The matrix's `func*_in_sel_cfg` / `func*_out_sel_cfg` words, `enable`,
/// `out`, and the per-pin `pin*` words are written and read back as
/// written; every reset is the PAC's (the table carries none, so the block
/// reads 0 before it is written).
///
/// What an accept block does *not* do: `strap` (`+0x38`, read-only) reads
/// **0** — the PAC's reset for a register whose value is the board's pins at
/// reset. The mask ROM's `main` reads it fifteen times to choose its boot
/// mode, and the C6's `Gpio::new(strap_word)` is the shape P7/P8 give it
/// (the desk board boots `boot:0x13 (SPI_FAST_FLASH_BOOT)`, L0). `in_`
/// (`+0x3c`) reads 0 for the same reason: no pad is driven from outside
/// until the fabric exists.
pub fn gpio() -> RegFile {
    RegFile::new("GPIO", GPIO_LEN)
        .with_names(regs::GPIO)
        .with_pac_grades()
}

/// `EFUSE`'s aperture: the generated table runs to `+0x1fc` (`date`).
pub const EFUSE_LEN: u32 = 0x200;

/// `EFUSE` — **P5's block**, accept-and-remember here.
///
/// The first strict stop of the ROM-up path, **seven instructions after
/// the reset vector**: `_ResetHandler_efuse_check_patch` (`0x4000_FDA0`,
/// reported as `~_rtc_trigger_sw_system_reset+0x11` because both labels are
/// zero-sized) reads `blk0_rdata0`, `blk0_rdata5` and `blk0_rdata6`
/// (`+0x00`, `+0x14`, `+0x18`), parks them at `0x3FFE_1320`, and calls
/// `_reload_efuses_and_check` three times to compare — the ROM's anti-glitch
/// check on its own fuses.
///
/// Every register is the PAC's: the burned words all read **0** because an
/// SVD cannot know what a part had burned into it, and this phase does not
/// invent a MAC or a chip revision. The desk board's identity (MAC
/// `30:76:f5:ec:f6:34`, silicon v3.1, L0) is P5's `EfuseIdentity` seed, the
/// way the C6's `periph/efuse.rs` carries its board's — and the ROM-up path
/// is where it will matter first, because the ROM's clock and boot-mode
/// decisions read chip-revision bits out of `blk0_rdata3`/`blk0_rdata5`.
pub fn efuse() -> RegFile {
    RegFile::new("EFUSE", EFUSE_LEN)
        .with_names(regs::EFUSE)
        .with_pac_grades()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::ResetCause;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    /// The accept blocks, each with the generated table it is built from.
    fn every_block() -> Vec<(RegFile, lp_emu_esp_common::regnames::RegNames)> {
        vec![
            (dport(), regs::DPORT),
            (rtc_cntl(ResetCause::PowerOn), regs::RTC_CNTL),
            (apb_ctrl(), regs::APB_CTRL),
            (timg("TIMG0"), regs::TIMG0),
            (i2c_ana_mst(), I2C_ANA_MST_NAMES),
            (timg("TIMG1"), regs::TIMG0),
            (gpio(), regs::GPIO),
            (efuse(), regs::EFUSE),
        ]
    }

    /// Where an accept block's power-on state differs from what the PAC
    /// states, and why. Anything not on this list is a bug in the seeding
    /// or an undocumented hand exception — the whole point of the sweep.
    ///
    /// `(block, offset, what this machine reads instead, why)`.
    const DEVIATIONS: &[(&str, u32, u32, &str)] = &[(
        "RTC_CNTL",
        0x034,
        0x0000_3041,
        "reset_state's two reset_cause fields are an input to the run, not a property of the \
         part: the machine asserts POWERON_RESET (1) for both cores, the value the ROM's \
         rtc_get_reset_reason masks out (extui 0,6 / 6,6) and the silicon banner printed",
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
                        "{name}+{off:#05x} is a listed deviation, but it reads {got:#010x} \
                         rather than the {expected:#010x} the list says"
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

    /// The other half: every listed deviation is real, and has a reason. A
    /// stale entry would otherwise sit here excusing something that no
    /// longer happens; an entry with no reason is the rule this phase exists
    /// to hold, broken.
    #[test]
    fn every_listed_deviation_is_one_and_has_a_reason() {
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

    /// Every block carries its generated names, so a trace line is readable
    /// without a second lookup, and every block is graded.
    #[test]
    fn the_blocks_carry_their_names_and_grades() {
        for (block, names) in every_block() {
            let (off, expected) = names.entries[0];
            assert_eq!(block.reg_name(off), Some(expected));
            assert!(
                block.reg_grade(off).is_some(),
                "`{}` publishes no grade table",
                Peripheral::name(&block)
            );
        }
    }

    #[test]
    fn the_reset_cause_reads_power_on_for_both_cores() {
        let mut sb = Sandbox::new();
        let mut r = rtc_cntl(ResetCause::PowerOn);
        assert_eq!(r.reg_name(0x034), Some("reset_state"));
        let word = sb.read(&mut r, 0x034);
        assert_eq!(word & 0x3f, 1, "PRO: rtc_get_reset_reason(0)");
        assert_eq!((word >> 6) & 0x3f, 1, "APP: rtc_get_reset_reason(1)");
        assert_eq!(word & !0xfff, 0x3000, "the rest of the word is the PAC's");
        // The RWDT write-protect key is the PAC's reset, so a disable that
        // writes it reads it back.
        assert_eq!(r.reg_name(0x0a4), Some("wdtwprotect"));
        assert_eq!(sb.read(&mut r, 0x0a4), 0x50d8_3aa1);
    }

    #[test]
    fn timg_carries_no_calibration_pretence() {
        let mut sb = Sandbox::new();
        let mut t = timg("TIMG0");
        assert_eq!(t.reg_name(0x068), Some("rtccalicfg"));
        assert_eq!(sb.read(&mut t, 0x068), 0x0001_3000, "the PAC's reset");
        // Start a calibration the way `measure_rtc_clock` does: `rdy` (bit
        // 15) stays whatever was written — nothing here pretends the count
        // finished, because nothing here counted.
        sb.write(&mut t, 0x068, 0x0001_3000 | (1 << 31));
        assert_eq!(sb.read(&mut t, 0x068) & (1 << 15), 0);
        assert_eq!(sb.read(&mut t, 0x06c), 0, "rtc_cali_value: nothing counted");
        assert_eq!(t.reg_name(0x064), Some("wdtwprotect"));
        assert_eq!(sb.read(&mut t, 0x064), 0x50d8_3aa1);
    }

    #[test]
    fn the_analog_master_answers_the_roms_busy_spin_without_a_pin() {
        let mut sb = Sandbox::new();
        let mut m = i2c_ana_mst();
        assert_eq!(m.reg_name(0x010), Some("host4_bbpll"));
        // `rom_chip_i2c_writeReg(0x66, 4, 3, 0x1c)`: the word the ROM stores.
        let word = (1 << 24) | (0x1c << 16) | (3 << 8) | 0x66;
        sb.write(&mut m, 0x010, word);
        assert_eq!(
            sb.read(&mut m, 0x010) & (1 << 25),
            0,
            "busy is a bit the guest never sets, so remembering answers the spin"
        );
        assert_eq!(sb.read(&mut m, 0x010), word);
        // Every register is modeled: the PAC does not know this block.
        assert_eq!(
            m.reg_grade(0x010),
            Some(lp_emu_esp_common::periph::RegGrade::Modeled)
        );
    }

    #[test]
    fn efuse_reads_the_pacs_zeros_and_remembers_nothing_it_is_not_told() {
        let mut sb = Sandbox::new();
        let mut e = efuse();
        assert_eq!(e.reg_name(0x000), Some("blk0_rdata0"));
        assert_eq!(sb.read(&mut e, 0x000), 0, "no MAC is invented here");
        assert_eq!(sb.read(&mut e, 0x014), 0);
        assert_eq!(sb.read(&mut e, 0x018), 0);
    }

    #[test]
    fn dport_remembers_the_interrupt_map_it_is_written() {
        let mut sb = Sandbox::new();
        let mut d = dport();
        assert_eq!(d.reg_name(0x218), Some("core_1_intr_map0"));
        sb.write(&mut d, 0x218, 16);
        assert_eq!(sb.read(&mut d, 0x218), 16);
        // D4's register, at the PAC's reset: `pro_cache_enable` (bit 3) is
        // whatever the PAC says, and P4 reads it, not this file.
        assert_eq!(d.reg_name(0x040), Some("pro_cache_ctrl"));
        assert_eq!(d.stored(0x040), regs::DPORT.reset(0x040).unwrap_or(0));
    }
}
