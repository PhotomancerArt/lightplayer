//! A deterministic, sans-IO model of SPI NOR flash that loses power on demand.
//!
//! [`NorFlashSim`] holds the cells of a NOR part (erase sets a sector to all
//! ones, a program can only clear bits), counts every program page and every
//! sector erase as one *operation*, and — when a [`FaultPlan`] says so — tears
//! the in-flight operation per a [`TearModel`] and refuses everything after it
//! with [`NorError::PowerLost`] until [`NorFlashSim::power_cycle`].
//!
//! It is the ground the storage testbed (`tools/lp-store-bench`) runs every
//! candidate store on. Nothing here reads a clock or ambient randomness: every
//! random choice comes from the plan's seed. See the README for what is and is
//! not modelled.

#![no_std]

extern crate alloc;

mod embedded_storage_impl;
mod fault_plan;
mod nor_error;
mod nor_flash_sim;
mod nor_geometry;
mod nor_sector_state;
mod nor_stats;
mod sim_rng;
mod wear_out;

pub use embedded_storage_impl::NorSimFlashError;
pub use fault_plan::{FaultPlan, TearModel};
pub use nor_error::NorError;
pub use nor_flash_sim::NorFlashSim;
pub use nor_geometry::NorGeometry;
pub use nor_sector_state::NorSectorState;
pub use nor_stats::NorStats;
pub use sim_rng::SimRng;
pub use wear_out::{WearMode, WearOut};
