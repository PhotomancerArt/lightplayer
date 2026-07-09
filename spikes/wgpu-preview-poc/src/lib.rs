//! Spike (M3, GPU preview roadmap): prove the GPU preview pipeline on real
//! authored shaders at f32 semantics.
//!
//! Pipeline under test:
//!
//! ```text
//! authored GLSL + M2 canonical lpfn GLSL prelude + generated wrapper main()
//!   → naga glsl-in → naga validation → naga wgsl-out
//!   → wgpu fragment shader on a fullscreen triangle → offscreen texture
//!   → readback → frame diff vs the authoritative Q32 wasm path
//! ```
//!
//! **Not intended for production.** wgpu stays a spike-only dependency; the
//! production backend design is M4's deliverable.

pub mod corpus;
pub mod diff;
pub mod glsl_to_wgsl;
pub mod gpu;
pub mod prelude;
pub mod reference;

pub use corpus::{CORPUS, CorpusShader};
pub use diff::{DiffStats, diff_frames, quantize_gpu_frame};
pub use glsl_to_wgsl::{FrontendTimings, GlslToWgsl, UniformSlot, UniformValue};
pub use gpu::{GpuFrameRenderer, GpuTimings};
pub use prelude::assemble_prelude;
pub use reference::ReferenceRenderer;
