//! `lp-cli wire`: tools over the bytes a board writes.

pub mod args;
pub mod handler;
pub mod tap_unpack;

pub use args::WireCli;
pub use handler::handle_wire;
