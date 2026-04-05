//! Worley noise (cellular noise) functions.
//!
//! Worley noise generates cellular patterns based on the distance to the nearest
//! feature point in a grid. This implementation uses Q32 fixed-point arithmetic.
//!
//! Reference: noise-rs library (https://github.com/Razaekel/noise-rs)

pub mod worley2_f32;
pub mod worley2_q32;
pub mod worley2_value_f32;
pub mod worley2_value_q32;
pub mod worley3_f32;
pub mod worley3_q32;
pub mod worley3_value_f32;
pub mod worley3_value_q32;
