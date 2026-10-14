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
    /// Flash content the store cannot use (a record that fails its CRC or
    /// does not parse), or a deflated chunk that does not inflate to its
    /// length or its id. Mount never returns it: see `Damaged`.
    Corrupt(&'static str),
    /// Mount only: no store is here — no sector carries a trusted header of
    /// this format (blank, littlefs, foreign data), or the trusted ones hold
    /// only what an interrupted `format` writes. Nothing a user wrote is on
    /// the flash; formatting it loses nothing.
    NoStore,
    /// Mount only: a store's records are here but no complete root (or the
    /// committed tree did not check). Keep the flash (`lp-cli hardware tree
    /// extract`); do not format over it.
    Damaged(&'static str),
    /// Flash this code must not read: a sector header with an incompat flag
    /// or head kind a newer format added, or written for another sector
    /// size (FORMAT.md "Sector"). Nothing was read past the headers; do not
    /// format over it without asking.
    Unsupported(&'static str),
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
