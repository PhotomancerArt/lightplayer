//! `GPIO` at `0x6000_4000`: the S3's matrix, as a routing **view**.
//!
//! P04–P06 had this block as accept-and-remember ([`super::accept::gpio`])
//! and everything the ROM-up boot writes here still lands in a [`RegFile`].
//! What P07 adds is that four groups of registers now *mean* something to
//! the machine: they are written into the bus's signal fabric
//! ([`lp_emu_esp_common::pins`], plan DD34 e), which is the one state a
//! peripheral and this block can share — because a peripheral never sees
//! another peripheral.
//!
//! | register | what it does here |
//! |---|---|
//! | `func_out_sel_cfg[n]` (`+0x554 + 4n`, n < 54) | `out_sel` names the signal pad `n` follows; **256** means "follow `GPIO_OUT[n]`". `inv_sel` inverts. `oen_sel`/`oen_inv_sel` are recorded and reported, never gated on. |
//! | `out` / `out_w1ts` / `out_w1tc` and the `out1*` bank | the GPIO output bitmap a pad routed to `GPIO_OUT` follows |
//! | `enable` / `enable_w1ts` / `enable_w1tc` and the `enable1*` bank | the output-**enable** bitmap. This is what makes a routed pad *drive* the wire (plan DD38): a pad whose bit is clear carries whatever the wire carries and contributes nothing. |
//! | `func_in_sel_cfg[s]` (`+0x154 + 4s`, s < 256) | the **input** half: `in_sel` names the pad signal `s` reads, `in_inv_sel` inverts it, `sel = 1` is the matrix route. |
//!
//! # ⚠️ Neither sibling is the parent of this file, and that is the finding
//!
//! `notes.md` §3.5 measured the S3's fixed registers against the C6's and
//! found every one at the same offset, which is true and is why the offsets
//! below are the C6's — `out 0x04`, `enable 0x20`, `strap 0x38`, `in_ 0x3c`,
//! `status 0x44`, `pcpu_int 0x5c`, `pin 0x074`, `func_in_sel_cfg 0x154`,
//! `func_out_sel_cfg 0x554`, `clock_gate 0x62c`. But the **bitfields and the
//! banks are the classic's**:
//!
//! | | S3 | C6 | classic |
//! |---|---|---|---|
//! | `out_sel` | bits **0:8** | 0:7 | 0:8 |
//! | `inv_sel` / `oen_sel` / `oen_inv_sel` | **9 / 10 / 11** | 8 / 9 / — | 9 / 10 / 11 |
//! | "follow `GPIO_OUT`" | **256** | 128 | 256 |
//! | `func_in_sel_cfg[n]` | **256** | 128 | 256 |
//! | `out1` / `enable1` / `in1` / `status1` | **live** (pads 32..48) | padding | live (pads 32..39) |
//! | interrupt outputs | `pcpu_*` only | `pcpu_*` only | `pcpu_*` **and** `acpu_*` |
//! | `in_sel` constants | `0x38` high / `0x3c` low | the same | 56 high / 48 low |
//!
//! So this chip is the C6's offsets with the classic's field widths, the
//! classic's two banks, and the C6's one interrupt output — a third
//! combination, not a parameterisation of either sibling. The phase brief's
//! lean was "parameterize the C6's `gpio.rs` by array length and base"; the
//! table above is why that was not done, and why this is a fresh file at the
//! chip's own numbers instead. (The RMT, where the two chips genuinely do
//! share a layout, **is** parameterised — see
//! [`lp_emu_esp_common::ip::rmt`].)
//!
//! ⚠️ **A view that reused the C6's `OUT_SEL_GPIO` would route every plain
//! output pad to signal 128**, which is a real peripheral signal on this
//! chip, and it would look like it worked because nothing drives 128 either.
//! [`OUT_SEL_GPIO`]'s own doc carries the three readings that establish 256.
//!
//! # Forty-nine pads in fifty-four register slots
//!
//! The S3 carries **49 pads**, `GPIO0` … `GPIO48` — that is what `IO_MUX`
//! has (`gpio0` at `+0x004` through `gpio48` at `+0x0c4`). But `pin[n]` runs
//! to `pin53` and `func_out_sel_cfg[n]` to `func53_out_sel_cfg`: **54 register
//! slots** over 49 pads. The extra five answer, are remembered and are graded
//! like their neighbours; they route nothing, and [`Gpio::set_route`] says so
//! once rather than handing the fabric a pad that does not exist.
//!
//! # What is observed, and what is not
//!
//! - **A pad becomes observed when the guest writes its `func_out_sel_cfg`.**
//!   The register resets to `0x0100` — every pad nominally follows `GPIO_OUT`
//!   — but seeding 54 routes at reset would give the machine 54 pads to
//!   decode and 54 pin logs for a boot that drives none of them. So an
//!   untouched pad stays unobserved; its `out` bit is still tracked, so the
//!   moment anything routes it the level is already right. A plain GPIO
//!   output pad that never writes `func_out_sel_cfg` is therefore *not* in
//!   the pin log, and that limit is stated rather than hidden.
//! - `out_w1ts`/`out_w1tc`/`enable_w1ts`/`enable_w1tc` and their bank-1 twins
//!   fold into `out`/`out1`/`enable`/`enable1` and **read back 0** (the PAC
//!   declares them write-only).
//! - `strap` (`+0x038`) is a **read override**: the pads as they were latched
//!   at reset. It is read-only on the chip and the mask ROM prints it
//!   verbatim as the `boot:0x%x` half of its banner, so a guest that wrote
//!   here must not be able to change what the chip booted as. P06 established
//!   the default from the ROM's own branches
//!   ([`super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT`]); `--strap` takes a
//!   board's real word.
//! - Drive strength, pull-ups, open-drain, pad filters and `IO_MUX.mcu_sel`
//!   are **not** gated on. A pad routed here carries its signal whatever
//!   `mcu_sel` says. [`super::io_mux`] takes exactly one field out of that
//!   block, `fun_ie`.
//!
//! # The input side
//!
//! | register | what it does here |
//! |---|---|
//! | `in_` (`+0x03c`) / `in1` (`+0x040`) | bit `n` is the fabric's **resolved** level for pad `n` (`n` / `n+32`), for every pad whose input enable ([`super::io_mux`]'s `fun_ie`) is set. A pad without it reads 0, which is what silicon's input buffer being off means. ⚠️ On this chip `fun_ie` is **set at reset**, so [`super::io_mux::seed_input_enables`] puts that in the fabric before the guest runs. |
//! | `pin[n]` (`+0x074 + 4n`) | `int_type` (bits 7:9) and `int_ena` (bits 13:17) are decoded; the rest of the word is remembered and read back. |
//! | `status` / `status1` | the per-pad interrupt **latch**. Sticky: an edge sets a bit and only a write clears it. |
//! | `status_w1ts` / `status_w1tc` (and bank 1) | write-1-to-set / write-1-to-clear over `status`, read back 0 |
//! | `pcpu_int` / `pcpu_int1` | `status & <the pads whose int_ena bit 0 is set>` |
//!
//! **`int_type`**, from the PAC's own field doc (esp32s3 0.35.2,
//! `gpio/pin.rs`, bits 7:9): *"0:disable GPIO interrupt. 1:trigger at
//! posedge. 2:trigger at negedge. 3:trigger at any edge. 4:valid at low
//! level. 5:valid at high level"*. 6 and 7 are values the PAC names nothing
//! for; they are read as disabled and the pad that asked is named once.
//!
//! **What gates `pcpu_int`.** The PAC's `INT_ENA` field is bits 13:17 of
//! `pin[n]`, documented *"set bit 13 to enable CPU interrupt. set bit 14 to
//! enable CPU(not shielded) interrupt"* — the C6's wording exactly, not the
//! classic's four-CPU-bit prose. So `int_ena` **bit 0**, which is `pin[n]`
//! **bit 13**, gates `pcpu_int`.
//!
//! ⚠️ **Two cores, one interrupt output.** The classic has `acpu_int`
//! alongside `pcpu_int` and gates them on different `int_ena` bits; the S3's
//! register table has **no `acpu_int` at all** (the generated table goes
//! `pcpu_int 0x5c`, `pcpu_nmi_int 0x60`, `cpusdio_int 0x64`), so a pad
//! interrupt on this part reaches the matrix once and both cores' matrices
//! see the same source. Nothing here routes it per core.
//!
//! **Source 16 is a level, not a pulse.** [`crate::regs::source::GPIO`] is
//! held high while any `pcpu_int`/`pcpu_int1` bit is set and drops when the
//! handler clears the last one through `status_w1tc`. That is what makes
//! esp-hal's handler work at all: it reads `status`, dispatches, and writes
//! the bits back, and a pulse would have been missed or re-entered.
//!
//! # What the input side does not model
//!
//! - **`pcpu_nmi_int` / `pcpu_nmi_int1` and source 17 (`GPIO_NMI`)** are not
//!   raised. `int_ena` bit 1 is the NMI enable and esp-hal 1.1.1 never sets
//!   it on this chip, so those registers are accept-and-remember at the PAC's
//!   reset, 0.
//! - **`cpusdio_int` / `cpusdio_int1` / `status_next` / `status_next1`** are
//!   accept-and-remember.
//! - **`func_in_sel_cfg` constants.** `in_sel = 0x38` (always high) and
//!   `0x3c` (always low) are the PAC's own, and they are the C6's numbers,
//!   **not** the classic's 56/48. They are accepted, left unrouted and named
//!   once; so is `sel = 0`, the pad's direct IO_MUX function.
//! - **Sub-sample pulses, the input synchroniser and the pad filter.** An
//!   edge is seen at the cycle it was stamped, with no synchroniser delay and
//!   no glitch rejection.
//!
//! # Register grades
//!
//! | grade | registers |
//! |---|---|
//! | `measured` | none. No committed transcript covers this block, and a waveform is not a register's bit map. |
//! | `documented` | `out*`, `enable*`, `in_`, `in1`, `strap`, `status*`, `pcpu_int*`, `pin0`…`pin53`, `func0_out_sel_cfg`…`func53_out_sel_cfg`, `func0_in_sel_cfg`…`func255_in_sel_cfg` — the PAC's bit map is the source, and the behaviour above is that bit map read out loud. |
//! | `modeled` | everything else in the window: `bt_select`, `sdio_select`, `*_nmi_int*`, `cpusdio_int*`, `status_next*`, `clock_gate`, `reg_date`. Accept-and-remember at the PAC's reset value. |

use lp_emu_esp_common::pins::Edge;
use lp_emu_esp_common::regfile::merge_lane;
use lp_emu_esp_common::{
    BusCx, PadId, Peripheral, RegFile, RegGrade, RegGrades, RouteSource, SignalId, Width,
};

use crate::regs;
use crate::regs::source;

/// The window length, unchanged from P06's accept block: the generated table
/// runs to `reg_date` at `+0x6fc`.
pub const LEN: u32 = super::accept::GPIO_LEN;

/// Pads the S3 carries: `GPIO0` … `GPIO48`, which is what `IO_MUX` has
/// (`gpio0` `+0x004` … `gpio48` `+0x0c4`).
pub const PAD_COUNT: u32 = 49;

/// Register slots in the per-pad arrays: `pin0`…`pin53` and
/// `func0_out_sel_cfg`…`func53_out_sel_cfg`. **Five more than there are
/// pads** — see the module docs.
pub const PAD_SLOTS: u32 = 54;

/// Pads in bank 0. Bank 1 is `BANK` … [`PAD_COUNT`], seventeen of them.
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
const PCPU_INT: u32 = 0x05c;
const PCPU_INT1: u32 = 0x068;
const PIN: u32 = 0x074;
const PIN_END: u32 = PIN + 4 * PAD_SLOTS;

/// `func0_in_sel_cfg` (`+0x154`), four bytes each up to `func255_in_sel_cfg`
/// (`+0x550`) — one register per peripheral **input** signal, not per pad.
const FUNC_IN_SEL_CFG: u32 = 0x154;
/// Input signals the S3's matrix carries: **256**, twice the C6's.
pub const IN_SIGNAL_COUNT: u32 = 256;
const FUNC_IN_SEL_CFG_END: u32 = FUNC_IN_SEL_CFG + 4 * IN_SIGNAL_COUNT;

/// `func0_out_sel_cfg` (`+0x554`) … `func53_out_sel_cfg` (`+0x628`).
const FUNC_OUT_SEL_CFG: u32 = 0x554;
const FUNC_OUT_SEL_CFG_END: u32 = FUNC_OUT_SEL_CFG + 4 * PAD_SLOTS;

/// `out_sel`, bits **0:8** (PAC: *"0<=s<=256 … s=0-255: output of GPIO\[n\]
/// equals input of peripheral\[s\]. s=256: output of GPIO\[n\] equals
/// GPIO_OUT_REG\[n\]"*). Nine bits, because 256 is a legal value — the C6's
/// is eight.
const OUT_SEL_MASK: u32 = 0x1ff;
/// `inv_sel`, bit 9: *"set this bit to invert output signal"*. The C6's is 8.
const INV_SEL: u32 = 1 << 9;
/// `oen_sel`, bit 10: *"use GPIO_ENABLE_REG\[n\] as output enable signal"*.
const OEN_SEL: u32 = 1 << 10;
/// `oen_inv_sel`, bit 11.
const OEN_INV_SEL: u32 = 1 << 11;

/// The `out_sel` value meaning "this pad follows `GPIO_OUT[n]`" — **256**.
/// Established, with its three citations, at
/// [`crate::regs::output_signals::OUT_SEL_GPIO`], which this is an alias of
/// so that the view and the table cannot drift apart.
pub const OUT_SEL_GPIO: u16 = crate::regs::output_signals::OUT_SEL_GPIO;

/// `in_sel`, bits 0:5 (PAC: *"s=0-53: connect GPIO\[s\] to this port"*).
const IN_SEL_MASK: u32 = 0x3f;
/// `in_inv_sel`, bit 6.
const IN_INV_SEL: u32 = 1 << 6;
/// `sel`, bit 7: *"set this bit to bypass GPIO. 1: do not bypass GPIO. 0:
/// bypass GPIO."* — 1 is the matrix route, which is what esp-hal's
/// `connect_input_to_peripheral` writes.
const SIG_IN_SEL: u32 = 1 << 7;
/// *"s=0x38: set this port always high level"* — the C6's number, not the
/// classic's 56.
const IN_SEL_ALWAYS_HIGH: u32 = 0x38;
/// *"s=0x3C: set this port always low level"*.
const IN_SEL_ALWAYS_LOW: u32 = 0x3c;

/// `pin[n].int_type`, bits 7:9.
const INT_TYPE_SHIFT: u32 = 7;
const INT_TYPE_MASK: u32 = 0b111;
/// `pin[n]` bit 13 — `int_ena` bit 0, the maskable CPU enable the PAC
/// documents as *"set bit 13 to enable CPU interrupt"*.
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
                    "GPIO: pin{pad}.int_type = {other}, which the PAC names no meaning for; \
                     read as disabled"
                );
                IntType::Disabled
            }
        }
    }
}

/// The S3's GPIO block: forty-nine pads in two banks.
#[derive(Debug)]
pub struct Gpio {
    regs: RegFile,
    /// The input word as this block last sampled it, both banks: bit `n` is
    /// pad `n`'s resolved level *if* its input enable is set. Edge detection
    /// compares against it, so it rides the state blob.
    last_in: u64,
    /// Pads whose `int_type` is not `Disabled`, both banks. Kept so the
    /// level-sensitive pass walks the pads a driver actually armed rather
    /// than all forty-nine on every access.
    armed: u64,
    /// Register slots past [`PAD_COUNT`] whose route was asked for and
    /// declined, so the note is written once per slot.
    warned_slot: u64,
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

/// `RMT_SIG_0` / `sig81` — what the routing note calls `out_sel`.
fn signal_name(sel: u16) -> String {
    crate::regs::output_signals::output_signal_name(sel)
        .map_or_else(|| format!("sig{sel}"), str::to_string)
}

/// The same, for the input half of the matrix.
fn input_signal_name(sel: u32) -> String {
    crate::regs::output_signals::input_signal_name(sel as u16)
        .map_or_else(|| format!("in_sig{sel}"), str::to_string)
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
            warned_slot: 0,
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
        ] {
            g = g.with_grade(off, RegGrade::Documented);
        }
        for slot in 0..PAD_SLOTS {
            g = g
                .with_grade(PIN + 4 * slot, RegGrade::Documented)
                .with_grade(FUNC_OUT_SEL_CFG + 4 * slot, RegGrade::Documented);
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
    /// The list of pads a strip decoder should be watching, and the fact a
    /// boot gate asserts instead of an absence nobody checked: the RMT view's
    /// symbol pump drives `RMT_SIG_0 + n` ([`super::rmt`]) and this call is
    /// where that becomes visible from outside.
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
        self.regs.poke(off, value);
        if pad >= PAD_COUNT {
            // One of the five register slots past the last pad: remembered,
            // routed nowhere, named once.
            if self.warned_slot & (1u64 << pad) == 0 {
                self.warned_slot |= 1u64 << pad;
                let at = cx.now;
                note(cx, || {
                    format!(
                        "cyc={at} PIN func{pad}_out_sel_cfg written: the S3 has {PAD_COUNT} pads \
                         and {PAD_SLOTS} slots, so slot {pad} routes nothing"
                    )
                });
            }
            return;
        }
        let was_routed = cx.pins.route_of(PadId(pad as u8)).is_some();
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
        let name = signal_name(sel);
        note(cx, || {
            format!(
                "cyc={at} PIN gpio{pad} <- {name} (out_sel={sel} inv={} oen_sel={} oen_inv={} \
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
    /// direct function), and the two constant selectors `0x38` / `0x3c`,
    /// which tie a port high or low with no pad at all.
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
                "in_sel=0x38 (always high)"
            } else if sel == IN_SEL_ALWAYS_LOW {
                "in_sel=0x3c (always low)"
            } else {
                "in_sel names no pad on this chip"
            };
            note(cx, || {
                format!(
                    "cyc={at} PIN {name} <- nothing: {why}, not modelled",
                    name = input_signal_name(signal),
                )
            });
            return;
        }
        cx.pins.route_in(sid, PadId(sel as u8), invert);
        if before == value {
            return;
        }
        note(cx, || {
            format!(
                "cyc={at} PIN {name} <- gpio{sel} (in_sel={sel} in_inv={})",
                u8::from(invert),
                name = input_signal_name(signal),
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

    /// The pads whose `int_ena` bit 0 is set.
    fn cpu_enable_mask(&self) -> u64 {
        let mut mask = 0u64;
        for pad in 0..PAD_COUNT {
            if self.regs.stored(PIN + 4 * pad) & INT_ENA_CPU != 0 {
                mask |= 1u64 << pad;
            }
        }
        mask
    }

    /// `pcpu_int`/`pcpu_int1` as they read now.
    fn pcpu_int(&self) -> u64 {
        self.status() & self.cpu_enable_mask()
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
    /// source 16 from `pcpu_int`.
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
        let pcpu = self.pcpu_int();
        self.regs.poke(PCPU_INT, pcpu as u32);
        self.regs.poke(PCPU_INT1, (pcpu >> BANK) as u32);
        self.regs.poke(IN, now as u32);
        self.regs.poke(IN1, (now >> BANK) as u32);
        // A level, not a pulse: high while anything is pending, low when the
        // handler has cleared the last one.
        cx.irq.set_level(source::GPIO, pcpu != 0);
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
        if matches!(word, IN | IN1 | STATUS | STATUS1 | PCPU_INT | PCPU_INT1) {
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
        // against, which a restore that lost it would read as forty-nine
        // edges.
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

    /// `D10` on the XIAO ESP32-S3 Plus — the pad `projects/test/shader-oracle`
    /// names, `"gpio": "/gpio/9"` in
    /// `lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.json`.
    const GPIO9: u32 = 9;
    /// A bank-1 pad, which the C6 does not have at all.
    const GPIO40: u32 = 40;
    /// `RMT_SIG_0`, the signal TX channel 0 drives.
    const RMT_SIG_0: u32 = crate::regs::output_signals::RMT_SIG_0 as u32;

    fn out_sel(pad: u32) -> u32 {
        FUNC_OUT_SEL_CFG + 4 * pad
    }

    fn rig() -> (Sandbox, Gpio) {
        let mut sb = Sandbox::new();
        super::super::io_mux::seed_input_enables(&mut sb.pins);
        (sb, Gpio::default())
    }

    #[test]
    fn the_accept_blocks_reads_are_unchanged_and_strap_is_the_roms() {
        let (mut sb, mut g) = rig();
        assert_eq!(
            sb.read(&mut g, STRAP),
            super::super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT
        );
        // Read-only on the chip: a guest cannot change what it booted as.
        sb.write(&mut g, STRAP, 0);
        assert_eq!(
            sb.read(&mut g, STRAP),
            super::super::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT
        );
        assert_eq!(g.reg_name(out_sel(53)), Some("func53_out_sel_cfg"));
        assert_eq!(
            g.reg_name(FUNC_IN_SEL_CFG + 4 * 255),
            Some("func255_in_sel_cfg")
        );
        assert_eq!(g.reg_name(PIN + 4 * 53), Some("pin53"));
    }

    /// The whole point of the file: a plain `Output` pin drive reaches a pad.
    /// esp-hal's sequence is `out_w1tc`, `IO_MUX.mcu_sel`, `enable_w1ts`,
    /// then `func_out_sel_cfg[n] = OutputSignal::GPIO`.
    #[test]
    fn a_plain_output_pin_drive_reaches_the_pad() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, OUT_W1TC, 1 << GPIO9);
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO9);
        sb.write(&mut g, out_sel(GPIO9), u32::from(OUT_SEL_GPIO));
        assert!(!sb.pins.pad_level(PadId(GPIO9 as u8)), "driven low");

        sb.write(&mut g, OUT_W1TS, 1 << GPIO9);
        assert!(sb.pins.pad_level(PadId(GPIO9 as u8)), "driven high");
        assert_eq!(g.out() & (1 << GPIO9), 1 << GPIO9);
        assert_eq!(sb.read(&mut g, OUT_W1TS), 0, "write-only in the PAC");

        // And clearing `enable` takes the pad off the wire (DD38): it is an
        // input pad again and contributes nothing.
        sb.write(&mut g, ENABLE_W1TC, 1 << GPIO9);
        assert!(!sb.pins.pad_level(PadId(GPIO9 as u8)));
    }

    /// Bank 1 is live here, the way it is on the classic and is not on the
    /// C6: pad 40 is `out1` bit 8.
    #[test]
    fn bank_one_carries_pads_thirty_two_and_up() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, ENABLE1_W1TS, 1 << (GPIO40 - BANK));
        sb.write(&mut g, out_sel(GPIO40), u32::from(OUT_SEL_GPIO));
        sb.write(&mut g, OUT1_W1TS, 1 << (GPIO40 - BANK));
        assert!(sb.pins.pad_level(PadId(GPIO40 as u8)));
        assert_eq!(g.out() & (1u64 << GPIO40), 1u64 << GPIO40);
        assert_eq!(sb.read(&mut g, OUT1), 1 << (GPIO40 - BANK));
    }

    /// **The C6's `OUT_SEL_GPIO` would route this pad to signal 128.** 128 is
    /// an ordinary signal number on the S3, not the GPIO selector, and nothing
    /// drives it — so the mistake would look like it worked.
    #[test]
    fn the_gpio_selector_is_256_and_128_is_an_ordinary_signal() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO9);
        sb.write(&mut g, OUT_W1TS, 1 << GPIO9);

        sb.write(&mut g, out_sel(GPIO9), 128);
        assert!(
            matches!(
                sb.pins.route_of(PadId(GPIO9 as u8)).map(|r| r.source),
                Some(RouteSource::Signal(SignalId(128), false))
            ),
            "128 is a peripheral signal here, not GPIO_OUT"
        );
        assert!(!sb.pins.pad_level(PadId(GPIO9 as u8)), "nothing drives 128");

        sb.write(&mut g, out_sel(GPIO9), u32::from(OUT_SEL_GPIO));
        assert!(
            matches!(
                sb.pins.route_of(PadId(GPIO9 as u8)).map(|r| r.source),
                Some(RouteSource::GpioOut)
            ),
            "256 is the selector"
        );
        assert!(sb.pins.pad_level(PadId(GPIO9 as u8)));
    }

    /// `inv_sel` is bit **9** here and bit 8 on the C6, so a route written
    /// with the C6's constant would land inside `out_sel` and name signal
    /// 256 + something. The assertion is on the decoded route.
    #[test]
    fn inv_sel_is_bit_nine_and_out_sel_is_nine_bits_wide() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO9);
        sb.write(&mut g, out_sel(GPIO9), RMT_SIG_0 | INV_SEL);
        assert!(
            matches!(
                sb.pins.route_of(PadId(GPIO9 as u8)).map(|r| r.source),
                Some(RouteSource::Signal(s, true)) if u32::from(s.0) == RMT_SIG_0
            ),
            "bit 9 inverts and leaves out_sel alone"
        );
        // The C6's `INV_SEL` (bit 8) is part of `out_sel` here: 81 | 0x100 is
        // signal 337, not "signal 81 inverted".
        sb.write(&mut g, out_sel(GPIO9), RMT_SIG_0 | (1 << 8));
        assert!(
            matches!(
                sb.pins.route_of(PadId(GPIO9 as u8)).map(|r| r.source),
                Some(RouteSource::Signal(s, false)) if s.0 == (RMT_SIG_0 as u16) | 0x100
            ),
            "bit 8 is inside out_sel on this chip"
        );
    }

    /// `peripheral_driven_pads` is the boot gate's evidence: routed **and**
    /// output-enabled, and `GPIO_OUT` does not count.
    #[test]
    fn peripheral_driven_pads_needs_a_signal_and_an_enable() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, out_sel(GPIO9), RMT_SIG_0);
        assert!(
            g.peripheral_driven_pads(&sb.cx()).is_empty(),
            "routed but not enabled"
        );
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO9);
        assert_eq!(
            g.peripheral_driven_pads(&sb.cx()),
            vec![(PadId(GPIO9 as u8), SignalId(RMT_SIG_0 as u16))]
        );
        sb.write(&mut g, out_sel(GPIO9), u32::from(OUT_SEL_GPIO));
        assert!(
            g.peripheral_driven_pads(&sb.cx()).is_empty(),
            "GPIO_OUT is not a peripheral"
        );
    }

    /// The five register slots past the last pad answer and route nothing.
    #[test]
    fn the_slots_past_pad_forty_eight_are_remembered_and_route_nothing() {
        let (mut sb, mut g) = rig();
        for slot in PAD_COUNT..PAD_SLOTS {
            sb.write(&mut g, ENABLE1_W1TS, 1 << (slot - BANK));
            sb.write(&mut g, out_sel(slot), RMT_SIG_0);
            assert_eq!(g.func_out_sel_cfg(slot), RMT_SIG_0, "slot {slot} remembers");
            assert!(
                sb.pins.route_of(PadId(slot as u8)).is_none(),
                "slot {slot} is not a pad"
            );
        }
    }

    /// `fun_ie` gates `in_`, and on this chip it starts **set**.
    #[test]
    fn fun_ie_gates_in_and_the_reset_word_has_it_on() {
        let (mut sb, mut g) = rig();
        // Drive the pad from outside the block, as a wired neighbour would.
        sb.pins.drive_pad(PadId(GPIO9 as u8), true, 0);
        assert_eq!(sb.read(&mut g, IN) & (1 << GPIO9), 1 << GPIO9);

        sb.pins.set_pad_input_enable(PadId(GPIO9 as u8), false);
        assert_eq!(sb.read(&mut g, IN) & (1 << GPIO9), 0, "input buffer off");
    }

    /// A rising edge on an armed pad latches `status`, raises `pcpu_int` and
    /// holds source 16 until the handler clears it.
    #[test]
    fn an_armed_pad_latches_status_and_holds_the_source_until_it_is_cleared() {
        let (mut sb, mut g) = rig();
        // `int_type = 1` (posedge), `int_ena` bit 0.
        sb.write(&mut g, PIN + 4 * GPIO9, (1 << INT_TYPE_SHIFT) | INT_ENA_CPU);
        assert!(!sb.irq.level(source::GPIO));

        sb.pins.drive_pad(PadId(GPIO9 as u8), true, 0);
        let edges = sb.pins.take_edges();
        g.observe_edges(&edges, &mut sb.cx());
        assert_eq!(g.status() & (1 << GPIO9), 1 << GPIO9);
        assert_eq!(sb.read(&mut g, PCPU_INT) & (1 << GPIO9), 1 << GPIO9);
        assert!(sb.irq.level(source::GPIO), "a level, not a pulse");

        // The pad falls again: the latch is sticky.
        sb.pins.drive_pad(PadId(GPIO9 as u8), false, 1);
        let edges = sb.pins.take_edges();
        g.observe_edges(&edges, &mut sb.cx());
        assert!(sb.irq.level(source::GPIO));

        sb.write(&mut g, STATUS_W1TC, 1 << GPIO9);
        assert_eq!(g.status(), 0);
        assert!(!sb.irq.level(source::GPIO));
    }

    /// The input constants are the C6's numbers, not the classic's.
    #[test]
    fn the_always_high_and_low_selectors_are_the_pacs_own() {
        let (mut sb, mut g) = rig();
        let sig = 81u32;
        sb.write(
            &mut g,
            FUNC_IN_SEL_CFG + 4 * sig,
            SIG_IN_SEL | IN_SEL_ALWAYS_HIGH,
        );
        assert!(sb.pins.input_route_of(SignalId(sig as u16)).is_none());
        sb.write(&mut g, FUNC_IN_SEL_CFG + 4 * sig, SIG_IN_SEL | GPIO9);
        assert_eq!(
            sb.pins.input_route_of(SignalId(sig as u16)),
            Some((PadId(GPIO9 as u8), false))
        );
        // The classic's "always low" is 48, which is a real pad here — and
        // routes to it.
        sb.write(&mut g, FUNC_IN_SEL_CFG + 4 * sig, SIG_IN_SEL | 48);
        assert_eq!(
            sb.pins.input_route_of(SignalId(sig as u16)),
            Some((PadId(48), false)),
            "48 is GPIO48 on this chip, not a constant"
        );
    }

    #[test]
    fn the_state_blob_round_trips_the_registers_and_the_sample() {
        let (mut sb, mut g) = rig();
        sb.write(&mut g, PIN + 4 * GPIO9, (3 << INT_TYPE_SHIFT) | INT_ENA_CPU);
        sb.write(&mut g, ENABLE_W1TS, 1 << GPIO9);
        let blob = g.save_state();
        let mut other = Gpio::default();
        other.load_state(&blob);
        assert_eq!(other.enable(), g.enable());
        assert_eq!(other.armed, g.armed);
        assert_eq!(other.last_in, g.last_in);
    }
}
