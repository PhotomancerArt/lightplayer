# Plan: Pre-computed Texture-to-Fixture Mapping

## Scope of Work

Replace the current per-frame texture sampling approach with a pre-computed pixel-to-channel mapping system that:

1. Pre-computes weights for each texture pixel mapping to fixture channels
2. Uses bit-packed encoding (32 bits per entry) for memory efficiency in embedded context
3. Computes accurate area overlap between mapping circles and pixel squares
4. Normalizes weights per-channel (each channel's total contribution from all pixels sums to 1.0)
5. Stores pixel values as Q32 (16.16 fixed-point) for precision before downsampling to 8-bit
6. Recomputes mapping when texture size or mapping configuration changes

## Current State

### Current Implementation
- **Location**: `lp-app/crates/lp-engine/src/nodes/fixture/runtime.rs`
- **Sampling approach**: Per-frame iteration through mapping points
- **Kernel-based sampling**: Uses `SamplingKernel` with precomputed sample points in a circle
- **Performance issue**: Iterates through every mapping point and samples multiple positions per frame

### Current Data Structures
- `MappingPoint`: Contains channel, center (texture space [0,1]), and radius
- `SamplingKernel`: Precomputed sample points with offsets and weights
- `FixtureRuntime`: Stores mapping points, kernel, and sampled lamp colors

### Current Sampling Flow
1. For each mapping point:
   - Calculate center position in texture space
   - Sample texture at multiple kernel positions (scaled by radius)
   - Accumulate weighted samples
   - Normalize and convert to u8
   - Store in lamp_colors array

### Current Limitations
- Per-frame sampling overhead (iterates through all mappings every frame)
- Kernel-based sampling doesn't accurately compute circle-pixel overlap
- No pre-computation of pixel contributions

## Questions

### Q1: Where should Q32 type come from?

**Context**: We need Q32 (16.16 fixed-point) for storing pixel values and contribution fractions. Q32 is currently defined in `lp-glsl/crates/lp-builtins/src/glsl/q32/types/q32.rs`, but `lp-engine` doesn't currently depend on `lp-builtins`.

**Answer**: Add `lp-builtins` as a dependency to `lp-engine`. We need Q32 for the fixed-point math. At some point we might want to move Q32 to a shared location, but for now adding the dependency is fine.

### Q2: How should we structure the pre-computed mapping data?

**Context**: We need to store a variable-length list of channel mappings per pixel. Each entry is 32 bits (bit-packed). We need to efficiently iterate through mappings for rendering.

**Answer**: Use a flat `Vec<PixelMappingEntry>` ordered by pixel (x, y), where:
- Entries for each pixel are consecutive
- The last entry for each pixel has `has_more = false`
- Pixels with no contributions get a SKIP sentinel entry (channel index = sentinel value, `has_more = true`)
- Channel order within each pixel doesn't matter
- During rendering, iterate sequentially and advance `pixel_index` when `has_more` is false

This provides:
- Fast sequential access for rendering (just iterate the vec)
- Simple structure (no offset table needed)
- Memory efficient (only stores entries that exist)

### Q3: How should we compute circle-pixel area overlap?

**Context**: We need to accurately compute the area of overlap between a circle (mapping point) and a pixel square. This is needed to compute proper weights.

**Answer**: Subdivide each pixel into an 8x8 grid (64 sub-pixels) and count how many sub-pixel centers fall within the circle. This provides good accuracy with reasonable computation cost during pre-computation. All utilities for this calculation should be clearly separated, organized, and well-tested.

### Q4: Where should the pre-computation logic live?

**Context**: The pre-computation needs to:
- Take mapping configuration and texture dimensions
- Compute pixel-to-channel weights using circle-pixel overlap
- Build the flat `Vec<PixelMappingEntry>` structure
- Handle normalization

**Answer**: Create a separate module `lp-engine/src/nodes/fixture/mapping_compute.rs` that contains:
- Circle-pixel overlap calculation utilities
- Pre-computation logic that builds the `Vec<PixelMappingEntry>`
- Well-organized, testable functions

This keeps pre-computation logic separate from runtime rendering and makes it easy to test independently. We'll keep `MappingPoint` for now since it's used elsewhere (state extraction, etc.), and add the pre-computed mapping alongside it.

### Q5: How should we handle the contribution encoding (0 = 100%)?

**Context**: We want to encode contribution where 0 fractional part = 100% contribution. This is a quirk to maximize the value range.

**Answer**: Store `(65536 - contribution_fraction)` where contribution_fraction is the 16-bit fractional value (0-65535). This allows 0 to represent 100% contribution while maintaining full 16-bit precision. Decode during rendering as: `contribution = 65536 - stored_value`. The SKIP sentinel uses a special channel index value (not the contribution field).

### Q6: Should we store pixel values as Q32 in the runtime state?

**Context**: The user mentioned storing pixel values as Vec<Q32> in state and downsampling to 8-bit when writing to texture. This provides 16 bits of precision per channel.

**Answer**: Store accumulated values as `Vec<i32>` per channel (one value per fixture channel, not per pixel). These represent channel values accumulated from texture pixels. Use Q32 for the math operations. Convert to u8 when writing to output buffer. The accumulation formula is: `ch_values[ch_index] += (65536 - stored_contribution) * pixel_value`.

### Q7: When should we trigger recomputation?

**Context**: The mapping needs to be recomputed when texture size changes or mapping configuration changes.

**Answer**: Use config versions to track changes. Compare `max(our_config_ver, texture_config_ver) > mapping_data_ver` to determine if recomputation is needed. Extend `regenerate_mapping_if_needed()` to check this condition and recompute the pre-computed mapping table when needed. This ensures we recompute when either our fixture config changes or the texture config changes (which might affect texture dimensions).
