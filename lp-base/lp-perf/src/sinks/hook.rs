//! RESEARCH sink (branch `research/frag-reads`, never shipped): forward every
//! marker to a function the firmware installs, so a diagnostic build can
//! bracket allocations by window (`project-read`, `shader-compile`, …).
use crate::JitSymbolEntry;
use crate::PerfEventKind;
use core::sync::atomic::{AtomicUsize, Ordering};

static HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install the marker hook. Last writer wins.
pub fn set_hook(f: fn(&'static str, PerfEventKind)) {
    HOOK.store(f as usize, Ordering::Release);
}

#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    let raw = HOOK.load(Ordering::Acquire);
    if raw != 0 {
        // SAFETY: only `set_hook` stores here, always a `fn(&str, PerfEventKind)`.
        let f: fn(&'static str, PerfEventKind) = unsafe { core::mem::transmute(raw) };
        f(name, kind);
    }
}

#[inline(always)]
pub fn emit_jit_map_load(_base: u32, _len: u32, _entries: &[JitSymbolEntry]) {}
