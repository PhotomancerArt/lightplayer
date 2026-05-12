# Render Texture Leak OOM

## Summary

On ESP32-C6, `examples/basic` could load successfully, tick for a short time,
then crash with an allocation failure. The failing allocation was often 2048
bytes, matching a 16x16 RGBA16 render target.

The immediate bug was that shader visual-product materialization allocated an
`LpsTextureBuf` every frame and treated it like an ordinary Rust value. It is
not ordinary Rust-owned memory: it wraps an `LpvmBuffer`, and `LpvmBuffer` does
not free on drop. That meant one shader output texture leaked per frame.

## Evidence

Typical device log:

```text
[mem] load_project after: 255k free / 57k used
[fixture] frame=1 recomputing mapping 16x16
[shader-node] compilation succeeded
Esp32OutputProvider::open: ... byte_count=723, num_leds=241
OOM allocation failed: requested=2048 align=1
```

Why 2048 bytes matters:

```text
16 * 16 * RGBA16 = 16 * 16 * 4 channels * 2 bytes = 2048 bytes
```

The old render path:

1. Fixture requested a full texture from a visual product.
2. Shader node allocated a new `LpsTextureBuf` through `LpGraphics`.
3. Shader rendered into that LPVM buffer.
4. Shader copied the bytes into `TextureRenderProduct`.
5. The LPVM buffer was dropped, but dropping did not free the allocation.

Cold boot without a loaded project did not crash, which separated the idle
server loop from the project/render path.

## Fix Applied

Two fixes landed during the investigation:

- ESP32 boot auto-load now scans for `project.toml`, not stale `project.json`,
  making it possible to boot and test a persisted project without a client.
- `LpGraphics` now exposes `free_output_buffer`, and shader materialization
  frees the transient `LpsTextureBuf` after copying its bytes.

The second fix stops the leak, but it is not the final architecture.

## Architectural Lesson

Render products should be caller-driven. In the current texture-mode bridge, the
fixture should own the render target because it decides the texture size and
consumes the rendered data. The shader should fill a caller-owned target rather
than allocating a target and returning an owned texture product.

That shape matches the product model better:

```text
shader produces VisualProduct handle
fixture resolves VisualProduct
fixture owns/reuses texture target
shader renders into fixture target
fixture samples/accumulates target into ControlProduct
output owns final control buffer
```

## Follow-Up Work

- Change visual-product rendering to accept a caller-owned texture target.
- Keep a fixture-owned texture buffer in the temporary texture-mode fixture
  path and reuse it across frames.
- Add live allocation accounting to graphics/resource managers.
- Add a long-running emu/host regression test that ticks a project for many
  frames and asserts render-target memory stays flat.
- Fix the profile harness: the attempted `lp-cli profile --collect alloc,events`
  run reached the cycle cap and only emitted three perf events, so it did not
  capture the steady render loop correctly.
