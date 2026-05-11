# Phase 3: Implement Descriptor Loads, Clamp, And Address Math

## Scope of phase

Implement the `texelFetch` descriptor lane loads, coordinate clamp/unchecked
selection, and byte address calculation in `lower_texture.rs`.

In scope:

- Load `ptr`, `width`, `height`, and `row_stride` from the `Texture2D` uniform
  descriptor.
- Add named descriptor ABI lane constants.
- Generate clamp-to-edge coordinate bounds code when
  `ctx.texel_fetch_bounds == ClampToEdge`.
- Generate raw unchecked coordinates when
  `ctx.texel_fetch_bounds == Unchecked`.
- Compute the byte address:
  `ptr + y * row_stride + x * bytes_per_pixel`.
- Keep the M3b placeholder diagnostic until phase 4 adds channel loads.
- Add focused tests/assertions for safe-vs-unchecked lowering shape if
  practical without relying on runtime out-of-bounds behavior.

Out of scope:

- Do not implement `Load16U` channel reads or `Unorm16toF` conversion.
- Do not convert texture filetests to positive runtime tests yet.
- Do not add new LPIR ops or backend support.

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
- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs` helper
  `load_lps_value_from_vmctx_with_base`
- `lp-shader/lpir/src/lpir_op.rs`
- `lp-shader/lpir/src/print.rs` if adding printed-LPIR assertions

In `lower_texture.rs`, add named constants for the descriptor ABI:

```rust
const TEXTURE_DESC_PTR_OFFSET: u32 = 0;
const TEXTURE_DESC_WIDTH_OFFSET: u32 = 4;
const TEXTURE_DESC_HEIGHT_OFFSET: u32 = 8;
const TEXTURE_DESC_ROW_STRIDE_OFFSET: u32 = 12;
```

Add helper structs:

```rust
struct TextureDescriptorVRegs {
    ptr: VReg,
    width: VReg,
    height: VReg,
    row_stride: VReg,
}

struct TexelFetchCoords {
    x: VReg,
    y: VReg,
}
```

Load descriptor lanes from VMContext using the sampler uniform's global map
entry byte offset. You can either use explicit `LpirOp::Load` ops with the named
offset constants or a small helper that mirrors
`load_lps_value_from_vmctx_with_base` behavior for `LpsType::Texture2D`. Keep
the code readable and make the lane order auditable.

Coordinate clamping:

- Only generated when `ctx.texel_fetch_bounds` is `ClampToEdge`.
- Clamp negative `x`/`y` to zero using signed comparison and `Select`.
- Clamp high `x` to `width - 1`, and high `y` to `height - 1`.
- Use existing LPIR ops such as `IconstI32`, `IsubImm`, `IltS`, `IgtS`,
  `Select`.
- Assume M2 fixtures/runtime validation keep texture dimensions positive.
  Runtime zero-size descriptor validation is out of scope for M3b.

Unchecked mode:

- If the mode is `Unchecked`, skip clamp ops and use raw `x`/`y`.
- This mode intentionally permits out-of-bounds reads for performance
  measurement; do not add runtime guards in unchecked mode.

Byte address:

- `row_offset = y * row_stride`
- `col_offset = x * bytes_per_pixel`
- `texel_offset = row_offset + col_offset`
- `texel_addr = ptr + texel_offset`

Use `TextureBindingSpec::format.bytes_per_pixel()` to choose `bytes_per_pixel`.
Use `ImulImm` for bytes per pixel where possible and ordinary `Imul` for
`y * row_stride`.

Keep the function returning the existing M3b placeholder diagnostic after
emitting this setup until phase 4 wires channel loads. This preserves existing
M3a placeholder tests for now.

Testing suggestion:

- If feasible, add a frontend unit test that lowers the same shader twice:
  default/safe and unchecked.
- Compare printed LPIR (`lpir::print_module`) or op counts:
  safe mode should contain clamp-related `Select`/comparison ops, unchecked
  mode should omit those extra clamp ops.
- Keep the assertion focused. Do not depend on exact full IR text if a smaller
  op-count check works.

## Validate

Run from workspace root:

```bash
cargo test -p lps-frontend sampler2d_metadata_tests
cargo check -p lps-frontend
```

