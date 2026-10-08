//! The store's dials.

/// How GC picks a victim sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcPolicy {
    /// The sector with the most garbage bytes.
    Greedy,
    /// LFS's cost-benefit: maximise `(1 - u) * age / (1 + u)`, `u` = the
    /// sector's live fraction, `age` = sector sequences since it was opened.
    CostBenefit,
}

/// How blob chunks are written. Decoding always handles every codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Stored,
    /// Raw deflate, no dictionary.
    Deflate,
    /// Raw deflate against the store-local dictionary (a `Dict` node).
    DeflateDict,
}

/// The store's dials. The flash geometry comes from the [`crate::Flash`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreConfig {
    /// Largest record on flash, header included (128 ..= sector − 20).
    pub record_max: u32,
    pub gc_policy: GcPolicy,
    /// Sectors a commit must leave free for GC.
    pub reserve: u32,
    pub codec: Codec,
    /// Target size of a trained dictionary (`deflate_dict`).
    pub dict_size: u32,
    /// New (not already stored) content in one commit at or above which a
    /// fresh dictionary is trained (`deflate_dict`): "a push".
    pub dict_train_min: u32,
    /// JSON-tree mode. Not implemented: `true` is refused with
    /// [`crate::StoreError::Unsupported`].
    pub json_tree: bool,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            record_max: 1024,
            gc_policy: GcPolicy::CostBenefit,
            reserve: 3,
            codec: Codec::Deflate,
            dict_size: 8192,
            dict_train_min: 16 * 1024,
            json_tree: false,
        }
    }
}
