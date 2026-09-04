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
//! On top of that, [`frag_counterfactual`] replays the same trace with one
//! lever already pulled — a scratch arena for a transient window, its
//! residents packed ahead of its churn, or TLSF ([`tlsf_heap`]) instead of the
//! first-fit list — so a lever can be priced before anyone implements it. No
//! lever is implemented here.
//!
//! Everything here is host-only analysis behind the `std` feature; nothing in
//! this module runs on a device.

pub mod first_fit_heap;
pub mod frag_counterfactual;
mod frag_discount;
pub mod frag_replay;
pub mod frag_report;
pub(crate) mod tlsf_heap;

pub use first_fit_heap::{FirstFitHeap, HeapGeometry, Hole};
pub use frag_counterfactual::{
    CounterfactualCell, CounterfactualColumn, CounterfactualReport, CounterfactualRow,
    CounterfactualSpec, CounterfactualTerm, analyze_counterfactuals,
};
pub use frag_replay::{
    BoundingBlock, CLASSIC_REGIONS, CrossCheckRow, DiscountRow, FRAME_ALLOC_OF_INTEREST,
    FragAnalysis, FragLayout, FragOptions, HISTOGRAM_LABELS, HoleDetail, MarkerShape, PinningRow,
    RegionShape, RegionSpec, SizedAllocSite, WouldOom, analyze_fragmentation,
};
pub use frag_report::{render_counterfactual_section, render_fragmentation_section};
