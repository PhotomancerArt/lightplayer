//! The radio tasks' stack high-water on silicon (`radio_stack_diag`, RAM
//! research E17; off by default, never shipped).
//!
//! The BLE controller task (asks 4,096 B), esp-radio's timer task (8,192 B,
//! a literal in esp-radio-rtos-driver) and the Wi-Fi task (6,656 B) run on
//! stacks esp-rtos allocates off the heap in `Task::new`: the asked size, plus
//! the stack guard's 4 + `ESP_HAL_CONFIG_STACK_GUARD_OFFSET` (60) bytes,
//! rounded down to 16 and plus 16 — 4,176, 8,272 and 6,736 B blocks. E13
//! measured how much of each the emulator touches idle; this is the same
//! question on a board, through a connection and a notify burst.
//!
//! esp-alloc's `alloc-hooks` report every allocation. A block of one of those
//! sizes, among the first [`SLOTS`] of them, is painted with [`PATTERN`] right
//! there — before `Task::new` writes its guard word near the bottom and long
//! before the task first runs (its context only records `sp`; nothing is
//! pushed). At the heartbeat each live slot is scanned from the bottom for
//! the first word that is no longer the pattern, and a `[radiostack]` line is
//! logged when that high-water grows. The `lp-net` thread also asks 8,192 B,
//! so an 8,272 B slot is the timer task or `lp-net`; `net_thread_stack_diag`
//! prints `lp-net`'s range, which tells them apart. Any other allocation of a
//! matching size is painted too (harmless: it is uninitialised memory its
//! owner is about to write) and shows up as a slot with no task's shape.
//!
//! The hooks live in RAM (`.rwtext`): esp-alloc runs from interrupts and with
//! the flash cache off.

use core::sync::atomic::{AtomicUsize, Ordering};

/// The pattern painted over a candidate stack.
const PATTERN: u32 = 0xA5A5_5A5A;
/// Left unpainted at the bottom: esp-rtos's guard word sits 60 B up.
const GUARD_SKIP: usize = 128;
/// The block sizes esp-rtos allocates for the three radio tasks.
const SIZES: [usize; 3] = [4176, 6736, 8272];
/// How many matching blocks to follow.
const SLOTS: usize = 8;

struct Slot {
    addr: AtomicUsize,
    size: AtomicUsize,
    reported: AtomicUsize,
}

#[allow(
    clippy::declare_interior_mutable_const,
    reason = "only the array initialiser below copies it; each slot is its own static atomic"
)]
const EMPTY: Slot = Slot {
    addr: AtomicUsize::new(0),
    size: AtomicUsize::new(0),
    reported: AtomicUsize::new(0),
};
static SLOT: [Slot; SLOTS] = [EMPTY; SLOTS];
/// Slots handed out so far (never reused: a slot's index is its birth order).
static NEXT: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
#[unsafe(link_section = ".rwtext")]
fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: enumset::EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    if ptr == 0 || !SIZES.contains(&size) {
        return;
    }
    let i = NEXT.fetch_add(1, Ordering::Relaxed);
    if i >= SLOTS {
        return;
    }
    let mut a = (ptr + GUARD_SKIP + 3) & !3;
    while a + 4 <= ptr + size {
        // SAFETY: `ptr..ptr+size` was just allocated to the caller, who has
        // not written it yet; the pattern is a value it will overwrite.
        unsafe { (a as *mut u32).write_volatile(PATTERN) };
        a += 4;
    }
    SLOT[i].size.store(size, Ordering::Relaxed);
    SLOT[i].addr.store(ptr, Ordering::Release);
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".rwtext")]
fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, _size: usize) {
    if ptr == 0 {
        return;
    }
    for s in &SLOT {
        if s.addr.load(Ordering::Relaxed) == ptr {
            s.addr.store(0, Ordering::Release);
        }
    }
}

/// Log each live slot's high-water when it has grown (the heartbeat's
/// cadence).
pub fn log_if_grown() {
    for (i, s) in SLOT.iter().enumerate() {
        let addr = s.addr.load(Ordering::Acquire);
        if addr == 0 {
            continue;
        }
        let size = s.size.load(Ordering::Relaxed);
        let top = addr + size;
        let mut a = (addr + GUARD_SKIP + 3) & !3;
        while a + 4 <= top {
            // SAFETY: reading a live heap block (a task's stack); a torn read
            // of a live frame fails the pattern test, the conservative answer.
            if unsafe { (a as *const u32).read_volatile() } != PATTERN {
                break;
            }
            a += 4;
        }
        let used = top - a;
        if used > s.reported.load(Ordering::Relaxed) {
            s.reported.store(used, Ordering::Relaxed);
            log::info!(
                "[radiostack] #{i} block {size} B @{addr:#x}..{top:#x}: high-water {used} B ({} B never touched)",
                size - used
            );
        }
    }
}
