# Phase 5: Add Mainline Backend Exact-Value Filetests

## Scope of phase

Add or update texture filetests that prove `texelFetch` behavior on the three
mainline Q32 backends: `wasm.q32`, `rv32n.q32`, and `rv32c.q32`.

In scope:

- Convert/rename the M3a placeholder expected-error test into a positive exact
  value run test.
- Add exact-value tests for:
  - `Rgba16Unorm`
  - `Rgb16Unorm`
  - `R16Unorm`
  - clamp-to-edge out-of-range coordinates
- Add one focused compile-option test for unchecked bounds behavior, preferably
  via lowering/IR shape rather than unsafe runtime reads.
- Ensure all new runtime tests target `wasm.q32`, `rv32n.q32`, and `rv32c.q32`.
- Fix existing backend lowering gaps surfaced by those required targets when
  needed for existing LPIR ops, especially `Load16U` on Cranelift RV32.

Out of scope:

- Do not add broader backend matrix coverage beyond the three mainline Q32
  targets.
- Do not add runtime descriptor validation.
- Do not add filtered `texture()` tests.
- Do not add a new texture opcode or a new LPIR memory op.
- Do not weaken or remove existing negative diagnostics unless the valid
  placeholder test is being intentionally converted.

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

- `lp-shader/lps-filetests/filetests/textures/error_texelfetch_m3b_placeholder.glsl`
- Existing texture fixture tests in `lp-shader/lps-filetests/filetests/textures/`
- `lp-shader/lps-filetests/src/parse/test_type.rs` for directive syntax if
  needed
- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs` for fixture encoding
- `lp-shader/lps-filetests/tests/filetests.rs` for target selection if needed

Runtime filetest directives should target the three mainline Q32 backends. Use
the existing target directive syntax from nearby filetests. If the exact syntax
is unclear, inspect existing `.glsl` tests that use target restrictions.

Suggested positive files:

- `texelfetch_rgba16_unorm.glsl`
- `texelfetch_rgb16_unorm.glsl`
- `texelfetch_r16_unorm.glsl`
- `texelfetch_clamp_bounds.glsl`

Convert or rename:

- `error_texelfetch_m3b_placeholder.glsl`

The converted minimal RGBA test should include:

- `// test run`
- target selection for `wasm.q32`, `rv32n.q32`, and `rv32c.q32`
- `// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d`
- `// texture-data: inputColor ...`
- `uniform sampler2D inputColor;`
- functions that return sampled scalar channels or a `vec4` if filetest return
  matching supports it
- `// run:` lines asserting exact or near-exact values

Prefer scalar channel-return helper functions if they make the filetest
expectations simpler and more robust:

```glsl
float sample_r() {
    return texelFetch(inputColor, ivec2(1, 0), 0).r;
}
```

Suggested coverage:

- RGBA: prove all four channels load from storage.
- RGB: prove alpha fills to `1.0`.
- R: prove G/B fill to `0.0` and A fills to `1.0`.
- Clamp: use negative and too-large coordinates and prove they sample edge
  texels, not arbitrary memory.

Compiler option test:

- Use `// compile-opt(texture.texel_fetch_bounds, unchecked)` in a test only if
  it can be validated without intentionally reading out of bounds.
- Prefer adding a frontend unit test in `lps-frontend` that compares safe vs
  unchecked LPIR shape, if phase 3 did not already add one.
- Do not add a runtime out-of-bounds unchecked test.

Cranelift RV32 support:

- If `rv32c.q32` fails because Cranelift RV32 cannot lower existing `Load16U`
  operations for texture reads, fix that backend lowering as part of this phase.
- Keep the fix scoped to `lpvm-cranelift` support for existing LPIR `Load16U`.
- Texture storage can be 2-byte aligned while RV32 word loads require 4-byte
  alignment, so a target-specific decomposition using aligned word loads and
  bit extraction is acceptable.

Update negative tests:

- Remove or rename the placeholder expected-error file so the suite no longer
  expects the M3b placeholder diagnostic.
- Keep missing spec, nonzero LOD, dynamic LOD, and malformed fixture diagnostics.

## Validate

Run from workspace root:

```bash
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
cargo check -p lpvm-cranelift -p lps-filetests
```

The older command `cargo test -p lps-filetests --test filetests -- textures`
does not run the ignored filetest harness; use the `TEST_FILE=textures` command
above.

