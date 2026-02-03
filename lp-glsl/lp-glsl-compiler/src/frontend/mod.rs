//! GLSL compilation logic.
//!
//! This module contains the core compilation components that transform GLSL source
//! into Cranelift IR, including parsing, semantic analysis, code generation, and linking.

pub(crate) mod glsl_compiler;
pub(crate) mod pipeline;
// Public modules
pub mod codegen;
pub mod semantic;
pub mod src_loc;
pub mod src_loc_manager;

// Re-exports used by crate root; suppress unused warnings within this module.
#[allow(unused_imports, reason = "Re-exports for crate root")]
pub use glsl_compiler::GlslCompiler;
#[allow(unused_imports, reason = "Re-exports for crate root")]
pub use pipeline::{
    Backend, CompilationPipeline, CompiledShader, ParseResult, SemanticResult, TransformationPass,
    parse_program_with_registry,
};

// ============================================================================
// Public API functions
// ============================================================================

#[cfg(feature = "emulator")]
use crate::backend::codegen::emu::EmulatorOptions;
use crate::backend::module::gl_module::GlModule;
use crate::backend::target::Target;
use crate::error::{ErrorCode, GlslError};
use crate::exec::executable::{GlslExecutable, GlslOptions, RunMode};
#[cfg(not(feature = "std"))]
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::JITModule;
#[cfg(feature = "emulator")]
use cranelift_object::ObjectModule;

use alloc::boxed::Box;
#[cfg(feature = "emulator")]
use alloc::string::String;

/// Compile GLSL to GlModule<JITModule> (internal, reusable)
/// This is the core compilation step for JIT execution
pub fn compile_glsl_to_gl_module_jit(
    source: &str,
    options: &GlslOptions,
) -> Result<GlModule<JITModule>, GlslError> {
    options.validate()?;
    use crate::exec::executable::DecimalFormat;

    #[cfg(feature = "std")]
    let mut compiler = GlslCompiler::new();

    // Determine target based on run mode
    #[cfg(feature = "std")]
    let target = match &options.run_mode {
        RunMode::HostJit => Target::host_jit()?,
        RunMode::Emulator { .. } => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "Emulator mode not supported for JIT compilation",
            ));
        }
    };

    #[cfg(not(feature = "std"))]
    let target = match &options.run_mode {
        RunMode::HostJit => {
            // In no_std mode, manually create HostJit target (riscv32 only)
            // Use the same approach as esp32-glsl-jit: create flags manually
            let mut flag_builder = settings::builder();
            flag_builder
                .set("is_pic", "false")
                .map_err(|e| GlslError::new(ErrorCode::E0400, alloc::format!("failed to set is_pic: {e}")))?;
            flag_builder
                .set("use_colocated_libcalls", "false")
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("failed to set use_colocated_libcalls: {e}"),
                    )
                })?;
            flag_builder
                .set("enable_multi_ret_implicit_sret", "true")
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("failed to set enable_multi_ret_implicit_sret: {e}"),
                    )
                })?;
            flag_builder
                .set("regalloc_algorithm", "single_pass")
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("failed to set regalloc_algorithm: {e}"),
                    )
                })?;
            let flags = settings::Flags::new(flag_builder);
            Target::HostJit {
                arch: None,
                flags,
                isa: None,
            }
        }
        RunMode::Emulator { .. } => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "Emulator mode not supported for JIT compilation",
            ));
        }
    };

    // Compile to GlModule (works in both std and no_std)
    let mut compiler = GlslCompiler::new();
    let mut module = compiler.compile_to_gl_module_jit(source, target)?;

    // Apply transformations
    match options.decimal_format {
        DecimalFormat::Q32 => {
            use crate::backend::transform::q32::{FixedPointFormat, Q32Transform};
            let transform = Q32Transform::new(FixedPointFormat::Fixed16x16);
            module = module.apply_transform(transform)?;
        }
        DecimalFormat::Float => {
            return Err(GlslError::new(
                crate::error::ErrorCode::E0400,
                "Float format is not yet supported. Only Q32 format is currently supported. \
                 Float format will cause TestCase relocation errors. Use Q32 format instead.",
            ));
        }
    }

    Ok(module)
}

/// Compile GLSL to GlModule<ObjectModule> (internal, reusable)
/// This is the core compilation step for emulator execution
/// Returns the module along with CLIF IR strings for debugging
#[cfg(feature = "emulator")]
pub fn compile_glsl_to_gl_module_object(
    source: &str,
    options: &GlslOptions,
) -> Result<(GlModule<ObjectModule>, Option<String>, Option<String>), GlslError> {
    #[cfg(feature = "std")]
    use crate::backend::util::clif_format::format_clif_module;
    use crate::exec::executable::DecimalFormat;

    options.validate()?;

    let mut compiler = GlslCompiler::new();

    // Determine target based on run mode
    let target = match &options.run_mode {
        RunMode::Emulator { .. } => Target::riscv32_emulator()?,
        RunMode::HostJit => {
            return Err(GlslError::new(
                crate::error::ErrorCode::E0400,
                "HostJit mode not supported for object compilation",
            ));
        }
    };

    // Compile to GlModule
    let mut module = compiler.compile_to_gl_module_object(source, target)?;

    // Capture original CLIF IR before transformation (only in std builds)
    #[cfg(feature = "std")]
    let original_clif = format_clif_module(&module).ok();
    #[cfg(not(feature = "std"))]
    let original_clif = None;

    // Apply transformations
    let transformed_clif = match options.decimal_format {
        DecimalFormat::Q32 => {
            use crate::backend::transform::q32::{FixedPointFormat, Q32Transform};
            let transform = Q32Transform::new(FixedPointFormat::Fixed16x16);
            module = module.apply_transform(transform)?;
            // Capture transformed CLIF IR after transformation (only in std builds)
            #[cfg(feature = "std")]
            {
                format_clif_module(&module).ok()
            }
            #[cfg(not(feature = "std"))]
            {
                None
            }
        }
        DecimalFormat::Float => {
            // No transformation needed, so transformed_clif is same as original_clif
            #[cfg(feature = "std")]
            {
                original_clif.clone()
            }
            #[cfg(not(feature = "std"))]
            {
                None
            }
        }
    };

    Ok((module, original_clif, transformed_clif))
}

/// Compile and JIT execute GLSL
/// Works in both std and no_std environments
pub fn glsl_jit(source: &str, options: GlslOptions) -> Result<Box<dyn GlslExecutable>, GlslError> {
    let module = compile_glsl_to_gl_module_jit(source, &options)?;
    module.build_executable()
}

/// Compile and execute GLSL in RISC-V 32-bit emulator
/// Requires `emulator` feature flag to be enabled
#[cfg(feature = "emulator")]
pub fn glsl_emu_riscv32(
    source: &str,
    options: GlslOptions,
) -> Result<Box<dyn GlslExecutable>, GlslError> {
    glsl_emu_riscv32_with_metadata(source, options, None)
}

/// Requires `emulator` feature flag to be enabled
/// Version that accepts source file path for better error messages
#[cfg(feature = "emulator")]
pub fn glsl_emu_riscv32_with_metadata(
    source: &str,
    options: GlslOptions,
    source_file_path: Option<String>,
) -> Result<Box<dyn GlslExecutable>, GlslError> {
    // Compile to GlModule (transformations already applied)
    let (module, original_clif, transformed_clif) =
        compile_glsl_to_gl_module_object(source, &options)?;

    let emulator_options = match &options.run_mode {
        RunMode::Emulator {
            max_memory,
            stack_size,
            max_instructions,
            log_level,
        } => {
            use lp_riscv_emu::LogLevel;
            EmulatorOptions {
                max_memory: *max_memory,
                stack_size: *stack_size,
                max_instructions: *max_instructions,
                log_level: log_level.unwrap_or(LogLevel::None),
            }
        }
        _ => {
            return Err(GlslError::new(
                crate::error::ErrorCode::E0400,
                "Invalid run mode for emulator",
            ));
        }
    };

    // Note: source_file_path is stored in GlModule but not currently used in build_emu_executable
    // This can be added later if needed
    let _ = source_file_path;

    module.build_executable(&emulator_options, original_clif, transformed_clif)
}
