#![no_std]

mod sinks;

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

/// JIT symbol-map load notification.
///
/// On RV32 firmware with `feature = "syscall"` this triggers
/// `SYSCALL_JIT_MAP_LOAD`. On host builds (`feature = "log"` or default
/// noop), it logs or no-ops.
#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    sinks::emit_jit_map_load(base, len, entries);
}
