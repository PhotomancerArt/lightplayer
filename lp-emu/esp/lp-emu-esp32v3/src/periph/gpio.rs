//! `GPIO` at `0x3FF4_4000`: the classic's matrix, as a routing **view**.
//!
//! P3–P7 had this block as accept-and-remember ([`super::accept::gpio`]) and
//! everything the boot path writes here still lands in a [`RegFile`]. What P8
//! adds is that four groups of registers now *mean* something to the machine:
//! they are written into the bus's signal fabric
//! ([`lp_emu_esp_common::pins`], plan DD34 e), which is the one state a
//! peripheral and this block can share — because a peripheral never sees
//! another peripheral.
//!
//! | register | what it does here |
//! |---|---|
//! | `func_out_sel_cfg[n]` (`+0x530 + 4n`, n < 40) | `out_sel` names the signal pad `n` follows; **256** means "follow `GPIO_OUT[n]`". `inv_sel` inverts. `oen_sel`/`oen_inv_sel` are recorded and reported, never gated on. |
//! | `out` / `out_w1ts` / `out_w1tc` and the `out1*` bank | the GPIO output bitmap a pad routed to `GPIO_OUT` follows |
//! | `enable` / `enable_w1ts` / `enable_w1tc` and the `enable1*` bank | the output-**enable** bitmap. This is what makes a routed pad *drive* the wire (plan DD38): a pad whose bit is clear carries whatever the wire carries and contributes nothing. |
//! | `func_in_sel_cfg[s]` (`+0x130 + 4s`, s < 256) | the **input** half: `in_sel` names the pad signal `s` reads, `in_inv_sel` inverts it, `sel = 1` is the matrix route. |
//!
//! # What the classic is, that the C6 is not: **two banks**
//!
//! Forty pads, in two 32-bit registers each for `out`, `enable`, `in`,
//! `status` and every per-core interrupt output. `out1`, `enable1`, `in1`,
//! `status1`, `pcpu_int1`, `acpu_int1` are **live registers carrying pads
//! 32..39**, not the padding they are on the C6 — the desk board's own strip
//! wires are IO18/IO13/IO2/IO14/IO16 (all bank 0), but a pad in bank 1 that
//! read 0 for ever would be a silent lie, and `MAX_PADS = 64` in `pins.rs`
//! already covers it.
//!
//! The second difference is per-core: the classic has **`pcpu_int` and
//! `acpu_int`**, one interrupt output per core, gated by different bits of
//! the same `pin[n].int_ena` field. M3 runs one core, and both are computed
//! anyway — see the `int_ena` note below.
//!
//! And the third is the sizes: **256 input signals** (`func0_in_sel_cfg` …
//! `func255_in_sel_cfg`) against the C6's 128, and `out_sel` is **9 bits**
//! (`0:8`, PAC: *"select one of the 256 output to 40 GPIO"*) against the C6's
//! 8, so the "follow `GPIO_OUT`" selector is **256**, not 128
//! (`OutputSignal::GPIO = 256`, `esp-metadata-generated-0.4.0`'s
//! `_generated_esp32.rs`). A view that reused the C6's constants would route
//! every plain output pad to signal 0 — `SPICLK` — and it would look like it
//! worked, because nothing drives `SPICLK` either.
//!
//! # What is observed, and what is not
//!
//! - **A pad becomes observed when the guest writes its `func_out_sel_cfg`.**
//!   The classic's register resets to **0**, not to the C6's `0x80`, so an
//!   untouched pad is not even nominally routed; it is left unobserved and
//!   its `out` bit is still tracked, so the moment anything routes it the
//!   level is already right.
//! - `out_w1ts`/`out_w1tc`/`enable_w1ts`/`enable_w1tc` and their bank-1
//!   twins fold into `out`/`out1`/`enable`/`enable1` and **read back 0** (the
//!   PAC declares them write-only).
//! - `strap` (`+0x038`) is a **read override**: the pads as they were latched
//!   at reset. It is read-only on the chip and the mask ROM prints it
//!   verbatim as the `boot:0x%x` half of its banner, so a guest that wrote
//!   here must not be able to change what the chip booted as. The desk
//!   board's own banner reads `boot:0x13 (SPI_FAST_FLASH_BOOT)`
//!   (`../bench.md`), which is where the default comes from; it is a builder
//!   parameter ([`crate::machine::Esp32V3Builder::strap`]) because a
//!   download-mode boot is a different word.
//! - Drive strength, pull-ups, open-drain, pad filters and `IO_MUX.mcu_sel`
//!   are **not** gated on. `mcu_sel = 2` is the classic's GPIO-matrix
//!   function (`gpio.gpio_function` in the generated metadata — the C6's is
//!   1); a pad routed here carries its signal whatever `mcu_sel` says.
//!   [`super::io_mux`] takes exactly one field out of that block, `fun_ie`.
//!
//! # The input side
//!
//! | register | what it does here |
//! |---|---|
//! | `in_` (`+0x03c`) / `in1` (`+0x040`) | bit `n` is the fabric's **resolved** level for pad `n` (`n` / `n+32`), for every pad whose input enable ([`super::io_mux`]'s `fun_ie`) is set. A pad without it reads 0, which is what silicon's input buffer being off means. |
//! | `pin[n]` (`+0x088 + 4n`) | `int_type` (bits 7:9) and `int_ena` (bits 13:17) are decoded; the rest of the word is remembered and read back. |
//! | `status` / `status1` | the per-pad interrupt **latch**. Sticky: an edge sets a bit and only a write clears it. |
//! | `status_w1ts` / `status_w1tc` (and bank 1) | write-1-to-set / write-1-to-clear over `status`, read back 0 |
//! | `pcpu_int` / `pcpu_int1`, `acpu_int` / `acpu_int1` | `status & <the pads whose int_ena bit for that core is set>` |
//!
//! **`int_type`**, from the PAC's own field doc (`esp32` 0.40.2,
//! `gpio/pin.rs`, bits 7:9): *"if set to 0: GPIO interrupt disable if set to
//! 1: rising edge trigger if set to 2: falling edge trigger if set to 3: any
//! edge trigger if set to 4: low level trigger if set to 5: high level
//! trigger"*. 6 and 7 are values the PAC names nothing for; they are read as
//! disabled and the pad that asked is named once.
//!
//! **`int_ena`'s bit order comes from esp-hal, not from the PAC's prose.**
//! The PAC documents bits 13:17 as *"bit0: APP CPU interrupt enable bit1:
//! APP CPU non-maskable interrupt enable bit3: PRO CPU interrupt enable
//! bit4: PRO CPU non-maskable interrupt enable bit5: SDIO's extent interrupt
//! enable"* — which skips bit 2 and runs to bit 5 inside a five-bit field, so
//! it cannot be read literally. `esp-hal-1.1.1`'s `gpio/mod.rs:1902-1911`
//! is the writer that actually runs on this part:
//!
//! ```text
//! fn gpio_intr_enable(int_enable: bool, nmi_enable: bool) -> u8 {
//!     …  esp32 =>
//!         Cpu::AppCpu => int_enable as u8 | ((nmi_enable as u8) << 1),
//!         Cpu::ProCpu => ((int_enable as u8) << 2) | ((nmi_enable as u8) << 3),
//! }
//! ```
//!
//! So within `int_ena`: **bit 0 = APP maskable, bit 1 = APP NMI, bit 2 = PRO
//! maskable, bit 3 = PRO NMI**, and bit 4 is the SDIO one. In `pin[n]` those
//! are bits 13, 14, **15** and 16. `pcpu_int` is gated on bit 15 and
//! `acpu_int` on bit 13.
//!
//! **Source 22 is a level, not a pulse.** [`SOURCE_GPIO`] is held high while
//! any `pcpu_int`/`pcpu_int1` bit is set and drops when the handler clears
//! the last one through `status_w1tc`. That is what makes esp-hal's handler
//! work at all: it reads `status`, dispatches, and writes the bits back, and
//! a pulse would have been missed or re-entered.
//!
//! # What the input side does not model
//!
//! - **`pcpu_nmi_int` / `acpu_nmi_int` and source 23 (`GPIO_NMI`)** are not
//!   raised. esp-hal 1.1.1 never sets the NMI bits on this chip
//!   (`gpio_intr_enable`, above), so those registers are accept-and-remember
//!   at the PAC's reset, 0. A driver that started setting them would find
//!   them unmodelled rather than wrong.
//! - **`cpusdio_int` / `cpusdio_int1`** are accept-and-remember.
//! - **`func_in_sel_cfg` constants.** `in_sel = 48` (always low) and `56`
//!   (always high) are the classic's — `gpio.constant_0_input` and
//!   `gpio.constant_1_input` in `esp-metadata-generated-0.4.0`, and **not**
//!   the C6's `0x3c`/`0x38`. They are accepted, left unrouted and named once;
//!   so is `sel = 0`, the pad's direct IO_MUX function.
//! - **Sub-sample pulses, the input synchroniser and the pad filter.** An
//!   edge is seen at the cycle it was stamped, with no synchroniser delay and
//!   no glitch rejection.
//!
//! # No waveform reaches a pad in M3, and that is asserted
//!
//! The routing is modelled; nothing drives an `RMT_SIG_n`, because
//! [`super::accept::rmt`] is an accept block and M4 is the phase that gives
//! the channels a time base. [`Gpio::peripheral_driven_pads`] is what turns
//! that into a fact a boot test can assert instead of an absence nobody
//! checked — the C6's boot gate has the same line.
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. No committed transcript covers this block; **M5** earns the first. |
//! | `documented` | `out*`, `enable*`, `in_`, `in1`, `strap`, `status*`, `pcpu_int*`, `acpu_int*`, `pin0`…`pin39`, `func0_out_sel_cfg`…`func39_out_sel_cfg`, `func0_in_sel_cfg`…`func255_in_sel_cfg` — the PAC's bit map is the source, and the behaviour above is that bit map read out loud (with esp-hal as the tie-break on `int_ena`). |
//! | `modeled` | everything else in the window: `bt_select`, `sdio_select`, `*_nmi_int*`, `cpusdio_int*`, `cali_conf`, `cali_data`, `date`, and the words the PAC does not name at all — including `+0xf24`, which the ROM's `SelectSpiFunction` read-modify-writes (`super::accept`'s note). |

use lp_emu_esp_common::pins::Edge;
use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{
    BusCx, PadId, Peripheral, RegFile, RegGrade, RegGrades, RouteSource, SignalId, Width,
};

use crate::regs;

/// The window length, unchanged from the accept block's: the **block's**
/// `0x1000`, not the generated table's `+0x5cc`. The ROM's
/// `SelectSpiFunction` reaches `+0xf24` (`super::accept::GPIO_LEN`).
pub const LEN: u32 = super::accept::GPIO_LEN;

/// Pads the classic carries: `func0_out_sel_cfg` … `func39_out_sel_cfg`,
/// `pin0` … `pin39`.
pub const PAD_COUNT: u32 = 40;

/// Pads in bank 0. Bank 1 is `BANK` … [`PAD_COUNT`], eight of them.
pub const BANK: u32 = 32;

const OUT: u32 = 0x004;
const OUT_W1TS: u32 = 0x008;
const OUT_W1TC: u32 = 0x00c;
const OUT1: u32 = 0x010;
const OUT1_W1TS: u32 = 0x014;
const OUT1_W1TC: u32 = 0x018;
const ENABLE: u32 = 0x020;
const ENABLE_W1TS: u32 = 0x024;
const ENABLE_W1TC: u32 = 0x028;
const ENABLE1: u32 = 0x02c;
const ENABLE1_W1TS: u32 = 0x030;
const ENABLE1_W1TC: u32 = 0x034;
/// The strapping pins, a read override — see the module docs.
pub const STRAP: u32 = super::accept::GPIO_STRAP;
const IN: u32 = 0x03c;
const IN1: u32 = 0x040;
const STATUS: u32 = 0x044;
const STATUS_W1TS: u32 = 0x048;
const STATUS_W1TC: u32 = 0x04c;
const STATUS1: u32 = 0x050;
const STATUS1_W1TS: u32 = 0x054;
const STATUS1_W1TC: u32 = 0x058;
const ACPU_INT: u32 = 0x060;
const PCPU_INT: u32 = 0x068;
const ACPU_INT1: u32 = 0x074;
const PCPU_INT1: u32 = 0x07c;
const PIN: u32 = 0x088;
const PIN_END: u32 = PIN + 4 * PAD_COUNT;

/// `func0_in_sel_cfg` (`+0x130`), four bytes each up to `func255_in_sel_cfg`
/// (`+0x52c`) — one register per peripheral **input** signal, not per pad.
const FUNC_IN_SEL_CFG: u32 = 0x130;
/// Input signals the classic's matrix carries: **256**, twice the C6's.
pub const IN_SIGNAL_COUNT: u32 = 256;
const FUNC_IN_SEL_CFG_END: u32 = FUNC_IN_SEL_CFG + 4 * IN_SIGNAL_COUNT;

/// `func0_out_sel_cfg` (`+0x530`) … `func39_out_sel_cfg` (`+0x5cc`).
const FUNC_OUT_SEL_CFG: u32 = 0x530;
const FUNC_OUT_SEL_CFG_END: u32 = FUNC_OUT_SEL_CFG + 4 * PAD_COUNT;

/// `out_sel`, bits **0:8** (PAC: *"select one of the 256 output to 40
/// GPIO"*). Nine bits, because 256 is a legal value.
const OUT_SEL_MASK: u32 = 0x1ff;
/// `inv_sel`, bit 9: *"invert the output value …"*.
const INV_SEL: u32 = 1 << 9;
/// `oen_sel`, bit 10: *"weather using the logical oen signal or not …"*.
const OEN_SEL: u32 = 1 << 10;
/// `oen_inv_sel`, bit 11.
const OEN_INV_SEL: u32 = 1 << 11;

/// The `out_sel` value meaning "this pad follows `GPIO_OUT[n]`":
/// `OutputSignal::GPIO` = **256** on the classic
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32.rs`, and
/// `gpio.output_signal_max` says 256 in the same file). The C6's is 128.
pub const OUT_SEL_GPIO: u16 = 256;

/// `in_sel`, bits 0:5 (PAC: *"select one of the 256 inputs"* — six bits, so
/// it names a pad or one of the two constants below).
const IN_SEL_MASK: u32 = 0x3f;
/// `in_inv_sel`, bit 6.
const IN_INV_SEL: u32 = 1 << 6;
/// `sel`, bit 7: *"if the slow signal bypass the io matrix or not"* — 1 is
/// the matrix route, which is what esp-hal's
/// `connect_to_peripheral_input` writes.
const SIG_IN_SEL: u32 = 1 << 7;
/// `gpio.constant_0_input` = **48** on the classic (the C6's is 0x3c = 60).
const IN_SEL_ALWAYS_LOW: u32 = 48;
/// `gpio.constant_1_input` = **56**.
const IN_SEL_ALWAYS_HIGH: u32 = 56;

/// `pin[n].int_type`, bits 7:9.
const INT_TYPE_SHIFT: u32 = 7;
const INT_TYPE_MASK: u32 = 0b111;
/// `pin[n]` bit 13 — `int_ena` bit 0, the **APP** core's maskable enable.
const INT_ENA_APP: u32 = 1 << 13;
/// `pin[n]` bit 15 — `int_ena` bit 2, the **PRO** core's maskable enable.
/// See the module docs: the order is esp-hal's, not the PAC prose's.
const INT_ENA_PRO: u32 = 1 << 15;

/// The classic's `GPIO` interrupt source: **22**
/// (`esp32-0.40.2/src/lib.rs:255-256`, `GPIO = 22`; `GPIO_NMI = 23` is the
/// one this block does not raise).
pub const SOURCE_GPIO: u16 = 22;

/// What `pin[n].int_type` asks for, in the PAC's own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntType {
    /// 0: GPIO interrupt disable.
    Disabled,
    /// 1: rising edge trigger.
    Rising,
    /// 2: falling edge trigger.
    Falling,
    /// 3: any edge trigger.
    AnyEdge,
    /// 4: low level trigger.
    LowLevel,
    /// 5: high level trigger.
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
                    "GPIO: pin{pad}.int_type = {other}, which the PAC names no meaning for; \
                     read as disabled"
                );
                IntType::Disabled
            }
        }
    }
}

/// The classic's GPIO block: forty pads in two banks.
#[derive(Debug)]
pub struct Gpio {
    regs: RegFile,
    /// The input word as this block last sampled it, both banks: bit `n` is
    /// pad `n`'s resolved level *if* its input enable is set. Edge detection
    /// compares against it, so it rides the state blob.
    last_in: u64,
    /// Pads whose `int_type` is not `Disabled`, both banks. Kept so the
    /// level-sensitive pass walks the pads a driver actually armed rather
    /// than all forty on every access.
    armed: u64,
    grades: RegGrades,
}

impl Default for Gpio {
    fn default() -> Self {
        Self::new(super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT)
    }
}

fn note(cx: &mut BusCx<'_>, f: impl FnOnce() -> String) {
    if cx.trace.is_enabled() {
        let line = f();
        cx.trace.note(&line);
    }
}

impl Gpio {
    /// `strap` is the word the pads were latched into at reset. See the
    /// module docs for why it is a read override rather than a stored value.
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

    /// The per-register grade table (the module docs').
    pub fn grades() -> RegGrades {
        let mut g = RegGrades::new();
        for off in [
            OUT,
            OUT_W1TS,
            OUT_W1TC,
            OUT1,
            OUT1_W1TS,
            OUT1_W1TC,
            ENABLE,
            ENABLE_W1TS,
            ENABLE_W1TC,
            ENABLE1,
            ENABLE1_W1TS,
            ENABLE1_W1TC,
            STRAP,
            IN,
            IN1,
            STATUS,
            STATUS_W1TS,
            STATUS_W1TC,
            STATUS1,
            STATUS1_W1TS,
            STATUS1_W1TC,
            PCPU_INT,
            PCPU_INT1,
            ACPU_INT,
            ACPU_INT1,
        ] {
            g = g.with_grade(off, RegGrade::Documented);
        }
        for pad in 0..PAD_COUNT {
            g = g
                .with_grade(PIN + 4 * pad, RegGrade::Documented)
                .with_grade(FUNC_OUT_SEL_CFG + 4 * pad, RegGrade::Documented);
        }
        for signal in 0..IN_SIGNAL_COUNT {
            g = g.with_grade(FUNC_IN_SEL_CFG + 4 * signal, RegGrade::Documented);
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

    /// Re-latch the strapping pins, as a chip reset does.
    pub fn set_strap(&mut self, strap: u32) {
        self.regs.set_read_override(STRAP, 0xffff_ffff, strap);
    }

    /// The `out` bitmap as last written, both banks: bit `n` is pad `n`.
    pub fn out(&self) -> u64 {
        u64::from(self.regs.stored(OUT)) | (u64::from(self.regs.stored(OUT1)) << BANK)
    }

    /// The `enable` bitmap as last written, both banks.
    pub fn enable(&self) -> u64 {
        u64::from(self.regs.stored(ENABLE)) | (u64::from(self.regs.stored(ENABLE1)) << BANK)
    }

    /// `func_out_sel_cfg[pad]` as last written.
    pub fn func_out_sel_cfg(&self, pad: u32) -> u32 {
        self.regs.stored(FUNC_OUT_SEL_CFG + 4 * pad)
    }

    /// The interrupt latch as it stands (`status` | `status1`), for a test.
    pub fn status(&self) -> u64 {
        u64::from(self.regs.stored(STATUS)) | (u64::from(self.regs.stored(STATUS1)) << BANK)
    }

    /// Which pads a **peripheral signal** — not `GPIO_OUT` — is routed to and
    /// output-enabled on.
    ///
    /// M3's boot gate asserts this is empty: the routing is modelled but no
    /// block drives a signal, so a claim that a waveform reached a pad would
    /// be false. M4 is the phase that makes it non-empty, and the same call
    /// is then the list of pads a strip decoder should be watching.
    pub fn peripheral_driven_pads(&self, cx: &BusCx<'_>) -> Vec<(PadId, SignalId)> {
        let enable = self.enable();
        (0..PAD_COUNT)
            .filter(|pad| enable & (1u64 << pad) != 0)
            .filter_map(|pad| {
                let id = PadId(pad as u8);
                match cx.pins.route_of(id)?.source {
                    RouteSource::Signal(sig, _) => Some((id, sig)),
                    RouteSource::GpioOut => None,
                }
            })
            .collect()
    }

    /// Write one bank of the `out` bitmap and push every changed bit into
    /// the fabric. `base` is the pad number the bank starts at.
    fn set_out_bank(&mut self, off: u32, base: u32, new: u32, cx: &mut BusCx<'_>) {
        let old = self.regs.stored(off);
        if old == new {
            return;
        }
        self.regs.poke(off, new);
        let at = cx.now;
        let mut changed = old ^ new;
        while changed != 0 {
            let bit = changed.trailing_zeros();
            changed &= changed - 1;
            let pad = base + bit;
            if pad >= PAD_COUNT {
                continue;
            }
            cx.pins
                .set_gpio_out(PadId(pad as u8), new & (1 << bit) != 0, at);
        }
    }

    /// The same for `enable`. Since plan DD38 this is what decides whether a
    /// routed pad drives the wire at all, so it settles the pad the way
    /// `out` does rather than only being recorded.
    fn set_enable_bank(&mut self, off: u32, base: u32, new: u32, cx: &mut BusCx<'_>) {
        let old = self.regs.stored(off);
        if old == new {
            return;
        }
        self.regs.poke(off, new);
        let at = cx.now;
        let mut changed = old ^ new;
        while changed != 0 {
            let bit = changed.trailing_zeros();
            changed &= changed - 1;
            let pad = base + bit;
            if pad >= PAD_COUNT {
                continue;
            }
            cx.pins
                .set_gpio_enable(PadId(pad as u8), new & (1 << bit) != 0, at);
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
        let oe = u8::from(self.enable() & (1u64 << pad) != 0);
        note(cx, || {
            let what = if sel == OUT_SEL_GPIO {
                "GPIO_OUT".to_string()
            } else {
                format!("sig{sel}")
            };
            format!(
                "cyc={at} PIN gpio{pad} <- {what} (out_sel={sel} inv={} oen_sel={} oen_inv={} \
                 oe={oe})",
                u8::from(invert),
                u8::from(oen_from_gpio),
                u8::from(value & OEN_INV_SEL != 0),
            )
        });
    }

    /// A write to `func_in_sel_cfg[signal]`: which pad a peripheral's input
    /// signal reads. The mirror of [`Gpio::set_route`].
    ///
    /// Three cases are **not** routed, and each says so once rather than
    /// pretending: `sel = 0` (bypass the matrix and take the pad's IO_MUX
    /// direct function), and the two constant selectors 48 and 56, which tie
    /// a port low or high with no pad at all.
    fn set_in_route(&mut self, signal: u32, value: u32, cx: &mut BusCx<'_>) {
        let off = FUNC_IN_SEL_CFG + 4 * signal;
        let before = self.regs.stored(off);
        self.regs.poke(off, value);
        let sel = value & IN_SEL_MASK;
        let invert = value & IN_INV_SEL != 0;
        let at = cx.now;
        let sid = SignalId(signal as u16);
        if value & SIG_IN_SEL == 0 || sel >= PAD_COUNT {
            cx.pins.unroute_in(sid);
            if before == value {
                return;
            }
            let why = if value & SIG_IN_SEL == 0 {
                "sel=0 (the pad's direct IO_MUX function)"
            } else if sel == IN_SEL_ALWAYS_HIGH {
                "in_sel=56 (always high)"
            } else if sel == IN_SEL_ALWAYS_LOW {
                "in_sel=48 (always low)"
            } else {
                "in_sel names no pad on this chip"
            };
            note(cx, || {
                format!("cyc={at} PIN in_sig{signal} <- nothing: {why}, not modelled")
            });
            return;
        }
        cx.pins.route_in(sid, PadId(sel as u8), invert);
        if before == value {
            return;
        }
        note(cx, || {
            format!(
                "cyc={at} PIN in_sig{signal} <- gpio{sel} (in_sel={sel} in_inv={})",
                u8::from(invert),
            )
        });
    }

    // ---- the input side ----------------------------------------------------

    /// The `in_`/`in1` pair as one word: pad `n`'s **resolved** fabric level
    /// when its input enable is set, 0 when it is not.
    fn input_word(&self, cx: &BusCx<'_>) -> u64 {
        let mut word = 0u64;
        for pad in 0..PAD_COUNT {
            let id = PadId(pad as u8);
            if cx.pins.pad_input_enable(id) && cx.pins.pad_level(id) {
                word |= 1u64 << pad;
            }
        }
        word
    }

    /// `pin[pad].int_type`.
    fn int_type(&self, pad: u32) -> IntType {
        IntType::decode(self.regs.stored(PIN + 4 * pad), pad)
    }

    /// The pads whose `int_ena` bit for one core is set.
    fn cpu_enable_mask(&self, bit: u32) -> u64 {
        let mut mask = 0u64;
        for pad in 0..PAD_COUNT {
            if self.regs.stored(PIN + 4 * pad) & bit != 0 {
                mask |= 1u64 << pad;
            }
        }
        mask
    }

    /// `pcpu_int`/`pcpu_int1` as they read now.
    fn pcpu_int(&self) -> u64 {
        self.status() & self.cpu_enable_mask(INT_ENA_PRO)
    }

    /// `acpu_int`/`acpu_int1` as they read now.
    fn acpu_int(&self) -> u64 {
        self.status() & self.cpu_enable_mask(INT_ENA_APP)
    }

    /// Recompute [`Gpio::armed`] from the `pin` registers.
    fn rearm(&mut self) {
        let mut armed = 0u64;
        for pad in 0..PAD_COUNT {
            if self.int_type(pad) != IntType::Disabled {
                armed |= 1u64 << pad;
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
            self.set_status_word(self.status() | (1u64 << pad));
        }
    }

    /// Write the two-bank latch from one 64-bit word.
    fn set_status_word(&mut self, value: u64) {
        self.regs.poke(STATUS, value as u32);
        self.regs.poke(STATUS1, (value >> BANK) as u32);
    }

    /// Resample the input word, latch the level-sensitive pads, and drive
    /// source 22 from `pcpu_int`.
    ///
    /// The last step of every path into this block's input side, so there is
    /// exactly one place the interrupt line is set. M3 runs the PRO core
    /// only, so the PRO half is what reaches the matrix; `acpu_int` is
    /// computed and readable, and M4 is what gives it a core to reach.
    fn finish(&mut self, cx: &mut BusCx<'_>) {
        let now = self.input_word(cx);
        self.last_in = now;
        let mut armed = self.armed;
        while armed != 0 {
            let pad = armed.trailing_zeros();
            armed &= armed - 1;
            let high = now & (1u64 << pad) != 0;
            let wanted = match self.int_type(pad) {
                IntType::LowLevel => !high,
                IntType::HighLevel => high,
                _ => false,
            };
            if wanted {
                self.set_status_word(self.status() | (1u64 << pad));
            }
        }
        self.regs.poke(PCPU_INT, self.pcpu_int() as u32);
        self.regs.poke(PCPU_INT1, (self.pcpu_int() >> BANK) as u32);
        self.regs.poke(ACPU_INT, self.acpu_int() as u32);
        self.regs.poke(ACPU_INT1, (self.acpu_int() >> BANK) as u32);
        self.regs.poke(IN, now as u32);
        self.regs.poke(IN1, (now >> BANK) as u32);
        // A level, not a pulse: high while anything is pending, low when the
        // handler has cleared the last one.
        cx.irq.set_level(SOURCE_GPIO, self.pcpu_int() != 0);
    }

    /// Sample the fabric and treat every bit that moved since the last
    /// sample as an edge — the path for a change this block itself caused
    /// inside one access, and the belt-and-braces path on a read.
    fn sync(&mut self, cx: &mut BusCx<'_>) {
        let now = self.input_word(cx);
        let mut changed = (now ^ self.last_in) & self.armed;
        while changed != 0 {
            let pad = changed.trailing_zeros();
            changed &= changed - 1;
            self.latch_edge(pad, now & (1u64 << pad) != 0);
        }
        self.finish(cx);
    }

    /// The machine hands this block the edges it drained off the fabric at a
    /// slice boundary, in order — the same [`Edge`] stream a pin log sees.
    ///
    /// Per-edge rather than a before/after diff, so a pad that rose and fell
    /// inside one slice latches both directions and neither is lost.
    pub fn observe_edges(&mut self, edges: &[Edge], cx: &mut BusCx<'_>) {
        for edge in edges {
            let pad = u32::from(edge.pad.0);
            if pad >= PAD_COUNT || self.armed & (1u64 << pad) == 0 {
                continue;
            }
            if !cx.pins.pad_input_enable(edge.pad) {
                continue;
            }
            self.latch_edge(pad, edge.level);
        }
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
                | OUT1_W1TS
                | OUT1_W1TC
                | ENABLE_W1TS
                | ENABLE_W1TC
                | ENABLE1_W1TS
                | ENABLE1_W1TC
                | STATUS_W1TS
                | STATUS_W1TC
                | STATUS1_W1TS
                | STATUS1_W1TC
        ) {
            // Write-only in the PAC; nothing is remembered to read back.
            return 0;
        }
        if matches!(
            word,
            IN | IN1 | STATUS | STATUS1 | PCPU_INT | PCPU_INT1 | ACPU_INT | ACPU_INT1
        ) {
            // Fresh at the read, so a guest that polls `in_` between slice
            // boundaries still sees the pad, and the interrupt words answer
            // the same value the line was set from.
            self.sync(cx);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        // The lane the access actually touches, in word position: a byte
        // store to `out_w1ts + 1` sets bits 8..15.
        let bits = merge_lane(0, off, width, value);
        match word {
            OUT | OUT1 => {
                let base = if word == OUT { 0 } else { BANK };
                let new = merge_lane(self.regs.stored(word), off, width, value);
                self.set_out_bank(word, base, new, cx);
                self.sync(cx);
            }
            OUT_W1TS | OUT1_W1TS => {
                let (store, base) = if word == OUT_W1TS {
                    (OUT, 0)
                } else {
                    (OUT1, BANK)
                };
                let new = self.regs.stored(store) | bits;
                self.set_out_bank(store, base, new, cx);
                self.sync(cx);
            }
            OUT_W1TC | OUT1_W1TC => {
                let (store, base) = if word == OUT_W1TC {
                    (OUT, 0)
                } else {
                    (OUT1, BANK)
                };
                let new = self.regs.stored(store) & !bits;
                self.set_out_bank(store, base, new, cx);
                self.sync(cx);
            }
            ENABLE | ENABLE1 => {
                let base = if word == ENABLE { 0 } else { BANK };
                let new = merge_lane(self.regs.stored(word), off, width, value);
                self.set_enable_bank(word, base, new, cx);
                self.sync(cx);
            }
            ENABLE_W1TS | ENABLE1_W1TS => {
                let (store, base) = if word == ENABLE_W1TS {
                    (ENABLE, 0)
                } else {
                    (ENABLE1, BANK)
                };
                let new = self.regs.stored(store) | bits;
                self.set_enable_bank(store, base, new, cx);
                self.sync(cx);
            }
            ENABLE_W1TC | ENABLE1_W1TC => {
                let (store, base) = if word == ENABLE_W1TC {
                    (ENABLE, 0)
                } else {
                    (ENABLE1, BANK)
                };
                let new = self.regs.stored(store) & !bits;
                self.set_enable_bank(store, base, new, cx);
                self.sync(cx);
            }
            STATUS | STATUS1 => {
                let new = merge_lane(self.regs.stored(word), off, width, value);
                self.regs.poke(word, new);
                self.finish(cx);
            }
            STATUS_W1TS => {
                self.set_status_word(self.status() | u64::from(bits));
                self.finish(cx);
            }
            STATUS1_W1TS => {
                self.set_status_word(self.status() | (u64::from(bits) << BANK));
                self.finish(cx);
            }
            STATUS_W1TC => {
                self.set_status_word(self.status() & !u64::from(bits));
                self.finish(cx);
            }
            STATUS1_W1TC => {
                self.set_status_word(self.status() & !(u64::from(bits) << BANK));
                self.finish(cx);
            }
            w if (PIN..PIN_END).contains(&w) => {
                self.regs.write(off, width, value, cx);
                self.rearm();
                // A pad just armed for a level it is already sitting at is
                // pending from here, and one whose `int_ena` changed moves
                // the interrupt line.
                self.sync(cx);
            }
            w if (FUNC_OUT_SEL_CFG..FUNC_OUT_SEL_CFG_END).contains(&w) => {
                let pad = (w - FUNC_OUT_SEL_CFG) / 4;
                let new = merge_lane(self.regs.stored(w), off, width, value);
                self.set_route(pad, new, cx);
                self.sync(cx);
            }
            w if (FUNC_IN_SEL_CFG..FUNC_IN_SEL_CFG_END).contains(&w) => {
                let signal = (w - FUNC_IN_SEL_CFG) / 4;
                let new = merge_lane(self.regs.stored(w), off, width, value);
                self.set_in_route(signal, new, cx);
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
        // against, which a restore that lost it would read as forty edges.
        let mut blob = self.regs.save_state();
        blob.extend_from_slice(&self.last_in.to_le_bytes());
        blob
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let split = bytes.len().saturating_sub(8);
        let (regs, tail) = bytes.split_at(split);
        self.regs.load_state(regs);
        self.last_in = <[u8; 8]>::try_from(tail)
            .map(u64::from_le_bytes)
            .unwrap_or(0);
        self.rearm();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// The strip pin L0 measured on the desk board (`../bench.md`: IO18,
    /// IO13, IO2, IO14, IO16).
    const GPIO18: u32 = 18;
    /// A bank-1 pad, which the C6 does not have at all.
    const GPIO33: u32 = 33;
    /// `U0RXD_IN`, the input signal `Uart::new(…).with_rx(GPIO3)` routes —
    /// the seventh strict stop of the direct load (`super::accept`'s note).
    const U0RXD_IN: u32 = 14;

    fn out_sel(off: u32) -> u32 {
        FUNC_OUT_SEL_CFG + 4 * off
    }

    #[test]
    fn the_accept_blocks_reads_are_unchanged_and_strap_is_the_boards() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        assert_eq!(
            sb.read(&mut g, STRAP),
            super::super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT,
            "the desk board's `boot:0x13 (SPI_FAST_FLASH_BOOT)`"
        );
        // Read-only on the chip: a guest cannot change what it booted as.
        sb.write(&mut g, STRAP, 0);
        assert_eq!(
            sb.read(&mut g, STRAP),
            super::super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT
        );
        assert_eq!(g.reg_name(out_sel(39)), Some("func39_out_sel_cfg"));
        assert_eq!(
            g.reg_name(FUNC_IN_SEL_CFG + 4 * 255),
            Some("func255_in_sel_cfg")
        );
        // The word the ROM's `SelectSpiFunction` reaches has no name and is
        // still accepted and remembered (`super::accept`'s note).
        sb.write(&mut g, 0xf24, 0x8000_0000);
        assert_eq!(sb.read(&mut g, 0xf24), 0x8000_0000);
        assert_eq!(g.reg_name(0xf24), None);
    }

    /// The whole point of the file: a plain `Output` pin drive reaches a pad.
    /// esp-hal's sequence is `out_w1tc`, `IO_MUX.mcu_sel`, `enable_w1ts`,
    /// then `func_out_sel_cfg[n] = OutputSignal::GPIO`.
    #[test]
    fn a_plain_output_pin_drive_reaches_the_pad() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.write(&mut g, OUT_W1TC, 1 << GPIO18);
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO18);
        sb.write(&mut g, out_sel(GPIO18), u32::from(OUT_SEL_GPIO));
        assert!(!sb.pins.pad_level(PadId(GPIO18 as u8)), "driven low");

        sb.write(&mut g, OUT_W1TS, 1 << GPIO18);
        assert!(sb.pins.pad_level(PadId(GPIO18 as u8)), "driven high");
        assert_eq!(g.out() & (1 << GPIO18), 1 << GPIO18);
        assert_eq!(sb.read(&mut g, OUT_W1TS), 0, "write-only in the PAC");

        // And clearing `enable` takes the pad off the wire (DD38): it is an
        // input pad again and contributes nothing.
        sb.write(&mut g, ENABLE_W1TC, 1 << GPIO18);
        assert!(!sb.pins.pad_level(PadId(GPIO18 as u8)));
    }

    /// **The C6's `OUT_SEL_GPIO` would route this pad to `SPICLK`.** 128 is
    /// an ordinary signal number on the classic, not the GPIO selector, and
    /// nothing drives it — so the mistake would look like it worked.
    #[test]
    fn the_gpio_selector_is_256_and_128_is_an_ordinary_signal() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO18);
        sb.write(&mut g, OUT_W1TS, 1 << GPIO18);

        sb.write(&mut g, out_sel(GPIO18), 128);
        assert!(
            matches!(
                sb.pins.route_of(PadId(GPIO18 as u8)).unwrap().source,
                RouteSource::Signal(SignalId(128), false)
            ),
            "128 is a peripheral signal here, not `follow GPIO_OUT`"
        );
        assert!(
            !sb.pins.pad_level(PadId(GPIO18 as u8)),
            "nothing drives 128"
        );

        sb.write(&mut g, out_sel(GPIO18), u32::from(OUT_SEL_GPIO));
        assert!(matches!(
            sb.pins.route_of(PadId(GPIO18 as u8)).unwrap().source,
            RouteSource::GpioOut
        ));
        assert!(sb.pins.pad_level(PadId(GPIO18 as u8)));
    }

    /// Bank 1 is live registers, not padding.
    #[test]
    fn bank_one_carries_pads_thirty_two_to_thirty_nine() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        let bit = 1 << (GPIO33 - BANK);
        sb.write(&mut g, ENABLE1_W1TS, bit);
        sb.write(&mut g, out_sel(GPIO33), u32::from(OUT_SEL_GPIO));
        sb.write(&mut g, OUT1_W1TS, bit);
        assert!(sb.pins.pad_level(PadId(GPIO33 as u8)), "gpio33 is driven");
        assert_eq!(sb.read(&mut g, OUT1), bit);
        assert_eq!(g.out() & (1 << GPIO33), 1 << GPIO33);
        // And bank 0 did not move.
        assert_eq!(sb.read(&mut g, OUT), 0);
    }

    /// The input half of the matrix: the write the direct load's seventh
    /// strict stop made.
    #[test]
    fn the_input_matrix_routes_u0rxd_from_a_pad() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.write(&mut g, FUNC_IN_SEL_CFG + 4 * U0RXD_IN, SIG_IN_SEL | 3);
        assert_eq!(
            sb.pins.input_route_of(SignalId(U0RXD_IN as u16)),
            Some((PadId(3), false))
        );
        // The classic's constants, which are NOT the C6's.
        sb.write(
            &mut g,
            FUNC_IN_SEL_CFG + 4 * U0RXD_IN,
            SIG_IN_SEL | IN_SEL_ALWAYS_HIGH,
        );
        assert_eq!(sb.pins.input_route_of(SignalId(U0RXD_IN as u16)), None);
        sb.write(
            &mut g,
            FUNC_IN_SEL_CFG + 4 * U0RXD_IN,
            SIG_IN_SEL | IN_SEL_ALWAYS_LOW,
        );
        assert_eq!(sb.pins.input_route_of(SignalId(U0RXD_IN as u16)), None);
        // `sel = 0` bypasses the matrix: accepted, unrouted, and read back.
        sb.write(&mut g, FUNC_IN_SEL_CFG + 4 * U0RXD_IN, 3);
        assert_eq!(sb.pins.input_route_of(SignalId(U0RXD_IN as u16)), None);
        assert_eq!(sb.read(&mut g, FUNC_IN_SEL_CFG + 4 * U0RXD_IN), 3);
    }

    /// `in_` serves a pad's bit only when IO_MUX's `fun_ie` says the input
    /// buffer is on — the seam [`super::io_mux`] writes.
    #[test]
    fn in_reads_the_fabric_only_through_the_pads_input_enable() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.pins.drive_pad(PadId(GPIO18 as u8), true, 0);
        assert_eq!(sb.read(&mut g, IN), 0, "the input buffer is off");
        sb.pins.set_pad_input_enable(PadId(GPIO18 as u8), true);
        assert_eq!(sb.read(&mut g, IN), 1 << GPIO18);
        // Bank 1 through the same door.
        sb.pins.drive_pad(PadId(GPIO33 as u8), true, 0);
        sb.pins.set_pad_input_enable(PadId(GPIO33 as u8), true);
        assert_eq!(sb.read(&mut g, IN1), 1 << (GPIO33 - BANK));
    }

    /// `int_ena`'s PRO bit is esp-hal's bit 2 — `pin[n]` bit 15 — and the
    /// APP core's is bit 0. Getting these the C6's way round would arm the
    /// wrong core's line and look like nothing happening.
    #[test]
    fn the_pro_cores_enable_is_pin_bit_fifteen_and_the_apps_is_bit_thirteen() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.pins.set_pad_input_enable(PadId(GPIO18 as u8), true);
        // Rising edge, PRO maskable.
        sb.write(
            &mut g,
            PIN + 4 * GPIO18,
            (1 << INT_TYPE_SHIFT) | INT_ENA_PRO,
        );
        sb.pins.drive_pad(PadId(GPIO18 as u8), true, 0);
        let edges = sb.pins.take_edges();
        g.observe_edges(&edges, &mut sb.cx());
        assert_eq!(sb.read(&mut g, STATUS), 1 << GPIO18, "latched");
        assert_eq!(sb.read(&mut g, PCPU_INT), 1 << GPIO18, "the PRO core's");
        assert_eq!(sb.read(&mut g, ACPU_INT), 0, "not the APP core's");

        // Sticky until written: a re-read does not clear it.
        assert_eq!(sb.read(&mut g, STATUS), 1 << GPIO18);
        sb.write(&mut g, STATUS_W1TC, 1 << GPIO18);
        assert_eq!(sb.read(&mut g, STATUS), 0);
        assert_eq!(sb.read(&mut g, PCPU_INT), 0);

        // The APP core's bit gates the other word.
        sb.write(
            &mut g,
            PIN + 4 * GPIO18,
            (1 << INT_TYPE_SHIFT) | INT_ENA_APP,
        );
        sb.write(&mut g, STATUS_W1TS, 1 << GPIO18);
        assert_eq!(sb.read(&mut g, ACPU_INT), 1 << GPIO18);
        assert_eq!(sb.read(&mut g, PCPU_INT), 0);
    }

    /// M3's boot claim, as a function rather than an absence: the routing is
    /// modelled and **no peripheral signal reaches a pad**, because nothing
    /// in this milestone drives one.
    #[test]
    fn no_peripheral_signal_reaches_a_pad_until_something_drives_one() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO18);
        sb.write(&mut g, out_sel(GPIO18), u32::from(OUT_SEL_GPIO));
        assert!(
            g.peripheral_driven_pads(&sb.cx()).is_empty(),
            "a GPIO_OUT pad is not a peripheral-driven one"
        );

        // Route the RMT's channel-0 signal at it and the pad is now waiting
        // on a block that does not drive yet (M4).
        sb.write(&mut g, out_sel(GPIO18), 87);
        let driven = g.peripheral_driven_pads(&sb.cx());
        assert_eq!(driven, vec![(PadId(GPIO18 as u8), SignalId(87))]);
        assert!(!sb.pins.pad_level(PadId(GPIO18 as u8)), "nothing drives it");
    }

    #[test]
    fn the_state_blob_round_trips_the_registers_and_the_input_sample() {
        let mut sb = Sandbox::new();
        let mut g = Gpio::default();
        sb.pins.set_pad_input_enable(PadId(GPIO33 as u8), true);
        sb.pins.drive_pad(PadId(GPIO33 as u8), true, 0);
        sb.write(
            &mut g,
            PIN + 4 * GPIO33,
            (3 << INT_TYPE_SHIFT) | INT_ENA_PRO,
        );
        sb.write(&mut g, out_sel(GPIO33), u32::from(OUT_SEL_GPIO));
        let _ = sb.read(&mut g, IN1);

        let blob = g.save_state();
        let mut other = Gpio::default();
        other.load_state(&blob);
        assert_eq!(other.func_out_sel_cfg(GPIO33), u32::from(OUT_SEL_GPIO));
        assert_eq!(other.armed, 1u64 << GPIO33, "rearmed from the `pin` words");
        assert_eq!(other.last_in, 1u64 << GPIO33, "the edge-detect sample");
        assert_eq!(
            other.regs.stored(STRAP),
            g.regs.stored(STRAP),
            "the strap is a read override and survives"
        );
    }
}
