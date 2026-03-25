//! GLSL frontend: parsing, semantic analysis, types, errors.
//!
//! No Cranelift dependency. Use lp-glsl-cranelift for codegen.

#![no_std]

extern crate alloc;

pub mod error;
pub mod pipeline;
pub mod semantic;
pub mod src_loc;
pub mod src_loc_manager;

pub use pipeline::{
    Backend, CompilationPipeline, CompiledShader, ParseResult, SemanticResult, TransformationPass,
    parse_program_with_registry,
};

/// Default maximum number of errors to collect before stopping.
pub const DEFAULT_MAX_ERRORS: usize = 20;

pub use lpir::FloatMode;
