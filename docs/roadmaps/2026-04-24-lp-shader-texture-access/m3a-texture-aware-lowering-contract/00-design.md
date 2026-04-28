# Scope of Work

Milestone 3a makes the frontend lowering contract texture-aware without
implementing the full `texelFetch` data path.

The implementation should add an options-based frontend lowering API, validate
and retain compile-time texture specs, recognize Naga's `texelFetch` image-load
shape, resolve supported texture operands back to direct uniform sampler names,
and produce clear diagnostics for unsupported texture-read forms.

Out of scope for this milestone: descriptor address math, `Load16U`,
`Unorm16toF`, vec4 channel fill, runtime `LpsTextureBuf` validation, backend
matrix filetests, and normalized-coordinate `texture()` sampling.

# File Structure

```text
lp-shader/
├── lps-shared/
│   └── src/
│       ├── sig.rs                       # UPDATE: retain texture specs on LpsModuleSig
│       └── lib.rs                       # UPDATE: re-export any new shared aliases if needed
├── lps-frontend/
│   └── src/
│       ├── lib.rs                       # UPDATE: export LowerOptions + lower_with_options
│       ├── lower.rs                     # UPDATE: options-aware lower entry and metadata validation
│       ├── lower_ctx.rs                 # UPDATE: carry texture lowering context/spec refs
│       ├── lower_expr.rs                # UPDATE: dispatch image-load/texelFetch expressions
│       └── lower_texture.rs             # NEW: texture operand/spec/LOD contract helpers
├── lp-shader/
│   └── src/
│       └── engine.rs                    # UPDATE: call lower_with_options from compile_px_desc
└── lps-filetests/
    └── src/
        ├── test_error/mod.rs            # UPDATE: use texture-aware lower for diagnostics
        └── test_run/filetest_lpvm.rs    # UPDATE: use texture-aware lower for run tests
```

# Conceptual Architecture

```text
GLSL source
    │
    ▼
lps_frontend::compile
    │
    ▼
NagaModule + LowerOptions { texture_specs }
    │
    ▼
lower_with_options
    │
    ├─ compute uniforms/globals metadata
    ├─ validate Texture2D uniforms against texture_specs
    ├─ retain texture_specs in LpsModuleSig
    └─ lower each user function with LowerCtx texture context
         │
         ▼
      lower_texture helper
         │
         ├─ identify Naga texelFetch/ImageLoad shape
         ├─ resolve direct uniform Texture2D operand to sampler name
         ├─ confirm matching TextureBindingSpec
         ├─ reject nonzero or dynamic LOD
         ├─ reject aliases/params/non-Texture2D operands
         └─ return clear M3b placeholder diagnostic for valid fetches
```

# Main Components

## `LowerOptions`

`lps-frontend` should expose an options struct rather than a narrowly named
`lower_with_texture_specs` function.

Expected shape:

```rust
pub struct LowerOptions<'a> {
    pub texture_specs: &'a BTreeMap<String, TextureBindingSpec>,
}

impl Default for LowerOptions<'_> {
    fn default() -> Self {
        Self {
            texture_specs: &EMPTY_TEXTURE_SPECS,
        }
    }
}
```

The exact default implementation can use a helper constructor if a static
`BTreeMap` is awkward in `no_std + alloc`; the public behavior should be that
`lower(&NagaModule)` remains the no-options convenience wrapper and
`lower_with_options(&NagaModule, &LowerOptions)` is the canonical extensible
entry.

## Metadata Retention

`LpsModuleSig` should retain compile-time texture specs, defaulting to an empty
map for modules without textures or callers using `lower()`.

This lets later milestones implement runtime validation from compiled metadata
instead of maintaining a parallel side channel in `lp-shader`.

## Texture Lowering Context

`LowerCtx` should receive access to texture specs and any derived lookup data
needed to lower texture operations. The context should preserve logical sampler
names and avoid exposing descriptor fields in diagnostics.

Supported texture operands in M3a are direct uniform `Texture2D` globals only.
Texture values routed through locals, parameters, or other aliases should fail
with a clear diagnostic rather than trying to infer provenance.

## `lower_texture`

Texture-specific contract logic should live in a new focused module. It should:

- Match the Naga expression shape for GLSL `texelFetch`.
- Validate the operand resolves to a direct uniform sampler name.
- Validate that the sampler has a `TextureBindingSpec`.
- Validate `lod == 0` only when the zero can be proven from a literal.
- Reject dynamic LOD and nonzero LOD.
- Stop before data-path codegen with a clear M3b placeholder diagnostic for
  otherwise valid `texelFetch` uses.

The placeholder diagnostic is intentional: M3a proves recognition, metadata,
and diagnostics; M3b owns descriptor loads, offset math, storage loads, and
vec4 result generation.

## Callers

`lp-shader::compile_px_desc` should call `lower_with_options` using
`CompilePxDesc::textures`.

`lps-filetests` should do the same for texture-aware compile/error paths so
diagnostics for `texelFetch` happen during lowering with the parsed
`// texture-spec:` map available.

