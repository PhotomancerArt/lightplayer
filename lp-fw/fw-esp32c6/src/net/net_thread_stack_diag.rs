//! The network thread's stack high-water (`net_thread_stack_diag`, off by
//! default, never shipped): [`crate::thread_stack_diag`] over `lp-net`'s
//! stack ([`super::net_thread::STACK_BYTES`]), logged as `[netstack]`. P08
//! sets the stack's size from it: idle, during a secure handshake, during an
//! upload.

use super::net_thread::STACK_BYTES;
use crate::thread_stack_diag::ThreadStackDiag;

static DIAG: ThreadStackDiag = ThreadStackDiag::new("netstack", "net", STACK_BYTES);

/// Paint `lp-net`'s stack (first thing on the thread).
pub fn paint() {
    DIAG.paint();
}

/// Log its high-water when it has grown (the heartbeat's cadence).
pub fn log_if_grown() {
    DIAG.log_if_grown();
}
