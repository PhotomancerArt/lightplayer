//! Temporary slot-model pressure crate.
//!
//! This crate defines fake LightPlayer-ish domain objects and forces them
//! through the real slot APIs in `lpc-model`.

pub mod engine;
pub mod model;
pub mod source;
pub mod view;
pub mod wire;

#[cfg(test)]
mod tests;
