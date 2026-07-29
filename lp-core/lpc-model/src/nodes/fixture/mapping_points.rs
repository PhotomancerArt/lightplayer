//! Mapping point generation from configuration
//!
//! Generates LED sample points (texture-space centers + radii) from a
//! [`MappingConfig`]. Lives in the model crate so both the engine and
//! Studio-side tooling can share the generator.

use alloc::vec::Vec;

use crate::nodes::fixture::{MappingConfig, PathSpec};
use crate::{MapSlot, XySlot};

/// Mapping point representing a single LED sampling location
#[derive(Debug, Clone)]
pub struct MappingPoint {
    pub channel: u32,
    pub center: [f32; 2], // Texture space coordinates [0, 1]
    pub radius: f32,
}

/// Generate mapping points from MappingConfig
///
/// `Map2d` is an authored source reference: the loader resolves it into
/// `PathPoints` before this runs, so it yields no sample points here.
pub fn generate_mapping_points(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint> {
    match config {
        MappingConfig::Unset => Vec::new(),
        MappingConfig::Map2d { .. } => Vec::new(),
        MappingConfig::PathPoints {
            paths,
            sample_diameter,
            ..
        } => {
            let mut all_points = Vec::new();
            let mut channel_offset = 0u32;

            for path_spec in paths.entries.values() {
                let points = match path_spec.value() {
                    PathSpec::PointList {
                        first_channel,
                        points,
                        ..
                    } => generate_point_list_points(
                        *first_channel.value(),
                        points,
                        sample_diameter.value().0,
                        texture_width,
                        texture_height,
                    ),
                };

                channel_offset = channel_offset.saturating_add(points.len() as u32);
                all_points.extend(points);
            }

            all_points
        }
    }
}

fn normalized_sample_radius(sample_diameter: f32, texture_width: u32, texture_height: u32) -> f32 {
    let max_dimension = texture_width.max(texture_height) as f32;
    (sample_diameter / 2.0) / max_dimension
}

fn generate_point_list_points(
    first_channel: u32,
    point_list: &MapSlot<u32, XySlot>,
    sample_diameter: f32,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint> {
    let normalized_radius =
        normalized_sample_radius(sample_diameter, texture_width, texture_height);
    point_list
        .entries
        .iter()
        .map(|(index, point)| {
            let [x, y] = point.value().0;
            MappingPoint {
                channel: first_channel.saturating_add(*index),
                center: [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)],
                radius: normalized_radius,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn config(paths: alloc::vec::Vec<PathSpec>) -> MappingConfig {
        MappingConfig::path_points_vec(paths, 2.0)
    }

    #[test]
    fn point_lists_generate_points_with_sequential_channels() {
        let config = config(vec![
            PathSpec::point_list(0, [[0.1, 0.2], [0.3, 0.4]]),
            PathSpec::point_list(2, [[0.5, 0.6]]),
        ]);
        let points = generate_mapping_points(&config, 100, 100);
        assert_eq!(points.len(), 3);
        for (index, point) in points.iter().enumerate() {
            assert_eq!(point.channel, index as u32);
        }
        assert_eq!(points[0].center, [0.1, 0.2]);
        assert_eq!(points[2].center, [0.5, 0.6]);
    }

    #[test]
    fn sample_diameter_normalizes_against_max_dimension() {
        let config = config(vec![PathSpec::point_list(0, [[0.5, 0.5]])]);
        let points = generate_mapping_points(&config, 200, 100);
        // diameter 2.0 px on a 200-wide texture -> radius 1/200.
        assert!((points[0].radius - 0.005).abs() < 1e-6);
    }

    #[test]
    fn coordinates_clamp_to_unit_range() {
        let config = config(vec![PathSpec::point_list(0, [[-0.5, 1.5]])]);
        let points = generate_mapping_points(&config, 100, 100);
        assert_eq!(points[0].center, [0.0, 1.0]);
    }

    #[test]
    fn unset_and_map2d_yield_no_points() {
        assert!(generate_mapping_points(&MappingConfig::Unset, 10, 10).is_empty());
        assert!(generate_mapping_points(&MappingConfig::map2d("a.map2d.json"), 10, 10).is_empty());
    }
}
