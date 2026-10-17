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
    /// Largest record on flash, header included (128 ..= sector − 24).
    pub record_max: u32,
    pub gc_policy: GcPolicy,
    /// Sectors every write must leave free for GC.
    pub reserve: u32,
    /// RAM a transaction may hold as not-yet-written directory changes
    /// (paths and entries) before it writes them as pending directories.
    pub txn_delta_max: u32,
    /// Most RAM mount's whole id → location index may take, in bytes (12 B
    /// a record on flash, garbage included; `mount_index.rs`). Above it the
    /// mount drops the index and finds each tree level by scanning sector
    /// headers instead — the same answer, more reads. 0: never index. A
    /// per-board dial: the C6's ~48 KB keeps it inside the transient RAM a
    /// boot can spare; a board with PSRAM may raise it.
    pub mount_index_max: u32,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            record_max: 1024,
            gc_policy: GcPolicy::CostBenefit,
            reserve: 3,
            txn_delta_max: 2048,
            mount_index_max: 48 * 1024,
        }
    }
}
