//! Tuning knobs for the blame ledger. These are behavior constants, not
//! architecture: adjust freely as field experience accumulates.

/// Ledger capacity: how many distinct paths (including parent prefixes) can
/// carry blame state at once. Eviction prefers the yellow entry closest to
/// green; red entries are never evicted.
pub const PATH_SLOTS: usize = 6;

/// Clean completions of a yellow path required to clear it back to green.
pub const CLEAN_COMPLETIONS_TO_GREEN: u8 = 3;

/// Distinct crashed children that turn a parent path red (hierarchical
/// escalation: a→b→c and a→b→f crashing gates b itself).
pub const DISTINCT_CHILDREN_TO_ESCALATE: u8 = 2;

/// Consecutive boots that died before the boot-complete milestone before
/// the next boot enters safe mode (project auto-load skipped).
pub const INCOMPLETE_BOOTS_TO_SAFE_MODE: u32 = 2;
