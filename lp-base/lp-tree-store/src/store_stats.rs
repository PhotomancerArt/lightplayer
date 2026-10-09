//! What the store measured about itself.

/// Counters and RAM estimates for the testbed's scoreboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeStoreStats {
    /// Records in the RAM index (one per distinct id on flash).
    pub index_entries: usize,
    /// `index_entries × size_of::<(ObjectId, RecordLoc)>()` — 16 B each.
    pub index_ram_bytes: usize,
    /// The RAM path map: path bytes + per-entry sizes (committed entries only).
    pub tree_ram_bytes: usize,
    /// Per-sector table: `sector_count × size_of` of its three columns.
    pub sector_table_ram_bytes: usize,
    /// Largest single transient buffer: staged content at commit, the record
    /// plan, a decode buffer, the dictionary, a mark set.
    pub largest_buffer: usize,
    pub dedup_hits: u64,
    /// Live records copied by GC.
    pub gc_copies: u64,
    pub gc_copy_bytes: u64,
    /// Victim sectors collected.
    pub gc_runs: u64,
    pub commits: u64,
    pub records_written: u64,
    pub record_bytes_written: u64,
    /// Flash bytes read by the last mount (headers, record scan, closure
    /// check, tree load, head tail checks).
    pub mount_bytes_read: u64,
    pub sectors_opened: u64,
    pub erases: u64,
}
