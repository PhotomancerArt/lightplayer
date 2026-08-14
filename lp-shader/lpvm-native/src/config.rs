//! Compile-time configuration for lpvm-native. Change constants here and rebuild.

/// Inline-storage threshold of [`crate::regset::RegSet`]: vregs `0..MAX_VREGS`
/// are tracked without heap allocation; higher ids (which large functions and
/// lowering temps legitimately mint) go to the set's overflow tail. NOT a cap
/// on vreg ids — nothing in the pipeline enforces one below `u16::MAX`.
pub const MAX_VREGS: usize = 256;

/// When `true`, use linear-scan register allocation (loop-aware, supports allocation trace).
/// When `false`, use greedy placement (simpler, faster compile, no trace).
pub const USE_LINEAR_SCAN_REGALLOC: bool = true;
