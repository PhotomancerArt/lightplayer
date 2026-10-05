//! Running a whole update, or a heal, over one board's links.

pub mod update_driver;

pub use update_driver::{DriverConfig, DriverEffect, Finish, Stage, StopReason, UpdateDriver};
