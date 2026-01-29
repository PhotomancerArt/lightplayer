# Phase 5: Implement pre-computation logic

## Scope of phase

Implement the `compute_mapping()` function that builds the `PrecomputedMapping` from mapping configuration and texture dimensions.

## Code Organization Reminders

- Place main public function first
- Place helper/utility functions at the bottom
- Keep related functionality grouped together
- Add comprehensive tests

## Implementation Details

### 1. Add compute_mapping function

Add to `lp-app/crates/lp-engine/src/nodes/fixture/mapping_compute.rs`:

```rust
use lp_model::nodes::fixture::mapping::{MappingConfig, PathSpec};
use lp_model::FrameId;

/// Compute pre-computed mapping from configuration
/// 
/// # Arguments
/// * `config` - Mapping configuration
/// * `texture_width` - Texture width in pixels
/// * `texture_height` - Texture height in pixels
/// * `mapping_data_ver` - FrameId for version tracking
/// 
/// # Returns
/// PrecomputedMapping with entries ordered by pixel (x, y)
pub fn compute_mapping(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
    mapping_data_ver: FrameId,
) -> PrecomputedMapping {
    let mut mapping = PrecomputedMapping::new(texture_width, texture_height, mapping_data_ver);
    
    match config {
        MappingConfig::PathPoints {
            paths,
            sample_diameter,
        } => {
            // First pass: collect all mapping points (circles)
            let mut mapping_points = Vec::new();
            let mut channel_offset = 0u32;
            
            for path_spec in paths {
                let points = match path_spec {
                    PathSpec::RingArray { .. } => {
                        // Use existing generate_mapping_points logic
                        // TODO: We'll need to extract this or refactor
                        // For now, generate points similar to existing code
                        generate_mapping_points_for_path(
                            path_spec,
                            texture_width,
                            texture_height,
                            *sample_diameter,
                            channel_offset,
                        )
                    }
                };
                
                channel_offset += points.len() as u32;
                mapping_points.extend(points);
            }
            
            // Second pass: for each pixel, compute contributions from all circles
            let mut pixel_contributions: Vec<Vec<(u32, f32)>> = 
                vec![Vec::new(); (texture_width * texture_height) as usize];
            
            for mapping_point in &mapping_points {
                let center_x = mapping_point.center[0] * texture_width as f32;
                let center_y = mapping_point.center[1] * texture_height as f32;
                let radius = mapping_point.radius * texture_width.max(texture_height) as f32;
                
                // Find pixels that might overlap with this circle
                let min_x = ((center_x - radius).floor() as i32).max(0) as u32;
                let max_x = ((center_x + radius).ceil() as i32).min(texture_width as i32 - 1) as u32;
                let min_y = ((center_y - radius).floor() as i32).max(0) as u32;
                let max_y = ((center_y + radius).ceil() as i32).min(texture_height as i32 - 1) as u32;
                
                for y in min_y..=max_y {
                    for x in min_x..=max_x {
                        let weight = circle_pixel_overlap(center_x, center_y, radius, x, y);
                        if weight > 0.0 {
                            let pixel_idx = (y * texture_width + x) as usize;
                            pixel_contributions[pixel_idx].push((mapping_point.channel, weight));
                        }
                    }
                }
            }
            
            // Third pass: normalize weights per-channel and build entries
            for y in 0..texture_height {
                for x in 0..texture_width {
                    let pixel_idx = (y * texture_width + x) as usize;
                    let contributions = &pixel_contributions[pixel_idx];
                    
                    if contributions.is_empty() {
                        // No contributions - add SKIP entry
                        mapping.entries.push(PixelMappingEntry::skip());
                    } else {
                        // Normalize weights per-channel: divide by channel total
                        // This ensures each channel's total contribution from all pixels = 1.0
                        let total_weight: f32 = contributions.iter().map(|(_, w)| w).sum();
                        if total_weight > 0.0 {
                            let normalized: Vec<(u32, f32)> = contributions
                                .iter()
                                .map(|(ch, w)| (*ch, w / total_weight))
                                .collect();
                            
                            // Add entries (last one has has_more = false)
                            for (idx, (channel, weight)) in normalized.iter().enumerate() {
                                let has_more = idx < normalized.len() - 1;
                                let contribution_q32 = Q32::from_f32(*weight);
                                mapping.entries.push(PixelMappingEntry::new(
                                    *channel,
                                    contribution_q32,
                                    has_more,
                                ));
                            }
                        } else {
                            // Zero total weight - add SKIP
                            mapping.entries.push(PixelMappingEntry::skip());
                        }
                    }
                }
            }
        }
    }
    
    mapping
}

/// Helper: Generate mapping points for a path spec
/// (Extracted/adapted from existing generate_ring_array_points)
fn generate_mapping_points_for_path(
    path_spec: &PathSpec,
    texture_width: u32,
    texture_height: u32,
    sample_diameter: f32,
    channel_offset: u32,
) -> Vec<MappingPoint> {
    // TODO: Extract this from runtime.rs or refactor
    // For now, placeholder that matches existing structure
    match path_spec {
        PathSpec::RingArray {
            center,
            diameter,
            start_ring_inclusive,
            end_ring_exclusive,
            ring_lamp_counts,
            offset_angle,
            order,
        } => {
            // Use existing generate_ring_array_points logic
            // We'll need to import or duplicate this
            // Placeholder for now
            Vec::new()
        }
    }
}

/// Temporary: Mapping point structure for pre-computation
/// (Matches existing MappingPoint from runtime.rs)
struct MappingPoint {
    channel: u32,
    center: [f32; 2],
    radius: f32,
}
```

Note: We'll need to either extract `generate_ring_array_points` from `runtime.rs` or refactor it to be shared. For now, this is a placeholder structure.

### 2. Add tests

```rust
#[cfg(test)]
mod compute_mapping_tests {
    use super::*;
    use lp_model::nodes::fixture::mapping::{PathSpec, RingOrder};
    
    #[test]
    fn test_empty_mapping() {
        // TODO: Create minimal config
        // let config = MappingConfig::PathPoints { paths: vec![], sample_diameter: 2.0 };
        // let mapping = compute_mapping(&config, 100, 100, FrameId::new(1));
        // assert_eq!(mapping.len(), 10000); // All pixels should have SKIP entries
    }
    
    // More tests to be added as we implement the full logic
}
```

## Validate

Run:
```bash
cd lp-app && cargo check --package lp-engine
```

Expected: Code compiles (may have TODOs for extracting shared logic). We'll complete the implementation in the next phase when we integrate with FixtureRuntime.
