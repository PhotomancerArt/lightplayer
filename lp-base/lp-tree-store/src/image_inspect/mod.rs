//! A read-only inspector for a raw image of a tree-store partition
//! (feature `inspect`; host tooling, never on a device): what
//! `lp-cli hardware tree inspect|check|extract` is built on.
//!
//! | file | concept |
//! |---|---|
//! | `image_report` | the plain-data answer (`ImageReport`), `Serialize` for `--json` |
//! | `image_scan` | sector size, header classification, a sector's records |
//! | `store_image` | [`StoreImage`]: root choice, the committed tree, node reads |
//! | `image_check` | [`StoreImage::check`]: the store's fsck |
//! | `image_extract` | [`StoreImage::extract`]: the committed files, as bytes |
//!
//! It never mounts, so it can read where mount refuses (a sector at a newer
//! format version); the format's decoders are the store's own.

mod image_check;
mod image_extract;
mod image_report;
mod image_scan;
mod store_image;

pub use image_check::{CheckReport, Finding, Severity};
pub use image_extract::{ExtractedFile, Extraction, SkippedEntry};
pub use image_report::{
    EntryKindReport, HeadReport, HeaderReport, ImageReport, MountVerdict, RecordKindReport,
    RecordReport, RecordStatus, RootOutcome, RootReport, SectorReport, SectorSizeFrom,
    SectorState, TreeEntryReport,
};
pub use image_scan::detect_sector_size;
pub use store_image::{ImageError, StoreImage};

#[cfg(test)]
mod image_inspect_tests;
