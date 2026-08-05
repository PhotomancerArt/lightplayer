//! Offline upgrades for authored LightPlayer projects.
//!
//! A project on disk is a flat map of files, stamped with a `format` version
//! in `project.json`. When that version moves, every project authored before
//! the move stops loading. This crate is the consumer of the
//! `schemas/history/` snapshot ritual: it classifies what format a project is
//! at, and migrates it forward through a chain of per-version steps.
//!
//! ```no_run
//! use lpa_upgrade::{FormatClass, ProjectFiles, classify, upgrade_to_current};
//!
//! let mut files: ProjectFiles = read_the_package().into_iter().collect();
//! match classify(&files) {
//!     FormatClass::Current => { /* open it */ }
//!     FormatClass::Upgradable { .. } => {
//!         let report = upgrade_to_current(&mut files).expect("upgrade");
//!         println!("rewrote {:?}", report.changed_files);
//!     }
//!     other => println!("{}", other.describe()),
//! }
//! # fn read_the_package() -> Vec<(String, Vec<u8>)> { Vec::new() }
//! ```
//!
//! Three properties are load-bearing, and each has tests that fail loudly if
//! it slips:
//!
//! - **Behavior preservation.** A migrated project does what it did before.
//!   Improvements that need information the old bytes do not contain (the
//!   phasor periods the gallery's hand migration mined out of GLSL, for
//!   instance) are authoring work, not upgrade work.
//! - **Minimum churn.** Only files a step changed are rewritten. The authored
//!   corpus is not canonically formatted, and a diff a human cannot read is a
//!   migration a human cannot review.
//! - **Loud refusal.** Anything outside the support floor, and any shape a
//!   step does not recognize, is refused with a message that names what was
//!   found, what was expected, and a remedy. Silent failure is the problem
//!   this crate exists to end, not a fallback it may use.
//!
//! Firmware never upgrades a project (ADR 2026-07-05, decision 5). This crate
//! is host/wasm studio tooling and is kept out of the firmware dependency
//! graph by `scripts/check-upgrade-fw.sh`, wired into `just check-lint`.
//!
//! Sans-IO (`docs/adr/2026-07-06-sans-io-core.md`): the library reads no
//! files, no clock, and no randomness. Callers hand it bytes and take bytes
//! back.

mod format_class;
mod json;
mod json_file_edit;
mod project_files;
mod steps;
mod upgrade;
mod upgrade_error;
mod upgrade_report;

pub use format_class::{FormatClass, UPGRADE_FLOOR, classify};
pub use json::{JsonError, JsonNode};
pub use project_files::{PROJECT_MANIFEST, ProjectFiles};
pub use steps::UpgradeStep;
pub use upgrade::{chain_tip, upgrade_steps, upgrade_to_current};
pub use upgrade_error::UpgradeError;
pub use upgrade_report::UpgradeReport;
