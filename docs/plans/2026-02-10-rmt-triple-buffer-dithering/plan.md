# RMT Triple-Buffer Dithering

## Overview

Extend the ESP32 RMT driver with a triple-buffered display pipeline supporting optional interpolation and dithering (inspired by LEDscape opc-server.c). Full 16-bit pipeline: Shader → Texture (Rgba16) → Fixture → Output → DisplayPipeline → RMT (8-bit).

### Design Summary

**DisplayPipeline** (`lp-shared`): Pure computation, no fw deps. Triple buffer (prev/current/next), LUT (gamma + white point), dithering. `write_frame(ts, &[u16])`, `tick(now, &mut [u8])`. ~10KB heap for 256 LEDs.

**OutputProvider**: `open()` gets optional `OutputDriverOptions`. `write()` accepts 16-bit RGB (`&[u16]`). When config changes: close and reopen (output node handles).

**Texture**: Hardcode Rgba16 in runtime. Add `TextureFormat::Rgba16`, `Texture::set_pixel` for u16, `Rgba16Sampler`.

**Fixture/Output**: Fixture samples Rgba16, outputs u16. Output buffer `Vec<u16>`, passes to provider.

**Esp32OutputProvider**: Creates DisplayPipeline on open, runs `write_frame` + `tick` on write, sends 8-bit to RMT.

### Key Decisions (from 00-notes.md)

- Q1: Add optional `options` to `open()` 
- Q2: Hardcode Rgba16 in texture runtime
- Q3: Config change → close/reopen; LUT built once
- Q4: !has_prev → current only; !has_current → skip; !has_next exhausted → hold
- Q5: Heap allocation OK
- Q6: Update demo/templates; no migration machinery

## Phases

| # | Phase | Doc |
|---|-------|-----|
| 1 | DisplayPipeline in lp-shared | [01-display-pipeline.md](01-display-pipeline.md) |
| 2 | TextureFormat Rgba16 | [02-texture-rgba16.md](02-texture-rgba16.md) |
| 3 | OutputProvider trait | [03-output-provider-trait.md](03-output-provider-trait.md) |
| 4 | Shader runtime u16 | [04-shader-runtime-u16.md](04-shader-runtime-u16.md) |
| 5 | Fixture Rgba16Sampler + u16 output | [05-fixture-rgba16-sampler.md](05-fixture-rgba16-sampler.md) |
| 6 | Output runtime 16-bit | [06-output-runtime-16bit.md](06-output-runtime-16bit.md) |
| 7 | Esp32OutputProvider | [07-esp32-provider.md](07-esp32-provider.md) |
| 8 | Demo and templates | [08-demo-templates.md](08-demo-templates.md) |

## Current Status

- Planning complete. All questions answered (00-notes.md). Design finalized (00-design.md). Phase docs created.
- Implementation complete: Phases 1-8 done. TextureFormat default is Rgba16. Full 16-bit pipeline: Shader → Texture (Rgba16) → Fixture → Output → DisplayPipeline → RMT (8-bit).

## Success Criteria

- DisplayPipeline unit tests pass
- Full pipeline: shader → texture → fixture → output → DisplayPipeline → RMT
- 16-bit throughout; 8-bit only at RMT boundary
- No alloc/panic in tick (ISR-safe)

## Key Files

| Area | Files |
|------|-------|
| DisplayPipeline | `lp-shared/src/display_pipeline/*` |
| TextureFormat | `lp-model/src/nodes/texture/format.rs` |
| Texture | `lp-shared/src/util/texture.rs` |
| OutputProvider | `lp-shared/src/output/provider.rs`, `memory.rs` |
| Shader runtime | `lp-engine/src/nodes/shader/runtime.rs` |
| Fixture | `lp-engine/.../fixture/mapping/sampling/*`, fixture render |
| Output runtime | `lp-engine/src/nodes/output/runtime.rs` |
| Esp32 provider | `lp-fw/fw-esp32/src/output/provider.rs` |
