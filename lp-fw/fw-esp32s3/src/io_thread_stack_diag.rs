//! The link thread's stack high-water (`io_thread_stack_diag`, off by
//! default, never shipped).
//!
//! [`crate::io_thread`] runs on a fixed [`STACK_BYTES`] stack that esp-rtos
//! allocates off the heap and does not report. This paints it when the
//! thread starts and logs its high-water (`[iostack]`) at the heartbeat's
//! cadence, when it grows — the measurement the 4 KB figure is sized from.
//! The S3's copy of the C6's diagnostic, with an Xtensa `sp` read (`a1`) and
//! a wider record scan (below).
//! Interrupts land on whatever stack is running, so the figure includes any
//! ISR frames that hit this thread.
//!
//! **Diagnostic only.** esp-rtos keeps a thread's stack range in its task
//! record, which is crate-private and not `repr(C)`; [`find_stack`] scans
//! that record for a (pointer, length) pair that holds the live `sp`. That
//! guess at another crate's private layout is acceptable for a desk build
//! and nowhere else, so nothing in a product image calls this. An upstream
//! ask (public thread creation, and a way to learn a thread's stack range)
//! would retire the scan.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::io_thread::STACK_BYTES;

/// The pattern painted over the unused stack.
const PATTERN: u32 = 0xA5A5_5A5A;
/// Left unpainted below the entry `sp` (the painter's own frames).
const PAINT_MARGIN: usize = 512;
/// Left unpainted above the stack bottom (esp-rtos's guard word sits 60 B
/// up — `ESP_HAL_CONFIG_STACK_GUARD_OFFSET`'s default — under a hardware
/// watchpoint while this thread runs).
const GUARD_SKIP: usize = 128;

/// The painted range and the stack's top, once the thread has started.
static PAINT_LO: AtomicUsize = AtomicUsize::new(0);
static STACK_TOP: AtomicUsize = AtomicUsize::new(0);
static STACK_LEN: AtomicUsize = AtomicUsize::new(0);
static HIGH_WATER_REPORTED: AtomicUsize = AtomicUsize::new(0);

/// Log the thread's high-water mark when it has grown (the heartbeat's
/// cadence, beside the main stack's).
pub fn log_if_grown() {
    let Some((used, len)) = high_water() else {
        return;
    };
    if used > HIGH_WATER_REPORTED.load(Ordering::Relaxed) {
        HIGH_WATER_REPORTED.store(used, Ordering::Relaxed);
        log::info!(
            "[iostack] high-water {used} B of {len} B ({} B headroom; requested {} B)",
            len.saturating_sub(used),
            STACK_BYTES
        );
    }
}

/// Find this thread's stack and paint everything below the live frames.
/// Called first thing on the link thread.
pub fn paint() {
    let sp = current_sp();
    let (bottom, top) = match find_stack(sp) {
        Some(range) => range,
        // Not found in the task record: assume the requested size below a
        // top just above `sp` (conservative: paints less).
        None => (sp + 256 - STACK_BYTES, sp + 64),
    };
    let lo = (bottom + GUARD_SKIP + 3) & !3;
    critical_section::with(|_| {
        let hi = current_sp().saturating_sub(PAINT_MARGIN);
        let mut addr = lo;
        while addr + 4 <= hi {
            // SAFETY: `lo..hi` is this thread's stack below its live frames
            // (by `PAINT_MARGIN`), above its guard word, and interrupts are
            // masked so nothing pushes into it meanwhile.
            unsafe { (addr as *mut u32).write_volatile(PATTERN) };
            addr += 4;
        }
    });
    STACK_LEN.store(top - bottom, Ordering::Relaxed);
    STACK_TOP.store(top, Ordering::Release);
    PAINT_LO.store(lo, Ordering::Release);
    esp_println::println!(
        "[INIT] io thread up: stack {:#x}..{:#x} ({} B), entry sp {:#x}",
        bottom,
        top,
        top - bottom,
        sp
    );
}

/// Bytes of the thread's stack ever used (the distance from its top to the
/// lowest painted word that is gone), and the stack's length; `None` before
/// the thread has painted.
fn high_water() -> Option<(usize, usize)> {
    let lo = PAINT_LO.load(Ordering::Acquire);
    let top = STACK_TOP.load(Ordering::Acquire);
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
    Some((top - addr, STACK_LEN.load(Ordering::Relaxed)))
}

/// The thread's stack range, read out of esp-rtos's task record: its
/// `stack: *mut [MaybeUninit<u32>]` is a (pointer, word count) pair whose
/// range holds `sp`. The record is crate-private and not `repr(C)`, so this
/// scans it for such a pair rather than assuming an offset (see the module
/// docs: diagnostic only). On Xtensa the record's saved context is esp-hal's
/// `TrapFrame` — the windowed register file's live half, the special
/// registers and, with `float-save-restore`, the FPU's — far larger than
/// RV32's, so the scan reaches [`SCAN_WORDS`] words rather than the C6's 96.
fn find_stack(sp: usize) -> Option<(usize, usize)> {
    let task = esp_radio_rtos_driver::current_task().as_ptr() as *const usize;
    for i in 0..SCAN_WORDS {
        // SAFETY: reads only. The record is a live heap object; a scan that
        // ran past its end would read other internal SRAM (this firmware
        // arms no memory protection, so no fault) and the range check
        // rejects what it finds there.
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

/// How far into the task record [`find_stack`] looks, in words.
const SCAN_WORDS: usize = 256;

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reads the stack pointer (`a1` in the windowed ABI); no memory
    // is touched.
    unsafe { core::arch::asm!("mov {0}, a1", out(reg) sp) };
    sp
}
