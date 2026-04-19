#![no_std]

mod sinks;

#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum PerfEventKind {
    Begin   = 0,
    End     = 1,
    Instant = 2,
}

// Canonical event-name constants. New names get added here, never
// inline in call sites.
pub const EVENT_FRAME:          &str = "frame";
pub const EVENT_SHADER_COMPILE: &str = "shader-compile";
pub const EVENT_SHADER_LINK:    &str = "shader-link";
pub const EVENT_PROJECT_LOAD:   &str = "project-load";

#[macro_export]
macro_rules! emit_begin {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::Begin) };
}
#[macro_export]
macro_rules! emit_end {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::End) };
}
#[macro_export]
macro_rules! emit_instant {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::Instant) };
}

// Single dispatch point. Implementation is selected at compile time.
#[inline(always)]
pub fn __emit(name: &'static str, kind: PerfEventKind) {
    sinks::emit(name, kind);
}
