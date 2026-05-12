# Sampling Fixture Summary

## What Changed

- Added direct fixture sampling as a `FixtureDef` strategy:

```toml
[sampling]
kind = "direct"
```

- Updated `examples/basic` to use direct sampling.
- Added a new shader/VM sample ABI:
  - `__render_samples_rgba16(points_ptr, out_ptr, count)`
  - points are packed normalized Q16.16 `i32` pairs
  - output is packed RGBA16 `u16` quads
- Added reusable sample point and RGBA16 sample buffers in `lp-shader`.
- Added backend dispatch for the new sample ABI across the LPVM backends.
- Added engine visual sampling APIs parallel to texture rendering.
- Implemented `ShaderNode` direct sampling by calling the compiled shader sample entry.
- Implemented fixture direct sampling:
  - precomputes one Q16.16 sample point per mapped lamp
  - reuses guest-addressable point/sample buffers
  - writes directly into output-owned unorm16 control targets
  - still applies fixture brightness, gamma, and color order

## Key Decisions

- `fixture` remains one node kind. Sampling is a mode on the fixture, not a separate node type.
- The existing texture/polygon path remains as `texture_area`.
- Direct sampling avoids texture allocation and polygon/area accumulation in the hot path.
- Pixel/sample hot-path data is unorm16, not `f32`.
- `render_size` and `sample_diameter` were not moved under `[sampling]` in this slice; that can be cleaned up once the strategy model settles.

## Validation

```bash
cargo fmt --check
cargo test -p lpvm validate_render_samples -- --nocapture
cargo test -p lp-shader render_samples_no_uniforms -- --nocapture
cargo test -p lpc-engine fixture -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
cargo test -p lpc-engine project_toml -- --nocapture
cargo check -p lp-cli
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

`fw-tests` currently reports the two selected emulator tests as ignored because they await the canonical project sync rebuild.
