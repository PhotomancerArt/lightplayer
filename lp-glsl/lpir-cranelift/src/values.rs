//! Typed GLSL values for Level-1 JIT calls (Q32 interchange uses `f64` for floats).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use lpsc_shared::{FnParam, LpsType, ParamQualifier};

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
    /// Fixed-size array; ABI matches flattened element scalars in order.
    Array(Vec<GlslQ32>),
    /// Struct; members in declaration order (flattened ABI not used until JIT supports structs).
    Struct(Vec<GlslQ32>),
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

pub(crate) fn glsl_component_count(ty: &LpsType) -> usize {
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

pub(crate) fn flatten_q32_arg(param: &FnParam, arg: &GlslQ32) -> Result<Vec<i32>, CallError> {
    if param.qualifier != ParamQualifier::In {
        return Err(CallError::Unsupported(String::from(
            "out/inout parameters are not supported by Level-1 call() yet",
        )));
    }
    match (&param.ty, arg) {
        (LpsType::Float, GlslQ32::Float(x)) => Ok(alloc::vec![crate::q32::q32_encode_f64(*x)]),
        (LpsType::Int, GlslQ32::Int(x)) => Ok(alloc::vec![*x]),
        (LpsType::UInt, GlslQ32::UInt(x)) => Ok(alloc::vec![*x as i32]),
        (LpsType::Bool, GlslQ32::Bool(b)) => Ok(alloc::vec![if *b { 1 } else { 0 }]),

        (LpsType::Vec2, GlslQ32::Vec2(a, b)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
        ]),
        (LpsType::Vec3, GlslQ32::Vec3(a, b, c)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
            crate::q32::q32_encode_f64(*c),
        ]),
        (LpsType::Vec4, GlslQ32::Vec4(a, b, c, d)) => Ok(alloc::vec![
            crate::q32::q32_encode_f64(*a),
            crate::q32::q32_encode_f64(*b),
            crate::q32::q32_encode_f64(*c),
            crate::q32::q32_encode_f64(*d),
        ]),

        (LpsType::IVec2, GlslQ32::IVec2(a, b)) => Ok(alloc::vec![*a, *b]),
        (LpsType::IVec3, GlslQ32::IVec3(a, b, c)) => Ok(alloc::vec![*a, *b, *c]),
        (LpsType::IVec4, GlslQ32::IVec4(a, b, c, d)) => Ok(alloc::vec![*a, *b, *c, *d]),

        (LpsType::UVec2, GlslQ32::UVec2(a, b)) => Ok(alloc::vec![*a as i32, *b as i32]),
        (LpsType::UVec3, GlslQ32::UVec3(a, b, c)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32])
        }
        (LpsType::UVec4, GlslQ32::UVec4(a, b, c, d)) => {
            Ok(alloc::vec![*a as i32, *b as i32, *c as i32, *d as i32,])
        }

        (LpsType::BVec2, GlslQ32::BVec2(a, b)) => {
            Ok(alloc::vec![if *a { 1 } else { 0 }, if *b { 1 } else { 0 },])
        }
        (LpsType::BVec3, GlslQ32::BVec3(a, b, c)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
        ]),
        (LpsType::BVec4, GlslQ32::BVec4(a, b, c, d)) => Ok(alloc::vec![
            if *a { 1 } else { 0 },
            if *b { 1 } else { 0 },
            if *c { 1 } else { 0 },
            if *d { 1 } else { 0 },
        ]),

        (LpsType::Mat2, GlslQ32::Mat2(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }
        (LpsType::Mat3, GlslQ32::Mat3(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }
        (LpsType::Mat4, GlslQ32::Mat4(a)) => {
            Ok(a.iter().map(|x| crate::q32::q32_encode_f64(*x)).collect())
        }

        (LpsType::Array { element, len }, GlslQ32::Array(items)) => {
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

        (LpsType::Struct { .. }, _) | (_, GlslQ32::Struct(_)) => Err(CallError::Unsupported(
            String::from("struct parameters are not supported by Level-1 call() yet"),
        )),

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
        GlslQ32::Array(_) => "Array",
        GlslQ32::Struct(_) => "Struct",
    }
}

pub(crate) fn decode_q32_return(ty: &LpsType, words: &[i32]) -> Result<GlslQ32, CallError> {
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
        LpsType::Float => GlslQ32::Float(crate::q32::q32_to_f64(words[0])),
        LpsType::Int => GlslQ32::Int(words[0]),
        LpsType::UInt => GlslQ32::UInt(words[0] as u32),
        LpsType::Bool => GlslQ32::Bool(words[0] != 0),
        LpsType::Vec2 => GlslQ32::Vec2(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
        ),
        LpsType::Vec3 => GlslQ32::Vec3(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
        ),
        LpsType::Vec4 => GlslQ32::Vec4(
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
        ),
        LpsType::IVec2 => GlslQ32::IVec2(words[0], words[1]),
        LpsType::IVec3 => GlslQ32::IVec3(words[0], words[1], words[2]),
        LpsType::IVec4 => GlslQ32::IVec4(words[0], words[1], words[2], words[3]),
        LpsType::UVec2 => GlslQ32::UVec2(words[0] as u32, words[1] as u32),
        LpsType::UVec3 => GlslQ32::UVec3(words[0] as u32, words[1] as u32, words[2] as u32),
        LpsType::UVec4 => GlslQ32::UVec4(
            words[0] as u32,
            words[1] as u32,
            words[2] as u32,
            words[3] as u32,
        ),
        LpsType::BVec2 => GlslQ32::BVec2(words[0] != 0, words[1] != 0),
        LpsType::BVec3 => GlslQ32::BVec3(words[0] != 0, words[1] != 0, words[2] != 0),
        LpsType::BVec4 => {
            GlslQ32::BVec4(words[0] != 0, words[1] != 0, words[2] != 0, words[3] != 0)
        }
        LpsType::Mat2 => GlslQ32::Mat2([
            crate::q32::q32_to_f64(words[0]),
            crate::q32::q32_to_f64(words[1]),
            crate::q32::q32_to_f64(words[2]),
            crate::q32::q32_to_f64(words[3]),
        ]),
        LpsType::Mat3 => GlslQ32::Mat3([
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
        LpsType::Mat4 => GlslQ32::Mat4([
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
        LpsType::Array { element, len } => {
            let per = glsl_component_count(element);
            let mut elems = Vec::with_capacity(*len as usize);
            for i in 0..(*len as usize) {
                let start = i * per;
                let end = start + per;
                elems.push(decode_q32_return(element, &words[start..end])?);
            }
            GlslQ32::Array(elems)
        }
    })
}
