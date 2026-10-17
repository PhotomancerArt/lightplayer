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
    /// The low water for [`crate::TreeStore::collect_step`]: while fewer
    /// than this percentage of the usable sectors is free, an idle step
    /// collects one sector with garbage (0 = idle steps collect none; they
    /// still forget garbage-only sectors).
    pub idle_free_percent: u32,
    /// The least garbage (bytes) a sector must hold for an idle step to
    /// collect it: each idle collection wins at least this much back.
    pub idle_min_garbage: u32,
    /// Near full: while fewer than `reserve` + this many sectors are free,
    /// an idle step also packs (collects sectors whose waste is only their
    /// tail, a GC pass at a time, while passes shrink the waste) — the work
    /// a write would otherwise do in front of itself (0 = never).
    pub idle_pack_free: u32,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            record_max: 1024,
            gc_policy: GcPolicy::CostBenefit,
            reserve: 3,
            txn_delta_max: 2048,
            idle_free_percent: 25,
            idle_min_garbage: 1024,
            idle_pack_free: 4,
        }
    }
}
