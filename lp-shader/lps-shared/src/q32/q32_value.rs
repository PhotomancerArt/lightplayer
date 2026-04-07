//! Typed GLSL values for Level-1 JIT calls (Q32 interchange uses `f64` for floats).

use crate::{FnParam, LpsType, ParamQualifier};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use lps_q32::q32_encode::{q32_encode_f64, q32_to_f64};

/// Q32 host-side value (floats as `f64` before fixed-point encode).
#[derive(Clone, Debug, PartialEq)]
pub enum Q32ShaderValue {
    Float(f64),
    Int(i32),
    UInt(u32),
    Bool(bool),
    Vec2(f64, f64),
    Vec3(f64, f64, f64),
    Vec4(f64, f64, f64, f64),
    IVec2(i32, i32),
    IVec3(i32, i32, i32),
    IVec4(i32, i32, i32, i32),
    UVec2(u32, u32),
    UVec3(u32, u32, u32),
    UVec4(u32, u32, u32, u32),
    BVec2(bool, bool),
    BVec3(bool, bool, bool),
    BVec4(bool, bool, bool, bool),
    /// Column-major `mat2` components (4 floats).
    Mat2([f64; 4]),
    Mat3([f64; 9]),
    Mat4([f64; 16]),
    /// Fixed-size array; ABI matches flattened element scalars in order.
    Array(Vec<Q32ShaderValue>),
    /// Struct; members in declaration order (flattened ABI not used until JIT supports structs).
    Struct(Vec<Q32ShaderValue>),
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

#[cfg(feature = "std")]
impl std::error::Error for CallError {}

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

pub fn flatten_q32_arg(param: &FnParam, arg: &Q32ShaderValue) -> Result<Vec<i32>, CallError> {
    if param.qualifier != ParamQualifier::In {
        return Err(CallError::Unsupported(String::from(
            "out/inout parameters are not supported by Level-1 call() yet",
        )));
    }
    match (&param.ty, arg) {
        (LpsType::Float, Q32ShaderValue::Float(x)) => Ok(alloc::vec![q32_encode_f64(*x)]),
        (LpsType::Int, Q32ShaderValue::Int(x)) => Ok(alloc::vec![*x]),
        (LpsType::UInt, Q32ShaderValue::UInt(x)) => Ok(alloc::vec![*x as i32]),
        (LpsType::Bool, Q32ShaderValue::Bool(b)) => Ok(alloc::vec![if *b { 1 } else { 0 }]),

        (LpsType::Vec2, Q32ShaderValue::Vec2(a, b)) => {
            Ok(alloc::vec![q32_encode_f64(*a), q32_encode_f64(*b),])
        }
        (LpsType::Vec3, Q32ShaderValue::Vec3(a, b, c)) => Ok(alloc::vec![
            q32_encode_f64(*a),
            q32_encode_f64(*b),
            q32_encode_f64(*c),
        ]),
        (LpsType::Vec4, Q32ShaderValue::Vec4(a, b, c, d)) => Ok(alloc::vec![
            q32_encode_f64(*a),
            q32_encode_f64(*b),
            q32_encode_f64(*c),
            q32_encode_f64(*d),
        ]),

        (LpsType::IVec2, Q32ShaderValue::IVec2(a, b)) => Ok(alloc::vec![*a, *b]),
        (LpsType::IVec3, Q32ShaderValue::IVec3(a, b, c)) => Ok(alloc::vec![*a, *b, *c]),
        (LpsType::IVec4, Q32ShaderValue::IVec4(a, b, c, d)) => Ok(alloc::vec![*a, *b, *c, *d]),

        (LpsType::UVec2, Q32ShaderValue::UVec2(a, b)) => Ok(alloc::vec![*a as i32, *b as i32]),
        (LpsType::UVec3, Q32ShaderValue::UVec3(a, b, c)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32])
        }
        (LpsType::UVec4, Q32ShaderValue::UVec4(a, b, c, d)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32, *d as i32,])
        }

        (LpsType::BVec2, Q32ShaderValue::BVec2(a, b)) => {
            Ok(alloc::vec![if *a { 1 } else { 0 }, if *b { 1 } else { 0 },])
        }
        (LpsType::BVec3, Q32ShaderValue::BVec3(a, b, c)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
        ]),
        (LpsType::BVec4, Q32ShaderValue::BVec4(a, b, c, d)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
            if *d { 1 } else { 0 },
        ]),

        (LpsType::Mat2, Q32ShaderValue::Mat2(a)) => {
            Ok(a.iter().map(|x| q32_encode_f64(*x)).collect())
        }
        (LpsType::Mat3, Q32ShaderValue::Mat3(a)) => {
            Ok(a.iter().map(|x| q32_encode_f64(*x)).collect())
        }
        (LpsType::Mat4, Q32ShaderValue::Mat4(a)) => {
            Ok(a.iter().map(|x| q32_encode_f64(*x)).collect())
        }

        (LpsType::Array { element, len }, Q32ShaderValue::Array(items)) => {
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
            for it in items {
                out.extend(flatten_q32_arg(&sub, it)?);
            }
            Ok(out)
        }

        (LpsType::Struct { .. }, _) | (_, Q32ShaderValue::Struct(_)) => {
            Err(CallError::Unsupported(String::from(
                "struct parameters are not supported by Level-1 call() yet",
            )))
        }

        (expected, got) => Err(CallError::TypeMismatch(format!(
            "argument type mismatch: expected {:?}, got {:?}",
            expected,
            got_ty_name(got)
        ))),
    }
}

fn got_ty_name(v: &Q32ShaderValue) -> &'static str {
    match v {
        Q32ShaderValue::Float(_) => "Float",
        Q32ShaderValue::Int(_) => "Int",
        Q32ShaderValue::UInt(_) => "UInt",
        Q32ShaderValue::Bool(_) => "Bool",
        Q32ShaderValue::Vec2(..) => "Vec2",
        Q32ShaderValue::Vec3(..) => "Vec3",
        Q32ShaderValue::Vec4(..) => "Vec4",
        Q32ShaderValue::IVec2(..) => "IVec2",
        Q32ShaderValue::IVec3(..) => "IVec3",
        Q32ShaderValue::IVec4(..) => "IVec4",
        Q32ShaderValue::UVec2(..) => "UVec2",
        Q32ShaderValue::UVec3(..) => "UVec3",
        Q32ShaderValue::UVec4(..) => "UVec4",
        Q32ShaderValue::BVec2(..) => "BVec2",
        Q32ShaderValue::BVec3(..) => "BVec3",
        Q32ShaderValue::BVec4(..) => "BVec4",
        Q32ShaderValue::Mat2(_) => "Mat2",
        Q32ShaderValue::Mat3(_) => "Mat3",
        Q32ShaderValue::Mat4(_) => "Mat4",
        Q32ShaderValue::Array(_) => "Array",
        Q32ShaderValue::Struct(_) => "Struct",
    }
}

pub fn decode_q32_return(ty: &LpsType, words: &[i32]) -> Result<Q32ShaderValue, CallError> {
    let n = glsl_component_count(ty);
    if words.len() < n {
        return Err(CallError::Unsupported(format!(
            "not enough return values: need {n}, got {}",
            words.len()
        )));
    }
    Ok(match ty {
        LpsType::Struct { .. } => {
            return Err(CallError::Unsupported(String::from(
                "struct returns are not supported by Level-1 call() yet",
            )));
        }
        LpsType::Void => {
            return Err(CallError::Unsupported(String::from(
                "decode_q32_return called for void",
            )));
        }
        LpsType::Float => Q32ShaderValue::Float(q32_to_f64(words[0])),
        LpsType::Int => Q32ShaderValue::Int(words[0]),
        LpsType::UInt => Q32ShaderValue::UInt(words[0] as u32),
        LpsType::Bool => Q32ShaderValue::Bool(words[0] != 0),
        LpsType::Vec2 => Q32ShaderValue::Vec2(q32_to_f64(words[0]), q32_to_f64(words[1])),
        LpsType::Vec3 => Q32ShaderValue::Vec3(
            q32_to_f64(words[0]),
            q32_to_f64(words[1]),
            q32_to_f64(words[2]),
        ),
        LpsType::Vec4 => Q32ShaderValue::Vec4(
            q32_to_f64(words[0]),
            q32_to_f64(words[1]),
            q32_to_f64(words[2]),
            q32_to_f64(words[3]),
        ),
        LpsType::IVec2 => Q32ShaderValue::IVec2(words[0], words[1]),
        LpsType::IVec3 => Q32ShaderValue::IVec3(words[0], words[1], words[2]),
        LpsType::IVec4 => Q32ShaderValue::IVec4(words[0], words[1], words[2], words[3]),
        LpsType::UVec2 => Q32ShaderValue::UVec2(words[0] as u32, words[1] as u32),
        LpsType::UVec3 => Q32ShaderValue::UVec3(words[0] as u32, words[1] as u32, words[2] as u32),
        LpsType::UVec4 => Q32ShaderValue::UVec4(
            words[0] as u32,
            words[1] as u32,
            words[2] as u32,
            words[3] as u32,
        ),
        LpsType::BVec2 => Q32ShaderValue::BVec2(words[0] != 0, words[1] != 0),
        LpsType::BVec3 => Q32ShaderValue::BVec3(words[0] != 0, words[1] != 0, words[2] != 0),
        LpsType::BVec4 => {
            Q32ShaderValue::BVec4(words[0] != 0, words[1] != 0, words[2] != 0, words[3] != 0)
        }
        LpsType::Mat2 => Q32ShaderValue::Mat2([
            q32_to_f64(words[0]),
            q32_to_f64(words[1]),
            q32_to_f64(words[2]),
            q32_to_f64(words[3]),
        ]),
        LpsType::Mat3 => Q32ShaderValue::Mat3([
            q32_to_f64(words[0]),
            q32_to_f64(words[1]),
            q32_to_f64(words[2]),
            q32_to_f64(words[3]),
            q32_to_f64(words[4]),
            q32_to_f64(words[5]),
            q32_to_f64(words[6]),
            q32_to_f64(words[7]),
            q32_to_f64(words[8]),
        ]),
        LpsType::Mat4 => Q32ShaderValue::Mat4([
            q32_to_f64(words[0]),
            q32_to_f64(words[1]),
            q32_to_f64(words[2]),
            q32_to_f64(words[3]),
            q32_to_f64(words[4]),
            q32_to_f64(words[5]),
            q32_to_f64(words[6]),
            q32_to_f64(words[7]),
            q32_to_f64(words[8]),
            q32_to_f64(words[9]),
            q32_to_f64(words[10]),
            q32_to_f64(words[11]),
            q32_to_f64(words[12]),
            q32_to_f64(words[13]),
            q32_to_f64(words[14]),
            q32_to_f64(words[15]),
        ]),
        LpsType::Array { element, len } => {
            let per = glsl_component_count(element);
            let mut elems = Vec::with_capacity(*len as usize);
            for i in 0..(*len as usize) {
                let start = i * per;
                let end = start + per;
                elems.push(decode_q32_return(element, &words[start..end])?);
            }
            Q32ShaderValue::Array(elems)
        }
    })
}
