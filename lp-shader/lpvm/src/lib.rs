//! LPVM runtime — traits, VM context, and execution abstractions.
//!
//! Core traits:
//! - [`LpvmEngine`] — compile LPIR and expose shared memory ([`LpvmMemory`])
//! - [`LpvmModule`] — compiled artifact + [`LpvmModule::instantiate`]
//! - [`LpvmInstance`] — call functions by name ([`LpsValueF32`] or flat Q32 words via [`LpvmInstance::call_q32`])
//! - [`LpvmMemory`] / [`ShaderPtr`] — host/guest shared heap
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]) and path
//! helpers come from [`lps_shared`]. This crate adds [`LpvmDataQ32`] and
//! [`VmContext`].

#![no_std]

extern crate alloc;

/// `path:line: …` at the **macro invocation** site ([`core::file!`], [`core::line!`]).
///
/// Takes the same arguments as [`alloc::format!`] (literal only, or `"…{}"` plus values).
///
/// ```ignore
/// CallError::Unsupported(traced_msg!("fixed message"));
/// CallError::Unsupported(traced_msg!("need {}, got {}", need, got));
/// ```
///
/// Prior art: `emit_err!` in `lpvm-native`, `log`'s `trace!`, many `internal_error!` macros.
#[macro_export]
macro_rules! traced_msg {
    ($($arg:tt)*) => {{
        $crate::alloc::format!(
            "{}:{}: {}",
            ::core::file!(),
            ::core::line!(),
            $crate::alloc::format!($($arg)*)
        )
    }};
}

mod buffer;
mod data_error;
mod debug;
mod engine;
mod instance;
mod lpvm_abi;
mod lpvm_data_q32;
mod memory;
mod module;
mod set_uniform;
mod vmcontext;

pub use buffer::{LpvmBuffer, LpvmPtr};
pub use data_error::DataError;
pub use debug::{FunctionDebugInfo, ModuleDebugInfo};
pub use engine::LpvmEngine;
pub use instance::LpvmInstance;
pub use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_shared::lps_value_f32::LpsValueF32;
pub use lps_shared::lps_value_q32::{LpsValueQ32, lps_value_f32_to_q32, q32_to_lps_value_f32};
pub use lps_shared::path::{LpsPathSeg, PathParseError, parse_path};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use lpvm_abi::{
    CallError, CallResult, GlslReturn, decode_q32_return, flat_q32_words_from_f32_args,
    flatten_q32_arg, flatten_q32_return, glsl_component_count, unflatten_q32_args,
};
pub use lpvm_data_q32::LpvmDataQ32;
pub use memory::{AllocError, BumpLpvmMemory, LpvmMemory};
pub use module::LpvmModule;
pub use set_uniform::{encode_uniform_write, encode_uniform_write_q32};
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};

use lpir::{IrFunction, IrType};

/// Verify an [`IrFunction`] has the shape required by [`LpvmInstance::call_render_texture`]:
/// `(Pointer, I32, I32) -> ()` in LPIR, with implicit vmctx in vreg 0.
pub fn validate_render_texture_sig_ir(ir: &IrFunction) -> Result<(), &'static str> {
    if !ir.return_types.is_empty() {
        return Err("render-texture function must return void");
    }
    if ir.param_count != 3 {
        return Err("render-texture function must take 3 parameters");
    }
    // vreg_types[0] is vmctx (always Pointer); user params start at index 1.
    let p0 = ir.vreg_types.get(1).copied();
    let p1 = ir.vreg_types.get(2).copied();
    let p2 = ir.vreg_types.get(3).copied();
    if p0 != Some(IrType::Pointer) {
        return Err("render-texture param 0 must be Pointer");
    }
    if p1 != Some(IrType::I32) {
        return Err("render-texture param 1 must be I32 width");
    }
    if p2 != Some(IrType::I32) {
        return Err("render-texture param 2 must be I32 height");
    }
    Ok(())
}

#[cfg(test)]
mod validate_render_texture_tests {
    use super::validate_render_texture_sig_ir;
    use lpir::IrType;
    use lpir::builder::FunctionBuilder;

    fn make_ir_fn_with_param_types(
        name: &str,
        params: &[IrType],
        rets: &[IrType],
    ) -> lpir::IrFunction {
        let mut fb = FunctionBuilder::new(name, rets);
        for ty in params {
            let _ = fb.add_param(*ty);
        }
        fb.push_return(&[]);
        fb.finish()
    }

    #[test]
    fn validate_render_texture_sig_ir_accepts_expected() {
        let f = make_ir_fn_with_param_types(
            "__render_texture_rgba16",
            &[IrType::Pointer, IrType::I32, IrType::I32],
            &[],
        );
        assert!(validate_render_texture_sig_ir(&f).is_ok());
    }

    #[test]
    fn validate_render_texture_sig_ir_rejects_wrong_return() {
        let f = make_ir_fn_with_param_types(
            "bad",
            &[IrType::Pointer, IrType::I32, IrType::I32],
            &[IrType::I32],
        );
        assert!(validate_render_texture_sig_ir(&f).is_err());
    }

    #[test]
    fn validate_render_texture_sig_ir_rejects_wrong_arity() {
        let f = make_ir_fn_with_param_types("bad", &[IrType::Pointer, IrType::I32], &[]);
        assert!(validate_render_texture_sig_ir(&f).is_err());
    }

    #[test]
    fn validate_render_texture_sig_ir_rejects_non_pointer_first_param() {
        let f = make_ir_fn_with_param_types("bad", &[IrType::I32, IrType::I32, IrType::I32], &[]);
        assert!(validate_render_texture_sig_ir(&f).is_err());
    }
}
