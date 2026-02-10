//! JIT-compiled GLSL module implementation
//!
//! This module provides the JIT execution backend for GLSL functions.

use crate::error::GlslError;
use crate::exec::executable::GlslExecutable;
use crate::exec::glsl_value::GlslValue;
use crate::frontend::semantic::functions::FunctionSignature;
use cranelift_codegen::ir::types;
use hashbrown::HashMap;
use lp_glsl_jit_util::{call_structreturn, call_structreturn_with_args};

use alloc::{format, string::String, vec, vec::Vec};
use log;

/// JIT-compiled GLSL module (executes on host or embedded)
/// Works in both std and no_std (JITModule supports no_std)
pub struct GlslJitModule {
    #[allow(dead_code, reason = "JIT module stored for future use")]
    pub(crate) jit_module: cranelift_jit::JITModule,
    pub(crate) function_ptrs: HashMap<String, *const u8>,
    pub(crate) signatures: HashMap<String, FunctionSignature>,
    // Store Cranelift signatures for proper function calling with arguments
    pub(crate) cranelift_signatures: HashMap<String, cranelift_codegen::ir::Signature>,
    pub(crate) call_conv: cranelift_codegen::isa::CallConv,
    pub(crate) pointer_type: cranelift_codegen::ir::Type,
}

impl GlslJitModule {
    /// Get function pointer by name
    pub fn get_function_ptr(&self, name: &str) -> Result<*const u8, GlslError> {
        use crate::error::ErrorCode;
        self.function_ptrs
            .get(name)
            .copied()
            .ok_or_else(|| GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found")))
    }

    /// Validate that only "main" function is being called
    fn validate_main_only(&self, name: &str) -> Result<(), GlslError> {
        use crate::error::ErrorCode;
        if name != "main" {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("Only 'main' function is supported, got '{name}'"),
            ));
        }
        Ok(())
    }

    // Helper to convert GlslValue to calling convention arguments for JIT
    // Vectors are expanded into multiple scalar arguments matching the calling convention
    fn glsl_value_to_jit_args(
        &self,
        value: &GlslValue,
        sig: &cranelift_codegen::ir::Signature,
        arg_idx: &mut usize,
    ) -> Result<Vec<u64>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        let mut args = Vec::new();

        match value {
            GlslValue::I32(v) => {
                if *arg_idx >= sig.params.len() {
                    return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                }
                // Skip StructReturn parameter if present
                if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                    *arg_idx += 1;
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                }
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::I32 => args.push(*v as u64),
                    types::I64 => args.push(*v as u64), // Sign-extend
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
                if *arg_idx >= sig.params.len() {
                    return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                }
                // Skip StructReturn parameter if present
                if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                    *arg_idx += 1;
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                }
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::F32 => args.push(v.to_bits() as u64),
                    types::I32 => {
                        // Convert f32 to fixed-point i32 (Q16.16 format)
                        let fixed =
                            (*v * crate::frontend::codegen::constants::FIXED16X16_SCALE) as i32;
                        args.push(fixed as u64);
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
                if *arg_idx >= sig.params.len() {
                    return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                }
                // Skip StructReturn parameter if present
                if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                    *arg_idx += 1;
                    if *arg_idx >= sig.params.len() {
                        return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                    }
                }
                let param_ty = sig.params[*arg_idx].value_type;
                match param_ty {
                    types::I8 => args.push(if *v { 1 } else { 0 } as u64),
                    types::I32 => args.push(if *v { 1 } else { 0 } as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => args.push(component.to_bits() as u64),
                        types::I32 => {
                            // Convert f32 to fixed-point i32 (Q16.16 format)
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(fixed as u64);
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => args.push(component.to_bits() as u64),
                        types::I32 => {
                            // Convert f32 to fixed-point i32 (Q16.16 format)
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(fixed as u64);
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::F32 => args.push(component.to_bits() as u64),
                        types::I32 => {
                            // Convert f32 to fixed-point i32 (Q16.16 format)
                            let fixed = (*component
                                * crate::frontend::codegen::constants::FIXED16X16_SCALE)
                                as i32;
                            args.push(fixed as u64);
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64),
                        types::I64 => args.push(*component as i64 as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64),
                        types::I64 => args.push(*component as i64 as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64),
                        types::I64 => args.push(*component as i64 as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64), // u32 passed as i32
                        types::I64 => args.push(*component as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64),
                        types::I64 => args.push(*component as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I32 => args.push(*component as u64),
                        types::I64 => args.push(*component as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(if *component { 1 } else { 0 } as u64),
                        types::I32 => args.push(if *component { 1 } else { 0 } as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(if *component { 1 } else { 0 } as u64),
                        types::I32 => args.push(if *component { 1 } else { 0 } as u64),
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
                    // Skip StructReturn parameter if present
                    if sig.params[*arg_idx].purpose == ArgumentPurpose::StructReturn {
                        *arg_idx += 1;
                        if *arg_idx >= sig.params.len() {
                            return Err(GlslError::new(ErrorCode::E0400, "Too many arguments"));
                        }
                    }
                    let param_ty = sig.params[*arg_idx].value_type;
                    match param_ty {
                        types::I8 => args.push(if *component { 1 } else { 0 } as u64),
                        types::I32 => args.push(if *component { 1 } else { 0 } as u64),
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
                    "Matrix arguments not yet supported in JIT calls",
                ));
            }
            GlslValue::U32(_) => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "U32 scalar arguments not yet supported in JIT calls (use UVec2/3/4 for vectors)",
                ));
            }
        }

        Ok(args)
    }
}

impl GlslExecutable for GlslJitModule {
    fn call_void(&mut self, name: &str, args: &[GlslValue]) -> Result<(), GlslError> {
        use crate::error::ErrorCode;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Handle no-argument case (most common)
        if args.is_empty() {
            let func: unsafe extern "C" fn() = unsafe { core::mem::transmute(*func_ptr) };
            unsafe { func() };
            return Ok(());
        }

        // For now, only support simple cases (1-2 scalar arguments)
        // Full implementation would need platform-specific inline assembly
        if args.len() > 2 {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "JIT calls with more than 2 arguments not yet supported (got {} args)",
                    args.len()
                ),
            ));
        }

        // Convert arguments
        let mut arg_idx = 0;
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Call based on number of arguments
        match jit_args.len() {
            0 => {
                let func: unsafe extern "C" fn() = unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func() };
            }
            1 => {
                let func: unsafe extern "C" fn(u64) = unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0]) };
            }
            2 => {
                let func: unsafe extern "C" fn(u64, u64) =
                    unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0], jit_args[1]) };
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("Too many arguments for simple JIT call: {}", jit_args.len()),
                ));
            }
        }

        Ok(())
    }

    fn call_i32(&mut self, name: &str, args: &[GlslValue]) -> Result<i32, GlslError> {
        use crate::error::ErrorCode;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Handle no-argument case
        if args.is_empty() {
            let func: unsafe extern "C" fn() -> i32 = unsafe { core::mem::transmute(*func_ptr) };
            return Ok(unsafe { func() });
        }

        // For now, only support simple cases (1-2 scalar arguments)
        if args.len() > 2 {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "JIT calls with more than 2 arguments not yet supported (got {} args)",
                    args.len()
                ),
            ));
        }

        // Convert arguments
        let mut arg_idx = 0;
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Call based on number of arguments
        let result = match jit_args.len() {
            1 => {
                let func: unsafe extern "C" fn(u64) -> i32 =
                    unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0]) }
            }
            2 => {
                let func: unsafe extern "C" fn(u64, u64) -> i32 =
                    unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0], jit_args[1]) }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("Too many arguments for simple JIT call: {}", jit_args.len()),
                ));
            }
        };

        Ok(result)
    }

    fn call_f32(&mut self, name: &str, args: &[GlslValue]) -> Result<f32, GlslError> {
        use crate::error::ErrorCode;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check return type: I32 means fixed-point, F32 means native float
        let return_type = sig
            .returns
            .first()
            .map(|r| r.value_type)
            .unwrap_or(types::F32);

        // Handle no-argument case
        if args.is_empty() {
            if return_type == types::I32 {
                // Fixed-point: call as i32, convert to f32
                let func: unsafe extern "C" fn() -> i32 =
                    unsafe { core::mem::transmute(*func_ptr) };
                let fixed_result = unsafe { func() };
                return Ok(fixed_result as f32 / 65536.0);
            } else {
                // Native float: call as f32
                let func: unsafe extern "C" fn() -> f32 =
                    unsafe { core::mem::transmute(*func_ptr) };
                return Ok(unsafe { func() });
            }
        }

        // For now, only support simple cases (1-2 scalar arguments)
        if args.len() > 2 {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "JIT calls with more than 2 arguments not yet supported (got {} args)",
                    args.len()
                ),
            ));
        }

        // Convert arguments
        let mut arg_idx = 0;
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Call based on number of arguments and return type
        let result = if return_type == types::I32 {
            // Fixed-point: call as i32, convert to f32
            match jit_args.len() {
                1 => {
                    let func: unsafe extern "C" fn(u64) -> i32 =
                        unsafe { core::mem::transmute(*func_ptr) };
                    let fixed_result = unsafe { func(jit_args[0]) };
                    fixed_result as f32 / 65536.0
                }
                2 => {
                    let func: unsafe extern "C" fn(u64, u64) -> i32 =
                        unsafe { core::mem::transmute(*func_ptr) };
                    let fixed_result = unsafe { func(jit_args[0], jit_args[1]) };
                    fixed_result as f32 / 65536.0
                }
                _ => {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Too many arguments for simple JIT call: {}", jit_args.len()),
                    ));
                }
            }
        } else {
            // Native float: call as f32
            match jit_args.len() {
                1 => {
                    let func: unsafe extern "C" fn(u64) -> f32 =
                        unsafe { core::mem::transmute(*func_ptr) };
                    unsafe { func(jit_args[0]) }
                }
                2 => {
                    let func: unsafe extern "C" fn(u64, u64) -> f32 =
                        unsafe { core::mem::transmute(*func_ptr) };
                    unsafe { func(jit_args[0], jit_args[1]) }
                }
                _ => {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Too many arguments for simple JIT call: {}", jit_args.len()),
                    ));
                }
            }
        };

        Ok(result)
    }

    fn call_bool(&mut self, name: &str, args: &[GlslValue]) -> Result<bool, GlslError> {
        use crate::error::ErrorCode;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Handle no-argument case
        if args.is_empty() {
            let func: unsafe extern "C" fn() -> i8 = unsafe { core::mem::transmute(*func_ptr) };
            return Ok(unsafe { func() != 0 });
        }

        // For now, only support simple cases (1-2 scalar arguments)
        if args.len() > 2 {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "JIT calls with more than 2 arguments not yet supported (got {} args)",
                    args.len()
                ),
            ));
        }

        // Convert arguments
        let mut arg_idx = 0;
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Call based on number of arguments
        let result = match jit_args.len() {
            1 => {
                let func: unsafe extern "C" fn(u64) -> i8 =
                    unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0]) }
            }
            2 => {
                let func: unsafe extern "C" fn(u64, u64) -> i8 =
                    unsafe { core::mem::transmute(*func_ptr) };
                unsafe { func(jit_args[0], jit_args[1]) }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("Too many arguments for simple JIT call: {}", jit_args.len()),
                ));
            }
        };

        Ok(result != 0)
    }

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to JIT arguments
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if jit_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    jit_args.len(),
                    sig
                ),
            ));
        }

        // Use struct return for boolean vectors (multiple i8s returned via pointer)
        // Boolean values are stored as i8 but with 4-byte alignment (matching return statement codegen)
        let buffer_size = dim * 4;
        let mut buffer = vec![0u8; buffer_size];

        if jit_args.is_empty() {
            unsafe {
                call_structreturn(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call failed for bvec{dim}: {e}"),
                    )
                })?;
            }
        } else {
            unsafe {
                call_structreturn_with_args(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    &jit_args,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call with args failed for bvec{dim}: {e}"),
                    )
                })?;
            }
        }
        // Extract i8 values from 4-byte-aligned positions and convert to bool
        // Values are stored at offsets 0, 4, 8, etc.
        let mut result = Vec::with_capacity(dim);
        for i in 0..dim {
            let offset = i * 4;
            result.push(buffer[offset] != 0); // Convert i8 to bool: 0 → false, non-zero → true
        }
        Ok(result)
    }

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to JIT arguments
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if jit_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    jit_args.len(),
                    sig
                ),
            ));
        }

        // Use struct return for integer vectors (multiple i32s returned via pointer)
        let buffer_size = dim * 4; // Each i32 is 4 bytes
        let mut buffer = vec![0u8; buffer_size];

        if jit_args.is_empty() {
            unsafe {
                call_structreturn(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call failed for ivec{dim}: {e}"),
                    )
                })?;
            }
        } else {
            unsafe {
                call_structreturn_with_args(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    &jit_args,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call with args failed for ivec{dim}: {e}"),
                    )
                })?;
            }
        }
        // Extract i32 values from buffer (no scaling)
        let mut result = Vec::with_capacity(dim);
        for i in 0..dim {
            let offset = i * 4;
            let bytes = [
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ];
            let value = i32::from_le_bytes(bytes);
            result.push(value);
        }
        Ok(result)
    }

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to JIT arguments
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if jit_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    jit_args.len(),
                    sig
                ),
            ));
        }

        // Use struct return for unsigned integer vectors (multiple i32s returned via pointer, interpreted as u32)
        let buffer_size = dim * 4; // Each i32/u32 is 4 bytes
        let mut buffer = vec![0u8; buffer_size];

        if jit_args.is_empty() {
            unsafe {
                call_structreturn(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call failed for uvec{dim}: {e}"),
                    )
                })?;
            }
        } else {
            unsafe {
                call_structreturn_with_args(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    &jit_args,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call with args failed for uvec{dim}: {e}"),
                    )
                })?;
            }
        }
        // Extract i32 values from buffer and convert to u32 (bit pattern preserved, no scaling)
        let mut result = Vec::with_capacity(dim);
        for i in 0..dim {
            let offset = i * 4;
            let bytes = [
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ];
            let value = i32::from_le_bytes(bytes) as u32;
            result.push(value);
        }
        Ok(result)
    }

    fn call_vec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError> {
        use crate::error::ErrorCode;
        use cranelift_codegen::ir::ArgumentPurpose;

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to JIT arguments
        // Start arg_idx at 1 if StructReturn is present (it's always at index 0)
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            // StructReturn parameter is added internally, don't count it
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if jit_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    jit_args.len(),
                    sig
                ),
            ));
        }

        // Check return type: I32 means fixed-point, F32 means native float
        // For struct return, we can't check the StructReturn parameter type (it's a pointer/I8)
        // Instead, check the argument types - if arguments are I32, return is also I32 (Q32)
        // Otherwise, check return registers if available
        let return_type = if uses_struct_return {
            // Check if any non-StructReturn parameter is I32 (indicates Q32 format)
            let has_i32_params =
                sig.params.iter().skip(1).any(|p| {
                    p.purpose != ArgumentPurpose::StructReturn && p.value_type == types::I32
                });
            if has_i32_params {
                types::I32 // Fixed-point
            } else {
                types::F32 // Native float (default)
            }
        } else {
            // No struct return - check return registers (shouldn't happen for vec, but handle it)
            sig.returns
                .first()
                .map(|r| r.value_type)
                .unwrap_or(types::F32)
        };

        // Use struct return for vectors (multiple values returned via pointer)
        // For fixed-point (I32), we need to use i32 buffer and convert to f32
        // For native float (F32), use f32 buffer directly
        let buffer_size = if return_type == types::I32 {
            // Fixed-point: buffer contains i32 values
            dim * core::mem::size_of::<i32>()
        } else {
            // Native float: buffer contains f32 values
            dim * core::mem::size_of::<f32>()
        };

        if return_type == types::I32 {
            // Fixed-point: use i32 buffer, then convert to f32
            let mut i32_buffer = vec![0i32; dim];

            if jit_args.is_empty() {
                // No arguments case - use simpler call
                unsafe {
                    call_structreturn(
                        *func_ptr,
                        i32_buffer.as_mut_ptr() as *mut u8,
                        buffer_size,
                        self.call_conv,
                        self.pointer_type,
                    )
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!("StructReturn call failed for vec{dim}: {e}"),
                        )
                    })?;
                }
            } else {
                // Has arguments - use call_structreturn_with_args
                log::trace!(
                    "GlslJitModule::call_vec: Calling with {} arguments: {:?}",
                    jit_args.len(),
                    jit_args
                );
                unsafe {
                    call_structreturn_with_args(
                        *func_ptr,
                        i32_buffer.as_mut_ptr() as *mut u8,
                        buffer_size,
                        &jit_args,
                        self.call_conv,
                        self.pointer_type,
                    )
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!("StructReturn call with args failed for vec{dim}: {e}"),
                        )
                    })?;
                }
            }

            // Convert fixed-point i32 values to f32
            let mut f32_buffer = Vec::with_capacity(dim);
            for fixed_value in i32_buffer {
                f32_buffer.push(
                    fixed_value as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE,
                );
            }
            Ok(f32_buffer)
        } else {
            // Native float: use f32 buffer directly
            let mut buffer = vec![0.0f32; dim];

            if jit_args.is_empty() {
                // No arguments case - use simpler call
                unsafe {
                    call_structreturn(
                        *func_ptr,
                        buffer.as_mut_ptr(),
                        buffer_size,
                        self.call_conv,
                        self.pointer_type,
                    )
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!("StructReturn call failed for vec{dim}: {e}"),
                        )
                    })?;
                }
            } else {
                // Has arguments - use call_structreturn_with_args
                unsafe {
                    call_structreturn_with_args(
                        *func_ptr,
                        buffer.as_mut_ptr(),
                        buffer_size,
                        &jit_args,
                        self.call_conv,
                        self.pointer_type,
                    )
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!("StructReturn call with args failed for vec{dim}: {e}"),
                        )
                    })?;
                }
            }
            Ok(buffer)
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

        self.validate_main_only(name)?;

        let func_ptr = self.function_ptrs.get(name).ok_or_else(|| {
            GlslError::new(ErrorCode::E0101, format!("Function '{name}' not found"))
        })?;

        // Get the actual Cranelift signature for this function
        let sig = self.cranelift_signatures.get(name).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0101,
                format!("Function signature for '{name}' not found"),
            )
        })?;

        // Check if function uses StructReturn (before processing arguments)
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        // Convert arguments to JIT arguments
        let mut arg_idx = if uses_struct_return { 1 } else { 0 };
        let mut jit_args = Vec::new();
        for arg in args {
            jit_args.extend(self.glsl_value_to_jit_args(arg, sig, &mut arg_idx)?);
        }

        // Validate argument count matches signature (excluding StructReturn parameter)
        let expected_params = if uses_struct_return {
            sig.params.len() - 1
        } else {
            sig.params.len()
        };

        if jit_args.len() != expected_params {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Argument count mismatch calling function '{}': expected {} parameter(s) (excluding StructReturn), got {} argument(s). Signature: {:?}",
                    name,
                    expected_params,
                    jit_args.len(),
                    sig
                ),
            ));
        }

        // Use struct return for matrices (column-major, rows*cols f32s)
        let count = rows * cols;
        let buffer_size = count * core::mem::size_of::<f32>();
        let mut buffer = vec![0.0f32; count];

        if jit_args.is_empty() {
            unsafe {
                call_structreturn(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call failed for mat{rows}x{cols}: {e}"),
                    )
                })?;
            }
        } else {
            unsafe {
                call_structreturn_with_args(
                    *func_ptr,
                    buffer.as_mut_ptr(),
                    buffer_size,
                    &jit_args,
                    self.call_conv,
                    self.pointer_type,
                )
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("StructReturn call with args failed for mat{rows}x{cols}: {e}"),
                    )
                })?;
            }
        }
        Ok(buffer)
    }

    fn get_function_signature(&self, name: &str) -> Option<&FunctionSignature> {
        self.signatures.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.signatures.keys().cloned().collect()
    }

    fn get_direct_call_info(&self, name: &str) -> Option<crate::exec::executable::DirectCallInfo> {
        let func_ptr = *self.function_ptrs.get(name)?;
        Some(crate::exec::executable::DirectCallInfo {
            func_ptr,
            call_conv: self.call_conv,
            pointer_type: self.pointer_type,
        })
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use crate::{DecimalFormat, GlslOptions, Q32Options, RunMode, glsl_jit};

    #[test]
    fn test_jit_int_literal() {
        let source = r#"
        int main() {
            return 42;
        }
    "#;

        let options = GlslOptions {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Q32,
            q32_opts: Q32Options::default(),
        };

        let mut executable = glsl_jit(source, options).expect("Compilation failed");
        let result = executable.call_i32("main", &[]).expect("Execution failed");
        assert_eq!(result, 42);
    }

    #[test]
    fn test_jit_int_addition() {
        let source = r#"
        int main() {
            int a = 10;
            int b = 20;
            return a + b;
        }
    "#;

        let options = GlslOptions {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Q32,
            q32_opts: crate::Q32Options::default(),
        };

        let mut executable = glsl_jit(source, options).expect("Compilation failed");
        let result = executable.call_i32("main", &[]).expect("Execution failed");
        assert_eq!(result, 30);
    }

    #[test]
    fn test_jit_float_literal() {
        let source = r#"
        float main() {
            return 3.14;
        }
    "#;

        let options = GlslOptions {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Q32,
            q32_opts: crate::Q32Options::default(),
        };

        let mut executable = glsl_jit(source, options).expect("Compilation failed");
        let result = executable.call_f32("main", &[]).expect("Execution failed");
        // Q32 format: allow tolerance for fixed-point conversion precision
        assert!((result - 3.14).abs() < 0.01);
    }

    #[test]
    fn test_jit_bool_literal() {
        let source = r#"
        bool main() {
            return true;
        }
    "#;

        let options = GlslOptions {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Q32,
            q32_opts: crate::Q32Options::default(),
        };

        let mut executable = glsl_jit(source, options).expect("Compilation failed");
        let result = executable.call_bool("main", &[]).expect("Execution failed");
        assert_eq!(result, true);
    }
}
