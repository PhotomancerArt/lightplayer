# Phase 7: Esp32OutputProvider DisplayPipeline Integration

## Goal

Esp32OutputProvider creates DisplayPipeline on open, runs it on write, sends 8-bit result to RMT.

## Tasks

### 7.1 ChannelState

- Add `pipeline: Option<DisplayPipeline>` per channel (or store in a map by handle)
- Pipeline created in `open()`, dropped on `close()`

### 7.2 open()

- Parse `options: Option<OutputDriverOptions>`
- `num_leds = byte_count / 3`
- `DisplayPipeline::new(num_leds, options.unwrap_or_default())`
- Store pipeline with channel state

### 7.3 write()

- Receive `data: &[u16]`
- Get pipeline for handle
- Get `now_us` from time provider (esp_timer or similar)
- `pipeline.write_frame(now_us, data)` 
- Temporary buffer for 8-bit output: `num_leds * 3` bytes
- `pipeline.tick(now_us, &mut rmt_buffer)`
- Pass `rmt_buffer` to LedChannel/RMT as today

### 7.4 Time source

- DisplayPipeline tick needs `now_us` for interpolation. Use `esp_timer_get_time()` or system time.
- Pass through from caller or read in write(). write() is synchronous so we can get time at call site.

### 7.5 RMT integration

- RMT driver unchanged: expects `&[u8]` RGB
- No triple-buffering in RMT itself – we block on wait_complete before next write
- DisplayPipeline provides temporal smoothing; RMT remains double-buffered for DMA

## Verification

- Open channel with options, write 16-bit frames
- RMT receives 8-bit output after LUT/dither
- LED output shows correct colors
