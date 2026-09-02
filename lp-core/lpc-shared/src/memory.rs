//! Heap memory checkpoint logging helpers.

/// Optional callback returning `(free_bytes, used_bytes)` for memory logging.
///
/// Platforms without heap stats pass `None`.
pub type MemoryStatsFn = fn() -> Option<(u32, u32)>;

/// Log a memory checkpoint if heap stats are available.
pub fn log_memory_checkpoint(memory_stats: Option<MemoryStatsFn>, label: &str) {
    if let Some((free, used)) = memory_stats.and_then(|stats| stats()) {
        log::info!(
            "[mem] {}: {}k free / {}k used",
            label,
            free / 1024,
            used / 1024
        );
    }
}

/// Log a memory checkpoint from an optionally borrowed callback.
pub fn log_memory_checkpoint_ref(memory_stats: Option<&MemoryStatsFn>, label: &str) {
    log_memory_checkpoint(memory_stats.copied(), label);
}

// --- Process-global stats hook -------------------------------------------
//
// The engine's shader nodes want a `[mem]` line around the on-device JIT
// compile (the compile transient is the number the ESP32-C6 flagship OOM
// turned on, 2026-09-01), but nothing threads a `MemoryStatsFn` through
// `EngineServices`/`TickContext` and the nodes should not grow a plumbing
// parameter for a log line. Same shape as `backtrace::set_oom_context`: one
// global, set once by whoever owns the platform's heap counters (the server,
// which already holds the `MemoryStatsFn` the firmware passes it), read by
// whoever wants a checkpoint. Absent on hosts without heap stats, in which
// case the checkpoint is a no-op.

use core::sync::atomic::{AtomicUsize, Ordering};

static GLOBAL_MEMORY_STATS: AtomicUsize = AtomicUsize::new(0);

/// Install (or clear, with `None`) the process-wide heap-stats callback.
pub fn set_global_memory_stats(memory_stats: Option<MemoryStatsFn>) {
    let raw = memory_stats.map_or(0, |f| f as usize);
    GLOBAL_MEMORY_STATS.store(raw, Ordering::Relaxed);
}

/// The process-wide heap-stats callback, if one was installed.
pub fn global_memory_stats() -> Option<MemoryStatsFn> {
    let raw = GLOBAL_MEMORY_STATS.load(Ordering::Relaxed);
    if raw == 0 {
        return None;
    }
    // SAFETY: the only writer is `set_global_memory_stats`, which stores a
    // `MemoryStatsFn` (a plain `fn` pointer, valid for the whole program)
    // or 0; 0 is filtered above.
    Some(unsafe { core::mem::transmute::<usize, MemoryStatsFn>(raw) })
}

/// Log a `[mem]` checkpoint through the process-wide callback; a no-op
/// when none is installed.
pub fn log_global_memory_checkpoint(label: &str) {
    log_memory_checkpoint(global_memory_stats(), label);
}
