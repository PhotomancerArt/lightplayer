//! `LpsValue` ↔ JS numbers / arrays for calling shader exports from the browser.

use std::format;

use js_sys::Array;
use lpir::FloatMode;
use lps_shared::LpsType;
use lpvm::LpsValue;
use wasm_bindgen::JsValue;

use wasm_bindgen::JsCast;

use crate::error::WasmError;
use crate::module::glsl_type_to_wasm_components;

const Q16_16_SCALE: f32 = 65536.0;

fn encode_f32_js(f: f32, fm: FloatMode) -> JsValue {
    match fm {
        FloatMode::Q32 => JsValue::from_f64(((f * Q16_16_SCALE) as i32) as f64),
        FloatMode::F32 => JsValue::from_f64(f as f64),
    }
}

fn js_num_as_i32(v: &JsValue) -> Result<i32, WasmError> {
    v.as_f64()
        .map(|x| x as i32)
        .ok_or_else(|| WasmError::runtime("expected numeric JS value"))
}

fn js_slot_as_f32(v: &JsValue, fm: FloatMode) -> Result<f32, WasmError> {
    match fm {
        FloatMode::Q32 => Ok(js_num_as_i32(v)? as f32 / Q16_16_SCALE),
        FloatMode::F32 => v
            .as_f64()
            .map(|d| d as f32)
            .ok_or_else(|| WasmError::runtime("expected numeric JS value")),
    }
}

fn glsl_value_to_js_flat(
    ty: &LpsType,
    v: &LpsValue,
    fm: FloatMode,
) -> Result<Vec<JsValue>, WasmError> {
    use LpsType::*;
    Ok(match (ty, v) {
        (Float, LpsValue::F32(f)) => vec![encode_f32_js(*f, fm)],
        (Int, LpsValue::I32(i)) => vec![JsValue::from_f64(*i as f64)],
        (UInt, LpsValue::U32(u)) => vec![JsValue::from_f64(*u as f64)],
        (Bool, LpsValue::Bool(b)) => vec![JsValue::from_f64(if *b { 1.0 } else { 0.0 })],
        (Vec2, LpsValue::Vec2(a)) => vec![encode_f32_js(a[0], fm), encode_f32_js(a[1], fm)],
        (Vec3, LpsValue::Vec3(a)) => vec![
            encode_f32_js(a[0], fm),
            encode_f32_js(a[1], fm),
            encode_f32_js(a[2], fm),
        ],
        (Vec4, LpsValue::Vec4(a)) => vec![
            encode_f32_js(a[0], fm),
            encode_f32_js(a[1], fm),
            encode_f32_js(a[2], fm),
            encode_f32_js(a[3], fm),
        ],
        (IVec2, LpsValue::IVec2(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
        ],
        (IVec3, LpsValue::IVec3(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
        ],
        (IVec4, LpsValue::IVec4(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
            JsValue::from_f64(a[3] as f64),
        ],
        (UVec2, LpsValue::UVec2(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
        ],
        (UVec3, LpsValue::UVec3(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
        ],
        (UVec4, LpsValue::UVec4(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
            JsValue::from_f64(a[3] as f64),
        ],
        (BVec2, LpsValue::BVec2(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
        ],
        (BVec3, LpsValue::BVec3(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[2] { 1.0 } else { 0.0 }),
        ],
        (BVec4, LpsValue::BVec4(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[2] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[3] { 1.0 } else { 0.0 }),
        ],
        (Mat2, LpsValue::Mat2x2(m)) => vec![
            encode_f32_js(m[0][0], fm),
            encode_f32_js(m[0][1], fm),
            encode_f32_js(m[1][0], fm),
            encode_f32_js(m[1][1], fm),
        ],
        (Mat3, LpsValue::Mat3x3(m)) => {
            let mut out = Vec::with_capacity(9);
            for col in m.iter() {
                for x in col.iter() {
                    out.push(encode_f32_js(*x, fm));
                }
            }
            out
        }
        (Mat4, LpsValue::Mat4x4(m)) => {
            let mut out = Vec::with_capacity(16);
            for col in m.iter() {
                for x in col.iter() {
                    out.push(encode_f32_js(*x, fm));
                }
            }
            out
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
                out.extend(glsl_value_to_js_flat(element, it, fm)?);
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
                out.extend(glsl_value_to_js_flat(&m.ty, fv, fm)?);
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

pub(crate) fn build_js_args(
    param_types: &[LpsType],
    export_param_slots: usize,
    args: &[LpsValue],
    fm: FloatMode,
) -> Result<Array, WasmError> {
    if args.len() != param_types.len() {
        return Err(WasmError::runtime(format!(
            "wrong argument count: expected {}, got {}",
            param_types.len(),
            args.len()
        )));
    }
    let arr = Array::new();
    arr.push(&JsValue::from_f64(0.0));
    for (v, ty) in args.iter().zip(param_types.iter()) {
        for slot in glsl_value_to_js_flat(ty, v, fm)? {
            arr.push(&slot);
        }
    }
    if arr.length() as usize != export_param_slots {
        return Err(WasmError::runtime(format!(
            "internal: flattened arg count {} != export param slots {}",
            arr.length(),
            export_param_slots
        )));
    }
    Ok(arr)
}

fn js_result_slots(result: &JsValue, n: usize) -> Result<Vec<JsValue>, WasmError> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == 1 {
        return Ok(vec![result.clone()]);
    }
    let a = result
        .dyn_ref::<Array>()
        .cloned()
        .ok_or_else(|| WasmError::runtime("multi-return must be a JS Array in this runtime"))?;
    if a.length() as usize != n {
        return Err(WasmError::runtime(format!(
            "return slot count: expected {n}, got {}",
            a.length()
        )));
    }
    Ok((0..n).map(|i| a.get(i as u32)).collect())
}

pub(crate) fn js_result_to_lps_value(
    ty: &LpsType,
    result: &JsValue,
    fm: FloatMode,
) -> Result<LpsValue, WasmError> {
    let n = glsl_type_to_wasm_components(ty, fm).len();
    let slots = js_result_slots(result, n)?;
    decode_lps_from_js_slots(ty, &slots, fm, 0).map(|(v, _)| v)
}

fn decode_lps_from_js_slots(
    ty: &LpsType,
    slots: &[JsValue],
    fm: FloatMode,
    off: usize,
) -> Result<(LpsValue, usize), WasmError> {
    use LpsType::*;
    match ty {
        Void => Err(WasmError::runtime("void type in js_result")),
        Float => Ok((LpsValue::F32(js_slot_as_f32(&slots[off], fm)?), 1)),
        Int => Ok((LpsValue::I32(js_num_as_i32(&slots[off])?), 1)),
        UInt => Ok((LpsValue::U32(js_num_as_i32(&slots[off])? as u32), 1)),
        Bool => Ok((LpsValue::Bool(js_num_as_i32(&slots[off])? != 0), 1)),
        Vec2 => Ok((
            LpsValue::Vec2([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
            ]),
            2,
        )),
        Vec3 => Ok((
            LpsValue::Vec3([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
                js_slot_as_f32(&slots[off + 2], fm)?,
            ]),
            3,
        )),
        Vec4 => Ok((
            LpsValue::Vec4([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
                js_slot_as_f32(&slots[off + 2], fm)?,
                js_slot_as_f32(&slots[off + 3], fm)?,
            ]),
            4,
        )),
        IVec2 => Ok((
            LpsValue::IVec2([js_num_as_i32(&slots[off])?, js_num_as_i32(&slots[off + 1])?]),
            2,
        )),
        IVec3 => Ok((
            LpsValue::IVec3([
                js_num_as_i32(&slots[off])?,
                js_num_as_i32(&slots[off + 1])?,
                js_num_as_i32(&slots[off + 2])?,
            ]),
            3,
        )),
        IVec4 => Ok((
            LpsValue::IVec4([
                js_num_as_i32(&slots[off])?,
                js_num_as_i32(&slots[off + 1])?,
                js_num_as_i32(&slots[off + 2])?,
                js_num_as_i32(&slots[off + 3])?,
            ]),
            4,
        )),
        UVec2 => Ok((
            LpsValue::UVec2([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
            ]),
            2,
        )),
        UVec3 => Ok((
            LpsValue::UVec3([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
                js_num_as_i32(&slots[off + 2])? as u32,
            ]),
            3,
        )),
        UVec4 => Ok((
            LpsValue::UVec4([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
                js_num_as_i32(&slots[off + 2])? as u32,
                js_num_as_i32(&slots[off + 3])? as u32,
            ]),
            4,
        )),
        BVec2 => Ok((
            LpsValue::BVec2([
                js_num_as_i32(&slots[off])? != 0,
                js_num_as_i32(&slots[off + 1])? != 0,
            ]),
            2,
        )),
        BVec3 => Ok((
            LpsValue::BVec3([
                js_num_as_i32(&slots[off])? != 0,
                js_num_as_i32(&slots[off + 1])? != 0,
                js_num_as_i32(&slots[off + 2])? != 0,
            ]),
            3,
        )),
        BVec4 => Ok((
            LpsValue::BVec4([
                js_num_as_i32(&slots[off])? != 0,
                js_num_as_i32(&slots[off + 1])? != 0,
                js_num_as_i32(&slots[off + 2])? != 0,
                js_num_as_i32(&slots[off + 3])? != 0,
            ]),
            4,
        )),
        Mat2 => {
            let mut col0 = [0f32; 2];
            let mut col1 = [0f32; 2];
            col0[0] = js_slot_as_f32(&slots[off], fm)?;
            col0[1] = js_slot_as_f32(&slots[off + 1], fm)?;
            col1[0] = js_slot_as_f32(&slots[off + 2], fm)?;
            col1[1] = js_slot_as_f32(&slots[off + 3], fm)?;
            Ok((LpsValue::Mat2x2([col0, col1]), 4))
        }
        Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] = js_slot_as_f32(&slots[off + col * 3 + row], fm)?;
                }
            }
            Ok((LpsValue::Mat3x3(m), 9))
        }
        Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                for row in 0..4 {
                    m[col][row] = js_slot_as_f32(&slots[off + col * 4 + row], fm)?;
                }
            }
            Ok((LpsValue::Mat4x4(m), 16))
        }
        Array { element, len } => {
            let mut elems = Vec::with_capacity(*len as usize);
            let mut o = off;
            for _ in 0..*len {
                let (v, n) = decode_lps_from_js_slots(element, slots, fm, o)?;
                o += n;
                elems.push(v);
            }
            Ok((LpsValue::Array(elems.into_boxed_slice()), o - off))
        }
        Struct { name, members } => {
            let mut o = off;
            let mut fields = Vec::with_capacity(members.len());
            for m in members {
                let key = m
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("_{}", fields.len()));
                let (v, n) = decode_lps_from_js_slots(&m.ty, slots, fm, o)?;
                o += n;
                fields.push((key, v));
            }
            Ok((
                LpsValue::Struct {
                    name: name.clone(),
                    fields,
                },
                o - off,
            ))
        }
    }
}
