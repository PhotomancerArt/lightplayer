# Scope of Work

Milestone 1 establishes the texture interface foundation for `lp-shader`.
Texture uniforms become visible in shader metadata, typed as logical
`Texture2D`, validated against compile-time binding specs, and associated with a
fixed uniform descriptor ABI. This milestone does not implement texture
sampling, `texelFetch` lowering, `texture` lowering, or runtime sample
execution.

# File Structure

```text
lp-shader/
├── lps-shared/src/
│   ├── lib.rs                    # UPDATE: re-export texture binding vocabulary
│   ├── texture_format.rs         # UPDATE: add binding spec/filter/wrap/shape types near storage format
│   ├── types.rs                  # UPDATE: add logical LpsType::Texture2D
│   ├── layout.rs                 # UPDATE: Texture2D ABI size/alignment = 16/4
│   └── path_resolve.rs           # UPDATE: keep Texture2D as leaf, not struct-addressable
├── lps-frontend/src/
│   ├── naga_types.rs             # UPDATE: map supported sampler2D to LpsType::Texture2D
│   ├── lower.rs                  # UPDATE: include texture uniforms in metadata/layout
│   └── lib.rs                    # UPDATE: metadata/validation tests
├── lp-shader/src/
│   ├── compile_px_desc.rs        # NEW: CompilePxDesc<'a> + texture spec map alias/helper
│   ├── texture_uniform.rs        # NEW: repr(C) Texture2DUniform from &LpsTextureBuf
│   ├── engine.rs                 # UPDATE: descriptor-based compile method + compatibility wrapper
│   ├── lib.rs                    # UPDATE: public exports
│   └── tests.rs                  # UPDATE: compile descriptor and texture interface validation tests
└── lpvm/src/
    └── set_uniform.rs            # UPDATE: if needed, reject Texture2D path writes or route typed descriptor cleanly
```

# Conceptual Architecture

```text
GLSL source
  └─ declares: uniform sampler2D inputColor;

CompilePxDesc
  ├─ glsl source
  ├─ output TextureStorageFormat
  ├─ CompilerConfig
  └─ textures: BTreeMap<String, TextureBindingSpec>
         └─ inputColor => format/filter/wrap/shape

lps-frontend
  ├─ Naga sampler2D uniform -> LpsType::Texture2D
  ├─ metadata uniforms_type includes logical texture member
  └─ validation matches shader sampler names against CompilePxDesc.textures

lps-shared layout
  └─ LpsType::Texture2D remains logical metadata,
     but ABI size/alignment is fixed descriptor: 16 bytes / 4-byte align

lp-shader runtime helper
  └─ &LpsTextureBuf -> Texture2DUniform {
       ptr, width, height, row_stride
     }
```

# Main Components

## Shared Texture Vocabulary

`lps-shared` owns the compile-time vocabulary so the frontend, runtime,
filetests, and future WGSL/wgpu work agree on one contract:

```rust
pub struct TextureBindingSpec {
    pub format: TextureStorageFormat,
    pub filter: TextureFilter,
    pub wrap_x: TextureWrap,
    pub wrap_y: TextureWrap,
    pub shape_hint: TextureShapeHint,
}

pub enum TextureFilter {
    Nearest,
    Linear,
}

pub enum TextureWrap {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

pub enum TextureShapeHint {
    General2D,
    HeightOne,
}
```

`LpsType::Texture2D` is a logical shader type. Metadata and diagnostics should
describe it as a texture/sampler uniform, not as a fake struct with descriptor
fields.

## Compile Descriptor

`lp-shader` adds a named descriptor for pixel shader compilation:

```rust
pub struct CompilePxDesc<'a> {
    pub glsl: &'a str,
    pub output_format: TextureStorageFormat,
    pub compiler_config: CompilerConfig,
    pub textures: BTreeMap<String, TextureBindingSpec>,
}
```

`LpsEngine::compile_px(...)` remains as a compatibility wrapper and calls the
descriptor path with an empty texture map. New texture-aware callers use the
descriptor method directly.

## Frontend Metadata and Validation

The frontend maps supported GLSL `sampler2D` uniforms to
`LpsType::Texture2D`. During descriptor-based pixel shader compilation,
`lp-shader` validates the extracted sampler uniforms against
`CompilePxDesc::textures`.

Validation is strict:

- A shader-declared sampler without a matching spec is an error.
- A spec for a nonexistent sampler is an error.
- Unsupported source texture types or unsupported sampler shapes are errors.
- This milestone accepts only GLSL `sampler2D` as a texture uniform shape.

## Texture Uniform ABI

Texture uniforms lower to a fixed guest ABI descriptor:

```rust
#[repr(C)]
pub struct Texture2DUniform {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}
```

`LpsType::Texture2D` has std430 size/alignment `16`/`4`. The descriptor is an
ABI detail, while `Texture2D` remains the public logical type in metadata.

`lp-shader` exposes a typed helper constructed from `&LpsTextureBuf`, so callers
can bind texture uniform descriptors without hand-authoring raw pointer structs.

