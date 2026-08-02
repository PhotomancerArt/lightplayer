//! The abort-tier panic path: **stage, commit, then** print and reset.
//!
//! Per ADR `2026-07-29-per-chip-fw-toolchains` the classic is **abort tier**.
//! There is no `unwinding`, no `catch_unwind`, and therefore no layer-1
//! in-process recovery: a panic is terminal for the boot. The only job left is
//! to make the *next* boot able to say what died.
//!
//! ## Why the order differs from fw-esp32s3's
//!
//! fw-esp32s3 prints first and stages afterwards, which is the natural reading
//! order and costs it nothing — its panic handler runs to completion.
//!
//! This handler does the opposite, and the inversion is the entire point of
//! the module. Measured on this board 2026-08-01
//! (`docs/defects/2026-08-01-classic-rmt-open-fault.md`): a fault raised while
//! WS281x channels are opening resets the chip after roughly **five characters**
//! of output, in well under a millisecond. Interrupt masking, draining the TX
//! FIFO first, printing the line before the path, chunking the path into 4-byte
//! writes and two different heap sizes all produced the same five characters.
//! Anything sequenced after a `println!` on this path does not happen.
//!
//! So the record goes into RTC RAM before a single byte is formatted for
//! output, and it is **committed** there rather than left tentative:
//! `Recovery::init` discards a tentative record on the next boot unless the
//! reset cause was a watchdog, and this fault is not a watchdog. See
//! [`lp_recovery::commit_staged_crash`] for why that is sound here and unsound
//! on the unwinding tier.
//!
//! ## Two consequences worth stating outright
//!
//! **1. No `is_esp_sync_reentrant_lock_panic` guard**, for the same reason
//! fw-esp32s3 has none: this path allocates nothing. `lp_recovery::stage_crash`
//! is zero-alloc by contract, and `esp_println` / `critical-section` are backed
//! by `esp_sync::RawMutex`, which is reentrant. The hazard the C6 guards
//! against — boxing a payload for `unwinding::begin_panic` under
//! `esp-alloc`'s `NonReentrantMutex` — cannot arise. [`PANICKING`] covers the
//! broader case of this handler panicking by some other route.
//!
//! **2. It always resets; it never hangs.** A hung board is indistinguishable
//! from a dead one, and `lp-recovery`'s incomplete-boot counter turns a genuine
//! boot loop into safe mode. Resetting unconditionally means the panic is
//! visible instead of silent.

use core::sync::atomic::{AtomicBool, Ordering};

use lpc_shared::backtrace::{MAX_FRAMES, capture_frames};

/// Whether `lpc_shared::backtrace::capture_frames` has a real stack walker for
/// this target.
///
/// `true`: the Xtensa arm of `capture_frames_arch` is LX6/LX7-generic (windowed
/// ABI, base-save-area chain) and this crate enables `lpc-shared`'s
/// `xt-map-esp32-classic` feature, which points its bounds checks at the
/// classic's IRAM / IROM / DRAM windows instead of the S3's.
///
/// ⚠️ This constant and that cargo feature are one fact in two places. Without
/// the feature the walker still *runs* — it just rejects every candidate
/// against S3 addresses and returns zero, which [`print_frames`] would then
/// report as "the stack was unreadable". If the feature is ever dropped from
/// `Cargo.toml`, this must go to `false` in the same commit.
const FRAME_WALKER_PRESENT: bool = true;

/// Set on entry to the panic path, never cleared. Guards against a panic
/// raised *by* the panic path (a panicking `Display` impl, most plausibly)
/// recursing forever — `no_std` has no double-panic detection of its own.
static PANICKING: AtomicBool = AtomicBool::new(false);

/// The biggest single allocation the heap could satisfy **right now**, in bytes.
///
/// `esp_alloc::HEAP.free()` is the *sum* of the free list. On a linked-list
/// first-fit heap — which is what `esp-alloc` defaults to, and this image does
/// not override — that number says nothing about whether any one request can be
/// served. "requested=3072 free=5304 → failed" is not a contradiction; it is the
/// signature of a fragmented heap, and without this figure the two failure modes
/// are indistinguishable from the report. `free - largest` is the amount of
/// memory the board owns but cannot hand out in one piece.
///
/// `linked_list_allocator` exposes no free-list walk, so this asks the allocator
/// the only question it answers: binary-search the largest size it will accept,
/// returning each probe immediately. ~17 probes bounded by `free()`, each a
/// first-fit walk — microseconds, and only on paths that already decided to
/// spend time reporting.
///
/// ⚠️ `alloc::alloc::alloc` is deliberately the raw entry point: it returns null
/// on failure. The `handle_alloc_error` wrappers are what route into
/// [`stage_oom_and_reset`], so probing through them from inside that function
/// would recurse.
pub fn largest_free_block() -> usize {
    /// Ignore differences below this; a 4-byte-precise answer costs probes and
    /// tells no one anything the rounded one does not.
    const GRANULARITY: usize = 16;

    // `free()` bounds the answer from above: no single block can exceed the sum
    // of every block.
    let mut too_big = esp_alloc::HEAP.free() + 1;
    let mut fits = 0usize;

    while too_big - fits > GRANULARITY {
        let mid = fits + (too_big - fits) / 2;
        let Ok(layout) = core::alloc::Layout::from_size_align(mid, 4) else {
            break;
        };
        // SAFETY: `mid > 0` (the loop condition keeps `mid` above `fits >= 0`
        // by at least GRANULARITY/2), and the pointer is freed with the same
        // layout it was allocated with, immediately, before anything else runs.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            too_big = mid;
        } else {
            unsafe { alloc::alloc::dealloc(ptr, layout) };
            fits = mid;
        }
    }

    fits
}

/// Stage a breadcrumb into the RTC ledger, commit it, report on serial, reset.
/// Never returns, and never hangs.
pub fn stage_and_reset(info: &core::panic::PanicInfo) -> ! {
    if PANICKING.swap(true, Ordering::AcqRel) {
        // Re-entered while handling a panic. Do the absolute minimum — no
        // formatting of caller-controlled values, no ledger write — and go.
        esp_println::println!("\n[PANIC] recursive panic in the panic path; resetting now");
        esp_hal::system::software_reset()
    }

    // Mask interrupts before anything else. The RMT refill ISR runs
    // continuously while strips are transmitting; if the panic came from
    // inside it — or from anything it can re-enter — the next interrupt lands
    // in the middle of this handler and panics again. `PANICKING` would catch
    // the recursion, but only by throwing away the record we are here to
    // write.
    esp_hal::xtensa_lx::interrupt::disable();

    // ── Everything below this comment must survive the chip resetting mid-way.
    // Ledger first; serial second. See the module docs.
    let mut frames = [0u32; MAX_FRAMES];
    let count = capture_frames(&mut frames);
    let location = info.location().map(|loc| (loc.file(), loc.line()));
    let staged = lp_recovery::stage_crash(
        lp_recovery::CrashCause::Panic,
        &info.message(),
        location,
        &frames[..count],
        None,
    );
    if staged {
        // Promote to committed *now*, not at reset time: a reset that beats us
        // to `finalize_crash_and_reset` would otherwise leave a tentative
        // record, and the next boot throws those away.
        lp_recovery::commit_staged_crash();
    }

    // ── From here on we are spending time we may not have. Nothing after this
    // point is load-bearing for the next boot's report.
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    print_frames(&frames[..count]);
    if staged {
        esp_println::println!("[RECOVERY] crash committed to the RTC ledger");
    } else {
        // No recovery global: a panic before `recovery::init_and_report`, or a
        // build that never boots recovery. Say so, so the missing next-boot
        // report is not read as a lost breadcrumb.
        esp_println::println!("[RECOVERY] no ledger installed; this crash will not be reported");
    }

    esp_println::println!("[RECOVERY] resetting");
    lp_recovery::finalize_crash_and_reset();
    esp_hal::system::software_reset()
}

/// The `#[alloc_error_handler]` path: record the heap state, then reset.
///
/// Without this, an allocation failure reaches the ledger through Rust's
/// default handler — which panics, so it arrives as [`lp_recovery::CrashCause::Panic`]
/// with the stock `memory allocation of N bytes failed` message and **no heap
/// numbers at all**. That is the difference between "a 12,864-byte `Vec` growth
/// failed" and "a 12,864-byte `Vec` growth failed with 3 KB free", which are
/// different bugs with different fixes.
///
/// The second thing it buys is a usable walk. Coming through the default
/// handler the first eight frames are `panic_nounwind_fmt` →
/// `__rdl_alloc_error_handler` → `handle_alloc_error` → `raw_vec::handle_error`
/// — all machinery, and the chain broke before reaching the caller that
/// actually asked for the memory (observed 2026-08-01). Capturing here starts
/// the walk at the allocation site instead.
pub fn stage_oom_and_reset(layout: core::alloc::Layout) -> ! {
    if PANICKING.swap(true, Ordering::AcqRel) {
        // Recursive OOM — the allocator failed again while we were reporting
        // the first failure. Nothing here allocates, so this should be
        // unreachable; say so rather than looping.
        esp_println::println!("\n[OOM] recursive allocation failure while reporting; resetting");
        esp_hal::system::software_reset()
    }

    esp_hal::xtensa_lx::interrupt::disable();

    // Both are plain reads of the allocator's counters — no allocation, which
    // matters because we are here precisely because allocation is failing.
    let free = esp_alloc::HEAP.free();
    let used = esp_alloc::HEAP.used();
    // Before the ledger write, because it is the number that decides which bug
    // this is — see `largest_free_block`. Allocating here is safe: the request
    // that failed has already released the allocator's lock, and interrupts are
    // masked, so nothing can be mid-allocation underneath us.
    let largest = largest_free_block();

    let mut frames = [0u32; MAX_FRAMES];
    let count = capture_frames(&mut frames);

    // Ledger first, exactly as in `stage_and_reset` — see the module docs.
    let staged = lp_recovery::stage_crash(
        lp_recovery::CrashCause::Oom,
        &format_args!(
            "alloc {} bytes failed (align {}) in {}",
            layout.size(),
            layout.align(),
            lpc_shared::backtrace::oom_context().unwrap_or("<unset>"),
        ),
        None,
        &frames[..count],
        Some(lp_recovery::OomStats {
            requested: layout.size() as u32,
            align: layout.align() as u32,
            free: free as u32,
            used: used as u32,
        }),
    );
    if staged {
        lp_recovery::commit_staged_crash();
    }

    esp_println::println!("\n\n====================== OOM ======================");
    esp_println::println!(
        "allocation failed: requested={} align={} free={} used={} largest_free={} context={}",
        layout.size(),
        layout.align(),
        free,
        used,
        largest,
        lpc_shared::backtrace::oom_context().unwrap_or("<unset>"),
    );
    // Spelled out rather than left as arithmetic for the reader: this is the
    // line that distinguishes "the board is full" from "the board is shredded".
    if largest < layout.size() && free >= layout.size() {
        esp_println::println!(
            "[OOM] FRAGMENTED: {} bytes free but the largest single block is {} — {} bytes unusable",
            free,
            largest,
            free.saturating_sub(largest),
        );
    }
    print_frames(&frames[..count]);
    if !staged {
        esp_println::println!("[RECOVERY] no ledger installed; this OOM will not be reported");
    }

    esp_println::println!("[RECOVERY] resetting");
    lp_recovery::finalize_crash_and_reset();
    esp_hal::system::software_reset()
}

/// Print captured PCs — or say plainly why there are none.
///
/// The wording matters. "0 frames" from a target with no walker reads as "the
/// stack was empty", which is never true and would send someone hunting the
/// wrong bug. [`FRAME_WALKER_PRESENT`] is what keeps the two apart.
fn print_frames(frames: &[u32]) {
    if !frames.is_empty() {
        esp_println::print!("frames:");
        for frame in frames {
            esp_println::print!(" 0x{frame:08x}");
        }
        esp_println::println!();
        // Chip-specific on purpose: the classic's flash text lives at
        // 0x400Dxxxx where the S3's and C6's live at 0x42xxxxxx, so the
        // generic recipe would symbolize these against the wrong image and be
        // confidently wrong.
        esp_println::print!("decode: just decode-backtrace-esp32v3");
        for frame in frames {
            esp_println::print!(" 0x{frame:08x}");
        }
        esp_println::println!();
    } else if FRAME_WALKER_PRESENT {
        esp_println::println!(
            "frames: the walk found none — every candidate failed the IRAM/IROM \
             and stack bounds checks. The stack was not empty; it was unreadable."
        );
    } else {
        esp_println::println!(
            "frames: unavailable — no Xtensa stack walker in this build. \
             This is NOT an empty stack; nothing looked at it."
        );
    }
}
