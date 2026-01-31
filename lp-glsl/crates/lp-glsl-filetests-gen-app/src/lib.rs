//! Library interface for lp-glsl-filetests-gen-app.

pub mod cli;
pub mod expand;
pub mod generator;
pub mod types;
pub mod util;
pub mod vec;

// Re-export commonly used types
pub use generator::TestSpec;
pub use types::{Dimension, VecType};
