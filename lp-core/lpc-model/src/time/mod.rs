//! Time value shapes shared across node and shader slot definitions.
//!
//! [`crate::TimeProduct`] (the lazy graph handle carried on `bus:time`) lives
//! under [`crate::products::time`]; this module holds the plain, non-product
//! value shapes evaluated from it: `Seconds` (unbounded elapsed time) and
//! `PhasorConfig` (how a consumer wants that timebase read as a wrapped
//! `[0,1)` cycle position).

pub mod phasor_config;
pub mod seconds;

pub use phasor_config::{
    DEFAULT_PHASOR_PERIOD_SECONDS, PHASOR_CONFIG_SHAPE_NAME, PhasorConfig, WAVEFORM_SHAPE_NAME,
    Waveform,
};
pub use seconds::{SECONDS_SHAPE_NAME, seconds_shape, static_seconds_shape};
