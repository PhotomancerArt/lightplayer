//! Host-side marshalling of Q32 call arguments to match M1 LPIR (pointer aggregates).

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use cranelift_codegen::isa::TargetIsa;
use lpir::lpir_module::IrFunction;
use lpir::types::IrType;
use lps_shared::{FnParam, LayoutRules, LpsType};
use lpvm::{CallError, LpvmMemory, glsl_component_count, type_alignment, type_size};
use lpvm_cranelift::signature_uses_struct_return;

use crate::memory::EmuSharedArena;

fn aggregate_by_value(ty: &LpsType) -> bool {
    matches!(ty, LpsType::Array { .. } | LpsType::Struct { .. })
}

/// Map flattened Q32 argument words (one GLSL parameter after another) to LPIR user arg words
/// (`i32` pointer values and scalar lanes), allocating aggregate storage in `arena`.
pub(crate) fn ir_user_args_from_q32_words(
    ir_func: &IrFunction,
    params: &[FnParam],
    words: &[i32],
    arena: &EmuSharedArena,
) -> Result<Vec<i32>, CallError> {
    let base = ir_func.vmctx_vreg.0 as usize + ir_func.hidden_param_slots() as usize;
    let mut out = Vec::with_capacity(ir_func.param_count as usize);
    let mut wi = 0usize;
    let mut ui = 0usize;

    for p in params {
        let ir_ty = ir_func.vreg_types.get(base + ui).copied().ok_or_else(|| {
            CallError::Unsupported(format!("IR vreg_types missing for user param slot {ui}"))
        })?;

        if ir_ty == IrType::Pointer && aggregate_by_value(&p.ty) {
            let n = glsl_component_count(&p.ty);
            if wi + n > words.len() {
                return Err(CallError::Unsupported(format!(
                    "not enough argument words for `{}`: need {}, have {} total",
                    p.name,
                    wi + n,
                    words.len()
                )));
            }
            let slice = &words[wi..wi + n];
            wi += n;
            ui += 1;

            // Guest `Memcpy` + Q32 loads expect std430-sized buffers with **Q32 lane words**
            // (same `i32` LE packing as scalar/vec call args), not `LpvmDataQ32::from_value`
            // IEEE floats.
            match &p.ty {
                LpsType::Array { .. } => {
                    let mut raw = Vec::with_capacity(n * 4);
                    for &w in slice {
                        raw.extend_from_slice(&w.to_le_bytes());
                    }
                    let align = type_alignment(&p.ty, LayoutRules::Std430);
                    let buf = arena
                        .alloc(raw.len(), align)
                        .map_err(|e| CallError::Unsupported(format!("shared alloc: {e}")))?;
                    unsafe {
                        core::ptr::copy_nonoverlapping(raw.as_ptr(), buf.native_ptr(), raw.len());
                    }
                    out.push(buf.guest_base() as i32);
                }
                LpsType::Struct { .. } => {
                    return Err(CallError::Unsupported(String::from(
                        "emu host marshalling: struct aggregate `in` args are not supported yet",
                    )));
                }
                _ => {
                    return Err(CallError::Unsupported(String::from(
                        "emu host marshalling: unexpected aggregate type",
                    )));
                }
            }
        } else {
            let n = glsl_component_count(&p.ty);
            if wi + n > words.len() {
                return Err(CallError::Unsupported(format!(
                    "not enough argument words for `{}`: need {}, have {} total",
                    p.name,
                    wi + n,
                    words.len()
                )));
            }
            if base + ui + n > ir_func.vreg_types.len() {
                return Err(CallError::Unsupported(format!(
                    "IR vreg_types shorter than expected for param `{}`",
                    p.name
                )));
            }
            out.extend_from_slice(&words[wi..wi + n]);
            wi += n;
            ui += n;
        }
    }

    if wi != words.len() {
        return Err(CallError::Unsupported(format!(
            "extra argument words after parameters: used {}, {} total",
            wi,
            words.len()
        )));
    }
    if ui != ir_func.param_count as usize {
        return Err(CallError::Unsupported(format!(
            "IR param_count {} does not match marshalled user slots {}",
            ir_func.param_count, ui
        )));
    }
    if out.len() != ir_func.param_count as usize {
        return Err(CallError::Unsupported(format!(
            "marshalled arg words {} != IR param_count {}",
            out.len(),
            ir_func.param_count
        )));
    }

    Ok(out)
}

/// Whether the emulator must use a struct-return buffer, and its size in bytes.
///
/// Uses [`signature_uses_struct_return`] as the single predicate matching
/// [`lpvm_cranelift::signature_for_ir_func`]. Explicit LPIR sret (`sret_arg`) sizes from
/// `return_ty_for_explicit` (std430); implicit ABI sret uses `return_types.len() * 4`.
pub(crate) fn emulator_struct_return_buffer(
    isa: &dyn TargetIsa,
    ir_func: &IrFunction,
    return_ty_for_explicit: Option<&LpsType>,
) -> Result<(bool, usize), CallError> {
    if !signature_uses_struct_return(isa, ir_func) {
        return Ok((false, 0));
    }
    let bytes = if ir_func.sret_arg.is_some() {
        let rt = return_ty_for_explicit.ok_or_else(|| {
            CallError::Unsupported(String::from(
                "internal: LPIR sret without host return type for sizing",
            ))
        })?;
        sret_buffer_byte_size(rt)?
    } else {
        ir_func.return_types.len() * 4
    };
    Ok((true, bytes))
}

/// Byte size of the sret buffer for an aggregate return under std430.
pub(crate) fn sret_buffer_byte_size(return_ty: &LpsType) -> Result<usize, CallError> {
    if matches!(return_ty, LpsType::Void) {
        return Err(CallError::Unsupported(String::from(
            "sret_buffer_byte_size: void return",
        )));
    }
    Ok(type_size(return_ty, LayoutRules::Std430))
}
