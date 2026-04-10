//! Compile-time configuration for lpvm-native. Change constants here and rebuild.

/// When `true`, use linear-scan register allocation (loop-aware, supports allocation trace).
/// When `false`, use greedy placement (simpler, faster compile, no trace).
pub const USE_LINEAR_SCAN_REGALLOC: bool = true;
