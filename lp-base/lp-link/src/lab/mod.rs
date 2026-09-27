//! **The comms lab**: a soak protocol that runs on top of a [`Link`](crate::Link)
//! so the link can be proved on a real pipe (USB-Serial-JTAG, BLE, UDP) the
//! same way everywhere. Milestone M3 of the reliable-device-link plan.
//!
//! Two sans-IO halves share one wire contract:
//!
//! - [`LabBoard`] is the board's application: it echoes soak messages, streams
//!   its own on request, answers control commands, and asks the edge for the
//!   things only the edge can do (stall the executor, panic, write log lines).
//! - [`LabHost`] is the host's test plan: bring the link up, echo soak
//!   messages of random sizes for a while, take a board stream for a while,
//!   ask for a burst of log lines, and read the board's own counters.
//!
//! Soak messages ([`soak_message`]) carry a sequence number, their length, a
//! CRC-32C and a payload derived from the sequence number, so the receiver
//! proves each one is whole, unaltered, in order and not repeated, without
//! either side keeping a copy.
//!
//! Channels: soak messages on [`CH_PROTO`](crate::CH_PROTO) (reliable),
//! commands and replies as text on [`CH_CONTROL`](crate::CH_CONTROL)
//! (reliable), log lines on [`CH_LOG`](crate::CH_LOG) (best effort).

mod board_stats;
mod lab_board;
mod lab_command;
mod lab_host;
mod lab_rng;
pub mod soak_message;

pub use board_stats::{BoardStats, counters_kv, parse_kv};
pub use lab_board::{BoardAction, LabBoard};
pub use lab_command::LabCommand;
pub use lab_host::{LabHost, LabPhase, LabPlan, LabReport};
pub use lab_rng::LabRng;

/// The marker every lab log line starts with, so the host can count the ones
/// a `log` command asked for apart from the board's other log lines.
pub const LAB_LOG_MARK: &str = "lab-log ";
