//! Classic-ESP32 (LX6) JIT code memory: a fixed region of internal SRAM that
//! the heap cannot use, written word by word at a placement-specific address.
//!
//! Unlike the S3 (uniform `+0x6F_0000` alias — "the heap is executable", see
//! [`crate::exec_addr`]), the classic chip's heap (SRAM2/dram_seg) has **no
//! I-bus view at all** — executing a D-bus address faults with EXCCAUSE=2.
//! Dynamically written code must go to a fixed region, and the chip offers
//! two ([`Placement`]):
//!
//! - **SRAM0** (`0x4008_0000..0x400A_0000`, the instruction RAM that holds
//!   `.vectors` and `.rwtext`) — the default since 2026-09-05. It has **no
//!   D-bus view**: the write address *is* the I-bus address, aligned 32-bit
//!   stores only, byte access faults (`LoadStoreError`, EXCCAUSE 3). Measured
//!   on the dig2go by `fw-esp32v3`'s `test_sram0_exec` rig: word-written code
//!   at `0x4008_8000` and at the region top executes on the PRO core, and
//!   3 × 1,000 rewrite-then-call iterations found **no barrier necessary**
//!   (none / `memw` / `memw`+`isync` all 0 stale). Nothing else can use this
//!   memory — a heap needs byte access — so the JIT costs the heap nothing
//!   here, and the whole SRAM1 tail goes to the allocator instead
//!   ([`CodeRegion::reclaimable_heap_span`]).
//! - **SRAM1**, written through its word-**mirrored** D-bus view
//!   (hardware-measured, all 5 sentinels; the linear hypothesis matched none):
//!
//!   ```text
//!   iram = 0x400B_FFFC − (dram − 0x3FFE_0000)     (word granularity)
//!   ```
//!
//!   The two windows run in opposite directions, so writing I-bus-contiguous
//!   code means walking the D-bus **downward** word by word. This was the
//!   placement from the 2026-07-28 bring-up until 2026-09-05, and it is kept
//!   as [`CodeRegion::ESP32_SRAM1_LEGACY`] so the mirrored path stays tested
//!   — it spends byte-addressable heap-grade memory, which is the reason it
//!   is no longer the default (`docs/reports/2026-09-04-classic-ram-budget.md`,
//!   lever 1).
//!
//! Everything here is keyed on the I-bus layout — byte `4*i` of an installed
//! image is fetchable at `span_base + 4*i` — and the write address is computed
//! per word by [`CodeRegion::write_addr`], which absorbs the placement in one
//! line of address math. Bytes within each little-endian 32-bit word are
//! verbatim — no byte swap.
//!
//! # Consequences for the JIT pipeline
//!
//! Code cannot execute from the staging `Vec` the linker fills (that Vec lives
//! in non-executable heap), so the classic pipeline is **link-at-base then
//! copy**: reserve a span in the region ([`CodeArena::alloc`]), patch
//! intra-module call targets against the span's I-bus base
//! ([`crate::link::link_jit_at`]), then install the staged bytes through the
//! word walk ([`install`]). [`crate::rt_jit::JitBuffer`]'s `Placed` variant
//! then names the installed code by its I-bus address directly —
//! [`crate::exec_addr`]'s in-place rule is never consulted.
//!
//! The default region ([`CodeRegion::ESP32_DEFAULT`]) matches
//! `lp-xt-emu`'s `BoardProfile::esp32()` **by construction**, and the
//! emulator models SRAM0's identity alias and word-only access from the same
//! silicon measurements, so the whole install-then-execute path is testable
//! on the host (`tests/xt_classic_profile.rs`). A region change that breaks
//! emulator parity fails the pinned const-asserts below.
//!
//! The firmware crate owns the *reservation* of the region (keeping the
//! linker out of it — for SRAM0 that is a boot-time assert that `.rwtext`
//! ends below the region); this module owns the address math and the write
//! discipline. Word-aligned volatile writes are the only access SRAM0 takes
//! and are harmless on SRAM1.

use alloc::vec::Vec;

/// SRAM1 D-bus window base (the mirrored rule's D-bus origin).
pub const SRAM1_DRAM_BASE: u32 = 0x3FFE_0000;
/// I-bus address of the word at [`SRAM1_DRAM_BASE`] (the mirrored rule's top).
pub const SRAM1_IRAM_TOP: u32 = 0x400B_FFFC;
/// D-bus end (exclusive) of SRAM1's dual-mapped window.
pub const SRAM1_DRAM_END: u32 = 0x4000_0000;

/// First I-bus address of SRAM0 a JIT region may use: esp-hal's `iram_seg`
/// origin (`ld/esp32/memory.x`), i.e. the byte after the 1 KiB `.vectors`.
/// `.rwtext` starts here and grows upward; the firmware asserts at boot that
/// it ends below [`CodeRegion::ESP32_DEFAULT`]'s base.
pub const SRAM0_IRAM_BASE: u32 = 0x4008_0400;
/// End (exclusive) of SRAM0's instruction bus.
pub const SRAM0_IRAM_END: u32 = 0x400A_0000;

/// `(base, len)` of the SRAM1 span esp-hal reserves for the ROM's **PRO-CPU**
/// boot stack, plus the unreserved hole beside it: `0x3FFE_0440..0x3FFE_3F20`.
///
/// esp-hal reserves 32,304 B of SRAM1 for the ROM in four blocks — two *data*
/// blocks and two *stacks* — and never hands any of them back. esp-idf gives
/// the stacks to its heap once the ROM is out of them, and so can the firmware:
/// xtensa-lx-rt's reset sets `a1 = _stack_start`, so the PRO stack is dead from
/// the first Rust instruction. The two ROM **data** blocks
/// (`0x3FFE_0000 + 1088` and `0x3FFE_3F20 + 1072`) are NOT in this span and
/// must stay reserved: ROM functions the image still calls read them.
///
/// Named here rather than in the firmware because this module is where SRAM1's
/// ownership is settled; the const-assert below pins the span against the
/// bounds the rest of the file already knows.
pub const SRAM1_ROM_PRO_STACK_SPAN: (u32, u32) = (0x3FFE_0440, 0x3FFE_3F20 - 0x3FFE_0440);

/// Base of the same reclaim for the **APP-CPU**: `0x3FFE_4350`, the end of the
/// second ROM data block.
///
/// Only the base is a constant. The span's *end* is the lowest address anything
/// else claims in SRAM1 — the JIT-or-heap-span boundary
/// ([`CodeRegion::reclaimable_heap_span`]'s base) — so the span also swallows the 464 B head of
/// `dram2_seg` that sits below the region. Deriving the end rather than writing
/// it down is what lets the JIT region move without a second edit here: when it
/// leaves SRAM1, this span and the reclaimable heap tail become contiguous.
///
/// ⚠️ Unlike the PRO stack, this one is live during boot: the APP core runs on
/// it while it comes up through the ROM, until esp-hal's `start_core1_init`
/// switches it to `APP_CORE_STACK`. The firmware must therefore register this
/// span only after the core-start call has returned.
pub const SRAM1_ROM_APP_STACK_BASE: u32 = 0x3FFE_4350;

// The two reclaimed spans must sit inside SRAM1, must not overlap the ROM data
// blocks they are named against, and — for the APP span — must end below
// whatever claims SRAM1 next. Asserted rather than trusted: these addresses
// reach an `unsafe esp_alloc::HeapRegion::new`, where being wrong means handing
// the allocator memory the ROM still uses, and the fault would land nowhere
// near this file.
const _: () = {
    let (pro_base, pro_len) = SRAM1_ROM_PRO_STACK_SPAN;
    assert!(pro_base >= SRAM1_DRAM_BASE && pro_len > 0);
    // Ends exactly where the second ROM data block begins.
    assert!(pro_base + pro_len == 0x3FFE_3F20);
    // ... which is where the APP span's base is one 1,072 B data block later.
    assert!(SRAM1_ROM_APP_STACK_BASE == 0x3FFE_3F20 + 1072);
    assert!(SRAM1_ROM_APP_STACK_BASE < CodeRegion::ESP32_DEFAULT.reclaimable_heap_span().0);
    // The two spans are separated by that data block, so they can never be
    // mistaken for one run by a free-list walk that recovers regions by address
    // contiguity (`fw-esp32v3`'s `free_list_shape` does exactly that).
    assert!(pro_base + pro_len < SRAM1_ROM_APP_STACK_BASE);
};

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

/// Where a [`CodeRegion`] lives, and therefore how its words are written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    /// SRAM1, written through the word-mirrored D-bus view. `dbus_base` is
    /// the region's D-bus base (word-aligned, inside SRAM1's window); the
    /// I-bus image is derived through the mirror rule, and the write walk
    /// runs downward.
    Sram1Mirrored { dbus_base: u32 },
    /// SRAM0 IRAM: no D-bus view; written with aligned word stores at the
    /// I-bus address itself. `ibus_base` is the region's I-bus base
    /// (word-aligned, inside [`SRAM0_IRAM_BASE`]`..`[`SRAM0_IRAM_END`]).
    Sram0 { ibus_base: u32 },
}

/// A fixed code region, named by its placement and length.
///
/// The firmware crate picks the region (and must keep the linker from placing
/// sections in it); everything else derives from these two values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CodeRegion {
    /// Which SRAM the region is in, and its base there.
    pub placement: Placement,
    /// Region length in bytes (word multiple).
    pub len_bytes: u32,
}

impl CodeRegion {
    /// The classic-ESP32 default: **SRAM0**, I-bus `0x4008_8000..0x4009_8000`,
    /// **64 KiB**, written at the I-bus address itself.
    ///
    /// # Why SRAM0 (since 2026-09-05)
    ///
    /// SRAM0's 128 KiB of IRAM held `.vectors` (1 KiB) and `.rwtext` (~15 KiB
    /// on the app image) and nothing else: ~112 KiB idle, and idle for good
    /// reason — it has no D-bus view and takes aligned word accesses only, so
    /// no heap can live there. A JIT code installer is the one consumer that
    /// wants exactly that. Moving the region here returns the 24 KiB it used
    /// to occupy in SRAM1 to the heap, where
    /// `docs/reports/2026-09-04-classic-ram-budget.md` measured every byte
    /// mattering (lever 1). Facts measured on the dig2go by `fw-esp32v3`'s
    /// `test_sram0_exec`: word stores across the whole `0x4008_8000..
    /// 0x400A_0000` land and read back; a byte access faults (EXCCAUSE 3);
    /// word-written code at both ends of this region executes on the PRO
    /// core; no barrier is needed.
    ///
    /// # Why this base and size
    ///
    /// 32 KiB into IRAM: `.rwtext` ends at `0x4008_142C` on the harness image
    /// and `0x4008_3DF0` on the app image, so the linker has 16,400 B of
    /// headroom below this base before the firmware's boot-time assert
    /// (`fw-esp32v3/src/main.rs`) refuses to install the arena. 64 KiB is
    /// 2.7× the SRAM1 region it replaces and leaves 32 KiB spare above for
    /// growth; it is generous because SRAM0 costs the heap nothing, not
    /// because the corpus asks for it (see below).
    ///
    /// # What the corpus says (measured, unchanged by the move)
    ///
    /// `tests/xt_classic_codemem_corpus.rs` compiles every shader in
    /// `examples/` and `projects/` through the device's own pipeline, at the
    /// device's own settings (Q32, fuel on):
    ///
    /// | figure | measured |
    /// |---|---|
    /// | largest single shader (`examples/basic`) | 6,516 B |
    /// | mean over 27 shaders | 3,348 B |
    /// | worst real project (`fyeah-button`, 2 shaders) | 10,260 B |
    /// | + one keep-last-good recompile copy | **16,776 B** |
    ///
    /// Those are device figures, not a host estimate of them: the classic
    /// reported 2,444 B for `examples/shader-oracle`, M3 measured 2,032 B for
    /// `quad-strips-v3`, and the 2026-08-04 dome walk measured 2,116 B for
    /// `zook-dome-1500` — the corpus test reproduces all of them exactly.
    /// 16,776 B is the peak model, because `shader_node.rs` holds the old
    /// program while the new one compiles ("Old + new coexist for the compile
    /// duration"); the corpus test's guard asserts the region holds that plus
    /// one more largest-in-repo shader.
    ///
    /// Nothing here is a hard bound: a shader is unbounded in principle, and
    /// [`CodeMemError::TooLarge`] is the real backstop — a project whose
    /// shader will not fit fails *that node* with a compile error while
    /// keep-last-good keeps the previous program rendering. `[JIT]` telemetry
    /// (`peak`, `peak_spans`, `fails`) is the field tripwire.
    pub const ESP32_DEFAULT: CodeRegion = CodeRegion {
        placement: Placement::Sram0 {
            ibus_base: 0x4008_8000,
        },
        len_bytes: 0x0001_0000,
    };

    /// The SRAM1 placement the classic used from 2026-07-28 to 2026-09-05:
    /// D-bus `0x3FFE_8000` + **24 KiB**, I-bus image `0x400B_2000..0x400B_8000`
    /// (92 KiB until 2026-08-02, 32 KiB until 2026-08-04, both measured down
    /// by the corpus test). Sits inside esp-hal's `dram2_seg` and clear of the
    /// ROM data/stack reservations lower in SRAM1 (the last of them,
    /// `reserved_rom_stack_app`, ends exactly at `dram2_seg`'s origin
    /// `0x3FFE_7E30`); hardware-proven by the experiment repo's payload
    /// runner and the 2026-08 hardware walks.
    ///
    /// Not the default any more — see [`CodeRegion::ESP32_DEFAULT`] — but
    /// kept so the mirrored write walk stays exercised by `mod tests` and
    /// `tests/xt_classic_profile.rs` against `lp-xt-emu`'s
    /// `BoardProfile::esp32_sram1_legacy()`.
    pub const ESP32_SRAM1_LEGACY: CodeRegion = CodeRegion {
        placement: Placement::Sram1Mirrored {
            dbus_base: 0x3FFE_8000,
        },
        len_bytes: 0x0000_6000,
    };

    /// D-bus end (exclusive) of `dram2_seg`, and so of the span an SRAM1
    /// region is carved from: esp-hal's `dram2_seg` is `0x3FFE_7E30 +
    /// 98,768 B`, which ends exactly at SRAM1's top.
    ///
    /// Everything between a region's D-bus end and here is what the firmware
    /// may hand to the allocator as a second heap region — see
    /// [`CodeRegion::reclaimable_heap_span`].
    pub const ESP32_DRAM2_END: u32 = 0x4000_0000;

    /// D-bus origin of esp-hal's `dram2_seg`. Below this address SRAM1 holds
    /// the ROM's data and per-core stacks (`reserved_rom_data_pro/app`,
    /// `reserved_rom_stack_pro/app`), the last of which ends exactly here.
    pub const ESP32_DRAM2_BASE: u32 = 0x3FFE_7E30;

    /// D-bus base of the SRAM1 tail the JIT no longer uses under the SRAM0
    /// placement: where [`CodeRegion::ESP32_SRAM1_LEGACY`] started. The
    /// 464 B between [`CodeRegion::ESP32_DRAM2_BASE`] and here are left to
    /// the ROM-stack reclaim (track A of the 2026-09-05 RAM work), whose
    /// chunk ends at [`CodeRegion::reclaimable_heap_span`]`().0` and so
    /// fuses with this tail into one region.
    pub const ESP32_SRAM1_TAIL_BASE: u32 = 0x3FFE_8000;

    /// The `(base, len)` of SRAM1 this region leaves over for the heap —
    /// "the SRAM1 bytes the JIT does not use".
    ///
    /// For an SRAM0 region that is the whole tail
    /// [`CodeRegion::ESP32_SRAM1_TAIL_BASE`]`..`[`CodeRegion::ESP32_DRAM2_END`]
    /// (98,304 B). For an SRAM1 region it is from the region's D-bus end up
    /// to [`CodeRegion::ESP32_DRAM2_END`].
    ///
    /// This exists so the boundary the firmware must respect is *computed
    /// from* the region rather than restated beside it. The two were prose
    /// warnings in two files before (a ⚠️ in `main.rs` and one in the flash
    /// budget ADR), which is exactly the arrangement that lets a region
    /// change silently hand the allocator and the JIT the same bytes.
    ///
    /// # Panics
    /// If an SRAM1 region does not start inside `dram2_seg`. The result feeds
    /// an `unsafe esp_alloc::HeapRegion::new`, so answering for a region
    /// placed lower in SRAM1 would hand the allocator the ROM's stacks and
    /// data — memory the ROM still uses after boot, whose corruption would
    /// present as an unattributable fault far from here. Refusing is the only
    /// safe answer, and for the two named regions it is checked at compile
    /// time by the const-asserts below.
    #[must_use]
    pub const fn reclaimable_heap_span(&self) -> (u32, u32) {
        match self.placement {
            Placement::Sram0 { .. } => (
                Self::ESP32_SRAM1_TAIL_BASE,
                Self::ESP32_DRAM2_END - Self::ESP32_SRAM1_TAIL_BASE,
            ),
            Placement::Sram1Mirrored { dbus_base } => {
                assert!(
                    dbus_base >= Self::ESP32_DRAM2_BASE,
                    "code region starts below dram2_seg — its leftover span would \
                     include the ROM's data/stack reservations, which must never \
                     reach the allocator"
                );
                let base = dbus_base + self.len_bytes;
                assert!(
                    base <= Self::ESP32_DRAM2_END,
                    "code region ends above SRAM1"
                );
                (base, Self::ESP32_DRAM2_END - base)
            }
        }
    }

    /// The lowest SRAM1 D-bus address the JIT or its reclaimable heap span
    /// claims — the ceiling for anything else that wants to carve SRAM1
    /// below the tail (the ROM-stack reclaim of the 2026-09-05 RAM work
    /// registers its ROM-APP-stack chunk as `0x3FFE_4350..` this).
    ///
    /// Under SRAM0 placement the JIT claims nothing in SRAM1, so this is the
    /// heap span's base ([`CodeRegion::ESP32_SRAM1_TAIL_BASE`]); under the
    /// mirrored placement it is the region's own D-bus base. Distinct from
    /// [`CodeRegion::reclaimable_heap_span`]`().0`, which is the heap span's
    /// base — the two coincide only for SRAM0.
    #[must_use]
    pub const fn sram1_claim_base(&self) -> u32 {
        match self.placement {
            Placement::Sram0 { .. } => self.reclaimable_heap_span().0,
            Placement::Sram1Mirrored { dbus_base } => dbus_base,
        }
    }

    /// I-bus address of byte 0 of the region's executable image — the
    /// *lowest* I-bus address. Under the mirrored rule that is the image of
    /// the D-bus **last** word; under SRAM0 it is the base itself.
    #[must_use]
    pub const fn ibus_base(&self) -> u32 {
        match self.placement {
            Placement::Sram0 { ibus_base } => ibus_base,
            Placement::Sram1Mirrored { dbus_base } => {
                SRAM1_IRAM_TOP - ((dbus_base + self.len_bytes - 4) - SRAM1_DRAM_BASE)
            }
        }
    }

    /// I-bus end (exclusive) of the executable image.
    #[must_use]
    pub const fn ibus_end(&self) -> u32 {
        self.ibus_base() + self.len_bytes
    }

    /// The address through which the word at I-bus address `ibus` is
    /// written: the I-bus address itself for SRAM0 (no D-bus view exists),
    /// the inverse of the mirrored rule for SRAM1 — where, as `ibus`
    /// ascends, this walks the D-bus **downward**.
    #[must_use]
    pub const fn write_addr(&self, ibus: u32) -> u32 {
        match self.placement {
            Placement::Sram0 { .. } => ibus,
            Placement::Sram1Mirrored { .. } => SRAM1_DRAM_BASE + (SRAM1_IRAM_TOP - ibus),
        }
    }

    /// Panics unless the region is word-aligned and inside its placement's
    /// window: SRAM1's dual-mapped window for [`Placement::Sram1Mirrored`],
    /// [`SRAM0_IRAM_BASE`]`..`[`SRAM0_IRAM_END`] for [`Placement::Sram0`].
    /// Called by [`CodeArena::new`]; call directly when constructing a custom
    /// region.
    ///
    /// What this cannot check is the linker: an SRAM0 region must also sit
    /// above `.rwtext`'s end, which only the firmware knows — `fw-esp32v3`
    /// asserts it at boot against the link-time `_rwtext_len` symbol before
    /// installing the arena.
    pub fn validate(&self) {
        assert!(
            self.len_bytes % 4 == 0 && self.len_bytes > 0,
            "code region {:?}+{:#x} length not a word multiple",
            self.placement,
            self.len_bytes
        );
        match self.placement {
            Placement::Sram0 { ibus_base } => {
                assert!(
                    ibus_base % 4 == 0,
                    "code region {:?}+{:#x} not word-aligned",
                    self.placement,
                    self.len_bytes
                );
                assert!(
                    ibus_base >= SRAM0_IRAM_BASE
                        && (ibus_base as u64 + self.len_bytes as u64) <= SRAM0_IRAM_END as u64,
                    "code region {:?}+{:#x} outside SRAM0's IRAM window",
                    self.placement,
                    self.len_bytes
                );
            }
            Placement::Sram1Mirrored { dbus_base } => {
                assert!(
                    dbus_base % 4 == 0,
                    "code region {:?}+{:#x} not word-aligned",
                    self.placement,
                    self.len_bytes
                );
                assert!(
                    dbus_base >= SRAM1_DRAM_BASE
                        && (dbus_base as u64 + self.len_bytes as u64) <= SRAM1_DRAM_END as u64,
                    "code region {:?}+{:#x} outside SRAM1's dual-mapped window",
                    self.placement,
                    self.len_bytes
                );
            }
        }
    }
}

// Pin the default so a change that breaks emulator parity is loud; the
// emulator's classic profile names the same SRAM0 window (cross-checked
// against `lp-xt-emu` itself in `tests/xt_classic_profile.rs`).
const _: () = assert!(CodeRegion::ESP32_DEFAULT.ibus_base() == 0x4008_8000);
const _: () = assert!(CodeRegion::ESP32_DEFAULT.ibus_end() == 0x4009_8000);
// SRAM0 writes are identity: the write address is the I-bus address, at both
// region ends.
const _: () = {
    let r = CodeRegion::ESP32_DEFAULT;
    assert!(r.write_addr(r.ibus_base()) == r.ibus_base());
    assert!(r.write_addr(r.ibus_end() - 4) == r.ibus_end() - 4);
};
// The region lies inside SRAM0's IRAM, above `.vectors`. Where `.rwtext` ends
// is a link-time fact the firmware asserts at boot; what can be pinned here
// is that the base leaves it room (32 KiB from the IRAM origin) and that
// nothing runs past the bus end.
const _: () = {
    let r = CodeRegion::ESP32_DEFAULT;
    assert!(r.ibus_base() >= SRAM0_IRAM_BASE);
    assert!(r.ibus_base() - SRAM0_IRAM_BASE == 0x7C00);
    assert!(r.ibus_end() <= SRAM0_IRAM_END);
};
// Under the SRAM0 placement the heap gets the WHOLE SRAM1 tail — the span
// the legacy region used to be carved from, 96 KiB, ending at SRAM1's top.
// This is the invariant `fw-esp32v3/src/main.rs` hands to `esp_alloc`; a
// placement change that forgets the heap boundary cannot compile.
const _: () = {
    let (heap_base, heap_len) = CodeRegion::ESP32_DEFAULT.reclaimable_heap_span();
    assert!(heap_base == 0x3FFE_8000);
    assert!(heap_len == 0x0001_8000); // 98,304 B returned to the allocator
    assert!(heap_base + heap_len == CodeRegion::ESP32_DRAM2_END);
    assert!(heap_base == CodeRegion::ESP32_SRAM1_TAIL_BASE);
    // The tail starts at or above dram2_seg's origin — never over the ROM's
    // data/stack reservations.
    assert!(heap_base >= CodeRegion::ESP32_DRAM2_BASE);
    // Nothing of the JIT's is below the tail: the lowest SRAM1 address the
    // JIT-or-heap claims IS the tail base (what the ROM-stack reclaim ends at).
    assert!(CodeRegion::ESP32_DEFAULT.sram1_claim_base() == 0x3FFE_8000);
    assert!(CodeRegion::ESP32_SRAM1_LEGACY.sram1_claim_base() == 0x3FFE_8000);
};
// The legacy SRAM1 region keeps the numbers it was measured with, so the
// mirrored path is still tested against the map the 2026-08 walks proved.
const _: () = assert!(CodeRegion::ESP32_SRAM1_LEGACY.ibus_base() == 0x400B_2000);
const _: () = assert!(CodeRegion::ESP32_SRAM1_LEGACY.ibus_end() == 0x400B_8000);
// The mirror rule round-trips at both region ends.
const _: () = {
    let r = CodeRegion::ESP32_SRAM1_LEGACY;
    let Placement::Sram1Mirrored { dbus_base } = r.placement else {
        panic!("legacy region is SRAM1")
    };
    assert!(r.write_addr(r.ibus_base()) == dbus_base + r.len_bytes - 4);
    assert!(r.write_addr(r.ibus_end() - 4) == dbus_base);
    // Its heap span and the region partition `0x3FFE_8000..0x4000_0000`
    // exactly: they abut, they do not overlap, nothing between them is lost.
    let (heap_base, heap_len) = r.reclaimable_heap_span();
    assert!(heap_base == dbus_base + r.len_bytes);
    assert!(heap_base + heap_len == CodeRegion::ESP32_DRAM2_END);
    assert!(heap_len == 0x0001_2000); // 72 KiB — the pre-SRAM0 figure
};
// The reclaimed span must not be able to ABUT the `dram_seg` arena, which is
// the firmware's other heap region. Adjacent regions are indistinguishable
// from one region to any tool that recovers the free list by address
// contiguity — `fw-esp32v3`'s `free_list_shape` walks runs exactly that way,
// and two abutting regions would merge into one run whose reported `largest`
// names a block no single allocation can ever get. `dram_seg` ends at
// `0x3FFE_0000`; the tail starts 32 KiB above it, so a gap always exists.
// Asserted rather than assumed because this file is where the boundary
// moves. (Track A's ROM-stack chunk fills part of that gap deliberately, as
// a region of its own that ends at the tail's base — see its ADR.)
const _: () = {
    const DRAM_SEG_END: u32 = 0x3FFE_0000;
    let (heap_base, _) = CodeRegion::ESP32_DEFAULT.reclaimable_heap_span();
    assert!(
        heap_base > DRAM_SEG_END,
        "SRAM1 heap span could abut the arena"
    );
    assert!(CodeRegion::ESP32_DRAM2_BASE > DRAM_SEG_END);
};

/// Where installed words go. The device implementation writes through the
/// address [`CodeRegion::write_addr`] names — the I-bus address itself for
/// SRAM0, the mirrored D-bus address for SRAM1; tests hand in an `lp-xt-emu`
/// memory (or a plain map) so the walk is verified against the
/// silicon-measured alias model without hardware.
pub trait CodeSink {
    fn write_word(&mut self, write_addr: u32, word: u32);
    /// Make installed code visible to the fetch path. Belt-and-braces on the
    /// classic chip (internal SRAM is uncached; measured on both placements),
    /// so default no-op.
    fn sync(&mut self) {}
}

/// The device sink: word-aligned volatile writes plus a full fence.
///
/// Only meaningful on the classic chip itself; compiled for any Xtensa target
/// so the crate builds identically across firmware crates, but constructing
/// it on the S3 would be a wiring bug (the S3 pipeline is heap-in-place).
///
/// No `isync` here, deliberately: internal SRAM is uncached on this chip and
/// silicon says freshly written code executes with **no barriers at all** —
/// the experiment repo measured it for SRAM1 (C2n), and `fw-esp32v3`'s
/// `test_sram0_exec` measured it for SRAM0 on 2026-09-05 (1,000
/// rewrite-then-call iterations each with no barrier, with this fence, and
/// with fence + `isync`: 0 stale results in all three). The `fence` (LLVM
/// lowers `SeqCst` to `memw` on Xtensa) is kept as free insurance, but
/// `isync` would need inline asm, which is `asm_experimental_arch` on Xtensa
/// — and this crate's app path deliberately never enables that feature (see
/// fw-esp32s3's `board/esp32s3/mod.rs` for the same posture). If silicon
/// ever proves an `isync` necessary, the firmware crate supplies its own
/// [`CodeSink`] with the asm, feature-gated there, rather than this crate
/// taking the feature.
#[cfg(target_arch = "xtensa")]
pub struct DeviceCodeSink;

#[cfg(target_arch = "xtensa")]
impl CodeSink for DeviceCodeSink {
    fn write_word(&mut self, write_addr: u32, word: u32) {
        // SAFETY: `install` only hands out word-aligned write addresses
        // inside a validated region the firmware reserved for JIT code.
        unsafe { (write_addr as *mut u32).write_volatile(word) };
    }

    fn sync(&mut self) {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}

/// Install `code` at I-bus address `ibus_base` inside `region`, one word at a
/// time through [`CodeRegion::write_addr`] (upward through SRAM0, downward
/// through SRAM1's mirror). Trailing bytes of the final word are zero-padded
/// (they sit after the final instruction and are never executed). Does NOT
/// call [`CodeSink::sync`]; the caller syncs once after all installs for a
/// module.
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
        sink.write_word(region.write_addr(ibus_base + i * 4), u32::from_le_bytes(w));
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
        // region's inverse map: write_addr(iram) == dram.
        let r = CodeRegion {
            placement: Placement::Sram1Mirrored {
                dbus_base: SRAM1_DRAM_BASE,
            },
            len_bytes: 0x2_0000,
        };
        for (dram, iram) in [
            (0x3FFE_0000u32, 0x400B_FFFCu32),
            (0x3FFE_0004, 0x400B_FFF8),
            (0x3FFE_8000, 0x400B_7FFC),
            (0x3FFF_EFFC, 0x400A_1000),
            (0x3FFF_FFFC, 0x400A_0000),
        ] {
            assert_eq!(r.write_addr(iram), dram, "iram {iram:#x}");
        }
    }

    /// The default is SRAM0: 64 KiB at `0x4008_8000`, identity writes, and a
    /// heap span that is the whole SRAM1 tail — the 2026-09-05 placement,
    /// measured on the dig2go.
    #[test]
    fn default_region_is_sram0_with_identity_writes() {
        let r = CodeRegion::ESP32_DEFAULT;
        r.validate();
        assert_eq!(
            r.placement,
            Placement::Sram0 {
                ibus_base: 0x4008_8000
            }
        );
        assert_eq!(r.len_bytes, 64 * 1024);
        assert_eq!(r.ibus_base(), 0x4008_8000);
        assert_eq!(r.ibus_end(), 0x4009_8000);
        // No mirror: every word is written where it is fetched, and the walk
        // ascends with the I-bus address.
        let mut ibus = r.ibus_base();
        while ibus < r.ibus_end() {
            assert_eq!(r.write_addr(ibus), ibus);
            ibus += 4;
        }
        assert_eq!(
            r.write_addr(r.ibus_base() + 4),
            r.write_addr(r.ibus_base()) + 4
        );
    }

    /// The legacy SRAM1 region keeps the numbers the 2026-08 walks proved: the
    /// D-bus base is the region's top word on the I-bus, and shrinking it
    /// moved the I-bus BASE up while the END stayed put — that asymmetry is
    /// the mirror, and it is the thing most worth pinning here.
    #[test]
    fn legacy_region_is_the_runner_region() {
        let r = CodeRegion::ESP32_SRAM1_LEGACY;
        r.validate();
        assert_eq!(r.ibus_base(), 0x400B_2000);
        assert_eq!(r.ibus_end(), 0x400B_8000);
        // Word 0 writes through the D-bus LAST word; the last word writes
        // through the D-bus base — the descending walk.
        assert_eq!(r.write_addr(r.ibus_base()), 0x3FFE_DFFC);
        assert_eq!(r.write_addr(r.ibus_end() - 4), 0x3FFE_8000);
    }

    /// Under SRAM0 the heap gets the whole SRAM1 tail: 98,304 B from where
    /// the legacy region used to start, up to SRAM1's top.
    #[test]
    fn sram0_region_reclaims_the_whole_sram1_tail() {
        let (base, len) = CodeRegion::ESP32_DEFAULT.reclaimable_heap_span();
        assert_eq!(base, 0x3FFE_8000);
        assert_eq!(len, 96 * 1024);
        assert_eq!(base + len, CodeRegion::ESP32_DRAM2_END);
        // The tail is exactly the legacy region plus its own leftover — the
        // 24 KiB the move returns to the heap is the legacy region's length.
        let legacy = CodeRegion::ESP32_SRAM1_LEGACY;
        let (legacy_base, legacy_len) = legacy.reclaimable_heap_span();
        assert_eq!(len - legacy_len, legacy.len_bytes);
        assert_eq!(legacy_base - base, legacy.len_bytes);
        // Both placements claim SRAM1 from the same floor: the SRAM0 region
        // through its heap span, the legacy region through its own base.
        assert_eq!(CodeRegion::ESP32_DEFAULT.sram1_claim_base(), base);
        assert_eq!(legacy.sram1_claim_base(), base);
        assert_ne!(legacy.sram1_claim_base(), legacy_base);
    }

    /// The legacy heap span and the legacy region tile
    /// `0x3FFE_8000..0x4000_0000` without gap or overlap — the property the
    /// firmware relied on when this placement was the default.
    #[test]
    fn legacy_heap_span_abuts_the_legacy_region() {
        let r = CodeRegion::ESP32_SRAM1_LEGACY;
        let Placement::Sram1Mirrored { dbus_base } = r.placement else {
            panic!("legacy region is SRAM1")
        };
        let (base, len) = r.reclaimable_heap_span();
        assert_eq!(base, dbus_base + r.len_bytes);
        assert_eq!(base, 0x3FFE_E000);
        assert_eq!(len, 72 * 1024);
        assert_eq!(base + len, CodeRegion::ESP32_DRAM2_END);
    }

    /// An SRAM1 region below `dram2_seg` must refuse to name a heap span
    /// rather than offer one that contains the ROM's stacks and data.
    #[test]
    #[should_panic(expected = "below dram2_seg")]
    fn reclaimable_heap_span_refuses_a_region_over_the_rom_reservations() {
        let rogue = CodeRegion {
            placement: Placement::Sram1Mirrored {
                dbus_base: SRAM1_DRAM_BASE, // 0x3FFE_0000 — ROM data lives here
            },
            len_bytes: 0x1000,
        };
        let _ = rogue.reclaimable_heap_span();
    }

    /// An SRAM0 region must sit inside the IRAM window, above `.vectors`.
    #[test]
    #[should_panic(expected = "outside SRAM0's IRAM window")]
    fn validate_refuses_an_sram0_region_over_the_vectors() {
        CodeRegion {
            placement: Placement::Sram0 {
                ibus_base: 0x4008_0000,
            },
            len_bytes: 0x1000,
        }
        .validate();
    }

    #[test]
    #[should_panic(expected = "outside SRAM0's IRAM window")]
    fn validate_refuses_an_sram0_region_past_the_bus_end() {
        CodeRegion {
            placement: Placement::Sram0 {
                ibus_base: SRAM0_IRAM_END - 0x800,
            },
            len_bytes: 0x1000,
        }
        .validate();
    }

    struct MapSink(alloc::collections::BTreeMap<u32, u32>);
    impl CodeSink for MapSink {
        fn write_word(&mut self, write_addr: u32, word: u32) {
            assert_eq!(write_addr % 4, 0);
            self.0.insert(write_addr, word);
        }
    }

    #[test]
    fn install_walks_upward_through_sram0_and_pads_the_tail() {
        let r = CodeRegion::ESP32_DEFAULT;
        let mut sink = MapSink(Default::default());
        // 6 bytes → 2 words, second word tail-padded with zeros.
        install(&r, r.ibus_base(), &[1, 2, 3, 4, 5, 6], &mut sink).unwrap();
        assert_eq!(
            sink.0.get(&r.ibus_base()),
            Some(&u32::from_le_bytes([1, 2, 3, 4]))
        );
        assert_eq!(
            sink.0.get(&(r.ibus_base() + 4)),
            Some(&u32::from_le_bytes([5, 6, 0, 0]))
        );
        assert_eq!(sink.0.len(), 2, "exactly the two words, nowhere else");
    }

    #[test]
    fn install_walks_downward_through_the_sram1_mirror() {
        let r = CodeRegion::ESP32_SRAM1_LEGACY;
        let mut sink = MapSink(Default::default());
        install(&r, r.ibus_base(), &[1, 2, 3, 4, 5, 6], &mut sink).unwrap();
        assert_eq!(
            sink.0.get(&r.write_addr(r.ibus_base())),
            Some(&u32::from_le_bytes([1, 2, 3, 4]))
        );
        assert_eq!(
            sink.0.get(&r.write_addr(r.ibus_base() + 4)),
            Some(&u32::from_le_bytes([5, 6, 0, 0]))
        );
        // Downward: word 1's write address is 4 BELOW word 0's.
        assert_eq!(
            r.write_addr(r.ibus_base() + 4),
            r.write_addr(r.ibus_base()) - 4
        );
    }

    #[test]
    fn install_rejects_out_of_region_spans() {
        for r in [CodeRegion::ESP32_DEFAULT, CodeRegion::ESP32_SRAM1_LEGACY] {
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
            // Below the base.
            assert!(matches!(
                install(&r, r.ibus_base() - 4, &[0; 4], &mut sink),
                Err(CodeMemError::BadSpan { .. })
            ));
            assert!(sink.0.is_empty(), "nothing written on rejection");
        }
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
