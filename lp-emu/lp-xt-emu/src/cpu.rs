//! CPU register state and the windowed-register projection.
//!
//! The ESP32-S3 (LX7) has a 64-entry *physical* address-register file. Software
//! sees a rotating 16-register window `a0..a15`; `WindowBase` (in units of 4
//! registers) selects which physical registers those names map to:
//!
//! ```text
//! a{i}  ==  AR[(WindowBase * 4 + i) mod 64]
//! ```
//!
//! `WindowStart` has one bit per 4-register group (16 bits): a set bit at
//! position `k` marks a live call frame based at `WindowBase == k`, whose
//! registers are currently *resident* in the physical file (not spilled).
//!
//! Original code from the Xtensa ISA Reference Manual's windowed-register
//! semantics; no QEMU/binutils source used (see the repo license ADR).

/// The number of physical address registers on the S3 (LX7).
pub const NUM_AR: usize = 64;
/// Number of `WindowBase` positions (`NUM_AR / 4`).
pub const NUM_BASES: u8 = 16;
/// The number of floating-point registers (`f0..f15`). Flat — see [`Cpu::fr`].
pub const NUM_FR: usize = 16;

/// `CPENABLE` bit for coprocessor 0, the FPU. Clear ⇒ every FP instruction
/// raises [`crate::error::EXC_COPROCESSOR0_DISABLED`].
pub const CPENABLE_FPU: u32 = 1 << 0;

// ---------------------------------------------------------------------------
// FCR / FSR bit layout
//
// Architectural, not measured: the Xtensa ISA Reference Manual (2011) fixes
// both layouts in §4.3.11.3 "Floating-Point State" — Table 4-47 "FCR fields"
// (p. 69-70) and Table 4-48 "FSR fields" (p. 70). Transcribed as field
// positions from those tables; no text is reproduced here and no binutils,
// GCC, or QEMU source was consulted (AGENTS.md license rule).
//
// The two registers are laid out so an implementation may back them with one
// physical 32-bit register, which is why the FSR flags sit *above* the FCR
// enables rather than overlapping them.
// ---------------------------------------------------------------------------

/// `FCR.RM`, bits 1:0 — the rounding mode (RM ISA RM §4.3.11.3, Table 4-47).
pub const FCR_RM_MASK: u32 = 0b11;
/// `FCR.RM = 0` — round to nearest. The reset value on the desk S3 (M6 P1) and
/// the only mode `docs/design/float.md` §2 lets shader code run under.
pub const FCR_RM_NEAREST: u32 = 0;
/// `FCR.RM = 1` — round toward zero (the `TRUNC` direction).
pub const FCR_RM_TOWARD_ZERO: u32 = 1;
/// `FCR.RM = 2` — round toward +∞ (the `CEIL` direction).
pub const FCR_RM_TOWARD_POS_INF: u32 = 2;
/// `FCR.RM = 3` — round toward −∞ (the `FLOOR` direction).
pub const FCR_RM_TOWARD_NEG_INF: u32 = 3;

/// `FCR.I`, bit 2 — inexact **exception enable**.
pub const FCR_ENABLE_INEXACT: u32 = 1 << 2;
/// `FCR.U`, bit 3 — underflow exception enable.
pub const FCR_ENABLE_UNDERFLOW: u32 = 1 << 3;
/// `FCR.O`, bit 4 — overflow exception enable.
pub const FCR_ENABLE_OVERFLOW: u32 = 1 << 4;
/// `FCR.Z`, bit 5 — divide-by-zero exception enable.
pub const FCR_ENABLE_DIV_BY_ZERO: u32 = 1 << 5;
/// `FCR.V`, bit 6 — invalid-operation exception enable.
pub const FCR_ENABLE_INVALID: u32 = 1 << 6;
/// `FCR` bits 11:7 — read as zero, ignored on write.
pub const FCR_IGNORE: u32 = 0x0000_0F80;
/// `FCR` bits 31:12 — reserved. They read back the last value written, and a
/// non-zero value is defined to make every FP instruction raise a
/// floating-point exception (ISA RM §4.3.11.3). §4.3.11.4 immediately adds that
/// *current implementations* do not do that, so the emulator does not model the
/// exception; the mask exists so a future measurement has a name to attach to.
pub const FCR_RESERVED: u32 = 0xFFFF_F000;

/// `FSR.I`, bit 7 — inexact flag (ISA RM §4.3.11.3, Table 4-48).
pub const FSR_INEXACT: u32 = 1 << 7;
/// `FSR.U`, bit 8 — underflow flag.
pub const FSR_UNDERFLOW: u32 = 1 << 8;
/// `FSR.O`, bit 9 — overflow flag.
pub const FSR_OVERFLOW: u32 = 1 << 9;
/// `FSR.Z`, bit 10 — divide-by-zero flag.
///
/// This is the bit M6 P1 read back as `0x400` after its 24-instruction sweep —
/// a sweep whose `div0.s`/`recip0.s`/`rsqrt0.s` probes all ran on a staged
/// `f0 = 0.0`. The layout therefore *explains* the measurement rather than
/// merely coexisting with it. Note that the RM's §4.3.11.4 claims current
/// implementations set no FSR flags at all, which this silicon contradicts:
/// a P6 triage item, not something to resolve from the document.
pub const FSR_DIV_BY_ZERO: u32 = 1 << 10;
/// `FSR.V`, bit 11 — invalid-operation flag.
pub const FSR_INVALID: u32 = 1 << 11;
/// `FSR` bits 6:0 — read as zero, ignored on write.
pub const FSR_IGNORE: u32 = 0x0000_007F;
/// `FSR` bits 31:12 — reserved, same rule as [`FCR_RESERVED`].
pub const FSR_RESERVED: u32 = 0xFFFF_F000;

/// One live call frame, in call order (a shadow of the register-window ring).
///
/// A frame's ABI register save area is located from its *callee's* stack pointer
/// — never a per-`WindowBase` slot, because bases are reused as the ring wraps
/// and a per-base slot would be clobbered. Tracking the call chain explicitly
/// keeps every frame's `sp`/`inc` available for spill and reload exactly as the
/// hardware handler chain recovers them by walking the resident window.
#[derive(Clone, Copy, Debug)]
pub struct FrameRec {
    /// `WindowBase` this frame occupies.
    pub base: u8,
    /// This frame's stack pointer (its `a1`).
    pub sp: u32,
    /// Call increment used to enter it (owns `4*inc` registers).
    pub inc: u8,
    /// Whether its registers are currently in the physical file (vs spilled).
    pub resident: bool,
}

/// Full architectural register state.
pub struct Cpu {
    /// Program counter (an I-bus / executable address while running).
    pub pc: u32,
    /// The 64 physical address registers.
    pub ar: [u32; NUM_AR],
    /// `WindowBase`, in units of 4 registers (`0..NUM_BASES`).
    pub window_base: u8,
    /// `WindowStart` bitmap: bit `k` set ⇒ frame based at `k` is resident.
    pub window_start: u16,
    /// Shift amount register.
    pub sar: u32,
    /// `PS.CALLINC` — the window increment (1/2/3) set by the last CALL,
    /// consumed by the callee's ENTRY to know how far to rotate.
    pub ps_callinc: u8,
    /// The live call chain, innermost last — the stable identity of each frame
    /// (base is reused as the ring wraps, so it is *not* a frame identity).
    /// Drives overflow spill (oldest resident frame first) and underflow reload.
    pub call_stack: Vec<FrameRec>,

    // --- floating-point coprocessor (coprocessor 0) ---
    /// The floating-point register file `f0..f15`, stored as **raw bits**.
    ///
    /// **The FR file is flat.** It takes no part in the `window_base` rotation
    /// that backs `ar` immediately above: there is no `phys()` indirection, no
    /// `FR[(window_base * 4 + i) % 64]`, and therefore **no FR analogue of the
    /// AR file's free preservation across a windowed call**. A callee that wants
    /// an FR value to survive `call8`/`entry` must spill it itself. This
    /// asymmetry is the sharpest thing about Xtensa FP and the one M7's frame
    /// layout has to answer for.
    ///
    /// Bits, not `f32`: round-tripping through `f32` canonicalizes signalling
    /// NaN payloads on some paths, and *which* NaN survives an operation is
    /// exactly what M6 exists to measure. Executors convert at their own
    /// boundary with `f32::from_bits` / `f32::to_bits`.
    pub fr: [u32; NUM_FR],
    /// The Boolean register file `b0..b15`, one bit each (bit `i` is `b{i}`).
    ///
    /// A `u16` rather than `[bool; 16]` because `rsr.br` / `wsr.br` move all
    /// sixteen at once — that is the register's architectural shape. FP compares
    /// write here and nowhere else, so without this file a compare result cannot
    /// be observed at all.
    pub br: u16,
    /// `FCR` (user register 232) — FP control: the rounding mode plus the five
    /// exception enables. Layout in [`FCR_RM_MASK`] and friends, transcribed
    /// from the ISA RM's Table 4-47 (§4.3.11.3, p. 69-70).
    ///
    /// Reset value **0** = round to nearest with every exception disabled,
    /// measured on an ESP32-S3 (M6 P1 silicon session, 2026-07-31) and matching
    /// `docs/design/float.md` §2, which makes RNE the only mode shader code ever
    /// runs under.
    pub fcr: u32,
    /// `FSR` (user register 233) — FP status: the sticky exception flags.
    ///
    /// Reset value **0**, and the flags **accumulate**: the P1 silicon session
    /// read `FSR = 0` on a fresh boot and `FSR = 0x400` after a 24-instruction FP
    /// sweep with no intervening write. So this is a sticky register on this
    /// chip, and the emulator models accumulation via [`Cpu::or_fsr`] — even
    /// though `float.md` §2 puts FSR out of reach of shader code.
    ///
    /// Which flag occupies which bit **is** now architectural — see
    /// [`FSR_INEXACT`]…[`FSR_INVALID`], and note that `0x400` is exactly
    /// [`FSR_DIV_BY_ZERO`]. What remains unmeasured is **which operation sets
    /// which flag**, which is the `fsr_flag_bits` FP-policy field (M6 P6).
    pub fsr: u32,
    /// `CPENABLE` (special register 224) — the per-coprocessor enable mask.
    /// Bit 0 ([`CPENABLE_FPU`]) gates the FPU.
    ///
    /// Reset to **0** here, which is the architectural reset and *not* what the
    /// P1 probe saw: on the S3 under the esp-hal boot chain, CPENABLE arrives
    /// already armed (provenance unpinned — presumably ROM or the second-stage
    /// bootloader, since no write exists in esp-hal 1.1.1 or xtensa-lx-rt 0.22).
    /// The emulator starts from the architectural value deliberately, so a
    /// payload that forgets to arm the coprocessor faults on the host instead of
    /// on a board. M7's JIT context arms it defensively for the same reason.
    pub cpenable: u32,
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            pc: 0,
            ar: [0; NUM_AR],
            window_base: 0,
            window_start: 0,
            sar: 0,
            ps_callinc: 0,
            call_stack: Vec::new(),
            fr: [0; NUM_FR],
            br: 0,
            fcr: 0,
            fsr: 0,
            cpenable: 0,
        }
    }

    /// Physical AR index backing windowed register `a{i}` (i in 0..=15).
    #[inline]
    pub fn phys(&self, i: u8) -> usize {
        ((self.window_base as usize) * 4 + i as usize) % NUM_AR
    }

    /// Physical AR index for windowed register `a{i}` at an explicit base.
    #[inline]
    pub fn phys_at(base: u8, i: u8) -> usize {
        ((base as usize) * 4 + i as usize) % NUM_AR
    }

    /// Read windowed register `a{i}`.
    #[inline]
    pub fn a(&self, i: u8) -> u32 {
        self.ar[self.phys(i)]
    }

    /// Write windowed register `a{i}`. Returns the physical index written (for
    /// tracing).
    #[inline]
    pub fn set_a(&mut self, i: u8, v: u32) -> u8 {
        let p = self.phys(i);
        self.ar[p] = v;
        p as u8
    }

    // --- floating point. No rotation, no `phys()`: that is the point. ---

    /// Read the raw bits of `f{i}` (i in 0..=15).
    #[inline]
    pub fn f(&self, i: u8) -> u32 {
        self.fr[i as usize & 0xf]
    }

    /// Write the raw bits of `f{i}` (i in 0..=15).
    #[inline]
    pub fn set_f(&mut self, i: u8, bits: u32) {
        self.fr[i as usize & 0xf] = bits;
    }

    /// Read boolean register `b{i}` (i in 0..=15).
    #[inline]
    pub fn b(&self, i: u8) -> bool {
        (self.br >> (i & 0xf)) & 1 != 0
    }

    /// Write boolean register `b{i}` (i in 0..=15).
    #[inline]
    pub fn set_b(&mut self, i: u8, v: bool) {
        let bit = 1u16 << (i & 0xf);
        if v {
            self.br |= bit;
        } else {
            self.br &= !bit;
        }
    }

    /// Accumulate sticky flags into `FSR`.
    ///
    /// The register is sticky on silicon (see [`Cpu::fsr`]), so flags are OR-ed
    /// in and only an explicit `wur.fsr` can clear them.
    #[inline]
    pub fn or_fsr(&mut self, bits: u32) {
        self.fsr |= bits;
    }

    /// The `FCR.RM` rounding-mode field alone — one of [`FCR_RM_NEAREST`],
    /// [`FCR_RM_TOWARD_ZERO`], [`FCR_RM_TOWARD_POS_INF`],
    /// [`FCR_RM_TOWARD_NEG_INF`] (ISA RM §4.3.11.3, Table 4-47).
    ///
    /// Separate from a bare `fcr != 0` test on purpose: setting an exception
    /// *enable* bit does not change how a result rounds, and conflating the two
    /// would make the emulator refuse a mode change that never happened.
    #[inline]
    pub fn fcr_rounding_mode(&self) -> u32 {
        self.fcr & FCR_RM_MASK
    }

    /// Whether coprocessor 0 (the FPU) is currently enabled.
    #[inline]
    pub fn fpu_enabled(&self) -> bool {
        self.cpenable & CPENABLE_FPU != 0
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu::new()
    }
}
