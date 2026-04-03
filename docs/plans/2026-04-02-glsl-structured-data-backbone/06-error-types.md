# Phase 6: Error Types and Integration

## Scope

Finalize error types, add integration tests, ensure all components work together.

## Implementation Details

### 1. Ensure error types implement Display (already done in previous phases)

Verify in:

- `lpir/src/path.rs` - `PathParseError` and `PathError`
- `lp-glsl-abi/src/glsl_data_error.rs` - `GlslDataError`
- `lp-glsl-abi/src/glsl_value.rs` - `GlslValueError`

### 2. Add From impls for error conversion

```rust
// In lp-glsl-abi/src/glsl_data_error.rs
impl From<PathError> for GlslDataError {
    fn from(e: PathError) -> Self {
        Self::Path(e)
    }
}

impl From<GlslValueError> for GlslDataError {
    fn from(e: GlslValueError) -> Self {
        // Convert to string representation
        Self::NotImplemented(format!("value error: {}", e))
    }
}
```

### 3. Create integration tests file

Create `lp-glsl-abi/tests/roundtrip_tests.rs`:

```rust
use lp_glsl_abi::{GlslData, GlslValue};
use lpir::{GlslType, LayoutRules, StructMember};

#[test]
fn scalar_roundtrip() {
    for (ty, val) in [
        (GlslType::Float, GlslValue::F32(3.14)),
        (GlslType::Int, GlslValue::I32(-42)),
        (GlslType::UInt, GlslValue::U32(123)),
        (GlslType::Bool, GlslValue::Bool(true)),
    ] {
        let data = GlslData::from_value(ty.clone(), &val).unwrap();
        let got = data.to_value().unwrap();
        assert_eq!(got, val, "roundtrip failed for {:?}", ty);
    }
}

#[test]
fn vector_roundtrip() {
    for (ty, val) in [
        (GlslType::Vec2, GlslValue::Vec2([1.0, 2.0])),
        (GlslType::Vec3, GlslValue::Vec3([1.0, 2.0, 3.0])),
        (GlslType::Vec4, GlslValue::Vec4([1.0, 2.0, 3.0, 4.0])),
    ] {
        let data = GlslData::from_value(ty.clone(), &val).unwrap();
        let got = data.to_value().unwrap();
        assert_eq!(got, val, "roundtrip failed for {:?}", ty);
    }
}

#[test]
fn array_roundtrip() {
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
    let got = data.to_value().unwrap();
    assert_eq!(got, val);
}

#[test]
fn nested_struct_roundtrip() {
    let ty = GlslType::Struct {
        name: Some("Outer".to_string()),
        members: vec![
            StructMember {
                name: Some("inner".to_string()),
                ty: GlslType::Struct {
                    name: Some("Inner".to_string()),
                    members: vec![
                        StructMember { name: Some("x".to_string()), ty: GlslType::Float },
                        StructMember { name: Some("y".to_string()), ty: GlslType::Float },
                    ],
                },
            },
            StructMember { name: Some("z".to_string()), ty: GlslType::Float },
        ],
    };
    
    let val = GlslValue::Struct {
        name: Some("Outer".to_string()),
        fields: vec![
            ("inner".to_string(), GlslValue::Struct {
                name: Some("Inner".to_string()),
                fields: vec![
                    ("x".to_string(), GlslValue::F32(1.0)),
                    ("y".to_string(), GlslValue::F32(2.0)),
                ],
            }),
            ("z".to_string(), GlslValue::F32(3.0)),
        ],
    };
    
    let data = GlslData::from_value(ty, &val).unwrap();
    let got = data.to_value().unwrap();
    assert_eq!(got, val);
}

#[test]
fn path_access_consistency() {
    // Create nested struct in GlslValue
    let mut val = GlslValue::Struct {
        name: Some("Scene".to_string()),
        fields: vec![
            ("lights".to_string(), GlslValue::Array(vec![
                GlslValue::Struct {
                    name: Some("Light".to_string()),
                    fields: vec![
                        ("intensity".to_string(), GlslValue::F32(0.5)),
                    ],
                },
            ].into_boxed_slice())),
        ],
    };
    
    // Modify via path
    val.set_path("lights[0].intensity", GlslValue::F32(1.0)).unwrap();
    
    // Verify via path
    assert_eq!(val.get_path("lights[0].intensity").unwrap(), &GlslValue::F32(1.0));
    
    // Convert to GlslData
    let ty = val.glsl_type();
    let data = GlslData::from_value(ty, &val).unwrap();
    
    // Verify same value via data path
    assert_eq!(data.get("lights[0].intensity").unwrap(), GlslValue::F32(1.0));
}

#[test]
fn std430_vec3_not_padded() {
    // Critical: vec3 is 12 bytes in std430, not 16
    let ty = GlslType::Struct {
        name: Some("Test".to_string()),
        members: vec![
            StructMember { name: Some("v".to_string()), ty: GlslType::Vec3 },
            StructMember { name: Some("f".to_string()), ty: GlslType::Float },
        ],
    };
    
    // v: 12 bytes, aligned to 4, offset 0
    // f: 4 bytes, aligned to 4, offset 12
    // Total: 16 bytes
    assert_eq!(ty.size(LayoutRules::Std430), 16);
    
    // Verify offsets
    assert_eq!(ty.offset_for_path("v", LayoutRules::Std430, 0).unwrap(), 0);
    assert_eq!(ty.offset_for_path("f", LayoutRules::Std430, 0).unwrap(), 12);
}

#[test]
fn error_messages_helpful() {
    let ty = GlslType::Struct {
        name: Some("Light".to_string()),
        members: vec![
            StructMember { name: Some("position".to_string()), ty: GlslType::Vec3 },
            StructMember { name: Some("color".to_string()), ty: GlslType::Vec3 },
        ],
    };
    let data = GlslData::new(ty);
    
    // Try to access non-existent field
    let err = data.get("intensity").unwrap_err();
    let msg = format!("{}", err);
    
    // Error should mention what's available
    assert!(msg.contains("position") || msg.contains("color"), 
            "error should suggest available fields: {}", msg);
}
```

### 4. Property-based tests (optional but good)

```rust
// If you have quickcheck or proptest
#[test]
fn all_scalar_types_roundtrip() {
    // Test that every scalar type can roundtrip through GlslData
}

#[test]
fn nested_depth_n() {
    // Test structs nested N levels deep
}
```

### 5. Verify re-exports in lib.rs files

`lp-glsl-abi/src/lib.rs`:

```rust
pub mod glsl_data;
pub mod glsl_data_error;
// glsl_value is already pub

pub use glsl_data::GlslData;
pub use glsl_data_error::GlslDataError;
pub use glsl_value::{GlslValue, GlslValueError};
```

`lpir/src/lib.rs`:

```rust
pub mod path;
pub mod layout;

pub use path::{PathError, PathParseError};
pub use layout::{type_size, type_alignment, round_up};
// GlslType changes are already in glsl_metadata exports
```

## Validation

```bash
# Check all packages compile
cargo check -p lpir
cargo check -p lp-glsl-abi

# Run all tests
cargo test -p lpir
cargo test -p lp-glsl-abi

# Check no warnings
cargo clippy -p lpir -- -D warnings
cargo clippy -p lp-glsl-abi -- -D warnings

# Check formatting
cargo fmt -- --check
```

## Notes

- Integration tests live in tests/ directory, not in src/
- Each test should be independent
- Tests should cover both success and error cases
- Verify error messages are helpful