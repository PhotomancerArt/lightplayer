//! [`GlslType`] path resolution: byte offsets and leaf types.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::layout::{array_stride, round_up, type_alignment, type_size};
use crate::metadata::{GlslType, LayoutRules, StructMember};
use crate::path::{PathParseError, PathSegment, parse_path};

/// Failure resolving a path against a [`GlslType`].
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
                write!(
                    f,
                    "field '{}' not found (available: {:?})",
                    field, available
                )
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len {})", index, len)
            }
            Self::NotIndexable { ty } => write!(f, "type `{}` is not indexable", ty),
            Self::NotAField { ty } => write!(f, "type `{}` has no fields", ty),
            Self::UnsupportedSwizzle(s) => write!(f, "unsupported swizzle `{}`", s),
            Self::TrailingPath { remaining } => {
                write!(f, "{} path segments remain after scalar", remaining)
            }
        }
    }
}

impl GlslType {
    /// Byte offset for `path` under `rules`, added to `base_offset`.
    pub fn offset_for_path(
        &self,
        path: &str,
        rules: LayoutRules,
        base_offset: usize,
    ) -> Result<usize, PathError> {
        let segs = parse_path(path).map_err(PathError::Parse)?;
        offset_walk(self, &segs, rules, base_offset)
    }

    /// Leaf [`GlslType`] after following `path` (owned; vector components yield scalars).
    pub fn type_at_path(&self, path: &str) -> Result<GlslType, PathError> {
        let segs = parse_path(path).map_err(PathError::Parse)?;
        type_walk(self, &segs)
    }
}

fn type_walk(ty: &GlslType, segs: &[PathSegment]) -> Result<GlslType, PathError> {
    if segs.is_empty() {
        return Ok(ty.clone());
    }
    match (&segs[0], ty) {
        (PathSegment::Field(name), GlslType::Struct { members, .. }) => {
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
            PathSegment::Field(name),
            vec_ty @ (GlslType::Vec2
            | GlslType::Vec3
            | GlslType::Vec4
            | GlslType::IVec2
            | GlslType::IVec3
            | GlslType::IVec4
            | GlslType::UVec2
            | GlslType::UVec3
            | GlslType::UVec4
            | GlslType::BVec2
            | GlslType::BVec3
            | GlslType::BVec4),
        ) => {
            let scalar_ty = vector_scalar_type(vec_ty, name)?;
            type_walk(&scalar_ty, &segs[1..])
        }
        (PathSegment::Index(idx), GlslType::Array { element, len }) => {
            if *idx >= *len as usize {
                return Err(PathError::IndexOutOfBounds {
                    index: *idx,
                    len: *len,
                });
            }
            type_walk(element, &segs[1..])
        }
        (PathSegment::Field(_name), GlslType::Void) => Err(PathError::NotAField {
            ty: String::from("void"),
        }),
        (
            PathSegment::Field(_name),
            scalar @ (GlslType::Float | GlslType::Int | GlslType::UInt | GlslType::Bool),
        ) => Err(PathError::NotAField {
            ty: format!("{:?}", scalar),
        }),
        (PathSegment::Field(name), _) => Err(PathError::FieldNotFound {
            field: name.clone(),
            available: field_names(ty),
        }),
        (PathSegment::Index(_), _) => Err(PathError::NotIndexable { ty: type_name(ty) }),
    }
}

fn offset_walk(
    ty: &GlslType,
    segs: &[PathSegment],
    rules: LayoutRules,
    base: usize,
) -> Result<usize, PathError> {
    if segs.is_empty() {
        return Ok(base);
    }
    match (&segs[0], ty) {
        (PathSegment::Field(name), GlslType::Struct { members, .. }) => {
            let (off, sub) = struct_field_offset(members, name, rules, base)?;
            offset_walk(sub, &segs[1..], rules, off)
        }
        (
            PathSegment::Field(name),
            vec_ty @ (GlslType::Vec2
            | GlslType::Vec3
            | GlslType::Vec4
            | GlslType::IVec2
            | GlslType::IVec3
            | GlslType::IVec4
            | GlslType::UVec2
            | GlslType::UVec3
            | GlslType::UVec4
            | GlslType::BVec2
            | GlslType::BVec3
            | GlslType::BVec4),
        ) => {
            let (comp_off, scalar_ty) = vector_component_meta(vec_ty, name)?;
            if !segs[1..].is_empty() {
                return match scalar_ty {
                    GlslType::Float | GlslType::Int | GlslType::UInt | GlslType::Bool => {
                        Err(PathError::TrailingPath {
                            remaining: segs.len() - 1,
                        })
                    }
                    _ => offset_walk(&scalar_ty, &segs[1..], rules, base + comp_off),
                };
            }
            Ok(base + comp_off)
        }
        (PathSegment::Index(idx), GlslType::Array { element, len }) => {
            if *idx >= *len as usize {
                return Err(PathError::IndexOutOfBounds {
                    index: *idx,
                    len: *len,
                });
            }
            let stride = array_stride(element, rules);
            offset_walk(element, &segs[1..], rules, base + idx * stride)
        }
        (PathSegment::Index(_), _) => Err(PathError::NotIndexable { ty: type_name(ty) }),
        (PathSegment::Field(name), _) => Err(PathError::FieldNotFound {
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
) -> Result<(usize, &'a GlslType), PathError> {
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

fn field_names(ty: &GlslType) -> Vec<String> {
    match ty {
        GlslType::Struct { members, .. } => members.iter().filter_map(|m| m.name.clone()).collect(),
        _ => Vec::new(),
    }
}

fn type_name(ty: &GlslType) -> String {
    format!("{:?}", ty)
}

fn vector_scalar_type(ty: &GlslType, name: &str) -> Result<GlslType, PathError> {
    Ok(vector_component_meta(ty, name)?.1)
}

fn vector_component_meta(ty: &GlslType, name: &str) -> Result<(usize, GlslType), PathError> {
    use GlslType::*;
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

    #[test]
    fn offset_simple_struct() {
        let s = GlslType::Struct {
            name: Some(String::from("T")),
            members: vec![
                StructMember {
                    name: Some(String::from("a")),
                    ty: GlslType::Float,
                },
                StructMember {
                    name: Some(String::from("b")),
                    ty: GlslType::Float,
                },
            ],
        };
        assert_eq!(s.offset_for_path("a", LayoutRules::Std430, 0).unwrap(), 0);
        assert_eq!(s.offset_for_path("b", LayoutRules::Std430, 0).unwrap(), 4);
    }

    #[test]
    fn std430_vec3_then_float() {
        let s = GlslType::Struct {
            name: Some(String::from("T")),
            members: vec![
                StructMember {
                    name: Some(String::from("v")),
                    ty: GlslType::Vec3,
                },
                StructMember {
                    name: Some(String::from("f")),
                    ty: GlslType::Float,
                },
            ],
        };
        assert_eq!(s.size(LayoutRules::Std430), 16);
        assert_eq!(s.offset_for_path("f", LayoutRules::Std430, 0).unwrap(), 12);
    }

    #[test]
    fn array_element_offset() {
        let a = GlslType::Array {
            element: Box::new(GlslType::Float),
            len: 10,
        };
        assert_eq!(
            a.offset_for_path("[3]", LayoutRules::Std430, 0).unwrap(),
            12
        );
    }
}
