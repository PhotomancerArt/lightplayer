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

use crate::texture_format::LpsTexture2DValue;
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
    /// Packed buffer twin of [`LpsValueF32::Buffer`]: float lanes already
    /// encoded per the chosen [`FloatLaneAbi`], integer lanes raw. PACKED
    /// words — any layout stride padding is added where bytes are emitted.
    Buffer {
        elem: crate::LpsBufferElem,
        words: Box<[i32]>,
    },
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueQ32)>,
    },
    /// Same host payload as [`LpsValueF32::Texture2D`] (guest uniform still encodes descriptor lanes only).
    Texture2D(LpsTexture2DValue),
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

/// How one GLSL `float` lane is packed into its ABI word.
///
/// Both float modes lay aggregates out identically — the same std430 offsets,
/// the same dense array lanes, the same word count per type. **Only the `float`
/// lane's contents differ.** This enum is that one difference, so the traversal
/// that walks structs, arrays, and matrices is shared instead of forked, and
/// cannot drift between the two modes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatLaneAbi {
    /// Q16.16 fixed point, via [`q32_encode`] (saturating) and [`Q32::to_f32`].
    Q16_16,
    /// The IEEE-754 binary32 **bit pattern**, as produced by
    /// [`f32::to_bits`]. Used by every native-f32 backend: on a soft-float ABI
    /// a `float` is literally this word in an integer register.
    Ieee754Bits,
}

impl FloatLaneAbi {
    fn encode(self, x: f32) -> Q32 {
        match self {
            FloatLaneAbi::Q16_16 => f32_to_q32_abi(x),
            // The `Q32` newtype is being used here as "one raw ABI lane word",
            // not as a Q16.16 number. That is the whole trick that lets the two
            // modes share a traversal; nothing downstream interprets the word
            // until [`FloatLaneAbi::decode`] reverses it.
            FloatLaneAbi::Ieee754Bits => Q32::from_fixed(x.to_bits() as i32),
        }
    }

    fn decode(self, q: Q32) -> f32 {
        match self {
            FloatLaneAbi::Q16_16 => q.to_f32(),
            FloatLaneAbi::Ieee754Bits => f32::from_bits(q.to_fixed() as u32),
        }
    }
}

/// Convert [`LpsValueF32`] to [`LpsValueQ32`] using [`q32_encode`] for float components
/// so host arguments match compiler constant emission and the historical `f64` path.
pub fn lps_value_f32_to_q32(
    ty: &LpsType,
    v: &LpsValueF32,
) -> Result<LpsValueQ32, LpsValueQ32Error> {
    lps_value_f32_to_lanes(ty, v, FloatLaneAbi::Q16_16)
}

/// [`lps_value_f32_to_q32`] with the float-lane encoding chosen explicitly.
///
/// With [`FloatLaneAbi::Ieee754Bits`] the result is **not** a Q16.16 value: each
/// `F32` component carries a raw IEEE bit pattern in the [`Q32`] newtype. Pair
/// it with [`lanes_to_lps_value_f32`] using the same `abi`.
pub fn lps_value_f32_to_lanes(
    ty: &LpsType,
    v: &LpsValueF32,
    abi: FloatLaneAbi,
) -> Result<LpsValueQ32, LpsValueQ32Error> {
    let f32_to_q32_abi = |x: f32| abi.encode(x);
    Ok(match (ty, v) {
        (LpsType::Texture2D, LpsValueF32::Texture2D(v)) => LpsValueQ32::Texture2D(*v),
        (LpsType::Texture2D, _) => {
            return Err(LpsValueQ32Error::TypeMismatch(String::from(
                "LpsType::Texture2D expects LpsValueF32::Texture2D (opaque descriptor), not a uvec4 stand-in",
            )));
        }
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
                out.push(lps_value_f32_to_lanes(element, it, abi)?);
            }
            LpsValueQ32::Array(out.into_boxed_slice())
        }

        (LpsType::Array { element, len }, LpsValueF32::Buffer(buffer)) => {
            let elem = crate::LpsBufferElem::from_lps_type(element).ok_or_else(|| {
                LpsValueQ32Error::TypeMismatch(format!(
                    "buffer value for array of non-buffer element {element:?}"
                ))
            })?;
            if buffer.elem != elem || buffer.len() != *len {
                return Err(LpsValueQ32Error::TypeMismatch(format!(
                    "buffer shape mismatch: expected {elem:?}[{len}], got {:?}[{}]",
                    buffer.elem,
                    buffer.len()
                )));
            }
            // One loop over words, no per-element boxing: integer lanes are
            // the same bits in both ABIs, float lanes transcode per `abi`.
            let words = if elem.is_float() {
                buffer
                    .words()
                    .iter()
                    .map(|w| f32_to_q32_abi(f32::from_bits(*w)).to_fixed())
                    .collect()
            } else {
                buffer.words().iter().map(|w| *w as i32).collect()
            };
            LpsValueQ32::Buffer { elem, words }
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
                out.push((fname.clone(), lps_value_f32_to_lanes(&m.ty, fv, abi)?));
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
    lanes_to_lps_value_f32(ty, v, FloatLaneAbi::Q16_16)
}

/// [`q32_to_lps_value_f32`] with the float-lane encoding chosen explicitly —
/// the inverse of [`lps_value_f32_to_lanes`] for the same `abi`.
pub fn lanes_to_lps_value_f32(
    ty: &LpsType,
    v: LpsValueQ32,
    abi: FloatLaneAbi,
) -> Result<LpsValueF32, LpsValueQ32Error> {
    let bad = || LpsValueQ32Error::TypeMismatch(format!("return shape mismatch for type {ty:?}"));
    let dec = |q: Q32| abi.decode(q);

    Ok(match (ty, v) {
        (LpsType::Texture2D, LpsValueQ32::Texture2D(v)) => LpsValueF32::Texture2D(v),
        (LpsType::Texture2D, _) => {
            return Err(LpsValueQ32Error::TypeMismatch(String::from(
                "LpsType::Texture2D expects LpsValueQ32::Texture2D (opaque descriptor), not a uvec4 stand-in",
            )));
        }
        (LpsType::Float, LpsValueQ32::F32(x)) => LpsValueF32::F32(dec(x)),
        (LpsType::Int, LpsValueQ32::I32(x)) => LpsValueF32::I32(x),
        (LpsType::UInt, LpsValueQ32::U32(x)) => LpsValueF32::U32(x),
        (LpsType::Bool, LpsValueQ32::Bool(b)) => LpsValueF32::Bool(b),

        (LpsType::Vec2, LpsValueQ32::Vec2(a)) => LpsValueF32::Vec2([dec(a[0]), dec(a[1])]),
        (LpsType::Vec3, LpsValueQ32::Vec3(a)) => {
            LpsValueF32::Vec3([dec(a[0]), dec(a[1]), dec(a[2])])
        }
        (LpsType::Vec4, LpsValueQ32::Vec4(a)) => {
            LpsValueF32::Vec4([dec(a[0]), dec(a[1]), dec(a[2]), dec(a[3])])
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

        (LpsType::Mat2, LpsValueQ32::Mat2x2(m)) => {
            LpsValueF32::Mat2x2([[dec(m[0][0]), dec(m[0][1])], [dec(m[1][0]), dec(m[1][1])]])
        }
        (LpsType::Mat3, LpsValueQ32::Mat3x3(m)) => LpsValueF32::Mat3x3([
            [dec(m[0][0]), dec(m[0][1]), dec(m[0][2])],
            [dec(m[1][0]), dec(m[1][1]), dec(m[1][2])],
            [dec(m[2][0]), dec(m[2][1]), dec(m[2][2])],
        ]),
        (LpsType::Mat4, LpsValueQ32::Mat4x4(m)) => LpsValueF32::Mat4x4([
            [dec(m[0][0]), dec(m[0][1]), dec(m[0][2]), dec(m[0][3])],
            [dec(m[1][0]), dec(m[1][1]), dec(m[1][2]), dec(m[1][3])],
            [dec(m[2][0]), dec(m[2][1]), dec(m[2][2]), dec(m[2][3])],
            [dec(m[3][0]), dec(m[3][1]), dec(m[3][2]), dec(m[3][3])],
        ]),

        (LpsType::Array { element, len }, LpsValueQ32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(bad());
            }
            let mut elems = Vec::with_capacity(items.len());
            for g in Vec::from(items) {
                elems.push(lanes_to_lps_value_f32(element, g, abi)?);
            }
            LpsValueF32::Array(elems.into_boxed_slice())
        }

        (LpsType::Array { element, len }, LpsValueQ32::Buffer { elem, words }) => {
            if crate::LpsBufferElem::from_lps_type(element) != Some(elem)
                || words.len() != (*len as usize) * elem.word_stride() as usize
            {
                return Err(bad());
            }
            let words: alloc::vec::Vec<u32> = if elem.is_float() {
                words
                    .iter()
                    .map(|w| dec(Q32::from_fixed(*w)).to_bits())
                    .collect()
            } else {
                words.iter().map(|w| *w as u32).collect()
            };
            LpsValueF32::Buffer(
                crate::LpsBuffer::from_words(elem, words.into_boxed_slice()).map_err(|_| bad())?,
            )
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
                fields.push((
                    fname.clone(),
                    lanes_to_lps_value_f32(&m.ty, fv.clone(), abi)?,
                ));
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
    use crate::TextureStorageFormat;

    #[test]
    fn round_trip_scalar_float() {
        let ty = LpsType::Float;
        let v = LpsValueF32::F32(1.25);
        let q = lps_value_f32_to_q32(&ty, &v).unwrap();
        let back = q32_to_lps_value_f32(&ty, q).unwrap();
        assert!(back.approx_eq_default(&v));
    }

    #[test]
    fn texture2d_f32_q32_noop() {
        use crate::{LpsTexture2DDescriptor, LpsTexture2DValue};

        let ty = LpsType::Texture2D;
        let tv = LpsTexture2DValue {
            descriptor: LpsTexture2DDescriptor {
                ptr: 0x10,
                width: 2,
                height: 3,
                row_stride: 8,
            },
            format: TextureStorageFormat::Rgba16Unorm,
            byte_len: 96,
        };
        let v = LpsValueF32::Texture2D(tv);
        let q = lps_value_f32_to_q32(&ty, &v).unwrap();
        assert_eq!(q, LpsValueQ32::Texture2D(tv));
        let back = q32_to_lps_value_f32(&ty, q).unwrap();
        assert!(back.eq(&v));
    }

    /// The property that makes the IEEE lane ABI worth having: values Q16.16
    /// cannot represent survive it **exactly**, bit for bit, and the two modes
    /// share one traversal so a struct or matrix cannot round-trip in one mode
    /// and not the other.
    #[test]
    fn ieee_lanes_round_trip_values_q32_cannot_hold() {
        for (ty, v) in [
            (LpsType::Float, LpsValueF32::F32(1.234_567_8e10)),
            (LpsType::Float, LpsValueF32::F32(1e-30)),
            (LpsType::Float, LpsValueF32::F32(-0.0)),
            (
                LpsType::Vec3,
                LpsValueF32::Vec3([f32::INFINITY, -1e20, 1.0e-38]),
            ),
        ] {
            let lanes = lps_value_f32_to_lanes(&ty, &v, FloatLaneAbi::Ieee754Bits).unwrap();
            let back = lanes_to_lps_value_f32(&ty, lanes, FloatLaneAbi::Ieee754Bits).unwrap();
            assert!(
                back.eq(&v),
                "{ty:?}: {v:?} did not survive the IEEE lane ABI"
            );
        }
    }

    /// NaN survives as a *bit pattern*. `eq` would be false for it, so this
    /// checks the lane word directly — and it is the case a "clever" conversion
    /// through `f64` or through `Q32` arithmetic would quietly destroy.
    #[test]
    fn ieee_lanes_carry_nan_bits_untouched() {
        let signalling = f32::from_bits(0x7f80_0001);
        let lanes = lps_value_f32_to_lanes(
            &LpsType::Float,
            &LpsValueF32::F32(signalling),
            FloatLaneAbi::Ieee754Bits,
        )
        .unwrap();
        let LpsValueQ32::F32(word) = lanes else {
            panic!("expected a float lane");
        };
        assert_eq!(word.to_fixed() as u32, 0x7f80_0001);
    }

    /// The Ieee754Bits lane is a raw word, not a number: reading it back under
    /// the Q16.16 codec must not accidentally agree. If these two ever match,
    /// the codec split has collapsed and the modes are silently sharing an
    /// encoding.
    #[test]
    fn the_two_lane_abis_are_not_interchangeable() {
        let v = LpsValueF32::F32(1.0);
        let ieee = lps_value_f32_to_lanes(&LpsType::Float, &v, FloatLaneAbi::Ieee754Bits).unwrap();
        let fixed = lps_value_f32_to_lanes(&LpsType::Float, &v, FloatLaneAbi::Q16_16).unwrap();
        assert_ne!(ieee, fixed);
    }
}
