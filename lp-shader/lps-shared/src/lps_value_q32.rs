//! Structured GLSL values with [`Q32`] fixed-point for float components.
//!
//! Use this type when you need exact Q32 semantics (same raw words as the VM ABI).
//! For user-level f32 values see [`crate::LpsValueF32`] and [`lps_value_f32_to_q32`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use lps_q32::Q32;
use lps_q32::q32_encode::q32_encode;

use crate::{LpsType, LpsValueF32};

/// Fixed-point semantic values aligned with [`LpsValueF32`] shape.
#[derive(Clone, Debug, PartialEq)]
pub enum LpsValueQ32 {
    I32(i32),
    U32(u32),
    F32(Q32),
    Bool(bool),
    Vec2([Q32; 2]),
    Vec3([Q32; 3]),
    Vec4([Q32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[Q32; 2]; 2]),
    Mat3x3([[Q32; 3]; 3]),
    Mat4x4([[Q32; 4]; 4]),
    Array(Box<[LpsValueQ32]>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueQ32)>,
    },
}

/// Conversion error for [`lps_value_f32_to_q32`] / [`q32_to_lps_value_f32`].
#[derive(Debug)]
pub enum LpsValueQ32Error {
    TypeMismatch(String),
    Unsupported(String),
}

impl fmt::Display for LpsValueQ32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LpsValueQ32Error::TypeMismatch(s) | LpsValueQ32Error::Unsupported(s) => {
                write!(f, "{s}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LpsValueQ32Error {}

fn f32_to_q32_abi(x: f32) -> Q32 {
    Q32::from_fixed(q32_encode(x))
}

/// Convert [`LpsValueF32`] to [`LpsValueQ32`] using [`q32_encode`] for float components
/// so host arguments match compiler constant emission and the historical `f64` path.
pub fn lps_value_f32_to_q32(
    ty: &LpsType,
    v: &LpsValueF32,
) -> Result<LpsValueQ32, LpsValueQ32Error> {
    if matches!(ty, LpsType::Texture2D) {
        return Err(LpsValueQ32Error::Unsupported(String::from(
            "Texture2D is not convertible via lps_value_f32_to_q32; use a typed Texture2D binding helper",
        )));
    }
    Ok(match (ty, v) {
        (LpsType::Float, LpsValueF32::F32(x)) => LpsValueQ32::F32(f32_to_q32_abi(*x)),
        (LpsType::Int, LpsValueF32::I32(x)) => LpsValueQ32::I32(*x),
        (LpsType::UInt, LpsValueF32::U32(x)) => LpsValueQ32::U32(*x),
        (LpsType::Bool, LpsValueF32::Bool(b)) => LpsValueQ32::Bool(*b),

        (LpsType::Vec2, LpsValueF32::Vec2(a)) => {
            LpsValueQ32::Vec2([f32_to_q32_abi(a[0]), f32_to_q32_abi(a[1])])
        }
        (LpsType::Vec3, LpsValueF32::Vec3(a)) => LpsValueQ32::Vec3([
            f32_to_q32_abi(a[0]),
            f32_to_q32_abi(a[1]),
            f32_to_q32_abi(a[2]),
        ]),
        (LpsType::Vec4, LpsValueF32::Vec4(a)) => LpsValueQ32::Vec4([
            f32_to_q32_abi(a[0]),
            f32_to_q32_abi(a[1]),
            f32_to_q32_abi(a[2]),
            f32_to_q32_abi(a[3]),
        ]),

        (LpsType::IVec2, LpsValueF32::IVec2(a)) => LpsValueQ32::IVec2(*a),
        (LpsType::IVec3, LpsValueF32::IVec3(a)) => LpsValueQ32::IVec3(*a),
        (LpsType::IVec4, LpsValueF32::IVec4(a)) => LpsValueQ32::IVec4(*a),

        (LpsType::UVec2, LpsValueF32::UVec2(a)) => LpsValueQ32::UVec2(*a),
        (LpsType::UVec3, LpsValueF32::UVec3(a)) => LpsValueQ32::UVec3(*a),
        (LpsType::UVec4, LpsValueF32::UVec4(a)) => LpsValueQ32::UVec4(*a),

        (LpsType::BVec2, LpsValueF32::BVec2(a)) => LpsValueQ32::BVec2(*a),
        (LpsType::BVec3, LpsValueF32::BVec3(a)) => LpsValueQ32::BVec3(*a),
        (LpsType::BVec4, LpsValueF32::BVec4(a)) => LpsValueQ32::BVec4(*a),

        (LpsType::Mat2, LpsValueF32::Mat2x2(m)) => LpsValueQ32::Mat2x2([
            [f32_to_q32_abi(m[0][0]), f32_to_q32_abi(m[0][1])],
            [f32_to_q32_abi(m[1][0]), f32_to_q32_abi(m[1][1])],
        ]),
        (LpsType::Mat3, LpsValueF32::Mat3x3(m)) => LpsValueQ32::Mat3x3([
            [
                f32_to_q32_abi(m[0][0]),
                f32_to_q32_abi(m[0][1]),
                f32_to_q32_abi(m[0][2]),
            ],
            [
                f32_to_q32_abi(m[1][0]),
                f32_to_q32_abi(m[1][1]),
                f32_to_q32_abi(m[1][2]),
            ],
            [
                f32_to_q32_abi(m[2][0]),
                f32_to_q32_abi(m[2][1]),
                f32_to_q32_abi(m[2][2]),
            ],
        ]),
        (LpsType::Mat4, LpsValueF32::Mat4x4(m)) => LpsValueQ32::Mat4x4([
            [
                f32_to_q32_abi(m[0][0]),
                f32_to_q32_abi(m[0][1]),
                f32_to_q32_abi(m[0][2]),
                f32_to_q32_abi(m[0][3]),
            ],
            [
                f32_to_q32_abi(m[1][0]),
                f32_to_q32_abi(m[1][1]),
                f32_to_q32_abi(m[1][2]),
                f32_to_q32_abi(m[1][3]),
            ],
            [
                f32_to_q32_abi(m[2][0]),
                f32_to_q32_abi(m[2][1]),
                f32_to_q32_abi(m[2][2]),
                f32_to_q32_abi(m[2][3]),
            ],
            [
                f32_to_q32_abi(m[3][0]),
                f32_to_q32_abi(m[3][1]),
                f32_to_q32_abi(m[3][2]),
                f32_to_q32_abi(m[3][3]),
            ],
        ]),

        (LpsType::Array { element, len }, LpsValueF32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(LpsValueQ32Error::TypeMismatch(format!(
                    "array length mismatch: expected {}, got {}",
                    len,
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for it in items.iter() {
                out.push(lps_value_f32_to_q32(element, it)?);
            }
            LpsValueQ32::Array(out.into_boxed_slice())
        }

        (LpsType::Struct { members, .. }, LpsValueF32::Struct { name, fields }) => {
            if members.len() != fields.len() {
                return Err(LpsValueQ32Error::TypeMismatch(format!(
                    "struct field count mismatch: expected {}, got {}",
                    members.len(),
                    fields.len()
                )));
            }
            let mut out = Vec::with_capacity(fields.len());
            for (i, m) in members.iter().enumerate() {
                let key = m.name.clone().unwrap_or_else(|| format!("_{i}"));
                let (fname, fv) = &fields[i];
                if fname != &key {
                    return Err(LpsValueQ32Error::TypeMismatch(format!(
                        "expected field `{key}`, got `{fname}`"
                    )));
                }
                out.push((fname.clone(), lps_value_f32_to_q32(&m.ty, fv)?));
            }
            LpsValueQ32::Struct {
                name: name.clone(),
                fields: out,
            }
        }

        (expected, _got) => {
            return Err(LpsValueQ32Error::TypeMismatch(format!(
                "argument type mismatch: expected {expected:?}, got incompatible LpsValueF32"
            )));
        }
    })
}

/// Convert [`LpsValueQ32`] to [`LpsValueF32`] (`Q32` components become `f32` via [`Q32::to_f32`]).
pub fn q32_to_lps_value_f32(ty: &LpsType, v: LpsValueQ32) -> Result<LpsValueF32, LpsValueQ32Error> {
    if matches!(ty, LpsType::Texture2D) {
        return Err(LpsValueQ32Error::Unsupported(String::from(
            "Texture2D is not convertible via q32_to_lps_value_f32; use a typed Texture2D binding helper",
        )));
    }
    let bad = || LpsValueQ32Error::TypeMismatch(format!("return shape mismatch for type {ty:?}"));

    Ok(match (ty, v) {
        (LpsType::Float, LpsValueQ32::F32(x)) => LpsValueF32::F32(x.to_f32()),
        (LpsType::Int, LpsValueQ32::I32(x)) => LpsValueF32::I32(x),
        (LpsType::UInt, LpsValueQ32::U32(x)) => LpsValueF32::U32(x),
        (LpsType::Bool, LpsValueQ32::Bool(b)) => LpsValueF32::Bool(b),

        (LpsType::Vec2, LpsValueQ32::Vec2(a)) => LpsValueF32::Vec2([a[0].to_f32(), a[1].to_f32()]),
        (LpsType::Vec3, LpsValueQ32::Vec3(a)) => {
            LpsValueF32::Vec3([a[0].to_f32(), a[1].to_f32(), a[2].to_f32()])
        }
        (LpsType::Vec4, LpsValueQ32::Vec4(a)) => {
            LpsValueF32::Vec4([a[0].to_f32(), a[1].to_f32(), a[2].to_f32(), a[3].to_f32()])
        }

        (LpsType::IVec2, LpsValueQ32::IVec2(a)) => LpsValueF32::IVec2(a),
        (LpsType::IVec3, LpsValueQ32::IVec3(a)) => LpsValueF32::IVec3(a),
        (LpsType::IVec4, LpsValueQ32::IVec4(a)) => LpsValueF32::IVec4(a),

        (LpsType::UVec2, LpsValueQ32::UVec2(a)) => LpsValueF32::UVec2(a),
        (LpsType::UVec3, LpsValueQ32::UVec3(a)) => LpsValueF32::UVec3(a),
        (LpsType::UVec4, LpsValueQ32::UVec4(a)) => LpsValueF32::UVec4(a),

        (LpsType::BVec2, LpsValueQ32::BVec2(a)) => LpsValueF32::BVec2(a),
        (LpsType::BVec3, LpsValueQ32::BVec3(a)) => LpsValueF32::BVec3(a),
        (LpsType::BVec4, LpsValueQ32::BVec4(a)) => LpsValueF32::BVec4(a),

        (LpsType::Mat2, LpsValueQ32::Mat2x2(m)) => LpsValueF32::Mat2x2([
            [m[0][0].to_f32(), m[0][1].to_f32()],
            [m[1][0].to_f32(), m[1][1].to_f32()],
        ]),
        (LpsType::Mat3, LpsValueQ32::Mat3x3(m)) => LpsValueF32::Mat3x3([
            [m[0][0].to_f32(), m[0][1].to_f32(), m[0][2].to_f32()],
            [m[1][0].to_f32(), m[1][1].to_f32(), m[1][2].to_f32()],
            [m[2][0].to_f32(), m[2][1].to_f32(), m[2][2].to_f32()],
        ]),
        (LpsType::Mat4, LpsValueQ32::Mat4x4(m)) => LpsValueF32::Mat4x4([
            [
                m[0][0].to_f32(),
                m[0][1].to_f32(),
                m[0][2].to_f32(),
                m[0][3].to_f32(),
            ],
            [
                m[1][0].to_f32(),
                m[1][1].to_f32(),
                m[1][2].to_f32(),
                m[1][3].to_f32(),
            ],
            [
                m[2][0].to_f32(),
                m[2][1].to_f32(),
                m[2][2].to_f32(),
                m[2][3].to_f32(),
            ],
            [
                m[3][0].to_f32(),
                m[3][1].to_f32(),
                m[3][2].to_f32(),
                m[3][3].to_f32(),
            ],
        ]),

        (LpsType::Array { element, len }, LpsValueQ32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(bad());
            }
            let mut elems = Vec::with_capacity(items.len());
            for g in Vec::from(items) {
                elems.push(q32_to_lps_value_f32(element, g)?);
            }
            LpsValueF32::Array(elems.into_boxed_slice())
        }

        (
            LpsType::Struct { name, members },
            LpsValueQ32::Struct {
                name: vname,
                fields: items,
            },
        ) => {
            if members.len() != items.len() {
                return Err(bad());
            }
            let mut fields = Vec::with_capacity(members.len());
            for (i, m) in members.iter().enumerate() {
                let key = m.name.clone().unwrap_or_else(|| format!("_{i}"));
                let (fname, fv) = &items[i];
                if fname != &key {
                    return Err(bad());
                }
                fields.push((fname.clone(), q32_to_lps_value_f32(&m.ty, fv.clone())?));
            }
            LpsValueF32::Struct {
                name: vname.or(name.clone()),
                fields,
            }
        }

        _ => return Err(bad()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_scalar_float() {
        let ty = LpsType::Float;
        let v = LpsValueF32::F32(1.25);
        let q = lps_value_f32_to_q32(&ty, &v).unwrap();
        let back = q32_to_lps_value_f32(&ty, q).unwrap();
        assert!(back.approx_eq_default(&v));
    }
}
