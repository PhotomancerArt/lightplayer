//! Marshal [`LpsValue`] ↔ wasmtime [`Val`] using the same layout as `lps-filetests` WASM runner.

use std::format;

use lpir::FloatMode;
use lps_shared::LpsType;
use lpvm::LpsValue;
use wasm_encoder::ValType as WasmValType;
use wasmtime::Val;

use crate::error::WasmError;
use crate::module::glsl_type_to_wasm_components;

const Q16_16_SCALE: f32 = 65536.0;

pub(crate) fn encode_f32_wasm(f: f32, fm: FloatMode) -> Val {
    match fm {
        FloatMode::Q32 => Val::I32((f * Q16_16_SCALE) as i32),
        FloatMode::F32 => Val::F32(f.to_bits()),
    }
}

pub(crate) fn wasm_val_to_f32(v: &Val, fm: FloatMode) -> Result<f32, WasmError> {
    match (v, fm) {
        (Val::I32(i), FloatMode::Q32) => Ok(*i as f32 / Q16_16_SCALE),
        (Val::F32(bits), FloatMode::F32) => Ok(f32::from_bits(*bits)),
        _ => Err(WasmError::runtime(format!(
            "unexpected value for float (float_mode={fm:?})"
        ))),
    }
}

fn glsl_value_to_wasm_flat(
    ty: &LpsType,
    v: &LpsValue,
    fm: FloatMode,
) -> Result<Vec<Val>, WasmError> {
    use LpsType::*;
    Ok(match (ty, v) {
        (Float, LpsValue::F32(f)) => vec![encode_f32_wasm(*f, fm)],
        (Int, LpsValue::I32(i)) => vec![Val::I32(*i)],
        (UInt, LpsValue::U32(u)) => vec![Val::I32(*u as i32)],
        (Bool, LpsValue::Bool(b)) => vec![Val::I32(if *b { 1 } else { 0 })],
        (Vec2, LpsValue::Vec2(a)) => vec![encode_f32_wasm(a[0], fm), encode_f32_wasm(a[1], fm)],
        (Vec3, LpsValue::Vec3(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
        ],
        (Vec4, LpsValue::Vec4(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
            encode_f32_wasm(a[3], fm),
        ],
        (IVec2, LpsValue::IVec2(a)) => vec![Val::I32(a[0]), Val::I32(a[1])],
        (IVec3, LpsValue::IVec3(a)) => vec![Val::I32(a[0]), Val::I32(a[1]), Val::I32(a[2])],
        (IVec4, LpsValue::IVec4(a)) => vec![
            Val::I32(a[0]),
            Val::I32(a[1]),
            Val::I32(a[2]),
            Val::I32(a[3]),
        ],
        (UVec2, LpsValue::UVec2(a)) => vec![Val::I32(a[0] as i32), Val::I32(a[1] as i32)],
        (UVec3, LpsValue::UVec3(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
        ],
        (UVec4, LpsValue::UVec4(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
            Val::I32(a[3] as i32),
        ],
        (BVec2, LpsValue::BVec2(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
        ],
        (BVec3, LpsValue::BVec3(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
        ],
        (BVec4, LpsValue::BVec4(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
            Val::I32(if a[3] { 1 } else { 0 }),
        ],
        (Mat2, LpsValue::Mat2x2(m)) => vec![
            encode_f32_wasm(m[0][0], fm),
            encode_f32_wasm(m[0][1], fm),
            encode_f32_wasm(m[1][0], fm),
            encode_f32_wasm(m[1][1], fm),
        ],
        (Mat3, LpsValue::Mat3x3(m)) => {
            let mut v = Vec::with_capacity(9);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
        }
        (Mat4, LpsValue::Mat4x4(m)) => {
            let mut v = Vec::with_capacity(16);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
        }
        (Array { element, len }, LpsValue::Array(items)) => {
            if items.len() != *len as usize {
                return Err(WasmError::runtime(format!(
                    "array value length {} does not match type length {}",
                    items.len(),
                    len
                )));
            }
            let mut out = Vec::new();
            for it in items.iter() {
                out.extend(glsl_value_to_wasm_flat(element, it, fm)?);
            }
            out
        }
        (Struct { members, .. }, LpsValue::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err(WasmError::runtime(format!(
                    "struct field count {} does not match type field count {}",
                    fields.len(),
                    members.len()
                )));
            }
            let mut out = Vec::new();
            for (m, (_, fv)) in members.iter().zip(fields.iter()) {
                out.extend(glsl_value_to_wasm_flat(&m.ty, fv, fm)?);
            }
            out
        }
        _ => {
            return Err(WasmError::runtime(format!(
                "value {v:?} does not match parameter type {ty:?}"
            )));
        }
    })
}

pub(crate) fn build_wasm_args(
    param_types: &[LpsType],
    export_param_slots: usize,
    args: &[LpsValue],
    fm: FloatMode,
) -> Result<Vec<Val>, WasmError> {
    if args.len() != param_types.len() {
        return Err(WasmError::runtime(format!(
            "wrong argument count: expected {}, got {}",
            param_types.len(),
            args.len()
        )));
    }
    let mut wasm_args = Vec::new();
    wasm_args.push(Val::I32(0));
    for (v, ty) in args.iter().zip(param_types.iter()) {
        wasm_args.extend(glsl_value_to_wasm_flat(ty, v, fm)?);
    }
    if wasm_args.len() != export_param_slots {
        return Err(WasmError::runtime(format!(
            "internal: flattened arg count {} != export param slots {}",
            wasm_args.len(),
            export_param_slots
        )));
    }
    Ok(wasm_args)
}

pub(crate) fn wasm_vals_to_lps_value(
    ty: &LpsType,
    vals: &[Val],
    fm: FloatMode,
) -> Result<(LpsValue, usize), WasmError> {
    use LpsType::*;
    match ty {
        Void => Err(WasmError::runtime("void type in wasm_vals_to_lps_value")),
        Float => {
            let f = wasm_val_to_f32(&vals[0], fm)?;
            Ok((LpsValue::F32(f), 1))
        }
        Int => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValue::I32(*i), 1)),
            _ => Err(WasmError::runtime("expected i32 for int return")),
        },
        UInt => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValue::U32(*i as u32), 1)),
            _ => Err(WasmError::runtime("expected i32 for uint return")),
        },
        Bool => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValue::Bool(*i != 0), 1)),
            _ => Err(WasmError::runtime("expected i32 for bool return")),
        },
        Vec2 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            Ok((LpsValue::Vec2([a, b]), 2))
        }
        Vec3 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            Ok((LpsValue::Vec3([a, b, c]), 3))
        }
        Vec4 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            let d = wasm_val_to_f32(&vals[3], fm)?;
            Ok((LpsValue::Vec4([a, b, c, d]), 4))
        }
        IVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValue::IVec2([*a, *b]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for ivec2")),
        },
        IVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => Ok((LpsValue::IVec3([*a, *b, *c]), 3)),
            _ => Err(WasmError::runtime("expected i32 triple for ivec3")),
        },
        IVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((LpsValue::IVec4([*a, *b, *c, *d]), 4))
            }
            _ => Err(WasmError::runtime("expected four i32 for ivec4")),
        },
        UVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValue::UVec2([*a as u32, *b as u32]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for uvec2")),
        },
        UVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((LpsValue::UVec3([*a as u32, *b as u32, *c as u32]), 3))
            }
            _ => Err(WasmError::runtime("expected i32 triple for uvec3")),
        },
        UVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => Ok((
                LpsValue::UVec4([*a as u32, *b as u32, *c as u32, *d as u32]),
                4,
            )),
            _ => Err(WasmError::runtime("expected four i32 for uvec4")),
        },
        BVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValue::BVec2([*a != 0, *b != 0]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for bvec2")),
        },
        BVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((LpsValue::BVec3([*a != 0, *b != 0, *c != 0]), 3))
            }
            _ => Err(WasmError::runtime("expected i32 triple for bvec3")),
        },
        BVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((LpsValue::BVec4([*a != 0, *b != 0, *c != 0, *d != 0]), 4))
            }
            _ => Err(WasmError::runtime("expected four i32 for bvec4")),
        },
        Mat2 => {
            let mut col0 = [0f32; 2];
            let mut col1 = [0f32; 2];
            col0[0] = wasm_val_to_f32(&vals[0], fm)?;
            col0[1] = wasm_val_to_f32(&vals[1], fm)?;
            col1[0] = wasm_val_to_f32(&vals[2], fm)?;
            col1[1] = wasm_val_to_f32(&vals[3], fm)?;
            Ok((LpsValue::Mat2x2([col0, col1]), 4))
        }
        Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 3 + row], fm)?;
                }
            }
            Ok((LpsValue::Mat3x3(m), 9))
        }
        Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                for row in 0..4 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 4 + row], fm)?;
                }
            }
            Ok((LpsValue::Mat4x4(m), 16))
        }
        Array { element, len } => {
            let mut off = 0;
            let mut elems = Vec::with_capacity(*len as usize);
            for _ in 0..*len {
                let (v, n) = wasm_vals_to_lps_value(element, &vals[off..], fm)?;
                off += n;
                elems.push(v);
            }
            Ok((LpsValue::Array(elems.into_boxed_slice()), off))
        }
        Struct { name, members } => {
            let mut off = 0;
            let mut fields = Vec::with_capacity(members.len());
            for m in members {
                let key = m
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("_{}", fields.len()));
                let (v, n) = wasm_vals_to_lps_value(&m.ty, &vals[off..], fm)?;
                off += n;
                fields.push((key, v));
            }
            Ok((
                LpsValue::Struct {
                    name: name.clone(),
                    fields,
                },
                off,
            ))
        }
    }
}

pub(crate) fn zero_results_for_type(ty: &LpsType, fm: FloatMode) -> Vec<Val> {
    glsl_type_to_wasm_components(ty, fm)
        .iter()
        .map(|t| match t {
            WasmValType::I32 => Val::I32(0),
            WasmValType::F32 => Val::F32(0f32.to_bits()),
            _ => Val::I32(0),
        })
        .collect()
}
