//! A CPU-time pc sampler for host-time attribution — `--features selfprof`,
//! macOS, and off in every default build.
//!
//! # Why a hand-rolled sampler
//!
//! `/usr/bin/sample` is the only profiler on this desk (no `perf`, no
//! `samply`, no Instruments), and it attributes by **symbol**. At
//! `opt-level = 3` the whole interpreter — fetch, decode, the window check,
//! dispatch, execute and the RAM fast path — inlines into one symbol,
//! `XtHart::run_slice`, which `sample` reports as most of host time and
//! cannot open. A **pc** histogram, symbolized afterwards with `atos
//! --inlineFrames` (or `llvm-symbolizer --inlining`) against a build that
//! carries line tables, recovers the inlined frame chain and with it the
//! split the speed ladder needs.
//!
//! This is the same instrument the C6's ladder used for its M5 P1 buckets,
//! ported: same `ITIMER_PROF` (CPU time, so a loaded desk does not bias the
//! sample), same open-addressed table, same dump format. It is written down
//! here rather than left on a throwaway branch because the classic's ladder
//! is going to ask the question more than once.
//!
//! # Using it
//!
//! ```bash
//! # A build that can be symbolized (line tables; the release profile has none)
//! RUSTFLAGS="-C debuginfo=1" cargo build -p lp-emu-esp32v3 --release \
//!     --features selfprof --target-dir target/emu-prof
//! LP_EMU_SELFPROF=target/emu-prof/render-loop.pcs \
//!     target/emu-prof/release/lp-emu-esp32v3 --elf … --time-grade t1
//! scripts/emu/selfprof-buckets.py target/emu-prof/render-loop.pcs \
//!     --bin target/emu-prof/release/lp-emu-esp32v3
//! ```
//!
//! `LP_EMU_SELFPROF_US` sets the period (default 1000 µs). The sampler is
//! installed only when `LP_EMU_SELFPROF` names an output path, so a
//! `selfprof` binary with the variable unset runs the stock loop.
//!
//! ⚠️ **A sample is not a gate.** Nothing here changes a transcript — the
//! handler writes to its own table and nothing else — but a sampled share is
//! an attribution, not a measurement of what removing that work would buy.
//! The ablation is the other instrument, and this milestone does not run one.

#![allow(unsafe_code)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const SIGPROF: i32 = 27;
const SA_SIGINFO: i32 = 0x0040;
const SA_RESTART: i32 = 0x0002;
const ITIMER_PROF: i32 = 2;

/// macOS `struct sigaction`.
#[repr(C)]
struct SigAction {
    handler: usize,
    mask: u32,
    flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TimeVal {
    sec: i64,
    usec: i32,
    _pad: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ITimerVal {
    interval: TimeVal,
    value: TimeVal,
}

unsafe extern "C" {
    fn sigaction(sig: i32, act: *const SigAction, old: *mut SigAction) -> i32;
    fn setitimer(which: i32, new: *const ITimerVal, old: *mut ITimerVal) -> i32;
    fn _dyld_get_image_vmaddr_slide(index: u32) -> isize;
    fn atexit(cb: extern "C" fn()) -> i32;
}

/// `uc_mcontext` sits at byte 48 of macOS's `ucontext_t`
/// (`int uc_onstack; sigset_t uc_sigmask; stack_t uc_stack; ucontext* uc_link;
/// size_t uc_mcsize; mcontext_t uc_mcontext;`), and `__ss.__pc` at byte 272 of
/// `__darwin_mcontext64` (16 bytes of `__es`, then 29 x-registers, fp, lr, sp,
/// pc). aarch64 only — which is what `target_arch` gates the module on.
const UC_MCONTEXT_OFF: usize = 48;
const MCONTEXT_PC_OFF: usize = 272;

/// Open-addressed, power of two, never resized: the handler must not
/// allocate, so the table is sized once for a run's worth of distinct pcs and
/// a full run of 64 probes gives up and counts a loss instead.
const SLOTS: usize = 1 << 17;

struct Table {
    key: [AtomicU64; SLOTS],
    count: [AtomicU64; SLOTS],
}

static TABLE: Table = Table {
    key: [const { AtomicU64::new(0) }; SLOTS],
    count: [const { AtomicU64::new(0) }; SLOTS],
};
static TOTAL: AtomicU64 = AtomicU64::new(0);
static LOST: AtomicU64 = AtomicU64::new(0);
static OUT_PATH: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

extern "C" fn on_sigprof(_sig: i32, _info: *mut c_void, ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: the kernel hands a `ucontext_t *`; both offsets are fixed by the
    // macOS arm64 ABI and the null check below covers a context with no
    // machine state attached.
    let pc = unsafe {
        let mc = *(ctx.cast::<u8>().add(UC_MCONTEXT_OFF).cast::<*const u8>());
        if mc.is_null() {
            return;
        }
        *(mc.add(MCONTEXT_PC_OFF).cast::<u64>())
    };
    TOTAL.fetch_add(1, Ordering::Relaxed);
    let mut i = (mix(pc) as usize) & (SLOTS - 1);
    for _ in 0..64 {
        let k = TABLE.key[i].load(Ordering::Relaxed);
        if k == pc {
            TABLE.count[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
        if k == 0
            && TABLE.key[i]
                .compare_exchange(0, pc, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            TABLE.count[i].fetch_add(1, Ordering::Relaxed);
            return;
        }
        i = (i + 1) & (SLOTS - 1);
    }
    LOST.fetch_add(1, Ordering::Relaxed);
}

extern "C" fn dump() {
    let p = OUT_PATH.load(Ordering::Relaxed);
    if p == 0 {
        return;
    }
    // SAFETY: leaked at install time and never freed.
    let path: &str = unsafe { &*(p as *const String) };
    // SAFETY: a plain dyld query, no arguments of ours.
    let slide = unsafe { _dyld_get_image_vmaddr_slide(0) };
    let mut out = String::with_capacity(1 << 20);
    out.push_str(&format!("# slide {slide}\n"));
    out.push_str(&format!(
        "# total {} lost {}\n",
        TOTAL.load(Ordering::Relaxed),
        LOST.load(Ordering::Relaxed)
    ));
    for i in 0..SLOTS {
        let k = TABLE.key[i].load(Ordering::Relaxed);
        if k == 0 {
            continue;
        }
        let c = TABLE.count[i].load(Ordering::Relaxed);
        // Runtime address, then the static one `llvm-symbolizer --obj=<bin>`
        // and `atos -o <bin>` want, then the sample count.
        out.push_str(&format!(
            "{:#x} {:#x} {c}\n",
            k,
            (k as i64).wrapping_sub(slide as i64),
        ));
    }
    let _ = std::fs::write(path, out);
}

/// Install the sampler if `LP_EMU_SELFPROF` names an output path.
///
/// A no-op otherwise, which is what lets the bin call it unconditionally in a
/// `selfprof` build and lets that build still be A/B'd against a stock one.
pub fn install() {
    let Ok(path) = std::env::var("LP_EMU_SELFPROF") else {
        return;
    };
    let period_us: i32 = std::env::var("LP_EMU_SELFPROF_US")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    OUT_PATH.store(
        Box::leak(Box::new(path)) as *mut String as usize,
        Ordering::Relaxed,
    );
    let act = SigAction {
        handler: on_sigprof as *const () as usize,
        mask: 0,
        flags: SA_SIGINFO | SA_RESTART,
    };
    // SAFETY: a plain sigaction install with a signal-safe handler (atomics
    // into a static table, no allocation), then an interval timer and an
    // `atexit` hook that runs on the main thread after the run is over.
    unsafe {
        if sigaction(SIGPROF, &act, core::ptr::null_mut()) != 0 {
            eprintln!("selfprof: sigaction failed");
            return;
        }
        let it = ITimerVal {
            interval: TimeVal {
                sec: 0,
                usec: period_us,
                _pad: 0,
            },
            value: TimeVal {
                sec: 0,
                usec: period_us,
                _pad: 0,
            },
        };
        if setitimer(ITIMER_PROF, &it, core::ptr::null_mut()) != 0 {
            eprintln!("selfprof: setitimer failed");
            return;
        }
        atexit(dump);
    }
    eprintln!("selfprof: sampling every {period_us} us");
}
