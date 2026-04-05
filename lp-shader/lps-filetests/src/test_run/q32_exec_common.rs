//! Shared Q32 marshalling for LPIR JIT and RV32 emulator executables.

use std::collections::BTreeMap;

use lpir_cranelift::{CallError, GlslQ32, GlslReturn};
use lps_diagnostics::{ErrorCode, GlslError};
use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};
use lpvm::LpsValue;

pub(crate) fn signatures_from_meta(meta: &LpsModuleSig) -> BTreeMap<String, LpsFnSig> {
    let mut m = BTreeMap::new();
    for g in &meta.functions {
        m.insert(g.name.clone(), g.clone());
    }
    m
}

/// Types supported as array elements when decoding Q32 array returns.
fn core_type_to_lpir_glsl(ty: &LpsType) -> Option<LpsType> {
    match ty {
        LpsType::Struct { .. } => None,
        LpsType::Array { element, len } => Some(LpsType::Array {
            element: Box::new(core_type_to_lpir_glsl(element)?),
            len: *len,
        }),
        _ => Some(ty.clone()),
    }
}

pub(crate) fn args_to_q32(gfn: &LpsFnSig, args: &[LpsValue]) -> Result<Vec<GlslQ32>, GlslError> {
    if gfn.parameters.len() != args.len() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "wrong argument count for '{}': expected {}, got {}",
                gfn.name,
                gfn.parameters.len(),
                args.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(args.len());
    for (p, v) in gfn.parameters.iter().zip(args.iter()) {
        out.push(glsl_value_to_q32(&p.ty, v)?);
    }
    Ok(out)
}

fn glsl_value_to_q32(param_ty: &LpsType, v: &LpsValue) -> Result<GlslQ32, GlslError> {
    use LpsType::*;
    let err = || {
        GlslError::new(
            ErrorCode::E0400,
            format!("argument type mismatch: expected {param_ty:?}, got {v:?}"),
        )
    };
    Ok(match (param_ty, v) {
        (Float, LpsValue::F32(x)) => GlslQ32::Float(*x as f64),
        (Int, LpsValue::I32(x)) => GlslQ32::Int(*x),
        (UInt, LpsValue::U32(x)) => GlslQ32::UInt(*x),
        (Bool, LpsValue::Bool(b)) => GlslQ32::Bool(*b),
        (Vec2, LpsValue::Vec2(a)) => GlslQ32::Vec2(a[0] as f64, a[1] as f64),
        (Vec3, LpsValue::Vec3(a)) => GlslQ32::Vec3(a[0] as f64, a[1] as f64, a[2] as f64),
        (Vec4, LpsValue::Vec4(a)) => {
            GlslQ32::Vec4(a[0] as f64, a[1] as f64, a[2] as f64, a[3] as f64)
        }
        (IVec2, LpsValue::IVec2(a)) => GlslQ32::IVec2(a[0], a[1]),
        (IVec3, LpsValue::IVec3(a)) => GlslQ32::IVec3(a[0], a[1], a[2]),
        (IVec4, LpsValue::IVec4(a)) => GlslQ32::IVec4(a[0], a[1], a[2], a[3]),
        (UVec2, LpsValue::UVec2(a)) => GlslQ32::UVec2(a[0], a[1]),
        (UVec3, LpsValue::UVec3(a)) => GlslQ32::UVec3(a[0], a[1], a[2]),
        (UVec4, LpsValue::UVec4(a)) => GlslQ32::UVec4(a[0], a[1], a[2], a[3]),
        (BVec2, LpsValue::BVec2(a)) => GlslQ32::BVec2(a[0], a[1]),
        (BVec3, LpsValue::BVec3(a)) => GlslQ32::BVec3(a[0], a[1], a[2]),
        (BVec4, LpsValue::BVec4(a)) => GlslQ32::BVec4(a[0], a[1], a[2], a[3]),
        (Mat2, LpsValue::Mat2x2(m)) => GlslQ32::Mat2([
            m[0][0] as f64,
            m[0][1] as f64,
            m[1][0] as f64,
            m[1][1] as f64,
        ]),
        (Mat3, LpsValue::Mat3x3(m)) => GlslQ32::Mat3([
            m[0][0] as f64,
            m[0][1] as f64,
            m[0][2] as f64,
            m[1][0] as f64,
            m[1][1] as f64,
            m[1][2] as f64,
            m[2][0] as f64,
            m[2][1] as f64,
            m[2][2] as f64,
        ]),
        (Mat4, LpsValue::Mat4x4(m)) => GlslQ32::Mat4([
            m[0][0] as f64,
            m[0][1] as f64,
            m[0][2] as f64,
            m[0][3] as f64,
            m[1][0] as f64,
            m[1][1] as f64,
            m[1][2] as f64,
            m[1][3] as f64,
            m[2][0] as f64,
            m[2][1] as f64,
            m[2][2] as f64,
            m[2][3] as f64,
            m[3][0] as f64,
            m[3][1] as f64,
            m[3][2] as f64,
            m[3][3] as f64,
        ]),
        (Array { element, len }, LpsValue::Array(items)) => {
            if items.len() != *len as usize {
                return Err(err());
            }
            let mut q = Vec::with_capacity(items.len());
            for v in items.iter() {
                q.push(glsl_value_to_q32(element.as_ref(), v)?);
            }
            GlslQ32::Array(q)
        }
        (Struct { members, .. }, LpsValue::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err(err());
            }
            let mut q = Vec::with_capacity(fields.len());
            for (m, (_, fv)) in members.iter().zip(fields.iter()) {
                q.push(glsl_value_to_q32(&m.ty, fv)?);
            }
            GlslQ32::Struct(q)
        }
        _ => return Err(err()),
    })
}

/// Convert a [`GlslQ32`] value to [`LpsValue`] using the logical LPIR [`LpsType`].
pub(crate) fn glsl_q32_to_glsl_value(ty: &LpsType, q: &GlslQ32) -> Result<LpsValue, GlslError> {
    use LpsType::*;
    let bad = || {
        GlslError::new(
            ErrorCode::E0400,
            format!("Q32 return shape mismatch: expected {ty:?}, got {q:?}"),
        )
    };
    Ok(match (ty, q) {
        (Float, GlslQ32::Float(x)) => LpsValue::F32(*x as f32),
        (Int, GlslQ32::Int(x)) => LpsValue::I32(*x),
        (UInt, GlslQ32::UInt(x)) => LpsValue::U32(*x),
        (Bool, GlslQ32::Bool(b)) => LpsValue::Bool(*b),
        (Vec2, GlslQ32::Vec2(a, b)) => LpsValue::Vec2([*a as f32, *b as f32]),
        (Vec3, GlslQ32::Vec3(a, b, c)) => LpsValue::Vec3([*a as f32, *b as f32, *c as f32]),
        (Vec4, GlslQ32::Vec4(a, b, c, d)) => {
            LpsValue::Vec4([*a as f32, *b as f32, *c as f32, *d as f32])
        }
        (IVec2, GlslQ32::IVec2(a, b)) => LpsValue::IVec2([*a, *b]),
        (IVec3, GlslQ32::IVec3(a, b, c)) => LpsValue::IVec3([*a, *b, *c]),
        (IVec4, GlslQ32::IVec4(a, b, c, d)) => LpsValue::IVec4([*a, *b, *c, *d]),
        (UVec2, GlslQ32::UVec2(a, b)) => LpsValue::UVec2([*a, *b]),
        (UVec3, GlslQ32::UVec3(a, b, c)) => LpsValue::UVec3([*a, *b, *c]),
        (UVec4, GlslQ32::UVec4(a, b, c, d)) => LpsValue::UVec4([*a, *b, *c, *d]),
        (BVec2, GlslQ32::BVec2(a, b)) => LpsValue::BVec2([*a, *b]),
        (BVec3, GlslQ32::BVec3(a, b, c)) => LpsValue::BVec3([*a, *b, *c]),
        (BVec4, GlslQ32::BVec4(a, b, c, d)) => LpsValue::BVec4([*a, *b, *c, *d]),
        (Mat2, GlslQ32::Mat2(a)) => {
            LpsValue::Mat2x2([[a[0] as f32, a[1] as f32], [a[2] as f32, a[3] as f32]])
        }
        (Mat3, GlslQ32::Mat3(a)) => LpsValue::Mat3x3([
            [a[0] as f32, a[1] as f32, a[2] as f32],
            [a[3] as f32, a[4] as f32, a[5] as f32],
            [a[6] as f32, a[7] as f32, a[8] as f32],
        ]),
        (Mat4, GlslQ32::Mat4(a)) => LpsValue::Mat4x4([
            [a[0] as f32, a[1] as f32, a[2] as f32, a[3] as f32],
            [a[4] as f32, a[5] as f32, a[6] as f32, a[7] as f32],
            [a[8] as f32, a[9] as f32, a[10] as f32, a[11] as f32],
            [a[12] as f32, a[13] as f32, a[14] as f32, a[15] as f32],
        ]),
        (Array { element, len }, GlslQ32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(bad());
            }
            let mut v = Vec::with_capacity(items.len());
            for it in items {
                v.push(glsl_q32_to_glsl_value(element, it)?);
            }
            LpsValue::Array(v.into_boxed_slice())
        }
        (Struct { name, members }, GlslQ32::Struct(items)) => {
            if items.len() != members.len() {
                return Err(bad());
            }
            let mut fields = Vec::with_capacity(members.len());
            for (i, m) in members.iter().enumerate() {
                let key = m.name.clone().unwrap_or_else(|| format!("_{i}"));
                fields.push((key, glsl_q32_to_glsl_value(&m.ty, &items[i])?));
            }
            LpsValue::Struct {
                name: name.clone(),
                fields,
            }
        }
        _ => return Err(bad()),
    })
}

pub(crate) fn map_call_err(e: CallError) -> GlslError {
    GlslError::new(ErrorCode::E0400, e.to_string())
}

/// Run a `call_q32` then map return using the same shape as [`lpir_cranelift::JitModule::call`].
pub(crate) trait Q32ShaderExecutable {
    fn call_q32_ret(
        &mut self,
        name: &str,
        args: &[LpsValue],
    ) -> Result<GlslReturn<GlslQ32>, GlslError>;

    fn signatures_map(&self) -> &BTreeMap<String, LpsFnSig>;
}

pub(crate) fn impl_call_void<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
) -> Result<(), GlslError> {
    let gfn = find_gfn(exec.signatures_map(), name)?;
    if gfn.return_type != LpsType::Void {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_void: '{name}' does not return void"),
        ));
    }
    exec.call_q32_ret(name, args)?;
    Ok(())
}

fn find_gfn<'a>(
    sigs: &'a BTreeMap<String, LpsFnSig>,
    name: &str,
) -> Result<&'a LpsFnSig, GlslError> {
    sigs.get(name)
        .ok_or_else(|| GlslError::new(ErrorCode::E0101, format!("function '{name}' not found")))
}

// GlslFunctionMeta lookup — signatures map has same names as meta; we need return type from signature
pub(crate) fn call_f32_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
) -> Result<f32, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    if sig.return_type != LpsType::Float {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_f32: '{name}' does not return float"),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match ret.value {
        Some(GlslQ32::Float(x)) => Ok(x as f32),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected float return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_i32_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
) -> Result<i32, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    match &sig.return_type {
        LpsType::Int | LpsType::UInt => {}
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("call_i32: '{name}' does not return int/uint"),
            ));
        }
    }
    let ret = exec.call_q32_ret(name, args)?;
    match ret.value {
        Some(GlslQ32::Int(x)) => Ok(x),
        Some(GlslQ32::UInt(x)) => Ok(x as i32),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected int return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_bool_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
) -> Result<bool, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    if sig.return_type != LpsType::Bool {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_bool: '{name}' does not return bool"),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match ret.value {
        Some(GlslQ32::Bool(b)) => Ok(b),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected bool return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_vec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    dim: usize,
) -> Result<Vec<f32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (LpsType::Vec2, 2) | (LpsType::Vec3, 3) | (LpsType::Vec4, 4)
    );
    if !ok {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "call_vec: function '{name}' returns {:?}, expected vec{dim}",
                sig.return_type
            ),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match ret.value {
        Some(GlslQ32::Vec2(a, b)) if dim == 2 => Ok(vec![a as f32, b as f32]),
        Some(GlslQ32::Vec3(a, b, c)) if dim == 3 => Ok(vec![a as f32, b as f32, c as f32]),
        Some(GlslQ32::Vec4(a, b, c, d)) if dim == 4 => {
            Ok(vec![a as f32, b as f32, c as f32, d as f32])
        }
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected vec return: {other:?}"),
        )),
    }
}

pub(crate) fn call_ivec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    dim: usize,
) -> Result<Vec<i32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (LpsType::IVec2, 2) | (LpsType::IVec3, 3) | (LpsType::IVec4, 4)
    );
    if !ok {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "call_ivec: function '{name}' returns {:?}, expected ivec{dim}",
                sig.return_type
            ),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match (&ret.value, dim) {
        (Some(GlslQ32::IVec2(a, b)), 2) => Ok(vec![*a, *b]),
        (Some(GlslQ32::IVec3(a, b, c)), 3) => Ok(vec![*a, *b, *c]),
        (Some(GlslQ32::IVec4(a, b, c, d)), 4) => Ok(vec![*a, *b, *c, *d]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected ivec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_uvec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    dim: usize,
) -> Result<Vec<u32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (LpsType::UVec2, 2) | (LpsType::UVec3, 3) | (LpsType::UVec4, 4)
    );
    if !ok {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "call_uvec: function '{name}' returns {:?}, expected uvec{dim}",
                sig.return_type
            ),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match (&ret.value, dim) {
        (Some(GlslQ32::UVec2(a, b)), 2) => Ok(vec![*a, *b]),
        (Some(GlslQ32::UVec3(a, b, c)), 3) => Ok(vec![*a, *b, *c]),
        (Some(GlslQ32::UVec4(a, b, c, d)), 4) => Ok(vec![*a, *b, *c, *d]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected uvec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_bvec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    dim: usize,
) -> Result<Vec<bool>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (LpsType::BVec2, 2) | (LpsType::BVec3, 3) | (LpsType::BVec4, 4)
    );
    if !ok {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "call_bvec: function '{name}' returns {:?}, expected bvec{dim}",
                sig.return_type
            ),
        ));
    }
    let ret = exec.call_q32_ret(name, args)?;
    match (&ret.value, dim) {
        (Some(GlslQ32::BVec2(a, b)), 2) => Ok(vec![*a, *b]),
        (Some(GlslQ32::BVec3(a, b, c)), 3) => Ok(vec![*a, *b, *c]),
        (Some(GlslQ32::BVec4(a, b, c, d)), 4) => Ok(vec![*a, *b, *c, *d]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected bvec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_mat_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    rows: usize,
    cols: usize,
) -> Result<Vec<f32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, rows, cols),
        (LpsType::Mat2, 2, 2) | (LpsType::Mat3, 3, 3) | (LpsType::Mat4, 4, 4)
    );
    if !ok {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "call_mat: function '{}' returns {:?}, expected mat{rows}x{cols}",
                name, sig.return_type
            ),
        ));
    }
    let n = cols * rows;
    let ret = exec.call_q32_ret(name, args)?;
    let flat: Vec<f32> = match &ret.value {
        Some(GlslQ32::Mat2(a)) if n == 4 => a.iter().map(|x| *x as f32).collect(),
        Some(GlslQ32::Mat3(a)) if n == 9 => a.iter().map(|x| *x as f32).collect(),
        Some(GlslQ32::Mat4(a)) if n == 16 => a.iter().map(|x| *x as f32).collect(),
        other => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("unexpected matrix return: {other:?}"),
            ));
        }
    };
    Ok(flat)
}

pub(crate) fn call_array_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValue],
    elem_ty: &LpsType,
    len: usize,
) -> Result<Vec<LpsValue>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    match &sig.return_type {
        LpsType::Array { element, len: n } if **element == *elem_ty && *n as usize == len => {}
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!(
                    "call_array: function '{name}' returns {:?}, expected array of {} × {:?}",
                    sig.return_type, len, elem_ty
                ),
            ));
        }
    }
    let lpir_elem = core_type_to_lpir_glsl(elem_ty).ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0400,
            format!("call_array: unsupported array element type {elem_ty:?}"),
        )
    })?;
    let ret = exec.call_q32_ret(name, args)?;
    match ret.value {
        Some(GlslQ32::Array(items)) if items.len() == len => items
            .iter()
            .map(|q| glsl_q32_to_glsl_value(&lpir_elem, q))
            .collect(),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_array: unexpected return: {other:?}"),
        )),
    }
}
