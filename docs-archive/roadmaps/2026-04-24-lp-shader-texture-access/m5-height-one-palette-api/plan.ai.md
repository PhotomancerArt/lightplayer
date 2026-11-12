# Design

## Scope of Work

Milestone 5 packages the already-implemented `TextureShapeHint::HeightOne`
sampling path for palette/gradient use through the high-level `lp-shader` API.

In scope:

- Add ergonomic `lp-shader` helpers so callers can construct texture binding
  specs and uniform values from `LpsTextureBuf` without hand-authoring raw
  descriptor/value structs.
- Make `HeightOne` easy and explicit for palette/gradient bindings.
- Add public `lp-shader` tests that compile a shader using `texture()`, bind a
  height-one input texture through the public API, render to an output buffer,
  and verify expected sampled pixels.
- Document the boundary for higher layers: `lp-shader` exposes buffer/spec/value
  helpers and runtime validation; lpfx/domain remain responsible for baking
  palette stops and choosing specs.

Out of scope:

- New GLSL sampling semantics or sampler builtin implementations.
- New texture formats.
- Palette stop interpolation/baking from TOML or domain values.
- lp-domain schema changes.
- wgpu runtime/source support.
- Changing `LpsPxShader::render_frame()`.

## File Structure

```text
lp-shader/lp-shader/src/
├── lib.rs                 # UPDATE: re-export helper types/functions
├── compile_px_desc.rs     # UPDATE: texture spec convenience methods
├── texture_buf.rs         # UPDATE: named texture uniform helper
└── tests.rs               # UPDATE: public HeightOne texture() render tests
```

## Conceptual Architecture

```text
lpfx/domain
  ├─ bakes palette/gradient bytes into LpsTextureBuf(height=1)
  └─ chooses binding policy
        │
        ▼
lp-shader helper layer
  ├─ TextureBindingSpec helper constructors
  │    ├─ general Texture2D spec
  │    └─ height-one palette/gradient spec
  ├─ CompilePxDesc::with_texture(...)
  └─ LpsTextureBuf::to_named_texture_uniform("palette")
        │
        ▼
existing runtime
  ├─ LpsPxShader::render_frame(uniforms, output)
  ├─ runtime texture validation (`height == 1` for HeightOne)
  └─ M4 texture() lowering selects texture1d_* builtins
```

## Main Components

### Texture Binding Spec Helpers

Add small public helpers near `CompilePxDesc` / existing texture API rather
than a large builder. The helpers should make the common cases clear:

- a general 2D texture spec;
- a height-one palette/gradient spec;
- adding a named spec to `CompilePxDesc`.

They should still expose the existing knobs that matter for sampling:
`TextureStorageFormat`, `TextureFilter`, `TextureWrap`, and
`TextureShapeHint`. Avoid hiding the format/filter/wrap contract from callers.

### Texture Uniform Helpers

Keep `LpsPxShader::render_frame()` unchanged. Add thin helpers around the
existing `LpsTextureBuf::to_texture2d_value()` path so consumers can produce a
named uniform field without hand-writing:

```rust
(
    String::from("palette"),
    LpsValueF32::Texture2D(buf.to_texture2d_value()),
)
```

The helper should not own or clone texture bytes; it only packages the existing
descriptor/value metadata into the shape `render_frame()` already accepts.

### Public HeightOne Render Coverage

Add a `lp-shader` API test that:

- compiles a shader using ordinary `texture(palette, vec2(u, varying_y))`;
- registers the binding as `TextureShapeHint::HeightOne`;
- allocates an `Rgba16Unorm` input texture with `height == 1`;
- writes palette-like color stops directly into `LpsTextureBuf::data_mut()`;
- binds it via the new helper;
- renders into an output texture through `LpsPxShader::render_frame()`;
- verifies the expected sampled color and that changing `uv.y` does not affect
  the result.

# Phases

## Phase 1: Public Helper API

[sub-agent: yes]

### Scope of Phase

Add thin public helper APIs for texture binding specs and named texture uniform
values.

In scope:

- `TextureBindingSpec` convenience constructors or functions for general 2D and
  height-one palette/gradient specs.
- `CompilePxDesc` helper method(s) for adding named texture specs.
- `LpsTextureBuf` helper for creating a named `LpsValueF32::Texture2D` uniform
  field.
- Public re-exports in `lp-shader/src/lib.rs` if needed.
- Focused unit tests for helper outputs.

Out of scope:

- Changing `LpsPxShader::render_frame()`.
- New GLSL helper builtins.
- Palette stop baking or domain-level APIs.
- New texture formats.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/compile_px_desc.rs`
- `lp-shader/lp-shader/src/texture_buf.rs`
- `lp-shader/lp-shader/src/lib.rs`
- `lp-shader/lp-shader/src/tests.rs`

Suggested API shape:

```rust
impl TextureBindingSpec {
    pub fn texture2d(
        format: TextureStorageFormat,
        filter: TextureFilter,
        wrap_x: TextureWrap,
        wrap_y: TextureWrap,
    ) -> Self { ... }

    pub fn height_one(
        format: TextureStorageFormat,
        filter: TextureFilter,
        wrap_x: TextureWrap,
    ) -> Self { ... }
}
```

If orphan rules prevent inherent impls because `TextureBindingSpec` is defined
in `lps-shared`, prefer public free functions or a small exported helper type in
`lp-shader`; do not move shared ownership unless clearly necessary.

Suggested `CompilePxDesc` helper:

```rust
impl<'a> CompilePxDesc<'a> {
    pub fn with_texture_spec(mut self, name: impl Into<String>, spec: TextureBindingSpec) -> Self {
        self.textures.insert(name.into(), spec);
        self
    }
}
```

Suggested `LpsTextureBuf` helper:

```rust
impl LpsTextureBuf {
    pub fn to_named_texture_uniform(&self, name: impl Into<String>) -> (String, LpsValueF32) {
        (name.into(), LpsValueF32::Texture2D(self.to_texture2d_value()))
    }
}
```

Add tests in `lp-shader/lp-shader/src/tests.rs` that assert:

- the height-one helper produces `TextureShapeHint::HeightOne`;
- the 2D helper produces `TextureShapeHint::General2D`;
- `with_texture_spec` inserts the named binding;
- `to_named_texture_uniform` returns the correct name and `LpsValueF32::Texture2D`
  value.

### Validate

Run:

```bash
cargo test -p lp-shader texture
cargo check -p lp-shader
```

## Phase 2: HeightOne Public Render Test

[sub-agent: yes]

### Scope of Phase

Add public API render coverage proving palette-like `HeightOne + texture()`
sampling works through `LpsEngine`, `CompilePxDesc`, `LpsTextureBuf`, and
`LpsPxShader::render_frame()`.

In scope:

- One or more tests in `lp-shader/lp-shader/src/tests.rs`.
- Test-only helpers for writing RGBA16 UNORM texels if needed.
- Assertions that `uv.y` is ignored for height-one sampling.

Out of scope:

- New helper APIs beyond what Phase 1 added.
- New filetest GLSL coverage; M4 already has filetests for the sampler path.
- Palette stop baking or interpolation APIs.
- Backend-specific changes.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/tests.rs`
- helper APIs from Phase 1

Add a test shaped like:

```rust
#[test]
fn render_frame_height_one_palette_texture_samples_through_public_api() {
    let engine = test_engine();
    let glsl = r#"
uniform sampler2D palette;
vec4 render(vec2 pos) {
    float u = 0.5;
    float y = pos.y; // deliberately varies by output pixel
    return texture(palette, vec2(u, y));
}
"#;

    let desc = CompilePxDesc::new(
        glsl,
        TextureStorageFormat::Rgba16Unorm,
        lpir::CompilerConfig::default(),
    )
    .with_texture_spec(
        "palette",
        /* height-one Rgba16Unorm nearest/clamp spec helper */,
    );

    let shader = engine.compile_px_desc(desc).expect("compile");
    let mut palette = engine
        .alloc_texture(2, 1, TextureStorageFormat::Rgba16Unorm)
        .expect("palette");
    // write two RGBA16 colors; sample u=0.5 should select/resolve expected color

    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![palette.to_named_texture_uniform("palette")],
    };

    let mut out = engine
        .alloc_texture(1, 2, TextureStorageFormat::Rgba16Unorm)
        .expect("out");
    shader.render_frame(&uniforms, &mut out).expect("render");

    // Both output rows should match, proving y is ignored by HeightOne lowering.
}
```

Use nearest filtering for the most stable public API assertion. It is fine to
add a second small linear test only if it stays simple and low-noise.

### Validate

Run:

```bash
cargo test -p lp-shader height_one
cargo test -p lp-shader texture
```

## Phase 3: Cleanup, Docs, And Validation

[sub-agent: supervised]

### Scope of Phase

Clean up the M5 helper API work, document the higher-layer boundary, and run
focused validation.

In scope:

- Add docs/comments to the new helper APIs explaining that lpfx/domain provide
  baked palette data and matching `HeightOne` specs.
- Move useful planning notes to the bottom of this file.
- Add `# Decisions for future reference`.
- Run focused and broad-enough validation.

Out of scope:

- New product/domain APIs.
- wgpu support.
- New sampling behavior.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Review the diff for:

- unnecessary helper abstractions;
- raw descriptor construction leaking into public tests;
- TODOs, debug prints, `dbg!`, `todo!`, `unimplemented!`;
- new `#[allow(...)]` attributes;
- tests that only check construction and not public render behavior.

Append a `# Decisions for future reference` section. Likely decisions:

- M5 does not add a new GLSL helper; `texture()` + `HeightOne` is the public
  shader surface.
- `render_frame()` remains unchanged; helpers package inputs around the
  existing uniform struct API.

### Validate

Run:

```bash
cargo test -p lp-shader texture
cargo check -p lp-shader
just check
```

# Notes

- Confirmation answers: all suggested answers accepted.
- M4 already implemented optimized `texture(sampler2D, vec2)` lowering for
  `TextureShapeHint::HeightOne`: frontend lowering selects `texture1d_*`
  builtins, passes only `uv.x` and `wrap_x`, and intentionally drops `uv.y` /
  `wrap_y`.
- Runtime validation already rejects `HeightOne` bindings whose runtime texture
  height is not `1`.
- `LpsTextureBuf` already provides `to_texture2d_descriptor()` and
  `to_texture2d_value()`.
- `LpsPxShader::render_frame()` already accepts texture uniforms when callers
  pass `LpsValueF32::Texture2D(buf.to_texture2d_value())` inside a matching
  `LpsValueF32::Struct`.
- Existing `lp-shader` API tests cover texture binding through public
  `render_frame()` for `texelFetch`, and validation failures for missing,
  mismatched, badly shaped, and badly aligned texture values.

# Decisions for future reference

- **Shader surface:** M5 adds no new GLSL builtin or sampler helper; callers use ordinary `texture()` with compile-time `TextureShapeHint::HeightOne` via `lp_shader::texture_binding::height_one`.
- **`render_frame`:** Unchanged; helpers only assemble `CompilePxDesc` texture maps and `(String, LpsValueF32)` uniform fields consumed by the existing struct uniform API.
- **Ownership:** Palette stop interpolation/baking stays in lpfx/domain (or other embedders); `lp-shader` exposes buffer/spec/value wiring and runtime validation only.
- **Tests:** Negative-path specs use `CompilePxDesc::with_texture_spec` and `texture_binding::*` helpers where possible so public tests mirror embedder usage; deliberate corruption tests still tweak `LpsTexture2DValue` fields.


