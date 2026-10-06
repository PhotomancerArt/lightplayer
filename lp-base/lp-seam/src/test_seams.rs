//! The reserved test range: seam ids `0x7F00..=0x7FFF`.
//!
//! A test seam exists to prove the mechanism under a real build — that the
//! generated call shim keeps its arguments and its result through the
//! release LTO build, and that the wake works — on a seam no product code
//! needs. The rules, all enforced:
//!
//! - its id is in [`TEST_IDS`] and its name starts `test_`, and nothing else
//!   is (checked when [`crate::declare!`] expands);
//! - its `doc:` starts [`DOC_PREFIX`];
//! - only a harness image carries one in its table (the C6's `test_seam_abi`
//!   harness), never a shipped image;
//! - an emulator answers one only with its dev feature `test-seams`, which no
//!   shipped command line turns on.

use core::ops::RangeInclusive;

/// The ids reserved for test seams.
pub const TEST_IDS: RangeInclusive<u16> = 0x7f00..=0x7fff;

/// How every test seam's `doc:` begins.
pub const DOC_PREFIX: &str = "TEST ONLY: never in a shipped table";

/// Whether `id` is a test seam's.
pub const fn is_test_id(id: u16) -> bool {
    id >= *TEST_IDS.start() && id <= *TEST_IDS.end()
}
