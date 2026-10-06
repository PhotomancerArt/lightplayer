//! Emulator seams, the chip-neutral half.
//!
//! A seam is a named function in the shipped firmware where an emulator may
//! answer (`lp-base/lp-seam`; the ADR `docs/adr/2026-10-05-emulator-seams.md`).
//! This module holds what is the same on every chip: finding the firmware's
//! descriptor tables in a flash image ([`seam_scan`]), what a run asked for
//! ([`seam_request`]), the implementations this emulator has
//! ([`seam_impl`]), choosing the live table and its arm sites
//! ([`seam_resolution`]), the host half of a capability seam
//! ([`seam_endpoint`]), the wake's pacing ([`seam_wake_pacer`]), what joins
//! endpoints of several machines ([`seam_medium`]) and the words every host
//! prints ([`seam_announce`]).
//!
//! It holds no register offset, no hart and no clock: arming, answering and
//! raising are the chip machine's, which owns the hart, the cache window and
//! the interrupt matrix. Nothing here is static, so several machines in one
//! process share no seam state.
//!
//! **With no seam asked for, nothing here runs**: a machine whose request is
//! empty never scans its flash.

pub mod seam_announce;
pub mod seam_endpoint;
pub mod seam_impl;
pub mod seam_medium;
pub mod seam_request;
pub mod seam_resolution;
pub mod seam_scan;
pub mod seam_wake_pacer;

pub use seam_endpoint::{EndpointEvent, EndpointId, SeamEndpoint};
pub use seam_impl::{SeamAnswer, SeamImpl};
pub use seam_medium::{LoopbackMedium, SeamMedium};
pub use seam_request::{SeamRequest, Strength};
pub use seam_resolution::{
    ArmSite, Engaged, Outcome, SiteKind, holds_seam_hint, resolve, resolve_static,
};
pub use seam_scan::{Candidate, ScanHit, ScanResult, ScannedEntry, scan};
pub use seam_wake_pacer::{PacerConfig, Tick, WakePacer};
