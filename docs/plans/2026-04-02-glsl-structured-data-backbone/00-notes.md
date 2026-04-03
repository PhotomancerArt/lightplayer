# Glsl Structured Data Backbone - Plan Notes

## Scope of Work

Create the foundational type and data representations needed for structs, uniforms, and globals in
LightPlayer. This is prep work that does NOT modify the JIT compiler or executable yet - it
establishes the data structures and APIs that will be used in later phases.

Key deliverables:

1. `GlslShape` - extended type system with struct support
2. `Layout` - computed memory layout with byte offsets
3. `GlslData` - memory-backed representation with path-based access
4. Path parsing for nested field access (`light.position.x`, `colors[3]`)
5. Bidirectional conversion between `GlslValue` (Rust enum) and `GlslData` (bytes)
6. Support for std140 and std430 layout rules

## Current State

### Existing Types

**`lpir/src/glsl_metadata.rs` - `GlslType`**:

```rust
pub enum GlslType {
    Void,
    Float, Int, UInt, Bool,
    Vec2, Vec3, Vec4,
    IVec2, IVec3, IVec4,
    UVec2, UVec3, UVec4,
    BVec2, BVec3, BVec4,
    Mat2, Mat3, Mat4,
    Array { element: Box<GlslType>, len: u32 },
}
```

- Does NOT support structs
- Used for function parameter/return metadata

**`lp-glsl-abi/src/glsl_value.rs` - `GlslValue`**:

```rust
pub enum GlslValue {
    I32(i32), U32(u32), F32(f32), Bool(bool),
    Vec2([f32; 2]), Vec3([f32; 3]), Vec4([f32; 4]),
    // ... other vectors, matrices, Array(Box<[GlslValue]>)
}
```

- Tree representation for convenient Rust manipulation
- Used for test inputs/outputs and marshalling
- Has `Array(Box<[GlslValue]>)` but no struct representation

### Gaps

1. No struct type representation anywhere
2. No memory layout computation (offsets, alignment, padding)
3. No flat memory representation for structured data
4. No path-based access (`data.set("a.b[2].c", value)`)
5. No std140/std430 layout rule implementation

## Questions

### Q1: Layout Rules

**Question:** Should we support both std140 and std430, or pick one as the default?

**Context:**

- std140 is the traditional OpenGL uniform block layout (lots of padding)
- std430 is the newer, tighter storage buffer layout
- GPU transpilation needs one or the other
- Our JIT on ESP32 doesn't strictly need either, but consistency with GPU is good

**Answer:** Use std430 as the default. Tighter packing is more efficient for embedded targets.
Document this choice in `docs/design/glsl-layout.md` or similar.

TODO: Create docs/design note about std430 choice.

### Q2: GlslShape vs GlslType

**Question:** Should `GlslShape` replace/extend `GlslType`, or be a separate type?

**Context:**

- `GlslType` is used for function signatures throughout the codebase
- Adding struct to `GlslType` might break existing code that handles all variants
- `GlslShape` needs recursive layout computation which `GlslType` doesn't do

**Answer:** Extend `GlslType` with struct support. Simpler to have one type hierarchy. Add
validation later that structs aren't used in function parameters (since current ABI doesn't support
struct-by-value).

### Q3: Path Syntax

**Question:** What syntax for path-based access?

**Context:**
Need to support: `struct.field`, `array[index]`, nesting both

Options:

- `light.position.x` - dot notation (GLSL-like)
- `lights[3].color[0]` - bracket notation for arrays
- `lights.3.color.0` - all dots (simpler parsing)

**Suggested Answer:** Use GLSL-like syntax:

- `light.position.x` for struct fields
- `colors[3]` for array access
- Combined: `lights[3].color.r`

Parser needs to handle: identifier, `[number]`, and `.` in sequence.

### Q4: Error Handling Strategy

**Question:** How should path access errors be reported?

**Context:**

- Path might be malformed (`light..position`)
- Path might reference non-existent field (`light.foo`)
- Path might have type mismatch (trying to access scalar as struct)
- Path might have out-of-bounds array index

**Suggested Answer:** Create `GlslDataError` enum:

```rust
pub enum GlslDataError {
    InvalidPath(String),           // Parse error
    FieldNotFound { path: String, field: String },
    IndexOutOfBounds { path: String, index: usize, len: usize },
    TypeMismatch { path: String, expected: GlslShape, got: GlslValue },
}
```

### Q5: Array Storage in GlslData

**Question:** How are arrays represented in `GlslData`?

**Context:**

- Arrays could be stored inline (contiguous with parent struct)
- Or as separate heap allocations (pointers in struct)

**Suggested Answer:** Inline contiguous storage:

- Matches C/rust struct layout
- Single allocation for entire data block
- Better cache locality
- Simpler pointer arithmetic for JIT

Example: `vec4 colors[4]` = 4 * 16 = 64 bytes contiguous

### Q6: Layout vs Type

**Question:** Should we have a separate Layout type, or compute offsets directly from GlslType?

**Answer:** Compute offsets directly from GlslType:

```rust
impl GlslType {
    pub fn offset_for_path(&self, path: &str, base_offset: usize) -> Result<usize, OffsetError>;
    pub fn size(&self) -> usize;
    pub fn alignment(&self) -> usize;
}
```

No separate Layout type needed. Computation is pure and lazy. Caching can be added later as an
optimization if needed.

### Q7: Mutable Access Patterns

**Question:** Should `GlslData` support interior mutability patterns, or &mut self?

**Answer:** Simple &mut self for mutation:

```rust
impl GlslData {
    pub fn get(&self, path: &str) -> Result<GlslValue, GlslDataError>;
    pub fn set(&mut self, path: &str, value: GlslValue) -> Result<(), GlslDataError>;
    
    // For zero-copy mutation of scalars
    pub fn get_f32(&self, path: &str) -> Result<f32, GlslDataError>;
    pub fn set_f32(&mut self, path: &str, val: f32) -> Result<(), GlslDataError>;
}
```

No need for complex interior mutability - keep it simple.
