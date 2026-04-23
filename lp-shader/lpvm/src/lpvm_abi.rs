//! Flattened **dense Q32 lane** words for Level-1 LPVM helpers (`Vec<i32>` scalars / small
//! composites passed to [`crate::LpvmInstance::call_q32`] and friends).
//!
//! Scalar, vector, and matrix types map one-to-one to consecutive `i32` lanes (float lanes use
//! [`Q32::to_fixed`] / [`Q32::from_fixed`]). Arrays use the same **dense** lane order (all lanes of
//! element `0`, then element `1`, …) so the emulator host can pack them into a guest buffer before
//! passing a pointer — see `lp-shader/lpvm-emu/src/host_marshal.rs`.
//!
//! **Aggregate memory layout elsewhere** (slots, uniforms, sret buffers, wasm shadow stack) is
//! [`crate::LpvmDataQ32`] over `lps_shared::layout` std430; per-backend pointer/sret marshalling
//! lives in `lpvm-cranelift`, `lpvm-native`, `lpvm-emu`, and `lpvm-wasm` (M1 pointer ABI).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use lps_q32::Q32;
use lps_shared::{FnParam, LpsType, LpsValueF32, LpsValueQ32, ParamQualifier};

/// Split a single slice of flattened Q32 argument words (per-parameter order) into
/// [`LpsValueQ32`] values matching `params`.
pub fn unflatten_q32_args(
    params: &[FnParam],
    words: &[i32],
) -> Result<Vec<LpsValueQ32>, CallError> {
    let mut out = Vec::with_capacity(params.len());
    let mut off = 0;
    for p in params {
        if p.qualifier != ParamQualifier::In {
            return Err(CallError::Unsupported(traced_msg!(
                "out/inout parameters are not supported by Level-1 call_q32 yet"
            )));
        }
        let n = glsl_component_count(&p.ty);
        if off + n > words.len() {
            return Err(CallError::Unsupported(traced_msg!(
                "not enough Q32 argument words at parameter `{}`: need {}, have {} total",
                p.name,
                off + n,
                words.len()
            )));
        }
        let v = decode_q32_return(&p.ty, &words[off..off + n])?;
        off += n;
        out.push(v);
    }
    if off != words.len() {
        return Err(CallError::Unsupported(traced_msg!(
            "extra Q32 argument words: used {}, got {} total",
            off,
            words.len()
        )));
    }
    Ok(out)
}

/// Flatten a returned [`LpsValueQ32`] to raw `i32` words (same layout as [`decode_q32_return`]).
pub fn flatten_q32_return(ty: &LpsType, v: &LpsValueQ32) -> Result<Vec<i32>, CallError> {
    let p = FnParam {
        name: String::new(),
        ty: ty.clone(),
        qualifier: ParamQualifier::In,
    };
    flatten_q32_arg(&p, v)
}

/// Build flattened Q32 words from marshaled [`LpsValueF32`] arguments using GLSL metadata.
pub fn flat_q32_words_from_f32_args(
    params: &[FnParam],
    args: &[LpsValueF32],
) -> Result<Vec<i32>, CallError> {
    if params.len() != args.len() {
        return Err(CallError::Arity {
            expected: params.len(),
            got: args.len(),
        });
    }
    let mut flat = Vec::new();
    for (p, a) in params.iter().zip(args.iter()) {
        let q = lps_shared::lps_value_f32_to_q32(&p.ty, a)
            .map_err(|e| CallError::TypeMismatch(e.to_string()))?;
        flat.extend(flatten_q32_arg(p, &q)?);
    }
    Ok(flat)
}

/// Result of a shader call: optional returned value plus `out` / `inout` values (future).
#[derive(Clone, Debug, PartialEq)]
pub struct GlslReturn<V> {
    pub value: Option<V>,
    pub outs: Vec<V>,
}

pub type CallResult<T> = Result<T, CallError>;

#[derive(Debug)]
pub enum CallError {
    MissingMetadata(String),
    Arity { expected: usize, got: usize },
    TypeMismatch(String),
    Unsupported(String),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::MissingMetadata(n) => write!(f, "no GLSL metadata for function `{n}`"),
            CallError::Arity { expected, got } => {
                write!(f, "wrong argument count: expected {expected}, got {got}")
            }
            CallError::TypeMismatch(s) | CallError::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

impl core::error::Error for CallError {}

/// Scalar/vector/matrix flattened word count for a logical [`LpsType`].
pub fn glsl_component_count(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
        LpsType::Mat2 => 4,
        LpsType::Mat3 => 9,
        LpsType::Mat4 => 16,
        LpsType::Array { element, len } => {
            glsl_component_count(element).saturating_mul(*len as usize)
        }
        LpsType::Struct { members, .. } => {
            members.iter().map(|m| glsl_component_count(&m.ty)).sum()
        }
    }
}

/// Flatten one function parameter using Q32 lane encoding.
pub fn flatten_q32_arg(param: &FnParam, arg: &LpsValueQ32) -> Result<Vec<i32>, CallError> {
    if param.qualifier != ParamQualifier::In {
        return Err(CallError::Unsupported(traced_msg!(
            "out/inout parameters are not supported by Level-1 call() yet"
        )));
    }
    match (&param.ty, arg) {
        (LpsType::Float, LpsValueQ32::F32(x)) => Ok(alloc::vec![x.to_fixed()]),
        (LpsType::Int, LpsValueQ32::I32(x)) => Ok(alloc::vec![*x]),
        (LpsType::UInt, LpsValueQ32::U32(x)) => Ok(alloc::vec![*x as i32]),
        (LpsType::Bool, LpsValueQ32::Bool(b)) => Ok(alloc::vec![if *b { 1 } else { 0 }]),

        (LpsType::Vec2, LpsValueQ32::Vec2(a)) => Ok(alloc::vec![a[0].to_fixed(), a[1].to_fixed()]),
        (LpsType::Vec3, LpsValueQ32::Vec3(a)) => Ok(alloc::vec![
            a[0].to_fixed(),
            a[1].to_fixed(),
            a[2].to_fixed(),
        ]),
        (LpsType::Vec4, LpsValueQ32::Vec4(a)) => Ok(alloc::vec![
            a[0].to_fixed(),
            a[1].to_fixed(),
            a[2].to_fixed(),
            a[3].to_fixed(),
        ]),

        (LpsType::IVec2, LpsValueQ32::IVec2(a)) => Ok(alloc::vec![a[0], a[1]]),
        (LpsType::IVec3, LpsValueQ32::IVec3(a)) => Ok(alloc::vec![a[0], a[1], a[2]]),
        (LpsType::IVec4, LpsValueQ32::IVec4(a)) => Ok(alloc::vec![a[0], a[1], a[2], a[3]]),

        (LpsType::UVec2, LpsValueQ32::UVec2(a)) => Ok(alloc::vec![a[0] as i32, a[1] as i32]),
        (LpsType::UVec3, LpsValueQ32::UVec3(a)) => {
            Ok(alloc::vec![a[0] as i32, a[1] as i32, a[2] as i32])
        }
        (LpsType::UVec4, LpsValueQ32::UVec4(a)) => Ok(alloc::vec![
            a[0] as i32,
            a[1] as i32,
            a[2] as i32,
            a[3] as i32,
        ]),

        (LpsType::BVec2, LpsValueQ32::BVec2(a)) => Ok(alloc::vec![
            if a[0] { 1 } else { 0 },
            if a[1] { 1 } else { 0 }
        ]),
        (LpsType::BVec3, LpsValueQ32::BVec3(a)) => Ok(alloc::vec![
            if a[0] { 1 } else { 0 },
            if a[1] { 1 } else { 0 },
            if a[2] { 1 } else { 0 },
        ]),
        (LpsType::BVec4, LpsValueQ32::BVec4(a)) => Ok(alloc::vec![
            if a[0] { 1 } else { 0 },
            if a[1] { 1 } else { 0 },
            if a[2] { 1 } else { 0 },
            if a[3] { 1 } else { 0 },
        ]),

        (LpsType::Mat2, LpsValueQ32::Mat2x2(m)) => Ok(alloc::vec![
            m[0][0].to_fixed(),
            m[0][1].to_fixed(),
            m[1][0].to_fixed(),
            m[1][1].to_fixed(),
        ]),
        (LpsType::Mat3, LpsValueQ32::Mat3x3(m)) => {
            let mut w = Vec::with_capacity(9);
            for col in m {
                for x in col {
                    w.push(x.to_fixed());
                }
            }
            Ok(w)
        }
        (LpsType::Mat4, LpsValueQ32::Mat4x4(m)) => {
            let mut w = Vec::with_capacity(16);
            for col in m {
                for x in col {
                    w.push(x.to_fixed());
                }
            }
            Ok(w)
        }

        (LpsType::Array { .. }, LpsValueQ32::Array(_)) => dense_q32_flatten_array(param, arg),

        (LpsType::Struct { .. }, _) | (_, LpsValueQ32::Struct { .. }) => {
            Err(CallError::Unsupported(traced_msg!(
                "struct parameters are not supported by Level-1 call() yet"
            )))
        }

        (expected, got) => Err(CallError::TypeMismatch(format!(
            "argument type mismatch: expected {:?}, got {:?}",
            expected,
            got_ty_name(got)
        ))),
    }
}

fn got_ty_name(v: &LpsValueQ32) -> &'static str {
    match v {
        LpsValueQ32::F32(_) => "F32",
        LpsValueQ32::I32(_) => "I32",
        LpsValueQ32::U32(_) => "U32",
        LpsValueQ32::Bool(_) => "Bool",
        LpsValueQ32::Vec2(_) => "Vec2",
        LpsValueQ32::Vec3(_) => "Vec3",
        LpsValueQ32::Vec4(_) => "Vec4",
        LpsValueQ32::IVec2(_) => "IVec2",
        LpsValueQ32::IVec3(_) => "IVec3",
        LpsValueQ32::IVec4(_) => "IVec4",
        LpsValueQ32::UVec2(_) => "UVec2",
        LpsValueQ32::UVec3(_) => "UVec3",
        LpsValueQ32::UVec4(_) => "UVec4",
        LpsValueQ32::BVec2(_) => "BVec2",
        LpsValueQ32::BVec3(_) => "BVec3",
        LpsValueQ32::BVec4(_) => "BVec4",
        LpsValueQ32::Mat2x2(_) => "Mat2x2",
        LpsValueQ32::Mat3x3(_) => "Mat3x3",
        LpsValueQ32::Mat4x4(_) => "Mat4x4",
        LpsValueQ32::Array(_) => "Array",
        LpsValueQ32::Struct { .. } => "Struct",
    }
}

/// Decode flattened return words into [`LpsValueQ32`].
pub fn decode_q32_return(ty: &LpsType, words: &[i32]) -> Result<LpsValueQ32, CallError> {
    let n = glsl_component_count(ty);
    if words.len() < n {
        return Err(CallError::Unsupported(traced_msg!(
            "not enough return values: need {n}, got {}",
            words.len()
        )));
    }
    Ok(match ty {
        LpsType::Struct { .. } => {
            return Err(CallError::Unsupported(traced_msg!(
                "struct returns are not supported by Level-1 call() yet"
            )));
        }
        LpsType::Void => {
            return Err(CallError::Unsupported(traced_msg!(
                "decode_q32_return called for void"
            )));
        }
        LpsType::Float => LpsValueQ32::F32(Q32::from_fixed(words[0])),
        LpsType::Int => LpsValueQ32::I32(words[0]),
        LpsType::UInt => LpsValueQ32::U32(words[0] as u32),
        LpsType::Bool => LpsValueQ32::Bool(words[0] != 0),
        LpsType::Vec2 => LpsValueQ32::Vec2([Q32::from_fixed(words[0]), Q32::from_fixed(words[1])]),
        LpsType::Vec3 => LpsValueQ32::Vec3([
            Q32::from_fixed(words[0]),
            Q32::from_fixed(words[1]),
            Q32::from_fixed(words[2]),
        ]),
        LpsType::Vec4 => LpsValueQ32::Vec4([
            Q32::from_fixed(words[0]),
            Q32::from_fixed(words[1]),
            Q32::from_fixed(words[2]),
            Q32::from_fixed(words[3]),
        ]),
        LpsType::IVec2 => LpsValueQ32::IVec2([words[0], words[1]]),
        LpsType::IVec3 => LpsValueQ32::IVec3([words[0], words[1], words[2]]),
        LpsType::IVec4 => LpsValueQ32::IVec4([words[0], words[1], words[2], words[3]]),
        LpsType::UVec2 => LpsValueQ32::UVec2([words[0] as u32, words[1] as u32]),
        LpsType::UVec3 => LpsValueQ32::UVec3([words[0] as u32, words[1] as u32, words[2] as u32]),
        LpsType::UVec4 => LpsValueQ32::UVec4([
            words[0] as u32,
            words[1] as u32,
            words[2] as u32,
            words[3] as u32,
        ]),
        LpsType::BVec2 => LpsValueQ32::BVec2([words[0] != 0, words[1] != 0]),
        LpsType::BVec3 => LpsValueQ32::BVec3([words[0] != 0, words[1] != 0, words[2] != 0]),
        LpsType::BVec4 => {
            LpsValueQ32::BVec4([words[0] != 0, words[1] != 0, words[2] != 0, words[3] != 0])
        }
        LpsType::Mat2 => LpsValueQ32::Mat2x2([
            [Q32::from_fixed(words[0]), Q32::from_fixed(words[1])],
            [Q32::from_fixed(words[2]), Q32::from_fixed(words[3])],
        ]),
        LpsType::Mat3 => LpsValueQ32::Mat3x3([
            [
                Q32::from_fixed(words[0]),
                Q32::from_fixed(words[1]),
                Q32::from_fixed(words[2]),
            ],
            [
                Q32::from_fixed(words[3]),
                Q32::from_fixed(words[4]),
                Q32::from_fixed(words[5]),
            ],
            [
                Q32::from_fixed(words[6]),
                Q32::from_fixed(words[7]),
                Q32::from_fixed(words[8]),
            ],
        ]),
        LpsType::Mat4 => LpsValueQ32::Mat4x4([
            [
                Q32::from_fixed(words[0]),
                Q32::from_fixed(words[1]),
                Q32::from_fixed(words[2]),
                Q32::from_fixed(words[3]),
            ],
            [
                Q32::from_fixed(words[4]),
                Q32::from_fixed(words[5]),
                Q32::from_fixed(words[6]),
                Q32::from_fixed(words[7]),
            ],
            [
                Q32::from_fixed(words[8]),
                Q32::from_fixed(words[9]),
                Q32::from_fixed(words[10]),
                Q32::from_fixed(words[11]),
            ],
            [
                Q32::from_fixed(words[12]),
                Q32::from_fixed(words[13]),
                Q32::from_fixed(words[14]),
                Q32::from_fixed(words[15]),
            ],
        ]),
        LpsType::Array { .. } => dense_q32_decode_array(ty, words)?,
    })
}

/// Dense packed lanes for array-typed parameters (see module docs — not padded std430 traversal).
fn dense_q32_flatten_array(param: &FnParam, arg: &LpsValueQ32) -> Result<Vec<i32>, CallError> {
    let (LpsType::Array { element, len }, LpsValueQ32::Array(items)) = (&param.ty, arg) else {
        return Err(CallError::TypeMismatch(format!(
            "argument type mismatch: expected {:?}, got {:?}",
            &param.ty,
            got_ty_name(arg)
        )));
    };
    if items.len() != *len as usize {
        return Err(CallError::TypeMismatch(format!(
            "array argument length mismatch: expected {}, got {}",
            len,
            items.len()
        )));
    }
    let sub = FnParam {
        name: String::new(),
        ty: element.as_ref().clone(),
        qualifier: param.qualifier,
    };
    let mut out = Vec::new();
    for it in items.iter() {
        out.extend(flatten_q32_arg(&sub, it)?);
    }
    Ok(out)
}

fn dense_q32_decode_array(ty: &LpsType, words: &[i32]) -> Result<LpsValueQ32, CallError> {
    let LpsType::Array { element, len } = ty else {
        return Err(CallError::Unsupported(traced_msg!(
            "dense_q32_decode_array: not an array type"
        )));
    };
    let per = glsl_component_count(element);
    let mut elems = Vec::with_capacity(*len as usize);
    for i in 0..(*len as usize) {
        let start = i * per;
        let end = start + per;
        elems.push(decode_q32_return(element, &words[start..end])?);
    }
    Ok(LpsValueQ32::Array(elems.into_boxed_slice()))
}
