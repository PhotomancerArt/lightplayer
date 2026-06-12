//! Triple-buffered display pipeline for LED output
//!
//! Converts 16-bit RGB frames to 8-bit with optional white-point LUT,
//! dithering, and frame interpolation.

mod dither;
mod lut;
mod pipeline;

pub use lpc_hardware::DisplayPipelineOptions;
pub use pipeline::DisplayPipeline;
