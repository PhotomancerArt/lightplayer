//! The shared adapter conformance suite.
//!
//! One battery of checks, written once against `&mut dyn MetaStore` and
//! `&mut dyn BlobStore`, run against every adapter there is: the in-memory
//! ones from `lp-cloud-store-mem` and the persistent ones from this crate.
//!
//! # Why this exists
//!
//! Because a fake that has drifted from the real store is a data-corruption
//! bug that keeps the test suite green. Every layer above the ports is
//! tested against the in-memory adapters — that is the point of them — so
//! the moment the in-memory store answers a question differently from
//! SQLite, every one of those tests is asserting something that is not true
//! in production. This suite is the only place that difference can be
//! caught.
//!
//! # How to add a check
//!
//! Write a function taking `&mut dyn MetaStore` (or `&mut dyn BlobStore`)
//! in the relevant suite module, then add its name to the list inside that
//! module's `*_conformance_tests!` macro. Every adapter picks it up.

pub mod blob_store_suite;
pub mod fixtures;
pub mod meta_store_suite;
