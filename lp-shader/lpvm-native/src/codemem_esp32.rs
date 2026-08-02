//! Classic-ESP32 (LX6) JIT code memory: a fixed SRAM1 region written through
//! the word-**mirrored** D-bus view.
//!
//! Unlike the S3 (uniform `+0x6F_0000` alias — "the heap is executable", see
//! [`crate::exec_addr`]), the classic chip's heap (SRAM2/dram_seg) has **no
//! I-bus view at all** — executing a D-bus address faults with EXCCAUSE=2.
//! Dynamically written code must go to a fixed region of SRAM1, whose dual
//! mapping is word-mirrored (hardware-measured, all 5 sentinels; the linear
//! hypothesis matched none):
//!
//! ```text
//! iram = 0x400B_FFFC − (dram − 0x3FFE_0000)     (word granularity)
//! ```
//!
//! The two windows run in opposite directions: writing I-bus-contiguous code
//! means walking the D-bus **downward** word by word. Everything here is keyed
//! on the I-bus layout — byte `4*i` of an installed image is fetchable at
//! `span_base + 4*i` — and the write address is computed per word, which
//! absorbs the mirroring in one line of address math. Bytes within each
//! little-endian 32-bit word are verbatim — no byte swap.
//!
//! # Consequences for the JIT pipeline
//!
//! Code cannot execute from the staging `Vec` the linker fills (that Vec lives
//! in non-executable heap), so the classic pipeline is **link-at-base then
//! copy**: reserve a span in the region ([`CodeArena::alloc`]), patch
//! intra-module call targets against the span's I-bus base
//! ([`crate::link::link_jit_at`]), then install the staged bytes through the
//! mirrored walk ([`install`]). [`crate::rt_jit::JitBuffer`]'s `Placed`
//! variant then names the installed code by its I-bus address directly —
//! [`crate::exec_addr`]'s in-place rule is never consulted.
//!
//! The default region ([`CodeRegion::ESP32_DEFAULT`]) matches
//! `lp-xt-emu`'s `BoardProfile::esp32()` **by construction**, and the
//! emulator models the mirrored alias from the same silicon measurements, so
//! the whole install-then-execute path is testable on the host
//! (`tests/xt_classic_profile.rs`). A region change that breaks emulator
//! parity fails the pinned const-asserts below.
//!
//! The firmware crate owns the *reservation* of the region (keeping the
//! linker out of it); this module owns the address math and the write
//! discipline. Word-aligned volatile writes are the safe default on this chip
//! (mandatory on SRAM0's word-only bus; harmless on SRAM1).

use alloc::vec::Vec;

/// SRAM1 D-bus window base (the mirrored rule's D-bus origin).
pub const SRAM1_DRAM_BASE: u32 = 0x3FFE_0000;
/// I-bus address of the word at [`SRAM1_DRAM_BASE`] (the mirrored rule's top).
pub const SRAM1_IRAM_TOP: u32 = 0x400B_FFFC;
/// D-bus end (exclusive) of SRAM1's dual-mapped window.
pub const SRAM1_DRAM_END: u32 = 0x4000_0000;

/// Errors from code-region placement and installation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodeMemError {
    /// The requested span does not fit the region — the real capacity limit
    /// the S3's heap-backed path does not have. `largest_free` distinguishes
    /// "region genuinely full" from "full enough that fragmentation bites".
    TooLarge {
        requested: u32,
        largest_free: u32,
        capacity: u32,
    },
    /// Span not word-aligned or outside the region.
    BadSpan { ibus_base: u32, len: u32 },
}

impl core::fmt::Display for CodeMemError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CodeMemError::TooLarge {
                requested,
                largest_free,
                capacity,
            } => write!(
                f,
                "JIT code of {requested} B does not fit the classic code region \
                 (largest free span {largest_free} B of {capacity} B)"
            ),
            CodeMemError::BadSpan { ibus_base, len } => {
                write!(f, "bad code span {ibus_base:#x}+{len:#x}")
            }
        }
    }
}

/// A fixed SRAM1 code region, named by its D-bus placement.
///
/// The firmware crate picks the region (and must keep the linker from placing
/// sections in it); everything else derives from these two numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CodeRegion {
    /// D-bus base of the region (word-aligned, inside SRAM1's window).
    pub dbus_base: u32,
    /// Region length in bytes (word multiple).
    pub len_bytes: u32,
}

impl CodeRegion {
    /// The classic-ESP32 default: D-bus `0x3FFE_8000` + **32 KiB**, I-bus
    /// image `0x400B_0000..0x400B_8000`. Sits inside esp-hal's `dram2_seg`
    /// free span and clear of the ROM data/stack reservations lower in SRAM1
    /// (the last of them, `reserved_rom_stack_app`, ends exactly at
    /// `dram2_seg`'s origin `0x3FFE_7E30`); hardware-proven by the experiment
    /// repo's payload runner, and modeled exactly by `lp-xt-emu`'s
    /// `BoardProfile::esp32()`.
    ///
    /// # Why 32 KiB and not the original 92 KiB
    ///
    /// The 92 KiB was picked as "a comfortable span", never measured, and it
    /// was expensive: this region is the sole reason `dram2_seg` cannot join
    /// the heap, on a chip whose entire heap is 110 KB and whose measured
    /// binding constraint is a ~65 KB *compile working set*
    /// (`docs/adr/2026-08-01-esp32v3-flash-budget.md`).
    ///
    /// `tests/xt_classic_codemem_corpus.rs` measures the real cost by
    /// compiling every shader in `examples/` and `projects/` through the
    /// device's own pipeline:
    ///
    /// | figure | measured |
    /// |---|---|
    /// | largest single shader (`examples/basic`) | 6,108 B |
    /// | mean over 27 shaders | 3,129 B |
    /// | worst real project (`fyeah-button`, 2 shaders) | 9,800 B |
    /// | + one keep-last-good recompile copy | **15,908 B** |
    ///
    /// 15,908 B is the peak model, because `shader_node.rs` holds the old
    /// program while the new one compiles. 32 KiB is **2.06×** that, and
    /// **5.4×** the largest single real shader — so a user shader five times
    /// larger than anything in this repo still compiles. The remaining
    /// 64 KiB goes to the heap (`fw-esp32v3`'s `main.rs`).
    ///
    /// Nothing here is a hard bound: a shader is unbounded in principle, and
    /// [`CodeMemError::TooLarge`] is the real backstop — a project whose
    /// shader will not fit fails *that node* with a compile error while
    /// keep-last-good keeps the previous program rendering. Raising this
    /// number is a one-line change if the corpus ever demands it, and the
    /// corpus test is what will say so.
    pub const ESP32_DEFAULT: CodeRegion = CodeRegion {
        dbus_base: 0x3FFE_8000,
        len_bytes: 0x0000_8000,
    };

    /// D-bus end (exclusive) of `dram2_seg`, and so of the span the region is
    /// carved from: esp-hal's `dram2_seg` is `0x3FFE_7E30 + 98,768 B`, which
    /// ends exactly at SRAM1's top.
    ///
    /// Everything between [`CodeRegion::dbus_end`] and here is what the
    /// firmware may hand to the allocator as a second heap region — see
    /// [`CodeRegion::reclaimable_heap_span`].
    pub const ESP32_DRAM2_END: u32 = 0x4000_0000;

    /// The `(base, len)` of SRAM1 this region leaves over for the heap: from
    /// the region's D-bus end up to [`CodeRegion::ESP32_DRAM2_END`].
    ///
    /// This exists so the boundary the firmware must respect is *computed
    /// from* the region rather than restated beside it. The two were prose
    /// warnings in two files before (a ⚠️ in `main.rs` and one in the flash
    /// budget ADR), which is exactly the arrangement that lets a region
    /// change silently hand the allocator and the JIT the same bytes.
    #[must_use]
    pub const fn reclaimable_heap_span(&self) -> (u32, u32) {
        let base = self.dbus_base + self.len_bytes;
        (base, Self::ESP32_DRAM2_END - base)
    }

    /// D-bus end (exclusive) of the region.
    #[must_use]
    pub const fn dbus_end(&self) -> u32 {
        self.dbus_base + self.len_bytes
    }

    /// I-bus address of byte 0 of the region's executable image — the
    /// *lowest* I-bus address, which under the mirrored rule is the image of
    /// the D-bus **last** word.
    #[must_use]
    pub const fn ibus_base(&self) -> u32 {
        SRAM1_IRAM_TOP - ((self.dbus_base + self.len_bytes - 4) - SRAM1_DRAM_BASE)
    }

    /// I-bus end (exclusive) of the executable image.
    #[must_use]
    pub const fn ibus_end(&self) -> u32 {
        self.ibus_base() + self.len_bytes
    }

    /// The D-bus address through which the word at I-bus address `ibus` is
    /// written: the inverse of the mirrored rule. As `ibus` ascends, this
    /// walks the D-bus **downward**.
    #[must_use]
    pub const fn dbus_write_addr(&self, ibus: u32) -> u32 {
        SRAM1_DRAM_BASE + (SRAM1_IRAM_TOP - ibus)
    }

    /// Panics unless the region is word-aligned and inside SRAM1's
    /// dual-mapped window. Called by [`CodeArena::new`]; call directly when
    /// constructing a custom region.
    pub fn validate(&self) {
        assert!(
            self.dbus_base % 4 == 0 && self.len_bytes % 4 == 0 && self.len_bytes > 0,
            "code region {:#x}+{:#x} not word-aligned",
            self.dbus_base,
            self.len_bytes
        );
        assert!(
            self.dbus_base >= SRAM1_DRAM_BASE
                && (self.dbus_base as u64 + self.len_bytes as u64) <= SRAM1_DRAM_END as u64,
            "code region {:#x}+{:#x} outside SRAM1's dual-mapped window",
            self.dbus_base,
            self.len_bytes
        );
    }
}

// Pin the default so a change that breaks emulator parity is loud; the
// emulator's classic profile derives 0x400A_1000 from the same numbers
// (cross-checked against `lp-xt-emu` itself in `tests/xt_classic_profile.rs`).
const _: () = assert!(CodeRegion::ESP32_DEFAULT.ibus_base() == 0x400B_0000);
const _: () = assert!(CodeRegion::ESP32_DEFAULT.ibus_end() == 0x400B_8000);
// The mirror rule round-trips at both region ends.
const _: () = {
    let r = CodeRegion::ESP32_DEFAULT;
    assert!(r.dbus_write_addr(r.ibus_base()) == r.dbus_base + r.len_bytes - 4);
    assert!(r.dbus_write_addr(r.ibus_end() - 4) == r.dbus_base);
};
// The heap span and the code region partition `0x3FFE_8000..0x4000_0000`
// exactly: they abut, they do not overlap, and nothing between them is lost.
// This is the invariant the ⚠️ in `fw-esp32v3/src/main.rs` used to assert in
// prose — a shrink that forgets to move the heap boundary cannot compile.
const _: () = {
    let r = CodeRegion::ESP32_DEFAULT;
    let (heap_base, heap_len) = r.reclaimable_heap_span();
    assert!(heap_base == r.dbus_end());
    assert!(heap_base + heap_len == CodeRegion::ESP32_DRAM2_END);
    assert!(heap_len == 0x0001_0000); // 64 KiB returned to the allocator
};

/// Where installed words go. The device implementation writes through the
/// mirrored D-bus addresses; tests hand in an `lp-xt-emu` memory (or a plain
/// map) so the walk is verified against the silicon-measured alias model
/// without hardware.
pub trait CodeSink {
    fn write_word(&mut self, dbus_addr: u32, word: u32);
    /// Make installed code visible to the fetch path. Belt-and-braces on the
    /// classic chip (internal SRAM is uncached; measured), so default no-op.
    fn sync(&mut self) {}
}

/// The device sink: word-aligned volatile writes plus a full fence.
///
/// Only meaningful on the classic chip itself; compiled for any Xtensa target
/// so the crate builds identically across firmware crates, but constructing
/// it on the S3 would be a wiring bug (the S3 pipeline is heap-in-place).
///
/// No `isync` here, deliberately: internal SRAM is uncached on this chip and
/// the experiment repo measured (C2n) that freshly written SRAM1 code
/// executes with **no barriers at all**. The `fence` (LLVM lowers `SeqCst`
/// to `memw` on Xtensa) is kept as free insurance, but `isync` would need
/// inline asm, which is `asm_experimental_arch` on Xtensa — and this crate's
/// app path deliberately never enables that feature (see fw-esp32s3's
/// `board/esp32s3/mod.rs` for the same posture). If silicon ever proves an
/// `isync` necessary, the firmware crate supplies its own [`CodeSink`] with
/// the asm, feature-gated there, rather than this crate taking the feature.
#[cfg(target_arch = "xtensa")]
pub struct DeviceCodeSink;

#[cfg(target_arch = "xtensa")]
impl CodeSink for DeviceCodeSink {
    fn write_word(&mut self, dbus_addr: u32, word: u32) {
        // SAFETY: `install` only hands out word-aligned D-bus addresses
        // inside a validated region the firmware reserved for JIT code.
        unsafe { (dbus_addr as *mut u32).write_volatile(word) };
    }

    fn sync(&mut self) {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}

/// Install `code` at I-bus address `ibus_base` inside `region`, walking the
/// mirrored D-bus write addresses word by word. Trailing bytes of the final
/// word are zero-padded (they sit after the final instruction and are never
/// executed). Does NOT call [`CodeSink::sync`]; the caller syncs once after
/// all installs for a module.
pub fn install(
    region: &CodeRegion,
    ibus_base: u32,
    code: &[u8],
    sink: &mut impl CodeSink,
) -> Result<(), CodeMemError> {
    let len = u32::try_from(code.len()).map_err(|_| CodeMemError::BadSpan {
        ibus_base,
        len: u32::MAX,
    })?;
    let padded = len.div_ceil(4) * 4;
    let in_region = ibus_base % 4 == 0
        && ibus_base >= region.ibus_base()
        && (ibus_base as u64 + padded as u64) <= region.ibus_end() as u64;
    if !in_region {
        return Err(CodeMemError::BadSpan { ibus_base, len });
    }
    for i in 0..(padded / 4) {
        let start = (i * 4) as usize;
        let end = (start + 4).min(code.len());
        let mut w = [0u8; 4];
        w[..end - start].copy_from_slice(&code[start..end]);
        sink.write_word(
            region.dbus_write_addr(ibus_base + i * 4),
            u32::from_le_bytes(w),
        );
    }
    Ok(())
}

/// Span allocator over a [`CodeRegion`]: first-fit over a sorted free list,
/// word-granular, with adjacent-free coalescing. This is what makes
/// "region full" a *clean error* ([`CodeMemError::TooLarge`]) instead of
/// corruption — the real capacity edge the classic chip has and the S3's
/// heap-backed path does not.
///
/// The arena hands out I-bus base addresses; freeing is by the same
/// `(ibus_base, len)` pair. Module lifetime wiring (who frees on drop) is the
/// engine's concern, not the arena's.
pub struct CodeArena {
    region: CodeRegion,
    /// Sorted, non-adjacent free spans as `(ibus_base, len)`.
    free: Vec<(u32, u32)>,
    stats: ArenaStats,
}

/// Residency counters for the arena — the numbers that decide how big the
/// region actually has to be.
///
/// `peak_used` is the high-water mark of *concurrent* residency, which is the
/// only figure a region size may be derived from: total-ever-allocated says
/// nothing when spans are recycled, and instantaneous `used` misses the peak
/// between two heartbeats.
///
/// `allocs`/`frees` exist to answer the span-leak question directly rather
/// than by inference. A workload that returns to idle with
/// `allocs == frees` and `live_spans == 0` has leaked nothing; a Studio
/// editing loop whose `allocs` climbs while `frees` does not is leaking a
/// slice of the region per recompile, and that must be fixed *before* the
/// region is shrunk — otherwise the shrink only converts a slow leak into a
/// fast one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ArenaStats {
    /// Bytes currently reserved (word-rounded, so this is what the region
    /// actually has to hold — not the callers' unrounded byte lengths).
    pub used: u32,
    /// High-water mark of [`ArenaStats::used`] since boot.
    pub peak_used: u32,
    /// Spans currently reserved.
    pub live_spans: u32,
    /// High-water mark of [`ArenaStats::live_spans`] since boot.
    pub peak_spans: u32,
    /// Successful [`CodeArena::alloc`] calls since boot.
    pub allocs: u32,
    /// [`CodeArena::free`] calls since boot.
    pub frees: u32,
    /// [`CodeArena::alloc`] calls that failed with
    /// [`CodeMemError::TooLarge`] — a nonzero value here means the region is
    /// too small for the workload that ran, and is the signal that a shrink
    /// went too far.
    pub alloc_failures: u32,
}

impl CodeArena {
    #[must_use]
    pub fn new(region: CodeRegion) -> Self {
        region.validate();
        Self {
            free: alloc::vec![(region.ibus_base(), region.len_bytes)],
            region,
            stats: ArenaStats::default(),
        }
    }

    /// Residency counters — see [`ArenaStats`].
    #[must_use]
    pub fn stats(&self) -> ArenaStats {
        self.stats
    }

    #[must_use]
    pub fn region(&self) -> &CodeRegion {
        &self.region
    }

    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.region.len_bytes
    }

    /// Total free bytes (may be fragmented).
    #[must_use]
    pub fn available(&self) -> u32 {
        self.free.iter().map(|(_, l)| l).sum()
    }

    /// Largest single allocatable span.
    #[must_use]
    pub fn largest_free(&self) -> u32 {
        self.free.iter().map(|(_, l)| *l).max().unwrap_or(0)
    }

    /// Reserve a word-aligned span of at least `len` bytes; returns its I-bus
    /// base. First-fit keeps the common single-module case at the region
    /// base, matching the experiment runner's layout.
    pub fn alloc(&mut self, len: u32) -> Result<u32, CodeMemError> {
        let need = len.max(4).div_ceil(4) * 4;
        for i in 0..self.free.len() {
            let (base, flen) = self.free[i];
            if flen >= need {
                if flen == need {
                    self.free.remove(i);
                } else {
                    self.free[i] = (base + need, flen - need);
                }
                self.stats.used += need;
                self.stats.peak_used = self.stats.peak_used.max(self.stats.used);
                self.stats.live_spans += 1;
                self.stats.peak_spans = self.stats.peak_spans.max(self.stats.live_spans);
                self.stats.allocs += 1;
                return Ok(base);
            }
        }
        self.stats.alloc_failures += 1;
        Err(CodeMemError::TooLarge {
            requested: len,
            largest_free: self.largest_free(),
            capacity: self.capacity(),
        })
    }

    /// Return a span. `len` is the length passed to [`CodeArena::alloc`]
    /// (re-rounded here, so callers may pass the original byte length).
    pub fn free(&mut self, ibus_base: u32, len: u32) {
        let need = len.max(4).div_ceil(4) * 4;
        debug_assert!(
            ibus_base >= self.region.ibus_base()
                && (ibus_base + need) <= self.region.ibus_end()
                && ibus_base % 4 == 0,
            "freeing span {ibus_base:#x}+{need:#x} outside the arena"
        );
        self.stats.used = self.stats.used.saturating_sub(need);
        self.stats.live_spans = self.stats.live_spans.saturating_sub(1);
        self.stats.frees += 1;
        let idx = self
            .free
            .iter()
            .position(|&(b, _)| b > ibus_base)
            .unwrap_or(self.free.len());
        self.free.insert(idx, (ibus_base, need));
        // Coalesce with the next, then the previous, neighbor.
        if idx + 1 < self.free.len() {
            let (b, l) = self.free[idx];
            let (nb, nl) = self.free[idx + 1];
            debug_assert!(b + l <= nb, "double free / overlap at {b:#x}");
            if b + l == nb {
                self.free[idx] = (b, l + nl);
                self.free.remove(idx + 1);
            }
        }
        if idx > 0 {
            let (pb, pl) = self.free[idx - 1];
            let (b, l) = self.free[idx];
            debug_assert!(pb + pl <= b, "double free / overlap at {b:#x}");
            if pb + pl == b {
                self.free[idx - 1] = (pb, pl + l);
                self.free.remove(idx);
            }
        }
    }
}

/// The firmware-installed global arena behind the `xt-placed-code` feature.
///
/// The JIT engine is constructed deep inside the graphics backend with no
/// classic-specific parameters, so the fw crate installs the arena once at
/// boot and the engine reaches it here. Single-threaded firmware is the
/// operating assumption; the busy flag turns an accidental concurrent
/// compile into a clean error instead of an aliased `&mut`.
#[cfg(feature = "xt-placed-code")]
pub mod global {
    use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

    use super::{CodeArena, CodeRegion};

    static ARENA: AtomicPtr<CodeArena> = AtomicPtr::new(core::ptr::null_mut());
    static BUSY: AtomicBool = AtomicBool::new(false);

    /// Install the classic code arena. Call once at boot, before the first
    /// shader compile; the arena is leaked (it lives for the firmware's
    /// lifetime by design).
    pub fn install(region: CodeRegion) {
        let arena = alloc::boxed::Box::leak(alloc::boxed::Box::new(CodeArena::new(region)));
        let prev = ARENA.swap(arena, Ordering::SeqCst);
        assert!(prev.is_null(), "classic JIT code arena installed twice");
    }

    /// True once [`install`] has run.
    #[must_use]
    pub fn installed() -> bool {
        !ARENA.load(Ordering::SeqCst).is_null()
    }

    /// Residency counters for the installed arena, plus its `largest_free`.
    ///
    /// `None` when no arena is installed or one is mid-compile — a reporting
    /// path (the heartbeat) must never block or alias the compiler's `&mut`,
    /// and a skipped sample is worth nothing lost when the next one is 5 s
    /// away. Note that `used`/`peak_used` are word-rounded reservation
    /// totals, so they are the figures a region size may be derived from.
    #[must_use]
    pub fn stats() -> Option<(super::ArenaStats, u32)> {
        with(|arena| (arena.stats(), arena.largest_free())).ok()
    }

    /// Run `f` with exclusive access to the arena.
    ///
    /// `Err(NotInstalled)` when boot never installed one; `Err(Busy)` if a
    /// second use overlaps the first (a reentrancy bug upstream, reported
    /// rather than aliased).
    pub(crate) fn with<R>(f: impl FnOnce(&mut CodeArena) -> R) -> Result<R, GlobalArenaError> {
        let ptr = ARENA.load(Ordering::SeqCst);
        if ptr.is_null() {
            return Err(GlobalArenaError::NotInstalled);
        }
        if BUSY.swap(true, Ordering::SeqCst) {
            return Err(GlobalArenaError::Busy);
        }
        // SAFETY: BUSY guarantees exclusivity; the pointer is never freed
        // after install.
        let result = f(unsafe { &mut *ptr });
        BUSY.store(false, Ordering::SeqCst);
        Ok(result)
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum GlobalArenaError {
        NotInstalled,
        Busy,
    }

    impl core::fmt::Display for GlobalArenaError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                GlobalArenaError::NotInstalled => f.write_str(
                    "classic JIT code arena not installed — firmware boot must call \
                     codemem_esp32::global::install before the first shader compile",
                ),
                GlobalArenaError::Busy => {
                    f.write_str("classic JIT code arena is busy (reentrant compile?)")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five hardware sentinels that decided mirrored-vs-linear, replayed
    /// against the pure math. (dram, iram) pairs from the measured rule.
    #[test]
    fn mirror_formula_matches_the_hardware_sentinels() {
        // iram = 0x400B_FFFC − (dram − 0x3FFE_0000), exercised via the
        // region's inverse map: dbus_write_addr(iram) == dram.
        let r = CodeRegion {
            dbus_base: SRAM1_DRAM_BASE,
            len_bytes: 0x2_0000,
        };
        for (dram, iram) in [
            (0x3FFE_0000u32, 0x400B_FFFCu32),
            (0x3FFE_0004, 0x400B_FFF8),
            (0x3FFE_8000, 0x400B_7FFC),
            (0x3FFF_EFFC, 0x400A_1000),
            (0x3FFF_FFFC, 0x400A_0000),
        ] {
            assert_eq!(r.dbus_write_addr(iram), dram, "iram {iram:#x}");
        }
    }

    #[test]
    fn default_region_is_the_runner_region() {
        let r = CodeRegion::ESP32_DEFAULT;
        r.validate();
        assert_eq!(r.ibus_base(), 0x400A_1000);
        assert_eq!(r.ibus_end(), 0x400B_8000);
        // Word 0 writes through the D-bus LAST word; the last word writes
        // through the D-bus base — the descending walk.
        assert_eq!(r.dbus_write_addr(r.ibus_base()), 0x3FFF_EFFC);
        assert_eq!(r.dbus_write_addr(r.ibus_end() - 4), 0x3FFE_8000);
    }

    struct MapSink(alloc::collections::BTreeMap<u32, u32>);
    impl CodeSink for MapSink {
        fn write_word(&mut self, dbus_addr: u32, word: u32) {
            assert_eq!(dbus_addr % 4, 0);
            self.0.insert(dbus_addr, word);
        }
    }

    #[test]
    fn install_walks_downward_and_pads_the_tail() {
        let r = CodeRegion::ESP32_DEFAULT;
        let mut sink = MapSink(Default::default());
        // 6 bytes → 2 words, second word tail-padded with zeros.
        install(&r, r.ibus_base(), &[1, 2, 3, 4, 5, 6], &mut sink).unwrap();
        assert_eq!(
            sink.0.get(&r.dbus_write_addr(r.ibus_base())),
            Some(&u32::from_le_bytes([1, 2, 3, 4]))
        );
        assert_eq!(
            sink.0.get(&r.dbus_write_addr(r.ibus_base() + 4)),
            Some(&u32::from_le_bytes([5, 6, 0, 0]))
        );
        // Downward: word 1's write address is 4 BELOW word 0's.
        assert_eq!(
            r.dbus_write_addr(r.ibus_base() + 4),
            r.dbus_write_addr(r.ibus_base()) - 4
        );
    }

    #[test]
    fn install_rejects_out_of_region_spans() {
        let r = CodeRegion::ESP32_DEFAULT;
        let mut sink = MapSink(Default::default());
        // Would overhang the region end by one word.
        let near_end = r.ibus_end() - 4;
        assert!(matches!(
            install(&r, near_end, &[0; 8], &mut sink),
            Err(CodeMemError::BadSpan { .. })
        ));
        // Unaligned base.
        assert!(matches!(
            install(&r, r.ibus_base() + 2, &[0; 4], &mut sink),
            Err(CodeMemError::BadSpan { .. })
        ));
        assert!(sink.0.is_empty(), "nothing written on rejection");
    }

    #[test]
    fn arena_alloc_free_coalesces() {
        let mut a = CodeArena::new(CodeRegion::ESP32_DEFAULT);
        let base = a.region().ibus_base();
        let s1 = a.alloc(101).unwrap(); // word-rounds to 104
        let s2 = a.alloc(8).unwrap();
        let s3 = a.alloc(16).unwrap();
        assert_eq!(s1, base);
        assert_eq!(s2, base + 104);
        assert_eq!(s3, base + 112);
        // Free the middle, then the first: they coalesce into one 112-byte
        // hole at the base, while s3 still splits it from the tail.
        a.free(s2, 8);
        a.free(s1, 101);
        assert_eq!(a.available(), a.capacity() - 16); // only s3 remains
        assert_eq!(a.largest_free(), a.capacity() - 128); // the tail
        // A span that fits the coalesced hole exactly lands at the base again.
        let s4 = a.alloc(112).unwrap();
        assert_eq!(s4, base);
        // Free everything: back to one full-capacity span.
        a.free(s4, 112);
        a.free(s3, 16);
        assert_eq!(a.available(), a.capacity());
        assert_eq!(a.largest_free(), a.capacity());
    }

    /// The counters the region-sizing decision rests on: `peak_used` survives
    /// the frees that follow it (an instantaneous `used` would not), and a
    /// balanced workload returns to zero live spans — which is exactly the
    /// shape "no leak" has on the device.
    #[test]
    fn stats_track_peak_and_return_to_zero_when_balanced() {
        let mut a = CodeArena::new(CodeRegion::ESP32_DEFAULT);
        // Two concurrent modules, then a third after the first is freed:
        // peak is the 2-module moment, not the 3-alloc total.
        let s1 = a.alloc(2_032).unwrap(); // word-rounds to 2_032
        let s2 = a.alloc(2_030).unwrap(); // word-rounds to 2_032
        assert_eq!(a.stats().used, 4_064);
        assert_eq!(a.stats().live_spans, 2);
        a.free(s1, 2_032);
        let s3 = a.alloc(1_000).unwrap();
        assert_eq!(a.stats().used, 3_032);
        // Peak is remembered across the free.
        assert_eq!(a.stats().peak_used, 4_064);
        assert_eq!(a.stats().peak_spans, 2);

        a.free(s2, 2_030);
        a.free(s3, 1_000);
        let st = a.stats();
        assert_eq!(st.used, 0, "balanced workload leaks nothing");
        assert_eq!(st.live_spans, 0);
        assert_eq!(st.allocs, 3);
        assert_eq!(st.frees, 3);
        assert_eq!(st.alloc_failures, 0);
        assert_eq!(st.peak_used, 4_064, "peak survives the drain");
        // And the arena really is whole again.
        assert_eq!(a.available(), a.capacity());
    }

    /// A failed `alloc` is counted and changes nothing else — the shrink's
    /// safety net has to be visible in the telemetry, not only in the error.
    #[test]
    fn stats_count_alloc_failures_without_disturbing_residency() {
        let mut a = CodeArena::new(CodeRegion::ESP32_DEFAULT);
        let s = a.alloc(64).unwrap();
        assert!(a.alloc(a.capacity()).is_err());
        let st = a.stats();
        assert_eq!(st.alloc_failures, 1);
        assert_eq!(st.allocs, 1, "a failure is not an alloc");
        assert_eq!(st.used, 64);
        assert_eq!(st.live_spans, 1);
        a.free(s, 64);
        assert_eq!(a.stats().used, 0);
    }

    #[test]
    fn arena_full_is_a_clean_toolarge() {
        let mut a = CodeArena::new(CodeRegion::ESP32_DEFAULT);
        let cap = a.capacity();
        let _ = a.alloc(cap - 8).unwrap();
        let err = a.alloc(64).unwrap_err();
        match err {
            CodeMemError::TooLarge {
                requested,
                largest_free,
                capacity,
            } => {
                assert_eq!(requested, 64);
                assert_eq!(largest_free, 8);
                assert_eq!(capacity, cap);
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }
}
