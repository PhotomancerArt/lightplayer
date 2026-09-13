//! `IO_MUX` at `0x6000_9000`: the pad configuration, as an **input-enable**
//! view.
//!
//! P06 had this block as accept-and-remember ([`super::accept::io_mux`]) and
//! every bit the ROM-up boot writes here still lands in that [`RegFile`].
//! What P07 adds is one field: `gpio[n].fun_ie` — bit 9, *"Input enable of
//! the pad. 1: input enabled; 0: input disabled."* (esp32s3 PAC 0.35.2,
//! `io_mux/gpio.rs`) — is pushed into the bus's signal fabric as
//! [`Fabric::set_pad_input_enable`](lp_emu_esp_common::pins::Fabric::set_pad_input_enable).
//!
//! That is the whole of it, and it is here rather than in `GPIO` because
//! `fun_ie` is an `IO_MUX` register and a peripheral never sees another
//! peripheral. The fabric is the one state the two blocks share, exactly as
//! the routing already is: `IO_MUX` writes the input enable, [`super::gpio`]
//! reads it back to decide whether to serve a pad's bit in `GPIO.in_`.
//!
//! # Why this is the C6's file and not the classic's
//!
//! The classic's `IO_MUX` pad map is **not in pad order** — its `+0x004` is
//! `gpio36` and `+0x044` is `gpio0` — so its view carries a transcribed map.
//! The S3's is in pad order, `gpio0` at `+0x004` through `gpio48` at
//! `+0x0c4`, and [`the_pad_map_is_in_pad_order`] asserts that against the
//! generated table's own names rather than trusting this sentence. So the
//! indirection is **not** ported and the arithmetic is the C6's.
//!
//! ⚠️ **`fun_ie` is set at reset on this chip and clear on the C6.** The S3's
//! `gpio[n]` resets to `0x0b00` — `fun_wpu` (8), `fun_ie` (9) and `fun_drv`
//! = 2 (10:11) — against the C6's `0x0800`. So a machine whose fabric started
//! with every input disabled would disagree with its own register file from
//! the first cycle, and [`seed_input_enables`] exists to put the reset word's
//! bit where the register file already says it is. The C6 needs no such call
//! because `false` is what its reset value means.
//!
//! # What is not modelled here
//!
//! Everything else in the word, and each one for the same reason the fabric
//! gives: it is an electrical fact. `fun_wpu` / `fun_wpd` (the *value* of a
//! pull is not modelled — an undriven pad reads low, not "pulled high"),
//! `fun_drv` (drive strength), `filter_en`, the four `slp_*` sleep-mode bits,
//! and `mcu_sel` (the matrix function — a pad routed in `GPIO` carries its
//! signal whatever `mcu_sel` says, as [`super::gpio`] states). All of them
//! are accepted and remembered, and read back as written.
//!
//! ⚠️ The C6 has `mcu_drv` at bits 5:6 and **this chip does not** (the PAC's
//! field list goes `slp_ie` 4 → `fun_wpd` 7). Nothing here reads those bits,
//! so it changes no behaviour; it is recorded because it is the one bitfield
//! difference between the two chips' pad words.

use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{BusCx, PadId, Peripheral, RegFile, Width};

use crate::regs;

/// The window length, unchanged from P06's accept block: the generated table
/// ends at `gpio48` (`+0x0c4`).
pub const LEN: u32 = super::accept::IO_MUX_LEN;

/// `gpio[0]`. The pads run `gpio0` … `gpio48` at four bytes each; `+0x000`
/// is `pin_ctrl`, which is not a pad.
const GPIO0: u32 = 0x004;

/// Pads the S3 carries — the same count [`super::gpio`] uses.
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
    /// The P06 accept block's register file, unchanged — reset values and
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

/// Put the block's **reset** input enables into the fabric, once, at build
/// time.
///
/// The S3's `gpio[n]` resets with `fun_ie` set, so every pad comes out of
/// reset with its input buffer on. The register file already says so; this
/// is the machine telling the fabric the same thing before the guest runs,
/// so that `GPIO.in_` reads what `IO_MUX` says from cycle zero rather than
/// from the guest's first pad write. See the module header.
pub fn seed_input_enables(pins: &mut lp_emu_esp_common::pins::Fabric) {
    let regs = super::accept::io_mux();
    for pad in 0..PAD_COUNT {
        let word = regs.stored(GPIO0 + 4 * pad);
        pins.set_pad_input_enable(PadId(pad as u8), word & FUN_IE != 0);
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

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        self.regs.reg_grade(off)
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

    const GPIO9: u32 = GPIO0 + 4 * 9;

    /// ⚠️ The classic's map is `+0x004 = gpio36`; the S3's is not, and this
    /// asserts it against the generated table's own names rather than
    /// against a comment. A chip whose map moved would fail here instead of
    /// silently configuring the wrong pad.
    #[test]
    fn the_pad_map_is_in_pad_order() {
        let m = IoMux::new();
        for pad in 0..PAD_COUNT {
            assert_eq!(
                m.reg_name(GPIO0 + 4 * pad),
                Some(format!("gpio{pad}").as_str()),
                "+0x{:03x} must be gpio{pad}",
                GPIO0 + 4 * pad
            );
        }
        assert_eq!(m.reg_name(0x000), Some("pin_ctrl"), "not a pad");
        assert_eq!(m.reg_name(GPIO_END), None, "there is no pad 49");
    }

    #[test]
    fn the_accept_blocks_reads_are_unchanged() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        for pad in 0..PAD_COUNT {
            assert_eq!(sb.read(&mut m, GPIO0 + 4 * pad), 0x0b00, "pad {pad}");
        }
    }

    /// The S3's reset word carries `fun_ie`, so the fabric has to be told —
    /// the C6's does not and is not.
    #[test]
    fn the_reset_word_enables_every_pads_input_and_the_seed_says_so() {
        let mut sb = Sandbox::new();
        let m = IoMux::new();
        assert_eq!(m.pad_config(9) & FUN_IE, FUN_IE, "fun_ie is set at reset");
        for pad in 0..PAD_COUNT {
            assert!(
                !sb.pins.pad_input_enable(PadId(pad as u8)),
                "before seeding"
            );
        }
        seed_input_enables(&mut sb.pins);
        for pad in 0..PAD_COUNT {
            assert!(sb.pins.pad_input_enable(PadId(pad as u8)), "pad {pad}");
        }
    }

    /// esp-hal's `Input::new` path: `init_gpio` clears `fun_ie`, then
    /// `set_input_enable(true)` sets it. Both are read-modify-writes of the
    /// whole word, and the pull bits ride along without meaning anything.
    #[test]
    fn fun_ie_reaches_the_fabric_and_nothing_else_in_the_word_does() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        seed_input_enables(&mut sb.pins);

        // `init_gpio`: mcu_sel = 1 (bits 12:14), fun_ie clear, slp_sel clear.
        sb.write(&mut m, GPIO9, 0x0800 | (1 << 12));
        assert!(!sb.pins.pad_input_enable(PadId(9)));

        // `set_input_enable(true)` with the pull-up `apply_input_config` set.
        sb.write(&mut m, GPIO9, 0x0800 | (1 << 12) | FUN_IE | (1 << 8));
        assert!(sb.pins.pad_input_enable(PadId(9)), "fun_ie is the seam");
        assert_eq!(m.pad_config(9), 0x0800 | (1 << 12) | FUN_IE | (1 << 8));

        // And clearing it takes the pad back off the input side.
        sb.write(&mut m, GPIO9, 0x0800 | (1 << 12));
        assert!(!sb.pins.pad_input_enable(PadId(9)));
    }

    #[test]
    fn a_byte_lane_store_reaches_fun_ie_without_carrying_the_rest_of_the_word() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        // `fun_ie` is bit 9, so it lives in the byte at `+1`.
        m.write(GPIO9 + 1, Width::Byte, 0, &mut sb.cx());
        assert!(!sb.pins.pad_input_enable(PadId(9)), "the lane cleared it");
        assert_eq!(m.pad_config(9), 0x0b00 & 0xffff_00ff);
        m.write(GPIO9 + 1, Width::Byte, FUN_IE >> 8, &mut sb.cx());
        assert!(sb.pins.pad_input_enable(PadId(9)));
        assert_eq!(m.pad_config(9), FUN_IE, "the lane replaced bits 8..15");
    }

    #[test]
    fn a_write_that_is_not_a_pad_touches_no_pad() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, 0x000, 0xffff_ffff);
        for pad in 0..PAD_COUNT {
            assert!(!sb.pins.pad_input_enable(PadId(pad as u8)), "pad {pad}");
        }
    }

    #[test]
    fn the_state_blob_round_trips_the_registers() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, GPIO9, FUN_IE | 0x0800);
        let blob = m.save_state();
        let mut other = IoMux::new();
        other.load_state(&blob);
        assert_eq!(other.pad_config(9), FUN_IE | 0x0800);
    }
}
