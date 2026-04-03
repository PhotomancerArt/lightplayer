# Phase 4: GlslData Core

## Scope

Implement GlslData struct with memory backing, path-based access, and conversion to/from GlslValue.

## Implementation Details

### 1. Create error type (lp-glsl-abi/src/glsl_data_error.rs)

```rust
use alloc::string::String;
use lpir::PathError;

#[derive(Clone, Debug, PartialEq)]
pub enum GlslDataError {
    Path(PathError),
    TypeMismatch {
        path: String,
        expected: String,
        got: String,
    },
    BufferTooSmall {
        expected: usize,
        got: usize,
    },
    NotImplemented(String),
}

impl core::fmt::Display for GlslDataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Path(e) => write!(f, "path error: {}", e),
            Self::TypeMismatch { path, expected, got } => {
                write!(f, "type mismatch at '{}': expected {}, got {}", path, expected, got)
            }
            Self::BufferTooSmall { expected, got } => {
                write!(f, "buffer too small: need {} bytes, got {}", expected, got)
            }
            Self::NotImplemented(msg) => write!(f, "not implemented: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GlslDataError {}
```

### 2. Create GlslData (lp-glsl-abi/src/glsl_data.rs)

```rust
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpir::{GlslType, LayoutRules, PathError};
use crate::{GlslValue, GlslDataError};

/// Memory-backed GLSL data with path-based access.
pub struct GlslData {
    ty: GlslType,
    rules: LayoutRules,
    data: Vec<u8>,
}

impl GlslData {
    /// Create zero-initialized data with std430 layout.
    pub fn new(ty: GlslType) -> Self {
        Self::with_rules(ty, LayoutRules::Std430)
    }
    
    /// Create with specific layout rules.
    pub fn with_rules(ty: GlslType, rules: LayoutRules) -> Self {
        let size = ty.size(rules);
        Self {
            ty,
            rules,
            data: vec![0u8; size],
        }
    }
    
    /// Create from a GlslValue tree.
    /// Validates that value shape matches type.
    pub fn from_value(ty: GlslType, value: &GlslValue) -> Result<Self, GlslDataError> {
        let mut data = Self::new(ty.clone());
        data.write_value(&ty, 0, value)?;
        Ok(data)
    }
    
    /// Write a value at given offset (recursive for structs/arrays).
    fn write_value(
        &mut self,
        ty: &GlslType,
        offset: usize,
        value: &GlslValue,
    ) -> Result<(), GlslDataError> {
        use GlslType::*;
        
        match (ty, value) {
            (Float, GlslValue::F32(v)) => {
                self.write_f32_at(offset, *v);
            }
            (Int, GlslValue::I32(v)) => {
                self.write_i32_at(offset, *v);
            }
            (UInt, GlslValue::U32(v)) => {
                self.write_u32_at(offset, *v);
            }
            (Bool, GlslValue::Bool(v)) => {
                self.write_bool_at(offset, *v);
            }
            (Vec2, GlslValue::Vec2(arr)) => {
                for (i, v) in arr.iter().enumerate() {
                    self.write_f32_at(offset + i * 4, *v);
                }
            }
            (Vec3, GlslValue::Vec3(arr)) => {
                for (i, v) in arr.iter().enumerate() {
                    self.write_f32_at(offset + i * 4, *v);
                }
            }
            (Vec4, GlslValue::Vec4(arr)) => {
                for (i, v) in arr.iter().enumerate() {
                    self.write_f32_at(offset + i * 4, *v);
                }
            }
            (Array { element, len }, GlslValue::Array(arr)) => {
                if arr.len() != *len as usize {
                    return Err(GlslDataError::TypeMismatch {
                        path: "".to_string(),
                        expected: format!("array[{}]", len),
                        got: format!("array[{}]", arr.len()),
                    });
                }
                let stride = element.size(self.rules);
                for (i, elem_val) in arr.iter().enumerate() {
                    self.write_value(element, offset + i * stride, elem_val)?;
                }
            }
            (Struct { members, .. }, GlslValue::Struct { fields, .. }) => {
                let mut field_map: alloc::collections::BTreeMap<&str, &GlslValue> = 
                    fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
                
                let mut member_offset = offset;
                for member in members {
                    let member_size = member.ty.size(self.rules);
                    let member_align = member.ty.alignment(self.rules);
                    member_offset = round_up(member_offset, member_align);
                    
                    let field_name = member.name.as_deref().unwrap_or("");
                    if let Some(field_val) = field_map.get(field_name) {
                        self.write_value(&member.ty, member_offset, field_val)?;
                    }
                    // If field not provided, leave as zero
                    
                    member_offset += member_size;
                }
            }
            (expected, got) => {
                return Err(GlslDataError::TypeMismatch {
                    path: "".to_string(),
                    expected: format!("{:?}", expected),
                    got: format!("{:?}", got),
                });
            }
        }
        Ok(())
    }
    
    /// Convert entire data block to GlslValue tree.
    pub fn to_value(&self) -> Result<GlslValue, GlslDataError> {
        self.read_value(&self.ty, 0)
    }
    
    /// Read a value at given offset (recursive).
    fn read_value(&self, ty: &GlslType, offset: usize) -> Result<GlslValue, GlslDataError> {
        use GlslType::*;
        
        match ty {
            Float => Ok(GlslValue::F32(self.read_f32_at(offset))),
            Int => Ok(GlslValue::I32(self.read_i32_at(offset))),
            UInt => Ok(GlslValue::U32(self.read_u32_at(offset))),
            Bool => Ok(GlslValue::Bool(self.read_bool_at(offset))),
            Vec2 => {
                let mut arr = [0.0; 2];
                for i in 0..2 {
                    arr[i] = self.read_f32_at(offset + i * 4);
                }
                Ok(GlslValue::Vec2(arr))
            }
            Vec3 => {
                let mut arr = [0.0; 3];
                for i in 0..3 {
                    arr[i] = self.read_f32_at(offset + i * 4);
                }
                Ok(GlslValue::Vec3(arr))
            }
            Vec4 => {
                let mut arr = [0.0; 4];
                for i in 0..4 {
                    arr[i] = self.read_f32_at(offset + i * 4);
                }
                Ok(GlslValue::Vec4(arr))
            }
            Array { element, len } => {
                let stride = element.size(self.rules);
                let mut elems = Vec::with_capacity(*len as usize);
                for i in 0..*len {
                    elems.push(self.read_value(element, offset + i as usize * stride)?);
                }
                Ok(GlslValue::Array(elems.into_boxed_slice()))
            }
            Struct { name, members } => {
                let mut fields = Vec::new();
                let mut member_offset = offset;
                for member in members {
                    let member_size = member.ty.size(self.rules);
                    let member_align = member.ty.alignment(self.rules);
                    member_offset = round_up(member_offset, member_align);
                    
                    let val = self.read_value(&member.ty, member_offset)?;
                    if let Some(name) = &member.name {
                        fields.push((name.clone(), val));
                    }
                    
                    member_offset += member_size;
                }
                Ok(GlslValue::Struct {
                    name: name.clone(),
                    fields,
                })
            }
            _ => Err(GlslDataError::NotImplemented(format!("{:?}", ty))),
        }
    }
    
    // === Path-based access ===
    
    /// Read value at path.
    pub fn get(&self, path: &str) -> Result<GlslValue, GlslDataError> {
        let offset = self.offset_of(path)?;
        let ty_at_path = self.ty.type_at_path(path).map_err(GlslDataError::Path)?;
        self.read_value(ty_at_path, offset)
    }
    
    /// Write value at path.
    pub fn set(&mut self, path: &str, value: GlslValue) -> Result<(), GlslDataError> {
        let offset = self.offset_of(path)?;
        let ty_at_path = self.ty.type_at_path(path).map_err(GlslDataError::Path)?;
        self.write_value(ty_at_path, offset, &value)
    }
    
    /// Get byte offset for a path.
    pub fn offset_of(&self, path: &str) -> Result<usize, GlslDataError> {
        self.ty.offset_for_path(path, self.rules, 0)
            .map_err(GlslDataError::Path)
    }
    
    // === Direct scalar access ===
    
    pub fn get_f32(&self, path: &str) -> Result<f32, GlslDataError> {
        let offset = self.offset_of(path)?;
        Ok(self.read_f32_at(offset))
    }
    
    pub fn set_f32(&mut self, path: &str, val: f32) -> Result<(), GlslDataError> {
        let offset = self.offset_of(path)?;
        self.write_f32_at(offset, val);
        Ok(())
    }
    
    pub fn get_i32(&self, path: &str) -> Result<i32, GlslDataError> {
        let offset = self.offset_of(path)?;
        Ok(self.read_i32_at(offset))
    }
    
    pub fn set_i32(&mut self, path: &str, val: i32) -> Result<(), GlslDataError> {
        let offset = self.offset_of(path)?;
        self.write_i32_at(offset, val);
        Ok(())
    }
    
    // === Raw access ===
    
    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
    
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }
    
    pub fn byte_len(&self) -> usize {
        self.data.len()
    }
    
    pub fn ty(&self) -> &GlslType {
        &self.ty
    }
    
    // === Private helpers ===
    
    fn read_f32_at(&self, offset: usize) -> f32 {
        f32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
    
    fn write_f32_at(&mut self, offset: usize, val: f32) {
        let bytes = val.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
    }
    
    fn read_i32_at(&self, offset: usize) -> i32 {
        i32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
    
    fn write_i32_at(&mut self, offset: usize, val: i32) {
        let bytes = val.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
    }
    
    fn read_u32_at(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
    
    fn write_u32_at(&mut self, offset: usize, val: u32) {
        let bytes = val.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&bytes);
    }
    
    fn read_bool_at(&self, offset: usize) -> bool {
        self.data[offset] != 0
    }
    
    fn write_bool_at(&mut self, offset: usize, val: bool) {
        self.data[offset] = if val { 1 } else { 0 };
    }
}

fn round_up(size: usize, alignment: usize) -> usize {
    ((size + alignment - 1) / alignment) * alignment
}
```

### 3. Update lib.rs exports

```rust
// lp-glsl-abi/src/lib.rs
pub mod glsl_data;
pub mod glsl_data_error;

pub use glsl_data::GlslData;
pub use glsl_data_error::GlslDataError;
```

## Code Organization

- Place read/write helpers at bottom of GlslData impl
- Place public API (new, from_value, to_value, get, set) at top
- Keep error types in separate file for clean imports

## Tests

```rust
#[test]
fn create_zero_initialized() {
    let data = GlslData::new(GlslType::Float);
    assert_eq!(data.byte_len(), 4);
    assert_eq!(data.to_value().unwrap(), GlslValue::F32(0.0));
}

#[test]
fn roundtrip_scalar() {
    let ty = GlslType::Float;
    let val = GlslValue::F32(3.14);
    let data = GlslData::from_value(ty, &val).unwrap();
    assert_eq!(data.to_value().unwrap(), val);
}

#[test]
fn roundtrip_vec3() {
    let ty = GlslType::Vec3;
    let val = GlslValue::Vec3([1.0, 2.0, 3.0]);
    let data = GlslData::from_value(ty, &val).unwrap();
    assert_eq!(data.to_value().unwrap(), val);
}

#[test]
fn roundtrip_array() {
    let ty = GlslType::Array {
        element: Box::new(GlslType::Float),
        len: 4,
    };
    let val = GlslValue::Array(vec![
        GlslValue::F32(1.0),
        GlslValue::F32(2.0),
        GlslValue::F32(3.0),
        GlslValue::F32(4.0),
    ].into_boxed_slice());
    let data = GlslData::from_value(ty, &val).unwrap();
    assert_eq!(data.to_value().unwrap(), val);
}

#[test]
fn path_access_scalar() {
    let ty = GlslType::Struct {
        name: Some("Test".to_string()),
        members: vec![
            StructMember { name: Some("x".to_string()), ty: GlslType::Float },
            StructMember { name: Some("y".to_string()), ty: GlslType::Float },
        ],
    };
    let mut data = GlslData::new(ty);
    data.set("x", GlslValue::F32(42.0)).unwrap();
    assert_eq!(data.get("x").unwrap(), GlslValue::F32(42.0));
}

#[test]
fn path_access_array_element() {
    let ty = GlslType::Array {
        element: Box::new(GlslType::Float),
        len: 4,
    };
    let mut data = GlslData::new(ty);
    data.set("[2]", GlslValue::F32(99.0)).unwrap();
    assert_eq!(data.get("[2]").unwrap(), GlslValue::F32(99.0));
    // Others still zero
    assert_eq!(data.get("[0]").unwrap(), GlslValue::F32(0.0));
}

#[test]
fn direct_scalar_access() {
    let ty = GlslType::Struct {
        name: Some("Test".to_string()),
        members: vec![
            StructMember { name: Some("value".to_string()), ty: GlslType::Float },
        ],
    };
    let mut data = GlslData::new(ty);
    data.set_f32("value", 123.0).unwrap();
    assert_eq!(data.get_f32("value").unwrap(), 123.0);
}

#[test]
fn type_mismatch_error() {
    let ty = GlslType::Float;
    let val = GlslValue::Vec3([1.0, 2.0, 3.0]);
    let err = GlslData::from_value(ty, &val).unwrap_err();
    assert!(matches!(err, GlslDataError::TypeMismatch { .. }));
}
```

## Validation

```bash
cargo check -p lp-glsl-abi
cargo test -p lp-glsl-abi -- --test-threads=1
```

## Notes

- Uses little-endian byte order (native for most targets)
- Struct to GlslValue uses field names; from GlslValue uses field names to match
- Missing fields in struct initialization are left as zero
- Array length mismatch is an error