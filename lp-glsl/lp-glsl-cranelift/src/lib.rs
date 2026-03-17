//! GLSL fragment shader compiler using Cranelift JIT.
//!
//! Phase 1: Basic architecture with int/bool support only.

#![no_std]

// Always declare alloc so we can use alloc::string::String etc. in both std and no_std modes

extern crate alloc;
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

pub mod frontend;

pub use lp_glsl_frontend::error;

// Backend2 module (public for filetests)
pub mod backend;
mod exec;

// Re-exports
pub use backend::q32::Q32Options;
#[cfg(feature = "emulator")]
pub use exec::GlslEmulatorModule;
pub use exec::GlslJitModule;
pub use exec::{DirectCallInfo, GlslExecutable, GlslOptions, GlslValue, RunMode};
pub use frontend::GlslCompiler;
pub use frontend::codegen;
pub use lp_glsl_frontend::pipeline::{
    Backend, CompilationPipeline, CompiledShader, ParseResult, SemanticResult, TransformationPass,
    parse_program_with_registry,
};
pub use lp_glsl_frontend::semantic;
pub use lp_glsl_frontend::{DEFAULT_MAX_ERRORS, FloatMode};

/// Type alias for convenience
pub type Compiler = GlslCompiler;
pub use lp_glsl_frontend::error::{ErrorCode, GlslDiagnostics, GlslError};
pub use lp_glsl_frontend::semantic::type_check::inference::infer_expr_type_in_context;

// Public API functions
pub use frontend::glsl_jit;
pub use frontend::glsl_jit_streaming;

#[cfg(feature = "emulator")]
pub use frontend::{glsl_emu_riscv32, glsl_emu_riscv32_with_metadata};

#[cfg(feature = "std")]
pub use exec::execute_fn::{execute_function, execute_main};
pub use lp_glsl_frontend::src_loc::GlSourceLoc;
