//! Soft-limit bench: measure how many LEDs a (firmware build × board) pair
//! survives, and record the answer.
//!
//! The pieces land in order: the measurement store (the records and how they
//! are filed), the workload generator (the project the bench uploads), then
//! the `lp-cli hardware bench` ramp/bisect loop that drives real hardware and
//! writes a record.

// The command that drives the store and the generator arrives with that loop;
// until then the bin target has no caller for either, and their own tests are
// the only consumers.
#![allow(dead_code, reason = "the bench command is not wired up yet")]

pub mod measurement_store;
pub mod workload;
