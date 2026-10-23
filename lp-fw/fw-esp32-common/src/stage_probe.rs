//! SPIKE (core-1 spike): per-stage cycle counters for one frame's serial
//! work, printed beside `[perf]` as `[stage]`. Off unless a chip installs a
//! cycle counter ([`set_clock`]); then a handful of atomic adds per frame.
//! Not product code.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

static CLOCK: AtomicUsize = AtomicUsize::new(0);
static EXTRA: AtomicUsize = AtomicUsize::new(0);

/// Output provider `write` (pipeline + staging + the LCD kick when it fires).
pub static WRITE: AtomicU32 = AtomicU32::new(0);
/// The LCD bus's encode (inside `WRITE`).
pub static ENCODE: AtomicU32 = AtomicU32::new(0);
/// The engine's whole tick (everything but the link).
pub static TICK: AtomicU32 = AtomicU32::new(0);

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
}
