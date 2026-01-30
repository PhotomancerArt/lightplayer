# Design: Pre-computed Texture-to-Fixture Mapping

## Scope of Work

Replace the current per-frame texture sampling approach with a pre-computed pixel-to-channel mapping system that:

1. Pre-computes weights for each texture pixel mapping to fixture channels
2. Uses bit-packed encoding (32 bits per entry) for memory efficiency in embedded context
3. Computes accurate area overlap between mapping circles and pixel squares
4. Normalizes weights per-channel (each channel's total contribution from all pixels sums to 1.0)
5. Stores accumulated channel values as 16.16 fixed-point for precision before downsampling to 8-bit
6. Recomputes mapping when texture size or mapping configuration changes (using config versions)

## File Structure

```
lp-app/crates/lp-engine/src/nodes/fixture/
├── mod.rs                                    # UPDATE: Export new modules
├── runtime.rs                                # UPDATE: Use pre-computed mapping, new render logic
├── sampling_kernel.rs                        # KEEP: Still used for now, may be deprecated later
└── mapping_compute.rs                        # NEW: Pre-computation logic
    ├── PixelMappingEntry                     # Bit-packed entry type
    ├── PrecomputedMapping                    # Container for pre-computed data
    ├── circle_pixel_overlap()                # Area overlap calculation
    ├── compute_mapping()                     # Main pre-computation function
    └── tests                                 # Comprehensive tests

lp-app/crates/lp-engine/Cargo.toml            # UPDATE: Add lp-builtins dependency
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    FixtureRuntime                            │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  PrecomputedMapping                                  │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │ entries: Vec<PixelMappingEntry>             │   │  │
│  │  │ (flat, ordered by pixel x,y)                │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  │  mapping_data_ver: FrameId                          │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Channel Accumulators                                │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │ ch_values: Vec<i32>                          │   │  │
│  │  │ (16.16 fixed-point per channel)              │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ uses
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              mapping_compute.rs                             │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ compute_mapping()                                    │  │
│  │  - Takes: MappingConfig, texture_width, height       │  │
│  │  - Returns: PrecomputedMapping                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ circle_pixel_overlap()                               │  │
│  │  - Subdivides pixel into 8x8 grid                  │  │
│  │  - Counts sub-pixels within circle                 │  │
│  │  - Returns normalized weight                        │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Main Components

### PixelMappingEntry

Bit-packed 32-bit entry encoding:

- Bit 0: `has_more` flag (1 = more entries for this pixel follow)
- Bits 1-15: Channel index (15 bits, max 32767; sentinel value indicates SKIP)
- Bits 16-31: Contribution fraction (16 bits, stored as `65536 - contribution`)

### PrecomputedMapping

Container for pre-computed mapping data:

- `entries: Vec<PixelMappingEntry>` - Flat list ordered by pixel (x, y)
- `mapping_data_ver: FrameId` - Version when this mapping was computed

### Rendering Flow

1. **Pre-computation** (when config/texture changes):
   - For each mapping point (circle):
     - For each pixel in texture:
       - Compute circle-pixel overlap area
       - Store contribution to channel
   - Normalize weights per-channel (each channel's total from all pixels sums to 1.0)
   - Build flat `Vec<PixelMappingEntry>` ordered by pixel

2. **Per-frame rendering**:
   - Initialize `ch_values: Vec<i32>` (one per channel)
   - Iterate through `entries` sequentially:
     - Decode contribution: `65536 - stored_value`
     - Accumulate: `ch_values[channel] += contribution * pixel_value`
     - Advance `pixel_index` when `has_more = false`
   - Convert accumulated values to u8 and write to output

### Version Tracking

- Track `mapping_data_ver: FrameId` in `PrecomputedMapping`
- Track `our_config_ver: FrameId` (fixture config version)
- Track `texture_config_ver: FrameId` (texture node config version)
- Recompute when: `max(our_config_ver, texture_config_ver) > mapping_data_ver`

## Key Design Decisions

1. **Flat Vec structure**: Simple sequential access, no offset table needed
2. **Bit-packed encoding**: Maximizes memory efficiency for embedded context
3. **8x8 subdivision**: Good balance of accuracy and computation cost
4. **Separate module**: Keeps pre-computation logic organized and testable
5. **Version-based invalidation**: Efficient change detection without deep comparisons
6. **16.16 fixed-point accumulation**: Provides precision needed for multi-pixel contributions
