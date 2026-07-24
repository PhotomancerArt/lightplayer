//! `PreviewHost`: leased, pooled, budgeted live project previews.
//!
//! One service owns "a project, rendered live, in a box" end to end
//! (`docs/adr/2026-07-16-preview-host.md`): it boots a small pool of
//! explicit-tick browser workers separate from the Studio session worker,
//! hands out **slot leases** (`lease(PreviewSlotRequest)` →
//! [`PreviewSlotHandle`]), deploys the requested content into a per-slot
//! tiered runtime, attaches the consumer's canvas on the GPU tier, and
//! drives every slot from one host-side deadline scheduler (per-slot fps,
//! in-flight backpressure, visibility suspend, global live-slot cap with
//! LRU eviction). Failure stays visible: tier fallback reasons, present
//! errors, and device loss all surface on the slot's observable status,
//! and a poisoned worker is recycled deliberately (respawn + re-lease of
//! still-visible slots), never retried in a flap.
//!
//! The browser-facing half ([`PreviewHost`] itself, its worker pool, and
//! the per-runtime deploy transport) only exists on
//! `wasm32 + feature = "browser-worker"`, mirroring how
//! `crate::app::server`'s browser worker client io is gated. The request,
//! status, scheduling, and content-materialization vocabulary below is
//! target-neutral so hosts, tests, and native tooling share one model.

pub mod frame_schedule;
mod preview_content;
mod preview_types;
pub mod slot_policy;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod preview_client_io;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod preview_host_impl;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod preview_sleep;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod preview_worker;

pub use frame_schedule::{FrameDecision, FrameSchedule, MAX_TICK_DELTA_MS};
pub use preview_content::{catalog_deploy_files, example_deploy_files};
pub use preview_types::{
    PreviewHostConfig, PreviewProfile, PreviewSlotRequest, PreviewSlotStatus, PreviewSource,
    PreviewTier,
};
pub use slot_policy::{EvictionCandidate, choose_eviction, choose_worker};

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use preview_host_impl::{PreviewHost, PreviewSlotHandle};
