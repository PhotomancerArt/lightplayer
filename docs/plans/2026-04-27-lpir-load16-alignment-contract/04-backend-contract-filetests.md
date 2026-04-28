# Scope Of Phase

Add focused backend/filetest coverage that exercises aligned `Load16U` texture
reads on the RV32 backends and records the alignment assumption.

Out of scope:

- Do not add unaligned `Load16U` support.
- Do not add large new texture feature coverage.
- Do not change parser syntax unless an existing directive can be reused.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations.

# Implementation Details

Inspect existing texture filetests under:

- `lp-shader/lps-filetests/filetests/textures/`

The current M3b tests may already cover aligned `R16Unorm`, `Rgb16Unorm`, and
`Rgba16Unorm` texture reads on `wasm.q32`, `rv32n.q32`, and `rv32c.q32`. If so,
prefer updating comments/expected coverage rather than adding duplicate tests.

Ensure at least one filetest makes the alignment invariant clear:

- Texture fixture allocation is 4-byte aligned.
- Texture formats use 2-byte channels.
- Texel address math produces even offsets.
- The RV32 backends therefore use ordinary halfword loads.

If existing filetests do not directly cover `rv32n.q32` and `rv32c.q32` for
`R16Unorm` or `Rgba16Unorm`, add a minimal filetest that:

- Declares a `sampler2D` texture spec.
- Provides a small texture fixture.
- Calls `texelFetch` at a known coordinate.
- Checks exact channel values.
- Runs on `wasm.q32`, `rv32n.q32`, and `rv32c.q32`.

Avoid adding an odd-address texture fixture. The chosen contract is that LPIR
`Load16U` requires 2-byte alignment.

# Validate

Run:

```bash
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

If adding or changing parser/unit tests, also run the relevant crate tests:

```bash
cargo test -p lps-filetests
```
