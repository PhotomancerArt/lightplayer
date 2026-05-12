# Phase 2: Implement RingArray Path Generation

## Goal

Implement functions to generate `MappingPoint` positions from `RingArray` path specifications.

## Implementation

### 1. Add Path Generation Function

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Add function to generate mapping points from `MappingConfig`:

```rust
fn generate_mapping_points(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint> {
    match config {
        MappingConfig::PathPoints { paths, sample_diameter } => {
            let mut all_points = Vec::new();
            let mut channel_offset = 0u32;

            for path_config in paths {
                let points = match &path_config.path_spec {
                    PathSpec::RingArray(ring_array) => {
                        generate_ring_array_points(
                            ring_array,
                            *sample_diameter,
                            texture_width,
                            texture_height,
                            channel_offset,
                        )
                    }
                };

                channel_offset += points.len() as u32;
                all_points.extend(points);
            }

            all_points
        }
    }
}
```

### 2. Implement RingArray Generation

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Add function to generate points from RingArray:

```rust
fn generate_ring_array_points(
    ring_array: &RingArray,
    sample_diameter: f32,
    texture_width: u32,
    texture_height: u32,
    channel_offset: u32,
) -> Vec<MappingPoint> {
    let (center_x, center_y) = ring_array.center;
    let diameter = ring_array.diameter;
    let start_ring = ring_array.start_ring_inclusive;
    let end_ring = ring_array.end_ring_exclusive;
    let ring_lamp_counts = &ring_array.ring_lamp_counts;
    let offset_angle = ring_array.offset_angle;

    // Calculate max ring index for spacing
    let max_ring_index = if end_ring > start_ring {
        (end_ring - start_ring - 1) as f32
    } else {
        0.0
    };

    // Convert sample_diameter (pixels) to normalized radius
    let max_dimension = texture_width.max(texture_height) as f32;
    let normalized_radius = (sample_diameter / 2.0) / max_dimension;

    // Determine ring processing order
    let ring_indices: Vec<u32> = match ring_array.order {
        RingOrder::InnerFirst => (start_ring..end_ring).collect(),
        RingOrder::OuterFirst => (start_ring..end_ring).rev().collect(),
    };

    let mut points = Vec::new();
    let mut current_channel = channel_offset;

    for ring_index in ring_indices {
        // Calculate ring radius (even spacing)
        let ring_radius = if max_ring_index > 0.0 {
            (diameter / 2.0) * ((ring_index - start_ring) as f32 / max_ring_index)
        } else {
            0.0
        };

        // Get lamp count for this ring
        let lamp_count = ring_lamp_counts
            .get(ring_index as usize)
            .copied()
            .unwrap_or(0);

        // Generate points for each lamp in the ring
        for lamp_index in 0..lamp_count {
            let angle = (2.0 * core::f32::consts::PI * lamp_index as f32 / lamp_count as f32)
                + offset_angle;

            let x = center_x + ring_radius * angle.cos();
            let y = center_y + ring_radius * angle.sin();

            // Clamp to [0, 1] range
            let x = x.max(0.0).min(1.0);
            let y = y.max(0.0).min(1.0);

            points.push(MappingPoint {
                channel: current_channel,
                center: [x, y],
                radius: normalized_radius,
            });

            current_channel += 1;
        }
    }

    points
}
```

### 3. Add Required Imports

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Add imports for `MappingConfig`, `PathConfig`, `PathSpec`, `RingArray`, `RingOrder`:

```rust
use lp_model::nodes::fixture::mapping::{
    MappingConfig, PathConfig, PathSpec, RingArray, RingOrder,
};
```

### 4. Update FixtureRuntime

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Add fields to track texture dimensions:

```rust
pub struct FixtureRuntime {
    // ... existing fields ...
    texture_width: Option<u32>,
    texture_height: Option<u32>,
}
```

Update `new()` to initialize these fields to `None`.

## Success Criteria

- `generate_mapping_points()` function implemented
- `generate_ring_array_points()` function implemented
- Ring positions calculated correctly with even spacing
- Angles calculated correctly with offset
- Channel numbers assigned sequentially
- InnerFirst and OuterFirst ordering work correctly
- Sample diameter converted to normalized radius
- Coordinates clamped to [0, 1] range
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
