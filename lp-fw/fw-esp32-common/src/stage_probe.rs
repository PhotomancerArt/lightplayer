//! SPIKE (core-1 spike): per-stage cycle counters for one frame's serial
//! work, printed beside `[perf]` as `[stage]`. Off unless a chip installs a
//! cycle counter ([`set_clock`]); then a handful of atomic adds per frame.
//! Not product code.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

static CLOCK: AtomicUsize = AtomicUsize::new(0);
static EXTRA: AtomicUsize = AtomicUsize::new(0);
static WINDOW_HOOK: AtomicUsize = AtomicUsize::new(0);

/// A chip's hook run once per `[perf]` window (the fps spike's PC sampler).
pub fn set_window_hook(hook: fn()) {
    WINDOW_HOOK.store(hook as usize, Ordering::Release);
}

/// Output provider `write` (pipeline + staging + the LCD kick when it fires).
pub static WRITE: AtomicU32 = AtomicU32::new(0);
/// The LCD bus's encode (inside `WRITE`).
pub static ENCODE: AtomicU32 = AtomicU32::new(0);
/// The engine's whole tick (everything but the link).
pub static TICK: AtomicU32 = AtomicU32::new(0);

/// SPIKE (fps spike): allocation calls, frees and bytes (a chip's
/// `alloc-hooks` feeds them; zero otherwise).
pub static ALLOCS: AtomicU32 = AtomicU32::new(0);
pub static FREES: AtomicU32 = AtomicU32::new(0);
pub static ALLOC_BYTES: AtomicU32 = AtomicU32::new(0);
pub static BIG: AtomicU32 = AtomicU32::new(0);
pub static BIG_LAST: AtomicU32 = AtomicU32::new(0);
static LAST_ALLOC: [AtomicU32; 3] = [const { AtomicU32::new(0) }; 3];

static LAST: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];

/// Install the chip's cycle counter.
pub fn set_clock(clock: fn() -> u32) {
    CLOCK.store(clock as usize, Ordering::Release);
}

/// A chip's extra cumulative cycle counter (the shader's sampling).
pub fn set_extra(extra: fn() -> u32) {
    EXTRA.store(extra as usize, Ordering::Release);
}

pub fn now() -> Option<u32> {
    let c = CLOCK.load(Ordering::Acquire);
    (c != 0).then(|| {
        // SAFETY: only `set_clock` stores here, and only a `fn() -> u32`.
        (unsafe { core::mem::transmute::<usize, fn() -> u32>(c) })()
    })
}

pub fn add(slot: &AtomicU32, started: Option<u32>) {
    if let (Some(t0), Some(t1)) = (started, now()) {
        slot.fetch_add(t1.wrapping_sub(t0), Ordering::Relaxed);
    }
}

/// Print per-frame averages (µs at 240 MHz) since the last call.
pub fn log(frames: u32) {
    let h = WINDOW_HOOK.load(Ordering::Acquire);
    if h != 0 {
        // SAFETY: only `set_window_hook` stores here, and only a `fn()`.
        (unsafe { core::mem::transmute::<usize, fn()>(h) })();
    }
    if CLOCK.load(Ordering::Relaxed) == 0 || frames == 0 {
        return;
    }
    let e = EXTRA.load(Ordering::Acquire);
    let sampling = if e == 0 {
        0
    } else {
        // SAFETY: only `set_extra` stores here, and only a `fn() -> u32`.
        (unsafe { core::mem::transmute::<usize, fn() -> u32>(e) })()
    };
    let now = [
        TICK.load(Ordering::Relaxed),
        sampling,
        WRITE.load(Ordering::Relaxed),
        ENCODE.load(Ordering::Relaxed),
    ];
    let mut per = [0u32; 4];
    for i in 0..4 {
        per[i] = now[i].wrapping_sub(LAST[i].swap(now[i], Ordering::Relaxed)) / frames / 240;
    }
    log::info!(
        "[stage] per frame us: tick={} sampling={} write={} (encode={}) rest={}",
        per[0],
        per[1],
        per[2],
        per[3],
        per[0].saturating_sub(per[1] + per[2])
    );
    let a = [
        ALLOCS.load(Ordering::Relaxed),
        FREES.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
    ];
    if a[0] != 0 {
        let mut d = [0u32; 3];
        for i in 0..3 {
            d[i] = a[i].wrapping_sub(LAST_ALLOC[i].swap(a[i], Ordering::Relaxed)) / frames;
        }
        log::info!(
            "[stage] per frame allocs={} frees={} bytes={} big_total={} big_last={}",
            d[0],
            d[1],
            d[2],
            BIG.load(Ordering::Relaxed),
            BIG_LAST.load(Ordering::Relaxed)
        );
    }
}
