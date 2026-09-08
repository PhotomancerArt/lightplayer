//! `IO_MUX` at `0x6009_0000`: the pad configuration, as an **input-enable**
//! view.
//!
//! P5 had this block as accept-and-remember ([`super::accept::io_mux`]) and
//! every bit the boot path writes here still lands in that [`RegFile`]. What
//! M2 P1 adds is one field: `gpio[n].fun_ie` — bit 9, *"Input enable of the
//! pad. 1: input enabled. 0: input disabled."* (esp32c6 PAC 0.23.2,
//! `io_mux/gpio.rs`) — is pushed into the bus's signal fabric as
//! [`Fabric::set_pad_input_enable`](lp_emu_esp_common::pins::Fabric::set_pad_input_enable).
//!
//! That is the whole of it, and it is here rather than in `GPIO` because
//! `fun_ie` is an `IO_MUX` register and a peripheral never sees another
//! peripheral. The fabric is the one state the two blocks share, exactly as
//! the routing already was (`pins.rs`, plan DD34 e): `IO_MUX` writes the
//! input enable, `super::gpio` reads it back to decide whether to serve a
//! pad's bit in `GPIO.in_`.
//!
//! esp-hal 1.1.1 is what writes it. `Flex::set_input_enable` →
//! `io_mux_reg(self.number()).modify(|_, w| w.fun_ie().bit(on))`
//! (`gpio/mod.rs:1859`), and `Input::new` calls it with `true`
//! (`gpio/mod.rs:1090-1096`); `init_gpio` clears it on every `Flex::new`
//! (`gpio/mod.rs:1707`). A read-modify-write of the whole word, so the
//! block sees the bit whichever way the driver got there.
//!
//! # What is not modelled here
//!
//! Everything else in the word, and each one for the same reason the fabric
//! gives: it is an electrical fact. `fun_wpu` / `fun_wpd` (the *value* of a
//! pull is not modelled — an undriven pad reads low, not "pulled high"),
//! `fun_drv` (drive strength), `filter_en`, the four `slp_*` sleep-mode
//! bits, and `mcu_sel` (the matrix function — a pad routed in `GPIO` carries
//! its signal whatever `mcu_sel` says, as `super::gpio` has always stated).
//! All of them are accepted and remembered, and read back as written.

use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{BusCx, PadId, Peripheral, RegFile, Width};

use crate::regs;

/// The window length, unchanged from P5's accept block.
pub const LEN: u32 = 0x100;

/// `gpio[0]`. The pads run `gpio0` … `gpio30` at four bytes each; `+0x000`
/// is `pin_ctrl`, which is not a pad.
const GPIO0: u32 = 0x004;

/// Pads the C6 carries — the same count `super::gpio` uses.
const PAD_COUNT: u32 = super::gpio::PAD_COUNT;

const GPIO_END: u32 = GPIO0 + 4 * PAD_COUNT;

/// `fun_ie`, bit 9: *"Input enable of the pad."*
const FUN_IE: u32 = 1 << 9;

/// The IO_MUX block.
#[derive(Debug)]
pub struct IoMux {
    regs: RegFile,
}

impl Default for IoMux {
    fn default() -> Self {
        Self::new()
    }
}

impl IoMux {
    /// The P5 accept block's register file, unchanged — reset values and
    /// names from the PAC's table — with the `fun_ie` seam on top.
    pub fn new() -> Self {
        Self {
            regs: super::accept::io_mux(),
        }
    }

    /// `gpio[pad]` as last written.
    pub fn pad_config(&self, pad: u32) -> u32 {
        self.regs.stored(GPIO0 + 4 * pad)
    }
}

impl Peripheral for IoMux {
    fn name(&self) -> &'static str {
        "IO_MUX"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        self.regs.write(off, width, value, cx);
        if (GPIO0..GPIO_END).contains(&word) {
            let pad = (word - GPIO0) / 4;
            // The stored word, not the value: a byte lane store reaches only
            // its own bits and `fun_ie` may not be in the lane at all.
            let new = merge_lane(self.regs.stored(word), off, width, value);
            cx.pins
                .set_pad_input_enable(PadId(pad as u8), new & FUN_IE != 0);
        }
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::IO_MUX.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The input enable lives in the fabric, which rides `BusScalars`;
        // this is the register file and nothing else.
        self.regs.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.regs.load_state(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    const GPIO20: u32 = GPIO0 + 4 * 20;

    #[test]
    fn the_accept_blocks_reads_are_unchanged() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        for pad in 0..PAD_COUNT {
            assert_eq!(sb.read(&mut m, GPIO0 + 4 * pad), 0x0800, "pad {pad}");
        }
        assert_eq!(sb.read(&mut m, 0x000), 0x1def, "pin_ctrl");
        assert_eq!(m.reg_name(0x07c), Some("gpio30"));
        assert_eq!(m.reg_name(0x080), None, "there is no pad 31");
        // Anything else is still accept-and-remember.
        sb.write(&mut m, 0x0f0, 0x1234);
        assert_eq!(sb.read(&mut m, 0x0f0), 0x1234);
    }

    /// esp-hal's `Input::new` path: `init_gpio` clears `fun_ie`, then
    /// `set_input_enable(true)` sets it. Both are read-modify-writes of the
    /// whole word, and the pull bits ride along without meaning anything.
    #[test]
    fn fun_ie_reaches_the_fabric_and_nothing_else_in_the_word_does() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        assert!(!sb.pins.pad_input_enable(PadId(20)), "off at reset");

        // `init_gpio`: mcu_sel = 1 (bits 12:14), fun_ie clear, slp_sel clear.
        sb.write(&mut m, GPIO20, 0x0800 | (1 << 12));
        assert!(!sb.pins.pad_input_enable(PadId(20)));

        // `set_input_enable(true)` with the pull-up `apply_input_config` set.
        sb.write(&mut m, GPIO20, 0x0800 | (1 << 12) | FUN_IE | (1 << 8));
        assert!(sb.pins.pad_input_enable(PadId(20)), "fun_ie is the seam");
        assert_eq!(m.pad_config(20), 0x0800 | (1 << 12) | FUN_IE | (1 << 8));

        // And clearing it takes the pad back off the input side.
        sb.write(&mut m, GPIO20, 0x0800 | (1 << 12));
        assert!(!sb.pins.pad_input_enable(PadId(20)));
    }

    #[test]
    fn a_byte_lane_store_reaches_fun_ie_without_carrying_the_rest_of_the_word() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        // `fun_ie` is bit 9, so it lives in the byte at `+1`.
        m.write(GPIO20 + 1, Width::Byte, FUN_IE >> 8, &mut sb.cx());
        assert!(sb.pins.pad_input_enable(PadId(20)));
        assert_eq!(m.pad_config(20), FUN_IE, "the lane replaced bits 8..15");
    }

    #[test]
    fn a_write_that_is_not_a_pad_touches_no_pad() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, 0x000, 0xffff_ffff);
        sb.write(&mut m, GPIO_END, 0xffff_ffff);
        for pad in 0..PAD_COUNT {
            assert!(!sb.pins.pad_input_enable(PadId(pad as u8)), "pad {pad}");
        }
    }

    #[test]
    fn the_state_blob_round_trips_the_registers() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, GPIO20, FUN_IE | 0x0800);
        let blob = m.save_state();
        let mut other = IoMux::new();
        other.load_state(&blob);
        assert_eq!(other.pad_config(20), FUN_IE | 0x0800);
    }
}
