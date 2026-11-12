# Phase 4 — Compile-Time Spec Validation In Filetests

## Scope of Phase

Thread parsed `texture-spec` directives into `lps-filetests` compilation and
validate them against lowered shader metadata.

Out of scope:

- Do not allocate runtime texture fixture memory.
- Do not bind `texture-data` fixtures before runs.
- Do not implement sampling behavior.
- Do not add broad diagnostic fixture files beyond targeted validation tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Read first:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m2-texture-filetests/00-design.md`
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- `lp-shader/lps-filetests/src/test_run/compile.rs`
- `lp-shader/lps-filetests/src/test_run/run_detail.rs`
- `lp-shader/lps-filetests/src/test_error/mod.rs`
- `lp-shader/lp-shader/src/texture_interface.rs`

Current state:

- `lps-filetests` compiles GLSL by calling `lps_frontend::compile` and
  `lps_frontend::lower` directly.
- M1 texture interface validation lives in `lp-shader/src/texture_interface.rs`
  and is private to `lp-shader`.
- `lps-filetests` should validate the same contract without invoking pixel
  shader synthesis.

Required validation:

- Every `LpsType::Texture2D` uniform in `LpsModuleSig::uniforms_type` has a
  matching `texture-spec` entry.
- Every `texture-spec` name matches a declared `Texture2D` uniform.
- If there are no texture uniforms and no specs, validation succeeds.
- If uniforms metadata is malformed, return a clear validation error.

Implementation options:

1. Move or duplicate the small validator into a shared place usable by both
   `lp-shader` and `lps-filetests`.
2. Add a dependency from `lps-filetests` to `lp-shader` only if that does not
   create an undesirable dependency cycle or pull in unrelated pixel shader
   behavior.

Suggested direction:

- Prefer sharing the validation helper without requiring `lps-filetests` to use
  `LpsEngine::compile_px_desc`.
- A good target is a small helper in `lps-shared` if it can stay free of
  `LpsError`; return a lightweight shared error or `Result<(), String>`.
- If moving the helper is too invasive for this phase, mirror the M1 validator
  in `lps-filetests` and leave a short comment explaining that the semantics
  intentionally match `lp-shader`.

Threading:

- Change `CompiledShader::compile_glsl` or its caller to receive the parsed
  texture specs.
- After `lower_glsl` returns `(ir, meta)`, validate `meta` against the parsed
  specs before compiling the backend module.
- Ensure compile failure reporting in `run_detail.rs` surfaces validation errors
  clearly.
- Extend `test_error` or add a small helper path so `// test error` can also
  validate texture specs and match expected errors. Do not bypass existing
  `expected-error` behavior.

Tests:

- Unit test validation helper:
  - missing spec errors with sampler name.
  - extra spec errors with spec name.
  - matching spec succeeds.
- Add small parser/compile tests if the harness already has a convenient test
  layer.

Keep messages stable enough for filetests, for example:

- `no texture binding spec for shader sampler 'inputColor'`
- `texture binding spec 'extraTex' does not match any shader sampler2D uniform`

Use the repo's existing quote style in diagnostics if it differs; consistency
matters more than the exact quote character.

## Validate

Run from repo root:

```bash
cargo test -p lps-filetests texture
cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

If the full ignored filetest run is too slow locally, report that and run a
targeted filetest command with `TEST_FILE=<texture diagnostic path>` after
Phase 6 adds fixtures.

