//! Running a whole update, or a heal, over one board's links — and what the
//! person asked for.

pub mod update_driver;
pub mod update_intent;

pub use update_driver::{DriverConfig, DriverEffect, Finish, Stage, StopReason, UpdateDriver};
pub use update_intent::{UpdateIntent, decide_for_intent};
