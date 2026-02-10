//! GLSL fragment shader compiler using Cranelift JIT.
//!
//! Phase 1: Basic architecture with int/bool support only.

#![no_std]

// Always declare alloc so we can use alloc::string::String etc. in both std and no_std modes

extern crate alloc;
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

pub mod error;
pub mod frontend;

// Backend2 module (public for filetests)
pub mod backend;
mod exec;

// Re-exports
pub use backend::transform::q32::Q32Options;
#[cfg(feature = "emulator")]
pub use exec::GlslEmulatorModule;
pub use exec::GlslJitModule;
pub use exec::{DecimalFormat, GlslExecutable, GlslOptions, GlslValue, RunMode};
pub use frontend::codegen;
pub use frontend::semantic;
pub use frontend::{
    Backend, CompilationPipeline, CompiledShader, GlslCompiler, ParseResult, SemanticResult,
    TransformationPass, parse_program_with_registry,
};

/// Type alias for convenience
pub type Compiler = GlslCompiler;
pub use error::{ErrorCode, GlslError};
pub use frontend::semantic::type_check::inference::infer_expr_type_in_context;

// Public API functions
pub use frontend::glsl_jit;

#[cfg(feature = "emulator")]
pub use frontend::{glsl_emu_riscv32, glsl_emu_riscv32_with_metadata};

#[cfg(feature = "std")]
pub use exec::execute_fn::{execute_function, execute_main};
pub use frontend::src_loc::GlSourceLoc;
