//! Shared aggregate pointer ABI helpers for wasm host marshalling (std430, Q32/F32).

use alloc::string::String;
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_shared::layout::type_size;
use lps_shared::{
    FnParam, LayoutRules, LpsType, ParamQualifier, lps_value_f32_to_q32, q32_to_lps_value_f32,
};
use lpvm::{LpsValueF32, LpvmDataQ32, decode_q32_return, flatten_q32_arg, glsl_component_count};

use crate::error::WasmError;
use crate::module::WasmExport;

pub(crate) fn type_passed_as_aggregate_ptr(ty: &LpsType) -> bool {
    matches!(ty, LpsType::Array { .. } | LpsType::Struct { .. })
}

pub(crate) fn export_needs_shadow_marshal(export: &WasmExport) -> bool {
    export.uses_sret
        || export
            .param_types
            .iter()
            .any(|t| type_passed_as_aggregate_ptr(t))
}

pub(crate) fn q32_std430_bytes_are_dense(ty: &LpsType) -> bool {
    type_size(ty, LayoutRules::Std430) == glsl_component_count(ty) * 4
}

/// Bytes from a callee sret buffer → flat Q32 return words (dense std430 only).
pub(crate) fn q32_sret_bytes_to_flat_return_words(
    ty: &LpsType,
    bytes: &[u8],
) -> Result<Vec<i32>, WasmError> {
    if !q32_std430_bytes_are_dense(ty) {
        return Err(WasmError::runtime(format!(
            "Q32 return `{ty:?}` is not densely packed; sret decode unsupported"
        )));
    }
    let need = type_size(ty, LayoutRules::Std430);
    if bytes.len() < need {
        return Err(WasmError::runtime(format!(
            "sret buffer too short: need {need}, have {}",
            bytes.len()
        )));
    }
    Ok(bytes[..need]
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

/// Pack flat Q32 argument words for one aggregate into std430 bytes (dense layouts only).
pub(crate) fn aggregate_flat_q32_words_to_std430_bytes(
    ty: &LpsType,
    words: &[i32],
) -> Result<Vec<u8>, WasmError> {
    let n = glsl_component_count(ty);
    if words.len() != n {
        return Err(WasmError::runtime(format!(
            "internal: expected {n} Q32 words for aggregate `{ty:?}`, got {}",
            words.len()
        )));
    }
    if !q32_std430_bytes_are_dense(ty) {
        return Err(WasmError::runtime(format!(
            "Q32 aggregate `{ty:?}` is not densely packed in std430; wasm host needs layout walk"
        )));
    }
    Ok(words.iter().flat_map(|w| w.to_le_bytes()).collect())
}

pub(crate) fn encode_aggregate_std430_bytes(
    ty: &LpsType,
    value: &LpsValueF32,
    fm: FloatMode,
) -> Result<Vec<u8>, WasmError> {
    match fm {
        FloatMode::F32 => LpvmDataQ32::from_value(ty.clone(), value)
            .map(|d| d.as_slice().to_vec())
            .map_err(|e| WasmError::runtime(format!("aggregate F32 encode: {e}"))),
        FloatMode::Q32 => {
            let q = lps_value_f32_to_q32(ty, value)
                .map_err(|e| WasmError::runtime(format!("aggregate Q32 encode: {e}")))?;
            let p = FnParam {
                name: String::new(),
                ty: ty.clone(),
                qualifier: ParamQualifier::In,
            };
            let flat = flatten_q32_arg(&p, &q)
                .map_err(|e| WasmError::runtime(format!("aggregate Q32 flatten: {e}")))?;
            let need = type_size(ty, LayoutRules::Std430);
            let bytes: Vec<u8> = flat.iter().flat_map(|w| w.to_le_bytes()).collect();
            if bytes.len() != need {
                return Err(WasmError::runtime(format!(
                    "Q32 aggregate encode size {} != std430 size {}",
                    bytes.len(),
                    need
                )));
            }
            Ok(bytes)
        }
    }
}

pub(crate) fn decode_aggregate_std430_bytes(
    ty: &LpsType,
    bytes: &[u8],
    fm: FloatMode,
) -> Result<LpsValueF32, WasmError> {
    match fm {
        FloatMode::F32 => {
            let need = type_size(ty, LayoutRules::Std430);
            if bytes.len() < need {
                return Err(WasmError::runtime(format!(
                    "aggregate F32 decode: need {need} bytes, have {}",
                    bytes.len()
                )));
            }
            let mut buf = LpvmDataQ32::new(ty.clone());
            buf.as_mut_slice().copy_from_slice(&bytes[..need]);
            buf.to_value()
                .map_err(|e| WasmError::runtime(format!("aggregate F32 decode: {e}")))
        }
        FloatMode::Q32 => {
            if !q32_std430_bytes_are_dense(ty) {
                return Err(WasmError::runtime(format!(
                    "Q32 aggregate `{ty:?}` decode is not implemented for padded std430"
                )));
            }
            let need = type_size(ty, LayoutRules::Std430);
            if bytes.len() < need {
                return Err(WasmError::runtime(format!(
                    "aggregate Q32 decode: need {need} bytes, have {}",
                    bytes.len()
                )));
            }
            let chunk = &bytes[..need];
            if chunk.len() % 4 != 0 {
                return Err(WasmError::runtime("aggregate Q32 decode: bad byte length"));
            }
            let words: Vec<i32> = chunk
                .chunks_exact(4)
                .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let q = decode_q32_return(ty, &words)
                .map_err(|e| WasmError::runtime(format!("decode_q32_return: {e}")))?;
            q32_to_lps_value_f32(ty, q).map_err(|e| WasmError::runtime(format!("q32 to f32: {e}")))
        }
    }
}
