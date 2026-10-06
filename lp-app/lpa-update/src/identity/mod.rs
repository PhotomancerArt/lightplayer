//! A board's identity against its release's (one-way-doors §2).

pub mod board_release_identity;

pub use board_release_identity::{IdentityMismatch, board_matches_release};
