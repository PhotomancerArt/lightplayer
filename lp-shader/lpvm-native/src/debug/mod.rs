//! Debug formatting and parsing for IR stages.
//!
//! This module provides textual representations of all IR stages
//! for debugging and testing. All formatting is in forward order
//! (even when the allocator walks backward).

pub mod filetest_snapshot;
pub mod sections;
pub mod vinst;
