# Scope Of Phase

Add or update focused backend coverage proving aligned texture `Load16U` works
on the main Q32 targets after the Cranelift workaround is removed.

In scope:

- Ensure existing M3b texture filetests cover `wasm.q32`, `rv32n.q32`, and
  `rv32c.q32` for aligned `R16Unorm`, `Rgb16Unorm`, and/or `Rgba16Unorm`
  channel reads.
- If current texture filetests already cover this clearly, avoid adding
  duplicate fixtures; update comments/expectations only if useful.
- Add one small targeted filetest only if existing coverage does not clearly
  prove aligned `Load16U` on both RV32 backends.

Out of scope:

- Adding misaligned texture fixtures as valid tests.
- Adding an unaligned LPIR load operation.
- Weakening or removing existing texture diagnostics.
- Changing the texture allocation alignment unless a test exposes a bug.

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

Relevant existing files:

- `lp-shader/lps-filetests/filetests/textures/texelfetch_rgba16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_rgb16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_r16_unorm.glsl`
- `lp-shader/lps-filetests/filetests/textures/texelfetch_clamp_bounds.glsl`
- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`

First inspect whether the existing texture filetests already:

- Include `rv32n.q32` and `rv32c.q32` targets.
- Read exact values through `texelFetch`.
- Exercise at least one `Load16U` channel from texture fixture storage.

If yes, prefer no new filetest. The main value of this phase is running the
same tests after phase 3 removes the workaround.

If a new focused test is needed, create a minimal texture file with:

- One `R16Unorm` or `Rgba16Unorm` texture.
- Explicit `// texture-spec:` and `// texture-data:` fixture.
- `// target:` or equivalent directives for `wasm.q32`, `rv32n.q32`, and
  `rv32c.q32`.
- Exact expected return value.

# Validate

Run the texture filetests:

```bash
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

If failures occur only on `rv32c.q32`, inspect whether Cranelift emitted an
illegal instruction or whether the emulator rejected a legal instruction. Do
not reintroduce the workaround without reporting the evidence.
