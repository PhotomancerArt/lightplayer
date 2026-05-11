# Scope Of Phase

Make the backend matrix coverage and the design-doc texture fixture test
explicit for M3c.

In scope:

- Replace or update the design-doc-only fixture smoke so it performs a real
  `texelFetch` and asserts exact behavior.
- Confirm positive `texelFetch` texture tests cover:
  - `R16Unorm`
  - `Rgb16Unorm`
  - `Rgba16Unorm`
  - default target matrix: `rv32n.q32`, `rv32c.q32`, `wasm.q32`
- Add or tighten comments only where they improve future readability.
- Run the texture filetest command that exercises ignored filetests.

Out of scope:

- New texture formats.
- Runtime validation implementation.
- Parser/harness changes not needed for the design-doc fixture update.
- Any change to default target selection.

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
- `lp-shader/lps-filetests/filetests/textures/positive_minimal_fixture_design_doc.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_r16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_rgb16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_rgba16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_clamp_bounds.glsl`

Design-doc fixture:

- The file `positive_minimal_fixture_design_doc.glsl` currently exists to prove
  fixture parsing/binding but does not assert a texture read.
- Update it to use `texelFetch` on its fixture and assert exact output.
- Keep it minimal and readable; it should remain a small design-doc-style
  example.
- Use current directive syntax and existing value assertion patterns.

Backend matrix:

- Do not add explicit `// target:` directives unless necessary.
- The texture harness default targets are expected to include `rv32n.q32`,
  `rv32c.q32`, and `wasm.q32`.
- If comments already state default matrix coverage, keep them accurate.
- If a positive texture file lacks a helpful comment, add a short ASCII-only
  comment. Do not churn comments unnecessarily.

Validation expectation:

- The texture filetest output should show each positive run expectation passing
  three times where default targets apply.

# Validate

Run:

```bash
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```
