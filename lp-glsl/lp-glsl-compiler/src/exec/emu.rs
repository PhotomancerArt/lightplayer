//! Emulator-based GLSL module implementation
//!
//! This module provides the RISC-V 32-bit emulator execution backend for GLSL functions.
//! Requires `emulator` feature flag to be enabled.

use crate::error::GlslError;
use crate::exec::executable::GlslExecutable;
use crate::exec::glsl_value::GlslValue;
use crate::frontend::semantic::functions::FunctionSignature;
use crate::frontend::src_loc::GlSourceMap;
use hashbrown::HashMap;
use lp_riscv_emu::{EmulatorError, trap_code_to_string};
use lp_riscv_inst::format_instruction;

use alloc::{format, string::String, vec::Vec};

/// Emulator-based GLSL module (executes in RISC-V emulator)
/// Requires `emulator` feature flag to be enabled
/// Currently only supports calling `main` with no arguments
#[cfg(feature = "emulator")]
pub struct GlslEmulatorModule {
    pub(crate) emulator: lp_riscv_emu::Riscv32Emulator,
    pub(crate) signatures: HashMap<String, FunctionSignature>,
    // Store Cranelift signatures for proper function calling with arguments
    pub(crate) cranelift_signatures: HashMap<String, cranelift_codegen::ir::Signature>,
    pub(crate) binary: Vec<u8>,
    // Function address map: function name -> address (populated from object file symbol map)
    pub(crate) function_addresses: HashMap<String, u32>,
    // Store formatted CLIF IR for all functions after transformation
    pub(crate) transformed_clif: Option<String>,
    // Store formatted CLIF IR for all functions before transformation
    pub(crate) original_clif: Option<String>,
    // Store VCode for all functions (used in error diagnostics)
    pub(crate) vcode: Option<String>,
    // Store disassembly for all functions (used in error diagnostics)
    pub(crate) disassembly: Option<String>,
    // Store trap source information for error reporting: (absolute_offset, trap_code, srcloc, func_name)
    pub(crate) trap_source_info: Vec<(
        u32,
        cranelift_codegen::ir::TrapCode,
        cranelift_codegen::ir::SourceLoc,
        String,
    )>,
    // Source location manager for mapping SourceLoc to GLSL source positions
    pub(crate) source_loc_manager: crate::frontend::src_loc_manager::SourceLocManager,
    // Source map for managing file locations
    pub(crate) source_map: GlSourceMap,
    // Track next buffer allocation address (allocated from start of RAM, growing upward)
    #[allow(
        dead_code,
        reason = "Reserved for future use when manual buffer allocation is needed"
    )]
    pub(crate) next_buffer_addr: u32,
}

#[cfg(feature = "emulator")]
impl GlslEmulatorModule {
    /// Get the main file ID from the source map (typically the first file added)
    fn get_main_file_id(&self) -> Option<crate::frontend::src_loc::GlFileId> {
        // The main file is typically the first file added, which gets GlFileId(1)
        // Check if GlFileId(1) exists, otherwise try to find the first available file
        let main_file_id = crate::frontend::src_loc::GlFileId(1);
        if self.source_map.get_file(main_file_id).is_some() {
            Some(main_file_id)
        } else {
            // Fallback: try to find any file (shouldn't happen in normal cases)
            // Since we can't iterate files directly, we'll try a few IDs
            for i in 1..=10 {
                let file_id = crate::frontend::src_loc::GlFileId(i);
                if self.source_map.get_file(file_id).is_some() {
                    return Some(file_id);
                }
            }
            None
        }
    }

    /// Validate function exists and get its address
    fn get_function_address(&self, name: &str) -> Result<u32, GlslError> {
        use crate::error::ErrorCode;
        self.function_addresses.get(name).copied().ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function '{name}' not found in object file"),
            )
        })
    }

    /// Validate function signature exists
    fn get_function_signature(
        &self,
        name: &str,
    ) -> Result<&cranelift_codegen::ir::Signature, GlslError> {
        use crate::error::ErrorCode;
        self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })
    }

    /// Convert GlslValue to DataValue for emulator function calls
    fn glsl_value_to_data_value(
        &self,
        value: &GlslValue,
        sig: &cranelift_codegen::ir::Signature,
        arg_idx: &mut usize,
    ) -> Result<Vec<cranelift_codegen::data_value::DataValue>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::data_value::DataValue;
        use cranelift_codegen::ir::types;

        let mut args = Vec::new();

        if *arg_idx >= sig.params.len() {
            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
        }

        match value {
            GlslValue::I32(v) => {
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::I32 => args.push(DataValue::I32(*v)),
                    types::I64 => args.push(DataValue::I64(*v as i64)),
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!("Type mismatch: expected {param_ty:?}, got I32"),
                        ));
                    }
                }
                *arg_idx += 1;
            }
            GlslValue::F32(v) => {
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::F32 => {
                        use cranelift_codegen::ir::immediates::Ieee32;
                        args.push(DataValue::F32(Ieee32::with_bits(v.to_bits())));
                    }
                    types::I32 => {
                        // Convert f32 to fixed-point i32
                        let fixed =
                            (*v * crate::frontend::codegen::constants::FIXED16X16_SCALE) as i32;
                        args.push(DataValue::I32(fixed));
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!("Type mismatch: expected {param_ty:?}, got F32"),
                        ));
                    }
                }
                *arg_idx += 1;
            }
            GlslValue::Bool(v) => {
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::I8 => args.push(DataValue::I8(if *v { 1 } else { 0 })),
                    types::I32 => args.push(DataValue::I32(if *v { 1 } else { 0 })),
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!("Type mismatch: expected {param_ty:?}, got Bool"),
                        ));
                    }
                }
                *arg_idx += 1;
            }
            GlslValue::Vec2(v) => {
                // Expand vec2 into 2 f32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => {
                            use cranelift_codegen::ir::immediates::Ieee32;
                            args.push(DataValue::F32(Ieee32::with_bits(component.to_bits())));
                        }
                        types::I32 => {
                            // Convert f32 to fixed-point i32
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(DataValue::I32(fixed));
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for vec2 component, got F32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::Vec3(v) => {
                // Expand vec3 into 3 f32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => {
                            use cranelift_codegen::ir::immediates::Ieee32;
                            args.push(DataValue::F32(Ieee32::with_bits(component.to_bits())));
                        }
                        types::I32 => {
                            // Convert f32 to fixed-point i32
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(DataValue::I32(fixed));
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for vec3 component, got F32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::Vec4(v) => {
                // Expand vec4 into 4 f32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => {
                            use cranelift_codegen::ir::immediates::Ieee32;
                            args.push(DataValue::F32(Ieee32::with_bits(component.to_bits())));
                        }
                        types::I32 => {
                            // Convert f32 to fixed-point i32
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(DataValue::I32(fixed));
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for vec4 component, got F32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::IVec2(v) => {
                // Expand ivec2 into 2 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component)),
                        types::I64 => args.push(DataValue::I64(*component as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for ivec2 component, got I32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::IVec3(v) => {
                // Expand ivec3 into 3 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component)),
                        types::I64 => args.push(DataValue::I64(*component as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for ivec3 component, got I32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::IVec4(v) => {
                // Expand ivec4 into 4 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component)),
                        types::I64 => args.push(DataValue::I64(*component as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for ivec4 component, got I32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::UVec2(v) => {
                // Expand uvec2 into 2 i32 arguments (u32 passed as i32 in calling convention)
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component as i32)), // u32 passed as i32
                        types::I64 => args.push(DataValue::I64(*component as u64 as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for uvec2 component, got U32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::UVec3(v) => {
                // Expand uvec3 into 3 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component as i32)),
                        types::I64 => args.push(DataValue::I64(*component as u64 as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for uvec3 component, got U32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::UVec4(v) => {
                // Expand uvec4 into 4 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(DataValue::I32(*component as i32)),
                        types::I64 => args.push(DataValue::I64(*component as u64 as i64)),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for uvec4 component, got U32"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::BVec2(v) => {
                // Expand bvec2 into 2 i32 arguments (bool passed as i32)
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(DataValue::I8(if *component { 1 } else { 0 })),
                        types::I32 => args.push(DataValue::I32(if *component { 1 } else { 0 })),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for bvec2 component, got Bool"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::BVec3(v) => {
                // Expand bvec3 into 3 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(DataValue::I8(if *component { 1 } else { 0 })),
                        types::I32 => args.push(DataValue::I32(if *component { 1 } else { 0 })),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for bvec3 component, got Bool"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            GlslValue::BVec4(v) => {
                // Expand bvec4 into 4 i32 arguments
                for component in v.iter() {
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(DataValue::I8(if *component { 1 } else { 0 })),
                        types::I32 => args.push(DataValue::I32(if *component { 1 } else { 0 })),
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "Type mismatch: expected {param_ty:?} for bvec4 component, got Bool"
                                ),
                            ));
                        }
                    }
                    *arg_idx += 1;
                }
            }
            // Matrices not yet supported as arguments
            GlslValue::Mat2x2(_) | GlslValue::Mat3x3(_) | GlslValue::Mat4x4(_) => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "Matrix arguments not yet supported in emulator calls",
                ));
            }
            GlslValue::U32(_) => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "U32 scalar arguments not yet supported in emulator calls (use UVec2/3/4 for vectors)",
                ));
            }
        }

        Ok(args)
    }

    /// Allocate a buffer in the emulator's RAM and return its address.
    /// Buffers are allocated from the start of RAM (growing upward), leaving space
    /// for the stack at the end (growing downward).
    #[allow(
        dead_code,
        reason = "Reserved for future use when manual buffer allocation is needed"
    )]
    fn allocate_buffer_in_ram(&mut self, size: usize) -> Result<u32, GlslError> {
        // DEFAULT_RAM_START is 0x80000000 (from lp-riscv-emu/src/emu/memory.rs)
        const DEFAULT_RAM_START: u32 = 0x80000000;

        // Get current RAM size
        let current_len = self.emulator.memory().ram().len();

        // Ensure buffer size is 4-byte aligned
        let aligned_size = (size + 3) & !3;

        // Calculate buffer address (from next_buffer_addr, growing upward)
        let buffer_addr = self.next_buffer_addr;

        // Check if buffer would exceed RAM bounds
        // Leave at least 64KB for the stack at the end of RAM
        const STACK_RESERVE: usize = 64 * 1024;
        let max_buffer_end = DEFAULT_RAM_START as usize + current_len - STACK_RESERVE;
        let buffer_end = buffer_addr as usize + aligned_size;

        if buffer_end > max_buffer_end {
            return Err(GlslError::new(
                crate::error::ErrorCode::E0400,
                format!(
                    "Buffer allocation would exceed RAM size (need {aligned_size} bytes at addr 0x{buffer_addr:x}, have {current_len} bytes RAM with {STACK_RESERVE} bytes reserved for stack)"
                ),
            ));
        }

        // Initialize the buffer area with zeros by writing to each word
        for i in 0..(aligned_size / 4) {
            let addr = buffer_addr + (i * 4) as u32;
            self.emulator
                .memory_mut()
                .write_word(addr, 0)
                .map_err(|e| {
                    GlslError::new(
                        crate::error::ErrorCode::E0400,
                        format!("Failed to initialize buffer at 0x{addr:x}: {e:?}"),
                    )
                })?;
        }

        // Update next_buffer_addr for next allocation
        self.next_buffer_addr = buffer_addr + aligned_size as u32;

        Ok(buffer_addr)
    }

    /// Build an enhanced error message with CLIF IR and assembly
    fn build_enhanced_error(
        &self,
        code: crate::error::ErrorCode,
        base_message: &str,
        function_name: &str,
    ) -> GlslError {
        use alloc::string::ToString;
        // Include function name in error message for better context
        let full_message = if function_name.is_empty() {
            base_message.to_string()
        } else {
            format!("{base_message} (function: {function_name})")
        };
        let mut error = GlslError::new(code, full_message);

        // Add CLIF IR if available (both before and after transformation)
        if let Some(ref original_clif) = self.original_clif {
            error = error.with_note(format!(
                "=== CLIF IR (BEFORE transformation) ===\n{original_clif}"
            ));
        }

        if let Some(ref transformed_clif) = self.transformed_clif {
            error = error.with_note(format!(
                "=== CLIF IR (AFTER transformation) ===\n{transformed_clif}"
            ));
        }

        // Add VCode and disassembly if available (from compilation)
        if let Some(ref vcode) = self.vcode {
            error = error.with_note(format!("=== VCode ===\n{vcode}"));
        }

        if let Some(ref disassembly) = self.disassembly {
            error = error.with_note(format!("=== Disassembled ===\n{disassembly}"));
        }

        // Fall back to runtime disassembly if stored disassembly not available
        if self.vcode.is_none() && self.disassembly.is_none() {
            if let Ok(disasm) = self.disassemble_binary() {
                error = error.with_note(format!("=== Assembly Disassembly ===\n{disasm}"));
            }
        }

        error
    }

    /// Format source lines around a trap location for error display
    fn format_source_lines_around_trap(
        &self,
        file_id: crate::frontend::src_loc::GlFileId,
        trap_line: usize,
        trap_column: usize,
    ) -> Option<String> {
        let source_file = self.source_map.get_file(file_id)?;
        let source_text = &source_file.contents;
        let lines: Vec<&str> = source_text.lines().collect();

        if trap_line == 0 || trap_line > lines.len() {
            return None;
        }

        let start_line = trap_line.saturating_sub(3).max(1);
        let end_line = (trap_line + 3).min(lines.len());
        let source_lines: Vec<&str> = lines[start_line.saturating_sub(1)..end_line].to_vec();

        let mut source_display = String::new();
        for (idx, line) in source_lines.iter().enumerate() {
            let line_num = start_line + idx;
            if line_num == trap_line {
                source_display.push_str(&format!("{line_num:>3} | {line}\n"));
                // Bound check col_pos to prevent excessive string allocation
                let col_pos = trap_column.saturating_sub(1).min(line.len()).min(200);
                source_display.push_str(&format!(
                    "    | {}^ trap occurred here\n",
                    " ".repeat(col_pos)
                ));
            } else {
                source_display.push_str(&format!("{line_num:>3} | {line}\n"));
            }
        }
        Some(String::from(source_display.trim_end()))
    }

    /// Format a trap error in Rust style with source location information
    /// Takes trap code, PC, and regs directly from EmulatorError::Trap variant
    fn format_trap_error_from_emulator_error(
        &self,
        code: cranelift_codegen::ir::TrapCode,
        pc: u32,
        _regs: &[i32; 32],
        function_name: &str,
    ) -> GlslError {
        use crate::error::ErrorCode;

        // Find the trap source information for this PC
        let trap_info = self
            .trap_source_info
            .iter()
            .find(|(trap_pc, _, _, _)| *trap_pc == pc);

        let (func_name, trap_code, srcloc) =
            if let Some((_trap_pc, stored_code, stored_srcloc, stored_func_name)) = trap_info {
                (stored_func_name.as_str(), *stored_code, *stored_srcloc)
            } else {
                // Fallback if trap info not found - use the code from the error
                // This means the PC doesn't match any trap_info entry, which shouldn't happen
                // but can occur if there's a mismatch between trap addresses
                (
                    function_name,
                    code,
                    cranelift_codegen::ir::SourceLoc::default(),
                )
            };

        let trap_name = trap_code_to_string(trap_code);

        let mut error = GlslError::new(ErrorCode::E0400, format!("execution trapped: {trap_name}"));

        // Try to find source location from SourceLoc
        // First, try the exact srcloc from trap_info
        let mut found_location = None;
        let mut found_span_text = None;

        // Get the main file ID once and reuse it
        let file_id = self.get_main_file_id().unwrap_or_else(|| {
            crate::frontend::src_loc::GlFileId(1) // Fallback to ID 1 if helper fails
        });

        if let Some((trap_line, trap_column)) = self.source_loc_manager.lookup_srcloc(srcloc) {
            // Found exact match
            let trap_location =
                crate::frontend::src_loc::GlSourceLoc::new(file_id, trap_line, trap_column);
            found_location = Some(trap_location);

            // Extract source lines
            found_span_text = self.format_source_lines_around_trap(file_id, trap_line, trap_column);
        }

        // Fallback: if location lookup failed, try closest trap_info entry
        if found_location.is_none() {
            // Try to find closest trap_info entry (either because srcloc is default or lookup failed)
            if let Some((_closest_pc, _, closest_srcloc, _)) = self
                .trap_source_info
                .iter()
                .min_by_key(|(trap_pc, _, _, _)| (*trap_pc as i64 - pc as i64).abs())
            {
                if !closest_srcloc.is_default() {
                    if let Some((trap_line, trap_column)) =
                        self.source_loc_manager.lookup_srcloc(*closest_srcloc)
                    {
                        let trap_location = crate::frontend::src_loc::GlSourceLoc::new(
                            file_id,
                            trap_line,
                            trap_column,
                        );
                        found_location = Some(trap_location);

                        // Extract source lines
                        found_span_text =
                            self.format_source_lines_around_trap(file_id, trap_line, trap_column);
                    }
                }
            }
        }

        // Apply found location and span_text to error
        if let Some(location) = found_location {
            error = error.with_location(location);
        }
        if let Some(span_text) = found_span_text {
            error = error.with_span_text(span_text);
        }

        // Add trap details as notes
        error = error.with_note(format!("Trap occurred at PC 0x{pc:08x}"));
        if !func_name.is_empty() {
            error = error.with_note(format!("Function: {func_name}"));
        }

        // Add CLIF IR (before and after transformation) if available
        if let Some(ref original_clif) = self.original_clif {
            error = error.with_note(format!(
                "=== CLIF IR (BEFORE transformation) ===\n{original_clif}"
            ));
        }

        if let Some(ref transformed_clif) = self.transformed_clif {
            error = error.with_note(format!(
                "=== CLIF IR (AFTER transformation) ===\n{transformed_clif}"
            ));
        }

        // Add VCode and disassembly if available
        if let Some(ref vcode) = self.vcode {
            error = error.with_note(format!("VCode:\n{vcode}"));
        }

        if let Some(ref disassembly) = self.disassembly {
            error = error.with_note(format!("Disassembled:\n{disassembly}"));
        }

        error
    }

    /// Safely format a function, avoiding panics from Display
    #[allow(dead_code, reason = "Reserved for future use in error reporting")]
    fn format_function_safely(&self, func: &cranelift_codegen::ir::Function) -> String {
        #[cfg(feature = "std")]
        {
            use std::panic;
            // Try full formatting first
            match panic::catch_unwind(panic::AssertUnwindSafe(|| format!("{func}"))) {
                Ok(s) => return s,
                Err(_) => {
                    // Fall through to manual formatting
                }
            }
        }

        // Manual formatting fallback - build CLIF IR string by iterating blocks
        let mut result = String::new();
        result.push_str(&format!("function {} ({})", func.name, func.signature));

        // Add stack slots info
        if !func.sized_stack_slots.is_empty() {
            result.push_str(&format!(
                "\n  stack slots: {}",
                func.sized_stack_slots.len()
            ));
            for (slot, data) in func.sized_stack_slots.iter() {
                result.push_str(&format!(
                    "\n    {}: size={}, align={}",
                    slot,
                    data.size,
                    1u32 << data.align_shift
                ));
            }
        }

        // Add blocks and instructions
        result.push_str("\n");
        for block in func.layout.blocks() {
            result.push_str(&format!("\n{block}"));

            // Block parameters
            let params = func.dfg.block_params(block);
            if !params.is_empty() {
                result.push_str("(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&format!("{}: {}", param, func.dfg.value_type(*param)));
                }
                result.push_str("):");
            } else {
                result.push_str(":");
            }

            // Instructions
            for inst in func.layout.block_insts(block) {
                result.push_str("\n  ");

                // Try to format instruction safely
                #[cfg(feature = "std")]
                {
                    use std::panic;
                    match panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        format!("{}", func.dfg.display_inst(inst))
                    })) {
                        Ok(s) => {
                            result.push_str(&s);
                            continue;
                        }
                        Err(_) => {
                            // Fall through to manual formatting
                        }
                    }
                }

                // Manual instruction formatting fallback
                let inst_data = &func.dfg.insts[inst];
                result.push_str(&format!("{:?}", inst_data.opcode()));
                let inst_results = func.dfg.inst_results(inst);
                if !inst_results.is_empty() {
                    result.push_str(" -> ");
                    for (i, res) in inst_results.iter().enumerate() {
                        if i > 0 {
                            result.push_str(", ");
                        }
                        result.push_str(&format!("v{res}"));
                    }
                }
            }
        }

        result
    }

    /// Disassemble the binary to assembly
    fn disassemble_binary(&self) -> Result<String, String> {
        let mut disasm = String::new();
        let mut offset = 0usize;

        // Disassemble up to a reasonable limit (first 2048 bytes or until binary ends)
        let limit = core::cmp::min(self.binary.len(), 2048);
        let mut zero_run_start: Option<usize> = None;
        const MAX_ZERO_RUN: usize = 16; // Show up to 16 consecutive zeros before summarizing

        while offset < limit {
            if offset + 4 > self.binary.len() {
                break;
            }

            // Read 4-byte instruction (RISC-V 32-bit, little-endian)
            let mut inst_bytes = [0u8; 4];
            inst_bytes.copy_from_slice(&self.binary[offset..offset + 4]);
            let instruction = u32::from_le_bytes(inst_bytes);

            if instruction == 0 {
                // Track zero runs
                if zero_run_start.is_none() {
                    zero_run_start = Some(offset);
                }
            } else {
                // Non-zero instruction - flush any pending zero run
                if let Some(zero_start) = zero_run_start.take() {
                    let zero_count = (offset - zero_start) / 4;
                    if zero_count > MAX_ZERO_RUN {
                        // Summarize long zero runs
                        disasm.push_str(&format!(
                            "  {zero_start:08x}: ... ({zero_count} zero words skipped)\n"
                        ));
                    } else {
                        // Show short zero runs
                        for i in 0..zero_count {
                            let inst_str = format_instruction(0);
                            disasm.push_str(&format!(
                                "  {:08x}: {:08x}    {}\n",
                                zero_start + i * 4,
                                0,
                                inst_str
                            ));
                        }
                    }
                }

                // Use proper disassembly formatting
                let inst_str = lp_riscv_inst::format_instruction(instruction);
                disasm.push_str(&format!(
                    "  {offset:08x}: {instruction:08x}    {inst_str}\n"
                ));
            }

            offset += 4;
        }

        // Flush any remaining zero run at the end
        if let Some(zero_start) = zero_run_start {
            let zero_count = (offset - zero_start) / 4;
            if zero_count > MAX_ZERO_RUN {
                disasm.push_str(&format!(
                    "  {zero_start:08x}: ... ({zero_count} zero words skipped)\n"
                ));
            } else {
                for i in 0..zero_count {
                    let inst_str = lp_riscv_inst::format_instruction(0);
                    disasm.push_str(&format!(
                        "  {:08x}: {:08x}    {}\n",
                        zero_start + i * 4,
                        0,
                        inst_str
                    ));
                }
            }
        }

        if offset < self.binary.len() {
            disasm.push_str(&format!(
                "  ... ({} more bytes)\n",
                self.binary.len() - offset
            ));
        }

        Ok(disasm)
    }

    /// Find the source location (line number) of a function definition in the GLSL source
    #[allow(dead_code, reason = "Reserved for future use in error reporting")]
    fn find_function_source_location(
        &self,
        func_name: &str,
    ) -> Option<crate::frontend::src_loc::GlSourceLoc> {
        let file_id = self.get_main_file_id()?;
        let source_file = self.source_map.get_file(file_id)?;
        let source_text = &source_file.contents;

        // Search for function definition: "type func_name(" or "func_name("
        // This is a simple heuristic - we look for the function name followed by (
        let pattern = if func_name == "main" {
            format!("{func_name}()")
        } else {
            format!("{func_name}(")
        };

        // Find the first occurrence of the pattern
        for (line_idx, line) in source_text.lines().enumerate() {
            if line.contains(&pattern) {
                // Found the function - return 1-indexed line number
                return Some(crate::frontend::src_loc::GlSourceLoc::new(
                    file_id,
                    line_idx + 1,
                    1,
                ));
            }
        }

        None
    }

    /// Extract source lines around a given line number for display
    #[allow(dead_code, reason = "Reserved for future use in error reporting")]
    fn extract_source_lines(
        &self,
        file_id: crate::frontend::src_loc::GlFileId,
        line_num: usize,
        context_lines: usize,
    ) -> Option<(Vec<String>, usize)> {
        let source_file = self.source_map.get_file(file_id)?;
        let source_text = &source_file.contents;
        let lines: Vec<&str> = source_text.lines().collect();

        if line_num == 0 || line_num > lines.len() {
            return None;
        }

        // Calculate start and end lines (1-indexed)
        let start_line = line_num.saturating_sub(context_lines).max(1);
        let end_line = (line_num + context_lines).min(lines.len());

        // Extract the relevant lines
        let extracted_lines: Vec<String> = lines[(start_line - 1)..end_line]
            .iter()
            .map(|s| String::from(*s))
            .collect();

        // Return lines and the relative line number (1-indexed within the extracted range)
        Some((extracted_lines, line_num - start_line + 1))
    }
}

#[cfg(feature = "emulator")]
impl GlslExecutable for GlslEmulatorModule {
    fn call_void(&mut self, name: &str, args: &[GlslValue]) -> Result<(), GlslError> {
        use crate::error::ErrorCode;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts with emulator)
        let sig = self.get_function_signature(name)?.clone();

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature
        if data_args.len() != sig.params.len() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s), got {} argument(s). Signature: {:?}",
                    name,
                    sig.params.len(),
                    data_args.len(),
                    sig
                ),
            ));
        }

        // Call function via emulator
        let _results = self
            .emulator
            .call_function(func_address, &data_args, &sig)
            .map_err(|e| match e {
                EmulatorError::Trap { code, pc, regs } => {
                    self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                }
                other => self.build_enhanced_error(
                    ErrorCode::E0400,
                    &format!("Emulator execution failed: {other}"),
                    name,
                ),
            })?;

        Ok(())
    }

    fn call_i32(&mut self, name: &str, args: &[GlslValue]) -> Result<i32, GlslError> {
        use crate::error::ErrorCode;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature
        if data_args.len() != sig.params.len() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s), got {} argument(s). Signature: {:?}",
                    name,
                    sig.params.len(),
                    data_args.len(),
                    sig
                ),
            ));
        }

        // Call function via emulator
        let results = self
            .emulator
            .call_function(func_address, &data_args, &sig)
            .map_err(|e| match e {
                EmulatorError::Trap { code, pc, regs } => {
                    log::debug!(
                        "Emulator trap calling '{}':\n{}",
                        name,
                        self.emulator.dump_state()
                    );
                    self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                }
                other => {
                    log::debug!(
                        "Emulator error calling '{}': {}\n{}",
                        name,
                        other,
                        self.emulator.dump_state()
                    );
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    )
                }
            })?;

        // Extract i32 return value
        match results.first() {
            Some(cranelift_codegen::data_value::DataValue::I32(v)) => Ok(*v),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("Expected i32 return value, got: {results:?}"),
            )),
        }
    }

    fn call_f32(&mut self, name: &str, args: &[GlslValue]) -> Result<f32, GlslError> {
        use crate::error::ErrorCode;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature
        if data_args.len() != sig.params.len() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s), got {} argument(s). Signature: {:?}",
                    name,
                    sig.params.len(),
                    data_args.len(),
                    sig
                ),
            ));
        }

        // Call function via emulator
        let results = self
            .emulator
            .call_function(func_address, &data_args, &sig)
            .map_err(|e| match e {
                EmulatorError::Trap { code, pc, regs } => {
                    self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                }
                other => self.build_enhanced_error(
                    ErrorCode::E0400,
                    &format!("Emulator execution failed: {other}"),
                    name,
                ),
            })?;

        // Extract i32 return value and convert from fixed-point to f32
        match results.first() {
            Some(cranelift_codegen::data_value::DataValue::I32(v)) => {
                // Convert from fixed-point (i32) to f32
                // Fixed-point values are stored as i32 with FIXED16X16_SCALE denominator
                Ok(*v as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE)
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "Expected i32 return value",
            )),
        }
    }

    fn call_bool(&mut self, name: &str, args: &[GlslValue]) -> Result<bool, GlslError> {
        use crate::error::ErrorCode;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature
        if data_args.len() != sig.params.len() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s), got {} argument(s). Signature: {:?}",
                    name,
                    sig.params.len(),
                    data_args.len(),
                    sig
                ),
            ));
        }

        // Call function via emulator
        let results = self
            .emulator
            .call_function(func_address, &data_args, &sig)
            .map_err(|e| {
                self.build_enhanced_error(
                    ErrorCode::E0400,
                    &format!("Emulator execution failed: {e}"),
                    name,
                )
            })?;

        // Extract i8 return value
        match results.first() {
            Some(cranelift_codegen::data_value::DataValue::I8(v)) => Ok(*v != 0),
            _ => Err(GlslError::new(ErrorCode::E0400, "Expected i8 return value")),
        }
    }

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if data_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    data_args.len(),
                    sig
                ),
            ));
        }

        if uses_struct_return {
            // Clone signature before mutable borrow
            let sig = sig.clone();

            // Calculate buffer size for struct return
            // Boolean values are stored as i8 but with 4-byte alignment (matching return statement codegen)
            let buffer_size = dim * 4;

            // Call main via emulator with struct return (buffer allocation handled internally)
            let results = self
                .emulator
                .call_function_with_struct_return(func_address, &data_args, &sig, buffer_size)
                .map_err(|e| match e {
                    EmulatorError::Trap { code, pc, regs } => {
                        self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                    }
                    other => self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    ),
                })?;

            // Convert results from returned Vec<DataValue> (i32 words containing i8 values)
            // The emulator reads StructReturn buffers as i32 words, but boolean values are stored as i8
            // Each i32 word contains one i8 value in its low byte (at 4-byte-aligned positions)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        // Extract i8 value from low byte of i32 word
                        let i8_val = (*v & 0xFF) as i8;
                        vec_result.push(i8_val != 0); // Convert i8 to bool: 0 → false, non-zero → true
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values (containing i8) for boolean vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        } else {
            // No StructReturn - read from return registers (legacy path)
            let results = self
                .emulator
                .call_function(func_address, &data_args, &sig)
                .map_err(|e| {
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {e}"),
                        name,
                    )
                })?;

            // Convert results from i8 to bool
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I8(v) => {
                        vec_result.push(*v != 0); // Convert i8 to bool
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i8 return values for boolean vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        }
    }

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if data_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    data_args.len(),
                    sig
                ),
            ));
        }

        if uses_struct_return {
            // Clone signature before mutable borrow
            let sig = sig.clone();

            // Calculate buffer size for struct return
            // Integer values are stored as i32 (4 bytes each)
            let buffer_size = dim * 4;

            // Call main via emulator with struct return (buffer allocation handled internally)
            let results = self
                .emulator
                .call_function_with_struct_return(func_address, &data_args, &sig, buffer_size)
                .map_err(|e| match e {
                    EmulatorError::Trap { code, pc, regs } => {
                        self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                    }
                    other => self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    ),
                })?;

            // Convert results from returned Vec<DataValue> (i32 values, no scaling)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        vec_result.push(*v); // Direct i32 value, no scaling
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values for integer vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        } else {
            // No StructReturn - read from return registers (legacy path)
            let results = self
                .emulator
                .call_function(func_address, &data_args, &sig)
                .map_err(|e| {
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {e}"),
                        name,
                    )
                })?;

            // Convert results from i32 (no scaling)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        vec_result.push(*v); // Direct i32 value
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values for integer vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        }
    }

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if data_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    data_args.len(),
                    sig
                ),
            ));
        }

        if uses_struct_return {
            // Clone signature before mutable borrow
            let sig = sig.clone();

            // Calculate buffer size for struct return
            // Unsigned integer values are stored as i32 (4 bytes each), interpreted as u32
            let buffer_size = dim * 4;

            // Call main via emulator with struct return (buffer allocation handled internally)
            let results = self
                .emulator
                .call_function_with_struct_return(func_address, &data_args, &sig, buffer_size)
                .map_err(|e| match e {
                    EmulatorError::Trap { code, pc, regs } => {
                        self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                    }
                    other => self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    ),
                })?;

            // Convert results from returned Vec<DataValue> (i32 values interpreted as u32, no scaling)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        vec_result.push(*v as u32); // Convert i32 to u32 (bit pattern preserved)
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values for unsigned integer vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        } else {
            // No StructReturn - read from return registers (legacy path)
            let results = self
                .emulator
                .call_function(func_address, &data_args, &sig)
                .map_err(|e| {
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {e}"),
                        name,
                    )
                })?;

            // Convert results from i32 to u32 (no scaling)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        vec_result.push(*v as u32); // Convert i32 to u32 (bit pattern preserved)
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values for unsigned integer vector",
                        ));
                    }
                }
            }
            Ok(vec_result)
        }
    }

    fn call_vec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to DataValue
        let mut arg_idx = 0;
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if data_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    data_args.len(),
                    sig
                ),
            ));
        }

        if uses_struct_return {
            // Clone signature before mutable borrow
            let sig = sig.clone();

            // Calculate buffer size for struct return
            // Each element is 4 bytes (i32 for fixed-point)
            let buffer_size = dim * 4;

            // Call main via emulator with struct return (buffer allocation handled internally)
            let results = self
                .emulator
                .call_function_with_struct_return(func_address, &data_args, &sig, buffer_size)
                .map_err(|e| match e {
                    EmulatorError::Trap { code, pc, regs } => {
                        self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                    }
                    other => self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    ),
                })?;

            // Convert results from returned Vec<DataValue> (fixed-point i32 values)
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        vec_result.push(
                            *v as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE,
                        );
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values",
                        ));
                    }
                }
            }
            Ok(vec_result)
        } else {
            // No StructReturn - read from return registers (legacy path)
            let results = self
                .emulator
                .call_function(func_address, &data_args, &sig)
                .map_err(|e| {
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {e}"),
                        name,
                    )
                })?;

            // Convert results from fixed-point i32 to f32
            let mut vec_result = Vec::with_capacity(dim);
            for result in results.iter().take(dim) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => vec_result
                        .push(*v as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE),
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values",
                        ));
                    }
                }
            }
            Ok(vec_result)
        }
    }

    fn call_mat(
        &mut self,
        name: &str,
        args: &[GlslValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        // Validate function exists and get address
        let func_address = self.get_function_address(name)?;

        // Get function signature (clone to avoid borrow conflicts)
        let sig = self.get_function_signature(name)?.clone();

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to DataValue
        // Skip StructReturn parameter if present (it's handled internally by emulator)
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut data_args = Vec::new();
        for arg in args {
            data_args.extend(self.glsl_value_to_data_value(arg, &sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if data_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    data_args.len(),
                    sig
                ),
            ));
        }

        if uses_struct_return {
            // Clone signature before mutable borrow
            let sig = sig.clone();

            // Calculate buffer size for struct return
            // Each element is 4 bytes (i32 for fixed-point)
            let count = rows * cols;
            let buffer_size = count * 4;

            // Call main via emulator with struct return (buffer allocation handled internally)
            let results = self
                .emulator
                .call_function_with_struct_return(func_address, &data_args, &sig, buffer_size)
                .map_err(|e| match e {
                    EmulatorError::Trap { code, pc, regs } => {
                        self.format_trap_error_from_emulator_error(code, pc, &regs, name)
                    }
                    other => self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {other}"),
                        name,
                    ),
                })?;

            // Convert results from returned Vec<DataValue> (fixed-point i32 values)
            let mut mat_result = Vec::with_capacity(count);
            for result in results.iter().take(count) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => {
                        mat_result.push(
                            *v as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE,
                        );
                    }
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values",
                        ));
                    }
                }
            }
            Ok(mat_result)
        } else {
            // No StructReturn - read from return registers (legacy path)
            let results = self
                .emulator
                .call_function(func_address, &data_args, &sig)
                .map_err(|e| {
                    self.build_enhanced_error(
                        ErrorCode::E0400,
                        &format!("Emulator execution failed: {e}"),
                        name,
                    )
                })?;

            // Convert results from fixed-point i32 to f32
            let count = rows * cols;
            let mut mat_result = Vec::with_capacity(count);
            for result in results.iter().take(count) {
                match result {
                    cranelift_codegen::data_value::DataValue::I32(v) => mat_result
                        .push(*v as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE),
                    _ => {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected i32 return values",
                        ));
                    }
                }
            }
            Ok(mat_result)
        }
    }

    fn get_function_signature(&self, name: &str) -> Option<&FunctionSignature> {
        self.signatures.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.signatures.keys().cloned().collect()
    }

    #[cfg(feature = "std")]
    fn format_emulator_state(&self) -> Option<String> {
        let state_dump = self.emulator.dump_state();
        let current_pc = self.emulator.get_pc();
        // Always show disassembly around current PC (like filetests do)
        let debug_info = self.emulator.format_debug_info(Some(current_pc), 100);
        // Only include debug info section if there's actual content
        if debug_info.is_empty() {
            Some(format!("\n=== Emulator State ===\n{state_dump}"))
        } else {
            Some(format!(
                "\n=== Emulator State ===\n{state_dump}\n\n=== Debug Info ===\n{debug_info}"
            ))
        }
    }

    #[cfg(feature = "std")]
    fn format_clif_ir(&self) -> (Option<String>, Option<String>) {
        (self.original_clif.clone(), self.transformed_clif.clone())
    }

    #[cfg(feature = "std")]
    fn format_vcode(&self) -> Option<String> {
        self.vcode.clone()
    }

    #[cfg(feature = "std")]
    fn format_disassembly(&self) -> Option<String> {
        self.disassembly.clone()
    }
}

#[cfg(feature = "emulator")]
#[cfg(test)]
mod tests {
    use crate::{GlslOptions, glsl_emu_riscv32};

    /// Convert float to 16.16 fixed-point for comparison
    fn float_to_q32(f: f32) -> i32 {
        let clamped = f.clamp(-32768.0, 32767.9999847412109375);
        let scaled = clamped * 65536.0;
        if scaled >= 0.0 {
            (scaled + 0.5) as i32
        } else {
            (scaled - 0.5) as i32
        }
    }

    /// Convert fixed-point back to float
    fn q32_to_float(fixed: i32) -> f32 {
        fixed as f32 / 65536.0
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_int_literal() {
        let source = r#"
        int main() {
            return 42;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_i32("main", &[]).expect("Execution failed");
        assert_eq!(result, 42);
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_int_addition() {
        let source = r#"
        int main() {
            int a = 10;
            int b = 20;
            return a + b;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_i32("main", &[]).expect("Execution failed");
        assert_eq!(result, 30);
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_float_constant_q32() {
        let source = r#"
        float main() {
            return 3.14159;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");

        // The emulator returns fixed-point as f32, so we need to check the fixed-point value
        let expected_fixed = float_to_q32(3.14159);
        let result_fixed = float_to_q32(result);

        // Allow some tolerance for fixed-point conversion
        assert!(
            (result_fixed - expected_fixed).abs() < 10,
            "Expected fixed-point value around {expected_fixed}, got {result_fixed}"
        );

        // Verify it's approximately correct as float
        assert!((result - 3.14159).abs() < 0.0001);
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_float_addition_q32() {
        let source = r#"
        float main() {
            float a = 2.5;
            float b = 1.25;
            return a + b;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");

        let expected = 3.75;
        let result_float = q32_to_float(float_to_q32(result));
        assert!(
            (result_float - expected).abs() < 0.0001,
            "Expected ~{expected}, got {result_float}"
        );
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_float_multiplication_q32() {
        let source = r#"
        float main() {
            float a = 2.0;
            float b = 3.5;
            return a * b;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");

        let expected = 7.0;
        let result_float = q32_to_float(float_to_q32(result));
        assert!(
            (result_float - expected).abs() < 0.001,
            "Expected ~{expected}, got {result_float}"
        );
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_user_fn_q32() {
        let source = r#"
        float main() {
            float a = 2.0;
            float b = 3.5;
            return multiply(a, b);
        }

        float multiply(float a, float b) {
            return a * b;
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");

        let expected = 7.0;
        let result_float = q32_to_float(float_to_q32(result));
        assert!(
            (result_float - expected).abs() < 0.001,
            "Expected ~{expected}, got {result_float}"
        );
    }

    #[test]
    #[ignore] // TODO emu: Emulator execution tests failing
    fn test_emu_builtin_sqrt_linked() {
        // Test that sqrt() uses the linked __lp_q32_sqrt function
        let source = r#"
        float main() {
            return sqrt(4.0);
        }
    "#;

        let options = GlslOptions::emu_riscv32_imac();

        let mut executable = glsl_emu_riscv32(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");

        let expected = 2.0;
        let result_float = q32_to_float(float_to_q32(result));
        assert!(
            (result_float - expected).abs() < 0.01,
            "Expected sqrt(4.0) ≈ {expected}, got {result_float}"
        );
    }
}
