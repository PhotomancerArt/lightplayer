//! RAII guard for a recovery frame.

use core::marker::PhantomData;

use crate::recovery::{EnteredFrame, with_global};

/// Guard returned by [`enter`](crate::enter): pops its frame on drop.
///
/// # Rules
///
/// - **LIFO**: guards must drop in reverse entry order. A mismatched pop is
///   debug-asserted; in release the stack self-corrects best-effort.
/// - **No `.await` while held** in code sharing the stack with other tasks:
///   the persistent stack is one linear stack, so interleaved frames from
///   another task would corrupt the LIFO discipline. (The guard is `!Send`,
///   which stops cross-thread misuse; same-executor interleaving is on the
///   caller.) Today all frame-taking work runs on the single server-loop
///   task, so wrapping synchronous sections is always safe.
/// - Dropping during unwind is fine and expected: the crash was already
///   staged with a snapshot of the full path, and a frame that unwound is
///   not counted as a clean completion.
#[derive(Debug)]
pub struct FrameGuard {
    token: Option<EnteredFrame>,
    /// `!Send + !Sync`: the frame stack is single-owner.
    _not_send: PhantomData<*mut ()>,
}

impl FrameGuard {
    pub(crate) fn active(token: EnteredFrame) -> Self {
        Self {
            token: Some(token),
            _not_send: PhantomData,
        }
    }

    /// A guard that does nothing on drop — returned when no global recovery
    /// is installed, so instrumented code needs no special casing.
    pub(crate) fn inert() -> Self {
        Self {
            token: None,
            _not_send: PhantomData,
        }
    }

    /// Whether this guard actually holds a frame (false on targets without
    /// an installed recovery instance).
    pub fn is_active(&self) -> bool {
        self.token.is_some()
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            with_global(|recovery| recovery.leave_frame(token));
        }
    }
}
