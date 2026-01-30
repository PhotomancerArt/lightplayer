# Phase 3: Create circle-pixel overlap calculation utilities

## Scope of phase

Implement `circle_pixel_overlap()` function that computes the area overlap between a circle and a pixel square using 8x8 subdivision.

## Code Organization Reminders

- Place public functions first
- Place helper/utility functions at the bottom
- Keep related functionality grouped together
- Add comprehensive tests with edge cases

## Implementation Details

### 1. Add circle-pixel overlap function

Add to `lp-app/crates/lp-engine/src/nodes/fixture/mapping_compute.rs`:

```rust
/// Compute the area overlap between a circle and a pixel square
///
/// Uses 8x8 subdivision (64 sub-pixels) to estimate overlap area.
/// Returns normalized weight (0.0 to 1.0) representing how much of the pixel
/// is covered by the circle.
///
/// # Arguments
/// * `circle_center_x` - Circle center X in pixel coordinates
/// * `circle_center_y` - Circle center Y in pixel coordinates
/// * `circle_radius` - Circle radius in pixels
/// * `pixel_x` - Pixel X coordinate (integer)
/// * `pixel_y` - Pixel Y coordinate (integer)
///
/// # Returns
/// Normalized weight (0.0 to 1.0) representing pixel coverage
pub fn circle_pixel_overlap(
    circle_center_x: f32,
    circle_center_y: f32,
    circle_radius: f32,
    pixel_x: u32,
    pixel_y: u32,
) -> f32 {
    const SUBDIVISIONS: u32 = 8;
    const TOTAL_SAMPLES: f32 = (SUBDIVISIONS * SUBDIVISIONS) as f32;

    // Pixel bounds
    let px_min = pixel_x as f32;
    let px_max = (pixel_x + 1) as f32;
    let py_min = pixel_y as f32;
    let py_max = (pixel_y + 1) as f32;

    // Sub-pixel size
    let sub_pixel_size = 1.0 / SUBDIVISIONS as f32;

    // Count sub-pixels within circle
    let mut count = 0u32;

    for i in 0..SUBDIVISIONS {
        for j in 0..SUBDIVISIONS {
            // Sub-pixel center coordinates
            let sub_x = px_min + (i as f32 + 0.5) * sub_pixel_size;
            let sub_y = py_min + (j as f32 + 0.5) * sub_pixel_size;

            // Distance from circle center to sub-pixel center
            let dx = sub_x - circle_center_x;
            let dy = sub_y - circle_center_y;
            let dist_sq = dx * dx + dy * dy;

            // Check if sub-pixel center is within circle
            if dist_sq <= circle_radius * circle_radius {
                count += 1;
            }
        }
    }

    // Normalize: count / total_samples gives coverage fraction
    count as f32 / TOTAL_SAMPLES
}

#[cfg(test)]
mod overlap_tests {
    use super::*;

    #[test]
    fn test_full_overlap() {
        // Circle completely covers pixel
        let weight = circle_pixel_overlap(0.5, 0.5, 1.0, 0, 0);
        assert!(weight >= 0.95, "Full overlap should be close to 1.0, got {}", weight);
    }

    #[test]
    fn test_no_overlap() {
        // Circle far from pixel
        let weight = circle_pixel_overlap(10.0, 10.0, 0.5, 0, 0);
        assert!(weight < 0.01, "No overlap should be close to 0.0, got {}", weight);
    }

    #[test]
    fn test_partial_overlap() {
        // Circle partially overlaps pixel (center at edge)
        let weight = circle_pixel_overlap(0.0, 0.5, 0.5, 0, 0);
        assert!(weight > 0.0 && weight < 1.0, "Partial overlap should be between 0 and 1, got {}", weight);
    }

    #[test]
    fn test_circle_at_pixel_center() {
        // Circle centered on pixel
        let weight = circle_pixel_overlap(0.5, 0.5, 0.3, 0, 0);
        assert!(weight > 0.0 && weight <= 1.0);
    }

    #[test]
    fn test_small_circle() {
        // Very small circle
        let weight = circle_pixel_overlap(0.5, 0.5, 0.1, 0, 0);
        assert!(weight > 0.0 && weight < 1.0);
    }

    #[test]
    fn test_large_circle() {
        // Very large circle covering multiple pixels
        let weight = circle_pixel_overlap(0.5, 0.5, 10.0, 0, 0);
        assert!(weight >= 0.95, "Large circle should cover pixel completely");
    }

    #[test]
    fn test_edge_pixel() {
        // Circle at edge of texture
        let weight = circle_pixel_overlap(0.0, 0.0, 0.5, 0, 0);
        assert!(weight > 0.0 && weight <= 1.0);
    }

    #[test]
    fn test_symmetry() {
        // Overlap should be symmetric
        let w1 = circle_pixel_overlap(1.5, 0.5, 0.5, 1, 0);
        let w2 = circle_pixel_overlap(0.5, 1.5, 0.5, 0, 1);
        // Should be similar (not necessarily equal due to discretization)
        assert!((w1 - w2).abs() < 0.1, "Symmetry check failed: {} vs {}", w1, w2);
    }
}
```

## Validate

Run:

```bash
cd lp-app && cargo test --package lp-engine circle_pixel_overlap
```

Expected: All tests pass, code compiles without warnings. Overlap calculations should be reasonably accurate (within ~5% of expected values).
