//! `GPIO` at `0x6009_1000`: the matrix, as a routing **view**.
//!
//! P5 had this block as accept-and-remember, and everything the boot path
//! writes here still lands in a [`RegFile`]. What M5 P2 adds is that three
//! groups of registers now *mean* something to the machine — they are
//! written into the bus's signal fabric ([`lp_emu_esp_common::pins`], plan
//! DD34 e), which is where the RMT's waveform meets the pad:
//!
//! | register | what it does here |
//! |---|---|
//! | `func_out_sel_cfg[n]` (`+0x554 + 4n`, n < 31) | `out_sel` names the signal pad `n` follows; `128` means "follow `GPIO_OUT[n]`" (PAC: "s=128: output of GPIO\[n\] equals GPIO_OUT_REG\[n\]"). `inv_sel` inverts. `oen_sel` is recorded and reported, never gated on. |
//! | `out` / `out_w1ts` / `out_w1tc` | the GPIO output bitmap a pad routed to `GPIO_OUT` follows |
//! | `enable` / `enable_w1ts` / `enable_w1tc` | the output-**enable** bitmap; recorded, reported in the routing note as `oe=`, never gated on |
//!
//! The sequence esp-hal writes at `Channel::with_pin` is exactly these
//! (M5 discovery §2a, §8): `out_w1tc` bit 18, `IO_MUX.gpio18.mcu_sel = 1`,
//! `enable_w1ts` bit 18, then `func_out_sel_cfg[18] = {out_sel 71}` — and the
//! last of those is the write that makes the RMT's channel-0 waveform reach
//! gpio18.
//!
//! # What is observed, and what is not
//!
//! - **A pad becomes observed when the guest writes its `func_out_sel_cfg`.**
//!   The register resets to `0x80` — every pad nominally follows `GPIO_OUT` —
//!   but seeding 31 routes at reset would give the machine 31 pads to decode
//!   and 31 pin logs for a boot that drives none of them. So an untouched pad
//!   stays unobserved; its `out` bit is still tracked, so the moment anything
//!   routes it the level is already right. A plain GPIO output pad that never
//!   writes `func_out_sel_cfg` is therefore *not* in the pin log — M5 is about
//!   the peripheral-driven pin, and that limit is stated rather than hidden.
//! - `in_` reads 0 and `pcpu_int` reads 0, as they did under the accept block:
//!   nothing drives a pad from outside and no GPIO interrupt is ever pending.
//!   Input modelling is not this milestone's.
//! - `out_w1ts`/`out_w1tc`/`enable_w1ts`/`enable_w1tc` fold into `out` and
//!   `enable` and **read back 0** (the PAC declares them write-only). The
//!   accept block used to remember them separately, which meant `out` never
//!   reflected a `w1ts` — honest here, and nothing in the boot path reads one.
//! - `out1`/`enable1` (pads 32+) are accepted: the C6 has 31 pads.
//! - Drive strength, pull-ups, open-drain, pad filters and `IO_MUX.mcu_sel`
//!   are **not** gated on. `mcu_sel = 1` (the matrix function) is what esp-hal
//!   writes before every route and `IO_MUX` stays an accept block; a pad
//!   routed here carries its signal whatever `mcu_sel` says.

use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{BusCx, PadId, Peripheral, RegFile, RouteSource, SignalId, Width};

use crate::regs;
use crate::regs::output_signals::{OUT_SEL_GPIO, output_signal_name};

/// The window length, unchanged from P5's accept block.
pub const LEN: u32 = 0x700;

/// Pads the C6 carries: `func_out_sel_cfg[0..31]`, `IO_MUX.gpio[0..31]`.
pub const PAD_COUNT: u32 = 31;

const OUT: u32 = 0x004;
const OUT_W1TS: u32 = 0x008;
const OUT_W1TC: u32 = 0x00c;
const ENABLE: u32 = 0x020;
const ENABLE_W1TS: u32 = 0x024;
const ENABLE_W1TC: u32 = 0x028;
const STRAP: u32 = 0x038;
const IN: u32 = 0x03c;
const PCPU_INT: u32 = 0x05c;
const FUNC_OUT_SEL_CFG: u32 = 0x554;
const FUNC_OUT_SEL_CFG_END: u32 = FUNC_OUT_SEL_CFG + 4 * PAD_COUNT;

const OUT_SEL_MASK: u32 = 0xff;
const INV_SEL: u32 = 1 << 8;
const OEN_SEL: u32 = 1 << 9;

/// `func_out_sel_cfg[n]` after reset: `out_sel = 128`, everything else 0.
const FUNC_OUT_SEL_RESET: u32 = 0x80;

/// The GPIO block.
#[derive(Debug)]
pub struct Gpio {
    regs: RegFile,
}

impl Default for Gpio {
    fn default() -> Self {
        Self::new(crate::loader::STRAP_APP)
    }
}

fn note(cx: &mut BusCx<'_>, f: impl FnOnce() -> String) {
    if cx.trace.is_enabled() {
        let line = f();
        cx.trace.note(&line);
    }
}

/// `RMT_SIG_0` / `sig71` — what the routing note calls `out_sel`.
fn signal_name(sel: u16) -> String {
    output_signal_name(sel).map_or_else(|| format!("sig{sel}"), str::to_string)
}

impl Gpio {
    /// `strap` is the word the pads were latched into at reset
    /// (`crate::loader::strap_word`). It is read-only on the chip and the
    /// mask ROM prints it verbatim as the `boot:0x%x` half of its banner, so
    /// it is a **read override**: a guest that wrote here would otherwise be
    /// able to change what the chip booted as.
    pub fn new(strap: u32) -> Self {
        let mut regs = RegFile::new("GPIO", LEN)
            .with_names(regs::GPIO)
            .with_read_override(STRAP, 0xffff_ffff, strap)
            .with_read_override(IN, 0xffff_ffff, 0)
            .with_read_override(PCPU_INT, 0xffff_ffff, 0);
        for pad in 0..PAD_COUNT {
            regs = regs.with_reset(FUNC_OUT_SEL_CFG + 4 * pad, FUNC_OUT_SEL_RESET);
        }
        Self { regs }
    }

    /// Re-latch the strapping pins, as a chip reset does. The word is a
    /// read override, so this replaces the rule rather than a stored value.
    pub fn set_strap(&mut self, strap: u32) {
        self.regs
            .set_read_override(STRAP, 0xffff_ffff, strap);
    }

    /// The `out` bitmap as last written.
    pub fn out(&self) -> u32 {
        self.regs.stored(OUT)
    }

    /// The `enable` bitmap as last written.
    pub fn enable(&self) -> u32 {
        self.regs.stored(ENABLE)
    }

    /// `func_out_sel_cfg[pad]` as last written.
    pub fn func_out_sel_cfg(&self, pad: u32) -> u32 {
        self.regs.stored(FUNC_OUT_SEL_CFG + 4 * pad)
    }

    /// Write the `out` bitmap and push every changed bit into the fabric.
    fn set_out(&mut self, new: u32, cx: &mut BusCx<'_>) {
        let old = self.regs.stored(OUT);
        if old == new {
            return;
        }
        self.regs.poke(OUT, new);
        let at = cx.now;
        let mut changed = old ^ new;
        while changed != 0 {
            let pad = changed.trailing_zeros();
            changed &= changed - 1;
            cx.pins
                .set_gpio_out(PadId(pad as u8), new & (1 << pad) != 0, at);
        }
    }

    /// Write the `enable` bitmap. Recorded only — see the module docs.
    fn set_enable(&mut self, new: u32, cx: &mut BusCx<'_>) {
        let old = self.regs.stored(ENABLE);
        if old == new {
            return;
        }
        self.regs.poke(ENABLE, new);
        let mut changed = old ^ new;
        while changed != 0 {
            let pad = changed.trailing_zeros();
            changed &= changed - 1;
            cx.pins
                .set_gpio_enable(PadId(pad as u8), new & (1 << pad) != 0);
        }
    }

    /// A write to `func_out_sel_cfg[pad]`: the routing itself.
    fn set_route(&mut self, pad: u32, value: u32, cx: &mut BusCx<'_>) {
        let off = FUNC_OUT_SEL_CFG + 4 * pad;
        let before = self.regs.stored(off);
        let was_routed = cx.pins.route_of(PadId(pad as u8)).is_some();
        self.regs.poke(off, value);
        let sel = (value & OUT_SEL_MASK) as u16;
        let invert = value & INV_SEL != 0;
        let oen_from_gpio = value & OEN_SEL != 0;
        let source = if sel == OUT_SEL_GPIO {
            RouteSource::GpioOut
        } else {
            RouteSource::Signal(SignalId(sel), invert)
        };
        let at = cx.now;
        cx.pins.route(PadId(pad as u8), source, oen_from_gpio, at);
        // One note per *change*: esp-hal rewrites the same routing on a
        // rebind of the same pin, and a note per write would say the pad
        // moved when it did not.
        if before == value && was_routed {
            return;
        }
        let oe = u8::from(self.regs.stored(ENABLE) & (1 << pad) != 0);
        let name = signal_name(sel);
        note(cx, || {
            format!(
                "cyc={at} PIN gpio{pad} <- {name} (out_sel={sel} inv={} oen_sel={} oe={oe})",
                u8::from(invert),
                u8::from(oen_from_gpio),
            )
        });
    }
}

impl Peripheral for Gpio {
    fn name(&self) -> &'static str {
        "GPIO"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        if matches!(word, OUT_W1TS | OUT_W1TC | ENABLE_W1TS | ENABLE_W1TC) {
            // Write-only in the PAC; nothing is remembered to read back.
            return 0;
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        // The lane the access actually touches, in word position: a byte
        // store to `out_w1ts + 1` sets bits 8..15.
        let bits = merge_lane(0, off, width, value);
        match word {
            OUT => {
                let new = merge_lane(self.regs.stored(OUT), off, width, value);
                self.set_out(new, cx);
            }
            OUT_W1TS => {
                let new = self.regs.stored(OUT) | bits;
                self.set_out(new, cx);
            }
            OUT_W1TC => {
                let new = self.regs.stored(OUT) & !bits;
                self.set_out(new, cx);
            }
            ENABLE => {
                let new = merge_lane(self.regs.stored(ENABLE), off, width, value);
                self.set_enable(new, cx);
            }
            ENABLE_W1TS => {
                let new = self.regs.stored(ENABLE) | bits;
                self.set_enable(new, cx);
            }
            ENABLE_W1TC => {
                let new = self.regs.stored(ENABLE) & !bits;
                self.set_enable(new, cx);
            }
            w if (FUNC_OUT_SEL_CFG..FUNC_OUT_SEL_CFG_END).contains(&w) => {
                let pad = (w - FUNC_OUT_SEL_CFG) / 4;
                let new = merge_lane(self.regs.stored(w), off, width, value);
                self.set_route(pad, new, cx);
            }
            _ => self.regs.write(off, width, value, cx),
        }
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::GPIO.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The routing lives in the fabric, which rides `BusScalars`; this is
        // the register file and nothing else.
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
    use lp_emu_esp_common::pins::Edge;

    const GPIO18: u32 = 18;
    const RMT_SIG_0: u32 = 71;
    const FUNC18: u32 = FUNC_OUT_SEL_CFG + 4 * GPIO18;

    #[test]
    fn the_esp_hal_with_pin_sequence_routes_the_pad_to_the_rmt_signal() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.now = 1_000;
        // `out_w1tc` bit 18 (idle low), `enable_w1ts` bit 18, then the route.
        sb.write(&mut g, OUT_W1TC, 1 << GPIO18);
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO18);
        sb.write(&mut g, FUNC18, RMT_SIG_0);

        assert_eq!(g.func_out_sel_cfg(GPIO18), RMT_SIG_0, "read back");
        assert_eq!(g.enable(), 1 << GPIO18);
        assert!(sb.pins.gpio_enable(PadId(18)));
        let route = sb.pins.route_of(PadId(18)).expect("routed");
        assert_eq!(
            route.source,
            RouteSource::Signal(SignalId(RMT_SIG_0 as u16), false)
        );
        assert!(!route.oen_from_gpio, "oen_sel = 0: the peripheral's OE");
        assert_eq!(sb.pins.routes().count(), 1, "no other pad is routed");

        // The RMT's signal now reaches the pad.
        sb.pins.drive(SignalId(RMT_SIG_0 as u16), true, 2_000);
        assert_eq!(
            sb.pins.take_edges(),
            [Edge {
                at: 2_000,
                pad: PadId(18),
                level: true
            }]
        );
    }

    #[test]
    fn the_guard_drop_puts_the_pad_back_on_gpio_out() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.write(&mut g, FUNC18, RMT_SIG_0);
        sb.now = 10;
        sb.write(&mut g, FUNC18, u32::from(OUT_SEL_GPIO));
        assert_eq!(
            sb.pins.route_of(PadId(18)).unwrap().source,
            RouteSource::GpioOut
        );
        let _ = sb.pins.take_edges();
        sb.now = 20;
        sb.write(&mut g, OUT_W1TS, 1 << GPIO18);
        assert_eq!(
            sb.pins.take_edges(),
            [Edge {
                at: 20,
                pad: PadId(18),
                level: true
            }]
        );
        assert_eq!(g.out(), 1 << GPIO18);
    }

    #[test]
    fn inv_sel_and_oen_sel_are_carried_into_the_route() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.write(&mut g, FUNC18, RMT_SIG_0 | INV_SEL | OEN_SEL);
        let route = sb.pins.route_of(PadId(18)).expect("routed");
        assert_eq!(
            route.source,
            RouteSource::Signal(SignalId(RMT_SIG_0 as u16), true)
        );
        assert!(route.oen_from_gpio);
        // An inverted route on a low signal is a high pad, from the route.
        assert!(sb.pins.pad_level(PadId(18)));
    }

    #[test]
    fn the_set_and_clear_registers_fold_into_out_and_read_zero() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.write(&mut g, OUT_W1TS, 0b1010);
        assert_eq!(g.out(), 0b1010);
        sb.write(&mut g, OUT_W1TC, 0b0010);
        assert_eq!(g.out(), 0b1000);
        sb.write(&mut g, OUT, 0b0101);
        assert_eq!(g.out(), 0b0101);
        assert_eq!(sb.read(&mut g, OUT), 0b0101);
        assert_eq!(sb.read(&mut g, OUT_W1TS), 0, "write-only");
        assert_eq!(sb.read(&mut g, OUT_W1TC), 0, "write-only");
        sb.write(&mut g, ENABLE_W1TS, 0b1100);
        assert_eq!(g.enable(), 0b1100);
        sb.write(&mut g, ENABLE_W1TC, 0b0100);
        assert_eq!(g.enable(), 0b1000);
        assert_eq!(sb.read(&mut g, ENABLE), 0b1000);
        assert_eq!(sb.read(&mut g, ENABLE_W1TS), 0, "write-only");
    }

    #[test]
    fn a_byte_lane_reaches_only_its_own_bits() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        // A byte store to `out_w1ts + 2` sets bits 16..23.
        g.write(OUT_W1TS + 2, Width::Byte, 0x04, &mut sb.cx());
        assert_eq!(g.out(), 1 << 18);
    }

    #[test]
    fn an_untouched_pad_is_not_routed_and_records_nothing() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        // Its `out` bit is tracked, so a later route starts at the right
        // level — but nothing is observed until something routes it.
        sb.write(&mut g, OUT_W1TS, 1 << 5);
        assert!(sb.pins.route_of(PadId(5)).is_none());
        assert!(sb.pins.take_edges().is_empty());
        assert_eq!(sb.pins.routes().count(), 0);
        sb.now = 99;
        sb.write(&mut g, FUNC_OUT_SEL_CFG + 4 * 5, u32::from(OUT_SEL_GPIO));
        assert_eq!(
            sb.pins.take_edges(),
            [Edge {
                at: 99,
                pad: PadId(5),
                level: true
            }],
            "the route picks up the out bit that was already set"
        );
    }

    #[test]
    fn the_accept_block_behaviour_the_boot_path_relies_on_is_unchanged() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.write(&mut g, IN, 0xffff_ffff);
        assert_eq!(sb.read(&mut g, IN), 0, "no pin is driven from outside");
        assert_eq!(sb.read(&mut g, PCPU_INT), 0);
        assert_eq!(g.reg_name(0x020), Some("enable"));
        assert_eq!(g.reg_name(0x59c), Some("func18_out_sel_cfg"));
        // Every pad's routing resets to `out_sel = 128`.
        for pad in 0..PAD_COUNT {
            assert_eq!(sb.read(&mut g, FUNC_OUT_SEL_CFG + 4 * pad), 0x80);
        }
        // A register outside the three groups is still accept-and-remember.
        sb.write(&mut g, 0x074, 0x1234);
        assert_eq!(sb.read(&mut g, 0x074), 0x1234);
    }

    #[test]
    fn the_state_blob_round_trips_the_registers() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::new(crate::loader::STRAP_APP);
        sb.write(&mut g, FUNC18, RMT_SIG_0);
        sb.write(&mut g, OUT_W1TS, 0x1234);
        let blob = g.save_state();
        let mut other = Gpio::new(crate::loader::STRAP_APP);
        other.load_state(&blob);
        assert_eq!(other.func_out_sel_cfg(GPIO18), RMT_SIG_0);
        assert_eq!(other.out(), 0x1234);
    }
}
