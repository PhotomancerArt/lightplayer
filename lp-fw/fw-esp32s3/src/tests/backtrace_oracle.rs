//! The silicon oracle for `lpc_shared::backtrace`'s Xtensa windowed walk.
//!
//! A backtrace walker that returns plausible-looking garbage is worse than one
//! that returns nothing, because garbage gets believed — and a half-correct
//! Xtensa walk produces exactly that, since anything left on the stack below a
//! frame looks like a code pointer. So nothing here judges output by eye.
//!
//! ## The oracle
//!
//! A recursive `chain(n)` calls itself `n` times and captures at the bottom.
//! Every one of those `n` frames returns to **the same call site**, so a
//! correct walk must contain a run of exactly `n` identical PCs — no more, no
//! fewer. That number is fixed by the source, not by anything the walker
//! reports, which is what makes it an oracle rather than a plausibility check:
//!
//! - It is **exact**. `run == depth`, asserted, at three depths.
//! - It is **inlining-proof**. Whether `capture_frames` inlines into `chain(0)`
//!   shifts the frames by one but leaves the run length untouched, so the
//!   assertion does not depend on codegen decisions. The *total* count does, so
//!   only its slope against depth is asserted (`count(d₂) − count(d₁) == d₂ − d₁`).
//! - It is **deep enough to need the spill**. The S3's register file is 16
//!   `WindowBase` units and `chain` is entered with `call8` (2 units), so a
//!   depth-25 chain wraps the ring three times over. The frames near the bottom
//!   are live in the physical register file, not in memory, until something
//!   spills them — depth 3 would prove nothing.
//!
//! [`no_spill_control`] runs the same measurement while deliberately skipping
//! the window spill. It is reported, not asserted (a control that happens to
//! agree is not a defect), and exists so the transcript shows the spill is
//! load-bearing rather than decorative.
//!
//! ## The negative cases
//!
//! [`corrupt_chains`] hands the walker save-area chains that are cyclic,
//! descending, torn mid-chain, or anchored outside DRAM, built in a real
//! 16-aligned stack buffer. They must terminate at an exact frame count. The
//! same expectations are asserted host-side in `lpc-shared`'s unit tests
//! against a synthetic stack; running them again here proves the host model
//! and the silicon agree.

use core::hint::black_box;

use esp_println::println;
use lpc_shared::backtrace::{capture_frames, walk_frames_from};

/// Marker every result line carries, so a transcript can be grepped.
const TAG: &str = "[XT-BT]";

/// Larger than any chain plus the boot frames above it, so a measurement is
/// never truncated by the buffer instead of by the chain.
const BUF: usize = 64;

/// Three depths, all past the ring wrap, spread far enough apart that a
/// constant offset cannot masquerade as a slope of one.
const DEPTHS: [u32; 3] = [5, 15, 25];

pub fn run_all() -> ! {
    println!("{TAG} oracle start");

    let mut failures = 0u32;
    failures += chain_depths();
    failures += corrupt_chains();
    no_spill_control();

    if failures == 0 {
        println!("{TAG} RESULT pass");
    } else {
        println!("{TAG} RESULT fail ({failures} checks)");
    }
    println!("{TAG} done");

    loop {
        core::hint::spin_loop();
    }
}

// ---------------------------------------------------------------------------
// The known-depth chain
// ---------------------------------------------------------------------------

/// `n` nested windowed frames, then a capture.
///
/// `black_box` on the argument stops LLVM constant-propagating the recursion
/// into distinct clones (which would give each depth its own call site and
/// destroy the run); `black_box` on the result stops it rewriting the
/// accumulator recursion into a loop, which would produce no frames at all.
/// `#[inline(never)]` keeps every level a real call.
#[inline(never)]
fn chain(n: u32, out: &mut [u32]) -> usize {
    if n == 0 {
        return capture_frames(out);
    }
    let got = chain(black_box(n - 1), out);
    black_box(got)
}

/// Same shape, but walking from the live `a0`/`a1` **without** the forced
/// window spill — the control.
#[inline(never)]
fn chain_unspilled(n: u32, out: &mut [u32]) -> usize {
    if n == 0 {
        let ra: u32;
        let sp: u32;
        // SAFETY: two register reads with no side effects.
        unsafe {
            core::arch::asm!(
                "mov {ra}, a0",
                "mov {sp}, a1",
                ra = out(reg) ra,
                sp = out(reg) sp,
                options(nomem, nostack, preserves_flags),
            );
        }
        return walk_frames_from(out, ra, sp);
    }
    let got = chain_unspilled(black_box(n - 1), out);
    black_box(got)
}

/// Length of the longest run of identical consecutive PCs, and that PC.
fn longest_run(frames: &[u32]) -> (usize, u32) {
    let mut best = (0usize, 0u32);
    let mut i = 0;
    while i < frames.len() {
        let mut j = i;
        while j < frames.len() && frames[j] == frames[i] {
            j += 1;
        }
        if j - i > best.0 {
            best = (j - i, frames[i]);
        }
        i = j;
    }
    best
}

/// Returns the number of failed checks.
fn chain_depths() -> u32 {
    let mut failures = 0;
    let mut counts = [0usize; DEPTHS.len()];
    let mut run_pcs = [0u32; DEPTHS.len()];

    for (slot, depth) in DEPTHS.iter().enumerate() {
        let mut frames = [0u32; BUF];
        let count = chain(*depth, &mut frames);
        let (run, run_pc) = longest_run(&frames[..count]);
        counts[slot] = count;
        run_pcs[slot] = run_pc;

        println!("{TAG} depth={depth} count={count} run={run} run_pc=0x{run_pc:08x}");
        for (i, frame) in frames[..count].iter().enumerate() {
            println!("{TAG}   frame[{i}] = 0x{frame:08x}");
        }

        failures += check(
            "run length equals the chain depth",
            run == *depth as usize,
            *depth as usize,
            run,
        );
        failures += check(
            "count leaves room for the frames above the chain",
            count > *depth as usize,
            *depth as usize + 1,
            count,
        );
        // Redundant with the walker's own bounds check, and deliberately so:
        // it is the property the whole phase exists to guarantee, and a
        // regression that widened the accepted window would show up here.
        let all_text = frames[..count].iter().all(|f| {
            (0x4037_0000..0x403E_0000).contains(f) || (0x4200_0000..0x4400_0000).contains(f)
        });
        failures += check("every frame is in IRAM or the flash window", all_text, 1, 1);
    }

    // The chain's call site is one instruction; every depth must find it.
    failures += check(
        "the run PC is the same call site at every depth",
        run_pcs[0] == run_pcs[1] && run_pcs[1] == run_pcs[2],
        run_pcs[0] as usize,
        run_pcs[2] as usize,
    );

    // Total frame count carries an inlining-dependent constant, so assert its
    // slope rather than its value: ten more frames of chain, ten more frames
    // reported.
    for i in 1..DEPTHS.len() {
        let expected = (DEPTHS[i] - DEPTHS[i - 1]) as usize;
        let observed = counts[i].wrapping_sub(counts[i - 1]);
        failures += check(
            "count grows one-for-one with depth",
            observed == expected,
            expected,
            observed,
        );
    }

    failures
}

// ---------------------------------------------------------------------------
// Negative cases
// ---------------------------------------------------------------------------

/// A 16-aligned scratch stack, so save-area addresses land where the walker's
/// alignment rule expects them.
#[repr(align(16))]
struct Scratch([u32; 64]);

impl Scratch {
    fn new() -> Self {
        Scratch([0; 64])
    }

    fn base(&self) -> u32 {
        self.0.as_ptr() as u32
    }

    /// Stack pointer of synthetic frame `index`. Frame 0 sits two frames in so
    /// `[sp-16, sp)` is inside the buffer.
    fn sp(&self, index: usize) -> u32 {
        self.base() + 64 + index as u32 * 32
    }

    fn write(&mut self, addr: u32, value: u32) {
        let index = (addr - self.base()) as usize / 4;
        self.0[index] = value;
    }

    /// Link `depth` frames so a walk seeded with `(ra, sp(0))` reports exactly
    /// `depth` frames.
    fn link(&mut self, depth: usize, ra: u32) {
        for i in 0..depth - 1 {
            let sp = self.sp(i);
            let next = self.sp(i + 1);
            self.write(sp - 16, ra);
            self.write(sp - 12, next);
        }
    }
}

/// Returns the number of failed checks.
fn corrupt_chains() -> u32 {
    let mut failures = 0;
    let mut frames = [0u32; BUF];
    // Any address inside the flash window; the walker only has to accept it.
    let ra = 0x4200_1000u32;

    let mut scratch = Scratch::new();
    scratch.link(8, ra);
    println!("{TAG} scratch base=0x{:08x}", scratch.base());

    let intact = walk_frames_from(&mut frames, ra, scratch.sp(0));
    failures += check(
        "an intact synthetic chain reports its exact depth",
        intact == 8,
        8,
        intact,
    );

    let torn = {
        let mut s = Scratch::new();
        s.link(8, ra);
        let sp = s.sp(4);
        s.write(sp - 12, 0xDEAD_BEEF);
        walk_frames_from(&mut frames, ra, s.sp(0))
    };
    failures += check("a torn chain stops at the tear", torn == 5, 5, torn);

    let cyclic = {
        let mut s = Scratch::new();
        s.link(8, ra);
        let sp = s.sp(0);
        s.write(sp - 12, sp);
        walk_frames_from(&mut frames, ra, s.sp(0))
    };
    failures += check(
        "a self-referential chain does not loop",
        cyclic == 1,
        1,
        cyclic,
    );

    let descending = {
        let mut s = Scratch::new();
        s.link(8, ra);
        let sp = s.sp(3);
        s.write(sp - 12, sp - 32);
        walk_frames_from(&mut frames, ra, s.sp(0))
    };
    failures += check(
        "a descending chain terminates",
        descending == 4,
        4,
        descending,
    );

    let off_stack = walk_frames_from(&mut frames, ra, 0xDEAD_BEEF);
    failures += check(
        "a stack pointer outside DRAM stops the walk",
        off_stack == 1,
        1,
        off_stack,
    );

    let unaligned = walk_frames_from(&mut frames, ra, scratch.sp(0) + 4);
    failures += check(
        "an unaligned stack pointer stops the walk",
        unaligned == 1,
        1,
        unaligned,
    );

    let no_ra = walk_frames_from(&mut frames, 0, scratch.sp(0));
    failures += check(
        "a null return address reports nothing",
        no_ra == 0,
        0,
        no_ra,
    );

    failures
}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

/// Reported, never asserted. The spilled measurement is the oracle; this only
/// shows what the same walk sees when the register windows were never pushed
/// to memory, so "the spill matters" is a line in the transcript instead of a
/// claim in a comment.
fn no_spill_control() {
    for depth in DEPTHS {
        let mut frames = [0u32; BUF];
        let count = chain_unspilled(depth, &mut frames);
        let (run, run_pc) = longest_run(&frames[..count]);
        println!(
            "{TAG} control(no-spill) depth={depth} count={count} run={run} run_pc=0x{run_pc:08x}"
        );
    }
}

fn check(what: &str, ok: bool, expected: usize, observed: usize) -> u32 {
    if ok {
        println!("{TAG} PASS {what} (expected {expected}, got {observed})");
        0
    } else {
        println!("{TAG} FAIL {what} (expected {expected}, got {observed})");
        1
    }
}
