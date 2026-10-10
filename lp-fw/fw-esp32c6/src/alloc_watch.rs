//! RESEARCH diagnostic (branch `research/frag-reads`; **never shipped**):
//! what a window of work allocates, and the heap's shape around it.
//!
//! Feature `alloc_watch_diag`. Built for the investigation
//! `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`: which allocations a
//! project read makes on the C6, how big the largest one is, and how the
//! heap's largest free block compares with its total free — on the emulated
//! C6 and on silicon, from the same image.
//!
//! - `lp-perf`'s `hook` sink forwards every marker here; `project-read`,
//!   `shader-compile`, `project-load` open a watched window (nested windows
//!   all see an allocation).
//! - esp-alloc's `alloc-hooks` report every allocation: each open window
//!   counts it, keeps its [`TOP`] largest with a frame-pointer backtrace, and
//!   tracks the peak of `HEAP.used()` above the window's opening level.
//! - At a window's close its record goes into a small ring; the heartbeat
//!   drains the ring as `[allocwatch]` log lines (the log ring, so the lines
//!   ride the link's log channel on silicon and the console on the emulator).
//!   `frame` windows are not logged one by one: the heaviest since the last
//!   drain is.
//! - Each heartbeat also logs the heap's free holes >= 512 B (the
//!   `heap_map` technique: allocate the largest block that fits until none
//!   does, then free them all) — `[allocwatch] holes`.

use core::cell::RefCell;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use critical_section::Mutex;

const TOP: usize = 5;
const FRAMES: usize = 6;
/// Frames to skip: the hook itself and `EspHeap::alloc_caps`.
const SKIP: usize = 2;
const OPEN: usize = 3;
const DONE: usize = 24;

#[derive(Clone, Copy, Default)]
struct Big {
    size: u32,
    frames: [u32; FRAMES],
}

#[derive(Clone, Copy, Default)]
struct Win {
    kind: u8,
    seq: u32,
    t_ms: u32,
    used0: u32,
    free0: u32,
    lfb0: u32,
    lfb1: u32,
    peak_above: u32,
    count: u32,
    bytes: u32,
    top: [Big; TOP],
}

#[allow(dead_code)]
const KINDS: [&str; 5] = [
    "?",
    "project-read",
    "shader-compile",
    "project-load",
    "frame",
];

struct State {
    open: [Option<Win>; OPEN],
    done: [Win; DONE],
    done_len: usize,
    dropped: u32,
    worst_frame: Option<Win>,
    frames: u32,
}

static STATE: Mutex<RefCell<State>> = Mutex::new(RefCell::new(State {
    open: [None; OPEN],
    done: [Win {
        kind: 0,
        seq: 0,
        t_ms: 0,
        used0: 0,
        free0: 0,
        lfb0: 0,
        lfb1: 0,
        peak_above: 0,
        count: 0,
        bytes: 0,
        top: [Big {
            size: 0,
            frames: [0; FRAMES],
        }; TOP],
    }; DONE],
    done_len: 0,
    dropped: 0,
    worst_frame: None,
    frames: 0,
}));
/// Allocations >= [`RET_MIN`] made while a window is open, until freed: at a
/// window's close the survivors are what it left behind (and where).
const RET_MIN: u32 = 256;
const RET_SLOTS: usize = 160;
/// Allocations at least this big are tracked while live, in or out of a window.
const BIG: u32 = 1024;
#[derive(Clone, Copy)]
struct Ret {
    addr: u32,
    size: u32,
    mask: u8,
    frames: [u32; 3],
}
static RET: Mutex<RefCell<[Ret; RET_SLOTS]>> = Mutex::new(RefCell::new(
    [Ret {
        addr: 0,
        size: 0,
        mask: 0,
        frames: [0; 3],
    }; RET_SLOTS],
));
static PAUSED: AtomicBool = AtomicBool::new(false);
static ANY_OPEN: AtomicBool = AtomicBool::new(false);
static SEQ: [AtomicU32; 5] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

/// Install the marker hook. Call once, early.
pub fn install() {
    lp_perf::set_hook(on_marker);
}

fn kind_of(name: &str) -> u8 {
    match name {
        lp_perf::EVENT_PROJECT_READ => 1,
        lp_perf::EVENT_SHADER_COMPILE => 2,
        lp_perf::EVENT_PROJECT_LOAD => 3,
        lp_perf::EVENT_FRAME => 4,
        _ => 0,
    }
}

fn now_ms() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}

fn on_marker(name: &'static str, kind: lp_perf::PerfEventKind) {
    let k = kind_of(name);
    if k == 0 {
        return;
    }
    match kind {
        lp_perf::PerfEventKind::Begin => {
            // Frames are cheap to open (no probe): only the reads, compiles
            // and loads pay for a largest-free-block search.
            let lfb0 = if k == 4 { 0 } else { probe_largest() };
            let seq = SEQ[k as usize].fetch_add(1, Ordering::Relaxed);
            let win = Win {
                kind: k,
                seq,
                t_ms: now_ms(),
                used0: esp_alloc::HEAP.used() as u32,
                free0: esp_alloc::HEAP.free() as u32,
                lfb0,
                ..Win::default()
            };
            critical_section::with(|cs| {
                let mut st = STATE.borrow_ref_mut(cs);
                if let Some(slot) = st.open.iter_mut().find(|w| w.is_none()) {
                    *slot = Some(win);
                }
            });
            ANY_OPEN.store(true, Ordering::Relaxed);
        }
        lp_perf::PerfEventKind::End => {
            let closing = critical_section::with(|cs| {
                let mut st = STATE.borrow_ref_mut(cs);
                let idx = st
                    .open
                    .iter()
                    .rposition(|w| w.is_some_and(|w| w.kind == k))?;
                let win = st.open[idx].take();
                let any = st.open.iter().any(Option::is_some);
                ANY_OPEN.store(any, Ordering::Relaxed);
                win
            });
            let Some(mut win) = closing else {
                return;
            };
            if k != 4 {
                win.lfb1 = probe_largest();
            }
            if k != 4 {
                // Reads, compiles and loads close on the server thread:
                // log them now, while the log ring has room for them.
                log_window(&win, 0);
                log_retained(k, win.seq, k != 1);
                if k != 1 {
                    log_holes();
                }
                return;
            }
            // A heavy frame (a reload, a compile inside it) says what it kept.
            let heavy = win.peak_above > 16 * 1024;
            if heavy {
                log_window(&win, 0);
            }
            log_retained(4, win.seq, heavy);
            if heavy {
                log_holes();
            }
            critical_section::with(|cs| {
                let mut st = STATE.borrow_ref_mut(cs);
                st.frames += 1;
                let heavier = st.worst_frame.is_none_or(|w| win.peak_above > w.peak_above);
                if heavier {
                    st.worst_frame = Some(win);
                }
            });
        }
        lp_perf::PerfEventKind::Instant => {}
    }
}

#[unsafe(no_mangle)]
fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: enumset::EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    if ptr == 0 || PAUSED.load(Ordering::Relaxed) {
        return;
    }
    if !ANY_OPEN.load(Ordering::Relaxed) {
        if size >= BIG as usize {
            let mut raw = [0u32; FRAMES + SKIP];
            lpc_shared::backtrace::capture_frames(&mut raw);
            critical_section::with(|cs| {
                let mut ret = RET.borrow_ref_mut(cs);
                if let Some(slot) = ret.iter_mut().find(|r| r.addr == 0) {
                    *slot = Ret {
                        addr: ptr as u32,
                        size: size as u32,
                        mask: 0x80,
                        frames: [raw[SKIP + 1], raw[SKIP + 2], raw[SKIP + 3]],
                    };
                }
            });
        }
        return;
    }
    let used = esp_alloc::HEAP.used() as u32;
    let mut raw = [0u32; FRAMES + SKIP];
    lpc_shared::backtrace::capture_frames(&mut raw);
    let mut frames = [0u32; FRAMES];
    frames.copy_from_slice(&raw[SKIP..]);
    let size = size as u32;
    critical_section::with(|cs| {
        let mut st = STATE.borrow_ref_mut(cs);
        if size >= RET_MIN {
            let mask = st
                .open
                .iter()
                .flatten()
                .fold(0u8, |m, w| m | (1u8 << w.kind))
                | if size >= BIG { 0x80 } else { 0 };
            let mut ret = RET.borrow_ref_mut(cs);
            if let Some(slot) = ret.iter_mut().find(|r| r.addr == 0) {
                *slot = Ret {
                    addr: ptr as u32,
                    size,
                    mask,
                    frames: [frames[1], frames[2], frames[3]],
                };
            }
        }
        for win in st.open.iter_mut().flatten() {
            win.count += 1;
            win.bytes = win.bytes.saturating_add(size);
            win.peak_above = win.peak_above.max(used.saturating_sub(win.used0));
            // Keep the TOP largest, smallest last.
            if size > win.top[TOP - 1].size {
                let mut i = TOP - 1;
                while i > 0 && win.top[i - 1].size < size {
                    win.top[i] = win.top[i - 1];
                    i -= 1;
                }
                win.top[i] = Big { size, frames };
            }
        }
    });
}

#[unsafe(no_mangle)]
fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, _size: usize) {
    if ptr == 0 || PAUSED.load(Ordering::Relaxed) {
        return;
    }
    critical_section::with(|cs| {
        let mut ret = RET.borrow_ref_mut(cs);
        if let Some(slot) = ret.iter_mut().find(|r| r.addr == ptr as u32) {
            slot.addr = 0;
        }
    });
}

/// Log (when `print`) and forget the survivors of window kind `k`.
fn log_retained(k: u8, seq: u32, print: bool) {
    const SHORT: [&str; 5] = ["?", "rd", "cc", "ld", "fr"];
    let regions = crate::board::esp32c6::init::heap_regions();
    let mut line: heapless::String<200> = heapless::String::new();
    let _ = write!(line, "[aw]  {}#{} kept", SHORT[k as usize], seq);
    let mut any = false;
    for i in 0..RET_SLOTS {
        let r = critical_section::with(|cs| {
            let mut ret = RET.borrow_ref_mut(cs);
            let r = ret[i];
            if r.addr != 0 && r.mask & (1 << k) != 0 {
                ret[i].mask &= !(1 << k);
                if ret[i].mask & 0x7f == 0 && ret[i].mask & 0x80 == 0 {
                    // No open window wants it any more.
                    ret[i].addr = 0;
                }
                Some(r)
            } else {
                None
            }
        });
        let Some(r) = r else { continue };
        if !print {
            continue;
        }
        any = true;
        let addr = r.addr as usize;
        let (ri, off) = regions
            .iter()
            .enumerate()
            .find(|(_, (s, l))| addr >= *s && addr < s + l)
            .map(|(i, (s, _))| (i, addr - s))
            .unwrap_or((9, addr));
        if line.len() > 140 {
            log::info!("{line}");
            line.clear();
            let _ = write!(line, "[aw]  {}#{} kept+", SHORT[k as usize], seq);
        }
        let _ = write!(
            line,
            " r{ri}+{off}:{}@{:x}/{:x}",
            r.size, r.frames[0], r.frames[1]
        );
    }
    if print && any {
        log::info!("{line}");
    }
}

/// Drain finished windows as log lines. Called from the heartbeat.
pub fn drain() {
    loop {
        let item = critical_section::with(|cs| {
            let mut st = STATE.borrow_ref_mut(cs);
            if st.done_len > 0 {
                let w = st.done[0];
                let n = st.done_len;
                st.done.copy_within(1..n, 0);
                st.done_len -= 1;
                Some((w, 0u32))
            } else if let Some(w) = st.worst_frame.take() {
                let f = st.frames;
                st.frames = 0;
                Some((w, f))
            } else {
                None
            }
        });
        let Some((w, frames)) = item else {
            break;
        };
        log_window(&w, frames);
    }
    let dropped =
        critical_section::with(|cs| core::mem::take(&mut STATE.borrow_ref_mut(cs).dropped));
    if dropped > 0 {
        log::info!("[aw] {dropped} window records dropped (ring full)");
    }
    log_holes();
}

fn log_window(w: &Win, frames: u32) {
    const SHORT: [&str; 5] = ["?", "rd", "cc", "ld", "fr"];
    log::info!(
        "[aw] {}#{} t={} u0={} f0={} l0={} l1={} pk={} n={} b={} of={}",
        SHORT[w.kind as usize],
        w.seq,
        w.t_ms,
        w.used0,
        w.free0,
        w.lfb0,
        w.lfb1,
        w.peak_above,
        w.count,
        w.bytes,
        frames
    );
    let mut line: heapless::String<200> = heapless::String::new();
    let _ = write!(line, "[aw]  {}#{} top", SHORT[w.kind as usize], w.seq);
    for b in w.top.iter().take(4) {
        if b.size == 0 {
            break;
        }
        let f = b.frames;
        let _ = write!(line, " {}@{:x}/{:x}/{:x}", b.size, f[1], f[2], f[3]);
    }
    log::info!("{line}");
}

/// Holes >= 512 B, by address, as one or two lines.
fn log_holes() {
    const MAX: usize = 24;
    const MIN: usize = 1024;
    let mut holes = [(0usize, 0usize); MAX];
    let used = esp_alloc::HEAP.used();
    let free = esp_alloc::HEAP.free();
    let n = critical_section::with(|_| take_holes(&mut holes, MIN));
    let holes = &mut holes[..n];
    holes.sort_unstable_by_key(|(a, _)| *a);
    let sum: usize = holes.iter().map(|(_, s)| *s).sum();
    let mut line: heapless::String<200> = heapless::String::new();
    let _ = write!(line, "[aw] holes u={used} f={free} h>={MIN}={sum} n={n}:");
    let regions = crate::board::esp32c6::init::heap_regions();
    for (addr, size) in holes.iter() {
        let (ri, off) = regions
            .iter()
            .enumerate()
            .find(|(_, (s, l))| *addr >= *s && *addr < s + l)
            .map(|(i, (s, _))| (i, addr - s))
            .unwrap_or((9, *addr));
        if line.len() > 140 {
            log::info!("{line}");
            line.clear();
            let _ = write!(line, "[aw] holes+");
        }
        let _ = write!(line, " r{ri}+{off}:{size}");
    }
    log::info!("{line}");
    log_big_live();
}

/// Live allocations >= [`BIG`] at r0 offset >= 150,000 or in r1: what sits in
/// the region's tail, where the largest free block lives.
fn log_big_live() {
    let regions = crate::board::esp32c6::init::heap_regions();
    let mut line: heapless::String<200> = heapless::String::new();
    let _ = write!(line, "[aw] big");
    let mut any = false;
    for i in 0..RET_SLOTS {
        let r = critical_section::with(|cs| RET.borrow_ref(cs)[i]);
        if r.addr == 0 || r.mask & 0x80 == 0 {
            continue;
        }
        let addr = r.addr as usize;
        let Some((ri, off)) = regions
            .iter()
            .enumerate()
            .find(|(_, (s, l))| addr >= *s && addr < s + l)
            .map(|(i, (s, _))| (i, addr - s))
        else {
            continue;
        };
        if ri == 0 && off < 150_000 {
            continue;
        }
        any = true;
        if line.len() > 140 {
            log::info!("{line}");
            line.clear();
            let _ = write!(line, "[aw] big+");
        }
        let _ = write!(
            line,
            " r{ri}+{off}:{}@{:x}/{:x}/{:x}",
            r.size, r.frames[0], r.frames[1], r.frames[2]
        );
    }
    if any {
        log::info!("{line}");
    }
}

fn take_holes(out: &mut [(usize, usize)], min: usize) -> usize {
    PAUSED.store(true, Ordering::Relaxed);
    let mut n = 0;
    while n < out.len() {
        let size = largest_fit();
        if size < min {
            break;
        }
        let Ok(layout) = core::alloc::Layout::from_size_align(size, 4) else {
            break;
        };
        // SAFETY: size > 0; freed below with the same layout.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            break;
        }
        out[n] = (ptr as usize, size);
        n += 1;
    }
    for (addr, size) in out[..n].iter().rev() {
        // SAFETY: allocated above with exactly this layout.
        unsafe {
            alloc::alloc::dealloc(
                *addr as *mut u8,
                core::alloc::Layout::from_size_align_unchecked(*size, 4),
            )
        };
    }
    PAUSED.store(false, Ordering::Relaxed);
    n
}

fn probe_largest() -> u32 {
    PAUSED.store(true, Ordering::Relaxed);
    let v = critical_section::with(|_| largest_fit());
    PAUSED.store(false, Ordering::Relaxed);
    v as u32
}

/// The largest 4-aligned allocation that succeeds, to 4 B.
fn largest_fit() -> usize {
    let mut too_big = esp_alloc::HEAP.free() + 1;
    let mut fits = 0usize;
    while too_big - fits > 4 {
        let mid = fits + (too_big - fits) / 2;
        let Ok(layout) = core::alloc::Layout::from_size_align(mid, 4) else {
            break;
        };
        // SAFETY: mid > 0; freed at once with the same layout.
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
