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
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu::new()
    }
}
