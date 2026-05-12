# Phase 6 — Texture Diagnostic Filetests

## Scope of Phase

Add positive and negative `.glsl` filetests that cover texture directive
parsing, compile-time texture spec validation, and runtime fixture validation.

Out of scope:

- Do not implement `texelFetch` or `texture` execution behavior.
- Do not add sidecar image fixtures.
- Do not add wgpu comparison tests.
- Do not change diagnostic semantics by weakening existing filetest assertions.

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
- Existing `lp-shader/lps-filetests/filetests/` layout.
- `lp-shader/lps-filetests/src/test_error/mod.rs`
- `lp-shader/lps-filetests/src/parse/parse_expected_error.rs`
- `lp-shader/lps-filetests/README.md`

Add a texture filetest directory if one does not already exist:

- `lp-shader/lps-filetests/filetests/textures/`

Add filetests for:

1. Minimal positive fixture setup
   - Use the example from `00-design.md`.
   - Mark sampling execution `@unimplemented(...)` for relevant targets until
     M3 implements `texelFetch`.
   - The test should still parse specs/data, validate interface, allocate
     fixture memory, and bind the descriptor before reaching unsupported
     sampling behavior.

2. Missing texture spec
   - Shader declares `uniform sampler2D inputColor;`.
   - No matching `texture-spec`.
   - Expect an error mentioning the missing sampler name.

3. Extra texture spec
   - `texture-spec` names `notInShader`.
   - Shader has no matching sampler uniform.
   - Expect an error mentioning the extra spec name.

4. Missing runtime fixture
   - Matching `texture-spec` exists.
   - No `texture-data` exists for the sampler.
   - Expect runtime fixture validation error.

5. Malformed fixture data
   - Cover a bad channel count or bad token shape.
   - Expect parser/fixture error.

6. Format mismatch
   - Spec says one format, fixture says another.
   - Expect mismatch diagnostic mentioning both or at least the sampler name.

7. Height-one promise mismatch
   - Spec uses `shape=height-one`.
   - Fixture has height greater than 1.
   - Expect height-one diagnostic.

8. Unsupported filter/wrap spellings
   - Use misspelled `filter=` and `wrap=`.
   - Expect line-aware parse errors.

Choose `// test error` versus `// test run` based on where the error is
reported:

- Compile/spec validation errors should work as `// test error` if Phase 4
  threaded texture validation into the error harness.
- Runtime fixture errors may be `// test run` failures if they occur before run
  execution. If the harness supports expected runtime error diagnostics, use
  that. If not, add minimal support rather than weakening the tests.

Keep the tests small. One file per behavior is preferred when it keeps expected
diagnostics easy to read.

Target annotations:

- Do not mark diagnostic tests `@unimplemented` just because sampling is not
  implemented. Diagnostics should fail before sampling.
- Positive sampling-like tests can be annotated `@unimplemented` until M3.

## Validate

Run from repo root:

```bash
cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

Also run at least one targeted command if a new texture file fails during
development, for example:

```bash
TEST_FILE=textures/<file-name>.glsl cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

