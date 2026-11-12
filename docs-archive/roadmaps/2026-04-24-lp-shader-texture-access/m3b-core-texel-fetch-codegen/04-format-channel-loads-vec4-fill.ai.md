# Phase 4: Implement Format Channel Loads And Vec4 Fill

## Scope of phase

Replace the M3a/M3b placeholder diagnostic for valid `texelFetch` with real
format-specialized channel loads, unorm conversion, and GLSL-compatible `vec4`
return values.

In scope:

- Emit `Load16U` for each stored unorm16 channel.
- Emit `Unorm16toF` directly after each `Load16U`.
- Support:
  - `R16Unorm`
  - `Rgb16Unorm`
  - `Rgba16Unorm`
- Fill missing channels to match GLSL texture fetch conventions:
  - R => `(r, 0.0, 0.0, 1.0)`
  - RGB => `(r, g, b, 1.0)`
  - RGBA => `(r, g, b, a)`
- Remove the placeholder diagnostic for otherwise valid supported
  `texelFetch`.
- Preserve all unsupported-form diagnostics from M3a.

Out of scope:

- Do not add filetests in this phase except minimal frontend tests if needed.
- Do not implement filtered `texture()` sampling.
- Do not add runtime descriptor validation.
- Do not add a dedicated texture opcode.

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

- `lp-shader/lps-frontend/src/lower_texture.rs`
- `lp-shader/lpir/src/lpir_op.rs`
- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs` constant helper patterns

Build on phase 3's computed texel byte address and texture format.

For each stored channel:

1. Allocate an `IrType::I32` vreg for the raw channel.
2. Emit `LpirOp::Load16U { dst: raw, base: texel_addr, offset }`.
3. Allocate an `IrType::F32` vreg for the shader channel.
4. Emit `LpirOp::Unorm16toF { dst: converted, src: raw }`.

Channel byte offsets are fixed by packed unorm16 layout:

- channel 0: `0`
- channel 1: `2`
- channel 2: `4`
- channel 3: `6`

Use `TextureStorageFormat::channel_count()` to decide how many stored channels
to load. Missing values:

- create `0.0` with `LpirOp::FconstF32`
- create `1.0` with `LpirOp::FconstF32`

Return a `VRegVec` with exactly four lanes for valid `texelFetch`.

Potential helper shape:

```rust
fn emit_unorm16_channel_load(
    ctx: &mut LowerCtx<'_>,
    texel_addr: VReg,
    channel_index: u32,
) -> VReg
```

and:

```rust
fn f32_const(ctx: &mut LowerCtx<'_>, value: f32) -> VReg
```

Make sure `lower_image_load_texel_fetch` still validates:

- direct uniform `Texture2D`
- matching texture spec
- `lod == 0`
- coordinate vector shape

before emitting the data path.

Tests:

- Update frontend tests that expected the placeholder diagnostic for valid
  `texelFetch`; they should now expect successful lowering.
- Keep missing spec, nonzero LOD, dynamic LOD, and unsupported operand tests as
  expected errors.
- If you add an IR-shape assertion, keep it narrow: e.g. valid RGBA lowering
  contains four `Load16U` ops and four `Unorm16toF` ops.

## Validate

Run from workspace root:

```bash
cargo test -p lps-frontend sampler2d_metadata_tests
cargo check -p lps-frontend
```

