//! Path-based navigation on [`crate::lps_value_f32::LpsValueF32`] trees (struct, array, vector components).

use alloc::borrow::Cow;
use alloc::string::String;

use crate::lps_value_f32::LpsValueF32;
use crate::path::{LpsPathSeg, PathParseError, parse_path};

/// Failure resolving a path on a [`LpsValueF32`].
#[derive(Debug)]
pub enum LpsValuePathError {
    Parse(PathParseError),
    FieldNotFound { field: String },
    IndexOutOfBounds { index: usize, len: usize },
    NotIndexable,
    NotAField { hint: &'static str },
}

impl core::fmt::Display for LpsValuePathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::FieldNotFound { field } => write!(f, "field `{field}` not found"),
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {index} out of bounds (len {len})")
            }
            Self::NotIndexable => write!(f, "value is not indexable"),
            Self::NotAField { hint } => write!(f, "value has no fields ({hint})"),
        }
    }
}

impl core::error::Error for LpsValuePathError {}

impl From<PathParseError> for LpsValuePathError {
    fn from(e: PathParseError) -> Self {
        Self::Parse(e)
    }
}

/// Path-based get/set on [`LpsValueF32`] trees ([`LpsValueF32`] lives in `lps-shared`).
pub trait LpsValuePathExt {
    /// Resolve `path` to a value; vector/matrix swizzles yield owned scalars or small composites.
    fn get_path<'a>(&'a self, path: &str) -> Result<Cow<'a, LpsValueF32>, LpsValuePathError>;

    /// Set the value at `path` when the path ends at a mutable slot (struct field, array element,
    /// or vector component).
    fn set_path(&mut self, path: &str, value: LpsValueF32) -> Result<(), LpsValuePathError>;
}

impl LpsValuePathExt for LpsValueF32 {
    fn get_path<'a>(&'a self, path: &str) -> Result<Cow<'a, LpsValueF32>, LpsValuePathError> {
        let segs = parse_path(path).map_err(LpsValuePathError::from)?;
        walk_get(self, &segs)
    }

    fn set_path(&mut self, path: &str, value: LpsValueF32) -> Result<(), LpsValuePathError> {
        let segs = parse_path(path).map_err(LpsValuePathError::from)?;
        walk_set(self, &segs, value)
    }
}

fn walk_get<'a>(
    v: &'a LpsValueF32,
    segs: &[LpsPathSeg],
) -> Result<Cow<'a, LpsValueF32>, LpsValuePathError> {
    if segs.is_empty() {
        return Ok(Cow::Borrowed(v));
    }
    match (&segs[0], v) {
        (LpsPathSeg::Field(name), LpsValueF32::Struct { fields, .. }) => {
            let (_, sub) = fields.iter().find(|(n, _)| n == name).ok_or_else(|| {
                LpsValuePathError::FieldNotFound {
                    field: name.clone(),
                }
            })?;
            walk_get(sub, &segs[1..])
        }
        (LpsPathSeg::Index(idx), LpsValueF32::Array(items)) => {
            let len = items.len();
            let sub = items
                .get(*idx)
                .ok_or(LpsValuePathError::IndexOutOfBounds { index: *idx, len })?;
            walk_get(sub, &segs[1..])
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec2(a)) => {
            let x = vec2_component(a, name)?;
            Ok(Cow::Owned(walk_vec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec3(a)) => {
            let x = vec3_component(a, name)?;
            Ok(Cow::Owned(walk_vec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec4(a)) => {
            let x = vec4_component(a, name)?;
            Ok(Cow::Owned(walk_vec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec2(a)) => {
            let x = ivec2_component(a, name)?;
            Ok(Cow::Owned(walk_ivec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec3(a)) => {
            let x = ivec3_component(a, name)?;
            Ok(Cow::Owned(walk_ivec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec4(a)) => {
            let x = ivec4_component(a, name)?;
            Ok(Cow::Owned(walk_ivec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec2(a)) => {
            let x = uvec2_component(a, name)?;
            Ok(Cow::Owned(walk_uvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec3(a)) => {
            let x = uvec3_component(a, name)?;
            Ok(Cow::Owned(walk_uvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec4(a)) => {
            let x = uvec4_component(a, name)?;
            Ok(Cow::Owned(walk_uvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec2(a)) => {
            let x = bvec2_component(a, name)?;
            Ok(Cow::Owned(walk_bvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec3(a)) => {
            let x = bvec3_component(a, name)?;
            Ok(Cow::Owned(walk_bvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec4(a)) => {
            let x = bvec4_component(a, name)?;
            Ok(Cow::Owned(walk_bvec_tail(x, &segs[1..])?))
        }
        (LpsPathSeg::Field(_), _) => Err(LpsValuePathError::NotAField {
            hint: "not a struct or vector",
        }),
        (LpsPathSeg::Index(_), _) => Err(LpsValuePathError::NotIndexable),
    }
}

fn walk_vec_tail(x: LpsValueF32, segs: &[LpsPathSeg]) -> Result<LpsValueF32, LpsValuePathError> {
    if segs.is_empty() {
        return Ok(x);
    }
    Err(LpsValuePathError::NotAField {
        hint: "no nested fields on scalar",
    })
}

fn walk_ivec_tail(x: LpsValueF32, segs: &[LpsPathSeg]) -> Result<LpsValueF32, LpsValuePathError> {
    if segs.is_empty() {
        return Ok(x);
    }
    Err(LpsValuePathError::NotAField {
        hint: "no nested fields on scalar",
    })
}

fn walk_uvec_tail(x: LpsValueF32, segs: &[LpsPathSeg]) -> Result<LpsValueF32, LpsValuePathError> {
    if segs.is_empty() {
        return Ok(x);
    }
    Err(LpsValuePathError::NotAField {
        hint: "no nested fields on scalar",
    })
}

fn walk_bvec_tail(x: LpsValueF32, segs: &[LpsPathSeg]) -> Result<LpsValueF32, LpsValuePathError> {
    if segs.is_empty() {
        return Ok(x);
    }
    Err(LpsValuePathError::NotAField {
        hint: "no nested fields on scalar",
    })
}

fn walk_set(
    v: &mut LpsValueF32,
    segs: &[LpsPathSeg],
    value: LpsValueF32,
) -> Result<(), LpsValuePathError> {
    if segs.is_empty() {
        *v = value;
        return Ok(());
    }
    match (&segs[0], v) {
        (LpsPathSeg::Field(name), LpsValueF32::Struct { fields, .. }) => {
            let (_, sub) = fields.iter_mut().find(|(n, _)| n == name).ok_or_else(|| {
                LpsValuePathError::FieldNotFound {
                    field: name.clone(),
                }
            })?;
            walk_set(sub, &segs[1..], value)
        }
        (LpsPathSeg::Index(idx), LpsValueF32::Array(items)) => {
            let len = items.len();
            let sub = items
                .get_mut(*idx)
                .ok_or(LpsValuePathError::IndexOutOfBounds { index: *idx, len })?;
            walk_set(sub, &segs[1..], value)
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec2(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::F32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "vec2 component requires f32",
                });
            };
            *vec2_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec3(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::F32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "vec3 component requires f32",
                });
            };
            *vec3_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::Vec4(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::F32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "vec4 component requires f32",
                });
            };
            *vec4_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec2(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::I32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "ivec2 component requires i32",
                });
            };
            *ivec2_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec3(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::I32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "ivec3 component requires i32",
                });
            };
            *ivec3_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::IVec4(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::I32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "ivec4 component requires i32",
                });
            };
            *ivec4_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec2(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::U32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "uvec2 component requires u32",
                });
            };
            *uvec2_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec3(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::U32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "uvec3 component requires u32",
                });
            };
            *uvec3_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::UVec4(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::U32(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "uvec4 component requires u32",
                });
            };
            *uvec4_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec2(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::Bool(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "bvec2 component requires bool",
                });
            };
            *bvec2_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec3(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::Bool(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "bvec3 component requires bool",
                });
            };
            *bvec3_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(name), LpsValueF32::BVec4(a)) => {
            if !segs[1..].is_empty() {
                return Err(LpsValuePathError::NotAField {
                    hint: "no nested fields on scalar",
                });
            }
            let LpsValueF32::Bool(x) = value else {
                return Err(LpsValuePathError::NotAField {
                    hint: "bvec4 component requires bool",
                });
            };
            *bvec4_component_mut(a, name)? = x;
            Ok(())
        }
        (LpsPathSeg::Field(_), _) => Err(LpsValuePathError::NotAField {
            hint: "not a struct, array, or vector",
        }),
        (LpsPathSeg::Index(_), _) => Err(LpsValuePathError::NotIndexable),
    }
}

fn vec2_component(a: &[f32; 2], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::F32(*component2_f32(a, name)?))
}

fn vec3_component(a: &[f32; 3], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::F32(*component3_f32(a, name)?))
}

fn vec4_component(a: &[f32; 4], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::F32(*component4_f32(a, name)?))
}

fn ivec2_component(a: &[i32; 2], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::I32(*component2_i32(a, name)?))
}

fn ivec3_component(a: &[i32; 3], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::I32(*component3_i32(a, name)?))
}

fn ivec4_component(a: &[i32; 4], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::I32(*component4_i32(a, name)?))
}

fn uvec2_component(a: &[u32; 2], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::U32(*component2_u32(a, name)?))
}

fn uvec3_component(a: &[u32; 3], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::U32(*component3_u32(a, name)?))
}

fn uvec4_component(a: &[u32; 4], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::U32(*component4_u32(a, name)?))
}

fn bvec2_component(a: &[bool; 2], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::Bool(*component2_bool(a, name)?))
}

fn bvec3_component(a: &[bool; 3], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::Bool(*component3_bool(a, name)?))
}

fn bvec4_component(a: &[bool; 4], name: &str) -> Result<LpsValueF32, LpsValuePathError> {
    Ok(LpsValueF32::Bool(*component4_bool(a, name)?))
}

fn vec2_component_mut<'a>(
    a: &'a mut [f32; 2],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    component2_f32_mut(a, name)
}

fn vec3_component_mut<'a>(
    a: &'a mut [f32; 3],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    component3_f32_mut(a, name)
}

fn vec4_component_mut<'a>(
    a: &'a mut [f32; 4],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    component4_f32_mut(a, name)
}

fn ivec2_component_mut<'a>(
    a: &'a mut [i32; 2],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    component2_i32_mut(a, name)
}

fn ivec3_component_mut<'a>(
    a: &'a mut [i32; 3],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    component3_i32_mut(a, name)
}

fn ivec4_component_mut<'a>(
    a: &'a mut [i32; 4],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    component4_i32_mut(a, name)
}

fn uvec2_component_mut<'a>(
    a: &'a mut [u32; 2],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    component2_u32_mut(a, name)
}

fn uvec3_component_mut<'a>(
    a: &'a mut [u32; 3],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    component3_u32_mut(a, name)
}

fn uvec4_component_mut<'a>(
    a: &'a mut [u32; 4],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    component4_u32_mut(a, name)
}

fn bvec2_component_mut<'a>(
    a: &'a mut [bool; 2],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    component2_bool_mut(a, name)
}

fn bvec3_component_mut<'a>(
    a: &'a mut [bool; 3],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    component3_bool_mut(a, name)
}

fn bvec4_component_mut<'a>(
    a: &'a mut [bool; 4],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    component4_bool_mut(a, name)
}

fn component2_f32<'a>(a: &'a [f32; 2], name: &str) -> Result<&'a f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_f32<'a>(a: &'a [f32; 3], name: &str) -> Result<&'a f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_f32<'a>(a: &'a [f32; 4], name: &str) -> Result<&'a f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        "w" | "a" | "q" => Ok(&a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_i32<'a>(a: &'a [i32; 2], name: &str) -> Result<&'a i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_i32<'a>(a: &'a [i32; 3], name: &str) -> Result<&'a i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_i32<'a>(a: &'a [i32; 4], name: &str) -> Result<&'a i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        "w" | "a" | "q" => Ok(&a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_u32<'a>(a: &'a [u32; 2], name: &str) -> Result<&'a u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_u32<'a>(a: &'a [u32; 3], name: &str) -> Result<&'a u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_u32<'a>(a: &'a [u32; 4], name: &str) -> Result<&'a u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        "w" | "a" | "q" => Ok(&a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_bool<'a>(a: &'a [bool; 2], name: &str) -> Result<&'a bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_bool<'a>(a: &'a [bool; 3], name: &str) -> Result<&'a bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_bool<'a>(a: &'a [bool; 4], name: &str) -> Result<&'a bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&a[0]),
        "y" | "g" | "t" => Ok(&a[1]),
        "z" | "b" | "p" => Ok(&a[2]),
        "w" | "a" | "q" => Ok(&a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_f32_mut<'a>(
    a: &'a mut [f32; 2],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_f32_mut<'a>(
    a: &'a mut [f32; 3],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_f32_mut<'a>(
    a: &'a mut [f32; 4],
    name: &str,
) -> Result<&'a mut f32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        "w" | "a" | "q" => Ok(&mut a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_i32_mut<'a>(
    a: &'a mut [i32; 2],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_i32_mut<'a>(
    a: &'a mut [i32; 3],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_i32_mut<'a>(
    a: &'a mut [i32; 4],
    name: &str,
) -> Result<&'a mut i32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        "w" | "a" | "q" => Ok(&mut a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_u32_mut<'a>(
    a: &'a mut [u32; 2],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_u32_mut<'a>(
    a: &'a mut [u32; 3],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_u32_mut<'a>(
    a: &'a mut [u32; 4],
    name: &str,
) -> Result<&'a mut u32, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        "w" | "a" | "q" => Ok(&mut a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component2_bool_mut<'a>(
    a: &'a mut [bool; 2],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component3_bool_mut<'a>(
    a: &'a mut [bool; 3],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

fn component4_bool_mut<'a>(
    a: &'a mut [bool; 4],
    name: &str,
) -> Result<&'a mut bool, LpsValuePathError> {
    match name {
        "x" | "r" | "s" => Ok(&mut a[0]),
        "y" | "g" | "t" => Ok(&mut a[1]),
        "z" | "b" | "p" => Ok(&mut a[2]),
        "w" | "a" | "q" => Ok(&mut a[3]),
        _ => Err(LpsValuePathError::FieldNotFound {
            field: String::from(name),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn get_path_struct_field() {
        let v = LpsValueF32::Struct {
            name: None,
            fields: vec![
                (String::from("a"), LpsValueF32::F32(1.0)),
                (String::from("b"), LpsValueF32::I32(2)),
            ],
        };
        match v.get_path("a").unwrap() {
            Cow::Borrowed(LpsValueF32::F32(x)) => assert!((*x - 1.0).abs() < 1e-6),
            Cow::Owned(LpsValueF32::F32(x)) => assert!((x - 1.0).abs() < 1e-6),
            _ => panic!("expected f32"),
        }
    }

    #[test]
    fn set_path_vec_component() {
        let mut v = LpsValueF32::Vec3([0.0, 0.0, 0.0]);
        v.set_path("y", LpsValueF32::F32(3.0)).unwrap();
        let LpsValueF32::Vec3(a) = &v else {
            panic!("vec3");
        };
        assert!((a[1] - 3.0).abs() < 1e-6);
    }
}
