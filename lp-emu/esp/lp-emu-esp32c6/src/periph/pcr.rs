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
//! Everything else PCR does is the P5 accept table ([`super::accept::pcr`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// `PCR.uart(0).clk_conf` / `uart(1).clk_conf` (`regs::PCR`: `+0x004`,
/// `+0x010`).
pub const UART0_CLK_CONF: u32 = 0x004;
pub const UART1_CLK_CONF: u32 = 0x010;

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

/// `PCR`: the P5 accept block with the two UART clock lines driven from it.
#[derive(Debug)]
pub struct Pcr {
    regs: RegFile,
    lines: UartClockLines,
}

impl Pcr {
    pub fn new(lines: UartClockLines) -> Self {
        let regs = super::accept::pcr()
            .with_reset(UART0_CLK_CONF, UART_CLK_CONF_RESET)
            .with_reset(UART1_CLK_CONF, UART_CLK_CONF_RESET);
        Self { regs, lines }
    }

    fn drive_lines(&self) {
        self.lines.uart0.set(self.regs.stored(UART0_CLK_CONF));
        self.lines.uart1.set(self.regs.stored(UART1_CLK_CONF));
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
        if matches!(off & !3, UART0_CLK_CONF | UART1_CLK_CONF) {
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
    fn the_uart_clock_lines_follow_the_pcr_words_and_start_at_xtal() {
        let lines = UartClockLines::default();
        let mut sb = Sandbox::new();
        let mut pcr = Pcr::new(lines.clone());
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
        let mut other = Pcr::new(fresh.clone());
        other.load_state(&blob);
        assert_eq!(fresh.uart0.get(), 0x0050_0000);
    }
}
