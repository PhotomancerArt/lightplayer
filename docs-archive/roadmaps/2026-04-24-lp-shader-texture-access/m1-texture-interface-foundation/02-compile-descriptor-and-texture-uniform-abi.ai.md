# Scope of Phase

Add the `lp-shader` descriptor-based pixel compile API and typed texture
uniform descriptor helper.

Depends on phase 1 shared texture types.

In scope:

- Add `CompilePxDesc<'a>` with GLSL source, output format, compiler config, and
  texture binding specs.
- Add a descriptor-based compile method on `LpsEngine`.
- Keep existing `LpsEngine::compile_px(&str, TextureStorageFormat,
  &CompilerConfig)` as a compatibility wrapper with no texture specs.
- Add a typed `Texture2DUniform` descriptor built from `&LpsTextureBuf`.
- Re-export the new public API from `lp-shader`.
- Add focused tests for descriptor defaults/wrapper behavior and texture
  uniform descriptor layout/construction.

Out of scope:

- Frontend `sampler2D` metadata support.
- Texture spec validation against shader sampler uniforms.
- Runtime sampling.
- Runtime texture binding validation beyond descriptor construction.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

# Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/lib.rs`
- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/texture_buf.rs`
- new `lp-shader/lp-shader/src/compile_px_desc.rs`
- new `lp-shader/lp-shader/src/texture_uniform.rs`
- `lp-shader/lp-shader/src/tests.rs`

Add a new module for the compile descriptor. Because this crate is `no_std`,
use `alloc::collections::BTreeMap` and `alloc::string::String`.

Shape:

```rust
pub type TextureBindingSpecs = BTreeMap<String, TextureBindingSpec>;

pub struct CompilePxDesc<'a> {
    pub glsl: &'a str,
    pub output_format: TextureStorageFormat,
    pub compiler_config: CompilerConfig,
    pub textures: TextureBindingSpecs,
}
```

It is fine to add a small constructor such as `CompilePxDesc::new(...)` or a
helper for empty texture specs if it keeps call sites clear. Do not invent a
large builder unless the existing code suggests one.

Update `engine.rs`:

- Add a descriptor-based method such as
  `compile_px_desc(&self, desc: CompilePxDesc<'_>) -> Result<LpsPxShader, LpsError>`.
- Move the existing compile implementation into that descriptor path.
- Keep `compile_px(&self, glsl, output_format, config)` as a wrapper that builds
  a descriptor with empty textures and clones/copies the compiler config into
  the descriptor as needed.
- In this phase, the descriptor method does not need to validate texture specs
  against shader metadata yet. Leave that for phase 4.

Add `texture_uniform.rs`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture2DUniform {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}
```

Construct it from `&LpsTextureBuf`. Use `texture.guest_ptr()` for `ptr`, and
the texture buffer dimensions/row stride for the remaining fields. If
`LpsTextureBuf` lacks public `width()`, `height()`, or `format()` inherent
methods, prefer adding clear inherent accessors rather than relying on users to
import `TextureBuffer`.

Add a test that checks:

- `core::mem::size_of::<Texture2DUniform>() == 16`
- `core::mem::align_of::<Texture2DUniform>() == 4`
- A descriptor built from an allocated `LpsTextureBuf` has the expected
  dimensions and row stride.

Add a compile API test that confirms the old `compile_px` wrapper still compiles
a simple texture-free render shader. If practical, add a new descriptor method
test using an empty texture map for the same source.

# Validate

Run from the workspace root:

```bash
cargo test -p lp-shader
cargo check -p lp-shader
```

