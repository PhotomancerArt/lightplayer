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
