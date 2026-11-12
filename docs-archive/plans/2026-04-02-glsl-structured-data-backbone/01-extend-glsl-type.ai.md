# Phase 1: Extend GlslType with Struct Support

## Scope

Add struct variant to GlslType enum, add LayoutRules enum, implement basic size/alignment methods.

## Implementation Details

### 1. Add LayoutRules enum (lpir/src/glsl_metadata.rs)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutRules {
    /// std430 - tighter packing for storage buffers (default)
    Std430,
    /// Reserved for future GPU transpilation
    Std140,
}

impl LayoutRules {
    /// Returns true if this layout rule is implemented.
    pub fn is_implemented(&self) -> bool {
        matches!(self, LayoutRules::Std430)
    }
}
```

### 2. Add StructMember struct

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct StructMember {
    pub name: Option<String>,
    pub ty: GlslType,
}
```

### 3. Extend GlslType enum

Add to existing `GlslType`:
```rust
Struct {
    name: Option<String>,
    members: Vec<StructMember>,
},
```

### 4. Add helper methods

```rust
impl GlslType {
    /// Check if this is a scalar type.
    pub fn is_scalar(&self) -> bool;
    
    /// Check if this is an aggregate type (struct or array).
    pub fn is_aggregate(&self) -> bool;
}
```

## Code Organization

- Place LayoutRules at the top of glsl_metadata.rs (before GlslType)
- Place StructMember after LayoutRules
- Add Struct variant to GlslType enum
- Place helper methods in impl block, near existing code

## Tests

```rust
#[test]
fn struct_type_creation() {
    let light_struct = GlslType::Struct {
        name: Some("Light".to_string()),
        members: vec![
            StructMember { name: Some("position".to_string()), ty: GlslType::Vec3 },
            StructMember { name: Some("intensity".to_string()), ty: GlslType::Float },
        ],
    };
    assert!(light_struct.is_aggregate());
    assert!(!light_struct.is_scalar());
}

#[test]
fn layout_rules_std430_is_implemented() {
    assert!(LayoutRules::Std430.is_implemented());
}

#[test]
fn layout_rules_std140_is_not_implemented() {
    assert!(!LayoutRules::Std140.is_implemented());
}
```

## Validation

```bash
cargo check -p lpir
cargo test -p lpir -- --test-threads=1
```

## Notes

- Keep existing GlslType variants unchanged
- No layout computation yet - just the type definitions
- StructMember.name is Option<String> because GLSL allows anonymous struct members