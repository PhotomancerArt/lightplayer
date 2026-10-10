//! A discardable cache living in the block while nobody has it.

/// A discardable tenant: a cache whose holder can rebuild it, and which may
/// therefore live in the block while no loan is held.
///
/// The protocol every working purgeable-memory system shares (E12 §B5:
/// Chromium `DiscardableMemory`, Apple `NSDiscardableContent`, ashmem's
/// `PIN`, the classic Mac's purgeable handles):
///
/// 1. **A pin**, here [`Tenant::pinned`]: a tenant in use is never purged.
/// 2. **A fallible checkout**: the holder asks its own cache "am I still
///    here?" before every use, and gets "no" after a purge. That is the
///    holder's code, not the lender's.
/// 3. **A holder-owned rebuild**: only the holder knows how.
/// 4. **A synchronous purge after cheaper options fail**: the lender calls
///    [`Tenant::purge`] only when a grant does not fit beside the tenants,
///    at the safe point the grant is made at, never from the allocator.
///
/// [`Tenant::resident`] is the Linux shrinker's `count_objects`;
/// [`Tenant::purge`] is its `scan_objects`, asked for everything.
pub trait Tenant {
    /// Bytes this tenant holds in the block now (0 once purged).
    fn resident(&self) -> u32;
    /// Whether the tenant is checked out (in use) right now.
    fn pinned(&self) -> bool;
    /// Drop everything; return the bytes freed. Called only when unpinned.
    fn purge(&mut self) -> u32;
}
