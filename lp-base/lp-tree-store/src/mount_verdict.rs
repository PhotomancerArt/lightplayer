//! What a mount that found no committed tree says about the flash, in the
//! three words a firmware acts on:
//!
//! - [`StoreError::NoStore`]: no sector carries a trusted header of this
//!   format (blank flash, a littlefs partition, anything foreign), **or**
//!   the trusted sectors hold nothing but the store's own empty-format
//!   records and no root (a power cut during the store's first `format`).
//!   Nothing a user wrote is here: format it.
//! - [`StoreError::Damaged`]: trusted sectors that hold a store's records
//!   (a file, a non-empty directory, a root, a record of a kind this
//!   version does not know) and no complete root; or a root that decoded
//!   and whose closure failed. The invariant says no cut produces this:
//!   keep the flash for `lp-cli hardware tree extract`, never format it.
//! - [`StoreError::Unsupported`] (decided by the sector headers, before
//!   this): a newer format. Keep it.
//!
//! The classification reads only what mount's first pass already reads
//! (every trusted record, CRC-checked): no extra flash traffic, and no byte
//! of the format involved.

use crate::record_kind::RecordKind;
use crate::store_error::StoreError;

/// The empty directory's payload: zero entries (FORMAT.md "Dir"). `format`
/// writes it, and it is the only record besides the root `format` writes.
pub(crate) const EMPTY_DIR: [u8; 2] = [0, 0];

/// What mount's first pass saw that `format` would not have written.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FormatResidue {
    /// A sector header of this format, trusted.
    trusted_sector: bool,
    /// A CRC-good record other than the empty directory (roots count: a
    /// root means a format finished, so a missing closure is damage).
    content: bool,
}

impl FormatResidue {
    pub fn saw_trusted_sector(&mut self) {
        self.trusted_sector = true;
    }

    /// A CRC-good record: `kind` is `None` for a kind this version does not
    /// know.
    pub fn saw_record(&mut self, kind: Option<RecordKind>, payload: &[u8]) {
        if !(kind == Some(RecordKind::Dir) && payload == EMPTY_DIR) {
            self.content = true;
        }
    }

    /// The verdict for a mount that adopted no root.
    pub fn no_root<E>(&self) -> StoreError<E> {
        if self.trusted_sector && self.content {
            StoreError::Damaged("no complete root")
        } else {
            StoreError::NoStore
        }
    }
}

/// A mount's error as the caller sees it: a record that did not check
/// while mount read the committed tree is the store being damaged, not a
/// runtime `Corrupt`.
pub(crate) fn mount_failure<E>(e: StoreError<E>) -> StoreError<E> {
    match e {
        StoreError::Corrupt(why) => StoreError::Damaged(why),
        e => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type E = StoreError<()>;

    #[test]
    fn no_trusted_sector_is_no_store() {
        let mut r = FormatResidue::default();
        r.saw_record(Some(RecordKind::Blob), b"x");
        assert_eq!(r.no_root::<()>(), E::NoStore);
    }

    #[test]
    fn only_the_empty_directory_is_an_interrupted_format() {
        let mut r = FormatResidue::default();
        r.saw_trusted_sector();
        assert_eq!(r.no_root::<()>(), E::NoStore);
        r.saw_record(Some(RecordKind::Dir), &EMPTY_DIR);
        assert_eq!(r.no_root::<()>(), E::NoStore);
    }

    #[test]
    fn any_other_record_is_damage() {
        for (kind, payload) in [
            (Some(RecordKind::Blob), &b"hi"[..]),
            (Some(RecordKind::Dir), &[1, 0][..]),
            (Some(RecordKind::Multi), &[0][..]),
            (Some(RecordKind::Root), &[0][..]),
            (None, &[][..]),
        ] {
            let mut r = FormatResidue::default();
            r.saw_trusted_sector();
            r.saw_record(kind, payload);
            assert_eq!(r.no_root::<()>(), E::Damaged("no complete root"));
        }
    }

    #[test]
    fn a_corrupt_record_at_mount_is_damage() {
        assert_eq!(mount_failure::<()>(E::Corrupt("dir")), E::Damaged("dir"));
        assert_eq!(
            mount_failure::<()>(E::Unsupported("x")),
            E::Unsupported("x")
        );
    }
}
