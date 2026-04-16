//! Encode uniform writes for [`crate::LpvmInstance::set_uniform`] / [`crate::LpvmInstance::set_uniform_q32`].

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_shared::layout::type_size;
use lps_shared::path_resolve::LpsTypePathExt;
use lps_shared::{
    FnParam, LayoutRules, LpsModuleSig, LpsValueF32, LpsValueQ32, ParamQualifier,
    lps_value_f32_to_q32,
};

use crate::LpvmDataQ32;
use crate::data_error::DataError;
use crate::lpvm_abi::{CallError, flatten_q32_arg};

/// Resolve `path` in the uniforms struct, encode `value` as vmctx bytes for `float_mode`.
///
/// Returns absolute byte offset from the vmctx buffer base and the payload.
pub fn encode_uniform_write(
    sig: &LpsModuleSig,
    path: &str,
    value: &LpsValueF32,
    float_mode: FloatMode,
) -> Result<(usize, Vec<u8>), DataError> {
    let ut = sig
        .uniforms_type
        .as_ref()
        .ok_or_else(|| DataError::type_mismatch("uniforms", "module has no uniforms"))?;
    let leaf_ty = ut.type_at_path(path)?;
    let rel = ut.offset_for_path(path, LayoutRules::Std430, 0)?;
    let abs = sig
        .uniforms_offset()
        .checked_add(rel)
        .ok_or_else(|| DataError::type_mismatch("offset", "overflow"))?;

    let bytes = match float_mode {
        FloatMode::F32 => {
            let d = LpvmDataQ32::from_value(leaf_ty.clone(), value)?;
            d.as_slice().to_vec()
        }
        FloatMode::Q32 => {
            let q = lps_value_f32_to_q32(&leaf_ty, value)
                .map_err(|e| DataError::type_mismatch("q32 encode", e.to_string()))?;
            let p = FnParam {
                name: String::new(),
                ty: leaf_ty.clone(),
                qualifier: ParamQualifier::In,
            };
            let words = flatten_q32_arg(&p, &q).map_err(flatten_err_to_data)?;
            words.into_iter().flat_map(|w| w.to_le_bytes()).collect()
        }
    };

    let need = type_size(&leaf_ty, LayoutRules::Std430);
    if bytes.len() != need {
        return Err(DataError::type_mismatch(
            "uniform payload",
            format!("encoded {} bytes, layout needs {}", bytes.len(), need),
        ));
    }
    Ok((abs, bytes))
}

/// Like [`encode_uniform_write`] but with pre-encoded Q32 values (raw `i32` lanes per ABI).
pub fn encode_uniform_write_q32(
    sig: &LpsModuleSig,
    path: &str,
    value: &LpsValueQ32,
) -> Result<(usize, Vec<u8>), DataError> {
    let ut = sig
        .uniforms_type
        .as_ref()
        .ok_or_else(|| DataError::type_mismatch("uniforms", "module has no uniforms"))?;
    let leaf_ty = ut.type_at_path(path)?;
    let rel = ut.offset_for_path(path, LayoutRules::Std430, 0)?;
    let abs = sig
        .uniforms_offset()
        .checked_add(rel)
        .ok_or_else(|| DataError::type_mismatch("offset", "overflow"))?;

    let p = FnParam {
        name: String::new(),
        ty: leaf_ty.clone(),
        qualifier: ParamQualifier::In,
    };
    let words = flatten_q32_arg(&p, value).map_err(flatten_err_to_data)?;
    let bytes: Vec<u8> = words.into_iter().flat_map(|w| w.to_le_bytes()).collect();

    let need = type_size(&leaf_ty, LayoutRules::Std430);
    if bytes.len() != need {
        return Err(DataError::type_mismatch(
            "uniform payload",
            format!("encoded {} bytes, layout needs {}", bytes.len(), need),
        ));
    }
    Ok((abs, bytes))
}

fn flatten_err_to_data(e: CallError) -> DataError {
    DataError::type_mismatch("flatten_q32", format!("{e}"))
}
