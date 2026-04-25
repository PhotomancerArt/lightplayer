//! Marshal [`LpsValueF32`] ↔ wasmtime [`Val`] using the same layout as `lps-filetests` WASM runner.

use std::format;

use lpir::FloatMode;
use lps_shared::layout::{type_alignment, type_size};
use lps_shared::{LayoutRules, LpsType};
use lpvm::{LpsValueF32, glsl_component_count};
use wasm_encoder::ValType as WasmValType;
use wasmtime::{Instance, Memory, Store, Val};

use crate::aggregate_abi::{
    aggregate_flat_q32_words_to_std430_bytes, encode_aggregate_std430_bytes,
    q32_sret_bytes_to_flat_return_words, type_passed_as_aggregate_ptr,
};
use crate::error::WasmError;
use crate::module::{SHADOW_STACK_GLOBAL_EXPORT, WasmExport, glsl_type_to_wasm_components};

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
    v: &LpsValueF32,
    fm: FloatMode,
) -> Result<Vec<Val>, WasmError> {
    use LpsType::*;
    Ok(match (ty, v) {
        (Float, LpsValueF32::F32(f)) => vec![encode_f32_wasm(*f, fm)],
        (Int, LpsValueF32::I32(i)) => vec![Val::I32(*i)],
        (UInt, LpsValueF32::U32(u)) => vec![Val::I32(*u as i32)],
        (Bool, LpsValueF32::Bool(b)) => vec![Val::I32(if *b { 1 } else { 0 })],
        (Vec2, LpsValueF32::Vec2(a)) => vec![encode_f32_wasm(a[0], fm), encode_f32_wasm(a[1], fm)],
        (Vec3, LpsValueF32::Vec3(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
        ],
        (Vec4, LpsValueF32::Vec4(a)) => vec![
            encode_f32_wasm(a[0], fm),
            encode_f32_wasm(a[1], fm),
            encode_f32_wasm(a[2], fm),
            encode_f32_wasm(a[3], fm),
        ],
        (IVec2, LpsValueF32::IVec2(a)) => vec![Val::I32(a[0]), Val::I32(a[1])],
        (IVec3, LpsValueF32::IVec3(a)) => vec![Val::I32(a[0]), Val::I32(a[1]), Val::I32(a[2])],
        (IVec4, LpsValueF32::IVec4(a)) => vec![
            Val::I32(a[0]),
            Val::I32(a[1]),
            Val::I32(a[2]),
            Val::I32(a[3]),
        ],
        (UVec2, LpsValueF32::UVec2(a)) => vec![Val::I32(a[0] as i32), Val::I32(a[1] as i32)],
        (UVec3, LpsValueF32::UVec3(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
        ],
        (UVec4, LpsValueF32::UVec4(a)) => vec![
            Val::I32(a[0] as i32),
            Val::I32(a[1] as i32),
            Val::I32(a[2] as i32),
            Val::I32(a[3] as i32),
        ],
        (BVec2, LpsValueF32::BVec2(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
        ],
        (BVec3, LpsValueF32::BVec3(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
        ],
        (BVec4, LpsValueF32::BVec4(a)) => vec![
            Val::I32(if a[0] { 1 } else { 0 }),
            Val::I32(if a[1] { 1 } else { 0 }),
            Val::I32(if a[2] { 1 } else { 0 }),
            Val::I32(if a[3] { 1 } else { 0 }),
        ],
        (Mat2, LpsValueF32::Mat2x2(m)) => vec![
            encode_f32_wasm(m[0][0], fm),
            encode_f32_wasm(m[0][1], fm),
            encode_f32_wasm(m[1][0], fm),
            encode_f32_wasm(m[1][1], fm),
        ],
        (Mat3, LpsValueF32::Mat3x3(m)) => {
            let mut v = Vec::with_capacity(9);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
        }
        (Mat4, LpsValueF32::Mat4x4(m)) => {
            let mut v = Vec::with_capacity(16);
            for col in m.iter() {
                for x in col.iter() {
                    v.push(encode_f32_wasm(*x, fm));
                }
            }
            v
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
                out.extend(glsl_value_to_wasm_flat(element, it, fm)?);
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

/// Build wasmtime [`Val`] arguments from flattened Q32 `i32` lanes (`FloatMode::Q32` only).
///
/// Scalar / vector parameters only (no aggregate pointer ABI).
pub(crate) fn build_wasm_args_q32_scalar_only(
    param_types: &[LpsType],
    export_param_slots: usize,
    words: &[i32],
) -> Result<Vec<Val>, WasmError> {
    let mut wasm_args = Vec::new();
    wasm_args.push(Val::I32(0));
    let mut woff = 0;
    for ty in param_types {
        let n = glsl_component_count(ty);
        if woff + n > words.len() {
            return Err(WasmError::runtime(format!(
                "not enough Q32 argument words: need {} for next parameter, have {} past offset {}",
                n,
                words.len().saturating_sub(woff),
                woff
            )));
        }
        for i in 0..n {
            wasm_args.push(Val::I32(words[woff + i]));
        }
        woff += n;
    }
    if woff != words.len() {
        return Err(WasmError::runtime(format!(
            "extra Q32 argument words: used {}, got {}",
            woff,
            words.len()
        )));
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

fn push_q32_words_from_val_slice(
    ty: &LpsType,
    vals: &[Val],
    fm: FloatMode,
    off: &mut usize,
    out: &mut Vec<i32>,
) -> Result<(), WasmError> {
    use LpsType::*;
    if fm != FloatMode::Q32 {
        return Err(WasmError::runtime(
            "internal: push_q32_words_from_val_slice expects FloatMode::Q32",
        ));
    }
    match ty {
        Void => Ok(()),
        Float | Int | UInt | Bool => match vals.get(*off) {
            Some(Val::I32(i)) => {
                out.push(*i);
                *off += 1;
                Ok(())
            }
            other => Err(WasmError::runtime(format!(
                "expected i32 return slot, got {other:?}"
            ))),
        },
        Vec2 | IVec2 | UVec2 | BVec2 => {
            for _ in 0..2 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Vec3 | IVec3 | UVec3 | BVec3 => {
            for _ in 0..3 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Vec4 | IVec4 | UVec4 | BVec4 => {
            for _ in 0..4 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Mat2 => {
            for _ in 0..4 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Mat3 => {
            for _ in 0..9 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Mat4 => {
            for _ in 0..16 {
                push_scalar_q32_word(vals, off, out)?;
            }
            Ok(())
        }
        Array { element, len } => {
            for _ in 0..*len {
                push_q32_words_from_val_slice(element, vals, fm, off, out)?;
            }
            Ok(())
        }
        Struct { members, .. } => {
            for m in members {
                push_q32_words_from_val_slice(&m.ty, vals, fm, off, out)?;
            }
            Ok(())
        }
    }
}

fn push_scalar_q32_word(
    vals: &[Val],
    off: &mut usize,
    out: &mut Vec<i32>,
) -> Result<(), WasmError> {
    match vals.get(*off) {
        Some(Val::I32(i)) => {
            out.push(*i);
            *off += 1;
            Ok(())
        }
        other => Err(WasmError::runtime(format!(
            "expected i32 wasm return value, got {other:?}"
        ))),
    }
}

/// Collect flattened Q32 `i32` words from wasm results (`FloatMode::Q32`).
pub(crate) fn wasm_vals_to_q32_words(
    ty: &LpsType,
    vals: &[Val],
    fm: FloatMode,
) -> Result<Vec<i32>, WasmError> {
    let mut off = 0;
    let mut out = Vec::new();
    push_q32_words_from_val_slice(ty, vals, fm, &mut off, &mut out)?;
    if off != vals.len() {
        return Err(WasmError::runtime(format!(
            "return decode used {} of {} wasm result values",
            off,
            vals.len()
        )));
    }
    Ok(out)
}

/// Scalar / vector parameters only (no aggregate pointer ABI).
pub(crate) fn build_wasm_args_scalar_only(
    param_types: &[LpsType],
    export_param_slots: usize,
    args: &[LpsValueF32],
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
) -> Result<(LpsValueF32, usize), WasmError> {
    use LpsType::*;
    match ty {
        Void => Err(WasmError::runtime("void type in wasm_vals_to_lps_value")),
        Float => {
            let f = wasm_val_to_f32(&vals[0], fm)?;
            Ok((LpsValueF32::F32(f), 1))
        }
        Int => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValueF32::I32(*i), 1)),
            _ => Err(WasmError::runtime("expected i32 for int return")),
        },
        UInt => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValueF32::U32(*i as u32), 1)),
            _ => Err(WasmError::runtime("expected i32 for uint return")),
        },
        Bool => match vals.first() {
            Some(Val::I32(i)) => Ok((LpsValueF32::Bool(*i != 0), 1)),
            _ => Err(WasmError::runtime("expected i32 for bool return")),
        },
        Vec2 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            Ok((LpsValueF32::Vec2([a, b]), 2))
        }
        Vec3 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            Ok((LpsValueF32::Vec3([a, b, c]), 3))
        }
        Vec4 => {
            let a = wasm_val_to_f32(&vals[0], fm)?;
            let b = wasm_val_to_f32(&vals[1], fm)?;
            let c = wasm_val_to_f32(&vals[2], fm)?;
            let d = wasm_val_to_f32(&vals[3], fm)?;
            Ok((LpsValueF32::Vec4([a, b, c, d]), 4))
        }
        IVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValueF32::IVec2([*a, *b]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for ivec2")),
        },
        IVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => Ok((LpsValueF32::IVec3([*a, *b, *c]), 3)),
            _ => Err(WasmError::runtime("expected i32 triple for ivec3")),
        },
        IVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((LpsValueF32::IVec4([*a, *b, *c, *d]), 4))
            }
            _ => Err(WasmError::runtime("expected four i32 for ivec4")),
        },
        UVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValueF32::UVec2([*a as u32, *b as u32]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for uvec2")),
        },
        UVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((LpsValueF32::UVec3([*a as u32, *b as u32, *c as u32]), 3))
            }
            _ => Err(WasmError::runtime("expected i32 triple for uvec3")),
        },
        UVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => Ok((
                LpsValueF32::UVec4([*a as u32, *b as u32, *c as u32, *d as u32]),
                4,
            )),
            _ => Err(WasmError::runtime("expected four i32 for uvec4")),
        },
        BVec2 => match (&vals[0], &vals[1]) {
            (Val::I32(a), Val::I32(b)) => Ok((LpsValueF32::BVec2([*a != 0, *b != 0]), 2)),
            _ => Err(WasmError::runtime("expected i32 pair for bvec2")),
        },
        BVec3 => match (&vals[0], &vals[1], &vals[2]) {
            (Val::I32(a), Val::I32(b), Val::I32(c)) => {
                Ok((LpsValueF32::BVec3([*a != 0, *b != 0, *c != 0]), 3))
            }
            _ => Err(WasmError::runtime("expected i32 triple for bvec3")),
        },
        BVec4 => match (&vals[0], &vals[1], &vals[2], &vals[3]) {
            (Val::I32(a), Val::I32(b), Val::I32(c), Val::I32(d)) => {
                Ok((LpsValueF32::BVec4([*a != 0, *b != 0, *c != 0, *d != 0]), 4))
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
            Ok((LpsValueF32::Mat2x2([col0, col1]), 4))
        }
        Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 3 + row], fm)?;
                }
            }
            Ok((LpsValueF32::Mat3x3(m), 9))
        }
        Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                for row in 0..4 {
                    m[col][row] = wasm_val_to_f32(&vals[col * 4 + row], fm)?;
                }
            }
            Ok((LpsValueF32::Mat4x4(m), 16))
        }
        Array { element, len } => {
            let mut off = 0;
            let mut elems = Vec::with_capacity(*len as usize);
            for _ in 0..*len {
                let (v, n) = wasm_vals_to_lps_value(element, &vals[off..], fm)?;
                off += n;
                elems.push(v);
            }
            Ok((LpsValueF32::Array(elems.into_boxed_slice()), off))
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
                LpsValueF32::Struct {
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

// --- Shadow stack + aggregate / sret host marshalling ---------------------------------

pub(crate) struct ShadowStackFrame {
    pub(crate) saved_sp: i32,
}

pub(crate) struct SretPlan {
    pub(crate) ptr: i32,
    pub(crate) size: usize,
}

pub(crate) fn shadow_stack_frame_open(
    instance: &Instance,
    store: &mut Store<()>,
) -> Result<ShadowStackFrame, WasmError> {
    let g = instance
        .get_global(&mut *store, SHADOW_STACK_GLOBAL_EXPORT)
        .ok_or_else(|| WasmError::runtime("missing exported shadow stack global"))?;
    let cur = g
        .get(&mut *store)
        .i32()
        .ok_or_else(|| WasmError::runtime("shadow stack global is not i32"))?;
    Ok(ShadowStackFrame { saved_sp: cur })
}

pub(crate) fn shadow_stack_frame_close(
    instance: &Instance,
    store: &mut Store<()>,
    frame: ShadowStackFrame,
) -> Result<(), WasmError> {
    let g = instance
        .get_global(&mut *store, SHADOW_STACK_GLOBAL_EXPORT)
        .ok_or_else(|| WasmError::runtime("missing exported shadow stack global"))?;
    g.set(&mut *store, Val::I32(frame.saved_sp))
        .map_err(|e| WasmError::runtime(format!("restore shadow sp: {e}")))?;
    Ok(())
}

fn shadow_stack_alloc(
    instance: &Instance,
    store: &mut Store<()>,
    size: u32,
    align: u32,
) -> Result<i32, WasmError> {
    if align == 0 || (align & (align - 1)) != 0 {
        return Err(WasmError::runtime(
            "shadow alloc: align must be a non-zero power of 2",
        ));
    }
    let g = instance
        .get_global(&mut *store, SHADOW_STACK_GLOBAL_EXPORT)
        .ok_or_else(|| WasmError::runtime("missing exported shadow stack global"))?;
    let cur = g
        .get(&mut *store)
        .i32()
        .ok_or_else(|| WasmError::runtime("shadow stack global is not i32"))?;
    let mask = i32::try_from(align).map_err(|_| WasmError::runtime("shadow alloc align"))? - 1;
    let size_i =
        i32::try_from(size).map_err(|_| WasmError::runtime("shadow alloc size too large"))?;
    let raw = cur
        .checked_sub(size_i)
        .ok_or_else(|| WasmError::runtime("shadow stack overflow"))?;
    let ptr = raw & !mask;
    g.set(&mut *store, Val::I32(ptr))
        .map_err(|e| WasmError::runtime(format!("set shadow sp: {e}")))?;
    Ok(ptr)
}

pub(crate) fn wasmtime_memory_write(
    mem: &Memory,
    store: &mut Store<()>,
    ptr: i32,
    bytes: &[u8],
) -> Result<(), WasmError> {
    let base = usize::try_from(ptr).map_err(|_| WasmError::runtime("negative guest pointer"))?;
    mem.write(store, base, bytes)
        .map_err(|e| WasmError::runtime(format!("linear memory write failed: {e}")))?;
    Ok(())
}

pub(crate) fn wasmtime_memory_read(
    mem: &Memory,
    store: &Store<()>,
    ptr: i32,
    len: usize,
) -> Result<Vec<u8>, WasmError> {
    let base = usize::try_from(ptr).map_err(|_| WasmError::runtime("negative guest pointer"))?;
    let end = base
        .checked_add(len)
        .ok_or_else(|| WasmError::runtime("guest read overflow"))?;
    let data = mem
        .data(store)
        .get(base..end)
        .ok_or_else(|| WasmError::runtime("guest read out of bounds"))?;
    Ok(data.to_vec())
}

pub(crate) fn build_wasm_args_for_call(
    instance: &Instance,
    store: &mut Store<()>,
    mem: &Memory,
    export: &WasmExport,
    args: &[LpsValueF32],
    fm: FloatMode,
    return_ty: &LpsType,
) -> Result<(Vec<Val>, Option<SretPlan>), WasmError> {
    if args.len() != export.param_types.len() {
        return Err(WasmError::runtime(format!(
            "wrong argument count: expected {}, got {}",
            export.param_types.len(),
            args.len()
        )));
    }
    let mut wasm_args = vec![Val::I32(0)];
    let mut sret = None;
    if export.uses_sret {
        let size_u = type_size(return_ty, LayoutRules::Std430);
        let size =
            u32::try_from(size_u).map_err(|_| WasmError::runtime("sret size exceeds u32"))?;
        let align = u32::try_from(type_alignment(return_ty, LayoutRules::Std430))
            .map_err(|_| WasmError::runtime("sret align exceeds u32"))?;
        let ptr = shadow_stack_alloc(instance, store, size, align)?;
        sret = Some(SretPlan { ptr, size: size_u });
        wasm_args.push(Val::I32(ptr));
    }
    for (v, ty) in args.iter().zip(export.param_types.iter()) {
        if type_passed_as_aggregate_ptr(ty) {
            let bytes = encode_aggregate_std430_bytes(ty, v, fm)?;
            let size =
                u32::try_from(bytes.len()).map_err(|_| WasmError::runtime("aggregate arg size"))?;
            let align = u32::try_from(type_alignment(ty, LayoutRules::Std430))
                .map_err(|_| WasmError::runtime("aggregate align"))?;
            let ptr = shadow_stack_alloc(instance, store, size, align)?;
            wasmtime_memory_write(mem, store, ptr, &bytes)?;
            wasm_args.push(Val::I32(ptr));
        } else {
            wasm_args.extend(glsl_value_to_wasm_flat(ty, v, fm)?);
        }
    }
    if wasm_args.len() != export.params.len() {
        return Err(WasmError::runtime(format!(
            "internal: wasm arg count {} != export.params {}",
            wasm_args.len(),
            export.params.len()
        )));
    }
    Ok((wasm_args, sret))
}

pub(crate) fn build_wasm_args_q32_for_call(
    instance: &Instance,
    store: &mut Store<()>,
    mem: &Memory,
    export: &WasmExport,
    words: &[i32],
    return_ty: &LpsType,
) -> Result<(Vec<Val>, Option<SretPlan>), WasmError> {
    let mut wasm_args = vec![Val::I32(0)];
    let mut sret = None;
    if export.uses_sret {
        let size_u = type_size(return_ty, LayoutRules::Std430);
        let size =
            u32::try_from(size_u).map_err(|_| WasmError::runtime("sret size exceeds u32"))?;
        let align = u32::try_from(type_alignment(return_ty, LayoutRules::Std430))
            .map_err(|_| WasmError::runtime("sret align exceeds u32"))?;
        let ptr = shadow_stack_alloc(instance, store, size, align)?;
        sret = Some(SretPlan { ptr, size: size_u });
        wasm_args.push(Val::I32(ptr));
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
            let ptr = shadow_stack_alloc(instance, store, size, align)?;
            wasmtime_memory_write(mem, store, ptr, &bytes)?;
            wasm_args.push(Val::I32(ptr));
        } else {
            let n = glsl_component_count(ty);
            if woff + n > words.len() {
                return Err(WasmError::runtime(format!(
                    "not enough Q32 argument words at offset {woff}"
                )));
            }
            for i in 0..n {
                wasm_args.push(Val::I32(words[woff + i]));
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
    if wasm_args.len() != export.params.len() {
        return Err(WasmError::runtime(format!(
            "internal: wasm arg count {} != export.params {}",
            wasm_args.len(),
            export.params.len()
        )));
    }
    Ok((wasm_args, sret))
}

pub(crate) fn decode_sret_q32_return(
    mem: &Memory,
    store: &Store<()>,
    plan: &SretPlan,
    return_ty: &LpsType,
) -> Result<Vec<i32>, WasmError> {
    let bytes = wasmtime_memory_read(mem, store, plan.ptr, plan.size)?;
    q32_sret_bytes_to_flat_return_words(return_ty, &bytes)
}
