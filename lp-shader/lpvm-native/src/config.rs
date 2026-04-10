//! Compile-time configuration for lpvm-native. Change constants here and rebuild.

/// Which register allocator runs before (or as) emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegAllocAlgorithm {
    /// Loop-aware linear scan; `alloc_trace` in emit is honored.
    LinearScan,
    /// Simple greedy placement.
    Greedy,
    /// Backward-walk fast allocator (straight-line only). Functions with lowered
    /// control flow (`Label` / `Br` / `BrIf`) fail with [`crate::error::NativeError::FastallocUnsupportedControlFlow`].
    Fast,
}

/// Active [`RegAllocAlgorithm`]. Rebuild after changing.
pub const REG_ALLOC_ALGORITHM: RegAllocAlgorithm = RegAllocAlgorithm::Fast;

/// When `true`, emit via [`crate::regalloc::FastAllocation`] (adapter + edit list).
/// When `false`, use the legacy [`crate::regalloc::Allocation`] path in the emitter.
pub const USE_FAST_ALLOC_EMIT: bool = true;
