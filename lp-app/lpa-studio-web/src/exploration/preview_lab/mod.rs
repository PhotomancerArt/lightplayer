//! Preview lab: dev-only CPU-path preview scaling measurement (PoC A).
//!
//! Scaffolding for the GPU-preview discovery roadmap's M1 milestone: spawn N
//! live preview cards, each backed by one `fw-browser` runtime instance in a
//! Web Worker, driven at a throttled tick rate with a binary pixel path
//! (transferable `ArrayBuffer`s) and instrumented per-card frame costs.
//!
//! Reached at `#/preview-lab` in `stories`-feature builds; not part of the
//! product UI. Pure config/stats helpers live in
//! [`crate::exploration::preview_lab_config`] and
//! [`crate::exploration::preview_lab_stats`] so they stay host-testable.

mod example_projects;
mod lab_client_io;
mod lab_runner;
mod lab_sleep;
mod preview_lab_page;
mod worker_rig;

pub(crate) use preview_lab_page::{PreviewLabPage, should_show_preview_lab};
