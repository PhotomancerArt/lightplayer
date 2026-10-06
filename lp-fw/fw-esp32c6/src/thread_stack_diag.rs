//! A side thread's stack high-water (`io_thread_stack_diag`,
//! `net_thread_stack_diag`; off by default, never shipped).
//!
//! The link thread ([`crate::io_thread`], `lp-io`) and the network thread
//! (`crate::net::net_thread`, `lp-net`) each run on a fixed stack that
//! esp-rtos allocates off the heap and does not report. A
//! [`ThreadStackDiag`] paints one when its thread starts and logs its
//! high-water (`[iostack]`, `[netstack]`) at the heartbeat's cadence, when it
//! grows — the measurement each stack's size rests on. Interrupts land on
//! whatever stack is running, so the figure includes any ISR frames that hit
//! the thread.
//!
//! **Diagnostic only.** esp-rtos keeps a thread's stack range in its task
//! record, which is crate-private and not `repr(C)`; [`find_stack`] scans
//! that record for a (pointer, length) pair that holds the live `sp`. That
//! guess at another crate's private layout is acceptable for a desk build
//! and nowhere else, so nothing in a product image calls this. An upstream
//! ask (public thread creation, and a way to learn a thread's stack range)
//! would retire the scan.

use core::sync::atomic::{AtomicUsize, Ordering};

/// The pattern painted over the unused stack.
const PATTERN: u32 = 0xA5A5_5A5A;
/// Left unpainted below the entry `sp` (the painter's own frames).
const PAINT_MARGIN: usize = 512;
/// Left unpainted above the stack bottom (esp-rtos's guard word sits 60 B
/// up, under a hardware watchpoint while this thread runs).
const GUARD_SKIP: usize = 128;

/// One thread's painted stack.
pub struct ThreadStackDiag {
    /// The log line's tag (`iostack`, `netstack`).
    tag: &'static str,
    /// The thread's name, for the boot line.
    thread: &'static str,
    /// The stack size the thread asked for.
    requested: usize,
    paint_lo: AtomicUsize,
    stack_top: AtomicUsize,
    stack_len: AtomicUsize,
    reported: AtomicUsize,
}

impl ThreadStackDiag {
    pub const fn new(tag: &'static str, thread: &'static str, requested: usize) -> Self {
        Self {
            tag,
            thread,
            requested,
            paint_lo: AtomicUsize::new(0),
            stack_top: AtomicUsize::new(0),
            stack_len: AtomicUsize::new(0),
            reported: AtomicUsize::new(0),
        }
    }

    /// Log the thread's high-water mark when it has grown (the heartbeat's
    /// cadence, beside the main stack's).
    pub fn log_if_grown(&self) {
        let Some((used, len)) = self.high_water() else {
            return;
        };
        if used > self.reported.load(Ordering::Relaxed) {
            self.reported.store(used, Ordering::Relaxed);
            log::info!(
                "[{}] high-water {used} B of {len} B ({} B headroom; requested {} B)",
                self.tag,
                len.saturating_sub(used),
                self.requested
            );
        }
    }

    /// Find the calling thread's stack and paint everything below the live
    /// frames. Called first thing on the thread.
    pub fn paint(&self) {
        let sp = current_sp();
        let (bottom, top) = match find_stack(sp) {
            Some(range) => range,
            // Not found in the task record: assume the requested size below
            // a top just above `sp` (conservative: paints less).
            None => (sp + 256 - self.requested, sp + 64),
        };
        let lo = (bottom + GUARD_SKIP + 3) & !3;
        critical_section::with(|_| {
            let hi = current_sp().saturating_sub(PAINT_MARGIN);
            let mut addr = lo;
            while addr + 4 <= hi {
                // SAFETY: `lo..hi` is this thread's stack below its live
                // frames (by `PAINT_MARGIN`), above its guard word, and
                // interrupts are masked so nothing pushes into it meanwhile.
                unsafe { (addr as *mut u32).write_volatile(PATTERN) };
                addr += 4;
            }
        });
        self.stack_len.store(top - bottom, Ordering::Relaxed);
        self.stack_top.store(top, Ordering::Release);
        self.paint_lo.store(lo, Ordering::Release);
        esp_println::println!(
            "[INIT] {} thread up: stack {:#x}..{:#x} ({} B), entry sp {:#x}",
            self.thread,
            bottom,
            top,
            top - bottom,
            sp
        );
    }

    /// Bytes of the thread's stack ever used (the distance from its top to
    /// the lowest painted word that is gone), and the stack's length; `None`
    /// before the thread has painted.
    fn high_water(&self) -> Option<(usize, usize)> {
        let lo = self.paint_lo.load(Ordering::Acquire);
        let top = self.stack_top.load(Ordering::Acquire);
        if lo == 0 || top == 0 {
            return None;
        }
        let mut addr = lo;
        while addr + 4 <= top {
            // SAFETY: reading this thread's stack, which outlives the boot; a
            // torn read of a live frame fails the pattern test, the
            // conservative answer.
            if unsafe { (addr as *const u32).read_volatile() } != PATTERN {
                break;
            }
            addr += 4;
        }
        Some((top - addr, self.stack_len.load(Ordering::Relaxed)))
    }
}

/// The calling thread's stack range, read out of esp-rtos's task record:
/// its `stack: *mut [MaybeUninit<u32>]` is a (pointer, word count) pair
/// whose range holds `sp`. The record is crate-private and not `repr(C)`,
/// so this scans it for such a pair rather than assuming an offset (see the
/// module docs: diagnostic only).
fn find_stack(sp: usize) -> Option<(usize, usize)> {
    let task = esp_radio_rtos_driver::current_task().as_ptr() as *const usize;
    for i in 0..96 {
        // SAFETY: reads only. The record is a live heap object; a scan that
        // ran past its end would read other internal RAM (no MPU on this
        // chip, no fault) and the range check rejects what it finds there.
        let (p, n) = unsafe { (task.add(i).read_volatile(), task.add(i + 1).read_volatile()) };
        let len = n.wrapping_mul(4);
        if p != 0
            && p % 16 == 0
            && (1024..=65536).contains(&len)
            && p <= sp
            && sp < p + len
            && p + len - sp < 1024
        {
            return Some((p, p + len));
        }
    }
    None
}

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reads the stack pointer register; no memory is touched.
    unsafe { core::arch::asm!("mv {0}, sp", out(reg) sp) };
    sp
}
