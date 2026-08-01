//! Chip-generic output layer: the trait-driven output provider and the
//! lock-free RMT channel state shared with chip-side interrupt handlers.

pub mod provider;
pub mod rmt_state;
