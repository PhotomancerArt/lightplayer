//! `LpsValue` ↔ JS numbers / arrays for calling shader exports from the browser.

use std::format;

use js_sys::{Array, ArrayBuffer, Reflect, Uint8Array, WebAssembly};
use lpir::FloatMode;
use lps_shared::layout::{type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};
use lpvm::{LpsValueF32, glsl_component_count};
use wasm_bindgen::JsValue;

use wasm_bindgen::JsCast;

use crate::aggregate_abi::{
    aggregate_flat_q32_words_to_std430_bytes, encode_aggregate_std430_bytes,
    q32_sret_bytes_to_flat_return_words, type_passed_as_aggregate_ptr,
};
use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport, glsl_type_to_wasm_components};

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
    v: &LpsValueF32,
    fm: FloatMode,
) -> Result<Vec<JsValue>, WasmError> {
    use LpsType::*;
    Ok(match (ty, v) {
        (Float, LpsValueF32::F32(f)) => vec![encode_f32_js(*f, fm)],
        (Int, LpsValueF32::I32(i)) => vec![JsValue::from_f64(*i as f64)],
        (UInt, LpsValueF32::U32(u)) => vec![JsValue::from_f64(*u as f64)],
        (Bool, LpsValueF32::Bool(b)) => vec![JsValue::from_f64(if *b { 1.0 } else { 0.0 })],
        (Vec2, LpsValueF32::Vec2(a)) => vec![encode_f32_js(a[0], fm), encode_f32_js(a[1], fm)],
        (Vec3, LpsValueF32::Vec3(a)) => vec![
            encode_f32_js(a[0], fm),
            encode_f32_js(a[1], fm),
            encode_f32_js(a[2], fm),
        ],
        (Vec4, LpsValueF32::Vec4(a)) => vec![
            encode_f32_js(a[0], fm),
            encode_f32_js(a[1], fm),
            encode_f32_js(a[2], fm),
            encode_f32_js(a[3], fm),
        ],
        (IVec2, LpsValueF32::IVec2(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
        ],
        (IVec3, LpsValueF32::IVec3(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
        ],
        (IVec4, LpsValueF32::IVec4(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
            JsValue::from_f64(a[3] as f64),
        ],
        (UVec2, LpsValueF32::UVec2(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
        ],
        (UVec3, LpsValueF32::UVec3(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
        ],
        (UVec4, LpsValueF32::UVec4(a)) => vec![
            JsValue::from_f64(a[0] as f64),
            JsValue::from_f64(a[1] as f64),
            JsValue::from_f64(a[2] as f64),
            JsValue::from_f64(a[3] as f64),
        ],
        (BVec2, LpsValueF32::BVec2(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
        ],
        (BVec3, LpsValueF32::BVec3(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[2] { 1.0 } else { 0.0 }),
        ],
        (BVec4, LpsValueF32::BVec4(a)) => vec![
            JsValue::from_f64(if a[0] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[1] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[2] { 1.0 } else { 0.0 }),
            JsValue::from_f64(if a[3] { 1.0 } else { 0.0 }),
        ],
        (Mat2, LpsValueF32::Mat2x2(m)) => vec![
            encode_f32_js(m[0][0], fm),
            encode_f32_js(m[0][1], fm),
            encode_f32_js(m[1][0], fm),
            encode_f32_js(m[1][1], fm),
        ],
        (Mat3, LpsValueF32::Mat3x3(m)) => {
            let mut out = Vec::with_capacity(9);
            for col in m.iter() {
                for x in col.iter() {
                    out.push(encode_f32_js(*x, fm));
                }
            }
            out
        }
        (Mat4, LpsValueF32::Mat4x4(m)) => {
            let mut out = Vec::with_capacity(16);
            for col in m.iter() {
                for x in col.iter() {
                    out.push(encode_f32_js(*x, fm));
                }
            }
            out
        }
        (Array { element, len }, LpsValueF32::Array(items)) => {
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
        (Struct { members, .. }, LpsValueF32::Struct { fields, .. }) => {
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

/// Scalar / vector parameters only (no aggregate pointer ABI).
pub(crate) fn build_js_args_scalar_only(
    param_types: &[LpsType],
    export_param_slots: usize,
    args: &[LpsValueF32],
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

/// Scalar / vector parameters only (no aggregate pointer ABI).
pub(crate) fn build_js_args_q32_scalar_only(
    param_types: &[LpsType],
    export_param_slots: usize,
    words: &[i32],
) -> Result<Array, WasmError> {
    let arr = Array::new();
    arr.push(&JsValue::from_f64(0.0));
    let mut woff = 0;
    for ty in param_types {
        let n = glsl_component_count(ty);
        if woff + n > words.len() {
            return Err(WasmError::runtime(format!(
                "not enough Q32 argument words for browser call at offset {woff}"
            )));
        }
        for i in 0..n {
            arr.push(&JsValue::from_f64(words[woff + i] as f64));
        }
        woff += n;
    }
    if woff != words.len() {
        return Err(WasmError::runtime(format!(
            "extra Q32 argument words: used {woff}, got {}",
            words.len()
        )));
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

fn collect_js_q32_words(
    ty: &LpsType,
    slots: &[JsValue],
    fm: FloatMode,
    off: &mut usize,
    out: &mut Vec<i32>,
) -> Result<(), WasmError> {
    use LpsType::*;
    if fm != FloatMode::Q32 {
        return Err(WasmError::runtime(
            "collect_js_q32_words requires FloatMode::Q32",
        ));
    }
    match ty {
        Void => Ok(()),
        Float | Int | UInt | Bool => {
            out.push(js_num_as_i32(&slots[*off])?);
            *off += 1;
            Ok(())
        }
        Vec2 | IVec2 | UVec2 | BVec2 => {
            for _ in 0..2 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Vec3 | IVec3 | UVec3 | BVec3 => {
            for _ in 0..3 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Vec4 | IVec4 | UVec4 | BVec4 => {
            for _ in 0..4 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Mat2 => {
            for _ in 0..4 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Mat3 => {
            for _ in 0..9 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Mat4 => {
            for _ in 0..16 {
                out.push(js_num_as_i32(&slots[*off])?);
                *off += 1;
            }
            Ok(())
        }
        Array { element, len } => {
            for _ in 0..*len {
                collect_js_q32_words(element, slots, fm, off, out)?;
            }
            Ok(())
        }
        Struct { members, .. } => {
            for m in members {
                collect_js_q32_words(&m.ty, slots, fm, off, out)?;
            }
            Ok(())
        }
    }
}

pub(crate) fn js_result_to_q32_words(
    ty: &LpsType,
    result: &JsValue,
    fm: FloatMode,
) -> Result<Vec<i32>, WasmError> {
    let n = glsl_type_to_wasm_components(ty, fm).len();
    let slots = js_result_slots(result, n)?;
    let mut off = 0;
    let mut out = Vec::new();
    collect_js_q32_words(ty, &slots, fm, &mut off, &mut out)?;
    if off != slots.len() {
        return Err(WasmError::runtime(format!(
            "js Q32 return decode used {off} of {} slots",
            slots.len()
        )));
    }
    Ok(out)
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
) -> Result<LpsValueF32, WasmError> {
    let n = glsl_type_to_wasm_components(ty, fm).len();
    let slots = js_result_slots(result, n)?;
    decode_lps_from_js_slots(ty, &slots, fm, 0).map(|(v, _)| v)
}

fn decode_lps_from_js_slots(
    ty: &LpsType,
    slots: &[JsValue],
    fm: FloatMode,
    off: usize,
) -> Result<(LpsValueF32, usize), WasmError> {
    use LpsType::*;
    match ty {
        Void => Err(WasmError::runtime("void type in js_result")),
        Float => Ok((LpsValueF32::F32(js_slot_as_f32(&slots[off], fm)?), 1)),
        Int => Ok((LpsValueF32::I32(js_num_as_i32(&slots[off])?), 1)),
        UInt => Ok((LpsValueF32::U32(js_num_as_i32(&slots[off])? as u32), 1)),
        Bool => Ok((LpsValueF32::Bool(js_num_as_i32(&slots[off])? != 0), 1)),
        Vec2 => Ok((
            LpsValueF32::Vec2([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
            ]),
            2,
        )),
        Vec3 => Ok((
            LpsValueF32::Vec3([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
                js_slot_as_f32(&slots[off + 2], fm)?,
            ]),
            3,
        )),
        Vec4 => Ok((
            LpsValueF32::Vec4([
                js_slot_as_f32(&slots[off], fm)?,
                js_slot_as_f32(&slots[off + 1], fm)?,
                js_slot_as_f32(&slots[off + 2], fm)?,
                js_slot_as_f32(&slots[off + 3], fm)?,
            ]),
            4,
        )),
        IVec2 => Ok((
            LpsValueF32::IVec2([js_num_as_i32(&slots[off])?, js_num_as_i32(&slots[off + 1])?]),
            2,
        )),
        IVec3 => Ok((
            LpsValueF32::IVec3([
                js_num_as_i32(&slots[off])?,
                js_num_as_i32(&slots[off + 1])?,
                js_num_as_i32(&slots[off + 2])?,
            ]),
            3,
        )),
        IVec4 => Ok((
            LpsValueF32::IVec4([
                js_num_as_i32(&slots[off])?,
                js_num_as_i32(&slots[off + 1])?,
                js_num_as_i32(&slots[off + 2])?,
                js_num_as_i32(&slots[off + 3])?,
            ]),
            4,
        )),
        UVec2 => Ok((
            LpsValueF32::UVec2([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
            ]),
            2,
        )),
        UVec3 => Ok((
            LpsValueF32::UVec3([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
                js_num_as_i32(&slots[off + 2])? as u32,
            ]),
            3,
        )),
        UVec4 => Ok((
            LpsValueF32::UVec4([
                js_num_as_i32(&slots[off])? as u32,
                js_num_as_i32(&slots[off + 1])? as u32,
                js_num_as_i32(&slots[off + 2])? as u32,
                js_num_as_i32(&slots[off + 3])? as u32,
            ]),
            4,
        )),
        BVec2 => Ok((
            LpsValueF32::BVec2([
                js_num_as_i32(&slots[off])? != 0,
                js_num_as_i32(&slots[off + 1])? != 0,
            ]),
            2,
        )),
        BVec3 => Ok((
            LpsValueF32::BVec3([
                js_num_as_i32(&slots[off])? != 0,
                js_num_as_i32(&slots[off + 1])? != 0,
                js_num_as_i32(&slots[off + 2])? != 0,
            ]),
            3,
        )),
        BVec4 => Ok((
            LpsValueF32::BVec4([
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
            Ok((LpsValueF32::Mat2x2([col0, col1]), 4))
        }
        Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] = js_slot_as_f32(&slots[off + col * 3 + row], fm)?;
                }
            }
            Ok((LpsValueF32::Mat3x3(m), 9))
        }
        Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                for row in 0..4 {
                    m[col][row] = js_slot_as_f32(&slots[off + col * 4 + row], fm)?;
                }
            }
            Ok((LpsValueF32::Mat4x4(m), 16))
        }
        Array { element, len } => {
            let mut elems = Vec::with_capacity(*len as usize);
            let mut o = off;
            for _ in 0..*len {
                let (v, n) = decode_lps_from_js_slots(element, slots, fm, o)?;
                o += n;
                elems.push(v);
            }
            Ok((LpsValueF32::Array(elems.into_boxed_slice()), o - off))
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
                LpsValueF32::Struct {
                    name: name.clone(),
                    fields,
                },
                o - off,
            ))
        }
    }
}

// --- Shadow stack + aggregate / sret (browser) ------------------------------------------

pub(crate) struct BrowserShadowFrame {
    pub(crate) saved_sp: i32,
}

pub(crate) struct BrowserSretPlan {
    pub(crate) ptr: i32,
    pub(crate) size: usize,
}

pub(crate) fn browser_shadow_frame_open(
    exports_obj: &JsValue,
) -> Result<BrowserShadowFrame, WasmError> {
    let global = Reflect::get(exports_obj, &JsValue::from_str(SHADOW_STACK_GLOBAL_EXPORT))
        .map_err(|e| WasmError::runtime(format!("shadow global: {e:?}")))?;
    let val = Reflect::get(&global, &JsValue::from_str("value"))
        .map_err(|e| WasmError::runtime(format!("shadow global.value: {e:?}")))?;
    let cur = val
        .as_f64()
        .ok_or_else(|| WasmError::runtime("shadow sp is not a number"))? as i32;
    Ok(BrowserShadowFrame { saved_sp: cur })
}

pub(crate) fn browser_shadow_frame_close(
    exports_obj: &JsValue,
    frame: BrowserShadowFrame,
) -> Result<(), WasmError> {
    let global = Reflect::get(exports_obj, &JsValue::from_str(SHADOW_STACK_GLOBAL_EXPORT))
        .map_err(|e| WasmError::runtime(format!("shadow global: {e:?}")))?;
    Reflect::set(
        &global,
        &JsValue::from_str("value"),
        &JsValue::from_f64(f64::from(frame.saved_sp)),
    )
    .map_err(|e| WasmError::runtime(format!("restore shadow sp: {e:?}")))?;
    Ok(())
}

fn browser_shadow_alloc(exports_obj: &JsValue, size: u32, align: u32) -> Result<i32, WasmError> {
    if align == 0 || (align & (align - 1)) != 0 {
        return Err(WasmError::runtime(
            "shadow alloc: align must be a non-zero power of 2",
        ));
    }
    let global = Reflect::get(exports_obj, &JsValue::from_str(SHADOW_STACK_GLOBAL_EXPORT))
        .map_err(|e| WasmError::runtime(format!("shadow global: {e:?}")))?;
    let val = Reflect::get(&global, &JsValue::from_str("value"))
        .map_err(|e| WasmError::runtime(format!("shadow global.value: {e:?}")))?;
    let cur = val
        .as_f64()
        .ok_or_else(|| WasmError::runtime("shadow sp is not a number"))? as i32;
    let mask = i32::try_from(align).map_err(|_| WasmError::runtime("shadow align"))? - 1;
    let size_i =
        i32::try_from(size).map_err(|_| WasmError::runtime("shadow alloc size too large"))?;
    let raw = cur
        .checked_sub(size_i)
        .ok_or_else(|| WasmError::runtime("shadow stack overflow"))?;
    let ptr = raw & !mask;
    Reflect::set(
        &global,
        &JsValue::from_str("value"),
        &JsValue::from_f64(f64::from(ptr)),
    )
    .map_err(|e| WasmError::runtime(format!("set shadow sp: {e:?}")))?;
    Ok(ptr)
}

pub(crate) fn browser_memory_write(
    mem: &WebAssembly::Memory,
    ptr: i32,
    bytes: &[u8],
) -> Result<(), WasmError> {
    let ab: ArrayBuffer = mem
        .buffer()
        .dyn_into()
        .map_err(|_| WasmError::runtime("memory.buffer is not ArrayBuffer"))?;
    let len = ab.byte_length() as usize;
    let base = usize::try_from(ptr).map_err(|_| WasmError::runtime("negative guest pointer"))?;
    let end = base
        .checked_add(bytes.len())
        .ok_or_else(|| WasmError::runtime("guest write overflow"))?;
    if end > len {
        return Err(WasmError::runtime(format!(
            "guest write out of bounds: end {end} len {len}"
        )));
    }
    let view = Uint8Array::new_with_byte_offset_and_length(&ab, ptr as u32, bytes.len() as u32);
    view.copy_from(bytes);
    Ok(())
}

pub(crate) fn browser_memory_read(
    mem: &WebAssembly::Memory,
    ptr: i32,
    len: usize,
) -> Result<Vec<u8>, WasmError> {
    let ab: ArrayBuffer = mem
        .buffer()
        .dyn_into()
        .map_err(|_| WasmError::runtime("memory.buffer is not ArrayBuffer"))?;
    let mem_len = ab.byte_length() as usize;
    let base = usize::try_from(ptr).map_err(|_| WasmError::runtime("negative guest pointer"))?;
    let end = base
        .checked_add(len)
        .ok_or_else(|| WasmError::runtime("guest read overflow"))?;
    if end > mem_len {
        return Err(WasmError::runtime(format!(
            "guest read out of bounds: end {end} len {mem_len}"
        )));
    }
    let view = Uint8Array::new_with_byte_offset_and_length(&ab, ptr as u32, len as u32);
    let mut out = vec![0u8; len];
    view.copy_to(&mut out);
    Ok(out)
}

pub(crate) fn build_js_args_for_call(
    exports_obj: &JsValue,
    mem: &WebAssembly::Memory,
    export: &WasmExport,
    args: &[LpsValueF32],
    fm: FloatMode,
    return_ty: &LpsType,
) -> Result<(Array, Option<BrowserSretPlan>), WasmError> {
    if args.len() != export.param_types.len() {
        return Err(WasmError::runtime(format!(
            "wrong argument count: expected {}, got {}",
            export.param_types.len(),
            args.len()
        )));
    }
    let arr = Array::new();
    arr.push(&JsValue::from_f64(0.0));
    let mut sret = None;
    if export.uses_sret {
        let size_u = type_size(return_ty, LayoutRules::Std430);
        let size =
            u32::try_from(size_u).map_err(|_| WasmError::runtime("sret size exceeds u32"))?;
        let align = u32::try_from(type_alignment(return_ty, LayoutRules::Std430))
            .map_err(|_| WasmError::runtime("sret align exceeds u32"))?;
        let ptr = browser_shadow_alloc(exports_obj, size, align)?;
        sret = Some(BrowserSretPlan { ptr, size: size_u });
        arr.push(&JsValue::from_f64(f64::from(ptr)));
    }
    for (v, ty) in args.iter().zip(export.param_types.iter()) {
        if type_passed_as_aggregate_ptr(ty) {
            let bytes = encode_aggregate_std430_bytes(ty, v, fm)?;
            let size =
                u32::try_from(bytes.len()).map_err(|_| WasmError::runtime("aggregate arg size"))?;
            let align = u32::try_from(type_alignment(ty, LayoutRules::Std430))
                .map_err(|_| WasmError::runtime("aggregate align"))?;
            let ptr = browser_shadow_alloc(exports_obj, size, align)?;
            browser_memory_write(mem, ptr, &bytes)?;
            arr.push(&JsValue::from_f64(f64::from(ptr)));
        } else {
            for slot in glsl_value_to_js_flat(ty, v, fm)? {
                arr.push(&slot);
            }
        }
    }
    if arr.length() as usize != export.params.len() {
        return Err(WasmError::runtime(format!(
            "internal: JS arg count {} != export.params {}",
            arr.length(),
            export.params.len()
        )));
    }
    Ok((arr, sret))
}

pub(crate) fn build_js_args_q32_for_call(
    exports_obj: &JsValue,
    mem: &WebAssembly::Memory,
    export: &WasmExport,
    words: &[i32],
    return_ty: &LpsType,
) -> Result<(Array, Option<BrowserSretPlan>), WasmError> {
    let arr = Array::new();
    arr.push(&JsValue::from_f64(0.0));
    let mut sret = None;
    if export.uses_sret {
        let size_u = type_size(return_ty, LayoutRules::Std430);
        let size =
            u32::try_from(size_u).map_err(|_| WasmError::runtime("sret size exceeds u32"))?;
        let align = u32::try_from(type_alignment(return_ty, LayoutRules::Std430))
            .map_err(|_| WasmError::runtime("sret align exceeds u32"))?;
        let ptr = browser_shadow_alloc(exports_obj, size, align)?;
        sret = Some(BrowserSretPlan { ptr, size: size_u });
        arr.push(&JsValue::from_f64(f64::from(ptr)));
    }
    let mut woff = 0usize;
    for ty in &export.param_types {
        if type_passed_as_aggregate_ptr(ty) {
            let n = glsl_component_count(ty);
            if woff + n > words.len() {
                return Err(WasmError::runtime(format!(
                    "not enough Q32 argument words at offset {woff}"
                )));
            }
            let bytes = aggregate_flat_q32_words_to_std430_bytes(ty, &words[woff..woff + n])?;
            woff += n;
            let size =
                u32::try_from(bytes.len()).map_err(|_| WasmError::runtime("aggregate arg size"))?;
            let align = u32::try_from(type_alignment(ty, LayoutRules::Std430))
                .map_err(|_| WasmError::runtime("aggregate align"))?;
            let ptr = browser_shadow_alloc(exports_obj, size, align)?;
            browser_memory_write(mem, ptr, &bytes)?;
            arr.push(&JsValue::from_f64(f64::from(ptr)));
        } else {
            let n = glsl_component_count(ty);
            if woff + n > words.len() {
                return Err(WasmError::runtime(format!(
                    "not enough Q32 argument words at offset {woff}"
                )));
            }
            for i in 0..n {
                arr.push(&JsValue::from_f64(f64::from(words[woff + i])));
            }
            woff += n;
        }
    }
    if woff != words.len() {
        return Err(WasmError::runtime(format!(
            "extra Q32 argument words: used {woff}, got {}",
            words.len()
        )));
    }
    if arr.length() as usize != export.params.len() {
        return Err(WasmError::runtime(format!(
            "internal: JS arg count {} != export.params {}",
            arr.length(),
            export.params.len()
        )));
    }
    Ok((arr, sret))
}

pub(crate) fn decode_browser_sret_q32_return(
    mem: &WebAssembly::Memory,
    plan: &BrowserSretPlan,
    return_ty: &LpsType,
) -> Result<Vec<i32>, WasmError> {
    let bytes = browser_memory_read(mem, plan.ptr, plan.size)?;
    q32_sret_bytes_to_flat_return_words(return_ty, &bytes)
}
