# Phase 1 — Typed Texture2D Uniform Writes

## Scope of Phase

Allow the normal typed uniform write path to accept
`LpsValueF32::Texture2D(LpsTexture2DDescriptor)` and
`LpsValueQ32::Texture2D(LpsTexture2DDescriptor)` when the target uniform path
resolves exactly to `LpsType::Texture2D`.

Out of scope:

- Do not add filetest texture directive parsing.
- Do not add texture fixture allocation or binding.
- Do not relax raw descriptor-shaped values. `UVec4` must still be rejected for
  `Texture2D`.
- Do not expose or accept `tex.ptr`, `tex.width`, or other subpaths. `Texture2D`
  is opaque.

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

Primary file:

- `lp-shader/lpvm/src/set_uniform.rs`

Current behavior:

- `encode_uniform_write` resolves the path and rejects `LpsType::Texture2D`
  before encoding.
- `encode_uniform_write_q32` does the same.
- Tests currently assert that all texture writes are rejected.

Required behavior:

- If `leaf_ty == LpsType::Texture2D` and the value is
  `LpsValueF32::Texture2D(_)`, encode it through the same typed ABI machinery
  used for other values.
- If `leaf_ty == LpsType::Texture2D` and the value is
  `LpsValueQ32::Texture2D(_)`, encode it through the same Q32 flattening path
  used for other Q32 values.
- If `leaf_ty == LpsType::Texture2D` and the value is any other variant,
  return a clear `DataError::TypeMismatch`.
- If the path attempts a subfield (`tex.ptr`) it should continue to fail during
  path resolution, because `Texture2D` is not a struct.

Suggested structure:

- Remove the unconditional `Texture2D` rejection in both encoder functions.
- Rely on `LpvmDataQ32::from_value` and `lps_value_f32_to_q32` to reject
  non-typed texture values. These already reject `UVec4` stand-ins with clear
  messages.
- Keep payload size validation against std430 layout size.

Update tests in `set_uniform.rs`:

- Replace `encode_uniform_write_rejects_texture2d_scalar_value` with a test
  that a scalar still rejects for `Texture2D`.
- Replace `encode_uniform_write_rejects_texture2d_uvec4_descriptor_shape` with a
  test that `UVec4` still rejects.
- Add a positive F32 test:
  `encode_uniform_write_accepts_texture2d_typed_value`.
- Add a positive Q32 test:
  `encode_uniform_write_q32_accepts_texture2d_typed_value`.
- Keep subpath rejection tests.

Use a descriptor such as:

```rust
LpsTexture2DDescriptor {
    ptr: 0x1000,
    width: 2,
    height: 1,
    row_stride: 16,
}
```

Expected bytes are little-endian `u32` fields in descriptor order:
`ptr`, `width`, `height`, `row_stride`.

## Validate

Run from repo root:

```bash
cargo test -p lpvm set_uniform
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

