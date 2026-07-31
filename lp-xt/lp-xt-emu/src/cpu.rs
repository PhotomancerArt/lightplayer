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
    /// `FCR` (user register 232) — FP control: the rounding mode.
    ///
    /// Reset value **0** = round-to-nearest-even, measured on an ESP32-S3
    /// (M6 P1 silicon session, 2026-07-31) and matching `docs/design/float.md`
    /// §2, which makes RNE the only mode shader code ever runs under.
    pub fcr: u32,
    /// `FSR` (user register 233) — FP status: the sticky exception flags.
    ///
    /// Reset value **0**, and the flags **accumulate**: the P1 silicon session
    /// read `FSR = 0` on a fresh boot and `FSR = 0x400` after a 24-instruction FP
    /// sweep with no intervening write. So this is a sticky register on this
    /// chip, and the emulator models accumulation via [`Cpu::or_fsr`] — even
    /// though `float.md` §2 puts FSR out of reach of shader code. Which flag
    /// occupies which bit, and which operation sets which flag, is **not** known
    /// and is an unresolved FP-policy field (M6 P6 measures it).
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
