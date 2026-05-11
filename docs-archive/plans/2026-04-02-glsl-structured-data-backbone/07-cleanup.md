# Phase 7: Cleanup and Documentation

## Scope

Final validation, documentation, and cleanup before commit.

## Implementation Details

### 1. Create design documentation

Create `docs/design/glsl-layout.md`:

```markdown
# GLSL Memory Layout in LightPlayer

## Layout Rules

LightPlayer currently implements **std430** layout only.

### std430 Rules

#### Scalar Types

| Type | Size | Alignment |
|------|------|-----------|
| float, int, uint, bool | 4 bytes | 4 bytes |

#### Vector Types

| Type | Size | Alignment | Notes |
|------|------|-----------|-------|
| vec2 | 8 bytes | 8 bytes | |
| vec3 | 12 bytes | 4 bytes | **Not padded to 16!** |
| vec4 | 16 bytes | 16 bytes | |

The vec3 handling is the critical difference from std140.
In std140, vec3 is 16 bytes with 16-byte alignment.

#### Matrix Types

Matrices are stored column-major:

| Type | Size | Alignment |
|------|------|-----------|
| mat2 | 16 bytes | 8 bytes | 2 × vec2 columns |
| mat3 | 36 bytes | 4 bytes | 3 × vec3 columns |
| mat4 | 64 bytes | 16 bytes | 4 × vec4 columns |

#### Array Types

- Element stride = element size (rounded to element alignment)
- No additional rounding (unlike std140 which rounds to 16 bytes)

Example: `vec3 arr[4]`
- Element size = 12, alignment = 4
- Stride = 12
- Total = 48 bytes

#### Struct Types

- Members laid out in declaration order
- Each member aligned to its natural alignment
- Member offset = round_up(current_offset, member_alignment)
- Struct alignment = max member alignment
- Struct size = round_up(total_size, struct_alignment)

Example:
```glsl
struct Example {
    float f;    // offset 0, size 4
    vec3 v;     // offset 4 (round_up(4, 4)), size 12
};              // total 16, alignment 4
```

## Path Syntax

Access structured data with GLSL-like paths:

```rust
// Struct field access
data.get("light.position")

// Array element access  
data.get("lights[3]")

// Combined
data.get("lights[3].color.r")
```

### Supported Syntax

- `.field` - struct field access
- `[n]` - array element access (0-indexed)
- Components: `.x`, `.y`, `.z`, `.w`, `.r`, `.g`, `.b`, `.a`

### Examples

```rust
let ty = GlslType::Struct {
    name: Some("Scene".to_string()),
    members: vec![
        StructMember {
            name: Some("lights".to_string()),
            ty: GlslType::Array {
                element: Box::new(GlslType::Struct {
                    name: Some("Light".to_string()),
                    members: vec![
                        StructMember { name: Some("position".to_string()), ty: GlslType::Vec3 },
                        StructMember { name: Some("color".to_string()), ty: GlslType::Vec3 },
                    ],
                }),
                len: 4,
            },
        },
    ],
};

let mut data = GlslData::new(ty);

// Set light 2's color's red component
data.set("lights[2].color.r", GlslValue::F32(1.0)).unwrap();
```

## API Overview

### GlslType (lpir)

```rust
// Layout computation
let size = ty.size(LayoutRules::Std430);
let align = ty.alignment(LayoutRules::Std430);
let offset = ty.offset_for_path("lights[3].color", LayoutRules::Std430, 0)?;
```

### GlslData (lpvm)

```rust
// Create and modify
data.set("position.x", GlslValue::F32(1.0))?;
let pos = data.get("position")?;

// Direct scalar access (faster)
data.set_f32("intensity", 0.5)?;
let intensity = data.get_f32("intensity")?;

// Round-trip with GlslValue
let data = GlslData::from_value(ty, &value)?;
let value = data.to_value()?;
```

### GlslValue (lpvm)

```rust
// Tree manipulation
let pos = value.get_path("light.position")?;
value.set_path("light.intensity", GlslValue::F32(1.0))?;

// Introspection
let ty = value.glsl_type();
```

## Future Work

- std140 layout support (for older OpenGL compatibility)
- Explicit layout qualifiers (layout(offset = 32))
- Memory qualifiers (readonly, coherent, etc.)

```

### 2. Verify documentation in code

Add module-level docs:

```rust
//! Memory layout computation for GLSL types.
//!
//! Implements std430 layout rules. See `docs/design/glsl-layout.md` for details.

//! Path parsing for GLSL data access.
//!
//! Supports syntax: `field.subfield`, `array[3]`, `lights[3].color.r`
```

### 3. Check for TODOs and temporary code

```bash
grep -r "TODO\|FIXME\|XXX\|hack\|temporary" lpir/src/ lpvm/src/ \
  --include="*.rs" | grep -v target
```

Fix or document any remaining TODOs.

### 4. Final validation checklist

Run in order:

```bash
# 1. Format check
cargo +nightly fmt -- --check

# 2. Build check
cargo check -p lpir --target riscv32imac-unknown-none-elf
cargo check -p lpvm --target riscv32imac-unknown-none-elf

# 3. Tests
cargo test -p lpir
cargo test -p lpvm

# 4. Clippy (strict)
cargo clippy -p lpir -- -D warnings -A clippy::new_without_default
cargo clippy -p lpvm -- -D warnings

# 5. Doc tests
cargo doc -p lpir --no-deps
cargo doc -p lpvm --no-deps
```

### 5. Review public API surface

Check that only intended items are public:

```bash
cargo rustdoc -p lpir -- --document-private-items 2>&1 | grep "pub " | head -30
cargo rustdoc -p lpvm -- --document-private-items 2>&1 | grep "pub " | head -30
```

Ensure no internal helpers leaked.

### 6. Create summary

Create `docs/plans/2026-04-02-glsl-structured-data-backbone/summary.md`:

```markdown
# Glsl Structured Data Backbone - Summary

## Completed Work

### New Types

- `LayoutRules` enum (std430 only for now)
- `GlslType::Struct` variant with `StructMember`
- `GlslValue::Struct` variant
- `GlslData` - memory-backed data with path access
- `GlslDataError`, `GlslValueError`, `PathError` - comprehensive errors

### New Modules

- `lpir/src/layout.rs` - layout computation
- `lpir/src/path.rs` - path parsing
- `lpvm/src/glsl_data.rs` - GlslData implementation
- `lpvm/src/glsl_data_error.rs` - error types

### Key Features

- Path-based access: `data.get("lights[3].color.r")`
- Direct scalar access: `data.set_f32("intensity", 0.5)`
- Bidirectional conversion: `GlslValue` ↔ `GlslData`
- Layout computation: size, alignment, offset_for_path
- Std430 layout rules implemented
- Helpful error messages with suggestions

### API Surface

```rust
// Type operations
GlslType::size(LayoutRules)
GlslType::alignment(LayoutRules)
GlslType::offset_for_path(path, rules, base)

// Data operations
GlslData::new(ty)
GlslData::from_value(ty, value)
GlslData::to_value()
GlslData::get(path)
GlslData::set(path, value)
GlslData::get_f32(path), set_f32(path, val)

// Value operations
GlslValue::get_path(path)
GlslValue::set_path(path, value)
GlslValue::glsl_type()
```

## Next Steps

This is prep work. The JIT compiler and executable integration will use:

- `GlslType::offset_for_path()` for generating memory access code
- `GlslData` for managing uniform and global storage
- Path-based API for external access to shader data

```

## Validation

```bash
# Final check everything works
cargo test -p lpir -p lpvm --no-fail-fast
```

## Commit

Once everything passes:

```bash
git add -A
git commit -m "feat(glsl): structured data backbone

Add GlslData and path-based access for GLSL types.

- GlslType::Struct variant with std430 layout
- Layout computation: size, alignment, offset_for_path
- GlslData: memory-backed storage with path access
- Path syntax: lights[3].color.r
- Bidirectional GlslValue ↔ GlslData conversion
- Comprehensive error types with helpful messages

See docs/design/glsl-layout.md for layout rules."
```

## Notes

- All tests must pass before commit
- Documentation must build without warnings
- No TODOs should remain in committed code
- Public API must be intentional and documented