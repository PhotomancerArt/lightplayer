//! **lpa-update**: the host side of an over-the-air update, protocol v1
//! (`lpc-update`). Studio (wasm, M7) and `lp-cli` (Part B; the firmware
//! distribution's packaging) share it.
//!
//! - [`board_view`]: a board's manifest and the facts derived from it;
//! - [`host_build`]: a build the host holds, from parts;
//! - [`serve`]: answering a board's requests, with send-ahead and `Z`;
//! - [`backup`]: reading the board's running engine back;
//! - [`login`]: answering a core's login challenge;
//! - `pack` (feature `pack`, std): **the one packer** of encoding 1 — a
//!   piece's `.z` stream and its chunk-length index — and its prover.
//!
//! Sans-IO, `no_std` + `alloc` (except under `pack`): no clock (time is a
//! caller's `now_ms`), no IO, no executor. It emits decisions, stages and
//! effects, never UI actions or copy (DM31). It never depends on the
//! firmware-distribution plan's crates: engine sources are effects the edge
//! resolves.

#![no_std]

extern crate alloc;
#[cfg(feature = "pack")]
extern crate std;

pub mod backup;
pub mod board_view;
pub mod encoded_piece;
pub mod host_build;
pub mod host_refusal;
pub mod login;
#[cfg(feature = "pack")]
pub mod pack;
pub mod serve;

pub use backup::{BackupError, BackupSession, BackupStep};
pub use board_view::{BoardView, Updating};
pub use encoded_piece::{EncodedPiece, EncodedPieceError};
pub use host_build::{HostBuild, HostBuildError, HostIdentity, HostPiece};
pub use host_refusal::HostRefusal;
pub use login::{Credential, LoginClient, LoginEvent};
pub use serve::{ServeConfig, ServeCounters, ServeEvent, ServeOutput, ServeSession};
