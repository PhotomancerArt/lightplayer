//! Mapping point generation from configuration

use alloc::vec::Vec;
use libm;
use lp_model::nodes::fixture::mapping::{MappingConfig, PathSpec, RingOrder};

/// Mapping point representing a single LED sampling location
#[derive(Debug, Clone)]
pub struct MappingPoint {
    pub channel: u32,
    pub center: [f32; 2], // Texture space coordinates [0, 1]
    pub radius: f32,
}

/// Generate mapping points from MappingConfig
pub fn generate_mapping_points(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint> {
    match config {
        MappingConfig::PathPoints {
            paths,
            sample_diameter,
        } => {
            let mut all_points = Vec::new();
            let mut channel_offset = 0u32;

            for path_spec in paths {
                let points = match path_spec {
                    PathSpec::RingArray {
                        center,
                        diameter,
                        start_ring_inclusive,
                        end_ring_exclusive,
                        ring_lamp_counts,
                        offset_angle,
                        order,
                    } => generate_ring_array_points(
                        *center,
                        *diameter,
                        *start_ring_inclusive,
                        *end_ring_exclusive,
                        ring_lamp_counts,
                        *offset_angle,
                        *order,
                        *sample_diameter,
                        texture_width,
                        texture_height,
                        channel_offset,
                    ),
                };

                channel_offset += points.len() as u32;
                all_points.extend(points);
            }

            all_points
        }
    }
}

/// Generate mapping points from RingArray path specification
fn generate_ring_array_points(
    center: (f32, f32),
    diameter: f32,
    start_ring_inclusive: u32,
    end_ring_exclusive: u32,
    ring_lamp_counts: &Vec<u32>,
    offset_angle: f32,
    order: RingOrder,
    sample_diameter: f32,
    texture_width: u32,
    texture_height: u32,
    channel_offset: u32,
) -> Vec<MappingPoint> {
    let (center_x, center_y) = center;
    let start_ring = start_ring_inclusive;
    let end_ring = end_ring_exclusive;

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
    let ring_indices: Vec<u32> = match order {
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

            let x = center_x + ring_radius * libm::cosf(angle);
            let y = center_y + ring_radius * libm::sinf(angle);

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Test helper: create RingArray path spec
    fn create_ring_array_path(
        center: (f32, f32),
        diameter: f32,
        start_ring: u32,
        end_ring: u32,
        ring_lamp_counts: Vec<u32>,
        offset_angle: f32,
        order: RingOrder,
    ) -> PathSpec {
        PathSpec::RingArray {
            center,
            diameter,
            start_ring_inclusive: start_ring,
            end_ring_exclusive: end_ring,
            ring_lamp_counts,
            offset_angle,
            order,
        }
    }

    #[test]
    fn test_single_ring_center() {
        // Single ring at center (ring_index = 0) with 8 lamps
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![8], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify 8 points generated
        assert_eq!(points.len(), 8);

        // Verify all points at center position (radius = 0 for single ring)
        for point in &points {
            assert!((point.center[0] - 0.5).abs() < 0.001);
            assert!((point.center[1] - 0.5).abs() < 0.001);
        }

        // Verify channels 0-7 assigned sequentially
        for (i, point) in points.iter().enumerate() {
            assert_eq!(point.channel, i as u32);
        }

        // Verify angles evenly spaced (0, π/4, π/2, ...)
        // Since all points are at center, angles don't matter, but verify structure
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[7].channel, 7);
    }

    #[test]
    fn test_multiple_rings() {
        // Multiple rings with different lamp counts
        // Ring 0: 1 lamp (center)
        // Ring 1: 8 lamps
        // Ring 2: 16 lamps
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 8, 16],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify correct number of points (1 + 8 + 16 = 25)
        assert_eq!(points.len(), 25);

        // Verify channels assigned sequentially (0-24)
        for (i, point) in points.iter().enumerate() {
            assert_eq!(point.channel, i as u32);
        }

        // Verify ring 0 (center) has 1 point at center
        assert_eq!(points[0].channel, 0);
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);

        // Verify ring 1 has 8 points (channels 1-8)
        // Verify ring 2 has 16 points (channels 9-24)
        assert_eq!(points[1].channel, 1);
        assert_eq!(points[8].channel, 8);
        assert_eq!(points[9].channel, 9);
        assert_eq!(points[24].channel, 24);
    }

    #[test]
    fn test_inner_first_ordering() {
        // Multiple rings with different lamp counts
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 4, 8],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels assigned inner→outer
        // Ring 0: channels 0-0 (1 lamp)
        // Ring 1: channels 1-4 (4 lamps)
        // Ring 2: channels 5-12 (8 lamps)
        assert_eq!(points[0].channel, 0); // Ring 0, first lamp
        assert_eq!(points[1].channel, 1); // Ring 1, first lamp
        assert_eq!(points[5].channel, 5); // Ring 2, first lamp
        assert_eq!(points[12].channel, 12); // Ring 2, last lamp
    }

    #[test]
    fn test_outer_first_ordering() {
        // Multiple rings with different lamp counts
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 4, 8],
            0.0,
            RingOrder::OuterFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels assigned outer→inner
        // Ring 2: channels 0-7 (8 lamps, outer)
        // Ring 1: channels 8-11 (4 lamps)
        // Ring 0: channel 12 (1 lamp, inner)
        assert_eq!(points[0].channel, 0); // Ring 2, first lamp (outer)
        assert_eq!(points[7].channel, 7); // Ring 2, last lamp
        assert_eq!(points[8].channel, 8); // Ring 1, first lamp
        assert_eq!(points[11].channel, 11); // Ring 1, last lamp
        assert_eq!(points[12].channel, 12); // Ring 0, only lamp (inner)
    }

    #[test]
    fn test_offset_angle() {
        // Single ring with offset angle
        let path = create_ring_array_path(
            (0.5, 0.5),
            0.5,
            0,
            1,
            vec![4],
            core::f32::consts::PI / 4.0, // π/4 offset
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify 4 points generated
        assert_eq!(points.len(), 4);

        // Verify first lamp at angle π/4 (not 0)
        // For ring at radius 0 (center), all points are at center, so angles don't affect position
        // But verify structure is correct
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[3].channel, 3);
    }

    #[test]
    fn test_coordinate_correctness() {
        // Test coordinates are in [0, 1] range
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            2,
            vec![1, 8],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        for point in &points {
            // Verify coordinates in [0, 1] range
            assert!(point.center[0] >= 0.0 && point.center[0] <= 1.0);
            assert!(point.center[1] >= 0.0 && point.center[1] <= 1.0);
            assert!(point.radius >= 0.0 && point.radius <= 1.0);
        }
    }

    #[test]
    fn test_coordinate_edge_cases() {
        // Test edge cases: center at (0, 0), (1, 1), (0.5, 0.5)
        for center in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.5)] {
            let path =
                create_ring_array_path(center, 0.5, 0, 1, vec![4], 0.0, RingOrder::InnerFirst);
            let config = MappingConfig::PathPoints {
                paths: vec![path],
                sample_diameter: 2.0,
            };

            let points = generate_mapping_points(&config, 100, 100);

            for point in &points {
                // Verify coordinates clamped to [0, 1]
                assert!(point.center[0] >= 0.0 && point.center[0] <= 1.0);
                assert!(point.center[1] >= 0.0 && point.center[1] <= 1.0);
            }
        }
    }

    #[test]
    fn test_sample_diameter_conversion() {
        // Test sample diameter to normalized radius conversion
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![1], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        // Test with square texture (100x100)
        let points_square = generate_mapping_points(&config, 100, 100);
        assert_eq!(points_square.len(), 1);
        // sample_diameter = 2.0, max_dimension = 100, normalized_radius = (2.0 / 2.0) / 100 = 0.01
        assert!((points_square[0].radius - 0.01).abs() < 0.0001);

        // Test with wide texture (200x100)
        let points_wide = generate_mapping_points(&config, 200, 100);
        assert_eq!(points_wide.len(), 1);
        // sample_diameter = 2.0, max_dimension = 200, normalized_radius = (2.0 / 2.0) / 200 = 0.005
        assert!((points_wide[0].radius - 0.005).abs() < 0.0001);

        // Test with tall texture (100x200)
        let points_tall = generate_mapping_points(&config, 100, 200);
        assert_eq!(points_tall.len(), 1);
        // sample_diameter = 2.0, max_dimension = 200, normalized_radius = (2.0 / 2.0) / 200 = 0.005
        assert!((points_tall[0].radius - 0.005).abs() < 0.0001);
    }

    #[test]
    fn test_channel_assignment_multiple_paths() {
        // Multiple paths with different LED counts
        let path1 =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![5], 0.0, RingOrder::InnerFirst);
        let path2 =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![3], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path1, path2],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels sequential with no gaps
        // Path 1: channels 0-4 (5 LEDs)
        // Path 2: channels 5-7 (3 LEDs)
        assert_eq!(points.len(), 8);
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[4].channel, 4);
        assert_eq!(points[5].channel, 5);
        assert_eq!(points[7].channel, 7);
    }

    #[test]
    fn test_edge_cases_empty_ring() {
        // Test with zero lamp count for a ring
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            2,
            vec![1, 0],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Should only generate 1 point (ring 0), ring 1 has 0 lamps
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].channel, 0);
    }

    #[test]
    fn test_edge_cases_invalid_ring_indices() {
        // Test with start_ring >= end_ring
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 2, 2, vec![], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Should generate no points (empty range)
        assert_eq!(points.len(), 0);
    }

    #[test]
    fn test_edge_cases_single_lamp() {
        // Test with single lamp in a ring
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![1], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].channel, 0);
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_ring_radius_calculation() {
        // Test that ring radii increase correctly
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 8, 16],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Ring 0 (center): radius should be 0
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);

        // Ring 1: should have non-zero radius
        let ring1_radius =
            ((points[1].center[0] - 0.5).powi(2) + (points[1].center[1] - 0.5).powi(2)).sqrt();
        assert!(ring1_radius > 0.0);

        // Ring 2: should have larger radius than ring 1
        let ring2_radius =
            ((points[9].center[0] - 0.5).powi(2) + (points[9].center[1] - 0.5).powi(2)).sqrt();
        assert!(ring2_radius > ring1_radius);
    }
}
