//! The abort-tier panic path: **stage, commit, then** print and reset.
//!
//! Per ADR `2026-08-02-rv32-firmwares-are-abort-tier` this chip is **abort
//! tier**, like `fw-esp32s3` and `fw-esp32v3`. There is no `unwinding`, no
//! `catch_unwind`, no `.eh_frame`, and therefore no layer-1 in-process
//! recovery: a panic is terminal for the boot. The only job left is to make the
//! *next* boot able to say what died.
//!
//! ## What this replaced, and why it matters to a reader
//!
//! Until 2026-08-02 this chip was the *only* one that unwound. Its panic
//! handler boxed a `PanicPayload` and called `unwinding::panic::begin_panic` so
//! that a per-node `catch_unwind` could turn a panic into a node error. That
//! design is gone for two reasons, both worth knowing before anyone proposes
//! bringing it back:
//!
//! - **It cost 796,624 B of flash** — `.eh_frame` plus LSDA plus landing pads,
//!   25.3% of the 3 MB app partition, measured A/B.
//! - **It had stopped working.** Unwinding one panic needs ~41 KB of stack
//!   (`unwinding`'s `Frame::from_context` alone builds a 20,736-byte gimli
//!   `UnwindContext`); the main stack is what DRAM is left after `.bss`, which
//!   is ~34 KB. So the first caught panic ran off the bottom of the stack and
//!   wrote `__stack_chk_guard`, and the fallout — esp-hal's `extern "C"`
//!   `ExceptionHandler` being nounwind, so `panic_cannot_unwind`, so re-entry
//!   into `esp_println`'s held `esp-sync` lock — turned every panic into an
//!   unbounded "lock is not reentrant" cascade and a bricked boot.
//!
//! The old handler carried an `is_esp_sync_reentrant_lock_panic` guard against
//! exactly that cascade. It was dead code: the four `println!`s that retake the
//! lock ran before the check could. **This path allocates nothing**, so the
//! hazard cannot arise: `lp_recovery::stage_crash` is zero-alloc by contract,
//! and `esp_println` / `critical-section` are backed by `esp_sync::RawMutex`,
//! which is reentrant (only `NonReentrantMutex::with` panics). [`PANICKING`]
//! covers the broader case of this handler panicking by some other route.
//!
//! ## Why the ledger is written before anything is printed
//!
//! Adopted from `fw-esp32v3`, whose ordering was forced by measurement: on the
//! classic, a fault raised while WS281x channels are opening resets the chip
//! after roughly five characters of output
//! (`docs/defects/2026-08-01-classic-rmt-open-fault.md`). No equivalent
//! measurement exists for this chip — its USB-Serial-JTAG path is a different
//! peripheral with different drain behaviour — so this ordering is adopted as
//! the safer default rather than because the C6 was seen to truncate. The cost
//! of being wrong in this direction is nothing; the cost of the other ordering
//! is a crash the next boot cannot name.
//!
//! The record is **committed** rather than left tentative: `Recovery::init`
//! discards a tentative record on the next boot unless the reset cause was a
//! watchdog. Committing early was unsound on the unwinding tier — a caught
//! panic had to be able to void the breadcrumb — and is sound here precisely
//! because nothing can catch a panic any more.
//!
//! ## It always resets; it never hangs
//!
//! The old handler's `fatal_reset_or_hang` parked in a `loop {}` when no
//! recovery global was installed. This makes the same trade `fw-esp32s3` and
//! `fw-esp32v3` do: a hung board is indistinguishable from a dead one, and
//! `lp-recovery`'s incomplete-boot counter already turns a genuine boot loop
//! into safe mode. Resetting unconditionally means the panic line is printed
//! once per loop and the failure is visible instead of silent.

use core::sync::atomic::{AtomicBool, Ordering};

use lpc_shared::backtrace::{MAX_FRAMES, capture_frames};

/// Whether `lpc_shared::backtrace::capture_frames` has a real stack walker for
/// this target.
///
/// `true`: the `riscv32` arm of `capture_frames_arch` walks the `s0` frame-pointer
/// chain, bounds-checked against the C6's DRAM window. It depends on
/// `-C force-frame-pointers` in `.cargo/config.toml` — that flag is **not**
/// part of the unwinding machinery and was deliberately kept when the rest was
/// removed. If it is ever dropped, this must go to `false` in the same commit.
///
/// It exists so the crash report can distinguish "we cannot see the stack" from
/// "we looked and the stack was empty", which are very different things to tell
/// someone reading a crash report.
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

    // Mask interrupts before anything else. If the panic came from inside an
    // ISR — or from anything an ISR can re-enter — the next interrupt lands in
    // the middle of this handler and panics again. `PANICKING` would catch the
    // recursion, but only by throwing away the record we are here to write.
    esp_hal::riscv::interrupt::disable();

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
        // harness build that never boots recovery. Say so, so the missing
        // next-boot report is not read as a lost breadcrumb.
        esp_println::println!("[RECOVERY] no ledger installed; this crash will not be reported");
    }

    esp_println::println!("[RECOVERY] resetting");
    lp_recovery::finalize_crash_and_reset();
    esp_hal::system::software_reset()
}

/// The `#[alloc_error_handler]` path: record the heap state, then reset.
///
/// Without this, an allocation failure reaches the ledger through Rust's
/// default handler — which panics, so it arrives as
/// [`lp_recovery::CrashCause::Panic`] with the stock
/// `memory allocation of N bytes failed` message and **no heap numbers at all**.
/// That is the difference between "a 12,864-byte `Vec` growth failed" and "a
/// 12,864-byte `Vec` growth failed with 3 KB free", which are different bugs
/// with different fixes.
///
/// The second thing it buys is a usable walk. Coming through the default
/// handler the first frames are `panic_nounwind_fmt` →
/// `__rdl_alloc_error_handler` → `handle_alloc_error` → `raw_vec::handle_error`
/// — all machinery. Capturing here starts the walk at the allocation site.
pub fn stage_oom_and_reset(layout: core::alloc::Layout) -> ! {
    if PANICKING.swap(true, Ordering::AcqRel) {
        // Recursive OOM — the allocator failed again while we were reporting
        // the first failure. Nothing here allocates except the probes below,
        // which return their memory immediately; say so rather than looping.
        esp_println::println!("\n[OOM] recursive allocation failure while reporting; resetting");
        esp_hal::system::software_reset()
    }

    esp_hal::riscv::interrupt::disable();

    // Both are plain reads of the allocator's counters — no allocation, which
    // matters because we are here precisely because allocation is failing.
    let free = esp_alloc::HEAP.free();
    let used = esp_alloc::HEAP.used();
    // Before the ledger write, because it is the number that decides which bug
    // this is — see `largest_free_block`. Allocating here is safe: the request
    // that failed has already released the allocator's lock, and interrupts are
    // masked, so nothing can be mid-allocation underneath us.
    let largest = largest_free_block();
    // Ask the allocator the caller's own question a second time. If the answer
    // is now yes, the shortfall was not the heap's state at this instant, and
    // no amount of reading `free`/`largest` here will explain it — the report
    // has to say so rather than let the next reader infer fragmentation from
    // numbers that do not support it. See
    // `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.
    //
    // SAFETY: `layout` came from a real allocation request, so its size is
    // non-zero, and the block is released immediately with the same layout.
    let retry_ok = unsafe {
        let ptr = alloc::alloc::alloc(layout);
        if ptr.is_null() {
            false
        } else {
            alloc::alloc::dealloc(ptr, layout);
            true
        }
    };

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
        "allocation failed: requested={} align={} free={} used={} largest_free={} retry_ok={} context={}",
        layout.size(),
        layout.align(),
        free,
        used,
        largest,
        retry_ok,
        lpc_shared::backtrace::oom_context().unwrap_or("<unset>"),
    );
    // Spelled out rather than left as arithmetic for the reader: these are the
    // lines that say which of three different bugs this is.
    if retry_ok {
        esp_println::println!(
            "[OOM] RETRY SUCCEEDED: the same {}-byte request fits now. The failure was not this \
             heap state — look for a second allocator (the JIT code region) or a caller that \
             asked for more than it reported",
            layout.size(),
        );
    } else if largest >= layout.size() {
        esp_println::println!(
            "[OOM] a {}-byte block is free but the request failed — an allocator edge, not \
             exhaustion and not fragmentation",
            largest,
        );
    } else if free >= layout.size() {
        esp_println::println!(
            "[OOM] FRAGMENTED: {} B free in total but only {} B in one piece",
            free,
            largest,
        );
    } else {
        esp_println::println!("[OOM] EXHAUSTED: the heap does not have {} B left", free);
    }

    print_frames(&frames[..count]);
    if staged {
        esp_println::println!("[RECOVERY] OOM committed to the RTC ledger");
    } else {
        esp_println::println!("[RECOVERY] no ledger installed; this crash will not be reported");
    }

    esp_println::println!("[RECOVERY] resetting");
    lp_recovery::finalize_crash_and_reset();
    esp_hal::system::software_reset()
}

/// Print captured PCs — or say plainly why there are none.
///
/// The wording matters. "0 frames" from a target with no walker reads as "the
/// stack was empty", which is never true and would send someone hunting the
/// wrong bug. [`FRAME_WALKER_PRESENT`] is what keeps the two apart: with a
/// walker present, zero frames means the walk ran and rejected everything it
/// found — a real and reportable outcome.
fn print_frames(frames: &[u32]) {
    if !frames.is_empty() {
        esp_println::print!("frames:");
        for frame in frames {
            esp_println::print!(" 0x{frame:08x}");
        }
        esp_println::println!();
        // Chip-specific on purpose: the C6 and the S3 both put flash text at
        // 0x42xxxxxx, so a generic recipe would symbolize these against the
        // wrong image and be confidently wrong.
        esp_println::print!("decode: just decode-backtrace");
        for frame in frames {
            esp_println::print!(" 0x{frame:08x}");
        }
        esp_println::println!();
    } else if FRAME_WALKER_PRESENT {
        esp_println::println!(
            "frames: the walk found none — every candidate failed the DRAM and \
             stack bounds checks. The stack was not empty; it was unreadable."
        );
    } else {
        esp_println::println!(
            "frames: unavailable — no RV32 stack walker in this build. \
             This is NOT an empty stack; nothing looked at it."
        );
    }
}
