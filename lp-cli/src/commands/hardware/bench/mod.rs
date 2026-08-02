//! Soft-limit bench: measure how many LEDs a (firmware build × board) pair
//! survives, and record the answer.
//!
//! `lp-cli hardware bench --board <vendor/product>` ramps a generated
//! single-strip workload on real hardware until the device runs out of memory,
//! bisects the boundary, and writes a [`MeasurementRecord`] into
//! `measurements/`. The pieces:
//!
//! - [`schedule`] — which LED count to try next. Pure, and where the procedure
//!   the metric definition pins actually lives.
//! - [`workload`] — the project each step deploys.
//! - [`run`] — the hardware loop: deploy, settle, and ask the recovery ledger
//!   what a death was.
//! - [`command`] — what is being measured, and the record that comes out.
//! - [`measurement_store`] — where records are filed, read and written.
//!
//! [`MeasurementRecord`]: lpc_model::MeasurementRecord

// `lp-cli` compiles these modules into both the library and the bin. The bin
// only ever WRITES a record, so the store's read side (and the workload's
// write-to-disk helper, which the integration test uses) has no caller there;
// its consumers are the library's tests today and the studio advisory from P5.
#![allow(
    dead_code,
    reason = "the store's read side has library and later-phase callers, not bin ones"
)]

pub mod command;
pub mod measurement_store;
pub mod run;
pub mod schedule;
pub mod workload;

pub use command::handle_bench;
