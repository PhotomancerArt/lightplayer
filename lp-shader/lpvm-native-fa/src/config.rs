//! Compile-time configuration for lpvm-native-fa. Change constants here and rebuild.

/// Maximum distinct virtual registers per function in the FA backend (`v0..v{N-1}`).
pub const MAX_VREGS: usize = 256;

/// When `true`, use linear-scan register allocation (loop-aware, supports allocation trace).
/// When `false`, use greedy placement (simpler, faster compile, no trace).
pub const USE_LINEAR_SCAN_REGALLOC: bool = true;
