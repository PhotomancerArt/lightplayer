//! What a store operation can fail with.

/// A failed store operation. After [`StoreError::Flash`] the store must be
/// dropped (the flash lost power); after any other error the committed state
/// on flash is unchanged, and so is the store's view of it (a failed call
/// inside a transaction leaves the transaction as it was before the call).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError<E> {
    /// The write would dip into the GC reserve. Usually decided before any
    /// record is written; never after a root.
    NoSpace,
    /// The flash failed (power lost); drop the store.
    Flash(E),
    /// Flash content the store cannot use (no complete root at mount, a
    /// record that fails its CRC or does not parse), or a deflated chunk
    /// that does not inflate to its length or its id.
    Corrupt(&'static str),
    /// A path or file too large for the format.
    TooLarge,
    /// A path that is not absolute, ends in `/`, or has an empty component.
    InvalidPath,
    /// `put_chunk_deflated` at an offset that is neither 0 nor the file's
    /// size.
    BadOffset,
    /// `begin` inside a transaction.
    InTransaction,
    /// A configuration that cannot work on this flash.
    BadConfig(&'static str),
}

impl<E> From<E> for StoreError<E> {
    fn from(e: E) -> Self {
        StoreError::Flash(e)
    }
}
