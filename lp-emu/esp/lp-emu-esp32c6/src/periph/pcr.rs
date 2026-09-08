//! `PCR` — the accept block of P5, plus the one thing a clock controller
//! does that "remember what was written" cannot express: it **feeds** other
//! blocks.
//!
//! esp-hal selects a UART's source clock in `PCR.uart(n).clk_conf`
//! (`soc/esp32c6/clocks.rs:740-761`: `sclk_sel` 1 = PLL_F80M, 2 = RC_FAST,
//! 3 = XTAL; `sclk_div_num` bits 12:19), not in the UART's own `clk_conf`
//! (discovery §5). The UART model needs that word to turn its `clkdiv` into
//! a baud rate, and a peripheral is not allowed to see another peripheral.
//! So the wire between them is a [`ClockLine`]: a shared cell PCR writes and
//! the UART reads — which is exactly what the clock tree is on the chip, a
//! signal from one block into another, and nothing more.
//!
//! The RMT's function clock is the same story one block over (M5 P1,
//! discovery §3): esp-hal's `Rmt::new` writes `PCR.rmt_sclk_conf`
//! (`sclk_sel = 1` = PLL 80 MHz, `div_num 0`) and gates the block through
//! `PCR.rmt_conf.clk_en` — the C6's `RMT.sys_conf` has no `sclk_*` fields at
//! all. Both words ride an [`RmtClockLine`] into [`super::rmt`].
//!
//! Everything else PCR does is the P5 accept table ([`super::accept::pcr`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// `PCR.uart(0).clk_conf` / `uart(1).clk_conf` (`regs::PCR`: `+0x004`,
/// `+0x010`).
pub const UART0_CLK_CONF: u32 = 0x004;
pub const UART1_CLK_CONF: u32 = 0x010;

/// `PCR.rmt_conf` (`+0x02c`): bit 0 `clk_en`, bit 1 `rst_en`
/// (`pcr/rmt_conf.rs`). Reset `0x01`: the block is clocked.
pub const RMT_CONF: u32 = 0x02c;
pub const RMT_CONF_RESET: u32 = 0x01;

/// `PCR.rmt_sclk_conf` (`+0x030`): 0:5 `sclk_div_b`, 6:11 `sclk_div_a`,
/// 12:19 `sclk_div_num`, 20:21 `sclk_sel` (0 none, 1 PLL 80 MHz, 2 FOSC,
/// 3 XTAL 40 MHz), 22 `sclk_en` (`pcr/rmt_sclk_conf.rs:26-46`). Reset
/// `0x0050_1000` = `div_num 1, sel 1, en` — 40 MHz until esp-hal writes
/// `div_num 0` for its 80 MHz.
pub const RMT_SCLK_CONF: u32 = 0x030;
pub const RMT_SCLK_CONF_RESET: u32 = 0x0050_1000;

/// The PAC reset value of `uart(n).clk_conf`: `sclk_sel = 3` (XTAL),
/// `sclk_en` set, no divider. The ROM console runs on XTAL, and this is what
/// makes the UART's reset `clkdiv` come out at 115,200.
pub const UART_CLK_CONF_RESET: u32 = 0x0070_0000;

/// A clock-configuration word fed from one peripheral to another.
///
/// Holds the raw `clk_conf` register word; the consumer decodes it. Cloned
/// handles share the cell.
#[derive(Clone, Debug)]
pub struct ClockLine(Arc<AtomicU32>);

impl ClockLine {
    pub fn new(word: u32) -> Self {
        Self(Arc::new(AtomicU32::new(word)))
    }

    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, word: u32) {
        self.0.store(word, Ordering::Relaxed);
    }
}

impl Default for ClockLine {
    fn default() -> Self {
        Self::new(UART_CLK_CONF_RESET)
    }
}

/// The UART clock lines PCR drives, one per instance.
#[derive(Clone, Debug, Default)]
pub struct UartClockLines {
    pub uart0: ClockLine,
    pub uart1: ClockLine,
}

/// The two PCR words the RMT block's clock is made of, fed to
/// [`super::rmt::Rmt`] the way the UART lines are.
#[derive(Clone, Debug)]
pub struct RmtClockLine {
    /// `rmt_conf`: the clock gate.
    pub conf: ClockLine,
    /// `rmt_sclk_conf`: source, divider, enable.
    pub sclk: ClockLine,
}

impl Default for RmtClockLine {
    fn default() -> Self {
        Self {
            conf: ClockLine::new(RMT_CONF_RESET),
            sclk: ClockLine::new(RMT_SCLK_CONF_RESET),
        }
    }
}

/// `PCR`: the P5 accept block with the UART and RMT clock lines driven
/// from it.
#[derive(Debug)]
pub struct Pcr {
    regs: RegFile,
    lines: UartClockLines,
    rmt: RmtClockLine,
}

impl Pcr {
    pub fn new(lines: UartClockLines, rmt: RmtClockLine) -> Self {
        let regs = super::accept::pcr();
        Self { regs, lines, rmt }
    }

    fn drive_lines(&self) {
        self.lines.uart0.set(self.regs.stored(UART0_CLK_CONF));
        self.lines.uart1.set(self.regs.stored(UART1_CLK_CONF));
        self.rmt.conf.set(self.regs.stored(RMT_CONF));
        self.rmt.sclk.set(self.regs.stored(RMT_SCLK_CONF));
    }
}

impl Peripheral for Pcr {
    fn name(&self) -> &'static str {
        self.regs.name()
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.regs.write(off, width, value, cx);
        if matches!(
            off & !3,
            UART0_CLK_CONF | UART1_CLK_CONF | RMT_CONF | RMT_SCLK_CONF
        ) {
            self.drive_lines();
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.regs.reg_name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        self.regs.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.regs.load_state(bytes);
        self.drive_lines();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_rmt_clock_line_starts_at_the_pac_reset_and_follows_esp_hal() {
        let rmt = RmtClockLine::default();
        let mut sb = Sandbox::new();
        let mut pcr = Pcr::new(UartClockLines::default(), rmt.clone());
        assert_eq!(rmt.sclk.get(), RMT_SCLK_CONF_RESET, "40 MHz: div_num 1");
        assert_eq!(rmt.conf.get(), RMT_CONF_RESET, "clocked");
        assert_eq!(sb.read(&mut pcr, RMT_SCLK_CONF), 0x0050_1000);
        assert_eq!(pcr.reg_name(RMT_SCLK_CONF), Some("rmt_sclk_conf"));
        assert_eq!(pcr.reg_name(RMT_CONF), Some("rmt_conf"));
        // `Rmt::new`, as the test_rmt trace shows it: the guard's clk_en,
        // an rst_en pulse, then sel 1 / div_num 0.
        sb.write(&mut pcr, RMT_CONF, 0x3);
        sb.write(&mut pcr, RMT_CONF, 0x1);
        sb.write(&mut pcr, RMT_SCLK_CONF, 0x0010_0000);
        sb.write(&mut pcr, RMT_SCLK_CONF, 0x0050_0000);
        assert_eq!(rmt.sclk.get(), 0x0050_0000, "80 MHz");
        assert_eq!(rmt.conf.get(), 0x1);
        let blob = pcr.save_state();
        let fresh = RmtClockLine::default();
        let mut other = Pcr::new(UartClockLines::default(), fresh.clone());
        other.load_state(&blob);
        assert_eq!(fresh.sclk.get(), 0x0050_0000, "a restore re-drives it");
    }

    #[test]
    fn the_uart_clock_lines_follow_the_pcr_words_and_start_at_xtal() {
        let lines = UartClockLines::default();
        let mut sb = Sandbox::new();
        let mut pcr = Pcr::new(lines.clone(), RmtClockLine::default());
        assert_eq!(lines.uart0.get(), UART_CLK_CONF_RESET, "XTAL, enabled");
        assert_eq!(sb.read(&mut pcr, UART0_CLK_CONF), UART_CLK_CONF_RESET);

        // esp-hal picking PLL_F80M with div_num 0 for 921,600.
        sb.write(&mut pcr, UART0_CLK_CONF, 0x0050_0000);
        assert_eq!(lines.uart0.get(), 0x0050_0000);
        assert_eq!(lines.uart1.get(), UART_CLK_CONF_RESET, "uart1 untouched");
        sb.write(&mut pcr, UART1_CLK_CONF, 0x0031_0000);
        assert_eq!(lines.uart1.get(), 0x0031_0000);

        // The P5 accept overrides survive the wrapper.
        assert_eq!((sb.read(&mut pcr, 0x110) >> 24) & 0x7f, 40);
        assert_eq!(pcr.reg_name(0x004), Some("uart0.clk_conf"));

        // A restore re-drives the lines.
        let blob = pcr.save_state();
        let fresh = UartClockLines::default();
        let mut other = Pcr::new(fresh.clone(), RmtClockLine::default());
        other.load_state(&blob);
        assert_eq!(fresh.uart0.get(), 0x0050_0000);
    }
}
