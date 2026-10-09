//! What the store measured about itself.

/// Counters and RAM accounting. RAM figures are capacities × element sizes
/// of the store's own vectors (what the allocator holds for them), not
/// estimates; see the README for how they were cross-checked against a
/// counting allocator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeStoreStats {
    /// Records in the RAM index (live after the last mark, plus every record
    /// written since).
    pub index_entries: usize,
    /// 12 B per index slot.
    pub index_ram_bytes: usize,
    /// 12 B per sector, plus the retired list.
    pub sector_table_ram_bytes: usize,
    /// Index + sector table: what the store holds between operations.
    pub resident_ram_bytes: usize,
    /// The largest single buffer an operation allocated (a record, a
    /// directory's bytes, a leaf list, a mark set, an inflate buffer, the
    /// transaction's delta, mount's scan index). Excludes the
    /// caller's file buffer.
    pub transient_peak_bytes: usize,
    pub dedup_hits: u64,
    /// Full marks (mount, and before GC when free space looked short).
    pub marks: u64,
    /// Live records copied by GC.
    pub gc_copies: u64,
    pub gc_copy_bytes: u64,
    /// Victim sectors collected.
    pub gc_runs: u64,
    /// Roots written.
    pub commits: u64,
    pub records_written: u64,
    pub record_bytes_written: u64,
    /// Flash bytes read by the last mount.
    pub mount_bytes_read: u64,
    /// Header scans the last mount made after its first pass (one per level
    /// of the tree it indexed).
    pub mount_scans: u32,
    pub sectors_opened: u64,
    pub erases: u64,
    /// Read-backs that did not match what was written (record, erase or
    /// sector header).
    pub verify_failures: u64,
    /// Sectors retired (never allocated again).
    pub retired_sectors: usize,
}
