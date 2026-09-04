//! Heap fragmentation analysis: replay a recorded heap trace on a target's
//! heap layout and say where the free space went.
//!
//! The emulator guest allocates from one 320 KiB region; the ESP32 classic
//! from two smaller ones. A trace recorded on the guest carries every
//! allocation, free and realloc with its call stack, which is enough to replay
//! the same event stream against any layout — [`FirstFitHeap`] models the
//! `linked_list_allocator` both targets actually run, at the target's own word
//! width. What comes back is, at every perf marker: the largest free block per
//! region, the hole histogram, and the live blocks that hold the biggest holes
//! open, attributed to the call site that allocated them.
//!
//! Everything here is host-only analysis behind the `std` feature; nothing in
//! this module runs on a device.

pub mod first_fit_heap;
pub mod frag_replay;
pub mod frag_report;

pub use first_fit_heap::{FirstFitHeap, HeapGeometry, Hole};
pub use frag_replay::{
    ASSUMED_ALIGN, BoundingBlock, CLASSIC_REGIONS, CrossCheckRow, FRAME_ALLOC_OF_INTEREST,
    FragAnalysis, FragLayout, FragOptions, HISTOGRAM_LABELS, HoleDetail, MarkerShape, PinningRow,
    RegionShape, RegionSpec, SizedAllocSite, WouldOom, analyze_fragmentation,
};
pub use frag_report::render_fragmentation_section;
