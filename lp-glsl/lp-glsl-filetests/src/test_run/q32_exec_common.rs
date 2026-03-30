//! Shared Q32 marshalling for LPIR JIT and RV32 emulator executables.

use std::collections::BTreeMap;

use lp_glsl_core::{FunctionSignature, ParamQualifier, Parameter, Type};
use lp_glsl_diagnostics::{ErrorCode, GlslError};
use lp_glsl_values::GlslValue;
use lpir::{GlslFunctionMeta, GlslParamQualifier, GlslType};
use lpir_cranelift::{CallError, GlslQ32, GlslReturn};

pub(crate) fn signatures_from_meta(
    meta: &lpir::GlslModuleMeta,
) -> BTreeMap<String, FunctionSignature> {
    let mut m = BTreeMap::new();
    for g in &meta.functions {
        m.insert(g.name.clone(), fn_meta_to_signature(g));
    }
    m
}

fn qualifier(q: GlslParamQualifier) -> ParamQualifier {
    match q {
        GlslParamQualifier::In => ParamQualifier::In,
        GlslParamQualifier::Out => ParamQualifier::Out,
        GlslParamQualifier::InOut => ParamQualifier::InOut,
    }
}

fn lpir_glsl_type_to_core(t: &GlslType) -> Type {
    match t {
        GlslType::Void => Type::Void,
        GlslType::Float => Type::Float,
        GlslType::Int => Type::Int,
        GlslType::UInt => Type::UInt,
        GlslType::Bool => Type::Bool,
        GlslType::Vec2 => Type::Vec2,
        GlslType::Vec3 => Type::Vec3,
        GlslType::Vec4 => Type::Vec4,
        GlslType::IVec2 => Type::IVec2,
        GlslType::IVec3 => Type::IVec3,
        GlslType::IVec4 => Type::IVec4,
        GlslType::UVec2 => Type::UVec2,
        GlslType::UVec3 => Type::UVec3,
        GlslType::UVec4 => Type::UVec4,
        GlslType::BVec2 => Type::BVec2,
        GlslType::BVec3 => Type::BVec3,
        GlslType::BVec4 => Type::BVec4,
        GlslType::Mat2 => Type::Mat2,
        GlslType::Mat3 => Type::Mat3,
        GlslType::Mat4 => Type::Mat4,
    }
}

fn fn_meta_to_signature(g: &GlslFunctionMeta) -> FunctionSignature {
    FunctionSignature {
        name: g.name.clone(),
        return_type: lpir_glsl_type_to_core(&g.return_type),
        parameters: g
            .params
            .iter()
            .map(|p| Parameter {
                name: p.name.clone(),
                ty: lpir_glsl_type_to_core(&p.ty),
                qualifier: qualifier(p.qualifier),
            })
            .collect(),
    }
}

pub(crate) fn args_to_q32(
    gfn: &GlslFunctionMeta,
    args: &[GlslValue],
) -> Result<Vec<GlslQ32>, GlslError> {
    if gfn.params.len() != args.len() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "wrong argument count for '{}': expected {}, got {}",
                gfn.name,
                gfn.params.len(),
                args.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(args.len());
    for (p, v) in gfn.params.iter().zip(args.iter()) {
        out.push(glsl_value_to_q32(&p.ty, v)?);
    }
    Ok(out)
}

fn glsl_value_to_q32(param_ty: &GlslType, v: &GlslValue) -> Result<GlslQ32, GlslError> {
    use GlslType::*;
    let err = || {
        GlslError::new(
            ErrorCode::E0400,
            format!("argument type mismatch: expected {param_ty:?}, got {v:?}"),
        )
    };
    Ok(match (param_ty, v) {
        (Float, GlslValue::F32(x)) => GlslQ32::Float(*x as f64),
        (Int, GlslValue::I32(x)) => GlslQ32::Int(*x),
        (UInt, GlslValue::U32(x)) => GlslQ32::UInt(*x),
        (Bool, GlslValue::Bool(b)) => GlslQ32::Bool(*b),
        (Vec2, GlslValue::Vec2(a)) => GlslQ32::Vec2(a[0] as f64, a[1] as f64),
        (Vec3, GlslValue::Vec3(a)) => GlslQ32::Vec3(a[0] as f64, a[1] as f64, a[2] as f64),
        (Vec4, GlslValue::Vec4(a)) => {
            GlslQ32::Vec4(a[0] as f64, a[1] as f64, a[2] as f64, a[3] as f64)
        }
        (IVec2, GlslValue::IVec2(a)) => GlslQ32::IVec2(a[0], a[1]),
        (IVec3, GlslValue::IVec3(a)) => GlslQ32::IVec3(a[0], a[1], a[2]),
        (IVec4, GlslValue::IVec4(a)) => GlslQ32::IVec4(a[0], a[1], a[2], a[3]),
        (UVec2, GlslValue::UVec2(a)) => GlslQ32::UVec2(a[0], a[1]),
        (UVec3, GlslValue::UVec3(a)) => GlslQ32::UVec3(a[0], a[1], a[2]),
        (UVec4, GlslValue::UVec4(a)) => GlslQ32::UVec4(a[0], a[1], a[2], a[3]),
        (BVec2, GlslValue::BVec2(a)) => GlslQ32::BVec2(a[0], a[1]),
        (BVec3, GlslValue::BVec3(a)) => GlslQ32::BVec3(a[0], a[1], a[2]),
        (BVec4, GlslValue::BVec4(a)) => GlslQ32::BVec4(a[0], a[1], a[2], a[3]),
        (Mat2, GlslValue::Mat2x2(m)) => GlslQ32::Mat2([
            m[0][0] as f64,
            m[0][1] as f64,
            m[1][0] as f64,
            m[1][1] as f64,
        ]),
        (Mat3, GlslValue::Mat3x3(m)) => GlslQ32::Mat3([
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
        (Mat4, GlslValue::Mat4x4(m)) => GlslQ32::Mat4([
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
        _ => return Err(err()),
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
        args: &[GlslValue],
    ) -> Result<GlslReturn<GlslQ32>, GlslError>;

    fn signatures_map(&self) -> &BTreeMap<String, FunctionSignature>;
}

pub(crate) fn impl_call_void<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[GlslValue],
) -> Result<(), GlslError> {
    let gfn = find_gfn(exec.signatures_map(), name)?;
    if gfn.return_type != Type::Void {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_void: '{name}' does not return void"),
        ));
    }
    exec.call_q32_ret(name, args)?;
    Ok(())
}

fn find_gfn<'a>(
    sigs: &'a BTreeMap<String, FunctionSignature>,
    name: &str,
) -> Result<&'a FunctionSignature, GlslError> {
    sigs.get(name)
        .ok_or_else(|| GlslError::new(ErrorCode::E0101, format!("function '{name}' not found")))
}

// GlslFunctionMeta lookup — signatures map has same names as meta; we need return type from signature
pub(crate) fn call_f32_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[GlslValue],
) -> Result<f32, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    if sig.return_type != Type::Float {
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
    args: &[GlslValue],
) -> Result<i32, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    match &sig.return_type {
        Type::Int | Type::UInt => {}
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
    args: &[GlslValue],
) -> Result<bool, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    if sig.return_type != Type::Bool {
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
    args: &[GlslValue],
    dim: usize,
) -> Result<Vec<f32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (Type::Vec2, 2) | (Type::Vec3, 3) | (Type::Vec4, 4)
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
    args: &[GlslValue],
    dim: usize,
) -> Result<Vec<i32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (Type::IVec2, 2) | (Type::IVec3, 3) | (Type::IVec4, 4)
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
    args: &[GlslValue],
    dim: usize,
) -> Result<Vec<u32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (Type::UVec2, 2) | (Type::UVec3, 3) | (Type::UVec4, 4)
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
    args: &[GlslValue],
    dim: usize,
) -> Result<Vec<bool>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, dim),
        (Type::BVec2, 2) | (Type::BVec3, 3) | (Type::BVec4, 4)
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
    args: &[GlslValue],
    rows: usize,
    cols: usize,
) -> Result<Vec<f32>, GlslError> {
    let sig = find_gfn(exec.signatures_map(), name)?;
    let ok = matches!(
        (&sig.return_type, rows, cols),
        (Type::Mat2, 2, 2) | (Type::Mat3, 3, 3) | (Type::Mat4, 4, 4)
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
