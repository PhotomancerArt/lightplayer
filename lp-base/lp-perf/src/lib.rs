#![no_std]

mod sinks;

use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum PerfEventKind {
    Begin = 0,
    End = 1,
    Instant = 2,
}

// Canonical event-name constants. New names get added here, never
// inline in call sites.
// ⚠️ The emulator host drops markers whose name it does not know: every
// name here must also be in `lp_emu_core::profile::perf_event::KNOWN_EVENT_NAMES`.
pub const EVENT_FRAME: &str = "frame";
pub const EVENT_SHADER_COMPILE: &str = "shader-compile";
pub const EVENT_SHADER_LINK: &str = "shader-link";
pub const EVENT_PROJECT_LOAD: &str = "project-load";
/// Server boot: recovery init through server + transport construction,
/// before the first tick. Its `retained` figure is what the server holds
/// before any project exists (emitted by fw-emu; a no-op on device sinks).
pub const EVENT_SERVER_BOOT: &str = "server-boot";
/// One accepted `ProjectRead` request, from just after the headroom gate
/// until its event stream has finished or failed. The read's cost is
/// hundreds of small transient allocations, which is exactly the shape a
/// largest-free-block gate cannot see; this window is what makes it
/// measurable (`docs/heap-budget-gate.md`).
pub const EVENT_PROJECT_READ: &str = "project-read";

#[macro_export]
macro_rules! emit_begin {
    ($name:expr) => {
        $crate::__emit($name, $crate::PerfEventKind::Begin)
    };
}
#[macro_export]
macro_rules! emit_end {
    ($name:expr) => {
        $crate::__emit($name, $crate::PerfEventKind::End)
    };
}
#[macro_export]
macro_rules! emit_instant {
    ($name:expr) => {
        $crate::__emit($name, $crate::PerfEventKind::Instant)
    };
}

// Single dispatch point. Implementation is selected at compile time.
#[inline(always)]
pub fn __emit(name: &'static str, kind: PerfEventKind) {
    sinks::emit(name, kind);
}

#[cfg(feature = "syscall")]
pub use lp_emu_abi::JitSymbolEntry;

/// When neither sink pulls in `lp_emu_abi`, we still need a
/// `JitSymbolEntry` symbol so the public signature compiles. Define a
/// local mirror behind the noop/log paths.
#[cfg(not(feature = "syscall"))]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct JitSymbolEntry {
    pub offset: u32,
    pub size: u32,
    pub name_ptr: u32,
    pub name_len: u32,
}

/// Hook run after a marker syscall when the host's return value says an
/// `AllocCollector` is active and wants the guest's exact heap free-list
/// shape (`lp_emu_core::profile::ProfileSession::wants_free_list_shape`).
///
/// Stored as a bare `fn()` in an `AtomicUsize` rather than a `static mut` —
/// `lp-perf` stays `no_std` and dependency-free, so it cannot hold the
/// guest allocator's type directly; the guest allocator crate installs
/// itself here instead (`lp-riscv-emu-guest::allocator::init_heap`, under
/// its `profile` feature).
static MARKER_SHAPE_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install the free-list-shape hook. Last writer wins; call once, at heap
/// init.
pub fn set_marker_shape_hook(f: fn()) {
    MARKER_SHAPE_HOOK.store(f as usize, Ordering::Release);
}

/// Called by the syscall sink after a marker `ecall` returns `1`. No-op
/// until [`set_marker_shape_hook`] has installed a hook (e.g. cpu-only
/// profiles, or non-syscall sinks that never see a `1`).
#[cfg(feature = "syscall")]
pub(crate) fn call_marker_shape_hook() {
    let raw = MARKER_SHAPE_HOOK.load(Ordering::Acquire);
    if raw != 0 {
        // SAFETY: the only writer is `set_marker_shape_hook`, which only
        // ever stores a `fn()` cast to `usize` via `as usize`, so a
        // non-zero value is always a valid `fn()` pointer.
        let f: fn() = unsafe { core::mem::transmute::<usize, fn()>(raw) };
        f();
    }
}

/// JIT symbol-map load notification.
///
/// On RV32 firmware with `feature = "syscall"` this triggers
/// `SYSCALL_JIT_MAP_LOAD`. On host builds (`feature = "log"` or default
/// noop), it logs or no-ops.
#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    sinks::emit_jit_map_load(base, len, entries);
}
