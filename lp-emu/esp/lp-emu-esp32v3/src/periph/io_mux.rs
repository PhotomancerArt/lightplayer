//! `IO_MUX` at `0x3FF4_9000`: the pad configuration, as an **input-enable**
//! view.
//!
//! P3–P7 had this block as accept-and-remember ([`super::accept::io_mux`])
//! and every bit the boot path writes here still lands in that [`RegFile`].
//! What P8 adds is one field: `gpio[n].fun_ie` — **bit 9**, *"Input enable of
//! the pad. 1: input enabled; 0: input disabled."* (`esp32` PAC 0.40.2,
//! `io_mux/gpio0.rs`) — is pushed into the bus's signal fabric as
//! [`Fabric::set_pad_input_enable`](lp_emu_esp_common::pins::Fabric::set_pad_input_enable).
//!
//! It is here rather than in [`super::gpio`] because `fun_ie` is an `IO_MUX`
//! register and a peripheral never sees another peripheral. The fabric is the
//! one state the two blocks share, exactly as the routing already was
//! (`pins.rs`, plan DD34 e): `IO_MUX` writes the input enable, `super::gpio`
//! reads it back to decide whether to serve a pad's bit in `GPIO.in_`.
//!
//! # ⚠️ The classic's pad registers are **not in pad order**
//!
//! On the C6 `gpio[n]` is at `+0x004 + 4n` and the arithmetic is the pad
//! number. On the classic the block is laid out in the order the pads come
//! out of the package, so `+0x004` is **`gpio36`**, `+0x044` is `gpio0`, and
//! `+0x088` is `gpio1`. A view that did the C6's arithmetic would push
//! `fun_ie` onto a pad eight to thirty-six places away from the one the
//! driver meant, and every one of those writes would still look plausible.
//!
//! [`PAD_OF_OFFSET`] is the map, transcribed from the **generated** table's
//! own register names ([`crate::regs::IO_MUX`], which
//! `scripts/emu/pac-regnames.py` writes out of the PAC and
//! `just lint-emu-regnames` checks). The transcription is not trusted:
//! `the_pad_map_is_the_generated_tables_own_names` walks every entry and
//! asserts the map agrees with `regs::IO_MUX.name(off)`, so the two cannot
//! drift.
//!
//! **Thirty-six pads, not forty.** The classic has no GPIO 28, 29, 30 or 31
//! — the PAC names no register for them — while [`super::gpio`] carries
//! forty `pin`/`func_out_sel_cfg` slots. A pad with no IO_MUX register has
//! no input enable to write, which is why the map is sparse rather than an
//! array.
//!
//! # What is not modelled here
//!
//! Everything else in the word, each for the reason the fabric gives: it is
//! an electrical fact. `fun_wpu` / `fun_wpd` (the *value* of a pull is not
//! modelled — an undriven pad reads low, not "pulled high"), `fun_drv`
//! (drive strength), the five `slp_*` sleep-mode bits, and `mcu_sel` (bits
//! 12:14, the function select; **2** is the GPIO matrix on this chip —
//! `gpio.gpio_function` in `esp-metadata-generated-0.4.0` — where the C6's
//! is 1). A pad routed in [`super::gpio`] carries its signal whatever
//! `mcu_sel` says, as that block has always stated. All of them are accepted
//! and remembered, and read back as written.

use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{BusCx, PadId, Peripheral, RegFile, RegGrade, Width};

use crate::regs;

/// The window length, unchanged from the accept block's: `pin_ctrl` and the
/// 36 pad words the PAC names, the last at `+0x90` (`gpio24`).
pub const LEN: u32 = super::accept::IO_MUX_LEN;

/// `fun_ie`, bit 9: *"Input enable of the pad."*
const FUN_IE: u32 = 1 << 9;

/// `mcu_sel`'s GPIO-matrix value on this chip: **2**
/// (`gpio.gpio_function`). Not gated on — see the module docs — and here so
/// a reader of a trace knows what esp-hal wrote.
pub const MCU_SEL_GPIO: u32 = 2;

/// Which pad each register offset configures, in offset order.
///
/// Transcribed from [`crate::regs::IO_MUX`]'s own generated names and
/// checked against them by a test. `+0x000` is `pin_ctrl`, which is not a
/// pad and is absent.
pub const PAD_OF_OFFSET: &[(u32, u8)] = &[
    (0x004, 36),
    (0x008, 37),
    (0x00c, 38),
    (0x010, 39),
    (0x014, 34),
    (0x018, 35),
    (0x01c, 32),
    (0x020, 33),
    (0x024, 25),
    (0x028, 26),
    (0x02c, 27),
    (0x030, 14),
    (0x034, 12),
    (0x038, 13),
    (0x03c, 15),
    (0x040, 2),
    (0x044, 0),
    (0x048, 4),
    (0x04c, 16),
    (0x050, 17),
    (0x054, 9),
    (0x058, 10),
    (0x05c, 11),
    (0x060, 6),
    (0x064, 7),
    (0x068, 8),
    (0x06c, 5),
    (0x070, 18),
    (0x074, 19),
    (0x078, 20),
    (0x07c, 21),
    (0x080, 22),
    (0x084, 3),
    (0x088, 1),
    (0x08c, 23),
    (0x090, 24),
];

/// The pad this offset configures, or `None` for `pin_ctrl` and for anything
/// past the named words.
#[must_use]
pub fn pad_of_offset(off: u32) -> Option<PadId> {
    PAD_OF_OFFSET
        .iter()
        .find(|(at, _)| *at == off)
        .map(|(_, pad)| PadId(*pad))
}

/// The offset that configures this pad, for a test or a driver model.
/// `None` for GPIO 28..31, which this part does not have.
#[must_use]
pub fn offset_of_pad(pad: u8) -> Option<u32> {
    PAD_OF_OFFSET
        .iter()
        .find(|(_, p)| *p == pad)
        .map(|(off, _)| *off)
}

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
    /// The accept block's register file, unchanged — names and reset values
    /// from the PAC's table — with the `fun_ie` seam on top.
    pub fn new() -> Self {
        Self {
            regs: super::accept::io_mux(),
        }
    }

    /// `gpio[pad]` as last written, by **pad number**. `None` for a pad this
    /// part does not have.
    pub fn pad_config(&self, pad: u8) -> Option<u32> {
        offset_of_pad(pad).map(|off| self.regs.stored(off))
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
        if let Some(pad) = pad_of_offset(word) {
            // The stored word, not the value: a byte-lane store reaches only
            // its own bits and `fun_ie` may not be in the lane at all.
            let new = merge_lane(self.regs.stored(word), off, width, value);
            cx.pins.set_pad_input_enable(pad, new & FUN_IE != 0);
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

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        // The pad words are the PAC's bit map read out loud; `pin_ctrl` and
        // anything unnamed stays `Modeled`.
        Some(if pad_of_offset(off & !3).is_some() {
            RegGrade::Documented
        } else {
            RegGrade::Modeled
        })
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

    /// The U0TXD pad, whose configuration word was the direct load's ninth
    /// strict stop — `gpio1`, at `+0x088`, which on the C6's arithmetic
    /// would be pad 33.
    const GPIO1: u32 = 0x088;

    /// The map is the generated table's own names, walked rather than
    /// eyeballed. A hand edit to either side fails here.
    #[test]
    fn the_pad_map_is_the_generated_tables_own_names() {
        for (off, pad) in PAD_OF_OFFSET {
            assert_eq!(
                regs::IO_MUX.name(*off),
                Some(alloc_name(*pad).as_str()),
                "offset {off:#05x}"
            );
        }
        assert_eq!(PAD_OF_OFFSET.len(), 36, "the classic has 36 IO_MUX pads");
        assert_eq!(regs::IO_MUX.name(0x000), Some("pin_ctrl"));
        assert_eq!(pad_of_offset(0x000), None, "`pin_ctrl` is not a pad");
        // Every named pad word is in the map, and nothing else is.
        for (off, name) in regs::IO_MUX.entries {
            if *name == "pin_ctrl" {
                continue;
            }
            assert!(pad_of_offset(*off).is_some(), "`{name}` is not mapped");
        }
        // The four pads the part does not have.
        for absent in [28u8, 29, 30, 31] {
            assert_eq!(offset_of_pad(absent), None, "gpio{absent} does not exist");
        }
    }

    fn alloc_name(pad: u8) -> String {
        format!("gpio{pad}")
    }

    /// ⚠️ The C6's arithmetic on this block is wrong by up to 35 pads, and
    /// this is the assertion that says so out loud.
    #[test]
    fn the_registers_are_not_in_pad_order() {
        assert_eq!(pad_of_offset(0x004), Some(PadId(36)), "not gpio0");
        assert_eq!(pad_of_offset(0x044), Some(PadId(0)));
        assert_eq!(pad_of_offset(GPIO1), Some(PadId(1)));
        // What the C6's `GPIO0 + 4 * pad` would have said for pad 1.
        assert_ne!(pad_of_offset(0x008), Some(PadId(1)));
    }

    /// esp-hal's `Input::new` path: `init_gpio` clears `fun_ie`, then
    /// `set_input_enable(true)` sets it. Both are read-modify-writes of the
    /// whole word, and the pull bits ride along without meaning anything.
    #[test]
    fn fun_ie_reaches_the_fabric_and_nothing_else_in_the_word_does() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        assert!(!sb.pins.pad_input_enable(PadId(1)), "off at reset");

        // `init_gpio`: mcu_sel = 2 (the classic's GPIO function), no fun_ie.
        sb.write(&mut m, GPIO1, MCU_SEL_GPIO << 12);
        assert!(!sb.pins.pad_input_enable(PadId(1)));

        // `set_input_enable(true)` with a pull-up along for the ride.
        sb.write(&mut m, GPIO1, (MCU_SEL_GPIO << 12) | FUN_IE | (1 << 8));
        assert!(sb.pins.pad_input_enable(PadId(1)), "fun_ie is the seam");
        assert_eq!(
            m.pad_config(1),
            Some((MCU_SEL_GPIO << 12) | FUN_IE | (1 << 8))
        );
        // And the pull-up did not make the pad read high: that is an
        // electrical fact the fabric does not model.
        assert!(!sb.pins.pad_level(PadId(1)));

        sb.write(&mut m, GPIO1, MCU_SEL_GPIO << 12);
        assert!(!sb.pins.pad_input_enable(PadId(1)));
    }

    #[test]
    fn a_byte_lane_store_reaches_fun_ie_without_carrying_the_rest_of_the_word() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        // `fun_ie` is bit 9, so it lives in the byte at `+1`.
        m.write(GPIO1 + 1, Width::Byte, FUN_IE >> 8, &mut sb.cx());
        assert!(sb.pins.pad_input_enable(PadId(1)));
        assert_eq!(m.pad_config(1), Some(FUN_IE), "the lane replaced bits 8..15");
    }

    #[test]
    fn a_write_that_is_not_a_pad_touches_no_pad() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, 0x000, 0xffff_ffff);
        for (_, pad) in PAD_OF_OFFSET {
            assert!(!sb.pins.pad_input_enable(PadId(*pad)), "gpio{pad}");
        }
        assert_eq!(sb.read(&mut m, 0x000), 0xffff_ffff, "still remembered");
    }

    #[test]
    fn the_accept_blocks_reads_are_unchanged() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        // The PAC carries no reset for this block: every pad reads 0 until
        // the firmware configures it (`super::accept::io_mux`'s note).
        for (off, _) in PAD_OF_OFFSET {
            assert_eq!(sb.read(&mut m, *off), 0, "{off:#05x}");
        }
        assert_eq!(m.reg_name(0x090), Some("gpio24"));
        assert_eq!(m.reg_name(0x094), None, "the table stops at +0x90");
    }

    #[test]
    fn the_state_blob_round_trips_the_registers() {
        let mut sb = Sandbox::new();
        let mut m = IoMux::new();
        sb.write(&mut m, GPIO1, FUN_IE | (MCU_SEL_GPIO << 12));
        let blob = m.save_state();
        let mut other = IoMux::new();
        other.load_state(&blob);
        assert_eq!(other.pad_config(1), Some(FUN_IE | (MCU_SEL_GPIO << 12)));
    }
}
