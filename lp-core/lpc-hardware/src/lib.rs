//! Hardware capabilities, manifests, endpoint routing, and driver traits.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod display_pipeline_options;
pub mod hardware;
pub mod output_error;

pub use display_pipeline_options::DisplayPipelineOptions;
pub use output_error::OutputError;
