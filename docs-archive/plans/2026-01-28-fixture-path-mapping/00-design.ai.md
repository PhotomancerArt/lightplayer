# Design: Fixture Path Mapping Support

## Overview

Implement support for structured path-based mapping configurations for fixtures, starting with circular displays (RingArray). This includes generating LED positions from path specifications, handling texture resolution changes, and standardizing coordinate space to [0, 1] across the fixture system.

## Goals

1. Support `MappingConfig::PathPoints` with `RingArray` path specification
2. Generate `MappingPoint` positions from `RingArray` configuration
3. Standardize fixture coordinate space to [0, 1] (texture space)
4. Handle texture resolution changes by regenerating mappings
5. Remove string-based mapping support
6. Update all fixture code and comments to reflect [0, 1] coordinate space

## Architecture

### File Structure

```
lp-app/crates/
├── lp-model/src/nodes/fixture/
│   ├── mapping.rs                    # UPDATE: Already has MappingConfig, PathConfig, PathSpec, RingOrder
│   └── config.rs                     # UPDATE: Already uses MappingConfig enum
├── lp-engine/src/nodes/fixture/
│   ├── runtime.rs                    # UPDATE: Update MappingPoint, implement path generation, update coordinate space
│   └── mod.rs                        # (no changes)
└── lp-shared/src/project/
    └── builder.rs                     # UPDATE: Update FixtureBuilder to use MappingConfig

examples/basic/src/fixture.fixture/
    └── node.json                      # UPDATE: Use new MappingConfig format
```

### Type Changes

#### MappingPoint (runtime.rs)

```rust
// UPDATE: Change coordinate space from [-1, 1] to [0, 1]
pub struct MappingPoint {
    pub channel: u32,
    pub center: [f32; 2], // UPDATE: Texture space coordinates [0, 1]
    pub radius: f32,     // Normalized radius in texture space [0, 1]
}
```

#### FixtureRuntime (runtime.rs)

```rust
pub struct FixtureRuntime {
    config: Option<FixtureConfig>,
    texture_handle: Option<TextureHandle>,
    output_handle: Option<OutputHandle>,
    kernel: SamplingKernel,
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,
    transform: [[f32; 4]; 4],
    // NEW: Track texture dimensions for mapping regeneration
    texture_width: Option<u32>,
    texture_height: Option<u32>,
}
```

### New Functions

#### Path Generation (runtime.rs)

```rust
// NEW: Generate mapping points from PathPoints config
fn generate_mapping_points(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint>

// NEW: Generate points from RingArray path spec
fn generate_ring_array_points(
    ring_array: &RingArray,
    sample_diameter: f32,
    texture_width: u32,
    texture_height: u32,
    channel_offset: u32,
) -> Vec<MappingPoint>

// NEW: Regenerate mapping when texture resolution changes
fn regenerate_mapping_if_needed(&mut self, texture_width: u32, texture_height: u32) -> Result<(), Error>
```

### Coordinate Space Standardization

All fixture coordinates will use texture space [0, 1]:

- `MappingPoint.center`: [0, 1] (was [-1, 1])
- Transform matrix: transforms from [0, 1] to [0, 1] (was [-1, 1] to [0, 1])
- RingArray positions: already [0, 1], used directly
- Render code: expects [0, 1] input (was [-1, 1])

### RingArray Generation Algorithm

For each ring from `start_ring_inclusive` to `end_ring_exclusive`:

1. Calculate ring radius: `radius = (diameter / 2) * (ring_index / max_ring_index)` (even spacing)
2. Get lamp count: `ring_lamp_counts[ring_index]`
3. For each lamp in ring:
   - Angle: `angle = (2π * lamp_index / lamp_count) + offset_angle`
   - Position: `(center_x + radius * cos(angle), center_y + radius * sin(angle))`
4. Order based on `RingOrder`:
   - `InnerFirst`: process rings inner→outer
   - `OuterFirst`: process rings outer→inner
5. Assign sequential channel numbers starting from `channel_offset`

### Sample Diameter Conversion

- `sample_diameter` is in texture pixels
- Convert to normalized radius: `radius = (sample_diameter / 2.0) / max(texture_width, texture_height)`
- This ensures pixel-perfect mappings work correctly
- Regenerate when texture resolution changes

### Multiple Paths Handling

- Process paths sequentially
- Each path gets sequential channel numbers
- Path 1: channels 0..N-1, Path 2: channels N..M-1, etc.

## Implementation Phases

1. Update coordinate space to [0, 1] across fixture code
2. Implement RingArray path generation
3. Add comprehensive tests for RingArray generation
4. Add texture resolution change detection
5. Remove string-based mapping support
6. Update builder and example JSON
7. Cleanup and finalization

## Testing Requirements

### RingArray Generation Tests

Test cases should cover:

- Single ring (center ring, ring_index = 0)
- Multiple rings with different lamp counts
- InnerFirst ordering (channels assigned inner→outer)
- OuterFirst ordering (channels assigned outer→inner)
- Offset angle rotation
- Edge cases: empty ring_lamp_counts, invalid ring indices
- Coordinate correctness: positions in [0, 1] range, correct angles
- Channel assignment: sequential, no gaps, correct offsets for multiple paths
- Sample diameter conversion: pixel-based to normalized radius
