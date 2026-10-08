//! The store's dials. The flash geometry comes from the [`crate::Flash`].

/// How GC picks a victim sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcPolicy {
    /// The sector with the most garbage bytes.
    Greedy,
    /// LFS's cost-benefit: maximise `(1 - u) * age / (1 + u)`, `u` = the
    /// sector's live fraction, `age` = sector sequences since it was opened.
    CostBenefit,
}

/// The store's dials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreConfig {
    /// Largest record on flash, header included (128 ..= sector − 20).
    pub record_max: u32,
    pub gc_policy: GcPolicy,
    /// Sectors every write must leave free for GC.
    pub reserve: u32,
    /// RAM a transaction may hold as not-yet-written directory changes
    /// (paths and entries) before it writes them as pending directories.
    pub txn_delta_max: u32,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            record_max: 1024,
            gc_policy: GcPolicy::CostBenefit,
            reserve: 3,
            txn_delta_max: 2048,
        }
    }
}
