//! What a mounted candidate says about itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Self-reported facts: RAM held, space as the store sees it, features.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CandidateReport {
    /// RAM the store holds while mounted (index, caches, buffers), bytes.
    pub ram_bytes: u64,
    /// Sectors the store considers in use (its own view; `None` = unknown).
    pub used_sectors: Option<u32>,
    /// Does `commit` make a whole step atomic?
    pub step_atomic: bool,
    /// Anything else worth a column (dedup hits, GC copies, …).
    pub extra: BTreeMap<String, f64>,
}
