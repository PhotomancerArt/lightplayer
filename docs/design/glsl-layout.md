# GLSL memory layout (LightPlayer)

LightPlayer uses **`std430`-style packing** for structured CPU-side buffers (
`lpir::LayoutRules::Std430`, [`GlslData`](../../lp-glsl/lpvm/src/glsl_data.rs)). This matches
common GPU storage-buffer rules for transparent types.

## Implemented rules (`Std430`)

| Type                              | Size                   | Alignment |
|-----------------------------------|------------------------|-----------|
| `float`, `int`, `uint`, `bool`    | 4                      | 4         |
| `vec2`, `ivec2`, `uvec2`, `bvec2` | 8                      | 8         |
| `vec3`, `ivec3`, `uvec3`, `bvec3` | 12                     | 4         |
| `vec4`, `ivec4`, `uvec4`, `bvec4` | 16                     | 16        |
| `mat2`                            | 16 (2× `vec2` columns) | 8         |
| `mat3`                            | 36 (3× `vec3` columns) | 4         |
| `mat4`                            | 64 (4× `vec4` columns) | 16        |

- **Arrays:** element stride = round up(element size, element alignment); total size = stride ×
  length.
- **Structs:** members laid out in order with per-member alignment padding; struct alignment = max
  member alignment; struct size = round up(used bytes, struct alignment).

Scalars in memory are **little-endian** (`GlslData` read/write).

## Not implemented

- **`std140`** — reserved; [`LayoutRules::Std140`](../../lp-glsl/lpir/src/glsl_metadata.rs) panics
  in [`layout::type_size`](../../lp-glsl/lpir/src/layout.rs) / `type_alignment`.

## Related code

- Layout math: `lp-glsl/lpir/src/layout.rs`
- Path offsets / leaf types: `lp-glsl/lpir/src/glsl_path.rs`, `path.rs`
- Runtime buffer: `lp-glsl/lpvm/src/glsl_data.rs`
