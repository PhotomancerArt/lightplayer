//! Q32 transform for backend2
//!
//! This module adapts the existing q32 transform to work with the new
//! backend2 architecture using the Transform trait.

mod converters;
mod instructions;
mod signature;
mod transform;
mod types;

#[cfg(test)]
mod q32_test_util;

pub use transform::Q32Transform;
pub use types::FixedPointFormat;
