//! What a store operation can fail with.

/// A failed store operation. After [`StoreError::Flash`] the store must be
/// dropped (the flash lost power); after any other error the committed state
/// on flash is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError<E> {
    /// The commit would dip into the GC reserve (decided before any record of
    /// the commit is written).
    NoSpace,
    /// The flash failed (power lost); drop the store.
    Flash(E),
    /// Flash content the store cannot use: no complete root at mount, or a
    /// live record that fails its CRC or does not parse.
    Corrupt(&'static str),
    /// A path or file too large for the format.
    TooLarge,
    /// A path that is not absolute, ends in `/`, or has an empty component.
    InvalidPath,
    /// A configuration the prototype does not implement (JSON-tree mode).
    Unsupported(&'static str),
    /// A configuration that cannot work on this flash.
    BadConfig(&'static str),
}

impl<E> From<E> for StoreError<E> {
    fn from(e: E) -> Self {
        StoreError::Flash(e)
    }
}
