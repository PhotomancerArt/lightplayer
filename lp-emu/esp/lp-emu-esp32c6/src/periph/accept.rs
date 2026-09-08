//! The accept-and-remember blocks: a [`RegFile`] each, with the exceptions
//! the boot path needs written down as overrides. Every override cites the
//! esp-hal line that reads or spins on it.
//!
//! The rule (`lp_emu_esp_common::regfile`): a `RegFile` never invents
//! behaviour, it only remembers, and everything it pretends about is one
//! line here. Values marked *modeled* are plausible, not measured.

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
/// `RtcSlowClockSource::current()` reads (`rtc/esp32c6.rs:126`).
pub fn lp_clkrst() -> RegFile {
    RegFile::new("LP_CLKRST", 0x400)
        .with_names(regs::LP_CLKRST)
        .with_reset(0x010, crate::loader::ResetCause::PowerOn.rom_code())
        .with_reset(0x000, 0x04)
}

// ---- MODEM_*: written by `modem_clock_*`, never read back.

pub fn modem_syscon() -> RegFile {
    RegFile::new("MODEM_SYSCON", 0x100).with_names(regs::MODEM_SYSCON)
}

pub fn modem_lpcon() -> RegFile {
    RegFile::new("MODEM_LPCON", 0x100).with_names(regs::MODEM_LPCON)
}

/// `I2C_ANA_MST` — the analog I2C master, three spin sites:
///
/// - `ana_conf0.cal_done` (`+0x18` bit 24) must read **1**:
///   `while I2C_ANA_MST::regs().ana_conf0().read().cal_done().bit_is_clear() {}`
///   at `soc/esp32c6/clocks.rs:181-186` (the BBPLL calibration wait).
/// - `i2c_ctrl(n).busy` (`+0x00`/`+0x04` bit 25) must read **0**:
///   `while ...i2c_ctrl(master).read().busy().bit() {}` at
///   `soc/esp32c6/regi2c.rs:187, 194, 209`.
/// - `ana_conf2` (`+0x20`) picks the master index (`regi2c.rs:160-170`):
///   bit set → master 0, clear → master 1. It resets to 0 in the PAC, so
///   every block uses master 1, and both `i2c_ctrl` registers carry the
///   override so the pick cannot matter.
///
/// `i2c_ctrl.data` (bits 16:23) reads back what was written, so a
/// `regi2c_read` returns the last value written to that register — the
/// accept-and-remember reading of an analog register, stated here so the
/// PLL "readback" is not mistaken for a measurement.
pub fn i2c_ana_mst() -> RegFile {
    RegFile::new("I2C_ANA_MST", 0x100)
        .with_names(regs::I2C_ANA_MST)
        .with_read_override(0x018, 1 << 24, 1 << 24)
        .with_read_override(0x000, 1 << 25, 0)
        .with_read_override(0x004, 1 << 25, 0)
}

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
        .with_reset(0x110, 0x2800_0200)
        .with_read_override(0x110, 0x7f << 24, 40 << 24)
        .with_reset(0x114, 0x0d)
        .with_read_override(0x114, 1 << 3, 0)
        .with_reset(0x040, 0x0040_0000)
        .with_reset(0x04c, 0x0040_0000)
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

pub fn extmem() -> RegFile {
    RegFile::new("EXTMEM", 0x400).with_names(regs::EXTMEM)
}

/// UART0/UART1: accept in P5 so the ROM's `uart_tx_one_char` does not fault
/// and the no-radio image (which prints nothing on UART0) boots; the FIFO,
/// the thresholds and `RXFIFO_TOUT` are P6. Both share `uart0`'s layout.
pub fn uart(name: &'static str) -> RegFile {
    RegFile::new(name, 0x100).with_names(regs::UART0)
}

/// `USB_DEVICE`: accept in P5, reading 0 everywhere. That is the *host
/// absent, FIFO full* reading: `ep1_conf.serial_in_ep_data_free` is 0, so
/// esp-println spins its 50,000 iterations once, latches `TIMED_OUT`, and
/// drops output from then on (`esp-println/src/lib.rs:275-296` — silence,
/// not a hang); `int_raw.sof` is 0, so the connection monitor decides the
/// host is not enumerated. The honest model, with the attach/detach
/// control channel, is M3 P6 and M6.
pub fn usb_device() -> RegFile {
    RegFile::new("USB_DEVICE", 0x100).with_names(regs::USB_DEVICE)
}

/// `IO_MUX`: `pin_ctrl` plus all **31** pads at `+0x04 + 4n` (the PAC's
/// `gpio: [GPIO; 31]`; the vendor emulator stopped at 15), each at the PAC
/// reset value `0x0800`.
pub fn io_mux() -> RegFile {
    let mut rf = RegFile::new("IO_MUX", 0x100).with_names(regs::IO_MUX);
    for pad in 0..31u32 {
        rf = rf.with_reset(0x004 + 4 * pad, 0x0800);
    }
    rf
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
        let mut i2c = i2c_ana_mst();
        sb.write(&mut i2c, 0x018, 0);
        assert!(sb.read(&mut i2c, 0x018) & (1 << 24) != 0, "cal_done");
        sb.write(&mut i2c, 0x000, 0xffff_ffff);
        sb.write(&mut i2c, 0x004, 0xffff_ffff);
        assert_eq!(sb.read(&mut i2c, 0x000) & (1 << 25), 0, "i2c0 busy");
        assert_eq!(sb.read(&mut i2c, 0x004) & (1 << 25), 0, "i2c1 busy");
        assert_eq!(
            sb.read(&mut i2c, 0x020),
            0,
            "ana_conf2 resets to 0: master 1"
        );

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

        let mut c = lp_clkrst();
        assert_eq!(sb.read(&mut c, 0x010) & 0x1f, 1, "POWERON");
        assert_eq!(sb.read(&mut c, 0x000) & 0b11, 0, "RC_SLOW");
    }

    #[test]
    fn io_mux_has_thirty_one_pads() {
        let mut sb = Sandbox::new();
        let mut m = io_mux();
        for pad in 0..31u32 {
            assert_eq!(sb.read(&mut m, 0x004 + 4 * pad), 0x0800);
        }
        assert_eq!(m.reg_name(0x07c), Some("gpio30"));
        assert_eq!(m.reg_name(0x080), None, "there is no pad 31");
        // GPIO's own reads are `super::gpio`'s tests now.
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
        assert_eq!(uart("UART1").name(), "UART1");
        assert_eq!(uart("UART1").reg_name(0x01c), Some("status"));
        assert_eq!(usb_device().reg_name(0x004), Some("ep1_conf"));
        assert_eq!(extmem().reg_name(0x000).is_some(), true);
    }
}
