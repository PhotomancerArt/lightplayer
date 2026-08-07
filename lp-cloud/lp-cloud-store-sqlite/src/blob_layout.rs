//! Where a blob lives, given its hash.
//!
//! One function, shared by the filesystem and S3 adapters on purpose: the
//! two must lay blobs out identically, so a deployment can be moved from
//! one to the other (or have one seeded from the other) by copying bytes.

use lpc_history::ContentHash;

/// How many hex characters of the hash name the shard directory.
const SHARD_LEN: usize = 2;

/// The path a blob is stored at, relative to the store's root:
/// `ab/cdef…` — the first two hex characters of the hash as a directory,
/// the remaining sixty-two as the file name.
///
/// # Why shard at all
///
/// Because a flat directory of a hundred thousand files is slow to list and
/// unpleasant to look at on every filesystem that has ever shipped. Two hex
/// characters give 256 shards, which keeps a shard browsable well past any
/// scale this service is planned for. S3 has no directories and does not
/// need the sharding, but it costs nothing there and keeps the two layouts
/// identical.
pub fn blob_object_path(hash: ContentHash) -> String {
    let hex = hash.to_string();
    let (shard, rest) = hex.split_at(SHARD_LEN);
    format!("{shard}/{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_hash_into_a_shard_and_a_name() {
        let hash = ContentHash::of(b"hello");
        let hex = hash.to_string();
        let path = blob_object_path(hash);

        assert_eq!(path, format!("{}/{}", &hex[..2], &hex[2..]));
        assert_eq!(path.len(), hex.len() + 1);
    }

    #[test]
    fn distinct_hashes_get_distinct_paths() {
        assert_ne!(
            blob_object_path(ContentHash::of(b"one")),
            blob_object_path(ContentHash::of(b"two"))
        );
    }
}
