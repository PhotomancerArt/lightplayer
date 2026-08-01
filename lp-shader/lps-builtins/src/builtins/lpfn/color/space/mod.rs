//! Color space conversion functions.
//!
//! This module contains functions for converting between different color spaces,
//! such as RGB, HSV, HSL, etc.

#[cfg(feature = "float-f32")]
pub mod hsv2rgb_f32;
pub mod hsv2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod hue2rgb_f32;
pub mod hue2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod rgb2hsv_f32;
pub mod rgb2hsv_q32;
