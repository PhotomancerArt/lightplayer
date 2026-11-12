# RMT Triple-Buffer Dithering - Planning Notes

## Scope of Work

Extend the ESP32 RMT driver with a triple-buffered display pipeline supporting optional interpolation and dithering (inspired by LEDscape opc-server.c). Key requirements:

- **Native 16-bit pipeline** (no backwards compatibility): Shader → Texture (Rgba16) → Fixture → Output → DisplayPipeline → RMT (8-bit)
- **Display pipeline in lp-shared**: Pure computation, no fw deps, unit-testable. `tick(now_us, out)` method, ISR-safe.
- **OutputDriverOptions on OutputNode**: Configurable brightness, dithering, interpolation, gamma LUT, white point
- **OpenGL alignment**: Rgba16 uses unsigned normalized u16 (0-65535 → [0,1]), maps to GL_RGBA16
- **8-bit helper**: `write_frame_from_u8()` on DisplayPipeline for porting existing code

## Current State of Codebase

### Output / RMT

- **OutputProvider** (`lp-shared/src/output/provider.rs`): `open(pin, byte_count, format)`, `write(handle, data: &[u8])`, `close(handle)`. No options.
- **OutputConfig** (`lp-model/src/nodes/output/config.rs`): `GpioStrip { pin }` only
- **OutputRuntime** (`lp-engine/src/nodes/output/runtime.rs`): `channel_data: Vec<u8>`, 3 bytes/px, calls `output_provider.write()`
- **Esp32OutputProvider** (`lp-fw/fw-esp32/src/output/provider.rs`): Sync write, blocks on RMT `wait_complete()`
- **RMT** (`lp-fw/fw-esp32/src/output/rmt/`): Double-buffered interrupt driver, `ch_tx_end` fires when frame done. Reads RGB8 from buffer, no display pipeline.

### Texture / Shader

- **TextureFormat** (`lp-model/src/nodes/texture/format.rs`): Rgb8, Rgba8, R8 only. No Rgba16.
- **Texture** (`lp-shared/src/util/texture.rs`): `set_pixel(x, y, color: [u8; 4])` for Rgba8
- **ShaderRuntime** (`lp-engine/src/nodes/shader/runtime.rs`): Outputs Q32 (i32), converts to u8 via `(q32*255)/65536`, writes to texture
- **Texture runtime**: Creates texture with Rgba8 format

### Fixture

- **accumulate_from_mapping** (`lp-engine/.../fixture/mapping/accumulation.rs`): Returns `ChannelAccumulators` with Q32 per channel
- **Fixture runtime render**: Samples texture (u8), multiplies by brightness (Q32), calls `to_u8_clamped()`, writes via `ColorOrder::write_rgb(buffer, 0, r, g, b)` - 3 bytes
- **ColorOrder** (`lp-model/src/nodes/fixture/config.rs`): `write_rgb(buffer, offset, r, g, b: u8)` only
- **get_output**: Returns `&mut [u8]`; fixture requests `ch_count: 3`

### Reference: LEDscape (opc-server.c)

- Triple buffer: prev, current, next + timestamps
- Interpolation: blend prev/current by time progress (frame_progress16)
- LUT: 257 entries, gamma + white balance
- Dithering: per-pixel overflow (int8), `(value+0x80)>>8`, carry remainder

## Questions That Need to Be Answered

### Q1: OutputProvider trait - how to pass driver options?

**Context**: OutputProvider::open() currently takes pin, byte_count, format. We need to pass OutputDriverOptions for DisplayPipeline creation. The provider (fw-esp32) creates the pipeline when a channel is opened.

**Options**:
- A) Add optional `options: Option<OutputDriverOptions>` to `open()` - backward compatible for other providers (MemoryOutputProvider)
- B) Add `open_with_options(pin, byte_count, format, options)` - separate method
- C) Add `configure(handle, options)` - configure after open

**Answered**: Option A - add optional param to open(). MemoryOutputProvider ignores it. Minimal trait surface change.

### Q2: Texture format - who sets Rgba16 for shader-output textures?

**Context**: Shader writes to a texture. That texture must be Rgba16. Texture nodes create textures from TextureConfig. TextureConfig currently has width/height, no format field.

**Answered**: Hardcode Rgba16 in texture runtime (where Rgba8 is currently hardcoded). No TextureConfig changes, no format field. This is an experiment; add configurability later.

### Q3: LUT rebuild when options change?

**Context**: DisplayPipelineOptions can change (e.g. via future SetDisplayConfig API). The LUT tables are built from lum_power and white_point. Currently the plan has options in the pipeline struct.

**Answered**: Don't support config change for open device. Output node handles it: when config changes, close and reopen. LUT built once at pipeline creation.

### Q4: Interpolation with insufficient frame data?

**Context**: On first run, we may have only `has_next` (one frame). Or we may have prev+current but no next. LEDscape (opc-server.c render_thread) handles this.

**Answered**: When !has_prev: use current only (no blend). When !has_current: skip tick / output black or last frame. When !has_next and time exceeds current_ts: hold current (freeze last frame) until next arrives. (LEDscape sleeps when !has_prev|!has_current; using current-only when !has_prev is more permissive.)

### Q5: DisplayPipeline allocation - heap OK for fw-esp32?

**Context**: DisplayPipeline allocates Box for frames (3 * num_leds * 3 * 2 bytes), dither overflow, LUT. For 256 LEDs: ~5KB frames + ~1.5KB overflow + 3KB LUT ≈ 10KB. fw-esp32 has 300KB heap (esp_alloc).

**Answered**: Heap allocation is fine. `DisplayPipeline::new(num_leds, options) -> Result<Self, Error>`. no_std + alloc.

### Q6: Demo project / existing projects - migration?

**Context**: Existing projects use Rgba8 textures. Switching to 16-bit means texture format and shader output path change.

**Answered**: Project under active dev, not released. Backwards compat not a concern. Update demo_project.rs and templates as part of the 16-bit work. No migration machinery needed.
