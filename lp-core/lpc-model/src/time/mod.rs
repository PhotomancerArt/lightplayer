//! Time value shapes shared across node and shader slot definitions.
//!
//! [`crate::TimeProduct`] (the lazy graph handle carried on `bus:time`) lives
//! under [`crate::products::time`]; this module holds the plain, non-product
//! value shapes evaluated from it (`Seconds` today; a wrapped-phase `Phasor`
//! shape is future work — see the TimeProduct M2 planning notes).

pub mod seconds;

pub use seconds::{SECONDS_SHAPE_NAME, seconds_shape, static_seconds_shape};
