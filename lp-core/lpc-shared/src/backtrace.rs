//! Backtrace capture and panic payload for panic recovery.
//!
//! Used by platform panic handlers to build a payload that survives unwinding,
//! and by the engine to format panic errors for node status.

use alloc::string::String;
use core::fmt::{self, Write as _};
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_FRAMES: usize = 16;
const MAX_MESSAGE_BYTES: usize = 160;
const MAX_FILE_BYTES: usize = 96;
static OOM_CONTEXT_PTR: AtomicUsize = AtomicUsize::new(0);
static OOM_CONTEXT_LEN: AtomicUsize = AtomicUsize::new(0);

/// Panic payload that survives unwinding.
///
/// Built by platform panic handlers, caught by catch_unwind in the engine.
/// Implements Send for compatibility with unwinding::panic::begin_panic.
pub struct PanicPayload {
    pub message: FixedStr<MAX_MESSAGE_BYTES>,
    pub file: Option<FixedStr<MAX_FILE_BYTES>>,
    pub line: Option<u32>,
    pub oom: Option<OomInfo>,
    pub frames: [u32; MAX_FRAMES],
    pub frame_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OomInfo {
    pub requested: usize,
    pub align: usize,
    pub free: usize,
    pub used: usize,
    pub context: Option<&'static str>,
}

impl PanicPayload {
    pub fn new(message: impl fmt::Display, file: Option<&str>, line: Option<u32>) -> Self {
        Self::new_inner(message, file, line, None)
    }

    pub fn new_oom(
        message: impl fmt::Display,
        file: Option<&str>,
        line: Option<u32>,
        oom: OomInfo,
    ) -> Self {
        Self::new_inner(message, file, line, Some(oom))
    }

    fn new_inner(
        message: impl fmt::Display,
        file: Option<&str>,
        line: Option<u32>,
        oom: Option<OomInfo>,
    ) -> Self {
        let mut payload = Self {
            message: FixedStr::from_display(message),
            file: file.map(FixedStr::from_str),
            line,
            oom,
            frames: [0; MAX_FRAMES],
            frame_count: 0,
        };
        payload.frame_count = capture_frames(&mut payload.frames);
        payload
    }

    /// Format as error string for NodeStatus::Error.
    ///
    /// Format: "panic: <msg> (at <file>:<line>) [0x00001234, 0x00005678, ...]; decode: just decode-backtrace 0x00001234 ..."
    pub fn format_error(&self) -> String {
        let mut s = String::new();
        if let Some(oom) = self.oom {
            push_fmt(
                &mut s,
                format_args!(
                    "oom: requested={} align={} free={} used={}; ",
                    oom.requested, oom.align, oom.free, oom.used
                ),
            );
            if let Some(context) = oom.context {
                push_fmt(&mut s, format_args!("context={context}; "));
            }
        }
        push_fmt(&mut s, format_args!("panic: {}", self.message.as_str()));
        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                push_fmt(&mut s, format_args!(" (at {}:{line})", file.as_str()));
            } else {
                push_fmt(&mut s, format_args!(" (at {})", file.as_str()));
            }
        }
        if self.frame_count > 0 {
            s.push_str(" [");
            for i in 0..self.frame_count {
                if i > 0 {
                    s.push_str(", ");
                }
                push_fmt(&mut s, format_args!("0x{:08x}", self.frames[i]));
            }
            s.push(']');
            s.push_str("; decode: just decode-backtrace");
            for i in 0..self.frame_count {
                push_fmt(&mut s, format_args!(" 0x{:08x}", self.frames[i]));
            }
        }
        s
    }
}

pub struct FixedStr<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> FixedStr<N> {
    pub fn from_str(value: &str) -> Self {
        let mut out = Self {
            bytes: [0; N],
            len: 0,
        };
        out.push_str(value);
        out
    }

    pub fn from_display(value: impl fmt::Display) -> Self {
        let mut out = Self {
            bytes: [0; N],
            len: 0,
        };
        let _ = write!(out, "{value}");
        out
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<invalid utf8>")
    }

    fn push_str(&mut self, value: &str) {
        if self.len >= N {
            return;
        }
        let remaining = N - self.len;
        let mut end = value.len().min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.bytes[self.len..self.len + end].copy_from_slice(&value.as_bytes()[..end]);
        self.len += end;
    }
}

impl<const N: usize> fmt::Write for FixedStr<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

fn push_fmt(out: &mut String, args: fmt::Arguments<'_>) {
    let _ = out.write_fmt(args);
}

pub fn set_oom_context(context: &'static str) {
    OOM_CONTEXT_PTR.store(context.as_ptr() as usize, Ordering::Relaxed);
    OOM_CONTEXT_LEN.store(context.len(), Ordering::Relaxed);
}

pub fn clear_oom_context() {
    OOM_CONTEXT_LEN.store(0, Ordering::Relaxed);
    OOM_CONTEXT_PTR.store(0, Ordering::Relaxed);
}

pub fn oom_context() -> Option<&'static str> {
    let ptr = OOM_CONTEXT_PTR.load(Ordering::Relaxed);
    let len = OOM_CONTEXT_LEN.load(Ordering::Relaxed);
    if ptr == 0 || len == 0 {
        return None;
    }

    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok()
}

/// Capture stack frame return addresses into `buf`.
///
/// Returns the number of frames written. Platform-specific: uses frame pointer
/// walking on supported architectures, returns 0 on unsupported platforms.
pub fn capture_frames(buf: &mut [u32]) -> usize {
    capture_frames_arch(buf)
}

/// RISC-V (ESP32-C6) frame-pointer walk.
///
/// The `s0` chain and the C6 DRAM window below are both RV32-specific, so this
/// gate is a bare `target_arch = "riscv32"`: the Xtensa walk is a **sibling**
/// `#[cfg(target_arch = "xtensa")] fn capture_frames_arch` below rather than a
/// widening of this arm. The only gate that took a mechanical
/// `, target_arch = "xtensa"` is the
/// `any(target_arch = "riscv32", target_arch = "xtensa")` inside the fallback's
/// exclusion set below (the JIT-capable-target spelling documented in
/// `lpvm_native`'s crate docs), which is what stops the new arm from colliding
/// with the 0-frame default.
#[cfg(target_arch = "riscv32")]
fn capture_frames_arch(buf: &mut [u32]) -> usize {
    const ESP32C6_DRAM_START: u32 = 0x4080_0000;
    const ESP32C6_DRAM_END: u32 = 0x4088_0000;

    fn is_valid_esp32c6_dram(address: u32) -> bool {
        (ESP32C6_DRAM_START..ESP32C6_DRAM_END).contains(&address)
    }

    if buf.is_empty() {
        return 0;
    }

    let fp: u32;
    unsafe { core::arch::asm!("mv {}, s0", out(reg) fp) };

    let mut fp = fp;
    if !is_valid_esp32c6_dram(fp) || fp % 4 != 0 {
        return 0;
    }

    let mut count = 0;
    while count < buf.len() {
        let ra = unsafe { (fp.wrapping_sub(4) as *const u32).read() };
        let prev_fp = unsafe { (fp.wrapping_sub(8) as *const u32).read() };

        if ra != 0 {
            // Saved RISC-V return addresses point after the call instruction.
            // Report the callsite PC so addr2line lands on the useful frame.
            buf[count] = ra.saturating_sub(4);
            count += 1;
        }

        if prev_fp == 0 || !is_valid_esp32c6_dram(prev_fp) || prev_fp <= fp {
            break;
        }
        fp = prev_fp;
    }
    count
}

#[cfg(target_arch = "wasm32")]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}

// ---------------------------------------------------------------------------
// Xtensa (ESP32-S3 / LX7) windowed-ABI save-area walk
// ---------------------------------------------------------------------------
//
// Split deliberately into a *pure* chain walk ([`walk_save_area_chain`], which
// takes its memory reader as a parameter) and a tiny target-only prologue
// ([`capture_frames_arch`]) that forces the spill and supplies the live
// registers. The pure half is compiled on the host under `cfg(test)`, so the
// exact-frame-count oracle, the bounds checks, and the corrupt-chain
// termination are proven mechanically without a device; the prologue is the
// part only silicon can confirm, which is what
// `fw-esp32s3 --features test_backtrace_oracle` exists for.
//
// The ABI facts this rests on, from the Xtensa ISA Reference Manual's Windowed
// Register Option and the ESP32-S3 TRM (no GDB/binutils/GCC/QEMU source was
// read or adapted — see `docs/adr/2026-07-29-license-provenance-discipline.md`):
//
//   * `CALLn` writes the return address into the caller's `a(4n)` as *30 bits*
//     — `a0[31:30]` carries the call increment `n`, not address bits. The full
//     address is recovered from the 1 GB region the caller executes in.
//   * A window overflow spills the overflowing frame's `a0..a3` into the
//     16-byte **base save area** at `[callee_sp - 16, callee_sp)`, where
//     `callee_sp` is the stack pointer of the frame it called. Read from the
//     current frame that is the ordinary statement of the ABI: `[sp - 16]`
//     holds the caller's `a0` (its return address) and `[sp - 12]` holds the
//     caller's `a1` (its stack pointer). Every overflow width — 4, 8 and 12 —
//     places `a0..a3` there, so the chain is uniform regardless of whether a
//     frame was entered by `call4`, `call8` or `call12`.
//   * `ENTRY` — and only `ENTRY`/`RETW` — performs the overflow/underflow
//     check. That is what [`force_window_spill`] exploits.
//
// The same placement is what `lp-xt-emu`'s window machinery models
// (`lp-xt/lp-xt-emu/src/executor/window.rs`, `save_slot`), which was dual-run
// against S3 silicon to depth 100 in the Xtensa backport spike.

// ---------------------------------------------------------------------------
// Chip memory windows
// ---------------------------------------------------------------------------
//
// Everything above this point — the window spill, the base-save-area chain,
// the region-bit restore — is pure windowed-ABI and byte-identical on LX6 and
// LX7. The only thing that differs between an ESP32-S3 and a classic ESP32 is
// *where* the six window boundaries sit.
//
// They are picked by a cargo feature rather than sniffed at runtime, because a
// firmware image targets exactly one chip and the wrong window set does not
// fail loudly. It reports **zero frames** — which `panic_path::print_frames`
// then renders as "the walk ran and rejected everything", i.e. "the stack was
// unreadable". That sentence would be confidently wrong; the truth would be
// "this walker was calibrated for a different chip". A silent miscalibration
// that reads as a real forensic result is worse than no walker at all, which
// is why the choice is explicit at the dependency line.
//
// `xt-map-esp32-classic` selects the classic ESP32 (LX6); the default is the
// ESP32-S3 (LX7). A build that enables neither and runs on classic silicon is
// the failure this comment exists to prevent.
//
// Both sets are *compiled* on every Xtensa and host build and only the alias
// below is cfg'd, so the host test suite bounds-checks the classic constants
// even though no host build ever selects them. Constants nothing can execute
// are exactly the kind that rot.

// `xt_map` is the window set this build's walker is calibrated for. Plain
// comments rather than doc comments on purpose: rustfmt sorts these two
// imports alphabetically, so a `///` here would drift onto whichever arm
// happens to sort first.
#[cfg(all(any(target_arch = "xtensa", test), feature = "xt-map-esp32-classic"))]
use esp32_classic_map as xt_map;
#[cfg(all(
    any(target_arch = "xtensa", test),
    not(feature = "xt-map-esp32-classic")
))]
use esp32s3_map as xt_map;

/// ESP32-S3 (LX7) windows — the default.
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(feature = "xt-map-esp32-classic", allow(dead_code))]
mod esp32s3_map {
    /// Internal SRAM as the D-bus sees it — SRAM1 `0x3FC8_8000..0x3FCF_0000`
    /// plus SRAM2 `0x3FCF_0000..0x3FD0_0000` (ESP32-S3 TRM, "System and
    /// Memory"). Task stacks live here. PSRAM (`0x3C00_0000..`) is deliberately
    /// **not** accepted: nothing in this firmware puts a stack there, and
    /// widening the window only buys a larger space in which garbage can look
    /// valid.
    pub const DRAM_START: u32 = 0x3FC8_8000;
    pub const DRAM_END: u32 = 0x3FD0_0000;

    /// Internal SRAM as the I-bus sees it: SRAM0 `0x4037_0000..0x4038_0000` and
    /// SRAM1 `0x4038_0000..0x403E_0000`.
    pub const IRAM_START: u32 = 0x4037_0000;
    pub const IRAM_END: u32 = 0x403E_0000;

    /// External flash through the instruction cache: `0x4200_0000..0x4400_0000`.
    pub const FLASH_START: u32 = 0x4200_0000;
    pub const FLASH_END: u32 = 0x4400_0000;
}

/// Classic ESP32 (LX6) windows — `fw-esp32v3`.
///
/// From the ESP32 TRM §1.3.2, "Embedded Memory" address mapping. Every bound
/// here is architectural, matching how the S3 set above is derived: these are
/// the ranges the *silicon* can execute or stack in, not the ranges this
/// firmware's linker segments happen to occupy. A tighter window would reject
/// real frames the moment a segment moved.
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(feature = "xt-map-esp32-classic"), allow(dead_code))]
mod esp32_classic_map {
    /// Internal SRAM as the D-bus sees it — SRAM2 `0x3FFA_E000..0x3FFE_0000`
    /// (200 KB) plus SRAM1 `0x3FFE_0000..0x4000_0000` (128 KB). esp-hal's
    /// `dram_seg` (`0x3FFB_0000..0x3FFE_0000`) sits inside it, and so does the
    /// `dram2_seg` the JIT's code region overlaps.
    ///
    /// RTC fast RAM's D-bus alias (`0x3FF8_0000`, 8 KB — where the recovery
    /// ledger itself lives) is **not** included: no stack is ever placed there,
    /// and the walker must not accept a save area inside the very region it is
    /// about to write a crash record into.
    pub const DRAM_START: u32 = 0x3FFA_E000;
    pub const DRAM_END: u32 = 0x4000_0000;

    /// Internal SRAM as the I-bus sees it: SRAM0 `0x4008_0000..0x400A_0000`
    /// (128 KB) and SRAM1 `0x400A_0000..0x400C_0000` (128 KB).
    ///
    /// Both halves matter on this chip. SRAM1's I-bus alias is where
    /// `lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT` installs JIT'd
    /// shader code (`0x400B_0000..0x400B_8000` as of 2026-08-02, when the
    /// region was measured down from 92 KiB to 32 KiB and the remainder became
    /// heap), so a frame that faulted inside a compiled shader lands in this
    /// window and is reported rather than silently dropped.
    ///
    /// The window is deliberately the *whole* SRAM1 alias rather than the JIT
    /// region's exact bounds: this crate cannot see `lpvm-native`, and a walker
    /// that tracked the region's precise address would need editing every time
    /// the region were resized — silently dropping shader frames until someone
    /// noticed. Accepting all of SRAM1 costs nothing (nothing else executes
    /// there) and cannot go stale.
    ///
    /// Internal ROM (`0x4000_0000..0x4008_0000`, 512 KB across ROM0 and ROM1)
    /// is excluded for the same reason as on the S3, and the exclusion is
    /// load-bearing in the same way: a zeroed `a0` restores to `0x4000_0000`,
    /// and accepting ROM would turn the chain terminator into a plausible
    /// frame. RTC fast RAM's I-bus alias (`0x400C_0000`, 8 KB) is excluded too
    /// — nothing here executes from it.
    pub const IRAM_START: u32 = 0x4008_0000;
    pub const IRAM_END: u32 = 0x400C_0000;

    /// External flash through the instruction cache: `0x400D_0000..0x40C0_0000`
    /// (11 MB of IROM). This is where `.text` actually lives on this image, so
    /// it is the window most frames fall in.
    pub const FLASH_START: u32 = 0x400D_0000;
    pub const FLASH_END: u32 = 0x40C0_0000;
}

/// Bytes in a `CALLn`/`CALLXn` instruction. The saved return address points at
/// the instruction *after* the call, so reporting `ra - 3` makes `addr2line`
/// land on the call itself rather than on whatever follows it — which for a
/// call in tail position is the next function entirely.
#[cfg(any(target_arch = "xtensa", test))]
const XT_CALL_INST_BYTES: u32 = 3;

/// Is `pc` inside a range this chip can execute LightPlayer code from?
///
/// Internal ROM is executable on both chips but deliberately **excluded** from
/// both [`xt_map`] sets: no frame in this firmware returns into ROM, and
/// accepting it would both widen the garbage-looks-valid window and make a
/// zeroed `a0` (which the region fix-up turns into `0x4000_0000`) look like a
/// real frame instead of the chain terminator it is.
#[cfg(any(target_arch = "xtensa", test))]
fn is_valid_xt_text(pc: u32) -> bool {
    (xt_map::IRAM_START..xt_map::IRAM_END).contains(&pc)
        || (xt_map::FLASH_START..xt_map::FLASH_END).contains(&pc)
}

/// Is `sp` a stack pointer we are willing to read a base save area from?
///
/// Requires 16-byte alignment (the windowed ABI's stack alignment) and leaves
/// room below for `[sp-16, sp)`, so a caller that passes this check may read
/// the save area unconditionally.
#[cfg(any(target_arch = "xtensa", test))]
fn is_valid_xt_stack(sp: u32) -> bool {
    sp % 16 == 0 && (xt_map::DRAM_START + 16..xt_map::DRAM_END).contains(&sp)
}

/// Walk the windowed-ABI base-save-area chain, starting from a frame whose
/// return address is `ra` and whose stack pointer is `sp`.
///
/// `read_u32` reads a 4-byte-aligned word; it is only ever called for addresses
/// that already passed [`is_valid_xt_stack`], so implementations may read
/// raw memory without further checks. Splitting it out is what lets the host
/// test suite drive this against a synthetic stack.
///
/// Terminates unconditionally: every iteration either breaks or advances `sp`
/// strictly upward, and `count` is bounded by `buf.len()`.
#[cfg(any(target_arch = "xtensa", test))]
fn walk_save_area_chain(buf: &mut [u32], ra: u32, sp: u32, read_u32: impl Fn(u32) -> u32) -> usize {
    let mut ra = ra;
    let mut sp = sp;
    let mut count = 0;

    while count < buf.len() {
        // Restore the region bits `CALLn` did not store. Everything either
        // chip executes — internal SRAM's I-bus alias and the flash cache
        // window — lives in region 1 (`0x4xxx_xxxx`) on both the S3 and the
        // classic, so this is a constant, not a per-chip fact. A wrong guess
        // cannot leak through: the result is bounds-checked immediately below.
        let pc = (ra & 0x3FFF_FFFF) | 0x4000_0000;
        if !is_valid_xt_text(pc) {
            break;
        }
        buf[count] = pc.saturating_sub(XT_CALL_INST_BYTES);
        count += 1;

        if !is_valid_xt_stack(sp) {
            break;
        }
        let next_ra = read_u32(sp - 16);
        let next_sp = read_u32(sp - 12);
        // The stack grows down, so every caller sits strictly above its
        // callee — that is what stops a self-referential or corrupt chain from
        // cycling. Both halves of the save area are validated *before* either
        // is used: a save area whose stack pointer is garbage is not a save
        // area, and reporting the return address next to it would be exactly
        // the plausible-looking-garbage failure this walk exists to avoid.
        if next_sp <= sp || !is_valid_xt_stack(next_sp) {
            break;
        }
        ra = next_ra;
        sp = next_sp;
    }
    count
}

/// Walk the chain from an explicit `(ra, sp)` pair, reading real memory.
///
/// Exposed for two callers that cannot use [`capture_frames`]: the hardware
/// oracle in `fw-esp32s3`'s `test_backtrace_oracle` harness, which drives it
/// with deliberately corrupt stack pointers and with *unspilled* live
/// registers to prove the spill in [`capture_frames_arch`] is load-bearing;
/// and any future exception-frame walker, which starts from a saved context
/// rather than from its own registers.
#[cfg(target_arch = "xtensa")]
pub fn walk_frames_from(buf: &mut [u32], ra: u32, sp: u32) -> usize {
    // SAFETY: `walk_save_area_chain` only calls this for addresses that passed
    // `is_valid_xt_stack`, i.e. 16-aligned words inside internal SRAM.
    walk_save_area_chain(buf, ra, sp, |addr| unsafe {
        (addr as *const u32).read_volatile()
    })
}

/// How many nested windowed calls guarantee every live window has spilled.
///
/// Both supported chips have a 64-entry physical register file = 16
/// `WindowBase` units (LX7 on the S3, LX6 on the classic — the register file
/// is one of the things the two cores do *not* differ on, which is why this
/// depth is not in [`xt_map`]).
/// Each nested call advances `WindowBase` by its call increment — 1 for
/// `call4`, 2 for `call8`, 3 for `call12` — and `ENTRY` spills whichever live
/// frame still owns the units the new frame claims. Sixteen nested calls
/// therefore sweep the whole ring even in the worst case where every call is a
/// `call4`, which is why the depth is 16 rather than the 8 a `call8`-only
/// chain would need. Measured codegen uses `call8` (48-byte frames), so this
/// costs about 800 bytes of stack, spent once, on a path that is already
/// resetting the chip.
#[cfg(target_arch = "xtensa")]
const WINDOW_SPILL_DEPTH: u32 = 16;

/// Push `n` nested windowed frames and unwind them again.
///
/// `black_box` on both the argument and the result is load-bearing: without it
/// LLVM turns this accumulator recursion into a loop and no window rotation
/// happens at all. `#[inline(never)]` keeps it a real call.
#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn spill_step(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let deeper = spill_step(core::hint::black_box(n - 1));
    core::hint::black_box(deeper).wrapping_add(1)
}

/// Force every live register window out to its stack base save area.
///
/// This is the step that makes an Xtensa backtrace possible at all. A frame's
/// `a0`/`a1` are in the *physical register file* until something spills them;
/// walking memory without spilling reads whatever was last left below those
/// stack pointers and reports plausible-looking garbage for the innermost
/// frames.
///
/// The mechanism is `ENTRY`'s own overflow check rather than a hand-written
/// spill loop: nest enough calls to rotate `WindowBase` through all 16 units
/// and the hardware's window-overflow handler necessarily writes every
/// previously-live frame to its save area. Unwinding back out reloads the
/// registers but does **not** erase the memory copies, so the chain is intact
/// when this returns.
///
/// Nothing between here and the walk may make a call that could clobber
/// `[sp-16, sp)` of the capturing frame — and nothing can: the ABI reserves
/// those 16 bytes in every frame precisely as a base save area, so a callee's
/// locals never land there, and the only writer is a re-spill of the same
/// caller with the same values.
#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn force_window_spill() {
    let _ = core::hint::black_box(spill_step(WINDOW_SPILL_DEPTH));
}

/// Xtensa windowed-ABI walk (LX7 on the ESP32-S3, LX6 on the classic ESP32 —
/// the walk is identical; only [`xt_map`]'s bounds differ).
///
/// Spills the register windows, then follows the base save-area chain from
/// this frame's live `a0`/`a1`. Frame 0 is the return address into *this*
/// function's caller, matching the riscv32 arm's convention.
#[cfg(target_arch = "xtensa")]
fn capture_frames_arch(buf: &mut [u32]) -> usize {
    if buf.is_empty() {
        return 0;
    }

    force_window_spill();

    let ra: u32;
    let sp: u32;
    // SAFETY: two register reads with no side effects. `a0` is the windowed
    // return address and `a1` the stack pointer; both are architecturally
    // live here because this function is a windowed frame (it just made a
    // call, so the compiler cannot have elided its `ENTRY`).
    unsafe {
        core::arch::asm!(
            "mov {ra}, a0",
            "mov {sp}, a1",
            ra = out(reg) ra,
            sp = out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
    }

    walk_frames_from(buf, ra, sp)
}

/// No frame walker for this target: report zero frames.
#[cfg(not(any(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    target_arch = "wasm32"
)))]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payload_formats_oom_context() {
        let payload = PanicPayload::new_oom(
            "memory allocation of 81920 bytes failed",
            Some("fw.rs"),
            Some(104),
            OomInfo {
                requested: 81920,
                align: 4,
                free: 108408,
                used: 211592,
                context: Some("load project"),
            },
        );

        let error = payload.format_error();

        assert!(error.contains("oom: requested=81920 align=4 free=108408 used=211592"));
        assert!(error.contains("context=load project"));
        assert!(error.contains("panic: memory allocation of 81920 bytes failed"));
        assert!(error.contains("fw.rs:104"));
    }

    #[test]
    fn panic_payload_formats_decode_command_for_frames() {
        let mut payload = PanicPayload::new("boom", Some("fw.rs"), Some(104));
        payload.frames[0] = 0x4208c8fa;
        payload.frames[1] = 0x42097332;
        payload.frame_count = 2;

        let error = payload.format_error();

        assert!(error.contains("[0x4208c8fa, 0x42097332]"));
        assert!(error.contains("decode: just decode-backtrace 0x4208c8fa 0x42097332"));
    }

    #[test]
    fn fixed_str_truncates_at_utf8_boundary() {
        let text = FixedStr::<5>::from_str("abcdé");

        assert_eq!(text.as_str(), "abcd");
    }

    // -----------------------------------------------------------------------
    // Xtensa windowed-ABI walk — the mechanical oracle for the memory half.
    //
    // Every stack here is synthesized *inside the real ESP32-S3 DRAM window*,
    // so the walker's bounds checks are exercised rather than bypassed, and
    // [`SynthStack::read`] panics on any access outside the region it owns —
    // which makes "the walker read memory it had not validated" a test failure
    // rather than an undetectable habit.
    //
    // The device half (the forced window spill) cannot be proven here; that is
    // what `fw-esp32s3`'s `test_backtrace_oracle` harness is for.
    // -----------------------------------------------------------------------

    /// Distinct so a frame's provenance is visible in the assertions: the
    /// innermost frame's return address, and the one every synthesized caller
    /// shares (mirroring the on-device recursion, whose callers all return to
    /// the same call site).
    const RA_INNER: u32 = 0x4200_1000;
    const RA_CALLER: u32 = 0x4200_2000;
    /// Bytes between synthesized frames. Any 16-multiple ≥ 16 works.
    const SYNTH_FRAME: u32 = 32;

    struct SynthStack {
        base: u32,
        words: alloc::vec::Vec<u32>,
    }

    impl SynthStack {
        fn new(base: u32, frames: usize) -> Self {
            assert_eq!(base % 16, 0);
            Self {
                base,
                words: alloc::vec![0; (frames + 4) * SYNTH_FRAME as usize / 4],
            }
        }

        fn sp(&self, index: usize) -> u32 {
            self.base + 2 * SYNTH_FRAME + index as u32 * SYNTH_FRAME
        }

        fn write(&mut self, addr: u32, value: u32) {
            let index = (addr - self.base) as usize / 4;
            self.words[index] = value;
        }

        fn read(&self, addr: u32) -> u32 {
            assert!(
                addr >= self.base && addr % 4 == 0,
                "walker read {addr:#x}, outside the stack it validated",
            );
            let index = (addr - self.base) as usize / 4;
            assert!(
                index < self.words.len(),
                "walker read {addr:#x}, past the stack"
            );
            self.words[index]
        }

        /// Link `depth` frames so a walk seeded with `(RA_INNER, sp(0))`
        /// reports exactly `depth` frames: `depth - 1` valid hops, then a
        /// zeroed save area that terminates it.
        fn chain(depth: usize) -> Self {
            let mut stack = SynthStack::new(0x3FCC_0000, depth + 2);
            for i in 0..depth - 1 {
                let sp = stack.sp(i);
                let next_sp = stack.sp(i + 1);
                stack.write(sp - 16, RA_CALLER);
                stack.write(sp - 12, next_sp);
            }
            stack
        }
    }

    fn walk(stack: &SynthStack, ra: u32, sp: u32, buf: &mut [u32]) -> usize {
        walk_save_area_chain(buf, ra, sp, |addr| stack.read(addr))
    }

    #[test]
    fn xtensa_walk_reports_the_exact_chain_depth() {
        // 20 is the depth the Xtensa backport proved forces real window
        // spills on silicon; the same number the device harness uses.
        let stack = SynthStack::chain(20);
        let mut buf = [0u32; 32];

        let count = walk(&stack, RA_INNER, stack.sp(0), &mut buf);

        assert_eq!(count, 20);
        assert_eq!(buf[0], RA_INNER - XT_CALL_INST_BYTES);
        for (i, frame) in buf[1..20].iter().enumerate() {
            assert_eq!(*frame, RA_CALLER - XT_CALL_INST_BYTES, "frame {}", i + 1);
        }
        assert_eq!(buf[20], 0, "wrote past the frames it reported");
    }

    #[test]
    fn xtensa_walk_reports_every_frame_inside_the_text_windows() {
        let stack = SynthStack::chain(20);
        let mut buf = [0u32; 32];

        let count = walk(&stack, RA_INNER, stack.sp(0), &mut buf);

        for frame in &buf[..count] {
            assert!(
                is_valid_xt_text(*frame),
                "reported {frame:#010x}, outside IRAM and the flash cache window",
            );
        }
    }

    #[test]
    fn xtensa_walk_saturates_at_the_buffer_length() {
        let stack = SynthStack::chain(20);
        let mut buf = [0u32; 8];

        assert_eq!(walk(&stack, RA_INNER, stack.sp(0), &mut buf), 8);
    }

    #[test]
    fn xtensa_walk_reports_nothing_into_an_empty_buffer() {
        let stack = SynthStack::chain(20);

        assert_eq!(walk(&stack, RA_INNER, stack.sp(0), &mut []), 0);
    }

    #[test]
    fn xtensa_walk_stops_where_a_chain_is_corrupted() {
        let mut stack = SynthStack::chain(20);
        // Frame 10's saved caller stack pointer is garbage.
        let sp = stack.sp(10);
        stack.write(sp - 12, 0xDEAD_BEEF);
        let mut buf = [0u32; 32];

        // 11 frames: the seed plus frames 1..=10. The garbage pointer is
        // rejected on the spot, and the return address sitting next to it in
        // the same corrupt save area is never reported.
        assert_eq!(walk(&stack, RA_INNER, stack.sp(0), &mut buf), 11);
    }

    #[test]
    fn xtensa_walk_stops_on_a_self_referential_chain() {
        let mut stack = SynthStack::chain(20);
        let sp = stack.sp(0);
        stack.write(sp - 16, RA_CALLER);
        stack.write(sp - 12, sp);
        let mut buf = [0u32; 32];

        assert_eq!(walk(&stack, RA_INNER, sp, &mut buf), 1);
    }

    #[test]
    fn xtensa_walk_stops_when_the_chain_descends() {
        let mut stack = SynthStack::chain(20);
        let sp = stack.sp(4);
        stack.write(sp - 12, sp - SYNTH_FRAME);
        let mut buf = [0u32; 32];

        // Frames 0..=4 report, then the backwards hop stops it.
        assert_eq!(walk(&stack, RA_INNER, stack.sp(0), &mut buf), 5);
    }

    #[test]
    fn xtensa_walk_rejects_a_stack_pointer_outside_dram() {
        let stack = SynthStack::chain(4);
        let mut buf = [0u32; 32];

        // The seed return address is still reportable; nothing is read.
        assert_eq!(walk(&stack, RA_INNER, 0x2000_0000, &mut buf), 1);
        assert_eq!(walk(&stack, RA_INNER, xt_map::DRAM_END, &mut buf), 1);
        assert_eq!(walk(&stack, RA_INNER, xt_map::DRAM_START, &mut buf), 1);
    }

    #[test]
    fn xtensa_walk_rejects_an_unaligned_stack_pointer() {
        let stack = SynthStack::chain(4);
        let mut buf = [0u32; 32];

        assert_eq!(walk(&stack, RA_INNER, stack.sp(0) + 4, &mut buf), 1);
    }

    #[test]
    fn xtensa_walk_reports_nothing_for_a_return_address_outside_text() {
        let stack = SynthStack::chain(4);
        let mut buf = [0u32; 32];

        // A zeroed `a0` becomes 0x4000_0000 once the region bits are restored;
        // it must read as the chain terminator, not as internal ROM.
        assert_eq!(walk(&stack, 0, stack.sp(0), &mut buf), 0);
        assert_eq!(walk(&stack, 0x4000_0000, stack.sp(0), &mut buf), 0);
        // DRAM is not executable.
        assert_eq!(walk(&stack, 0x3FCC_0000, stack.sp(0), &mut buf), 0);
    }

    #[test]
    fn xtensa_walk_strips_the_call_increment_bits() {
        let stack = SynthStack::chain(4);
        let mut buf = [0u32; 32];

        // `call12` leaves 0b11 in a0[31:30]; the address underneath is
        // RA_INNER.
        let encoded = 0xC000_0000 | (RA_INNER & 0x3FFF_FFFF);

        assert_eq!(walk(&stack, encoded, stack.sp(0), &mut buf), 4);
        assert_eq!(buf[0], RA_INNER - XT_CALL_INST_BYTES);
    }

    #[test]
    fn xtensa_text_window_excludes_rom_dram_and_psram() {
        assert!(is_valid_xt_text(xt_map::IRAM_START));
        assert!(is_valid_xt_text(xt_map::IRAM_END - 4));
        assert!(is_valid_xt_text(xt_map::FLASH_START));
        assert!(is_valid_xt_text(xt_map::FLASH_END - 4));

        assert!(!is_valid_xt_text(0x4000_0000), "internal ROM");
        assert!(!is_valid_xt_text(xt_map::IRAM_START - 4));
        assert!(!is_valid_xt_text(xt_map::IRAM_END));
        assert!(!is_valid_xt_text(xt_map::FLASH_END));
        assert!(!is_valid_xt_text(0x3FCC_0000), "DRAM");
        assert!(!is_valid_xt_text(0x3C00_0000), "PSRAM");
    }

    #[test]
    fn xtensa_stack_window_requires_alignment_and_headroom() {
        assert!(is_valid_xt_stack(xt_map::DRAM_START + 16));
        assert!(is_valid_xt_stack(xt_map::DRAM_END - 16));

        assert!(
            !is_valid_xt_stack(xt_map::DRAM_START),
            "no room for [sp-16, sp)"
        );
        assert!(!is_valid_xt_stack(xt_map::DRAM_END));
        assert!(
            !is_valid_xt_stack(xt_map::DRAM_START + 20),
            "not 16-aligned"
        );
        assert!(!is_valid_xt_stack(0));
    }

    /// Every window is non-empty and correctly ordered.
    #[test]
    fn both_chip_maps_are_ordered() {
        for (name, start, end) in [
            ("s3 dram", esp32s3_map::DRAM_START, esp32s3_map::DRAM_END),
            ("s3 iram", esp32s3_map::IRAM_START, esp32s3_map::IRAM_END),
            ("s3 flash", esp32s3_map::FLASH_START, esp32s3_map::FLASH_END),
            (
                "classic dram",
                esp32_classic_map::DRAM_START,
                esp32_classic_map::DRAM_END,
            ),
            (
                "classic iram",
                esp32_classic_map::IRAM_START,
                esp32_classic_map::IRAM_END,
            ),
            (
                "classic flash",
                esp32_classic_map::FLASH_START,
                esp32_classic_map::FLASH_END,
            ),
        ] {
            assert!(start < end, "{name}: empty or inverted window");
        }
    }

    /// Running the S3 map on classic silicon rejects **everything** — which is
    /// the miscalibration that actually happens, and the reason
    /// `xt-map-esp32-classic` exists.
    ///
    /// ⚠️ The converse is NOT true and must not be asserted: the classic's IROM
    /// window is 11 MB (`0x400D_0000..0x40C0_0000`) and architecturally
    /// **contains** the S3's IRAM (`0x4037_0000..0x403E_0000`). A build that ran
    /// the classic map on an S3 would therefore accept some genuine S3 IRAM
    /// addresses and report a partially-plausible walk. There is no window
    /// arithmetic that removes that — the address spaces really do overlap — so
    /// the protection against it is the feature being opt-in and named for one
    /// chip, not a bound anyone should try to tighten here.
    #[test]
    fn the_s3_map_accepts_no_classic_text_address() {
        for (name, addr) in [
            ("classic iram start", esp32_classic_map::IRAM_START),
            ("classic iram end", esp32_classic_map::IRAM_END - 4),
            ("classic irom start", esp32_classic_map::FLASH_START),
            ("classic .text (~1.7 MB image)", 0x4020_0000),
        ] {
            assert!(
                !(esp32s3_map::IRAM_START..esp32s3_map::IRAM_END).contains(&addr)
                    && !(esp32s3_map::FLASH_START..esp32s3_map::FLASH_END).contains(&addr),
                "{name} ({addr:#010x}) would pass the S3 walker's bounds"
            );
        }
    }

    /// The classic's I-bus window must contain the JIT's code region.
    ///
    /// `lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT` installs compiled
    /// shader code into SRAM1's I-bus alias. A shader that faults is one of the
    /// likelier things to want a backtrace for on this chip, and it is the one
    /// address range that is *not* a linker-placed section — so if the JIT
    /// region ever moved out from under this window, the walk would drop exactly
    /// the frames that matter without saying anything.
    ///
    /// The region is carved from D-bus `0x3FFE_8000..0x4000_0000`, and the
    /// word-mirrored alias (`iram = 0x400B_FFFC − (dram − 0x3FFE_0000)`) maps
    /// that whole span into `0x400A_0000..0x400B_8000`. So rather than pin
    /// today's region bounds — which this crate cannot import and which moved
    /// once already (92 KiB → 32 KiB on 2026-08-02) — this asserts the window
    /// covers **every** address the region could occupy at any size. That
    /// holds for any future resize without an edit here.
    #[test]
    fn the_classic_iram_window_covers_the_jit_code_region() {
        // Images of the two ends of the D-bus span the region is carved from.
        const MIRROR_TOP: u32 = 0x400B_FFFC;
        const DRAM_BASE: u32 = 0x3FFE_0000;
        const CARVE_DBUS_START: u32 = 0x3FFE_8000;
        const CARVE_DBUS_END: u32 = 0x4000_0000;

        let widest_start = MIRROR_TOP - (CARVE_DBUS_END - 4 - DRAM_BASE);
        let widest_end = MIRROR_TOP - (CARVE_DBUS_START - DRAM_BASE) + 4;
        assert_eq!(widest_start, 0x400A_0000);
        assert_eq!(widest_end, 0x400B_8000);

        assert!(esp32_classic_map::IRAM_START <= widest_start);
        assert!(widest_end <= esp32_classic_map::IRAM_END);
    }
}
