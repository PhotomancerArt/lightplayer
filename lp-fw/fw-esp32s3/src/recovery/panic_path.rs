//! The abort-tier panic path: print, stage a breadcrumb, reset.
//!
//! Per ADR `2026-08-02-rv32-firmwares-are-abort-tier` **every** firmware target
//! is abort tier, this one included. There is no `unwinding`, no
//! `catch_unwind`, no `.eh_frame`, and therefore no layer-1 in-process
//! recovery: a panic is terminal for the boot. The only job left is to make the
//! *next* boot able to say what died, which is what [`stage_and_reset`] does.
//!
//! Until 2026-08-02 the C6 was the exception, and this module doc used to
//! define itself against it: "structurally the C6's `panic_handler` minus
//! everything unwinding needed". The C6 now runs a near-copy of this file, so
//! the contrast is gone — but one piece of it is worth keeping, because it
//! explains an absence a reader may still go looking for:
//!
//! **1. No `is_esp_sync_reentrant_lock_panic` guard**, because this path
//! allocates nothing. The C6 needed one only because its old handler *allocated*
//! — boxing a `PanicPayload` for `unwinding::begin_panic` — and allocation takes
//! `esp-alloc`'s [`esp_sync::NonReentrantMutex`], so a panic raised while that
//! lock was held re-entered it inside the panic handler and panicked again
//! ("lock is not reentrant"), forever. (Its guard never worked: the `println!`s
//! that retake the lock ran first.)
//!
//! `lp_recovery::stage_crash` is zero-alloc by contract; `esp_println` and
//! `critical-section` are both backed by `esp_sync::RawMutex`, which *is*
//! reentrant (only `NonReentrantMutex::with` panics); and
//! `esp_hal::system::software_reset` is a lock-free ROM call. So that hazard
//! cannot arise on any chip now.
//!
//! What *can* still arise is a panic raised from inside this handler by some
//! other route — a user `Display` impl that panics while we format the message
//! is the realistic one, since `no_std` has no double-panic detection and the
//! handler would simply recurse until the stack dies. [`PANICKING`] covers that
//! case, and covers it more broadly than a file-path sniff would: it is set
//! before the first byte is formatted, so any re-entry at all takes the short
//! path to reset.
//!
//! **2. It always resets; it never hangs.** The C6's old `fatal_reset_or_hang`
//! parked in a `loop {}` when no recovery global was installed, so that a panic
//! before recovery init did not boot-loop a dev board. The abort tier makes
//! the opposite trade: a hung board is indistinguishable from a dead one, and
//! `lp-recovery`'s incomplete-boot counter already turns a genuine boot loop
//! into safe mode. Resetting unconditionally means the panic line is printed
//! once per loop and the failure is visible instead of silent.

use core::sync::atomic::{AtomicBool, Ordering};

use lpc_shared::backtrace::{MAX_FRAMES, capture_frames};

/// Whether `lpc_shared::backtrace::capture_frames` has a real stack walker for
/// this target.
///
/// `true` since M3 P4: the Xtensa arm of `capture_frames_arch` forces a
/// register-window spill and then walks the windowed base-save-area chain,
/// proven on silicon by `--features test_backtrace_oracle` (a known-depth call
/// chain whose frame count must match exactly). It exists so the crash report
/// can distinguish "we cannot see the stack" from "we looked and the stack was
/// empty", which are very different things to tell someone reading a crash
/// report — so it must go back to `false` if the walker is ever removed for a
/// new chip, not stay `true` out of habit.
const FRAME_WALKER_PRESENT: bool = true;

/// Set on entry to the panic path, never cleared. Guards against a panic
/// raised *by* the panic path (a panicking `Display` impl, most plausibly)
/// recursing forever — `no_std` has no double-panic detection of its own.
static PANICKING: AtomicBool = AtomicBool::new(false);

/// A cheap **estimate** of the biggest single allocation the heap could
/// satisfy right now, in bytes. Never larger than the truth; possibly smaller.
///
/// Ported from `fw-esp32v3/src/recovery/panic_path.rs`, minus its two-region
/// caveats: **this image's heap is one region** — `main.rs`'s
/// `esp_alloc::heap_allocator!(size: HEAP_SIZE)` and nothing else — so
/// `free - largest` here measures fragmentation and only fragmentation.
///
/// `esp_alloc::HEAP.free()` is the *sum* of the free list. On a linked-list
/// first-fit heap — which is what `esp-alloc` defaults to, and this image does
/// not override — that number says nothing about whether any one request can be
/// served. "requested=3072 free=5304 → failed" is not a contradiction; it is
/// the signature of a fragmented heap, and without this figure the two failure
/// modes are indistinguishable from the report. `free - largest` is the amount
/// of memory the board owns but cannot hand out in one piece.
///
/// `linked_list_allocator` exposes no free-list walk, so this asks the
/// allocator the only question it answers: binary-search the largest size it
/// will accept, returning each probe immediately. ~17 probes bounded by
/// `free()`, each a first-fit walk — microseconds, and only on paths that
/// already decided to spend time reporting.
///
/// ⚠️ **The predicate it bisects is not monotonic, so this is not a bound.**
/// `HoleList::split_current` rejects a hole outright when the leftover would be
/// too small to record as a `Hole` — a hole of `S + 4` bytes refuses a request
/// of `S` on this 32-bit target (`size_of::<Hole>()` is 8) while happily
/// serving `S + 4`. So `alloc(S)` can fail where `alloc(S + 4)` succeeds, and
/// bisection over that predicate lands on an arbitrary point below the true
/// maximum. Measured on the host against `linked_list_allocator` 0.10.5 — the
/// version `esp-alloc` 0.10 pins — 186 such size-pairs in 409,000 probes of
/// randomised heaps. Every value it returns *did* allocate, so it never
/// over-reports; treat it as a floor and nothing more.
///
/// ⚠️ `alloc::alloc::alloc` is deliberately the raw entry point: it returns
/// null on failure rather than routing into `handle_alloc_error`, which on a
/// chip that grew an OOM path would recurse.
///
/// ⚠️ Unlike the classic's, this probe is **not** reachable from this module's
/// panic path. It lives here because this is where its sibling lives on the
/// classic and because a future OOM report is the obvious second caller; today
/// its callers are `main.rs`'s two memory-stats functions, both of which run on
/// a healthy board.
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

/// Report the panic on serial, stage a breadcrumb into the RTC ledger, and
/// reset. Never returns, and never hangs.
pub fn stage_and_reset(info: &core::panic::PanicInfo) -> ! {
    if PANICKING.swap(true, Ordering::AcqRel) {
        // Re-entered while handling a panic. Do the absolute minimum — no
        // formatting of caller-controlled values, no ledger write — and go.
        esp_println::println!("\n[PANIC] recursive panic in the panic path; resetting now");
        esp_hal::system::software_reset()
    }

    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");

    let mut frames = [0u32; MAX_FRAMES];
    let count = capture_frames(&mut frames);
    print_frames(&frames[..count]);
    esp_println::println!();

    let location = info.location().map(|loc| (loc.file(), loc.line()));
    let staged = lp_recovery::stage_crash(
        lp_recovery::CrashCause::Panic,
        &info.message(),
        location,
        &frames[..count],
        None,
    );
    if !staged {
        // No recovery global: a panic before `recovery::init_and_report`, or a
        // harness build that never boots recovery. Nothing to commit — say so,
        // so the missing next-boot report is not read as a lost breadcrumb.
        esp_println::println!("[RECOVERY] no ledger installed; this crash will not be reported");
    }

    esp_println::println!("[RECOVERY] resetting");
    // Commits the staged record and resets through the backend. Returns
    // (rather than diverging) only when no global is installed, which is
    // exactly the `!staged` case above.
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
        // 0x42xxxxxx, so the generic recipe would symbolize these against the
        // wrong image and be confidently wrong.
        esp_println::print!("decode: just decode-backtrace-esp32s3");
        for frame in frames {
            esp_println::print!(" 0x{frame:08x}");
        }
        esp_println::println!();
    } else if FRAME_WALKER_PRESENT {
        esp_println::println!(
            "frames: the walk found none — every candidate failed the IRAM/flash \
             and stack bounds checks. The stack was not empty; it was unreadable."
        );
    } else {
        esp_println::println!(
            "frames: unavailable — no Xtensa stack walker in this build. \
             This is NOT an empty stack; nothing looked at it."
        );
    }
}
