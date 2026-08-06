//! What every `BlobStore` must do, whatever it stores bytes in.
//!
//! Instantiate the battery for an adapter with
//! [`blob_store_conformance_tests!`].
//!
//! The S3 adapter is not run here: a hand-written S3 fake would only prove
//! the fake matches the code. It shares its layout with the filesystem
//! adapter (`blob_layout`) and is smoke-tested against real Tigris in P11.

use lp_cloud_domain::BlobStore;
use lpc_history::ContentHash;

/// Generate the whole `BlobStore` battery as `#[test]` functions.
///
/// `$with_store` names a function taking `impl FnOnce(&mut dyn BlobStore)`
/// that builds a fresh, empty store, runs the check, and tears it down.
macro_rules! blob_store_conformance_tests {
    ($with_store:path) => {
        $crate::conformance::blob_store_suite::blob_store_conformance_tests!(
            @checks $with_store,
            bytes_round_trip_under_their_own_hash,
            storing_the_same_bytes_twice_is_idempotent,
            an_unstored_blob_is_none,
            an_empty_payload_round_trips,
            distinct_payloads_get_distinct_addresses,
            a_payload_larger_than_one_buffer_round_trips,
        );
    };
    (@checks $with_store:path, $($check:ident),+ $(,)?) => {
        $(
            #[test]
            fn $check() {
                $with_store($crate::conformance::blob_store_suite::$check);
            }
        )+
    };
}

pub(crate) use blob_store_conformance_tests;

/// `put` returns the address it stored the bytes at, and that address is
/// the hash of those bytes — a store that let the caller name the address
/// could be made to lie.
pub fn bytes_round_trip_under_their_own_hash(store: &mut dyn BlobStore) {
    let hash = store.put(b"hello");

    assert_eq!(hash, ContentHash::of(b"hello"));
    assert!(store.has(hash));
    assert_eq!(store.get(hash).as_deref(), Some(&b"hello"[..]));
}

pub fn storing_the_same_bytes_twice_is_idempotent(store: &mut dyn BlobStore) {
    let first = store.put(b"hello");
    let second = store.put(b"hello");

    assert_eq!(first, second);
    assert_eq!(store.get(first).as_deref(), Some(&b"hello"[..]));
}

pub fn an_unstored_blob_is_none(store: &mut dyn BlobStore) {
    let absent = ContentHash::of(b"never stored");

    assert!(!store.has(absent));
    assert_eq!(store.get(absent), None);
}

/// Zero bytes are a legal blob (an empty file in a tree), and must not be
/// confused with a missing one.
pub fn an_empty_payload_round_trips(store: &mut dyn BlobStore) {
    let hash = store.put(b"");

    assert!(store.has(hash));
    assert_eq!(store.get(hash), Some(Vec::new()));
}

pub fn distinct_payloads_get_distinct_addresses(store: &mut dyn BlobStore) {
    let one = store.put(b"one");
    let two = store.put(b"two");

    assert_ne!(one, two);
    assert_eq!(store.get(one).as_deref(), Some(&b"one"[..]));
    assert_eq!(store.get(two).as_deref(), Some(&b"two"[..]));
}

/// A megabyte is an ordinary preview PNG. Small enough to keep the suite
/// fast, big enough that a store writing through a fixed-size buffer would
/// have to do it more than once.
pub fn a_payload_larger_than_one_buffer_round_trips(store: &mut dyn BlobStore) {
    let bytes: Vec<u8> = (0..1024 * 1024).map(|index| (index % 251) as u8).collect();
    let hash = store.put(&bytes);

    assert_eq!(store.get(hash), Some(bytes));
}
