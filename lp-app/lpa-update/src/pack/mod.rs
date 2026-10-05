//! The one packer of encoding 1, and its prover (feature `pack`).

pub mod pack_piece;

pub use pack_piece::{ProveError, pack_piece, prove_piece};
