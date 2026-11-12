# Phase 3: Path Parser

## Scope

Implement path tokenization and parsing for field access and array indexing.

## Implementation Details

### 1. Create path module (lpir/src/path.rs)

```rust
//! Path parsing for GLSL data access.
//! Syntax: field.subfield, array[3], combined[0].field

use alloc::string::String;
use alloc::vec::Vec;

/// A path segment (field or index).
#[derive(Clone, Debug, PartialEq)]
pub enum PathSegment {
    Field(String),
    Index(usize),
}

/// Parse a path string into segments.
/// Returns Err with reason if parsing fails.
pub fn parse_path(path: &str) -> Result<Vec<PathSegment>, PathParseError> {
    if path.is_empty() {
        return Err(PathParseError::EmptyPath);
    }
    
    let mut segments = Vec::new();
    let mut chars = path.chars().peekable();
    
    // First token must be a field name (no leading . or [)
    let first_field = parse_identifier(&mut chars)?;
    segments.push(PathSegment::Field(first_field));
    
    // Continue parsing .field or [n] until exhausted
    while chars.peek().is_some() {
        match chars.peek() {
            Some('.') => {
                chars.next(); // consume '.'
                let field = parse_identifier(&mut chars)?;
                segments.push(PathSegment::Field(field));
            }
            Some('[') => {
                chars.next(); // consume '['
                let index = parse_number(&mut chars)?;
                // Expect closing ']'
                match chars.next() {
                    Some(']') => {}
                    _ => return Err(PathParseError::MissingBracket),
                }
                segments.push(PathSegment::Index(index));
            }
            Some(c) => {
                return Err(PathParseError::UnexpectedChar(*c));
            }
            None => break,
        }
    }
    
    Ok(segments)
}

fn parse_identifier<I>(chars: &mut core::iter::Peekable<I>) -> Result<String, PathParseError>
where
    I: Iterator<Item = char>,
{
    let mut ident = String::new();
    
    // First char must be alphabetic or underscore
    match chars.peek() {
        Some(c) if c.is_alphabetic() || *c == '_' => {
            ident.push(chars.next().unwrap());
        }
        Some(c) => return Err(PathParseError::InvalidIdentifierStart(*c)),
        None => return Err(PathParseError::EmptyIdentifier),
    }
    
    // Rest can be alphanumeric or underscore
    while let Some(c) = chars.peek() {
        if c.is_alphanumeric() || *c == '_' {
            ident.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    
    Ok(ident)
}

fn parse_number<I>(chars: &mut core::iter::Peekable<I>) -> Result<usize, PathParseError>
where
    I: Iterator<Item = char>,
{
    let mut num_str = String::new();
    
    while let Some(c) = chars.peek() {
        if c.is_ascii_digit() {
            num_str.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    
    if num_str.is_empty() {
        return Err(PathParseError::EmptyIndex);
    }
    
    num_str.parse()
        .map_err(|_| PathParseError::InvalidNumber(num_str))
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathParseError {
    EmptyPath,
    EmptyIdentifier,
    EmptyIndex,
    InvalidIdentifierStart(char),
    InvalidNumber(String),
    MissingBracket,
    UnexpectedChar(char),
}

impl core::fmt::Display for PathParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "path is empty"),
            Self::EmptyIdentifier => write!(f, "expected field name"),
            Self::EmptyIndex => write!(f, "expected array index"),
            Self::InvalidIdentifierStart(c) => write!(f, "invalid start of identifier: '{}'", c),
            Self::InvalidNumber(s) => write!(f, "invalid number: '{}'", s),
            Self::MissingBracket => write!(f, "missing closing ']'"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character: '{}'", c),
        }
    }
}
```

### 2. Add path resolution to GlslType

```rust
// In lpir/src/glsl_metadata.rs
use crate::path::{parse_path, PathSegment, PathParseError};

#[derive(Clone, Debug, PartialEq)]
pub enum PathError {
    Parse(PathParseError),
    FieldNotFound { path: String, field: String },
    IndexOutOfBounds { path: String, index: usize, len: u32 },
    NotIndexable { path: String, ty: GlslType },
    NotAField { path: String, ty: GlslType },
}

impl GlslType {
    /// Compute byte offset for a path.
    pub fn offset_for_path(
        &self, 
        path: &str, 
        rules: LayoutRules, 
        base_offset: usize
    ) -> Result<usize, PathError> {
        let segments = parse_path(path).map_err(PathError::Parse)?;
        let mut offset = base_offset;
        let mut current_ty = self;
        
        for segment in segments {
            match segment {
                PathSegment::Field(name) => {
                    let (field_offset, field_ty) = 
                        self.resolve_field(current_ty, &name, rules, offset)?;
                    offset = field_offset;
                    current_ty = field_ty;
                }
                PathSegment::Index(idx) => {
                    let (elem_offset, elem_ty) = 
                        self.resolve_index(current_ty, idx, rules, offset)?;
                    offset = elem_offset;
                    current_ty = elem_ty;
                }
            }
        }
        
        Ok(offset)
    }
    
    fn resolve_field(
        &self,
        ty: &GlslType,
        name: &str,
        rules: LayoutRules,
        base: usize,
    ) -> Result<(usize, &GlslType), PathError> {
        match ty {
            GlslType::Struct { members, .. } => {
                let mut offset = base;
                for member in members {
                    let align = member.ty.alignment(rules);
                    offset = crate::layout::round_up(offset, align);
                    
                    if member.name.as_deref() == Some(name) {
                        return Ok((offset, &member.ty));
                    }
                    
                    offset += member.ty.size(rules);
                }
                Err(PathError::FieldNotFound {
                    path: name.to_string(),
                    field: name.to_string(),
                })
            }
            // Vectors allow component access: pos.x, color.rgb
            GlslType::Vec2 | GlslType::Vec3 | GlslType::Vec4 |
            GlslType::IVec2 | GlslType::IVec3 | GlslType::IVec4 |
            GlslType::UVec2 | GlslType::UVec3 | GlslType::UVec4 |
            GlslType::BVec2 | GlslType::BVec3 | GlslType::BVec4 => {
                // Component access - returns same type (handled at value level)
                // For offset computation, we just need to validate the component
                let valid = match ty {
                    GlslType::Vec2 | GlslType::IVec2 | GlslType::UVec2 | GlslType::BVec2 => 
                        ["x", "y", "r", "g", "s", "t"].contains(&name),
                    GlslType::Vec3 | GlslType::IVec3 | GlslType::UVec3 | GlslType::BVec3 => 
                        ["x", "y", "z", "r", "g", "b", "s", "t", "p"].contains(&name),
                    GlslType::Vec4 | GlslType::IVec4 | GlslType::UVec4 | GlslType::BVec4 => 
                        ["x", "y", "z", "w", "r", "g", "b", "a", "s", "t", "p", "q"].contains(&name),
                    _ => false,
                };
                if !valid {
                    return Err(PathError::FieldNotFound {
                        path: name.to_string(),
                        field: name.to_string(),
                    });
                }
                // Component offset is computed at value extraction time
                Ok((base, ty))
            }
            _ => Err(PathError::NotAField {
                path: name.to_string(),
                ty: ty.clone(),
            }),
        }
    }
    
    fn resolve_index(
        &self,
        ty: &GlslType,
        idx: usize,
        rules: LayoutRules,
        base: usize,
    ) -> Result<(usize, &GlslType), PathError> {
        match ty {
            GlslType::Array { element, len } => {
                if idx >= *len as usize {
                    return Err(PathError::IndexOutOfBounds {
                        path: format!("[{}]", idx),
                        index: idx,
                        len: *len,
                    });
                }
                let stride = crate::layout::round_up(
                    element.size(rules),
                    element.alignment(rules)
                );
                Ok((base + idx * stride, element.as_ref()))
            }
            _ => Err(PathError::NotIndexable {
                path: format!("[{}]", idx),
                ty: ty.clone(),
            }),
        }
    }
}
```

## Code Organization

- path.rs is a new module - put parsing at top, errors at bottom
- Add path module to lib.rs exports
- Place resolve_field, resolve_index as private helpers in glsl_metadata.rs
- Keep offset_for_path public

## Tests

```rust
#[test]
fn parse_simple_field() {
    let segs = parse_path("position").unwrap();
    assert_eq!(segs, vec![PathSegment::Field("position".to_string())]);
}

#[test]
fn parse_nested_field() {
    let segs = parse_path("light.position.x").unwrap();
    assert_eq!(segs, vec![
        PathSegment::Field("light".to_string()),
        PathSegment::Field("position".to_string()),
        PathSegment::Field("x".to_string()),
    ]);
}

#[test]
fn parse_array_index() {
    let segs = parse_path("lights[3]").unwrap();
    assert_eq!(segs, vec![
        PathSegment::Field("lights".to_string()),
        PathSegment::Index(3),
    ]);
}

#[test]
fn parse_complex_path() {
    let segs = parse_path("lights[3].color.r").unwrap();
    assert_eq!(segs, vec![
        PathSegment::Field("lights".to_string()),
        PathSegment::Index(3),
        PathSegment::Field("color".to_string()),
        PathSegment::Field("r".to_string()),
    ]);
}

#[test]
fn parse_error_empty_path() {
    assert!(matches!(parse_path(""), Err(PathParseError::EmptyPath)));
}

#[test]
fn parse_error_missing_bracket() {
    assert!(matches!(parse_path("lights[3"), Err(PathParseError::MissingBracket)));
}

#[test]
fn offset_simple_struct() {
    let s = GlslType::Struct {
        name: Some("Test".to_string()),
        members: vec![
            StructMember { name: Some("a".to_string()), ty: GlslType::Float },
            StructMember { name: Some("b".to_string()), ty: GlslType::Float },
        ],
    };
    assert_eq!(s.offset_for_path("a", LayoutRules::Std430, 0).unwrap(), 0);
    assert_eq!(s.offset_for_path("b", LayoutRules::Std430, 0).unwrap(), 4);
}

#[test]
fn offset_array_element() {
    let arr = GlslType::Array {
        element: Box::new(GlslType::Float),
        len: 10,
    };
    assert_eq!(arr.offset_for_path("[0]", LayoutRules::Std430, 0).unwrap(), 0);
    assert_eq!(arr.offset_for_path("[3]", LayoutRules::Std430, 0).unwrap(), 12);
}

#[test]
fn error_index_out_of_bounds() {
    let arr = GlslType::Array {
        element: Box::new(GlslType::Float),
        len: 4,
    };
    let err = arr.offset_for_path("[10]", LayoutRules::Std430, 0).unwrap_err();
    assert!(matches!(err, PathError::IndexOutOfBounds { index: 10, len: 4, .. }));
}
```

## Validation

```bash
cargo check -p lpir
cargo test -p lpir path::
cargo test -p lpir offset_for_path
```

## Notes

- Path parsing is separate from offset computation for testability
- Vector component access (.x, .y, etc.) is validated but offset is base (component extracted later)
- Array index is validated at offset computation time, not parse time