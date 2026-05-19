//! Triple-buffered display pipeline for LED output
//!
//! Converts 16-bit RGB frames to 8-bit with optional white-point LUT,
//! dithering, and frame interpolation.

mod dither;
mod lut;
mod options;
mod pipeline;

pub use options::DisplayPipelineOptions;
pub use pipeline::DisplayPipeline;
