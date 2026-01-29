# Phase 3: Add Comprehensive Tests for RingArray Generation

## Goal

Add comprehensive test coverage for RingArray path generation to ensure correctness and catch edge cases.

## Test Cases

### 1. Single Ring (Center Ring)

**File**: `lp-engine/src/nodes/fixture/runtime.rs` (in `mod tests`)

Test a single ring at center (ring_index = 0) with 8 lamps:

- Verify 8 points generated
- Verify all points at center position (radius = 0)
- Verify channels 0-7 assigned sequentially
- Verify angles evenly spaced (0, π/4, π/2, ...)

### 2. Multiple Rings

Test multiple rings with different lamp counts:

- Ring 0: 1 lamp (center)
- Ring 1: 8 lamps
- Ring 2: 16 lamps
- Verify correct number of points (1 + 8 + 16 = 25)
- Verify ring radii increase correctly
- Verify channels assigned sequentially (0-24)

### 3. InnerFirst Ordering

Test `RingOrder::InnerFirst`:

- Multiple rings with different lamp counts
- Verify channels assigned inner→outer
- Verify ring 0 channels come before ring 1 channels, etc.

### 4. OuterFirst Ordering

Test `RingOrder::OuterFirst`:

- Multiple rings with different lamp counts
- Verify channels assigned outer→inner
- Verify ring N channels come before ring N-1 channels, etc.

### 5. Offset Angle

Test offset angle rotation:

- Single ring with offset_angle = π/4
- Verify first lamp at angle π/4 (not 0)
- Verify angles spaced correctly with offset

### 6. Coordinate Correctness

Test coordinate generation:

- Verify all coordinates in [0, 1] range
- Verify center position matches RingArray center
- Verify ring radii calculated correctly
- Test edge case: center at (0, 0), (1, 1), (0.5, 0.5)

### 7. Sample Diameter Conversion

Test sample diameter to normalized radius conversion:

- Test with different texture dimensions (square, wide, tall)
- Verify normalized radius calculated correctly
- Test edge cases: sample_diameter = 0, 1, large values

### 8. Channel Assignment

Test channel assignment:

- Multiple paths with different LED counts
- Verify channels sequential with no gaps
- Verify channel_offset works correctly
- Test with channel_offset > 0

### 9. Edge Cases

Test edge cases:

- Empty ring_lamp_counts (should handle gracefully)
- Invalid ring indices (start_ring >= end_ring)
- Zero lamp count for a ring
- Single lamp in a ring
- Very large ring counts

### 10. Integration Test

Test full path through `generate_mapping_points()`:

- Multiple paths with RingArray
- Verify all paths processed
- Verify channel offsets correct
- Verify all points generated correctly

## Implementation

Add test module at bottom of `runtime.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lp_model::nodes::fixture::mapping::{MappingConfig, PathConfig, PathSpec, RingArray, RingOrder};

    // Test helper: create RingArray config
    fn create_ring_array(
        center: (f32, f32),
        diameter: f32,
        start_ring: u32,
        end_ring: u32,
        ring_lamp_counts: Vec<u32>,
        offset_angle: f32,
        order: RingOrder,
    ) -> PathConfig {
        PathConfig {
            path_spec: PathSpec::RingArray(RingArray {
                center,
                diameter,
                start_ring_inclusive: start_ring,
                end_ring_exclusive: end_ring,
                ring_lamp_counts,
                offset_angle,
                order,
            }),
        }
    }

    // Individual test cases...
}
```

## Success Criteria

- All test cases implemented
- Tests cover single ring, multiple rings, ordering, angles, channels
- Edge cases handled and tested
- All tests pass
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
