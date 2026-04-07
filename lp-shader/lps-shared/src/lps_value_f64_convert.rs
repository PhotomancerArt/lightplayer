//! Q32 marshalling between [`LpsValueF32`] and [`LpsValueF64`] for LPVM call paths.

use crate::lps_value_f64::{CallError, LpsValueF64};
use crate::{LpsType, LpsValueF32};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Convert a typed [`LpsValueF32`] into Q32 representation for Cranelift / RV32 calls.
pub fn lps_value_to_f64(ty: &LpsType, v: &LpsValueF32) -> Result<LpsValueF64, CallError> {
    Ok(match (ty, v) {
        (LpsType::Float, LpsValueF32::F32(x)) => LpsValueF64::Float(f64::from(*x)),
        (LpsType::Int, LpsValueF32::I32(x)) => LpsValueF64::Int(*x),
        (LpsType::UInt, LpsValueF32::U32(x)) => LpsValueF64::UInt(*x),
        (LpsType::Bool, LpsValueF32::Bool(b)) => LpsValueF64::Bool(*b),

        (LpsType::Vec2, LpsValueF32::Vec2(a)) => {
            LpsValueF64::Vec2(f64::from(a[0]), f64::from(a[1]))
        }
        (LpsType::Vec3, LpsValueF32::Vec3(a)) => {
            LpsValueF64::Vec3(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]))
        }
        (LpsType::Vec4, LpsValueF32::Vec4(a)) => LpsValueF64::Vec4(
            f64::from(a[0]),
            f64::from(a[1]),
            f64::from(a[2]),
            f64::from(a[3]),
        ),

        (LpsType::IVec2, LpsValueF32::IVec2(a)) => LpsValueF64::IVec2(a[0], a[1]),
        (LpsType::IVec3, LpsValueF32::IVec3(a)) => LpsValueF64::IVec3(a[0], a[1], a[2]),
        (LpsType::IVec4, LpsValueF32::IVec4(a)) => LpsValueF64::IVec4(a[0], a[1], a[2], a[3]),

        (LpsType::UVec2, LpsValueF32::UVec2(a)) => LpsValueF64::UVec2(a[0], a[1]),
        (LpsType::UVec3, LpsValueF32::UVec3(a)) => LpsValueF64::UVec3(a[0], a[1], a[2]),
        (LpsType::UVec4, LpsValueF32::UVec4(a)) => LpsValueF64::UVec4(a[0], a[1], a[2], a[3]),

        (LpsType::BVec2, LpsValueF32::BVec2(a)) => LpsValueF64::BVec2(a[0], a[1]),
        (LpsType::BVec3, LpsValueF32::BVec3(a)) => LpsValueF64::BVec3(a[0], a[1], a[2]),
        (LpsType::BVec4, LpsValueF32::BVec4(a)) => LpsValueF64::BVec4(a[0], a[1], a[2], a[3]),

        (LpsType::Mat2, LpsValueF32::Mat2x2(m)) => LpsValueF64::Mat2([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
        ]),
        (LpsType::Mat3, LpsValueF32::Mat3x3(m)) => LpsValueF64::Mat3([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[0][2]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
            f64::from(m[1][2]),
            f64::from(m[2][0]),
            f64::from(m[2][1]),
            f64::from(m[2][2]),
        ]),
        (LpsType::Mat4, LpsValueF32::Mat4x4(m)) => LpsValueF64::Mat4([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[0][2]),
            f64::from(m[0][3]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
            f64::from(m[1][2]),
            f64::from(m[1][3]),
            f64::from(m[2][0]),
            f64::from(m[2][1]),
            f64::from(m[2][2]),
            f64::from(m[2][3]),
            f64::from(m[3][0]),
            f64::from(m[3][1]),
            f64::from(m[3][2]),
            f64::from(m[3][3]),
        ]),

        (LpsType::Array { element, len }, LpsValueF32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(CallError::TypeMismatch(format!(
                    "array length mismatch: expected {}, got {}",
                    len,
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for it in items.iter() {
                out.push(lps_value_to_f64(element, it)?);
            }
            LpsValueF64::Array(out)
        }

        (LpsType::Struct { .. }, LpsValueF32::Struct { .. }) => {
            return Err(CallError::Unsupported(String::from(
                "struct parameters are not supported by Q32 marshalling yet",
            )));
        }

        (expected, _got) => {
            return Err(CallError::TypeMismatch(format!(
                "argument type mismatch: expected {expected:?}, got incompatible LpsValue"
            )));
        }
    })
}

/// Convert Q32 return words into [`LpsValueF32`].
pub fn glsl_f64_to_lps_value(ty: &LpsType, v: LpsValueF64) -> Result<LpsValueF32, CallError> {
    let bad = || CallError::TypeMismatch(format!("return shape mismatch for type {ty:?}"));

    Ok(match (ty, v) {
        (LpsType::Float, LpsValueF64::Float(x)) => LpsValueF32::F32(x as f32),
        (LpsType::Int, LpsValueF64::Int(x)) => LpsValueF32::I32(x),
        (LpsType::UInt, LpsValueF64::UInt(x)) => LpsValueF32::U32(x),
        (LpsType::Bool, LpsValueF64::Bool(b)) => LpsValueF32::Bool(b),

        (LpsType::Vec2, LpsValueF64::Vec2(a, b)) => LpsValueF32::Vec2([a as f32, b as f32]),
        (LpsType::Vec3, LpsValueF64::Vec3(a, b, c)) => {
            LpsValueF32::Vec3([a as f32, b as f32, c as f32])
        }
        (LpsType::Vec4, LpsValueF64::Vec4(a, b, c, d)) => {
            LpsValueF32::Vec4([a as f32, b as f32, c as f32, d as f32])
        }

        (LpsType::IVec2, LpsValueF64::IVec2(a, b)) => LpsValueF32::IVec2([a, b]),
        (LpsType::IVec3, LpsValueF64::IVec3(a, b, c)) => LpsValueF32::IVec3([a, b, c]),
        (LpsType::IVec4, LpsValueF64::IVec4(a, b, c, d)) => LpsValueF32::IVec4([a, b, c, d]),

        (LpsType::UVec2, LpsValueF64::UVec2(a, b)) => LpsValueF32::UVec2([a, b]),
        (LpsType::UVec3, LpsValueF64::UVec3(a, b, c)) => LpsValueF32::UVec3([a, b, c]),
        (LpsType::UVec4, LpsValueF64::UVec4(a, b, c, d)) => LpsValueF32::UVec4([a, b, c, d]),

        (LpsType::BVec2, LpsValueF64::BVec2(a, b)) => LpsValueF32::BVec2([a, b]),
        (LpsType::BVec3, LpsValueF64::BVec3(a, b, c)) => LpsValueF32::BVec3([a, b, c]),
        (LpsType::BVec4, LpsValueF64::BVec4(a, b, c, d)) => LpsValueF32::BVec4([a, b, c, d]),

        (LpsType::Mat2, LpsValueF64::Mat2(a)) => {
            LpsValueF32::Mat2x2([[a[0] as f32, a[1] as f32], [a[2] as f32, a[3] as f32]])
        }
        (LpsType::Mat3, LpsValueF64::Mat3(a)) => LpsValueF32::Mat3x3([
            [a[0] as f32, a[1] as f32, a[2] as f32],
            [a[3] as f32, a[4] as f32, a[5] as f32],
            [a[6] as f32, a[7] as f32, a[8] as f32],
        ]),
        (LpsType::Mat4, LpsValueF64::Mat4(a)) => LpsValueF32::Mat4x4([
            [a[0] as f32, a[1] as f32, a[2] as f32, a[3] as f32],
            [a[4] as f32, a[5] as f32, a[6] as f32, a[7] as f32],
            [a[8] as f32, a[9] as f32, a[10] as f32, a[11] as f32],
            [a[12] as f32, a[13] as f32, a[14] as f32, a[15] as f32],
        ]),

        (LpsType::Array { element, len }, LpsValueF64::Array(items)) => {
            if items.len() != *len as usize {
                return Err(bad());
            }
            let mut elems = Vec::with_capacity(items.len());
            for g in items {
                elems.push(glsl_f64_to_lps_value(element, g)?);
            }
            LpsValueF32::Array(elems.into_boxed_slice())
        }

        _ => return Err(bad()),
    })
}
