# Phase 2: Wire Texture-Aware Lowering Callers

## Scope of phase

Update existing texture-aware compile paths to pass texture specs into the new frontend lowering API.

In scope:

- Update `lp-shader::LpsEngine::compile_px_desc` to call `lps_frontend::lower_with_options`.
- Update `lps-filetests` run compile path to call `lower_with_options`.
- Update `lps-filetests` GLSL error diagnostic path to call `lower_with_options`.
- Preserve existing post-lower texture-spec validation in `lp-shader` and
  `lps-filetests`; `lower_with_options` validates non-empty maps, but callers
  still need the shared validation to catch missing specs when the map is empty
  and the shader declares a sampler.
- Keep texture-free callers using `lps_frontend::lower` where appropriate.
- Add/update narrow tests that prove caller wiring preserves missing/extra texture-spec diagnostics.

Out of scope:

- Do not implement `texelFetch` recognition or `lower_texture.rs`.
- Do not change public `CompilePxDesc` shape.
- Do not change runtime texture binding or fixture allocation behavior.
- Do not implement M3b data-path codegen.

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

- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/compile_px_desc.rs`
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- `lp-shader/lps-filetests/src/test_error/mod.rs`
- `lp-shader/lps-filetests/src/test_run/compile.rs`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-design.md`

This phase depends on Phase 1 having added:

- `lps_frontend::LowerOptions`
- `lps_frontend::lower_with_options`
- `LpsModuleSig::texture_specs`

### `lp-shader`

In `LpsEngine::compile_px_desc`, replace:

```rust
let (mut ir, mut meta) =
    lps_frontend::lower(&naga).map_err(|e| LpsError::Lower(format!("{e}")))?;
...
validate_texture_interface(&meta, &textures)?;
```

with an options-aware lower call. Suggested shape:

```rust
let lower_options = lps_frontend::LowerOptions {
    texture_specs: textures.clone(),
};
let (mut ir, mut meta) =
    lps_frontend::lower_with_options(&naga, &lower_options)
        .map_err(|e| LpsError::Lower(format!("{e}")))?;
```

Keep `validate_texture_interface(&meta, &textures)?`. Phase 1 intentionally keeps
`lower()` usable for sampler metadata without specs and makes
`lower_with_options` validate only non-empty maps. The `lp-shader` compile path
therefore still needs its existing validation to catch:

- shader declares a sampler but `CompilePxDesc::textures` is empty,
- shader/spec mismatches in cases that do not reach texture-operation lowering.

### `lps-filetests`

In `CompiledShader::compile_glsl`, replace the helper that calls `lps_frontend::lower(&naga)` and then validates specs with a texture-aware lowering call.

Prefer keeping a helper such as:

```rust
fn lower_glsl(
    source: &str,
    texture_specs: &BTreeMap<String, TextureBindingSpec>,
) -> anyhow::Result<(LpirModule, LpsModuleSig)> {
    let naga = lps_frontend::compile(source).map_err(|e| anyhow::anyhow!("{e}"))?;
    let options = lps_frontend::LowerOptions {
        texture_specs: texture_specs.clone(),
    };
    lps_frontend::lower_with_options(&naga, &options).map_err(|e| anyhow::anyhow!("{e}"))
}
```

Keep the direct call to
`lps_shared::validate_texture_binding_specs_against_module` after lowering.
Reason: filetests must still catch missing texture specs in shader files that
declare a sampler but do not call `texelFetch`. `lower_with_options` supplies
spec metadata to texture-operation lowering, but the harness's existing
file-level interface validation remains part of M2/M3a behavior.

In `lps-filetests/src/test_error/mod.rs`, update `collect_glsl_error_test_diagnostics` so texture specs are passed into frontend lowering. It should not lower with an empty map and then separately validate if M3a requires lower-time diagnostics for `texelFetch`.

### Tests

Update existing tests that expect texture spec validation errors if the error path moved from post-lower validation into `LowerError`. Keep the user-visible diagnostic text stable enough for existing filetests.

At minimum, run existing texture diagnostic filetests that exercise:

- missing spec for declared sampler,
- extra spec for nonexistent sampler.

## Validate

Run:

```bash
cargo test -p lp-shader -p lps-filetests
```

If this is too broad or package names differ, run the closest crate-specific commands and report exactly what passed. Do not skip failing tests by weakening them.

