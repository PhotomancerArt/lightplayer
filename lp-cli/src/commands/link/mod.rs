//! `lp-cli link`: measure a board's host link.

pub mod args;
pub mod handler;
pub mod soak_text;
pub mod soak_verifier;

pub use args::LinkCli;
pub use handler::handle_link;
