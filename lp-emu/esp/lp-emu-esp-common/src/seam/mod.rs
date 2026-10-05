//! Emulator seams, the chip-neutral half (plan
//! `lp2025/2026-10-05-1026-emulator-seams`, M0 spike).
//!
//! A seam is a named function in the shipped firmware where an emulator may
//! answer. The firmware lists its seams in a descriptor table
//! (`lp-base/lp-seam`); this module finds that table in a flash image
//! ([`seam_scan`]), maps a seam function's address to the flash bytes behind
//! it ([`esp_app_image`]) and parses what a run asked for ([`seam_request`]).
//! Arming and answering are the chip machine's (it owns the hart and the
//! cache window).
//!
//! **Seam off is today's machine.** Nothing here runs unless a run asks for a
//! seam: no scan, no patch, no hook.

pub mod esp_app_image;
pub mod seam_request;
pub mod seam_scan;

pub use seam_request::{SeamImpl, SeamRequest};
pub use seam_scan::{ScanOutcome, ScannedEntry, ScannedTable, scan};
