# Phase 1: LowerOptions and Metadata Retention

## Scope of phase

Add the frontend API and shared metadata foundation for texture-aware lowering.

In scope:

- Add `LowerOptions` and `lower_with_options` to `lps-frontend`.
- Keep `lps_frontend::lower(&NagaModule)` as the zero-config convenience wrapper.
- Add texture-spec retention to `LpsModuleSig`, defaulting to an empty map.
- Validate texture specs during `lower_with_options` using the existing shared
  validation helper.
- Add/update focused unit tests for the API and retained metadata.

Out of scope:

- Do not update `lp-shader` or `lps-filetests` callers in this phase.
- Do not add `lower_texture.rs` or inspect/lower `texelFetch` expressions.
- Do not implement descriptor loads, address math, `Load16U`, or
  `Unorm16toF`.
- Do not add runtime validation for `LpsTextureBuf`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back what changed, what was validated, and any deviations from this phase plan.

## Implementation Details

Read these files first:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-design.md`
- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/lib.rs`
- `lp-shader/lps-shared/src/sig.rs`
- `lp-shader/lps-shared/src/texture_binding_validate.rs`
- `lp-shader/lps-shared/src/texture_format.rs`

### Shared metadata

Update `LpsModuleSig` in `lp-shader/lps-shared/src/sig.rs` so it retains texture specs:

```rust
pub struct LpsModuleSig {
    pub functions: Vec<LpsFnSig>,
    pub uniforms_type: Option<LpsType>,
    pub globals_type: Option<LpsType>,
    pub texture_specs: BTreeMap<String, TextureBindingSpec>,
}
```

Use `alloc::collections::BTreeMap` and import `TextureBindingSpec` from the same crate. Keep `Default`, `Clone`, `Debug`, `PartialEq`, and `Eq` working.

Update existing struct literals/tests that construct `LpsModuleSig` manually to include `texture_specs: BTreeMap::new()` or `..Default::default()` as appropriate.

### Frontend options API

In `lp-shader/lps-frontend/src/lower.rs`, add a public options struct. The exact lifetime shape can be adjusted, but prefer an owned default-friendly shape over a static empty `BTreeMap` if that is simpler in `no_std + alloc`:

```rust
#[derive(Clone, Debug, Default)]
pub struct LowerOptions {
    pub texture_specs: BTreeMap<String, TextureBindingSpec>,
}
```

This owned form is acceptable because compile descriptors already own a map and can clone it at the frontend boundary. If you choose a borrowed form instead, keep `Default` ergonomic and do not introduce `std`.

Add:

```rust
pub fn lower(naga_module: &NagaModule) -> Result<(LpirModule, LpsModuleSig), LowerError> {
    lower_with_options(naga_module, &LowerOptions::default())
}

pub fn lower_with_options(
    naga_module: &NagaModule,
    options: &LowerOptions,
) -> Result<(LpirModule, LpsModuleSig), LowerError> {
    ...
}
```

Refactor the current body of `lower` into `lower_with_options`.

After `compute_global_layout` and before lowering functions, validate:

```rust
let mut glsl_meta = LpsModuleSig {
    uniforms_type,
    globals_type,
    texture_specs: options.texture_specs.clone(),
    ..Default::default()
};
lps_shared::validate_texture_binding_specs_against_module(&glsl_meta, &options.texture_specs)
    .map_err(LowerError::UnsupportedExpression)?;
```

The exact error variant can be `UnsupportedExpression` or `Internal` if existing conventions suggest otherwise, but the message must preserve the shared validation text so filetests can match sampler names.

Export `LowerOptions` and `lower_with_options` from `lp-shader/lps-frontend/src/lib.rs`.

### Tests

Add focused tests in `lps-frontend` or `lps-shared`:

- Existing `lower(&naga)` callers still work and produce empty `meta.texture_specs`.
- `lower_with_options` with a matching `sampler2D` spec succeeds and stores the spec in `meta.texture_specs`.
- `lower_with_options` with a missing spec for a `sampler2D` returns an error containing the sampler name.
- `lower_with_options` with an extra spec for a shader with no sampler returns an error containing the extra name.

Use existing `sampler2d_metadata_tests.rs` style where possible.

## Validate

Run:

```bash
cargo test -p lps-shared -p lps-frontend
```

If package names differ locally, use the workspace package names for the same crates. Report the exact command and result.

