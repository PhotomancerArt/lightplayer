//! Filesystem abstraction for LightPlayer.
//!
//! Provides the `LpFs` trait and a few implementations:
//! - `LpFsMemory`: in-memory filesystem (always available)
//! - `LpFsView`: chrooted view over another `LpFs`
//! - `LpFsStd`: host filesystem backed by `std::fs` (behind the `std` feature)
//!
//! All paths handled by these types are absolute paths relative to a project
//! root; see [`LpFs`] for the path contract.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod lpc_model_artifact;

pub mod error;
pub mod fs_event;
pub mod lp_fs;
pub mod lp_fs_mem;
#[cfg(feature = "std")]
pub mod lp_fs_std;
pub mod lp_fs_view;

pub use error::FsError;
pub use fs_event::{ChangeType, FsChange, FsVersion};
pub use lp_fs::LpFs;
pub use lp_fs_mem::LpFsMemory;
pub use lp_fs_view::LpFsView;

#[cfg(feature = "std")]
pub use lp_fs_std::LpFsStd;
