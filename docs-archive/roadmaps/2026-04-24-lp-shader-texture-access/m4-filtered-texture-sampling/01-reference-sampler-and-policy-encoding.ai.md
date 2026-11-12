# Scope of Phase

Add the small shared policy and reference-sampler foundation for M4 filtered
texture sampling.

This phase should produce pure Rust helpers that define the expected semantics
for:

- normalized texel-center coordinates: `coord = uv * extent - 0.5`;
- nearest sampling;
- linear sampling;
- `ClampToEdge`, `Repeat`, and `MirrorRepeat` wrap modes;
- 2D sampling;
- height-one / 1D sampling where the Y coordinate has no effect.

Also add numeric ABI conversion helpers for `TextureFilter` and `TextureWrap`
if they are needed for builtin calls.

Out of scope:

- Adding the actual `extern "C"` sampler builtins.
- Regenerating builtin ID/ABI files.
- Frontend `texture()` lowering.
- Filetest GLSL coverage beyond unit tests for the reference helpers.

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

- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lps-builtins/src/builtins/mod.rs`
- new `lp-shader/lps-builtins/src/builtins/texture/`

Create a texture helper module under `lps-builtins`, for example:

```text
lp-shader/lps-builtins/src/builtins/texture/
├── mod.rs
└── sample_ref.rs
```

Wire `texture` into `lp-shader/lps-builtins/src/builtins/mod.rs`, but do not
add exported sampler builtins yet.

In `sample_ref.rs`, add pure helper functions/types for expected sampling
behavior. The exact public/internal API is flexible, but it should make these
operations testable without a full LPVM call:

```rust
pub fn texel_center_coord_q32(uv: i32, extent: u32) -> i32;
pub fn wrap_coord(coord: i32, extent: u32, wrap: TextureWrap) -> u32;
pub fn nearest_index_q32(uv: i32, extent: u32, wrap: TextureWrap) -> u32;
pub fn linear_indices_q32(uv: i32, extent: u32, wrap: TextureWrap) -> LinearAxis;
```

Names can differ if the local style suggests something better.

The reference sampler should encode these decisions:

- `texture()` uses texel centers: continuous coordinate is `uv * extent - 0.5`.
- nearest chooses the closest texel center.
- linear chooses `floor(coord)` and `floor(coord) + 1`, with interpolation by
  the fractional part.
- `ClampToEdge` clamps integer texel coordinates to `[0, extent - 1]`.
- `Repeat` wraps with Euclidean modulo.
- `MirrorRepeat` mirrors repeated periods.
- height-one / 1D sampling ignores Y entirely.

Prefer Q32/integer helpers where feasible so the reference path can compare
against actual q32 builtin behavior. If intermediate `f32` helpers are needed
for readability in tests, keep the canonical behavior documented and avoid
making production builtin code depend on host-only floating point assumptions.

Add unit tests covering:

- center sample at `uv = 0.5` for a small texture;
- edge samples near `0.0` and `1.0`;
- repeat wrapping for negative and >1 coordinates;
- mirror-repeat wrapping across several periods;
- linear interpolation indices and weights;
- 1D sampling ignores the Y coordinate if a small helper exposes full sample behavior.

If numeric ABI helpers are needed, add methods near the existing enums in
`lps-shared/src/texture_format.rs`, for example:

```rust
impl TextureFilter {
    pub const fn to_builtin_abi(self) -> u32 { ... }
}

impl TextureWrap {
    pub const fn to_builtin_abi(self) -> u32 { ... }
}
```

Add reverse conversion helpers only if builtin implementations need them. Keep
the mapping explicit and covered by tests.

# Validate

Run:

```bash
cargo test -p lps-shared texture
cargo test -p lps-builtins texture
```

If the test names do not match exactly after implementation, run the closest
focused package tests and report the exact commands used.
