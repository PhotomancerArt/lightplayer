//! Where a board's stamped manifest lives on its filesystem, and where a
//! stamp in flight stages it.
//!
//! The firmware reads [`HARDWARE_MANIFEST_PATH`] at boot and prefers it to
//! its compiled-in manifest. A stamp (Studio's flash ladder, `lp-cli
//! hardware stamp`) cannot write that file in one request — a whole manifest
//! in one frame ran a classic out of heap in the decode — so it streams
//! chunks, and a file written in chunks is a prefix at every chunk boundary.
//! The stamp is therefore journaled: the whole manifest goes to
//! [`HARDWARE_MANIFEST_NEXT_PATH`] first, then to the live path, and the
//! staged copy is removed last. A board that boots with a staged copy that
//! parses takes it (the stamp got that far, so the live file may be torn);
//! one whose staged copy does not parse drops it (the stamp never reached
//! the live file, so the live file is the previous stamp, whole). Neither
//! interruption loses the board's manifest.
//!
//! The wire has no rename, and the stamp needs none: both paths are written
//! with the ordinary chunked file write.

/// The board manifest the firmware loads at boot.
pub const HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// A stamp's staged copy of the manifest, complete only once the stamp has
/// started writing [`HARDWARE_MANIFEST_PATH`].
pub const HARDWARE_MANIFEST_NEXT_PATH: &str = "/hardware.json.next";
