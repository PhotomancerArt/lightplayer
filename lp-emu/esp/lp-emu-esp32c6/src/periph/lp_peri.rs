//! `LP_PERI` at `0x600B_2800` — the LP domain's clock gates and reset
//! lines, and the RNG that lives in the same window.
//!
//! The block keeps the peripheral `name()` **`RNG`**, which it has had since
//! P5: the power-on snapshot restores peripherals by name
//! ([`lp_emu_esp_common::periph::SocBus::restore_peripherals`]) and every
//! committed transcript and `--map` listing names it that way. The file is
//! named for the block because the register the RNG owns is one of ten in it
//! and the other two that matter are the reason this file exists.
//!
//! # `rng_data`
//!
//! esp-hal reads `RNG.data` (`rng/ll.rs:59-73`) spaced by the CPU cycle
//! counter; the C6's `Trng` path that stirs the SAR ADC into it is never
//! taken by the shipped firmware (discovery §4). Every read of `+0x08`
//! advances an xorshift64\* generator seeded from the machine's `--seed`, so
//! a run is the same run (plan PD5) and a different seed is a different one.
//! Silicon's RNG is not reproducible; this one is, on purpose, and says so.
//!
//! # `clk_en` bit 29 and `reset_en` bit 29 — the gate on the LP analog master
//!
//! `clk_en` (`+0x000`) is `LPPERI_CLK_EN` and `reset_en` (`+0x004`) is
//! `LPPERI_RESET_EN`. Bit 29 of each is the LP analog I2C master's:
//! `LP_ANA_I2C_CK_EN` (power-on **1**) and `LP_ANA_I2C_RESET_EN` (power-on
//! **0**). A board whose previous firmware gated that clock hangs the
//! ESP-IDF second-stage bootloader on [`super::lp_i2c_ana_mst`]'s busy bit
//! before its first console line
//! (`docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`,
//! `docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`), and
//! the cure the flashers ship is a write to each of these two registers.
//!
//! A peripheral is not allowed to see another peripheral, so the two bits
//! ride an [`LpPeriLines`] handle into the master — the same seam
//! [`super::pcr::UartClockLines`] and [`super::pcr::RmtClockLine`] are, one
//! domain over. The handle carries one thing those do not: the **rising
//! edge** of the reset line. The cure is a *pulse* (`reset_en |= bit29`,
//! then `reset_en &= !bit29`) with no bus access to the master in between,
//! so a consumer that only ever sampled the level would never see it.
//!
//! Every other offset is accept-and-remember with `LP_PERI`'s names.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use lp_emu_esp_common::regfile::lane_of;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use super::systimer::Reader;
use crate::regs;

/// `clk_en` = `LPPERI_CLK_EN` (`lp_analog_i2c.rs`: `0x600B_2800`).
pub const CLK_EN: u32 = 0x000;
/// `reset_en` = `LPPERI_RESET_EN` (`0x600B_2804`).
pub const RESET_EN: u32 = 0x004;
/// `rng_data`, the one register the RNG owns.
pub const DATA: u32 = 0x08;

/// The PAC reset value of `clk_en` (`regs::LP_PERI`): bits 23:30 set, bit 29
/// among them, so a power-on chip has the LP analog master clocked.
pub const CLK_EN_RESET: u32 = 0x7f80_0000;

/// Bit 29 of both registers: `LP_ANA_I2C_CK_EN` in `clk_en`,
/// `LP_ANA_I2C_RESET_EN` in `reset_en`. Bench-read values of `clk_en` on the
/// real boards, for scale: `0x7f00_0000` on a clean one, `0x4100_0000` on the
/// board a factory ESP-IDF app had gated, `0x5f00_0000` on the one the bench
/// induced by hand (`lp_analog_i2c.rs`, and the 2026-09-08 confirmation in
/// the first-flash defect).
pub const LP_ANA_I2C_BIT: u32 = 1 << 29;

/// The shared cells behind [`LpPeriLines`].
#[derive(Debug)]
struct Lines {
    /// The whole `clk_en` word, as the guest last wrote it.
    clk_en: AtomicU32,
    /// The whole `reset_en` word.
    reset_en: AtomicU32,
    /// A rising edge of `reset_en` bit 29 that the master has not consumed
    /// yet. Sticky, because the pulse is two writes to *this* block and the
    /// master is only looked at when the guest touches it.
    reset_pulse: AtomicBool,
}

/// `LPPERI_CLK_EN` and `LPPERI_RESET_EN`, fed from this block to
/// [`super::lp_i2c_ana_mst::LpI2cAnaMst`]. Cloned handles share the cells.
#[derive(Clone, Debug)]
pub struct LpPeriLines(Arc<Lines>);

impl LpPeriLines {
    /// Lines at power-on, with `clk_en` seeded: [`CLK_EN_RESET`] for a clean
    /// board, or whatever the previous firmware left
    /// ([`crate::machine::Esp32C6Builder::lp_peri_clk_en`]).
    pub fn new(clk_en: u32) -> Self {
        Self(Arc::new(Lines {
            clk_en: AtomicU32::new(clk_en),
            reset_en: AtomicU32::new(0),
            reset_pulse: AtomicBool::new(false),
        }))
    }

    /// Is the LP analog master clocked?
    pub fn ana_i2c_clk(&self) -> bool {
        self.0.clk_en.load(Ordering::Relaxed) & LP_ANA_I2C_BIT != 0
    }

    /// Is its reset line **held** high right now?
    pub fn ana_i2c_reset(&self) -> bool {
        self.0.reset_en.load(Ordering::Relaxed) & LP_ANA_I2C_BIT != 0
    }

    /// Take the unconsumed rising edge of the reset line, if there is one.
    pub fn take_ana_i2c_reset_pulse(&self) -> bool {
        self.0.reset_pulse.swap(false, Ordering::Relaxed)
    }

    /// Is there an unconsumed edge? For `save_state`, which may not consume.
    pub fn ana_i2c_reset_pulse_pending(&self) -> bool {
        self.0.reset_pulse.load(Ordering::Relaxed)
    }

    /// The two words, for the block that drives them.
    pub fn words(&self) -> (u32, u32) {
        (
            self.0.clk_en.load(Ordering::Relaxed),
            self.0.reset_en.load(Ordering::Relaxed),
        )
    }

    /// Publish both words, raising the reset pulse on a rising edge of bit
    /// 29. What a guest write to either register does.
    fn drive(&self, clk_en: u32, reset_en: u32) {
        let was = self.0.reset_en.swap(reset_en, Ordering::Relaxed);
        self.0.clk_en.store(clk_en, Ordering::Relaxed);
        if was & LP_ANA_I2C_BIT == 0 && reset_en & LP_ANA_I2C_BIT != 0 {
            self.0.reset_pulse.store(true, Ordering::Relaxed);
        }
    }

    /// Publish both words **without** edge detection, and set the pending
    /// edge from a blob. What a `load_state` does: a restore is not a write,
    /// and a snapshot taken with the line high must not pulse on the way
    /// back in.
    fn restore(&self, clk_en: u32, reset_en: u32, pending: bool) {
        self.0.clk_en.store(clk_en, Ordering::Relaxed);
        self.0.reset_en.store(reset_en, Ordering::Relaxed);
        self.0.reset_pulse.store(pending, Ordering::Relaxed);
    }
}

impl Default for LpPeriLines {
    fn default() -> Self {
        Self::new(CLK_EN_RESET)
    }
}

/// The `LP_PERI` block: the RNG, the two gates, and accept-and-remember for
/// the rest.
#[derive(Debug)]
pub struct LpPeri {
    regs: RegFile,
    state: u64,
    lines: LpPeriLines,
}

impl LpPeri {
    /// `seed` seeds the RNG; `clk_en` is the power-on value of `clk_en`
    /// (the induced board's is not the PAC reset); `lines` is the handle the
    /// LP analog master reads.
    pub fn new(seed: u64, clk_en: u32, lines: LpPeriLines) -> Self {
        let mut regs = RegFile::new("RNG", 0x400).with_names(regs::LP_PERI);
        regs.poke(CLK_EN, clk_en);
        let block = Self {
            regs,
            // xorshift needs a non-zero state; the constant is SplitMix64's.
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            lines,
        };
        block.drive_lines();
        block
    }

    /// The next 32 bits (xorshift64\*, Vigna 2016), advancing the state.
    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    fn drive_lines(&self) {
        self.lines
            .drive(self.regs.stored(CLK_EN), self.regs.stored(RESET_EN));
    }
}

impl Peripheral for LpPeri {
    fn name(&self) -> &'static str {
        "RNG"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == DATA {
            return lane_of(self.next(), off, width);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        if off & !3 == DATA {
            return;
        }
        self.regs.write(off, width, value, cx);
        if matches!(off & !3, CLK_EN | RESET_EN) {
            self.drive_lines();
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::LP_PERI.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x400 + 12);
        out.extend_from_slice(&self.state.to_le_bytes());
        out.extend_from_slice(&u32::from(self.lines.ana_i2c_reset_pulse_pending()).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(state), Some(pending)) = (r.u64(), r.u32()) else {
            log::warn!("RNG: load_state blob too short, ignored");
            return;
        };
        self.state = state;
        self.regs.load_state(r.0);
        self.lines.restore(
            self.regs.stored(CLK_EN),
            self.regs.stored(RESET_EN),
            pending != 0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    fn block() -> (LpPeri, LpPeriLines) {
        let lines = LpPeriLines::default();
        (LpPeri::new(7, CLK_EN_RESET, lines.clone()), lines)
    }

    #[test]
    fn every_read_advances_and_the_same_seed_is_the_same_sequence() {
        let mut sb = Sandbox::new();
        let mut a = LpPeri::new(7, CLK_EN_RESET, LpPeriLines::default());
        let mut b = LpPeri::new(7, CLK_EN_RESET, LpPeriLines::default());
        let mut c = LpPeri::new(8, CLK_EN_RESET, LpPeriLines::default());
        let sa: Vec<u32> = (0..4).map(|_| sb.read(&mut a, DATA)).collect();
        let sb_: Vec<u32> = (0..4).map(|_| sb.read(&mut b, DATA)).collect();
        let sc: Vec<u32> = (0..4).map(|_| sb.read(&mut c, DATA)).collect();
        assert_eq!(sa, sb_);
        assert_ne!(sa, sc);
        assert!(sa.windows(2).all(|w| w[0] != w[1]), "advances: {sa:?}");
        assert_eq!(a.reg_name(DATA), Some("rng_data"));
        assert_eq!(a.name(), "RNG", "the snapshot restores peripherals by name");
    }

    #[test]
    fn a_zero_seed_is_still_a_live_generator_and_the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut z = LpPeri::new(0, CLK_EN_RESET, LpPeriLines::default());
        let first = sb.read(&mut z, DATA);
        assert_ne!(first, 0);
        let blob = z.save_state();
        let mut w = LpPeri::new(0, CLK_EN_RESET, LpPeriLines::default());
        w.load_state(&blob);
        assert_eq!(sb.read(&mut w, DATA), sb.read(&mut z, DATA));
    }

    /// A clean board's power-on: the clock line is up, the reset line down.
    #[test]
    fn a_clean_board_powers_on_with_the_analog_master_clocked() {
        let mut sb = Sandbox::new();
        let (mut p, lines) = block();
        assert_eq!(sb.read(&mut p, CLK_EN), CLK_EN_RESET);
        assert!(lines.ana_i2c_clk());
        assert!(!lines.ana_i2c_reset());
        assert!(!lines.take_ana_i2c_reset_pulse());
        assert_eq!(p.reg_name(CLK_EN), Some("clk_en"));
        assert_eq!(p.reg_name(RESET_EN), Some("reset_en"));
    }

    /// The induced board: the seed is the register's power-on value, so the
    /// gate is down before the first instruction runs.
    #[test]
    fn the_induced_boards_clk_en_seed_is_the_power_on_value() {
        let mut sb = Sandbox::new();
        let lines = LpPeriLines::new(0x5f00_0000);
        let mut p = LpPeri::new(0, 0x5f00_0000, lines.clone());
        assert_eq!(sb.read(&mut p, CLK_EN), 0x5f00_0000);
        assert!(!lines.ana_i2c_clk(), "bit 29 clear: LP_ANA_I2C_CK_EN gated");
    }

    /// Every write publishes both words, and the flasher's cure is seen as
    /// a set-then-clear even though the master is never touched between.
    #[test]
    fn a_write_publishes_the_two_bits_and_a_reset_pulse_survives_its_own_clear() {
        let mut sb = Sandbox::new();
        let lines = LpPeriLines::new(0x5f00_0000);
        let mut p = LpPeri::new(0, 0x5f00_0000, lines.clone());

        // `clk_en |= bit29`
        sb.write(&mut p, CLK_EN, 0x5f00_0000 | LP_ANA_I2C_BIT);
        assert!(lines.ana_i2c_clk());
        assert!(!lines.ana_i2c_reset());
        assert!(
            !lines.take_ana_i2c_reset_pulse(),
            "the clock is not a reset"
        );

        // `reset_en |= bit29`, then `reset_en &= !bit29`.
        sb.write(&mut p, RESET_EN, LP_ANA_I2C_BIT);
        assert!(lines.ana_i2c_reset());
        sb.write(&mut p, RESET_EN, 0);
        assert!(!lines.ana_i2c_reset(), "the line is down again");
        assert!(
            lines.take_ana_i2c_reset_pulse(),
            "…and the edge survived it"
        );
        assert!(!lines.take_ana_i2c_reset_pulse(), "consumed once");
        assert_eq!(lines.words(), (0x7f00_0000, 0));
    }

    /// A restore re-drives the lines and does not invent an edge.
    #[test]
    fn a_restore_re_drives_the_lines_without_pulsing() {
        let mut sb = Sandbox::new();
        let (mut p, _) = block();
        sb.write(&mut p, CLK_EN, 0x5f00_0000);
        sb.write(&mut p, RESET_EN, LP_ANA_I2C_BIT);
        let blob = p.save_state();

        let fresh = LpPeriLines::default();
        let mut other = LpPeri::new(0, CLK_EN_RESET, fresh.clone());
        other.load_state(&blob);
        assert_eq!(fresh.words(), (0x5f00_0000, LP_ANA_I2C_BIT));
        assert!(!fresh.ana_i2c_clk());
        assert!(fresh.ana_i2c_reset(), "the level comes back");
        assert!(
            fresh.ana_i2c_reset_pulse_pending(),
            "the edge the snapshot had not been consumed yet, so it comes back too"
        );
        assert_eq!(other.save_state(), blob, "and the blob round-trips");
        assert!(
            fresh.take_ana_i2c_reset_pulse(),
            "and it is still there to take"
        );
    }
}
