# lp-shader Texture Access Roadmap

**Status:** Milestones M1–M5 (texture binding, filetests, lowering, filtered
sampling, height-one API) are implemented in-tree. M6 is documentation,
validation, and cleanup — see `m6-integration-validation-cleanup/plan.md`.

## Motivation / rationale

`lp-shader` can already render *to* textures through the synthesized
`__render_texture_<format>` path, but shaders still cannot read *from*
textures. That blocks the LightPlayer visual model: effects need one input
texture, transitions need two, and palettes/gradients likely want baked
height-one lookup textures.

The main problem is not just "add `texelFetch`." Texture reads touch the shader
type system, uniform ABI, runtime binding validation, filetests, and future
wgpu compatibility. The design doc settled the key boundary:

- `lp-shader` owns the texture binding contract, validation, and lowering.
- lpfx/domain own source routing and policy derivation.

## Architecture / design

### File and crate shape

```text
docs/
├── design/
│   └── lp-shader-texture-access.md      # Reference design
└── roadmaps/
    └── 2026-04-24-lp-shader-texture-access/
        ├── notes.md
        ├── overview.md
        ├── decisions.md
        └── m<N>-*.md

lp-shader/
├── lps-shared/
│   └── src/
│       ├── texture_format.rs            # TextureBindingSpec/filter/wrap/shape
│       ├── types.rs                     # logical LpsType::Texture2D
│       ├── lps_value_f32.rs             # typed texture runtime value/helper
│       └── lps_value_q32.rs             # ABI conversion support
├── lps-frontend/
│   └── src/
│       ├── naga_types.rs                # sampler2D -> Texture2D
│       ├── lower_call.rs                # texelFetch / texture lowering
│       └── lower_expr.rs                # texture call operands / descriptor loads
├── lp-shader/
│   └── src/
│       ├── engine.rs                    # compile descriptor input
│       ├── px_shader.rs                 # runtime validation/binding helpers
│       └── texture_buf.rs               # existing texture buffer handle
└── lps-filetests/
    ├── src/parse/                       # texture-spec / texture-data directives
    └── filetests/texture/               # behavior + diagnostics
```

### Data flow

```text
GLSL sampler2D uniform
        +
TextureBindingSpec map
        │
        ▼
lps-frontend validates and lowers texture calls
        │
        ▼
LPIR loads LpsTexture2DDescriptor fields from uniforms
        │
        ▼
format-specialized loads + conversion
        │
        ▼
LPVM backends execute on wasm / rv32c / rv32n
```

The ABI descriptor is:

```rust
#[repr(C)]
struct LpsTexture2DDescriptor {
    ptr: u32,
    width: u32,
    height: u32,
    row_stride: u32,
}
```

## Alternatives considered

- **Treat `sampler2D` as a plain user struct** — rejected because it leaks ABI
  details into metadata and diagnostics, and maps poorly to future WGSL.
- **Put filter/wrap policy in GLSL layout qualifiers or custom function names**
  — rejected because lpfx/domain already own visual context, and WGSL/wgpu
  models sampler policy outside shader source.
- **Start with palette lookup first** — rejected because `texelFetch` is the
  smallest operation that proves the binding, ABI, filetest, and backend path.
- **Add WGSL source support now** — rejected because texture access should not
  expand into source-language migration; WGSL belongs with a later real wgpu
  roadmap.

## Risks

- **Naga GLSL sampler representation** may differ from assumptions in
  `lps-frontend`, so the first milestone needs a focused frontend spike.
- **Uniform layout changes** for logical textures need care so existing
  scalar/struct uniforms keep working.
- **Filtered sampling cost** can be high on RV32; the roadmap needs a
  performance knob and filetests with tolerance.
- **Filetest fixture syntax** is new and must stay small/readable while
  remaining precise enough for exact unorm boundary tests.
- **Future wgpu parity** is a design target, not a current validation target;
  some behavior, especially filtering, may need tolerances or backend-specific
  notes later.

## Scope estimate

Eight milestones:

| # | Milestone | Strategy |
|---|-----------|----------|
| M1 | Texture interface foundation | Full plan |
| M2 | Texture filetest fixtures and diagnostics | Full plan |
| M3a | Texture-aware lowering contract | Full plan |
| M3b | Core `texelFetch` codegen | Full plan |
| M3c | Runtime validation and backend filetests | Full plan |
| M4 | Filtered `texture()` sampling and wrap modes | Full plan |
| M5 | Height-one palette lookup and lp-shader API integration | Small plan |
| M6 | Integration validation and cleanup | Small plan |

