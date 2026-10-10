//! Who the borrower is, asked by the allocator on every allocation.
//!
//! A loan belongs to the embassy task that opened it, on the thread that
//! polls that task. A thread is its `tp` register (esp-rtos keeps the
//! running task's record there); the task being polled on each executor is
//! what embassy-executor's `trace` hooks report. So while a project read
//! awaits its send, the Bluetooth tasks the main executor polls meanwhile
//! are not the borrower, and the link thread never is.
//!
//! Everything here is atomics: [`_esp_alloc_lend_borrower`] runs inside the
//! heap's lock.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Executors tracked (main, the link thread, the net thread, spare).
const SLOTS: usize = 6;

#[allow(clippy::declare_interior_mutable_const)]
const ZERO_U32: AtomicU32 = AtomicU32::new(0);
#[allow(clippy::declare_interior_mutable_const)]
const ZERO_USIZE: AtomicUsize = AtomicUsize::new(0);

static EXECUTOR: [AtomicU32; SLOTS] = [ZERO_U32; SLOTS];
static POLLED_TASK: [AtomicU32; SLOTS] = [ZERO_U32; SLOTS];
static POLLING_THREAD: [AtomicUsize; SLOTS] = [ZERO_USIZE; SLOTS];

static OPEN: AtomicBool = AtomicBool::new(false);
static HOLDER_SLOT: AtomicUsize = AtomicUsize::new(0);
static HOLDER_TASK: AtomicU32 = AtomicU32::new(0);
static HOLDER_THREAD: AtomicUsize = AtomicUsize::new(0);
/// Loans opened outside any polled task (the thread alone is the borrower).
static THREAD_ONLY_OPENS: AtomicU32 = AtomicU32::new(0);

/// Make the running embassy task, on this thread, the borrower.
pub fn open() {
    let thread = thread();
    let mut found = None;
    for slot in 0..SLOTS {
        if POLLING_THREAD[slot].load(Ordering::Relaxed) == thread {
            let task = POLLED_TASK[slot].load(Ordering::Relaxed);
            if task != 0 {
                found = Some((slot, task));
                break;
            }
        }
    }
    let (slot, task) = found.unwrap_or_else(|| {
        THREAD_ONLY_OPENS.fetch_add(1, Ordering::Relaxed);
        (usize::MAX, 0)
    });
    HOLDER_SLOT.store(slot, Ordering::Relaxed);
    HOLDER_TASK.store(task, Ordering::Relaxed);
    HOLDER_THREAD.store(thread, Ordering::Relaxed);
    OPEN.store(true, Ordering::Release);
}

/// No borrower.
pub fn close() {
    OPEN.store(false, Ordering::Release);
}

/// How many loans opened with no polled task (thread-only identity).
pub fn thread_only_opens() -> u32 {
    THREAD_ONLY_OPENS.load(Ordering::Relaxed)
}

/// esp-alloc's question, from inside the heap lock: is this allocation the
/// borrower's?
#[unsafe(no_mangle)]
pub fn _esp_alloc_lend_borrower() -> bool {
    if !OPEN.load(Ordering::Acquire) {
        return false;
    }
    if thread() != HOLDER_THREAD.load(Ordering::Relaxed) {
        return false;
    }
    let slot = HOLDER_SLOT.load(Ordering::Relaxed);
    if slot >= SLOTS {
        return true;
    }
    POLLED_TASK[slot].load(Ordering::Relaxed) == HOLDER_TASK.load(Ordering::Relaxed)
}

fn thread() -> usize {
    let tp: usize;
    // SAFETY: reads a register.
    unsafe { core::arch::asm!("mv {0}, tp", out(reg) tp) };
    tp
}

fn slot_of(executor_id: u32, claim: bool) -> Option<usize> {
    for slot in 0..SLOTS {
        if EXECUTOR[slot].load(Ordering::Relaxed) == executor_id {
            return Some(slot);
        }
    }
    if !claim {
        return None;
    }
    for slot in 0..SLOTS {
        if EXECUTOR[slot]
            .compare_exchange(0, executor_id, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return Some(slot);
        }
    }
    None
}

// embassy-executor's `trace` hooks: only the polled task matters here.

#[unsafe(no_mangle)]
fn _embassy_trace_task_exec_begin(executor_id: u32, task_id: u32) {
    if let Some(slot) = slot_of(executor_id, true) {
        POLLING_THREAD[slot].store(thread(), Ordering::Relaxed);
        POLLED_TASK[slot].store(task_id, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
fn _embassy_trace_task_exec_end(executor_id: u32, _task_id: u32) {
    if let Some(slot) = slot_of(executor_id, false) {
        POLLED_TASK[slot].store(0, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
fn _embassy_trace_poll_start(_executor_id: u32) {}

#[unsafe(no_mangle)]
fn _embassy_trace_task_new(_executor_id: u32, _task_id: u32) {}

#[unsafe(no_mangle)]
fn _embassy_trace_task_end(_executor_id: u32, _task_id: u32) {}

#[unsafe(no_mangle)]
fn _embassy_trace_task_ready_begin(_executor_id: u32, _task_id: u32) {}

#[unsafe(no_mangle)]
fn _embassy_trace_executor_idle(_executor_id: u32) {}
