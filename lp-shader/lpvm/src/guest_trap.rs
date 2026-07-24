//! Structured guest-trap access on backend call errors.
//!
//! When a guest writes a trap code to the vmctx trap slot (see
//! [`crate::VmContext`]), the backend's call error carries the code and the
//! trapping invocation index as data. Hosts recover them through
//! [`GuestTrapError`] — typed, no substring matching on `Display` output —
//! so out-of-fuel traps can be routed into structured diagnostics (see the
//! lpvm-native fuel ADR, `docs/adr/2026-07-20-lpvm-native-fuel.md`).

/// A guest trap recovered from a backend call error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestTrap {
    /// Trap code from the vmctx trap slot (`crate::TRAP_CODE_*`).
    pub code: u32,
    /// Invocation index from the vmctx fuel high word: the linear
    /// pixel/sample index written by the render wrappers, or
    /// [`crate::INVOCATION_INDEX_ARMED`] when the trap occurred outside a
    /// per-invocation wrapper (flat call, init, compute tick).
    pub invocation: u32,
}

/// Structured guest-trap details on an [`crate::LpvmInstance::Error`].
///
/// Backends whose calls can end in a guest trap (lpvm-native's rt_jit /
/// rt_emu, lpvm-wasm's hosts) return `Some`; backends without the vmctx
/// trap contract keep the default `None`.
pub trait GuestTrapError {
    /// The guest trap behind this error, if that is what it is.
    fn guest_trap(&self) -> Option<GuestTrap> {
        None
    }
}
