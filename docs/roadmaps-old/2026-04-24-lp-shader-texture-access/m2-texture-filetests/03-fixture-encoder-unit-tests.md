# Phase 3 — Fixture Encoder Unit Tests

## Scope of Phase

Implement a reusable texture fixture encoder for `lps-filetests` and cover it
with focused unit tests.

Out of scope:

- Do not parse texture directives unless Phase 2 has already landed and the
  local model is available.
- Do not allocate backend memory.
- Do not bind texture uniforms.
- Do not add sampling execution behavior.

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
- `lp-shader/lps-filetests/src/parse/test_type.rs`
- `lp-shader/lps-shared/src/texture_format.rs`
- Existing texture storage conversion helpers, if any, before writing new
  conversion code.

Add:

- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`

The module should provide fixture validation and encoding. Suggested public API:

```rust
pub struct EncodedTextureFixture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: TextureStorageFormat,
    pub bytes: Vec<u8>,
    pub row_stride: u32,
}

pub fn encode_texture_fixture(fixture: &TextureFixture) -> anyhow::Result<EncodedTextureFixture>
```

Adjust names if Phase 2 used different parser model names.

Encoding rules:

- Validate `width > 0` and `height > 0`.
- Validate pixel count equals `width * height`.
- Validate each pixel channel count matches the format:
  - `R16Unorm`: 1 channel
  - `Rgb16Unorm`: 3 channels
  - `Rgba16Unorm`: 4 channels
- For normalized float channels:
  - Accept inclusive `0.0..=1.0`.
  - Convert through canonical unorm storage conversion.
  - Prefer the existing codebase conversion if one exists.
  - If no helper exists, use `round(clamp(v, 0.0, 1.0) * 65535.0)` only if that
    matches existing texture output conventions; otherwise stop and report.
- For exact hex channels:
  - Use the provided `u16` value exactly.
- Output little-endian `u16` channel bytes in pixel order.
- `row_stride` for M2 should be tightly packed:
  `width * format.bytes_per_pixel()`.

Tests:

- Encode a 2x1 `Rgba16Unorm` fixture with normalized floats.
- Encode exact hex values and verify exact little-endian bytes.
- Encode `R16Unorm` and `Rgb16Unorm` channel counts.
- Reject pixel count mismatch.
- Reject channel count mismatch.
- Reject out-of-range normalized floats, if the parser allows such values to
  reach the encoder.
- Confirm row stride for each supported format.

If Phase 2 has not landed:

- Create the encoder with minimal local structs only if that will not conflict
  badly with Phase 2. Otherwise stop and report that this phase should wait for
  Phase 2. Do not invent a parallel parser model that will cause merge churn.

## Validate

Run from repo root:

```bash
cargo test -p lps-filetests texture_fixture
```

