//! Triple-buffered display pipeline for LED output
//!
//! Converts 16-bit RGB frames to 8-bit with optional LUT (gamma + white point),
//! dithering, and frame interpolation.

mod dither;
mod lut;
mod options;
mod pipeline;

pub use options::DisplayPipelineOptions;
pub use pipeline::DisplayPipeline;
