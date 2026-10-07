//! The link thread's stack high-water (`io_thread_stack_diag`, off by
//! default, never shipped): [`crate::thread_stack_diag`] over
//! [`crate::io_thread`]'s stack, logged as `[iostack]`.

use crate::io_thread::STACK_BYTES;
use crate::thread_stack_diag::ThreadStackDiag;

static DIAG: ThreadStackDiag = ThreadStackDiag::new("iostack", "io", STACK_BYTES);

/// Paint the link thread's stack (first thing on the thread).
pub fn paint() {
    DIAG.paint();
}

/// Log its high-water when it has grown (the heartbeat's cadence).
pub fn log_if_grown() {
    DIAG.log_if_grown();
}
