//! Encode and decode VMContext private global values by path.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpir::FloatMode;
use lps_q32::Q32;
use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
use lps_shared::path_resolve::LpsTypePathExt;
use lps_shared::{
    FnParam, LayoutRules, LpsModuleSig, LpsTexture2DDescriptor, LpsTexture2DValue, LpsType,
    LpsValueF32, ParamQualifier, lps_value_f32_to_q32,
};

use crate::LpvmDataQ32;
use crate::data_error::DataError;
use crate::lpvm_abi::{CallError, flatten_q32_arg};

/// Absolute VMContext byte range for one private global path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalDataSpan {
    pub offset: usize,
    pub len: usize,
    pub ty: LpsType,
}

/// Resolve a private-global path into its VMContext byte range and logical type.
pub fn global_data_span(sig: &LpsModuleSig, path: &str) -> Result<GlobalDataSpan, DataError> {
    let globals = sig
        .globals_type
        .as_ref()
        .ok_or_else(|| DataError::type_mismatch("globals", "module has no globals"))?;
    let ty = globals.type_at_path(path)?;
    let rel = globals.offset_for_path(path, LayoutRules::Std430, 0)?;
    let offset = sig
        .globals_offset()
        .checked_add(rel)
        .ok_or_else(|| DataError::type_mismatch("offset", "overflow"))?;
    Ok(GlobalDataSpan {
        offset,
        len: type_size(&ty, LayoutRules::Std430),
        ty,
    })
}

/// Resolve `path` in the globals struct and encode `value` as VMContext bytes.
pub fn encode_global_write(
    sig: &LpsModuleSig,
    path: &str,
    value: &LpsValueF32,
    float_mode: FloatMode,
) -> Result<(usize, Vec<u8>), DataError> {
    let span = global_data_span(sig, path)?;
    let bytes = encode_value_bytes(&span.ty, value, float_mode)?;
    if bytes.len() != span.len {
        return Err(DataError::type_mismatch(
            "global payload",
            format!("encoded {} bytes, layout needs {}", bytes.len(), span.len),
        ));
    }
    Ok((span.offset, bytes))
}

/// Decode VMContext bytes for a private global value.
pub fn decode_global_read(
    ty: &LpsType,
    bytes: &[u8],
    float_mode: FloatMode,
) -> Result<LpsValueF32, DataError> {
    let need = type_size(ty, LayoutRules::Std430);
    if bytes.len() < need {
        return Err(DataError::BufferTooShort {
            need,
            have: bytes.len(),
        });
    }
    let bytes = &bytes[..need];
    match float_mode {
        FloatMode::F32 => {
            let mut data = LpvmDataQ32::new(ty.clone());
            data.as_mut_slice().copy_from_slice(bytes);
            data.to_value()
        }
        FloatMode::Q32 => decode_q32_memory_value(ty, bytes, LayoutRules::Std430),
    }
}

fn encode_value_bytes(
    ty: &LpsType,
    value: &LpsValueF32,
    float_mode: FloatMode,
) -> Result<Vec<u8>, DataError> {
    match float_mode {
        FloatMode::F32 => Ok(LpvmDataQ32::from_value(ty.clone(), value)?
            .as_slice()
            .to_vec()),
        FloatMode::Q32 => {
            let q = lps_value_f32_to_q32(ty, value)
                .map_err(|e| DataError::type_mismatch("q32 encode", e.to_string()))?;
            let param = FnParam {
                name: String::new(),
                ty: ty.clone(),
                qualifier: ParamQualifier::In,
            };
            let words = flatten_q32_arg(&param, &q).map_err(call_err_to_data)?;
            Ok(words.into_iter().flat_map(i32::to_le_bytes).collect())
        }
    }
}

fn call_err_to_data(e: CallError) -> DataError {
    DataError::type_mismatch("q32 global data", format!("{e}"))
}

fn decode_q32_memory_value(
    ty: &LpsType,
    bytes: &[u8],
    rules: LayoutRules,
) -> Result<LpsValueF32, DataError> {
    let need = type_size(ty, rules);
    if bytes.len() < need {
        return Err(DataError::BufferTooShort {
            need,
            have: bytes.len(),
        });
    }
    let bytes = &bytes[..need];
    Ok(match ty {
        LpsType::Void => {
            return Err(DataError::type_mismatch("void", "cannot load void global"));
        }
        LpsType::Float => LpsValueF32::F32(q32_at(bytes, 0)?.to_f32()),
        LpsType::Int => LpsValueF32::I32(i32_at(bytes, 0)?),
        LpsType::UInt => LpsValueF32::U32(u32_at(bytes, 0)?),
        LpsType::Bool => LpsValueF32::Bool(i32_at(bytes, 0)? != 0),
        LpsType::Vec2 => {
            LpsValueF32::Vec2([q32_at(bytes, 0)?.to_f32(), q32_at(bytes, 4)?.to_f32()])
        }
        LpsType::Vec3 => LpsValueF32::Vec3([
            q32_at(bytes, 0)?.to_f32(),
            q32_at(bytes, 4)?.to_f32(),
            q32_at(bytes, 8)?.to_f32(),
        ]),
        LpsType::Vec4 => LpsValueF32::Vec4([
            q32_at(bytes, 0)?.to_f32(),
            q32_at(bytes, 4)?.to_f32(),
            q32_at(bytes, 8)?.to_f32(),
            q32_at(bytes, 12)?.to_f32(),
        ]),
        LpsType::IVec2 => LpsValueF32::IVec2([i32_at(bytes, 0)?, i32_at(bytes, 4)?]),
        LpsType::IVec3 => {
            LpsValueF32::IVec3([i32_at(bytes, 0)?, i32_at(bytes, 4)?, i32_at(bytes, 8)?])
        }
        LpsType::IVec4 => LpsValueF32::IVec4([
            i32_at(bytes, 0)?,
            i32_at(bytes, 4)?,
            i32_at(bytes, 8)?,
            i32_at(bytes, 12)?,
        ]),
        LpsType::UVec2 => LpsValueF32::UVec2([u32_at(bytes, 0)?, u32_at(bytes, 4)?]),
        LpsType::UVec3 => {
            LpsValueF32::UVec3([u32_at(bytes, 0)?, u32_at(bytes, 4)?, u32_at(bytes, 8)?])
        }
        LpsType::UVec4 => LpsValueF32::UVec4([
            u32_at(bytes, 0)?,
            u32_at(bytes, 4)?,
            u32_at(bytes, 8)?,
            u32_at(bytes, 12)?,
        ]),
        LpsType::BVec2 => LpsValueF32::BVec2([i32_at(bytes, 0)? != 0, i32_at(bytes, 4)? != 0]),
        LpsType::BVec3 => LpsValueF32::BVec3([
            i32_at(bytes, 0)? != 0,
            i32_at(bytes, 4)? != 0,
            i32_at(bytes, 8)? != 0,
        ]),
        LpsType::BVec4 => LpsValueF32::BVec4([
            i32_at(bytes, 0)? != 0,
            i32_at(bytes, 4)? != 0,
            i32_at(bytes, 8)? != 0,
            i32_at(bytes, 12)? != 0,
        ]),
        LpsType::Mat2 => LpsValueF32::Mat2x2([
            [q32_at(bytes, 0)?.to_f32(), q32_at(bytes, 4)?.to_f32()],
            [q32_at(bytes, 8)?.to_f32(), q32_at(bytes, 12)?.to_f32()],
        ]),
        LpsType::Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for (col, dst) in m.iter_mut().enumerate() {
                let base = col * 12;
                *dst = [
                    q32_at(bytes, base)?.to_f32(),
                    q32_at(bytes, base + 4)?.to_f32(),
                    q32_at(bytes, base + 8)?.to_f32(),
                ];
            }
            LpsValueF32::Mat3x3(m)
        }
        LpsType::Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for (col, dst) in m.iter_mut().enumerate() {
                let base = col * 16;
                *dst = [
                    q32_at(bytes, base)?.to_f32(),
                    q32_at(bytes, base + 4)?.to_f32(),
                    q32_at(bytes, base + 8)?.to_f32(),
                    q32_at(bytes, base + 12)?.to_f32(),
                ];
            }
            LpsValueF32::Mat4x4(m)
        }
        LpsType::Texture2D => LpsValueF32::Texture2D(LpsTexture2DValue::from_guest_descriptor(
            LpsTexture2DDescriptor {
                ptr: u32_at(bytes, 0)?,
                width: u32_at(bytes, 4)?,
                height: u32_at(bytes, 8)?,
                row_stride: u32_at(bytes, 12)?,
            },
        )),
        LpsType::Array { element, len } => {
            let stride = array_stride(element, rules);
            let esz = type_size(element, rules);
            let mut elems = Vec::with_capacity(*len as usize);
            for i in 0..(*len as usize) {
                let base = i * stride;
                elems.push(decode_q32_memory_value(
                    element,
                    &bytes[base..base + esz],
                    rules,
                )?);
            }
            LpsValueF32::Array(elems.into_boxed_slice())
        }
        LpsType::Struct { name, members } => {
            let mut cursor = 0usize;
            let mut fields = Vec::with_capacity(members.len());
            for (i, m) in members.iter().enumerate() {
                cursor = round_up(cursor, type_alignment(&m.ty, rules));
                let msz = type_size(&m.ty, rules);
                let key = m.name.clone().unwrap_or_else(|| format!("_{i}"));
                let value = decode_q32_memory_value(&m.ty, &bytes[cursor..cursor + msz], rules)?;
                fields.push((key, value));
                cursor += msz;
            }
            LpsValueF32::Struct {
                name: name.clone(),
                fields,
            }
        }
    })
}

fn q32_at(bytes: &[u8], offset: usize) -> Result<Q32, DataError> {
    Ok(Q32::from_fixed(i32_at(bytes, offset)?))
}

fn i32_at(bytes: &[u8], offset: usize) -> Result<i32, DataError> {
    let word = bytes
        .get(offset..offset + 4)
        .ok_or(DataError::BufferTooShort {
            need: offset + 4,
            have: bytes.len(),
        })?;
    Ok(i32::from_le_bytes([word[0], word[1], word[2], word[3]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, DataError> {
    Ok(i32_at(bytes, offset)? as u32)
}
