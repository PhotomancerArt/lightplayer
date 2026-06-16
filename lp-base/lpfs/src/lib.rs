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

pub mod error;
pub mod fs_event;
pub mod impls;
pub mod lp_fs;
pub mod lp_fs_view;
pub mod lp_path;

pub use lp_path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};

pub use error::FsError;
#[allow(deprecated, reason = "legacy fs event type aliases for migration")]
pub use fs_event::{ChangeType, FsChange};
pub use fs_event::{FsEvent, FsEventKind, FsVersion};
pub use impls::lp_fs_mem::LpFsMemory;
pub use lp_fs::LpFs;
pub use lp_fs_view::LpFsView;

#[cfg(feature = "std")]
pub use impls::lp_fs_std::LpFsStd;
