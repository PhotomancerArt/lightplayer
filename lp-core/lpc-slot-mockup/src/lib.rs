//! Temporary slot-model pressure crate.
//!
//! This crate defines fake LightPlayer-ish domain objects and forces them
//! through the real slot APIs in `lpc-model`.

pub mod engine;
pub mod model;
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
pub mod source;
pub mod wire;

#[cfg(test)]
mod tests;
