//! Color space conversion functions.
//!
//! This module contains functions for converting between different color spaces,
//! such as RGB, HSV, and Oklab/Oklch.

#[cfg(feature = "float-f32")]
pub mod hsv2rgb_f32;
pub mod hsv2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod hue2rgb_f32;
pub mod hue2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod oklab2rgb_f32;
pub mod oklab2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod oklch2rgb_f32;
pub mod oklch2rgb_q32;
#[cfg(feature = "float-f32")]
pub mod rgb2hsv_f32;
pub mod rgb2hsv_q32;
#[cfg(feature = "float-f32")]
pub mod rgb2oklab_f32;
pub mod rgb2oklab_q32;
#[cfg(feature = "float-f32")]
pub mod rgb2oklch_f32;
pub mod rgb2oklch_q32;
