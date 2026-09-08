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
//! - `out_w1ts`/`out_w1tc`/`enable_w1ts`/`enable_w1tc` fold into `out` and
//!   `enable` and **read back 0** (the PAC declares them write-only). The
//!   accept block used to remember them separately, which meant `out` never
//!   reflected a `w1ts` — honest here, and nothing in the boot path reads one.
//! - `out1`/`enable1` (pads 32+) are accepted: the C6 has 31 pads.
//! - Drive strength, pull-ups, open-drain, pad filters and `IO_MUX.mcu_sel`
//!   are **not** gated on. `mcu_sel = 1` (the matrix function) is what esp-hal
//!   writes before every route; a pad routed here carries its signal whatever
//!   `mcu_sel` says. [`super::io_mux`] takes exactly one field out of that
//!   block — `fun_ie` — and the rest is still accept-and-remember.
//!
//! # The input side (M2 P1)
//!
//! The other direction, and the same shape: the fabric holds the state and
//! this block is the **view**.
//!
//! | register | what it does here |
//! |---|---|
//! | `in_` (`+0x03c`) | bit `n` is the fabric's **resolved** level for pad `n`, for every pad whose input enable ([`super::io_mux`]'s `fun_ie`) is set. A pad without it reads 0, which is what silicon's input buffer being off means. |
//! | `pin[n]` (`+0x074 + 4n`) | `int_type` (bits 7:9) and `int_ena` (bits 13:17) are decoded; the rest of the word is remembered and read back. |
//! | `status` (`+0x044`) | the per-pad interrupt **latch**. Sticky: an edge sets a bit and only a write clears it. |
//! | `status_w1ts` / `status_w1tc` (`+0x048` / `+0x04c`) | write-1-to-set and write-1-to-clear over `status`, read back 0 (write-only in the PAC) |
//! | `pcpu_int` (`+0x05c`) | `status & <the pads whose `int_ena` bit 0 is set>` — see below |
//!
//! **`int_type`**, from the PAC's own field doc (esp32c6 0.23.2,
//! `gpio/pin.rs`, `INT_TYPE`, bits 7:9): *"0:disable GPIO interrupt.
//! 1:trigger at posedge. 2:trigger at negedge. 3:trigger at any edge.
//! 4:valid at low level. 5:valid at high level"*. 6 and 7 are not values the
//! PAC names; they are treated as disabled and logged once.
//!
//! **What gates `pcpu_int`.** The PAC's `INT_ENA` field is bits 13:17 of
//! `pin[n]`, documented *"set bit 13 to enable CPU interrupt. set bit 14 to
//! enable CPU(not shielded) interrupt"* — so `int_ena` **bit 0**, which is
//! `pin[n]` **bit 13**, is the one that gates `pcpu_int`. esp-hal 1.1.1
//! writes exactly that: `listen_with_options`' `gpio_intr_enable(int_enable,
//! nmi_enable)` returns `int_enable as u8 | ((nmi_enable as u8) << 1)`, and
//! `Flex::listen` passes `nmi_enable = false` — so on this chip `int_ena` is
//! 1 and never 2.
//!
//! **Source 30 is a level, not a pulse.** [`crate::regs::source::GPIO`] is
//! held high while any `pcpu_int` bit is set and drops when the ISR clears
//! the last one through `status_w1tc`. That is what makes esp-hal's handler
//! work at all: it reads `status`, dispatches, and writes the bits back to
//! `status_w1tc`, and a pulse would have been missed or re-entered.
//!
//! # What the input side does not model
//!
//! - **`pcpu_nmi_int` (`+0x060`) and source 31 (`GPIO_NMI`)** are not raised.
//!   `int_ena` bit 1 is the NMI enable and esp-hal 1.1.1 never sets it on
//!   this chip (`gpio_intr_enable`, above), so the register is
//!   accept-and-remember and reads what the PAC resets it to, 0. A driver
//!   that started setting bit 14 would find it unmodelled rather than wrong,
//!   and the strict grade says so.
//! - **`in1` / `status1` / `pcpu_int1` (pads 32+) read 0.** The C6 has 31.
//! - **`status_next` / `cpusdio_int` / `func_in_sel_cfg`** are
//!   accept-and-remember: nothing routes an input *signal* to a peripheral
//!   yet, which is a different question from a pad's level and is not this
//!   phase's.
//! - **Sub-sample pulses.** The input side is evaluated at every access to
//!   this block and at every slice boundary, from the edges the fabric
//!   recorded — so an edge is never missed. But `wakeup_enable`,
//!   `sync1_bypass` / `sync2_bypass` (the input synchroniser) and the pad
//!   filter are not modelled at all: an edge is seen at the cycle it was
//!   stamped, with no synchroniser delay and no glitch rejection.
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. No committed transcript covers this block yet; M2 P2 earns the first, on a `gpio-input` payload with a silicon twin. |
//! | `documented` | `out`, `out_w1ts`, `out_w1tc`, `enable`, `enable_w1ts`, `enable_w1tc`, `strap`, `in_`, `status`, `status_w1ts`, `status_w1tc`, `pcpu_int`, `pin0`…`pin30`, `func0_out_sel_cfg`…`func30_out_sel_cfg` — the PAC's bit map is the source, and the behaviour above is that bit map read out loud. |
//! | `modeled` | everything else in the window: `bt_select`, `sdio_select`, `out1*`, `enable1*`, `in1`, `status1*`, `pcpu_nmi_int*`, `cpusdio_int*`, `pin31`…`pin34`, `status_next*`, `func*_in_sel_cfg`, `clock_gate`, `date` and the register at `+0x074`'s neighbours. Accept-and-remember, at the PAC's reset value: nothing on the C6 boot or run path reaches one, and a run under `--strict-grade documented` that did would stop with the register's name. |

use lp_emu_esp_common::pins::Edge;
use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{
    BusCx, PadId, Peripheral, RegFile, RegGrade, RegGrades, RouteSource, SignalId, Width,
};

use crate::regs;
use crate::regs::output_signals::{OUT_SEL_GPIO, output_signal_name};
use crate::regs::source;

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
const IN1: u32 = 0x040;
const STATUS: u32 = 0x044;
const STATUS_W1TS: u32 = 0x048;
const STATUS_W1TC: u32 = 0x04c;
const STATUS1: u32 = 0x050;
const STATUS1_W1TS: u32 = 0x054;
const STATUS1_W1TC: u32 = 0x058;
const PCPU_INT: u32 = 0x05c;
const PCPU_INT1: u32 = 0x068;
const PIN: u32 = 0x074;
const PIN_END: u32 = PIN + 4 * PAD_COUNT;
const FUNC_OUT_SEL_CFG: u32 = 0x554;
const FUNC_OUT_SEL_CFG_END: u32 = FUNC_OUT_SEL_CFG + 4 * PAD_COUNT;

const OUT_SEL_MASK: u32 = 0xff;
const INV_SEL: u32 = 1 << 8;
const OEN_SEL: u32 = 1 << 9;

/// `pin[n].int_type`, bits 7:9 (PAC `INT_TYPE`).
const INT_TYPE_SHIFT: u32 = 7;
const INT_TYPE_MASK: u32 = 0b111;
/// `pin[n].int_ena` bit 0 — `pin[n]` bit 13 — the PRO_CPU maskable enable
/// the PAC documents as *"set bit 13 to enable CPU interrupt"*.
const INT_ENA_CPU: u32 = 1 << 13;

/// What `pin[n].int_type` asks for, in the PAC's own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntType {
    /// 0: disable GPIO interrupt.
    Disabled,
    /// 1: trigger at posedge.
    Rising,
    /// 2: trigger at negedge.
    Falling,
    /// 3: trigger at any edge.
    AnyEdge,
    /// 4: valid at low level.
    LowLevel,
    /// 5: valid at high level.
    HighLevel,
}

impl IntType {
    /// 6 and 7 are values the PAC names nothing for; they are read as
    /// disabled, and the pad that asked is named once.
    fn decode(value: u32, pad: u32) -> Self {
        match (value >> INT_TYPE_SHIFT) & INT_TYPE_MASK {
            0 => IntType::Disabled,
            1 => IntType::Rising,
            2 => IntType::Falling,
            3 => IntType::AnyEdge,
            4 => IntType::LowLevel,
            5 => IntType::HighLevel,
            other => {
                log::warn!(
                    "GPIO: pin{pad}.int_type = {other}, which the PAC names no meaning for;                      read as disabled"
                );
                IntType::Disabled
            }
        }
    }
}

/// The GPIO block.
#[derive(Debug)]
pub struct Gpio {
    regs: RegFile,
    /// The input word as this block last sampled it: bit `n` is pad `n`'s
    /// resolved level *if* its input enable is set. Edge detection compares
    /// against it, so it rides the state blob.
    last_in: u32,
    /// Pads whose `int_type` is not `Disabled`. Kept so the level-sensitive
    /// pass walks the pads a driver actually armed rather than all 31 on
    /// every access.
    armed: u32,
    grades: RegGrades,
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
        let regs = RegFile::new("GPIO", LEN)
            .with_names(regs::GPIO)
            .with_read_override(STRAP, 0xffff_ffff, strap);
        Self {
            regs,
            last_in: 0,
            armed: 0,
            grades: Self::grades(),
        }
    }

    /// The per-register grade table (the file header's).
    ///
    /// `documented` is the PAC's bit map read out loud; nothing here is
    /// `measured`, because no committed transcript covers this block yet —
    /// M2 P2 earns the first. Everything unlisted is `Modeled`, which for
    /// this block means accept-and-remember at the PAC's reset value.
    pub fn grades() -> RegGrades {
        let mut g = RegGrades::new()
            .with_grade(OUT, RegGrade::Documented)
            .with_grade(OUT_W1TS, RegGrade::Documented)
            .with_grade(OUT_W1TC, RegGrade::Documented)
            .with_grade(ENABLE, RegGrade::Documented)
            .with_grade(ENABLE_W1TS, RegGrade::Documented)
            .with_grade(ENABLE_W1TC, RegGrade::Documented)
            .with_grade(STRAP, RegGrade::Documented)
            .with_grade(IN, RegGrade::Documented)
            .with_grade(STATUS, RegGrade::Documented)
            .with_grade(STATUS_W1TS, RegGrade::Documented)
            .with_grade(STATUS_W1TC, RegGrade::Documented)
            .with_grade(PCPU_INT, RegGrade::Documented);
        for pad in 0..PAD_COUNT {
            g = g
                .with_grade(PIN + 4 * pad, RegGrade::Documented)
                .with_grade(FUNC_OUT_SEL_CFG + 4 * pad, RegGrade::Documented);
        }
        g
    }

    /// Every register this block grades `Modeled`, in offset order — the
    /// list the README and `--strict-grade` both mean by "the modeled
    /// registers".
    pub fn modeled_registers() -> Vec<&'static str> {
        let grades = Self::grades();
        (0..LEN)
            .step_by(4)
            .filter(|off| grades.grade(*off) == RegGrade::Modeled)
            .filter_map(|off| regs::GPIO.name(off))
            .collect()
    }

    /// Re-latch the strapping pins, as a chip reset does. The word is a
    /// read override, so this replaces the rule rather than a stored value.
    pub fn set_strap(&mut self, strap: u32) {
        self.regs.set_read_override(STRAP, 0xffff_ffff, strap);
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

    // ---- the input side (M2 P1) ------------------------------------------

    /// The `in_` word: pad `n`'s **resolved** fabric level when its input
    /// enable is set, 0 when it is not.
    fn input_word(&self, cx: &BusCx<'_>) -> u32 {
        let mut word = 0;
        for pad in 0..PAD_COUNT {
            let id = PadId(pad as u8);
            if cx.pins.pad_input_enable(id) && cx.pins.pad_level(id) {
                word |= 1 << pad;
            }
        }
        word
    }

    /// `pin[pad].int_type`.
    fn int_type(&self, pad: u32) -> IntType {
        IntType::decode(self.regs.stored(PIN + 4 * pad), pad)
    }

    /// The pads whose `int_ena` bit 0 is set — what gates `pcpu_int`.
    fn cpu_enable_mask(&self) -> u32 {
        let mut mask = 0;
        for pad in 0..PAD_COUNT {
            if self.regs.stored(PIN + 4 * pad) & INT_ENA_CPU != 0 {
                mask |= 1 << pad;
            }
        }
        mask
    }

    /// `pcpu_int` as it reads now.
    fn pcpu_int(&self) -> u32 {
        self.regs.stored(STATUS) & self.cpu_enable_mask()
    }

    /// Recompute [`Gpio::armed`] from the `pin` registers. Called whenever
    /// one is written and after a state load.
    fn rearm(&mut self) {
        let mut armed = 0;
        for pad in 0..PAD_COUNT {
            if self.int_type(pad) != IntType::Disabled {
                armed |= 1 << pad;
            }
        }
        self.armed = armed;
    }

    /// Latch pad `pad`'s edge into `status` if its `int_type` asks for that
    /// direction. Sticky: only a write clears a bit.
    fn latch_edge(&mut self, pad: u32, rising: bool) {
        let wanted = match self.int_type(pad) {
            IntType::Rising => rising,
            IntType::Falling => !rising,
            IntType::AnyEdge => true,
            IntType::Disabled | IntType::LowLevel | IntType::HighLevel => false,
        };
        if wanted {
            let status = self.regs.stored(STATUS) | (1 << pad);
            self.regs.poke(STATUS, status);
        }
    }

    /// Resample the input word, latch the level-sensitive pads, and drive
    /// source 30 from `pcpu_int`.
    ///
    /// The last step of every path into this block's input side, so there is
    /// exactly one place the interrupt line is set.
    fn finish(&mut self, cx: &mut BusCx<'_>) {
        let now = self.input_word(cx);
        self.last_in = now;
        let mut armed = self.armed;
        while armed != 0 {
            let pad = armed.trailing_zeros();
            armed &= armed - 1;
            let high = now & (1 << pad) != 0;
            let wanted = match self.int_type(pad) {
                IntType::LowLevel => !high,
                IntType::HighLevel => high,
                _ => false,
            };
            if wanted {
                let status = self.regs.stored(STATUS) | (1 << pad);
                self.regs.poke(STATUS, status);
            }
        }
        // A level, not a pulse: high while anything is pending, low when the
        // handler has cleared the last bit.
        cx.irq.set_level(source::GPIO, self.pcpu_int() != 0);
    }

    /// Sample the fabric and treat every bit that moved since the last
    /// sample as an edge.
    ///
    /// The path for a change this block itself caused inside one access — a
    /// write to `out` on a pad that is also input-enabled, which is the
    /// self-loop `Flex` gives you — and the belt-and-braces path on a read.
    fn sync(&mut self, cx: &mut BusCx<'_>) {
        let now = self.input_word(cx);
        let mut changed = (now ^ self.last_in) & self.armed;
        while changed != 0 {
            let pad = changed.trailing_zeros();
            changed &= changed - 1;
            self.latch_edge(pad, now & (1 << pad) != 0);
        }
        self.finish(cx);
    }

    /// The machine hands this block the edges it drained off the fabric at a
    /// slice boundary, in order — the same [`Edge`] stream the pin log and
    /// the strip decoders see.
    ///
    /// Per-edge rather than a before/after diff, so a pad that rose and fell
    /// inside one slice latches both directions and neither is lost.
    pub fn observe_edges(&mut self, edges: &[Edge], cx: &mut BusCx<'_>) {
        for edge in edges {
            let pad = u32::from(edge.pad.0);
            if pad >= PAD_COUNT || self.armed & (1 << pad) == 0 {
                continue;
            }
            if !cx.pins.pad_input_enable(edge.pad) {
                continue;
            }
            self.latch_edge(pad, edge.level);
        }
        self.finish(cx);
    }

    /// The interrupt latch as it stands (`status`), for a test.
    pub fn status(&self) -> u32 {
        self.regs.stored(STATUS)
    }

    /// Write the interrupt latch and re-evaluate the interrupt line.
    fn set_status(&mut self, new: u32, cx: &mut BusCx<'_>) {
        self.regs.poke(STATUS, new);
        self.finish(cx);
    }
}

impl Peripheral for Gpio {
    fn name(&self) -> &'static str {
        "GPIO"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        if matches!(
            word,
            OUT_W1TS
                | OUT_W1TC
                | ENABLE_W1TS
                | ENABLE_W1TC
                | STATUS_W1TS
                | STATUS_W1TC
                | STATUS1_W1TS
                | STATUS1_W1TC
        ) {
            // Write-only in the PAC; nothing is remembered to read back.
            return 0;
        }
        if matches!(word, IN | STATUS | PCPU_INT) {
            // Fresh at the read, so a guest that polls `in_` between slice
            // boundaries still sees the pad, and `status`/`pcpu_int` answer
            // the same word the interrupt line was set from.
            self.sync(cx);
            let value = match word {
                IN => self.last_in,
                STATUS => self.regs.stored(STATUS),
                _ => self.pcpu_int(),
            };
            self.regs.poke(word, value);
        }
        if matches!(word, IN1 | STATUS1 | PCPU_INT1) {
            // The C6 has 31 pads; bank 1 is empty and says so.
            self.regs.poke(word, 0);
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
                self.sync(cx);
            }
            OUT_W1TS => {
                let new = self.regs.stored(OUT) | bits;
                self.set_out(new, cx);
                self.sync(cx);
            }
            OUT_W1TC => {
                let new = self.regs.stored(OUT) & !bits;
                self.set_out(new, cx);
                self.sync(cx);
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
            STATUS => {
                let new = merge_lane(self.regs.stored(STATUS), off, width, value);
                self.set_status(new, cx);
            }
            STATUS_W1TS => {
                let new = self.regs.stored(STATUS) | bits;
                self.set_status(new, cx);
            }
            STATUS_W1TC => {
                let new = self.regs.stored(STATUS) & !bits;
                self.set_status(new, cx);
            }
            w if (PIN..PIN_END).contains(&w) => {
                self.regs.write(off, width, value, cx);
                self.rearm();
                // A pad that has just been armed for a level it is already
                // sitting at is pending from here, and one whose `int_ena`
                // changed moves the interrupt line.
                self.sync(cx);
            }
            w if (FUNC_OUT_SEL_CFG..FUNC_OUT_SEL_CFG_END).contains(&w) => {
                let pad = (w - FUNC_OUT_SEL_CFG) / 4;
                let new = merge_lane(self.regs.stored(w), off, width, value);
                self.set_route(pad, new, cx);
                self.sync(cx);
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

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        Some(self.grades.grade(off))
    }

    fn save_state(&self) -> Vec<u8> {
        // The routing and the pad levels live in the fabric, which rides
        // `BusScalars`. This is the register file plus the one sample this
        // block keeps of its own: the input word edge detection compares
        // against, which a restore that lost it would read as a whole
        // word's worth of edges.
        let mut blob = self.regs.save_state();
        blob.extend_from_slice(&self.last_in.to_le_bytes());
        blob
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let split = bytes.len().saturating_sub(4);
        let (regs, tail) = bytes.split_at(split);
        self.regs.load_state(regs);
        self.last_in = <[u8; 4]>::try_from(tail)
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        self.rearm();
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
        assert_eq!(sb.read(&mut g, STATUS), 0, "nothing is pending at reset");
        assert!(!sb.irq.level(source::GPIO), "and source 30 is low");
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

    // ---- the input side (M2 P1) ------------------------------------------
    //
    // G1-1: these replay esp-hal 1.1.1's own register sequences. Every write
    // below carries the `gpio/mod.rs` line it comes from, and the order is
    // the driver's, not ours.

    use super::super::io_mux::IoMux;

    /// A free pad: not GPIO9 (BOOT), 12/13 (USB D-/D+), 16/17 (UART0) or 18
    /// (the strip). The same pad `test_button` uses on the desk board.
    const GPIO20: u32 = 20;
    const PIN20: u32 = PIN + 4 * GPIO20;
    const FUNC20: u32 = FUNC_OUT_SEL_CFG + 4 * GPIO20;
    /// `IO_MUX.gpio20`, `+0x004 + 4n`.
    const IOMUX20: u32 = 0x004 + 4 * GPIO20;
    const M20: u32 = 1 << GPIO20;

    /// `IO_MUX.gpio[n]` bits esp-hal writes: `fun_ie` 9, `fun_wpu` 8,
    /// `fun_wpd` 7, `mcu_sel` 12:14, `slp_sel` 1. Reset is `0x0800`
    /// (`fun_drv = 2`).
    const FUN_IE: u32 = 1 << 9;
    const FUN_WPU: u32 = 1 << 8;
    const MCU_SEL_GPIO: u32 = 1 << 12;
    const IOMUX_RESET: u32 = 0x0800;

    /// `pin[n].int_ena = 1` — `gpio_intr_enable(int_enable: true,
    /// nmi_enable: false)` in `listen_with_options`.
    const INT_ENA_1: u32 = 1 << 13;

    fn int_type_bits(event: u32) -> u32 {
        event << 7
    }

    /// The two blocks a pin driver writes, over one fabric.
    struct Chip {
        sb: Sandbox,
        g: Gpio,
        m: IoMux,
    }

    impl Chip {
        fn new() -> Self {
            Self {
                sb: Sandbox::new(),
                g: Gpio::new(crate::loader::STRAP_APP),
                m: IoMux::new(),
            }
        }

        fn gpio_w(&mut self, off: u32, value: u32) {
            self.sb.write(&mut self.g, off, value);
        }

        fn gpio_r(&mut self, off: u32) -> u32 {
            self.sb.read(&mut self.g, off)
        }

        fn iomux_w(&mut self, off: u32, value: u32) {
            self.sb.write(&mut self.m, off, value);
        }

        fn iomux_r(&mut self, off: u32) -> u32 {
            self.sb.read(&mut self.m, off)
        }

        /// `AnyPin::init_gpio` (`gpio/mod.rs:1707`), which every `Flex::new`
        /// runs first:
        ///
        /// ```text
        /// self.set_output_enable(false);          -> GPIO.enable_w1tc = 1 << n
        /// self.disable_usb_pads();                -> nothing on a non-USB pad
        /// GPIO::regs().func_out_sel_cfg(n)
        ///     .write(|w| w.out_sel().bits(OutputSignal::GPIO as _));
        /// io_mux_reg(n).modify(|_, w| {
        ///     w.mcu_sel().bits(AlternateFunction::GPIO as u8);
        ///     w.fun_ie().clear_bit();
        ///     w.slp_sel().clear_bit()
        /// });
        /// ```
        fn init_gpio(&mut self, pad: u32) {
            self.gpio_w(ENABLE_W1TC, 1 << pad);
            self.gpio_w(FUNC_OUT_SEL_CFG + 4 * pad, u32::from(OUT_SEL_GPIO));
            let word = self.iomux_r(0x004 + 4 * pad);
            self.iomux_w(0x004 + 4 * pad, (word | MCU_SEL_GPIO) & !FUN_IE & !(1 << 1));
        }

        /// An outside driver — a button to ground, a script line — and the
        /// slice boundary that hands the edges to the block.
        fn drive(&mut self, pad: u32, level: bool, at: u64) {
            self.sb.now = at;
            self.sb.pins.drive_pad(PadId(pad as u8), level, at);
            self.slice_boundary();
        }

        /// What `Esp32C6Machine::drain_pins` does: take the slice's edges off
        /// the fabric and hand them to this block.
        fn slice_boundary(&mut self) {
            let edges = self.sb.pins.take_edges();
            let mut cx = self.sb.cx();
            self.g.observe_edges(&edges, &mut cx);
        }
    }

    /// **G1-1 (a).** `Input::new(pin, InputConfig::default().with_pull(Pull::Up))`
    /// then `is_low()` across a scripted press.
    ///
    /// `Input::new` (`gpio/mod.rs:1090`) is
    ///
    /// ```text
    /// let mut pin = Flex::new(pin);   // -> init_gpio()
    /// pin.set_output_enable(false);   // -> GPIO.enable_w1tc = 1 << n
    /// pin.set_input_enable(true);     // -> io_mux_reg(n).modify(w.fun_ie().bit(true))
    /// pin.apply_input_config(&config);// -> io_mux_reg(n).modify(fun_wpd=0, fun_wpu=1)
    /// ```
    ///
    /// and `is_low()` (`:1159`) is `level() == Level::Low`, `level()` is
    /// `pin.is_input_high()` (`:1971`), which is
    /// `GPIO::regs().in_().read().bits() & (1 << n)` (`:555`).
    #[test]
    fn g1_1a_esp_hal_input_new_with_a_pull_up_reads_the_scripted_press() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        c.gpio_w(ENABLE_W1TC, M20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, (word & !(1 << 7)) | FUN_WPU);

        // The driver's own read-back, register by register.
        assert_eq!(
            c.iomux_r(IOMUX20),
            IOMUX_RESET | MCU_SEL_GPIO | FUN_IE | FUN_WPU,
            "IO_MUX.gpio20 after Input::new"
        );
        assert_eq!(
            c.gpio_r(FUNC20),
            u32::from(OUT_SEL_GPIO),
            "init_gpio's route"
        );
        assert_eq!(c.gpio_r(ENABLE) & M20, 0, "the output driver is off");
        assert!(
            c.sb.pins.pad_input_enable(PadId(20)),
            "fun_ie reached the fabric"
        );

        // Nothing is pressing it: the pad reads low, so `is_low()` is true.
        assert_eq!(c.gpio_r(IN) & M20, 0);

        // The button closes to ground — held high by the script, released
        // low. (A pull-up's *value* is not modelled; the script states the
        // level, which is what `--pin-script` is for.)
        c.drive(GPIO20, true, 1_000);
        assert_eq!(c.gpio_r(IN) & M20, M20, "is_low() == false");
        assert_eq!(c.gpio_r(IN), M20, "and no other pad moved");

        c.drive(GPIO20, false, 2_000);
        assert_eq!(c.gpio_r(IN) & M20, 0, "is_low() == true");

        // A pad whose input enable is clear reads 0 however it is driven.
        c.sb.pins.drive_pad(PadId(21), true, 3_000);
        assert_eq!(c.gpio_r(IN), 0, "gpio21 has no fun_ie");
        assert!(
            c.sb.pins.pad_level(PadId(21)),
            "the fabric still carries it"
        );

        // No interrupt was asked for, so none is pending.
        assert_eq!(c.gpio_r(STATUS), 0);
        assert!(!c.sb.irq.level(source::GPIO));
    }

    /// **G1-1 (b).** `Flex` with input **and** output enabled on one pad: a
    /// write to `GPIO_OUT` is read back through `in_`.
    ///
    /// This is the self-loop M2 P2's silicon twin uses — no wire, no hands —
    /// so it has to work here before that payload is written.
    ///
    /// ```text
    /// let mut pin = Flex::new(pin);      // -> init_gpio()
    /// pin.set_input_enable(true);        // -> io_mux fun_ie = 1
    /// pin.set_output_enable(true);       // -> GPIO.enable_w1ts = 1 << n
    /// pin.set_high();                    // -> GPIO.out_w1ts  = 1 << n   (:599)
    /// pin.is_high();                     // -> GPIO.in_       (:555)
    /// ```
    #[test]
    fn g1_1b_a_flex_pad_with_input_and_output_reads_its_own_output_back() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        c.gpio_w(ENABLE_W1TS, M20);

        assert_eq!(c.gpio_r(IN) & M20, 0, "low before anything is written");

        c.gpio_w(OUT_W1TS, M20);
        assert_eq!(c.gpio_r(IN) & M20, M20, "the pad reads its own output");
        assert_eq!(c.gpio_r(OUT) & M20, M20, "is_set_high()");

        c.gpio_w(OUT_W1TC, M20);
        assert_eq!(c.gpio_r(IN) & M20, 0);

        // And the loop is the fabric's, not a shortcut inside this block:
        // the edges are in the same stream the pin log reads.
        let edges = c.sb.pins.take_edges();
        assert_eq!(edges.len(), 2, "{edges:?}");
        assert_eq!(edges[0].pad, PadId(20));
        assert!(edges[0].level);
        assert!(!edges[1].level);
    }

    /// **G1-1 (c).** `Input::listen(Event::RisingEdge)`, the edge, source 30,
    /// the ISR's `status_w1tc`, source 30 low.
    ///
    /// `Flex::listen` → `listen_with_options(event, true, false, false)`
    /// (`gpio/mod.rs:1894`):
    ///
    /// ```text
    /// self.with_gpio_lock(|| {              // -> reads GPIO.pin(n).int_ena (:2324)
    ///     self.clear_interrupt();           // -> GPIO.status_w1tc = 1 << n (:579)
    ///     set_int_enable(n, Some(gpio_intr_enable(true, false)), event as u8, false);
    /// });                                   // -> GPIO.pin(n).modify: int_ena = 1,
    ///                                       //    int_type = event, wakeup_enable = 0
    /// ```
    ///
    /// `gpio_intr_enable(true, false)` is `int_enable as u8 | ((nmi_enable
    /// as u8) << 1)` = **1**, so `pin[n]` bit 13 and never bit 14.
    /// `Event::RisingEdge = 1`, `FallingEdge = 2`, `AnyEdge = 3`
    /// (`gpio/mod.rs:144-155`).
    ///
    /// The handler's half is `read_interrupt_status()` →
    /// `GPIO::regs().status().read()` (`:571`) then
    /// `write_interrupt_status_clear(mask)` → `GPIO.status_w1tc` (`:579`).
    #[test]
    fn g1_1c_listen_for_a_rising_edge_raises_source_30_until_the_isr_clears_it() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);

        // `listen(Event::RisingEdge)`.
        assert_eq!(c.gpio_r(PIN20) & 0x3_e000, 0, "is_int_enabled: no");
        c.gpio_w(STATUS_W1TC, M20);
        let pin = c.gpio_r(PIN20);
        c.gpio_w(PIN20, pin | INT_ENA_1 | int_type_bits(1));

        assert_eq!(
            c.gpio_r(PIN20),
            INT_ENA_1 | int_type_bits(1),
            "pin20 read back"
        );
        assert_eq!(c.gpio_r(STATUS), 0);
        assert!(!c.sb.irq.level(source::GPIO));

        // The edge.
        c.drive(GPIO20, true, 1_000);
        assert_eq!(c.gpio_r(STATUS), M20, "status latched the posedge");
        assert_eq!(c.gpio_r(PCPU_INT), M20, "gated by pin20.int_ena bit 0");
        assert!(c.sb.irq.level(source::GPIO), "source 30 is high");

        // It is a LEVEL: it stays high while the bit is pending, across as
        // many slice boundaries as you like.
        c.slice_boundary();
        c.slice_boundary();
        assert!(c.sb.irq.level(source::GPIO));

        // The ISR.
        assert_eq!(c.gpio_r(STATUS), M20, "the handler reads it");
        c.gpio_w(STATUS_W1TC, M20);
        assert_eq!(c.gpio_r(STATUS), 0);
        assert_eq!(c.gpio_r(PCPU_INT), 0);
        assert!(
            !c.sb.irq.level(source::GPIO),
            "and drops when the last is cleared"
        );

        // A falling edge is not a posedge: nothing latches.
        c.drive(GPIO20, false, 2_000);
        assert_eq!(c.gpio_r(STATUS), 0);
        assert!(!c.sb.irq.level(source::GPIO));

        // The next rising one does.
        c.drive(GPIO20, true, 3_000);
        assert!(c.sb.irq.level(source::GPIO));
    }

    /// **G1-1 (d).** The same on **both** edges (`Event::AnyEdge = 3`), which
    /// is what M2 P2's quadrature encoder needs.
    #[test]
    fn g1_1d_any_edge_latches_both_directions() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        c.gpio_w(STATUS_W1TC, M20);
        c.gpio_w(PIN20, INT_ENA_1 | int_type_bits(3));

        for (n, level) in [(0, true), (1, false), (2, true), (3, false)] {
            c.drive(GPIO20, level, 1_000 + n * 1_000);
            assert_eq!(c.gpio_r(STATUS), M20, "edge {n} ({level}) latched");
            assert!(c.sb.irq.level(source::GPIO), "edge {n}");
            c.gpio_w(STATUS_W1TC, M20);
            assert!(!c.sb.irq.level(source::GPIO), "edge {n} cleared");
        }

        // Both directions inside ONE slice latch — the block reads the
        // fabric's edge stream, not a before/after snapshot.
        c.sb.now = 9_000;
        c.sb.pins.drive_pad(PadId(20), true, 9_000);
        c.sb.pins.drive_pad(PadId(20), false, 9_100);
        c.slice_boundary();
        assert_eq!(c.gpio_r(STATUS), M20, "the pulse was not lost");
    }

    /// The falling half of the encoder's pair, and the two level types, from
    /// the PAC's own numbering: 2 negedge, 4 low level, 5 high level.
    #[test]
    fn every_int_type_the_pac_names_behaves_the_way_it_is_named() {
        // 2: trigger at negedge.
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        c.gpio_w(PIN20, INT_ENA_1 | int_type_bits(2));
        c.drive(GPIO20, true, 1_000);
        assert_eq!(c.gpio_r(STATUS), 0, "a posedge is not a negedge");
        c.drive(GPIO20, false, 2_000);
        assert_eq!(c.gpio_r(STATUS), M20);

        // 4: valid at low level — pending for as long as the pad is low,
        // so clearing it while the level holds is pending again at once.
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        c.gpio_w(PIN20, INT_ENA_1 | int_type_bits(4));
        assert_eq!(c.gpio_r(STATUS), M20, "the pad is already low");
        c.gpio_w(STATUS_W1TC, M20);
        assert_eq!(c.gpio_r(STATUS), M20, "and it is low again immediately");
        c.drive(GPIO20, true, 1_000);
        c.gpio_w(STATUS_W1TC, M20);
        assert_eq!(c.gpio_r(STATUS), 0, "high: nothing to be valid at");

        // 5: valid at high level.
        c.gpio_w(PIN20, INT_ENA_1 | int_type_bits(5));
        assert_eq!(c.gpio_r(STATUS), M20);

        // 0: disabled.
        c.gpio_w(PIN20, INT_ENA_1);
        c.gpio_w(STATUS_W1TC, M20);
        c.drive(GPIO20, false, 2_000);
        c.drive(GPIO20, true, 3_000);
        assert_eq!(c.gpio_r(STATUS), 0);
        assert!(!c.sb.irq.level(source::GPIO));
    }

    /// `int_ena` bit 0 (`pin[n]` bit 13) is what gates `pcpu_int`, and
    /// therefore source 30. `status` latches either way.
    #[test]
    fn status_latches_without_int_ena_and_pcpu_int_does_not() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        // int_type set, int_ena clear.
        c.gpio_w(PIN20, int_type_bits(1));
        c.drive(GPIO20, true, 1_000);
        assert_eq!(c.gpio_r(STATUS), M20, "the latch does not need the enable");
        assert_eq!(c.gpio_r(PCPU_INT), 0, "but the CPU's view does");
        assert!(!c.sb.irq.level(source::GPIO));

        // Setting the enable with the bit already pending raises it.
        c.gpio_w(PIN20, int_type_bits(1) | INT_ENA_1);
        assert_eq!(c.gpio_r(PCPU_INT), M20);
        assert!(c.sb.irq.level(source::GPIO));

        // `int_ena` bit 1 (the NMI enable, `pin[n]` bit 14) gates nothing
        // here: source 31 is not modelled, and esp-hal never sets it.
        c.gpio_w(PIN20, int_type_bits(1) | (1 << 14));
        assert_eq!(c.gpio_r(PCPU_INT), 0);
        assert!(!c.sb.irq.level(source::GPIO));
    }

    /// Two pads pending at once: source 30 drops only when the LAST one is
    /// cleared, which is the whole reason it is a level.
    #[test]
    fn source_30_drops_only_when_the_last_pending_pad_is_cleared() {
        let mut c = Chip::new();
        for pad in [20u32, 21] {
            c.init_gpio(pad);
            let word = c.iomux_r(0x004 + 4 * pad);
            c.iomux_w(0x004 + 4 * pad, word | FUN_IE);
            c.gpio_w(PIN + 4 * pad, INT_ENA_1 | int_type_bits(1));
        }
        c.drive(20, true, 1_000);
        c.drive(21, true, 1_100);
        assert_eq!(c.gpio_r(STATUS), M20 | (1 << 21));
        assert!(c.sb.irq.level(source::GPIO));

        c.gpio_w(STATUS_W1TC, M20);
        assert!(c.sb.irq.level(source::GPIO), "gpio21 is still pending");
        c.gpio_w(STATUS_W1TC, 1 << 21);
        assert!(!c.sb.irq.level(source::GPIO));
    }

    /// `status` is read/write in the PAC; `status_w1ts`/`status_w1tc` are
    /// write-only and read back 0, the way `out_w1ts` already did.
    #[test]
    fn the_status_set_and_clear_registers_fold_into_status_and_read_zero() {
        let mut c = Chip::new();
        c.gpio_w(STATUS_W1TS, 0b1010);
        assert_eq!(c.gpio_r(STATUS), 0b1010);
        c.gpio_w(STATUS_W1TC, 0b0010);
        assert_eq!(c.gpio_r(STATUS), 0b1000);
        c.gpio_w(STATUS, 0b0101);
        assert_eq!(c.gpio_r(STATUS), 0b0101);
        assert_eq!(c.gpio_r(STATUS_W1TS), 0, "write-only");
        assert_eq!(c.gpio_r(STATUS_W1TC), 0, "write-only");
    }

    /// The C6 has 31 pads: bank 1 is empty and says so.
    #[test]
    fn the_second_bank_is_empty_because_the_chip_has_thirty_one_pads() {
        let mut c = Chip::new();
        c.sb.pins.set_pad_input_enable(PadId(40), true);
        c.sb.pins.drive_pad(PadId(40), true, 10);
        c.gpio_w(0x050, 0xffff_ffff);
        assert_eq!(c.gpio_r(0x040), 0, "in1");
        assert_eq!(c.gpio_r(0x050), 0, "status1");
        assert_eq!(c.gpio_r(0x068), 0, "pcpu_int1");
        assert_eq!(c.gpio_r(0x054), 0, "status1_w1ts is write-only");
    }

    /// A wired pair, through the block that reads it: what `--wire a:b` does
    /// for M2 P3's RMT loopback and for G1-2.
    #[test]
    fn a_wire_carries_a_pads_output_into_another_pads_in_register() {
        let mut c = Chip::new();
        c.sb.pins.wire(PadId(18), PadId(19), 0).unwrap();
        // gpio19 is the input side.
        c.init_gpio(19);
        let word = c.iomux_r(0x004 + 4 * 19);
        c.iomux_w(0x004 + 4 * 19, word | FUN_IE);
        // gpio18 drives, through its own `out` bit.
        c.init_gpio(18);
        c.gpio_w(ENABLE_W1TS, 1 << 18);

        c.gpio_w(OUT_W1TS, 1 << 18);
        assert_eq!(c.gpio_r(IN) & (1 << 19), 1 << 19, "gpio18 reaches gpio19");
        // gpio18 itself has no fun_ie, so it reads 0 — the input enable is
        // per pad, not per wire.
        assert_eq!(c.gpio_r(IN) & (1 << 18), 0);

        c.gpio_w(OUT_W1TC, 1 << 18);
        assert_eq!(c.gpio_r(IN) & (1 << 19), 0);
    }

    #[test]
    fn the_input_sample_rides_the_state_blob_so_a_restore_is_not_a_word_of_edges() {
        let mut c = Chip::new();
        c.init_gpio(GPIO20);
        let word = c.iomux_r(IOMUX20);
        c.iomux_w(IOMUX20, word | FUN_IE);
        c.gpio_w(PIN20, INT_ENA_1 | int_type_bits(1));
        c.drive(GPIO20, true, 1_000);
        c.gpio_w(STATUS_W1TC, M20);
        assert_eq!(c.gpio_r(IN), M20);

        let blob = c.g.save_state();
        let mut other = Gpio::new(crate::loader::STRAP_APP);
        other.load_state(&blob);
        assert_eq!(other.status(), 0);
        // The pad is still high; a restore that lost the sample would read
        // that as a fresh posedge.
        let edges = c.sb.pins.take_edges();
        let mut cx = c.sb.cx();
        other.observe_edges(&edges, &mut cx);
        assert_eq!(other.status(), 0, "no phantom edge");
    }

    /// The grade table is the file header's, read back.
    #[test]
    fn the_register_grades_are_documented_where_the_pac_is_the_source() {
        let g = Gpio::new(crate::loader::STRAP_APP);
        for off in [
            OUT,
            OUT_W1TS,
            ENABLE,
            STRAP,
            IN,
            STATUS,
            STATUS_W1TS,
            STATUS_W1TC,
            PCPU_INT,
            PIN20,
            FUNC20,
        ] {
            assert_eq!(g.reg_grade(off), Some(RegGrade::Documented), "{off:#05x}");
        }
        for off in [0x000, 0x010, 0x040, 0x050, 0x060, 0x154] {
            assert_eq!(g.reg_grade(off), Some(RegGrade::Modeled), "{off:#05x}");
        }
        let modeled = Gpio::modeled_registers();
        assert!(modeled.contains(&"in1"), "{modeled:?}");
        assert!(modeled.contains(&"pcpu_nmi_int"), "{modeled:?}");
        assert!(!modeled.contains(&"in_"), "{modeled:?}");
        assert!(!modeled.contains(&"pin30"), "{modeled:?}");
    }
}
