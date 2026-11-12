# Phase 1: Add Texture Bounds Compiler Option

## Scope of phase

Add a compiler option that controls whether `texelFetch` bounds clamps are
generated. The default must be safe (`ClampToEdge`). The explicit opt-out is
`Unchecked`, for performance measurement only.

In scope:

- Add texture bounds config types to `lp-shader/lpir/src/compiler_config.rs`.
- Add `CompilerConfig::texture`.
- Add `CompilerConfig::apply("texture.texel_fetch_bounds", ...)` parsing.
- Add tests for defaults, parsing, display/from-str round-trip, and invalid
  values.
- Thread the setting into frontend lowering options:
  - `lp-shader/lps-frontend/src/lower.rs`
  - `lp-shader/lps-frontend/src/lower_ctx.rs`
  - `lp-shader/lp-shader/src/engine.rs`
  - `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
  - `lp-shader/lps-filetests/src/test_error/mod.rs` if it constructs
    `LowerOptions`

Out of scope:

- Do not implement texel address math or channel loads.
- Do not remove the M3a placeholder diagnostic.
- Do not add runtime validation or public texture binding APIs.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away. Fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Read:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/00-design.md`
- `lp-shader/lpir/src/compiler_config.rs`
- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- `lp-shader/lps-filetests/src/test_error/mod.rs`

Add config types in `lp-shader/lpir/src/compiler_config.rs` near the existing
`InlineConfig` and `InlineMode` types:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureConfig {
    pub texel_fetch_bounds: TexelFetchBoundsMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexelFetchBoundsMode {
    ClampToEdge,
    Unchecked,
}
```

Expected behavior:

- `CompilerConfig::default().texture.texel_fetch_bounds ==
  TexelFetchBoundsMode::ClampToEdge`.
- `TexelFetchBoundsMode` parses lowercase strings:
  - `"clamp-to-edge"` => `ClampToEdge`
  - `"unchecked"` => `Unchecked`
- `Display` returns the same lowercase strings.
- `CompilerConfig::apply("texture.texel_fetch_bounds", "unchecked")` sets
  unchecked mode.
- Invalid values return `ConfigError::InvalidValue`.

Update `lp-shader/lpir/src/lib.rs` re-exports if needed so downstream crates can
name `TextureConfig` and `TexelFetchBoundsMode` as `lpir::...`.

Update `lps_frontend::LowerOptions` in `lp-shader/lps-frontend/src/lower.rs`:

```rust
pub struct LowerOptions {
    pub texture_specs: BTreeMap<String, TextureBindingSpec>,
    pub texel_fetch_bounds: lpir::TexelFetchBoundsMode,
}
```

Make `LowerOptions::default()` keep `texture_specs` empty and
`texel_fetch_bounds` at `ClampToEdge`. Existing tests and call sites that use
struct literals must be updated.

Update `LowerCtx` in `lp-shader/lps-frontend/src/lower_ctx.rs` to store the
mode, so `lower_texture.rs` can branch on it in later phases:

```rust
pub(crate) texel_fetch_bounds: lpir::TexelFetchBoundsMode,
```

Update the lowering call path to pass this mode from `LowerOptions` to
`LowerCtx::new`.

Update existing call sites:

- `lp-shader/lp-shader/src/engine.rs`: when constructing `LowerOptions`, set
  `texel_fetch_bounds: compiler_config.texture.texel_fetch_bounds`.
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`: lower with the
  `compiler_config.texture.texel_fetch_bounds` value. This likely means changing
  the helper that currently only receives `texture_specs` so it can also receive
  `compiler_config` or just the texture bounds mode.
- `lp-shader/lps-filetests/src/test_error/mod.rs`: if this path has no
  `CompilerConfig`, use the safe default unless parsed compile opts are already
  available there.
- Unit tests in `lps-frontend` that construct `LowerOptions { texture_specs }`
  should add `..Default::default()` or the explicit safe mode.

Add tests in `compiler_config.rs`:

- default texture bounds is `ClampToEdge`
- apply `texture.texel_fetch_bounds = unchecked`
- apply `texture.texel_fetch_bounds = clamp-to-edge`
- invalid value errors
- parse/display round trip

Do not implement clamp code in this phase. Later phases will use
`ctx.texel_fetch_bounds`.

## Validate

Run from workspace root:

```bash
cargo test -p lpir compiler_config
cargo check -p lps-frontend -p lp-shader -p lps-filetests
```

