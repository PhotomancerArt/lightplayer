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

/// A cheap **estimate** of the biggest single allocation the heap could satisfy
/// right now, in bytes. Never larger than the truth; possibly smaller.
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
/// spend time reporting. That makes it cheap enough for the 5-second heartbeat,
/// which is the only reason it still exists.
///
/// ⚠️ **The predicate it bisects is not monotonic, so this is not a bound.**
/// `HoleList::split_current` rejects a hole outright when the leftover would be
/// too small to record as a `Hole` — a hole of `S + 4` bytes refuses a request
/// of `S` on this 32-bit target (`size_of::<Hole>()` is 8) while happily serving
/// `S + 4`. So `alloc(S)` can fail where `alloc(S + 4)` succeeds, and bisection
/// over that predicate lands on an arbitrary point below the true maximum.
/// Measured on the host against `linked_list_allocator` 0.10.5 — the version
/// `esp-alloc` 0.10 pins — 186 such size-pairs in 409,000 probes of randomised
/// heaps. Every value it returns *did* allocate, so it never over-reports; treat
/// it as a floor and nothing more.
///
/// When the answer has to be exact, use [`free_list_shape`], which reads the
/// list instead of guessing at it.
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

/// The shape of the free list: how many holes, how big the biggest is, and how
/// much of `free()` the walk could actually account for.
#[derive(Clone, Copy)]
pub struct FreeListShape {
    /// Number of distinct holes found.
    pub holes: usize,
    /// Size of the largest hole, in bytes.
    pub largest: usize,
    /// Sum of every hole found, in bytes. Rounded down per hole to a multiple
    /// of the allocator's 8-byte minimum block, so this can sit up to `4 *
    /// holes` bytes below `free()` without anything being wrong.
    pub total: usize,
    /// The walk hit [`MAX_RUNS`] and stopped early; `holes`/`largest`/`total`
    /// describe only the low end of the heap.
    pub truncated: bool,
}

/// How many holes the walk will describe before it gives up. Each costs one
/// `(usize, usize)` of stack and nothing else.
const MAX_RUNS: usize = 32;

/// Read the free list exactly, using only the allocator's public API.
///
/// `linked_list_allocator` exposes no way to walk its holes, and vendoring it
/// to add one is a fork to carry forever. It does not need one: take the
/// smallest block the allocator will hand out (8 bytes on this target) over and
/// over until it refuses, and the returned addresses *are* the free list.
/// First-fit over an address-sorted list returns them ascending, so a run of
/// blocks with no gap is exactly one hole, and a gap is exactly one allocated
/// block in between. Then give every block back.
///
/// This is what [`largest_free_block`] only estimates, and it answers the
/// question that estimate cannot: is the heap one block or forty? At the OOM in
/// `docs/defects/2026-08-02-classic-oom-retry-succeeds.md` the two numbers agreed
/// to within 13 bytes, which *implied* one hole — but "implied" is what put an
/// hour into a fragmentation theory the free list would have killed in one line.
///
/// ⚠️ **This briefly owns every free byte in the heap.** Anything that allocates
/// while it runs — an ISR, another task — gets a null and dies. Call it only
/// with interrupts masked and a reset already committed. That is why the
/// heartbeat still uses the cheap estimate: a 5-second periodic that can OOM the
/// board it is monitoring is worse than an approximate number.
///
/// Cost is O(free / 8) allocations and the same number of frees. Both stay O(1)
/// each — allocation always takes the head hole, and the frees go back in
/// ascending address order so each merges into the front rather than walking the
/// list — so the whole walk is linear, well under a millisecond for a 110 KB
/// arena.
pub fn free_list_shape() -> FreeListShape {
    /// Smallest block `linked_list_allocator` will hand out: `size_of::<Hole>()`,
    /// which is `2 * size_of::<usize>()` — 8 bytes on this 32-bit target. A
    /// request of 1 byte is rounded up to exactly this.
    const STEP: usize = 2 * core::mem::size_of::<usize>();

    let Ok(unit) = core::alloc::Layout::from_size_align(1, 4) else {
        return FreeListShape {
            holes: 0,
            largest: 0,
            total: 0,
            truncated: false,
        };
    };

    // (start address, length in bytes) per contiguous run.
    let mut runs = [(0usize, 0usize); MAX_RUNS];
    let mut n = 0usize;
    let mut last_end = 0usize;
    let mut truncated = false;

    loop {
        // SAFETY: `unit` has non-zero size; every block taken here is released
        // in the loop below with the identical layout.
        let ptr = unsafe { alloc::alloc::alloc(unit) } as usize;
        if ptr == 0 {
            break;
        }
        if n > 0 && ptr == last_end {
            runs[n - 1].1 += STEP;
        } else {
            if n == MAX_RUNS {
                // Give this one straight back rather than leaking it, and stop:
                // beyond here we could not free what we took.
                unsafe { alloc::alloc::dealloc(ptr as *mut u8, unit) };
                truncated = true;
                break;
            }
            runs[n] = (ptr, STEP);
            n += 1;
        }
        last_end = ptr + STEP;
    }

    let mut largest = 0usize;
    let mut total = 0usize;
    for &(_, len) in &runs[..n] {
        total += len;
        if len > largest {
            largest = len;
        }
    }

    // Ascending order, so each block merges into the hole growing behind it.
    for &(start, len) in &runs[..n] {
        let mut off = 0usize;
        while off < len {
            // SAFETY: every address in the run came from the loop above and is
            // freed exactly once, with the layout it was allocated with.
            unsafe { alloc::alloc::dealloc((start + off) as *mut u8, unit) };
            off += STEP;
        }
    }

    FreeListShape {
        holes: n,
        largest,
        total,
        truncated,
    }
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

    // ── ORDER IS LOAD-BEARING BELOW THIS LINE ──────────────────────────────
    //
    // The retry goes FIRST, before any probe touches the heap. It is the only
    // measurement here that has to be taken on the heap the caller actually
    // saw; everything after it is describing a heap that has since had blocks
    // taken and given back.
    //
    // This was the other way round when the probe was written, and the report
    // it produced — `largest_free=3495 retry_ok=true` for a failed 3,072-byte
    // request — read as "the allocator refused something it could serve".
    // Perhaps, but the evidence did not say so: `retry_ok` had ~17 allocate/free
    // round trips standing between it and the failure. Replaying
    // `linked_list_allocator` 0.10.5 on the host says those round trips are in
    // fact inert (0 flips in 3,969,868 failing-request states), so that report
    // survives — but it survived by luck, the reasoning could not be checked
    // without a host replay, and it would NOT hold under `esp-alloc`'s TLSF
    // algorithm, whose free lists are rebuilt by exactly this traffic. A probe
    // that has to be proven harmless before its output means anything is the
    // wrong probe. See `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.
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

    // Now that the retry has been taken, the heap may be disturbed freely. Read
    // the free list exactly rather than bisecting for it: `largest_free_block`
    // is a floor, and a floor is what made the first report of this failure
    // ambiguous. This is safe here and nowhere else — interrupts are masked and
    // the reset below is already committed.
    let shape = free_list_shape();
    let largest = shape.largest;

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
    // The free list itself, not an inference from it. `holes=1` and `holes=40`
    // are the whole difference between exhaustion and fragmentation, and no
    // combination of `free`/`largest` tells them apart on its own.
    esp_println::println!(
        "[OOM] free list: holes={} largest={} total={}{}",
        shape.holes,
        shape.largest,
        shape.total,
        if shape.truncated { " (truncated)" } else { "" },
    );
    // Spelled out rather than left as arithmetic for the reader: these are the
    // lines that say which of three different bugs this is.
    if retry_ok {
        esp_println::println!(
            "[OOM] RETRY SUCCEEDED: the same {}-byte request fits now, on the heap the caller \
             saw — this retry runs before any probe touches the list. So the failure was not \
             this heap state. With interrupts unmasked between the null return and this handler, \
             the first suspect is something that freed in that window; after that, a second \
             allocator (the JIT code region) or a caller that asked for more than it reported",
            layout.size(),
        );
    } else if largest < layout.size() && free >= layout.size() {
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
