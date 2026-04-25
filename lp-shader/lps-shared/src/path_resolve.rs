//! [`LpsType`] path resolution: byte offsets and leaf types.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::layout::{array_stride, round_up, type_alignment, type_size};
use crate::path::{LpsPathSeg, PathParseError, parse_path};
use crate::{LayoutRules, LpsType, StructMember};

/// Path-based offset and type projection for [`LpsType`].
///
/// Implemented as a trait because [`LpsType`] is defined in `lps-shared`.
pub trait LpsTypePathExt {
    /// Byte offset for `path` under `rules`, added to `base_offset`.
    fn offset_for_path(
        &self,
        path: &str,
        rules: LayoutRules,
        base_offset: usize,
    ) -> Result<usize, PathError>;

    /// Leaf [`LpsType`] after following `path` (owned; vector components yield scalars).
    fn type_at_path(&self, path: &str) -> Result<LpsType, PathError>;
}

impl LpsTypePathExt for LpsType {
    fn offset_for_path(
        &self,
        path: &str,
        rules: LayoutRules,
        base_offset: usize,
    ) -> Result<usize, PathError> {
        let segs = parse_path(path).map_err(PathError::Parse)?;
        offset_walk(self, &segs, rules, base_offset)
    }

    fn type_at_path(&self, path: &str) -> Result<LpsType, PathError> {
        let segs = parse_path(path).map_err(PathError::Parse)?;
        type_walk(self, &segs)
    }
}

/// Failure resolving a path against a [`LpsType`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    Parse(PathParseError),
    FieldNotFound {
        field: String,
        available: Vec<String>,
    },
    IndexOutOfBounds {
        index: usize,
        len: u32,
    },
    NotIndexable {
        ty: String,
    },
    NotAField {
        ty: String,
    },
    UnsupportedSwizzle(String),
    TrailingPath {
        remaining: usize,
    },
}

impl core::fmt::Display for PathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::FieldNotFound { field, available } => {
                write!(f, "field '{field}' not found (available: {available:?})")
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {index} out of bounds (len {len})")
            }
            Self::NotIndexable { ty } => write!(f, "type `{ty}` is not indexable"),
            Self::NotAField { ty } => write!(f, "type `{ty}` has no fields"),
            Self::UnsupportedSwizzle(s) => write!(f, "unsupported swizzle `{s}`"),
            Self::TrailingPath { remaining } => {
                write!(f, "{remaining} path segments remain after scalar")
            }
        }
    }
}

fn type_walk(ty: &LpsType, segs: &[LpsPathSeg]) -> Result<LpsType, PathError> {
    if segs.is_empty() {
        return Ok(ty.clone());
    }
    match (&segs[0], ty) {
        (LpsPathSeg::Field(name), LpsType::Struct { members, .. }) => {
            let sub = members
                .iter()
                .find(|m| m.name.as_deref() == Some(name.as_str()))
                .map(|m| &m.ty)
                .ok_or_else(|| PathError::FieldNotFound {
                    field: name.clone(),
                    available: members.iter().filter_map(|m| m.name.clone()).collect(),
                })?;
            type_walk(sub, &segs[1..])
        }
        (
            LpsPathSeg::Field(name),
            vec_ty @ (LpsType::Vec2
            | LpsType::Vec3
            | LpsType::Vec4
            | LpsType::IVec2
            | LpsType::IVec3
            | LpsType::IVec4
            | LpsType::UVec2
            | LpsType::UVec3
            | LpsType::UVec4
            | LpsType::BVec2
            | LpsType::BVec3
            | LpsType::BVec4),
        ) => {
            let scalar_ty = vector_scalar_type(vec_ty, name)?;
            type_walk(&scalar_ty, &segs[1..])
        }
        (LpsPathSeg::Index(idx), LpsType::Array { element, len }) => {
            if *idx >= *len as usize {
                return Err(PathError::IndexOutOfBounds {
                    index: *idx,
                    len: *len,
                });
            }
            type_walk(element, &segs[1..])
        }
        (LpsPathSeg::Field(_name), LpsType::Void) => Err(PathError::NotAField {
            ty: String::from("void"),
        }),
        (LpsPathSeg::Field(_name), LpsType::Texture2D) => Err(PathError::NotAField {
            ty: String::from("Texture2D"),
        }),
        (
            LpsPathSeg::Field(_name),
            scalar @ (LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool),
        ) => Err(PathError::NotAField {
            ty: format!("{scalar:?}"),
        }),
        (LpsPathSeg::Field(name), _) => Err(PathError::FieldNotFound {
            field: name.clone(),
            available: field_names(ty),
        }),
        (LpsPathSeg::Index(_), _) => Err(PathError::NotIndexable { ty: type_name(ty) }),
    }
}

fn offset_walk(
    ty: &LpsType,
    segs: &[LpsPathSeg],
    rules: LayoutRules,
    base: usize,
) -> Result<usize, PathError> {
    if segs.is_empty() {
        return Ok(base);
    }
    match (&segs[0], ty) {
        (LpsPathSeg::Field(name), LpsType::Struct { members, .. }) => {
            let (off, sub) = struct_field_offset(members, name, rules, base)?;
            offset_walk(sub, &segs[1..], rules, off)
        }
        (LpsPathSeg::Field(_name), LpsType::Texture2D) => Err(PathError::NotAField {
            ty: String::from("Texture2D"),
        }),
        (
            LpsPathSeg::Field(name),
            vec_ty @ (LpsType::Vec2
            | LpsType::Vec3
            | LpsType::Vec4
            | LpsType::IVec2
            | LpsType::IVec3
            | LpsType::IVec4
            | LpsType::UVec2
            | LpsType::UVec3
            | LpsType::UVec4
            | LpsType::BVec2
            | LpsType::BVec3
            | LpsType::BVec4),
        ) => {
            let (comp_off, scalar_ty) = vector_component_meta(vec_ty, name)?;
            if !segs[1..].is_empty() {
                return match scalar_ty {
                    LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => {
                        Err(PathError::TrailingPath {
                            remaining: segs.len() - 1,
                        })
                    }
                    _ => offset_walk(&scalar_ty, &segs[1..], rules, base + comp_off),
                };
            }
            Ok(base + comp_off)
        }
        (LpsPathSeg::Index(idx), LpsType::Array { element, len }) => {
            if *idx >= *len as usize {
                return Err(PathError::IndexOutOfBounds {
                    index: *idx,
                    len: *len,
                });
            }
            let stride = array_stride(element, rules);
            offset_walk(element, &segs[1..], rules, base + idx * stride)
        }
        (LpsPathSeg::Index(_), LpsType::Texture2D) => Err(PathError::NotIndexable {
            ty: String::from("Texture2D"),
        }),
        (LpsPathSeg::Index(_), _) => Err(PathError::NotIndexable { ty: type_name(ty) }),
        (LpsPathSeg::Field(name), _) => Err(PathError::FieldNotFound {
            field: name.clone(),
            available: field_names(ty),
        }),
    }
}

fn struct_field_offset<'a>(
    members: &'a [StructMember],
    name: &str,
    rules: LayoutRules,
    base: usize,
) -> Result<(usize, &'a LpsType), PathError> {
    let mut offset = base;
    for m in members {
        let a = type_alignment(&m.ty, rules);
        offset = round_up(offset, a);
        if m.name.as_deref() == Some(name) {
            return Ok((offset, &m.ty));
        }
        offset += type_size(&m.ty, rules);
    }
    Err(PathError::FieldNotFound {
        field: String::from(name),
        available: members.iter().filter_map(|m| m.name.clone()).collect(),
    })
}

fn field_names(ty: &LpsType) -> Vec<String> {
    match ty {
        LpsType::Struct { members, .. } => members.iter().filter_map(|m| m.name.clone()).collect(),
        _ => Vec::new(),
    }
}

fn type_name(ty: &LpsType) -> String {
    format!("{ty:?}")
}

fn vector_scalar_type(ty: &LpsType, name: &str) -> Result<LpsType, PathError> {
    Ok(vector_component_meta(ty, name)?.1)
}

fn vector_component_meta(ty: &LpsType, name: &str) -> Result<(usize, LpsType), PathError> {
    use crate::LpsType::*;
    let idx = match ty {
        Vec2 | IVec2 | UVec2 | BVec2 => match name {
            "x" | "r" | "s" => 0usize,
            "y" | "g" | "t" => 1usize,
            _ => return Err(PathError::UnsupportedSwizzle(String::from(name))),
        },
        Vec3 | IVec3 | UVec3 | BVec3 => match name {
            "x" | "r" | "s" => 0usize,
            "y" | "g" | "t" => 1usize,
            "z" | "b" | "p" => 2usize,
            _ => return Err(PathError::UnsupportedSwizzle(String::from(name))),
        },
        Vec4 | IVec4 | UVec4 | BVec4 => match name {
            "x" | "r" | "s" => 0usize,
            "y" | "g" | "t" => 1usize,
            "z" | "b" | "p" => 2usize,
            "w" | "a" | "q" => 3usize,
            _ => return Err(PathError::UnsupportedSwizzle(String::from(name))),
        },
        _ => {
            return Err(PathError::NotAField { ty: type_name(ty) });
        }
    };
    let scalar = match ty {
        Vec2 | Vec3 | Vec4 => Float,
        IVec2 | IVec3 | IVec4 => Int,
        UVec2 | UVec3 | UVec4 => UInt,
        BVec2 | BVec3 | BVec4 => Bool,
        _ => unreachable!(),
    };
    Ok((idx * 4, scalar))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec;

    use crate::layout::type_size;

    #[test]
    fn offset_simple_struct() {
        let s = LpsType::Struct {
            name: Some(String::from("T")),
            members: vec![
                StructMember {
                    name: Some(String::from("a")),
                    ty: LpsType::Float,
                },
                StructMember {
                    name: Some(String::from("b")),
                    ty: LpsType::Float,
                },
            ],
        };
        assert_eq!(s.offset_for_path("a", LayoutRules::Std430, 0).unwrap(), 0);
        assert_eq!(s.offset_for_path("b", LayoutRules::Std430, 0).unwrap(), 4);
    }

    #[test]
    fn std430_vec3_then_float() {
        let s = LpsType::Struct {
            name: Some(String::from("T")),
            members: vec![
                StructMember {
                    name: Some(String::from("v")),
                    ty: LpsType::Vec3,
                },
                StructMember {
                    name: Some(String::from("f")),
                    ty: LpsType::Float,
                },
            ],
        };
        assert_eq!(type_size(&s, LayoutRules::Std430), 16);
        assert_eq!(s.offset_for_path("f", LayoutRules::Std430, 0).unwrap(), 12);
    }

    #[test]
    fn array_element_offset() {
        let a = LpsType::Array {
            element: Box::new(LpsType::Float),
            len: 10,
        };
        assert_eq!(
            a.offset_for_path("[3]", LayoutRules::Std430, 0).unwrap(),
            12
        );
    }

    #[test]
    fn texture2d_rejects_field_paths() {
        let tex = LpsType::Texture2D;
        assert!(matches!(
            tex.type_at_path("ptr"),
            Err(PathError::NotAField { ref ty }) if ty == "Texture2D"
        ));
        let err = tex
            .offset_for_path("ptr", LayoutRules::Std430, 0)
            .unwrap_err();
        assert!(matches!(err, PathError::NotAField { ref ty } if ty == "Texture2D"));
    }

    #[test]
    fn texture2d_index_only_inside_array() {
        let tex = LpsType::Texture2D;
        assert!(matches!(
            tex.type_at_path("[0]"),
            Err(PathError::NotIndexable { ref ty })
                if ty.contains("Texture2D")
        ));

        let arr = LpsType::Array {
            element: Box::new(LpsType::Texture2D),
            len: 2,
        };
        assert_eq!(arr.type_at_path("[0]").unwrap(), LpsType::Texture2D);
    }
}
