# Scope of Phase

Add the shared texture interface vocabulary and logical texture type in
`lps-shared`.

In scope:

- Add `TextureBindingSpec`, `TextureFilter`, `TextureWrap`, and
  `TextureShapeHint` near `TextureStorageFormat`.
- Add `LpsType::Texture2D`.
- Give `LpsType::Texture2D` std430 ABI layout size/alignment `16`/`4`.
- Ensure texture type path resolution behaves as a leaf type.
- Re-export the new shared types from `lps-shared`.
- Add focused unit tests.

Out of scope:

- `lp-shader` compile descriptor APIs.
- Frontend `sampler2D` parsing/lowering.
- Runtime texture sampling or binding behavior.

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

- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lps-shared/src/types.rs`
- `lp-shader/lps-shared/src/layout.rs`
- `lp-shader/lps-shared/src/path_resolve.rs`
- `lp-shader/lps-shared/src/lib.rs`

Add the binding vocabulary next to `TextureStorageFormat`:

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

Use derives consistent with `TextureStorageFormat`: at least
`Debug, Clone, Copy, PartialEq, Eq, Hash` for the enums. `TextureBindingSpec`
should derive `Debug, Clone, Copy, PartialEq, Eq, Hash` as well unless an
implementation detail prevents that.

Add `LpsType::Texture2D` to `types.rs`. It is a logical shader type for GLSL
`sampler2D`; do not model it as a public struct with `ptr`, `width`, `height`,
and `row_stride` fields.

Update `layout.rs`:

- `type_size(&LpsType::Texture2D, LayoutRules::Std430) == 16`
- `type_alignment(&LpsType::Texture2D, LayoutRules::Std430) == 4`
- arrays/structs containing `Texture2D` should naturally use those rules.

Update `path_resolve.rs` only as needed for exhaustiveness and clarity.
`Texture2D` should behave as a leaf:

- Empty path returns the texture type.
- Field path such as `tex.ptr` should fail; metadata must not expose ABI fields.
- Index path such as `tex[0]` should fail unless the type is inside an array.

Update `lps-shared/src/lib.rs` to re-export the new shared types.

Tests to add:

- Binding spec values are constructible and comparable.
- `Texture2D` std430 size/alignment is `16`/`4`.
- A struct containing a float, texture, and float has expected offsets and size
  under existing std430 layout behavior.
- Path resolution rejects fields on `Texture2D`.

# Validate

Run from the workspace root:

```bash
cargo test -p lps-shared
cargo check -p lps-shared
```

