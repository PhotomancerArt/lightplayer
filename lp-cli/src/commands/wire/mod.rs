//! `lp-cli wire`: tools over the bytes a board writes.

pub mod args;
pub mod handler;
pub mod line_unpack;
pub mod link_unpack;
pub mod tap_unpack;
#[cfg(test)]
pub(crate) mod test_capture;
pub mod unpack_report;

pub use args::WireCli;
pub use handler::handle_wire;
