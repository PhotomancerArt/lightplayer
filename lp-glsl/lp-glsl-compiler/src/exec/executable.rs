//! Executable GLSL module trait and implementations
//!
//! This module provides a trait-based API for executing GLSL functions that
//! abstracts away JIT vs Emulator implementations.

use crate::error::GlslError;
use crate::exec::glsl_value::GlslValue;
use crate::frontend::semantic::functions::FunctionSignature;

use alloc::{format, string::String, vec::Vec};

/// Information needed for direct function pointer calls
/// Contains the function pointer and calling convention details
#[derive(Clone, Copy)]
pub struct DirectCallInfo {
    /// Raw function pointer
    pub func_ptr: *const u8,
    /// Calling convention
    pub call_conv: cranelift_codegen::isa::CallConv,
    /// Pointer type (I32 for 32-bit, I64 for 64-bit)
    pub pointer_type: cranelift_codegen::ir::Type,
}

/// Trait for executing GLSL functions with various return types
/// Abstracts away JIT vs Emulator implementations
///
/// **Current State**: Supports basic function calling with in-parameters only.
/// Future extensions will add:
/// - Uniform variables (`set_uniform`, `get_uniform`, `list_uniforms`)
/// - Texture/sampler binding (`bind_texture`, `bind_sampler`)
/// - Built-in variables (`set_builtin`, e.g., `gl_Position`, `gl_FragCoord`)
/// - `out` and `inout` parameters
pub trait GlslExecutable {
    /// Call a function that returns void
    fn call_void(&mut self, name: &str, args: &[GlslValue]) -> Result<(), GlslError>;

    /// Call a function that returns i32
    fn call_i32(&mut self, name: &str, args: &[GlslValue]) -> Result<i32, GlslError>;

    /// Call a function that returns f32
    fn call_f32(&mut self, name: &str, args: &[GlslValue]) -> Result<f32, GlslError>;

    /// Call a function that returns bool
    fn call_bool(&mut self, name: &str, args: &[GlslValue]) -> Result<bool, GlslError>;

    /// Call a function that returns a boolean vector (bvec2, bvec3, or bvec4)
    /// `dim` is the dimension (2, 3, or 4)
    /// Returns a Vec<bool> with the boolean values
    fn call_bvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError>;

    /// Call a function that returns a signed integer vector (ivec2, ivec3, or ivec4)
    /// `dim` is the dimension (2, 3, or 4)
    /// Returns a Vec<i32> with the integer values (no fixed-point scaling)
    fn call_ivec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError>;

    /// Call a function that returns an unsigned integer vector (uvec2, uvec3, or uvec4)
    /// `dim` is the dimension (2, 3, or 4)
    /// Returns a Vec<u32> with the unsigned integer values (no fixed-point scaling)
    fn call_uvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError>;

    /// Call a function that returns a vector (vec2, vec3, or vec4)
    /// `dim` is the dimension (2, 3, or 4)
    fn call_vec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError>;

    /// Call a function that returns a matrix
    /// `rows` and `cols` specify the matrix dimensions (e.g., 2x2, 3x3, 4x4)
    /// Returns a flat vector in column-major order
    fn call_mat(
        &mut self,
        name: &str,
        args: &[GlslValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError>;

    /// Get the signature of a function by name
    fn get_function_signature(&self, name: &str) -> Option<&FunctionSignature>;

    /// List all available function names
    fn list_functions(&self) -> Vec<String>;

    /// Get emulator state as a formatted string, if this is an emulator module.
    /// Returns None for non-emulator implementations (e.g., JIT).
    #[cfg(feature = "std")]
    fn format_emulator_state(&self) -> Option<String> {
        None
    }

    /// Get CLIF IR (before and after transformation) as formatted strings, if available.
    /// Returns (original_ir, transformed_ir) where each is Some(String) if available.
    #[cfg(feature = "std")]
    fn format_clif_ir(&self) -> (Option<String>, Option<String>) {
        (None, None)
    }

    /// Get VCode as a formatted string, if available.
    #[cfg(feature = "std")]
    fn format_vcode(&self) -> Option<String> {
        None
    }

    /// Get disassembly as a formatted string, if available.
    #[cfg(feature = "std")]
    fn format_disassembly(&self) -> Option<String> {
        None
    }

    /// Get direct call information for a function, if available.
    /// Returns None for emulator modules or if the function is not found.
    /// This allows bypassing the GlslValue conversion overhead for JIT-compiled functions.
    fn get_direct_call_info(&self, _name: &str) -> Option<DirectCallInfo> {
        None
    }

    // TODO: Future extensions:
    // fn set_uniform(&mut self, name: &str, value: GlslValue) -> Result<(), GlslError>;
    // fn get_uniform(&self, name: &str) -> Option<&GlslValue>;
    // fn list_uniforms(&self) -> Vec<String>;
    // fn bind_texture(&mut self, unit: u32, texture: Texture) -> Result<(), GlslError>;
    // fn bind_sampler(&mut self, unit: u32, sampler: Sampler) -> Result<(), GlslError>;
    // fn set_builtin(&mut self, name: &str, value: GlslValue) -> Result<(), GlslError>;
}

/// Execution mode for GLSL compilation
#[derive(Debug, Clone)]
pub enum RunMode {
    /// JIT compile and execute on the host architecture
    HostJit,
    /// Emulate execution (currently RISC-V 32-bit only)
    /// Requires `emulator` feature flag to be enabled
    Emulator {
        /// Maximum memory size in bytes (RAM)
        max_memory: usize,
        /// Stack size in bytes
        stack_size: usize,
        /// Maximum instruction count before timeout
        max_instructions: u64,
        /// Log level for emulator execution (None for no logging, Instructions for instruction-level logging)
        #[cfg(feature = "emulator")]
        log_level: Option<lp_riscv_emu::LogLevel>,
    },
}

/// Decimal format for floating-point operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecimalFormat {
    /// Native floating-point (f32/f64)
    Float,
    /// Fixed-point 32-bit (Q format)
    Q32,
}

/// Compilation options
#[derive(Debug, Clone)]
pub struct GlslOptions {
    pub run_mode: RunMode,
    pub decimal_format: DecimalFormat,
    pub q32_opts: crate::backend::transform::q32::Q32Options,
}

impl GlslOptions {
    pub fn validate(&self) -> Result<(), GlslError> {
        use crate::error::{ErrorCode, GlslError};
        use target_lexicon::Triple;

        // Validate option combinations
        match (&self.run_mode, self.decimal_format) {
            (RunMode::Emulator { .. }, DecimalFormat::Float) => {
                // TODO: Float support will be added for riscv32_imafc in the future
                Err(GlslError::new(
                    ErrorCode::E0400,
                    "Float format not yet supported in emulator mode (will be supported for riscv32_imafc)",
                ))
            }
            (RunMode::HostJit, DecimalFormat::Float) => {
                // Check if host supports float by checking triple string
                let triple = Triple::host();
                let arch_str = format!("{:?}", triple.architecture);
                if arch_str.contains("riscv32") {
                    Err(GlslError::new(
                        ErrorCode::E0400,
                        "Float format not supported on RISC-V 32-bit",
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    /// Default options for JIT execution
    pub fn jit() -> Self {
        Self {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Float,
            q32_opts: crate::backend::transform::q32::Q32Options::default(),
        }
    }

    /// Default options for emulator execution
    pub fn emulator(max_memory: usize, stack_size: usize) -> Self {
        Self {
            run_mode: RunMode::Emulator {
                max_memory,
                stack_size,
                max_instructions: 10_000,
                #[cfg(feature = "emulator")]
                log_level: None,
            },
            decimal_format: DecimalFormat::Q32,
            q32_opts: crate::backend::transform::q32::Q32Options::default(),
        }
    }

    /// Convenience constructor for RISC-V 32-bit IMA(C) emulator
    /// Uses 1MB RAM, 64KB stack, and Q32 format
    #[cfg(feature = "emulator")]
    pub fn emu_riscv32_imac() -> Self {
        Self {
            run_mode: RunMode::Emulator {
                max_memory: 1024 * 1024, // 1MB
                stack_size: 64 * 1024,   // 64KB
                log_level: None,
                max_instructions: 10_000,
            },
            decimal_format: DecimalFormat::Q32,
            q32_opts: crate::backend::transform::q32::Q32Options::default(),
        }
    }
}

// ============================================================================
// Module implementations (in separate files)
// ============================================================================

// Re-export module types for convenience
// Note: GlslEmulatorModule is conditionally compiled and may not be used in all builds
#[cfg(feature = "emulator")]
#[allow(unused_imports, reason = "Conditionally compiled re-export")]
pub use crate::exec::emu::GlslEmulatorModule;
