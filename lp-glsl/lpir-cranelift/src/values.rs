//! Typed GLSL values for Level-1 JIT calls (Q32 interchange uses `f64` for floats).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use lpir::{GlslParamMeta, GlslParamQualifier, GlslType};

/// Q32 host-side value (floats as `f64` before fixed-point encode).
#[derive(Clone, Debug, PartialEq)]
pub enum GlslQ32 {
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

pub(crate) fn glsl_component_count(ty: &GlslType) -> usize {
    match ty {
        GlslType::Void => 0,
        GlslType::Float | GlslType::Int | GlslType::UInt | GlslType::Bool => 1,
        GlslType::Vec2 | GlslType::IVec2 | GlslType::UVec2 | GlslType::BVec2 => 2,
        GlslType::Vec3 | GlslType::IVec3 | GlslType::UVec3 | GlslType::BVec3 => 3,
        GlslType::Vec4 | GlslType::IVec4 | GlslType::UVec4 | GlslType::BVec4 => 4,
        GlslType::Mat2 => 4,
        GlslType::Mat3 => 9,
        GlslType::Mat4 => 16,
        GlslType::Array { element, len } => {
            glsl_component_count(element).saturating_mul(*len as usize)
        }
    }
}

pub(crate) fn flatten_q32_arg(param: &GlslParamMeta, arg: &GlslQ32) -> Result<Vec<i32>, CallError> {
    if param.qualifier != GlslParamQualifier::In {
        return Err(CallError::Unsupported(String::from(
            "out/inout parameters are not supported by Level-1 call() yet",
        )));
    }
    match (&param.ty, arg) {
        (GlslType::Float, GlslQ32::Float(x)) => Ok(alloc::vec![crate::q32::q32_encode_f64(*x)]),
        (GlslType::Int, GlslQ32::Int(x)) => Ok(alloc::vec![*x]),
        (GlslType::UInt, GlslQ32::UInt(x)) => Ok(alloc::vec![*x as i32]),
        (GlslType::Bool, GlslQ32::Bool(b)) => Ok(alloc::vec![if *b { 1 } else { 0 }]),

        (GlslType::Vec2, GlslQ32::Vec2(a, b)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
        ]),
        (GlslType::Vec3, GlslQ32::Vec3(a, b, c)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
            crate::q32::q32_encode_f64(*c),
        ]),
        (GlslType::Vec4, GlslQ32::Vec4(a, b, c, d)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
            crate::q32::q32_encode_f64(*c),
            crate::q32::q32_encode_f64(*d),
        ]),

        (GlslType::IVec2, GlslQ32::IVec2(a, b)) => Ok(alloc::vec![*a, *b]),
        (GlslType::IVec3, GlslQ32::IVec3(a, b, c)) => Ok(alloc::vec![*a, *b, *c]),
        (GlslType::IVec4, GlslQ32::IVec4(a, b, c, d)) => Ok(alloc::vec![*a, *b, *c, *d]),

        (GlslType::UVec2, GlslQ32::UVec2(a, b)) => Ok(alloc::vec![*a as i32, *b as i32]),
        (GlslType::UVec3, GlslQ32::UVec3(a, b, c)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32])
        }
        (GlslType::UVec4, GlslQ32::UVec4(a, b, c, d)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32, *d as i32,])
        }

        (GlslType::BVec2, GlslQ32::BVec2(a, b)) => {
            Ok(alloc::vec![if *a { 1 } else { 0 }, if *b { 1 } else { 0 },])
        }
        (GlslType::BVec3, GlslQ32::BVec3(a, b, c)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
        ]),
        (GlslType::BVec4, GlslQ32::BVec4(a, b, c, d)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
            if *d { 1 } else { 0 },
        ]),

        (GlslType::Mat2, GlslQ32::Mat2(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }
        (GlslType::Mat3, GlslQ32::Mat3(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }
        (GlslType::Mat4, GlslQ32::Mat4(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }

        (GlslType::Array { .. }, _) => Err(CallError::Unsupported(String::from(
            "array arguments are not supported by Level-1 call() yet",
        ))),

        (expected, got) => Err(CallError::TypeMismatch(format!(
            "argument type mismatch: expected {:?}, got {:?}",
            expected,
            got_ty_name(got)
        ))),
    }
}

fn got_ty_name(v: &GlslQ32) -> &'static str {
    match v {
        GlslQ32::Float(_) => "Float",
        GlslQ32::Int(_) => "Int",
        GlslQ32::UInt(_) => "UInt",
        GlslQ32::Bool(_) => "Bool",
        GlslQ32::Vec2(..) => "Vec2",
        GlslQ32::Vec3(..) => "Vec3",
        GlslQ32::Vec4(..) => "Vec4",
        GlslQ32::IVec2(..) => "IVec2",
        GlslQ32::IVec3(..) => "IVec3",
        GlslQ32::IVec4(..) => "IVec4",
        GlslQ32::UVec2(..) => "UVec2",
        GlslQ32::UVec3(..) => "UVec3",
        GlslQ32::UVec4(..) => "UVec4",
        GlslQ32::BVec2(..) => "BVec2",
        GlslQ32::BVec3(..) => "BVec3",
        GlslQ32::BVec4(..) => "BVec4",
        GlslQ32::Mat2(_) => "Mat2",
        GlslQ32::Mat3(_) => "Mat3",
        GlslQ32::Mat4(_) => "Mat4",
    }
}

pub(crate) fn decode_q32_return(ty: &GlslType, words: &[i32]) -> Result<GlslQ32, CallError> {
    let n = glsl_component_count(ty);
    if words.len() < n {
        return Err(CallError::Unsupported(format!(
            "not enough return values: need {n}, got {}",
            words.len()
        )));
    }
    Ok(match ty {
        GlslType::Void => {
            return Err(CallError::Unsupported(String::from(
                "decode_q32_return called for void",
            )));
        }
        GlslType::Float => GlslQ32::Float(crate::q32::q32_to_f64(words[0])),
        GlslType::Int => GlslQ32::Int(words[0]),
        GlslType::UInt => GlslQ32::UInt(words[0] as u32),
        GlslType::Bool => GlslQ32::Bool(words[0] != 0),
        GlslType::Vec2 => GlslQ32::Vec2(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
        ),
        GlslType::Vec3 => GlslQ32::Vec3(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
        ),
        GlslType::Vec4 => GlslQ32::Vec4(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
        ),
        GlslType::IVec2 => GlslQ32::IVec2(words[0], words[1]),
        GlslType::IVec3 => GlslQ32::IVec3(words[0], words[1], words[2]),
        GlslType::IVec4 => GlslQ32::IVec4(words[0], words[1], words[2], words[3]),
        GlslType::UVec2 => GlslQ32::UVec2(words[0] as u32, words[1] as u32),
        GlslType::UVec3 => GlslQ32::UVec3(words[0] as u32, words[1] as u32, words[2] as u32),
        GlslType::UVec4 => GlslQ32::UVec4(
            words[0] as u32,
            words[1] as u32,
            words[2] as u32,
            words[3] as u32,
        ),
        GlslType::BVec2 => GlslQ32::BVec2(words[0] != 0, words[1] != 0),
        GlslType::BVec3 => GlslQ32::BVec3(words[0] != 0, words[1] != 0, words[2] != 0),
        GlslType::BVec4 => {
            GlslQ32::BVec4(words[0] != 0, words[1] != 0, words[2] != 0, words[3] != 0)
        }
        GlslType::Mat2 => GlslQ32::Mat2([
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
        ]),
        GlslType::Mat3 => GlslQ32::Mat3([
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
            crate::q32::q32_to_f64(words[4]),
            crate::q32::q32_to_f64(words[5]),
            crate::q32::q32_to_f64(words[6]),
            crate::q32::q32_to_f64(words[7]),
            crate::q32::q32_to_f64(words[8]),
        ]),
        GlslType::Mat4 => GlslQ32::Mat4([
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
            crate::q32::q32_to_f64(words[4]),
            crate::q32::q32_to_f64(words[5]),
            crate::q32::q32_to_f64(words[6]),
            crate::q32::q32_to_f64(words[7]),
            crate::q32::q32_to_f64(words[8]),
            crate::q32::q32_to_f64(words[9]),
            crate::q32::q32_to_f64(words[10]),
            crate::q32::q32_to_f64(words[11]),
            crate::q32::q32_to_f64(words[12]),
            crate::q32::q32_to_f64(words[13]),
            crate::q32::q32_to_f64(words[14]),
            crate::q32::q32_to_f64(words[15]),
        ]),
        GlslType::Array { .. } => {
            return Err(CallError::Unsupported(String::from(
                "array return values are not supported by Level-1 decode yet",
            )));
        }
    })
}
