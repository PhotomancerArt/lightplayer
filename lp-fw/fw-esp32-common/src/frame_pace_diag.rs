//! Frame pacing, measured on the board (`frame-pace-diag`, OFF by default,
//! never shipped).
//!
//! Answers "do the LEDs get choppy while a host is talking to the board?" with
//! the numbers that word means, not with the average frame rate:
//!
//! - **interval** — the time between two frames reaching the output driver
//!   (the chip calls [`frame_emitted`] when a frame's first wire is handed
//!   off). This is what the eye sees.
//! - **offset** — from the loop reading the frame's clock (the `delta_ms`
//!   the frame is rendered at) to that frame reaching the driver. A frame
//!   shows its pattern time `offset` late; a varying offset is motion that
//!   stutters even at a steady frame rate.
//! - **judder** — how much the offset moved from one frame to the next,
//!   which is `|interval − delta|`: how far the picture's clock moved against
//!   how long the previous picture was on the LEDs.
//! - **tick** — the server's whole tick (requests and render), split by
//!   whether the frame answered anything, so the cost of a request is
//!   `busy − idle`.
//! - **link passes** — how often the link task ran a pass and how long the
//!   passes took (the classic's UART link task calls [`link_pass`]), and the
//!   share of wall time they took (`link_busy`, per mille).
//!
//! One `[pace]` line every [`PERIOD_US`], from the server loop. Times are
//! the board's own clock (`embassy_time`), printed as ms.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::Mutex;

/// How often the line is printed, µs.
pub const PERIOD_US: u64 = 5_000_000;
/// Frames kept per window for the percentiles (33 fps × 5 s fits).
const CAP: usize = 256;

struct Window {
    interval: [u32; CAP],
    offset: [u32; CAP],
    judder: [u32; CAP],
    n: usize,
    dropped: u32,
    tick_idle_sum: u64,
    tick_idle_n: u32,
    tick_busy_sum: u64,
    tick_busy_n: u32,
    tick_busy_max: u32,
    responses: u32,
}

struct Frame {
    started_us: u64,
    emitted_us: Option<u64>,
    last_emitted_us: Option<u64>,
    last_offset_us: Option<u64>,
    window_started_us: u64,
}

/// On the heap, not in a static: on the classic the main stack is whatever
/// DRAM the statics leave, and 3 KB of window here once cost it the headroom
/// Studio's editor read needs (the diagnostic build overflowed; the product
/// does not).
static WINDOW: Mutex<RefCell<Option<alloc::boxed::Box<Window>>>> = Mutex::new(RefCell::new(None));

static FRAME: Mutex<RefCell<Frame>> = Mutex::new(RefCell::new(Frame {
    started_us: 0,
    emitted_us: None,
    last_emitted_us: None,
    last_offset_us: None,
    window_started_us: 0,
}));

static PASSES: AtomicU32 = AtomicU32::new(0);
static PASS_US_SUM: AtomicU32 = AtomicU32::new(0);
static PASS_US_MAX: AtomicU32 = AtomicU32::new(0);

/// The board's clock, µs.
pub fn now_us() -> u64 {
    embassy_time::Instant::now().as_micros()
}

/// The server loop read the frame's clock. Call at the top of each loop pass.
pub fn frame_start() {
    let now = now_us();
    critical_section::with(|cs| {
        let mut frame = FRAME.borrow(cs).borrow_mut();
        if frame.window_started_us == 0 {
            frame.window_started_us = now;
        }
        frame.started_us = now;
        frame.emitted_us = None;
    });
}

/// A frame reached the output driver (the chip's first wire of the frame).
/// Only the first call per frame counts.
pub fn frame_emitted() {
    let now = now_us();
    critical_section::with(|cs| {
        let mut frame = FRAME.borrow(cs).borrow_mut();
        if frame.emitted_us.is_none() {
            frame.emitted_us = Some(now);
        }
    });
}

/// One link-task pass took `us`.
pub fn link_pass(us: u32) {
    PASSES.fetch_add(1, Ordering::Relaxed);
    PASS_US_SUM.fetch_add(us, Ordering::Relaxed);
    PASS_US_MAX.fetch_max(us, Ordering::Relaxed);
}

/// The tick is done: `tick_us` long, `responses` answered. Prints the
/// window's line when it is due.
pub fn frame_end(tick_us: u64, responses: usize) {
    let now = now_us();
    let line = critical_section::with(|cs| {
        let mut frame = FRAME.borrow(cs).borrow_mut();
        let mut slot = WINDOW.borrow(cs).borrow_mut();
        let window = slot.get_or_insert_with(|| {
            alloc::boxed::Box::new(Window {
                interval: [0; CAP],
                offset: [0; CAP],
                judder: [0; CAP],
                n: 0,
                dropped: 0,
                tick_idle_sum: 0,
                tick_idle_n: 0,
                tick_busy_sum: 0,
                tick_busy_n: 0,
                tick_busy_max: 0,
                responses: 0,
            })
        });
        record(&mut frame, window, tick_us, responses);
        if now.saturating_sub(frame.window_started_us) < PERIOD_US {
            return None;
        }
        let elapsed_us = now.saturating_sub(frame.window_started_us);
        frame.window_started_us = now;
        Some(take_line(window, elapsed_us))
    });
    if let Some(line) = line {
        line.print();
    }
}

fn record(frame: &mut Frame, window: &mut Window, tick_us: u64, responses: usize) {
    let tick = clamp(tick_us);
    if responses > 0 {
        window.tick_busy_sum += u64::from(tick);
        window.tick_busy_n += 1;
        window.tick_busy_max = window.tick_busy_max.max(tick);
        window.responses += responses as u32;
    } else {
        window.tick_idle_sum += u64::from(tick);
        window.tick_idle_n += 1;
    }
    let Some(emitted) = frame.emitted_us else {
        return;
    };
    let offset = emitted.saturating_sub(frame.started_us);
    if let (Some(last_emitted), Some(last_offset)) = (frame.last_emitted_us, frame.last_offset_us) {
        let n = window.n;
        if n < CAP {
            window.interval[n] = clamp(emitted.saturating_sub(last_emitted));
            window.offset[n] = clamp(offset);
            window.judder[n] = clamp(offset.abs_diff(last_offset));
            window.n += 1;
        } else {
            window.dropped += 1;
        }
    }
    frame.last_emitted_us = Some(emitted);
    frame.last_offset_us = Some(offset);
}

fn take_line(w: &mut Window, elapsed_us: u64) -> Line {
    let n = w.n;
    w.interval[..n].sort_unstable();
    w.offset[..n].sort_unstable();
    w.judder[..n].sort_unstable();
    let interval = Pct::of(&w.interval[..n]);
    let hitch_us = interval.p50 + interval.p50 / 2;
    let line = Line {
        elapsed_us,
        n,
        hitches: w.interval[..n].iter().filter(|&&us| us > hitch_us).count(),
        interval,
        offset: Pct::of(&w.offset[..n]),
        judder: Pct::of(&w.judder[..n]),
        tick_idle: mean(w.tick_idle_sum, w.tick_idle_n),
        tick_idle_n: w.tick_idle_n,
        tick_busy: mean(w.tick_busy_sum, w.tick_busy_n),
        tick_busy_n: w.tick_busy_n,
        tick_busy_max: w.tick_busy_max,
        responses: w.responses,
        passes: PASSES.swap(0, Ordering::Relaxed),
        pass_sum: PASS_US_SUM.swap(0, Ordering::Relaxed),
        pass_max: PASS_US_MAX.swap(0, Ordering::Relaxed),
    };
    w.n = 0;
    w.dropped = 0;
    w.tick_idle_sum = 0;
    w.tick_idle_n = 0;
    w.tick_busy_sum = 0;
    w.tick_busy_n = 0;
    w.tick_busy_max = 0;
    w.responses = 0;
    line
}

fn clamp(us: u64) -> u32 {
    us.min(u64::from(u32::MAX)) as u32
}

fn mean(sum: u64, n: u32) -> u32 {
    if n == 0 {
        0
    } else {
        (sum / u64::from(n)) as u32
    }
}

struct Pct {
    min: u32,
    p50: u32,
    p90: u32,
    p99: u32,
    max: u32,
}

impl Pct {
    fn of(sorted: &[u32]) -> Self {
        let at = |q: usize| -> u32 {
            if sorted.is_empty() {
                0
            } else {
                sorted[((sorted.len() - 1) * q) / 100]
            }
        };
        Self {
            min: sorted.first().copied().unwrap_or(0),
            p50: at(50),
            p90: at(90),
            p99: at(99),
            max: sorted.last().copied().unwrap_or(0),
        }
    }
}

struct Line {
    elapsed_us: u64,
    n: usize,
    /// Intervals longer than 1.5× the window's median: the frames an eye
    /// reads as a hitch.
    hitches: usize,
    interval: Pct,
    offset: Pct,
    judder: Pct,
    tick_idle: u32,
    tick_idle_n: u32,
    tick_busy: u32,
    tick_busy_n: u32,
    tick_busy_max: u32,
    responses: u32,
    passes: u32,
    pass_sum: u32,
    pass_max: u32,
}

/// µs as `ms.d`.
struct Ms(u32);

impl core::fmt::Display for Ms {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.0 / 1000, (self.0 % 1000) / 100)
    }
}

impl Line {
    /// Two lines, each short enough to survive the log record whole.
    #[inline(never)]
    fn print(&self) {
        let ms = (self.elapsed_us / 1000).max(1);
        let pass_mean = self.pass_sum.checked_div(self.passes).unwrap_or(0);
        log::info!(
            "[pace] n={} int={}/{}/{}/{}/{} hitch={} off={}/{}/{}/{} jud={}/{}/{}/{}",
            self.n,
            Ms(self.interval.min),
            Ms(self.interval.p50),
            Ms(self.interval.p90),
            Ms(self.interval.p99),
            Ms(self.interval.max),
            self.hitches,
            Ms(self.offset.min),
            Ms(self.offset.p50),
            Ms(self.offset.p90),
            Ms(self.offset.max),
            Ms(self.judder.p50),
            Ms(self.judder.p90),
            Ms(self.judder.p99),
            Ms(self.judder.max),
        );
        log::info!(
            "[pace] tick idle={}/{} busy={}/{} max={} resp={} pass/s={} mean={}us max={}us busy={}pm",
            Ms(self.tick_idle),
            self.tick_idle_n,
            Ms(self.tick_busy),
            self.tick_busy_n,
            Ms(self.tick_busy_max),
            self.responses,
            u64::from(self.passes) * 1000 / ms,
            pass_mean,
            self.pass_max,
            u64::from(self.pass_sum) / ms,
        );
    }
}
