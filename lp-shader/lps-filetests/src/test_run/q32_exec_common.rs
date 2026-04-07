//! Shared Q32 marshalling for LPIR JIT and RV32 emulator executables.

use std::collections::BTreeMap;

use lps_diagnostics::{ErrorCode, GlslError};
use lps_shared::{
    LpsFnSig, LpsModuleSig, LpsType, LpsValueQ32, lps_value_f32_to_q32, q32_to_lps_value_f32,
};
use lpvm::LpsValueF32;
use lpvm_cranelift::{CallError, GlslReturn};

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

pub(crate) fn args_to_q32(
    gfn: &LpsFnSig,
    args: &[LpsValueF32],
) -> Result<Vec<LpsValueQ32>, GlslError> {
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
        out.push(
            lps_value_f32_to_q32(&p.ty, v)
                .map_err(|e| GlslError::new(ErrorCode::E0400, e.to_string()))?,
        );
    }
    Ok(out)
}

/// Convert [`LpsValueQ32`] to [`LpsValueF32`] using the logical LPIR [`LpsType`].
pub(crate) fn glsl_q32_to_glsl_value(
    ty: &LpsType,
    q: &LpsValueQ32,
) -> Result<LpsValueF32, GlslError> {
    q32_to_lps_value_f32(ty, q.clone()).map_err(|e| GlslError::new(ErrorCode::E0400, e.to_string()))
}

pub(crate) fn map_call_err(e: CallError) -> GlslError {
    GlslError::new(ErrorCode::E0400, e.to_string())
}

/// Run a `call_q32` then map return using the same shape as [`lpvm_cranelift::JitModule::call`].
pub(crate) trait Q32ShaderExecutable {
    fn call_q32_ret(
        &mut self,
        name: &str,
        args: &[LpsValueF32],
    ) -> Result<GlslReturn<LpsValueQ32>, GlslError>;

    fn signatures_map(&self) -> &BTreeMap<String, LpsFnSig>;
}

pub(crate) fn impl_call_void<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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

pub(crate) fn call_f32_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        Some(LpsValueQ32::F32(x)) => Ok(x.to_f32()),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected float return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_i32_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        Some(LpsValueQ32::I32(x)) => Ok(x),
        Some(LpsValueQ32::U32(x)) => Ok(x as i32),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected int return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_bool_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        Some(LpsValueQ32::Bool(b)) => Ok(b),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("expected bool return, got {other:?}"),
        )),
    }
}

pub(crate) fn call_vec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        Some(LpsValueQ32::Vec2(a)) if dim == 2 => Ok(vec![a[0].to_f32(), a[1].to_f32()]),
        Some(LpsValueQ32::Vec3(a)) if dim == 3 => {
            Ok(vec![a[0].to_f32(), a[1].to_f32(), a[2].to_f32()])
        }
        Some(LpsValueQ32::Vec4(a)) if dim == 4 => Ok(vec![
            a[0].to_f32(),
            a[1].to_f32(),
            a[2].to_f32(),
            a[3].to_f32(),
        ]),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected vec return: {other:?}"),
        )),
    }
}

pub(crate) fn call_ivec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        (Some(LpsValueQ32::IVec2(a)), 2) => Ok(vec![a[0], a[1]]),
        (Some(LpsValueQ32::IVec3(a)), 3) => Ok(vec![a[0], a[1], a[2]]),
        (Some(LpsValueQ32::IVec4(a)), 4) => Ok(vec![a[0], a[1], a[2], a[3]]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected ivec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_uvec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        (Some(LpsValueQ32::UVec2(a)), 2) => Ok(vec![a[0], a[1]]),
        (Some(LpsValueQ32::UVec3(a)), 3) => Ok(vec![a[0], a[1], a[2]]),
        (Some(LpsValueQ32::UVec4(a)), 4) => Ok(vec![a[0], a[1], a[2], a[3]]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected uvec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_bvec_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        (Some(LpsValueQ32::BVec2(a)), 2) => Ok(vec![a[0], a[1]]),
        (Some(LpsValueQ32::BVec3(a)), 3) => Ok(vec![a[0], a[1], a[2]]),
        (Some(LpsValueQ32::BVec4(a)), 4) => Ok(vec![a[0], a[1], a[2], a[3]]),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unexpected bvec return: {:?}", ret.value),
        )),
    }
}

pub(crate) fn call_mat_from_q32<E: Q32ShaderExecutable>(
    exec: &mut E,
    name: &str,
    args: &[LpsValueF32],
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
        Some(LpsValueQ32::Mat2x2(m)) if n == 4 => m
            .iter()
            .flat_map(|col| col.iter().map(|x| x.to_f32()))
            .collect(),
        Some(LpsValueQ32::Mat3x3(m)) if n == 9 => m
            .iter()
            .flat_map(|col| col.iter().map(|x| x.to_f32()))
            .collect(),
        Some(LpsValueQ32::Mat4x4(m)) if n == 16 => m
            .iter()
            .flat_map(|col| col.iter().map(|x| x.to_f32()))
            .collect(),
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
    args: &[LpsValueF32],
    elem_ty: &LpsType,
    len: usize,
) -> Result<Vec<LpsValueF32>, GlslError> {
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
        Some(LpsValueQ32::Array(items)) if items.len() == len => items
            .iter()
            .map(|q| glsl_q32_to_glsl_value(&lpir_elem, q))
            .collect(),
        other => Err(GlslError::new(
            ErrorCode::E0400,
            format!("call_array: unexpected return: {other:?}"),
        )),
    }
}
