# Glsl Structured Data Backbone - Design

## Scope of Work

Create foundational types and APIs for structured data in LightPlayer:

- Extended `GlslType` with struct support and layout rules
- `GlslData` - memory-backed representation with path-based access
- Bidirectional conversion between `GlslValue` (Rust tree) and `GlslData` (bytes)
- Comprehensive error handling for type/shape mismatches

**Non-goals:** JIT compiler integration, executable changes. This is prep work only.

## File Structure

```
lp-glsl/
├── lp-glsl-abi/
│   └── src/
│       ├── lib.rs                    # UPDATE: re-export GlslData, GlslDataError
│       ├── glsl_value.rs             # EXISTING: GlslValue enum (minor updates)
│       ├── glsl_data.rs              # NEW: GlslData, path parsing, memory access
│       └── glsl_data_error.rs        # NEW: comprehensive error types
│
lpir/
└── src/
    ├── lib.rs                        # UPDATE: re-export GlslType changes
    ├── glsl_metadata.rs              # UPDATE: Add struct to GlslType, LayoutRules
    └── layout.rs                     # NEW: std430 computation
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         GlslType (metadata)                       │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ Scalar       │  │ Array        │  │ Struct                │ │
│  │ (Float,etc)  │  │ {elem, len}  │  │ {name, members,       │ │
│  │              │  │              │  │  layout_rules}        │ │
│  └──────────────┘  └──────────────┘  └─────────────────────────┘ │
│                                                                  │
│  Methods: size(rules), alignment(rules), offset_for_path(...)   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ describes shape
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         GlslData (runtime)                      │
│                                                                  │
│  ┌─────────────────┐    ┌─────────────────┐                       │
│  │  ty: GlslType   │───▶│  data: Vec<u8>  │                       │
│  │  rules: Layout  │    │  (the bytes)    │                       │
│  └─────────────────┘    └─────────────────┘                       │
│                                                                  │
│  Methods:                                                        │
│    get(path) -> GlslValue       (read + convert)                 │
│    set(path, GlslValue)         (convert + write)                  │
└─────────────────────────────────────────────────────────────────┘
```

## API Specification

### LayoutRules Enum (explicit)

```rust
/// Memory layout rules for GLSL data.
/// Only Std430 is supported for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutRules {
    /// std430 - tighter packing for storage buffers
    /// - scalars: 4 bytes, 4-byte aligned
    /// - vec2: 8 bytes, 8-byte aligned
    /// - vec3: 12 bytes, 4-byte aligned (NO padding!)
    /// - vec4: 16 bytes, 16-byte aligned
    /// - arrays: element stride = element size (no rounding)
    /// - structs: alignment = max member alignment
    Std430,
    
    /// Reserved for future GPU transpilation
    /// Not implemented - will panic if used
    Std140,
}

impl LayoutRules {
    /// Size of scalar type under these rules.
    pub fn scalar_size(&self, ty: ScalarType) -> usize;
    
    /// Alignment of scalar type under these rules.
    pub fn scalar_alignment(&self, ty: ScalarType) -> usize;
    
    /// Round up size to alignment.
    pub fn round_up(&self, size: usize, alignment: usize) -> usize;
}
```

### GlslType Extension

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum GlslType {
    // Existing variants...
    Array {
        element: Box<GlslType>,
        len: u32,
    },
    // NEW: Struct support
    Struct {
        name: Option<String>,
        members: Vec<StructMember>,
    },
}

pub struct StructMember {
    pub name: Option<String>,
    pub ty: GlslType,
    // Future: explicit layout overrides
    // pub explicit_offset: Option<u32>,
    // pub explicit_align: Option<u32>,
}

impl GlslType {
    /// Compute total size under given layout rules.
    pub fn size(&self, rules: LayoutRules) -> usize;
    
    /// Compute alignment under given layout rules.
    pub fn alignment(&self, rules: LayoutRules) -> usize;
    
    /// Compute byte offset for a path.
    pub fn offset_for_path(&self, path: &str, rules: LayoutRules, base_offset: usize) 
        -> Result<usize, PathError>;
    
    /// Get the type at a path.
    pub fn type_at_path(&self, path: &str) -> Result<&GlslType, PathError>;
}
```

### GlslData

```rust
pub struct GlslData {
    ty: GlslType,
    rules: LayoutRules,  // Currently always Std430
    data: Vec<u8>,
}

impl GlslData {
    /// Create zero-initialized data with Std430 layout.
    pub fn new(ty: GlslType) -> Self {
        Self::with_rules(ty, LayoutRules::Std430)
    }
    
    /// Create with specific layout rules.
    pub fn with_rules(ty: GlslType, rules: LayoutRules) -> Self;
    
    /// Create from a GlslValue tree.
    pub fn from_value(ty: GlslType, value: &GlslValue) -> Result<Self, GlslDataError>;
    
    /// Convert entire data block to GlslValue tree.
    pub fn to_value(&self) -> Result<GlslValue, GlslDataError>;
    
    // Path-based access
    pub fn get(&self, path: &str) -> Result<GlslValue, GlslDataError>;
    pub fn set(&mut self, path: &str, value: GlslValue) -> Result<(), GlslDataError>;
    
    // Direct scalar access
    pub fn get_f32(&self, path: &str) -> Result<f32, GlslDataError>;
    pub fn set_f32(&mut self, path: &str, val: f32) -> Result<(), GlslDataError>;
    pub fn get_i32(&self, path: &str) -> Result<i32, GlslDataError>;
    pub fn set_i32(&mut self, path: &str, val: i32) -> Result<(), GlslDataError>;
    
    // Raw access
    pub fn as_ptr(&self) -> *const u8;
    pub fn as_mut_ptr(&mut self) -> *mut u8;
    pub fn offset_of(&self, path: &str) -> Result<usize, GlslDataError>;
}
```

### Path Access for GlslValue

```rust
impl GlslValue {
    // NEW: Struct variant
    Struct {
        name: Option<String>,
        fields: Vec<(String, GlslValue)>,
    }
    
    /// Get value at path.
    pub fn get_path(&self, path: &str) -> Result<&GlslValue, GlslValueError>;
    
    /// Get mutable reference at path.
    pub fn get_path_mut(&mut self, path: &str) -> Result<&mut GlslValue, GlslValueError>;
    
    /// Set value at path.
    pub fn set_path(&mut self, path: &str, value: GlslValue) -> Result<(), GlslValueError>;
}
```

## Layout Rules (std430 only for now)

### Scalar Types

| Type                   | Size    | Alignment |
|------------------------|---------|-----------|
| float, int, uint, bool | 4 bytes | 4 bytes   |

### Vector Types

| Type                      | Size     | Alignment | Notes             |
|---------------------------|----------|-----------|-------------------|
| vec2, ivec2, uvec2, bvec2 | 8 bytes  | 8 bytes   |                   |
| vec3, ivec3, uvec3, bvec3 | 12 bytes | 4 bytes   | NOT padded to 16! |
| vec4, ivec4, uvec4, bvec4 | 16 bytes | 16 bytes  |                   |

### Matrix Types

| Type | Size     | Alignment | Notes                          |
|------|----------|-----------|--------------------------------|
| mat2 | 16 bytes | 8 bytes   | 2 vec2 columns                 |
| mat3 | 36 bytes | 4 bytes   | 3 vec3 columns (12 bytes each) |
| mat4 | 64 bytes | 16 bytes  | 4 vec4 columns                 |

### Array Types

- Element stride = element size (no rounding up)
- Total size = element_count * stride
- Alignment = element alignment

### Struct Types

- Members laid out in order
- Each member aligned to its natural alignment
- Struct alignment = max member alignment
- Struct size = round up(total member size to struct alignment)

## Error Types

```rust
pub enum PathError {
    InvalidSyntax { path: String, reason: String },
    FieldNotFound { path: String, field: String },
    IndexOutOfBounds { path: String, index: usize, len: usize },
    NotIndexable { path: String, ty: String },
    NotAField { path: String, ty: String },
}

pub enum GlslDataError {
    Path(PathError),
    TypeMismatch { path: String, expected: String, got: String },
    LayoutNotImplemented { rules: LayoutRules }, // for Std140
}

pub enum GlslValueError {
    Path(PathError),
    NotAStruct { got: String },
    NotAnArray { got: String },
}
```

## Implementation Phases

1. **Extend GlslType with struct support**
    - Add Struct variant to GlslType enum
    - Add StructMember type
    - Add LayoutRules enum with just Std430

2. **Layout computation (std430)**
    - Implement size(), alignment() for all types under Std430
    - Document the rules in code comments
    - Add unit tests for each rule

3. **Path parser**
    - Tokenize paths: ident, `[`, number, `]`, `.`
    - Parse into PathSegment: Field(String), Index(usize)
    - Comprehensive error handling with helpful messages

4. **GlslData core**
    - GlslData struct with ty, rules, data
    - new(), from_value(), to_value()
    - get(), set() using path parser and layout
    - Direct scalar accessors

5. **GlslValue struct support and path access**
    - Add Struct variant to GlslValue
    - get_path(), get_path_mut(), set_path()
    - Tests for nested access

6. **Error types and polish**
    - Define all error types with Display impls
    - Integration tests
    - Property tests for round-trip

7. **Documentation and cleanup**
    - Add layout rules to docs/design/
    - Final validation
    - Commit
