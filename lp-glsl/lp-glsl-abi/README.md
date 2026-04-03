# lp-glsl-abi

GLSL **Application Binary Interface** (ABI) - types, values, memory layout, and path-based access for the LightPlayer shader system.

This crate provides the foundation for working with GLSL types at the ABI level. It is **self-contained** and does not depend on the LPIR (LightPlayer IR) crate, enabling it to be used independently for value manipulation, serialization, and host-side shader data management.

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         lp-glsl-abi                               │
├─────────────────────────────────────────────────────────────────┤
│  metadata.rs    │  GlslType, StructMember, LayoutRules             │
│                 │  GlslFunctionMeta, GlslModuleMeta                 │
├─────────────────────────────────────────────────────────────────┤
│  layout.rs      │  std430 layout computation                       │
│                 │  type_size, type_alignment, array_stride          │
├─────────────────────────────────────────────────────────────────┤
│  value.rs       │  GlslValue - tree representation of values       │
│                 │  scalars, vectors, matrices, arrays, structs    │
├─────────────────────────────────────────────────────────────────┤
│  data.rs        │  GlslData - byte buffer with layout rules      │
│                 │  get/set by path, round-trip with GlslValue    │
├─────────────────────────────────────────────────────────────────┤
│  path.rs        │  PathSegment, parse_path - "lights[3].color"   │
├─────────────────────────────────────────────────────────────────┤
│  path_resolve.rs│  type_at_path, offset_for_path on GlslType     │
├─────────────────────────────────────────────────────────────────┤
│  value_path.rs  │  get_path, set_path on GlslValue trees         │
└─────────────────────────────────────────────────────────────────┘
```

## Key Types

### `GlslType`

The complete GLSL type system:

```rust
pub enum GlslType {
    // Scalars
    Float, Int, UInt, Bool,
    // Vectors
    Vec2, Vec3, Vec4, IVec2, IVec3, IVec4, UVec2, UVec3, UVec4, BVec2, BVec3, BVec4,
    // Matrices (column-major)
    Mat2, Mat3, Mat4,
    // Arrays
    Array { element: Box<GlslType>, len: u32 },
    // Structs
    Struct { name: Option<String>, members: Vec<StructMember> },
}
```

### `GlslValue`

Tree representation for runtime values:

```rust
pub enum GlslValue {
    F32(f32), I32(i32), U32(u32), Bool(bool),
    Vec2([f32; 2]), Vec3([f32; 3]), Vec4([f32; 4]),
    // ... vectors, matrices, arrays, structs
}
```

### `GlslData`

Byte-buffer representation using std430 layout:

```rust
let ty = GlslType::Struct {
    name: Some("Light".into()),
    members: vec![
        StructMember { name: Some("position".into()), ty: GlslType::Vec3 },
        StructMember { name: Some("intensity".into()), ty: GlslType::Float },
    ],
};

let mut data = GlslData::new(ty);
data.set_f32("intensity", 1.5).unwrap();
let pos = data.get("position").unwrap();
```

## Layout Rules

The crate implements **std430** layout rules for storage-buffer-style packing:

| Type | Size | Alignment |
|------|------|-----------|
| float, int, uint, bool | 4 | 4 |
| vec2 | 8 | 8 |
| vec3 | 12 | 4 (not padded to 16!) |
| vec4 | 16 | 16 |
| mat2 | 16 | 8 |
| mat3 | 36 | 4 |
| mat4 | 64 | 16 |

## Path Syntax

Access nested values using GLSL-style paths:

```rust
"lights[3].color.r"     // array index + field + swizzle
"material.albedo"       // struct field
"transform[0][1]"       // nested array access
```

## Features

- **`std`** (default) - Enable std-dependent functionality
- **`parse`** (default) - Enable `GlslValue::parse()` using the GLSL parser (requires `glsl` crate)

The crate is `no_std` compatible without the `std` feature. The `parse` feature can be disabled for embedded builds to avoid the `nom` parser dependency.

## Relationship to Other Crates

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   lp-glsl-abi   │────→│ lpir-cranelift  │←────│   lp-glsl-naga  │
│   (this crate)  │     │   (JIT codegen) │     │  (GLSL parser)  │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │
         ↓                       ↓
┌─────────────────┐     ┌─────────────────┐
│  lp-glsl-exec   │     │      lpir       │
│  (shader exec)  │     │  (pure LPIR)   │
└─────────────────┘     └─────────────────┘
```

- **`lpir`** - Pure intermediate representation (no GLSL dependencies)
- **`lpir-cranelift`** - Combines `lp-glsl-abi` (types) with `lpir` (IR) for JIT compilation
- **`lp-glsl-naga`** - GLSL frontend that produces LPIR + metadata

See [`../CRATES.md`](../CRATES.md) for the full crate map.
