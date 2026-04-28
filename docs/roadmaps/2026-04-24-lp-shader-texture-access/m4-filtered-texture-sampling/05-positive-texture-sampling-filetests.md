# Scope of Phase

Add positive filetests proving `texture(sampler2D, vec2)` sampling behavior
across the supported M4 sampler matrix.

This phase focuses on successful runtime behavior:

- nearest filtering;
- linear filtering;
- clamp-to-edge wrapping;
- repeat wrapping;
- mirror-repeat wrapping if implemented by the builtin helpers;
- mixed per-axis wrap policy for 2D;
- R16 vec4 fill;
- height-one / 1D path where `uv.y` has no effect.

Out of scope:

- Adding frontend lowering or builtin implementation.
- Adding negative diagnostics tests; that is phase 6.
- Broad performance benchmarking.
- New texture formats beyond those implemented in phase 3.

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
- Report back: what changed, what was validated, and any deviations from this phase.

# Implementation Details

Relevant files:

- `lp-shader/lps-filetests/filetests/textures/`
- `lp-shader/lps-filetests/src/test_run/`
- `lp-shader/lps-filetests/src/parse/parse_texture.rs`
- `lp-shader/lps-builtins/src/builtins/texture/sample_ref.rs`

Add texture sampling filetests under:

```text
lp-shader/lps-filetests/filetests/textures/
```

Suggested test files:

```text
texture_nearest_rgba16_clamp.glsl
texture_nearest_rgba16_repeat.glsl
texture_linear_rgba16_clamp.glsl
texture_linear_rgba16_repeat.glsl
texture_mixed_axis_wrap.glsl
texture_nearest_r16.glsl
texture_height_one_1d.glsl
texture_mirror_repeat.glsl              # if mirror-repeat is implemented
```

Use existing texture directive syntax from M2/M3:

```glsl
// test run
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x2 rgba16unorm
//   ...
vec4 render(vec2 pos) {
    return texture(inputColor, vec2(...));
}
// run: render(...) ~= ...
```

Follow existing filetest target conventions from M3c texture tests. Prefer the
same q32 backend matrix where practical:

- `wasm.q32`
- `rv32n.q32`
- `rv32c.q32` if supported by the current branch

Expected values:

- Nearest tests should use exact expectations where Q32 representation makes
  that reasonable.
- Linear tests should use approximate expectations with tolerances.
- Use the Rust reference sampler helpers from phase 1 where the filetest harness
  can do so cleanly. If filetests require literal expected values, compute them
  from the reference helper and keep the cases small/readable.

Coverage requirements:

- A clamp test that samples outside `[0, 1]`.
- A repeat test that samples outside `[0, 1]`.
- A linear test where the expected result is a real blend, not exactly one texel.
- A mixed-axis 2D test, for example `wrap_x=repeat wrap_y=clamp`.
- A height-one test with two calls using different `uv.y` values that produce
  the same result.
- An R16 test proving returned vec4 is `(R, 0, 0, 1)`.

Keep each file focused. Do not build one giant omnibus texture test.

# Validate

Run focused texture filetests, using the repo's existing filetest command. If
there is a documented command for a single directory, use it. Otherwise run the
full GLSL filetest suite.

Likely commands:

```bash
cargo test -p lps-filetests textures
```

or:

```bash
cargo test -p lps-filetests
```

Report the exact command used and any unsupported targets.
