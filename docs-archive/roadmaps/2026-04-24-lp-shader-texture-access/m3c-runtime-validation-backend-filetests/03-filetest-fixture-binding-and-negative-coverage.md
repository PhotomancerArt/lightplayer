# Scope Of Phase

Update texture filetest fixture binding to use the typed host texture value and
fill any remaining negative runtime/setup coverage gaps.

In scope:

- Ensure `lps-filetests` binds texture fixtures through the typed host texture
  value introduced in phase 1.
- Preserve existing fixture/spec validation behavior.
- Add negative filetests only for M3c runtime/setup scenarios not already
  covered.
- Keep filetest diagnostics precise enough for `EXPECT_SETUP_FAILURE` or
  existing error expectations.

Out of scope:

- Public `lp-shader::render_frame` unit tests; those belong to phase 2.
- New parser syntax unless an existing directive cannot express the needed
  negative coverage.
- Backend codegen changes.
- New texture formats.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from the
  phase plan.

# Implementation Details

Read these first:

- `00-notes.md` and `00-design.md` in this plan directory.
- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- Existing files under `lp-shader/lps-filetests/filetests/textures/`
- `lp-shader/lps-filetests/src/parse/parse_expect_setup_failure.rs`

Fixture binding:

- `bind_texture_fixtures_for_run` should construct the typed host texture value
  with:
  - descriptor from allocated guest memory and encoded fixture dimensions
  - format from the encoded fixture
  - byte length from the encoded byte vector or allocated buffer size
- Pass `LpsValueF32::Texture2D(value)` to `inst.set_uniform`.
- Do not manually write raw `UVec4` descriptor values.
- Keep `validate_runtime_texture_fixtures` as the harness-level check for:
  missing fixture, extra fixture, format mismatch, `shape=height-one` fixture
  height mismatch.

Negative coverage:

- First inventory existing texture files. Avoid duplicating cases already
  covered by:
  - `run_missing_texture_fixture.glsl`
  - `run_texture_format_mismatch.glsl`
  - `run_texture_height_one_mismatch.glsl`
  - `run_malformed_fixture_normalized_float.glsl`
  - parser error tests for malformed fixture syntax
- Add only focused missing cases if phase 1/2 creates a new setup/runtime
  failure mode expressible in filetests.
- Prefer `// EXPECT_SETUP_FAILURE: {{...}}` for harness binding/setup failures.

Keep all texture fixture tests readable. Do not broaden the test directory
outside `filetests/textures/`.

# Validate

Run:

```bash
cargo test -p lps-filetests
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```
