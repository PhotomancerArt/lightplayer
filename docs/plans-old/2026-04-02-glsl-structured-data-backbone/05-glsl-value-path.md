# Phase 5: GlslValue Struct Support and Path Access

## Scope

Add Struct variant to GlslValue, implement path access methods.

## Implementation Details

### 1. Add Struct variant to GlslValue (lpvm/src/glsl_value.rs)

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum GlslValue {
    // Existing variants...
    I32(i32), U32(u32), F32(f32), Bool(bool),
    Vec2([f32; 2]), Vec3([f32; 3]), Vec4([f32; 4]),
    IVec2([i32; 2]), IVec3([i32; 3]), IVec4([i32; 4]),
    UVec2([u32; 2]), UVec3([u32; 3]), UVec4([u32; 4]),
    BVec2([bool; 2]), BVec3([bool; 3]), BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]), Mat3x3([[f32; 3]; 3]), Mat4x4([[f32; 4]; 4]),
    
    // Existing array variant
    Array(Box<[GlslValue]>),
    
    // NEW: Struct variant
    Struct {
        name: Option<String>,
        fields: Vec<(String, GlslValue)>,
    },
}
```

### 2. Add error type for GlslValue operations

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum GlslValueError {
    NotAStruct { got: String },
    NotAnArray { got: String },
    NotIndexable { got: String },
    FieldNotFound { name: String, available: Vec<String> },
    IndexOutOfBounds { index: usize, len: usize },
    PathError { msg: String },
}

impl core::fmt::Display for GlslValueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAStruct { got } => write!(f, "expected struct, got {}", got),
            Self::NotAnArray { got } => write!(f, "expected array, got {}", got),
            Self::NotIndexable { got } => write!(f, "not indexable: {}", got),
            Self::FieldNotFound { name, available } => {
                write!(f, "field '{}' not found, available: {:?}", name, available)
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len {})", index, len)
            }
            Self::PathError { msg } => write!(f, "path error: {}", msg),
        }
    }
}
```

### 3. Add field/array accessors

```rust
impl GlslValue {
    /// Get field by name (for struct values).
    pub fn get_field(&self, name: &str) -> Option<&GlslValue> {
        match self {
            Self::Struct { fields, .. } => {
                fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
            }
            _ => None,
        }
    }
    
    /// Get mutable field by name.
    pub fn get_field_mut(&mut self, name: &str) -> Option<&mut GlslValue> {
        match self {
            Self::Struct { fields, .. } => {
                fields.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
            }
            _ => None,
        }
    }
    
    /// Set field by name.
    pub fn set_field(&mut self, name: &str, value: GlslValue) -> Result<(), GlslValueError> {
        match self {
            Self::Struct { fields, .. } => {
                if let Some((_, existing)) = fields.iter_mut().find(|(n, _)| n == name) {
                    *existing = value;
                    Ok(())
                } else {
                    let available = fields.iter().map(|(n, _)| n.clone()).collect();
                    Err(GlslValueError::FieldNotFound {
                        name: name.to_string(),
                        available,
                    })
                }
            }
            _ => Err(GlslValueError::NotAStruct { got: self.type_name() }),
        }
    }
    
    /// Get array element.
    pub fn get_index(&self, index: usize) -> Option<&GlslValue> {
        match self {
            Self::Array(arr) => arr.get(index),
            _ => None,
        }
    }
    
    /// Get mutable array element.
    pub fn get_index_mut(&mut self, index: usize) -> Option<&mut GlslValue> {
        match self {
            Self::Array(arr) => {
                if let Some(slice) = arr.get_mut(index) {
                    Some(slice)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    /// Set array element.
    pub fn set_index(&mut self, index: usize, value: GlslValue) -> Result<(), GlslValueError> {
        match self {
            Self::Array(arr) => {
                if index < arr.len() {
                    arr[index] = value;
                    Ok(())
                } else {
                    Err(GlslValueError::IndexOutOfBounds {
                        index,
                        len: arr.len(),
                    })
                }
            }
            _ => Err(GlslValueError::NotAnArray { got: self.type_name() }),
        }
    }
    
    /// Helper to get type name for errors.
    fn type_name(&self) -> String {
        match self {
            Self::I32(_) => "int".to_string(),
            Self::U32(_) => "uint".to_string(),
            Self::F32(_) => "float".to_string(),
            Self::Bool(_) => "bool".to_string(),
            Self::Vec2(_) => "vec2".to_string(),
            Self::Vec3(_) => "vec3".to_string(),
            Self::Vec4(_) => "vec4".to_string(),
            Self::Array(_) => "array".to_string(),
            Self::Struct { name, .. } => format!("struct {}", name.as_deref().unwrap_or("(anonymous)")),
            _ => "unknown".to_string(),
        }
    }
}
```

### 4. Add path access to GlslValue

```rust
impl GlslValue {
    /// Get value at path.
    /// Supports: field.subfield, array[3], lights[3].color.r
    pub fn get_path(&self, path: &str) -> Result<&GlslValue, GlslValueError> {
        if path.is_empty() {
            return Ok(self);
        }
        
        let segments = parse_value_path(path)?;
        let mut current = self;
        
        for segment in segments {
            match segment {
                PathSegment::Field(name) => {
                    current = current.get_field(&name)
                        .ok_or_else(|| GlslValueError::FieldNotFound {
                            name: name.clone(),
                            available: match current {
                                GlslValue::Struct { fields, .. } => 
                                    fields.iter().map(|(n, _)| n.clone()).collect(),
                                _ => vec![],
                            },
                        })?;
                }
                PathSegment::Index(idx) => {
                    current = current.get_index(idx)
                        .ok_or_else(|| {
                            let len = match current {
                                GlslValue::Array(arr) => arr.len(),
                                _ => 0,
                            };
                            GlslValueError::IndexOutOfBounds { index: idx, len }
                        })?;
                }
            }
        }
        
        Ok(current)
    }
    
    /// Get mutable reference at path.
    pub fn get_path_mut(&mut self, path: &str) -> Result<&mut GlslValue, GlslValueError> {
        if path.is_empty() {
            return Ok(self);
        }
        
        let segments = parse_value_path(path)?;
        let mut current = self;
        
        // Navigate to parent, then get final component
        // (This is tricky with borrow checker - use recursion instead)
        Self::get_path_mut_recursive(current, &segments)
    }
    
    fn get_path_mut_recursive<'a>(
        val: &'a mut GlslValue,
        segments: &[PathSegment],
    ) -> Result<&'a mut GlslValue, GlslValueError> {
        if segments.is_empty() {
            return Ok(val);
        }
        
        let (first, rest) = (&segments[0], &segments[1..]);
        
        match first {
            PathSegment::Field(name) => {
                let next = val.get_field_mut(name)
                    .ok_or_else(|| GlslValueError::FieldNotFound {
                        name: name.clone(),
                        available: match val {
                            GlslValue::Struct { fields, .. } => 
                                fields.iter().map(|(n, _)| n.clone()).collect(),
                            _ => vec![],
                        },
                    })?;
                Self::get_path_mut_recursive(next, rest)
            }
            PathSegment::Index(idx) => {
                let next = val.get_index_mut(*idx)
                    .ok_or_else(|| {
                        let len = match val {
                            GlslValue::Array(arr) => arr.len(),
                            _ => 0,
                        };
                        GlslValueError::IndexOutOfBounds { index: *idx, len }
                    })?;
                Self::get_path_mut_recursive(next, rest)
            }
        }
    }
    
    /// Set value at path.
    /// Creates intermediate structs/arrays if needed? No - GLSL requires strict matching.
    pub fn set_path(&mut self, path: &str, value: GlslValue) -> Result<(), GlslValueError> {
        if path.is_empty() {
            return Err(GlslValueError::PathError { 
                msg: "cannot set empty path".to_string() 
            });
        }
        
        let segments = parse_value_path(path)?;
        let mut current = self;
        
        // Navigate to parent
        let (parent_segments, final_segment) = segments.split_at(segments.len() - 1);
        
        for segment in parent_segments {
            match segment {
                PathSegment::Field(name) => {
                    current = current.get_field_mut(name)
                        .ok_or_else(|| GlslValueError::FieldNotFound {
                            name: name.clone(),
                            available: match current {
                                GlslValue::Struct { fields, .. } => 
                                    fields.iter().map(|(n, _)| n.clone()).collect(),
                                _ => vec![],
                            },
                        })?;
                }
                PathSegment::Index(idx) => {
                    current = current.get_index_mut(*idx)
                        .ok_or_else(|| {
                            let len = match current {
                                GlslValue::Array(arr) => arr.len(),
                                _ => 0,
                            };
                            GlslValueError::IndexOutOfBounds { index: *idx, len }
                        })?;
                }
            }
        }
        
        // Set final component
        match (&final_segment[0], value) {
            (PathSegment::Field(name), val) => {
                current.set_field(name, val)
            }
            (PathSegment::Index(idx), val) => {
                current.set_index(*idx, val)
            }
        }
    }
}

// Path parsing for GlslValue (simpler version than GlslData path)
#[derive(Clone, Debug, PartialEq)]
enum PathSegment {
    Field(String),
    Index(usize),
}

fn parse_value_path(path: &str) -> Result<Vec<PathSegment>, GlslValueError> {
    if path.is_empty() {
        return Ok(vec![]);
    }
    
    // Simple parser: split by . and []
    // This is less strict than GlslData path parser
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars().peekable();
    
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Field(current));
                    current = String::new();
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Field(current));
                    current = String::new();
                }
                // Parse number until ]
                let mut num_str = String::new();
                for c in &mut chars {
                    if c == ']' {
                        break;
                    }
                    if c.is_ascii_digit() {
                        num_str.push(c);
                    } else {
                        return Err(GlslValueError::PathError {
                            msg: format!("invalid character '{}' in index", c),
                        });
                    }
                }
                let idx = num_str.parse::<usize>()
                    .map_err(|_| GlslValueError::PathError {
                        msg: format!("invalid index: {}", num_str),
                    })?;
                segments.push(PathSegment::Index(idx));
            }
            _ => current.push(c),
        }
    }
    
    if !current.is_empty() {
        segments.push(PathSegment::Field(current));
    }
    
    Ok(segments)
}
```

### 5. Add type introspection

```rust
impl GlslValue {
    /// Infer GlslType from this value.
    /// Useful for validation and round-trip testing.
    pub fn glsl_type(&self) -> lpir::GlslType {
        use lpir::GlslType::*;
        match self {
            Self::I32(_) => Int,
            Self::U32(_) => UInt,
            Self::F32(_) => Float,
            Self::Bool(_) => Bool,
            Self::Vec2(_) => Vec2,
            Self::Vec3(_) => Vec3,
            Self::Vec4(_) => Vec4,
            Self::IVec2(_) => IVec2,
            Self::IVec3(_) => IVec3,
            Self::IVec4(_) => IVec4,
            Self::UVec2(_) => UVec2,
            Self::UVec3(_) => UVec3,
            Self::UVec4(_) => UVec4,
            Self::BVec2(_) => BVec2,
            Self::BVec3(_) => BVec3,
            Self::BVec4(_) => BVec4,
            Self::Mat2x2(_) => Mat2,
            Self::Mat3x3(_) => Mat3,
            Self::Mat4x4(_) => Mat4,
            Self::Array(arr) => {
                if arr.is_empty() {
                    Array { element: Box::new(Void), len: 0 }
                } else {
                    let elem_type = arr[0].glsl_type();
                    Array { element: Box::new(elem_type), len: arr.len() as u32 }
                }
            }
            Self::Struct { name, fields } => {
                Struct {
                    name: name.clone(),
                    members: fields.iter()
                        .map(|(n, v)| lpir::StructMember {
                            name: Some(n.clone()),
                            ty: v.glsl_type(),
                        })
                        .collect(),
                }
            }
        }
    }
}
```

## Code Organization

- Place Struct variant in GlslValue enum
- Place accessor methods (get_field, set_field, etc.) in main impl block
- Place path methods (get_path, set_path) in their own impl block
- Place parse_value_path at bottom of file (private helper)

## Tests

```rust
#[test]
fn struct_creation() {
    let s = GlslValue::Struct {
        name: Some("Light".to_string()),
        fields: vec![
            ("position".to_string(), GlslValue::Vec3([1.0, 2.0, 3.0])),
            ("intensity".to_string(), GlslValue::F32(0.8)),
        ],
    };
    assert_eq!(s.get_field("position"), Some(&GlslValue::Vec3([1.0, 2.0, 3.0])));
}

#[test]
fn struct_set_field() {
    let mut s = GlslValue::Struct {
        name: Some("Test".to_string()),
        fields: vec![
            ("x".to_string(), GlslValue::F32(0.0)),
        ],
    };
    s.set_field("x", GlslValue::F32(42.0)).unwrap();
    assert_eq!(s.get_field("x"), Some(&GlslValue::F32(42.0)));
}

#[test]
fn path_access_nested() {
    let s = GlslValue::Struct {
        name: Some("Scene".to_string()),
        fields: vec![
            ("light".to_string(), GlslValue::Struct {
                name: Some("Light".to_string()),
                fields: vec![
                    ("position".to_string(), GlslValue::Vec3([1.0, 2.0, 3.0])),
                ],
            }),
        ],
    };
    let pos = s.get_path("light.position").unwrap();
    assert_eq!(pos, &GlslValue::Vec3([1.0, 2.0, 3.0]));
}

#[test]
fn path_access_array_element() {
    let arr = GlslValue::Array(vec![
        GlslValue::F32(1.0),
        GlslValue::F32(2.0),
        GlslValue::F32(3.0),
    ].into_boxed_slice());
    assert_eq!(arr.get_path("[1]").unwrap(), &GlslValue::F32(2.0));
}

#[test]
fn path_access_complex() {
    let s = GlslValue::Struct {
        name: Some("Scene".to_string()),
        fields: vec![
            ("lights".to_string(), GlslValue::Array(vec![
                GlslValue::Struct {
                    name: Some("Light".to_string()),
                    fields: vec![
                        ("color".to_string(), GlslValue::Vec3([1.0, 0.0, 0.0])),
                    ],
                },
                GlslValue::Struct {
                    name: Some("Light".to_string()),
                    fields: vec![
                        ("color".to_string(), GlslValue::Vec3([0.0, 1.0, 0.0])),
                    ],
                },
            ].into_boxed_slice())),
        ],
    };
    let color = s.get_path("lights[1].color").unwrap();
    assert_eq!(color, &GlslValue::Vec3([0.0, 1.0, 0.0]));
}

#[test]
fn set_path_nested() {
    let mut s = GlslValue::Struct {
        name: Some("Scene".to_string()),
        fields: vec![
            ("light".to_string(), GlslValue::Struct {
                name: Some("Light".to_string()),
                fields: vec![
                    ("position".to_string(), GlslValue::Vec3([0.0, 0.0, 0.0])),
                ],
            }),
        ],
    };
    s.set_path("light.position", GlslValue::Vec3([1.0, 2.0, 3.0])).unwrap();
    assert_eq!(s.get_path("light.position").unwrap(), &GlslValue::Vec3([1.0, 2.0, 3.0]));
}

#[test]
fn field_not_found_error() {
    let s = GlslValue::Struct {
        name: Some("Test".to_string()),
        fields: vec![
            ("x".to_string(), GlslValue::F32(1.0)),
        ],
    };
    let err = s.get_path("y").unwrap_err();
    assert!(matches!(err, GlslValueError::FieldNotFound { name, .. } if name == "y"));
}

#[test]
fn index_out_of_bounds_error() {
    let arr = GlslValue::Array(vec![
        GlslValue::F32(1.0),
    ].into_boxed_slice());
    let err = arr.get_path("[5]").unwrap_err();
    assert!(matches!(err, GlslValueError::IndexOutOfBounds { index: 5, len: 1 }));
}

#[test]
fn glsl_type_roundtrip() {
    let val = GlslValue::Struct {
        name: Some("Light".to_string()),
        fields: vec![
            ("position".to_string(), GlslValue::Vec3([1.0, 2.0, 3.0])),
            ("intensity".to_string(), GlslValue::F32(0.5)),
        ],
    };
    let ty = val.glsl_type();
    // Verify type matches structure
    match ty {
        lpir::GlslType::Struct { name, members } => {
            assert_eq!(name, Some("Light".to_string()));
            assert_eq!(members.len(), 2);
        }
        _ => panic!("expected struct type"),
    }
}
```

## Validation

```bash
cargo check -p lpvm
cargo test -p lpvm -- glsl_value
```

## Notes

- GlslValue path parsing is separate from GlslData path parsing
- GlslValue uses owned strings; path parser allocates
- get_path_mut uses recursive helper to satisfy borrow checker
- set_path validates path exists - no auto-creation of intermediate nodes