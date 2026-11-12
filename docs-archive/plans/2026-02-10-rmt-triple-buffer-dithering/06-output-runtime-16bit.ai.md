# Phase 6: Output Runtime 16-bit

## Goal

Output runtime uses 16-bit channel buffer, passes to provider via `write(&[u16])`. Integrates with trait from phase 3.

## Tasks

### 6.1 Channel buffer type

- `channel_data: Vec<u8>` → `channel_data: Vec<u16>`
- `byte_count` for open: still 3 * num_leds (8-bit output size). For provider, num_leds = byte_count / 3.
- Internal buffer size: num_leds * 3 (u16 elements) = num_leds * 6 bytes

### 6.2 get_buffer_mut

- Returns `&mut [u16]` for fixtures. `start_ch`, `ch_count` in terms of u16 channels.
- Fixture requests ch_count=3 (R,G,B) per pixel; output aggregates across fixtures.

### 6.3 init

- `byte_count` = computed from fixtures (num_leds * 3 for 8-bit).
- `open(pin, byte_count, format, options)` – options from OutputConfig if/when we add them.

### 6.4 render

- `ctx.output_provider().write(handle, &self.channel_data)` – channel_data is `&[u16]`
- Provider receives 16-bit, runs DisplayPipeline (Esp32) or stores (Memory).

### 6.5 OutputState / channel_data

- `OutputState.channel_data`: may be stored as Vec<u8> for JSON/sync. Consider storing u16 or scaled u8 for client display. Defer if complex.

## Verification

- Output passes 16-bit to provider
- MemoryOutputProvider receives and stores 16-bit
- Engine render loop flows: fixture → output buffer (u16) → provider.write(u16)
